//! Cannons in Bevy (`cannons.ts`): lid, base and barrel models, the step before the tick, the
//! cannon camera and the shot-from-cannon follow camera.
use super::dialog::{CameraOverride, TriggerSet};
use crate::camera::CameraRig;
use crate::game::cannons::{CannonCamera, Cannons, CANNON_BARREL, CANNON_BASE};
use crate::game::runner::GameSim;
use crate::render::{scene, to_bevy, AssetRoot};
use crate::sim_plugin::{PostTickSet, PreTickSet, SimResetMsg};
use bevy::prelude::*;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::state::*;
use std::collections::HashSet;

pub struct CannonsPlugin;

impl Plugin for CannonsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CannonsOpen>()
            .init_resource::<CannonCameraState>()
            .add_systems(Startup, setup)
            .add_systems(FixedUpdate, step_cannons.in_set(PreTickSet).after(TriggerSet))
            .add_systems(FixedUpdate, cannon_cameras.in_set(PostTickSet))
            .add_systems(Update, (reset_cannons, sync_views).chain().before(crate::CameraSet));
    }
}

#[derive(Resource)]
pub struct CannonSet(pub Cannons);

/// Courses whose cannons have been opened (`save.cannonOpen`); persisted by the save module later.
#[derive(Resource, Default)]
pub struct CannonsOpen(pub HashSet<String>);

#[derive(Resource, Default)]
struct CannonCameraState {
    camera: Option<CannonCamera>,
    previous_action: u32,
    active: bool,
}

#[derive(Component)]
struct CannonView {
    index: usize,
    part: Part,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Lid,
    Base,
    Barrel,
}

const LID_TEXTURE: &str = "extracted/actors/cannon_lid/cannon_lid.rgba16.png";

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    mut sim: ResMut<GameSim>,
    open: Res<CannonsOpen>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if sim.course.extras.cannons.is_empty() {
        return;
    }
    let sim = &mut *sim;
    let mut cannons = Cannons::new(&sim.course.extras.cannons, sim.course.extras.extra_collision.get("cannonLid"));
    cannons.reset(&mut sim.mario.world, open.0.contains(&sim.course.request.id));
    let lid_mesh = meshes.add(Plane3d::default().mesh().size(203.0 * crate::render::SCALE, 203.0 * crate::render::SCALE));
    let lid_material = materials.add(StandardMaterial {
        base_color_texture: root.exists(LID_TEXTURE).then(|| assets.load(LID_TEXTURE.to_owned())),
        unlit: true,
        ..default()
    });
    for (i, _) in cannons.entries.iter().enumerate() {
        commands.spawn((Mesh3d(lid_mesh.clone()), MeshMaterial3d(lid_material.clone()), Transform::default(), CannonView { index: i, part: Part::Lid }));
        for (part, model) in [(Part::Base, CANNON_BASE), (Part::Barrel, CANNON_BARREL)] {
            if root.exists(model) {
                commands.spawn((scene(&assets, model), Transform::default(), Visibility::Hidden, CannonView { index: i, part }));
            }
        }
    }
    commands.insert_resource(CannonSet(cannons));
}

fn step_cannons(mut sim: ResMut<GameSim>, cannons: Option<ResMut<CannonSet>>) {
    let Some(mut cannons) = cannons else { return };
    cannons.0.step(&mut sim.mario);
}

fn reset_cannons(mut resets: EventReader<SimResetMsg>, mut sim: ResMut<GameSim>, cannons: Option<ResMut<CannonSet>>, open: Res<CannonsOpen>) {
    let Some(mut cannons) = cannons else { return };
    if resets.read().count() > 0 {
        let unlocked = open.0.contains(&sim.course.request.id) || cannons.0.open;
        cannons.0.reset(&mut sim.mario.world, unlocked);
    }
}

/// `CannonCamera` while Mario aims, `followCannonShot` while he flies.
fn cannon_cameras(sim: Res<GameSim>, mut rig: ResMut<CameraRig>, mut state: ResMut<CannonCameraState>, mut camera: ResMut<CameraOverride>) {
    let m = &sim.mario;
    let before = state.previous_action;
    state.previous_action = m.action;
    if m.action == ACT_IN_CANNON && m.cannon.is_some() {
        let seat = m.cannon.clone().unwrap();
        let cam = state.camera.get_or_insert_with(|| CannonCamera::new(rig.eye, &seat));
        let out = cam.update(m);
        camera.fov_offset = cam.fov_offset;
        if let Some(out) = out {
            camera.set(Some(out));
        } else if state.active {
            // Pre-raise frames: keep whatever the camera was showing.
            let cur = camera.cur;
            camera.set(cur);
        }
        state.active = true;
        return;
    }
    state.camera = None;
    camera.fov_offset = 0.0;
    if m.action == ACT_SHOT_FROM_CANNON {
        rig.follow_cannon_shot(m.pos, m.face_angle, 0.0, 0.0, before == ACT_IN_CANNON, &m.world);
        camera.set(Some((rig.eye, rig.look_at)));
        state.active = true;
    } else if state.active {
        state.active = false;
        camera.set(None);
    }
}

fn sync_views(cannons: Option<Res<CannonSet>>, mut views: Query<(&CannonView, &mut Transform, &mut Visibility)>) {
    let Some(cannons) = cannons else { return };
    for (view, mut tf, mut vis) in &mut views {
        let Some(c) = cannons.0.entries.get(view.index) else { continue };
        match view.part {
            Part::Lid => {
                tf.translation = to_bevy(c.lid_pos.to_array());
                tf.rotation = Quat::from_rotation_y(c.lid_yaw_degrees.to_radians() as f32);
                *vis = if c.lid_visible { Visibility::Inherited } else { Visibility::Hidden };
            }
            Part::Base | Part::Barrel => {
                tf.translation = to_bevy(c.base_pos.to_array());
                let yaw = angle_to_radians(c.base_yaw) as f32;
                tf.rotation = if view.part == Part::Base {
                    Quat::from_rotation_y(yaw)
                } else {
                    Quat::from_euler(EulerRot::YXZ, yaw, -angle_to_radians(c.barrel_pitch) as f32, 0.0)
                };
                *vis = if c.parts_visible { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

