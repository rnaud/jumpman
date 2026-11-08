//! Banjo-Kazooie enemies in the BK worlds: the presentation of `jumpman_sim::bk_enemy` objects (rigged
//! models exported by `scripts/export-bk-enemies.py`, skinned on the CPU with the same Rare animation
//! sampler as Banjo), Banjo's loot from defeated enemies, and the real placements of notes, eggs, feathers,
//! honeycombs and Jiggies (`bkPickups` of the course JSON, `scripts/export-bk-setup.py`).
//!
//! Everything is distance gated (views only for the nearest [`MAX_VIEWS`] enemies within [`VIEW_RANGE`],
//! pickups streamed in within [`PICKUP_RANGE`], at most [`MAX_LIVE_PICKUPS`] at once) so the iPhone budget holds.
//! The behavior itself lives in the sim crate; hit detection, damage and Link's Z-target come from the shared
//! object interaction pass, so this module only draws.
pub mod pickups;
#[cfg(test)]
mod tests;

use crate::characters::assets::{BanjoBatch, BanjoBone, BanjoClip, TextureSpec};
use crate::characters::banjo_pose::sample;
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::{EulerRot, Mat4, Quat, Vec3};
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::camera::visibility::NoFrustumCulling;
use jumpman_sim::bk_enemy::{kind_of, Kind, S};
use jumpman_sim::object::{ObjectBehaviorKind, SimObject};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Enemies further than this from Mario are not drawn (sim units).
pub const VIEW_RANGE: f64 = 6500.0;
/// Skinned enemies drawn at once.
pub const MAX_VIEWS: usize = 18;
/// Cross-fade between two clips (seconds), as `banjo_pose::BLEND`.
const BLEND: f64 = 0.15;

pub struct BkEnemiesPlugin;

impl Plugin for BkEnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EnemyAssets>()
            .init_resource::<EnemyViews>()
            .add_plugins(pickups::PickupsPlugin)
            .add_systems(Update, (enemy_drops, sync_views).chain().before(crate::CameraSet));
    }
}

// ---------------------------------------------------------------------------------------------
// Assets
// ---------------------------------------------------------------------------------------------

/// `public/models/bk/enemies/<key>.json` (`export-bk-enemies.py`): the Banjo rig format plus the decomp's
/// per-state animation table.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EnemyAsset {
    pub trans_scale: f64,
    pub bones: Vec<BanjoBone>,
    pub textures: HashMap<String, TextureSpec>,
    pub batches: Vec<BanjoBatch>,
    pub clips: HashMap<String, BanjoClip>,
    /// `states[actor state] = [clip, seconds]`.
    pub states: Vec<Option<(String, f64)>>,
}

impl EnemyAsset {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }

    /// Clip and nominal seconds for an actor state (the original `ActorAnimationInfo` row).
    pub fn state_clip(&self, state: i32) -> Option<(&str, f64)> {
        self.states.get(state as usize)?.as_ref().map(|(c, d)| (c.as_str(), *d))
    }
}

/// Where the exported models live: bundled next to the assets (`<assets>/models`) or the repo's `public/models`.
pub fn model_path(root: &std::path::Path, key: &str) -> std::path::PathBuf {
    let rel = format!("bk/enemies/{key}.json");
    let bundled = root.join("models").join(&rel);
    if bundled.is_file() {
        return bundled;
    }
    root.join("..").join("public/models").join(rel)
}

/// Asset key of a kind in a course (the hood's clothes follow the season).
pub fn asset_key(kind: Kind, course: &str) -> String {
    if kind == Kind::GrublinHood {
        let season = ["summer", "autumn", "winter", "spring"].iter().find(|s| course.ends_with(**s)).unwrap_or(&"spring");
        return format!("grublin_hood_{season}");
    }
    kind.model().to_string()
}

pub struct Rig {
    pub asset: EnemyAsset,
    /// clip -> bone id -> 9 channels (see `banjo_pose::BanjoAnimator`).
    channels: HashMap<String, HashMap<i64, Vec<Option<Vec<f64>>>>>,
}

#[derive(Clone)]
pub struct Pose {
    rot: Vec<Quat>,
    scale: Vec<Vec3>,
    trans: Vec<Vec3>,
}

impl Pose {
    fn new(n: usize) -> Self {
        Self { rot: vec![Quat::IDENTITY; n], scale: vec![Vec3::ONE; n], trans: vec![Vec3::ZERO; n] }
    }
}

