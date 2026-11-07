//! Scripted camera shots driven by progression state (`Game.ts`): the secret / red-coin star
//! reveal (`stepSecretStar`), the spawned-star flight (`stepSpawnedStar`) and the chain chomp
//! release (`stepChompCamera`). Each keeps a focus point that is lerped toward its subject while
//! the camera keeps its own place, and hands the original view back when it ends.
use super::flow::Progress;
use super::stars::{RevealInfo, SpawnInfo};
use crate::camera::{CameraRig, CameraScript};
use crate::game::runner::GameSim;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::object::ObjectBehaviorKind;

pub struct ShotsPlugin;

impl Plugin for ShotsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, director_shot.after(crate::SimSet).after(super::flow_post));
    }
}

/// Which subject the shot is following (one at a time).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Reveal,
    Spawn,
    Chomp,
}

pub struct Shot {
    kind: Kind,
    original_eye: DVec3,
    original_focus: DVec3,
    eye: DVec3,
    focus: DVec3,
    ticks: u32,
}

impl Shot {
    fn begin(kind: Kind, eye: DVec3, look: DVec3, mario: DVec3) -> Self {
        // `getWorldDirection * distance to Mario + position`: the point the camera looks at.
        let focus = eye + (look - eye).normalize_or_zero() * eye.distance(mario);
        Self { kind, original_eye: eye, original_focus: focus, eye, focus, ticks: 0 }
    }
}

/// The secret-star reveal: after the star pops out (t = 34) the camera hops to a 3/4 view of it.
pub fn step_reveal(shot: &mut Shot, info: &RevealInfo, mario: DVec3) -> (DVec3, DVec3) {
    let target = DVec3::from(info.target);
    let group = DVec3::from(info.group);
    if info.t == 34 && !info.regular {
        let to_mario = (mario.x - target.x).atan2(mario.z - target.z);
        let to_camera = (shot.eye.x - target.x).atan2(shot.eye.z - target.z);
        let yaw = to_mario + if to_mario < to_camera { std::f64::consts::FRAC_PI_2 } else { -std::f64::consts::FRAC_PI_2 };
        let horizontal = 400.0 * (std::f64::consts::PI / 8.0).cos();
        shot.eye = DVec3::new(target.x + horizontal * yaw.sin(), target.y + 400.0 * (std::f64::consts::PI / 8.0).sin(), target.z + horizontal * yaw.cos());
    }
    if info.regular {
        shot.focus = shot.focus.lerp(group, 0.1);
    } else {
        shot.focus.x += (target.x - shot.focus.x) * 0.15;
        shot.focus.z += (target.z - shot.focus.z) * 0.15;
        if info.t < 34 {
            shot.focus.y += (group.y - shot.focus.y) * 0.1;
        } else {
            shot.focus.y = target.y + info.y * 0.8;
        }
    }
    (shot.eye, shot.focus)
}

pub fn step_spawn(shot: &mut Shot, info: &SpawnInfo) -> (DVec3, DVec3) {
    if info.returning {
        let t = shot.ticks;
        shot.ticks += 1;
        shot.focus = shot.focus.lerp(shot.original_focus, 1.0 / f64::from((15 - t as i32).max(1)));
    } else {
        shot.focus = shot.focus.lerp(DVec3::from(info.pos), 0.1);
    }
    (shot.eye, shot.focus)
}

pub fn step_chomp(shot: &mut Shot, chomp_top: DVec3, release: f64, release_timer: f64) -> (DVec3, DVec3) {
    if release == 5.0 {
        shot.focus = shot.focus.lerp(shot.original_focus, 1.0 / (15.0 - release_timer).max(1.0));
    } else {
        shot.focus = shot.focus.lerp(chomp_top, 0.1);
    }
    (shot.eye, shot.focus)
}

pub fn director_shot(sim: Res<GameSim>, progress: Res<Progress>, rig: Res<CameraRig>, mut script: ResMut<CameraScript>, mut shot: Local<Option<Shot>>) {
    let mario = DVec3::from(sim.mario.pos);
    let chomp = sim
        .mario
        .used_obj
        .map(|id| &sim.mario.objects[id])
        .filter(|o| o.behavior == ObjectBehaviorKind::ChainChomp && o.active && o.var("release") >= 2.0);
    let wanted = if let Some(info) = progress.director.reveal.as_ref().filter(|_| progress.director.cutscene_active()) {
        Some((Kind::Reveal, Some(*info), None))
    } else if let Some(info) = progress.director.spawn_shot {
        Some((Kind::Spawn, None, Some(info)))
    } else if chomp.is_some() {
        Some((Kind::Chomp, None, None))
    } else {
        None
    };
    let Some((kind, reveal, spawn)) = wanted else {
        if let Some(s) = shot.take() {
            // Hand the original view back to the rig.
            script.0 = Some((s.original_eye, s.original_focus));
        } else if script.0.is_some() && script.is_changed() {
            // (another feature owns the script)
        }
        return;
    };
    if script.0.is_some() && shot.is_none() {
        return; // the ending / first-person camera has the floor
    }
    if shot.as_ref().is_some_and(|s| s.kind != kind) {
        *shot = None;
    }
    let s = shot.get_or_insert_with(|| Shot::begin(kind, rig.eye, rig.look_at, mario));
    let (eye, focus) = match (kind, reveal, spawn) {
        (Kind::Reveal, Some(info), _) => step_reveal(s, &info, mario),
        (Kind::Spawn, _, Some(info)) => step_spawn(s, &info),
        _ => {
            let o = chomp.expect("chomp shot");
            let top = DVec3::new(o.pos[0], o.pos[1] + o.hitbox.height * o.scale, o.pos[2]);
            step_chomp(s, top, o.var("release"), o.var("releaseTimer"))
        }
    };
    script.0 = Some((eye, focus));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_camera_hops_to_the_star_and_focus_tracks_it() {
        let mut shot = Shot::begin(Kind::Reveal, DVec3::new(0., 300., 800.), DVec3::new(0., 250., 0.), DVec3::ZERO);
        let target = [500.0, 200.0, -300.0];
        let mut last = DVec3::ZERO;
        for t in 4..120 {
            let info = RevealInfo { t, group: target, target, y: 0.0, regular: false };
            last = step_reveal(&mut shot, &info, DVec3::ZERO).1;
        }
        assert!((last - DVec3::from(target)).length() < 1.0);
        // The hop places the camera 400 units from the star at 22.5 degrees elevation.
        let dist = shot.eye.distance(DVec3::from(target));
        assert!((dist - 400.0).abs() < 1e-6);
    }

    #[test]
    fn spawn_shot_returns_to_the_original_focus() {
        let mut shot = Shot::begin(Kind::Spawn, DVec3::new(0., 300., 800.), DVec3::new(0., 250., 0.), DVec3::ZERO);
        let original = shot.original_focus;
        for _ in 0..30 {
            step_spawn(&mut shot, &SpawnInfo { pos: [1000., 500., 0.], returning: false });
        }
        for _ in 0..16 {
            step_spawn(&mut shot, &SpawnInfo { pos: [1000., 500., 0.], returning: true });
        }
        assert!((shot.focus - original).length() < 1e-6);
    }
}
