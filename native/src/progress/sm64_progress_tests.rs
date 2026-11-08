//! SM64 progression bookkeeping with scripted save flags: the 120-star total, star doors (1 / 3 / 8 / 30 / 50 / 70
//! stars), key doors, the three cap switches and their boxes, the 12-star cage gate, the 100-coin star and coin high
//! scores, and the course-complete exits (`successCourse`).
use super::data::{CastleDoor, ProgressData};
use super::doors::{probe_entrance, unlock_flag, EntranceHit};
use super::flow::{Flow, Progress};
use super::save::{CastleKey, SaveFile, FLAG_METAL_SWITCH, FLAG_VANISH_SWITCH, FLAG_WING_SWITCH};
use crate::game::course_data::set_extensions;
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use jumpman_sim::object::ObjectBehaviorKind;
use jumpman_sim::state::*;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

const SCORE: [&str; 15] = ["bob", "wf", "jrb", "ccm", "bbh", "hmc", "lll", "ssl", "ddd", "sl", "wdw", "ttm", "thi", "ttc", "rr"];
/// The 15 secret / Bowser / cap courses that hold a star: (course, acts).
const SECRET: [(&str, &[i32]); 10] = [
    ("pss", &[1, 2]),
    ("sa", &[1]),
    ("totwc", &[1]),
    ("cotmc", &[1]),
    ("vcutm", &[1]),
    ("wmotr", &[1]),
    ("bitdw", &[1]),
    ("bitfs", &[1]),
    ("bits", &[1]),
    ("castle_secret", &[1]),
];

/// A save with exactly `n` counted stars (course:act keys).
fn save_with(n: u32) -> SaveFile {
    let mut s = SaveFile::in_memory(1);
    let mut given = 0;
    'outer: for c in SCORE {
        for act in 1..=7 {
            if given == n {
                break 'outer;
            }
            s.collect_star(c, act, 0);
            given += 1;
        }
    }
    assert_eq!(s.stars(), n);
    s
}

fn castle(area: i32) -> (GameSim, Progress) {
    set_extensions(true);
    let mut req = CourseRequest::new("castle_inside");
    req.area = area;
    let mut sim = GameSim::load(&assets(), req).unwrap();
    let p = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
    (sim, p)
}

#[test]
fn the_120_stars_are_all_countable_and_distinct() {
    let mut s = SaveFile::in_memory(1);
    for c in SCORE {
        for act in 1..=7 {
            s.collect_star(c, act, 0);
        }
    }
    assert_eq!(s.stars(), 105, "15 courses x (6 acts + the 100-coin star)");
    for (c, acts) in SECRET.iter().filter(|(c, _)| *c != "castle_secret") {
        for a in *acts {
            s.collect_star(c, *a, 0);
        }
    }
    assert_eq!(s.stars(), 115, "+ pss 2, sa, totwc, cotmc, vcutm, wmotr, bitdw, bitfs, bits");
    // The five castle stars (3 Toads, 2 MIPS) are saved under their own ids so that they count (the castle's own
    // `castle_inside` / `castle_grounds` stars never do).
    for id in ["toad", "toad", "toad", "mips", "mips"].iter().enumerate().map(|(i, c)| (*c, i as i32 + 1)) {
        s.collect_star(id.0, id.1, 0);
    }
    assert_eq!(s.stars(), 120);
    s.collect_star("castle_inside", 1, 0);
    s.collect_star("bowser_3", 1, 0);
    assert_eq!(s.stars(), 120, "castle bonus stars and the final arena do not count");
}

