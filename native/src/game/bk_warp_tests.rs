//! Banjo-Kazooie doors, pipes, windows and the lair's world doors (`scripts/export-bk-setup.py`): the exit graph built
//! from the exported course JSON, and scripted walks through the real engine (`GameSim` + `Progress`: warp trigger,
//! iris wipe, course switch, arrival pose).
use super::bk_tests::worlds;
use super::course_data::CourseData;
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

fn course(id: &str) -> Value {
    serde_json::from_slice(&std::fs::read(CourseData::path(&assets(), id, 1)).unwrap_or_else(|e| panic!("{id}: {e}"))).unwrap()
}

fn report() -> Value {
    serde_json::from_slice(&std::fs::read(assets().join("courses/bk_warp_report.json")).expect("run scripts/export-bk-setup.py")).unwrap()
}

/// course id -> (warp target, entry) for every warp of the exported JSON.
fn graph() -> BTreeMap<String, Vec<(String, String)>> {
    worlds()
        .into_iter()
        .map(|id| {
            let c = course(id);
            let edges = c["warps"].as_array().map(Vec::as_slice).unwrap_or(&[]).iter().map(|w| (w["warpTo"].as_str().unwrap().to_string(), w["warpEntry"].as_str().unwrap().to_string())).collect();
            (id.to_string(), edges)
        })
        .collect()
}

#[test]
fn every_exit_targets_an_exported_map_with_a_matching_entry() {
    let g = graph();
    let mut warps = 0;
    for (id, edges) in &g {
        let spawns = course(id)["warpSpawns"].clone();
        for (to, entry) in edges {
            warps += 1;
            assert!(g.contains_key(to), "{id}: warp to unknown course {to}");
            let target = course(to);
            assert!(target["warpSpawns"][entry].is_object(), "{id} -> {to}: no entry {entry}");
        }
        let _ = spawns;
    }
    assert!(warps > 250, "{warps} warps");
    // Every warp the exporter dropped carries a reason (handlers that are not map changes, cutscene targets, hard-coded exits).
    let rep = report();
    let mut skipped = 0;
    for (id, c) in rep["courses"].as_object().unwrap() {
        for s in c["skippedWarps"].as_array().unwrap() {
            skipped += 1;
            assert!(s["reason"].as_str().is_some_and(|r| r.len() > 10), "{id}: skipped warp without a reason: {s}");
        }
    }
    assert!(skipped > 0);
}

#[test]
fn every_map_is_reachable_from_spiral_mountain_except_the_listed_script_only_ones() {
    let g = graph();
    let mut seen = BTreeSet::from(["bk_spiral_mountain".to_string()]);
    let mut queue = VecDeque::from(["bk_spiral_mountain".to_string()]);
    while let Some(a) = queue.pop_front() {
        for (b, _) in &g[&a] {
            if seen.insert(b.clone()) {
                queue.push_back(b.clone());
            }
        }
    }
    let listed: BTreeSet<String> = report()["unreachable"].as_object().unwrap().keys().cloned().collect();
    let missing: BTreeSet<String> = g.keys().filter(|k| !seen.contains(*k)).cloned().collect();
    assert_eq!(missing, listed, "unreachable maps differ from the explicit skip list in bk_warp_report.json");
    for (id, reason) in report()["unreachable"].as_object().unwrap() {
        assert!(reason.as_str().unwrap().len() > 10, "{id}");
    }
    assert!(seen.len() >= 100, "{} reachable maps", seen.len());
}

#[test]
fn round_trips_are_consistent() {
    // A -> B through an exit lands on B's matching entry (checked above); B has a way back to A.
    let g = graph();
    let mut one_way = vec![];
    for (a, edges) in &g {
        for (b, _) in edges {
            if a != b && !g[b].iter().any(|(t, _)| t == a) {
                one_way.push(format!("{a} -> {b}"));
            }
        }
    }
    assert!(one_way.is_empty(), "one-way connections: {one_way:?}");
}

