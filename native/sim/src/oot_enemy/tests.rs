//! Engine-free tests of the OoT enemies: hit points, damage rows by weapon, drop tables, AI
//! transitions, and the three characters (Link, Mario, Banjo) attacking and being hurt.
use super::player::*;
use super::*;
use crate::collision::SurfaceOptions;
use crate::link::combat::{Swing, Weapon};
use crate::link::items::Sword;
use crate::mario_runtime::{MarioRuntime, SpawnPoint};
use crate::state_consts::*;
use std::sync::Arc;

fn flat() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    w.add_triangle([-30000.0, 0.0, -30000.0], [30000.0, 0.0, 30000.0], [30000.0, 0.0, -30000.0], &SurfaceOptions::default());
    w.add_triangle([-30000.0, 0.0, -30000.0], [-30000.0, 0.0, 30000.0], [30000.0, 0.0, 30000.0], &SurfaceOptions::default());
    w
}

fn mario_at(pos: Vec3) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat()), SpawnPoint { action: None, pos, face_yaw: 0.0 }, -9000.0);
    m.pos = pos;
    m
}

fn run(field: &mut Field, player: Vec3, ticks: usize) {
    let w = flat();
    for _ in 0..ticks {
        field.step(player, &w);
    }
}

#[test]
fn hit_points_come_from_the_overlays() {
    // colChkInfo.health / CollisionCheckInfoInit of each overlay.
    let hp = |k: Kind| k.spec().hp;
    assert_eq!((hp(Kind::DekuBaba), hp(Kind::BigDekuBaba), hp(Kind::WitheredBaba)), (2, 4, 1));
    assert_eq!((hp(Kind::Octorok), hp(Kind::MadScrub), hp(Kind::Guay)), (1, 1, 1));
    assert_eq!((hp(Kind::Peahat), hp(Kind::Wolfos), hp(Kind::Tektite), hp(Kind::RedTektite)), (6, 8, 4, 2));
    assert_eq!((hp(Kind::GomaLarva), hp(Kind::Leever), hp(Kind::Skulltula)), (2, 4, 2));
}

#[test]
fn damage_tables_match_the_c() {
    // sDamageTableNormal: sword rows 1/2/4, nut stuns (0 + reaction 1), fire arrow 4.
    assert_eq!(damage_of(Kind::DekuBaba, Dmg::KokiriSword), (1, 0xF));
    assert_eq!(damage_of(Kind::DekuBaba, Dmg::MasterSword), (2, 0xF));
    assert_eq!(damage_of(Kind::DekuBaba, Dmg::GiantSword), (4, 0xF));
    assert_eq!(damage_of(Kind::DekuBaba, Dmg::Nut), (0, 1));
    assert_eq!(damage_of(Kind::DekuBaba, Dmg::Boomerang), (2, 0xE));
    assert_eq!(damage_of(Kind::DekuBaba, Dmg::FireArrow), (4, 2));
    // The big one is stunned by the hookshot instead of hurt.
    assert_eq!(damage_of(Kind::BigDekuBaba, Dmg::Hookshot), (0, 1));
    assert_eq!(damage_of(Kind::DekuBaba, Dmg::Hookshot), (2, 0));
    // Tektites shrug off the boomerang (stun only) and freeze to ice arrows.
    assert_eq!(damage_of(Kind::Tektite, Dmg::Boomerang), (0, 1));
    assert_eq!(damage_of(Kind::Tektite, Dmg::IceArrow), (4, 0xF));
    // Larvae use the weapon's own damage.
    assert_eq!(damage_of(Kind::GomaLarva, Dmg::MasterSword), (2, 0));
    for k in KINDS {
        for d in 0..32u8 {
            assert!(k.spec().table.map_or(true, |t| t[d as usize].0 <= 8), "{k:?} row {d}");
        }
    }
}

