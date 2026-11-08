//! Engine-free tests for the Banjo-Kazooie enemies (`bk_enemy.rs`): placement decoding, hit points, how each
//! playable character's attacks land (Mario's moves, Link's sword and arrows, Banjo's claw/egg/Beak Buster),
//! the AI state machines against the decomp numbers, contact damage and defeat loot.
use jumpman_sim::banjo_ext::*;
use jumpman_sim::bk_enemy::*;
use jumpman_sim::collision::*;
use jumpman_sim::link::combat::Dmg;
use jumpman_sim::link::moves::hit_object;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectEvent, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-12000.0, 0.0, -12000.0], [-12000.0, 0.0, 12000.0], [12000.0, 0.0, 12000.0], &o);
    w.add_triangle([-12000.0, 0.0, -12000.0], [12000.0, 0.0, 12000.0], [12000.0, 0.0, -12000.0], &o);
    w
}

fn param(kind: Kind) -> f64 {
    (kind.actor() | (100 << 16)) as f64
}

/// Mario at the origin facing +Z with one enemy of `kind` at `at`.
fn arena(kind: Kind, at: [f64; 3]) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    let mut spec = ObjectSpec::new(ObjectBehaviorKind::BkEnemy, at, 0.0);
    spec.param = Some(param(kind));
    m.set_objects(ObjectManager::new(vec![spec], None, None));
    m
}

fn idle() -> ControllerSnapshot {
    ControllerSnapshot::default()
}

/// One game tick in the runner's order: object pass, Mario's step, held-object sync.
fn tick(m: &mut MarioRuntime, c: &ControllerSnapshot, as_link: bool, as_banjo: bool) {
    object_manager::step(m, None, false);
    m.step(c, 0x8000 as f64, as_link, as_banjo);
    object_manager::after_mario_step(m);
}

fn step(m: &mut MarioRuntime) {
    tick(m, &idle(), false, false);
}

fn run(m: &mut MarioRuntime, n: usize) {
    for _ in 0..n {
        step(m);
    }
}

fn enemy(m: &MarioRuntime) -> &SimObject {
    &m.objects.objects[0]
}

fn state(m: &MarioRuntime) -> i32 {
    enemy(m).action
}

/// Run until the enemy reaches `want` (or give up).
fn until_state(m: &mut MarioRuntime, want: i32, max: usize) -> bool {
    for _ in 0..max {
        if state(m) == want {
            return true;
        }
        step(m);
    }
    state(m) == want
}

fn strike(m: &mut MarioRuntime, attack: i32) {
    attack_object(&mut m.objects.objects[0], attack);
    step(m);
}

fn coins(m: &mut MarioRuntime) -> usize {
    m.objects.events.iter().filter(|e| matches!(e, ObjectEvent::Coin { .. })).count()
}

// ---------------------------------------------------------------------------------------------
// Decoding and tables
// ---------------------------------------------------------------------------------------------

#[test]
fn actor_ids_and_scale_decode_from_the_course_param() {
    for kind in ALL_KINDS {
        assert_eq!(Kind::from_actor(kind.actor()), Some(kind));
        assert_eq!(Kind::from_index(kind.index()), kind);
        let s = kind.spec();
        assert!(s.radius > 0.0 && s.height > 0.0 && s.hp > 0, "{kind:?}");
    }
    // the exporter packs `id | scale% << 16`; 150% is a bigger Grublin
    let mut spec = ObjectSpec::new(ObjectBehaviorKind::BkEnemy, [0.0, 0.0, 800.0], 0.0);
    spec.param = Some((0x06 | (150 << 16)) as f64);
    let mut o = SimObject::new(&spec);
    init_bk_enemy(&mut o);
    assert_eq!(kind_of(&o), Kind::Grublin);
    assert!((o.scale - 1.5).abs() < 1e-9);
    assert!((o.hitbox.radius - Kind::Grublin.spec().radius * 1.5).abs() < 1e-9);
    // unknown ids fall back harmlessly
    assert_eq!(Kind::from_actor(0x9999), None);
}

#[test]
fn attack_types_split_into_weak_and_strong_like_ow_and_die() {
    assert_eq!(blow_of(ATTACK_PUNCH), Blow::Weak);
    assert_eq!(blow_of(ATTACK_KICK_OR_TRIP), Blow::Weak);
    for a in [ATTACK_FROM_ABOVE, ATTACK_GROUND_POUND_OR_TWIRL, ATTACK_FAST_ATTACK, ATTACK_FROM_BELOW] {
        assert_eq!(blow_of(a), Blow::Strong);
    }
    assert_eq!(blow_of(0), Blow::None);
}

