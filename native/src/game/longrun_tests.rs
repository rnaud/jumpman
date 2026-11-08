//! Long-run + focus-fuzz differential tests against `tests/rustFixtures.longrun.gen.test.ts`: 34 course/act
//! runs of 3000 frames each (full `MarioRuntime` + `ObjectManager`, seeded random controller and
//! `Math.random`), comparing a bit-exact hash of Mario and of every object after every frame.
//! `LONGRUN_ONLY=bob.1` restricts the run; `LONGRUN_DUMP=<file>` writes Rust's per-frame state.
use super::world::{CourseRequest, LoadedCourse};
use jumpman_sim::math::seed_random;
use jumpman_sim::state::ControllerSnapshot;
use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
use jumpman_sim::object::ObjId;
use jumpman_sim::object_manager::{self, ObjectManager};
use serde_json::Value;
use std::path::PathBuf;

/// Event count as the TS goldens saw it: the native-only `Sound` / `SoundLoop` events are not part of the parity.
fn non_sound_events(m: &MarioRuntime) -> usize {
    use jumpman_sim::object_manager::ObjectEvent;
    m.objects.events.iter().filter(|e| !matches!(e, ObjectEvent::Sound { .. } | ObjectEvent::SoundLoop { .. })).count()
}

pub(super) struct Hasher(pub u32);
impl Hasher {
    pub(super) fn new() -> Self { Hasher(0x811c9dc5) }
    pub(super) fn word(&mut self, w: u32) { self.0 = (self.0 ^ w).wrapping_mul(0x01000193); }
    pub(super) fn num(&mut self, v: f64) {
        if v.is_nan() { self.word(0x7ff80000); self.word(0); return; }
        let b = v.to_bits();
        self.word(b as u32);
        self.word((b >> 32) as u32);
    }
    pub(super) fn str(&mut self, s: &str) { for c in s.encode_utf16() { self.word(c as u32); } self.word(0xff); }
}

pub(super) fn idx(o: Option<ObjId>) -> f64 { o.map_or(-1.0, |i| i.0 as f64) }

pub(super) fn mario_fields(m: &MarioRuntime) -> Vec<f64> {
    let mut v = vec![];
    v.extend_from_slice(&m.pos);
    v.extend_from_slice(&m.vel);
    v.extend_from_slice(&m.face_angle);
    v.extend_from_slice(&[
        m.forward_vel, m.action as f64, m.prev_action as f64, m.action_state, m.action_timer, m.action_arg,
        m.health, m.hurt_counter, m.heal_counter, m.slide_vel_x, m.slide_vel_z, m.slide_yaw, m.peak_height, m.squish_timer,
        m.floor_height, m.ceil_height, m.intended_yaw, m.intended_mag, m.flags as f64, m.input as f64, m.invinc_timer,
        m.wing_cap_timer, m.metal_cap_timer, m.vanish_cap_timer, idx(m.held_obj), idx(m.used_obj), idx(m.interact_obj),
        m.pole_pos, m.pole_yaw_velocity, m.twirl_yaw, m.quicksand_depth, m.double_jump_timer, m.wall_kick_timer,
        m.angle_vel[0], m.angle_vel[1], m.angle_vel[2], m.lives, m.gravity_scale, m.shell_roll, m.grab_timer,
        idx(m.riding_shell), idx(m.grabbed_by), m.frames_since_a, m.frames_since_b, m.particle_flags as f64,
    ]);
    v
}

#[derive(Clone)]
pub(super) enum F { N(f64), S(String) }

