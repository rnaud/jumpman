//! Scenario tests for the faithful OoT Link controller (`link/moves.rs`) running on a real
//! `MarioRuntime`: swings and combos, stab, spin charge, jump slash, item use per C button,
//! projectile hits, hookshot pull, ocarina songs, health bridge, boots.
use jumpman_sim::collision::*;
use jumpman_sim::link::combat::*;
use jumpman_sim::link::items::*;
use jumpman_sim::link::moves::*;
use jumpman_sim::link::ocarina::{Note, Song};
use jumpman_sim::link::projectiles::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::*;
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

fn link(age: Age, world: CollisionWorld, objects: Vec<ObjectSpec>) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(objects, None, None));
    m.link.enable(age);
    m
}

#[derive(Default, Clone)]
struct In {
    x: f64,
    y: f64,
    down: u32,
    pressed: u32,
    focus: Option<f64>,
}

fn tick(m: &mut MarioRuntime, i: &In) {
    let mag = (i.x * i.x + i.y * i.y).sqrt().min(64.0);
    let c = ControllerSnapshot { focus_yaw: i.focus, stick_x: i.x, stick_y: i.y, stick_mag: mag, button_down: i.down | i.pressed, button_pressed: i.pressed };
    m.step(&c, 32768.0, true, false);
    LinkExt::step_projectiles(m, &[]);
}

fn tick_extra(m: &mut MarioRuntime, i: &In, extra: &[Target]) {
    let mag = (i.x * i.x + i.y * i.y).sqrt().min(64.0);
    let c = ControllerSnapshot { focus_yaw: i.focus, stick_x: i.x, stick_y: i.y, stick_mag: mag, button_down: i.down | i.pressed, button_pressed: i.pressed };
    m.step(&c, 32768.0, true, false);
    LinkExt::step_projectiles(m, extra);
}

fn idle(m: &mut MarioRuntime, n: usize) {
    for _ in 0..n {
        tick(m, &In::default());
    }
}

fn press(m: &mut MarioRuntime, bits: u32) {
    tick(m, &In { pressed: bits, ..Default::default() });
}

fn events(m: &mut MarioRuntime) -> Vec<LinkEvent> {
    m.link.take_events()
}

#[test]
fn b_starts_a_sword_swing_with_a_live_hitbox_then_returns_to_idle() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 2);
    press(&mut m, B_BUTTON);
    assert_eq!(m.link.mv, Mv::Swing);
    let mut live = 0;
    for _ in 0..40 {
        tick(&mut m, &In::default());
        if m.flags & MARIO_PUNCHING != 0 {
            live += 1;
            assert!(sword_reach_of(&m) > 0.0);
        }
    }
    assert!(live >= 3 && live <= 12, "hit window {live}");
    assert_eq!(m.link.mv, Mv::Free);
    assert_eq!(m.action, ACT_IDLE);
}

fn sword_reach_of(m: &MarioRuntime) -> f64 {
    jumpman_sim::characters::sword_reach(m)
}

#[test]
fn reach_follows_the_sword() {
    for (sword, want) in [(Sword::Kokiri, 70.0), (Sword::Master, 90.0), (Sword::Biggoron, 118.0)] {
        let mut m = link(Age::Adult, flat_world(), vec![]);
        m.link.inv.swords[sword as usize] = true;
        m.link.inv.sword = Some(sword);
        idle(&mut m, 1);
        press(&mut m, B_BUTTON);
        let mut best: f64 = 0.0;
        for _ in 0..20 {
            tick(&mut m, &In::default());
            best = best.max(sword_reach_of(&m));
        }
        assert_eq!(best, want);
    }
}

#[test]
fn three_presses_chain_into_the_combo_finisher() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    let mut seen = vec![];
    press(&mut m, B_BUTTON);
    for i in 0..80 {
        // Mash B every few ticks.
        let b = if i % 4 == 3 { B_BUTTON } else { 0 };
        tick(&mut m, &In { pressed: b, ..Default::default() });
        for e in events(&mut m) {
            if let LinkEvent::Swing(s) = e {
                seen.push(s);
            }
        }
    }
    assert!(seen.len() >= 2, "{seen:?}");
    assert!(seen.iter().any(|s| matches!(s, Swing::Combo(_))), "third hit is the finisher: {seen:?}");
}

trait SwingTimer {
    fn timer_in_swing(&self) -> u32;
}
impl SwingTimer for MarioRuntime {
    fn timer_in_swing(&self) -> u32 {
        self.action_timer as u32
    }
}

