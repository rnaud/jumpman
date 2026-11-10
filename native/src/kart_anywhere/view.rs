//! What the player sees of the summoned kart: a small procedural kart under the driver (no game data involved, so
//! it is there whatever was imported), its dust, sparks and splashes, and a chase camera behind it.
use super::ride::{Fx, Ride, SEAT};
use super::KartAnywhere;
use crate::camera::rig::camera_ray;
use crate::camera::CameraScript;
use crate::characters::{Character, CharacterRoot};
use crate::experiments::feel::Pop;
use crate::fx::particles::{BurstOptions, Particles};
use crate::game::runner::GameSim;
use crate::input::PlayerInput;
use crate::render::mario_view::MarioRoot;
use crate::render::{to_bevy, SCALE};
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::collision::{CollisionWorld, Surface};
use jumpman_sim::surfaces::*;
use std::f64::consts::{PI, TAU};

/// Chase camera: this far behind and above the kart, looking at a point above its seat.
const CAM_BACK: f64 = 640.;
const CAM_UP: f64 = 250.;
const CAM_LOOK_UP: f64 = 120.;
/// The camera stays where the player swung it for this long after the last look input, then eases back behind.
const SWING_HOLD: f64 = 0.8;
/// The camera's height above the horizon the player can add or take away (radians).
const LIFT: (f64, f64) = (-0.3, 0.8);

/// The top of the seat pan above the kart's floor point, where the driver is drawn.
pub const SEAT_DRAWN: f32 = 22.;

/// The kart is built for Mario; taller drivers get a bigger one, so nobody sits on a toy.
pub fn kart_scale(who: Character) -> f32 {
    match who {
        Character::Mario => 1.,
        Character::YoungLink => 1.12,
        Character::Banjo => 1.3,
        Character::Link => 1.45,
    }
}

#[derive(Resource, Default)]
pub struct ChaseCam {
    yaw: f64,
    /// The player's own swing around the kart and lift above it (right stick, mouse or touch drag).
    swing: f64,
    lift: f64,
    idle: f64,
    /// Height the camera looks at, eased so kerbs and steps do not jolt the picture.
    look_y: f64,
    /// 0..1 with speed: the camera hangs back a little at full speed.
    pull: f64,
    dist: f64,
    /// The shot this camera last wrote; anything else in `CameraScript` is somebody else's and wins.
    mine: Option<(DVec3, DVec3)>,
}

/// One frame of look input folded into the swing: `turn` / `tip` in radians this frame.
fn swing(cam: &mut ChaseCam, turn: f64, tip: f64, dt: f64) {
    if turn.abs() + tip.abs() > 1e-5 {
        cam.idle = 0.;
        cam.swing = (cam.swing + turn + PI).rem_euclid(TAU) - PI;
        cam.lift = (cam.lift + tip).clamp(LIFT.0, LIFT.1);
    } else {
        cam.idle += dt;
        if cam.idle > SWING_HOLD {
            let k = (-dt * 3.5).exp();
            cam.swing *= k;
            cam.lift *= k;
        }
    }
}