pub(super) fn object_fields(o: &jumpman_sim::object::SimObject) -> Vec<F> {
    let mut v = vec![F::S(o.behavior.as_str().to_string())];
    let n = |x: f64| F::N(x);
    let b = |x: bool| F::N(x as i32 as f64);
    v.extend([
        n(o.pos[0]), n(o.pos[1]), n(o.pos[2]), n(o.vel[0]), n(o.vel[1]), n(o.vel[2]), n(o.move_yaw), n(o.face_yaw), n(o.forward_vel),
        n(o.action as f64), n(o.action_timer), b(o.active), n(o.scale), n(o.hitbox.radius), n(o.hitbox.height), n(o.timer),
        n(o.held_state as f64), b(o.grounded), b(o.hit_wall), n(o.interact_status as f64), b(o.visible), n(o.graph_y_offset),
        n(o.distance_to_mario),
    ]);
    let mut vars: Vec<(&String, &f64)> = o.vars.iter().filter(|(_, x)| x.is_finite()).collect();
    vars.sort_by(|a, b| a.0.cmp(b.0));
    for (k, x) in vars { v.push(F::S(k.clone())); v.push(F::N(*x)); }
    v
}

fn hash_mario(fields: &[f64], e1: usize, e2: usize) -> u32 {
    let mut h = Hasher::new();
    fields.iter().for_each(|v| h.num(*v));
    h.num(e1 as f64);
    h.num(e2 as f64);
    h.0
}

fn hash_objects(mgr: &ObjectManager) -> u32 {
    let mut h = Hasher::new();
    h.word(mgr.len() as u32);
    for id in mgr.ids() {
        for f in object_fields(&mgr[id]) {
            match f { F::N(v) => h.num(v), F::S(s) => h.str(&s) }
        }
    }
    h.0
}

struct Inp { x: f64, y: f64, mag: f64, down: u32, pressed: u32, yaw: f64 }

fn input_script(seed: u32, frames: usize) -> Vec<Inp> {
    let mut s = seed;
    let mut next = || { s = s.wrapping_mul(1664525).wrapping_add(1013904223); s >> 16 };
    let mut out = vec![];
    let mut prev = 0u32;
    while out.len() < frames {
        let len = 6 + next() % 50;
        let still = next() % 5 == 0;
        let (x, y) = if still { (0, 0) } else { (next() as i32 % 129 - 64, next() as i32 % 129 - 64) };
        let (x, y) = (x as f64, y as f64);
        let mag = (x * x + y * y).sqrt().min(64.0);
        let mut down = 0u32;
        if next() % 100 < 45 { down |= 0x8000; }
        if next() % 100 < 25 { down |= 0x4000; }
        if next() % 100 < 15 { down |= 0x2000; }
        let yaw = (next() % 64) as f64 * 1024.0 - 32768.0;
        let pulse = 3 + next() % 12;
        for i in 0..len {
            if out.len() >= frames { break; }
            let mut d = down;
            if (i / pulse) % 2 == 1 { d &= 0x2000; }
            out.push(Inp { x, y, mag, down: d, pressed: d & !prev, yaw });
            prev = d;
        }
    }
    out
}

pub(super) fn nums(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }

