//! Engine-free scenarios for the "last stars" ports (`behaviors/r3`: Ukiki and his cage, the HMC rolling rocks;
//! the Big Chill Bully rules of `big_bully.rs`). Every number asserted here is a constant of the decomp source
//! cited in the module (`ukiki.inc.c`, `ukiki_cage.inc.c`, `boulder.inc.c`, `bully.inc.c`, interaction.c).
use jumpman_sim::behaviors::g2::dyn_collision::{register_mesh, CollisionMesh};
use jumpman_sim::behaviors::r3::{self, boulder, ukiki};
use jumpman_sim::collision::*;
use jumpman_sim::cutscene_ext::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectEvent, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;
use ObjectBehaviorKind as K;

fn flat(y: f64, room: Option<i32>) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions { room, ..SurfaceOptions::default() };
    let r = 7900.0;
    w.add_triangle([-r, y, -r], [-r, y, r], [r, y, r], &o);
    w.add_triangle([-r, y, -r], [r, y, r], [r, y, -r], &o);
    w
}

fn runtime(world: CollisionWorld, specs: Vec<ObjectSpec>, mario: Vec3) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: mario, face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(specs, None, None));
    m
}

fn spec(kind: K, pos: Vec3, param: u32) -> ObjectSpec {
    let mut s = ObjectSpec::new(kind, pos, 0.0);
    s.param = Some(param as f64);
    s
}

/// One game tick in `GameSim::tick` order: objects, Mario, held-object sync. Returns the object events and the cues.
fn tick(m: &mut MarioRuntime) -> (Vec<ObjectEvent>, Vec<CutCue>) {
    object_manager::step(m, None, false);
    let events = m.objects.drain_events();
    m.step(&ControllerSnapshot::default(), 0.0, false, false);
    object_manager::after_mario_step(m);
    (events, m.cut.take_cues())
}

fn with_cap(m: &mut MarioRuntime) {
    m.cut.spawn_with_cap = true;
    m.state.flags |= MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD;
}

fn hold(m: &mut MarioRuntime, id: ObjId) {
    m.used_obj = Some(id);
    m.grab_used_object();
    m.set_action(ACT_HOLD_IDLE, 0.0);
}

/// Tick until `cue` is queued (the dialog box opens), then close the box like the player does.
fn talk_through(m: &mut MarioRuntime, cue: CutCue, response: Option<i32>, max: usize) -> bool {
    for _ in 0..max {
        let (_, cues) = tick(m);
        if cues.contains(&cue) {
            for _ in 0..3 {
                tick(m);
            }
            m.cut.dialog_open = false;
            if let Some(r) = response {
                m.cut.dialog_response = r;
            }
            return true;
        }
    }
    false
}

// ------------------------------------------------------------------------------------------------ interaction.c

#[test]
fn mario_loses_his_cap_to_ukiki_and_gets_it_back_in_hand() {
    let mut m = runtime(flat(0.0, None), vec![], [0.0; 3]);
    assert!(!r3::does_mario_have_normal_cap_on_head(&m), "the headless runtime spawns without cap flags");
    with_cap(&mut m);
    assert!(r3::does_mario_have_normal_cap_on_head(&m));
    // A wing cap on the head is not the normal cap: nothing to steal.
    m.state.flags |= MARIO_WING_CAP;
    assert!(!r3::mario_lose_cap_to_enemy(&mut m, 2));
    m.state.flags &= !MARIO_WING_CAP;
    assert!(r3::mario_lose_cap_to_enemy(&mut m, 2));
    assert_eq!(m.flags & (MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD), 0);
    assert_eq!(m.cut.cap_save_flags, SAVE_FLAG_CAP_ON_UKIKI);
    // init_mario: with SAVE_FLAG_CAP_ON_UKIKI Mario respawns capless.
    m.reset_to_spawn();
    assert_eq!(m.flags & (MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD), 0);
    assert_eq!(m.cut.cap_save_flags, SAVE_FLAG_CAP_ON_UKIKI, "the save flag survives the respawn");
    r3::mario_retrieve_cap(&mut m);
    assert_eq!(m.cut.cap_save_flags, 0);
    assert_eq!(m.flags & (MARIO_NORMAL_CAP | MARIO_CAP_IN_HAND | MARIO_CAP_ON_HEAD), MARIO_NORMAL_CAP | MARIO_CAP_IN_HAND);
    m.reset_to_spawn();
    assert_eq!(m.flags & (MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD), MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD);
}

