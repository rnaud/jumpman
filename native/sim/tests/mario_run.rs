//! Scripted MarioRuntime runs on bob.json compared frame-by-frame against the TypeScript
//! (`GEN_RUST_FIXTURES=1 pnpm vitest run tests/rustFixtures.gen.test.ts`).
//!
//! The action handlers are still stubs (stage 3), so only frames up to the first action
//! change away from the idle/stub behaviour can match; `walk` asserts the idle prefix and
//! reports where it diverges. Remove the `#[ignore]`s / raise the thresholds as actions land.
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

/// Runs a fixture scenario; returns the first frame whose state differs (None = all 300 match).
fn run(name: &str) -> (Option<usize>, usize) {
    let f = load("tests/fixtures/mario_run_bob.json");
    let sc = &f[name];
    let spawn = SpawnPoint {
        action: sc["spawn"].get("action").and_then(|a| a.as_u64()).map(|a| a as u32),
        pos: { let p = nums(&sc["spawn"]["pos"]); [p[0], p[1], p[2]] },
        face_yaw: sc["spawn"]["faceYaw"].as_f64().unwrap(),
    };
    let mut m = MarioRuntime::new(Arc::new(world("bob")), spawn, -1600.0);
    let total = sc["script"].as_array().unwrap().len();
    for i in 0..total {
        let s = nums(&sc["script"][i]);
        let c = ControllerSnapshot { focus_yaw: None, stick_x: s[0], stick_y: s[1], stick_mag: s[2], button_down: s[3] as u32, button_pressed: s[4] as u32 };
        m.step(&c, 0.0, false, false);
        let want = nums(&sc["frames"][i]);
        let got = [
            m.pos[0], m.pos[1], m.pos[2], m.vel[0], m.vel[1], m.vel[2],
            m.face_angle[0], m.face_angle[1], m.face_angle[2], m.action as f64, m.forward_vel, m.health,
        ];
        if got.to_vec() != want { return (Some(i), total); }
    }
    (None, total)
}

#[test]
fn idle_prefix_matches() {
    let (first_bad, total) = run("walk");
    // Frames 0..60 are pure idle in the script (no input): position/health/angles must match exactly.
    match first_bad {
        None => {}
        Some(i) => {
            eprintln!("walk: first divergence at frame {i} of {total} (expected while actions are stubs)");
            assert!(i >= 60, "idle frames diverged at frame {i}");
        }
    }
}

#[test]
#[ignore = "needs landing (stationary/moving) actions ported; the airborne part matches up to the landing frame"]
fn freefall_matches() {
    assert_eq!(run("freefall").0, None);
}

/// Airborne actions are ported: the fall from 1500 must match exactly until the landing frame (30).
#[test]
fn freefall_airborne_prefix_matches() {
    let (first_bad, _) = run("freefall");
    assert!(first_bad.map_or(true, |i| i >= 30), "freefall diverged at frame {first_bad:?}");
}

#[test]
#[ignore = "needs all actions ported"]
fn walk_run_matches() {
    assert_eq!(run("walk").0, None);
}
