//! BBH coffins (`coffin.inc.c`): six coffins, the animated ones stand up when Mario walks in front of them and lie back down.
mod r2a_common;
use jumpman_sim::behaviors::g2::dyn_collision::{register_mesh, CollisionMesh};
use jumpman_sim::object::*;
use r2a_common::*;

fn coffins(m: &jumpman_sim::mario_runtime::MarioRuntime) -> Vec<ObjId> {
    find(m, ObjectBehaviorKind::Coffin)
}

#[test]
fn six_coffins_spawn_and_an_animated_one_stands_up_for_mario_then_lies_down() {
    register_mesh(
        "g2/bbh_seg7_collision_coffin",
        CollisionMesh { vertices: vec![[-100.0, 0.0, -200.0], [100.0, 0.0, -200.0], [100.0, 0.0, 200.0], [-100.0, 0.0, 200.0]], triangles: vec![[0, 2, 1, 0, 0], [0, 3, 2, 0, 0]] },
    );
    let mut m = runtime(ground(-3000.0, 9000.0), vec![spec(ObjectBehaviorKind::CoffinSpawner, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 0.0, -2000.0);
    for _ in 0..3 {
        m.state.pos = [0.0, 0.0, -2000.0];
        tick(&mut m);
    }
    let all = coffins(&m);
    assert_eq!(all.len(), 6);
    // Odd ones animate (param 1). Take the one at (762, -150): index 1.
    let animated: Vec<ObjId> = all.iter().copied().filter(|c| m.objects[*c].var("param2") == 1.0).collect();
    assert_eq!(animated.len(), 3);
    let c = animated[0];
    let at = m.objects[c].pos;
    // Walk onto its front: within 140 forwards, sideways in (-450, 150): its face yaw is 0, so forward is +z... stand right on it.
    let mut stood = false;
    for _ in 0..150 {
        m.state.pos = [at[0], at[1] + 5.0, at[2] - 100.0];
        tick(&mut m);
        if m.objects[c].action == 1 {
            stood = true;
            break;
        }
    }
    assert!(stood, "the coffin got up for Mario");
    // It rises to 0x4000, stays ~60 frames, and lies down again.
    let mut peak = 0.0f64;
    for _ in 0..300 {
        m.state.pos = [0.0, 0.0, -2000.0];
        tick(&mut m);
        peak = peak.max(m.objects[c].var("pitch"));
    }
    assert_eq!(peak, 0x4000 as f64);
    assert_eq!(m.objects[c].var("pitch"), 0.0, "lay back down");
    assert_eq!(m.objects[c].action, 0);
}