// ------------------------------------------------------------------------------------------------ ukiki.inc.c

#[test]
fn cap_ukiki_runs_at_a_capped_mario_and_from_a_bare_one() {
    set_fixed_random(Some(0.0));
    // ukiki_act_idle: Mario with his cap 700..1000 away -> UKIKI_ACT_RUN toward him.
    let mut m = runtime(flat(0.0, None), vec![spec(K::Ukiki, [0.0, 0.0, 0.0], (ukiki::UKIKI_BP_CAP as u32) << 16)], [0.0, 0.0, 850.0]);
    with_cap(&mut m);
    tick(&mut m);
    let id = ObjId(0);
    assert_eq!(m.objects[id].home, [0.0; 3]);
    tick(&mut m);
    assert_eq!(m.objects[id].action, ukiki::UKIKI_ACT_RUN);
    let before = m.objects[id].distance_to_mario;
    for _ in 0..10 {
        tick(&mut m);
    }
    assert!(m.objects[id].distance_to_mario < before, "runs toward the cap: {before} -> {}", m.objects[id].distance_to_mario);
    // Without a cap, Mario within 300 makes him flee (goal yaw angleToMario + 0x8000).
    let mut m = runtime(flat(0.0, None), vec![spec(K::Ukiki, [0.0, 0.0, 0.0], (ukiki::UKIKI_BP_CAP as u32) << 16)], [0.0, 0.0, 250.0]);
    for _ in 0..3 {
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, ukiki::UKIKI_ACT_RUN);
    let mut farthest: f64 = 0.0;
    let mut run_speed: f64 = 0.0;
    for _ in 0..80 {
        tick(&mut m);
        farthest = farthest.max(m.objects[id].distance_to_mario);
        if m.objects[id].action == ukiki::UKIKI_ACT_RUN {
            run_speed = run_speed.max(m.objects[id].forward_vel);
        }
    }
    // He flees until oUkikiChaseFleeRange (350 + 100 * random_float() = 400) away, then turns back.
    assert!(farthest > 380.0, "flees away from Mario: {farthest}");
    // cur_obj_set_vel_from_mario_vel(20, 0.9): at least 18 units per frame.
    assert!(run_speed >= 18.0, "run speed {run_speed}");
    set_fixed_random(None);
}

#[test]
fn grabbing_the_cap_ukiki_steals_the_cap_and_grabbing_him_again_returns_it() {
    set_fixed_random(Some(0.5));
    let mut m = runtime(flat(0.0, None), vec![spec(K::Ukiki, [0.0, 0.0, 100.0], (ukiki::UKIKI_BP_CAP as u32) << 16)], [0.0; 3]);
    with_cap(&mut m);
    tick(&mut m);
    let id = ObjId(0);
    hold(&mut m, id);
    // cap_ukiki_held_loop: mario_lose_cap_to_enemy(2), then DIALOG_100.
    assert!(talk_through(&mut m, CutCue::Dialog(ukiki::DIALOG_100), None, 60), "DIALOG_100 opens");
    assert_eq!(m.flags & (MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD), 0, "the cap is gone");
    assert_eq!(m.cut.cap_save_flags, SAVE_FLAG_CAP_ON_UKIKI);
    assert_eq!(m.objects[id].var("hasCap"), ukiki::UKIKI_CAP_ON);
    let mut dropped = false;
    for _ in 0..60 {
        tick(&mut m);
        dropped |= m.held_obj.is_none();
    }
    assert!(dropped, "INT_SUBTYPE_DROP_IMMEDIATELY: Mario puts him down");
    // ukiki_act_idle: UKIKI_TEXT_STOLE_CAP -> jump away, UKIKI_TEXT_HAS_CAP.
    assert_eq!(m.objects[id].var("textState"), ukiki::UKIKI_TEXT_HAS_CAP);
    assert_eq!(m.objects[id].var("animState"), ukiki::UKIKI_ANIM_STATE_CAP_ON, "drawn with the cap (oAnimState 2)");
    // Grab him again: DIALOG_101, mario_retrieve_cap, ACT_PUTTING_ON_CAP.
    m.state.pos = m.objects[id].pos;
    m.set_action(ACT_IDLE, 0.0);
    hold(&mut m, id);
    assert!(talk_through(&mut m, CutCue::Dialog(ukiki::DIALOG_101), None, 60), "DIALOG_101 opens");
    let mut put_on = false;
    for _ in 0..120 {
        tick(&mut m);
        put_on |= m.action == ACT_PUTTING_ON_CAP;
    }
    assert!(put_on, "act_reading_npc_dialog puts the handed-back cap on");
    assert_eq!(m.flags & (MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD | MARIO_CAP_IN_HAND), MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD);
    assert_eq!(m.cut.cap_save_flags, 0, "save_file_clear_flags(SAVE_FLAG_CAP_ON_UKIKI)");
    assert_eq!(m.objects[id].var("hasCap"), 0.0);
    assert_eq!(m.held_obj, None);
    set_fixed_random(None);
}

