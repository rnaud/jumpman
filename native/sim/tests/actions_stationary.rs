//! Golden test for the stationary/automatic/submerged/drowning action ports against the TypeScript
//! (`GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.stationary.gen.test.ts`).
use jumpman_sim::actions::{automatic, drowning, stationary, submerged};
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::state::*;
use serde_json::Value;
use std::sync::Arc;

fn load(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}
fn nums(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }

fn world() -> CollisionWorld {
    let data = load("../../assets/courses/bob.json");
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
fn stationary_automatic_submerged_drowning_match() {
    let f = load("tests/fixtures/actions_stationary.json");
    let data = load("../../assets/courses/bob.json");
    let world = Arc::new(world());
    let sp = nums(&data["spawn"]["pos"]);
    let mut bad: Vec<String> = Vec::new();
    let mut total_frames = 0;
    for (name, sc) in f.as_object().unwrap() {
        let spawn = SpawnPoint { action: None, pos: [sp[0], sp[1], sp[2]], face_yaw: data["spawn"]["faceYaw"].as_f64().unwrap() };
        let mut m = MarioRuntime::new(world.clone(), spawn, -1600.0);
        let kind = sc["kind"].as_str().unwrap();
        if kind == "submerged" { m.pos[1] = m.floor_height + sc["yOff"].as_f64().unwrap(); }
        m.set_action(sc["action"].as_u64().unwrap() as u32, sc["arg"].as_f64().unwrap());
        if sc["ceil"].as_bool().unwrap() {
            // The TS draws one rng value for the ceiling slack before the frame script; it is baked into frame 0's
            // expectation only through ceilHeight, so recover it from the first hanging position instead.
            m.ceil = Some(Arc::new(Surface {
                object: None, room: None, kind: 5, force: 0, flags: 0, lower_y: 0.0, upper_y: 0.0,
                v1: [0.0; 3], v2: [0.0; 3], v3: [0.0; 3], normal: [0.0, -1.0, 0.0], origin_offset: 0.0,
            }));
            m.ceil_height = m.pos[1] + 160.0 + sc["ceilSlack"].as_f64().unwrap();
        } else if kind == "automatic" {
            m.ceil_height = m.floor_height + 200.0;
        }
        let script = sc["script"].as_array().unwrap();
        let frames = sc["frames"].as_array().unwrap();
        for i in 0..script.len() {
            let s = nums(&script[i]);
            m.input = s[0] as u32;
            m.intended_yaw = s[1];
            m.stick_y = s[3];
            m.controller = ControllerSnapshot { focus_yaw: None, stick_x: s[2], stick_y: s[3], stick_mag: 0.0, button_down: 0, button_pressed: 0 };
            m.anim_frame = s[4];
            match kind {
                "submerged" => { let surf = m.floor_height + 700.0; submerged::submerged_step(&mut m, surf); }
                "stationary" => { stationary::stationary_action(&mut m).expect("handler"); }
                "automatic" => { automatic::automatic_action(&mut m).expect("handler"); }
                _ => { drowning::drowning_action(&mut m).expect("handler"); }
            }
            let got = [
                m.pos[0], m.pos[1], m.pos[2], m.vel[0], m.vel[1], m.vel[2], m.face_angle[0], m.face_angle[1], m.face_angle[2],
                m.action as f64, m.action_state, m.action_timer, m.action_arg, m.forward_vel, m.floor_height, m.anim_length,
                m.anim_frame, m.swim_strength, m.swim_yaw_vel, m.respawns,
            ];
            total_frames += 1;
            if got.to_vec() != nums(&frames[i]) {
                bad.push(format!("{name}: frame {i} want {:?} got {:?}", nums(&frames[i]), got));
                break;
            }
        }
    }
    println!("compared {total_frames} frames over {} scenarios", f.as_object().unwrap().len());
    assert!(bad.is_empty(), "{} scenarios diverged:\n{}", bad.len(), bad.iter().take(8).cloned().collect::<Vec<_>>().join("\n"));
}