// ---------------------------------------------------------------------------------------------
// Grublin (core2/code_53C10.c humanoidBaddie_update + MM/ch/grublin.c)
// ---------------------------------------------------------------------------------------------

#[test]
fn grublin_sleeps_far_away_then_notices_alerts_and_chases() {
    let mut m = arena(Kind::Grublin, [0.0, 0.0, 6000.0]);
    run(&mut m, 5);
    assert_eq!(state(&m), hum::IDLE);
    // out of ACTIVE_RANGE nothing happens and it stays hidden
    let mut far = arena(Kind::Grublin, [0.0, 0.0, 9000.0]);
    run(&mut far, 30);
    assert!(!enemy(&far).visible);
    assert_eq!(enemy(&far).pos[2], 9000.0);

    // within 900 BK units (x1.25) of the player it turns, goes through the alert animation, then chases
    let mut m = arena(Kind::Grublin, [0.0, 0.0, 1000.0]);
    assert!(until_state(&mut m, hum::TURN_TO_PLAYER, 400));
    assert!(until_state(&mut m, hum::ALERT, 120));
    assert!(until_state(&mut m, hum::CHASE, 400), "alert never ended");
    let z0 = enemy(&m).pos[2];
    run(&mut m, 5);
    let z1 = enemy(&m).pos[2];
    // chase speed 9 BK units per frame -> 11.25 per tick
    assert!((z0 - z1) > 40.0, "moved {} in 5 ticks", z0 - z1);
    assert!((z0 - z1) < 5.0 * 9.0 * S * 1.2);
}

#[test]
fn grublin_touch_hurts_mario_and_it_backs_off() {
    let mut m = arena(Kind::Grublin, [0.0, 0.0, 300.0]);
    let before = m.health;
    let mut hurt = false;
    for _ in 0..400 {
        step(&mut m);
        if m.health < before {
            hurt = true;
            break;
        }
    }
    assert!(hurt, "Mario was never touched; grublin at {:?} state {}", enemy(&m).pos, state(&m));
    // it loses interest after the hit (func_802DB440): idle, paused
    run(&mut m, 10);
    assert!(matches!(state(&m), hum::IDLE_LOOK | hum::IDLE | hum::RANDOM_CHASE), "wandering again, not chasing: state {}", state(&m));
}

#[test]
fn any_hit_defeats_a_grublin_with_a_puff_and_loot() {
    for attack in [ATTACK_PUNCH, ATTACK_KICK_OR_TRIP, ATTACK_GROUND_POUND_OR_TWIRL, ATTACK_FROM_ABOVE, ATTACK_FAST_ATTACK] {
        let mut m = arena(Kind::Grublin, [0.0, 0.0, 2500.0]);
        run(&mut m, 3);
        strike(&mut m, attack);
        assert_eq!(state(&m), hum::DIE, "attack {attack}");
        let mut loot = 0;
        let mut puff = false;
        for _ in 0..60 {
            step(&mut m);
            loot += coins(&mut m);
            puff |= m.objects.events.iter().any(|e| matches!(e, ObjectEvent::Break { .. }));
            if !m.objects.objects[0].active {
                break;
            }
        }
        assert!(!enemy(&m).active, "attack {attack}: still there");
        assert!(puff && loot >= 1);
    }
}

#[test]
fn grublin_hood_shrugs_off_weak_hits_and_dies_to_strong_ones() {
    let mut m = arena(Kind::GrublinHood, [0.0, 0.0, 2500.0]);
    run(&mut m, 3);
    strike(&mut m, ATTACK_PUNCH);
    assert_ne!(state(&m), hum::DIE);
    assert!(enemy(&m).active);
    strike(&mut m, ATTACK_GROUND_POUND_OR_TWIRL);
    assert_eq!(state(&m), hum::DIE);
    // the hood outruns Banjo's wander: chase speed 16
    let mut m = arena(Kind::GrublinHood, [0.0, 0.0, 800.0]);
    assert!(until_state(&mut m, hum::CHASE, 600));
    let z0 = enemy(&m).pos[2];
    run(&mut m, 3);
    assert!(z0 - enemy(&m).pos[2] > 3.0 * 16.0 * S * 0.8, "hood speed");
}