#[test]
fn cage_ukiki_asks_to_be_freed_and_heads_for_his_cage_on_yes() {
    set_fixed_random(Some(0.5));
    let home = [729.0, 0.0, 335.0];
    let mut m = runtime(flat(0.0, None), vec![spec(K::Ukiki, home, (ukiki::UKIKI_BP_CAGE as u32) << 16)], [729.0, 0.0, 235.0]);
    tick(&mut m);
    let id = ObjId(0);
    hold(&mut m, id);
    // cage_ukiki_held_loop: create_dialog_box_with_response(DIALOG_079); "No" pesters again after 60 frames.
    assert!(talk_through(&mut m, CutCue::DialogWithResponse(ukiki::DIALOG_079), Some(DIALOG_RESPONSE_NO), 60));
    tick(&mut m);
    assert_eq!(m.objects[id].var("textState"), ukiki::UKIKI_TEXT_DO_NOT_LET_GO);
    assert_eq!(m.held_obj, Some(id), "Mario keeps holding him after No");
    assert!(talk_through(&mut m, CutCue::DialogWithResponse(ukiki::DIALOG_079), Some(DIALOG_RESPONSE_YES), 120), "asks again");
    let mut dropped = false;
    for _ in 0..60 {
        tick(&mut m);
        dropped |= m.held_obj.is_none();
    }
    assert!(dropped, "Yes: INT_SUBTYPE_DROP_IMMEDIATELY");
    assert_eq!(m.objects[id].action, ukiki::UKIKI_ACT_GO_TO_CAGE);
    assert!(m.objects[id].hitbox.interact_type == 0, "intangible on the way to the cage");
    // sCageUkikiPath: he runs the eleven waypoints, then waits for Mario at the last one.
    let mut waited = false;
    for _ in 0..900 {
        tick(&mut m);
        if m.objects[id].action_state as i32 == ukiki::UKIKI_SUB_ACT_CAGE_WAIT_FOR_MARIO {
            waited = true;
            break;
        }
    }
    assert!(waited, "reached the end of the path (sub {})", m.objects[id].action_state);
    let p = m.objects[id].pos;
    let end = ukiki::CAGE_PATH[10];
    assert!((p[0] - end[0]).hypot(p[2] - end[2]) < 60.0, "waits at the last waypoint: {p:?}");
    // Mario walks up: DIALOG_080 (cur_obj_update_dialog_with_cutscene), then he turns to the cage.
    m.state.pos = [p[0], 0.0, p[2] - 150.0];
    m.set_action(ACT_IDLE, 0.0);
    assert!(talk_through(&mut m, CutCue::Dialog(ukiki::DIALOG_080), None, 120), "DIALOG_080 opens");
    let mut turned = false;
    for _ in 0..200 {
        tick(&mut m);
        turned |= m.objects[id].action_state as i32 >= ukiki::UKIKI_SUB_ACT_CAGE_TURN_TO_CAGE;
    }
    assert!(turned);
    set_fixed_random(None);
}