#[test]
fn star_doors_open_exactly_at_their_requirement() {
    use super::data::RoomDoorSpec;
    let mut flags = std::collections::BTreeMap::new();
    for area in 1..=3 {
        let (_sim, p) = castle(area);
        let leaves: Vec<RoomDoorSpec> = p.doors.as_ref().map(|d| d.leaves.iter().map(|l| l.door.clone()).collect()).unwrap_or_default();
        for door in leaves.iter().filter(|d| d.stars > 0 && d.stars < 70) {
            flags.insert(door.stars, unlock_flag(door));
            let yaw = door.yaw.to_radians();
            for (stars, opens) in [(door.stars - 1, false), (door.stars, true)] {
                // Mario walks at the door from the side he faces
                let (mut sim, mut p) = castle(area);
                let mut save = save_with(stars);
                sim.mario.pos = [door.pos[0] + 90.0 * yaw.sin(), door.pos[1], door.pos[2] + 90.0 * yaw.cos()];
                sim.mario.set_action(ACT_WALKING, 0.0);
                let mut notes = vec![];
                let busy = p.doors.as_mut().unwrap().step(&mut sim.mario, &mut save, &mut |n| notes.push(n));
                if opens {
                    assert!(busy, "area {area}: door at {:?} needing {} stars did not react with {stars} stars", door.pos, door.stars);
                } else {
                    assert!(!busy, "area {area}: door at {:?} opened with {stars} < {} stars", door.pos, door.stars);
                    assert!(notes.iter().any(|n| n.contains("MORE STARS")), "no hint for the door needing {}: {notes:?}", door.stars);
                }
            }
        }
    }
    // the flag bits that remember the opened doors (get_door_save_file_flag)
    for need in [1u32, 3, 8, 30, 50] {
        assert!(flags.get(&need).is_some_and(|f| *f != 0), "no remembered flag for the {need}-star door: {flags:?}");
    }
    assert_eq!(flags[&8], 1 << 14);
    assert_eq!(flags[&30], 1 << 15);
    assert_eq!(flags[&50], 1 << 20);
}

#[test]
fn castle_star_door_requirements_match_the_decomp() {
    // castle_inside/script.c bhvStarDoor params + the SPECIAL_OBJECT presets of the three areas.
    let mut seen = std::collections::BTreeSet::new();
    for area in 1..=3 {
        let (_sim, p) = castle(area);
        for l in &p.doors.as_ref().map(|d| d.leaves.iter().map(|l| l.door.stars).collect::<Vec<_>>()).unwrap_or_default() {
            seen.insert(*l);
        }
    }
    for need in [0, 1, 3, 8, 30, 50, 70] {
        assert!(seen.contains(&need), "no {need}-star door in the castle: {seen:?}");
    }
}

#[test]
fn key_doors_need_the_key_and_open_once() {
    set_extensions(true);
    let data = ProgressData::load(&assets(), "castle_inside", 1);
    let basement: Vec<&CastleDoor> = data.doors.iter().filter(|d| d.required_key == Some(1)).collect();
    let upstairs: Vec<&CastleDoor> = data.doors.iter().filter(|d| d.required_key == Some(2)).collect();
    assert_eq!((basement.len(), upstairs.len()), (2, 2), "two leaves per key door");
    let (mut sim, _p) = castle(1);
    for (doors, key) in [(basement, CastleKey::Basement), (upstairs, CastleKey::Upstairs)] {
        let d = doors[0];
        sim.mario.pos = [d.pos[0], d.pos[1], d.pos[2] + 60.0 * if d.yaw == 0.0 { 1.0 } else { -1.0 }];
        sim.mario.set_action(ACT_WALKING, 0.0);
        let mut save = SaveFile::in_memory(1);
        assert_eq!(probe_entrance(&data.doors, &sim.mario, &save), Some(EntranceHit::NeedsKey));
        save.collect_key(key);
        assert!(matches!(probe_entrance(&data.doors, &sim.mario, &save), Some(EntranceHit::Enter { locked: true, .. })), "unlocking with the key");
        assert!(save.unlock_door(key));
        assert!(matches!(probe_entrance(&data.doors, &sim.mario, &save), Some(EntranceHit::Enter { locked: false, .. })), "open for good");
    }
}