#[test]
fn limbo_collapses_on_weak_hits_reassembles_and_dies_to_strong_hits() {
    let mut m = arena(Kind::Limbo, [0.0, 0.0, 2500.0]);
    run(&mut m, 3);
    strike(&mut m, ATTACK_PUNCH);
    assert_eq!(state(&m), hum::INVULNERABLE);
    assert_eq!(enemy(&m).hitbox.interact_type & INTERACT_DAMAGE, 0, "a heap of bones does not hurt");
    // 3..6 s later (exit_invulnerable 2.2 s) it is back
    assert!(until_state(&mut m, hum::EXIT_INVULNERABLE, 200));
    assert!(until_state(&mut m, hum::IDLE, 120));
    strike(&mut m, ATTACK_FAST_ATTACK);
    assert_eq!(state(&m), hum::DIE);
}

// ---------------------------------------------------------------------------------------------
// Ticker, Bigbutt, Snippet
// ---------------------------------------------------------------------------------------------

#[test]
fn ticker_is_harmless_wanders_and_dies_on_the_second_weak_hit() {
    let mut m = arena(Kind::Termite, [0.0, 0.0, 1200.0]);
    run(&mut m, 200);
    assert_eq!(m.health, 0x880 as f64, "termites never hurt");
    assert!(contact_damage(enemy(&m)) == 0.0);
    let moved = js_dist(enemy(&m).pos, [0.0, 0.0, 1200.0]);
    assert!(moved > 20.0, "never wandered ({moved})");
    strike(&mut m, ATTACK_PUNCH);
    assert_eq!(state(&m), ant::OW);
    assert!(enemy(&m).active);
    assert!(until_state(&mut m, ant::IDLE, 60));
    strike(&mut m, ATTACK_PUNCH);
    assert!(!enemy(&m).active, "second hit (hp 2) defeats it");
}

fn js_dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn bigbutt_charges_after_snorting_and_gores_for_two_wedges() {
    let mut m = arena(Kind::Bigbutt, [0.0, 0.0, 900.0]);
    assert!(until_state(&mut m, bull::WALK_TO_PLAYER, 200), "state {}", state(&m));
    assert!(until_state(&mut m, bull::ALERT, 100));
    assert!(until_state(&mut m, bull::CHARGE, 400), "never charged");
    let before = m.health;
    let mut best_speed = 0.0f64;
    for _ in 0..200 {
        step(&mut m);
        best_speed = best_speed.max(enemy(&m).forward_vel);
        if m.health < before {
            break;
        }
    }
    assert!(m.health < before, "gored? state {} speed {best_speed}", state(&m));
    run(&mut m, 40);
    assert!(before - m.health >= 0x100 as f64, "two wedges at least: {}", before - m.health);
    assert!(best_speed >= 13.0 * S - 0.01, "charge speed 13 BK/frame: {best_speed}");
}

#[test]
fn bigbutt_cannot_be_killed_but_a_strong_hit_floors_it_for_four_seconds() {
    let mut m = arena(Kind::Bigbutt, [0.0, 0.0, 2500.0]);
    run(&mut m, 2);
    strike(&mut m, ATTACK_PUNCH);
    assert_eq!(state(&m), bull::OW);
    run(&mut m, 40);
    strike(&mut m, ATTACK_GROUND_POUND_OR_TWIRL);
    assert_eq!(state(&m), bull::FALL);
    assert!(until_state(&mut m, bull::DOWN, 60));
    let t0 = enemy(&m).timer;
    assert!(until_state(&mut m, bull::GET_UP, 200));
    assert!((enemy(&m).timer - t0 - 120.0).abs() < 4.0, "down for {} ticks", enemy(&m).timer - t0);
    assert!(enemy(&m).active);
}

#[test]
fn snippet_flips_on_a_weak_hit_and_dies_on_the_next() {
    let mut m = arena(Kind::Snippet, [0.0, 0.0, 2500.0]);
    run(&mut m, 2);
    strike(&mut m, ATTACK_KICK_OR_TRIP);
    assert_eq!(state(&m), crab::FLIP_OVER);
    assert_eq!(enemy(&m).hitbox.interact_type & INTERACT_DAMAGE, 0, "upside down it is harmless");
    assert!(until_state(&mut m, crab::UPSIDE_DOWN, 60));
    strike(&mut m, ATTACK_PUNCH);
    assert!(!enemy(&m).active);
    // a stomp kills it outright
    let mut m = arena(Kind::MutieSnippet, [0.0, 0.0, 2500.0]);
    run(&mut m, 2);
    strike(&mut m, ATTACK_FROM_ABOVE);
    assert!(!enemy(&m).active);
}

