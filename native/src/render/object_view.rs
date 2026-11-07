//! Sim object presentation (`CourseObjects.syncViews`): one scene per [`SimObject`] with a
//! model, positioned from the interpolated sim snapshot, with behavior-specific rigs: chain chomp
//! jaw frames and pitch, koopa shell/bare swap and animation index tables, snowman pitch,
//! racing-penguin floor alignment, chest lid, wing-box outline, box scales and cannon barrels.
//!
//! TODO(object-views): actor shadows.
use super::billboard::Billboard;
use super::{glb_animation_count, scene, to_bevy, AssetRoot};
use crate::game::runner::GameSim;
use crate::objects::colliders::WingBoxes;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use jumpman_sim::behaviors::koopa::KOOPA_SQUISHED;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::object::{ObjectBehaviorKind as K, SimObject};
use std::collections::HashSet;

pub struct ObjectViewPlugin;

impl Plugin for ObjectViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ObjectViews>()
            .add_systems(Update, (spawn_object_views, sync_object_views, sync_object_parts, drive_anims).chain().before(crate::CameraSet))
            .add_systems(Update, double_sided);
    }
}

#[derive(Component)]
pub struct ObjectView(pub usize);

/// Which part of an object an entity is (children of the [`ObjectView`] root).
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Body,
    BareKoopa,
    ChestLid,
    WingOutline,
}

#[derive(Component)]
struct PartOf(usize);

/// Clip driver for a glb scene: every clip is a graph node; `want` picks the playing one.
#[derive(Component)]
pub struct AnimRig {
    pub clips: Vec<Handle<AnimationClip>>,
    nodes: Vec<AnimationNodeIndex>,
    players: Vec<Entity>,
    pub want: usize,
    active: Option<usize>,
    pub speed: f32,
    /// Hold the clip at this time (seconds) instead of playing.
    pub seek: Option<f32>,
    pub looped: bool,
}

impl AnimRig {
    pub fn new(assets: &AssetServer, model: &str, count: usize) -> Self {
        Self {
            clips: (0..count).map(|i| assets.load(GltfAssetLabel::Animation(i).from_asset(model.to_owned()))).collect(),
            nodes: vec![],
            players: vec![],
            want: 0,
            active: None,
            speed: 1.0,
            seek: None,
            looped: true,
        }
    }
}

/// Render both faces of the model's materials (grills and gates).
#[derive(Component)]
struct DoubleSided;

#[derive(Resource, Default)]
pub struct ObjectViews {
    /// Entity per object arena index (None = no model).
    entities: Vec<Option<Entity>>,
    missing: HashSet<String>,
}

/// Spawn `model` as a scene entity with an [`AnimRig`] when the glb has animations.
pub fn spawn_model<'a>(commands: &'a mut Commands, assets: &AssetServer, root: &AssetRoot, model: &str, looped: bool) -> EntityCommands<'a> {
    let mut e = commands.spawn((scene(assets, model), Transform::default(), Visibility::Inherited));
    let count = glb_animation_count(&root.0.join(model));
    if count > 0 {
        let mut rig = AnimRig::new(assets, model, count);
        rig.looped = looped;
        e.insert(rig);
        e.observe(anim_ready);
    }
    e
}

fn spawn_object_views(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, sim: Res<GameSim>, mut views: ResMut<ObjectViews>) {
    let objects = &sim.mario.objects.objects;
    // The arena shrinks when the manager resets; drop views of vanished dynamic objects.
    while views.entities.len() > objects.len() {
        if let Some(Some(e)) = views.entities.pop() {
            commands.entity(e).despawn();
        }
    }
    for idx in views.entities.len()..objects.len() {
        let o = &objects[idx];
        let mut entity = None;
        if let Some(model) = o.model.as_deref().filter(|m| !m.is_empty()) {
            if root.exists(model) {
                let mut view = commands.spawn((Transform::default(), Visibility::Hidden, ObjectView(idx)));
                if o.billboard {
                    view.insert(Billboard::Full);
                }
                let view_id = view.id();
                let mut body = spawn_model(&mut commands, &assets, &root, model, o.behavior != K::ChainChomp);
                body.insert((Part::Body, PartOf(idx)));
                if matches!(o.behavior, K::OpenableGrill | K::GrillDoor | K::ChainChompGate) {
                    body.insert(DoubleSided);
                }
                let body_id = body.id();
                commands.entity(view_id).add_child(body_id);
                let extra = |commands: &mut Commands, path: &str, part: Part, looped: bool| {
                    if !root.exists(path) {
                        warn!("object part model missing: {path}");
                        return;
                    }
                    let mut e = spawn_model(commands, &assets, &root, path, looped);
                    e.insert((part, PartOf(idx)));
                    let id = e.id();
                    commands.entity(view_id).add_child(id);
                };
                match o.behavior {
                    K::Koopa => extra(&mut commands, "actors/koopa/koopa_without_shell_geo.glb", Part::BareKoopa, true),
                    K::WingBox => extra(&mut commands, "actors/exclamation_box_outline/exclamation_box_outline_geo.glb", Part::WingOutline, false),
                    K::Chest => extra(&mut commands, "actors/treasure_chest/treasure_chest_lid_geo.glb", Part::ChestLid, false),
                    _ => {}
                }
                entity = Some(view_id);
            } else if views.missing.insert(model.to_owned()) {
                warn!("object model missing: {model}");
            }
        }
        views.entities.push(entity);
    }
}

