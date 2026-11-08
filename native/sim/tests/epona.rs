//! Engine-free tests for Epona (`link/epona.rs`): gait speeds against the decomp constants, carrots,
//! jumps, stopping, the mount / dismount state machine on a real `MarioRuntime`, summoning, water, slopes.
use jumpman_sim::collision::*;
use jumpman_sim::link::combat::OOT_SCALE;
use jumpman_sim::link::epona::*;
use jumpman_sim::link::moves::*;
use jumpman_sim::link::ocarina::Note;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::state::*;
use std::sync::Arc;

fn quad(w: &mut CollisionWorld, a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) {
    w.add_quad(a, b, c, d, &SurfaceOptions::default());
}

/// A flat floor at y = 0 spanning +-8000 (the `link_controller` tests use the same).
fn flat() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    quad(&mut w, [-30000.0, 0.0, -30000.0], [-30000.0, 0.0, 30000.0], [30000.0, 0.0, 30000.0], [30000.0, 0.0, -30000.0]);
    w
}

/// Ground with a wall-and-top fence `height` units tall across z = `at`, width 3000.
fn fence(height: f64, at: f64) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let t = 60.0;
    quad(&mut w, [-30000.0, 0.0, -30000.0], [-30000.0, 0.0, at], [30000.0, 0.0, at], [30000.0, 0.0, -30000.0]);
    quad(&mut w, [-30000.0, 0.0, at + 2.0 * t], [-30000.0, 0.0, 30000.0], [30000.0, 0.0, 30000.0], [30000.0, 0.0, at + 2.0 * t]);
    quad(&mut w, [-30000.0, 0.0, at + 2.0 * t], [-30000.0, 0.0, 30000.0], [30000.0, 0.0, 30000.0], [30000.0, 0.0, at + 2.0 * t]);
    // Top of the fence and its two faces (normals toward the approach / away).
    quad(&mut w, [-3000.0, height, at], [-3000.0, height, at + t], [3000.0, height, at + t], [3000.0, height, at]);
    quad(&mut w, [-3000.0, 0.0, at], [-3000.0, height, at], [3000.0, height, at], [3000.0, 0.0, at]);
    quad(&mut w, [-3000.0, 0.0, at + t], [3000.0, 0.0, at + t], [3000.0, height, at + t], [-3000.0, height, at + t]);
    // ground behind the fence
    quad(&mut w, [-30000.0, 0.0, at + t], [-30000.0, 0.0, 30000.0], [30000.0, 0.0, 30000.0], [30000.0, 0.0, at + t]);
    w
}

/// Ground that ends at z = `from` and resumes at z = `to` (a gap with no floor).
fn gap(from: f64, to: f64) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    quad(&mut w, [-30000.0, 0.0, -30000.0], [-30000.0, 0.0, from], [30000.0, 0.0, from], [30000.0, 0.0, -30000.0]);
    quad(&mut w, [-30000.0, 0.0, to], [-30000.0, 0.0, 30000.0], [30000.0, 0.0, 30000.0], [30000.0, 0.0, to]);
    w
}

fn riding_epona(world: &CollisionWorld) -> Epona {
    let mut e = Epona::default();
    e.place(world, [0.0, 0.0, 0.0], 0.0);
    e.begin_mount(-1);
    let link = e.rider_point();
    let mut t = 0;
    while !e.riding() {
        e.step(world, link, &RideInput::default());
        t += 1;
        assert!(t < 400, "mounting never finished");
    }
    e
}

fn run(e: &mut Epona, world: &CollisionWorld, input: RideInput, ticks: usize) {
    let link = e.rider_point();
    for _ in 0..ticks {
        e.step(world, link, &input);
    }
}

const FORWARD: RideInput = RideInput { stick_x: 0.0, stick_y: 64.0, a_pressed: false, b_pressed: false };
const NOTHING: RideInput = RideInput { stick_x: 0.0, stick_y: 0.0, a_pressed: false, b_pressed: false };
const CARROT: RideInput = RideInput { stick_x: 0.0, stick_y: 64.0, a_pressed: true, b_pressed: false };

