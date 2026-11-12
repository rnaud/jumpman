//! Lighting quality tiers. `Standard` is the classic look; `Enhanced` adds screen-space ambient
//! occlusion, soft (Gaussian-filtered) shadows with a larger shadow map, and FXAA (SSAO needs MSAA
//! off). `RayTraced` is real hardware ray tracing: `bevy_solari` (Bevy 0.20 / wgpu 30) traces inline
//! ray queries through Metal acceleration structures (macOS 15+ / iOS 18+ GPUs with Metal raytracing).
//! It is only offered when the adapter reports `SolariPlugins::required_wgpu_features()`. Solari switches
//! the whole renderer to deferred, so the tier is read at launch: choosing it in Settings applies on the
//! next start.
//!
//! Test hook: `JUMPMAN_RT=off|enhanced|rt` overrides the setting without persisting it. `rt` asks for
//! hardware ray tracing; when the GPU cannot do it the reason is logged and `enhanced` is used.
use crate::camera::MainCamera;
use crate::shell::settings::{Lighting, Settings};
use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::{CameraMainTextureUsages, RenderTarget};
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::prepass::{DeferredPrepass, DepthPrepass, NormalPrepass};
use bevy::light::{DirectionalLightShadowMap, ShadowFilteringMethod};
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::material::OpaqueRendererMethod;
use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel};
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::solari::prelude::*;
use std::collections::HashSet;
use std::sync::OnceLock;

/// Result of probing the GPU once at launch.
#[derive(Clone, Debug)]
pub struct RtSupport {
    /// The adapter exposes every wgpu feature Solari needs.
    pub supported: bool,
    /// Ray tracing was requested at launch and the Solari plugins are installed (this run is deferred-rendered).
    pub active: bool,
    /// Adapter name and the verdict, for logs and the Settings note.
    pub detail: String,
}

static RT: OnceLock<RtSupport> = OnceLock::new();

pub fn rt_support() -> &'static RtSupport {
    RT.get_or_init(|| RtSupport { supported: false, active: false, detail: "not probed".into() })
}

/// Ask wgpu (same options Bevy uses) whether the high-performance adapter can do ray queries.
fn probe() -> (bool, String) {
    if cfg!(target_os = "android") {
        return (false, "not probed on Android".into());
    }
    let required = SolariPlugins::required_wgpu_features();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let options = wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..default() };
    match bevy::tasks::block_on(instance.request_adapter(&options)) {
        Ok(adapter) => {
            let name = adapter.get_info().name;
            let missing = required.difference(adapter.features());
            if missing.is_empty() {
                (true, name)
            } else {
                (false, format!("{name} lacks {missing:?}"))
            }
        }
        Err(e) => (false, format!("no GPU adapter: {e}")),
    }
}

/// One-line status shown under the lighting row in Settings.
pub fn ray_tracing_note() -> String {
    let rt = rt_support();
    if rt.active {
        format!("Hardware ray tracing: ACTIVE on {}.", rt.detail)
    } else if rt.supported {
        format!("Hardware ray tracing is available on {}; the Ray traced choice applies after restart.", rt.detail)
    } else {
        format!("Hardware ray tracing: unavailable ({}).", rt.detail)
    }
}

/// Parse `JUMPMAN_RT`. `true` in the second slot means ray tracing was requested.
pub fn parse_env(v: &str) -> Option<(Lighting, bool)> {
    match v.trim().to_ascii_lowercase().as_str() {
        "off" | "standard" | "0" => Some((Lighting::Standard, false)),
        "enhanced" | "on" | "1" => Some((Lighting::Enhanced, false)),
        "rt" | "raytracing" | "ray" => Some((Lighting::RayTraced, true)),
        _ => None,
    }
}

/// The tier requested by the environment hook if set, else the persisted setting.
pub fn requested(settings: &Settings) -> Lighting {
    std::env::var("JUMPMAN_RT").ok().and_then(|v| parse_env(&v)).map_or(settings.lighting, |(l, _)| l)
}

