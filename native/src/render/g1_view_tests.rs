//! App-level checks for the SM64 group 1 actors: placements, models, clips and a smoke run.
use super::g1_view::{anim_table, glb_animation_names};
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use jumpman_sim::object::ObjectBehaviorKind as K;
use jumpman_sim::state::ControllerSnapshot;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

/// (course, area, behavior, count) that `export-sm64-g1-actors.py` places.
const PLACED: &[(&str, i32, K, usize)] = &[
    ("wf", 1, K::Thwomp, 1), ("wf", 1, K::Thwomp2, 1), ("wf", 1, K::PiranhaPlant, 3), ("wf", 1, K::BulletBill, 1), ("wf", 1, K::Butterfly, 10),
    ("hmc", 1, K::Swoop, 11), ("hmc", 1, K::Scuttlebug, 4), ("hmc", 1, K::ScuttlebugSpawner, 2), ("hmc", 1, K::Snufit, 4),
    ("hmc", 1, K::MontyMole, 1), ("hmc", 1, K::MontyMoleHole, 3), ("bbh", 1, K::Scuttlebug, 3), ("bbh", 1, K::MadPiano, 1), ("bbh", 1, K::HauntedChair, 1), ("bbh", 1, K::FlyingBookend, 3), ("bbh", 1, K::BookendSpawn, 4), ("ccm", 1, K::Spindrift, 5),
    ("castle_grounds", 1, K::Butterfly, 12), ("castle_grounds", 1, K::Bird, 3), ("castle_inside", 3, K::Mips, 1),
];

fn request(course: &str, area: i32) -> CourseRequest {
    let mut r = CourseRequest::new(course);
    r.area = area;
    // WF's Bullet Bill is an OBJECT_WITH_ACTS placement (acts 2-6).
    if course == "wf" {
        r.act = 2;
    }
    // The BBH furniture (Mad Piano, Haunted Chair, bookends) is a g2/r2a placement the app loads only outside the TS goldens.
    if course == "bbh" {
        r.g2 = true;
    }
    r
}

#[test]
fn group_one_actors_are_placed_with_models_and_clips() {
    for &(course, area, kind, count) in PLACED {
        let sim = GameSim::load(&assets(), request(course, area)).unwrap();
        let found: Vec<_> = sim.mario.objects.objects.iter().filter(|o| o.behavior == kind).collect();
        assert_eq!(found.len(), count, "{course} {kind:?}");
        assert!(sim.course.skipped_behaviors.is_empty(), "{course}: {:?}", sim.course.skipped_behaviors);
        for o in found {
            if let Some(model) = o.model.as_deref() {
                let path = assets().join(model);
                assert!(path.exists(), "{model}");
                // Every decomp animation name of the table exists as a glb clip.
                let clips = glb_animation_names(&path);
                // The Flying Bookend swaps to `bookend_part_geo`, which carries its clips.
                let path = if kind == K::FlyingBookend { assets().join(jumpman_sim::behaviors::r3::bbh::BOOKEND_MODEL) } else { path };
                let clips = if kind == K::FlyingBookend { glb_animation_names(&path) } else { clips };
                for name in anim_table(kind) {
                    assert!(clips.iter().any(|c| c == name), "{model} lacks clip {name}: {clips:?}");
                }
            } else {
                assert!(matches!(kind, K::ScuttlebugSpawner | K::MontyMoleHole | K::BookendSpawn));
            }
        }
    }
}

#[test]
fn group_one_courses_survive_ten_seconds_of_play() {
    let idle = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: 0.0, stick_mag: 0.0, button_down: 0, button_pressed: 0 };
    for (course, area) in [("wf", 1), ("hmc", 1), ("bbh", 1), ("ccm", 1), ("castle_grounds", 1), ("castle_inside", 3)] {
        let mut sim = GameSim::load(&assets(), request(course, area)).unwrap();
        for _ in 0..300 {
            sim.tick(&idle);
        }
        for o in &sim.mario.objects.objects {
            assert!(o.pos.iter().all(|v| v.is_finite()), "{course} {:?} {:?}", o.behavior, o.pos);
            assert!(o.scale.is_finite());
        }
        assert!(sim.mario.pos.iter().all(|v| v.is_finite()));
    }
}

/// WF's Thwomp is dropped onto the floor at start and then cycles between home and +416.
#[test]
fn wf_thwomp_cycles_above_its_floor() {
    let idle = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: 0.0, stick_mag: 0.0, button_down: 0, button_pressed: 0 };
    let mut sim = GameSim::load(&assets(), CourseRequest::new("wf")).unwrap();
    let id = sim.mario.objects.objects.iter().position(|o| o.behavior == K::Thwomp).unwrap();
    let mut seen = (f64::MAX, f64::MIN);
    for _ in 0..600 {
        sim.tick(&idle);
        let o = &sim.mario.objects.objects[id];
        assert!(o.active && o.visible);
        seen = (seen.0.min(o.pos[1]), seen.1.max(o.pos[1]));
    }
    eprintln!("thwomp y range {seen:?} model {:?}", sim.mario.objects.objects[id].model);
    assert_eq!(seen.0, 1076.0);
    assert_eq!(seen.1, 1076.0 + 41.0 * 10.0 + 5.0);
}

/// The old differential fixtures must still see exactly the original object set.
#[test]
fn legacy_fixture_mode_hides_the_group_one_objects() {
    crate::game::world::legacy_fixture_mode(true);
    let sim = GameSim::load(&assets(), CourseRequest::new("hmc")).unwrap();
    crate::game::world::legacy_fixture_mode(false);
    assert!(!sim.mario.objects.objects.iter().any(|o| jumpman_sim::behaviors::g1::is_g1(o.behavior)));
}
