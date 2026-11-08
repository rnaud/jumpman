//! Mario's blob shadow (classic look) and projected `GEO_SHADOW` actor shadows (`actorShadows.ts`).
use crate::button_marker::LoadWithSettings;
use super::display::{DisplaySettings, Look};
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::asset::{AssetServer, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use jumpman_sim::state::*;
use std::collections::HashMap;
use std::path::Path;

/// `GEO_SHADOW` arguments of an actor model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    pub square: bool,
    pub solidity: f32,
    pub size: f32,
}

/// Parse `sm64Args` `[type, solidity, size]` of the first `GEO_SHADOW` node in a glb's JSON chunk.
pub fn shadow_spec_from_json(json: &serde_json::Value) -> Option<ShadowSpec> {
    let node = json["nodes"].as_array()?.iter().find(|n| n["extras"]["sm64Command"] == "GEO_SHADOW")?;
    let args = node["extras"]["sm64Args"].as_array()?;
    let text = |i: usize| args.get(i).and_then(|v| v.as_str()).unwrap_or("");
    let number = |s: &str| -> f32 {
        match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(h) => i64::from_str_radix(h, 16).unwrap_or(0) as f32,
            None => s.parse().unwrap_or(0.0),
        }
    };
    let (solidity, size) = (number(text(1)), number(text(2)));
    if solidity == 0.0 || size == 0.0 {
        return None;
    }
    Some(ShadowSpec { square: text(0).contains("SQUARE"), solidity, size })
}

fn read_glb_json(path: &Path) -> Option<serde_json::Value> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 20];
    file.read_exact(&mut head).ok()?;
    if &head[0..4] != b"glTF" {
        return None;
    }
    let len = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    let mut json = vec![0u8; len];
    file.read_exact(&mut json).ok()?;
    serde_json::from_slice(&json).ok()
}

/// Per-frame shadow placement: `(size, opacity)` for an actor at height `y` over `ground`.
/// `scale` is the actor's model scale.
pub fn shadow_scale_opacity(spec: ShadowSpec, scale: f32, y: f32, ground: f32) -> (f32, f32) {
    let dist = ((y - ground) / 600.0).clamp(0.0, 1.0);
    let size = spec.size * scale / 100.0 * (1.0 - 0.5 * dist);
    let opacity = (spec.solidity - (spec.solidity - 120.0).max(0.0) * dist) / 255.0;
    (size, opacity)
}

/// Mario's blob: scale shrinks with height down to 0.45.
pub fn blob_scale(y: f32, floor: f32) -> f32 {
    (1.0 - (y - floor).max(0.0) / 1200.0).max(0.45)
}

#[derive(Component)]
struct BlobShadow;

#[derive(Component)]
struct ActorShadow(usize);

#[derive(Resource, Default)]
struct ActorShadows {
    specs: HashMap<String, Option<ShadowSpec>>,
    entities: Vec<Option<(Entity, ShadowSpec)>>,
    quad: Handle<Mesh>,
    circle: Handle<Image>,
    square: Handle<Image>,
}

pub struct ShadowsPlugin;

impl Plugin for ShadowsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActorShadows>().add_systems(Startup, setup.in_set(crate::progress::PersistentSpawn)).add_systems(Update, (blob_shadow, actor_shadows).after(crate::CameraSet));
    }
}

/// 2x2 grid whose UVs are `|x|,|z|`: the quarter texture mirrored into a full shadow
/// (three.js `MirroredRepeatWrapping`, repeat 2, offset -1).
pub(crate) fn mirrored_quad() -> Mesh {
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    for iz in -1..=1i32 {
        for ix in -1..=1i32 {
            pos.push([ix as f32 * 0.5, 0.0, iz as f32 * 0.5]);
            uv.push([ix.abs() as f32, iz.abs() as f32]);
        }
    }
    let mut idx = Vec::new();
    for z in 0..2u32 {
        for x in 0..2u32 {
            let a = z * 3 + x;
            idx.extend_from_slice(&[a, a + 3, a + 1, a + 1, a + 3, a + 4]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 9]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, assets: Res<AssetServer>, mut shadows: ResMut<ActorShadows>) {
    shadows.quad = meshes.add(mirrored_quad());
    let load = |name: &str| -> Handle<Image> {
        assets.load_builder_with_settings(format!("extracted/textures/segment2/{name}.ia8.png"), |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::ClampToEdge, address_mode_v: ImageAddressMode::ClampToEdge, ..ImageSamplerDescriptor::nearest() });
        })
    };
    shadows.circle = load("shadow_quarter_circle");
    shadows.square = load("shadow_quarter_square");
    commands.spawn((
        BlobShadow,
        Mesh3d(meshes.add(Circle::new(55.0 * SCALE))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgba(0., 0., 0., 0.28), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, fog_enabled: true, ..default() })),
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn blob_shadow(
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    settings: Res<DisplaySettings>,
    mut blob: Single<(&mut Transform, &mut Visibility), With<BlobShadow>>,
) {
    let m = &sim.mario;
    let emerging = (m.action == ACT_SPECIAL_DEATH_EXIT || m.action == ACT_SPECIAL_EXIT_AIRBORNE) && m.action_timer <= 11.0;
    let Some(floor) = m.floor.as_ref() else {
        *blob.1 = Visibility::Hidden;
        return;
    };
    if emerging || settings.look != Look::Classic || m.action == ACT_IN_CANNON || m.vanished() {
        *blob.1 = Visibility::Hidden;
        return;
    }
    let (pos, _) = sim.mario_at(fixed.overstep_fraction() as f64);
    *blob.1 = Visibility::Inherited;
    let normal = Vec3::new(floor.normal[0] as f32, floor.normal[1] as f32, floor.normal[2] as f32);
    blob.0.translation = to_bevy([pos[0], m.floor_height + 2.0, pos[2]]);
    blob.0.rotation = Quat::from_rotation_arc(Vec3::Z, normal.normalize_or_zero());
    blob.0.scale = Vec3::splat(blob_scale(pos[1] as f32, m.floor_height as f32));
}