#[test]
fn z_target_plus_forward_stab_and_neutral_forward_slash() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    // Z-targeting along +Z, stick forward in world terms (camera yaw 0: stick up = intended yaw 0).
    tick(&mut m, &In { y: 64.0, focus: Some(0.0), pressed: B_BUTTON, ..Default::default() });
    let ev = events(&mut m);
    assert!(ev.contains(&LinkEvent::Swing(Swing::Stab)), "{ev:?}");

    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    tick(&mut m, &In { focus: Some(0.0), pressed: B_BUTTON, ..Default::default() });
    assert!(events(&mut m).contains(&LinkEvent::Swing(Swing::Forward)));

    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    press(&mut m, B_BUTTON);
    assert!(events(&mut m).contains(&LinkEvent::Swing(Swing::Right)), "untargeted standing slash is horizontal");
}

#[test]
fn holding_b_charges_and_releasing_does_a_big_spin_whose_turn_is_in_the_clip() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    press(&mut m, B_BUTTON);
    // Keep B held through the swing and the charge.
    for _ in 0..140 {
        tick(&mut m, &In { down: B_BUTTON, ..Default::default() });
    }
    assert_eq!(m.link.mv, Mv::Charge);
    assert!(m.link.charge.is_big(), "charge {:?}", m.link.charge);
    let start = m.face_angle[1];
    events(&mut m);
    tick(&mut m, &In::default()); // release
    assert_eq!(m.link.swing, Some(Swing::BigSpin));
    let mut turned = 0.0;
    let mut last = start;
    for _ in 0..60 {
        tick(&mut m, &In::default());
        let d = ((m.face_angle[1] - last + 98304.0) % 65536.0) - 32768.0;
        turned += d.abs();
        last = m.face_angle[1];
    }
    assert!(turned < 100.0, "facing must not turn (the clip carries the turn), turned {turned}");
    assert_eq!(m.link.mv, Mv::Free);
}

#[test]
fn a_short_hold_releases_a_normal_spin_and_a_tap_cancels() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    press(&mut m, B_BUTTON);
    for _ in 0..30 {
        tick(&mut m, &In { down: B_BUTTON, ..Default::default() });
    }
    assert_eq!(m.link.mv, Mv::Charge);
    for _ in 0..12 {
        tick(&mut m, &In { down: B_BUTTON, ..Default::default() });
    }
    tick(&mut m, &In::default());
    assert_eq!(m.link.swing, Some(Swing::Spin));
}

#[test]
fn spin_attack_hits_enemies_on_every_side_of_link() {
    // A breakable box behind Link is smashed by the sweeping spin hitbox.
    let boxes = vec![ObjectSpec::new(ObjectBehaviorKind::BreakableBox, [0.0, 0.0, -90.0], 0.0)];
    let mut m = link(Age::Adult, flat_world(), boxes);
    idle(&mut m, 1);
    press(&mut m, B_BUTTON);
    for _ in 0..140 {
        tick(&mut m, &In { down: B_BUTTON, ..Default::default() });
        jumpman_sim::object_manager::after_mario_step(&mut m);
    }
    let was_hit = |m: &MarioRuntime| m.objects[ObjId(0)].interact_status & INT_STATUS_WAS_ATTACKED != 0;
    let mut hit = false;
    tick(&mut m, &In::default());
    for _ in 0..40 {
        jumpman_sim::object_manager::step(&mut m, None, false);
        tick(&mut m, &In::default());
        hit |= was_hit(&m);
    }
    assert!(hit || !m.objects[ObjId(0)].active, "the box behind was struck");
}

#[test]
fn b_in_the_air_is_a_jump_slash_and_a_z_targeted_forward_jump_gives_a_flip_slash() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.pos = [0.0, 400.0, 0.0];
    m.set_action(ACT_FREEFALL, 0.0);
    tick(&mut m, &In::default());
    tick(&mut m, &In { pressed: B_BUTTON, ..Default::default() });
    assert_eq!(m.link.swing, Some(Swing::JumpSlash));
    assert_eq!(m.action, ACT_JUMP_KICK);
    let mut kicked = false;
    for _ in 0..80 {
        tick(&mut m, &In::default());
        kicked |= m.flags & MARIO_KICKING != 0;
        if m.action == ACT_IDLE {
            break;
        }
    }
    assert!(kicked, "the jump slash has a live window");
    assert_eq!(m.action, ACT_IDLE, "landed");
    assert!(m.pos[1].abs() < 1.0);

    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    tick(&mut m, &In { y: 64.0, focus: Some(0.0), pressed: A_BUTTON, ..Default::default() });
    assert_eq!(m.action, ACT_FREEFALL);
    assert!(m.vel[1] > 20.0 && m.forward_vel > 20.0, "forward jump");
    tick(&mut m, &In { focus: Some(0.0), pressed: B_BUTTON, ..Default::default() });
    assert_eq!(m.link.swing, Some(Swing::FlipSlash));
}

