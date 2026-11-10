//! Link on SM64 courses: targeting, jumping and sword hits against the real course data.
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use jumpman_sim::link::items::Age;
use jumpman_sim::state::*;

fn assets() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn load(course: &str) -> Option<GameSim> {
    let mut sim = GameSim::load(&assets(), CourseRequest::new(course)).ok()?;
    sim.as_link = true;
    sim.mario.link.enable(Age::Adult);
    for _ in 0..40 {
        sim.tick(&ControllerSnapshot::default());
    }
    Some(sim)
}

fn ctl(x: f64, y: f64, down: u32, pressed: u32) -> ControllerSnapshot {
    let mag = (x * x + y * y).sqrt().min(64.0);
    ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: mag, button_down: down | pressed, button_pressed: pressed, ..Default::default() }
}

/// Walk Link in eight directions from the spawn and print everything that looks wrong.
#[test]
#[ignore]
fn walk_trace() {
    let only = std::env::var("LM_COURSE").ok();
    for course in ["bob", "wf", "ccm", "castle_grounds", "castle_inside", "kokiri_forest"] {
        if only.as_deref().is_some_and(|o| o != course) {
            continue;
        }
        for dir in 0..8 {
            let Some(mut sim) = load(course) else { continue };
            let a = dir as f64 * std::f64::consts::FRAC_PI_4;
            let (sx, sy) = (a.sin() * 64.0, a.cos() * 64.0);
            let mut prev = (sim.mario.action, sim.mario.action_arg);
            let mut prev_pos = sim.mario.pos;
            let mut air = 0;
            let mut log = vec![];
            for t in 0..600 {
                let press = if t % 97 == 96 { A_BUTTON } else { 0 };
                sim.tick(&ctl(sx, sy, 0, press));
                let m = &sim.mario;
                let now = (m.action, m.action_arg);
                let d = [m.pos[0] - prev_pos[0], m.pos[1] - prev_pos[1], m.pos[2] - prev_pos[2]];
                let floor = m.world.find_floor_height(m.pos[0], m.pos[1] + 80.0, m.pos[2]);
                if now != prev {
                    log.push(format!("t{t} {:#x}/{:#x} -> {:#x}/{:#x} pos [{:.0},{:.0},{:.0}] floor {:.0} vy {:.1} fv {:.1}", prev.0, prev.1 as i64, now.0, now.1 as i64, m.pos[0], m.pos[1], m.pos[2], floor, m.vel[1], m.forward_vel));
                }
                let h = (d[0] * d[0] + d[2] * d[2]).sqrt();
                if h > 70.0 || d[1] > 60.0 || d[1] < -90.0 {
                    log.push(format!("t{t} JUMP IN POSITION d {:?} act {:#x}", d, m.action));
                }
                if m.action & ACT_FLAG_AIR != 0 {
                    air += 1;
                    if air == 200 {
                        log.push(format!("t{t} LONG AIR act {:#x} pos {:?} vy {}", m.action, m.pos, m.vel[1]));
                    }
                } else {
                    air = 0;
                    if matches!(m.action, ACT_IDLE | ACT_WALKING) && (m.pos[1] - floor).abs() > 4.0 {
                        log.push(format!("t{t} OFF FLOOR on ground act {:#x} y {:.1} floor {:.1}", m.action, m.pos[1], floor));
                    }
                }
                prev = now;
                prev_pos = m.pos;
            }
            println!("== {course} dir {dir}: end {:?} act {:#x} health {}", sim.mario.pos.map(|v| v.round()), sim.mario.action, sim.mario.health);
            for l in log.iter().take(60) {
                println!("   {l}");
            }
            if log.len() > 60 {
                println!("   ... {} more", log.len() - 60);
            }
        }
    }
}

#[test]
#[ignore]
fn probe() {
    let course = std::env::var("LM_COURSE").unwrap_or("bob".into());
    let dir: f64 = std::env::var("LM_DIR").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);
    let from: i32 = std::env::var("LM_FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(90);
    let to: i32 = std::env::var("LM_TO").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    let Some(mut sim) = load(&course) else { return };
    if std::env::var_os("LM_MARIO").is_some() {
        sim.as_link = false;
        sim.mario.link.enabled = false;
    }
    let a = dir * std::f64::consts::FRAC_PI_4;
    let (sx, sy) = (a.sin() * 64.0, a.cos() * 64.0);
    for t in 0..to {
        let press = if t % 97 == 96 { A_BUTTON } else { 0 };
        sim.tick(&ctl(sx, sy, 0, press));
        let m = &sim.mario;
        if t >= from {
            let hit = m.world.find_floor(m.pos[0], m.pos[1], m.pos[2]);
            println!("   floor {:?} kind {:?} at-feet {:?} bound {} death {} z {:.4}", m.floor_height, m.floor.as_ref().map(|f| f.kind), hit.surface.as_ref().map(|_| hit.height), m.world.horizontal_boundary, m.death_height, m.pos[2]);
            println!("t{t} act {:#x}/{:#x} pos [{:.3},{:.3},{:.3}] vel [{:.1},{:.1},{:.1}] fv {:.1} face {:.0} ev {:?} sim {:?}", m.action, m.action_arg as i64, m.pos[0], m.pos[1], m.pos[2], m.vel[0], m.vel[1], m.vel[2], m.forward_vel, m.face_angle[1], m.events, sim.events);
        }
    }
}