#[test]
fn gallop_speed_is_eight_and_a_carrot_reaches_fourteen() {
    let w = flat();
    let mut e = riding_epona(&w);
    run(&mut e, &w, FORWARD, 300);
    assert_eq!(e.act, Act::Gallop);
    // `EnHorse_UpdateSpeed` hovers around base * stick / 54 (stick capped at 60) = 8.9, never below the base 8.
    let z0 = e.pos[2];
    let mut sum = 0.0;
    for _ in 0..90 {
        run(&mut e, &w, FORWARD, 1);
        assert!(e.speed >= 8.0 - 1e-9 && e.speed < 9.6, "gallop speed {}", e.speed);
        sum += e.speed;
    }
    let mean = sum / 90.0;
    // Sim units per tick = speed * 4.4 / 1.5.
    let per_tick = (e.pos[2] - z0) / 90.0;
    assert!((per_tick - mean * OOT_SCALE / 1.5).abs() < 0.5, "{per_tick} vs mean speed {mean}");

    let mut peak = 0.0f64;
    run(&mut e, &w, CARROT, 1);
    for _ in 0..60 {
        run(&mut e, &w, FORWARD, 1);
        peak = peak.max(e.speed);
    }
    assert!(peak > 13.5 && peak <= 14.0 + 1e-6, "boost speed {peak}");
    assert_eq!(e.boosts, 5);
}

#[test]
fn half_stick_walks_and_trots() {
    let w = flat();
    let mut e = riding_epona(&w);
    // A light stick walks at base 3 * stick / 54; only a nearly full stick pushes past 3 into the trot and gallop.
    let walk = RideInput { stick_y: 30.0, ..NOTHING };
    run(&mut e, &w, walk, 300);
    assert!(matches!(e.act, Act::Walk) && e.speed <= 3.0 + 1e-9 && e.speed > 1.0, "{:?} {}", e.act, e.speed);
    run(&mut e, &w, FORWARD, 60);
    assert!(matches!(e.act, Act::Trot | Act::Gallop) && e.speed > 3.0, "{:?} {}", e.act, e.speed);
}

#[test]
fn six_carrots_with_cooldowns() {
    let w = flat();
    let mut e = riding_epona(&w);
    assert_eq!(e.boosts, 6);
    run(&mut e, &w, FORWARD, 300);
    // Spend all six: a carrot can only be used while no boost is in progress.
    let mut spent = 0;
    for _ in 0..2000 {
        let before = e.boosts;
        run(&mut e, &w, CARROT, 1);
        if e.boosts < before {
            spent += 1;
        }
        if e.boosts == 0 {
            break;
        }
        run(&mut e, &w, FORWARD, 1);
    }
    assert_eq!(e.boosts, 0, "spent {spent}");
    assert_eq!(spent, 6);
    // With all spent the whole set returns after 140 OoT frames (210 ticks), not before.
    run(&mut e, &w, FORWARD, 150);
    assert_eq!(e.boosts, 0);
    run(&mut e, &w, FORWARD, 80);
    assert_eq!(e.boosts, 6);
    // One carrot: back after 60 frames (90 ticks).
    run(&mut e, &w, CARROT, 1);
    assert_eq!(e.boosts, 5);
    run(&mut e, &w, FORWARD, 80);
    assert_eq!(e.boosts, 5);
    run(&mut e, &w, FORWARD, 20);
    assert_eq!(e.boosts, 6);
}

#[test]
fn stopping_at_a_gallop_skids_to_a_halt() {
    let w = flat();
    let mut e = riding_epona(&w);
    run(&mut e, &w, FORWARD, 300);
    let z0 = e.pos[2];
    let back = RideInput { stick_y: -64.0, ..NOTHING };
    // Holding the stick back brakes 0.3 per frame; at the end of a stride a stick pulled back starts the skid.
    run(&mut e, &w, back, 40);
    let mut stopped_at = None;
    for t in 0..400 {
        run(&mut e, &w, NOTHING, 1);
        if e.speed == 0.0 && stopped_at.is_none() {
            stopped_at = Some(t);
        }
    }
    assert!(stopped_at.is_some(), "never stopped, speed {}", e.speed);
    let dist = (e.pos[2] - z0) / OOT_SCALE;
    assert!(dist > 20.0 && dist < 500.0, "stopping distance {dist} OoT units");
    assert!(e.speed == 0.0);
}

