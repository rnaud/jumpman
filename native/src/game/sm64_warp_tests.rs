//! The Super Mario 64 world graph (gap #5 of DECOMP_TALLY.md): every level of the decomp is exported or named
//! unplayable, every warp of `levels/*/script.c` (WARP_NODE objects, doors, pipes, fading warps, painting nodes, instant
//! warps, death / success / warp-floor targets) has a mechanism in the exported course JSON, every arrival point exists,
//! everything is reachable from the castle grounds, and scripted walks through the real engine (`GameSim` + `Progress`)
//! take the warps. `assets/courses/sm64_warp_report.json` is the decomp side (written by `scripts/sm64_world.py`).
use super::course_data::{read_course_value, set_extensions, CourseData};
use super::runner::GameSim;
use super::world::CourseRequest;
use crate::progress::flow::{Flow, Progress};
use crate::progress::save::SaveFile;
use jumpman_sim::state::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn report() -> Value {
    serde_json::from_slice(&std::fs::read(assets().join("courses/sm64_warp_report.json")).expect("run scripts/export-courses.py")).unwrap()
}

/// The course JSON with the world extensions merged, as the app loads it.
fn course(id: &str, area: i32) -> Value {
    set_extensions(true);
    read_course_value(&assets(), id, area).unwrap_or_else(|e| panic!("{id}.{area}: {e}"))
}

fn split(key: &str) -> (String, i32) {
    let (id, area) = key.rsplit_once('.').unwrap();
    (id.to_string(), area.parse().unwrap())
}

fn pos(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap().round() as i64).collect()
}

fn has_entry(c: &Value, node: &str) -> bool {
    c["warpSpawns"].get(node).is_some() || c["doors"].as_array().is_some_and(|d| d.iter().any(|d| d["node"] == node))
}

