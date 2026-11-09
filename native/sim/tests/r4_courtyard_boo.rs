//! Courtyard Boo triplet: three Boos at 12 stars or more, nothing below.
mod r2a_common;
use jumpman_sim::object::*;
use r2a_common::*;

fn run_with(stars: f64) -> usize {
    let mut m = runtime(ground(0.0, 9000.0), vec![spec(ObjectBehaviorKind::CourtyardBooTriplet, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 4000.0, 0.0);
    let id = find(&m, ObjectBehaviorKind::CourtyardBooTriplet)[0];
    // Nothing happens before the app publishes the star count.
    for _ in 0..3 {
        tick(&mut m);
    }
    assert!(find(&m, ObjectBehaviorKind::Boo).is_empty());
    m.objects[id].set_var("stars", stars);
    for _ in 0..3 {
        tick(&mut m);
    }
    find(&m, ObjectBehaviorKind::Boo).len()
}

#[test]
fn three_boos_at_twelve_stars() {
    assert_eq!(run_with(12.0), 3);
}

#[test]
fn none_below_twelve() {
    assert_eq!(run_with(11.0), 0);
}
