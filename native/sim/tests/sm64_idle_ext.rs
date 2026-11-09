//! Panting, coughing, shivering and quicksand idles (`mario_actions_stationary.c`).
mod r2a_common;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::*;
use r2a_common::*;
use std::sync::Arc;

fn rig() -> MarioRuntime {
    let mut m = runtime(ground(0.0, 5000.0), vec![]);
    stand_at(&mut m, 0.0, 0.0);
    m.invinc_timer = 0.0;
    m
}

#[test]
fn low_health_idle_pants_until_health_returns() {
    let mut m = rig();
    m.state.health = 0x2ff as f64;
    tick(&mut m);
    assert_eq!(m.action, ACT_PANTING);
    // Not healed past 0x500: keeps panting.
    for _ in 0..30 {
        tick(&mut m);
    }
    assert_eq!(m.action, ACT_PANTING);
    m.state.health = 0x500 as f64;
    tick(&mut m);
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn healthy_idle_never_pants() {
    let mut m = rig();
    for _ in 0..90 {
        tick(&mut m);
    }
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn deep_quicksand_makes_mario_sink_idle_and_leaves_when_shallow() {
    let mut m = rig();
    m.state.quicksand_depth = 50.0;
    tick(&mut m);
    assert_eq!(m.action, ACT_IN_QUICKSAND);
    m.state.quicksand_depth = 10.0;
    tick(&mut m);
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn coughing_cancels_into_walking_like_idle() {
    let mut m = rig();
    m.set_action(ACT_COUGHING, 0.0);
    for _ in 0..100 {
        tick(&mut m);
    }
    assert_eq!(m.action, ACT_COUGHING, "keeps coughing while nothing is pressed");
}

#[test]
fn idle_in_snow_shivers_instead_of_sleeping() {
    let mut w: CollisionWorld = ground(0.0, 5000.0);
    w.snow_terrain = true;
    let mut m = runtime(w, vec![]);
    stand_at(&mut m, 0.0, 0.0);
    m.invinc_timer = 0.0;
    let mut shivered = false;
    for _ in 0..900 {
        tick(&mut m);
        assert_ne!(m.action, ACT_START_SLEEPING);
        if m.action == ACT_SHIVERING {
            shivered = true;
            break;
        }
    }
    assert!(shivered, "ends in the shivering action");
    // The warming-hand animation (86 frames) then shiver loop (state 1).
    for _ in 0..120 {
        tick(&mut m);
    }
    assert_eq!(m.action, ACT_SHIVERING);
    assert_eq!(m.action_state, 1.0);
    let _ = Arc::new(0);
}
