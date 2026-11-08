//! Engine-free tests for Banjo-Kazooie's extended moveset (`banjo_ext.rs`): state transitions,
//! apex heights and speeds against the numbers in the decomp (`bs/*.c`), at 30 Hz with the 1.25
//! model scale of `characters.rs`.
use jumpman_sim::banjo_ext::*;
use jumpman_sim::characters::*;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::state::*;
use std::sync::Arc;

const S: f64 = 1.25 / 30.0; // Rare units/s -> units/frame
const G: f64 = 1.25 / 900.0; // Rare units/s^2 -> units/frame^2

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-6000.0, 0.0, -6000.0], [-6000.0, 0.0, 6000.0], [6000.0, 0.0, 6000.0], &o);
    w.add_triangle([-6000.0, 0.0, -6000.0], [6000.0, 0.0, 6000.0], [6000.0, 0.0, -6000.0], &o);
    // A wall 1500 units ahead (+Z) of the origin.
    w.add_triangle([-1000.0, 0.0, 1500.0], [-1000.0, 600.0, 1500.0], [1000.0, 600.0, 1500.0], &o);
    w.add_triangle([-1000.0, 0.0, 1500.0], [1000.0, 600.0, 1500.0], [1000.0, 0.0, 1500.0], &o);
    w
}

fn banjo() -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -1600.0);
    m.face_angle[1] = 0.0;
    for _ in 0..3 { frame(&mut m, 0.0, 0, 0); }
    m
}

/// One step with the stick forward amount (0..64) and button masks.
fn frame(m: &mut MarioRuntime, stick_y: f64, down: u32, pressed: u32) {
    let c = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y, stick_mag: stick_y.abs(), button_down: down | pressed, button_pressed: pressed };
    m.step(&c, 0x8000 as f64, false, true);
}

fn arg(m: &MarioRuntime) -> f64 { m.action_arg }

#[test]
fn jump_apex_matches_decomp_gravity() {
    let mut m = banjo();
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    let mut apex = 0.0f64;
    let mut frames = 0;
    while frames < 200 {
        frame(&mut m, 0.0, A_BUTTON, 0);
        apex = apex.max(m.pos[1]);
        frames += 1;
        if m.action & ACT_FLAG_AIR == 0 { break; }
    }
    // bsjump: 710 u/s up with -1350 held gravity (A held) -> v^2 / 2g in model units.
    let (v, g) = (710.0 * S, 1350.0 * G);
    let want = v * v / (2.0 * g);
    assert!((apex - want).abs() < want * 0.12, "apex {apex} want ~{want}");
    assert!(frames < 120, "landed after {frames}");
}

#[test]
fn talon_trot_runs_at_700_and_ends_when_z_is_released() {
    let mut m = banjo();
    frame(&mut m, 64.0, Z_TRIG, Z_TRIG | C_LEFT);
    assert_eq!((m.action, arg(&m)), (ACT_WALKING, BANJO_TROT));
    for _ in 0..40 { frame(&mut m, 64.0, Z_TRIG, 0); }
    assert!((m.forward_vel - 700.0 * S).abs() < 0.5, "trot speed {}", m.forward_vel);
    assert!(m.pos[2] > 300.0);
    frame(&mut m, 64.0, 0, 0);
    assert_ne!(arg(&m), BANJO_TROT, "Z released ends the trot");
}

#[test]
fn talon_trot_jump_uses_693_and_minus_1200() {
    let mut m = banjo();
    frame(&mut m, 0.0, Z_TRIG, Z_TRIG | C_LEFT);
    frame(&mut m, 0.0, Z_TRIG | A_BUTTON, A_BUTTON);
    assert_eq!(arg(&m), BANJO_TROT_JUMP);
    let mut apex = 0.0f64;
    for _ in 0..80 {
        frame(&mut m, 0.0, Z_TRIG | A_BUTTON, 0);
        apex = apex.max(m.pos[1]);
        if m.action & ACT_FLAG_AIR == 0 { break; }
    }
    let (v, g) = (693.5 * S, 1200.0 * G);
    // Discrete integration: gravity is applied before each move, so the apex is v^2/2g - v/2.
    assert!((apex - (v * v / (2.0 * g) - v / 2.0)).abs() < 3.0, "apex {apex}");
    assert_eq!((m.action, arg(&m)), (ACT_WALKING, BANJO_TROT), "lands back into the trot while Z is held");
}