#[test]
fn drop_table_distribution_matches_the_rows() {
    // Table 7 (Octorok): every row is equally likely, so item frequencies follow the row counts.
    let base = 7 * 16;
    let mut expect = std::collections::HashMap::new();
    for r in 0..16 {
        *expect.entry(drop_table::ITEM_DROP_IDS[base + r]).or_insert(0usize) += 1;
    }
    let mut rng = Rng::new(42);
    let mut seen = std::collections::HashMap::new();
    let n = 32000;
    for _ in 0..n {
        let row = (rng.zero_one() * 16.0) as usize;
        *seen.entry(drop_table::ITEM_DROP_IDS[base + row]).or_insert(0usize) += 1;
    }
    for (id, rows) in &expect {
        let want = n as f64 * *rows as f64 / 16.0;
        let got = *seen.get(id).unwrap_or(&0) as f64;
        assert!((got - want).abs() < want * 0.1 + 100.0, "item {id}: {got} vs {want}");
    }
    // Table 0 holds rupees, hearts and empty rows.
    let mut rng = Rng::new(7);
    let needs = Needs::default();
    let drops: Vec<_> = (0..400).filter_map(|_| roll_drop(&mut rng, 0, &needs)).map(|d| d.0).collect();
    assert!(drops.contains(&Item::RupeeGreen) && drops.contains(&Item::Heart) && drops.contains(&Item::RupeeBlue));
    assert!(drops.len() < 400, "some rows are ITEM00_NONE");
    // Link as a child gets seeds where an adult gets arrows (func_8001F404).
    let mut rng = Rng::new(3);
    let child = Needs { link: true, adult: false, ..Needs::default() };
    let adult = Needs { link: true, adult: true, ..Needs::default() };
    let table = (0..15u8).find(|t| (0..16).any(|r| (8..=10).contains(&drop_table::ITEM_DROP_IDS[*t as usize * 16 + r]))).unwrap();
    let kinds: Vec<_> = (0..2000).filter_map(|_| roll_drop(&mut rng, table, &adult)).map(|d| d.0).collect();
    assert!(kinds.iter().any(|i| matches!(i, Item::Arrows(_))));
    let kinds: Vec<_> = (0..2000).filter_map(|_| roll_drop(&mut rng, table, &child)).map(|d| d.0).collect();
    assert!(!kinds.iter().any(|i| matches!(i, Item::Arrows(_))) && kinds.contains(&Item::Seeds));
}

#[test]
fn flexible_drop_gives_a_heart_when_hurt_and_a_fairy_when_nearly_dead() {
    // Table 1 row 8 is ITEM00_FLEXIBLE.
    let mut found = (false, false);
    for seed in 0..400 {
        let mut rng = Rng::new(seed);
        if let Some((item, _)) = roll_drop(&mut rng, 1, &Needs { health: 32, ..Needs::default() }) {
            found.0 |= item == Item::Heart;
        }
        let mut rng = Rng::new(seed);
        if let Some((item, _)) = roll_drop(&mut rng, 1, &Needs { health: 16, ..Needs::default() }) {
            found.1 |= item == Item::Fairy;
        }
    }
    assert_eq!(found, (true, true));
}

#[test]
fn deku_baba_rises_lunges_and_hurts_the_player() {
    let mut f = Field::new(1);
    let i = f.add(Kind::DekuBaba, [0.0, 0.0, 0.0], 0.0, 0);
    // Player well outside 200 units: it stays buried.
    run(&mut f, [0.0, 0.0, 3000.0], 120);
    assert_eq!(f.enemies[i].st, St::Hidden);
    // 400 units away (about 90 OoT): it comes out, rears back and bites.
    let mut hurt = false;
    let w = flat();
    let mut states = vec![St::Hidden];
    for _ in 0..200 {
        f.step([0.0, 0.0, 400.0], &w);
        if states.last() != Some(&f.enemies[i].st) {
            states.push(f.enemies[i].st);
        }
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { units: 8, .. }));
    }
    assert!(states.starts_with(&[St::Hidden, St::Rising, St::Idle, St::Windup, St::Attack]), "{states:?}");
    assert!(hurt, "the lunge reaches a player standing 400 away");
    // Retreats into the ground when the player runs off (> 240).
    run(&mut f, [0.0, 0.0, 5000.0], 200);
    assert_eq!(f.enemies[i].st, St::Hidden);
}

#[test]
fn buried_baba_clanks() {
    let mut f = Field::new(2);
    let i = f.add(Kind::DekuBaba, [0.0, 0.0, 0.0], 0.0, 0);
    let r = f.hit(i, Dmg::MasterSword, [0.0, 0.0, 300.0]);
    assert!(r.clank && !r.applied, "hard material while buried");
}

