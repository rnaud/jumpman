//! Warp differential test against `tests/rustFixtures.warps.gen.test.ts`: what every kind of warp asks the course
//! loader / the linked-area swap / the act selector for (objects, doors, paintings, death, fall and floor warps), on
//! which tick, with which carried state, plus the arrival state (spawn and the next 60 frames) for every
//! (course, entry, push, act, area) a warp can load and the area-changing instant warps (ddd, wdw, castle_inside).
//! `WARPS_ONLY=<substring>` restricts the run to matching names.
use super::game2_tests::{all_objects_hash, assets, ctl, hash, lcg, mario_hash, nn};
use super::game_tests::WorldHasher;
use super::runner::{GameSim, SimEvent};
use super::world::CourseRequest;
use crate::objects::colliders::{build_colliders, ObjectColliders};
use crate::objects::stage::StageHooks;
use crate::progress::flow::{Flow, Progress, Switch, Then};
use crate::progress::save::SaveFile;
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::*;
use serde_json::Value;
use std::path::PathBuf;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sim/tests/fixtures/warps.json");
    serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn dash(e: &Option<String>) -> String {
    e.clone().unwrap_or_else(|| "-".into())
}

fn load_string(sw: &Switch) -> String {
    let r = &sw.request;
    format!("load:{}:{}:{}:{}:{}", r.id, dash(&r.entry), u8::from(r.push), r.act, r.area)
}

/// What the progression layer queued, in the fixture's notation.
fn resolution(p: &Progress) -> String {
    if let Some(ch) = &p.pending_area {
        return format!("swap:{}:{}:{}", ch.area, dash(&ch.entry), u8::from(ch.push));
    }
    if let Some(sw) = &p.pending {
        return load_string(sw);
    }
    match &p.flow {
        Flow::ActSelect(sel) => format!("select:{}:{}", sel.course, dash(&sel.entry)),
        Flow::Closing { then: Then::Load(sw), .. } => load_string(sw),
        Flow::Closing { then: Then::GameOver, .. } | Flow::GameOver { .. } => "gameover".into(),
        _ => "none".into(),
    }
}

fn request(id: &str, act: i32, area: i32, entry: Option<&str>, push: bool) -> CourseRequest {
    let mut r = CourseRequest::new(id);
    r.act = act;
    r.area = area;
    r.entry = entry.map(str::to_string);
    r.push = push;
    r
}

fn load_req(req: CourseRequest, index: u32) -> GameSim {
    jumpman_sim::math::seed_random(0x2badbeefu32.wrapping_add(index.wrapping_mul(7919)));
    GameSim::load(&assets(), req).unwrap()
}

fn start(id: &str, act: i32, area: i32, index: u32) -> (GameSim, Progress) {
    let mut sim = load_req(request(id, act, area, None, false), index);
    let p = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
    (sim, p)
}

/// `foo.2.json` -> ("foo", 2)
fn split_file(file: &str) -> (String, i32) {
    let stem = file.trim_end_matches(".json");
    match stem.rsplit_once('.') {
        Some((id, a)) if a.len() == 1 && a.chars().all(|c| c.is_ascii_digit()) => (id.to_string(), a.parse().unwrap()),
        _ => (stem.to_string(), 1),
    }
}

fn course_json(file: &str) -> Value {
    serde_json::from_slice(&std::fs::read(assets().join("courses").join(file)).unwrap()).unwrap()
}

/// The world factory's own colliders (moving platforms, the cannon lid) and the object colliders: part of the TS world.
fn rig_up(sim: &mut GameSim, p: &Progress, id: &str) -> ObjectColliders {
    let specs = sim.course.extras.platforms.clone();
    let mut platforms = super::platform::Platforms::new(&specs);
    platforms.install(&mut sim.mario);
    if !sim.course.extras.cannons.is_empty() {
        let mut c = super::cannons::Cannons::new(&sim.course.extras.cannons, sim.course.extras.extra_collision.get("cannonLid"));
        c.reset(&mut sim.mario.world, p.save.cannon_open(id));
    }
    build_colliders(sim, false)
}

