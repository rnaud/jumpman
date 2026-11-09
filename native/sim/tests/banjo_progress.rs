//! Banjo-Kazooie climbing (`bs/climb.c`, `ch/climbBase.c`), ability progression (`abilityprogress.c`,
//! `ch/mole.c`), Mumbo tokens and the jiggy / note locks (`jigsawpicture.c`, `lairspawnqueue.c`).
use jumpman_sim::banjo_ext::*;
use jumpman_sim::characters::*;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::state::*;
use std::sync::Arc;

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-6000.0, 0.0, -6000.0], [-6000.0, 0.0, 6000.0], [6000.0, 0.0, 6000.0], &o);
    w.add_triangle([-6000.0, 0.0, -6000.0], [6000.0, 0.0, 6000.0], [6000.0, 0.0, -6000.0], &o);
    w
}

fn banjo() -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -1600.0);
    m.face_angle[1] = 0.0;
    for _ in 0..3 { frame(&mut m, 0.0, 0, 0); }
    m
}

fn frame(m: &mut MarioRuntime, stick_y: f64, down: u32, pressed: u32) {
    let c = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y, stick_mag: stick_y.abs(), button_down: down | pressed, button_pressed: pressed };
    m.step(&c, 0x8000 as f64, false, true);
}

fn pole(kind: u8) -> ClimbPole {
    ClimbPole { bottom: [0.0, 0.0, 200.0], top: [0.0, 600.0, 200.0], radius: 60.0, kind }
}

#[test]
fn walking_into_a_pole_grabs_it_and_the_stick_climbs_at_100_to_300() {
    let mut m = banjo();
    m.banjo.climbs.push(pole(1));
    let mut grabbed = false;
    for _ in 0..60 {
        frame(&mut m, 64.0, 0, 0);
        if banjo_move(&m) == BANJO_CLIMB { grabbed = true; break; }
    }
    assert!(grabbed, "action {:#x} pos {:?}", m.action, m.pos);
    assert!(m.banjo.events.iter().any(|e| matches!(e, BanjoEvent::ClimbGrab)));
    // Hold the stick up: full tilt = 300 u/s * 1.25 / 30 per frame.
    for _ in 0..8 { frame(&mut m, 64.0, 0, 0); }
    let y0 = m.pos[1];
    frame(&mut m, 64.0, 0, 0);
    let dy = m.pos[1] - y0;
    assert!((dy - 300.0 * 1.25 / 30.0).abs() < 0.2, "climb speed {dy}");
    for _ in 0..120 { frame(&mut m, 64.0, 0, 0); }
    assert!((m.pos[1] - 600.0).abs() < 0.01, "ledge pole stops at the top: {}", m.pos[1]);
    assert_eq!(banjo_move(&m), BANJO_CLIMB);
    let off = ((m.pos[0] - 0.0).powi(2) + (m.pos[2] - 200.0).powi(2)).sqrt();
    assert!((off - 60.0).abs() < 1.0, "distance from the axis {off}");
}

#[test]
fn climbing_down_to_the_base_lets_go() {
    let mut m = banjo();
    m.banjo.climbs.push(pole(1));
    for _ in 0..60 { frame(&mut m, 64.0, 0, 0); if banjo_move(&m) == BANJO_CLIMB { break; } }
    for _ in 0..30 { frame(&mut m, 64.0, 0, 0); }
    assert!(m.pos[1] > 100.0);
    for _ in 0..80 { frame(&mut m, -64.0, 0, 0); if banjo_move(&m) != BANJO_CLIMB { break; } }
    assert_ne!(banjo_move(&m), BANJO_CLIMB);
    assert!(m.pos[1].abs() < 1.0, "back on the floor, y {}", m.pos[1]);
    assert!(m.banjo.events.iter().any(|e| matches!(e, BanjoEvent::ClimbRelease)));
    // The regrab delay keeps the held stick from grabbing the pole again at once.
    frame(&mut m, 64.0, 0, 0);
    assert_ne!(banjo_move(&m), BANJO_CLIMB);
}