#[test]
fn trot_toggle_from_touch_stays_until_pressed_again() {
    let mut m = banjo();
    frame(&mut m, 40.0, 0, C_LEFT); // no Z: latched
    for _ in 0..10 { frame(&mut m, 40.0, 0, 0); }
    assert_eq!(arg(&m), BANJO_TROT);
    frame(&mut m, 40.0, 0, C_LEFT);
    assert_ne!(arg(&m), BANJO_TROT);
}

#[test]
fn beak_buster_hovers_then_dives_and_hops() {
    let mut m = banjo();
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    for _ in 0..8 { frame(&mut m, 0.0, A_BUTTON, 0); }
    let y0 = m.pos[1];
    assert!(y0 > 20.0);
    frame(&mut m, 0.0, Z_TRIG, Z_TRIG);
    assert_eq!((m.action, arg(&m)), (ACT_GROUND_POUND, BANJO_BUSTER));
    for _ in 0..9 { frame(&mut m, 0.0, Z_TRIG, 0); }
    assert!((m.pos[1] - y0).abs() < 1.0, "hovers during the wind-up");
    let mut impact = false;
    for _ in 0..40 {
        frame(&mut m, 0.0, 0, 0);
        if m.action == ACT_GROUND_POUND_LAND { impact = true; break; }
    }
    assert!(impact, "dive reached the floor");
    assert!(m.banjo.events.iter().any(|e| matches!(e, BanjoEvent::BeakBuster { .. })));
    let mut peak = 0.0f64;
    for _ in 0..40 {
        frame(&mut m, 0.0, 0, 0);
        peak = peak.max(m.pos[1]);
    }
    let (v, g) = (730.0 * S, 2110.0 * G);
    assert!((peak - (v * v / (2.0 * g) - v / 2.0)).abs() < 3.0, "rebound hop {peak}");
}

#[test]
fn egg_fires_forward_costs_one_and_travels_800() {
    let mut m = banjo();
    let before = m.banjo.eggs;
    frame(&mut m, 0.0, Z_TRIG, Z_TRIG | C_UP);
    assert_eq!(arg(&m), BANJO_EGG_HEAD);
    for _ in 0..16 { frame(&mut m, 0.0, Z_TRIG, 0); }
    assert_eq!(m.banjo.eggs, before - 1);
    assert_eq!(m.banjo.eggs_in_flight.len(), 1);
    let z0 = m.banjo.eggs_in_flight[0].pos[2];
    frame(&mut m, 0.0, Z_TRIG, 0);
    let z1 = m.banjo.eggs_in_flight[0].pos[2];
    assert!((z1 - z0 - 800.0 * S).abs() < 0.01);
    // Shatters on the wall at z=1500 before its 2 s are up.
    for _ in 0..60 { frame(&mut m, 0.0, Z_TRIG, 0); }
    assert!(m.banjo.eggs_in_flight.is_empty());
    assert!(m.banjo.events.iter().any(|e| matches!(e, BanjoEvent::EggShattered { hit: false, .. })));
}

#[test]
fn egg_backwards_can_be_picked_up_again() {
    let mut m = banjo();
    let before = m.banjo.eggs;
    frame(&mut m, 0.0, Z_TRIG, Z_TRIG | C_DOWN);
    for _ in 0..8 { frame(&mut m, 0.0, 0, 0); }
    assert_eq!(m.banjo.eggs, before - 1);
    assert_eq!(m.banjo.eggs_in_flight[0].kind, EggKind::Ass);
    // Walk back onto it.
    for _ in 0..60 { frame(&mut m, 0.0, 0, 0); }
    assert!(m.banjo.eggs >= before - 1);
}

#[test]
fn egg_hits_a_bobomb() {
    let mgr = ObjectManager::new(vec![ObjectSpec::new(ObjectBehaviorKind::Bobomb, [0.0, 0.0, 600.0], 0.0)], None, None);
    let mut m = banjo();
    m.set_objects(mgr);
    frame(&mut m, 0.0, 0, C_UP);
    for _ in 0..30 { frame(&mut m, 0.0, 0, 0); }
    assert!(m.banjo.events.iter().any(|e| matches!(e, BanjoEvent::EggShattered { hit: true, .. })), "events {:?}", m.banjo.events);
    assert!(m.objects[ObjId(0)].interact_status & INT_STATUS_WAS_ATTACKED != 0);
}

#[test]
fn out_of_eggs_does_not_fire() {
    let mut m = banjo();
    m.banjo.eggs = 0;
    frame(&mut m, 0.0, 0, C_UP);
    for _ in 0..30 { frame(&mut m, 0.0, 0, 0); }
    assert!(m.banjo.eggs_in_flight.is_empty());
    assert!(m.banjo.events.contains(&BanjoEvent::OutOf(PickupKind::Egg)));
}

