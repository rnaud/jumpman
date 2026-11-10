//! What the player sees of the summoned kart: a small procedural kart under the driver (no game data involved, so
//! it is there whatever was imported) and a chase camera behind it.
use super::ride::SEAT;
use super::KartAnywhere;
use crate::camera::rig::camera_ray;
use crate::camera::CameraScript;
use crate::game::runner::GameSim;
use crate::render::{to_bevy, SCALE};
use bevy::math::DVec3;
use bevy::prelude::*;
use std::f64::consts::{PI, TAU};

/// Chase camera: this far behind and above the kart, looking at a point above its seat.
const CAM_BACK: f64 = 640.;
const CAM_UP: f64 = 250.;
const CAM_LOOK_UP: f64 = 120.;

#[derive(Resource, Default)]
pub struct ChaseCam {
    yaw: f64,
    dist: f64,
    /// The shot this camera last wrote; anything else in `CameraScript` is somebody else's and wins.
    mine: Option<(DVec3, DVec3)>,
}

pub fn chase_camera(mut ka: ResMut<KartAnywhere>, sim: Res<GameSim>, fixed: Res<Time<Fixed>>, time: Res<Time>, mut script: ResMut<CameraScript>, mut cam: ResMut<ChaseCam>) {
    if ka.ride.is_none() {
        if let Some(mine) = cam.mine.take() {
            if script.0 == Some(mine) {
                script.0 = None;
            }
        }
        return;
    }
    if script.0.is_some() && script.0 != cam.mine {
        cam.mine = None;
        return;
    }
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let yaw = yaw * PI / 32768.;
    let dt = time.delta_secs_f64().min(0.1);
    let full = (CAM_BACK * CAM_BACK + CAM_UP * CAM_UP).sqrt();
    if std::mem::take(&mut ka.snap) || cam.mine.is_none() {
        cam.yaw = yaw;
        cam.dist = full;
    } else {
        let d = (yaw - cam.yaw + PI).rem_euclid(TAU) - PI;
        cam.yaw += d * (1. - (-dt * 4.5).exp());
    }
    let look = DVec3::new(pos[0], pos[1] - SEAT + CAM_LOOK_UP, pos[2]);
    let dir = DVec3::new(-cam.yaw.sin() * CAM_BACK, CAM_UP, -cam.yaw.cos() * CAM_BACK).normalize();
    // Walls and ceilings pull the camera in at once; it eases back out.
    let room = camera_ray(&sim.mario.world, look, dir, full).map_or(full, |d| (d - 40.).max(150.));
    cam.dist = if room < cam.dist { room } else { cam.dist + (room - cam.dist) * (1. - (-dt * 3.).exp()) };
    let shot = (look + dir * cam.dist, look);
    script.0 = Some(shot);
    cam.mine = Some(shot);
}

#[derive(Component)]
pub struct KartRoot;

#[derive(Component)]
pub struct FrontWheel;

pub fn sync_kart(
    mut commands: Commands,
    ka: Res<KartAnywhere>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut roots: Query<(Entity, &mut Transform), (With<KartRoot>, Without<FrontWheel>)>,
    mut wheels: Query<&mut Transform, (With<FrontWheel>, Without<KartRoot>)>,
) {
    let Some(ride) = &ka.ride else {
        for (e, _) in &roots {
            commands.entity(e).despawn();
        }
        return;
    };
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let at = to_bevy([pos[0], pos[1] - SEAT, pos[2]]);
    let heading = Quat::from_rotation_y((yaw * PI / 32768.) as f32);
    // Lean with the floor the kart stands on; level in the air.
    let up = sim.mario.floor.as_ref().filter(|_| ride.grounded()).map_or(Vec3::Y, |f| Vec3::new(f.normal[0] as f32, f.normal[1] as f32, f.normal[2] as f32));
    let want = Quat::from_rotation_arc(Vec3::Y, up.normalize_or(Vec3::Y)) * heading;
    let Ok((_, mut tf)) = roots.single_mut() else {
        for (e, _) in &roots {
            commands.entity(e).despawn();
        }
        spawn(&mut commands, &mut meshes, &mut materials, Transform::from_translation(at).with_rotation(want));
        return;
    };
    tf.translation = at;
    tf.rotation = tf.rotation.slerp(want, 1. - (-time.delta_secs() * 12.).exp());
    for mut w in &mut wheels {
        w.rotation = Quat::from_rotation_y(ride.steer as f32 * 0.45) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    }
}