fn arrows_inv(m: &mut MarioRuntime) {
    m.link.inv.set_c(0, Some(CItem::Item(Item::Bow)), Age::Adult);
    m.link.inv.set_c(1, Some(CItem::Item(Item::FireArrow)), Age::Adult);
    m.link.inv.set_c(2, Some(CItem::Item(Item::Hookshot)), Age::Adult);
}

#[test]
fn bow_draws_while_the_c_button_is_down_and_fires_on_release() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    arrows_inv(&mut m);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, ..Default::default() });
    assert_eq!(m.link.mv, Mv::Aim);
    for _ in 0..10 {
        tick(&mut m, &In { down: C_LEFT, ..Default::default() });
    }
    assert_eq!(m.link.inv.ammo(Ammo::Arrow), 50, "nothing spent while drawing");
    tick(&mut m, &In::default());
    assert_eq!(m.link.inv.ammo(Ammo::Arrow), 49);
    assert!(m.link.take_events().contains(&LinkEvent::Shot(Kind::Arrow)));
    assert_eq!(m.link.projectiles.count(Kind::Arrow), 1);
}

#[test]
fn fire_arrow_costs_magic_and_burns_an_app_target() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    arrows_inv(&mut m);
    idle(&mut m, 1);
    let magic = m.link.inv.magic;
    tick(&mut m, &In { pressed: C_DOWN, down: C_DOWN, ..Default::default() });
    tick(&mut m, &In::default());
    assert_eq!(m.link.inv.magic, magic - 4, "fire arrows cost 4");
    let goomba = Target { id: EXTRA_TARGET_BASE, pos: [0.0, 0.0, 1200.0], radius: 40.0, height: 100.0, kind: TargetKind::Enemy };
    let mut hit = None;
    for _ in 0..12 {
        tick_extra(&mut m, &In::default(), &[goomba]);
        for e in events(&mut m) {
            if let LinkEvent::HitExtra { id, dmg, .. } = e {
                hit = Some((id, dmg));
            }
        }
    }
    assert_eq!(hit, Some((EXTRA_TARGET_BASE, Dmg::FireArrow)));
}

#[test]
fn magic_arrows_are_refused_without_magic_and_the_bow_without_arrows() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    arrows_inv(&mut m);
    m.link.inv.magic = 0;
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_DOWN, down: C_DOWN, ..Default::default() });
    assert_eq!(m.link.mv, Mv::Free);
    assert!(events(&mut m).contains(&LinkEvent::NoMagic(Item::FireArrow)));
    m.link.inv.ammo[Ammo::Arrow as usize] = 0;
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert!(events(&mut m).contains(&LinkEvent::NoAmmo(Item::Bow)));
}

#[test]
fn child_items_are_refused_by_adult_link_and_vice_versa() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    assert!(!m.link.inv.set_c(0, Some(CItem::Item(Item::Slingshot)), Age::Adult));
    // Even forced onto a button the use is refused.
    m.link.inv.c[0] = Some(CItem::Item(Item::Slingshot));
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert!(events(&mut m).contains(&LinkEvent::WrongAge(Item::Slingshot)));
    assert_eq!(m.link.mv, Mv::Free);
}

#[test]
fn slingshot_seed_hits_a_target_and_spends_a_seed() {
    let mut m = link(Age::Child, flat_world(), vec![]);
    assert_eq!(m.link.inv.c[0], Some(CItem::Item(Item::Slingshot)));
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    tick(&mut m, &In::default());
    assert_eq!(m.link.inv.ammo(Ammo::Seed), 49);
    let t = Target { id: EXTRA_TARGET_BASE + 1, pos: [0.0, 0.0, 900.0], radius: 40.0, height: 100.0, kind: TargetKind::Enemy };
    let mut hit = false;
    for _ in 0..14 {
        tick_extra(&mut m, &In::default(), &[t]);
        hit |= events(&mut m).iter().any(|e| matches!(e, LinkEvent::HitExtra { dmg: Dmg::Slingshot, .. }));
    }
    assert!(hit);
}

#[test]
fn aiming_follows_the_stick_and_a_z_target_locks_on() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    arrows_inv(&mut m);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    let yaw0 = m.link.aim.unwrap().yaw;
    for _ in 0..10 {
        tick(&mut m, &In { x: 64.0, y: 30.0, down: C_LEFT, ..Default::default() });
    }
    let a = m.link.aim.unwrap();
    assert!(a.yaw != yaw0 && a.pitch > 0.0, "{a:?}");
    // Lock-on: the target to the right at the same height.
    m.link.target_point = Some([800.0, 100.0, 0.0]);
    tick(&mut m, &In { down: C_LEFT, focus: Some(0.0), ..Default::default() });
    let a = m.link.aim.unwrap();
    assert!((a.yaw - 0x4000 as f64).abs() < 900.0, "aims at +X: {a:?}");
}

