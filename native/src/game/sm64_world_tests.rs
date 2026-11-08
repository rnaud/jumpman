//! The levels and areas added by `scripts/sm64_world.py` (the castle courtyard, the three cap levels and the second /
//! third areas of lll, ssl, sl, thi, ttm, wdw): every one loads with the world extensions, has a floor under its spawn,
//! lets Mario stand and walk, arrives correctly through every warp node, and keeps its sky, cap switch and cap boxes.
use super::course_data::{read_course_value, set_extensions};
use super::runner::GameSim;
use super::world::CourseRequest;
use jumpman_sim::state::*;
use jumpman_sim::object::ObjectBehaviorKind;
use serde_json::Value;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

/// (course, area) that did not exist before the world extensions.
const NEW: &[(&str, i32)] = &[
    ("castle_courtyard", 1),
    ("cotmc", 1),
    ("vcutm", 1),
    ("wmotr", 1),
    ("lll", 2),
    ("ssl", 2),
    ("ssl", 3),
    ("sl", 2),
    ("thi", 2),
    ("thi", 3),
    ("ttm", 2),
    ("ttm", 3),
    ("ttm", 4),
    ("wdw", 2),
];

fn load(id: &str, area: i32, entry: Option<&str>) -> GameSim {
    set_extensions(true);
    let mut req = CourseRequest::new(id);
    req.area = area;
    req.entry = entry.map(str::to_string);
    GameSim::load(&assets(), req).unwrap_or_else(|e| panic!("{id}.{area}: {e}"))
}

fn idle() -> ControllerSnapshot {
    ControllerSnapshot::default()
}

fn stick(x: f64, y: f64) -> ControllerSnapshot {
    ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: 64.0, ..Default::default() }
}

fn in_water(sim: &GameSim) -> bool {
    sim.mario.action & (ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER) != 0
}

fn json(id: &str, area: i32) -> Value {
    set_extensions(true);
    read_course_value(&assets(), id, area).unwrap()
}

#[test]
fn new_levels_are_in_the_catalogue_and_have_geometry() {
    for (id, area) in NEW {
        let c = json(id, *area);
        assert!(c["triangles"].as_array().unwrap().len() > 100, "{id}.{area}");
        let model = c["model"].as_str().unwrap();
        assert!(assets().join(model).is_file(), "{id}.{area}: model {model} missing");
        if *area == 1 {
            assert!(super::courses::info(id).is_some(), "{id} is missing from the catalogue");
        }
    }
}

#[test]
fn spawn_has_a_floor_and_mario_stands() {
    let mut bad = vec![];
    for (id, area) in NEW {
        let mut sim = load(id, *area, None);
        let spawn = sim.course.spawn.clone();
        let floor = sim.course.world.find_floor(spawn.pos[0], spawn.pos[1] + 100.0, spawn.pos[2]);
        if floor.surface.is_none() && spawn.action.is_none() && !(spawn.pos[1] > floor.height) {
            bad.push(format!("{id}.{area}: no floor under the spawn {:?}", spawn.pos));
            continue;
        }
        for _ in 0..240 {
            sim.tick(&idle());
        }
        let m = &sim.mario;
        if m.pos[1] <= sim.course.death_height + 100.0 {
            bad.push(format!("{id}.{area}: Mario fell through to {}", m.pos[1]));
        } else if !in_water(&sim) && !(m.floor.is_some() && (m.pos[1] - m.floor_height).abs() < 5.0) && m.action & ACT_FLAG_AIR == 0 {
            bad.push(format!("{id}.{area}: not standing: y {} floor {} action {:#x}", m.pos[1], m.floor_height, m.action));
        }
        if m.health <= 0.0 {
            bad.push(format!("{id}.{area}: died on the spawn"));
        }
    }
    assert!(bad.is_empty(), "{} areas:\n{}", bad.len(), bad.join("\n"));
}

fn walk(sim: &mut GameSim, dx: f64, dy: f64) -> f64 {
    for _ in 0..60 {
        sim.tick(&idle());
    }
    let start = sim.mario.pos;
    let swim = in_water(sim);
    for t in 0..60 {
        let mut c = stick(dx, dy);
        if swim && t % 8 == 0 {
            c.button_pressed = A_BUTTON;
            c.button_down = A_BUTTON;
        }
        sim.tick(&c);
    }
    ((sim.mario.pos[0] - start[0]).powi(2) + (sim.mario.pos[2] - start[2]).powi(2)).sqrt()
}

