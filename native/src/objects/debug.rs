//! Headless test hooks for object features (`JUMPMAN_TELEPORT`, `JUMPMAN_TAP`).
//!
//! * `JUMPMAN_STARS=n` (progress/save.rs) fakes the star count. `JUMPMAN_TELEPORT=x,y,z[,yaw_degrees]` places Mario (SM64 units) at startup.
//! * `JUMPMAN_TAP=a@3.0,b@5.5` latches an A / B press once the app clock passes the time.
use crate::camera::CameraRig;
use crate::game::runner::GameSim;
use crate::input::{self, PlayerInput};
use bevy::prelude::*;

pub struct DebugHooksPlugin;

impl Plugin for DebugHooksPlugin {
    fn build(&self, app: &mut App) {
        if let Some(taps) = std::env::var("JUMPMAN_TAP").ok().map(|v| parse_taps(&v)) {
            app.insert_resource(Taps(taps)).add_systems(PreUpdate, tap.after(input::read_input));
        }
        if let Ok(v) = std::env::var("JUMPMAN_SIDECAM") {
            let n: Vec<f32> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            if n.len() == 3 {
                app.insert_resource(SideCam([n[0], n[1], n[2]])).add_systems(Update, side_cam.after(crate::CameraSet));
            }
        }
        if let Ok(v) = std::env::var("JUMPMAN_TELEPORT") {
            app.insert_resource(Teleport(v)).add_systems(Update, teleport.before(crate::CameraSet));
        }
    }
}

#[derive(Resource)]
struct Taps(Vec<(f32, bool, bool)>);

#[derive(Resource)]
struct Teleport(String);

/// `JUMPMAN_SIDECAM=dist,height,yaw_degrees`: the main camera at a fixed bearing around Mario (yaw from +z towards +x),
/// looking at his chest. The contact sheets of the Link parity pass use it for side views of jumps, rolls and slides.
#[derive(Resource)]
struct SideCam([f32; 3]);

fn side_cam(spec: Res<SideCam>, sim: Res<GameSim>, mut cams: Query<&mut Transform, With<crate::camera::MainCamera>>) {
    let [dist, height, yaw] = spec.0;
    let target = crate::render::to_bevy([sim.mario.pos[0], sim.mario.pos[1] + 90.0, sim.mario.pos[2]]);
    let a = yaw.to_radians();
    let at = target + Vec3::new(a.sin() * dist, height, a.cos() * dist) * crate::render::SCALE;
    for mut t in &mut cams {
        *t = Transform::from_translation(at).looking_at(target, Vec3::Y);
    }
}

pub fn parse_taps(s: &str) -> Vec<(f32, bool, bool)> {
    s.split(',')
        .filter_map(|t| {
            let (button, time) = t.split_once('@')?;
            Some((time.trim().parse().ok()?, button.trim().eq_ignore_ascii_case("a"), button.trim().eq_ignore_ascii_case("b")))
        })
        .collect()
}

fn tap(mut taps: ResMut<Taps>, mut input: ResMut<PlayerInput>, time: Res<Time>) {
    let now = time.elapsed_secs();
    taps.0.retain(|&(at, a, b)| {
        if now >= at {
            input.tap(a, b);
            false
        } else {
            true
        }
    });
}

/// Runs once the requested course (`JUMPMAN_COURSE`, else the first one) is the loaded one: at `PostStartup` the shell
/// had not switched to it yet and the position was thrown away with the old sim.
fn teleport(spec: Res<Teleport>, mut sim: ResMut<GameSim>, mut rig: ResMut<CameraRig>, mut done: Local<bool>, mut frames: Local<u32>) {
    if *done {
        return;
    }
    *frames += 1;
    let want = std::env::var("JUMPMAN_COURSE").ok();
    let ready = match &want {
        Some(id) => sim.course.request.id == *id && *frames > 3,
        None => *frames > 3,
    };
    if !ready {
        return;
    }
    *done = true;
    let v: Vec<f64> = spec.0.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if v.len() < 3 {
        warn!("JUMPMAN_TELEPORT needs x,y,z");
        return;
    }
    sim.mario.pos = [v[0], v[1], v[2]];
    // a teleport starts a new fall (OoT resets `fallStartHeight` on warps)
    sim.mario.link.phys.fall_start = v[1];
    if let Some(yaw) = v.get(3) {
        sim.mario.face_angle[1] = jumpman_sim::math::deg_to_angle(*yaw) as f64;
    }
    // A spawn animation would swallow the move: drop into the free fall instead.
    {
        use jumpman_sim::state::*;
        if sim.mario.action & ACT_GROUP_MASK == ACT_GROUP_CUTSCENE {
            sim.mario.vel = [0.0; 3];
            sim.mario.set_action(ACT_FREEFALL, 0.0);
        }
    }
    sim.curr = sim.snapshot();
    sim.prev = sim.curr.clone();
    rig.snap_behind(sim.mario.face_angle[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tap_script() {
        assert_eq!(parse_taps("a@3.0, b@5.5,x"), vec![(3.0, true, false), (5.5, false, true)]);
    }
}