fn tick(sim: &mut GameSim, colliders: &mut ObjectColliders, c: &ControllerSnapshot) {
    let mut hooks = StageHooks { colliders: Some(colliders), cannons: None, wing_unlocked: false, course_id: "x", cannon_unlocked: false, operator: None };
    sim.tick_with(c, &mut hooks);
}

/// One fixed step as `progress/mod.rs` schedules it.
fn step(sim: &mut GameSim, p: &mut Progress, colliders: Option<&mut ObjectColliders>, c: &ControllerSnapshot) {
    sim.camera_yaw = 0;
    if p.sim_running() {
        p.pre_tick(sim);
        if p.sim_running() {
            match colliders {
                Some(k) => tick(sim, k, c),
                None => sim.tick(c),
            }
        }
    }
    p.post_tick(sim, c);
}

fn carry_string(m: &jumpman_sim::mario_runtime::MarioRuntime) -> String {
    format!("{},{},{}", m.health, m.hurt_counter, m.heal_counter)
}

fn static_resolution(name: &str) -> Option<String> {
    let mut words = name.split(' ');
    let kind = words.next()?;
    match kind {
        "warp" => {
            let file = words.next()?;
            let i: usize = words.next()?.trim_start_matches('#').parse().ok()?;
            let (id, area) = split_file(file);
            let data = course_json(file);
            let mut list: Vec<Value> = data["warps"].as_array().cloned().unwrap_or_default();
            list.extend(data["objects"].as_array().cloned().unwrap_or_default().into_iter().filter(|o| o.get("warpTo").is_some_and(|v| !v.is_null())));
            let w = &list[i];
            let (mut sim, mut p) = start(&id, 1, area, 0);
            sim.events.push(SimEvent::Object(ObjectEvent::Warp {
                destination: w["warpTo"].as_str().unwrap().to_string(),
                entry: w["warpEntry"].as_str().map(str::to_string),
                area: w["warpArea"].as_i64().map(|a| a as i32).filter(|a| *a != 0),
            }));
            // `post_tick` consumes the events the last tick left; keep them (the sim was not ticked).
            p.post_tick(&mut sim, &ctl(0.0, 0.0, 0, 0));
            Some(resolution(&p))
        }
        "door" => {
            let file = words.next()?;
            let i: usize = words.next()?.trim_start_matches('#').parse().ok()?;
            let push = words.next()? == "push1";
            let (id, area) = split_file(file);
            let (mut sim, mut p) = start(&id, 1, area, 0);
            sim.mario.set_action(if push { ACT_PUSHING_DOOR } else { ACT_PULLING_DOOR }, if push { 6.0 } else { 5.0 });
            sim.mario.action_timer = 35.0;
            p.flow = Flow::Entering { door: i, locked: false, frame: 0 };
            p.pre_tick(&mut sim);
            Some(resolution(&p))
        }
        "death" | "fall" if name.contains(".json") => {
            let file = words.next()?;
            let (id, area) = split_file(file);
            let (mut sim, mut p) = start(&id, 3, area, 0);
            p.test_request_death(kind == "fall");
            p.post_tick(&mut sim, &ctl(0.0, 0.0, 0, 0));
            Some(resolution(&p))
        }
        _ => None,
    }
}