#[test]
fn a_jumps_off_the_pole_backwards() {
    let mut m = banjo();
    m.banjo.climbs.push(pole(1));
    for _ in 0..60 { frame(&mut m, 64.0, 0, 0); if banjo_move(&m) == BANJO_CLIMB { break; } }
    for _ in 0..20 { frame(&mut m, 64.0, 0, 0); }
    let (y, z) = (m.pos[1], m.pos[2]);
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    assert_ne!(banjo_move(&m), BANJO_CLIMB);
    for _ in 0..6 { frame(&mut m, 0.0, 0, 0); }
    assert!(m.pos[1] > y, "rising");
    assert!(m.pos[2] < z, "away from the pole (the pole is at +Z)");
}

#[test]
fn exit_poles_hop_onto_the_top() {
    let mut m = banjo();
    m.banjo.climbs.push(pole(2));
    for _ in 0..60 { frame(&mut m, 64.0, 0, 0); if banjo_move(&m) == BANJO_CLIMB { break; } }
    let mut exited = false;
    for _ in 0..200 {
        frame(&mut m, 64.0, 0, 0);
        if banjo_move(&m) == BANJO_CLIMB_EXIT { exited = true; break; }
    }
    assert!(exited, "reaching the top of an exit pole starts the exit hop");
    assert!(m.vel[1] > 0.0);
}

#[test]
fn the_pole_needs_the_climb_move() {
    let mut m = banjo();
    m.banjo.begin_progression();
    m.banjo.climbs.push(pole(1));
    for _ in 0..60 { frame(&mut m, 64.0, 0, 0); }
    assert_ne!(banjo_move(&m), BANJO_CLIMB);
    m.banjo.learn(Ability::Climb);
    m.pos = [0.0, 0.0, 0.0];
    for _ in 0..60 { frame(&mut m, 64.0, 0, 0); if banjo_move(&m) == BANJO_CLIMB { break; } }
    assert_eq!(banjo_move(&m), BANJO_CLIMB);
}

#[test]
fn progression_starts_without_the_moves_and_each_one_is_gated() {
    let mut m = banjo();
    assert!(!m.banjo.in_progression());
    m.banjo.begin_progression();
    assert!(m.banjo.in_progression());
    m.banjo.eggs = 10;
    frame(&mut m, 0.0, 0, C_UP);
    assert!(banjo_move(&m) != BANJO_EGG_HEAD, "eggs need the Egg Firing lesson");
    frame(&mut m, 0.0, Z_TRIG, Z_TRIG | C_LEFT);
    assert!(banjo_move(&m) != BANJO_TROT);
    // Jump yes; Feathery Flap no.
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    for _ in 0..10 { frame(&mut m, 0.0, A_BUTTON, 0); }
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    assert_ne!(banjo_move(&m), BANJO_FLAP);
    // Learning it brings it back.
    m.banjo.learn(Ability::FeatheryFlap);
    for _ in 0..80 { frame(&mut m, 0.0, 0, 0); if m.action & ACT_FLAG_AIR == 0 { break; } }
    for _ in 0..20 { frame(&mut m, 0.0, 0, 0); }
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    for _ in 0..10 { frame(&mut m, 0.0, A_BUTTON, 0); }
    frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
    assert_eq!(banjo_move(&m), BANJO_FLAP);
}

#[test]
fn without_the_high_jump_lesson_holding_a_does_not_add_height() {
    fn apex(learned: bool) -> f64 {
        let mut m = banjo();
        m.banjo.begin_progression();
        if learned { m.banjo.learn(Ability::HoldAJumpHigher); }
        frame(&mut m, 0.0, A_BUTTON, A_BUTTON);
        let mut apex = 0.0f64;
        for _ in 0..80 { frame(&mut m, 0.0, A_BUTTON, 0); apex = apex.max(m.pos[1]); }
        apex
    }
    assert!(apex(true) > apex(false) * 1.3, "{} vs {}", apex(true), apex(false));
}

