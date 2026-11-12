//! Drawing the ported enemies for the special modes. A mode publishes [`FoeItem`]s into the
//! [`FoeBoard`] each frame (its bridge keeps a `Vec<FoeItem>` and a small copy system moves them
//! over); [`sync_foes`] owns the entities: it pools one model per item id, picks the animation
//! clip from the pose, hides what is not published and never touches a mode's own pools.
//!
//! OoT species are glb scenes driven by `AnimRig` (the same driver the course objects use). BK
//! species are CPU-skinned rigs sampled by `bk_enemies::Rig`, sharing its texture / mesh builders.
//! Both are sized from the species' course hurt-box so a mode only says how big the radius is in
//! its own units.
use super::foes::{Pose, Species, Src};
use super::XpEntity;
use crate::bk_enemies::{empty_mesh, make_image, material, EnemyAssets, Pose as BkPose, Rig};
use crate::render::object_view::{spawn_model, AnimRig};
use crate::render::{AssetRoot, SCALE};
use bevy::prelude::*;
use bevy::camera::visibility::NoFrustumCulling;
use std::collections::HashMap;
use std::sync::Arc;

/// Skinned BK rigs re-posed per frame (the rest keep their last pose).
pub const MAX_SKINNED: usize = if cfg!(any(target_os = "ios", target_os = "android")) { 16 } else { 28 };
pub const GLB_DIR: &str = "levels/oot_enemies";

/// One enemy to draw this frame. `pos` is in Bevy units, `radius` in the mode's Jumpman units.
#[derive(Clone, Copy, Debug)]
pub struct FoeItem {
    pub id: u32,
    pub species: Species,
    pub pos: Vec3,
    pub yaw: f32,
    pub radius: f32,
    pub pose: Pose,
    /// Extra size pulse (1 = none) for hit flashes and telegraphs.
    pub pulse: f32,
    /// Pose clock, seconds since the pose started (death plays once, from 0).
    pub age: f32,
}

#[derive(Resource, Default)]
pub struct FoeBoard {
    pub items: Vec<FoeItem>,
}

/// Marker on every pooled foe root.
#[derive(Component)]
pub struct FoeRoot;

struct BkNode {
    rig: Arc<Rig>,
    meshes: Vec<Handle<Mesh>>,
    pose: BkPose,
    clip: String,
    progress: f64,
}

struct Node {
    root: Entity,
    species: Species,
    glb: Option<Entity>,
    bk: Option<BkNode>,
}

#[derive(Resource, Default)]
pub struct FoeState {
    live: HashMap<u32, Node>,
    pool: HashMap<Species, Vec<Node>>,
    clips: HashMap<Species, Vec<String>>,
}

impl FoeState {
    pub fn live_count(&self) -> usize {
        self.live.len()
    }
}

pub struct FoesViewPlugin;

impl Plugin for FoesViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FoeBoard>().init_resource::<FoeState>().add_systems(Update, sync_foes.after(super::run_frame));
    }
}

/// Size factor of a species' model for a wanted hurt-box radius (Jumpman units).
pub fn size_factor(species: Species, radius: f32) -> f32 {
    radius / species.course_radius().max(1.0)
}

