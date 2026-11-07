//! Smoke test for the object arena, behavior registration hook and Mario interaction pass.
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectManager};
use std::sync::Arc;

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-2000.0, 0.0, -2000.0], [-2000.0, 0.0, 2000.0], [2000.0, 0.0, 2000.0], &o);
    w.add_triangle([-2000.0, 0.0, -2000.0], [2000.0, 0.0, 2000.0], [2000.0, 0.0, -2000.0], &o);
    w
}

fn tick(id: ObjId, m: &mut MarioRuntime) { let t = m.objects[id].var("ticks") + 1.0; m.objects[id].set_var("ticks", t); }

#[test]
fn behavior_hook_and_step() {
    let mut mgr = ObjectManager::new(vec![ObjectSpec::new(ObjectBehaviorKind::Bobomb, [500.0, 0.0, 500.0], 0.0)], None, None);
    mgr.behaviors.set_update(ObjectBehaviorKind::Bobomb, tick);
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -1600.0);
    m.set_objects(mgr);
    for _ in 0..5 { object_manager::step(&mut m, None, false); }
    let id = ObjId(0);
    assert_eq!(m.objects[id].var("ticks"), 5.0);
    assert_eq!(m.objects[id].timer, 5.0);
    assert!((m.objects[id].distance_to_mario - 500.0 * 2f64.sqrt()).abs() < 1e-9);
    // Held object bookkeeping through Mario.
    m.used_obj = Some(id);
    m.grab_used_object();
    assert_eq!(m.held_obj, Some(id));
    m.throw_held_object();
    assert_eq!(m.objects[id].held_state, HELD_THROWN);
    assert_eq!(m.held_obj, None);
}

#[test]
fn hypot_matches_v8_samples() {
    // Math.hypot(3, 4) === 5, Math.hypot(1, 1) === Math.SQRT2 in V8.
    assert_eq!(jumpman_sim::math::js_hypot(3.0, 4.0), 5.0);
    assert_eq!(jumpman_sim::math::js_hypot(1.0, 1.0), std::f64::consts::SQRT_2);
}