/// The tier actually rendered. A ray-traced request needs the launch-time plugins, so it degrades to Enhanced
/// (this GPU cannot) or Standard (needs a restart) and the UI never claims a look that is not on screen.
pub fn effective(settings: &Settings) -> Lighting {
    match requested(settings) {
        Lighting::RayTraced if rt_support().active => Lighting::RayTraced,
        Lighting::RayTraced if !rt_support().supported => Lighting::Enhanced,
        Lighting::RayTraced => Lighting::Standard,
        other => other,
    }
}

/// Called from `run()` before the renderer exists: probe the GPU and decide whether this launch is ray traced.
pub fn init_launch(settings: &Settings) -> bool {
    let (supported, detail) = probe();
    let wants = requested(settings) == Lighting::RayTraced;
    let active = supported && wants;
    if wants && !supported {
        warn!("ray tracing requested but unavailable: {detail}; using enhanced lighting");
    }
    eprintln!("ray tracing: supported={supported} active={active} ({detail})");
    let _ = RT.set(RtSupport { supported, active, detail });
    active
}

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, apply);
        if std::env::var_os("JUMPMAN_DEBUG_CAMERAS").is_some() {
            app.add_systems(Update, debug_cameras);
        }
        if rt_support().active {
            if std::env::var_os("JUMPMAN_RT_DEBUG").is_some() {
                app.add_systems(Update, (debug_rt_scene, debug_pick));
            }
            app.add_plugins(crate::rt_fog::RtFogPlugin);
            app.init_resource::<crate::rt_scene::RtProxies>().init_resource::<crate::rt_scene::RtSolidMaterials>();
            app.add_systems(PostUpdate, (forward_cutout_materials, configure_sun, crate::rt_scene::register_rt_meshes, crate::rt_scene::sync_proxies, sky_environment, deferred_secondary_cameras).chain());
        }
    }
}

/// True when the mesh still holds its vertex data in the main world (it was not extracted to the render world).
/// Touching the attributes of an extracted mesh panics in Bevy.
pub(crate) fn mesh_is_editable(mesh: &Mesh) -> bool {
    mesh.asset_usage.contains(bevy::asset::RenderAssetUsages::MAIN_WORLD) && mesh.try_attributes().is_ok()
}

/// True when the entity is drawn by the world camera (render layer 0, which an entity without `RenderLayers` has).
pub(crate) fn in_world_layer(layers: Option<&RenderLayers>) -> bool {
    layers.is_none_or(|l| l.intersects(&RenderLayers::layer(0)))
}

/// Solari renders every opaque lit material through the deferred G-buffer (the `SolariPlugins` default), and only the
/// main camera has the Solari lighting pass that turns that G-buffer into colour. A second 3D camera (the front end's
/// private low-resolution camera, the kart split-screen's second view) draws deferred materials into a G-buffer it
/// does not have, so every lit surface came out black. Give those cameras the deferred prepass: Bevy's own deferred
/// lighting pass then shades them with the classic lights, exactly as the non-ray-traced tiers do.
fn deferred_secondary_cameras(
    mut commands: Commands,
    cameras: Query<(Entity, Option<&Msaa>), (With<Camera3d>, Without<MainCamera>, Without<DeferredPrepass>)>,
) {
    for (e, msaa) in &cameras {
        let mut c = commands.entity(e);
        c.insert((DepthPrepass, DeferredPrepass));
        if msaa.is_none_or(|m| *m != Msaa::Off) {
            c.insert(Msaa::Off);
        }
    }
}

