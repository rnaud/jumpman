//! Scripted scenarios for the faithful Bowser port (`behaviors/bowser_sm64.rs`, a port of
//! `bowser.inc.c`): the intro walk, the BitDW action list, a swing-and-throw into a mine, the
//! defeat sequence ending in the key drop event and Mario's tail grab. Mario is teleported by the
//! script so only the object manager runs, like `tests/bob_behaviors.rs`.
use jumpman_sim::behaviors::bowser_sm64 as bw;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectEvent, ObjectManager};
use jumpman_sim::state::{ACT_IDLE, ACT_PUNCHING};
use std::sync::Arc;

fn arena() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    let r = 3000.0;
    w.add_triangle([-r, 0.0, -r], [-r, 0.0, r], [r, 0.0, r], &o);
    w.add_triangle([-r, 0.0, -r], [r, 0.0, r], [r, 0.0, -r], &o);
    w
}

fn setup(specs: Vec<ObjectSpec>) -> MarioRuntime {
    seed_random(11);
    let mut m = MarioRuntime::new(Arc::new(arena()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -1600.0);
    m.set_objects(ObjectManager::new(specs, None, None));
    m
}

fn bowser_spec(level: f64, pos: Vec3) -> ObjectSpec {
    let mut s = ObjectSpec::new(ObjectBehaviorKind::SmBowser, pos, 0.0);
    s.param = Some(level);
    s
}

fn mine_spec(pos: Vec3) -> ObjectSpec { ObjectSpec::new(ObjectBehaviorKind::SmBowserBomb, pos, 0.0) }

fn tick(m: &mut MarioRuntime) { object_manager::step(m, None, false); }

#[test]
fn init_matches_bhv_bowser_init() {
    let m = setup(vec![bowser_spec(0.0, [0.0; 3]), bowser_spec(1.0, [0.0; 3]), bowser_spec(2.0, [0.0; 3])]);
    // sBowserHealth = { 1, 1, 3 }, hitbox 400 x 400, starts in BOWSER_ACT_WAIT
    let health: Vec<f64> = m.objects.objects.iter().map(|o| o.var("health")).collect();
    assert_eq!(health, vec![1.0, 1.0, 3.0]);
    let o = &m.objects.objects[0];
    assert_eq!((o.hitbox.radius, o.hitbox.height), (400.0, 400.0));
    assert_eq!(o.action, bw::ACT_WAIT);
    assert_eq!(o.var("bp"), 0.0);
    assert_eq!(m.objects.objects[2].var("rainbow"), 1.0);
}

#[test]
fn intro_walk_then_the_bitdw_action_list() {
    let mut m = setup(vec![bowser_spec(0.0, [0.0, 0.0, 0.0])]);
    m.pos = [1500.0, 0.0, 0.0];
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..900 {
        tick(&mut m);
        let o = &m.objects.objects[0];
        seen.insert(o.action);
        // the animation frame always stays inside the current animation
        let id = o.var("animId") as usize;
        assert!(o.var("animFrame") < bw::ANIM_LOOP_END[id] as f64, "anim {id} frame {}", o.var("animFrame"));
        assert!(o.pos[1] >= -1.0, "fell through the arena: {}", o.pos[1]);
    }
    // WAIT -> INTRO_WALK -> DEFAULT and then the BitDW picks: walk, breath fire, quick jump or dance
    assert!(seen.contains(&bw::ACT_INTRO_WALK), "{seen:?}");
    assert!(seen.contains(&bw::ACT_DEFAULT) && seen.contains(&bw::ACT_WALK_TO_MARIO), "{seen:?}");
    assert!(seen.iter().any(|a| [bw::ACT_BREATH_FIRE, bw::ACT_QUICK_JUMP, bw::ACT_DANCE].contains(a)), "{seen:?}");
    // he turned to face Mario while walking
    let o = &m.objects.objects[0];
    assert!(o.pos[0] > 100.0, "Bowser walked toward Mario: {:?}", o.pos);
}

#[test]
fn breath_flames_come_from_the_orientation_table() {
    let mut m = setup(vec![bowser_spec(0.0, [0.0, 0.0, 0.0])]);
    m.pos = [1500.0, 0.0, 0.0];
    m.objects.objects[0].action = bw::ACT_BREATH_FIRE;
    m.objects.objects[0].move_yaw = 0x4000 as f64;
    let mut flames = 0;
    for _ in 0..110 {
        tick(&mut m);
        flames = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::SmBowserFlame && o.param == 0.0).count();
    }
    // frames 46..84 of the breath, every even frame: 19 flames
    assert!(flames >= 15, "{flames} flames spawned");
    // every flame started at the animated mouth: 221..274 above Bowser's origin for frames 46..84
    assert!(m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::SmBowserFlame && o.param == 0.0).all(|o| o.home[1] >= 221.0 && o.home[1] <= 274.0));
}