/// Replays one course; returns a description of the first mismatching frame.
fn replay(fx: &Value, index: usize, frames: usize) -> Result<(), String> {
    let id = fx["id"].as_str().unwrap();
    let act = fx["act"].as_i64().unwrap() as i32;
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
    // The TS seeds Math.random before the ObjectManager runs its initialisers.
    seed_random((0x1234abcdu32).wrapping_add((index as u32).wrapping_mul(7919)));
    let mut req = CourseRequest::new(id);
    req.act = act;
    let mut course = LoadedCourse::load(&assets, req).map_err(|e| format!("{id}: load {e}"))?;

    // Loader parity: spawn, death height, object list.
    let sp = &fx["spawn"];
    let sp_pos = nums(&sp["pos"]);
    if course.spawn.pos.to_vec() != sp_pos { return Err(format!("{id}: spawn pos {:?} vs {:?}", course.spawn.pos, sp_pos)); }
    if course.spawn.action.map(|a| a as u64) != sp.get("action").and_then(|a| a.as_u64()) { return Err(format!("{id}: spawn action {:?} vs {}", course.spawn.action, sp["action"])); }
    if course.spawn.face_yaw != sp["faceYaw"].as_f64().unwrap() { return Err(format!("{id}: spawn yaw")); }
    if course.death_height != fx["deathHeight"].as_f64().unwrap() { return Err(format!("{id}: death height {} vs {}", course.death_height, fx["deathHeight"])); }
    let kinds = fx["kinds"].as_array().unwrap();
    if course.objects.len() != kinds.len() { return Err(format!("{id}: object count {} vs {}", course.objects.len(), kinds.len())); }
    for (i, k) in kinds.iter().enumerate() {
        let o = &course.objects[ObjId(i as u32)];
        let name = k[0].as_str().unwrap();
        let want = nums(&Value::Array(k.as_array().unwrap()[1..4].to_vec()));
        if o.behavior.as_str() != name || o.home.to_vec() != want {
            return Err(format!("{id}: object {i} is {} {:?}, want {name} {want:?}", o.behavior.as_str(), o.home));
        }
    }

    let spawn = SpawnPoint { action: course.spawn.action, pos: course.spawn.pos, face_yaw: course.spawn.face_yaw };
    let mut m = MarioRuntime::new(course.world.clone(), spawn, course.death_height);
    m.set_objects(std::mem::take(&mut course.objects));
    let script = input_script(0x9e3779b9u32.wrapping_add((index as u32).wrapping_mul(104729)), frames);
    let want = nums(&fx["frames"]);
    let dump = std::env::var("LONGRUN_DUMP").ok().filter(|_| std::env::var("LONGRUN_ONLY").is_ok());
    let mut dumped = vec![];
    for (f, c) in script.iter().enumerate() {
        object_manager::step(&mut m, None, false);
        let e1 = non_sound_events(&m);
        m.objects.drain_events();
        let ctl = ControllerSnapshot { focus_yaw: None, stick_x: c.x, stick_y: c.y, stick_mag: c.mag, button_down: c.down, button_pressed: c.pressed };
        m.step(&ctl, c.yaw, false, false);
        object_manager::after_mario_step(&mut m);
        let e2 = non_sound_events(&m);
        m.objects.drain_events();
        let mf = mario_fields(&m);
        if dump.is_some() {
            let objs: Vec<Vec<Value>> = m.objects.ids().map(|i| object_fields(&m.objects[i]).into_iter().map(|f| match f {
                F::N(v) => serde_json::json!(if v.is_finite() { v } else { f64::NAN }), F::S(s) => Value::String(s) }).collect()).collect();
            dumped.push(serde_json::json!({ "mario": mf, "e1": e1, "e2": e2, "objects": objs }));
        }
        let (mh, oh) = (hash_mario(&mf, e1, e2), hash_objects(&m.objects));
        if mh as f64 != want[f * 2] || oh as f64 != want[f * 2 + 1] {
            if let Some(p) = &dump { let _ = std::fs::write(p, serde_json::to_vec(&dumped).unwrap()); }
            return Err(format!(
                "{id}.{act} frame {f}: mario {} objects {} (action {:#x} pos {:?} events {e1},{e2})",
                if mh as f64 == want[f * 2] { "ok" } else { "DIFFERS" },
                if oh as f64 == want[f * 2 + 1] { "ok" } else { "DIFFERS" },
                m.action, m.pos
            ));
        }
    }
    Ok(())
}

