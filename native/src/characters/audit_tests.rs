//! Per-course audit of the playable characters (adult Link, Young Link, Banjo): every catalogue
//! course is loaded headless, ticked for a few hundred frames with a seeded controller and the
//! Z-focus button held, with the camera rig following. Checks: no panic, finite state, a floor under
//! the spawn, a sane camera (finite, not inside Mario, not absurdly far, looking at him) and that
//! Link's focus actually targets every opponent put in front of him (objects and walking goombas).
//! `AUDIT_ONLY=<course>` restricts the run, `AUDIT_VERBOSE=1` prints one line per course/character.
use crate::camera::rig::{CameraRig, RigInput};
use crate::game::courses::COURSES;
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use bevy::math::DVec3;
use jumpman_sim::math::{atan2s, to_s16};
use jumpman_sim::state::*;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

#[derive(Clone, Copy, Debug)]
enum Who {
    Link,
    Banjo,
}

fn lcg(seed: u32) -> impl FnMut() -> u32 {
    let mut s = seed;
    move || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        s >> 16
    }
}

fn controller(next: &mut impl FnMut() -> u32, tick: usize, prev_down: &mut u32) -> ControllerSnapshot {
    let (x, y) = if next() % 6 == 0 { (0.0, 0.0) } else { ((next() % 129) as f64 - 64.0, (next() % 129) as f64 - 64.0) };
    let mut down = 0;
    if tick % 12 < 4 && next() % 3 != 0 {
        down |= A_BUTTON;
    }
    if next() % 100 < 25 {
        down |= B_BUTTON;
    }
    if next() % 100 < 8 {
        down |= Z_TRIG;
    }
    let c = ControllerSnapshot {
        focus_yaw: None,
        stick_x: x,
        stick_y: y,
        stick_mag: (x * x + y * y).sqrt().min(64.0),
        button_down: down,
        button_pressed: down & !*prev_down,
    };
    *prev_down = down;
    c
}

/// Runs one course as one character; returns the problems found.
fn audit_one(id: &str, who: Who, log: &mut String) -> Vec<String> {
    let mut problems = vec![];
    let tag = format!("{id}/{who:?}");
    let mut sim = match GameSim::load(&assets(), CourseRequest::new(id)) {
        Ok(s) => s,
        Err(e) => return vec![format!("{tag}: load failed: {e}")],
    };
    sim.as_link = matches!(who, Who::Link);
    sim.as_banjo = matches!(who, Who::Banjo);
    sim.focus_held = true;
    let mut rig = CameraRig::default();
    rig.snap_behind(sim.mario.face_angle[1]);
    let mut next = lcg(0xc0ffee ^ id.len() as u32 * 977 ^ id.bytes().map(u32::from).sum::<u32>() * 31);
    let mut prev_down = 0;
    let spawn = sim.mario.pos;
    let mut min_clearance = f64::INFINITY;
    let (mut cam_min, mut cam_max) = (f64::INFINITY, 0.0f64);
    for tick in 0..240 {
        // The app's camera glue sets these before each rig update.
        let m = &sim.mario;
        rig.set_water_mode(m.action & (ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER) != 0);
        rig.set_slide_mode(id == "pss" || id == "ccm" && sim.course.request.area == 2);
        rig.set_interior_mode(id == "castle_inside");
        rig.set_flight_mode(m.action == ACT_FLYING);
        let c = controller(&mut next, tick, &mut prev_down);
        sim.camera_yaw = to_s16(rig.yaw_s16() as f64) as i32;
        sim.tick(&c);
        let m = &sim.mario;
        if !m.pos.iter().all(|v| v.is_finite()) || !m.vel.iter().all(|v| v.is_finite()) {
            problems.push(format!("{tag}: non-finite Mario state at tick {tick}"));
            break;
        }
        if tick == 8 {
            let floor = m.world.find_floor(m.pos[0], m.pos[1] + 100.0, m.pos[2]);
            if floor.surface.is_none() && m.action & (ACT_FLAG_AIR | ACT_FLAG_SWIMMING) == 0 {
                problems.push(format!("{tag}: no floor under the spawn {:?}", spawn));
            }
        }
        if m.action & ACT_FLAG_AIR == 0 {
            min_clearance = min_clearance.min(m.pos[1] - m.floor_height);
        }
        let input = RigInput {
            dt: 1.0 / 30.0,
            mario_pos: DVec3::from(m.pos),
            mario_yaw: m.face_angle[1],
            moving: m.forward_vel.abs() > 2.0,
            speed: m.forward_vel,
            opponent: sim.link_opponent.map(DVec3::from),
            action: m.action,
            pole: m.used_obj.map(|o| (m.objects[o].pos[1], m.objects[o].hitbox.height)),
            mario_pitch: m.face_angle[0],
            spin: m.angle_vel[1],
        };
        rig.update(&input, &m.world);
        if !rig.eye.is_finite() || !rig.look_at.is_finite() {
            problems.push(format!("{tag}: non-finite camera at tick {tick}"));
            break;
        }
        let d = (rig.eye - DVec3::from(m.pos)).length();
        cam_min = cam_min.min(d);
        cam_max = cam_max.max(d);
        let flying = m.action == ACT_FLYING || m.action == ACT_SHOT_FROM_CANNON || m.action == ACT_IN_CANNON;
        if tick > 30 && !flying && sim.link_opponent.is_none() && !(if id.starts_with("bk_") { 15.0 } else { 30.0 }..4000.0).contains(&d) {
            problems.push(format!("{tag}: camera {d:.0} units from Mario at tick {tick} (eye {:?}, mario {:?})", rig.eye, m.pos));
            break;
        }
    }
    if min_clearance < -1.0 {
        problems.push(format!("{tag}: Mario sank {:.1} units into the floor", -min_clearance));
    }
    log.push_str(&format!("{tag}: spawn {:?} clearance {min_clearance:.1} camera {cam_min:.0}..{cam_max:.0}\n", spawn.map(|v| v.round())));
    problems
}

