//! Camera: engine-free rig (`rig`, `transition`, `wall`, `summit`, ...) plus the Bevy plugin
//! that feeds it from [`GameSim`] / [`PlayerInput`] and writes the `Camera3d` transform.
pub mod ending;
pub mod link_focus;
pub mod npc_dialog;
pub mod pitch_shake;
pub mod rig;
pub mod summit;
pub mod transition;
pub mod wall;

use crate::game::runner::GameSim;
use crate::input::PlayerInput;
use crate::render::to_bevy;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::math::to_s16;
use jumpman_sim::object::ObjectBehaviorKind;
use jumpman_sim::state::*;
pub use pitch_shake::CameraPitchShake;
pub use rig::{CameraMode, CameraRig, RigInput};

impl Resource for CameraRig {}

#[derive(Resource, Default)]
pub struct CameraShake(pub CameraPitchShake);

/// A scripted shot (sim-space eye and focus) that replaces the rig while set: the ending
/// cameras, secret-star and boss cutscene cameras write it from their own systems.
#[derive(Resource, Default)]
pub struct CameraScript(pub Option<(DVec3, DVec3)>);

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CameraRig::default())
            .init_resource::<CameraShake>()
            .init_resource::<CameraScript>()
            .add_systems(Startup, spawn_camera)
            .add_systems(FixedUpdate, step_shake.before(crate::SimSet))
            .add_systems(Update, (camera_controls, update_camera).chain().in_set(crate::CameraSet));
    }
}

#[derive(Component)]
pub struct MainCamera;

fn spawn_camera(mut commands: Commands, sim: Res<GameSim>, mut rig: ResMut<CameraRig>) {
    rig.snap_behind(sim.mario.face_angle[1]);
    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Tonemapping::ReinhardLuminance,
        Projection::Perspective(PerspectiveProjection { fov: 50f32.to_radians(), near: 0.05, far: 400.0, ..default() }),
    ));
}

/// Bowling balls and cannon raises shake the camera pitch (camera.c SHAKE_POS_*).
fn step_shake(sim: Res<GameSim>, rig: Res<CameraRig>, mut shake: ResMut<CameraShake>) {
    for o in &sim.mario.objects.objects {
        if o.active && matches!(o.behavior, ObjectBehaviorKind::PitBowlingBall | ObjectBehaviorKind::MountainBall) {
            let view = (rig.look_at - rig.eye).normalize_or_zero();
            shake.0.request_bowling_ball(rig.eye, view, o.pos);
        }
    }
    if sim.mario.action == ACT_IN_CANNON && sim.mario.action_timer == 72.0 {
        shake.0.request_cannon_raise();
    }
    shake.0.step();
}

fn camera_controls(mut input: ResMut<PlayerInput>, mut rig: ResMut<CameraRig>, time: Res<Time>, sim: Res<GameSim>) {
    let dt = time.delta_secs_f64();
    let cam = input.consume_camera();
    if sim.mario.action != ACT_SHOT_FROM_CANNON {
        if cam.dx != 0.0 || cam.dy != 0.0 {
            rig.rotate(cam.dx as f64, cam.dy as f64);
        }
        rig.orbit(cam.orbit_x as f64 * dt * 1.8, cam.orbit_y as f64 * dt * 1.1);
        if cam.wheel != 0.0 {
            rig.zoom(cam.wheel as f64);
        }
    }
    if input.cycle_camera {
        input.cycle_camera = false;
        rig.cycle_mode();
    }
    if input.snap_camera {
        input.snap_camera = false;
        rig.snap_behind(sim.mario.face_angle[1]);
    }
}

#[allow(clippy::too_many_arguments)]
fn update_camera(
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    shake: Res<CameraShake>,
    script: Res<CameraScript>,
    mut rig: ResMut<CameraRig>,
    mut camera: Single<&mut Transform, With<MainCamera>>,
) {
    let m = &sim.mario;
    let t = fixed.overstep_fraction() as f64;
    let (pos, yaw) = sim.mario_at(t);
    let mut render_yaw = yaw;
    if matches!(m.action, ACT_SPECIAL_EXIT_AIRBORNE | ACT_EXIT_AIRBORNE | ACT_FALLING_EXIT_AIRBORNE | ACT_EXIT_LAND_SAVE_DIALOG) {
        render_yaw = to_s16(render_yaw + 0x8000 as f64) as f64;
    }

    let underwater = m.action & (ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER) != 0;
    rig.set_water_mode(underwater);
    rig.set_slide_mode(sim.course.request.id == "pss" || (sim.course.request.id == "ccm" && sim.course.request.area == 2));
    rig.set_interior_mode(sim.course.request.id == "castle_inside");
    rig.set_flight_mode(m.action == ACT_FLYING);

    let battlefield = sim.course.request.id == "bob";
    let surface = m.floor.as_ref().map_or(0, |f| f.kind);
    rig.set_battlefield_floor(battlefield.then_some(surface));
    let objects = &m.objects.objects;
    let king = objects.iter().find(|o| o.behavior == ObjectBehaviorKind::KingBobomb && o.active);
    let summit = battlefield && surface == 0x65;
    let bowser = objects
        .iter()
        .find(|o| matches!(o.behavior, ObjectBehaviorKind::Bowser | ObjectBehaviorKind::BigBully) && o.active);
    let bowser_near = bowser.filter(|b| (b.pos[0] - m.pos[0]).hypot(b.pos[2] - m.pos[2]) < 3000.0);
    if summit {
        let second = king.map_or(DVec3::new(3072.0, m.pos[1], -4608.0), |k| DVec3::from(k.pos));
        let held = king.map_or(false, |k| m.held_obj.map_or(false, |h| m.objects[h].id == k.id));
        rig.set_boss_focus(Some(second), held);
    } else {
        rig.set_boss_focus(bowser_near.map(|b| DVec3::new(b.pos[0], b.pos[1] + 180.0, b.pos[2])), false);
    }

    let pole = m.used_obj.map(|id| (m.objects[id].pos[1], m.objects[id].hitbox.height));
    let input = RigInput {
        dt: time.delta_secs_f64().min(0.1),
        mario_pos: DVec3::from(pos),
        mario_yaw: render_yaw,
        moving: m.forward_vel.abs() > 2.0,
        speed: m.forward_vel,
        opponent: sim.link_opponent.map(DVec3::from),
        action: m.action,
        pole,
        mario_pitch: m.face_angle[0],
        spin: m.angle_vel[1],
    };
    if let Some((eye, look)) = script.0 {
        rig.eye = eye;
        rig.look_at = look;
    } else {
        rig.update(&input, &m.world);
    }

    if std::env::var("JUMPMAN_CAMDBG").is_ok() { eprintln!("CAMDBG mario {:?} eye {:?} look {:?}", pos, rig.eye, rig.look_at); }
    let eye = to_bevy(rig.eye.into());
    let look = to_bevy(rig.look_at.into());
    **camera = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    if shake.0.radians != 0.0 {
        camera.rotate_local_x(shake.0.radians as f32);
    }
}