#[test]
#[ignore]
fn survey() {
    for course in ["bob", "wf", "ccm", "castle_grounds", "castle_inside", "jrb", "bbh", "ssl", "lll", "hmc", "thi", "kokiri_forest"] {
        let Some(sim) = load(course) else {
            println!("{course}: not loadable");
            continue;
        };
        let mut seen = std::collections::BTreeMap::new();
        for o in &sim.mario.objects.objects {
            if !o.active || o.hitbox.interact_type == 0 {
                continue;
            }
            let e = seen.entry(o.behavior.as_str().to_string()).or_insert((0, o.hitbox.interact_type, o.hitbox.radius, o.hitbox.height, o.hitbox.damage_or_coin_value));
            e.0 += 1;
        }
        println!("== {course}: mario at {:?} act {:#x} goombas {}", sim.mario.pos, sim.mario.action, sim.course.extras.goombas.len());
        for (k, (n, it, r, h, d)) in seen {
            let bits: Vec<u32> = (0..32).filter(|b| it & (1 << b) != 0).collect();
            println!("   {k:28} x{n:3} interact bits {bits:?} r {r} h {h} dmg {d}");
        }
    }
}

// ---- Lock button ----

mod lock {
    use super::super::{LockOn, EMPTY_LOCK_GRACE};

    const DT: f32 = 1.0 / 60.0;

    /// Press Z for `frames` frames, then leave it for `after` frames; `target` says whether the sim has one.
    fn tap(lock: &mut LockOn, frames: usize, after: usize, target: bool) {
        for _ in 0..frames {
            lock.step(true, true, target, DT);
        }
        for _ in 0..after {
            lock.step(false, true, target, DT);
        }
    }

    #[test]
    fn a_tap_with_nothing_to_target_recentres_then_lets_go() {
        let mut lock = LockOn::default();
        tap(&mut lock, 2, 5, false);
        assert!(lock.locked, "the camera gets its moment behind Link");
        tap(&mut lock, 0, (EMPTY_LOCK_GRACE / DT) as usize + 2, false);
        assert!(!lock.locked, "Link is not left strafing with A turned into hops");
    }

    #[test]
    fn holding_z_with_nothing_to_target_keeps_the_stance() {
        let mut lock = LockOn::default();
        tap(&mut lock, 120, 0, false);
        assert!(lock.locked);
        tap(&mut lock, 0, 1, false);
        assert!(!lock.locked, "released as soon as the button is");
    }

    #[test]
    fn a_tap_on_a_target_stays_locked_until_it_is_gone() {
        let mut lock = LockOn::default();
        tap(&mut lock, 2, 0, false); // the sim has not ticked yet
        tap(&mut lock, 0, 300, true);
        assert!(lock.locked);
        tap(&mut lock, 0, 1, false);
        assert!(!lock.locked, "target defeated or out of range");
    }

    #[test]
    fn z_again_asks_for_the_next_target() {
        let mut lock = LockOn::default();
        tap(&mut lock, 2, 30, true);
        assert!(lock.step(true, true, true, DT), "second press on a target: switch");
        assert!(lock.locked);
        assert!(!lock.step(true, true, true, DT), "held, not pressed again");
        // The caller found no other target.
        lock.release();
        assert!(!lock.locked);
    }

    #[test]
    fn hold_mode_follows_the_button() {
        let mut lock = LockOn::default();
        assert!(!lock.step(true, false, false, DT));
        assert!(lock.locked);
        lock.step(false, false, true, DT);
        assert!(!lock.locked);
    }
}

// ---- Course data (skipped when the imported courses are not on disk) ----

use crate::camera::link_focus::FocusTarget;
use jumpman_sim::link::moves::LinkEvent;
use jumpman_sim::math::atan2s;
use jumpman_sim::object::{ObjId, ObjectBehaviorKind};

/// Stand Link `back` units in front of the first active object of `kind`, facing it. Returns its id.
fn face(sim: &mut GameSim, kind: ObjectBehaviorKind, back: f64) -> Option<ObjId> {
    let ids: Vec<ObjId> = sim.mario.objects.ids().collect();
    for id in ids {
        let o = &sim.mario.objects[id];
        if !o.active || o.behavior != kind {
            continue;
        }
        let p = o.pos;
        let m = &mut sim.mario;
        // Approach from whichever side has floor at the object's level.
        for step in 0..16 {
            let a = step as f64 * std::f64::consts::FRAC_PI_8;
            let (x, z) = (p[0] - a.sin() * back, p[2] - a.cos() * back);
            let floor = m.world.find_floor_height(x, p[1] + 150.0, z);
            if (floor - p[1]).abs() < 120.0 {
                m.pos = [x, floor, z];
                m.vel = [0.0; 3];
                m.forward_vel = 0.0;
                m.face_angle[1] = atan2s(p[2] - z, p[0] - x) as f64;
                m.set_action(ACT_IDLE, 0.0);
                return Some(id);
            }
        }
    }
    None
}