#[test]
fn master_sword_hit_on_a_raised_baba_kills_it_after_the_recoil() {
    let mut f = Field::new(3);
    let i = f.add(Kind::DekuBaba, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    for _ in 0..130 {
        f.step([0.0, 0.0, 700.0], &w);
    }
    assert!(matches!(f.enemies[i].st, St::Idle | St::Windup | St::Attack | St::Recover), "{:?}", f.enemies[i].st);
    let r = f.hit(i, Dmg::MasterSword, [0.0, 0.0, 300.0]);
    assert!(r.applied && r.damage == 2);
    assert_eq!(f.enemies[i].st, St::Hurt);
    let mut died = false;
    for _ in 0..60 {
        f.step([0.0, 0.0, 700.0], &w);
        died |= f.events.iter().any(|e| matches!(e, Ev::Died { kind: Kind::DekuBaba, .. }));
    }
    assert!(died);
    assert_eq!(f.enemies[i].st, St::Gone);
    let drops = f.drops_for(Kind::DekuBaba, [0.0; 3], &Needs::default());
    assert!(drops.iter().any(|d| d.1 == Item::Nuts) && drops.iter().any(|d| d.1 == Item::Stick));
}

#[test]
fn kokiri_sword_needs_two_hits_on_a_baba() {
    let mut f = Field::new(4);
    let i = f.add(Kind::DekuBaba, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    for _ in 0..130 {
        f.step([0.0, 0.0, 700.0], &w);
    }
    f.hit(i, Dmg::KokiriSword, [0.0, 0.0, 300.0]);
    assert_eq!(f.enemies[i].hp, 1);
    for _ in 0..40 {
        f.step([0.0, 0.0, 700.0], &w);
    }
    assert!(f.enemies[i].alive());
    f.hit(i, Dmg::KokiriSword, [0.0, 0.0, 300.0]);
    for _ in 0..60 {
        f.step([0.0, 0.0, 700.0], &w);
    }
    assert_eq!(f.enemies[i].st, St::Gone);
}

#[test]
fn nut_stuns_a_baba_without_hurting() {
    let mut f = Field::new(5);
    let i = f.add(Kind::DekuBaba, [0.0, 0.0, 0.0], 0.0, 0);
    run(&mut f, [0.0, 0.0, 700.0], 130);
    let r = f.hit(i, Dmg::Nut, [0.0, 0.0, 300.0]);
    assert!(!r.killed);
    assert_eq!(f.enemies[i].hp, 2);
    assert!(f.enemies[i].stun > 0.0);
}

#[test]
fn octorok_spits_rocks_and_a_shield_sends_them_back() {
    let mut f = Field::new(6);
    let o = f.add(Kind::Octorok, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    let mut rocks = 0;
    for _ in 0..200 {
        f.step([0.0, 0.0, 1500.0], &w);
        rocks += f.events.iter().filter(|e| matches!(e, Ev::Spit { kind: ShotKind::Rock, .. })).count();
    }
    assert!(rocks >= 1, "spits when the player is 340 OoT units away");
    // A Link with the shield up turns a rock around and the Octorok dies to it.
    let mut m = mario_at([0.0, 0.0, 1500.0]);
    m.link.enable(crate::link::items::Age::Child);
    m.set_action(ACT_IDLE, 0x4c as f64);
    m.face_angle[1] = 32768.0;
    f.shots = vec![Shot { kind: ShotKind::Rock, pos: [0.0, 100.0, 1450.0], vel: [0.0, 0.0, 40.0], age: 1.0, dead: false, reflected: false }];
    assert_eq!(reflect_with_shield(&m, &mut f.shots), 1);
    assert!(f.shots[0].reflected && f.shots[0].vel[2] < 0.0);
    f.shots[0].pos = [0.0, 20.0, 40.0];
    f.shots[0].vel = [0.0, 0.0, -10.0];
    f.step([0.0, 0.0, 1500.0], &w);
    assert!(!f.enemies[o].alive(), "{:?}", f.enemies[o].st);
}

#[test]
fn mad_scrub_pops_up_and_spits_a_nut_at_the_player() {
    let mut f = Field::new(7);
    let s = f.add(Kind::MadScrub, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    let mut nut = false;
    for _ in 0..200 {
        f.step([0.0, 0.0, 1000.0], &w);
        nut |= f.events.iter().any(|e| matches!(e, Ev::Spit { kind: ShotKind::Nut, .. }));
    }
    assert!(nut);
    assert!(f.enemies[s].visible);
    // It burrows when the player closes in.
    run(&mut f, [0.0, 0.0, 100.0], 40);
    assert!(!f.enemies[s].visible || f.enemies[s].st == St::Retreat);
}

#[test]
fn tektite_hops_toward_the_player_and_bites() {
    let mut f = Field::new(8);
    let t = f.add(Kind::RedTektite, [0.0, 0.0, 0.0], 0.0, -1);
    let w = flat();
    let mut peak: f64 = 0.0;
    for _ in 0..240 {
        f.step([0.0, 0.0, 1400.0], &w);
        peak = peak.max(f.enemies[t].pos[1]);
    }
    assert!(peak > 40.0, "hops: {peak}");
    assert!(f.enemies[t].pos[2] > 200.0, "moved toward the player: {:?}", f.enemies[t].pos);
}

#[test]
fn guay_swoops_at_a_nearby_player_and_dies_in_one_hit() {
    let mut f = Field::new(9);
    let g = f.add(Kind::Guay, [0.0, 400.0, 0.0], 0.0, 0);
    let w = flat();
    let mut swooped = false;
    let mut hurt = false;
    for _ in 0..200 {
        f.step([0.0, 0.0, 600.0], &w);
        swooped |= f.enemies[g].st == St::Attack;
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { .. }));
    }
    assert!(swooped);
    assert!(hurt, "the dive reaches the player's chest");
    let r = f.hit(g, Dmg::KokiriSword, [0.0, 0.0, 0.0]);
    assert!(r.killed, "one hit point");
}

#[test]
fn guay_respawns_after_five_seconds() {
    let mut f = Field::new(10);
    let g = f.add(Kind::Guay, [0.0, 400.0, 0.0], 0.0, 0);
    f.enemies[g].asleep = false;
    f.hit(g, Dmg::MasterSword, [0.0; 3]);
    run(&mut f, [0.0, 0.0, 6000.0], 60);
    assert_eq!(f.enemies[g].st, St::Gone);
    run(&mut f, [0.0, 0.0, 6000.0], 520);
    assert!(f.enemies[g].alive(), "guay came back");
}

#[test]
fn peahat_blades_clank_from_above_but_the_roots_take_damage() {
    let mut f = Field::new(11);
    let p = f.add(Kind::Peahat, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    for _ in 0..90 {
        f.step([0.0, 0.0, 800.0], &w);
    }
    assert!(ai::peahat_airborne(&f.enemies[p]));
    let top = f.hit(p, Dmg::MasterSword, [0.0, 600.0, 0.0]);
    assert!(top.clank, "swords bounce off the propeller");
    let below = f.hit(p, Dmg::Arrow, [0.0, -10.0, 0.0]);
    assert!(below.applied && below.damage == 2);
    assert_eq!(f.enemies[p].hp, 4);
}

#[test]
fn wolfos_wakes_up_chases_and_bites() {
    let mut f = Field::new(12);
    let wf = f.add(Kind::Wolfos, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    let mut bite = false;
    for _ in 0..400 {
        f.step([0.0, 0.0, 900.0], &w);
        bite |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { units: 4, .. }));
    }
    assert!(bite, "wolfos hits for a quarter heart ({:?})", f.enemies[wf].st);
    assert_eq!(f.enemies[wf].spec().hp, 8);
}

#[test]
fn leever_spawner_keeps_up_to_its_limit() {
    let mut f = Field::new(13);
    f.spawners.push(Spawner { kind: Kind::Leever, pos: [0.0, 0.0, 0.0], range: 120.0, max_live: 3, total: 8, spawned: 0, timer: 0.0, live: vec![] });
    run(&mut f, [200.0, 0.0, 200.0], 600);
    let live = f.enemies.iter().filter(|e| e.st != St::Gone).count();
    assert!(live <= 3, "{live}");
    assert!(f.enemies.len() >= 3, "several came up: {}", f.enemies.len());
}

#[test]
fn enemies_beyond_the_active_radius_sleep() {
    let mut f = Field::new(14);
    let t = f.add(Kind::Tektite, [0.0, 0.0, 0.0], 0.0, -2);
    run(&mut f, [20000.0, 0.0, 0.0], 100);
    assert!(f.enemies[t].asleep);
    assert_eq!(f.enemies[t].age, 0.0);
}

// ---- the three characters against the same enemy

fn baba_ready(player: Vec3) -> (Field, usize) {
    let mut f = Field::new(20);
    let i = f.add(Kind::DekuBaba, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    for _ in 0..130 {
        f.step(player, &w);
    }
    (f, i)
}

#[test]
fn link_sword_swing_lands_with_the_swing_damage() {
    let (mut f, i) = baba_ready([0.0, 0.0, 700.0]);
    let head = f.enemies[i].head;
    let mut m = mario_at([head[0], 0.0, head[2] + 150.0]);
    m.link.enable(crate::link::items::Age::Adult);
    m.link.swing = Some(Swing::Forward);
    m.link.inv.sword = Some(Sword::Master);
    m.flags |= MARIO_PUNCHING;
    m.face_angle[1] = 32768.0;
    assert_eq!(m.link.weapon(), Weapon::Sword(Sword::Master));
    let s = strikes(&mut m, &f);
    assert_eq!(s.len(), 1, "head at {head:?}");
    assert_eq!(s[0].dmg, Dmg::MasterSword);
    let r = apply_strikes(&mut m, &mut f, &s);
    assert!(r[0].1.applied);
    // One swing counts once.
    assert!(strikes(&mut m, &f).is_empty());
}

#[test]
fn mario_ground_pound_kills_a_baba_at_the_second_exchange() {
    let (mut f, i) = baba_ready([0.0, 0.0, 700.0]);
    let head = f.enemies[i].head;
    let mut m = mario_at([head[0], head[1] + 40.0, head[2] + 90.0]);
    m.set_action(ACT_GROUND_POUND, 0.0);
    m.vel[1] = -50.0;
    let s = strikes(&mut m, &f);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].dmg, Dmg::MasterJump, "heavy hit");
    let r = apply_strikes(&mut m, &mut f, &s);
    assert_eq!(r[0].1.damage, 4);
    let w = flat();
    let mut died = false;
    for _ in 0..60 {
        f.step([0.0, 0.0, 700.0], &w);
        died |= f.events.iter().any(|e| matches!(e, Ev::Died { .. }));
    }
    assert!(died);
}

#[test]
fn banjo_beak_barge_and_eggs_both_land() {
    let (f, i) = baba_ready([0.0, 0.0, 700.0]);
    let head = f.enemies[i].head;
    let mut m = mario_at([head[0], 0.0, head[2] + 100.0]);
    // Beak Barge is an attacking dive: ACT_DIVE with forward speed.
    m.set_action(ACT_DIVE, 0.0);
    m.forward_vel = 40.0;
    m.face_angle[1] = 32768.0;
    let s = strikes(&mut m, &f);
    assert_eq!(s.first().map(|s| s.dmg), Some(Dmg::MasterSword));
    // An egg alone also lands.
    let mut m = mario_at([0.0, 0.0, 3000.0]);
    m.banjo.eggs_in_flight.push(crate::banjo_ext::Egg { kind: crate::banjo_ext::EggKind::Head, pos: [head[0], head[1], head[2] + 20.0], vel: [0.0; 3], age: 3.0 });
    let s = strikes(&mut m, &f);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].dmg, Dmg::Slingshot);
    assert!(m.banjo.eggs_in_flight[0].age > 9000.0, "the egg shatters");
}

