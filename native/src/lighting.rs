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
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
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
            app.add_systems(PostUpdate, (forward_cutout_materials, prepare_raytracing_scene, sky_environment).chain());
        }
    }
}

/// Solari traces triangle lists with position + normal + uv0 + tangent and u32 indices. The game's meshes are
/// built without tangents/uvs/u16 indices, so complete them once and register every static lit mesh with the
/// ray-tracing scene. Skinned meshes keep rasterising (they are lit but neither cast nor receive bounce light).
fn prepare_raytracing_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    todo: Query<(Entity, &Mesh3d, &MeshMaterial3d<StandardMaterial>), (Without<RaytracingMesh3d>, Without<bevy::mesh::skinning::SkinnedMesh>)>,
    materials: Res<Assets<StandardMaterial>>,
    mut lights: Query<&mut DirectionalLight>,
    mut done: Local<HashSet<AssetId<Mesh>>>,
) {
    // Solari replaces shadow maps with traced shadows.
    for mut l in &mut lights {
        if l.shadow_maps_enabled {
            l.shadow_maps_enabled = false;
        }
    }
    for (e, m, mat) in &todo {
        let id = m.0.id();
        // Alpha-masked quads (billboard trees, fences) would trace as solid rectangles: they stay out of the scene.
        if materials.get(&mat.0).is_some_and(|mm| matches!(mm.alpha_mode, AlphaMode::Mask(_))) {
            continue;
        }
        if !done.contains(&id) {
            let Some(mut mesh) = meshes.get_mut(id) else { continue };
            if mesh.primitive_topology() != bevy::mesh::PrimitiveTopology::TriangleList || !mesh.contains_attribute(Mesh::ATTRIBUTE_NORMAL) {
                done.insert(id);
                continue;
            }
            let n = mesh.count_vertices();
            if !mesh.contains_attribute(Mesh::ATTRIBUTE_UV_0) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, VertexAttributeValues::Float32x2(vec![[0.0, 0.0]; n]));
            }
            if !mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, VertexAttributeValues::Float32x4(vec![[1.0, 0.0, 0.0, 1.0]; n]));
            }
            match mesh.indices() {
                Some(Indices::U32(_)) => {}
                Some(Indices::U16(v)) => {
                    let w: Vec<u32> = v.iter().map(|&i| i as u32).collect();
                    mesh.insert_indices(Indices::U32(w));
                }
                None => mesh.insert_indices(Indices::U32((0..n as u32).collect())),
            }
            done.insert(id);
        }
        // A mesh that failed the checks above (`done` without attributes) must not enter the scene.
        if meshes.get(id).is_some_and(|me| me.contains_attribute(Mesh::ATTRIBUTE_TANGENT) && me.contains_attribute(Mesh::ATTRIBUTE_NORMAL)) {
            commands.entity(e).try_insert(RaytracingMesh3d(m.0.clone()));
        }
    }
}

/// Solari shades everything through the deferred G-buffer, which does not survive the game's N64-style materials:
/// unlit ones are emissive there and its output is multiplied by the camera exposure (~1e-3), and alpha-masked
/// ones (tree and bush billboards, Bob-omb, fences, the castle's cut-outs) come out black. Those materials are
/// moved to the forward path, which draws them exactly as the other tiers do after Solari has lit the rest.
/// Blended materials already render forward.
fn forward_cutout_materials(mut events: MessageReader<AssetEvent<StandardMaterial>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let ids: Vec<_> = events.read().filter_map(|e| match e {
        AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(*id),
        _ => None,
    }).collect();
    for id in ids {
        let Some(m) = materials.get(id) else { continue };
        let cutout = m.unlit || matches!(m.alpha_mode, AlphaMode::Mask(_));
        if cutout && m.opaque_render_method != OpaqueRendererMethod::Forward {
            if let Some(mut m) = materials.get_mut(id) {
                m.opaque_render_method = OpaqueRendererMethod::Forward;
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
    mut images: ResMut<Assets<Image>>,
    cameras: Query<(Entity, Has<EnvironmentMapLight>), (With<MainCamera>, With<SolariLighting>)>,
    mut last: Local<Option<crate::fx::sky::SkyPalette>>,
) {
    for (e, has) in &cameras {
        if has && *last == Some(*palette) {
            continue;
        }
        let c = |v: [f32; 3]| Color::srgb(v[0] + (1.0 - v[0]) * SKY_WASH, v[1] + (1.0 - v[1]) * SKY_WASH, v[2] + (1.0 - v[2]) * SKY_WASH);
        let ground = palette.bottom.map(|v| v * 0.45);
        commands.entity(e).insert(EnvironmentMapLight::hemispherical_gradient(&mut images, c(palette.top), c(palette.mid), c(ground)).with_intensity(RT_SKY_INTENSITY));
        *last = Some(*palette);
    }
}

/// Radiance scale of the sky environment map (the rasteriser's ambient is 320; the sky gradient averages ~0.7).
const RT_SKY_INTENSITY: f32 = 500.0;
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
                    SolariLighting::default(),
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
