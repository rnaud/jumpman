//! The OoT Z-target camera on top of the existing orbit rig, plus Navi.
//!
//! While Link is locked on (L, toggle or hold) the camera is `camera::zlock`, a port of
//! `Camera_Battle1` (opponent), `Camera_KeepOn1` (somebody to talk to) and `Camera_Parallel1`
//! (L held with no target) with the `CAM_SET_NORMAL0` / `CAM_SET_DUNGEON0` numbers. This module feeds
//! it Link's state, collision probes and the target, blends the result over the rig's camera and
//! fades back to the rig when the lock is released. Riding Epona keeps the older fixed chase
//! placement ([`z_placement`] `Horse`).
use super::{link_active, LockOn};
use crate::camera::rig::{camera_ray, CameraRig};
use crate::camera::zlock::{ZIn, ZLock};
use crate::camera::MainCamera;
use crate::characters::Character;
use crate::experiments::ExperimentSlot;
use crate::game::runner::GameSim;
use crate::render::{to_bevy, SCALE};
use bevy::math::DVec3;
use bevy::prelude::*;

pub struct ZTargetPlugin;

impl Plugin for ZTargetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZCam>().add_systems(Startup, (spawn_navi, spawn_marker).in_set(crate::progress::PersistentSpawn)).add_systems(Update, (z_camera, move_navi, move_marker).chain().after(crate::CameraSet));
    }
}