#[test]
fn bowser_arenas_award_the_keys_and_the_castle_asks_for_them() {
    use super::stars::StarDirector;
    set_extensions(true);
    for (arena, key) in [("bowser_1", CastleKey::Basement), ("bowser_2", CastleKey::Upstairs)] {
        let sim = GameSim::load(&assets(), CourseRequest::new(arena)).unwrap();
        let data = ProgressData::load(&assets(), arena, 1);
        let d = StarDirector::new(arena, 1, &data, sim.course.spawn.pos, &sim.mario.world, &sim.mario, &SaveFile::in_memory(1));
        assert_eq!(d.reward_key, Some(key), "{arena}");
        assert!(d.no_exit || d.reward_key.is_some());
    }
    let mut s = SaveFile::in_memory(1);
    s.collect_key(CastleKey::Basement);
    s.collect_key(CastleKey::Upstairs);
    s.unlock_door(CastleKey::Basement);
    assert!(s.door_unlocked(CastleKey::Basement) && !s.door_unlocked(CastleKey::Upstairs) && s.has_key(CastleKey::Upstairs));
    // Bowser keys are not stars
    assert_eq!(s.stars(), 0);
}

#[test]
fn cap_switch_flags_persist_per_slot_and_open_the_matching_boxes() {
    let dir = std::env::temp_dir().join(format!("jumpman-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = SaveFile::open(&dir, 1);
    assert!(!s.cap_switch_on(0) && !s.cap_switch_on(1) && !s.cap_switch_on(2));
    s.press_cap_switch(1);
    s.confirm_course_save();
    let s = SaveFile::open(&dir, 1);
    assert!(s.cap_switch_on(1) && !s.cap_switch_on(0) && !s.cap_switch_on(2));
    assert!(s.has_flag(FLAG_METAL_SWITCH) && !s.has_flag(FLAG_WING_SWITCH) && !s.has_flag(FLAG_VANISH_SWITCH));
    assert!(!SaveFile::open(&dir, 2).cap_switch_on(1), "another slot is untouched");
    let _ = std::fs::remove_dir_all(&dir);

    // boxes of a level follow the save flags (cotmc: metal, vcutm: vanish)
    for (course, kind, flag) in [("cotmc", 1, FLAG_METAL_SWITCH), ("vcutm", 2, FLAG_VANISH_SWITCH)] {
        for pressed in [false, true] {
            set_extensions(true);
            let mut sim = GameSim::load(&assets(), CourseRequest::new(course)).unwrap();
            let mut save = SaveFile::in_memory(1);
            if pressed {
                save.set_flag(flag);
            }
            let p = Progress::new(&mut sim, &assets(), save);
            let boxes: Vec<_> = sim.mario.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::WingBox).collect();
            assert!(!boxes.is_empty());
            assert!(boxes.iter().all(|o| o.param == f64::from(kind) && (o.var("unlocked") != 0.0) == pressed), "{course} boxes with pressed={pressed}");
            assert_eq!(p.wing.as_ref().map(|w| w.kind), Some(kind));
        }
    }
}

#[test]
fn cap_boxes_release_their_cap() {
    // Break a metal and a vanish box in the engine: the cap flies out, touching it grants the cap for 600 frames.
    set_extensions(true);
    for (course, kind) in [("cotmc", 1.0), ("vcutm", 2.0)] {
        let mut sim = GameSim::load(&assets(), CourseRequest::new(course)).unwrap();
        let mut save = SaveFile::in_memory(1);
        save.set_flag(if kind == 1.0 { FLAG_METAL_SWITCH } else { FLAG_VANISH_SWITCH });
        let _p = Progress::new(&mut sim, &assets(), save);
        let id = sim.mario.objects.objects.iter().position(|o| o.behavior == ObjectBehaviorKind::WingBox).unwrap();
        let home = sim.mario.objects.objects[id].home;
        sim.mario.pos = [home[0], home[1] + 160.0, home[2]];
        sim.mario.vel = [0.0; 3];
        sim.mario.set_action(ACT_GROUND_POUND, 0.0);
        // ground pound onto the box: the sim's star-box behavior reads ACT_GROUND_POUND_LAND at the box top
        sim.mario.set_action(ACT_GROUND_POUND_LAND, 0.0);
        sim.mario.pos = [home[0], home[1] + 104.0, home[2]];
        for _ in 0..20 {
            sim.tick(&ControllerSnapshot::default());
            sim.mario.pos = [home[0], home[1] + 104.0, home[2]];
        }
        let cap = sim.mario.objects.objects.iter().find(|o| o.behavior == ObjectBehaviorKind::WingCap);
        assert!(cap.is_some_and(|c| c.param == kind), "{course}: no cap of kind {kind} spawned");
        // pick it up
        let c = cap.unwrap().pos;
        sim.mario.pos = c;
        for _ in 0..40 {
            sim.tick(&ControllerSnapshot::default());
            let c = sim.mario.objects.objects.iter().find(|o| o.behavior == ObjectBehaviorKind::WingCap).map(|o| o.pos);
            if let Some(c) = c {
                sim.mario.pos = c;
            }
        }
        if kind == 1.0 {
            assert!(sim.mario.metal_cap_timer > 400.0, "metal cap granted: {}", sim.mario.metal_cap_timer);
        } else {
            assert!(sim.mario.vanish_cap_timer > 400.0, "vanish cap granted: {}", sim.mario.vanish_cap_timer);
        }
    }
}

#[test]
fn the_bbh_cage_waits_for_twelve_stars() {
    set_extensions(true);
    for (stars, opens) in [(11, false), (12, true)] {
        let mut sim = GameSim::load(&assets(), CourseRequest::new("castle_courtyard")).unwrap();
        let save = save_with(stars);
        let mut p = Progress::new(&mut sim, &assets(), save);
        let cage = crate::game::course_data::read_course_value(&assets(), "castle_courtyard", 1).unwrap()["warps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["warpTo"] == "bbh")
            .unwrap()["pos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>();
        sim.mario.pos = [cage[0], cage[1], cage[2]];
        for _ in 0..20 {
            sim.mario.pos = [cage[0], cage[1], cage[2]];
            p.pre_tick(&mut sim);
            sim.tick(&ControllerSnapshot::default());
            p.post_tick(&mut sim, &ControllerSnapshot::default());
        }
        let left = p.pending.is_some() || matches!(p.flow, Flow::ActSelect(_));
        assert_eq!(left, opens, "{stars} stars at the cage");
    }
}

#[test]
fn course_complete_exits_return_to_the_right_place() {
    set_extensions(true);
    // star exits: castle for the main courses, the grounds for the moat level, the courtyard for BBH
    let expect = [("bob", None, "castle_inside"), ("vcutm", Some("castle_grounds"), "castle_grounds"), ("bbh", Some("castle_courtyard"), "castle_courtyard"), ("cotmc", None, "castle_inside"), ("wmotr", None, "castle_inside")];
    for (course, success_course, dest) in expect {
        let sim = GameSim::load(&assets(), CourseRequest::new(course)).unwrap();
        assert_eq!(sim.course.success_course.as_deref(), success_course, "{course}");
        let to = sim.course.success_course.clone().unwrap_or_else(|| if sim.course.success_entry.is_some() { "castle_inside".into() } else { "castle_grounds".into() });
        assert_eq!(to, dest, "{course}");
        // the target has the arrival
        let target = GameSim::load(&assets(), {
            let mut r = CourseRequest::new(&to);
            r.area = sim.course.success_area.unwrap_or(1);
            r.entry = sim.course.success_entry.clone();
            r
        })
        .unwrap();
        assert!(target.course.world.find_floor(target.course.spawn.pos[0], target.course.spawn.pos[1] + 100.0, target.course.spawn.pos[2]).surface.is_some(), "{course}: arrival in {to}");
    }
}

#[test]
fn coin_high_scores_and_the_hundred_coin_star_are_per_course() {
    let mut s = SaveFile::in_memory(1);
    s.collect_star("jrb", 7, 120);
    assert!(s.has_star("jrb", 7) && !s.has_star("bob", 7));
    assert_eq!(s.best_coins("jrb"), 120);
    s.collect_star("jrb", 1, 90);
    assert_eq!(s.best_coins("jrb"), 120, "a lower run does not replace the record");
    s.collect_star("cotmc", 1, 255);
    assert_eq!(s.best_coins("cotmc"), 0, "only the 15 score courses keep a record");
}
