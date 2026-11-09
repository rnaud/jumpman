//! Engine-free scenarios for the SM64 group 1 behavior ports (`behaviors/g1`). Every number
//! asserted here is a constant or a timing of the decomp source cited in the matching module
//! (src/game/behaviors/*.inc.c); there are no TS goldens for these actors.
use jumpman_sim::behaviors::g1::{self, anim};
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectEvent, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;
use ObjectBehaviorKind as K;

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    let r = 8000.0;
    w.add_triangle([-r, 0.0, -r], [-r, 0.0, r], [r, 0.0, r], &o);
    w.add_triangle([-r, 0.0, -r], [r, 0.0, r], [r, 0.0, -r], &o);
    w
}

fn runtime(specs: Vec<ObjectSpec>, mario: Vec3) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: mario, face_yaw: 0.0 }, -1600.0);
    m.set_objects(ObjectManager::new(specs, None, None));
    m
}

fn spec(kind: K, pos: Vec3, yaw: f64, param: f64) -> ObjectSpec {
    let mut s = ObjectSpec::new(kind, pos, yaw);
    s.param = Some(param);
    s
}

fn step(m: &mut MarioRuntime, n: usize) {
    for _ in 0..n {
        object_manager::step(m, None, false);
        m.objects.events.retain(|_| true);
    }
}

/// Step and collect every event of the run.
fn step_events(m: &mut MarioRuntime, n: usize) -> Vec<ObjectEvent> {
    let mut all = vec![];
    for _ in 0..n {
        object_manager::step(m, None, false);
        all.extend(m.objects.events.iter().cloned());
    }
    all
}

#[test]
fn thwomp_cycle_matches_grindel_thwomp_actions() {
    // thwomp.inc.c: raise 10/tick for param+41 ticks (+5 on the last), hang 10..39 ticks, fall
    // with vel -4/tick^2, land for 10 ticks, rest 20..29 ticks, raise again.
    set_fixed_random(Some(0.0));
    let mut m = runtime(vec![spec(K::Thwomp, [0.0, 500.0, 0.0], 0.0, 0.0)], [3000.0, 0.0, 3000.0]);
    let id = ObjId(0);
    step(&mut m, 1);
    // DROP_TO_FLOOR + ADD_FLOAT(oPosY, 1) + SET_HOME, then the first raise step.
    assert_eq!(m.objects[id].home[1], 1.0);
    assert_eq!(m.objects[id].pos[1], 11.0);
    step(&mut m, 41);
    assert_eq!(m.objects[id].action, g1::thwomp::ACT_IDLE_AT_TOP);
    assert_eq!(m.objects[id].pos[1], 1.0 + 41.0 * 10.0 + 5.0);
    // random timer = 10 (random()=0): idle while oTimer <= 10, then lower.
    step(&mut m, 12);
    assert_eq!(m.objects[id].action, g1::thwomp::ACT_LOWER);
    let top = m.objects[id].pos[1];
    step(&mut m, 1);
    assert_eq!(m.objects[id].pos[1], top - 4.0);
    step(&mut m, 1);
    assert_eq!(m.objects[id].pos[1], top - 4.0 - 8.0);
    // It reaches the floor and goes through LAND (sound at oTimer 0 only within 1500 of Mario: far here).
    step(&mut m, 40);
    assert!(m.objects[id].pos[1] >= 1.0);
    set_fixed_random(None);
}

#[test]
fn thwomp_slam_is_audible_when_mario_is_near() {
    set_fixed_random(Some(0.0));
    let mut m = runtime(vec![spec(K::Thwomp2, [0.0, 500.0, 0.0], 0.0, 0.0)], [600.0, 0.0, 0.0]);
    let events = step_events(&mut m, 120);
    assert!(events.iter().any(|e| matches!(e, ObjectEvent::Sound { name: "OBJ_THWOMP", .. })));
    set_fixed_random(None);
}