#[allow(clippy::too_many_arguments)]
pub fn sync_foes(
    mut commands: Commands,
    mut board: ResMut<FoeBoard>,
    mut state: ResMut<FoeState>,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    time: Res<Time>,
    mut bk: ResMut<EnemyAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut roots: Query<(&mut Transform, &mut Visibility), With<FoeRoot>>,
    mut rigs: Query<&mut AnimRig>,
) {
    // Entities despawned by the mode's exit: forget them.
    state.live.retain(|_, n| roots.contains(n.root));
    for list in state.pool.values_mut() {
        list.retain(|n| roots.contains(n.root));
    }
    let items = std::mem::take(&mut board.items);
    let seen: std::collections::HashSet<u32> = items.iter().map(|i| i.id).collect();
    // Return what is no longer published to the pool.
    let gone: Vec<u32> = state.live.keys().copied().filter(|id| !seen.contains(id)).collect();
    for id in gone {
        if let Some(n) = state.live.remove(&id) {
            if let Ok((_, mut v)) = roots.get_mut(n.root) {
                *v = Visibility::Hidden;
            }
            state.pool.entry(n.species).or_default().push(n);
        }
    }
    let dt = time.delta_secs_f64();
    let mut skinned = 0;
    for item in items {
        // A recycled slot may now hold another species.
        if state.live.get(&item.id).is_some_and(|n| n.species != item.species) {
            let n = state.live.remove(&item.id).unwrap();
            if let Ok((_, mut v)) = roots.get_mut(n.root) {
                *v = Visibility::Hidden;
            }
            state.pool.entry(n.species).or_default().push(n);
        }
        if !state.live.contains_key(&item.id) {
            let node = match state.pool.get_mut(&item.species).and_then(Vec::pop) {
                Some(n) => n,
                None => spawn_node(&mut commands, &assets, &root, &mut meshes, &mut materials, &mut images, &mut bk, item.species),
            };
            state.live.insert(item.id, node);
        }
        // Glb clip names, read once per species.
        if let Src::Glb(stem) = item.species.src() {
            if !state.clips.contains_key(&item.species) {
                let names = crate::zelda::glb::animation_names(&root.0.join(format!("{GLB_DIR}/{stem}.glb")));
                state.clips.insert(item.species, names);
            }
        }
        let names = state.clips.get(&item.species).cloned().unwrap_or_default();
        let node = state.live.get_mut(&item.id).unwrap();
        let k = size_factor(item.species, item.radius) * item.pulse;
        if let Ok((mut tf, mut vis)) = roots.get_mut(node.root) {
            tf.translation = item.pos;
            tf.rotation = Quat::from_rotation_y(item.yaw);
            tf.scale = Vec3::splat(match node.bk {
                Some(_) => k * item.species.model_base() * SCALE,
                None => k,
            });
            *vis = Visibility::Inherited;
        }
        let clip = item.species.clip(item.pose);
        if let Some(gm) = node.glb {
            if let Ok(mut rig) = rigs.get_mut(gm) {
                let want = names.iter().position(|n| n == clip).unwrap_or(0);
                rig.looped = !Species::once(item.pose);
                if rig.want != want {
                    rig.want = want;
                }
            }
        }
        if let Some(b) = &mut node.bk {
            if skinned < MAX_SKINNED {
                skinned += 1;
                if b.clip != clip {
                    b.clip = clip.to_string();
                    b.progress = 0.0;
                } else {
                    b.progress += dt / item.species.clip_secs(item.pose);
                }
                b.progress = if Species::once(item.pose) { (item.age as f64 / item.species.clip_secs(item.pose)).min(0.999) } else { b.progress % 1.0 };
                b.rig.evaluate(&b.clip, b.progress, &mut b.pose);
                let mats = b.rig.matrices(&b.pose);
                for (handle, batch) in b.meshes.iter().zip(&b.rig.asset.batches) {
                    let Some(mut mesh) = meshes.get_mut(handle) else { continue };
                    let positions: Vec<[f32; 3]> = batch.vertices.iter().map(|v| mats[(v[9].max(0.0) as usize).min(mats.len() - 1)].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).to_array()).collect();
                    let normals = crate::characters::flat_normals(&positions);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_node(commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, bk: &mut EnemyAssets, species: Species) -> Node {
    let rootent = commands.spawn((XpEntity, FoeRoot, Transform::default(), Visibility::Hidden)).id();
    let mut node = Node { root: rootent, species, glb: None, bk: None };
    match species.src() {
        Src::Glb(stem) => {
            let rel = format!("{GLB_DIR}/{stem}.glb");
            if root.exists(&rel) {
                let base = crate::zelda::glb::bevy_scale(&root.0.join(&rel)) * species.model_base();
                let model = spawn_model(commands, assets, root, &rel, true).insert(Transform::from_scale(Vec3::splat(base))).id();
                commands.entity(rootent).add_child(model);
                node.glb = Some(model);
            } else {
                warn!("foe model missing: {rel}");
                add_blob(commands, meshes, materials, rootent);
            }
        }
        Src::Bk(key) => match bk.get(&root.0, key) {
            Some(rig) => {
                let mut textures: HashMap<String, Handle<Image>> = HashMap::new();
                let mut handles = vec![];
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
                    let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), NoFrustumCulling, Transform::default(), Visibility::Inherited, crate::rt_scene::RtSolid)).id();
                    commands.entity(rootent).add_child(e);
                    handles.push(mesh);
                }
                let pose = BkPose::new(rig.asset.bones.len());
                node.bk = Some(BkNode { rig, meshes: handles, pose, clip: String::new(), progress: 0.0 });
            }
            None => add_blob(commands, meshes, materials, rootent),
        },
    }
    node
}

/// Stand-in for a missing model: a coloured ball the size of a hurt-box (course units).
fn add_blob(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, parent: Entity) {
    let mesh = meshes.add(Sphere::new(60.0 * SCALE).mesh().ico(1).unwrap());
    let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.7, 0.3, 0.6), ..default() });
    let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Inherited)).id();
    commands.entity(parent).add_child(e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::foes::SPECIES;

    fn assets_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("assets")
    }

    /// Every clip a species asks for exists in its exported model, so no pose silently falls back.
    #[test]
    fn clip_names_exist_in_the_exported_models() {
        let root = assets_dir();
        for s in SPECIES {
            let poses = [Pose::Idle, Pose::Move, Pose::Attack, Pose::Hurt, Pose::Die];
            match s.src() {
                Src::Glb(stem) => {
                    let path = root.join(format!("{GLB_DIR}/{stem}.glb"));
                    assert!(path.is_file(), "{} missing", path.display());
                    let names = crate::zelda::glb::animation_names(&path);
                    for p in poses {
                        assert!(names.iter().any(|n| n == s.clip(p)), "{s:?} {p:?}: no clip {} in {names:?}", s.clip(p));
                    }
                }
                Src::Bk(key) => {
                    let path = crate::bk_enemies::model_path(&root, key);
                    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                    let asset = crate::bk_enemies::EnemyAsset::parse(&bytes).unwrap();
                    for p in poses {
                        assert!(asset.clips.contains_key(s.clip(p)), "{s:?} {p:?}: no clip {} in {:?}", s.clip(p), asset.clips.keys().collect::<Vec<_>>());
                    }
                }
            }
        }
    }
}
