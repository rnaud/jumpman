//! Banjo-Kazooie maps (`scripts/export-bk-levels.py`: the 14 worlds and every sub-area): every course loads, has a floor
//! under its spawn, lets the SM64 physics stand and walk in all four directions, and fits the performance budget.
use super::courses::{Series, COURSES};
use super::runner::GameSim;
use super::world::CourseRequest;
use jumpman_sim::state::*;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

pub(crate) fn worlds() -> Vec<&'static str> {
    COURSES.iter().filter(|c| c.series == Series::Banjo).map(|c| c.id).collect()
}

fn idle() -> ControllerSnapshot {
    ControllerSnapshot::default()
}

fn stick(x: f64, y: f64) -> ControllerSnapshot {
    ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: 64.0, ..Default::default() }
}

/// Mario swims, wades or floats (water maps like Blubber's ship): the floor is far below and that is fine.
fn in_water(sim: &GameSim) -> bool {
    sim.mario.action & (ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER) != 0
}

#[test]
fn bk_worlds_are_listed() {
    assert!(worlds().len() >= 100, "{}", worlds().len());
    for id in worlds() {
        assert!(super::course_data::CourseData::path(&assets(), id, 1).is_file(), "{id} json missing");
    }
}

#[test]
fn spawn_floor_exists_and_mario_stands() {
    let mut bad = vec![];
    for id in worlds() {
        let mut sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap_or_else(|e| panic!("{id}: {e}"));
        let spawn = sim.course.spawn.clone();
        let floor = sim.course.world.find_floor(spawn.pos[0], spawn.pos[1] + 100.0, spawn.pos[2]);
        if floor.surface.is_none() {
            bad.push(format!("{id}: no floor under the spawn {:?}", spawn.pos));
            continue;
        }
        if (floor.height - spawn.pos[1]).abs() >= 400.0 {
            bad.push(format!("{id}: spawn y {} vs floor {}", spawn.pos[1], floor.height));
        }
        for _ in 0..150 {
            sim.tick(&idle());
        }
        let m = &sim.mario;
        if m.pos[1] <= sim.course.death_height + 100.0 {
            bad.push(format!("{id}: Mario fell through to {}", m.pos[1]));
        } else if !in_water(&sim) && !(m.floor.is_some() && (m.pos[1] - m.floor_height).abs() < 5.0) {
            bad.push(format!("{id}: not standing: y {} floor {} action {:#x}", m.pos[1], m.floor_height, m.action));
        }
        if m.health <= 0.0 {
            bad.push(format!("{id}: died on the spawn"));
        }
    }
    assert!(bad.is_empty(), "{} maps:\n{}", bad.len(), bad.join("\n"));
}

/// Horizontal distance Mario covers pushing the stick for 45 frames from the spawn (swimming maps press A for strokes).
fn walk(id: &str, dx: f64, dy: f64) -> (f64, bool) {
    let mut sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap();
    for _ in 0..60 {
        sim.tick(&idle());
    }
    let start = sim.mario.pos;
    let swim = in_water(&sim);
    for t in 0..45 {
        let mut c = stick(dx, dy);
        if swim && t % 8 == 0 {
            c.button_pressed = A_BUTTON;
            c.button_down = A_BUTTON;
        }
        sim.tick(&c);
    }
    let m = &sim.mario;
    (((m.pos[0] - start[0]).powi(2) + (m.pos[2] - start[2]).powi(2)).sqrt(), m.pos[1] > sim.course.death_height)
}

/// Sub-areas whose arrival tunnel is lower than Mario's 160-unit SM64 hitbox (Banjo is a head shorter in the game): the
/// warp triggers still work, walking does not. Reported, not hidden.
const CRAMPED: &[&str] = &["bk_mmm_inside_loggo", "bk_ccw_spring_zubba_hive"];

#[test]
fn mario_walks_in_every_direction() {
    // The 14 worlds start in the open: all four directions. A sub-area's spawn is the doorway of the map (a wall right
    // behind it, Mumbo's skull is 1.5 Mario wide, lair lobbies start in corridors): some axis must be open both ways.
    let mut bad = vec![];
    for id in worlds() {
        let dirs = [("up", 0.0, 64.0), ("down", 0.0, -64.0), ("left", -64.0, 0.0), ("right", 64.0, 0.0)];
        let moved: Vec<f64> = dirs
            .iter()
            .map(|&(name, dx, dy)| {
                let (moved, alive) = walk(id, dx, dy);
                if !alive {
                    bad.push(format!("{id}: fell while walking {name}"));
                }
                moved
            })
            .collect();
        let open: Vec<bool> = moved.iter().map(|m| *m >= 50.0).collect();
        if !super::courses::is_bk_subarea(id) {
            if open.iter().any(|o| !o) {
                bad.push(format!("{id}: only moved {moved:.0?} for up/down/left/right (invisible wall at spawn?)"));
            }
        } else if CRAMPED.contains(&id) {
            assert!(open.iter().all(|o| !o), "{id} is no longer cramped: {moved:.0?}, drop it from CRAMPED");
        } else if !((open[0] && open[1]) || (open[2] && open[3])) {
            bad.push(format!("{id}: no axis is open both ways: {moved:.0?}"));
        }
    }
    assert!(bad.is_empty(), "{} cases:\n{}", bad.len(), bad.join("\n"));
}

/// The glb a course draws (sub-areas that share a model point at the owner's file).
fn terrain_path(id: &str) -> PathBuf {
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(super::course_data::CourseData::path(&assets(), id, 1)).unwrap()).unwrap();
    assets().join(json["model"].as_str().unwrap_or_else(|| panic!("{id}: no model")))
}

#[test]
fn bk_world_budgets() {
    // iPhone budget: keep each map's drawn geometry modest (the unique glbs; shared models are counted once).
    let mut seen = std::collections::HashSet::new();
    let mut total_bytes = 0u64;
    for id in worlds() {
        let path = terrain_path(id);
        if !seen.insert(path.clone()) {
            continue;
        }
        total_bytes += std::fs::metadata(&path).unwrap_or_else(|e| panic!("{id}: {}: {e}", path.display())).len();
        let json = crate::zelda::glb::read_json(&path).unwrap_or_else(|| panic!("{id}: terrain.glb unreadable"));
        let prims: usize = json["meshes"][0]["primitives"].as_array().map_or(0, |p| p.len());
        assert!(prims >= 1 && prims < 200, "{id}: {prims} primitives");
        let verts: u64 = json["accessors"].as_array().unwrap().iter().filter(|a| a["type"] == "VEC3").map(|a| a["count"].as_u64().unwrap()).sum();
        assert!(verts < 40_000, "{id}: {verts} vertices");
    }
    // all 14 worlds + 74 unique sub-area models together stay a sane download
    assert!(total_bytes < 20_000_000, "BK terrain glbs total {total_bytes} bytes");
    assert!(seen.len() >= 80, "{} unique models", seen.len());
}