#[test]
fn boomerang_flies_out_and_back_and_only_one_at_a_time() {
    let mut m = link(Age::Child, flat_world(), vec![]);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_DOWN, down: C_DOWN, ..Default::default() });
    assert_eq!(m.link.projectiles.count(Kind::Boomerang), 1);
    tick(&mut m, &In { pressed: C_DOWN, down: C_DOWN, ..Default::default() });
    assert_eq!(m.link.projectiles.count(Kind::Boomerang), 1, "second throw refused");
    let mut caught = false;
    for _ in 0..120 {
        tick(&mut m, &In::default());
        caught |= m.link.projectiles.count(Kind::Boomerang) == 0;
    }
    assert!(caught);
}

#[test]
fn bomb_is_placed_counts_down_and_destroys_a_breakable_box() {
    let boxes = vec![ObjectSpec::new(ObjectBehaviorKind::BreakableBox, [0.0, 0.0, 150.0], 0.0)];
    let mut m = link(Age::Adult, flat_world(), boxes);
    m.link.inv.set_c(2, Some(CItem::Item(Item::Bomb)), Age::Adult);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_RIGHT, down: C_RIGHT, ..Default::default() });
    assert_eq!(m.link.inv.ammo(Ammo::Bomb), 39);
    assert_eq!(m.link.projectiles.count(Kind::Bomb), 1);
    // Step away so the blast cannot hurt Link, then wait for the fuse.
    let mut exploded = false;
    for i in 0..130 {
        tick(&mut m, &In { y: if i < 60 { -64.0 } else { 0.0 }, ..Default::default() });
        exploded |= events(&mut m).iter().any(|e| matches!(e, LinkEvent::Explosion { .. }));
    }
    assert!(exploded, "70 frames later");
    assert!(m.objects[ObjId(0)].interact_status & INT_STATUS_WAS_ATTACKED != 0 || !m.objects[ObjId(0)].active);
}

#[test]
fn bombchu_and_nut_use_ammo() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.set_c(0, Some(CItem::Item(Item::Bombchu)), Age::Adult);
    m.link.inv.set_c(1, Some(CItem::Item(Item::DekuNut)), Age::Adult);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.inv.ammo(Ammo::Bombchu), 19);
    assert_eq!(m.link.projectiles.count(Kind::Bombchu), 1);
    for _ in 0..20 {
        tick(&mut m, &In::default());
    }
    tick(&mut m, &In { pressed: C_DOWN, down: C_DOWN, ..Default::default() });
    assert_eq!(m.link.inv.ammo(Ammo::Nut), 39);
}

#[test]
fn dins_fire_spends_12_magic_and_scorches_a_nearby_enemy() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.set_c(0, Some(CItem::Item(Item::DinsFire)), Age::Adult);
    idle(&mut m, 1);
    let magic = m.link.inv.magic;
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.inv.magic, magic - 12);
    let t = Target { id: EXTRA_TARGET_BASE + 5, pos: [250.0, 0.0, 0.0], radius: 40.0, height: 100.0, kind: TargetKind::Enemy };
    let mut scorched = 0;
    for _ in 0..140 {
        tick_extra(&mut m, &In::default(), &[t]);
        scorched += events(&mut m).iter().filter(|e| matches!(e, LinkEvent::HitExtra { dmg: Dmg::FireMagic, .. })).count();
    }
    assert_eq!(scorched, 1);
}

#[test]
fn farores_wind_sets_a_point_and_warps_back() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.set_c(0, Some(CItem::Item(Item::FaroresWind)), Age::Adult);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert!(m.link.farore.is_some());
    idle(&mut m, 70);
    for _ in 0..60 {
        tick(&mut m, &In { y: 64.0, ..Default::default() });
    }
    assert!(m.pos[2] > 100.0);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    // Wait for the cast animation, magic spent twice.
    assert!(m.pos[2].abs() < 50.0, "warped back to the marker: {:?}", m.pos);
    assert_eq!(m.link.inv.magic, MAGIC_DOUBLE_METER - 48);
}

#[test]
fn nayrus_love_blocks_damage_for_a_minute() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.set_c(0, Some(CItem::Item(Item::NayrusLove)), Age::Adult);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert!(m.link.nayru > 1700.0);
    idle(&mut m, 60);
    assert!(!m.take_damage(1.0, None), "invincible");
    let hearts = m.link.inv.health;
    assert_eq!(hearts, 20 * HEART);
}

#[test]
fn hearts_replace_sm64_wedges_and_a_potion_heals() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 3);
    assert!(m.take_damage(1.0, None));
    for _ in 0..30 {
        tick(&mut m, &In::default());
    }
    assert_eq!(m.link.inv.health, 19 * HEART, "one wedge = one heart");
    assert_eq!(m.health, m.max_health, "Mario's meter is refilled");
    m.link.inv.set_c(0, Some(CItem::Bottle(0)), Age::Adult);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.inv.health, 20 * HEART);
    assert_eq!(m.link.inv.bottles[0], Some(Bottle::Empty));
    // Empty bottle does nothing.
    for _ in 0..110 {
        tick(&mut m, &In::default());
    }
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.mv, Mv::Free);
}