#[test]
fn low_fence_is_jumped_at_a_gallop() {
    let w = fence(30.0 * OOT_SCALE, 1500.0);
    let mut e = riding_epona(&w);
    let mut jumped = false;
    for _ in 0..600 {
        run(&mut e, &w, FORWARD, 1);
        jumped |= e.jumping;
        if e.pos[2] > 2200.0 {
            break;
        }
    }
    assert!(jumped, "never jumped, at z={} {:?}", e.pos[2], e.act);
    assert!(e.pos[2] > 1800.0, "stuck at the fence: z={}", e.pos[2]);
}

#[test]
fn tall_fence_needs_a_carrot() {
    let tall = 60.0 * OOT_SCALE;
    let w = fence(tall, 4000.0);
    // Without: she brakes / rears and never gets over.
    let mut e = riding_epona(&w);
    for _ in 0..900 {
        run(&mut e, &w, FORWARD, 1);
    }
    assert!(e.pos[2] < 4000.0, "went through the fence: {}", e.pos[2]);
    // With a carrot just before: fast enough for the high jump.
    let mut e = riding_epona(&w);
    run(&mut e, &w, FORWARD, 130);
    let mut jumped = false;
    for _ in 0..900 {
        let near = e.pos[2] > 4000.0 - 1700.0 && e.boosts == 6 && !e.boost;
        run(&mut e, &w, if near { CARROT } else { FORWARD }, 1);
        jumped |= e.jumping && e.act == Act::HighJump;
        if e.pos[2] > 5200.0 {
            break;
        }
    }
    assert!(jumped, "no high jump at z={} speed {}", e.pos[2], e.speed);
    assert!(e.pos[2] > 4300.0, "z={}", e.pos[2]);
}

#[test]
fn gallop_jumps_a_gap() {
    let w = gap(1500.0, 2300.0);
    let mut e = riding_epona(&w);
    let mut jumped = false;
    for _ in 0..800 {
        run(&mut e, &w, FORWARD, 1);
        jumped |= e.jumping;
        assert!(e.pos[1] > -1.0, "fell into the gap at z={}", e.pos[2]);
        if e.pos[2] > 3200.0 {
            break;
        }
    }
    assert!(jumped && e.pos[2] > 2300.0, "z={} {:?}", e.pos[2], e.act);
}

#[test]
fn walking_into_a_cliff_stops_her_and_does_not_fall() {
    let w = gap(1500.0, 99999.0);
    let mut e = riding_epona(&w);
    let walk = RideInput { stick_y: 30.0, ..NOTHING };
    for _ in 0..1500 {
        run(&mut e, &w, walk, 1);
        assert!(e.pos[1] > -1.0);
    }
    assert!(e.pos[2] < 1500.0);
}

#[test]
fn water_blocks_her() {
    let mut w = flat();
    w.water_boxes.push(WaterBox(0.0, -3000.0, 1500.0, 3000.0, 5000.0, 400.0));
    let mut e = riding_epona(&w);
    for _ in 0..1200 {
        run(&mut e, &w, FORWARD, 1);
    }
    assert!(e.pos[2] < 1500.0, "rode into the water: z={}", e.pos[2]);
}

