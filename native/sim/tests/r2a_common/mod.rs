//! Shared scaffolding of the round-2 behavior tests: a flat floor, a `MarioRuntime`, the object manager's step.
#![allow(dead_code)]
use jumpman_sim::behaviors::g2::dyn_collision::register_meshes_from_json;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectEvent, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;

pub fn load_meshes(course: &str) {
    let path = format!("{}/../../assets/courses/{course}.json", env!("CARGO_MANIFEST_DIR"));
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(register_meshes_from_json(&v["extraCollision"]) > 0, "no g2 meshes in {course}");
}

/// A square floor of half-width `r` at height `y`, surface type `kind`.
pub fn ground_kind(y: f64, r: f64, kind: i32) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions { kind, ..SurfaceOptions::default() };
    w.add_triangle([-r, y, -r], [-r, y, r], [r, y, r], &o);
    w.add_triangle([-r, y, -r], [r, y, r], [r, y, -r], &o);
    w
}

pub fn ground(y: f64, r: f64) -> CollisionWorld {
    ground_kind(y, r, 0)
}

pub fn runtime(world: CollisionWorld, specs: Vec<ObjectSpec>) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(specs, None, None));
    m
}

/// One 30 Hz tick; returns the object events of the step.
pub fn tick(m: &mut MarioRuntime) -> Vec<ObjectEvent> {
    object_manager::step(m, None, false);
    let events = m.objects.events.clone();
    m.step(&ControllerSnapshot::default(), 0.0, false, false);
    object_manager::after_mario_step(m);
    events
}

pub fn stand_at(m: &mut MarioRuntime, x: f64, z: f64) {
    let y = m.world.find_floor(x, 9000.0, z).height;
    m.state.pos = [x, y, z];
    m.state.vel = [0.0; 3];
    m.state.forward_vel = 0.0;
    m.set_action(ACT_IDLE, 0.0);
    m.invinc_timer = 100.0;
}

pub fn find(m: &MarioRuntime, kind: ObjectBehaviorKind) -> Vec<ObjId> {
    m.objects.ids().filter(|&i| m.objects[i].behavior == kind && m.objects[i].active).collect()
}

pub fn spec(kind: ObjectBehaviorKind, pos: [f64; 3], yaw: f64, param: u32) -> ObjectSpec {
    let mut s = ObjectSpec::new(kind, pos, yaw);
    s.param = Some(param as f64);
    s
}

pub fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