#[test]
fn running_out_of_hearts_kills_link() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.health = HEART;
    idle(&mut m, 3);
    m.take_damage(1.0, None);
    for _ in 0..60 {
        tick(&mut m, &In::default());
    }
    assert_eq!(m.link.inv.health, 0);
    assert!(m.health < 0x100 as f64 || m.action == ACT_STANDING_DEATH || m.action & ACT_FLAG_INTANGIBLE != 0 || m.health <= 0xff as f64 + 1.0);
}

#[test]
fn ocarina_plays_notes_and_recognises_the_song_of_time() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.set_c(0, Some(CItem::Item(Item::Ocarina)), Age::Adult);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.mv, Mv::Ocarina);
    let mut song = None;
    let mut notes = vec![];
    for n in Song::Time.notes() {
        let bit = match n {
            Note::A => A_BUTTON,
            Note::CDown => C_DOWN,
            Note::CRight => C_RIGHT,
            Note::CLeft => C_LEFT,
            Note::CUp => C_UP,
        };
        tick(&mut m, &In { pressed: bit, ..Default::default() });
        tick(&mut m, &In::default());
        for e in events(&mut m) {
            match e {
                LinkEvent::Song(s) => song = Some(s),
                LinkEvent::Note(n) => notes.push(n),
                _ => {}
            }
        }
    }
    assert_eq!(notes.len(), 6);
    assert_eq!(song, Some(Song::Time));
    // B puts the ocarina away.
    press(&mut m, B_BUTTON);
    assert_eq!(m.link.mv, Mv::Free);
}

#[test]
fn hookshot_latches_a_wall_and_pulls_link_to_it() {
    let mut w = flat_world();
    let o = SurfaceOptions::default();
    // A wall at z = 900 (normal facing -Z), 600 high.
    w.add_triangle([-1500.0, 0.0, 900.0], [1500.0, 600.0, 900.0], [-1500.0, 600.0, 900.0], &o);
    w.add_triangle([-1500.0, 0.0, 900.0], [1500.0, 0.0, 900.0], [1500.0, 600.0, 900.0], &o);
    let mut m = link(Age::Adult, w, vec![]);
    m.link.inv.set_c(2, Some(CItem::Item(Item::Hookshot)), Age::Adult);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_RIGHT, down: C_RIGHT, ..Default::default() });
    tick(&mut m, &In::default());
    let mut attached = false;
    let mut flew = false;
    for _ in 0..60 {
        tick(&mut m, &In::default());
        attached |= events(&mut m).contains(&LinkEvent::HookAttached);
        flew |= m.link.mv == Mv::HookFly;
    }
    assert!(attached && flew, "attached={attached} flew={flew}");
    assert!(m.pos[2] > 500.0, "pulled toward the wall: {:?}", m.pos);
}

#[test]
fn hover_boots_float_after_leaving_a_ledge() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.boots = Boots::Hover;
    idle(&mut m, 2);
    m.pos[1] = 600.0;
    m.set_action(ACT_FREEFALL, 0.0);
    m.vel = [0.0, -5.0, 0.0];
    for _ in 0..30 {
        tick(&mut m, &In { down: A_BUTTON, ..Default::default() });
    }
    assert!(m.pos[1] > 590.0, "hovering: {}", m.pos[1]);
    let mut m2 = link(Age::Adult, flat_world(), vec![]);
    m2.pos[1] = 600.0;
    m2.set_action(ACT_FREEFALL, 0.0);
    m2.vel = [0.0, -5.0, 0.0];
    for _ in 0..30 {
        tick(&mut m2, &In { down: A_BUTTON, ..Default::default() });
    }
    assert!(m2.pos[1] < 500.0, "no hover without the boots: {}", m2.pos[1]);
}

#[test]
fn deku_stick_swaps_the_weapon_and_the_hammer_swings_two_handed() {
    let mut m = link(Age::Child, flat_world(), vec![]);
    m.link.inv.set_c(0, Some(CItem::Item(Item::DekuStick)), Age::Child);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.weapon(), Weapon::Stick);

    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.set_c(0, Some(CItem::Item(Item::MegatonHammer)), Age::Adult);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.swing, Some(Swing::HammerForward));
    let mut slammed = false;
    for _ in 0..40 {
        tick(&mut m, &In::default());
        slammed |= events(&mut m).iter().any(|e| matches!(e, LinkEvent::HammerSlam(_)));
    }
    assert!(slammed);
}

