//! Golden test for the moving-action group (src/sim/actions/moving.ts) against the TypeScript
//! (`GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.moving.gen.test.ts`).
//!
//! Frames are compared while Mario starts and ends the frame inside the moving group (0x40);
//! once he leaves it (jumps, idles, ...) the other groups' ports decide the outcome, so the
//! comparison stops there. Every scenario must match for at least `min` frames.
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

/// Returns (frames compared, first mismatching frame while still in the moving group).
fn run(name: &str) -> (usize, Option<usize>) {
    let f = load("tests/fixtures/mario_moving_bob.json");
    let sc = &f[name];
    let spawn = SpawnPoint {
        action: sc["spawn"].get("action").and_then(|a| a.as_u64()).map(|a| a as u32),
        pos: { let p = nums(&sc["spawn"]["pos"]); [p[0], p[1], p[2]] },
        face_yaw: sc["spawn"]["faceYaw"].as_f64().unwrap(),
    };
    let mut prev_action = spawn.action.unwrap_or(ACT_IDLE);
    let mut m = MarioRuntime::new(Arc::new(world("bob")), spawn, -1600.0);
    let total = sc["script"].as_array().unwrap().len();
    for i in 0..total {
        let s = nums(&sc["script"][i]);
        let c = ControllerSnapshot { focus_yaw: None, stick_x: s[0], stick_y: s[1], stick_mag: s[2], button_down: s[3] as u32, button_pressed: s[4] as u32 };
        m.step(&c, 0.0, false, false);
        let want = nums(&sc["frames"][i]);
        let want_action = want[9] as u32;
        if prev_action & 0x1c0 != 0x40 || want_action & 0x1c0 != 0x40 { return (i, None); }
        prev_action = want_action;
        let got = [
            m.pos[0], m.pos[1], m.pos[2], m.vel[0], m.vel[1], m.vel[2],
            m.face_angle[0], m.face_angle[1], m.face_angle[2], m.action as f64, m.forward_vel, m.health,
        ];
        if got.to_vec() != want {
            eprintln!("{name}: frame {i}\n got  {got:?}\n want {want:?}");
            return (i, Some(i));
        }
    }
    (total, None)
}

fn check(name: &str, min: usize) {
    let (n, bad) = run(name);
    assert_eq!(bad, None, "{name}: diverged");
    assert!(n >= min, "{name}: only {n} frames stayed in the moving group (expected >= {min})");
}

#[test]
fn turn_around() { check("turn", 50); }
#[test]
fn crouch_slide() { check("crouch", 40); }
#[test]
fn punch() { check("punch", 40); }
#[test]
fn wander() { check("wander", 30); }
#[test]
fn crouch_slide_direct() { check("crouch_slide", 0); }
#[test]
fn butt_slide_direct() { check("butt_slide", 0); }
#[test]
fn dive_slide_direct() { check("dive_slide", 0); }
#[test]
fn jump_press() { check("jump_press", 40); }