pub fn chase_camera(
    mut ka: ResMut<KartAnywhere>,
    sim: Res<GameSim>,
    who: Res<Character>,
    input: Res<PlayerInput>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut script: ResMut<CameraScript>,
    mut cam: ResMut<ChaseCam>,
) {
    let Some(ratio) = ka.ride.as_ref().map(|r| r.speed_ratio().clamp(0., 1.)) else {
        if let Some(mine) = cam.mine.take() {
            if script.0 == Some(mine) {
                script.0 = None;
            }
        }
        return;
    };
    if script.0.is_some() && script.0 != cam.mine {
        cam.mine = None;
        return;
    }
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let yaw = yaw * PI / 32768.;
    let dt = time.delta_secs_f64().min(0.1);
    let size = 1. + 0.6 * (kart_scale(*who) as f64 - 1.);
    let (back, up) = (CAM_BACK * size, CAM_UP * size);
    let look_y = pos[1] - SEAT + CAM_LOOK_UP * size;
    if std::mem::take(&mut ka.snap) || cam.mine.is_none() {
        *cam = ChaseCam { yaw, look_y, dist: back.hypot(up), ..default() };
    } else {
        let d = (yaw - cam.yaw + PI).rem_euclid(TAU) - PI;
        cam.yaw += d * (1. - (-dt * 4.5).exp());
        // Same sense and speed as the on-foot camera (`CameraRig::rotate` / `orbit`).
        let (drag, orbit) = input.look_peek();
        swing(&mut cam, orbit.x as f64 * dt * 1.8 - drag.x as f64 * 0.0042, orbit.y as f64 * dt * 1.1 + drag.y as f64 * 0.0034, dt);
        // Test hook: `JUMPMAN_KART_SWING=<degrees>` holds the camera swung around the kart (side and front views).
        if let Some(deg) = std::env::var("JUMPMAN_KART_SWING").ok().and_then(|v| v.parse::<f64>().ok()) {
            cam.swing = deg.to_radians();
            cam.idle = 0.;
        }
        cam.look_y += (look_y - cam.look_y) * (1. - (-dt * 9.).exp());
        cam.look_y = cam.look_y.clamp(look_y - 140., look_y + 140.);
        cam.pull += (ratio - cam.pull) * (1. - (-dt * 2.).exp());
    }
    let full = back.hypot(up) * (1. + 0.14 * cam.pull);
    let look = DVec3::new(pos[0], cam.look_y, pos[2]);
    let (around, above) = (cam.yaw + cam.swing, (up.atan2(back) + cam.lift).clamp(0.05, 1.35));
    let dir = DVec3::new(-around.sin() * above.cos(), above.sin(), -around.cos() * above.cos());
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
pub struct Wheel {
    front: bool,
    radius: f32,
}

/// How the kart is drawn this frame; the driver is drawn with it ([`seat_driver`]).
#[derive(Resource)]
pub struct Look {
    at: Vec3,
    /// The kart's rotation without its heading: the floor's tilt, the lean into the turn and the drift angle.
    tilt: Quat,
    size: f32,
    /// Landing squash (1 right after touching down).
    squash: Pop,
    squash_by: f32,
    lean: f32,
    slide: f32,
    spin: f32,
    /// Seconds until the tyres may throw the next puff.
    puff: f32,
    left: bool,
    /// The driver transform this module last wrote: an untouched one is not seated twice.
    wrote: Option<Transform>,
}

impl Default for Look {
    fn default() -> Self {
        Look { at: Vec3::ZERO, tilt: Quat::IDENTITY, size: 1., squash: Pop::default(), squash_by: 0., lean: 0., slide: 0., spin: 0., puff: 0., left: false, wrote: None }
    }
}

/// Stick right is positive steer; a positive turn about +y is a turn to the left.
const STEER_TURN: f32 = -1.;

pub fn sync_kart(
    mut commands: Commands,
    mut ka: ResMut<KartAnywhere>,
    sim: Res<GameSim>,
    who: Res<Character>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut look: ResMut<Look>,
    mut particles: Option<ResMut<Particles>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut roots: Query<(Entity, &mut Transform), (With<KartRoot>, Without<Wheel>)>,
    mut wheels: Query<(&Wheel, &mut Transform), Without<KartRoot>>,
) {
    let Some(ride) = ka.ride.as_mut() else {
        for (e, _) in &roots {
            commands.entity(e).despawn();
        }
        *look = Look::default();
        return;
    };
    let dt = time.delta_secs().min(0.1);
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let floor = [pos[0], pos[1] - SEAT, pos[2]];
    let yaw = (yaw * PI / 32768.) as f32;
    let size = kart_scale(*who);
    let ratio = ride.speed_ratio().clamp(0., 1.) as f32;
    let steer = ride.steer as f32;

    for fx in std::mem::take(&mut ride.fx) {
        match fx {
            Fx::Landed(air) => {
                look.squash_by = (air as f32 / 14.).clamp(0.25, 1.);
                look.squash.hit();
                if let Some(p) = particles.as_mut() {
                    let colour = dust_colour(&sim.mario.world, sim.mario.floor.as_deref());
                    for at in tyres(floor, yaw, size) {
                        p.burst(at, BurstOptions { color: colour, count: 2 + (air as usize / 5).min(4), speed: 150., life: 0.45, gravity: 120., size: 34., upward: 25., spread: 1. });
                    }
                }
            }
            Fx::Bump => {
                look.squash_by = 0.3;
                look.squash.hit();
                if let Some(p) = particles.as_mut() {
                    let nose = [floor[0] + (yaw.sin() * 90. * size) as f64, floor[1] + 30., floor[2] + (yaw.cos() * 90. * size) as f64];
                    p.sparks(nose, 5);
                }
            }
            Fx::Splash => {
                if let Some(p) = particles.as_mut() {
                    let level = sim.mario.world.water_level(floor[0], floor[2]);
                    p.splash([floor[0], level.max(floor[1]), floor[2]]);
                }
            }
        }
    }
    look.squash.update(dt, 9.);
    if let Some(p) = particles.as_mut() {
        look.puff -= dt;
        if look.puff <= 0. {
            look.left = !look.left;
            let at = tyres(floor, yaw, size)[look.left as usize];
            look.puff = tyre_puff(p, ride, &sim, at).unwrap_or(0.);
        }
    }

    // Lean with the floor the kart stands on (level in the air), into the turn, and out at the tail in a drift.
    let up = sim.mario.floor.as_ref().filter(|_| ride.grounded()).map_or(Vec3::Y, |f| Vec3::new(f.normal[0] as f32, f.normal[1] as f32, f.normal[2] as f32));
    let ease = 1. - (-dt * 10.).exp();
    look.lean += (steer * ratio * 0.13 - look.lean) * ease;
    look.slide += (if ride.kart.drifting { STEER_TURN * steer.signum() * 0.3 } else { 0. } - look.slide) * ease;
    let heading = Quat::from_rotation_y(yaw);
    let want = Quat::from_rotation_arc(Vec3::Y, up.normalize_or(Vec3::Y)) * heading * Quat::from_rotation_y(look.slide) * Quat::from_rotation_z(look.lean);
    let v = look.squash.value() * look.squash_by;
    let scale = Vec3::new(1. + 0.12 * v, 1. - 0.25 * v, 1. + 0.12 * v) * size;
    let at = to_bevy(floor);
    let Ok((_, mut tf)) = roots.single_mut() else {
        for (e, _) in &roots {
            commands.entity(e).despawn();
        }
        spawn(&mut commands, &mut meshes, &mut materials, Transform::from_translation(at).with_rotation(want).with_scale(scale));
        *look = Look { at, tilt: want * heading.inverse(), size, ..default() };
        return;
    };
    tf.translation = at;
    tf.rotation = tf.rotation.slerp(want, 1. - (-dt * 12.).exp());
    tf.scale = scale;
    look.at = at;
    look.tilt = tf.rotation * heading.inverse();
    look.size = size;
    let roll = if ride.kart.back_up { -1. } else { 1. } * ride.speed() as f32 * 30. * dt;
    look.spin = (look.spin + roll / 25.).rem_euclid(std::f32::consts::TAU);
    let sideways = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    for (wheel, mut w) in &mut wheels {
        let turned = if wheel.front { Quat::from_rotation_y(STEER_TURN * steer * 0.45) } else { Quat::IDENTITY };
        w.rotation = turned * sideways * Quat::from_rotation_y(-look.spin * 25. / wheel.radius);
    }
}

/// The rear tyres' contact points (world units): right, left.
fn tyres(floor: [f64; 3], yaw: f32, size: f32) -> [[f64; 3]; 2] {
    let (s, c) = (yaw.sin() as f64, yaw.cos() as f64);
    let (back, side) = (58. * size as f64, 62. * size as f64);
    [-1., 1.].map(|k: f64| [floor[0] - s * back + c * side * k, floor[1] + 6., floor[2] - c * back - s * side * k])
}

/// Dust colour for the floor: snow and ice throw white, sand throws sand, anything else pale road dust (the
/// levels carry no surface material beyond that).
pub fn dust_colour(world: &CollisionWorld, floor: Option<&Surface>) -> u32 {
    let kind = floor.map_or(0, |f| f.kind);
    if world.snow_terrain || kind == SURFACE_ICE {
        0xf4f8ff
    } else if is_quicksand(kind) {
        0xdcb878
    } else {
        0xd0c8b8
    }
}

/// What a rear tyre throws right now, if anything: returns the wait before the next puff.
fn tyre_puff(p: &mut Particles, ride: &Ride, sim: &GameSim, at: [f64; 3]) -> Option<f32> {
    let k = &ride.kart;
    let ratio = ride.speed_ratio();
    if k.midair || ratio < 0.06 {
        return None;
    }
    if ride.wading {
        let level = sim.mario.world.water_level(at[0], at[2]);
        p.burst([at[0], level, at[2]], BurstOptions { color: 0xa6ddff, count: 2, speed: 80. + 120. * ratio as f32, life: 0.45, gravity: 420., size: 12., upward: 90. + 120. * ratio as f32, spread: 1. });
        return Some(0.07);
    }
    let colour = dust_colour(&sim.mario.world, sim.mario.floor.as_deref());
    let smoke = |p: &mut Particles, size: f32| p.burst(at, BurstOptions { color: colour, count: 1, speed: 50., life: 0.5, gravity: -40., size, upward: 30., spread: 1. });
    if k.drifting && !k.hop {
        if k.drift_state >= 1 {
            // The mini-turbo is charging: sparks in its colour, as in the kart mode.
            p.burst(at, BurstOptions { color: crate::experiments::kart_puffs::drift_colour(k.drift_state), count: 2, speed: 170., life: 0.25, gravity: 620., size: 12., upward: 110., spread: 1. });
        }
        smoke(p, 30.);
        return Some(0.045);
    }
    let pulling_away = ride.accel && ratio < 0.5;
    let scrubbing = k.braking && ratio > 0.25 || ride.steer.abs() > 0.8 && ratio > 0.6;
    let loose = colour != 0xd0c8b8 && ratio > 0.45;
    if pulling_away || scrubbing {
        smoke(p, 26.);
        Some(0.06)
    } else if loose {
        smoke(p, 22.);
        Some(0.11)
    } else {
        None
    }
}

/// The white puff the kart appears in and leaves in.
pub fn poof(p: &mut Particles, at: [f64; 3], size: f32) {
    let at = [at[0], at[1] + 40. * size as f64, at[2]];
    p.burst(at, BurstOptions { color: 0xffffff, count: 14, speed: 240. * size, life: 0.5, gravity: -60., size: 60. * size, upward: 40., spread: 1. });
    p.burst(at, BurstOptions { color: 0xffe9a0, count: 6, speed: 320. * size, life: 0.35, gravity: 300., size: 16., upward: 160., spread: 1. });
}

/// The kart, in world units around its floor point: +z forward, the seat pan [`SEAT_DRAWN`] up.
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
    // A bar across each wheel face, so the wheels are seen to turn.
    let spoke = meshes.add(Cuboid::new(34. * SCALE, 31. * SCALE, 7. * SCALE));
    let steering = meshes.add(Cylinder::new(15. * SCALE, 4. * SCALE));
    let sideways = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    commands.spawn((KartRoot, tf, Visibility::Inherited)).with_children(|k| {
        for (size, centre, material) in boxes {
            k.spawn((Mesh3d(meshes.add(Cuboid::new(size[0] as f32 * SCALE, size[1] as f32 * SCALE, size[2] as f32 * SCALE))), MeshMaterial3d(material.clone()), Transform::from_translation(to_bevy(centre))));
        }
        for x in [-60., 60.] {
            for (front, mesh, radius, at) in [(true, &wheel_front, 22., [x, 22., 62.]), (false, &wheel_rear, 27., [x * 1.03, 27., -58.])] {
                k.spawn((Wheel { front, radius }, Mesh3d(mesh.clone()), MeshMaterial3d(rubber.clone()), Transform::from_translation(to_bevy(at)).with_rotation(sideways))).with_children(|w| {
                    w.spawn((Mesh3d(hub.clone()), MeshMaterial3d(steel.clone()), Transform::default()));
                    w.spawn((Mesh3d(spoke.clone()), MeshMaterial3d(steel.clone()), Transform::default()));
                });
            }
        }
        // The steering wheel faces the driver, tipped back.
        k.spawn((Mesh3d(steering), MeshMaterial3d(rubber.clone()), Transform::from_translation(to_bevy([0., 62., 48.])).with_rotation(Quat::from_rotation_x(1.1))));
    });
}