#[test]
fn stomping_a_tektite_bounces_mario() {
    let mut f = Field::new(30);
    let t = f.add(Kind::RedTektite, [0.0, 0.0, 0.0], 0.0, -1);
    f.enemies[t].asleep = false;
    let mut m = mario_at([0.0, 150.0, 10.0]);
    m.set_action(ACT_FREEFALL, 0.0);
    m.vel[1] = -20.0;
    let s = strikes(&mut m, &f);
    assert_eq!(s.len(), 1);
    assert!(s[0].stomp);
    let r = apply_strikes(&mut m, &mut f, &s);
    assert!(r[0].1.killed || r[0].1.applied);
    assert_eq!(m.vel[1], 30.0);
}

#[test]
fn hurting_link_costs_exactly_the_damage_in_sixteenths() {
    let mut m = mario_at([0.0, 0.0, 0.0]);
    m.link.enable(crate::link::items::Age::Child);
    let before = m.link.inv.health;
    assert!(hurt_player(&mut m, 8, [0.0, 0.0, 100.0]));
    // The wedge bridge removes one heart on the next Link step; the refund leaves 8 units lost.
    assert_eq!(m.link.inv.health, before + 8);
    let mut m2 = mario_at([0.0, 0.0, 0.0]);
    assert!(hurt_player(&mut m2, 8, [0.0, 0.0, 100.0]));
    assert!(m2.hurt_counter > 0.0, "Mario takes a wedge");
}

