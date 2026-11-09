//! Hidden 1-up in a pole: near it the spawner places the 1-up and two triggers; touching both pops the 1-up out toward Mario.
mod r2a_common;
use jumpman_sim::object::*;
use r2a_common::*;

#[test]
fn touching_both_triggers_pops_the_one_up_out() {
    let mut m = runtime(ground(0.0, 9000.0), vec![spec(ObjectBehaviorKind::Hidden1UpInPoleSpawner, [0.0, 400.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 3000.0, 0.0);
    for _ in 0..3 {
        m.state.pos = [3000.0, 0.0, 0.0];
        tick(&mut m);
    }
    assert!(find(&m, ObjectBehaviorKind::Hidden1UpInPole).is_empty(), "far away: still the spawner");
    // Climb past the trigger at y 400 and the one at y 200 (the pole is climbed to the top).
    for y in [400.0, 200.0] {
        m.state.pos = [0.0, y, 0.0];
        tick(&mut m);
        tick(&mut m);
    }
    let one_up = find(&m, ObjectBehaviorKind::Hidden1UpInPole);
    assert_eq!(one_up.len(), 1);
    assert_eq!(find(&m, ObjectBehaviorKind::Hidden1UpInPoleTrigger).len(), 0, "both triggers used up");
    let id = one_up[0];
    for _ in 0..4 {
        m.state.pos = [600.0, 0.0, 0.0];
        tick(&mut m);
    }
    assert!(m.objects[id].visible, "popped out");
    assert!(m.objects[id].pos[1] > 450.0, "rising: {}", m.objects[id].pos[1]);
}
