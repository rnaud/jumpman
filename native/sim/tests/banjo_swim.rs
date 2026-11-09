//! Banjo-Kazooie's air meter (`ITEM_17_AIR`, `code_BEF20.c`): drains under water, refills on the
//! surface, drowns Banjo at zero. Worlds are synthetic pools, like `water.rs`.
use jumpman_sim::actions::water;
use jumpman_sim::banjo_ext::*;
use jumpman_sim::collision::*;
use jumpman_sim::math::Vec3;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object_manager::{self, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;

fn pool() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    let r = 6000.0;
    w.add_triangle([-r, -4000.0, -r], [-r, -4000.0, r], [r, -4000.0, r], &o);
    w.add_triangle([-r, -4000.0, -r], [r, -4000.0, r], [r, -4000.0, -r], &o);
    w.water_boxes.push(WaterBox(0.0, -7000.0, -7000.0, 7000.0, 7000.0, 0.0));
    w
}

fn banjo_at(pos: Vec3, action: u32) -> MarioRuntime {
    water::set_faithful(true);
    let spawn = SpawnPoint { action: Some(action), pos, face_yaw: 0.0 };
    let mut m = MarioRuntime::new(Arc::new(pool()), spawn, -20000.0);
    m.set_objects(ObjectManager::new(vec![], None, None));
    m
}

fn tick(m: &mut MarioRuntime) {
    let c = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: 0.0, stick_mag: 0.0, button_down: 0, button_pressed: 0 };
    object_manager::step(m, None, false);
    m.step(&c, 0.0, false, true);
    object_manager::after_mario_step(m);
    m.objects.drain_events();
}

#[test]
fn air_meter_drains_two_units_per_frame_under_water() {
    let mut m = banjo_at([0.0, -1000.0, 0.0], ACT_WATER_IDLE);
    for _ in 0..10 { tick(&mut m); }
    assert!(underwater(&m), "pos y {} level {}", m.pos[1], m.water.level);
    assert_eq!(m.banjo.air, AIR_MAX - 10 * AIR_DRAIN);
    assert_eq!(m.banjo.air_bubbles(), 6);
}

#[test]
fn sm64_health_drain_is_off_for_banjo_so_the_meter_decides() {
    let mut m = banjo_at([0.0, -1000.0, 0.0], ACT_WATER_IDLE);
    let health = m.health;
    for _ in 0..600 { tick(&mut m); }
    assert_eq!(m.health, health, "SM64 breath drain must not apply");
    assert_ne!(m.action, ACT_DROWNING);
}

#[test]
fn banjo_drowns_when_the_meter_is_empty() {
    let mut m = banjo_at([0.0, -1000.0, 0.0], ACT_WATER_IDLE);
    m.banjo.air = 20;
    let mut frames = 0;
    while m.action != ACT_DROWNING && m.action != ACT_WATER_DEATH && frames < 200 {
        tick(&mut m);
        frames += 1;
    }
    assert!(m.action == ACT_DROWNING || m.action == ACT_WATER_DEATH, "action {:#x} after {frames}", m.action);
    assert!(frames >= 10 && frames < 40, "{frames}");
}

#[test]
fn full_minute_of_air() {
    // 3600 / 2 per 30 Hz frame = 1800 frames = 60 s.
    let mut m = banjo_at([0.0, -1000.0, 0.0], ACT_WATER_IDLE);
    let mut frames = 0;
    while m.banjo.air > 0 && frames < 3000 { tick(&mut m); frames += 1; }
    assert_eq!(frames, 1800);
}

#[test]
fn surface_refills_the_meter_and_catches_breath() {
    let mut m = banjo_at([0.0, -1000.0, 0.0], ACT_WATER_IDLE);
    for _ in 0..300 { tick(&mut m); }
    assert!(m.banjo.air < AIR_MAX - 500);
    // Lift Banjo to the surface.
    m.state.pos[1] = -60.0;
    m.banjo.events.clear();
    for _ in 0..30 { tick(&mut m); }
    assert!(!underwater(&m));
    assert_eq!(m.banjo.air, AIR_MAX);
    assert!(m.banjo.events.iter().any(|e| matches!(e, BanjoEvent::CatchBreath { .. })));
}

#[test]
fn polluted_water_drains_double_under_water_and_also_on_the_surface() {
    let mut m = banjo_at([0.0, -1000.0, 0.0], ACT_WATER_IDLE);
    m.banjo.polluted_water = true;
    for _ in 0..10 { tick(&mut m); }
    assert_eq!(m.banjo.air, AIR_MAX - 10 * AIR_DRAIN_POLLUTED);
    let mut s = banjo_at([0.0, -60.0, 0.0], ACT_WATER_IDLE);
    s.banjo.polluted_water = true;
    for _ in 0..10 { tick(&mut s); }
    assert_eq!(s.banjo.air, AIR_MAX - 10 * AIR_DRAIN, "surface swimming drains at the normal rate in polluted water");
}

#[test]
fn infinite_air_flag_keeps_the_meter_full() {
    let mut m = banjo_at([0.0, -1000.0, 0.0], ACT_WATER_IDLE);
    m.banjo.infinite_air = true;
    for _ in 0..200 { tick(&mut m); }
    assert_eq!(m.banjo.air, AIR_MAX);
}
