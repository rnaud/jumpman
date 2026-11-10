//! The bow from Epona's saddle (`func_80834380`, `func_80836AB8`): only the bow is drawn from a horse, every arrow flies as
//! `ARROW_NORMAL_HORSE` (free of magic), the aim is limited to 3500 up or down and 19114 to either side of the horse.
use jumpman_sim::collision::*;
use jumpman_sim::link::items::*;
use jumpman_sim::link::moves::*;
use jumpman_sim::link::projectiles::Kind;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::state::*;
use std::sync::Arc;

fn flat() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    w.add_quad([-30000.0, 0.0, -30000.0], [-30000.0, 0.0, 30000.0], [30000.0, 0.0, 30000.0], [30000.0, 0.0, -30000.0], &SurfaceOptions::default());
    w
}

fn tick(m: &mut MarioRuntime, x: f64, y: f64, down: u32, pressed: u32) {
    let mag = (x * x + y * y).sqrt().min(64.0);
    let c = ControllerSnapshot { focus_yaw: None, stick_x: x, stick_y: y, stick_mag: mag, button_down: down | pressed, button_pressed: pressed };
    m.step(&c, 0.0, true, false);
}

fn mounted() -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.link.enable(Age::Adult);
    let world = m.world.clone();
    m.link.epona.place(&world, [150.0, 0.0, 0.0], 0.0);
    m.pos = [350.0, 0.0, 0.0];
    tick(&mut m, 0.0, 0.0, 0, 0);
    tick(&mut m, 0.0, 0.0, 0, A_BUTTON);
    for _ in 0..110 {
        tick(&mut m, 0.0, 0.0, 0, 0);
    }
    assert!(m.link.epona.riding());
    m
}

#[test]
fn only_the_bow_is_drawn_from_the_saddle_and_it_stops_the_horses_steering_while_he_aims() {
    let mut m = mounted();
    m.link.inv.set_c(0, Some(CItem::Item(Item::Boomerang)), Age::Adult);
    m.link.inv.set_c(1, Some(CItem::Item(Item::Bow)), Age::Adult);
    tick(&mut m, 0.0, 0.0, 0, C_LEFT);
    assert_ne!(m.link.mv, Mv::Aim, "the boomerang is not used on a horse");
    tick(&mut m, 0.0, 0.0, 0, 0);
    tick(&mut m, 0.0, 0.0, C_DOWN, C_DOWN);
    assert_eq!(m.link.mv, Mv::Aim);
    assert!(m.link.epona.riding(), "he stays in the saddle");
    assert!((m.pos[1] - (m.link.epona.pos[1] + jumpman_sim::link::epona::RIDER_HEIGHT)).abs() < 1e-6, "and does not drop to the floor");
}

#[test]
fn the_aim_is_limited_to_3500_up_and_19114_to_the_side_and_the_arrow_is_free_and_normal() {
    let mut m = mounted();
    m.link.inv.set_c(0, Some(CItem::Item(Item::LightArrow)), Age::Adult);
    m.link.inv.magic = 0;
    let arrows = m.link.inv.ammo(Ammo::Arrow);
    tick(&mut m, 0.0, 0.0, C_LEFT, C_LEFT);
    assert_eq!(m.link.mv, Mv::Aim, "no magic is needed on a horse: the arrow is a plain one");
    // look up and to the right for a long time
    for _ in 0..200 {
        tick(&mut m, 64.0, 64.0, C_LEFT, 0);
    }
    let aim = m.link.aim.expect("aiming");
    assert!(aim.pitch.abs() <= HORSE_AIM_PITCH + 1e-9, "pitch {}", aim.pitch);
    let side = jumpman_sim::math::to_s16(aim.yaw - m.link.epona.yaw).abs() as f64;
    assert!(side <= 19114.0 + 1.0, "side {side}");
    assert!((HORSE_AIM_PITCH - 0.3356).abs() < 1e-3);
    // let go of the C button: the arrow flies
    tick(&mut m, 0.0, 0.0, 0, 0);
    assert_eq!(m.link.inv.ammo(Ammo::Arrow), arrows - 1);
    // the follow-through of the shot ends and he is back in the saddle with nothing in his hands
    for _ in 0..60 {
        tick(&mut m, 0.0, 0.0, 0, 0);
    }
    assert_eq!(m.link.mv, Mv::Free);
    assert!(m.link.epona.riding());
    assert!(m.link.projectiles.count(Kind::Arrow) >= 1 && m.link.projectiles.count(Kind::LightArrow) == 0);
}