/// Materials that must not be in the ray-traced scene (and are drawn by the forward path instead): anything that is
/// not a plain opaque surface (alpha cut-outs would trace as solid rectangles, blended sprites/water/particles are not
/// occluders) and anything emissive. Solari makes every emissive instance a light source and picks one uniformly per
/// sample, so ~100 glowing coins in a level left the sun with ~1% of the samples: no sun, no sun shadows.
pub(crate) fn rt_excluded(m: &StandardMaterial, solid: bool) -> bool {
    let mask_ok = solid && matches!(m.alpha_mode, AlphaMode::Mask(_));
    // Unlit surfaces carry their lighting baked into the colours (the OoT scenes, sky-like backdrops) and are drawn by the
    // forward path: they cannot receive a traced shadow, and a one-sided backdrop would shadow everything inside it.
    (!matches!(m.alpha_mode, AlphaMode::Opaque) && !mask_ok) || m.emissive != LinearRgba::BLACK || m.unlit
}

/// Solari replaces shadow maps with traced shadows: switch them off, and size the sun disk so the traced shadow edge
/// has a soft penumbra instead of the razor edge of the real sun's 0.5 degrees.
fn configure_sun(mut commands: Commands, mut lights: Query<(Entity, &mut DirectionalLight, Has<bevy::light::SunDisk>)>) {
    for (e, mut l, has_disk) in &mut lights {
        if l.shadow_maps_enabled {
            l.shadow_maps_enabled = false;
        }
        if !has_disk {
            // Intensity 0: no atmosphere draws the disk, it only sizes the penumbra.
            commands.entity(e).try_insert(bevy::light::SunDisk { angular_size: tune("JUMPMAN_RT_DISK", RT_SUN_DISK), intensity: 0.0 });
        }
    }
}

/// Solari shades everything through the deferred G-buffer, which does not survive the game's N64-style materials:
/// unlit ones are emissive there and its output is multiplied by the camera exposure (~1e-3), and alpha-masked
/// ones (tree and bush billboards, Bob-omb, fences, the castle's cut-outs) come out black. Those materials are
/// moved to the forward path, which draws them exactly as the other tiers do after Solari has lit the rest.
/// Blended materials already render forward.
fn forward_cutout_materials(mut events: MessageReader<AssetEvent<StandardMaterial>>, mut materials: ResMut<Assets<StandardMaterial>>, solid: Res<crate::rt_scene::RtSolidMaterials>) {
    let ids: Vec<_> = events.read().filter_map(|e| match e {
        AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(*id),
        _ => None,
    }).collect();
    for id in ids {
        if solid.0.contains(&id) {
            continue;
        }
        let Some(m) = materials.get(id) else { continue };
        let cutout = m.unlit || (rt_excluded(m, false) && !(std::env::var_os("JUMPMAN_RT_MASKDEF").is_some() && matches!(m.alpha_mode, AlphaMode::Mask(_))));
        if cutout && m.opaque_render_method != OpaqueRendererMethod::Forward {
            if let Some(mut m) = materials.get_mut(id) {
                m.opaque_render_method = OpaqueRendererMethod::Forward;
                // The fog overlay (`rt_fog`) fogs every depth-writing pixel; the material must not fog itself as well.
                if !matches!(m.alpha_mode, AlphaMode::Blend | AlphaMode::Add | AlphaMode::Multiply | AlphaMode::Premultiplied) {
                    m.fog_enabled = false;
                }
            }
        }
    }
}

