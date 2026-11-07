//! World-space presentation of progression state: star / key models, the star-dance model and
//! the castle paintings (with their ripples), plus the `JUMPMAN_FLOW` debug hook.
use super::flow::{Flow, Progress};
use super::menus::{ActSelector, BonusSave, CompletedCourse, CourseResult};
use super::paintings::PaintingSpec;
use super::CourseScoped;
use crate::game::runner::GameSim;
use crate::render::{scene, to_bevy, AssetRoot, SCALE};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

pub struct ProgressViewsPlugin;

impl Plugin for ProgressViewsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StarViews>()
            .init_resource::<PaintingViews>()
            .add_systems(Update, (sync_star_views, sync_dance_model, sync_painting_views, sync_door_views).before(crate::CameraSet));
    }
}

pub fn rebuild(world: &mut World) {
    world.insert_resource(StarViews::default());
    world.insert_resource(PaintingViews::default());
}

const STAR: &str = "actors/star/star_geo.glb";
const GHOST: &str = "actors/transparent_star/transparent_star_geo.glb";
const KEY: &str = "actors/bowser_key/bowser_key_geo.glb";

#[derive(Component)]
struct StarView {
    /// Index into the director's rewards (`usize::MAX` = the 100-coin star).
    index: usize,
    gold: Entity,
    ghost: Entity,
}

#[derive(Resource, Default)]
struct StarViews {
    key: Option<(String, i32, usize)>,
}

