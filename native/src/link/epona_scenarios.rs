//! Epona on real courses: mount, gallop for five seconds, jump a fence put in her way, stop and dismount.
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::link::combat::OOT_SCALE;
use jumpman_sim::link::epona::{Rider, RIDER_HEIGHT};
use jumpman_sim::link::items::Age;
use jumpman_sim::math::{coss, sins};
use jumpman_sim::state::*;
use std::sync::Arc;

fn assets() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn load(course: &str) -> GameSim {
    let mut sim = GameSim::load(&assets(), CourseRequest::new(course)).unwrap();
    sim.as_link = true;
    sim.mario.link.enable(Age::Adult);
    for _ in 0..40 {
        sim.tick(&ControllerSnapshot::default());
    }
    sim
}

fn step(sim: &mut GameSim, x: f64, y: f64, pressed: u32, n: usize) {
    for i in 0..n {
        let mag = (x * x + y * y).sqrt().min(64.0);
        let p = if i == 0 { pressed } else { 0 };
        let c = ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: mag, button_down: p, button_pressed: p, ..Default::default() };
        sim.tick(&c);
    }
}

fn assert_sane(sim: &GameSim) {
    let e = &sim.mario.link.epona;
    assert!(e.pos.iter().all(|v| v.is_finite()) && sim.mario.pos.iter().all(|v| v.is_finite()), "{:?}", e.pos);
    let floor = sim.mario.world.find_floor_height(e.pos[0], e.pos[1] + 300.0, e.pos[2]);
    assert!(e.pos[1] >= floor - 2.0 || e.jumping, "Epona under the floor: {} < {floor}", e.pos[1]);
    assert!(sim.mario.world.water_level(e.pos[0], e.pos[2]) <= floor + 1.0, "Epona in water");
}

/// Mount next to her, ride five seconds with a little steering, stop, dismount.
fn ride_and_dismount(course: &str) {
    let mut sim = load(course);
    let p = sim.mario.pos;
    let world = sim.mario.world.clone();
    // She stands to Link's left (+90 degrees), facing the same way.
    let yaw = sim.mario.face_angle[1];
    let at = [p[0] + sins(yaw + 16384.0) * 220.0, p[1], p[2] + coss(yaw + 16384.0) * 220.0];
    sim.mario.link.epona.place(&world, at, yaw);
    assert!(sim.mario.link.epona.mount_side(sim.mario.pos).is_some(), "{course}: Link is beside her");
    step(&mut sim, 0.0, 0.0, A_BUTTON, 1);
    assert!(matches!(sim.mario.link.epona.rider, Rider::Mounting { .. }), "{course}: {:?}", sim.mario.link.epona.rider);
    step(&mut sim, 0.0, 0.0, 0, 120);
    assert!(sim.mario.link.epona.riding(), "{course}: mounted");
    assert!((sim.mario.pos[1] - sim.mario.link.epona.pos[1] - RIDER_HEIGHT).abs() < 1e-6);
    // Five seconds (150 ticks) of galloping, weaving left and right.
    let start = sim.mario.link.epona.pos;
    let mut moved = 0.0f64;
    for k in 0..150 {
        let turn = if (k / 25) % 2 == 0 { 12.0 } else { -12.0 };
        step(&mut sim, turn, 64.0, 0, 1);
        assert_sane(&sim);
        let e = sim.mario.link.epona.pos;
        moved = moved.max(((e[0] - start[0]).powi(2) + (e[2] - start[2]).powi(2)).sqrt());
    }
    assert!(moved > 600.0, "{course}: Epona barely moved ({moved})");
    // Stop and get off.
    step(&mut sim, 0.0, -64.0, 0, 60);
    step(&mut sim, 0.0, 0.0, 0, 400);
    assert!(sim.mario.link.epona.can_dismount(), "{course}: she should stand: {:?} speed {}", sim.mario.link.epona.act, sim.mario.link.epona.speed);
    step(&mut sim, 0.0, 0.0, A_BUTTON, 1);
    step(&mut sim, 0.0, 0.0, 0, 120);
    assert_eq!(sim.mario.link.epona.rider, Rider::None, "{course}");
    assert_sane(&sim);
    let (e, m) = (sim.mario.link.epona.pos, sim.mario.pos);
    let d = ((e[0] - m[0]).powi(2) + (e[2] - m[2]).powi(2)).sqrt();
    assert!(d > 100.0 && d < 700.0, "{course}: dismounted {d} away");
}

#[test]
fn ride_bob_omb_battlefield() {
    ride_and_dismount("bob");
}

#[test]
fn ride_kokiri_forest() {
    ride_and_dismount("kokiri_forest");
}

#[test]
fn ride_a_banjo_and_a_kart_course() {
    ride_and_dismount("bk_spiral_mountain");
    ride_and_dismount("mk_mario_raceway");
}

/// A fence across her path: she jumps it at a gallop and carries on.
#[test]
fn epona_jumps_a_fence_on_a_real_course() {
    let mut sim = load("bob");
    let world = sim.mario.world.clone();
    let p = sim.mario.pos;
    let yaw = sim.mario.face_angle[1];
    sim.mario.link.epona.place(&world, [p[0], p[1], p[2]], yaw);
    sim.mario.link.epona.begin_mount(-1);
    sim.mario.link.epona.start_mounted();
    // A 30-unit (OoT) fence 2000 ahead on level ground: add a wall across the heading, wide enough to see.
    let (fx, fz) = (p[0] + sins(yaw) * 2400.0, p[2] + coss(yaw) * 2400.0);
    let base = world.find_floor_height(fx, p[1] + 300.0, fz);
    let mut w: CollisionWorld = (*world).clone();
    let h = 30.0 * OOT_SCALE;
    let (sx, sz) = (sins(yaw + 16384.0) * 1500.0, coss(yaw + 16384.0) * 1500.0);
    let (nx, nz) = (sins(yaw) * 60.0, coss(yaw) * 60.0);
    let o = SurfaceOptions::default();
    let pt = |dx: f64, dz: f64, y: f64| [fx + dx, base + y, fz + dz];
    // Front face (toward Link), top, back face; wound so the normals face outward / up.
    w.add_quad(pt(sx, sz, 0.0), pt(sx, sz, h), pt(-sx, -sz, h), pt(-sx, -sz, 0.0), &o);
    w.add_quad(pt(sx, sz, h), pt(sx + nx, sz + nz, h), pt(-sx + nx, -sz + nz, h), pt(-sx, -sz, h), &o);
    w.add_quad(pt(sx + nx, sz + nz, 0.0), pt(-sx + nx, -sz + nz, 0.0), pt(-sx + nx, -sz + nz, h), pt(sx + nx, sz + nz, h), &o);
    sim.mario.world = Arc::new(w);
    let mut jumped = false;
    let start = sim.mario.link.epona.pos;
    for _ in 0..240 {
        step(&mut sim, 0.0, 64.0, 0, 1);
        jumped |= sim.mario.link.epona.jumping;
    }
    let e = sim.mario.link.epona.pos;
    let along = (e[0] - start[0]) * sins(yaw) + (e[2] - start[2]) * coss(yaw);
    assert!(jumped, "never jumped; at {along} of 2400 along the heading");
    assert!(along > 2600.0, "did not get over the fence: {along}");
}