/// Solari has no ambient light: everything the sun does not face is black unless the sky is an environment map
/// (the rasteriser's `GlobalAmbientLight` does not exist for it). Rebuild a 1x1 gradient cubemap from the dome's
/// palette whenever the palette moves, so shaded walls, tree billboards and characters get sky fill and bounce light.
fn sky_environment(
    mut commands: Commands,
    palette: Res<crate::fx::sky::SkyPalette>,
    ambient: Res<GlobalAmbientLight>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<(Entity, Has<EnvironmentMapLight>), (With<MainCamera>, With<SolariLighting>)>,
    mut last: Local<Option<(crate::fx::sky::SkyPalette, [f32; 4])>>,
) {
    let tint = ambient.color.to_srgba();
    let key = (*palette, [ambient.brightness, tint.red, tint.green, tint.blue]);
    for (e, has) in &cameras {
        if has && *last == Some(key) {
            continue;
        }
        // The level's own ambient (SM64 320, the Zelda scenes' time-of-day ambient) sets how bright and what colour the
        // sky fill is, so every course keeps the look it has in the other tiers.
        let c = |v: [f32; 3]| Color::srgb((v[0] + (1.0 - v[0]) * SKY_WASH) * tint.red, (v[1] + (1.0 - v[1]) * SKY_WASH) * tint.green, (v[2] + (1.0 - v[2]) * SKY_WASH) * tint.blue);
        let ground = palette.bottom.map(|v| v * 0.45);
        let intensity = ambient.brightness * tune("JUMPMAN_RT_SKY", RT_SKY_FILL);
        commands.entity(e).insert(EnvironmentMapLight::hemispherical_gradient(&mut images, c(palette.top), c(palette.mid), c(ground)).with_intensity(intensity));
        *last = Some(key);
    }
}

/// Solari's quality settings for the main camera.
fn solari_settings() -> SolariLighting {
    let d = SolariLighting::default();
    SolariLighting {
        restir: tune("JUMPMAN_RT_RESTIR", 0.0) > 0.5,
        max_bounces: tune("JUMPMAN_RT_BOUNCES", d.max_bounces as f32) as u32,
        world_cache_max_gi_ray_distance: tune("JUMPMAN_RT_GIDIST", d.world_cache_max_gi_ray_distance),
        world_cache_position_base_cell_size: tune("JUMPMAN_RT_CELL", d.world_cache_position_base_cell_size),
        world_cache_position_lod_scale: tune("JUMPMAN_RT_LOD", d.world_cache_position_lod_scale),
        primary_di_samples: tune("JUMPMAN_RT_DI", d.primary_di_samples as f32) as u32,
        ..d
    }
}

