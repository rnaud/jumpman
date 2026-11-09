//! Fire Spitter (`fire_spitter.inc.c`): floats at its place, shrinks to 0.2, spits one flame at Mario within 800 after 150 frames.
mod r2a_common;
use jumpman_sim::object::*;
use r2a_common::*;

#[test]
fn fire_spitter_shrinks_then_spits_one_flame_at_a_near_mario() {
    let mut m = runtime(ground(-3000.0, 9000.0), vec![spec(ObjectBehaviorKind::FireSpitter, [0.0, 400.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 5000.0, 0.0);
    let id = find(&m, ObjectBehaviorKind::FireSpitter)[0];
    assert!((m.objects[id].scale - 0.4).abs() < 1e-9);
    for _ in 0..200 {
        m.state.pos = [5000.0, 0.0, 0.0];
        tick(&mut m);
    }
    assert!((m.objects[id].scale - 0.2).abs() < 1e-6, "settled at 0.2: {}", m.objects[id].scale);
    assert_eq!(m.objects[id].pos[1], 400.0, "no gravity");
    assert!(find(&m, ObjectBehaviorKind::SmallPiranhaFlame).is_empty(), "Mario is 5000 away");
    let mut shot = false;
    for _ in 0..60 {
        m.state.pos = [500.0, 0.0, 0.0];
        tick(&mut m);
        if !find(&m, ObjectBehaviorKind::SmallPiranhaFlame).is_empty() {
            shot = true;
            break;
        }
    }
    assert!(shot, "spat a flame");
    assert_eq!(m.objects[id].action, 1);
    for _ in 0..80 {
        m.state.pos = [500.0, 0.0, 0.0];
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 0, "back to idle");
}