impl Rig {
    pub fn new(asset: EnemyAsset) -> Self {
        let mut channels = HashMap::new();
        for (name, clip) in &asset.clips {
            let mut by_bone: HashMap<i64, Vec<Option<Vec<f64>>>> = HashMap::new();
            for (bone, channel, keys) in &clip.elements {
                by_bone.entry(*bone).or_insert_with(|| vec![None; 9])[*channel] = Some(keys.clone());
            }
            channels.insert(name.clone(), by_bone);
        }
        Self { asset, channels }
    }

    /// Evaluate `clip` at `progress` (0..1) into per-bone TRS.
    pub fn evaluate(&self, clip_name: &str, progress: f64, out: &mut Pose) {
        let (Some(clip), Some(by_bone)) = (self.asset.clips.get(clip_name), self.channels.get(clip_name)) else { return };
        let time = clip.start + progress.clamp(0.0, 0.999999) * (clip.end - clip.start);
        let mut values = [0.0; 9];
        for (i, bone) in self.asset.bones.iter().enumerate() {
            let chans = by_bone.get(&bone.id);
            for c in 0..9 {
                values[c] = match chans.and_then(|ch| ch[c].as_ref()) {
                    Some(keys) if !keys.is_empty() => sample(clip, c, keys, time),
                    _ => crate::characters::banjo_pose::DEFAULTS[c],
                };
            }
            let r = |v: f64| (v * std::f64::consts::PI / 180.0) as f32;
            out.rot[i] = Quat::from_euler(EulerRot::ZYX, r(values[2]), r(values[1]), r(values[0]));
            out.scale[i] = Vec3::new(values[3] as f32, values[4] as f32, values[5] as f32);
            out.trans[i] = Vec3::new(values[6] as f32, values[7] as f32, values[8] as f32);
        }
    }

    /// Bone world matrices of a pose (`BanjoAnimator::matrices`).
    pub fn matrices(&self, pose: &Pose) -> Vec<Mat4> {
        let s = self.asset.trans_scale as f32;
        let mut world: Vec<Mat4> = Vec::with_capacity(self.asset.bones.len());
        for (i, bone) in self.asset.bones.iter().enumerate() {
            let p = Vec3::new(bone.pivot[0] as f32, bone.pivot[1] as f32, bone.pivot[2] as f32);
            let local = Mat4::from_translation(p + pose.trans[i] * s) * Mat4::from_quat(pose.rot[i]) * Mat4::from_scale(pose.scale[i]) * Mat4::from_translation(-p);
            world.push(if bone.parent >= 0 && (bone.parent as usize) < world.len() { world[bone.parent as usize] * local } else { local });
        }
        world
    }
}

#[derive(Resource, Default)]
pub struct EnemyAssets {
    rigs: HashMap<String, Option<Arc<Rig>>>,
}

impl EnemyAssets {
    pub fn get(&mut self, root: &std::path::Path, key: &str) -> Option<Arc<Rig>> {
        self.rigs
            .entry(key.to_string())
            .or_insert_with(|| {
                let path = model_path(root, key);
                match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| EnemyAsset::parse(&b)) {
                    Ok(asset) => Some(Arc::new(Rig::new(asset))),
                    Err(e) => {
                        warn!("BK enemy model {key}: {}: {e}", path.display());
                        None
                    }
                }
            })
            .clone()
    }
}

// ---------------------------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------------------------

struct View {
    root: Entity,
    kind: Kind,
    rig: Option<Arc<Rig>>,
    batches: Vec<(Entity, Handle<Mesh>)>,
    clip: String,
    progress: f64,
    blend: f64,
    from: Pose,
    pose: Pose,
    last_state: i32,
}

#[derive(Resource, Default)]
pub struct EnemyViews {
    views: HashMap<usize, View>,
}

fn sampler(wrap: [usize; 2]) -> ImageSampler {
    let mode = |i: usize| match i {
        0 => ImageAddressMode::Repeat,
        2 => ImageAddressMode::ClampToEdge,
        _ => ImageAddressMode::MirrorRepeat,
    };
    ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: mode(wrap[0]), address_mode_v: mode(wrap[1]), ..default() })
}