/// Debug knob: `JUMPMAN_RT_<NAME>` overrides a tuning constant (read once per name).
fn tune(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
/// Angular diameter of the sun disk in radians: wider means softer traced shadow edges (the real sun is 0.0093).
const RT_SUN_DISK: f32 = 0.05;

/// Radiance of the sky environment map relative to the rasteriser's ambient light (the washed sky gradient averages ~0.85,
/// and traced bounce light comes on top of it).
const RT_SKY_FILL: f32 = 1.0;
/// How far the sky colours are pulled towards white so shaded surfaces keep their hue instead of turning blue-grey.
const SKY_WASH: f32 = 0.5;

fn apply(
    mut commands: Commands,
    settings: Res<Settings>,
    cameras: Query<(Entity, &Msaa, Has<ScreenSpaceAmbientOcclusion>, Has<SolariLighting>), With<MainCamera>>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut saved_msaa: Local<Option<Msaa>>,
) {
    let want = effective(&settings);
    let size = if want == Lighting::Enhanced { 4096 } else { 2048 };
    if shadow_map.size != size {
        shadow_map.size = size;
    }
    for (e, msaa, has, has_rt) in &cameras {
        if want == Lighting::RayTraced {
            if !has_rt {
                saved_msaa.get_or_insert(*msaa);
                // No denoiser exists for non-NVIDIA GPUs (DLSS-RR only), so noise is handled by temporal accumulation:
                // Solari's world cache for indirect light and Bevy's TAA (jittered, motion-vector reprojected) for the rest.
                commands.entity(e).remove::<(ScreenSpaceAmbientOcclusion, Fxaa)>().insert((
                    Msaa::Off,
                    CameraMainTextureUsages::default().with(TextureUsages::STORAGE_BINDING),
                    solari_settings(),
                    bevy::anti_alias::taa::TemporalAntiAliasing::default(),
                ));
            }
            continue;
        }
        if has_rt {
            commands.entity(e).remove::<SolariLighting>();
        }
        match (want, has) {
            (Lighting::Enhanced, false) => {
                saved_msaa.get_or_insert(*msaa);
                commands.entity(e).insert((
                    Msaa::Off,
                    DepthPrepass,
                    NormalPrepass,
                    ScreenSpaceAmbientOcclusion { quality_level: ScreenSpaceAmbientOcclusionQualityLevel::High, radius: 0.7285, constant_object_thickness: 0.5 },
                    ShadowFilteringMethod::Gaussian,
                    Fxaa::default(),
                ));
            }
            (Lighting::Standard, true) => {
                commands.entity(e).remove::<(DepthPrepass, NormalPrepass, ScreenSpaceAmbientOcclusion, Fxaa)>().insert((saved_msaa.take().unwrap_or(Msaa::Sample4), ShadowFilteringMethod::default()));
            }
            _ => {}
        }
    }
}

/// Debug hook (`JUMPMAN_RT_DEBUG=1`): after 6 s list the ray traced instances, biggest first.
fn debug_rt_scene(
    time: Res<Time>,
    q: Query<(Entity, Option<&Name>, &GlobalTransform, Option<&bevy::camera::primitives::Aabb>, &MeshMaterial3d<StandardMaterial>), With<RaytracingMesh3d>>,
    materials: Res<Assets<StandardMaterial>>,
    meshes: Res<Assets<Mesh>>,
    sun: Query<(&DirectionalLight, &GlobalTransform)>,
    ambient: Res<GlobalAmbientLight>,
    palette: Res<crate::fx::sky::SkyPalette>,
    mario: Query<Entity, With<crate::render::mario_view::MarioRoot>>,
    children: Query<&Children>,
    outside: Query<(Entity, Option<&Name>, &Mesh3d, Option<&MeshMaterial3d<StandardMaterial>>, Option<&RenderLayers>, Has<bevy::mesh::skinning::SkinnedMesh>, &Visibility), Without<RaytracingMesh3d>>,
    mut done: Local<bool>,
) {
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    *done = true;
    for (l, g) in &sun {
        eprintln!("RTDEBUG sun illuminance {} color {:?} to_light {:?} shadows {} | ambient {} {:?} | palette {:?}", l.illuminance, l.color, g.back(), l.shadow_maps_enabled, ambient.brightness, ambient.color, *palette);
    }
    let out: Vec<_> = outside.iter().collect();
    for root in &mario {
        let (mut total, mut traced) = (0, 0);
        for d in children.iter_descendants(root) {
            if let Ok((_, _, _, mat, _, _, vis)) = outside.get(d) {
                total += 1;
                if total % 20 == 0 {
                    eprintln!("RTDEBUG mario part outside {d:?} vis {vis:?} mat {:?}", mat.and_then(|m| materials.get(&m.0)).map(|m| (m.alpha_mode, m.unlit, m.emissive, m.cull_mode, m.opaque_render_method, m.base_color)));
                }
            } else if q.contains(d) {
                total += 1;
                traced += 1;
            }
        }
        eprintln!("RTDEBUG mario mesh parts {total}, traced {traced}");
    }
    let mut v: Vec<(f32, String)> = q
        .iter()
        .filter_map(|(e, n, g, aabb, mat)| {
            let a = aabb?;
            let size = (Vec3::from(a.half_extents) * 2.0 * g.compute_transform().scale.abs()).length();
            let m = materials.get(&mat.0);
            Some((size, format!("{e:?} {:?} size {size:.1} at {:?} alpha {:?} unlit {:?} method {:?}", n.map(|n| n.as_str()), g.translation(), m.map(|m| m.alpha_mode), m.map(|m| m.unlit), m.map(|m| m.opaque_render_method))))
        })
        .collect();
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    let emissive = q.iter().filter(|(.., mat)| materials.get(&mat.0).is_some_and(|m| m.emissive != LinearRgba::BLACK)).count();
    eprintln!("RTDEBUG {} ray traced instances, {emissive} emissive (each is a light source that competes with the sun)", q.iter().count());
    for (e, n, mesh, mat, layers, skinned, vis) in out.iter().take(60) {
        eprintln!("RTDEBUG outside {e:?} {:?} layers {layers:?} skinned {skinned} vis {vis:?} mat {:?} editable {:?}", n.map(|n| n.as_str()), mat.and_then(|m| materials.get(&m.0)).map(|m| (m.alpha_mode, m.unlit, m.emissive != LinearRgba::BLACK, m.cull_mode, m.double_sided)), meshes.get(&mesh.0).map(mesh_is_editable));
    }
    eprintln!("RTDEBUG {} mesh entities outside the ray traced scene", out.len());
    for (e, n, _, _, mat) in q.iter().filter(|(.., mat)| materials.get(&mat.0).is_some_and(|m| m.emissive != LinearRgba::BLACK)).take(12) {
        eprintln!("RTDEBUG emissive {e:?} {:?} {:?}", n.map(|n| n.as_str()), materials.get(&mat.0).map(|m| (m.emissive, m.base_color, m.base_color_texture.is_some())));
    }
    for (_, l) in v.iter().take(25) {
        eprintln!("RTDEBUG {l}");
    }
}

/// Debug hook (`JUMPMAN_RT_PICK=x,y`): at 6 s shoot a ray through that pixel and describe the first mesh it hits.
fn debug_pick(
    time: Res<Time>,
    cam: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    q: Query<(Entity, Option<&Name>, &Mesh3d, &MeshMaterial3d<StandardMaterial>, &GlobalTransform, Has<RaytracingMesh3d>, Has<crate::rt_scene::RtSkip>, &InheritedVisibility)>,
    mut done: Local<bool>,
) {
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    *done = true;
    let list = std::env::var("JUMPMAN_RT_PICK").unwrap_or_default();
    for pair in list.split(';') {
    let Some((x, y)) = pair.split_once(',').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?))) else { continue };
    let Ok(ray) = cam.0.viewport_to_world(cam.1, Vec2::new(x, y)) else { continue };
    let mut best: Option<(f32, Entity, usize)> = None;
    for (e, _, mesh, mat, g, _, _, vis) in &q {
        if !vis.get() {
            continue;
        }
        let Some(m) = meshes.get(&mesh.0) else { continue };
        if !mesh_is_editable(m) {
            continue;
        }
        let Some(pos) = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) else { continue };
        let idx: Vec<u32> = match m.indices() {
            Some(Indices::U32(v)) => v.clone(),
            Some(Indices::U16(v)) => v.iter().map(|&i| i as u32).collect(),
            None => (0..pos.len() as u32).collect(),
        };
        let inv = g.affine().inverse();
        let o = inv.transform_point3(ray.origin);
        let d = inv.transform_vector3(*ray.direction);
        for (ti, t) in idx.chunks_exact(3).enumerate() {
            let (a, b, c) = (Vec3::from(pos[t[0] as usize]), Vec3::from(pos[t[1] as usize]), Vec3::from(pos[t[2] as usize]));
            // Moller-Trumbore, both sides.
            let (e1, e2) = (b - a, c - a);
            let p = d.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-12 {
                continue;
            }
            let tv = o - a;
            let u = tv.dot(p) / det;
            let qv = tv.cross(e1);
            let v = d.dot(qv) / det;
            let dist = e2.dot(qv) / det;
            let front = e1.cross(e2).dot(d) < 0.0;
            if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && dist > 0.0 && (front || materials.get(&mat.0).is_some_and(|m| m.cull_mode.is_none())) && best.is_none_or(|b| dist < b.0) {
                best = Some((dist, e, ti));
            }
        }
    }
    let Some((dist, e, ti)) = best else {
        eprintln!("RTPICK nothing hit at {x},{y}");
        continue;
    };
    let Ok((e, n, mesh, mat, g, traced, skipped, _)) = q.get(e) else { continue };
    eprintln!("RTPICK at {x},{y}");
    let m = meshes.get(&mesh.0).unwrap();
    let mm = materials.get(&mat.0).unwrap();
    let names: Vec<_> = m.attributes().map(|(a, _)| a.name).collect();
    eprintln!("RTPICK {e:?} {:?} dist {dist:.2} tri {ti} traced {traced} skipped {skipped} attrs {names:?} scale {:?}", n.map(|n| n.as_str()), g.compute_transform().scale);
    eprintln!("RTPICK material alpha {:?} cull {:?} double_sided {} unlit {} method {:?} base {:?} tex {} rough {} metallic {} ior {}", mm.alpha_mode, mm.cull_mode, mm.double_sided, mm.unlit, mm.opaque_render_method, mm.base_color, mm.base_color_texture.is_some(), mm.perceptual_roughness, mm.metallic, mm.ior);
    let pos = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).unwrap();
    let idx: Vec<u32> = match m.indices() {
        Some(Indices::U32(v)) => v.clone(),
        Some(Indices::U16(v)) => v.iter().map(|&i| i as u32).collect(),
        None => (0..pos.len() as u32).collect(),
    };
    let t = &idx[ti * 3..ti * 3 + 3];
    let (a, b, c) = (Vec3::from(pos[t[0] as usize]), Vec3::from(pos[t[1] as usize]), Vec3::from(pos[t[2] as usize]));
    let face = (b - a).cross(c - a).normalize_or_zero();
    let normals = m.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3());
    let cols = m.attribute(Mesh::ATTRIBUTE_COLOR);
    eprintln!("RTPICK face normal {face:?} (local) vertex normals {:?} colours {:?}", normals.map(|n| t.iter().map(|&i| n[i as usize]).collect::<Vec<_>>()), cols.map(|c| match c { VertexAttributeValues::Float32x4(v) => t.iter().map(|&i| v[i as usize]).collect::<Vec<_>>(), _ => vec![] }));
    }
}