#[test]
fn mario_walks_somewhere_from_every_spawn() {
    let mut bad = vec![];
    for (id, area) in NEW {
        let mut best = 0.0f64;
        for (dx, dy) in [(0.0, 64.0), (0.0, -64.0), (64.0, 0.0), (-64.0, 0.0)] {
            let mut sim = load(id, *area, None);
            best = best.max(walk(&mut sim, dx, dy));
        }
        if best < 100.0 {
            bad.push(format!("{id}.{area}: Mario moves at most {best:.0} units from the spawn"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn every_arrival_point_has_a_floor_or_lets_mario_fall_safely() {
    let mut bad = vec![];
    for (id, area) in NEW {
        let c = json(id, *area);
        for (node, sp) in c["warpSpawns"].as_object().into_iter().flatten() {
            let sim = load(id, *area, Some(node));
            let p = sim.course.spawn.pos;
            let floor = sim.course.world.find_floor(p[0], p[1] + 150.0, p[2]);
            if floor.surface.is_none() {
                bad.push(format!("{id}.{area} {node}: no floor below the arrival {p:?} ({})", sp["behavior"]));
            } else if p[1] - floor.height > 6000.0 {
                bad.push(format!("{id}.{area} {node}: {} units above the floor", p[1] - floor.height));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn cap_levels_have_their_switch_boxes_and_red_coins() {
    for (id, kind) in [("cotmc", 1), ("vcutm", 2)] {
        let c = json(id, 1);
        assert_eq!(c["capSwitchKind"].as_i64(), Some(kind), "{id}");
        assert!(c["wingSwitch"].is_array(), "{id} switch");
        assert!(c["extraCollision"]["switchTop"].is_object() && c["extraCollision"]["switchBase"].is_object(), "{id} switch collision");
        assert_eq!(c["redCoins"].as_array().unwrap().len(), 8, "{id} red coins");
        assert!(c["redCoinStar"].is_array(), "{id}");
        // two boxes of this kind, solid only once the switch was pressed
        let sim = load(id, 1, None);
        let boxes: Vec<_> = sim.mario.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::WingBox).collect();
        assert!(boxes.len() >= 2 && boxes.iter().all(|o| o.param == kind as f64), "{id}: {} boxes", boxes.len());
    }
    let w = json("wmotr", 1);
    assert_eq!(w["redCoins"].as_array().unwrap().len(), 8);
    assert!(w["cannons"].as_array().map_or(0, Vec::len) >= 2, "the cannons of the rainbow");
    assert!(w["sky"].is_object() && w["sky"]["top"][2].as_f64().unwrap() > 0.5, "cloud sky");
    // metal and vanish boxes exist in the other courses too (extensions)
    for (id, kind, area) in [("hmc", 1, 1), ("wf", 1, 1), ("jrb", 1, 1), ("bbh", 2, 1), ("ddd", 1, 2), ("ddd", 2, 2), ("sl", 2, 2), ("wdw", 2, 2)] {
        let sim = load(id, area, None);
        assert!(sim.mario.objects.objects.iter().any(|o| o.behavior == ObjectBehaviorKind::WingBox && o.param == kind as f64), "{id}.{area} has no cap box of kind {kind}");
    }
}

#[test]
fn courtyard_has_its_door_cage_and_exits() {
    let c = json("castle_courtyard", 1);
    let doors = c["doors"].as_array().unwrap();
    assert!(doors.iter().any(|d| d["destination"] == "castle_inside" && d["destNode"] == "WARP_NODE_02"), "back to the castle");
    let cage = c["warps"].as_array().unwrap().iter().find(|w| w["warpTo"] == "bbh").expect("BBH cage");
    assert_eq!(cage["minStars"], 12);
    assert_eq!(c["warpGates"]["bbh"], 12);
    assert_eq!(c["deathWarp"]["course"], "castle_grounds");
    // the castle's door leads here
    let inside = json("castle_inside", 1);
    assert!(inside["doors"].as_array().unwrap().iter().any(|d| d["destination"] == "castle_courtyard"));
    // BBH's exits come back to the courtyard
    let bbh = json("bbh", 1);
    assert_eq!(bbh["successCourse"], "castle_courtyard");
    assert_eq!(bbh["deathWarp"]["course"], "castle_courtyard");
}