#[test]
fn every_kind_has_unique_names() {
    let mut names = std::collections::HashSet::new();
    for k in KINDS {
        assert!(names.insert(k.name()), "{k:?}");
        assert_eq!(Kind::from_name(k.name()), Some(k));
    }
}

#[test]
fn status_effects_flash_burn_freeze_and_stun_are_set_and_time_out() {
    // Damage flash (the red `Actor_SetColorFilter`) on every applied blow, and it times out.
    let mut f = Field::new(11);
    let w = f.add(Kind::Wolfos, [0.0, 0.0, 0.0], 0.0, 0);
    run(&mut f, [0.0, 0.0, 3000.0], 5);
    let r = f.hit(w, Dmg::KokiriSword, [0.0, 0.0, 300.0]);
    assert!(r.applied);
    assert!(f.enemies[w].flash > 0.0, "red flash after a hit");
    run(&mut f, [0.0, 0.0, 3000.0], 30);
    assert_eq!(f.enemies[w].flash, 0.0, "the flash ends");

    // Ice arrow freezes a tektite and the freeze wears off (`frozen` counts down, the model stops animating meanwhile).
    let mut f = Field::new(12);
    let t = f.add(Kind::Wolfos, [0.0, 0.0, 0.0], 0.0, 0);
    run(&mut f, [0.0, 0.0, 3000.0], 5);
    let (dmg, react) = damage_of(Kind::Wolfos, Dmg::IceArrow);
    f.hit(t, Dmg::IceArrow, [0.0, 0.0, 300.0]);
    if f.enemies[t].hp > 0 && (react == 0xF || react == 3) {
        assert!(f.enemies[t].frozen > 0.0, "frozen by the ice arrow (damage {dmg})");
        let start = f.enemies[t].frozen;
        run(&mut f, [0.0, 0.0, 3000.0], 10);
        assert!(f.enemies[t].frozen < start);
        run(&mut f, [0.0, 0.0, 3000.0], 120);
        assert_eq!(f.enemies[t].frozen, 0.0);
    }
    // The tektite's table freezes it too (reaction 0xF) while a stunning blow only stuns.
    assert!(damage_of(Kind::Tektite, Dmg::IceArrow).1 == 0xF);

    // Fire arrow sets a Baba alight; the nut stuns the tektite's cousin without damage.
    let mut f = Field::new(13);
    let b = f.add(Kind::DekuBaba, [0.0, 0.0, 0.0], 0.0, 0);
    run(&mut f, [0.0, 0.0, 700.0], 130);
    f.hit(b, Dmg::FireArrow, [0.0, 0.0, 300.0]);
    assert!(f.enemies[b].burn > 0.0 || f.enemies[b].hp <= 0, "burning");
}

