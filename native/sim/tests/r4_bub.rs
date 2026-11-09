//! Chirp Chirp / Bub (`bub.inc.c`): the spawner releases one Bub within 1500 of Mario; the Bub keeps just under the surface.
mod r2a_common;
use jumpman_sim::collision::*;
use jumpman_sim::object::*;
use r2a_common::*;

#[test]
fn chirp_chirp_releases_a_bub_that_hugs_the_water_surface() {
    let mut w = ground(-4000.0, 12000.0);
    w.water_boxes.push(WaterBox(0.0, -12000.0, -12000.0, 12000.0, 12000.0, -1000.0));
    let mut m = runtime(w, vec![spec(ObjectBehaviorKind::ChirpChirp, [0.0, -2000.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 5000.0, 0.0);
    for _ in 0..3 {
        m.state.pos = [5000.0, -1100.0, 0.0];
        tick(&mut m);
    }
    assert!(find(&m, ObjectBehaviorKind::Bub).is_empty(), "Mario is 5000 away");
    for _ in 0..3 {
        m.state.pos = [600.0, -1100.0, 0.0];
        tick(&mut m);
    }
    let bubs = find(&m, ObjectBehaviorKind::Bub);
    assert_eq!(bubs.len(), 1, "one Bub per Chirp Chirp");
    for _ in 0..200 {
        m.state.pos = [600.0, -1100.0, 0.0];
        tick(&mut m);
    }
    let y = m.objects[bubs[0]].pos[1];
    assert!(y <= -1050.0 + 1.0, "never above the surface - 50: {y}");
    assert!(y > -2300.0, "stays in the band around the spawner: {y}");
}