fn anim_ready(
    trigger: Trigger<SceneInstanceReady>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
    mut rigs: Query<&mut AnimRig>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
) {
    let Ok(mut rig) = rigs.get_mut(trigger.target()) else { return };
    let (graph, nodes) = AnimationGraph::from_clips(rig.clips.iter().cloned());
    let graph = graphs.add(graph);
    rig.nodes = nodes;
    for child in children.iter_descendants(trigger.target()) {
        if players.contains(child) {
            commands.entity(child).insert((AnimationPlayer::default(), AnimationGraphHandle(graph.clone())));
            rig.players.push(child);
        }
    }
}

fn drive_anims(mut rigs: Query<&mut AnimRig>, mut players: Query<&mut AnimationPlayer>) {
    for mut rig in &mut rigs {
        if rig.nodes.is_empty() {
            continue;
        }
        let want = rig.want.min(rig.nodes.len() - 1);
        let node = rig.nodes[want];
        let switch = rig.active != Some(want);
        for &p in &rig.players.clone() {
            let Ok(mut player) = players.get_mut(p) else { continue };
            if switch {
                player.stop_all();
                let a = player.play(node);
                if rig.looped && rig.seek.is_none() {
                    a.repeat();
                }
            }
            if let Some(a) = player.animation_mut(node) {
                a.set_speed(rig.speed);
                if let Some(t) = rig.seek {
                    a.seek_to(t);
                    a.pause();
                }
            }
        }
        rig.active = Some(want);
    }
}

/// `DoubleSide` for grill and gate models once their scene exists.
pub fn double_sided(
    mut ready: Query<Entity, (With<DoubleSided>, Without<DoneDoubleSided>)>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for e in &mut ready {
        let mut found = false;
        for c in children.iter_descendants(e) {
            if let Ok(h) = handles.get(c) {
                if let Some(m) = materials.get_mut(&h.0) {
                    m.double_sided = true;
                    m.cull_mode = None;
                    found = true;
                }
            }
        }
        if found {
            commands.entity(e).insert(DoneDoubleSided);
        }
    }
}

#[derive(Component)]
pub struct DoneDoubleSided;

fn euler_yxz(yaw: f64, pitch: f64, roll: f64) -> Quat {
    Quat::from_euler(EulerRot::YXZ, angle_to_radians(yaw) as f32, angle_to_radians(pitch) as f32, angle_to_radians(roll) as f32)
}

fn object_rotation(o: &SimObject, sim: &GameSim) -> Quat {
    match o.behavior {
        K::ChainChomp => euler_yxz(o.move_yaw, o.var("facePitch"), 0.0),
        K::SnowmanBody => euler_yxz(o.face_yaw, o.var("pitch"), 0.0),
        K::CannonBarrel => euler_yxz(o.face_yaw, o.var("pitch"), 0.0),
        K::WingCap | K::KoopaShell | K::WaterBombCannon => euler_yxz(o.face_yaw, 0.0, 0.0),
        K::RacingPenguin => {
            let floor = sim.mario.world.find_floor(o.pos[0], o.pos[1], o.pos[2]);
            match floor.surface {
                Some(s) => {
                    // Basis (left, up, forward) from the floor normal and the facing direction.
                    let yaw = angle_to_radians(o.face_yaw) as f32;
                    let up = Vec3::new(s.normal[0] as f32, s.normal[1] as f32, s.normal[2] as f32);
                    let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                    let left = up.cross(forward).normalize_or_zero();
                    let forward = left.cross(up).normalize_or_zero();
                    if left == Vec3::ZERO {
                        euler_yxz(o.move_yaw, 0.0, 0.0)
                    } else {
                        Quat::from_mat3(&Mat3::from_cols(left, up, forward))
                    }
                }
                None => euler_yxz(o.move_yaw, 0.0, 0.0),
            }
        }
        _ => euler_yxz(o.move_yaw, 0.0, 0.0),
    }
}