fn actor_shadows(
    mut commands: Commands,
    sim: Res<GameSim>,
    root: Res<AssetRoot>,
    fixed: Res<Time<Fixed>>,
    mut shadows: ResMut<ActorShadows>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&ActorShadow, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>)>,
) {
    let objects = &sim.mario.objects.objects;
    while shadows.entities.len() > objects.len() {
        if let Some(Some((e, _))) = shadows.entities.pop() {
            commands.entity(e).despawn();
        }
    }
    for idx in shadows.entities.len()..objects.len() {
        let mut entry = None;
        if let Some(model) = objects[idx].model.as_deref().filter(|m| !m.is_empty()) {
            let spec = *shadows.specs.entry(model.to_owned()).or_insert_with(|| read_glb_json(&root.0.join(model)).and_then(|j| shadow_spec_from_json(&j)));
            if let Some(spec) = spec {
                let texture = if spec.square { shadows.square.clone() } else { shadows.circle.clone() };
                let material = materials.add(StandardMaterial { base_color: Color::srgba(0., 0., 0., 0.), base_color_texture: Some(texture), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
                let e = commands
                    .spawn((ActorShadow(idx), Mesh3d(shadows.quad.clone()), MeshMaterial3d(material), Transform::default(), Visibility::Hidden, NotShadowCaster, NotShadowReceiver))
                    .id();
                entry = Some((e, spec));
            }
        }
        shadows.entities.push(entry);
    }

    let t = fixed.overstep_fraction() as f64;
    for (ActorShadow(idx), mut tf, mut vis, material) in &mut q {
        let (Some(o), Some(Some((_, spec)))) = (objects.get(*idx), shadows.entities.get(*idx)) else { continue };
        if !(o.active && o.visible) {
            *vis = Visibility::Hidden;
            continue;
        }
        let cur = sim.curr.objects.get(*idx).copied().unwrap_or_default();
        let prev = sim.prev.objects.get(*idx).copied().unwrap_or(cur);
        let near = (0..3).all(|i| (cur.pos[i] - prev.pos[i]).abs() < 300.0);
        let pos = if near { [0, 1, 2].map(|i| prev.pos[i] + (cur.pos[i] - prev.pos[i]) * t) } else { cur.pos };
        let y = pos[1] + o.graph_y_offset;
        let world = &sim.mario.world;
        let floor = world.find_floor(pos[0], y, pos[2]);
        let water = world.water_level(pos[0], pos[2]);
        let on_water = water > floor.height && water <= y;
        if !on_water && floor.surface.is_none() {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let height = if on_water { water } else { floor.height };
        let normal = match (&floor.surface, on_water) {
            (Some(s), false) => Vec3::new(s.normal[0] as f32, s.normal[1] as f32, s.normal[2] as f32).normalize_or_zero(),
            _ => Vec3::Y,
        };
        let (size, opacity) = shadow_scale_opacity(*spec, o.scale as f32, y as f32, height as f32);
        tf.translation = to_bevy([pos[0], height + 1.0, pos[2]]);
        tf.rotation = Quat::from_rotation_arc(Vec3::Y, normal);
        tf.scale = Vec3::splat(size * SCALE);
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.base_color = Color::srgba(0., 0., 0., opacity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_goomba_shadow() {
        let json = serde_json::json!({"nodes":[{"extras":{"sm64Command":"GEO_SHADOW","sm64Args":["SHADOW_CIRCLE_4_VERTS","0x96","100"]}}]});
        assert_eq!(shadow_spec_from_json(&json), Some(ShadowSpec { square: false, solidity: 150.0, size: 100.0 }));
        let sq = serde_json::json!({"nodes":[{"extras":{"sm64Command":"GEO_SHADOW","sm64Args":["SHADOW_SQUARE_PERMANENT","0xB4","50"]}}]});
        assert!(shadow_spec_from_json(&sq).unwrap().square);
        assert_eq!(shadow_spec_from_json(&serde_json::json!({"nodes":[{}]})), None);
    }

    #[test]
    fn shadow_fades_with_height() {
        let spec = ShadowSpec { square: false, solidity: 180.0, size: 100.0 };
        let (s0, o0) = shadow_scale_opacity(spec, 2.0, 0.0, 0.0);
        assert_eq!((s0, o0), (2.0, 180.0 / 255.0));
        let (s1, o1) = shadow_scale_opacity(spec, 2.0, 600.0, 0.0);
        assert_eq!((s1, o1), (1.0, 120.0 / 255.0));
    }

    #[test]
    fn blob_shrinks_to_floor() {
        assert_eq!(blob_scale(0.0, 0.0), 1.0);
        assert_eq!(blob_scale(5000.0, 0.0), 0.45);
        assert!((blob_scale(600.0, 0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn mirrored_quad_has_9_vertices() {
        let m = mirrored_quad();
        assert_eq!(m.count_vertices(), 9);
        assert_eq!(m.indices().unwrap().len(), 24);
    }
}