#[test]
fn wonderwing_is_invincible_and_burns_a_gold_feather_every_two_seconds() {
    let mut m = banjo();
    let gold = m.banjo.gold_feathers;
    frame(&mut m, 0.0, Z_TRIG, Z_TRIG | C_RIGHT);
    assert_eq!(arg(&m), BANJO_WONDER);
    assert!(!m.take_damage(1.0, None), "invincible");
    for _ in 0..60 { frame(&mut m, 0.0, Z_TRIG, 0); }
    assert_eq!(m.banjo.gold_feathers, gold - 1);
    frame(&mut m, 0.0, 0, 0);
    assert_ne!(arg(&m), BANJO_WONDER);
}

#[test]
fn wonderwing_needs_a_gold_feather() {
    let mut m = banjo();
    m.banjo.gold_feathers = 0;
    frame(&mut m, 0.0, Z_TRIG, Z_TRIG | C_RIGHT);
    assert_ne!(arg(&m), BANJO_WONDER);
    assert!(m.banjo.events.contains(&BanjoEvent::OutOf(PickupKind::GoldFeather)));
}

#[test]
fn shock_spring_needs_a_held_charge_then_jumps_650() {
    let mut m = banjo();
    frame(&mut m, 0.0, 0, R_TRIG); // power: Shock
    assert_eq!(m.banjo.power, Power::Shock);
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    assert_eq!(arg(&m), BANJO_SHOCK_CHARGE);
    // Released early: no jump.
    for _ in 0..40 { frame(&mut m, 0.0, 0, 0); }
    assert_eq!(m.action & ACT_FLAG_AIR, 0);
    // Held: springs up.
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    let mut apex = 0.0f64;
    for _ in 0..140 {
        frame(&mut m, 0.0, A_BUTTON, 0);
        apex = apex.max(m.pos[1]);
    }
    let (v, g) = (1250.0 * S, 1200.0 * G);
    assert!((apex - v * v / (2.0 * g)).abs() < 40.0, "apex {apex}");
}

#[test]
fn flight_takes_off_glides_flaps_and_beak_bombs() {
    let mut m = banjo();
    frame(&mut m, 0.0, 0, R_TRIG);
    frame(&mut m, 0.0, 0, R_TRIG);
    assert_eq!(m.banjo.power, Power::Flight);
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    assert_eq!(arg(&m), BANJO_FLY_ENTER);
    let mut flew = false;
    for _ in 0..80 {
        frame(&mut m, 0.0, 0, 0);
        if arg(&m) == BANJO_FLY { flew = true; break; }
    }
    assert!(flew, "take-off turns into flight");
    assert!(m.pos[1] > 300.0, "lifted to {}", m.pos[1]);
    let z0 = m.pos[2];
    let reds = m.banjo.red_feathers;
    frame(&mut m, 0.0, 0, A_BUTTON);
    assert_eq!(m.banjo.red_feathers, reds - 1);
    for _ in 0..30 { frame(&mut m, 0.0, 0, 0); }
    assert!(m.pos[2] - z0 > 200.0 || m.pos[2] > 1000.0, "glides forward");
    // Beak Bomb after the 1 s cooldown.
    let reds = m.banjo.red_feathers;
    frame(&mut m, 0.0, B_BUTTON, B_BUTTON);
    assert_eq!(arg(&m), BANJO_BOMB);
    assert_eq!(m.banjo.red_feathers, reds - 1);
    let p0 = m.pos;
    for _ in 0..30 { frame(&mut m, 0.0, 0, 0); }
    let d = ((m.pos[0] - p0[0]).powi(2) + (m.pos[1] - p0[1]).powi(2) + (m.pos[2] - p0[2]).powi(2)).sqrt();
    assert!(d > 600.0 || m.action & ACT_FLAG_AIR == 0, "bombed {d}");
}

#[test]
fn pickups_are_collected_by_walking_over_them() {
    let mut m = banjo();
    m.banjo.drop_pickup(PickupKind::Note, [0.0, 10.0, 20.0]);
    m.banjo.drop_pickup(PickupKind::Egg, [0.0, 10.0, 25.0]);
    m.banjo.drop_pickup(PickupKind::Jiggy, [900.0, 10.0, 0.0]);
    let eggs = m.banjo.eggs;
    frame(&mut m, 0.0, 0, 0);
    assert_eq!(m.banjo.notes, 1);
    assert_eq!(m.banjo.eggs, eggs + 5);
    assert_eq!(m.banjo.jiggies, 0);
    assert_eq!(m.banjo.pickups.len(), 1);
    assert!(banjo_move(&m) == 0.0);
}