fn object_scale(o: &SimObject) -> Vec3 {
    let mut scale = Vec3::splat(o.scale as f32);
    match o.behavior {
        K::Koopa if o.action == KOOPA_SQUISHED => {
            let height = (0.3f64).max(1.0 - 0.14 * o.action_timer) as f32;
            scale *= Vec3::new(2.0 - height, height, 2.0 - height);
        }
        K::StarBox | K::WingBox | K::WaterBomb => {
            let width = if o.has_var("width") { o.var("width") as f32 } else { 1.0 };
            let height = if o.has_var("height") { o.var("height") as f32 } else { 1.0 };
            scale.x *= width;
            scale.z = scale.x;
            scale.y *= height;
        }
        _ => {}
    }
    scale
}

fn sync_object_views(sim: Res<GameSim>, fixed: Res<Time<Fixed>>, mut views: Query<(&ObjectView, &mut Transform, &mut Visibility, Has<Billboard>)>) {
    let t = fixed.overstep_fraction() as f64;
    let objects = &sim.mario.objects.objects;
    for (view, mut tf, mut vis, billboard) in &mut views {
        let Some(o) = objects.get(view.0) else { continue };
        let cur = sim.curr.objects.get(view.0).copied().unwrap_or_default();
        let prev = sim.prev.objects.get(view.0).copied().unwrap_or(cur);
        // Objects teleport (respawn, held, warps): only interpolate short moves.
        let near = (0..3).all(|i| (cur.pos[i] - prev.pos[i]).abs() < 300.0);
        let pos = if near { [0, 1, 2].map(|i| prev.pos[i] + (cur.pos[i] - prev.pos[i]) * t) } else { cur.pos };
        let mut p = pos;
        p[1] += o.graph_y_offset;
        tf.translation = to_bevy(p);
        if !billboard {
            tf.rotation = object_rotation(o, &sim);
        }
        let mut scale = object_scale(o);
        if o.behavior == K::RacingPenguin && o.scale == 8.0 {
            // `view.group.scale.y = baseScale * 5` replaces the uniform 8.
            scale.y = 5.0;
        }
        tf.scale = scale;
        *vis = if o.active && o.visible { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// Penguin clip order differs from the sim's animation numbering (`penguin_seg5_anims_05008B74`).
fn clip_index(o: &SimObject) -> usize {
    let a = o.var("animation") as usize;
    match o.behavior {
        K::BabyPenguin | K::TuxiesMother | K::RacingPenguin => [3, 0, 1, 2].get(a).copied().unwrap_or(0),
        K::Bowser | K::KoopaTheQuick | K::Koopa => a,
        _ => 0,
    }
}

fn sync_object_parts(
    sim: Res<GameSim>,
    wing: Res<WingBoxes>,
    mut parts: Query<(&Part, &PartOf, &mut Visibility, &mut Transform, Option<&mut AnimRig>)>,
) {
    let objects = &sim.mario.objects.objects;
    for (part, owner, mut vis, mut tf, rig) in &mut parts {
        let Some(o) = objects.get(owner.0) else { continue };
        let bare = o.var("unshelled") != 0.0;
        match part {
            Part::Body => {
                if o.behavior == K::Koopa {
                    *vis = if bare { Visibility::Hidden } else { Visibility::Inherited };
                }
                if o.behavior == K::WingBox {
                    *vis = if wing.unlocked { Visibility::Inherited } else { Visibility::Hidden };
                }
                if let Some(mut rig) = rig {
                    match o.behavior {
                        K::ChainChomp => {
                            rig.want = 0;
                            rig.seek = Some((o.var("jawFrame") / 30.0) as f32);
                        }
                        K::Koopa | K::BabyPenguin | K::TuxiesMother | K::RacingPenguin | K::Bowser | K::KoopaTheQuick => {
                            rig.want = clip_index(o);
                            rig.speed = if o.behavior == K::KoopaTheQuick && o.var("animation") == 9.0 && o.action == 3 { (o.forward_vel * 0.09) as f32 } else { 1.0 };
                        }
                        _ => {}
                    }
                }
            }
            Part::BareKoopa => {
                *vis = if bare { Visibility::Inherited } else { Visibility::Hidden };
                if let Some(mut rig) = rig {
                    rig.want = clip_index(o);
                }
            }
            Part::WingOutline => *vis = if wing.unlocked { Visibility::Hidden } else { Visibility::Inherited },
            Part::ChestLid => {
                tf.translation = to_bevy([0.0, 102.0, -77.0]);
                tf.rotation = Quat::from_rotation_x(angle_to_radians(o.var("lidPitch")) as f32);
            }
        }
    }
}
