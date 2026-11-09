//! Yoshi (120-star gate, strolls on the roof) and the Jumping Box (hops by itself, bursts when thrown).
mod r2a_common;
use jumpman_sim::object::*;
use r2a_common::*;

#[test]
fn yoshi_needs_120_stars() {
    for (stars, shown) in [(119.0, false), (120.0, true)] {
        let mut m = runtime(ground(3174.0, 9000.0), vec![spec(ObjectBehaviorKind::Yoshi, [0.0, 3174.0, -5625.0], 0.0, 0)]);
        stand_at(&mut m, 3000.0, 0.0);
        let id = find(&m, ObjectBehaviorKind::Yoshi)[0];
        m.objects[id].set_var("stars", stars);
        for _ in 0..200 {
            m.state.pos = [3000.0, 3174.0, 0.0];
            tick(&mut m);
        }
        assert_eq!(!find(&m, ObjectBehaviorKind::Yoshi).is_empty(), shown, "stars {stars}");
        if shown {
            assert!(m.objects[id].visible);
        }
    }
}

#[test]
fn a_jumping_box_hops_and_bursts_once_thrown() {
    let mut m = runtime(ground(0.0, 9000.0), vec![spec(ObjectBehaviorKind::JumpingBox, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 4000.0, 0.0);
    let id = find(&m, ObjectBehaviorKind::JumpingBox)[0];
    let mut peak = 0.0f64;
    for _ in 0..300 {
        m.state.pos = [4000.0, 0.0, 0.0];
        tick(&mut m);
        peak = peak.max(m.objects[id].pos[1]);
    }
    assert!(peak > 10.0, "it hopped: {peak}");
    assert!(m.objects[id].active);
    m.objects[id].held_state = HELD_THROWN;
    for _ in 0..80 {
        m.state.pos = [4000.0, 0.0, 0.0];
        tick(&mut m);
    }
    assert!(!m.objects[id].active, "burst after the throw");
}