/// The kart, in world units around its floor point: +z forward, the seat pan [`SEAT`] up.
fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, tf: Transform) {
    let mut paint = |c: [f32; 3]| materials.add(StandardMaterial { base_color: Color::srgb(c[0], c[1], c[2]), perceptual_roughness: 0.8, ..default() });
    let red = paint([0.82, 0.1, 0.1]);
    let white = paint([0.92, 0.92, 0.9]);
    let dark = paint([0.12, 0.12, 0.14]);
    let steel = paint([0.55, 0.57, 0.6]);
    let rubber = paint([0.06, 0.06, 0.07]);
    // (size, centre, material)
    let boxes: [([f64; 3], [f64; 3], &Handle<StandardMaterial>); 10] = [
        ([92., 14., 150.], [0., 15., 0.], &red),     // floor pan
        ([54., 18., 46.], [0., 30., 82.], &red),     // nose
        ([40., 6., 24.], [0., 41., 88.], &white),    // number plate
        ([14., 24., 84.], [-46., 30., -6.], &white), // side pods
        ([14., 24., 84.], [46., 30., -6.], &white),
        ([66., 58., 12.], [0., 50., -50.], &dark),   // seat back
        ([72., 30., 34.], [0., 36., -74.], &steel),  // engine
        ([116., 8., 10.], [0., 16., 110.], &steel),  // front bumper
        ([104., 8., 10.], [0., 20., -96.], &steel),  // rear bumper
        ([6., 34., 6.], [0., 44., 52.], &dark),      // steering column
    ];
    let wheel_front = meshes.add(Cylinder::new(22. * SCALE, 22. * SCALE));
    let wheel_rear = meshes.add(Cylinder::new(27. * SCALE, 28. * SCALE));
    let hub = meshes.add(Cylinder::new(10. * SCALE, 30. * SCALE));
    let steering = meshes.add(Cylinder::new(15. * SCALE, 4. * SCALE));
    let sideways = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    commands.spawn((KartRoot, tf, Visibility::Inherited)).with_children(|k| {
        for (size, centre, material) in boxes {
            k.spawn((Mesh3d(meshes.add(Cuboid::new(size[0] as f32 * SCALE, size[1] as f32 * SCALE, size[2] as f32 * SCALE))), MeshMaterial3d(material.clone()), Transform::from_translation(to_bevy(centre))));
        }
        for x in [-60., 60.] {
            k.spawn((FrontWheel, Mesh3d(wheel_front.clone()), MeshMaterial3d(rubber.clone()), Transform::from_translation(to_bevy([x, 22., 62.])).with_rotation(sideways)))
                .with_child((Mesh3d(hub.clone()), MeshMaterial3d(steel.clone()), Transform::default()));
            k.spawn((Mesh3d(wheel_rear.clone()), MeshMaterial3d(rubber.clone()), Transform::from_translation(to_bevy([x * 1.03, 27., -58.])).with_rotation(sideways)))
                .with_child((Mesh3d(hub.clone()), MeshMaterial3d(steel.clone()), Transform::default()));
        }
        // The steering wheel faces the driver, tipped back.
        k.spawn((Mesh3d(steering), MeshMaterial3d(rubber.clone()), Transform::from_translation(to_bevy([0., 62., 48.])).with_rotation(Quat::from_rotation_x(1.1))));
    });
}

/// Link's riding clips keep his hip 71.5 OoT units (times the 4.4 course scale) above his origin, since on Epona the
/// origin hangs under the saddle. In the kart his origin is at the seat, so the drawn model drops by that much
/// (less the cushion) to sit in it. Drawing only: the simulation's position is untouched.
const LINK_SEAT_DROP: f32 = 71.5 * 4.4 - 34.;

pub fn seat_link(ka: Res<KartAnywhere>, sim: Res<GameSim>, mut roots: Query<(&crate::characters::CharacterRoot, &mut Transform)>) {
    if !ka.driving() || !sim.as_link {
        return;
    }
    for (who, mut tf) in &mut roots {
        if who.0 == crate::characters::Character::Link {
            tf.translation.y -= LINK_SEAT_DROP * SCALE;
        }
    }
}
