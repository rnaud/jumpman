//! Scenarios of Link's items against the world props (`link/props.rs`) on a real `MarioRuntime`: sword and hammer swings, a
//! slingshot seed on an eye switch, a fire arrow on a torch and a web, a bomb on a boulder, Din's Fire, and a Deku Stick lit at a
//! torch that then burns a web.
use jumpman_sim::collision::*;
use jumpman_sim::link::combat::*;
use jumpman_sim::link::items::*;
use jumpman_sim::link::moves::*;
use jumpman_sim::link::projectiles::*;
use jumpman_sim::link::props::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::state::*;
use std::sync::Arc;

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-8000.0, 0.0, -8000.0], [-8000.0, 0.0, 8000.0], [8000.0, 0.0, 8000.0], &o);
    w.add_triangle([-8000.0, 0.0, -8000.0], [8000.0, 0.0, 8000.0], [8000.0, 0.0, -8000.0], &o);
    w
}

fn link(age: Age) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(vec![], None, None));
    m.link.enable(age);
    m
}

#[derive(Default, Clone)]
struct In {
    x: f64,
    y: f64,
    down: u32,
    pressed: u32,
}

/// One world tick: the player, the projectiles, then the props (events routed like the app does).
fn tick(m: &mut MarioRuntime, p: &mut Props, i: &In) -> Vec<LinkEvent> {
    let mag = (i.x * i.x + i.y * i.y).sqrt().min(64.0);
    let c = ControllerSnapshot { focus_yaw: None, stick_x: i.x, stick_y: i.y, stick_mag: mag, button_down: i.down | i.pressed, button_pressed: i.pressed };
    m.step(&c, 32768.0, true, false);
    LinkExt::step_projectiles(m, &p.targets());
    p.step_link(m);
    let events = m.link.take_events();
    for e in &events {
        p.feed(e);
    }
    events
}

fn idle(m: &mut MarioRuntime, p: &mut Props, n: usize) {
    for _ in 0..n {
        tick(m, p, &In::default());
    }
}

fn prop(kind: PropKind, pos: [f64; 3], flag: i32) -> Prop {
    let mut p = Prop::new(kind, pos);
    p.flag = flag;
    p
}

#[test]
fn a_sword_swing_cuts_the_grass_in_front_and_not_behind() {
    let mut m = link(Age::Adult);
    let mut p = Props::new(vec![Prop { table: Some(1), ..prop(PropKind::Grass, [0.0, 0.0, 110.0], -1) }, prop(PropKind::Grass, [0.0, 0.0, -110.0], -1)]);
    idle(&mut m, &mut p, 2);
    tick(&mut m, &mut p, &In { pressed: B_BUTTON, ..Default::default() });
    idle(&mut m, &mut p, 20);
    assert!(p.list[0].gone(), "the grass in front is cut");
    assert!(!p.list[1].gone(), "a forward slash does not reach behind");
    assert!(p.take_events().iter().any(|e| matches!(e, PropEvent::Drop { table: Some(1), .. })));
}

#[test]
fn a_spin_attack_cuts_grass_on_every_side() {
    let mut m = link(Age::Adult);
    let mut p = Props::new(vec![prop(PropKind::Grass, [0.0, 0.0, 100.0], -1), prop(PropKind::Grass, [0.0, 0.0, -100.0], -1), prop(PropKind::Grass, [100.0, 0.0, 0.0], -1)]);
    idle(&mut m, &mut p, 2);
    tick(&mut m, &mut p, &In { pressed: B_BUTTON, down: B_BUTTON, ..Default::default() });
    for _ in 0..60 {
        tick(&mut m, &mut p, &In { down: B_BUTTON, ..Default::default() });
    }
    for _ in 0..40 {
        tick(&mut m, &mut p, &In::default());
    }
    assert!(p.list.iter().all(|g| g.gone()), "{:?}", p.list.iter().map(|g| g.state).collect::<Vec<_>>());
}