#[test]
fn every_playable_level_of_the_decomp_is_exported() {
    let rep = report();
    let exported: BTreeSet<String> = rep["exported"].as_array().unwrap().iter().map(|k| split(k.as_str().unwrap()).0).collect();
    let unplayable: BTreeSet<String> = rep["unplayable"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let in_decomp: BTreeSet<String> = rep["levelsInDecomp"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let playable: BTreeSet<String> = in_decomp.difference(&unplayable).cloned().collect();
    assert_eq!(playable.len(), 30, "the decomp has 30 playable level scripts (33 minus intro, menu, ending): {playable:?}");
    assert_eq!(playable, exported, "levels missing from the export");
    // Every area of every level is exported too.
    let areas = rep["levelAreasInDecomp"].as_object().unwrap();
    for (level, list) in areas {
        for a in list.as_array().unwrap() {
            let key = format!("{level}.{}", a);
            assert!(rep["exported"].as_array().unwrap().iter().any(|k| k == &Value::String(key.clone())), "{key} not exported");
            assert!(CourseData::path(&assets(), level, a.as_i64().unwrap() as i32).is_file(), "{key} json missing");
        }
    }
    // ... and every exported course is in the catalogue (the LEVELS tab).
    for id in &exported {
        assert!(super::courses::info(id).is_some(), "{id} missing from courses_table.rs");
    }
}

#[test]
fn every_decomp_warp_has_a_mechanism_in_the_export() {
    let rep = report();
    let mut missing = vec![];
    let mut checked = 0;
    for (key, area) in rep["areas"].as_object().unwrap() {
        let (id, a) = split(key);
        let c = course(&id, a);
        for t in area["triggers"].as_array().unwrap() {
            let (to, to_area, to_node) = (t["to"].as_str().unwrap(), t["toArea"].as_i64().unwrap(), t["toNode"].as_str().unwrap());
            let node = t["node"].as_str().unwrap();
            let ok = match t["kind"].as_str().unwrap() {
                "warp" | "pipe" | "cage" => c["warps"].as_array().is_some_and(|w| {
                    w.iter().any(|w| pos(&w["pos"]) == pos(&t["pos"]) && w["warpTo"] == to && w["warpEntry"] == to_node && w["warpArea"].as_i64().map_or(to_area == 1, |x| x == to_area))
                }),
                "door" => c["doors"].as_array().is_some_and(|d| d.iter().any(|d| d["node"] == node && d["destination"] == to && d["destNode"] == to_node && d["destArea"] == to_area)),
                "fading" => c["fadingWarps"].as_array().is_some_and(|f| f.iter().any(|f| pos(&f["pos"]) == pos(&t["pos"]))),
                "instant" => c["instantWarps"].as_object().is_some_and(|m| m.values().any(|w| w["area"] == to_area)),
                "painting" => {
                    let idx = i64::from_str_radix(&node[node.len() - 2..], 16).unwrap().to_string();
                    let want = if to_area == 1 { to.to_string() } else { format!("{to}:{to_area}") };
                    c["paintings"].get(&idx).is_some_and(|d| d == &Value::String(want))
                }
                other => panic!("unknown trigger kind {other}"),
            };
            checked += 1;
            if !ok {
                missing.push(format!("{key}: {} {node} at {:?} -> {to}.{to_area} {to_node}", t["kind"].as_str().unwrap(), pos(&t["pos"])));
            }
        }
    }
    assert!(checked > 100, "{checked} decomp triggers");
    assert!(missing.is_empty(), "{} decomp warps without a mechanism:\n{}", missing.len(), missing.join("\n"));
}

#[test]
fn every_arrival_exists_and_every_special_target_is_wired() {
    let rep = report();
    let mut bad = vec![];
    for (key, area) in rep["areas"].as_object().unwrap() {
        let (id, a) = split(key);
        let c = course(&id, a);
        for (node, bhv) in area["arrivals"].as_object().unwrap() {
            if !has_entry(&c, node) {
                bad.push(format!("{key}: arrival {node} ({bhv}) missing from warpSpawns"));
            }
        }
        // Targets of triggers land on an arrival of the target area.
        for t in area["triggers"].as_array().unwrap() {
            if t["kind"] == "instant" || t["kind"] == "painting" && id != "castle_inside" {
                continue;
            }
            let (to, to_area, to_node) = (t["to"].as_str().unwrap(), t["toArea"].as_i64().unwrap() as i32, t["toNode"].as_str().unwrap());
            if !has_entry(&course(to, to_area), to_node) {
                bad.push(format!("{key} -> {to}.{to_area}: no arrival {to_node}"));
            }
        }
        // death / success / warp-floor
        let sp = &area["specials"];
        if let Some(d) = sp.get("death") {
            let w = &c["deathWarp"];
            if w["course"] != d["to"] || w["area"] != d["toArea"] || w["entry"] != d["toNode"] {
                bad.push(format!("{key}: deathWarp {w} != decomp {d}"));
            }
            if !has_entry(&course(d["to"].as_str().unwrap(), d["toArea"].as_i64().unwrap() as i32), d["toNode"].as_str().unwrap()) {
                bad.push(format!("{key}: death target {d} has no arrival"));
            }
        }
        if let Some(d) = sp.get("success") {
            let to = c["successCourse"].as_str().unwrap_or(if c["successEntry"].is_null() { "castle_grounds" } else { "castle_inside" });
            if to != d["to"] || c["successEntry"] != d["toNode"] && !(d["to"] == "castle_grounds" && c["successEntry"].is_null()) || (!c["successArea"].is_null() && c["successArea"] != d["toArea"]) {
                bad.push(format!("{key}: success exit {to}/{}/{} != decomp {d}", c["successEntry"], c["successArea"]));
            }
        }
        if let Some(d) = sp.get("floor") {
            let ok = (c["fallWarp"]["course"] == d["to"] && c["fallWarp"]["entry"] == d["toNode"]) || (id == "jrb" && c["floorWarp"]["entry"] == d["toNode"]);
            if !ok {
                bad.push(format!("{key}: warp floor {} / {} != decomp {d}", c["fallWarp"], c["floorWarp"]));
            }
        }
    }
    assert!(bad.is_empty(), "{} problems:\n{}", bad.len(), bad.join("\n"));
}

/// course key -> reachable course keys over everything Mario can walk through (warps, doors, paintings, fading
/// warps, instant warps) plus the exits that bring him back (star, death, warp floor).
fn graph() -> BTreeMap<String, BTreeSet<String>> {
    let rep = report();
    let mut g: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for key in rep["exported"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        let (id, a) = split(key);
        let c = course(&id, a);
        let edges = g.entry(key.to_string()).or_default();
        let mut add = |to: &str, area: i64| {
            edges.insert(format!("{to}.{area}"));
        };
        for w in c["warps"].as_array().into_iter().flatten() {
            add(w["warpTo"].as_str().unwrap(), w["warpArea"].as_i64().unwrap_or(1));
        }
        for d in c["doors"].as_array().into_iter().flatten() {
            add(d["destination"].as_str().unwrap(), d["destArea"].as_i64().unwrap_or(1));
        }
        for p in c["paintings"].as_object().into_iter().flat_map(|m| m.values()) {
            let p = p.as_str().unwrap();
            match p.split_once(':') {
                Some((course, area)) => add(course, area.parse().unwrap()),
                None => add(p, 1),
            }
        }
        for w in c["instantWarps"].as_object().into_iter().flat_map(|m| m.values()) {
            add(&id, w["area"].as_i64().unwrap());
        }
        for f in ["deathWarp", "fallWarp", "floorWarp"] {
            if let Some(w) = c.get(f).filter(|w| !w.is_null()) {
                add(w["course"].as_str().unwrap_or(&id), w["area"].as_i64().unwrap_or(1));
            }
        }
        if let Some(entry) = c["successEntry"].as_str() {
            let _ = entry;
            add(c["successCourse"].as_str().unwrap_or("castle_inside"), c["successArea"].as_i64().unwrap_or(1));
        } else if c["deathWarp"].is_object() {
            add(c["successCourse"].as_str().unwrap_or("castle_grounds"), 1);
        }
    }
    g
}

/// Edges the engine adds on top of the exported JSON, each with its decomp source.
const ENGINE_EDGES: &[(&str, &str, &str)] = &[
    // `ACT_..` tower: looking up on the castle's tower floor with 10 stars fades into the Tower of the Wing Cap
    // (`Progress::step_tower`; decomp `WARP_NODE_TOTWC`, castle_inside/script.c:51).
    ("castle_inside.1", "totwc.1", "castle_inside/script.c WARP_NODE_TOTWC"),
    // The painting-less courses are entered from the course list of the LEVELS tab: not a walkable edge.
];

fn reachable(from: &str) -> BTreeSet<String> {
    let mut g = graph();
    for (a, b, _) in ENGINE_EDGES {
        g.entry(a.to_string()).or_default().insert(b.to_string());
    }
    let mut seen = BTreeSet::from([from.to_string()]);
    let mut queue = VecDeque::from([from.to_string()]);
    while let Some(a) = queue.pop_front() {
        for b in g.get(&a).into_iter().flatten() {
            if seen.insert(b.clone()) {
                queue.push_back(b.clone());
            }
        }
    }
    seen
}

#[test]
fn everything_is_reachable_from_the_castle_grounds() {
    let rep = report();
    let all: BTreeSet<String> = rep["exported"].as_array().unwrap().iter().map(|k| k.as_str().unwrap().to_string()).collect();
    let seen = reachable("castle_grounds.1");
    let missing: Vec<_> = all.difference(&seen).collect();
    assert!(missing.is_empty(), "not reachable from castle_grounds: {missing:?}");
}

#[test]
fn every_area_can_get_back_to_the_castle_grounds() {
    // Round trips: from every area some chain of exits (door, pipe, star, death) leads back to the grounds.
    let g = graph();
    let all: Vec<String> = g.keys().cloned().collect();
    let mut stuck = vec![];
    for k in &all {
        if !reachable(k).contains("castle_grounds.1") {
            stuck.push(k.clone());
        }
    }
    assert!(stuck.is_empty(), "no way back to the grounds from {stuck:?}");
}

#[test]
fn doors_and_pipes_come_in_pairs() {
    // A door / pipe / cage A -> B (not a death, star or painting exit) has an edge B -> A, except these one-way ones
    // (decomp reason in the comment).
    let one_way: BTreeSet<(&str, &str)> = BTreeSet::from([
        // Pipes into the Bowser levels end in the arena: the way out is the death warp (bitdw 0C) or the star exit.
        ("bitdw.1", "bowser_1.1"),
        ("bitfs.1", "bowser_2.1"),
        ("bits.1", "bowser_3.1"),
        // `WARP_NODE_0B` of LLL: the volcano interior has no warp out; stars and death lead back to the castle.
        ("lll.1", "lll.2"),
        // The courtyard cage opens BBH; BBH's own exits (star, death) bring Mario back to the courtyard.
        ("castle_courtyard.1", "bbh.1"),
        // The castle's pipe / hole entrances to the small levels: Mario leaves them through their exits.
        ("castle_inside.1", "pss.1"),
        ("castle_inside.1", "sa.1"),
        ("castle_inside.1", "bitdw.1"),
        ("castle_inside.2", "bits.1"),
        ("castle_inside.2", "wmotr.1"),
        ("castle_inside.3", "bitfs.1"),
        ("castle_grounds.1", "vcutm.1"),
        ("hmc.1", "cotmc.1"),
        // SSL's pyramid interior (area 2) lists no warp out in the decomp (levels/ssl/script.c): its exits are the stars
        // and death.
        ("ssl.1", "ssl.2"),
    ]);
    let mut bad = vec![];
    for key in report()["exported"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        let (id, a) = split(key);
        let c = course(&id, a);
        let mut out: Vec<(String, i64)> = vec![];
        for w in c["warps"].as_array().into_iter().flatten() {
            out.push((w["warpTo"].as_str().unwrap().to_string(), w["warpArea"].as_i64().unwrap_or(1)));
        }
        for d in c["doors"].as_array().into_iter().flatten() {
            out.push((d["destination"].as_str().unwrap().to_string(), d["destArea"].as_i64().unwrap_or(1)));
        }
        for (to, to_area) in out {
            let target = format!("{to}.{to_area}");
            if target == key || one_way.contains(&(key, target.as_str())) {
                continue;
            }
            let back = course(&to, to_area as i32);
            let returns = back["warps"].as_array().into_iter().flatten().any(|w| w["warpTo"] == id && w["warpArea"].as_i64().unwrap_or(1) == a as i64)
                || back["doors"].as_array().into_iter().flatten().any(|d| d["destination"] == id && d["destArea"].as_i64().unwrap_or(1) == a as i64);
            if !returns {
                bad.push(format!("{key} -> {target} has no way back"));
            }
        }
    }
    assert!(bad.is_empty(), "one-way connections:\n{}", bad.join("\n"));
}

// ------------------------------------------------------------------------------------------- engine walks

fn start(id: &str, area: i32, entry: Option<&str>) -> (GameSim, Progress) {
    set_extensions(true);
    let mut req = CourseRequest::new(id);
    req.area = area;
    req.entry = entry.map(str::to_string);
    let mut sim = GameSim::load(&assets(), req).unwrap_or_else(|e| panic!("{id}.{area}: {e}"));
    let p = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
    (sim, p)
}

fn idle() -> ControllerSnapshot {
    ControllerSnapshot::default()
}

fn frame(sim: &mut GameSim, p: &mut Progress, c: &ControllerSnapshot) {
    p.pre_tick(sim);
    if p.sim_running() {
        sim.tick(c);
    }
    p.post_tick(sim, c);
}

/// Drop Mario on a warp's trigger and run until the progression layer queues a course change.
fn step_on_warp(id: &str, area: i32, warp: &Value, stars: u32) -> Option<(String, i32, Option<String>)> {
    let (mut sim, mut p) = start(id, area, None);
    for i in 0..stars {
        p.save.collect_star("bob", (i % 6) as i32 + 1 + 10 * (i as i32 / 6), 0);
    }
    let w = pos(&warp["pos"]);
    sim.mario.pos = [w[0] as f64, w[1] as f64, w[2] as f64];
    sim.mario.vel = [0.0; 3];
    sim.mario.set_action(ACT_IDLE, 0.0);
    sim.mario.health = 0x880 as f64;
    for _ in 0..30 {
        // keep Mario on the trigger (water, gravity)
        sim.mario.pos = [w[0] as f64, w[1] as f64, w[2] as f64];
        frame(&mut sim, &mut p, &idle());
        if let Some(sw) = &p.pending {
            return Some((sw.request.id.clone(), sw.request.area, sw.request.entry.clone()));
        }
        if let Some(ch) = &p.pending_area {
            return Some((id.to_string(), ch.area, ch.entry.clone()));
        }
        if let Flow::ActSelect(sel) = &p.flow {
            return Some((sel.course.clone(), sel.area, sel.entry.clone()));
        }
    }
    None
}

#[test]
fn stepping_on_every_exported_warp_takes_it() {
    let rep = report();
    let mut bad = vec![];
    let mut taken = 0;
    for (key, _) in rep["areas"].as_object().unwrap() {
        let (id, a) = split(key);
        let c = course(&id, a);
        for w in c["warps"].as_array().into_iter().flatten() {
            let to = w["warpTo"].as_str().unwrap();
            let to_area = w["warpArea"].as_i64().unwrap_or(1) as i32;
            let gate = w["minStars"].as_u64().unwrap_or(0) as u32;
            if gate > 0 {
                let (mut sim, mut p) = start(&id, a, None);
                let at = pos(&w["pos"]);
                sim.mario.pos = [at[0] as f64, at[1] as f64, at[2] as f64];
                for _ in 0..10 {
                    frame(&mut sim, &mut p, &idle());
                }
                if p.pending.is_some() {
                    bad.push(format!("{key}: gated warp to {to} opened without {gate} stars"));
                }
            }
            match step_on_warp(&id, a, w, gate) {
                Some((course_id, area, entry)) => {
                    taken += 1;
                    // score courses open the act selector with the course id, everything else loads directly
                    if course_id != to || area != to_area {
                        bad.push(format!("{key}: warp to {to}.{to_area} loaded {course_id}.{area}"));
                    }
                    if entry.as_deref() != w["warpEntry"].as_str() {
                        bad.push(format!("{key}: warp to {to} arrives at {entry:?}, decomp {}", w["warpEntry"]));
                    }
                }
                None => bad.push(format!("{key}: standing on the warp to {to}.{to_area} at {:?} does nothing", pos(&w["pos"]))),
            }
        }
    }
    assert!(taken >= 20, "{taken} warps taken");
    assert!(bad.is_empty(), "{} warps:\n{}", bad.len(), bad.join("\n"));
}

/// Mario standing on an instant-warp floor (`SURFACE_INSTANT_WARP_1B..1E`) of the pyramid, downtown WDW, the TTM slide ...
/// swaps to the destination area keeping his place (`INSTANT_WARP` displacement); the swap works both ways.
#[test]
fn instant_warp_floors_swap_areas() {
    let rep = report();
    let mut swaps = 0;
    let mut bad = vec![];
    for (key, _) in rep["areas"].as_object().unwrap() {
        let (id, a) = split(key);
        let c = course(&id, a);
        for (index, w) in c["instantWarps"].as_object().into_iter().flatten() {
            let target = w["area"].as_i64().unwrap() as i32;
            if target == a {
                continue; // same-area shifts (ssl, castle upstairs) are position jumps
            }
            let kind = 0x1b + index.parse::<i64>().unwrap();
            let verts = c["vertices"].as_array().unwrap();
            let tri = c["triangles"].as_array().unwrap().iter().find(|t| t[3].as_i64() == Some(kind));
            let Some(t) = tri else {
                bad.push(format!("{key}: no floor of type {kind:#x} for instant warp {index}"));
                continue;
            };
            let mut at = [0.0f64; 3];
            for i in 0..3 {
                let v = &verts[t[i].as_u64().unwrap() as usize];
                for k in 0..3 {
                    at[k] += v[k].as_f64().unwrap() / 3.0;
                }
            }
            let (mut sim, mut p) = start(&id, a, None);
            sim.mario.pos = [at[0], at[1] + 5.0, at[2]];
            sim.mario.vel = [0.0; 3];
            sim.mario.set_action(ACT_IDLE, 0.0);
            let mut got = None;
            for _ in 0..8 {
                sim.mario.pos = [at[0], at[1] + 5.0, at[2]];
                frame(&mut sim, &mut p, &idle());
                if let Some(ch) = p.pending_area.take() {
                    got = Some(ch);
                    break;
                }
            }
            match got {
                Some(ch) if ch.area == target => {
                    p.change_area(&mut sim, &assets(), ch).unwrap();
                    swaps += 1;
                    if sim.course.request.area != target {
                        bad.push(format!("{key}: swapped to area {} instead of {target}", sim.course.request.area));
                    }
                }
                other => bad.push(format!("{key}: standing on the {kind:#x} floor gave {other:?}, wanted area {target}")),
            }
        }
    }
    assert!(swaps >= 3, "{swaps} swaps");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