#[test]
fn legacy_controller_is_untouched_while_the_layer_is_disabled() {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    let c = ControllerSnapshot { button_pressed: B_BUTTON, button_down: B_BUTTON, ..Default::default() };
    m.step(&c, 32768.0, true, false);
    assert_eq!(m.action, ACT_PUNCHING);
    assert_eq!(m.action_arg, 0x53 as f64, "the legacy SLASH");
}

// ---------------------------------------------------------------- first-person aim and held items

#[test]
fn aim_mode_enters_on_the_c_button_and_exits_on_release_cancel_or_fire() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    arrows_inv(&mut m);
    idle(&mut m, 1);
    assert!(!m.link.first_person());
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert!(m.link.first_person(), "entered");
    assert_eq!(m.link.held(), Some(Item::Bow));
    // A cancels without spending an arrow.
    tick(&mut m, &In { pressed: A_BUTTON, down: C_LEFT | A_BUTTON, ..Default::default() });
    assert!(!m.link.first_person());
    assert_eq!(m.link.inv.ammo(Ammo::Arrow), 50);
    assert_eq!(m.link.held(), None);
    // Releasing fires and the bow stays in the hand through the shot animation, then goes away.
    idle(&mut m, 2);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    tick(&mut m, &In::default());
    assert!(!m.link.first_person());
    assert_eq!(m.link.held(), Some(Item::Bow));
    idle(&mut m, 30);
    assert_eq!(m.link.held(), None);
    assert_eq!(m.link.inv.ammo(Ammo::Arrow), 49);
}

#[test]
fn aim_limits_stick_and_drag_turn_and_invert() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    arrows_inv(&mut m);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    // Looking up for a long time stops at the limit.
    for _ in 0..400 {
        tick(&mut m, &In { y: 64.0, down: C_LEFT, ..Default::default() });
    }
    assert!((m.link.aim.unwrap().pitch - AIM_PITCH_MAX).abs() < 1e-9);
    for _ in 0..900 {
        tick(&mut m, &In { y: -64.0, down: C_LEFT, ..Default::default() });
    }
    assert!((m.link.aim.unwrap().pitch - AIM_PITCH_MIN).abs() < 1e-9);
    // A drag turns the aim once (it is consumed).
    let y0 = m.link.aim.unwrap().yaw;
    m.link.look_drag = [100.0, 0.0];
    tick(&mut m, &In { down: C_LEFT, ..Default::default() });
    let y1 = m.link.aim.unwrap().yaw;
    tick(&mut m, &In { down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.aim.unwrap().yaw, y1, "the drag is consumed");
    assert_ne!(y1, y0);
    // The right stick adds to the left one; invert flips the pitch.
    assert_eq!(aim_turn(0.0, 10.0, [0.0, 0.0], [0.0; 2], false).1, -aim_turn(0.0, 10.0, [0.0, 0.0], [0.0; 2], true).1);
    assert_eq!(aim_turn(10.0, 0.0, [20.0, 0.0], [0.0; 2], false), aim_turn(30.0, 0.0, [0.0; 2], [0.0; 2], false));
    // Dragging up looks up, right turns right.
    let (yaw, pitch) = aim_turn(0.0, 0.0, [0.0; 2], [10.0, -10.0], false);
    assert!(yaw < 0.0 && pitch > 0.0);
}