#[derive(Resource, Default)]
pub struct ZCam {
    /// 0 = the rig's own camera, 1 = fully the Z-target camera.
    pub w: f32,
    eye: Option<DVec3>,
    look: Option<DVec3>,
    fov_touched: bool,
    /// The running OoT lock-on camera (`camera::zlock`), alive from the first lock until the fade-out ends.
    lock: Option<ZLock>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZMode {
    /// Riding Epona (`CAM_SET_HORSE`-like): a wide, further chase camera behind her.
    Horse,
}

/// Camera placement for a Z-target mode: (eye, look-at, fov radians). Pure so it can be tested.
/// `link` is Link's feet, `target` the opponent's marker point, `facing` Link's yaw (s16), `height` his
/// height in sim units.
pub fn z_placement(mode: ZMode, link: DVec3, target: Option<DVec3>, facing: f64, height: f64) -> (DVec3, DVec3, f32) {
    let _ = target;
    match mode {
        ZMode::Horse => {
            // Behind the horse, 3.2 x Link's height away and 14 degrees above, looking at the rider with a lead of 0.8 heights.
            let ang = facing * std::f64::consts::PI / 32768.0;
            let (mut dist, mut yaw_off, mut pitch_deg) = (3.2, 0.0, 14.0);
            // Screenshot hook: JUMPMAN_HORSE_CAM="<distance in heights>,<yaw offset deg>,<pitch deg>".
            if let Ok(v) = std::env::var("JUMPMAN_HORSE_CAM") {
                let p: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if p.len() == 3 {
                    (dist, yaw_off, pitch_deg) = (p[0], p[1].to_radians(), p[2]);
                }
            }
            let pitch = f64::to_radians(pitch_deg);
            let at = link + DVec3::new(ang.sin() * 0.8 * height, 0.35 * height, ang.cos() * 0.8 * height);
            let back = ang + std::f64::consts::PI + yaw_off;
            let eye = at + DVec3::new(back.sin() * pitch.cos(), pitch.sin(), back.cos() * pitch.cos()) * (dist * height);
            (eye, at, 50f32.to_radians())
        }
    }
}

pub fn link_height(child: bool) -> f64 {
    (if child { 44.0 } else { 68.0 }) * 4.4
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn z_camera(
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    lock: Res<LockOn>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut rig: ResMut<CameraRig>,
    mut zc: ResMut<ZCam>,
    mut camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let dt = time.delta_secs().min(0.1);
    let riding = link_active(&character, &slot) && sim.mario.link.enabled && sim.mario.link.epona.is_mounted();
    let want = riding || (link_active(&character, &slot) && (lock.locked || sim.focus_held) && sim.mario.link.enabled && sim.mario.link.mv != jumpman_sim::link::Mv::Aim);
    if !riding {
        return oot_lock_camera(want, dt, &character, &sim, &fixed, &mut rig, &mut zc, &mut *camera);
    }
    zc.lock = None;
    let goal = if want { 1.0 } else { 0.0 };
    zc.w += (goal - zc.w) * (1.0 - (-7.0 * dt).exp());
    if (goal - zc.w).abs() < 0.01 {
        zc.w = goal;
    }
    let (tf, proj) = &mut *camera;
    if zc.w <= 0.0 {
        if zc.fov_touched {
            if let Projection::Perspective(p) = &mut **proj {
                p.fov = 50f32.to_radians();
            }
            zc.fov_touched = false;
        }
        zc.eye = None;
        zc.look = None;
        return;
    }
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let link = DVec3::from(pos);
    let child = *character == Character::YoungLink;
    let target = sim.link_opponent.map(DVec3::from);
    let mode = ZMode::Horse;
    // Riding: follow Epona's heading (smoothed), not the rider's raw yaw.
    let facing = if riding { sim.mario.link.epona.yaw } else { yaw };
    let (mut eye, look, fov) = z_placement(mode, link, target, facing, link_height(child));
    // Keep the camera out of walls between Link and the eye.
    let from = [link.x, link.y + 120.0, link.z];
    let to = [eye.x, eye.y, eye.z];
    if let Some((hit, _)) = jumpman_sim::link::projectiles::terrain_hit(&sim.mario.world, &from, &to) {
        let h = DVec3::from(hit);
        let origin = DVec3::from(from);
        let dir = (h - origin).normalize_or_zero();
        eye = h - dir * 30.0;
    }
    // Temporal smoothing of the placement itself, then the blend with the rig's camera.
    let k_eye = 1.0 - (-6.0 * dt as f64).exp();
    let k_look = 1.0 - (-12.0 * dt as f64).exp();
    let e = zc.eye.map_or(DVec3::from(rig.eye), |p| p.lerp(eye, k_eye));
    let l = zc.look.map_or(DVec3::from(rig.look_at), |p| p.lerp(look, k_look));
    zc.eye = Some(e);
    zc.look = Some(l);
    let w = zc.w as f64;
    let be = DVec3::from(rig.eye).lerp(e, w);
    let bl = DVec3::from(rig.look_at).lerp(l, w);
    **tf = Transform::from_translation(to_bevy(be.into())).looking_at(to_bevy(bl.into()), Vec3::Y);
    // The rig continues from where the Z camera ended, so releasing the lock does not snap.
    rig.yaw = (e.x - l.x).atan2(e.z - l.z);
    if let Projection::Perspective(p) = &mut **proj {
        p.fov = 50f32.to_radians() + (fov - 50f32.to_radians()) * zc.w;
        zc.fov_touched = true;
    }
}

const DUNGEONS: &[&str] = &["deku_tree", "dodongos_cavern", "jabu_jabu", "forest_temple", "fire_temple", "water_temple", "shadow_temple", "spirit_temple", "ice_cavern", "bottom_of_the_well", "gerudo_training_ground", "ganons_castle"];

fn focus_id(t: crate::camera::link_focus::FocusTarget) -> u64 {
    use crate::camera::link_focus::FocusTarget;
    match t {
        FocusTarget::Object(id) => 1 << 40 | id.0 as u64,
        FocusTarget::Goomba(i) => 2 << 40 | i as u64,
    }
}

/// Seconds the lock-on camera takes to hand back to the normal follow camera.
const RELEASE_RATE: f32 = 6.0;

/// The lock-on camera of OoT (`camera::zlock`): runs while Link is locked or holds Z, then fades to the rig.
#[allow(clippy::too_many_arguments)]
fn oot_lock_camera(
    want: bool,
    dt: f32,
    character: &Character,
    sim: &GameSim,
    fixed: &Time<Fixed>,
    rig: &mut CameraRig,
    zc: &mut ZCam,
    camera: &mut (Mut<Transform>, Mut<Projection>),
) {
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let link = DVec3::from(pos);
    let child = *character == Character::YoungLink;
    let m = &sim.mario;
    let sample = if want {
        let lock = zc.lock.get_or_insert_with(|| ZLock::engage(rig.eye, rig.look_at, link, 50.0));
        let target = sim.link_opponent.zip(sim.link_focus.target).map(|(p, t)| (DVec3::from(p), focus_id(t)));
        let zin = ZIn {
            link,
            link_yaw: yaw * std::f64::consts::PI / 32768.0,
            height: link_height(child),
            ground_y: m.floor_height,
            on_ground: m.action & jumpman_sim::state::ACT_FLAG_AIR == 0,
            speed_ratio: (m.forward_vel.abs() / 32.0).min(1.0),
            target,
            friendly: sim.link_focus.friendly(&m.objects),
            dungeon: DUNGEONS.contains(&sim.course.request.id.as_str()),
        };
        let probe = |o: DVec3, d: DVec3, max: f64| camera_ray(&m.world, o, d, max);
        zc.w = 1.0;
        Some(lock.advance(dt as f64, &zin, &probe))
    } else {
        // Released: the camera stops where it is and hands over to the rig.
        zc.w += (0.0 - zc.w) * (1.0 - (-RELEASE_RATE * dt).exp());
        if zc.w < 0.01 {
            zc.w = 0.0;
        }
        zc.lock.as_ref().map(|l| l.sample())
    };
    let (tf, proj) = camera;
    let Some(mut s) = sample.filter(|_| zc.w > 0.0) else {
        zc.lock = None;
        if zc.fov_touched {
            if let Projection::Perspective(p) = &mut **proj {
                p.fov = 50f32.to_radians();
            }
            zc.fov_touched = false;
        }
        return;
    };
    // Keep the eye above the ground like the rig does.
    let floor = m.world.find_floor(s.eye.x, s.eye.y, s.eye.z);
    if floor.surface.is_some() {
        s.eye.y = s.eye.y.max(floor.height + 30.0);
    }
    let w = zc.w as f64;
    let be = DVec3::from(rig.eye).lerp(s.eye, w);
    let bl = DVec3::from(rig.look_at).lerp(s.at, w);
    **tf = Transform::from_translation(to_bevy(be.into())).looking_at(to_bevy(bl.into()), Vec3::Y);
    // The rig continues from where the lock camera ended, so releasing the lock does not snap.
    rig.yaw = (s.eye.x - s.at.x).atan2(s.eye.z - s.at.z);
    if let Projection::Perspective(p) = &mut **proj {
        p.fov = 50f32.to_radians() * (1.0 - zc.w) + (s.fov as f32).to_radians() * zc.w;
        zc.fov_touched = true;
    }
}

#[derive(Component)]
pub(crate) struct Navi;

fn spawn_navi(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mesh = meshes.add(Sphere::new(1.0));
    let core = materials.add(StandardMaterial { base_color: Color::srgb(0.85, 0.95, 1.0), unlit: true, ..default() });
    let halo = materials.add(StandardMaterial { base_color: Color::srgba(0.5, 0.75, 1.0, 0.35), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
    commands
        .spawn((Navi, Transform::from_scale(Vec3::splat(SCALE)), Visibility::Hidden))
        .with_children(|p| {
            p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(core), Transform::from_scale(Vec3::splat(7.0))));
            p.spawn((Mesh3d(mesh), MeshMaterial3d(halo), Transform::from_scale(Vec3::splat(18.0))));
        });
}

/// The Z-target marker: four yellow arrows around the locked opponent, facing the camera.
#[derive(Component)]
struct ZMarker {
    age: f32,
    last: Option<Vec3>,
}

/// Radius of the ring of arrows once closed and the size of an arrow, sim units.
const MARKER_RADIUS: f32 = 62.0;
const MARKER_ARROW: f32 = 26.0;

/// The marker `age` seconds after it took a target: (scale of the ring of arrows, its turn in radians). The arrows
/// fly in from wide and settle (`TargetContext`'s triangles), then keep turning slowly.
pub fn marker_pose(age: f32) -> (f32, f32) {
    let k = (age / 0.22).clamp(0.0, 1.0);
    let e = 1.0 - (1.0 - k).powi(3);
    (2.6 - 1.6 * e, age.max(0.0) * 2.4)
}

/// Whether the lock moved to another opponent between two frames (the marker flies in again): no target before,
/// or a jump no enemy makes in a frame.
pub fn marker_retargeted(last: Option<Vec3>, now: Vec3) -> bool {
    last.is_none_or(|l| l.distance(now) > 1.5)
}

fn spawn_marker(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let h = MARKER_ARROW * 0.5;
    // an arrow head pointing at the centre (down, for the arrow placed on top)
    let mesh = meshes.add(Triangle2d::new(Vec2::new(-h, h), Vec2::new(h, h), Vec2::new(0.0, -h)));
    let mat = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.9, 0.15, 0.92), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    commands.spawn((ZMarker { age: 0.0, last: None }, Transform::default(), Visibility::Hidden)).with_children(|p| {
        for i in 0..4 {
            let a = i as f32 * std::f32::consts::FRAC_PI_2;
            p.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(Vec3::new(a.sin(), a.cos(), 0.0) * MARKER_RADIUS).with_rotation(Quat::from_rotation_z(-a)),
                bevy::light::NotShadowCaster,
                bevy::light::NotShadowReceiver,
            ));
        }
    });
}