fn make_image(spec: &TextureSpec, wrap: [usize; 2]) -> Image {
    let mut image = Image::new(
        Extent3d { width: spec.width, height: spec.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        spec.bytes(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler(wrap);
    image
}

fn empty_mesh(uvs: Vec<[f32; 2]>, colors: Vec<[f32; 4]>) -> Mesh {
    let n = uvs.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

fn material(texture: Option<Handle<Image>>) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: texture,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        double_sided: true,
        cull_mode: None,
        alpha_mode: AlphaMode::Mask(0.3),
        ..default()
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_view(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    kind: Kind,
    rig: Option<Arc<Rig>>,
) -> View {
    let root = commands.spawn((Transform::default(), Visibility::Hidden)).id();
    let mut batches = vec![];
    let mut textures: HashMap<String, Handle<Image>> = HashMap::new();
    let mut pose = Pose::new(1);
    if let Some(rig) = &rig {
        for b in &rig.asset.batches {
            let tex = b.texture.as_ref().and_then(|name| {
                let spec = rig.asset.textures.get(name)?;
                Some(textures.entry(format!("{name}/{:?}", b.wrap)).or_insert_with(|| images.add(make_image(spec, b.wrap))).clone())
            });
            let to_linear = |c: f64| (c / 255.0).powf(2.2) as f32;
            let uvs = b.vertices.iter().map(|v| [v[3] as f32, v[4] as f32]).collect();
            let colors = b.vertices.iter().map(|v| [to_linear(v[5]), to_linear(v[6]), to_linear(v[7]), 1.0]).collect();
            let mesh = meshes.add(empty_mesh(uvs, colors));
            let mat = materials.add(material(tex));
            let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), NoFrustumCulling, Transform::default(), Visibility::Inherited)).id();
            commands.entity(root).add_child(e);
            batches.push((e, mesh));
        }
        pose = Pose::new(rig.asset.bones.len());
    } else if kind == Kind::Snowball {
        let mesh = meshes.add(Sphere::new(45.0 * S as f32));
        let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.97, 1.0), perceptual_roughness: 0.9, ..default() });
        let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::default(), Visibility::Inherited)).id();
        commands.entity(root).add_child(e);
    }
    View { root, kind, rig, batches, clip: String::new(), progress: 0.0, blend: 1.0, from: pose.clone(), pose, last_state: -1 }
}

/// The clip a view should play for an object: the state's `ActorAnimationInfo` row, with the sim's overrides
/// (`animDur` for speed-scaled walks) and whether it plays once.
pub fn clip_for(rig: &Rig, o: &SimObject) -> Option<(String, f64, bool)> {
    let (clip, seconds) = rig.asset.state_clip(o.action)?;
    let override_dur = o.var("animDur");
    let dur = if override_dur > 0.0 { override_dur } else { seconds };
    Some((clip.to_string(), dur.max(0.05), o.var("once") != 0.0))
}