#[test]
fn slingshot_aim_uses_a_seed_and_the_hookshot_none() {
    let mut m = link(Age::Child, flat_world(), vec![]);
    m.link.inv.set_c(0, Some(CItem::Item(Item::Slingshot)), Age::Child);
    idle(&mut m, 1);
    let seeds = m.link.inv.ammo(Ammo::Seed);
    tick(&mut m, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    assert_eq!(m.link.held(), Some(Item::Slingshot));
    assert_eq!(m.link.inv.ammo(Ammo::Seed), seeds, "nothing spent while aiming");
    tick(&mut m, &In::default());
    assert_eq!(m.link.inv.ammo(Ammo::Seed), seeds - 1);
    let mut h = link(Age::Adult, flat_world(), vec![]);
    h.link.inv.set_c(2, Some(CItem::Item(Item::Hookshot)), Age::Adult);
    idle(&mut h, 1);
    tick(&mut h, &In { pressed: C_RIGHT, down: C_RIGHT, ..Default::default() });
    assert_eq!(h.link.held(), Some(Item::Hookshot));
    assert!(h.link.first_person());
}

#[test]
fn bomb_explosion_reports_its_blast_and_terrain_impacts_are_reported() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.link.inv.set_c(2, Some(CItem::Item(Item::Bomb)), Age::Adult);
    idle(&mut m, 1);
    tick(&mut m, &In { pressed: C_RIGHT, down: C_RIGHT, ..Default::default() });
    let mut blast = None;
    for i in 0..130 {
        tick(&mut m, &In { y: if i < 60 { -64.0 } else { 0.0 }, ..Default::default() });
        for e in events(&mut m) {
            if let LinkEvent::Explosion { radius, .. } = e {
                blast = Some((i, radius));
            }
        }
    }
    let (at, radius) = blast.expect("exploded");
    // 70 OoT frames = 105 ticks of fuse (minus the throw frames).
    assert!((95..=110).contains(&at), "{at}");
    assert!(radius > 100.0, "{radius}");
    // An arrow into a wall reports the impact.
    let mut w = flat_world();
    let o = SurfaceOptions::default();
    w.add_triangle([-2000.0, -500.0, 600.0], [2000.0, 3000.0, 600.0], [-2000.0, 3000.0, 600.0], &o);
    w.add_triangle([-2000.0, -500.0, 600.0], [2000.0, -500.0, 600.0], [2000.0, 3000.0, 600.0], &o);
    let mut a = link(Age::Adult, w, vec![]);
    arrows_inv(&mut a);
    idle(&mut a, 1);
    tick(&mut a, &In { pressed: C_LEFT, down: C_LEFT, ..Default::default() });
    tick(&mut a, &In::default());
    let mut impact = false;
    let mut seen = vec![];
    for _ in 0..20 {
        tick(&mut a, &In::default());
        let ev = events(&mut a);
        impact |= ev.iter().any(|e| matches!(e, LinkEvent::Impact { kind: Kind::Arrow, .. }));
        seen.extend(ev);
    }
    let _ = seen;
    assert!(impact, "the arrow reports where it stuck: {:?}", a.link.projectiles.list.iter().map(|p| (p.kind, p.phase, p.pos)).collect::<Vec<_>>());
}

// ---- sword animation timing and selection (z_player.c D_80854190, func_80837818, func_80837948)

/// Swing clip and hint frame (OoT frames at 20 fps) for each tick.
fn swing_trace(m: &mut MarioRuntime, ticks: usize) -> Vec<(&'static str, f64)> {
    let mut out = vec![];
    for _ in 0..ticks {
        tick(m, &In::default());
        if let Some(p) = m.link.pose {
            out.push((p.clip, p.frame * 20.0 / 30.0));
        }
    }
    out
}

#[test]
fn a_slash_lasts_its_clip_then_plays_the_end_clip_instead_of_freezing() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 2);
    press(&mut m, B_BUTTON);
    let trace = swing_trace(&mut m, 30);
    // `fighter_Lside_kiru` has 5 frames: it plays frames 0..4 and never past them, and no frame is held for an invented tail.
    let slash: Vec<_> = trace.iter().filter(|(c, _)| *c == "slashR").collect();
    assert!(slash.len() >= 5 && slash.len() <= 8, "{} ticks of the swing clip", slash.len());
    assert!(slash.iter().all(|(_, f)| *f <= 4.0 + 1e-9));
    assert!(slash.windows(2).filter(|w| w[0].1 == w[1].1).count() <= 1, "no frozen tail: {slash:?}");
    // Then the `_end` clip (9 frames at 30 fps = 9 ticks) takes over.
    let end: Vec<_> = trace.iter().filter(|(c, _)| *c == "slashREnd").collect();
    assert!(end.len() >= 8 && end.len() <= 9, "{} ticks of the end clip", end.len());
    // Hint frames are pre-scaled so the renderer's 20/30 conversion leaves one clip frame per tick.
    assert!((end[1].1 - end[0].1 - 1.0).abs() < 1e-9, "the end clip runs at 30 fps: {end:?}");
}

#[test]
fn the_third_matching_slash_is_the_finisher_and_the_combo_window_closes() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    let mut seen = vec![];
    for i in 0..60 {
        let b = if i % 8 == 0 { B_BUTTON } else { 0 };
        tick(&mut m, &In { pressed: b, ..Default::default() });
        for e in events(&mut m) {
            if let LinkEvent::Swing(s) = e {
                seen.push(s);
            }
        }
    }
    // The same kind twice, then the finisher (`unk_845` counting `meleeWeaponAnimation` repeats), not an alternating left/right.
    assert_eq!(&seen[..3], &[Swing::Right, Swing::Right, Swing::Combo(Direction::Right)], "{seen:?}");
    // A press after the 8-frame window (12 ticks) starts the chain over.
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    press(&mut m, B_BUTTON);
    events(&mut m);
    idle(&mut m, 40);
    press(&mut m, B_BUTTON);
    assert!(events(&mut m).iter().all(|e| *e != LinkEvent::Swing(Swing::Combo(Direction::Right))));
    assert_eq!(m.link.combo, 1);
}