#[test]
fn steep_slopes_are_not_climbed() {
    // A 55 degree ramp starting at z = 1000.
    let mut w = CollisionWorld::default();
    quad(&mut w, [-30000.0, 0.0, -30000.0], [-30000.0, 0.0, 1000.0], [30000.0, 0.0, 1000.0], [30000.0, 0.0, -30000.0]);
    let rise = 2000.0 * 55f64.to_radians().tan();
    quad(&mut w, [-30000.0, 0.0, 1000.0], [-30000.0, rise, 3000.0], [30000.0, rise, 3000.0], [30000.0, 0.0, 1000.0]);
    let mut e = riding_epona(&w);
    let mut top = 0.0f64;
    for _ in 0..900 {
        run(&mut e, &w, FORWARD, 1);
        top = top.max(e.pos[1]);
    }
    assert!(top < 300.0, "climbed a 55 degree slope to y={top}");
    // A gentle 15 degree ramp is fine.
    let mut w = CollisionWorld::default();
    quad(&mut w, [-30000.0, 0.0, -30000.0], [-30000.0, 0.0, 1000.0], [30000.0, 0.0, 1000.0], [30000.0, 0.0, -30000.0]);
    let rise = 3000.0 * 15f64.to_radians().tan();
    quad(&mut w, [-30000.0, 0.0, 1000.0], [-30000.0, rise, 4000.0], [30000.0, rise, 4000.0], [30000.0, 0.0, 1000.0]);
    let mut e = riding_epona(&w);
    let mut top = 0.0f64;
    for _ in 0..300 {
        run(&mut e, &w, FORWARD, 1);
        top = top.max(e.pos[1]);
    }
    assert!(top > 400.0, "did not climb the gentle slope: y={top}");
}

// ---- the state machine on a real MarioRuntime --------------------------------------------------

fn link_on(world: CollisionWorld) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.link.enable(jumpman_sim::link::items::Age::Adult);
    m
}

fn tick(m: &mut MarioRuntime, x: f64, y: f64, pressed: u32) {
    let mag = (x * x + y * y).sqrt().min(64.0);
    let c = ControllerSnapshot { focus_yaw: None, stick_x: x, stick_y: y, stick_mag: mag, button_down: pressed, button_pressed: pressed };
    m.step(&c, 0.0, true, false);
}