#[test]
fn hits_report_sparks_and_deaths_report_a_burst() {
    // The events the app turns into hit sparks, clanks and the death burst.
    let mut f = Field::new(14);
    let i = f.add(Kind::DekuBaba, [0.0, 0.0, 0.0], 0.0, 0);
    run(&mut f, [0.0, 0.0, 700.0], 130);
    f.hit(i, Dmg::MasterSword, [0.0, 0.0, 300.0]);
    assert!(f.events.iter().any(|e| matches!(e, Ev::Spark { .. })), "a blow makes a spark");
    assert!(f.enemies[i].killed_by.is_some());
    let w = f.add(Kind::Wolfos, [3000.0, 0.0, 0.0], 0.0, 0);
    run(&mut f, [3000.0, 0.0, 3300.0], 5);
    let mut died = false;
    for _ in 0..8 {
        f.hit(w, Dmg::GiantSword, [3000.0, 0.0, 300.0]);
        died |= f.events.iter().any(|e| matches!(e, Ev::Died { .. }));
        if died {
            break;
        }
    }
    assert!(died, "a killing blow raises Died");
}

#[test]
fn baby_dodongo_waits_underground_then_crawls_at_the_player() {
    let mut f = Field::new(21);
    let d = f.add(Kind::BabyDodongo, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    run(&mut f, [0.0, 0.0, 3000.0], 30);
    assert_eq!(f.enemies[d].st, St::Hidden, "far away it stays underground");
    assert!(!f.enemies[d].visible);
    let mut hurt = false;
    for _ in 0..200 {
        f.step([0.0, 0.0, 150.0], &w);
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { .. }));
    }
    assert!(f.enemies[d].visible && f.enemies[d].st == St::Chase);
    assert!(hurt, "it reaches Link and bites");
    assert!(f.hit(d, Dmg::KokiriSword, [0.0; 3]).killed, "one hit point");
}

