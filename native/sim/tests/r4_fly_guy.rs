//! Fly Guy (`fly_guy.inc.c`): approaches Mario, lunges at him; the fire variant spits a flame.
mod r2a_common;
use jumpman_sim::object::*;
use r2a_common::*;

fn fly_guy(param: Option<f64>) -> (jumpman_sim::mario_runtime::MarioRuntime, ObjId) {
    let mut s = spec(ObjectBehaviorKind::FlyGuy, [0.0, 600.0, 0.0], 0.0, 0);
    s.param = param;
    let mut m = runtime(ground(-3000.0, 9000.0), vec![s]);
    stand_at(&mut m, 800.0, 0.0);
    let id = find(&m, ObjectBehaviorKind::FlyGuy)[0];
    (m, id)
}

#[test]
fn fly_guy_approaches_and_lunges_at_mario() {
    let (mut m, id) = fly_guy(None);
    let mut lunged = false;
    let mut min_y = 600.0f64;
    for _ in 0..400 {
        m.state.pos = [800.0, 0.0, 0.0];
        tick(&mut m);
        lunged |= m.objects[id].action == 2;
        min_y = min_y.min(m.objects[id].pos[1]);
    }
    assert!(lunged, "lunged at Mario");
    assert!(min_y < 400.0, "dived toward him: {min_y}");
    assert!(find(&m, ObjectBehaviorKind::SmallPiranhaFlame).is_empty(), "the generic one never spits fire");
}

#[test]
fn fire_fly_guy_swells_and_spits_a_flame() {
    let (mut m, id) = fly_guy(Some((1u32 << 16) as f64));
    let mut spat = false;
    for _ in 0..1500 {
        m.state.pos = [800.0, 0.0, 0.0];
        tick(&mut m);
        if !find(&m, ObjectBehaviorKind::SmallPiranhaFlame).is_empty() {
            spat = true;
            break;
        }
    }
    assert!(spat, "the fire variant spat a flame");
    let _ = id;
}

#[test]
fn far_fly_guy_idles() {
    let (mut m, id) = fly_guy(None);
    for _ in 0..200 {
        m.state.pos = [8000.0, 0.0, 0.0];
        tick(&mut m);
    }
    assert_ne!(m.objects[id].action, 2);
    assert!((m.objects[id].pos[1] - 600.0).abs() < 25.0, "just bobbing: {}", m.objects[id].pos[1]);
}