#[test]
fn thrown_into_the_mine_defeats_bowser_and_drops_the_key() {
    // Arena 1 (BitDW, health 1): a mine 500 units east, Bowser thrown that way at spin 3000.
    let mut m = setup(vec![bowser_spec(0.0, [0.0, 0.0, 0.0]), mine_spec([500.0, 0.0, 0.0])]);
    m.pos = [-800.0, 0.0, 0.0];
    for _ in 0..3 { tick(&mut m); }
    {
        let b = &mut m.objects.objects[0];
        b.move_yaw = 0x4000 as f64;
        b.face_yaw = 0x4000 as f64;
        b.set_var("spin", 3000.0);
        b.held_state = HELD_THROWN;
    }
    let mut action_seen = vec![];
    let mut events = vec![];
    for f in 0..2000 {
        // Mario waits west of the fallen Bowser looking at him (his yaw is >0x6000 off the angle *to* Mario)
        let bpos = m.objects.objects[0].pos;
        m.pos = [bpos[0] - 600.0, 0.0, bpos[2]];
        m.face_angle[1] = 0x4000 as f64;
        tick(&mut m);
        events.extend(m.objects.drain_events());
        let a = m.objects.objects[0].action;
        if action_seen.last() != Some(&a) { action_seen.push(a); }
        if events.iter().any(|e| matches!(e, ObjectEvent::Bowser { .. })) { break; }
        assert!(f < 1999, "no defeat: {action_seen:?}");
    }
    // THROWN then straight to DEAD (one health), the mine was consumed
    assert!(action_seen.contains(&bw::ACT_THROWN) && action_seen.contains(&bw::ACT_DEAD), "{action_seen:?}");
    assert!(!m.objects.objects[1].active, "mine exploded");
    assert!(events.iter().any(|e| matches!(e, ObjectEvent::Explosion { mine: true, .. })));
    assert_eq!(m.objects.objects[0].var("health"), 0.0);
}

#[test]
fn three_health_bowser_survives_the_first_mine_and_flips_back() {
    // Arena 3 (BitS, health 3): the first mine knocks him back to the arena center (HIT_MINE).
    let mut m = setup(vec![bowser_spec(2.0, [0.0, 0.0, 0.0]), mine_spec([500.0, 0.0, 0.0])]);
    m.pos = [-800.0, 0.0, 0.0];
    for _ in 0..3 { tick(&mut m); }
    {
        let b = &mut m.objects.objects[0];
        b.move_yaw = 0x4000 as f64;
        b.set_var("spin", 3000.0);
        b.held_state = HELD_THROWN;
    }
    let mut seen = vec![];
    for _ in 0..400 {
        m.pos = [-800.0, 0.0, 0.0];
        tick(&mut m);
        let a = m.objects.objects[0].action;
        if seen.last() != Some(&a) { seen.push(a); }
    }
    assert_eq!(m.objects.objects[0].var("health"), 2.0);
    assert!(seen.contains(&bw::ACT_HIT_MINE), "{seen:?}");
    assert!(!seen.contains(&bw::ACT_DEAD));
}

#[test]
fn only_the_tail_can_be_grabbed() {
    let mut m = setup(vec![bowser_spec(0.0, [0.0, 0.0, 0.0])]);
    for _ in 0..3 { tick(&mut m); }
    m.objects.objects[0].move_yaw = 0.0; // facing +Z, tail toward -Z
    // standing in the tail zone with a punch: grabbable
    m.pos = [0.0, 0.0, -300.0];
    m.action = ACT_PUNCHING;
    m.action_arg = 0.0;
    tick(&mut m);
    assert!(m.near_grabbable, "tail zone");
    assert!(m.interact_obj.is_some());
    // standing in front of him, inside the body radius: damage instead
    m.pos = [0.0, 0.0, 60.0];
    m.action = ACT_IDLE;
    m.state.hurt_counter = 0.0;
    tick(&mut m);
    // bhvBowserBodyAnchor: INTERACT_DAMAGE 2 -> hurt counter 4 * 2
    assert_eq!(m.state.hurt_counter, 8.0, "body contact hurts");
}