#[test]
fn the_hammer_breaks_a_crate_and_presses_a_rusty_switch_but_the_sword_does_not() {
    let mut m = link(Age::Adult);
    let mut p = Props::new(vec![prop(PropKind::Crate, [0.0, 0.0, 130.0], -1), prop(PropKind::RustySwitch, [0.0, 0.0, 140.0], 7)]);
    idle(&mut m, &mut p, 2);
    tick(&mut m, &mut p, &In { pressed: B_BUTTON, ..Default::default() });
    idle(&mut m, &mut p, 20);
    assert!(!p.list[0].gone() && !p.flag(7), "a plain sword slash does neither");
    m.link.inv.set_c(0, Some(CItem::Item(Item::MegatonHammer)), Age::Adult);
    tick(&mut m, &mut p, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    idle(&mut m, &mut p, 40);
    assert!(p.list[0].gone() && p.flag(7));
}

#[test]
fn a_slingshot_seed_presses_the_eye_switch_and_a_fire_arrow_does_not_but_burns_the_web() {
    let mut m = link(Age::Child);
    let mut p = Props::new(vec![prop(PropKind::EyeSwitch, [0.0, 40.0, 900.0], 1)]);
    idle(&mut m, &mut p, 1);
    tick(&mut m, &mut p, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    tick(&mut m, &mut p, &In::default());
    idle(&mut m, &mut p, 16);
    assert!(p.flag(1), "the seed hit the eye and its switch flag is set");
    assert!(p.take_events().iter().any(|e| matches!(e, PropEvent::Switch { flag: 1, on: true, .. })));

    let mut m = link(Age::Adult);
    m.link.inv.set_c(0, Some(CItem::Item(Item::Bow)), Age::Adult);
    m.link.inv.set_c(1, Some(CItem::Item(Item::FireArrow)), Age::Adult);
    let mut p = Props::new(vec![prop(PropKind::WallWeb, [0.0, 0.0, 1100.0], 3), prop(PropKind::Torch, [900.0, 0.0, 1100.0], 4)]);
    idle(&mut m, &mut p, 1);
    tick(&mut m, &mut p, &In { pressed: C_DOWN, down: C_DOWN, ..Default::default() });
    tick(&mut m, &mut p, &In::default());
    idle(&mut m, &mut p, 20);
    assert!(p.flag(3), "the fire arrow burned the web");
    assert!(matches!(p.list[0].state, State::Burning(_) | State::Gone));
    assert!(!p.list[1].is_lit(), "the torch off to the side stays dark");
}

#[test]
fn a_plain_arrow_does_not_burn_a_web() {
    let mut m = link(Age::Adult);
    m.link.inv.set_c(0, Some(CItem::Item(Item::Bow)), Age::Adult);
    let mut p = Props::new(vec![prop(PropKind::WallWeb, [0.0, 0.0, 1100.0], 3)]);
    idle(&mut m, &mut p, 1);
    tick(&mut m, &mut p, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    tick(&mut m, &mut p, &In::default());
    idle(&mut m, &mut p, 20);
    assert!(!p.flag(3) && p.list[0].state == State::Intact);
}

#[test]
fn a_bomb_blows_up_a_boulder_and_a_cracked_wall_and_sets_their_flags() {
    let mut m = link(Age::Adult);
    m.link.inv.set_c(2, Some(CItem::Item(Item::Bomb)), Age::Adult);
    let mut p = Props::new(vec![prop(PropKind::Boulder, [0.0, 0.0, 220.0], 7), prop(PropKind::CrackedWall, [60.0, 0.0, 260.0], 9), prop(PropKind::Boulder, [0.0, 0.0, 6000.0], 8)]);
    idle(&mut m, &mut p, 1);
    tick(&mut m, &mut p, &In { pressed: C_RIGHT, down: C_RIGHT, ..Default::default() });
    for i in 0..140 {
        tick(&mut m, &mut p, &In { y: if i < 60 { -64.0 } else { 0.0 }, ..Default::default() });
    }
    assert!(p.flag(7) && p.flag(9) && !p.flag(8), "flags {:b}", p.flags);
}

#[test]
fn dins_fire_lights_nearby_torches() {
    let mut m = link(Age::Adult);
    m.link.inv.set_c(0, Some(CItem::Item(Item::DinsFire)), Age::Adult);
    let mut p = Props::new(vec![prop(PropKind::Torch, [180.0, 0.0, 0.0], 5), prop(PropKind::Torch, [5000.0, 0.0, 0.0], 6)]);
    idle(&mut m, &mut p, 1);
    tick(&mut m, &mut p, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    idle(&mut m, &mut p, 140);
    assert!(p.list[0].is_lit() && p.flag(5));
    assert!(!p.list[1].is_lit());
}

#[test]
fn a_deku_stick_lit_at_a_torch_burns_a_web_and_burns_out() {
    let mut m = link(Age::Child);
    m.link.inv.set_c(0, Some(CItem::Item(Item::DekuStick)), Age::Child);
    let mut torch = prop(PropKind::Torch, [0.0, 0.0, 170.0], 4);
    torch.state = State::Lit(f64::INFINITY);
    let mut p = Props::new(vec![torch, prop(PropKind::FloorWeb, [0.0, 0.0, 2200.0], 3)]);
    idle(&mut m, &mut p, 1);
    let ev = tick(&mut m, &mut p, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.weapon(), Weapon::Stick);
    assert!(ev.contains(&LinkEvent::StickLit), "{ev:?}");
    assert!(m.link.stick_fire > 0.0, "the stick caught fire at the torch: {}", m.link.stick_fire);
    // Walk the flame to the web.
    for _ in 0..80 {
        tick(&mut m, &mut p, &In { y: 64.0, ..Default::default() });
    }
    assert!(p.flag(3), "the burning stick set the web alight at {:?}", m.pos);
    // And the stick is used up when the flame dies.
    let sticks = m.link.inv.ammo(Ammo::Stick);
    let mut broke = false;
    for _ in 0..400 {
        broke |= tick(&mut m, &mut p, &In::default()).contains(&LinkEvent::StickBroke);
    }
    assert!(broke && m.link.stick_fire <= 0.0 && !m.link.weapon_stick);
    assert_eq!(m.link.inv.ammo(Ammo::Stick), sticks - 1);
}

#[test]
fn an_unlit_stick_stays_dark_away_from_flames() {
    let mut m = link(Age::Child);
    m.link.inv.set_c(0, Some(CItem::Item(Item::DekuStick)), Age::Child);
    let mut p = Props::new(vec![prop(PropKind::Torch, [0.0, 0.0, 3000.0], 4)]);
    idle(&mut m, &mut p, 1);
    tick(&mut m, &mut p, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    idle(&mut m, &mut p, 10);
    assert!(m.link.weapon_stick && m.link.stick_fire <= 0.0);
}

#[test]
fn standing_on_a_floor_switch_presses_it() {
    let mut m = link(Age::Adult);
    let mut p = Props::new(vec![prop(PropKind::FloorSwitch, [0.0, 0.0, 400.0], 2)]);
    idle(&mut m, &mut p, 2);
    assert!(!p.flag(2));
    for _ in 0..30 {
        tick(&mut m, &mut p, &In { y: 64.0, ..Default::default() });
    }
    for _ in 0..40 {
        tick(&mut m, &mut p, &In { y: 64.0, ..Default::default() });
    }
    assert!(p.flag(2) && p.list[0].state == State::On, "pressed when Link stepped on it at {:?}", m.pos);
}

#[test]
fn a_placed_bomb_is_lifted_with_a_throws_forward_and_blows_up_a_boulder_far_away() {
    let mut m = link(Age::Adult);
    m.link.inv.set_c(2, Some(CItem::Item(Item::Bomb)), Age::Adult);
    let mut p = Props::new(vec![prop(PropKind::Boulder, [0.0, 0.0, 900.0], 7)]);
    idle(&mut m, &mut p, 1);
    tick(&mut m, &mut p, &In { pressed: C_RIGHT, down: C_RIGHT, ..Default::default() });
    assert_eq!(m.link.projectiles.count(Kind::Bomb), 1);
    idle(&mut m, &mut p, 20);
    // A next to the resting bomb lifts it.
    let mut ev = vec![];
    for _ in 0..2 {
        ev.extend(tick(&mut m, &mut p, &In { pressed: A_BUTTON, ..Default::default() }));
    }
    assert!(ev.contains(&LinkEvent::BombLifted), "{ev:?}");
    assert!(m.link.projectiles.carrying());
    idle(&mut m, &mut p, 10);
    let head = m.link.projectiles.list[0].pos;
    assert!(head[1] > m.pos[1] + 100.0, "overhead: {head:?} vs {:?}", m.pos);
    // A throws it forward; the fuse is still running, so it lands near the boulder before it goes off.
    let mut thrown = false;
    for i in 0..4 {
        thrown |= tick(&mut m, &mut p, &In { pressed: if i == 0 { A_BUTTON } else { 0 }, ..Default::default() }).contains(&LinkEvent::BombThrown);
    }
    assert!(thrown && !m.link.projectiles.carrying());
    for _ in 0..120 {
        tick(&mut m, &mut p, &In::default());
    }
    assert_eq!(m.link.projectiles.count(Kind::Bomb), 0, "it went off");
}
