//! Golden tests for the world-actor behaviors (koopa, penguins, snowman, whomp, boo, bully, ...)
//! generated from the TypeScript: `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.behaviors.gen.test.ts`.
//! Mario is pinned/scripted (his actions are not under test); every frame the full state of every
//! object (including behavior vars) is compared bit-for-bit.
use jumpman_sim::math::set_fixed_random;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectManager};
use serde_json::Value;
use std::sync::Arc;

fn load(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}
fn nums(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }
fn v3(v: &Value) -> [f64; 3] { let n = nums(v); [n[0], n[1], n[2]] }

fn bob_world() -> CollisionWorld {
    let data = load("../../assets/courses/bob.json");
    let verts: Vec<[f64; 3]> = data["vertices"].as_array().unwrap().iter().map(|v| v3(v)).collect();
    let mut w = CollisionWorld::default();
    for t in data["triangles"].as_array().unwrap() {
        let t = nums(t);
        let kind = t[3] as i32;
        let opts = SurfaceOptions { kind, force: t[4] as i32, flags: if (0x76..=0x79).contains(&kind) { 2 } else { 0 }, ..Default::default() };
        w.add_triangle(verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize], &opts);
    }
    w
}

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-7000.0, 0.0, -7000.0], [-7000.0, 0.0, 7000.0], [7000.0, 0.0, 7000.0], &o);
    w.add_triangle([-7000.0, 0.0, -7000.0], [7000.0, 0.0, 7000.0], [7000.0, 0.0, -7000.0], &o);
    w
}

fn spec_of(s: &Value) -> ObjectSpec {
    let kind = ObjectBehaviorKind::from_name(s["behavior"].as_str().unwrap()).unwrap();
    let mut spec = ObjectSpec::new(kind, v3(&s["pos"]), s["yaw"].as_f64().unwrap());
    spec.param = s.get("param").and_then(|p| p.as_f64());
    spec
}

fn same(a: f64, b: &Value) -> bool {
    match b.as_f64() {
        Some(b) => a == b,
        None => !a.is_finite(), // JSON null = NaN / Infinity
    }
}

