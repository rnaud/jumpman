//! Tumbling bridge (`tumbling_bridge.inc.c`): nine platforms replace the intact model near Mario, a platform Mario stands on
//! shakes for 5 frames and then falls away, and the bridge resets when he is 1200 units away.
mod r2a_common;
use jumpman_sim::behaviors::g2::dyn_collision::{register_mesh, CollisionMesh};
use jumpman_sim::object::*;
use r2a_common::*;

const KEY: &str = "g2/wf_seg7_collision_tumbling_bridge";

fn mesh() -> CollisionMesh {
    // A 128 x 128 slab on y = 0 (the WF bridge section reduces to its top).
    CollisionMesh {
        vertices: vec![[-64.0, 0.0, -64.0], [64.0, 0.0, -64.0], [64.0, 0.0, 64.0], [-64.0, 0.0, 64.0]],
        triangles: vec![[0, 2, 1, 0, 0], [0, 3, 2, 0, 0]],
    }
}

fn platforms(m: &jumpman_sim::mario_runtime::MarioRuntime) -> Vec<ObjId> {
    find(m, ObjectBehaviorKind::TumblingBridgePlatform)
}

#[test]
fn the_bridge_spawns_nine_platforms_one_tips_when_stood_on_and_it_resets_far_away() {
    register_mesh(KEY, mesh());
    // WF bridge, bparam2 0: nine sections from z = -512 in steps of 128 along the bridge.
    let mut m = runtime(ground(-3000.0, 9000.0), vec![spec(ObjectBehaviorKind::TumblingBridge, [0.0, 0.0, 0.0], 0.0, 0)]);
    let bridge = ObjId(0);
    stand_at(&mut m, 0.0, 800.0);
    m.state.pos = [0.0, 0.0, 800.0];
    // Out of range (800 < 1000 is in range): it spawns on the first frames.
    for _ in 0..4 {
        m.state.pos = [0.0, 300.0, 800.0];
        tick(&mut m);
    }
    assert_eq!(platforms(&m).len(), 9, "nine sections");
    assert_eq!(m.objects[bridge].action, 2);
    assert!(!m.objects[bridge].visible, "the intact model is hidden while the platforms exist");
    let zs: Vec<f64> = platforms(&m).iter().map(|p| m.objects[*p].pos[2]).collect();
    assert_eq!(zs[0], -512.0);
    assert_eq!(zs[8], 512.0);

    // Stand on the middle section (z = 0).
    let middle = *platforms(&m).iter().find(|p| m.objects[**p].pos[2] == 0.0).unwrap();
    m.state.pos = [0.0, 30.0, 0.0];
    m.state.vel = [0.0; 3];
    let mut tipped = false;
    for _ in 0..40 {
        let y = m.objects[middle].pos[1];
        m.state.pos = [0.0, y + 2.0, 0.0];
        tick(&mut m);
        if m.objects[middle].action >= 1 {
            tipped = true;
        }
    }
    assert!(tipped, "the platform Mario stood on started to tip");
    // 40 frames later it is falling (below its start) and rotated.
    for _ in 0..20 {
        m.state.pos = [0.0, 400.0, 0.0];
        tick(&mut m);
    }
    assert!(m.objects[middle].pos[1] < -50.0, "fell: {}", m.objects[middle].pos[1]);
    assert!(m.objects[middle].var("pitch") != 0.0);

    // Far away: the intact bridge comes back and the platforms are deleted.
    for _ in 0..6 {
        m.state.pos = [0.0, 0.0, 2500.0];
        tick(&mut m);
    }
    assert!(m.objects[bridge].visible);
    assert!(platforms(&m).is_empty(), "platforms removed once the bridge resets");
}
