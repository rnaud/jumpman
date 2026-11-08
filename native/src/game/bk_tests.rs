//! Banjo-Kazooie worlds (`scripts/export-bk-levels.py`): every world loads, has a floor under its
//! spawn, lets the SM64 physics stand and walk in all four directions, and fits the performance budget.
use super::courses::{Series, COURSES};
use super::runner::GameSim;
use super::world::CourseRequest;
use jumpman_sim::state::ControllerSnapshot;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn worlds() -> Vec<&'static str> {
    COURSES.iter().filter(|c| c.series == Series::Banjo).map(|c| c.id).collect()
}

fn idle() -> ControllerSnapshot {
    ControllerSnapshot::default()
}

fn stick(x: f64, y: f64) -> ControllerSnapshot {
    ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: 64.0, ..Default::default() }
}

#[test]
fn bk_worlds_are_listed() {
    assert!(worlds().len() >= 3, "{:?}", worlds());
    for id in worlds() {
        assert!(super::course_data::CourseData::path(&assets(), id, 1).is_file(), "{id} json missing");
    }
}

#[test]
fn spawn_floor_exists_and_mario_stands() {
    for id in worlds() {
        let mut sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap_or_else(|e| panic!("{id}: {e}"));
        let spawn = sim.course.spawn.clone();
        let floor = sim.course.world.find_floor(spawn.pos[0], spawn.pos[1] + 100.0, spawn.pos[2]);
        assert!(floor.surface.is_some(), "{id}: no floor under the spawn {:?}", spawn.pos);
        assert!((floor.height - spawn.pos[1]).abs() < 400.0, "{id}: spawn y {} vs floor {}", spawn.pos[1], floor.height);
        for _ in 0..150 {
            sim.tick(&idle());
        }
        let m = &sim.mario;
        assert!(m.pos[1] > sim.course.death_height + 100.0, "{id}: Mario fell through to {}", m.pos[1]);
        assert!(m.floor.is_some() && (m.pos[1] - m.floor_height).abs() < 5.0, "{id}: not standing: y {} floor {}", m.pos[1], m.floor_height);
        assert!(m.health > 0.0, "{id}: died on the spawn");
    }
}

#[test]
fn mario_walks_in_every_direction() {
    for id in worlds() {
        for (name, dx, dy) in [("up", 0.0, 64.0), ("down", 0.0, -64.0), ("left", -64.0, 0.0), ("right", 64.0, 0.0)] {
            let mut sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap();
            for _ in 0..60 {
                sim.tick(&idle());
            }
            let start = sim.mario.pos;
            for _ in 0..45 {
                sim.tick(&stick(dx, dy));
            }
            let m = &sim.mario;
            let moved = ((m.pos[0] - start[0]).powi(2) + (m.pos[2] - start[2]).powi(2)).sqrt();
            assert!(moved >= 50.0, "{id}: stick {name} only moved {moved:.1} units (invisible wall at spawn?)");
            assert!(m.pos[1] > sim.course.death_height, "{id}: fell while walking {name}");
        }
    }
}

#[test]
fn bk_world_budgets() {
    // iPhone budget: keep each world's drawn geometry modest.
    for id in worlds() {
        let path = assets().join(format!("levels/{id}/terrain.glb"));
        let json = crate::zelda::glb::read_json(&path).unwrap_or_else(|| panic!("{id}: terrain.glb unreadable"));
        let prims: usize = json["meshes"][0]["primitives"].as_array().map_or(0, |p| p.len());
        assert!(prims > 5 && prims < 200, "{id}: {prims} primitives");
        let verts: u64 = json["accessors"].as_array().unwrap().iter().filter(|a| a["type"] == "VEC3").map(|a| a["count"].as_u64().unwrap()).sum();
        assert!(verts < 40_000, "{id}: {verts} vertices");
    }
}