#[test]
fn arrival_points_have_a_floor_and_are_outside_every_trigger() {
    let mut bad = vec![];
    for id in worlds() {
        let c = course(id);
        let sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap();
        for (entry, sp) in c["warpSpawns"].as_object().unwrap() {
            let p: Vec<f64> = sp["pos"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            let floor = sim.mario.world.find_floor(p[0], p[1] + 150.0, p[2]);
            if floor.surface.is_none() {
                bad.push(format!("{id} {entry}: no floor at {p:?}"));
                continue;
            }
            if (floor.height - p[1]).abs() > 300.0 && floor.height < p[1] - 300.0 {
                // far above the floor (flooded rooms, shafts): fine, Mario falls or swims; only report absurd heights
                if p[1] - floor.height > 3000.0 {
                    bad.push(format!("{id} {entry}: {} units above the floor", p[1] - floor.height));
                }
            }
            for w in c["warps"].as_array().unwrap() {
                let wp: Vec<f64> = w["pos"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
                let param = w["param"].as_f64().unwrap();
                let (r, h) = ((param % 1000.0) * 10.0 + 37.0, (param / 1000.0).floor() * 10.0);
                let dxz2 = (p[0] - wp[0]).powi(2) + (p[2] - wp[2]).powi(2);
                if dxz2 < r * r && p[1] >= wp[1] - 160.0 && p[1] <= wp[1] + h {
                    bad.push(format!("{id} {entry}: arrival inside the trigger of the warp to {}", w["warpTo"]));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} problems:\n{}", bad.len(), bad.join("\n"));
}

// ------------------------------------------------------------------------------------------------
// Scripted walks through the engine

fn frame(sim: &mut GameSim, p: &mut Progress, c: &ControllerSnapshot) {
    p.pre_tick(sim);
    if p.sim_running() {
        sim.tick(c);
    }
    p.post_tick(sim, c);
}

/// The arrival point of `from` closest to one of its doors to `to` (`""`: any door).
fn nearest_entry(from: &str, to: &str) -> Option<String> {
    let c = course(from);
    let mut best: Option<(f64, String)> = None;
    for w in c["warps"].as_array().unwrap().iter().filter(|w| to.is_empty() || w["warpTo"] == to) {
        for (name, sp) in c["warpSpawns"].as_object().unwrap() {
            let d = (w["pos"][0].as_f64().unwrap() - sp["pos"][0].as_f64().unwrap()).powi(2) + (w["pos"][2].as_f64().unwrap() - sp["pos"][2].as_f64().unwrap()).powi(2);
            if best.as_ref().map_or(true, |b| d < b.0) {
                best = Some((d, name.clone()));
            }
        }
    }
    best.map(|b| b.1)
}

struct Rig {
    sim: GameSim,
    p: Progress,
    seen: Vec<String>,
    wipes: u32,
}

impl Rig {
    fn new(id: &str, entry: Option<&str>, banjo: bool) -> Self {
        let mut req = CourseRequest::new(id);
        req.entry = entry.map(str::to_string).or_else(|| nearest_entry(id, ""));
        let mut sim = GameSim::load(&assets(), req).unwrap_or_else(|e| panic!("{id}: {e}"));
        sim.as_banjo = banjo;
        let p = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
        let mut rig = Rig { sim, p, seen: vec![id.to_string()], wipes: 0 };
        for _ in 0..40 {
            rig.step(&ControllerSnapshot::default());
        }
        rig
    }

    fn step(&mut self, c: &ControllerSnapshot) {
        frame(&mut self.sim, &mut self.p, c);
        if matches!(self.p.flow, Flow::Closing { .. }) {
            self.wipes += 1;
        }
        if self.p.pending.is_some() {
            let banjo = self.sim.as_banjo;
            self.p.perform_switch(&mut self.sim, &assets()).unwrap();
            self.sim.as_banjo = banjo;
            self.seen.push(self.sim.course.request.id.clone());
            // let the arrival settle, standing still
            for _ in 0..40 {
                frame(&mut self.sim, &mut self.p, &ControllerSnapshot::default());
            }
        }
    }

    fn id(&self) -> &str {
        &self.sim.course.request.id
    }

    /// Walk straight at the nearest trigger of a warp to `to` until the course changes (or give up after 6 seconds).
    fn walk_through(&mut self, to: &str) {
        let from = self.id().to_string();
        let c = course(&from);
        let here = self.sim.mario.pos;
        let dist = |w: &Value| (w["pos"][0].as_f64().unwrap() - here[0]).powi(2) + (w["pos"][2].as_f64().unwrap() - here[2]).powi(2);
        let w = c["warps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w["warpTo"] == to)
            .min_by(|a, b| dist(a).total_cmp(&dist(b)))
            .unwrap_or_else(|| panic!("{from} has no warp to {to}"));
        let target = [w["pos"][0].as_f64().unwrap(), w["pos"][2].as_f64().unwrap()];
        for _ in 0..180 {
            let m = &self.sim.mario;
            let (dx, dz) = (target[0] - m.pos[0], target[1] - m.pos[2]);
            let dir = dx.atan2(dz) * 32768.0 / std::f64::consts::PI;
            // stick "up" walks along camera_yaw + 0x8000
            self.sim.camera_yaw = jumpman_sim::math::to_s16(dir + 32768.0) as i32;
            let stick = ControllerSnapshot { stick_y: 64.0, stick_mag: 64.0, ..Default::default() };
            self.step(&stick);
            if self.id() != from {
                return;
            }
        }
        let m = &self.sim.mario;
        panic!("{from}: never reached the warp to {to}: Mario at {:?}, trigger {:?}, flow {:?}", m.pos, w["pos"], self.p.flow);
    }

    /// Put Mario on the nearest trigger of a warp to `to` (windows and pipes of the interiors need real climbing and jumping, which
    /// the straight-walk helper cannot do) and let the engine fire it.
    fn step_onto(&mut self, to: &str) {
        let from = self.id().to_string();
        let c = course(&from);
        let here = self.sim.mario.pos;
        let w = c["warps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w["warpTo"] == to)
            .min_by(|a, b| {
                let d = |w: &Value| (w["pos"][0].as_f64().unwrap() - here[0]).powi(2) + (w["pos"][2].as_f64().unwrap() - here[2]).powi(2);
                d(a).total_cmp(&d(b))
            })
            .unwrap_or_else(|| panic!("{from} has no warp to {to}"));
        self.sim.mario.pos = [w["pos"][0].as_f64().unwrap(), w["pos"][1].as_f64().unwrap() + 40.0, w["pos"][2].as_f64().unwrap()];
        for _ in 0..30 {
            self.step(&ControllerSnapshot::default());
            if self.id() != from {
                return;
            }
        }
        panic!("{from}: standing in the trigger of the warp to {to} did nothing (Mario at {:?})", self.sim.mario.pos);
    }

    fn standing(&self) -> bool {
        let m = &self.sim.mario;
        m.health > 0.0 && m.pos[1] > self.sim.course.death_height && (m.floor.is_some() || m.action & ACT_FLAG_SWIMMING != 0)
    }
}

/// The scripted chain: out the door of one map, check the arrival pose against the target's `warpSpawns`, and back.
fn round_trip(a: &str, entry: Option<&str>, b: &str, banjo: bool) {
    round_trip_with(a, entry, b, banjo, false)
}

fn round_trip_with(a: &str, entry: Option<&str>, b: &str, banjo: bool, return_by_stepping: bool) {
    let mut rig = Rig::new(a, entry, banjo);
    rig.walk_through(b);
    assert_eq!(rig.id(), b, "{:?}", rig.seen);
    assert!(rig.wipes > 0, "the iris wipe ran");
    assert!(rig.standing(), "{b}: Mario does not stand on arrival: {:?} action {:#x}", rig.sim.mario.pos, rig.sim.mario.action);
    // arrival pose: within the nudge of one of the target's entries
    let spawns = course(b)["warpSpawns"].clone();
    let m = rig.sim.mario.pos;
    let near = spawns.as_object().unwrap().values().any(|s| {
        let p = &s["pos"];
        ((p[0].as_f64().unwrap() - m[0]).powi(2) + (p[2].as_f64().unwrap() - m[2]).powi(2)).sqrt() < 120.0
    });
    assert!(near, "{b}: arrived at {m:?}, not at an exit {spawns}");
    // and back through the door behind
    if return_by_stepping {
        rig.step_onto(a);
    } else {
        rig.walk_through(a);
    }
    assert_eq!(rig.id(), a, "{:?}", rig.seen);
    assert!(rig.standing());
}

#[test]
fn walk_spiral_mountain_into_the_lair_and_back() {
    round_trip("bk_spiral_mountain", Some("bk_exit_19"), "bk_gl_mm_lobby", false);
}

#[test]
fn walk_the_lair_into_mumbos_mountain_and_back_to_the_lobby() {
    round_trip("bk_gl_mm_lobby", Some("bk_exit_2"), "bk_mumbos_mountain", false);
}

#[test]
fn walk_rusty_bucket_bay_deck_into_the_kitchen_and_back() {
    // the ship's rooms are entered through windows and pipes: walk in through the nearest door, step back onto the trigger
    round_trip_with("bk_rusty_bucket_bay", nearest_entry("bk_rusty_bucket_bay", "bk_rbb_kitchen").as_deref(), "bk_rbb_kitchen", false, true);
}

#[test]
fn walk_mad_monster_mansion_door_and_back() {
    round_trip("bk_mad_monster_mansion", nearest_entry("bk_mad_monster_mansion", "bk_mmm_church").as_deref(), "bk_mmm_church", false);
}

#[test]
fn banjo_uses_the_doors_too() {
    round_trip("bk_spiral_mountain", Some("bk_exit_19"), "bk_gl_mm_lobby", true);
}

#[test]
fn link_uses_the_doors_too() {
    // Link and Young Link run on the same Mario runtime with `as_link`: the warp objects do not care who walks in.
    let mut rig = Rig::new("bk_spiral_mountain", Some("bk_exit_19"), false);
    rig.sim.as_link = true;
    rig.walk_through("bk_gl_mm_lobby");
    assert_eq!(rig.id(), "bk_gl_mm_lobby");
    assert!(rig.standing());
}

#[test]
fn three_doors_in_a_row() {
    // Spiral Mountain -> lair entrance hall -> Mumbo's Mountain (through the hall's door), then back into the hall.
    let mut rig = Rig::new("bk_spiral_mountain", Some("bk_exit_19"), false);
    rig.walk_through("bk_gl_mm_lobby");
    // the hall's world door: stand where the pause-menu exit puts the player, in front of it
    let spawn = course("bk_gl_mm_lobby")["warpSpawns"]["bk_exit_2"].clone();
    rig.sim.mario.pos = [spawn["pos"][0].as_f64().unwrap(), spawn["pos"][1].as_f64().unwrap(), spawn["pos"][2].as_f64().unwrap()];
    for _ in 0..20 {
        rig.step(&ControllerSnapshot::default());
    }
    rig.walk_through("bk_mumbos_mountain");
    rig.walk_through("bk_gl_mm_lobby");
    assert_eq!(rig.seen, ["bk_spiral_mountain", "bk_gl_mm_lobby", "bk_mumbos_mountain", "bk_gl_mm_lobby"]);
}

/// Coverage probe (`cargo test --release --lib bk_door_coverage -- --ignored --nocapture`): for every warp of every map,
/// arrive at the nearest entry of its own map and walk straight at the trigger. Prints how many doors a plain walk reaches.
#[test]
#[ignore]
fn bk_door_coverage() {
    let (mut ok, mut total) = (0, 0);
    for id in worlds() {
        let c = course(id);
        for (i, w) in c["warps"].as_array().unwrap().iter().enumerate() {
            total += 1;
            let to = w["warpTo"].as_str().unwrap();
            // nearest arrival point to this very warp
            let mut best: Option<(f64, String)> = None;
            for (name, sp) in c["warpSpawns"].as_object().unwrap() {
                let d = (w["pos"][0].as_f64().unwrap() - sp["pos"][0].as_f64().unwrap()).powi(2) + (w["pos"][2].as_f64().unwrap() - sp["pos"][2].as_f64().unwrap()).powi(2);
                if best.as_ref().map_or(true, |b| d < b.0) {
                    best = Some((d, name.clone()));
                }
            }
            let Some((_, entry)) = best else { continue };
            let mut rig = Rig::new(id, Some(&entry), false);
            let target = [w["pos"][0].as_f64().unwrap(), w["pos"][2].as_f64().unwrap()];
            let mut hit = false;
            for _ in 0..240 {
                let m = &rig.sim.mario;
                let dir = (target[0] - m.pos[0]).atan2(target[1] - m.pos[2]) * 32768.0 / std::f64::consts::PI;
                rig.sim.camera_yaw = jumpman_sim::math::to_s16(dir + 32768.0) as i32;
                rig.step(&ControllerSnapshot { stick_y: 64.0, stick_mag: 64.0, ..Default::default() });
                if rig.id() != id {
                    hit = true;
                    break;
                }
            }
            if hit {
                ok += 1;
            } else {
                println!("MISS {id} warp #{i} -> {to} ({}) from {entry}, Mario ended at {:?}", w["fn"], rig.sim.mario.pos);
            }
        }
    }
    println!("door coverage: {ok}/{total} warps reached by a straight walk from the nearest arrival point");
}