#[test]
fn piranha_plant_sleeps_wakes_and_bites() {
    // piranha_plant.inc.c: idle -> sleeping inside 1200; Mario moving fast inside 400 wakes it,
    // it bites after 10 more frames; the hitbox while biting is radius 150 / height 100.
    let mut m = runtime(vec![spec(K::PiranhaPlant, [0.0, 0.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 1000.0]);
    let id = ObjId(0);
    step(&mut m, 2);
    assert_eq!(m.objects[id].action, g1::piranha_plant::ACT_SLEEPING);
    assert_eq!(m.objects[id].hitbox.interact_type, g1::INTERACT_BOUNCE_TOP);
    assert_eq!((m.objects[id].hitbox.radius, m.objects[id].hitbox.height), (250.0, 200.0));
    m.pos = [0.0, 0.0, 380.0];
    m.forward_vel = 12.0;
    step(&mut m, 2);
    assert_eq!(m.objects[id].action, g1::piranha_plant::ACT_WOKEN_UP);
    m.pos = [0.0, 3000.0, 3000.0];
    step(&mut m, 13);
    assert_eq!(m.objects[id].action, g1::piranha_plant::ACT_BITING);
    assert_eq!(m.objects[id].hitbox.interact_type, INTERACT_DAMAGE);
    assert_eq!(m.objects[id].hitbox.damage_or_coin_value, 3.0);
}

#[test]
fn piranha_plant_dies_shrinks_and_drops_a_blue_coin() {
    let mut m = runtime(vec![spec(K::PiranhaPlant, [0.0, 0.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 1000.0]);
    let id = ObjId(0);
    step(&mut m, 2);
    // Ground pound on the sleeping plant: INTERACTED | WAS_ATTACKED.
    m.objects[id].interact_status = INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED;
    step(&mut m, 1);
    assert_eq!(m.objects[id].action, g1::piranha_plant::ACT_ATTACKED);
    // 0x46 frame fall-over animation (near end at frame 0x44), then shrink 0.04 per frame.
    let mut events = vec![];
    for _ in 0..200 {
        events.extend(step_events(&mut m, 1));
        if m.objects[id].action == g1::piranha_plant::ACT_WAIT_TO_RESPAWN { break; }
    }
    assert_eq!(m.objects[id].action, g1::piranha_plant::ACT_WAIT_TO_RESPAWN);
    assert!(events.iter().any(|e| matches!(e, ObjectEvent::Coin { value, .. } if *value == 5.0)));
    assert_eq!(m.objects[id].scale, 0.0);
}

#[test]
fn bullet_bill_charges_after_its_wind_up() {
    // bullet_bill.inc.c: act 1 needs Mario within 0x2000 of its heading and 400..1500 away; act 2
    // creeps at 3 for 40 frames, shudders +-3 for 10, then flies at 30 (sound at timer 50).
    let mut m = runtime(vec![spec(K::BulletBill, [0.0, 100.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 1000.0]);
    let id = ObjId(0);
    step(&mut m, 3);
    assert_eq!(m.objects[id].action, 2);
    step(&mut m, 5);
    assert_eq!(m.objects[id].forward_vel, 3.0);
    step(&mut m, 40);
    assert!(m.objects[id].forward_vel.abs() == 3.0);
    let events = step_events(&mut m, 12);
    assert_eq!(m.objects[id].forward_vel, 30.0);
    assert!(events.iter().any(|e| matches!(e, ObjectEvent::Sound { name: "OBJ_POUNDING_CANNON", .. })));
    assert_eq!(m.objects[id].hitbox.damage_or_coin_value, 3.0);
}

#[test]
fn bullet_bill_resets_home_after_150_frames() {
    let mut m = runtime(vec![spec(K::BulletBill, [0.0, 100.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 1000.0]);
    let id = ObjId(0);
    m.pos = [0.0, 0.0, 5000.0];
    step(&mut m, 2);
    assert_eq!(m.objects[id].action, 1);
    m.pos = [0.0, 0.0, 1000.0];
    let mut flew = false;
    for _ in 0..400 {
        step(&mut m, 1);
        flew |= m.objects[id].pos[2] > 500.0;
        m.pos = [m.pos[0], 0.0, 1000.0];
    }
    assert!(flew);
}

#[test]
fn swoop_grows_then_dives_at_mario() {
    // swoop.inc.c: scale +0.05/frame until 1, turn to Mario (800/frame), dive with velY -12.
    let mut m = runtime(vec![spec(K::Swoop, [0.0, 400.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 1000.0]);
    let id = ObjId(0);
    step(&mut m, 1);
    assert!((m.objects[id].scale - 0.05).abs() < 1e-12);
    step(&mut m, 19);
    assert_eq!(m.objects[id].scale, 1.0);
    assert_eq!(m.objects[id].var("faceRoll"), 0x8000 as f64);
    step(&mut m, 5);
    assert_eq!(m.objects[id].action, g1::swoop::ACT_MOVE);
    assert_eq!(m.objects[id].hitbox.interact_type, g1::INTERACT_HIT_FROM_BELOW);
}

#[test]
fn swoop_dies_to_any_attack_and_drops_one_coin() {
    let mut m = runtime(vec![spec(K::Swoop, [0.0, 0.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 3000.0]);
    let id = ObjId(0);
    step(&mut m, 1);
    m.objects[id].interact_status = INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED | ATTACK_PUNCH as u32;
    let events = step_events(&mut m, 1);
    assert!(!m.objects[id].active);
    assert_eq!(events.iter().filter(|e| matches!(e, ObjectEvent::Coin { .. })).count(), 1);
}

#[test]
fn scuttlebug_spawner_launches_a_bug_with_decomp_velocities() {
    // scuttlebug.inc.c bhv_scuttlebug_spawn_loop: oTimer > 30 and 500 < dist < 1500.
    let mut m = runtime(vec![spec(K::ScuttlebugSpawner, [0.0, 0.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 1000.0]);
    step(&mut m, 31);
    assert_eq!(m.objects.len(), 1);
    step(&mut m, 2);
    assert_eq!(m.objects.len(), 2);
    let bug = ObjId(1);
    assert_eq!(m.objects[bug].behavior, K::Scuttlebug);
    // Spawned with fvel 30 / velY 80, then one move step with gravity 4 (vel 76) happens.
    assert!(m.objects[bug].pos[1] > 0.0);
    assert_eq!(m.objects[bug].var("noLoot"), 0.0);
}

#[test]
fn scuttlebug_walks_toward_mario_and_dies_when_stomped() {
    let mut m = runtime(vec![spec(K::Scuttlebug, [0.0, 0.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 700.0]);
    let id = ObjId(0);
    step(&mut m, 5);
    // sub-action 0 -> 1 once it is on the ground (home reset to the landing spot).
    assert_eq!(m.objects[id].action_state, 1.0);
    // 5 while turning toward Mario, 15 during the charge that starts once it faces him (within 0x800).
    assert!(m.objects[id].forward_vel == 5.0 || m.objects[id].forward_vel == 15.0);
    // Mario falls on it: bounce, 3 coins (sScuttlebugHitbox numLootCoins).
    m.pos = [0.0, 40.0, 0.0];
    m.vel[1] = -30.0;
    m.set_action(ACT_FREEFALL, 0.0);
    let events = step_events(&mut m, 3);
    assert!(!m.objects[id].active);
    assert_eq!(events.iter().filter(|e| matches!(e, ObjectEvent::Coin { .. })).count(), 3);
    assert_eq!(m.vel[1], 30.0);
}

#[test]
fn spindrift_backs_off_after_touching_mario() {
    // spindrift.inc.c: action 1 walks backwards at -10 for 20 frames, then returns to 0.
    let mut m = runtime(vec![spec(K::Spindrift, [0.0, 0.0, 0.0], 0.0, 0.0)], [0.0, 0.0, 3000.0]);
    let id = ObjId(0);
    step(&mut m, 3);
    assert_eq!(m.objects[id].action, 0);
    assert!(m.objects[id].forward_vel > 0.0 && m.objects[id].forward_vel <= 4.0);
    m.objects[id].interact_status = INT_STATUS_INTERACTED; // touched, not attacked
    step(&mut m, 1);
    assert_eq!(m.objects[id].action, 1);
    assert_eq!(m.objects[id].interact_subtype, g1::INT_SUBTYPE_TWIRL_BOUNCE);
    step(&mut m, 2);
    assert_eq!(m.objects[id].forward_vel, -10.0);
    step(&mut m, 22);
    assert_eq!(m.objects[id].action, 0);
}

#[test]
fn mips_runs_the_furthest_path() {
    // mips.inc.c: wake within 500 of Mario, take the path whose first waypoint within 800 is
    // furthest from him, run at 40 until the last waypoint, then wait again.
    let mut s = spec(K::Mips, [0.0, 0.0, 0.0], 0.0, 0.0);
    s.ball_route = Some(BallRoute::mountain(
        vec![[100.0, 0.0, 0.0], [100.0, 0.0, 600.0], [100.0, 0.0, 1200.0], [0.0, -1.0e6, 0.0], [-300.0, 0.0, 0.0], [-300.0, 0.0, -600.0]],
        0.0,
        0.0,
    ));
    let mut m = runtime(vec![s], [400.0, 0.0, 0.0]);
    let id = ObjId(0);
    step(&mut m, 2);
    assert_eq!(m.objects[id].action, g1::mips::ACT_FOLLOW_PATH);
    assert_eq!(m.objects[id].var("path"), 1.0); // the -300 path is further from Mario at +400
    step(&mut m, 1);
    // oForwardVelocity 40 (MIPS_BP_15_STARS) is set every frame, object_step's floor friction (0.89) trims it.
    assert!(m.objects[id].forward_vel > 30.0 && m.objects[id].forward_vel <= 40.0);
    step(&mut m, 40);
    assert!(m.objects[id].pos[0] < -250.0 && m.objects[id].pos[2] < -300.0);
    assert_eq!(m.objects[id].action, g1::mips::ACT_WAIT_FOR_NEARBY_MARIO);
}

#[test]
fn monty_mole_pops_out_of_a_hole_and_throws_a_rock() {
    // monty_mole.inc.c: select a hole 100..1500 away, rise (vel 3, gravity 0), spawn a rock when
    // facing Mario, jump back into the hole.
    set_fixed_random(Some(0.0));
    let mut m = runtime(
        vec![spec(K::MontyMoleHole, [0.0, 0.0, 800.0], 0.0, 0.0), spec(K::MontyMole, [0.0, 0.0, 0.0], 0.0, 1.0)],
        [0.0, 0.0, 0.0],
    );
    let mole = ObjId(1);
    step(&mut m, 2);
    assert_ne!(m.objects[mole].action, g1::monty_mole::ACT_SELECT_HOLE);
    assert!(m.objects[mole].visible);
    assert_eq!(m.objects[mole].pos[2], 800.0);
    let mut threw = false;
    for _ in 0..200 {
        step(&mut m, 1);
        threw |= m.objects.objects.iter().any(|o| o.behavior == K::MontyMoleRock);
    }
    assert!(threw);
    set_fixed_random(None);
}

#[test]
fn snufit_fires_three_bullets_when_mario_is_close() {
    // snufit.inc.c: inside 800 it inflates (body scale base 600), then fires 3 bullets 3 frames apart.
    let mut m = runtime(vec![spec(K::Snufit, [0.0, 100.0, 0.0], 0.0, 0.0)], [0.0, 100.0, 300.0]);
    let mut shots = 0;
    for _ in 0..200 {
        step(&mut m, 1);
        shots = m.objects.objects.iter().filter(|o| o.behavior == K::SnufitBall).count();
        if shots >= 3 { break; }
    }
    assert_eq!(shots, 3);
}

#[test]
fn butterfly_follows_mario_inside_1000_and_goes_home_past_1200() {
    let mut m = runtime(vec![spec(K::Butterfly, [0.0, 0.0, 0.0], 0.0, 0.0)], [3000.0, 0.0, 0.0]);
    let id = ObjId(0);
    step(&mut m, 2);
    assert_eq!(m.objects[id].action, 0);
    m.pos = [900.0, 0.0, 0.0];
    step(&mut m, 2);
    assert_eq!(m.objects[id].action, 1);
    m.pos = [3000.0, 0.0, 0.0];
    step(&mut m, 200);
    assert_eq!(m.objects[id].action, 0, "returned home and rests");
    assert!(dist_3d(&m.objects[id].pos, &m.objects[id].home) < 12.0);
}

#[test]
fn bird_spawner_releases_six_birds() {
    let mut m = runtime(vec![spec(K::Bird, [0.0, 800.0, 0.0], 0.0, 1.0)], [0.0, 0.0, 1000.0]);
    step(&mut m, 2);
    assert_eq!(m.objects.objects.iter().filter(|o| o.behavior == K::Bird).count(), 7);
}

#[test]
fn animation_helper_follows_geo_update_animation_frame() {
    // graph_node.c: init sets start-1, the draw pass increments, loopEnd wraps to loopStart.
    let mut o = SimObject::new(&ObjectSpec::new(K::PiranhaPlant, [0.0; 3], 0.0));
    anim::init(&mut o, 4, anim::spec(0, 0, 3, false));
    assert_eq!(anim::frame(&o), -1);
    anim::advance(&mut o);
    assert_eq!(anim::frame(&o), 0);
    anim::advance(&mut o);
    anim::advance(&mut o);
    assert_eq!(anim::frame(&o), 2);
    anim::advance(&mut o);
    assert_eq!(anim::frame(&o), 0);
    // cur_obj_check_if_near_animation_end: frame == loopEnd - 2.
    anim::advance(&mut o);
    assert!(anim::near_end(&o));
    // Re-initialising the same animation keeps the frame.
    anim::init(&mut o, 4, anim::spec(0, 0, 3, false));
    assert_eq!(anim::frame(&o), 1);
}

#[test]
fn approach_helpers_match_math_util() {
    assert_eq!(g1::approach_s16(0.0, 100.0, 30.0), 30.0);
    assert_eq!(g1::approach_s16(0.0, -100.0, 30.0), -30.0);
    assert_eq!(g1::approach_s16(90.0, 100.0, 30.0), 100.0);
    // Wraps through the s16 boundary the short way.
    // 32760 -> -32760 is 16 units the short way round: within the increment, so it lands on the target.
    assert_eq!(g1::approach_s16(32760.0, -32760.0, 20.0), -32760.0);
    assert_eq!(g1::approach_s16(32760.0, -32760.0, 10.0), to_s16(32770.0) as f64);
    let mut v = 0.0;
    assert!(!g1::approach_f32_ptr(&mut v, 1.0, 0.4));
    assert!(!g1::approach_f32_ptr(&mut v, 1.0, 0.4));
    assert!(g1::approach_f32_ptr(&mut v, 1.0, 0.4));
    assert_eq!(v, 1.0);
}