/// A 300 x 300 box standing on the cage's origin: enough to land on (the real model is `ttm_seg7_collision_ukiki_cage`).
fn register_test_cage() {
    let v = vec![[-150.0, 300.0, -150.0], [-150.0, 300.0, 150.0], [150.0, 300.0, 150.0], [150.0, 300.0, -150.0]];
    register_mesh(ukiki::CAGE_COLLISION, CollisionMesh { vertices: v, triangles: vec![[0, 1, 2, 0, 0], [0, 2, 3, 0, 0]] });
}

#[test]
fn ukiki_lands_on_the_cage_spins_it_and_the_broken_cage_pays_the_act_2_star() {
    set_fixed_random(Some(0.5));
    register_test_cage();
    let cage_param = 1u32 << 24; // BPARAM1(STAR_INDEX_ACT_2)
    let mut m = runtime(
        flat(0.0, None),
        vec![spec(K::UkikiCage, [0.0, 1000.0, 0.0], cage_param), spec(K::Ukiki, [0.0, 1500.0, 0.0], (ukiki::UKIKI_BP_CAGE as u32) << 16)],
        [3000.0, 0.0, 3000.0],
    );
    tick(&mut m);
    // The cage spawned its star child, which sits in the cage.
    let star = m.objects.objects.iter().position(|o| o.behavior == K::UkikiCageStar).map(|i| ObjId(i as u32)).expect("cage star");
    assert_eq!(m.objects[star].pos, [0.0, 1000.0, 0.0]);
    assert!(m.world.find_floor(0.0, 1400.0, 0.0).height > 1290.0, "the cage collision is loaded");
    // Put Ukiki in UKIKI_SUB_ACT_CAGE_LAND_ON_CAGE above the cage.
    let u = ObjId(1);
    {
        let o = &mut m.objects[u];
        o.action = ukiki::UKIKI_ACT_GO_TO_CAGE;
        o.prev_action = ukiki::UKIKI_ACT_GO_TO_CAGE;
        o.action_state = ukiki::UKIKI_SUB_ACT_CAGE_LAND_ON_CAGE as f64;
        o.pos = [0.0, 1500.0, 0.0];
        o.forward_vel = 0.0;
        o.vel = [0.0; 3];
    }
    let mut jingle = false;
    let mut star_event = None;
    let mut spun = false;
    for _ in 0..600 {
        let (events, _) = tick(&mut m);
        for e in events {
            match e {
                ObjectEvent::Sound { name: "PUZZLE_JINGLE", .. } => jingle = true,
                ObjectEvent::DefaultStar { pos, act } => star_event = Some((pos, act)),
                _ => {}
            }
        }
        spun |= m.objects[ObjId(0)].action == ukiki::UKIKI_CAGE_ACT_SPIN;
        if star_event.is_some() {
            break;
        }
    }
    assert!(jingle, "play_puzzle_jingle when he lands on the cage");
    assert!(spun, "UKIKI_CAGE_ACT_SPIN while he spins for 32 frames");
    assert!(!m.objects[ObjId(0)].visible, "UKIKI_CAGE_ACT_HIDE once it landed");
    assert_eq!(star_event, Some((ukiki::CAGE_STAR_POS, 2)), "spawn_default_star(2500, -1200, 1300) with STAR_INDEX_ACT_2");
    assert!(!m.objects[star].active, "the cage star object is deleted");
    assert!(m.world.find_floor(0.0, 1400.0, 0.0).height < 10.0, "the falling cage has no collision any more");
    set_fixed_random(None);
}

// ------------------------------------------------------------------------------------------------ boulder.inc.c

