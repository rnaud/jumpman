//! Moneybag (`moneybag.inc.c`): a coin that turns into a hopping bag near Mario, wanders, and returns home.
mod r2a_common;
use jumpman_sim::object::*;
use r2a_common::*;

#[test]
fn the_coin_turns_into_a_moneybag_near_mario_and_back_when_he_leaves() {
    let mut m = runtime(ground(0.0, 9000.0), vec![spec(ObjectBehaviorKind::MoneybagHidden, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 3000.0, 0.0);
    for _ in 0..5 {
        m.state.pos = [3000.0, 0.0, 0.0];
        tick(&mut m);
    }
    assert!(find(&m, ObjectBehaviorKind::Moneybag).is_empty(), "far away it stays a coin");
    for _ in 0..60 {
        m.state.pos = [300.0, 0.0, 0.0];
        tick(&mut m);
    }
    let bags = find(&m, ObjectBehaviorKind::Moneybag);
    assert_eq!(bags.len(), 1);
    assert!(find(&m, ObjectBehaviorKind::MoneybagHidden).is_empty(), "the coin is gone while the bag is out");
    // Mario walks away: it hops home and becomes the coin again.
    let mut back = false;
    for _ in 0..1500 {
        m.state.pos = [3000.0, 0.0, 0.0];
        tick(&mut m);
        if !find(&m, ObjectBehaviorKind::MoneybagHidden).is_empty() {
            back = true;
            break;
        }
    }
    assert!(back, "returned home as a coin");
}
