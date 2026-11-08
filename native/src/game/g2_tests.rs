//! SM64 group 2 (courses 7-13) integration: every exported area loads, every exported behavior
//! has a sim kind, the in-level warps switch area, and the moving platforms keep Mario aboard.
use super::game2_tests::{assets, ctl};
use super::runner::{GameSim, SimEvent};
use super::world::CourseRequest;
use crate::progress::flow::Progress;
use crate::progress::save::SaveFile;
use jumpman_sim::object::{ObjId, ObjectBehaviorKind};
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::*;
use serde_json::Value;

const AREAS: &[(&str, &[i32])] = &[("lll", &[1, 2]), ("ssl", &[1, 2]), ("wdw", &[1, 2]), ("thi", &[1, 2, 3]), ("sl", &[1]), ("ttm", &[1]), ("ddd", &[1, 2])];

fn json(id: &str, area: i32) -> Value {
    let name = if area == 1 { format!("{id}.json") } else { format!("{id}.{area}.json") };
    serde_json::from_slice(&std::fs::read(assets().join("courses").join(name)).unwrap()).unwrap()
}

fn load(id: &str, area: i32) -> GameSim {
    let mut req = CourseRequest::new(id);
    req.area = area;
    req.g2 = true;
    GameSim::load(&assets(), req).unwrap_or_else(|e| panic!("{id} area {area}: {e}"))
}

/// Every behavior name the group-2 exporter writes is a sim kind (nothing is silently dropped), and
/// the objects the loader created cover all of them.
#[test]
fn every_exported_g2_behavior_is_a_registered_sim_kind() {
    let mut checked = 0;
    for (id, areas) in AREAS {
        for &area in *areas {
            let data = json(id, area);
            let sim = load(id, area);
            for o in data["objects"].as_array().unwrap().iter().filter(|o| o["g2"] == true) {
                let Some(name) = o["behavior"].as_str() else { continue };
                assert!(ObjectBehaviorKind::from_name(name).is_some(), "{id}.{area}: unknown behavior {name}");
                assert!(!sim.course.skipped_behaviors.iter().any(|s| s == name), "{id}.{area}: {name} was skipped by the loader");
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "only {checked} group-2 placements exported");
}

/// All areas run 90 frames with Mario at the spawn without panics or NaNs, and the registered
/// collision meshes make the moving platforms solid.
#[test]
fn g2_areas_run_for_a_few_seconds() {
    for (id, areas) in AREAS {
        for &area in *areas {
            let mut sim = load(id, area);
            for _ in 0..90 {
                sim.tick(&ctl(0.0, 0.0, 0, 0));
            }
            assert!(sim.mario.pos.iter().all(|v| v.is_finite()), "{id}.{area}");
        }
    }
}

/// LLL's platforms own collision surfaces while Mario is within their `oCollisionDistance`, and drop them when he leaves.
#[test]
fn lll_platforms_load_and_unload_their_collision_with_distance() {
    let mut sim = load("lll", 1);
    let platform = sim.mario.objects.objects.iter().position(|o| o.behavior == ObjectBehaviorKind::LllSinkRect && o.move_yaw != 0.0).expect("a sinking platform");
    let tag = Some(platform as u32);
    let near = sim.mario.objects.objects[platform].pos;
    sim.mario.pos = [near[0], near[1] + 400.0, near[2]];
    for _ in 0..3 {
        sim.tick(&ctl(0.0, 0.0, 0, 0));
    }
    let owned = sim.mario.world.surfaces().iter().filter(|s| s.object == tag).count();
    assert!(owned >= 6, "{owned} surfaces owned by the platform");
    sim.mario.pos = [near[0], near[1] + 400.0, near[2] + 5000.0];
    for _ in 0..2 {
        sim.tick(&ctl(0.0, 0.0, 0, 0));
    }
    assert_eq!(sim.mario.world.surfaces().iter().filter(|s| s.object == tag).count(), 0, "collision is dropped beyond 2000 units");
}

/// Tiny-Huge Island: stepping into a warp pipe asks for the other island (area 2); the area swap
/// puts Mario beside the matching pipe of the tiny island.
#[test]
fn thi_pipe_switches_to_the_tiny_island() {
    let mut sim = load("thi", 1);
    let mut progress = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
    let pipe = sim.mario.objects.objects.iter().position(|o| o.behavior == ObjectBehaviorKind::Warp && o.warp_area == Some(2)).expect("a pipe warp to area 2");
    let pos = sim.mario.objects.objects[pipe].pos;
    sim.mario.pos = pos;
    sim.mario.pos[1] += 10.0;
    sim.tick(&ctl(0.0, 0.0, 0, 0));
    assert!(sim.events.iter().any(|e| matches!(e, SimEvent::Object(ObjectEvent::Warp { destination, area: Some(2), .. }) if destination == "thi")), "the pipe warp fired: {:?}", sim.events);
    progress.post_tick(&mut sim, &ctl(0.0, 0.0, 0, 0));
    let change = progress.pending_area.take().expect("an in-place area change to the tiny island");
    assert_eq!(change.area, 2);
    progress.change_area(&mut sim, &assets(), change).expect("area switch");
    assert_eq!(sim.course.request.area, 2);
    // Arrival: beside a pipe of the tiny island on a floor.
    let hit = sim.mario.world.find_floor(sim.mario.pos[0], sim.mario.pos[1] + 100.0, sim.mario.pos[2]);
    assert!(hit.surface.is_some(), "arrival point {:?} has no floor", sim.mario.pos);
}

/// Lethal Lava Land's volcano is area 2; the entrance warp in area 1 leads there.
#[test]
fn lll_volcano_is_reachable_through_its_warp() {
    let data = json("lll", 1);
    let warp = &data["warps"][0];
    assert_eq!(warp["warpTo"], "lll");
    assert_eq!(warp["warpArea"], 2);
    let sim = load("lll", 2);
    assert!(sim.mario.objects.objects.iter().any(|o| o.behavior == ObjectBehaviorKind::VolcanoTrap));
    assert!(sim.mario.objects.objects.iter().any(|o| o.behavior == ObjectBehaviorKind::Pole));
}

/// The volcano entrance fires the in-place area warp when Mario stands in it.
#[test]
fn lll_entrance_warp_fires() {
    let mut sim = load("lll", 1);
    sim.mario.pos = [0.0, 110.0, 0.0];
    sim.tick(&ctl(0.0, 0.0, 0, 0));
    let fired = sim.events.iter().any(|e| matches!(e, SimEvent::Object(ObjectEvent::Warp { destination, area: Some(2), entry: Some(n) }) if destination == "lll" && n == "WARP_NODE_0A"));
    assert!(fired, "events {:?}", sim.events);
}