#[test]
fn the_generator_rolls_boulders_only_while_mario_is_in_room_4_and_away() {
    set_fixed_random(Some(0.5));
    let gen = [0.0, 0.0, 0.0];
    // Mario 3000 away in room 4: a boulder every 64 frames.
    let mut m = runtime(flat(0.0, Some(boulder::BOULDER_ROOM)), vec![spec(K::BigBoulderGenerator, gen, 0)], [3000.0, 0.0, 0.0]);
    for _ in 0..200 {
        tick(&mut m);
        m.state.pos = [3000.0, 0.0, 0.0];
        m.state.health = 0x880 as f64;
    }
    let boulders = m.objects.objects.iter().filter(|o| o.behavior == K::BigBoulder).count();
    assert_eq!(boulders, 4, "frames 0, 64, 128, 192");
    // The newest boulder (spawned at frame 192) still rolls at about 40; object_step's friction (0.999) slows it.
    let b = m.objects.objects.iter().rev().find(|o| o.behavior == K::BigBoulder).unwrap();
    assert!(b.forward_vel > 39.0 && b.forward_vel <= 70.0, "40 units per frame at start, capped at 70: {}", b.forward_vel);
    assert_eq!(b.scale, 1.5);
    assert_eq!(b.graph_y_offset, 270.0);
    assert_eq!(b.hitbox.radius, 315.0, "sRollingSphereHitbox scaled by 1.5");
    assert_eq!(b.hitbox.height, 525.0);
    assert!(b.var("facePitch") != 0.0, "adjust_rolling_face_pitch");
    assert!(b.move_yaw >= 0.0 && b.move_yaw < 4096.0);
    // Other room: nothing.
    let mut m = runtime(flat(0.0, Some(5)), vec![spec(K::BigBoulderGenerator, gen, 0)], [3000.0, 0.0, 0.0]);
    for _ in 0..200 {
        tick(&mut m);
    }
    assert_eq!(m.objects.objects.iter().filter(|o| o.behavior == K::BigBoulder).count(), 0);
    // Within 1500: nothing.
    let mut m = runtime(flat(0.0, Some(boulder::BOULDER_ROOM)), vec![spec(K::BigBoulderGenerator, gen, 0)], [1000.0, 0.0, 0.0]);
    for _ in 0..200 {
        tick(&mut m);
        m.state.pos = [1000.0, 0.0, 0.0];
    }
    assert_eq!(m.objects.objects.iter().filter(|o| o.behavior == K::BigBoulder).count(), 0);
    set_fixed_random(None);
}

#[test]
fn a_boulder_hurts_mario_for_three_and_disappears_below_minus_1000() {
    let mut m = runtime(flat(0.0, None), vec![spec(K::BigBoulder, [0.0, 0.0, 0.0], 0)], [0.0, 0.0, 400.0]);
    let start = m.health;
    let mut hurt = false;
    for _ in 0..30 {
        tick(&mut m);
        hurt |= m.health < start || m.hurt_counter > 0.0;
    }
    assert!(hurt, "INTERACT_DAMAGE, 3 damage");
    m.objects[ObjId(0)].pos = [0.0, -1100.0, 0.0];
    m.objects[ObjId(0)].vel[1] = 0.0;
    tick(&mut m);
    assert!(!m.objects[ObjId(0)].active, "ACTIVE_FLAG_DEACTIVATED below y -1000");
}

// ------------------------------------------------------------------------------------------------ bully.inc.c

#[test]
fn the_big_chill_bully_knocked_into_the_water_pays_its_own_star() {
    let mut m = runtime(flat(500.0, None), vec![spec(K::BigChillBully, [315.0, 1331.0, -4852.0], 1 << 24)], [3000.0, 500.0, 3000.0]);
    tick(&mut m);
    // bully_step: below y 1030 -> BULLY_ACT_LAVA_DEATH -> spawn_default_star(130, 1600, -4335).
    let mut star = None;
    for _ in 0..200 {
        let (events, _) = tick(&mut m);
        for e in events {
            if let ObjectEvent::DefaultStar { pos, act } = e {
                star = Some((pos, act));
            }
            assert!(!matches!(e, ObjectEvent::BigBully { .. }), "no mission-complete event for the chill bully");
        }
        if star.is_some() {
            break;
        }
    }
    assert_eq!(star, Some(([130.0, 1600.0, -4335.0], 2)));
    assert!(!m.objects[ObjId(0)].active);
}