#[allow(clippy::too_many_arguments)]
fn sync_views(
    mut commands: Commands,
    sim: Res<GameSim>,
    root: Res<AssetRoot>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut assets: ResMut<EnemyAssets>,
    mut views: ResMut<EnemyViews>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
) {
    let objects = &sim.mario.objects.objects;
    let mpos = sim.mario.pos;
    // The nearest drawable enemies.
    let mut want: Vec<(usize, f64)> = objects
        .iter()
        .enumerate()
        .filter(|(_, o)| o.behavior == ObjectBehaviorKind::BkEnemy && o.active && o.visible)
        .map(|(i, o)| (i, jumpman_sim::math::dist_3d(&o.pos, &mpos)))
        .filter(|(_, d)| *d < VIEW_RANGE)
        .collect();
    want.sort_by(|a, b| a.1.total_cmp(&b.1));
    want.truncate(MAX_VIEWS);
    let keep: std::collections::HashSet<usize> = want.iter().map(|(i, _)| *i).collect();
    let gone: Vec<usize> = views.views.keys().copied().filter(|i| !keep.contains(i)).collect();
    for i in gone {
        if let Some(v) = views.views.remove(&i) {
            commands.entity(v.root).despawn();
        }
    }
    let t = fixed.overstep_fraction() as f64;
    let dt = time.delta_secs_f64();
    let course = sim.course.request.id.clone();
    for (idx, _) in want {
        let o = &objects[idx];
        let kind = kind_of(o);
        if !views.views.contains_key(&idx) {
            let rig = if kind.model().is_empty() { None } else { assets.get(&root.0, &asset_key(kind, &course)) };
            if rig.is_none() && kind != Kind::Snowball {
                continue;
            }
            let v = spawn_view(&mut commands, &mut meshes, &mut materials, &mut images, kind, rig);
            views.views.insert(idx, v);
        }
        let view = views.views.get_mut(&idx).unwrap();
        // placement: interpolate the sim step like the goombas
        let cur = sim.curr.objects.get(idx).copied().unwrap_or_default();
        let prev = sim.prev.objects.get(idx).copied().unwrap_or(cur);
        let near = (0..3).all(|i| (cur.pos[i] - prev.pos[i]).abs() < 300.0);
        let pos = if near { [0, 1, 2].map(|i| prev.pos[i] + (cur.pos[i] - prev.pos[i]) * t) } else { cur.pos };
        if let Ok(mut tf) = transforms.get_mut(view.root) {
            tf.translation = to_bevy(pos);
            tf.rotation = Quat::from_rotation_y(jumpman_sim::math::angle_to_radians(o.move_yaw) as f32);
            let spin = if view.kind == Kind::Snowball { o.var("spin") as f32 } else { 0.0 };
            tf.rotation *= Quat::from_rotation_x(spin.to_radians());
            tf.scale = Vec3::splat(S as f32 * o.scale as f32 * SCALE);
        }
        if let Ok(mut v) = visibility.get_mut(view.root) {
            *v = Visibility::Inherited;
        }
        let Some(rig) = view.rig.clone() else { continue };
        // animation: the state's clip, restarted on a state change, cross-faded
        let Some((clip, dur, once)) = clip_for(&rig, o) else { continue };
        if view.clip != clip || view.last_state != o.action {
            if view.clip != clip || once {
                view.from = view.pose.clone();
                view.blend = if view.clip.is_empty() { 1.0 } else { 0.0 };
                view.progress = 0.0;
            }
            view.clip = clip.clone();
            view.last_state = o.action;
        } else {
            view.progress += dt / dur;
            view.progress = if once { view.progress.min(0.999) } else { view.progress % 1.0 };
        }
        view.blend = (view.blend + dt / BLEND).min(1.0);
        let mut target = Pose::new(rig.asset.bones.len());
        rig.evaluate(&clip, view.progress, &mut target);
        let b = view.blend as f32;
        for i in 0..view.pose.rot.len() {
            view.pose.rot[i] = view.from.rot[i].slerp(target.rot[i], b);
            view.pose.scale[i] = view.from.scale[i].lerp(target.scale[i], b);
            view.pose.trans[i] = view.from.trans[i].lerp(target.trans[i], b);
        }
        let mats = rig.matrices(&view.pose);
        for ((_, handle), batch) in view.batches.iter().zip(&rig.asset.batches) {
            let Some(mut mesh) = meshes.get_mut(handle) else { continue };
            let positions: Vec<[f32; 3]> = batch
                .vertices
                .iter()
                .map(|v| mats[(v[9].max(0.0) as usize).min(mats.len() - 1)].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).to_array())
                .collect();
            let normals = crate::characters::flat_normals(&positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Loot
// ---------------------------------------------------------------------------------------------

/// Banjo's share of a defeated enemy (the others get the sim's coins and hearts): musical notes, with an egg or
/// red feather now and then. Port-only: BK enemies drop nothing, but the sandbox has no other income of notes.
fn enemy_drops(mut sim: ResMut<GameSim>) {
    use jumpman_sim::banjo_ext::PickupKind;
    if !sim.as_banjo {
        return;
    }
    let mut drops = vec![];
    for (i, o) in sim.mario.objects.objects.iter().enumerate() {
        if o.behavior == ObjectBehaviorKind::BkEnemy && o.var("died") != 0.0 && o.var("dropped") == 0.0 && kind_of(o) != Kind::Snowball {
            drops.push((i, o.pos, kind_of(o)));
        }
    }
    for (i, pos, kind) in drops {
        sim.mario.objects.objects[i].set_var("dropped", 1.0);
        if kind == Kind::Beehive {
            sim.mario.banjo.drop_pickup(PickupKind::Honeycomb, [pos[0], pos[1] + 60.0, pos[2]]);
            continue;
        }
        for n in 0..2 {
            let a = (i as f64 * 1.7 + n as f64 * 3.1) % std::f64::consts::TAU;
            sim.mario.banjo.drop_pickup(PickupKind::Note, [pos[0] + a.sin() * 70.0, pos[1] + 60.0, pos[2] + a.cos() * 70.0]);
        }
        match i % 3 {
            0 => sim.mario.banjo.drop_pickup(PickupKind::Egg, [pos[0], pos[1] + 60.0, pos[2]]),
            1 => sim.mario.banjo.drop_pickup(PickupKind::RedFeather, [pos[0], pos[1] + 60.0, pos[2]]),
            _ => {}
        }
    }
}
