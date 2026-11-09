//! Chuckya and Heave-Ho grab Mario (`ACT_GRABBED`) and throw him (`ACT_THROWN_FORWARD` / `ACT_THROWN_BACKWARD`).
mod r2a_common;
use jumpman_sim::mario_r2a::{ACT_THROWN_BACKWARD, ACT_THROWN_FORWARD};
use jumpman_sim::object::*;
use jumpman_sim::state::*;
use r2a_common::*;

fn run_until(m: &mut jumpman_sim::mario_runtime::MarioRuntime, at: [f64; 3], frames: usize, mut stop: impl FnMut(&jumpman_sim::mario_runtime::MarioRuntime) -> bool) -> bool {
    for i in 0..frames {
        if i == 0 {
            m.state.pos = at;
        }
        tick(m);
        if stop(m) {
            return true;
        }
    }
    false
}

#[test]
fn chuckya_grabs_a_mario_in_front_of_it_and_throws_him_forward() {
    let mut m = runtime(ground(0.0, 9000.0), vec![spec(ObjectBehaviorKind::Chuckya, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 0.0, 120.0);
    let id = find(&m, ObjectBehaviorKind::Chuckya)[0];
    assert!(run_until(&mut m, [0.0, 0.0, 120.0], 30, |m| m.grabbed_by == Some(id)), "grabbed");
    assert_eq!(m.action, ACT_GRABBED);
    // The throw: forward 40, up 40 into ACT_THROWN_FORWARD.
    let mut thrown = false;
    for _ in 0..200 {
        tick(&mut m);
        if m.action == ACT_THROWN_FORWARD || m.prev_action == ACT_THROWN_FORWARD {
            thrown = true;
            break;
        }
    }
    assert!(thrown, "thrown forward");
    assert!(m.grabbed_by.is_none());
    assert!(m.vel[1] > 20.0, "tossed up: {}", m.vel[1]);
    assert!(m.forward_vel > 30.0, "tossed forward: {}", m.forward_vel);
}

#[test]
fn mashing_a_frees_mario_early_with_a_short_hop() {
    let mut m = runtime(ground(0.0, 9000.0), vec![spec(ObjectBehaviorKind::Chuckya, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 0.0, 120.0);
    let id = find(&m, ObjectBehaviorKind::Chuckya)[0];
    assert!(run_until(&mut m, [0.0, 0.0, 120.0], 30, |m| m.grabbed_by == Some(id)));
    let mut freed = false;
    for i in 0..100 {
        // A pressed every other frame.
        let mut snap = ControllerSnapshot::default();
        snap.button_pressed = if i % 2 == 0 { A_BUTTON } else { 0 };
        snap.button_down = snap.button_pressed;
        jumpman_sim::object_manager::step(&mut m, None, false);
        m.step(&snap, 0.0, false, false);
        jumpman_sim::object_manager::after_mario_step(&mut m);
        if m.grabbed_by.is_none() {
            freed = true;
            break;
        }
    }
    assert!(freed, "A presses broke the hold");
    assert_eq!(m.prev_action, ACT_THROWN_FORWARD);
}

#[test]
fn heave_ho_grabs_and_tosses_mario_backward() {
    let mut m = runtime(ground(0.0, 9000.0), vec![spec(ObjectBehaviorKind::HeaveHo, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 0.0, 150.0);
    let mut grabbed = false;
    let mut thrown = false;
    for _ in 0..600 {
        m.state.pos = if grabbed { m.state.pos } else { [0.0, 0.0, 150.0] };
        tick(&mut m);
        grabbed |= m.action == ACT_GRABBED;
        if m.action == ACT_THROWN_BACKWARD || m.prev_action == ACT_THROWN_BACKWARD {
            thrown = true;
            break;
        }
        if !grabbed {
            m.state.pos = [0.0, 0.0, 150.0];
        }
    }
    assert!(grabbed && thrown, "grabbed {grabbed}, thrown backward {thrown}");
}