#[test]
fn a_molehill_teaches_its_move_once_banjo_stands_next_to_it() {
    let mut m = banjo();
    m.banjo.begin_progression();
    m.banjo.moles.push(Molehill { pos: [0.0, 0.0, 600.0], id: 0xA, sm: false });
    m.banjo.moles.push(Molehill { pos: [0.0, 0.0, 0.0], id: 7, sm: true });
    frame(&mut m, 0.0, 0, 0);
    assert!(m.banjo.has(Ability::Climb), "Spiral Mountain's climb lesson");
    assert!(!m.banjo.has(Ability::Eggs), "the far molehill is out of reach");
    assert!(m.banjo.events.iter().any(|e| *e == BanjoEvent::Learned(Ability::Climb)));
    m.pos = [0.0, 0.0, 450.0];
    frame(&mut m, 0.0, 0, 0);
    assert!(m.banjo.has(Ability::Eggs));
}

#[test]
fn mole_table_matches_the_decomp() {
    assert_eq!(mole_abilities(false, 0xD), &[Ability::ShockJump]);
    assert_eq!(mole_abilities(false, 0xE), &[Ability::Flight]);
    assert_eq!(mole_abilities(false, 0x12), &[Ability::FirstNoteDoor]);
    assert_eq!(mole_abilities(true, 7), &[Ability::Climb]);
    assert!(mole_abilities(true, 1).is_empty() && mole_abilities(true, 8).is_empty());
    assert_eq!(Ability::Climb.bit(), 1 << 5);
    assert_eq!(Ability::FirstNoteDoor.bit(), 1 << 0x13);
}

#[test]
fn pads_only_cycle_through_the_learned_moves() {
    let mut b = BanjoExt::default();
    b.begin_progression();
    b.learn(Ability::Flight);
    b.cycle_power();
    assert_eq!(b.power, Power::Flight);
    b.cycle_power();
    assert_eq!(b.power, Power::None);
}

#[test]
fn mumbo_tokens_are_picked_up_and_counted() {
    let mut m = banjo();
    m.banjo.drop_pickup(PickupKind::MumboToken, [0.0, 0.0, 20.0]);
    m.banjo.drop_pickup(PickupKind::MumboToken, [30.0, 0.0, 0.0]);
    frame(&mut m, 0.0, 0, 0);
    assert_eq!(m.banjo.mumbo_tokens, 2);
    assert!(m.banjo.pickups.is_empty());
    assert_eq!(PickupKind::MumboToken.label(), "MUMBO TOKEN");
}

#[test]
fn jiggy_puzzles_take_their_cost_from_the_stock() {
    let mut b = BanjoExt::default();
    b.jiggies = 1;
    assert!(!b.try_complete_puzzle(2), "world 2 costs 2 jiggies");
    assert!(b.try_complete_puzzle(1));
    assert_eq!(b.jiggy_stock(), 0);
    b.jiggies = 2;
    assert!(!b.try_complete_puzzle(2), "one jiggy is already placed");
    b.jiggies = 3;
    assert!(b.try_complete_puzzle(2));
    assert_eq!(b.jiggy_stock(), 0);
    assert!(b.puzzle_complete(1) && b.puzzle_complete(2) && !b.puzzle_complete(3));
    assert_eq!(PUZZLE_COST, [1, 2, 5, 7, 8, 9, 10, 12, 15]);
}

#[test]
fn note_doors_open_with_the_lesson_and_enough_notes() {
    let mut b = BanjoExt::default();
    b.begin_progression();
    b.notes = 450;
    assert!(!b.try_open_note_door(1), "the Note Door lesson comes first");
    b.learn(Ability::FirstNoteDoor);
    assert!(b.try_open_note_door(1));
    assert!(b.try_open_note_door(5), "door 5 costs 450");
    assert!(!b.try_open_note_door(6), "door 6 costs 640");
}

