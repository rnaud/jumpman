//! Link's dive (`link/dive.rs`): A at the surface goes under, holding A sinks until the Zora Scale's depth, letting go turns him
//! up and he comes out of the water.
use jumpman_sim::collision::*;
use jumpman_sim::link::items::*;
use jumpman_sim::link::moves::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::state::*;
use std::sync::Arc;

const U: f64 = 4.4;

fn pool() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    let r = 9000.0;
    w.add_triangle([-r, -6000.0, -r], [-r, -6000.0, r], [r, -6000.0, r], &o);
    w.add_triangle([-r, -6000.0, -r], [r, -6000.0, r], [r, -6000.0, -r], &o);
    w.water_boxes.push(WaterBox(0.0, -9500.0, -9500.0, 9500.0, 9500.0, 0.0));
    w
}

fn swimmer(scale: u8) -> MarioRuntime {
    jumpman_sim::actions::water::set_faithful(true);
    let spawn = SpawnPoint { action: Some(ACT_WATER_IDLE), pos: [0.0, -150.0, 0.0], face_yaw: 0.0 };
    let mut m = MarioRuntime::new(Arc::new(pool()), spawn, -20000.0);
    m.set_objects(ObjectManager::new(vec![], None, None));
    m.link.enable(Age::Adult);
    m.link.inv.scale = scale;
    m
}

fn tick(m: &mut MarioRuntime, a_down: bool, a_pressed: bool) {
    let c = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: 0.0, stick_mag: 0.0, button_down: if a_down { A_BUTTON } else { 0 }, button_pressed: if a_pressed { A_BUTTON } else { 0 } };
    m.step(&c, 32768.0, true, false);
}

fn depth(m: &MarioRuntime) -> f64 {
    -m.pos[1] / U
}

/// Dive and hold A for `ticks`; returns the deepest point in OoT units.
fn dive_and_hold(scale: u8, ticks: usize) -> (f64, MarioRuntime) {
    let mut m = swimmer(scale);
    for _ in 0..3 {
        tick(&mut m, false, false);
    }
    assert!(m.link.can_dive(&m), "swimming at the surface with boots on");
    tick(&mut m, true, true);
    assert_eq!(m.link.mv, Mv::Dive);
    let mut deepest = depth(&m);
    for _ in 0..ticks {
        tick(&mut m, true, false);
        deepest = deepest.max(depth(&m));
    }
    (deepest, m)
}

#[test]
fn the_zora_scale_sets_how_deep_a_held_dive_goes() {
    let (none, _) = dive_and_hold(0, 900);
    let (silver, _) = dive_and_hold(1, 900);
    let (gold, _) = dive_and_hold(2, 900);
    // sinking 2 units a frame, the dive stops going down once the depth passes the limit (momentum adds a few units)
    assert!((120.0..135.0).contains(&none), "no scale: {none}");
    assert!((240.0..255.0).contains(&silver), "silver: {silver}");
    assert!((360.0..375.0).contains(&gold), "gold: {gold}");
}

#[test]
fn letting_go_turns_him_up_and_he_breaks_the_surface() {
    let (deep, mut m) = dive_and_hold(1, 400);
    assert!(deep > 200.0);
    // release A: the pitch comes back (15 frames), then he rises at depth * 0.018 + 4 a frame
    let mut out = false;
    for _ in 0..900 {
        tick(&mut m, false, false);
        if m.link.mv != Mv::Dive {
            out = true;
            break;
        }
    }
    assert!(out, "still diving at {} units", depth(&m));
    assert!(depth(&m) < 70.0, "surfaced: {}", depth(&m));
    assert_eq!(m.action & ACT_FLAG_SWIMMING, ACT_FLAG_SWIMMING, "back to the surface swim");
}

#[test]
fn the_iron_boots_do_not_dive_and_a_tap_of_a_only_kicks_off_and_comes_back() {
    let mut m = swimmer(2);
    m.link.inv.boots = Boots::Iron;
    for _ in 0..3 {
        tick(&mut m, false, false);
    }
    assert!(!m.link.can_dive(&m));
    // a tap: A is let go before frame 22 of the start animation, so he settles at once and rises
    let mut m = swimmer(2);
    for _ in 0..3 {
        tick(&mut m, false, false);
    }
    tick(&mut m, true, true);
    for _ in 0..4 {
        tick(&mut m, false, false);
    }
    assert_eq!(m.link.mv, Mv::Dive);
    let mut deepest: f64 = 0.0;
    for _ in 0..900 {
        tick(&mut m, false, false);
        deepest = deepest.max(depth(&m));
        if m.link.mv != Mv::Dive {
            break;
        }
    }
    assert_eq!(m.link.mv, Mv::Free);
    assert!(deepest < 120.0, "a tap does not take him far: {deepest}");
}
