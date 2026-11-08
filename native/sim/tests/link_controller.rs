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
    m.step(&c, 0.0, true, false);
    LinkExt::step_projectiles(m, &[]);
}

fn tick_extra(m: &mut MarioRuntime, i: &In, extra: &[Target]) {
    let mag = (i.x * i.x + i.y * i.y).sqrt().min(64.0);
    let c = ControllerSnapshot { focus_yaw: i.focus, stick_x: i.x, stick_y: i.y, stick_mag: mag, button_down: i.down | i.pressed, button_pressed: i.pressed };
    m.step(&c, 0.0, true, false);
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
    for _ in 0..80 {
        // Mash B every few ticks.
        let b = if m.link.mv == Mv::Swing && m.timer_in_swing() % 5 == 3 { B_BUTTON } else { 0 };
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
fn holding_b_charges_and_releasing_does_a_big_spin_that_sweeps_a_full_turn() {
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
    assert!(turned > 60000.0, "a full revolution, turned {turned}");
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
    assert!((a.yaw - 0x4000 as f64).abs() < 400.0, "aims at +X: {a:?}");
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
    for _ in 0..80 {
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
    m.step(&c, 0.0, true, false);
    assert_eq!(m.action, ACT_PUNCHING);
    assert_eq!(m.action_arg, 0x53 as f64, "the legacy SLASH");
}