#[test]
fn a_closed_note_door_is_a_wall_until_the_notes_are_there() {
    let mut m = banjo();
    m.banjo.begin_progression();
    m.banjo.learn(Ability::FirstNoteDoor);
    m.banjo.notes = 100; // door 2 costs 180
    // The door sits across +Z at z = 400, facing +Z (yaw 0).
    m.banjo.door_actors.push(NoteDoor { pos: [0.0, 0.0, 400.0], yaw: 0.0, index: 2 });
    for _ in 0..120 { frame(&mut m, 64.0, 0, 0); }
    assert!(m.pos[2] < 400.0 - DOOR_THICKNESS + 1.0, "stopped in front of the door at z {}", m.pos[2]);
    assert!(!m.banjo.note_door_open(2));
    // With 180 notes it opens on approach and Banjo walks through.
    m.banjo.notes = 180;
    for _ in 0..120 { frame(&mut m, 64.0, 0, 0); }
    assert!(m.banjo.note_door_open(2));
    assert!(m.pos[2] > 450.0, "through the open door: z {}", m.pos[2]);
    assert!(m.banjo.events.iter().any(|e| *e == BanjoEvent::NoteDoorOpened(2)));
}

#[test]
fn note_doors_stay_open_outside_a_progression_game() {
    let mut m = banjo();
    m.banjo.notes = 0;
    m.banjo.door_actors.push(NoteDoor { pos: [0.0, 0.0, 400.0], yaw: 0.0, index: 12 });
    for _ in 0..120 { frame(&mut m, 64.0, 0, 0); }
    assert!(m.pos[2] > 450.0, "sandbox: z {}", m.pos[2]);
}

#[test]
fn the_progress_survives_a_map_change_but_not_the_projectiles() {
    let mut b = BanjoExt::default();
    b.begin_progression();
    b.learn(Ability::Climb);
    b.notes = 77;
    b.jiggies = 4;
    b.mumbo_tokens = 3;
    b.try_complete_puzzle(1);
    b.climbs.push(ClimbPole { bottom: [0.0; 3], top: [0.0, 1.0, 0.0], radius: 1.0, kind: 1 });
    b.air = 100;
    let c = b.carried();
    assert!(c.has(Ability::Climb) && !c.has(Ability::Eggs));
    assert_eq!((c.notes, c.jiggies, c.mumbo_tokens), (77, 4, 3));
    assert!(c.puzzle_complete(1));
    assert!(c.climbs.is_empty() && c.pickups.is_empty());
    assert_eq!(c.air, AIR_MAX);
}

#[test]
fn lair_world_doors_map_to_the_puzzle_worlds() {
    assert_eq!(puzzle_world("bk_mumbos_mountain"), Some(1));
    assert_eq!(puzzle_world("bk_click_clock_wood_winter"), Some(9));
    assert_eq!(puzzle_world("bk_rusty_bucket_bay"), Some(8));
    assert_eq!(puzzle_world("bk_gl_mm_lobby"), None);
    assert_eq!(puzzle_world("bk_spiral_mountain"), None);
}

#[test]
fn climbing_emits_a_step_sound_event_every_14_frames() {
    let mut m = banjo();
    m.banjo.climbs.push(pole(1));
    for _ in 0..60 { frame(&mut m, 64.0, 0, 0); if banjo_move(&m) == BANJO_CLIMB { break; } }
    m.banjo.events.clear();
    for _ in 0..42 { frame(&mut m, 64.0, 0, 0); }
    let steps = m.banjo.events.iter().filter(|e| matches!(e, BanjoEvent::ClimbStep)).count();
    assert!((2..=4).contains(&steps), "{steps} steps in 42 frames");
}