/// Drive the loop until the queued warp appears; returns (ask tick, load tick) with the resolution.
fn dynamic_death(name: &str, index: u32) -> String {
    // "death <id>.<area> <die|fall> lives<n>"
    let w: Vec<&str> = name.split(' ').collect();
    let (id, area) = {
        let (a, b) = w[1].split_once('.').unwrap();
        (a.to_string(), b.parse::<i32>().unwrap())
    };
    let fall = w[2] != "die";
    let lives: f64 = w[3].trim_start_matches("lives").parse().unwrap();
    let act = if id == "totwc" { 1 } else { 2 };
    let (mut sim, mut p) = start(&id, act, area, index);
    sim.mario.lives = lives;
    if fall {
        sim.mario.pos[1] = if w[2] == "plane" { -6000.0 } else { sim.course.death_height + 30.0 };
        sim.mario.vel = [0.0, -60.0, 0.0];
        sim.mario.set_action(ACT_FREEFALL, 0.0);
    } else {
        sim.mario.health = 255.0;
    }
    let c0 = ctl(0.0, 0.0, 0, 0);
    let mut ask = None;
    for t in 1..=400u32 {
        step(&mut sim, &mut p, None, &c0);
        if matches!(p.flow, Flow::Closing { .. }) {
            ask = Some(t);
            break;
        }
    }
    let Some(ask) = ask else { return "none".into() };
    let fall_kind = matches!(&p.flow, Flow::Closing { kind: crate::progress::wipe::WipeKind::Circle, .. });
    let mut t = ask;
    let mut resolved = resolution(&p);
    let mut carry = None;
    loop {
        if let Some(sw) = &p.pending {
            carry = Some(sw.carry.is_some());
            resolved = load_string(sw);
            break;
        }
        if matches!(p.flow, Flow::GameOver { .. }) {
            resolved = "gameover".into();
            break;
        }
        t += 1;
        step(&mut sim, &mut p, None, &c0);
        if t > ask + 100 {
            break;
        }
    }
    format!("{resolved}|ask@{ask}|fall{}|load@{t}|carry{}:{}", u8::from(fall_kind), u8::from(carry.unwrap_or(false)), carry_string(&sim.mario))
}

fn type_dump(kind: i32) -> bool {
    kind == 0xff && std::env::var("WARP_DUMP").is_ok()
}

fn painting(name: &str, index: u32) -> String {
    // "painting area<A> type<hex> y<0|1>"
    let w: Vec<&str> = name.split(' ').collect();
    let area: i32 = w[1].trim_start_matches("area").parse().unwrap();
    let kind = i32::from_str_radix(w[2].trim_start_matches("type"), 16).unwrap();
    let odd = w[3] == "y1";
    let (mut sim, mut p) = start("castle_inside", 1, area, index);
    let s = sim
        .mario
        .world
        .surfaces()
        .iter()
        .find(|s| s.kind == kind)
        .cloned()
        .expect("painting surface");
    sim.mario.pos = [
        (s.v1[0] + s.v2[0] + s.v3[0]) / 3.0,
        s.v1[1].max(s.v2[1]).max(s.v3[1]) + if odd { 80.0 } else { 0.0 },
        (s.v1[2] + s.v2[2] + s.v3[2]) / 3.0,
    ];
    sim.mario.vel = [0.0; 3];
    sim.mario.forward_vel = 0.0;
    sim.mario.set_action(ACT_FREEFALL, 0.0);
    for f in 0..6 {
        step(&mut sim, &mut p, None, &ctl(0.0, 0.0, 0, 0));
        if type_dump(kind) {
            eprintln!("PT {area} {f} {:?} {:?} {} {:x}", sim.mario.pos.map(f64::round), sim.mario.floor.as_ref().map(|s| s.kind), sim.mario.floor_height.round(), sim.mario.action);
        }
        let r = resolution(&p);
        if r != "none" {
            return format!("{r}@{f}");
        }
    }
    "none@never".into()
}

fn force_floor_type(sim: &mut GameSim, kind: i32) {
    let m = &mut sim.mario;
    let hit = m.world.find_floor(m.pos[0], m.pos[1] + 100.0, m.pos[2]);
    let old = hit.surface.expect("floor under spawn");
    let (v1, v2, v3, room, force, flags) = (old.v1, old.v2, old.v3, old.room, old.force, old.flags);
    let target = std::sync::Arc::as_ptr(&old);
    let world = std::sync::Arc::make_mut(&mut m.world);
    world.remove_surfaces(&|s| std::ptr::eq(s, target));
    world.add_triangle(v1, v2, v3, &jumpman_sim::collision::SurfaceOptions { room, kind, force, flags, object: None });
}