/// One tick with Z held.
fn tick_z(sim: &mut GameSim) {
    sim.focus_held = true;
    sim.tick(&ControllerSnapshot::default());
}

/// The spindrifts of Cool, Cool Mountain only bounce (INTERACT_BOUNCE_TOP): they used to be invisible to Z.
#[test]
fn z_targets_a_spindrift_on_ccm() {
    let Some(mut sim) = load("ccm") else { return };
    let Some(id) = face(&mut sim, ObjectBehaviorKind::Spindrift, 500.0) else { panic!("no reachable spindrift on ccm") };
    for _ in 0..5 {
        tick_z(&mut sim);
    }
    assert_eq!(sim.link_focus.target, Some(FocusTarget::Object(id)));
    assert!(sim.link_opponent.is_some(), "the marker has a position");
}

/// Bob-omb Battlefield: Z never grabs the rolling bowling balls, and finds the bob-ombs.
#[test]
fn z_on_bob_ignores_the_bowling_balls() {
    let Some(mut sim) = load("bob") else { return };
    if let Some(ball) = face(&mut sim, ObjectBehaviorKind::PitBowlingBall, 400.0) {
        for _ in 0..3 {
            tick_z(&mut sim);
        }
        assert_ne!(sim.link_focus.target, Some(FocusTarget::Object(ball)));
    }
    let Some(_) = face(&mut sim, ObjectBehaviorKind::Bobomb, 400.0) else { panic!("no reachable bob-omb on bob") };
    sim.link_focus.clear();
    for _ in 0..3 {
        tick_z(&mut sim);
    }
    assert!(sim.link_focus.target.is_some(), "a bob-omb 400 units ahead locks (or something closer does)");
}

/// One swing at the SM64 enemy `kind` on `course`: the blade lands, reports it for the impact effect,
/// and Link neither takes the contact damage nor Mario's punch recoil.
/// Returns whether the enemy is still there afterwards (`None`: course not on disk).
fn swing_at(course: &str, kind: ObjectBehaviorKind, back: f64) -> Option<bool> {
    let mut sim = load(course)?;
    let Some(id) = face(&mut sim, kind, back) else { panic!("no reachable {kind:?} on {course}") };
    let health = sim.mario.health;
    let start = sim.mario.pos;
    let before = sim.mario.objects[id].struck;
    sim.mario.link.take_events();
    let mut hit = None;
    for t in 0..25 {
        sim.tick(&ctl(0.0, 0.0, 0, if t == 0 { B_BUTTON } else { 0 }));
        if std::env::var_os("LM_TRACE").is_some() {
            let m = &sim.mario;
            eprintln!("t{t} act {:#x} pos {:?} fv {:.1} hp {} obj {:?} struck {}", m.action, m.pos.map(|v| v.round()), m.forward_vel, m.health, m.objects[id].pos.map(|v| v.round()), m.objects[id].struck);
        }
        for e in sim.mario.link.take_events() {
            if let LinkEvent::HitObject { id: o, pos, dmg } = e {
                if o == id.0 {
                    hit.get_or_insert((t, pos, dmg));
                }
            }
        }
    }
    let o = &sim.mario.objects[id];
    assert!(o.struck > before || !o.active, "{kind:?} on {course} was never attacked");
    assert!(o.struck <= before + 1, "one swing is one blow, not {}", o.struck - before);
    let alive = o.active;
    let (_, pos, dmg) = hit.unwrap_or_else(|| panic!("no HitObject for {kind:?} on {course}"));
    assert!(dmg.is_sword());
    let d = ((pos[0] - start[0]).powi(2) + (pos[2] - start[2]).powi(2)).sqrt();
    assert!(d < back + 200.0, "the impact is at the enemy, not at {pos:?}");
    assert_eq!(sim.mario.health, health, "striking from the blade's reach costs no health");
    let moved = ((sim.mario.pos[0] - start[0]).powi(2) + (sim.mario.pos[2] - start[2]).powi(2)).sqrt();
    assert!(moved < 120.0, "Link was thrown {moved:.0} units by Mario's punch recoil: {start:?} -> {:?}, act {:#x}, {kind:?} at {:?}", sim.mario.pos, sim.mario.action, sim.mario.objects[id].pos);
    Some(alive)
}

#[test]
fn the_sword_strikes_a_spindrift() {
    assert_ne!(swing_at("ccm", ObjectBehaviorKind::Spindrift, 150.0), Some(true), "one blow defeats a spindrift");
}

#[test]
fn the_sword_strikes_a_bobomb() {
    swing_at("bob", ObjectBehaviorKind::Bobomb, 130.0);
}

#[test]
fn the_sword_strikes_a_scuttlebug() {
    swing_at("bbh", ObjectBehaviorKind::Scuttlebug, 190.0);
}

#[test]
fn the_sword_defeats_a_boo() {
    // The simplified Boo could not be beaten by anybody.
    assert_ne!(swing_at("bbh", ObjectBehaviorKind::Boo, 150.0), Some(true));
}