fn sync_star_views(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    progress: Res<Progress>,
    sim: Res<GameSim>,
    mut views: ResMut<StarViews>,
    existing: Query<Entity, With<StarView>>,
    mut roots: Query<(&StarView, &mut Transform, &mut Visibility)>,
    mut parts: Query<&mut Visibility, Without<StarView>>,
) {
    let d = &progress.director;
    let count = d.rewards.len() + usize::from(d.hundred.is_some());
    let key = (d.course.clone(), d.act, count);
    if views.key.as_ref() != Some(&key) {
        for e in &existing {
            commands.entity(e).despawn();
        }
        views.key = Some(key);
        for index in 0..count {
            let is_hundred = index >= d.rewards.len();
            let reward_key = index == 0 && (d.reward_key.is_some());
            let model = if reward_key { KEY } else { STAR };
            let grand = index == 0 && d.course == "bowser_3";
            let child_tf = if reward_key {
                Transform { translation: Vec3::Y * 1.65, rotation: Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2), scale: Vec3::splat(0.5) }
            } else if grand {
                Transform { translation: Vec3::Y * 1.1, scale: Vec3::splat(2.0), ..default() }
            } else {
                Transform::default()
            };
            let mut gold = Entity::PLACEHOLDER;
            let mut ghost = Entity::PLACEHOLDER;
            let owner = commands
                .spawn((Transform::default(), Visibility::Hidden, CourseScoped))
                .with_children(|p| {
                    gold = if root.exists(model) { p.spawn((scene(&assets, model), child_tf)).id() } else { p.spawn(Transform::default()).id() };
                    ghost = if root.exists(GHOST) { p.spawn((scene(&assets, GHOST), Visibility::Hidden)).id() } else { p.spawn(Transform::default()).id() };
                })
                .id();
            let _ = is_hundred;
            commands.entity(owner).insert(StarView { index: if is_hundred { usize::MAX } else { index }, gold, ghost });
        }
        return;
    }
    let spin = if d.reward_key.is_some() || d.course == "bowser_3" { std::f32::consts::PI / 32.0 } else { std::f32::consts::PI / 16.0 };
    for (view, mut tf, mut vis) in &mut roots {
        let reward = if view.index == usize::MAX { d.hundred.as_ref() } else { d.rewards.get(view.index) };
        let Some(r) = reward else { continue };
        tf.translation = to_bevy(r.pos);
        let yaw = if r.yaw != 0.0 { r.yaw as f32 * std::f32::consts::PI / 32768.0 } else { sim.ticks as f32 * spin };
        tf.rotation = Quat::from_rotation_y(yaw);
        *vis = if r.visible { Visibility::Inherited } else { Visibility::Hidden };
        if let Ok(mut g) = parts.get_mut(view.gold) {
            *g = if r.saved { Visibility::Hidden } else { Visibility::Inherited };
        }
        if let Ok(mut g) = parts.get_mut(view.ghost) {
            *g = if r.saved { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

#[derive(Component)]
struct DanceModel;

fn sync_dance_model(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    progress: Res<Progress>,
    mut model: Query<(Entity, &mut Transform, &mut Visibility), With<DanceModel>>,
) {
    let Flow::Dance(d) = &progress.flow else {
        for (e, ..) in &model {
            commands.entity(e).despawn();
        }
        return;
    };
    match model.single_mut() {
        Ok((_, mut tf, mut vis)) => {
            tf.translation = to_bevy(d.model_pos);
            tf.rotation = Quat::from_rotation_y(d.model_rot as f32);
            tf.scale = Vec3::splat(d.model_scale as f32);
            *vis = if d.model_visible { Visibility::Inherited } else { Visibility::Hidden };
        }
        Err(_) => {
            if root.exists(STAR) {
                commands.spawn((DanceModel, scene(&assets, STAR), Transform::default(), Visibility::Hidden, CourseScoped));
            }
        }
    }
}

// ------------------------------------------------------------------ doors

/// Door props are plain scenes spawned by `course_view`; bind them by position and follow the
/// collision leaf (sliding star doors move, hinged doors disappear while open).
fn sync_door_views(
    progress: Res<Progress>,
    sim: Res<GameSim>,
    mut bound: Local<(String, i32, Vec<Option<Entity>>, Vec<Option<Entity>>)>,
    mut props: Query<(Entity, &mut Transform, &mut Visibility), (With<SceneRoot>, With<CourseScoped>)>,
) {
    let req = &sim.course.request;
    let near = |a: Vec3, b: Vec3| a.distance(b) < 0.02;
    if bound.0 != req.id || bound.1 != req.area || bound.2.len() != progress.doors.as_ref().map_or(0, |d| d.leaves.len()) {
        let found = |pos: [f64; 3], props: &Query<(Entity, &mut Transform, &mut Visibility), (With<SceneRoot>, With<CourseScoped>)>| props.iter().find(|(_, t, _)| near(t.translation, to_bevy(pos))).map(|(e, ..)| e);
        let rooms = progress.doors.as_ref().map_or(vec![], |d| d.leaves.iter().map(|l| found(l.door.pos, &props)).collect());
        let entrances = progress.data.doors.iter().map(|d| found(d.pos, &props)).collect();
        *bound = (req.id.clone(), req.area, rooms, entrances);
    }
    if let Some(doors) = &progress.doors {
        for (leaf, entity) in doors.leaves.iter().zip(&bound.2) {
            let Some(Ok((_, mut tf, mut vis))) = entity.map(|e| props.get_mut(e)) else { continue };
            if leaf.door.sliding {
                tf.translation = to_bevy(leaf.pos);
            } else {
                *vis = if leaf.timer >= 0 { Visibility::Hidden } else { Visibility::Inherited };
            }
        }
    }
    let entering = match &progress.flow {
        Flow::Entering { door, frame, locked } if !*locked && *frame > 0 => Some(*door),
        _ => None,
    };
    for (i, entity) in bound.3.iter().enumerate() {
        if let Some(Ok((_, _, mut vis))) = entity.map(|e| props.get_mut(e)) {
            *vis = if entering == Some(i) { Visibility::Hidden } else { Visibility::Inherited };
        }
    }
}

// ------------------------------------------------------------------ paintings

#[derive(Component)]
struct PaintingView {
    spec: usize,
}

#[derive(Component)]
struct PaintingPartView {
    painting: usize,
    /// Per vertex: (x, y, movable) in painting units.
    vertices: Vec<[f64; 3]>,
}

#[derive(Resource, Default)]
struct PaintingViews {
    built: bool,
    rippling: Option<usize>,
}

fn sync_painting_views(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    progress: Res<Progress>,
    sim: Res<GameSim>,
    mut views: ResMut<PaintingViews>,
    mut roots: Query<(&PaintingView, &mut Visibility, &mut Transform)>,
    parts: Query<(&PaintingPartView, &Mesh3d)>,
) {
    let Some(anim) = &progress.paintings else { return };
    if !views.built {
        views.built = true;
        for (i, spec) in anim.specs.iter().enumerate() {
            let rotation = Quat::from_euler(EulerRot::XYZ, spec.rotation[0].to_radians() as f32, spec.rotation[1].to_radians() as f32, 0.0);
            commands
                .spawn((PaintingView { spec: i }, Transform { translation: to_bevy(spec.pos), rotation, scale: Vec3::splat(spec.size as f32 / 614.0 * SCALE) }, Visibility::Hidden, CourseScoped))
                .with_children(|p| {
                    for part in &spec.parts {
                        let Some(shape) = progress.painting_art.shapes.get(&part.shape) else { continue };
                        let mesh = painting_mesh(spec, shape, &|_| 0.0);
                        let texture: Handle<Image> = assets.load(format!("extracted/levels/castle_inside/{}", part.texture));
                        let material = StandardMaterial {
                            base_color: Color::srgba(1.0, 1.0, 1.0, spec.opacity as f32),
                            base_color_texture: Some(texture),
                            unlit: true,
                            alpha_mode: if spec.opacity < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque },
                            cull_mode: None,
                            ..default()
                        };
                        p.spawn((
                            Mesh3d(meshes.add(mesh)),
                            MeshMaterial3d(materials.add(material)),
                            PaintingPartView { painting: i, vertices: shape.vertices.iter().map(|v| [v[0], v[1], v[2]]).collect() },
                        ));
                    }
                });
        }
        return;
    }
    // Visibility by room, DDD painting slide.
    let room = sim.mario.floor.as_ref().and_then(|f| f.room).filter(|r| *r >= 0).map(|r| r as usize);
    let visible: &[i32] = room.and_then(|r| progress.data.room_paintings.get(r)).map_or(&[], |v| v.as_slice());
    for (view, mut vis, mut tf) in &mut roots {
        let spec = &anim.specs[view.spec];
        *vis = if visible.contains(&spec.index) { Visibility::Inherited } else { Visibility::Hidden };
        if spec.name == "ddd" {
            if let Some(x) = anim.docks_x {
                tf.translation.x = x as f32 * SCALE;
            }
        }
    }
    // Re-deform the rippling painting (and flatten the previous one).
    let active = anim.active;
    if active.is_some() || views.rippling.is_some() || anim.dirty {
        for (part, mesh) in &parts {
            let mine = Some(part.painting) == active;
            if !mine && Some(part.painting) != views.rippling {
                continue;
            }
            let Some(mesh) = meshes.get_mut(&mesh.0) else { continue };
            let positions: Vec<[f32; 3]> = part
                .vertices
                .iter()
                .map(|&[x, y, movable]| {
                    let z = if mine && movable != 0.0 { anim.displacement(x, y) } else { 0.0 };
                    [x as f32, y as f32, z as f32]
                })
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        }
        views.rippling = active;
    }
}

fn painting_mesh(spec: &PaintingSpec, shape: &super::paintings::PaintingShape, z: &dyn Fn(usize) -> f32) -> Mesh {
    let positions: Vec<[f32; 3]> = shape.vertices.iter().enumerate().map(|(i, v)| [v[0] as f32, v[1] as f32, z(i)]).collect();
    let uvs: Vec<[f32; 2]> = shape
        .vertices
        .iter()
        .map(|v| [((v[3] / 32.0 + 0.5) / spec.dimensions[0]) as f32, (1.0 - (v[4] / 32.0 + 0.5) / spec.dimensions[1]) as f32])
        .collect();
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(shape.triangles.clone()));
    mesh
}

// ------------------------------------------------------------------ debug hook

/// `JUMPMAN_FLOW=<name>`: put the flow into a state for screenshots.
pub fn debug_flow(name: &str, sim: &mut GameSim, p: &mut Progress) {
    match name {
        "selector" => {
            let collected = [true, true, false, false, false, false];
            let mut sel = ActSelector::new("bob", Some("WARP_NODE_0A".into()), collected, true, 37);
            sel.timer = 20;
            p.flow = Flow::ActSelect(sel);
        }
        "result" => {
            let mut r = CourseResult::new(CompletedCourse { title: "Big Bob-omb on the Summit".into(), heading: "COURSE 1".into(), coins: 42, high_score: true, key: false });
            r.shown = 42;
            r.timer = 130;
            r.options_visible = true;
            r.buttons_enabled = true;
            p.flow = Flow::Result(r);
        }
        "gameover" => p.flow = Flow::GameOver { timer: 30 },
        "credits" => p.flow = Flow::Credits { timer: 30 },
        "ending" => {
            sim.mario.set_action(jumpman_sim::state::ACT_JUMBO_STAR_CUTSCENE, 0.0);
        }
        "bonus" => {
            p.flow = Flow::Dance(super::flow::Dance {
                bonus: true,
                hundred: true,
                yaw: 0.0,
                diameter: 100.0,
                model_visible: false,
                model_pos: [0.0; 3],
                model_scale: 1.0,
                model_rot: 0.0,
                prompt: Some(BonusSave::new(true)),
                frame: 80,
                saved: false,
            })
        }
        "death" => {
            sim.mario.lives = 3.0;
            sim.mario.respawn(false);
        }
        "star" => {
            // Stand on the primary star with the mission complete.
            p.director.mission_complete = true;
            p.director.star_ready = true;
            p.director.rewards[0].visible = true;
            let pos = p.director.rewards[0].pos;
            sim.mario.pos = pos;
            sim.mario.pos[1] -= 40.0;
        }
        other => warn!("unknown JUMPMAN_FLOW '{other}'"),
    }
}