/// Link's riding clips keep his hip 71.5 OoT units (times the 4.4 course scale) above his origin, since on Epona the
/// origin hangs under the saddle. In the kart his origin is at the seat, so the drawn model drops by that much
/// (less the cushion) to sit in it.
const LINK_SEAT_DROP: f32 = 71.5 * 4.4 - 34.;
const YOUNG_SEAT_DROP: f32 = 176.;
const BANJO_LIFT: f32 = 8.;

/// Where the driver is drawn for a kart drawn as `look`: lifted to the (scaled) seat, squashed down with it and
/// tipped with the kart about its floor point. Drawing only: the simulation's position is untouched.
fn seated(tf: &Transform, look: &Look, drop: f32) -> Transform {
    let v = look.squash.value() * look.squash_by;
    let seat = SEAT_DRAWN * look.size * (1. - 0.25 * v) - SEAT as f32;
    let mut out = *tf;
    out.translation.y += (seat - drop) * SCALE;
    out.translation = look.at + look.tilt * (out.translation - look.at);
    out.rotation = look.tilt * out.rotation;
    out
}

/// Runs after every system that places the character models and before their transforms are propagated.
pub fn seat_driver(
    ka: Res<KartAnywhere>,
    who: Res<Character>,
    mut look: ResMut<Look>,
    mut mario: Query<&mut Transform, (With<MarioRoot>, Without<CharacterRoot>)>,
    mut rigs: Query<(&CharacterRoot, &mut Transform), Without<MarioRoot>>,
) {
    if !ka.driving() {
        return;
    }
    let drop = match *who {
        Character::Link => LINK_SEAT_DROP,
        // The child rides with the adult's clips on his smaller frame.
        Character::YoungLink => YOUNG_SEAT_DROP,
        // Banjo's sitting slide is drawn a little lower than the others: lift him out of the bodywork.
        Character::Banjo => -BANJO_LIFT,
        Character::Mario => 0.,
    };
    let driver = match *who {
        Character::Mario => mario.iter_mut().next(),
        _ => rigs.iter_mut().find(|(root, _)| root.0 == *who).map(|(_, tf)| tf),
    };
    let Some(mut tf) = driver else { return };
    // Nobody placed the model since it was last seated (the game is paused): it is seated already.
    if look.wrote == Some(*tf) {
        return;
    }
    *tf = seated(&tf, &look, drop);
    look.wrote = Some(*tf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swung_camera_holds_then_eases_back_behind_the_kart() {
        let mut cam = ChaseCam::default();
        // Half a second of right stick.
        for _ in 0..30 {
            swing(&mut cam, 1.8 / 60., 0., 1. / 60.);
        }
        let swung = cam.swing;
        assert!(swung > 0.8, "{swung}");
        // Let go: it stays put for the hold, then comes back.
        for _ in 0..30 {
            swing(&mut cam, 0., 0., 1. / 60.);
        }
        assert_eq!(cam.swing, swung);
        for _ in 0..180 {
            swing(&mut cam, 0., 0., 1. / 60.);
        }
        assert!(cam.swing.abs() < 0.01, "{}", cam.swing);
        // The lift is bounded both ways.
        for _ in 0..600 {
            swing(&mut cam, 0., 0.05, 1. / 60.);
        }
        assert_eq!(cam.lift, LIFT.1);
        for _ in 0..600 {
            swing(&mut cam, 0., -0.05, 1. / 60.);
        }
        assert_eq!(cam.lift, LIFT.0);
    }

    #[test]
    fn taller_drivers_get_a_bigger_kart() {
        assert_eq!(kart_scale(Character::Mario), 1.);
        assert!(kart_scale(Character::Link) > kart_scale(Character::Banjo) && kart_scale(Character::Banjo) > kart_scale(Character::YoungLink));
        assert!(kart_scale(Character::YoungLink) > 1.);
    }

    #[test]
    fn the_driver_tips_with_the_kart_about_its_floor_point() {
        let at = Vec3::new(1., 2., 3.);
        let level = Look { at, ..default() };
        let on_seat = Transform::from_translation(at + Vec3::Y * SEAT as f32 * SCALE);
        assert!(seated(&on_seat, &level, 0.).translation.abs_diff_eq(at + Vec3::Y * SEAT_DRAWN * SCALE, 1e-6));
        // Rolled a quarter turn about the forward axis, the seat is beside the floor point, not above it.
        let rolled = Look { at, tilt: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2), ..default() };
        let out = seated(&on_seat, &rolled, 0.);
        assert!((out.translation.y - at.y).abs() < 1e-5 && (out.translation.x - at.x).abs() > SEAT_DRAWN * SCALE * 0.9);
        // A bigger kart has a higher seat.
        let big = Look { at, size: 1.45, ..default() };
        assert!(seated(&on_seat, &big, 0.).translation.y > on_seat.translation.y);
    }

    #[test]
    fn dust_takes_the_colour_of_snow_and_sand() {
        let mut world = CollisionWorld::new(1024., 8192.);
        let road = dust_colour(&world, None);
        world.snow_terrain = true;
        assert_ne!(dust_colour(&world, None), road);
    }
}
