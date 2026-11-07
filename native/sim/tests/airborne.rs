//! Airborne-action goldens generated from the TypeScript (tests/rustFixtures.airborne.gen.test.ts):
//! every frame while Mario stays in the airborne group must match bit-for-bit.
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::state::*;
use serde_json::Value;
use std::sync::Arc;

fn load(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}
fn nums(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }

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

#[test]
fn airborne_actions_match() {
    let cases = load("tests/fixtures/airborne_bob.json");
    let w = Arc::new(world("bob"));
    let mut checked = 0;
    for (ci, c) in cases.as_array().unwrap().iter().enumerate() {
        let spawn = SpawnPoint {
            action: c["spawn"].get("action").and_then(|a| a.as_u64()).map(|a| a as u32),
            pos: { let p = nums(&c["spawn"]["pos"]); [p[0], p[1], p[2]] },
            face_yaw: c["spawn"]["faceYaw"].as_f64().unwrap(),
        };
        let mut m = MarioRuntime::new(w.clone(), spawn, -1600.0);
        m.forward_vel = c["fv"].as_f64().unwrap();
        let frames = c["frames"].as_array().unwrap();
        for (i, s) in c["script"].as_array().unwrap().iter().enumerate() {
            let s = nums(s);
            let ctl = ControllerSnapshot { focus_yaw: None, stick_x: s[0], stick_y: s[1], stick_mag: s[2], button_down: s[3] as u32, button_pressed: s[4] as u32 };
            m.step(&ctl, 0.0, false, false);
            if i >= frames.len() { break; }
            let want = nums(&frames[i]);
            let got = [m.pos[0], m.pos[1], m.pos[2], m.vel[0], m.vel[1], m.vel[2], m.face_angle[0], m.face_angle[1], m.face_angle[2],
                m.action as f64, m.forward_vel, m.health];
            assert!(got == want[..], "case {ci} (action 0x{:x}, fv {}) frame {i}: {got:?} vs {want:?}", c["spawn"]["action"].as_u64().unwrap(), c["fv"]);
            checked += 1;
        }
    }
    assert!(checked > 500, "only {checked} frames checked");
}
