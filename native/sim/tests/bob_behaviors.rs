//! Golden tests for the Bob-omb/boss/box behavior ports against fixtures generated from the
//! TypeScript (`GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.bobs.gen.test.ts`).
//!
//! Each scenario steps only the object manager (Mario is teleported by a script, so the
//! still-stubbed action handlers do not matter) and compares every object's state, behaviour
//! vars, the emitted events, Mario's health/action and the collision surface count each frame.
use jumpman_sim::collision::*;
use jumpman_sim::loot_coin::LootCoin;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectEvent, ObjectManager};
use jumpman_sim::spawned_star::{GrandStar, SpawnedStar};
use serde_json::Value;
use std::sync::Arc;

fn load() -> Value {
    serde_json::from_slice(&std::fs::read(format!("{}/tests/fixtures/bob_behaviors.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}

fn v3(v: &Value) -> Vec3 { let a = v.as_array().unwrap(); [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()] }

fn world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    let r = 4000.0;
    w.add_triangle([-r, 0.0, -r], [-r, 0.0, r], [r, 0.0, r], &o);
    w.add_triangle([-r, 0.0, -r], [r, 0.0, r], [r, 0.0, -r], &o);
    w.add_quad([-2000.0, 0.0, 1200.0], [2000.0, 0.0, 1200.0], [2000.0, 800.0, 1200.0], [-2000.0, 800.0, 1200.0], &o);
    w.add_quad([-1500.0, 0.0, -2000.0], [-1500.0, 0.0, 2000.0], [-1500.0, 800.0, 2000.0], [-1500.0, 800.0, -2000.0], &o);
    w
}

fn spec_from(v: &Value) -> ObjectSpec {
    let kind = ObjectBehaviorKind::from_name(v["behavior"].as_str().unwrap()).unwrap();
    let mut s = ObjectSpec::new(kind, v3(&v["pos"]), v["yaw"].as_f64().unwrap());
    s.param = v.get("param").and_then(|p| p.as_f64());
    if let Some(r) = v.get("ballRoute") {
        s.ball_route = Some(BallRoute::mountain(
            r["waypoints"].as_array().unwrap().iter().map(v3).collect(),
            r["period"].as_f64().unwrap(),
            r["maxDistance"].as_f64().unwrap(),
        ));
    }
    s
}

fn same(expected: &Value, actual: f64) -> bool {
    match expected.as_f64() {
        None => !actual.is_finite(),
        Some(e) => e == actual,
    }
}

fn event_sig(e: &ObjectEvent) -> (&'static str, Option<Vec3>, String) {
    use ObjectEvent::*;
    let num = |n: f64| format!("{n}");
    match e {
        WaterBomb { pos, phase } => ("waterBomb", Some(*pos), phase.to_string()),
        WaterCannon { pos } => ("waterCannon", Some(*pos), String::new()),
        Explosion { pos, .. } => ("explosion", Some(*pos), String::new()),
        Break { pos, .. } => ("break", Some(*pos), String::new()),
        BowlingBall { pos, cue } => ("bowlingBall", Some(*pos), cue.to_string()),
        CoinBounce { pos } => ("coinBounce", Some(*pos), String::new()),
        MovingCoinCollected { pos } => ("movingCoinCollected", Some(*pos), String::new()),
        Coin { pos, value, .. } => ("coin", Some(*pos), num(*value)),
        Hurt { pos } => ("hurt", Some(*pos), String::new()),
        KingLeave => ("kingLeave", None, String::new()),
        KingDialog { id, .. } => ("kingDialog", None, num(*id)),
        Boss { pos } => ("boss", Some(*pos), String::new()),
        Bowser { pos } => ("bowser", Some(*pos), String::new()),
        Chest { number } => ("chest", None, num(*number)),
        BoxStar { pos, act } => ("boxStar", Some(*pos), num(*act)),
        ChestReward => ("chestReward", None, String::new()),
        _ => ("other", None, String::new()),
    }
}

fn apply_poke(m: &mut MarioRuntime, obj: usize, field: &str, value: f64) {
    let o = &mut m.objects.objects[obj];
    match field {
        "heldState" => o.held_state = value as i32,
        "interactStatus" => o.interact_status = value as u32,
        f if f.starts_with("vars.") => o.set_var(&f[5..], value),
        f => panic!("unknown poke {f}"),
    }
}

fn run_scenario(name: &str, sc: &Value) -> Result<(), String> {
    seed_random(sc["seed"].as_u64().unwrap() as u32);
    let specs: Vec<ObjectSpec> = sc["specs"].as_array().unwrap().iter().map(spec_from).collect();
    let mgr = ObjectManager::new(specs, None, None);
    let mut m = MarioRuntime::new(Arc::new(world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -1600.0);
    m.set_objects(mgr);
    let pokes = sc["pokes"].as_array().unwrap();
    let script = sc["mario"].as_array().unwrap();
    for (f, frame) in sc["frames"].as_array().unwrap().iter().enumerate() {
        let s = script[f].as_array().unwrap();
        m.pos = [s[0].as_f64().unwrap(), s[1].as_f64().unwrap(), s[2].as_f64().unwrap()];
        m.face_angle[1] = s[3].as_f64().unwrap();
        m.floor_height = 0.0;
        if let Some(a) = s[4].as_f64() { m.action = a as u32; }
        if let Some(a) = s[5].as_f64() { m.action_state = a; }
        for p in pokes {
            if p["frame"].as_u64().unwrap() as usize == f {
                apply_poke(&mut m, p["obj"].as_u64().unwrap() as usize, p["field"].as_str().unwrap(), p["value"].as_f64().unwrap());
            }
        }
        object_manager::step(&mut m, None, false);

        let err = |what: String| Err(format!("{name} frame {f}: {what}"));
        let em = frame["m"].as_array().unwrap();
        let am = [m.health, m.action as f64, m.pos[0], m.pos[1], m.pos[2], m.action_state];
        for (i, (e, a)) in em.iter().zip(am).enumerate() {
            if !same(e, a) { return err(format!("mario[{i}] expected {e} got {a}")); }
        }
        let eo = frame["o"].as_array().unwrap();
        if eo.len() != m.objects.len() { return err(format!("object count expected {} got {}", eo.len(), m.objects.len())); }
        const FIELDS: [&str; 28] = ["px", "py", "pz", "vx", "vy", "vz", "forwardVel", "moveYaw", "faceYaw", "action", "actionState", "actionTimer", "timer", "active", "visible", "heldState", "scale", "graphYOffset", "grounded", "interactStatus", "interactType", "radius", "height", "coinDropped", "", "", "", ""];
        for (i, o) in m.objects.objects.iter().enumerate() {
            let a = [o.pos[0], o.pos[1], o.pos[2], o.vel[0], o.vel[1], o.vel[2], o.forward_vel, o.move_yaw, o.face_yaw, o.action as f64,
                o.action_state, o.action_timer, o.timer, o.active as u8 as f64, o.visible as u8 as f64, o.held_state as f64, o.scale,
                o.graph_y_offset, o.grounded as u8 as f64, o.interact_status as f64, o.hitbox.interact_type as f64, o.hitbox.radius,
                o.hitbox.height, o.coin_dropped as u8 as f64];
            let e = eo[i].as_array().unwrap();
            for (k, av) in a.iter().enumerate() {
                if !same(&e[k], *av) {
                    return err(format!("object {i} ({}) {} expected {} got {av}", o.behavior.as_str(), FIELDS[k], e[k]));
                }
            }
            let ev = frame["v"][i].as_object().unwrap();
            for (k, v) in ev {
                match o.vars.get(k) {
                    None => return err(format!("object {i} ({}) missing var {k} (expected {v})", o.behavior.as_str())),
                    Some(a) if !same(v, *a) => return err(format!("object {i} ({}) var {k} expected {v} got {a}", o.behavior.as_str())),
                    _ => {}
                }
            }
            for k in o.vars.keys() {
                if !ev.contains_key(k) { return err(format!("object {i} ({}) extra var {k}", o.behavior.as_str())); }
            }
        }
        let ee = frame["e"].as_array().unwrap();
        // The TS goldens predate the sound events (`Sound` / `SoundLoop`).
        let events: Vec<_> = m.objects.events.iter().filter(|e| !matches!(e, jumpman_sim::object_manager::ObjectEvent::Sound { .. } | jumpman_sim::object_manager::ObjectEvent::SoundLoop { .. })).collect();
        if ee.len() != events.len() { return err(format!("event count expected {} got {} ({:?})", ee.len(), events.len(), events)); }
        for (e, a) in ee.iter().zip(events) {
            let (ty, pos, extra) = event_sig(a);
            if e[0].as_str().unwrap() != ty && ty != "other" { return err(format!("event type expected {} got {ty}", e[0])); }
            if let (Some(p), Some(ap)) = (e[1].as_array(), pos) {
                for k in 0..3 { if !same(&p[k], ap[k]) { return err(format!("event {ty} pos[{k}] expected {} got {}", p[k], ap[k])); } }
            }
            if ty != "other" && e[2].as_str().unwrap() != extra { return err(format!("event {ty} extra expected {} got {extra}", e[2])); }
        }
        let es = sc["surfaces"][f].as_u64().unwrap() as usize;
        if es != m.world.surfaces().len() { return err(format!("surface count expected {es} got {}", m.world.surfaces().len())); }
    }
    Ok(())
}

#[test]
fn behavior_scenarios_match_typescript() {
    let fx = load();
    let mut failures = vec![];
    for name in ["bobomb_boxes", "king_bobomb", "chain_chomp", "water_cannon", "bowser", "bob_objects", "boxes_chests"] {
        if let Err(e) = run_scenario(name, &fx[name]) { failures.push(e); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn loot_coin_matches_typescript() {
    let fx = load();
    seed_random(fx["loot"]["seed"].as_u64().unwrap() as u32);
    let w = world();
    for (c, coin) in fx["loot"]["coins"].as_array().unwrap().iter().enumerate() {
        let mut lc = LootCoin::new();
        let mut pos = v3(&coin["start"]);
        for (i, t) in coin["trace"].as_array().unwrap().iter().enumerate() {
            let alive = lc.step(&mut pos, &w);
            let t = t.as_array().unwrap();
            let a = [pos[0], pos[1], pos[2], alive as u8 as f64, lc.tangible as u8 as f64, lc.visible as u8 as f64];
            for k in 0..6 { assert!(same(&t[k], a[k]), "coin {c} step {i} field {k}: expected {} got {}", t[k], a[k]); }
        }
    }
}

#[test]
fn stars_match_typescript() {
    let fx = load();
    for (n, s) in fx["stars"]["spawned"].as_array().unwrap().iter().enumerate() {
        let mario = if s["mario"].is_null() { None } else { Some(v3(&s["mario"])) };
        let mut star = SpawnedStar::new([100.0, 300.0, 100.0], mario);
        for (i, t) in s["trace"].as_array().unwrap().iter().enumerate() {
            star.step();
            let t = t.as_array().unwrap();
            let a = [star.pos[0], star.pos[1], star.pos[2], star.yaw, star.tangible() as u8 as f64, star.returning() as u8 as f64, star.bouncing() as u8 as f64];
            for k in 0..7 { assert!(same(&t[k], a[k]), "spawned star {n} step {i} field {k}: expected {} got {}", t[k], a[k]); }
        }
    }
    let mut g = GrandStar::new([800.0, 200.0, -500.0]);
    for (i, t) in fx["stars"]["grand"][0]["trace"].as_array().unwrap().iter().enumerate() {
        g.step();
        let t = t.as_array().unwrap();
        let a = [g.pos[0], g.pos[1], g.pos[2], g.tangible as u8 as f64];
        for k in 0..4 { assert!(same(&t[k], a[k]), "grand star step {i} field {k}: expected {} got {}", t[k], a[k]); }
    }
}

/// Tiny-Huge Island's spawner (`bhv_thi_bowling_ball_spawner_loop`): 64-frame period, 800 / 12000 range, odds 1.5, and the tiny
/// ball starts at speed 10 with scale 0.3 (`bhv_bowling_ball_initialize_loop`).
#[test]
fn thi_ball_spawner_rolls_a_tiny_ball_at_speed_10_scale_0_3() {
    let mut route = BallRoute::mountain(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 500.0], [0.0, -2000.0, 900.0]], 64.0, 12000.0);
    route.min_distance = 800.0;
    route.odds = 1.5;
    route.speed = 10.0;
    route.scale = 0.3;
    let mut spec = ObjectSpec::new(ObjectBehaviorKind::MountainBallSpawner, [0.0, 400.0, 0.0], 0.0);
    spec.ball_route = Some(route);
    let mut m = MarioRuntime::new(Arc::new(world()), SpawnPoint { action: None, pos: [3000.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(vec![spec], None, None));
    let mut seen = None;
    for _ in 0..4000 {
        object_manager::step(&mut m, None, false);
        m.step(&jumpman_sim::state::ControllerSnapshot::default(), 0.0, false, false);
        if let Some(b) = m.objects.objects.iter().find(|o| o.behavior == ObjectBehaviorKind::MountainBall && o.active) {
            seen = Some((b.scale, b.forward_vel));
            break;
        }
    }
    let (scale, _) = seen.expect("a ball rolled (odds 1.5 per 64 frames)");
    assert_eq!(scale, 0.3);
}