fn move_marker(
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    sim: Res<GameSim>,
    time: Res<Time<Real>>,
    camera: Single<&Transform, (With<MainCamera>, Without<ZMarker>)>,
    mut marker: Query<(&mut ZMarker, &mut Transform, &mut Visibility)>,
) {
    let Ok((mut mk, mut tf, mut vis)) = marker.single_mut() else { return };
    let target = sim.link_opponent.filter(|_| link_active(&character, &slot) && sim.mario.link.enabled);
    let Some(p) = target else {
        *vis = Visibility::Hidden;
        mk.last = None;
        return;
    };
    let at = to_bevy(p);
    if marker_retargeted(mk.last, at) {
        mk.age = 0.0;
    }
    mk.last = Some(at);
    mk.age += time.delta_secs().min(0.1);
    let (scale, turn) = marker_pose(mk.age);
    // in front of the body, so the arrows are not buried in it
    let toward = (camera.translation - at).normalize_or_zero();
    tf.translation = at + toward * (MARKER_RADIUS * SCALE);
    tf.rotation = camera.rotation * Quat::from_rotation_z(turn);
    tf.scale = Vec3::splat(SCALE * scale);
    *vis = Visibility::Inherited;
}

/// Navi hovers by Link's shoulder and flies to the locked target (the OoT Z-target indicator).
fn move_navi(
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut navi: Query<(&mut Transform, &mut Visibility), With<Navi>>,
) {
    let Ok((mut tf, mut vis)) = navi.single_mut() else { return };
    if !link_active(&character, &slot) || !sim.mario.link.enabled {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Inherited;
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let t = time.elapsed_secs();
    let side = (yaw * std::f64::consts::PI / 32768.0) as f32 + std::f32::consts::FRAC_PI_2;
    let home = to_bevy(pos) + Vec3::new(side.sin() * 60.0 * SCALE, (150.0 + 12.0 * (t * 3.0).sin()) * SCALE, side.cos() * 60.0 * SCALE);
    let goal = match sim.link_opponent {
        Some(p) => to_bevy(p) + Vec3::new(0.0, (70.0 + 10.0 * (t * 4.0).sin()) * SCALE, 0.0),
        None => home,
    };
    let k = 1.0 - (-8.0 * time.delta_secs()).exp();
    tf.translation = tf.translation.lerp(goal, k);
    tf.scale = Vec3::splat(SCALE * if sim.link_opponent.is_some() { 1.5 } else { 1.0 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_marker_flies_in_then_only_turns() {
        let (wide, _) = marker_pose(0.0);
        let (mid, _) = marker_pose(0.1);
        let (closed, t1) = marker_pose(0.3);
        let (later, t2) = marker_pose(2.0);
        assert!(wide > mid && mid > closed, "{wide} {mid} {closed}");
        assert!((closed - 1.0).abs() < 1e-5 && (later - 1.0).abs() < 1e-5, "{closed} {later}");
        assert!(t2 > t1);
        assert!(marker_retargeted(None, Vec3::ZERO));
        assert!(!marker_retargeted(Some(Vec3::ZERO), Vec3::new(0.1, 0.0, 0.0)), "a walking enemy keeps its marker");
        assert!(marker_retargeted(Some(Vec3::ZERO), Vec3::new(3.0, 0.0, 0.0)));
    }
}