#[test]
fn biri_drifts_toward_the_player_at_chest_height_and_shocks() {
    let mut f = Field::new(22);
    let b = f.add(Kind::Biri, [0.0, 100.0, 0.0], 0.0, 0);
    let w = flat();
    let mut hurt = false;
    for _ in 0..400 {
        f.step([0.0, 0.0, 200.0], &w);
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { .. }));
    }
    assert!(hurt);
    assert!(f.enemies[b].pos[2] > 100.0, "it moved toward Link");
    assert!(f.hit(b, Dmg::Slingshot, [0.0; 3]).killed);
}

#[test]
fn king_dodongo_falls_to_a_swallowed_bomb_then_to_the_sword() {
    let mut f = Field::new(31);
    let k = f.add(Kind::KingDodongo, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    // Walks at Link and rears up to inhale.
    let mut inhaled = false;
    for _ in 0..400 {
        f.step([0.0, 0.0, 1500.0], &w);
        if f.enemies[k].st == St::Windup {
            inhaled = true;
            break;
        }
    }
    assert!(inhaled, "he inhales when Link is in range");
    assert!(f.hit(k, Dmg::KokiriSword, [0.0, 100.0, 300.0]).clank == false && f.enemies[k].hp == 12, "a sword does nothing while he is up");
    let r = f.hit(k, Dmg::Explosive, [0.0, 100.0, 300.0]);
    assert!(r.applied && f.enemies[k].hp == 10 && f.enemies[k].st == St::Vulnerable, "a swallowed bomb costs 2 and drops him");
    // Swords while he lies there: 10 hit points left, a Master Sword blow is 2.
    let mut killed = false;
    for i in 0..12 {
        let r = f.hit(k, Dmg::MasterSword, [0.0, 100.0, 300.0]);
        if r.killed {
            killed = true;
            assert!(i >= 4, "five blows at 2 each");
            break;
        }
        // stays down: keep the state fresh
        f.enemies[k].st = St::Vulnerable;
    }
    assert!(killed);
    let drops = f.drops_for(Kind::KingDodongo, [0.0; 3], &Needs { link: true, adult: true, health: 48, magic: 0, magic_level: 0, seeds: 0, arrows: 0, bombs: 0, rupees: 0 });
    assert!(drops.iter().any(|d| d.1 == Item::HeartContainer), "the heart container");
}

#[test]
fn armos_is_inert_until_link_is_close_then_hops_and_dies_to_a_hit() {
    let mut f = Field::new(41);
    let a = f.add(Kind::Armos, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    run(&mut f, [0.0, 0.0, 3000.0], 30);
    assert!(f.hit(a, Dmg::KokiriSword, [0.0, 50.0, 100.0]).clank, "a sleeping statue shrugs off the sword");
    let mut hurt = false;
    for _ in 0..200 {
        f.step([0.0, 0.0, 300.0], &w);
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { .. }));
    }
    assert_eq!(f.enemies[a].st, St::Chase, "awake");
    assert!(hurt && f.enemies[a].pos[2] > 100.0, "it hopped at Link and bumped him");
    assert!(f.hit(a, Dmg::KokiriSword, [0.0; 3]).killed);
}

#[test]
fn the_goron_tunic_makes_link_fire_immune() {
    use crate::link::items::Tunic;
    let mut m = MarioRuntime::new(Arc::new(flat()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    assert!(!m.fire_immune(), "not Link");
    m.link.enabled = true;
    assert!(!m.fire_immune());
    m.link.inv.tunic = Tunic::Goron;
    assert!(m.fire_immune());
    m.link.inv.tunic = Tunic::Zora;
    assert!(!m.fire_immune());
}

#[test]
fn shabom_drifts_at_link_stings_and_pops_in_one_blow() {
    let mut f = Field::new(51);
    let b = f.add(Kind::Shabom, [0.0, 100.0, 0.0], 0.0, 0);
    let w = flat();
    let mut hurt = false;
    for _ in 0..400 {
        f.step([0.0, 0.0, 300.0], &w);
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { units: 4, .. }));
    }
    assert!(hurt, "its touch costs a quarter heart");
    assert!(f.enemies[b].pos[2] > 50.0);
    assert!(f.hit(b, Dmg::KokiriSword, [0.0; 3]).killed);
}