fn check(name: &str) {
    let all = load("tests/fixtures/behaviors.json");
    let sc = &all[name];
    let world = Arc::new(if sc["world"] == "bob" { bob_world() } else { flat_world() });
    set_fixed_random(Some(0.37));
    let specs: Vec<ObjectSpec> = sc["specs"].as_array().unwrap().iter().map(spec_of).collect();
    let race = sc.get("race").filter(|r| !r.is_null()).map(|r| PenguinRace {
        path: r["path"].as_array().unwrap().iter().map(|p| { let n = nums(p); [n[0], n[1], n[2], n[3]] }).collect(),
        finish: v3(&r["finish"]),
        shortcut: v3(&r["shortcut"]),
    });
    let path = sc.get("snowmanPath").filter(|r| !r.is_null()).map(|p| p.as_array().unwrap().iter().map(v3).collect());
    let mp = v3(&sc["mario"]);
    let mut m = MarioRuntime::new(world, SpawnPoint { action: None, pos: mp, face_yaw: 0.0 }, -1600.0);
    m.set_objects(ObjectManager::new(specs, race, path));
    m.pos = mp;
    let frames = sc["frames"].as_u64().unwrap() as usize;
    let sets = sc["sets"].as_array().unwrap();
    for f in 0..frames {
        let script = nums(&sc["marioScript"][f]);
        if sc["follow"] == true { m.pos = [script[0], script[1], script[2]]; }
        m.action = script[3] as u32;
        m.health = 0x880 as f64;
        for s in sets {
            if s[0].as_u64().unwrap() as usize == f { m.objects[ObjId(s[1].as_u64().unwrap() as u32)].set_var(s[2].as_str().unwrap(), s[3].as_f64().unwrap()); }
        }
        object_manager::step(&mut m, None, false);
        let snap = &sc["snapshots"][f];
        let want_objs = snap["objects"].as_array().unwrap();
        assert_eq!(m.objects.len(), want_objs.len(), "{name} frame {f}: object count");
        let mario = nums(&snap["mario"]);
        let got_mario = [m.pos[0], m.pos[1], m.pos[2], m.action as f64, m.health];
        assert_eq!(got_mario.to_vec(), mario, "{name} frame {f}: mario");
        assert_eq!(m.objects.events.len() as u64, snap["events"].as_u64().unwrap(), "{name} frame {f}: events");
        for (i, w) in want_objs.iter().enumerate() {
            let o = &m.objects[ObjId(i as u32)];
            assert_eq!(o.behavior.as_str(), w[0].as_str().unwrap(), "{name} frame {f} obj {i}: behavior");
            let fields: [(&str, f64); 23] = [
                ("pos0", o.pos[0]), ("pos1", o.pos[1]), ("pos2", o.pos[2]), ("vel0", o.vel[0]), ("vel1", o.vel[1]), ("vel2", o.vel[2]),
                ("moveYaw", o.move_yaw), ("faceYaw", o.face_yaw), ("forwardVel", o.forward_vel), ("action", o.action as f64),
                ("actionTimer", o.action_timer), ("active", o.active as i32 as f64), ("scale", o.scale), ("radius", o.hitbox.radius),
                ("height", o.hitbox.height), ("timer", o.timer), ("held", o.held_state as f64), ("grounded", o.grounded as i32 as f64),
                ("hitWall", o.hit_wall as i32 as f64), ("interactStatus", o.interact_status as f64), ("visible", o.visible as i32 as f64),
                ("graphY", o.graph_y_offset), ("dist", o.distance_to_mario),
            ];
            for (k, (label, got)) in fields.iter().enumerate() {
                assert!(same(*got, &w[k + 1]), "{name} frame {f} obj {i} ({}): {label} got {got} want {}", o.behavior.as_str(), w[k + 1]);
            }
            let want_vars = w[24].as_array().unwrap();
            let mut got_vars: Vec<(&String, &f64)> = o.vars.iter().filter(|(_, v)| v.is_finite()).collect();
            got_vars.sort_by(|a, b| a.0.cmp(b.0));
            let keys: Vec<&str> = got_vars.iter().map(|(k, _)| k.as_str()).collect();
            let want_keys: Vec<&str> = want_vars.iter().map(|p| p[0].as_str().unwrap()).collect();
            assert_eq!(keys, want_keys, "{name} frame {f} obj {i} ({}): var keys", o.behavior.as_str());
            for (p, (k, v)) in want_vars.iter().zip(got_vars) {
                assert!(same(*v, &p[1]), "{name} frame {f} obj {i} ({}): var {k} got {v} want {}", o.behavior.as_str(), p[1]);
            }
        }
    }
    set_fixed_random(None);
}

#[test] fn bob_actors_match() { check("bob_actors"); }
#[test] fn quick_race_matches() { check("quick_race"); }
#[test] fn penguin_race_matches() { check("penguin_race"); }
#[test] fn snowman_matches() { check("snowman"); }
#[test] fn koopa_kick_matches() { check("koopa_kick"); }

#[test]
fn js_sin_cos_match_v8() {
    use jumpman_sim::math::{js_cos, js_sin};
    let f = load("tests/fixtures/js_math.json");
    let xs = nums(&f["xs"]);
    let (sin, cos) = (nums(&f["sin"]), nums(&f["cos"]));
    let mut bad = vec![];
    for (i, x) in xs.iter().enumerate() {
        if js_sin(*x) != sin[i] { bad.push(format!("sin({x}) got {} want {}", js_sin(*x), sin[i])); }
        if js_cos(*x) != cos[i] { bad.push(format!("cos({x}) got {} want {}", js_cos(*x), cos[i])); }
    }
    assert!(bad.is_empty(), "{} mismatches of {}: {:?}", bad.len(), xs.len() * 2, &bad[..bad.len().min(6)]);
}