#[test]
fn stick_direction_picks_the_slash_against_links_facing() {
    // Facing +Z (yaw 0); the stick is read against it. `D_80854480`: forward stab (Z only), left and back RIGHT_SLASH, right LEFT_SLASH.
    for (x, y, focus, want) in [
        (0.0, 64.0, Some(0.0), Swing::Stab),
        (0.0, 64.0, None, Swing::Forward),
        (-64.0, 0.0, Some(0.0), Swing::Right),
        (64.0, 0.0, Some(0.0), Swing::Left),
        (0.0, -64.0, Some(0.0), Swing::Right),
        (0.0, 30.0, Some(0.0), Swing::Forward),
        (0.0, 0.0, None, Swing::Right),
    ] {
        let mut m = link(Age::Adult, flat_world(), vec![]);
        idle(&mut m, 1);
        tick(&mut m, &In { x, y, focus, pressed: B_BUTTON, ..Default::default() });
        let got = events(&mut m).into_iter().find_map(|e| if let LinkEvent::Swing(s) = e { Some(s) } else { None });
        assert_eq!(got, Some(want), "stick ({x},{y}) focus {focus:?}");
    }
}

#[test]
fn swing_lengths_are_the_clip_frame_counts() {
    // `link_animetion.xml` frame counts of the swing clips.
    for (s, frames) in [
        (Swing::Forward, 5.0), (Swing::Right, 5.0), (Swing::Left, 5.0), (Swing::Stab, 4.0), (Swing::Combo(Direction::Forward), 6.0),
        (Swing::Combo(Direction::Right), 9.0), (Swing::Combo(Direction::Left), 7.0), (Swing::Combo(Direction::Stab), 10.0),
        (Swing::Spin, 13.0), (Swing::BigSpin, 17.0), (Swing::HammerForward, 11.0), (Swing::HammerSide, 12.0),
    ] {
        assert_eq!(s.info().frames, frames, "{s:?}");
        assert_eq!(s.total_ticks(), (frames - 1.0) * 1.5, "{s:?}");
        // The hit window always ends inside the clip.
        assert!(s.info().hit_end <= frames - 1.0, "{s:?}");
    }
}

#[test]
fn jump_slash_blade_goes_live_at_frame_seven_stays_until_landing_and_the_landing_plays_the_hit_clip() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.pos = [0.0, 600.0, 0.0];
    m.set_action(ACT_FREEFALL, 0.0);
    tick(&mut m, &In::default());
    tick(&mut m, &In { pressed: B_BUTTON, ..Default::default() });
    let mut first_live = None;
    let mut last_air_live = false;
    let mut hit_clip = false;
    for i in 0..200 {
        tick(&mut m, &In::default());
        let live = m.flags & MARIO_KICKING != 0;
        if live && first_live.is_none() {
            first_live = Some(i);
        }
        if m.action & ACT_FLAG_AIR != 0 {
            last_air_live = live;
        }
        hit_clip |= m.link.pose.is_some_and(|p| p.clip == "jumpSlashHit");
    }
    assert!(first_live.is_some_and(|t| (8..=12).contains(&t)), "first live tick {first_live:?} (frame 7 = tick 10.5)");
    assert!(last_air_live, "the blade stays live while he falls (no 16-frame timeout)");
    assert!(hit_clip, "landing plays `Lpower_jump_kiru_hit`");
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn b_held_through_a_slash_starts_the_charge_one_frame_before_the_combo_window_ends() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    press(&mut m, B_BUTTON);
    let mut began = None;
    for i in 0..30 {
        tick(&mut m, &In { down: B_BUTTON, ..Default::default() });
        if m.link.mv == Mv::Charge && began.is_none() {
            began = Some(i);
        }
    }
    // `unk_844` counts 8 frames (12 ticks) from the swing start and the charge begins at 1: about tick 10.
    assert!(began.is_some_and(|t| (8..=12).contains(&t)), "{began:?}");
}

#[test]
fn the_sword_stays_out_after_a_swing() {
    let mut m = link(Age::Adult, flat_world(), vec![]);
    idle(&mut m, 1);
    press(&mut m, B_BUTTON);
    idle(&mut m, 60);
    assert!(m.link.sword_out > 0.0);
    idle(&mut m, 200);
    assert_eq!(m.link.sword_out, 0.0);
}

#[test]
fn b_during_an_ordinary_jump_action_is_the_jump_slash_too() {
    // OoT has no A jump (A rolls); the SM64 jump actions can still happen (springs, cannons' exits): B there swings as well.
    let mut m = link(Age::Adult, flat_world(), vec![]);
    m.pos = [0.0, 400.0, 0.0];
    m.set_action(ACT_JUMP, 0.0);
    m.vel[1] = 10.0;
    tick(&mut m, &In::default());
    tick(&mut m, &In { pressed: B_BUTTON, ..Default::default() });
    assert_eq!(m.link.swing, Some(Swing::JumpSlash));
}
