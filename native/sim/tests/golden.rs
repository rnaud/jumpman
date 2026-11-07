//! Compares the Rust sim against fixtures generated from the TypeScript original
//! (`GEN_RUST_FIXTURES=1 pnpm vitest run tests/rustFixtures.gen.test.ts`).
use jumpman_sim::collision::*;
use jumpman_sim::math::*;
use serde_json::Value;
use std::sync::Arc;

fn load(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}
fn nums(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }

#[test]
fn math_matches() {
    let f = load("tests/fixtures/math.json");
    let angles = nums(&f["angles"]);
    for (i, a) in angles.iter().enumerate() {
        assert_eq!(sins(*a), f["sins"][i].as_f64().unwrap(), "sins {a}");
        assert_eq!(coss(*a), f["coss"][i].as_f64().unwrap(), "coss {a}");
        assert_eq!(to_s16(*a) as f64, f["s16"][i].as_f64().unwrap(), "s16 {a}");
    }
    for (i, p) in f["pairs"].as_array().unwrap().iter().enumerate() {
        let p = nums(p);
        assert_eq!(atan2s(p[0], p[1]) as f64, f["atan"][i].as_f64().unwrap(), "atan2s {p:?}");
    }
}

fn world(id: &str) -> CollisionWorld {
    let data = load(&format!("../../assets/courses/{id}.json"));
    let verts: Vec<[f64; 3]> = data["vertices"].as_array().unwrap().iter().map(|v| { let v = nums(v); [v[0], v[1], v[2]] }).collect();
    let mut w = CollisionWorld::default();
    for t in data["triangles"].as_array().unwrap() {
        let t = nums(t);
        let kind = t[3] as i32;
        let opts = SurfaceOptions { kind, force: t[4] as i32, flags: if (0x76..=0x79).contains(&kind) { 2 } else { 0 }, ..Default::default() };
        w.add_triangle(verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize], &opts);
    }
    w
}

fn index_of(w: &CollisionWorld, s: &Option<Arc<Surface>>) -> f64 {
    match s { Some(s) => w.surfaces().iter().position(|o| Arc::ptr_eq(o, s)).unwrap() as f64, None => -1.0 }
}

fn check(id: &str) {
    let f = load(&format!("tests/fixtures/collision_{id}.json"));
    let w = world(id);
    for (i, q) in f["queries"].as_array().unwrap().iter().enumerate() {
        let q = nums(q);
        let fl = w.find_floor(q[0], q[1], q[2]);
        let want = nums(&f["floors"][i]);
        assert_eq!((fl.height, index_of(&w, &fl.surface)), (want[0], want[1]), "{id} floor {q:?}");
        let ce = w.find_ceil(q[0], q[1], q[2]);
        let want = nums(&f["ceils"][i]);
        assert_eq!((ce.height, index_of(&w, &ce.surface)), (want[0], want[1]), "{id} ceil {q:?}");
        let mut p = [q[0], q[1] - 250.0, q[2]];
        let n = w.f32_find_wall_collision(&mut p, 60.0, 50.0);
        let want = nums(&f["walls"][i]);
        assert_eq!([n as f64, p[0], p[1], p[2]].to_vec(), want, "{id} wall {q:?}");
    }
}

#[test] fn bob_collision_matches() { check("bob"); }
#[test] fn castle_inside_collision_matches() { check("castle_inside"); }