#[test]
fn longrun_matches_typescript() {
    super::world::legacy_fixture_mode(true); // goldens predate the group 1 actors
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sim/tests/fixtures/longrun.json");
    let fx: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let frames = fx["frames"].as_u64().unwrap() as usize;
    let only = std::env::var("LONGRUN_ONLY").ok();
    let mut failures = vec![];
    for (i, c) in fx["courses"].as_array().unwrap().iter().enumerate() {
        let tag = format!("{}.{}", c["id"].as_str().unwrap(), c["act"]);
        if only.as_deref().is_some_and(|o| o != tag) { continue; }
        // rr's terrain was exported with both `#ifdef VERSION_JP` vertex lists (ghost triangles across the
        // level); the TS fixture recorded that data, the corrected export cannot match it.
        if matches!(c["id"].as_str(), Some("rr" | "lll")) { continue; }
        if let Err(e) = replay(c, i, frames) { failures.push(e); }
    }
    assert!(failures.is_empty(), "{} course(s) diverged:\n{}", failures.len(), failures.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// Focus fuzz (see `runFocus` in the generator): Mario is dropped next to each object in turn and
// walks at it / mashes buttons, so poles, held objects, chain chomp, penguins, snowman... all run.

fn replay_focus(fx: &Value, index: usize, frames: usize, window: usize) -> Result<(), String> {
    use jumpman_sim::math::{atan2s, to_s16};
    use jumpman_sim::state::ACT_FREEFALL;
    let id = fx["id"].as_str().unwrap();
    let act = fx["act"].as_i64().unwrap() as i32;
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
    seed_random((0x77aa55ccu32).wrapping_add((index as u32).wrapping_mul(6151)));
    let mut req = CourseRequest::new(id);
    req.act = act;
    let mut course = LoadedCourse::load(&assets, req).map_err(|e| format!("{id}: load {e}"))?;
    let spawn = SpawnPoint { action: course.spawn.action, pos: course.spawn.pos, face_yaw: course.spawn.face_yaw };
    // Teleport order: first object of each behavior, then the rest (warps excluded).
    let mut seen = std::collections::HashSet::new();
    let (mut first, mut rest) = (vec![], vec![]);
    for i in course.objects.ids() {
        let b = course.objects[i].behavior;
        if b.as_str() == "warp" { continue; }
        if seen.insert(b.as_str().to_string()) { first.push(i); } else { rest.push(i); }
    }
    let order: Vec<ObjId> = first.into_iter().chain(rest).collect();
    let mut m = MarioRuntime::new(course.world.clone(), spawn, course.death_height);
    m.set_objects(std::mem::take(&mut course.objects));
    if fx["snowmanDone"] == true { m.objects.set_snowman_completed(true); }
    let mut s = (0xabc12345u32).wrapping_add((index as u32).wrapping_mul(7717));
    let mut next = || { s = s.wrapping_mul(1664525).wrapping_add(1013904223); s >> 16 };
    let base = atan2s(-64.0, 0.0) as f64;
    let want = nums(&fx["frames"]);
    let mut actions = std::collections::BTreeSet::new();
    let mut prev_down = 0u32;
    struct Seg { x: f64, y: f64, mag: f64, down: u32, pulse: u32, yaw: f64, left: u32, i: u32 }
    let mut seg: Option<Seg> = None;
    for f in 0..frames {
        let k = f % window;
        if k == 0 && !order.is_empty() {
            let tp = m.objects[order[(f / window) % order.len()]].pos;
            let dx = ((next() % 9) as f64 - 4.0) * 70.0;
            let dz = ((next() % 9) as f64 - 4.0) * 70.0;
            m.pos = [tp[0] + dx, tp[1] + 100.0, tp[2] + dz];
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.face_angle[1] = atan2s(tp[2] - m.pos[2], tp[0] - m.pos[0]) as f64;
            m.set_action(ACT_FREEFALL, 0.0);
            seg = None;
        }
        let (cx, cy, cmag, cdown, cyaw);
        if k < 45 && !order.is_empty() {
            let tp = m.objects[order[(f / window) % order.len()]].pos;
            let target = atan2s(tp[2] - m.pos[2], tp[0] - m.pos[0]) as f64;
            cdown = if k % 20 == 5 || k % 20 == 6 { 0x8000 } else { 0 };
            cx = 0.0; cy = 64.0; cmag = 64.0;
            cyaw = to_s16(target - base) as f64;
        } else {
            if seg.as_ref().map_or(true, |g| g.left == 0) {
                let len = 6 + next() % 30;
                let still = next() % 6 == 0;
                let (x, y) = if still { (0, 0) } else { (next() as i32 % 129 - 64, next() as i32 % 129 - 64) };
                let (x, y) = (x as f64, y as f64);
                let mut down = 0u32;
                if next() % 100 < 45 { down |= 0x8000; }
                if next() % 100 < 40 { down |= 0x4000; }
                if next() % 100 < 15 { down |= 0x2000; }
                let pulse = 3 + next() % 10;
                let yaw = (next() % 64) as f64 * 1024.0 - 32768.0;
                seg = Some(Seg { x, y, mag: (x * x + y * y).sqrt().min(64.0), down, pulse, yaw, left: len, i: 0 });
            }
            let g = seg.as_mut().unwrap();
            let mut d = g.down;
            if (g.i / g.pulse) % 2 == 1 { d &= 0x2000; }
            if k % 14 == 0 { d |= 0x4000; }
            g.i += 1; g.left -= 1;
            cx = g.x; cy = g.y; cmag = g.mag; cdown = d; cyaw = g.yaw;
        }
        let cpressed = cdown & !prev_down;
        prev_down = cdown;
        object_manager::step(&mut m, None, false);
        let e1 = non_sound_events(&m);
        m.objects.drain_events();
        let ctl = ControllerSnapshot { focus_yaw: None, stick_x: cx, stick_y: cy, stick_mag: cmag, button_down: cdown, button_pressed: cpressed };
        m.step(&ctl, cyaw, false, false);
        object_manager::after_mario_step(&mut m);
        let e2 = non_sound_events(&m);
        m.objects.drain_events();
        let mf = mario_fields(&m);
        actions.insert(m.action);
        let (mh, oh) = (hash_mario(&mf, e1, e2), hash_objects(&m.objects));
        if mh as f64 != want[f * 2] || oh as f64 != want[f * 2 + 1] {
            return Err(format!(
                "focus {id}.{act} frame {f} (window {}, k {k}): mario {} objects {} (action {:#x} pos {:?})",
                f / window,
                if mh as f64 == want[f * 2] { "ok" } else { "DIFFERS" },
                if oh as f64 == want[f * 2 + 1] { "ok" } else { "DIFFERS" },
                m.action, m.pos
            ));
        }
    }
    let want_actions: std::collections::BTreeSet<u32> = fx["actions"].as_array().unwrap().iter().map(|a| a.as_u64().unwrap() as u32).collect();
    if actions != want_actions { return Err(format!("focus {id}.{act}: action sets differ")); }
    Ok(())
}

#[test]
fn focus_fuzz_matches_typescript() {
    super::world::legacy_fixture_mode(true); // goldens predate the group 1 actors
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sim/tests/fixtures/focus.json");
    let fx: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let frames = fx["frames"].as_u64().unwrap() as usize;
    let window = fx["window"].as_u64().unwrap() as usize;
    let only = std::env::var("LONGRUN_ONLY").ok();
    let mut failures = vec![];
    for (i, c) in fx["courses"].as_array().unwrap().iter().enumerate() {
        let tag = format!("{}.{}", c["id"].as_str().unwrap(), c["act"]);
        if only.as_deref().is_some_and(|o| o != tag) { continue; }
        // lll's terrain was exported with both `#ifdef VERSION_JP` vertex lists (see longrun above)
        if c["id"].as_str() == Some("lll") { continue; }
        if let Err(e) = replay_focus(c, i, frames, window) { failures.push(e); }
    }
    // Guard against a vacuous run: the interactions this test exists for must actually happen.
    let seen: std::collections::HashSet<u32> = fx["courses"].as_array().unwrap().iter()
        .flat_map(|c| c["actions"].as_array().unwrap().iter().map(|a| a.as_u64().unwrap() as u32)).collect();
    use jumpman_sim::state::*;
    for (name, a) in [("pole grab", ACT_GRAB_POLE_SLOW), ("holding pole", ACT_HOLDING_POLE), ("climbing pole", ACT_CLIMBING_POLE),
                      ("picking up", ACT_PICKING_UP), ("hold walking", ACT_HOLD_WALKING), ("hold jump", ACT_HOLD_JUMP), ("air throw", ACT_AIR_THROW)] {
        if only.is_none() { assert!(seen.contains(&a), "focus fixture never reached {name}"); }
    }
    assert!(failures.is_empty(), "{} focus course(s) diverged:\n{}", failures.len(), failures.join("\n"));
}