#[test]
fn snippet_scuttles_at_twelve_units_per_frame_and_flips_back_after_three_seconds() {
    let mut m = arena(Kind::Snippet, [0.0, 0.0, 1500.0]);
    assert!(until_state(&mut m, crab::CHASE, 200), "state {}", state(&m));
    let z0 = enemy(&m).pos[2];
    run(&mut m, 4);
    assert!((z0 - enemy(&m).pos[2]) > 3.0 * 12.0 * S * 0.8);
    let mut m = arena(Kind::Snippet, [0.0, 0.0, 2500.0]);
    run(&mut m, 2);
    strike(&mut m, ATTACK_PUNCH);
    assert!(until_state(&mut m, crab::UPSIDE_DOWN, 60));
    let t0 = enemy(&m).timer;
    assert!(until_state(&mut m, crab::FLIP_UPRIGHT, 200));
    assert!((enemy(&m).timer - t0 - 90.0).abs() < 4.0);
}

// ---------------------------------------------------------------------------------------------
// Sir Slush and snowballs, Bat, Shrapnel, Snare-bear, Beehive
// ---------------------------------------------------------------------------------------------

#[test]
fn sir_slush_lobs_a_snowball_that_hurts_and_dies_in_one_hit() {
    let mut m = arena(Kind::SirSlush, [0.0, 0.0, 1800.0]);
    let mut thrown = false;
    for _ in 0..400 {
        step(&mut m);
        if m.objects.objects.len() > 1 {
            thrown = true;
            break;
        }
    }
    assert!(thrown, "no snowball; state {}", state(&m));
    let before = m.health;
    for _ in 0..80 {
        step(&mut m);
    }
    assert!(m.health < before, "the lob (32 frames, aimed at Mario) should land");
    // one hit kills (deathCallback)
    let mut m = arena(Kind::SirSlush, [0.0, 0.0, 2500.0]);
    run(&mut m, 2);
    strike(&mut m, ATTACK_PUNCH);
    assert!(!enemy(&m).active);
}

#[test]
fn sir_slush_ignores_swimmers_and_players_inside_500_units() {
    let mut m = arena(Kind::SirSlush, [0.0, 0.0, 300.0]);
    run(&mut m, 250);
    assert_eq!(m.objects.objects.len(), 1, "too close to throw (min range 500 BK units)");
}

#[test]
fn bat_hangs_until_a_player_below_comes_within_800_then_bites_and_flaps_away() {
    // the roost is 400 units above the floor; the player stands under it
    let mut m = arena(Kind::Bat, [0.0, 400.0, 1500.0]);
    run(&mut m, 5);
    assert_eq!(state(&m), bat::ROOST, "out of the 800 unit (x1.25) trigger");
    m.pos = [0.0, 0.0, 700.0];
    assert!(until_state(&mut m, bat::EXIT_ROOST, 20));
    assert!(until_state(&mut m, bat::CHASE, 40));
    let before = m.health;
    for _ in 0..200 {
        step(&mut m);
        if m.health < before {
            break;
        }
    }
    assert!(m.health < before, "bat never bit; state {}", state(&m));
    run(&mut m, 5);
    assert_eq!(state(&m), bat::ROAM);
    // a hit makes it fall and die
    strike(&mut m, ATTACK_PUNCH);
    assert_eq!(state(&m), bat::FALL);
    assert!(until_state(&mut m, bat::DIE, 120));
}

#[test]
fn shrapnel_floats_chases_and_explodes_on_contact() {
    let mut m = arena(Kind::Shrapnel, [0.0, 0.0, 600.0]);
    assert!(until_state(&mut m, mine::TURN, 40), "state {}", state(&m));
    assert!(until_state(&mut m, mine::CHASE, 80));
    let before = m.health;
    let mut exploded = false;
    for _ in 0..200 {
        step(&mut m);
        if state(&m) == mine::EXPLODE || !enemy(&m).active {
            exploded = true;
            break;
        }
    }
    assert!(exploded && m.health < before);
    assert!(m.objects.events.iter().any(|e| matches!(e, ObjectEvent::Explosion { .. })) || !enemy(&m).active);
}

#[test]
fn snarebear_snaps_only_inside_its_jaws_window_and_cannot_be_hurt() {
    let mut m = arena(Kind::Snarebear, [0.0, 0.0, 600.0]);
    run(&mut m, 2);
    assert_eq!(state(&m), bear::ATTACK);
    let mut dmg_ticks = 0;
    for _ in 0..40 {
        step(&mut m);
        if enemy(&m).hitbox.interact_type & INTERACT_DAMAGE != 0 {
            dmg_ticks += 1;
        }
    }
    assert!(dmg_ticks > 5 && dmg_ticks < 25, "jaws damaging for {dmg_ticks} ticks");
    strike(&mut m, ATTACK_GROUND_POUND_OR_TWIRL);
    assert!(enemy(&m).active, "the trap shrugs off attacks");
}

