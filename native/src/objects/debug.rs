//! Headless test hooks for object features (`JUMPMAN_TELEPORT`, `JUMPMAN_TAP`).
//!
//! * `JUMPMAN_TELEPORT=x,y,z[,yaw_degrees]` places Mario (SM64 units) at startup.
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
        if let Ok(v) = std::env::var("JUMPMAN_TELEPORT") {
            app.insert_resource(Teleport(v)).add_systems(Update, teleport.before(crate::CameraSet));
        }
    }
}

#[derive(Resource)]
struct Taps(Vec<(f32, bool, bool)>);

#[derive(Resource)]
struct Teleport(String);

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