#[test]
fn mount_ride_and_dismount() {
    let mut m = link_on(flat());
    let world = m.world.clone();
    m.link.epona.place(&world, [150.0, 0.0, 0.0], 0.0);
    // Pressing A far away does nothing.
    m.set_action(ACT_IDLE, 0.0);
    m.pos = [-2000.0, 0.0, 0.0];
    tick(&mut m, 0.0, 0.0, A_BUTTON);
    assert!(!m.link.epona.is_mounted());
    // Next to her left flank (Epona faces +z: her left is +x), within 75 units: A mounts.
    m.pos = [150.0 - 200.0, 0.0, 0.0];
    assert_eq!(m.link.epona.mount_side(m.pos), Some(1), "right flank is x < 0");
    m.pos = [150.0 + 200.0, 0.0, 0.0];
    assert_eq!(m.link.epona.mount_side(m.pos), Some(-1));
    tick(&mut m, 0.0, 0.0, 0);
    tick(&mut m, 0.0, 0.0, A_BUTTON);
    assert!(matches!(m.link.epona.rider, Rider::Mounting { side: -1, .. }), "{:?}", m.link.epona.rider);
    assert_eq!(m.link.pose.map(|p| p.clip), Some("uma_mountLeft"));
    // 70 OoT frames of climbing = 105 ticks.
    for _ in 0..110 {
        tick(&mut m, 0.0, 0.0, 0);
    }
    assert!(m.link.epona.riding());
    assert!((m.pos[1] - (m.link.epona.pos[1] + RIDER_HEIGHT)).abs() < 1e-6, "Link sits on the saddle");
    // Ride a bit; A while she moves is a carrot, not a dismount.
    for _ in 0..200 {
        tick(&mut m, 0.0, 64.0, 0);
    }
    assert!(m.link.epona.speed > 3.0);
    tick(&mut m, 0.0, 64.0, A_BUTTON);
    assert!(m.link.epona.riding() && m.link.epona.boosts == 5);
    // Stop her, then A gets off on the side he came from.
    for _ in 0..500 {
        tick(&mut m, 0.0, 0.0, 0);
    }
    assert!(m.link.epona.can_dismount());
    tick(&mut m, 0.0, 0.0, A_BUTTON);
    assert!(matches!(m.link.epona.rider, Rider::Dismounting { side: -1, .. }));
    for _ in 0..110 {
        tick(&mut m, 0.0, 0.0, 0);
    }
    assert_eq!(m.link.epona.rider, Rider::None);
    let epona = m.link.epona.pos;
    let d = ((m.pos[0] - epona[0]).powi(2) + (m.pos[2] - epona[2]).powi(2)).sqrt();
    assert!(d > 100.0 && d < 600.0 && m.pos[1] < epona[1] + 5.0, "Link stands {d} from her at y {}", m.pos[1]);
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn epona_song_summons_her_and_she_arrives() {
    let mut m = link_on(flat());
    assert!(!m.link.epona.present);
    // Play Epona's Song (U L R U L R = C-up, C-left, C-right, ...) through the ocarina recorder.
    let song = [Note::CUp, Note::CLeft, Note::CRight, Note::CUp, Note::CLeft, Note::CRight];
    let bit = |n: Note| match n {
        Note::A => A_BUTTON,
        Note::CDown => C_DOWN,
        Note::CRight => C_RIGHT,
        Note::CLeft => C_LEFT,
        Note::CUp => C_UP,
    };
    tick(&mut m, 0.0, 0.0, C_UP); // opens the ocarina
    for _ in 0..30 {
        tick(&mut m, 0.0, 0.0, 0);
    }
    for n in song {
        tick(&mut m, 0.0, 0.0, bit(n));
        tick(&mut m, 0.0, 0.0, 0);
    }
    assert!(m.link.epona.present, "Epona did not appear");
    let start = ((m.link.epona.pos[0] - m.pos[0]).powi(2) + (m.link.epona.pos[2] - m.pos[2]).powi(2)).sqrt();
    assert!(start > 400.0 * OOT_SCALE * 0.9, "appears at the edge of view: {start}");
    assert_eq!(m.link.epona.act, Act::Follow);
    assert_eq!(m.link.epona.anim, Anim::Gallop);
    let mut closest = f64::MAX;
    let mut gaits = std::collections::BTreeSet::new();
    m.link.mv = Mv::Free;
    m.set_action(ACT_IDLE, 0.0);
    for _ in 0..1500 {
        tick(&mut m, 0.0, 0.0, 0);
        gaits.insert(m.link.epona.anim.index());
        let e = m.link.epona.pos;
        closest = closest.min(((e[0] - m.pos[0]).powi(2) + (e[2] - m.pos[2]).powi(2)).sqrt());
    }
    assert!(closest < 100.0 * OOT_SCALE, "she never reached Link: {closest}");
    assert_eq!(m.link.epona.act, Act::Idle, "she stands once she arrives");
    assert!(gaits.contains(&(Anim::Gallop as usize)) && gaits.contains(&(Anim::Trot as usize)) && gaits.contains(&(Anim::Walk as usize)));
}

#[test]
fn real_jump_profiles_load_when_assets_exist() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public/models/oot/epona.json");
    let Ok(bytes) = std::fs::read(path) else { return };
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let ys = |name: &str| -> Vec<f64> { v["clips"][name].as_array().unwrap().iter().map(|f| f[1].as_f64().unwrap() * 0.01).collect() };
    let (low, high) = (ys("jump"), ys("jumpHigh"));
    assert_eq!((low.len(), high.len()), (23, 30));
    assert!(high.iter().cloned().fold(0.0, f64::max) > low.iter().cloned().fold(0.0, f64::max));
    // The animation lengths used by the state machine are the exported ones.
    for (i, name) in ["idle", "whinny", "refuse", "rearing", "walk", "trot", "gallop", "jump", "jumpHigh"].iter().enumerate() {
        assert_eq!(v["clips"][name].as_array().unwrap().len() as f64, ANIM_LEN[i], "{name}");
    }
    let mut w = fence(30.0 * OOT_SCALE, 1500.0);
    let mut e = Epona::default();
    e.set_jump_profiles(low, high);
    e.place(&w, [0.0, 0.0, 0.0], 0.0);
    e.begin_mount(-1);
    let link = e.rider_point();
    while !e.riding() {
        e.step(&w, link, &RideInput::default());
    }
    let mut peak = 0.0f64;
    for _ in 0..600 {
        e.step(&w, link, &FORWARD);
        peak = peak.max(e.pos[1]);
        if e.pos[2] > 2200.0 {
            break;
        }
    }
    assert!(e.pos[2] > 1800.0 && peak > 40.0 * OOT_SCALE, "z={} peak {peak}", e.pos[2]);
    w.clear();
}