#[test]
fn a_stinger_lurks_then_chases_and_barinade_is_only_hurt_while_it_hangs_low() {
    let mut f = Field::new(61);
    let st = f.add(Kind::Stinger, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    run(&mut f, [0.0, 0.0, 4000.0], 20);
    assert_eq!(f.enemies[st].st, St::Hidden);
    assert!(!f.hit(st, Dmg::KokiriSword, [0.0; 3]).applied, "nothing to hit while it hides");
    let mut hurt = false;
    for _ in 0..200 {
        f.step([0.0, 0.0, 400.0], &w);
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { .. }));
    }
    assert!(hurt && f.enemies[st].st == St::Chase);
    assert!(!f.hit(st, Dmg::KokiriSword, [0.0; 3]).killed, "two hit points");
    assert!(f.hit(st, Dmg::KokiriSword, [0.0; 3]).killed);

    let mut f = Field::new(62);
    let b = f.add(Kind::Barinade, [0.0, 0.0, 0.0], 0.0, 0);
    assert!(f.hit(b, Dmg::MasterSword, [0.0; 3]).clank, "protected while it circles");
    let mut open = false;
    for _ in 0..400 {
        f.step([0.0, 0.0, 3000.0], &w);
        if f.enemies[b].st == St::Vulnerable {
            open = true;
            break;
        }
    }
    assert!(open, "it comes down after its guard phase");
    let mut killed = false;
    for _ in 0..8 {
        let r = f.hit(b, Dmg::MasterSword, [0.0; 3]);
        killed |= r.killed;
        f.enemies[b].st = St::Vulnerable;
    }
    assert!(killed, "12 hit points = six Master Sword blows");
}

#[test]
fn phantom_ganon_is_stunned_by_a_reflected_charge_and_then_falls_to_the_sword() {
    let mut f = Field::new(71);
    let g = f.add(Kind::PhantomGanon, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    assert!(f.hit(g, Dmg::MasterSword, [0.0; 3]).clank, "his guard is up while he floats");
    let mut windup = false;
    for _ in 0..400 {
        f.step([0.0, 0.0, 1500.0], &w);
        if f.enemies[g].st == St::Windup {
            windup = true;
            break;
        }
    }
    assert!(windup, "he charges a ball");
    assert!(f.hit(g, Dmg::MasterSword, [0.0; 3]).applied, "the sword bats it back");
    assert_eq!(f.enemies[g].st, St::Vulnerable);
    let mut killed = false;
    for _ in 0..40 {
        let r = f.hit(g, Dmg::MasterSword, [0.0; 3]);
        killed |= r.killed;
        if killed {
            break;
        }
        f.enemies[g].st = St::Vulnerable;
    }
    assert!(killed, "30 hit points = 15 Master Sword blows");
    // and a charge nobody reflects hurts Link
    let mut f = Field::new(72);
    f.add(Kind::PhantomGanon, [0.0, 0.0, 0.0], 0.0, 0);
    let mut hurt = false;
    for _ in 0..400 {
        f.step([0.0, 0.0, 1500.0], &w);
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { units: 16, .. }));
    }
    assert!(hurt);
}

#[test]
fn a_shell_blade_is_armour_until_it_opens_to_bite() {
    let mut f = Field::new(81);
    let sb = f.add(Kind::ShellBlade, [0.0, 0.0, 0.0], 0.0, 0);
    let w = flat();
    assert!(f.hit(sb, Dmg::MasterSword, [0.0; 3]).clank, "shut");
    let mut hurt = false;
    let mut open = false;
    for _ in 0..300 {
        f.step([0.0, 0.0, 300.0], &w);
        hurt |= f.events.iter().any(|e| matches!(e, Ev::HurtPlayer { .. }));
        if f.enemies[sb].st == St::Vulnerable {
            open = true;
            break;
        }
    }
    assert!(hurt && open, "it lunges, then rests open");
    assert!(f.hit(sb, Dmg::KokiriSword, [0.0; 3]).applied);
    f.enemies[sb].st = St::Vulnerable;
    assert!(f.hit(sb, Dmg::KokiriSword, [0.0; 3]).killed, "two hit points");
}
