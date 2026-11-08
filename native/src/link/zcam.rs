//! The OoT Z-target camera on top of the existing orbit rig, plus Navi.
//!
//! While Link is locked on (L, toggle or hold) this blends the rig's camera into the placement
//! `z_camera.c` gives the Z-target modes. The numbers come from `sSetNormal0Mode*` in
//! `z_camera_data.inc.c` (field setting, `CAM_SET_NORMAL0`):
//!
//! * hostile target - `CAM_FUNC_BATTLE1`: `yOffset -20`, `distance 180`, swing yaw 10 -> 80 deg,
//!   swing pitch 0 -> 10 deg, `fov 50`;
//! * no target (L held in the open) - `CAM_FUNC_PARA1`: `yOffset -20`, `distTarget 250`, `fov 45`;
//! * friendly target - `CAM_FUNC_KEEP1`: distance 120-140, `fov 45`.
//!
//! Distances are percentages of Link's height (`Player_GetHeight`: about 68 adult, 44 child; the
//! rig draws Link at 4.4 x). The yaw swing interpolation of `Camera_Battle1` is approximated by the
//! smaller 10 -> 30 deg shoulder offset; the camera eases in and out (`w`), which the original does
//! with its `CAM_STATE_*` interpolation counters.
use super::{link_active, LockOn};
use crate::camera::rig::CameraRig;
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
        app.init_resource::<ZCam>().add_systems(Startup, spawn_navi).add_systems(Update, (z_camera, move_navi).chain().after(crate::CameraSet));
    }
}

#[derive(Resource, Default)]
pub struct ZCam {
    /// 0 = the rig's own camera, 1 = fully the Z-target camera.
    pub w: f32,
    eye: Option<DVec3>,
    look: Option<DVec3>,
    fov_touched: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZMode {
    Battle,
    Parallel,
    /// Riding Epona (`CAM_SET_HORSE`-like): a wide, further chase camera behind her.
    Horse,
}

/// Camera placement for a Z-target mode: (eye, look-at, fov radians). Pure so it can be tested.
/// `link` is Link's feet, `target` the opponent's marker point, `facing` Link's yaw (s16), `height` his
/// height in sim units.
pub fn z_placement(mode: ZMode, link: DVec3, target: Option<DVec3>, facing: f64, height: f64) -> (DVec3, DVec3, f32) {
    let head = link + DVec3::Y * (height * 0.9);
    match (mode, target) {
        (ZMode::Horse, _) => {
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
        (ZMode::Battle, Some(t)) => {
            let to_link = DVec3::new(link.x - t.x, 0.0, link.z - t.z);
            let d = to_link.length().max(1.0);
            let dir = to_link / d;
            // Closer fights swing the camera further to the shoulder (10 -> 30 deg).
            let closeness = (1.0 - d / (6.0 * height)).clamp(0.0, 1.0);
            let swing = (10.0 + 20.0 * closeness).to_radians();
            let base = dir.x.atan2(dir.z) + swing;
            let distance = (1.8 * height).max(height + 0.55 * d);
            let pitch = (closeness * 10.0).to_radians() + ((t.y - link.y) * -0.0004).clamp(-0.2, 0.2);
            // yOffset -20%: look slightly below head height, pulled toward the target.
            let at = DVec3::new(head.x, head.y - 0.1 * height, head.z).lerp(t, 0.35);
            let eye = at + DVec3::new(base.sin() * pitch.cos(), pitch.sin(), base.cos() * pitch.cos()) * distance;
            (eye, at, 50f32.to_radians())
        }
        _ => {
            // Parallel: stay behind Link, 250% of his height away, a shallow 5 deg down.
            let ang = facing * std::f64::consts::PI / 32768.0 + std::f64::consts::PI;
            let pitch = 5f64.to_radians();
            let distance = 2.5 * height;
            let at = DVec3::new(head.x, head.y - 0.1 * height, head.z);
            let eye = at + DVec3::new(ang.sin() * pitch.cos(), pitch.sin(), ang.cos() * pitch.cos()) * distance;
            (eye, at, 45f32.to_radians())
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
    let mode = if riding { ZMode::Horse } else if target.is_some() { ZMode::Battle } else { ZMode::Parallel };
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
    fn battle_camera_keeps_link_between_the_eye_and_the_target() {
        let h = link_height(false);
        let link = DVec3::new(0.0, 0.0, 0.0);
        let target = DVec3::new(0.0, 0.0, 800.0);
        let (eye, at, fov) = z_placement(ZMode::Battle, link, Some(target), 0.0, h);
        // Camera is on the far side of Link from the target (negative z) and looks toward the target.
        assert!(eye.z < link.z - 200.0, "{eye:?}");
        assert!(at.z > link.z && at.z < target.z);
        assert!((fov - 50f32.to_radians()).abs() < 1e-6);
        // Further away from the opponent never gets closer than 1.8 x Link's height.
        let d = (eye - at).length();
        assert!(d >= 1.8 * h * 0.99, "{d}");
    }

    #[test]
    fn closer_fights_swing_further_to_the_shoulder() {
        let h = link_height(false);
        let yaw_of = |dist: f64| {
            let (eye, at, _) = z_placement(ZMode::Battle, DVec3::ZERO, Some(DVec3::new(0.0, 0.0, dist)), 0.0, h);
            // Angle away from the straight line behind Link (which is +-pi for a target on +z).
            std::f64::consts::PI - (eye.x - at.x).atan2(eye.z - at.z).abs()
        };
        assert!(yaw_of(300.0) > yaw_of(2500.0));
    }

    #[test]
    fn parallel_camera_sits_behind_link_and_uses_the_narrower_fov() {
        let h = link_height(true);
        let (eye, at, fov) = z_placement(ZMode::Parallel, DVec3::ZERO, None, 0.0, h);
        assert!(eye.z < 0.0 && at.z.abs() < 1.0, "behind a Link facing +z: {eye:?}");
        assert!((fov - 45f32.to_radians()).abs() < 1e-6);
        assert!(((eye - at).length() - 2.5 * h).abs() < 1.0);
        assert!(link_height(true) < link_height(false));
    }
}