fn floor_warp(kind: &str, index: u32) -> (String, Vec<u32>) {
    let (mut sim, mut p) = start(kind, 1, 1, index);
    let mut colliders = rig_up(&mut sim, &p, kind);
    let m = &mut sim.mario;
    m.health = 0x500 as f64;
    m.hurt_counter = 4.0;
    m.heal_counter = 8.0;
    if kind == "jrb" {
        force_floor_type(&mut sim, 0x32);
        sim.mario.set_action(ACT_FREEFALL, 0.0);
    } else {
        sim.mario.pos[1] = -6000.0;
        sim.mario.vel = [0.0; 3];
        sim.mario.forward_vel = 0.0;
        sim.mario.set_action(ACT_FREEFALL, 0.0);
    }
    let mut next = lcg(8 + index);
    let mut hasher = WorldHasher { cache: Default::default(), keep: vec![] };
    let mut frames = vec![];
    let mut value = "none".to_string();
    step(&mut sim, &mut p, Some(&mut colliders), &ctl(0.0, 0.0, 0, 0)); // settle the floor
    for f in 1..=400u32 {
        let c = ctl(if next() % 2 != 0 { 0.0 } else { 30.0 }, 0.0, 0, 0);
        step(&mut sim, &mut p, Some(&mut colliders), &c);
        frames.extend([mario_hash(&sim.mario), all_objects_hash(&sim.mario), hasher.hash(&sim.mario.world)]);
        // the TS loads at the start of the next tick, before Mario's step
        if let Some(sw) = &p.pending {
            value = format!("{}|tick{}|carry:{}", load_string(sw), f + 1, carry_string(&sim.mario));
            break;
        }
    }
    (value, frames)
}

fn frame_hashes(sim: &GameSim, hasher: &mut WorldHasher) -> [u32; 3] {
    [mario_hash(&sim.mario), all_objects_hash(&sim.mario), hasher.hash(&sim.mario.world)]
}

fn arrival(tuple: &Value, index: u32) -> Vec<u32> {
    let id = tuple["id"].as_str().unwrap();
    let req = request(id, tuple["act"].as_i64().unwrap() as i32, tuple["area"].as_i64().unwrap() as i32, tuple["entry"].as_str(), tuple["push"].as_bool().unwrap());
    let mut sim = load_req(req, index);
    let progress = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
    sim.mario.request_death_warp = None;
    let mut colliders = rig_up(&mut sim, &progress, id);
    let mut hasher = WorldHasher { cache: Default::default(), keep: vec![] };
    let mut next = lcg(500 + index);
    let mut prev_down = 0u32;
    let mut out = vec![];
    for f in 0..60u32 {
        let (mut down, mut sx, mut sy) = (0u32, 0.0, 0.0);
        if f > 20 {
            let mag = if next() % 3 == 0 { 0 } else { 64 };
            if mag != 0 {
                sx = (next() % 129) as f64 - 64.0;
                sy = (next() % 129) as f64 - 64.0;
            }
            if next() % 7 == 0 {
                down |= A_BUTTON;
            }
        }
        let c = ctl(sx, sy, down, prev_down);
        prev_down = down;
        sim.camera_yaw = 0;
        tick(&mut sim, &mut colliders, &c);
        out.extend(frame_hashes(&sim, &mut hasher));
    }
    out
}