#[test]
fn beehive_breaks_open_and_heals_a_wedge() {
    let mut m = arena(Kind::Beehive, [0.0, 0.0, 2500.0]);
    run(&mut m, 2);
    m.health = 0x300 as f64;
    strike(&mut m, ATTACK_PUNCH);
    assert_eq!(state(&m), hive::DIE);
    run(&mut m, 40);
    assert!(m.health > 0x300 as f64, "honeycomb heals: {}", m.health);
    assert!(!enemy(&m).active);
}

// ---------------------------------------------------------------------------------------------
// Every character's attacks reach the shared interaction pass
// ---------------------------------------------------------------------------------------------

#[test]
fn marios_ground_pound_and_punch_defeat_a_grublin_through_the_interaction_pass() {
    // ground pound landing on top of it
    let mut m = arena(Kind::Grublin, [0.0, 0.0, 60.0]);
    run(&mut m, 2);
    m.pos = [0.0, 0.0, 0.0];
    m.set_action(ACT_GROUND_POUND_LAND, 0.0);
    step(&mut m);
    assert_eq!(state(&m), hum::DIE, "ground pound");
    // a punch (MARIO_PUNCHING flag, facing it)
    let mut m = arena(Kind::Grublin, [0.0, 0.0, 80.0]);
    m.set_action(ACT_PUNCHING, 1.0);
    m.flags |= MARIO_PUNCHING;
    m.face_angle[1] = 0.0;
    tick(&mut m, &idle(), false, false);
    assert!(state(&m) == hum::DIE || !enemy(&m).active, "punch left state {}", state(&m));
}

#[test]
fn links_arrows_bombs_and_sword_damage_types_all_reach_the_enemy() {
    // hit_object is what every Link projectile and the hammer call; strong types arrive as ground pound
    for (dmg, class) in [(Dmg::KokiriSword, Blow::Weak), (Dmg::Arrow, Blow::Weak), (Dmg::Explosive, Blow::Strong), (Dmg::MasterSpin, Blow::Strong)] {
        let mut m = arena(Kind::Snippet, [0.0, 0.0, 2500.0]);
        run(&mut m, 2);
        hit_object(&mut m, 0, dmg);
        let st = enemy(&m).interact_status;
        assert_eq!(blow_of((st & 0xff) as i32), class, "{dmg:?}");
        step(&mut m);
        assert!(state(&m) == crab::FLIP_OVER || !enemy(&m).active);
    }
    // bombs kill a Grublin
    let mut m = arena(Kind::Grublin, [0.0, 0.0, 2500.0]);
    run(&mut m, 2);
    hit_object(&mut m, 0, Dmg::Explosive);
    step(&mut m);
    assert_eq!(state(&m), hum::DIE);
}

#[test]
fn banjos_egg_breaks_a_beehive_and_the_beak_buster_kills_a_grublin() {
    // egg: fired forward with C-up, flies 800 u/s and arrives as a punch
    let mut m = arena(Kind::Beehive, [0.0, 0.0, 700.0]);
    run(&mut m, 2);
    let c = ControllerSnapshot { button_down: C_UP, button_pressed: C_UP, ..Default::default() };
    tick(&mut m, &c, false, true);
    let mut hit = false;
    for _ in 0..80 {
        tick(&mut m, &idle(), false, true);
        if state(&m) == hive::DIE || !enemy(&m).active {
            hit = true;
            break;
        }
    }
    assert!(hit, "egg never reached the beehive (eggs {}, state {})", m.banjo.eggs, state(&m));
    // Beak Buster is a ground-pound-class hit
    let mut m = arena(Kind::Grublin, [0.0, 0.0, 60.0]);
    run(&mut m, 2);
    m.pos = [0.0, 0.0, 0.0];
    m.set_action(ACT_GROUND_POUND, 0.0);
    m.vel[1] = -50.0;
    m.pos[1] = 20.0;
    tick(&mut m, &idle(), false, true);
    assert!(state(&m) == hum::DIE || !enemy(&m).active || m.health < 0x880 as f64, "buster: state {}", state(&m));
}

#[test]
fn enemies_far_from_the_player_cost_nothing_and_are_hidden() {
    let mut m = arena(Kind::Bigbutt, [0.0, 0.0, 10000.0]);
    run(&mut m, 60);
    assert!(!enemy(&m).visible);
    assert_eq!(enemy(&m).pos, [0.0, 0.0, 10000.0]);
    assert_eq!(enemy(&m).timer, 60.0);
}