/// Z-focus must pick up every opponent standing in front of Link (sim objects and walking goombas),
/// and the yaw it returns must point at the opponent it chose.
fn audit_focus(id: &str) -> Vec<String> {
    use crate::camera::link_focus::FocusTarget;
    let mut problems = vec![];
    let Ok(mut sim) = GameSim::load(&assets(), CourseRequest::new(id)) else { return problems };
    sim.as_link = true;
    sim.focus_held = true;
    let goombas = crate::game::goomba::spawn_goombas(&sim.course.extras.goombas, &sim.mario.world);
    sim.focus_goombas = crate::game::goomba::focus_targets(&goombas);
    let mut candidates: Vec<(String, [f64; 3])> = vec![];
    let mut seen = std::collections::HashSet::new();
    for o in &sim.mario.objects.objects {
        let targetable = jumpman_sim::link::targeting::target_class(o) == Some(jumpman_sim::link::targeting::TargetClass::Enemy);
        if targetable && seen.insert(o.behavior.as_str()) {
            candidates.push((o.behavior.as_str().to_string(), o.pos));
        }
    }
    for g in goombas.iter().take(1) {
        candidates.push(("goomba".to_string(), g.pos.to_array()));
    }
    for (name, pos) in candidates {
        // Stand 420 units south-west of it, facing it.
        let m = &mut sim.mario;
        m.pos = [pos[0] - 300.0, pos[1], pos[2] - 300.0];
        m.face_angle[1] = atan2s(300.0, 300.0) as f64;
        for o in &mut m.objects.objects {
            o.active = true;
            o.held_state = 0;
        }
        sim.link_focus.clear();
        let yaw = sim.link_focus.acquire(&sim.mario.objects, &sim.focus_goombas, sim.mario.pos, sim.mario.face_angle[1]);
        let Some(chosen) = sim.link_focus.target else {
            problems.push(format!("{id}: Link's focus ignores {name} at {pos:?}"));
            continue;
        };
        let at = match chosen {
            FocusTarget::Object(o) => sim.mario.objects[o].pos,
            FocusTarget::Goomba(i) => sim.focus_goombas[i].unwrap().0,
        };
        let want = atan2s(at[2] - sim.mario.pos[2], at[0] - sim.mario.pos[0]) as f64;
        if to_s16(yaw - want) != 0 {
            problems.push(format!("{id}: focus yaw {yaw} does not point at its target ({want})"));
        }
    }
    problems
}

#[test]
fn every_course_works_for_link_young_link_and_banjo() {
    let only = std::env::var("AUDIT_ONLY").ok();
    let verbose = std::env::var("AUDIT_VERBOSE").is_ok();
    let mut problems = vec![];
    let mut log = String::new();
    for info in COURSES {
        if only.as_deref().is_some_and(|o| o != info.id) {
            continue;
        }
        if !crate::game::course_data::CourseData::path(&assets(), info.id, 1).is_file() && info.id != "playground" && info.id != "mushroom2d" {
            continue;
        }
        for who in [Who::Link, Who::Banjo] {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut l = String::new();
                let p = audit_one(info.id, who, &mut l);
                (p, l)
            }));
            match result {
                Ok((p, l)) => {
                    problems.extend(p);
                    log.push_str(&l);
                }
                Err(e) => problems.push(format!("{}/{who:?}: PANIC {:?}", info.id, e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())))),
            }
        }
        problems.extend(audit_focus(info.id));
    }
    if verbose {
        println!("{log}");
    }
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
}