/// The shift/swap block of `Game.stepWorld` (applied at the start of the next step in the TS, at the end of the tick
/// here), including the hot swap to the cached other area.
fn instant_area(id: &str, area: i32, index: u32) -> (Vec<u32>, Vec<f64>) {
    let (mut sim, mut p) = start(id, 1, area, index);
    sim.mario.request_death_warp = None;
    let mut colliders = rig_up(&mut sim, &p, id);
    let mut hasher = WorldHasher { cache: Default::default(), keep: vec![] };
    let mut out = vec![];
    let (mut shifts, mut swaps) = (0.0, 0.0);
    let surfaces_of = |p: &Progress, sim: &GameSim| -> Vec<std::sync::Arc<jumpman_sim::collision::Surface>> {
        sim.mario.world.surfaces().iter().filter(|s| s.kind >= 0x1b && p.data.instant_warps.contains_key(&(s.kind - 0x1b).to_string())).cloned().collect()
    };
    let mut surfaces = surfaces_of(&p, &sim);
    let mut pending_hash: Option<[u32; 3]> = None;
    let _ = &mut pending_hash;
    for f in 0..360u32 {
        if f % 60 == 0 && !surfaces.is_empty() {
            let s = &surfaces[((f / 60) as usize * 3) % surfaces.len()];
            let m = &mut sim.mario;
            m.pos = [(s.v1[0] + s.v2[0] + s.v3[0]) / 3.0, s.v1[1].max(s.v2[1]).max(s.v3[1]) + 60.0, (s.v1[2] + s.v2[2] + s.v3[2]) / 3.0];
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.set_action(ACT_FREEFALL, 0.0);
        }
        sim.camera_yaw = 0;
        tick(&mut sim, &mut colliders, &ctl(0.0, 0.0, 0, 0));
        let h = frame_hashes(&sim, &mut hasher);
        out.extend(h);
        out.push(sim.course.request.area as u32);
        out.push(shifts as u32);
        // end-of-tick shift (the TS applies it at the start of the next tick)
        let before = sim.mario.pos;
        let area_before = sim.course.request.area;
        p.post_tick(&mut sim, &ctl(0.0, 0.0, 0, 0));
        if let Some(ch) = p.pending_area.take() {
            p.change_area(&mut sim, &assets(), ch).unwrap();
            swaps += 1.0;
            surfaces = surfaces_of(&p, &sim);
            colliders = rig_up(&mut sim, &p, id);
        } else if sim.mario.pos != before && sim.course.request.area == area_before {
            shifts += 1.0;
        }
    }
    let _ = swaps;
    (out, vec![shifts])
}

#[test]
fn warps_match_typescript() {
    super::world::legacy_fixture_mode(true); // goldens predate the group 1 actors
    let fx = fixture();
    let only = std::env::var("WARPS_ONLY").ok();
    let wanted = |n: &str| only.as_ref().map_or(true, |o| n.contains(o.as_str()));
    let mut failures: Vec<String> = vec![];
    let (mut checked, mut skipped) = (0, 0);
    for r in fx["resolutions"].as_array().unwrap() {
        let name = r["name"].as_str().unwrap();
        if !wanted(name) {
            continue;
        }
        let want = r["value"].as_str().unwrap();
        let index = r["index"].as_u64().unwrap_or(0) as u32;
        let got = if name.starts_with("painting ") {
            Some(painting(name, index))
        } else if name.starts_with("floorwarp ") {
            Some(floor_warp(name.trim_start_matches("floorwarp "), index).0)
        } else if (name.starts_with("death ") || name.starts_with("fall ")) && !name.contains(".json") {
            Some(dynamic_death(name, index))
        } else if name.starts_with("floor ") || name == "totwc fall" {
            None // covered by the floorwarp scenarios
        } else {
            static_resolution(name)
        };
        match got {
            Some(g) if g == want => checked += 1,
            Some(g) => failures.push(format!("{name}: Rust {g}  vs TS {want}")),
            None => skipped += 1,
        }
    }
    for s in fx["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        if !wanted(name) {
            continue;
        }
        let index = s["index"].as_u64().unwrap() as u32;
        let want: Vec<u32> = super::longrun_tests::nums(&s["frames"]).into_iter().map(|v| v as u32).collect();
        let got = if name.starts_with("arrival ") {
            arrival(&s["tuple"], index)
        } else if name.starts_with("floorwarp ") {
            floor_warp(name.trim_start_matches("floorwarp "), index).1
        } else if name.starts_with("instant ") {
            let w: Vec<&str> = name.trim_start_matches("instant ").split('.').collect();
            instant_area(w[0], w[1].parse().unwrap(), index).0
        } else {
            panic!("unknown scenario {name}")
        };
        if got.len() != want.len() {
            failures.push(format!("{name}: {} hashes vs TS {}", got.len(), want.len()));
            continue;
        }
        let groups = s["groups"].as_u64().unwrap() as usize;
        if let Some(i) = got.iter().zip(&want).position(|(a, b)| a != b) {
            failures.push(format!("{name}: frame {} group {} differs ({:08x} vs TS {:08x})", i / groups, i % groups, got[i], want[i]));
        } else {
            checked += 1;
        }
    }
    let _ = (hash(&[]), nn(0.0));
    eprintln!("warps: {checked} matched, {skipped} covered elsewhere, {} failures", failures.len());
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