/// Debug hook (`JUMPMAN_DEBUG_CAMERAS=1`): print every camera once after 3 s.
fn debug_cameras(time: Res<Time>, cams: Query<(Entity, &Camera, &RenderTarget, Option<&Msaa>)>, windows: Query<&Window>, mut done: Local<bool>) {
    if *done || time.elapsed_secs() < 3.0 {
        return;
    }
    *done = true;
    for (e, c, t, m) in &cams {
        eprintln!("camera {e:?} active={} order={} viewport={:?} target={:?} msaa={m:?} physical={:?}", c.is_active, c.order, c.viewport, t, c.physical_target_size());
    }
    for w in &windows {
        eprintln!("window physical {}x{} scale {}", w.physical_width(), w.physical_height(), w.scale_factor());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_values_parse() {
        assert_eq!(parse_env("off"), Some((Lighting::Standard, false)));
        assert_eq!(parse_env("Enhanced"), Some((Lighting::Enhanced, false)));
        assert_eq!(parse_env("rt"), Some((Lighting::RayTraced, true)));
        assert_eq!(parse_env("bogus"), None);
    }

    #[test]
    fn unprobed_gpu_never_claims_ray_tracing() {
        let s = Settings { lighting: Lighting::RayTraced, ..Settings::default() };
        assert_ne!(effective(&s), Lighting::RayTraced);
    }
}

#[cfg(test)]
mod rt_mesh_tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::PrimitiveTopology;

    fn tri(usage: RenderAssetUsages) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, usage);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 3]);
        m
    }

    #[test]
    fn render_world_only_mesh_is_not_editable_and_does_not_panic() {
        assert!(mesh_is_editable(&tri(RenderAssetUsages::default())));
        let mut m = tri(RenderAssetUsages::RENDER_WORLD);
        assert!(!mesh_is_editable(&m));
        // Simulate Bevy's extraction (data moved out, MAIN_WORLD absent): the guard still answers without panicking.
        let _ = m.take_gpu_data();
        assert!(!mesh_is_editable(&m));
    }

    #[test]
    fn private_layers_stay_out_of_the_ray_traced_world() {
        assert!(in_world_layer(None));
        assert!(in_world_layer(Some(&RenderLayers::layer(0))));
        assert!(in_world_layer(Some(&RenderLayers::from_layers(&[0, 3]))));
        assert!(!in_world_layer(Some(&RenderLayers::layer(7))));
    }

    #[test]
    fn secondary_cameras_get_the_deferred_prepass() {
        let mut app = App::new();
        app.add_systems(Update, deferred_secondary_cameras);
        let main = app.world_mut().spawn((Camera3d::default(), MainCamera)).id();
        let menu = app.world_mut().spawn((Camera3d::default(), Msaa::Sample4, RenderLayers::layer(7))).id();
        let ui = app.world_mut().spawn(Camera2d).id();
        app.update();
        let w = app.world();
        assert!(!w.entity(main).contains::<DeferredPrepass>(), "the main camera has Solari's own lighting pass");
        assert!(w.entity(menu).contains::<DeferredPrepass>() && w.entity(menu).contains::<DepthPrepass>());
        assert_eq!(*w.entity(menu).get::<Msaa>().unwrap(), Msaa::Off, "deferred rendering has no MSAA");
        assert!(!w.entity(ui).contains::<DeferredPrepass>());
    }

    #[test]
    fn materials_the_ray_tracer_cannot_shade_go_to_the_forward_path() {
        use crate::rt_scene::RtSolidMaterials;
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
        app.init_asset::<StandardMaterial>().init_resource::<RtSolidMaterials>();
        app.add_systems(Update, forward_cutout_materials);
        let mut add = |m: StandardMaterial| app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(m);
        let lit = add(StandardMaterial::default());
        let cutout = add(StandardMaterial { alpha_mode: AlphaMode::Mask(0.5), ..default() });
        let baked = add(StandardMaterial { unlit: true, ..default() });
        let glow = add(StandardMaterial { emissive: LinearRgba::rgb(0.5, 0.4, 0.1), ..default() });
        let blended = add(StandardMaterial { alpha_mode: AlphaMode::Blend, ..default() });
        let character = add(StandardMaterial { alpha_mode: AlphaMode::Mask(0.3), ..default() });
        app.world_mut().resource_mut::<RtSolidMaterials>().0.insert(character.id());
        // Asset events are flushed at the end of the frame the assets were added in.
        app.update();
        app.update();
        let mats = app.world().resource::<Assets<StandardMaterial>>();
        let method = |h: &Handle<StandardMaterial>| mats.get(h).unwrap().opaque_render_method;
        assert_eq!(method(&lit), OpaqueRendererMethod::Auto, "plain lit surfaces are shaded by Solari");
        assert_eq!(method(&character), OpaqueRendererMethod::Auto, "so are masked characters");
        for h in [&cutout, &baked, &glow] {
            assert_eq!(method(h), OpaqueRendererMethod::Forward);
            assert!(!mats.get(h).unwrap().fog_enabled, "the fog overlay fogs forward opaque surfaces, they must not fog twice");
        }
        assert_eq!(method(&blended), OpaqueRendererMethod::Forward);
        assert!(mats.get(&blended).unwrap().fog_enabled, "blended sprites keep their own fog (they are drawn after the overlay)");
    }

    #[test]
    fn pickup_fx_meshes_keep_main_world_data() {
        let m = crate::fx::pickup_fx::test_sample_mesh();
        assert!(m.asset_usage.contains(RenderAssetUsages::MAIN_WORLD));
    }
}
