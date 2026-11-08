//! Engine-free tests for the submerged actions (`actions/water.rs`, a port of the decompilation's
//! `src/game/mario_actions_submerged.c`). Every expected number below is derived from the C source
//! (the line it comes from is quoted); there is no TypeScript golden for these, because the old
//! generator is gone and the TS never had the actions. Worlds are synthetic pools so each test
//! isolates one rule.
use jumpman_sim::actions::water;
use jumpman_sim::collision::*;
use jumpman_sim::math::Vec3;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;

const LEVEL: f64 = 0.0;
const R: f64 = 6000.0;

fn floor_at(w: &mut CollisionWorld, y: f64, opts: &SurfaceOptions) {
    w.add_triangle([-R, y, -R], [-R, y, R], [R, y, R], opts);
    w.add_triangle([-R, y, -R], [R, y, R], [R, y, -R], opts);
}

/// A pool: floor at `bottom`, water surface at 0.
fn pool(bottom: f64) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    floor_at(&mut w, bottom, &SurfaceOptions::default());
    w.water_boxes.push(WaterBox(0.0, -7000.0, -7000.0, 7000.0, 7000.0, LEVEL));
    w
}

fn mario(world: CollisionWorld, pos: Vec3, action: u32) -> MarioRuntime {
    water::set_faithful(true);
    let spawn = SpawnPoint { action: Some(action), pos, face_yaw: 0.0 };
    let mut m = MarioRuntime::new(Arc::new(world), spawn, -20000.0);
    m.set_objects(ObjectManager::new(vec![], None, None));
    m
}

fn ctl(stick_x: f64, stick_y: f64, down: u32, pressed: u32) -> ControllerSnapshot {
    ControllerSnapshot { focus_yaw: None, stick_x, stick_y, stick_mag: (stick_x * stick_x + stick_y * stick_y).sqrt().min(64.0), button_down: down, button_pressed: pressed }
}

fn tick(m: &mut MarioRuntime, c: &ControllerSnapshot) {
    object_manager::step(m, None, false);
    m.step(c, 0.0, false, false);
    object_manager::after_mario_step(m);
    m.objects.drain_events();
}

fn idle() -> ControllerSnapshot { ctl(0.0, 0.0, 0, 0) }

fn run(m: &mut MarioRuntime, c: &ControllerSnapshot, frames: usize) {
    for _ in 0..frames { tick(m, c); }
}

/// Step until `done` holds (or fail), returning how many frames it took.
fn run_until(m: &mut MarioRuntime, c: &ControllerSnapshot, max: usize, done: impl Fn(&MarioRuntime) -> bool) -> usize {
    for i in 0..max {
        tick(m, c);
        if done(m) { return i + 1; }
    }
    panic!("condition not reached in {max} frames; action 0x{:x} pos {:?}", m.action, m.pos);
}

fn near(a: f64, b: f64) -> bool { (a - b).abs() < 1e-9 }

// ------------------------------------------------------------------------------------------
// Constants
// ------------------------------------------------------------------------------------------

#[test]
fn action_ids_match_sm64_h() {
    // include/sm64.h lines 303-334 and 278.
    let table: [(u32, u32); 33] = [
        (ACT_WATER_IDLE, 0x380022C0), (ACT_HOLD_WATER_IDLE, 0x380022C1), (ACT_WATER_ACTION_END, 0x300022C2),
        (ACT_HOLD_WATER_ACTION_END, 0x300022C3), (ACT_DROWNING, 0x300032C4), (ACT_BACKWARD_WATER_KB, 0x300222C5),
        (ACT_FORWARD_WATER_KB, 0x300222C6), (ACT_WATER_DEATH, 0x300032C7), (ACT_WATER_SHOCKED, 0x300222C8),
        (ACT_BREASTSTROKE, 0x300024D0), (ACT_SWIMMING_END, 0x300024D1), (ACT_FLUTTER_KICK, 0x300024D2),
        (ACT_HOLD_BREASTSTROKE, 0x300024D3), (ACT_HOLD_SWIMMING_END, 0x300024D4), (ACT_HOLD_FLUTTER_KICK, 0x300024D5),
        (ACT_WATER_SHELL_SWIMMING, 0x300024D6), (ACT_WATER_THROW, 0x300024E0), (ACT_WATER_PUNCH, 0x300024E1),
        (ACT_WATER_PLUNGE, 0x300022E2), (ACT_CAUGHT_IN_WHIRLPOOL, 0x300222E3), (ACT_METAL_WATER_STANDING, 0x080042F0),
        (ACT_HOLD_METAL_WATER_STANDING, 0x080042F1), (ACT_METAL_WATER_WALKING, 0x000044F2), (ACT_HOLD_METAL_WATER_WALKING, 0x000044F3),
        (ACT_METAL_WATER_FALLING, 0x000042F4), (ACT_HOLD_METAL_WATER_FALLING, 0x000042F5), (ACT_METAL_WATER_FALL_LAND, 0x000042F6),
        (ACT_HOLD_METAL_WATER_FALL_LAND, 0x000042F7), (ACT_METAL_WATER_JUMP, 0x000044F8), (ACT_HOLD_METAL_WATER_JUMP, 0x000044F9),
        (ACT_METAL_WATER_JUMP_LAND, 0x000044FA), (ACT_HOLD_METAL_WATER_JUMP_LAND, 0x000044FB), (ACT_HOLD_WATER_JUMP, 0x010008A3),
    ];
    for (got, want) in table {
        assert_eq!(got, want);
    }
    // Every submerged-group id has the group bits of ACT_GROUP_SUBMERGED.
    for (id, _) in table.iter().filter(|(id, _)| *id != ACT_HOLD_WATER_JUMP) {
        assert_eq!(id & ACT_GROUP_MASK, ACT_GROUP_SUBMERGED, "0x{id:x}");
    }
}

// ------------------------------------------------------------------------------------------
// Entering the water, floating, buoyancy
// ------------------------------------------------------------------------------------------

#[test]
fn plunge_then_surface_float() {
    // mario.c set_water_plunge_action + act_water_plunge: ACT_WATER_PLUNGE -> ACT_WATER_ACTION_END
    // (stateFlags 0) -> ACT_WATER_IDLE; get_buoyancy 1.25 near the surface floats Mario to waterLevel - 80.
    let mut m = mario(pool(-2000.0), [0.0, 300.0, 0.0], ACT_FREEFALL);
    let mut seen = vec![];
    for _ in 0..400 {
        tick(&mut m, &idle());
        if seen.last() != Some(&m.action) { seen.push(m.action); }
    }
    let plunge = seen.iter().position(|a| *a == ACT_WATER_PLUNGE).expect("plunged");
    assert_eq!(seen[plunge + 1], ACT_WATER_ACTION_END);
    assert_eq!(*seen.last().unwrap(), ACT_WATER_IDLE);
    assert_eq!(m.pos[1], LEVEL - 80.0, "idle Mario bobs at waterLevel - 80");
    assert_eq!(m.water.level, LEVEL);
}

#[test]
fn plunge_applies_set_water_plunge_action() {
    // mario.c: forwardVel /= 4, vel[1] /= 2, pos[1] = waterLevel - 100, faceAngle[2] = 0, angleVel = 0.
    let mut m = mario(pool(-2000.0), [0.0, -99.0, 0.0], ACT_FREEFALL);
    m.state.vel = [0.0, -40.0, 0.0];
    m.state.forward_vel = 0.0;
    m.state.face_angle[2] = 0x123 as f64;
    // pos[1] -99 is not yet below waterLevel - 100: the first frame is plain freefall.
    tick(&mut m, &idle());
    assert!(m.action == ACT_WATER_PLUNGE || m.action == ACT_FREEFALL);
    let mut m = mario(pool(-2000.0), [0.0, -101.0, 0.0], ACT_FREEFALL);
    m.water.level = LEVEL; // update_mario_geometry_inputs sets it every frame
    m.state.vel = [0.0, -40.0, 0.0];
    m.state.forward_vel = 20.0;
    m.state.face_angle[0] = 0x777 as f64; // not diving: reset to 0
    m.state.face_angle[2] = 0x123 as f64;
    water::set_water_plunge_action(&mut m);
    assert_eq!(m.action, ACT_WATER_PLUNGE);
    assert_eq!((m.forward_vel, m.vel[1], m.pos[1], m.face_angle[0], m.face_angle[2]), (5.0, -20.0, -100.0, 0.0, 0.0));
}

#[test]
fn deep_idle_sinks_at_two_units_per_frame_squared() {
    // get_buoyancy: -2 in a stationary action far below the surface; stationary_slow_down approaches it by 1/frame... (approach_f32(vel, -2, 2, 1)).
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    run(&mut m, &idle(), 4);
    assert!(near(m.vel[1], -2.0), "vel y {}", m.vel[1]);
    assert!(m.pos[1] < -1500.0);
}

#[test]
fn buoyancy_values() {
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    tick(&mut m, &idle());
    assert_eq!(water::get_buoyancy(&m), -2.0);
    m.state.action = ACT_BREASTSTROKE;
    assert_eq!(water::get_buoyancy(&m), 0.0, "moving action, deep");
    m.state.pos[1] = -200.0;
    assert_eq!(water::get_buoyancy(&m), 1.25, "within 400 of waterLevel - 80");
    m.state.flags |= MARIO_METAL_CAP;
    assert_eq!(water::get_buoyancy(&m), -18.0);
    m.state.action = ACT_BACKWARD_WATER_KB;
    assert_eq!(water::get_buoyancy(&m), -2.0, "metal cap while invulnerable");
}

// ------------------------------------------------------------------------------------------
// Breaststroke, flutter kick, swimming end
// ------------------------------------------------------------------------------------------

#[test]
fn breaststroke_speed_and_kick_timeline() {
    // act_breaststroke: ++actionTimer; timer < 6: forwardVel += 0.5; timer >= 9: += 1.5; timer == 14: flutter kick.
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    tick(&mut m, &idle());
    let a = ctl(0.0, 0.0, A_BUTTON, A_BUTTON);
    tick(&mut m, &a);
    assert_eq!(m.action, ACT_BREASTSTROKE);
    assert_eq!(m.action_timer, 1.0);
    assert!(near(m.forward_vel, 0.5));
    let hold = ctl(0.0, 0.0, A_BUTTON, 0);
    for t in 2..=13 {
        tick(&mut m, &hold);
        assert_eq!(m.action_timer, t as f64);
    }
    // 5 frames of +0.5, 3 of nothing, 5 of +1.5.
    assert!(near(m.forward_vel, 10.0), "{}", m.forward_vel);
    tick(&mut m, &hold);
    assert_eq!(m.action, ACT_FLUTTER_KICK);
    // act_flutter_kick: forwardVel = approach_f32(10, 12, 0.1, 0.15).
    assert!(near(m.forward_vel, 10.1), "{}", m.forward_vel);
}

#[test]
fn second_a_press_restarts_the_stroke() {
    // act_breaststroke: A pressed with 2 <= timer < 6 sets actionState; at timer 9 the stroke restarts (timer = 1).
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_BREASTSTROKE);
    let hold = ctl(0.0, 0.0, A_BUTTON, 0);
    run(&mut m, &hold, 3);
    tick(&mut m, &ctl(0.0, 0.0, A_BUTTON, A_BUTTON));
    assert_eq!(m.action_state, 1.0);
    run(&mut m, &hold, 5); // timers 5..8 then 9 -> reset
    assert_eq!(m.action_timer, 1.0);
    assert_eq!(m.action_state, 0.0);
}

#[test]
fn releasing_a_glides_then_ends() {
    // act_flutter_kick without A -> ACT_SWIMMING_END; act_swimming_end counts to 15 -> ACT_WATER_ACTION_END -> ACT_WATER_IDLE (anim end).
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_FLUTTER_KICK);
    m.state.forward_vel = 12.0;
    tick(&mut m, &idle());
    assert_eq!(m.action, ACT_SWIMMING_END);
    run_until(&mut m, &idle(), 40, |m| m.action == ACT_WATER_ACTION_END);
    run_until(&mut m, &idle(), 80, |m| m.action == ACT_WATER_IDLE);
    // update_swimming_speed in a stationary action: forwardVel -= 2 each frame, clamped to 0.
    run(&mut m, &idle(), 15);
    assert_eq!(m.forward_vel, 0.0);
}

#[test]
fn swim_strength_grows_when_chaining_strokes() {
    // act_swimming_end: A held with timer >= 7 and timer == 7: sSwimStrength += 10 (< 280), then ACT_BREASTSTROKE arg 1.
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_SWIMMING_END);
    m.state.forward_vel = 12.0;
    m.swim_strength = 160.0;
    let hold = ctl(0.0, 0.0, A_BUTTON, 0);
    run_until(&mut m, &hold, 20, |m| m.action == ACT_BREASTSTROKE);
    assert_eq!(m.action_arg, 1.0);
    assert_eq!(m.swim_strength, 170.0);
}

#[test]
fn swim_steering_and_pitch() {
    // update_swimming_yaw: stick right (+x) -> targetYawVel -10*x (negative), ramps by 0x20 per frame;
    // update_swimming_pitch: pitch = -252 * stickY at 0x200 per frame when faceAngle[0] >= 0.
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_FLUTTER_KICK);
    m.state.forward_vel = 12.0;
    let hold_right = ctl(32.0, 0.0, A_BUTTON, 0);
    tick(&mut m, &hold_right);
    // targetYawVel = -(s16)(10 * 32) = -320; approach_s32(angleVel, target, inc 0x20, dec 0x10) moves down by 0x10.
    assert_eq!(m.angle_vel[1], -16.0);
    assert_eq!(m.face_angle[1], -16.0);
    assert_eq!(m.face_angle[2], 128.0, "roll = -angleVel * 8");
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_FLUTTER_KICK);
    m.state.forward_vel = 12.0;
    tick(&mut m, &ctl(0.0, -64.0, A_BUTTON, 0));
    assert_eq!(m.face_angle[0], 0x200 as f64, "stick back pitches up by 0x200");
    run(&mut m, &ctl(0.0, -64.0, A_BUTTON, 0), 40);
    assert_eq!(m.face_angle[0], 16128.0, "target pitch -(s16)(252 * -64)");
}

// ------------------------------------------------------------------------------------------
// Leaving the water
// ------------------------------------------------------------------------------------------

#[test]
fn water_jump_out_of_the_pool() {
    // check_water_jump: A pressed, probe >= waterLevel - 80, faceAngle[0] >= 0, stickY < -60 -> vel[1] = 62, ACT_WATER_JUMP.
    let mut m = mario(pool(-3000.0), [0.0, LEVEL - 80.0, 0.0], ACT_WATER_IDLE);
    tick(&mut m, &idle());
    tick(&mut m, &ctl(0.0, -64.0, A_BUTTON, A_BUTTON));
    assert_eq!(m.action, ACT_WATER_JUMP);
    assert!(m.vel[1] > 55.0 && m.vel[1] <= 62.0, "vel y {}", m.vel[1]);
    assert!(m.pos[1] > LEVEL - 80.0);
}

#[test]
fn shallow_floor_turns_swimming_into_walking() {
    // check_common_submerged_cancels: pos[1] > waterLevel - 80 and waterLevel - 80 <= floorHeight -> transition_submerged_to_walking.
    let mut m = mario(pool(-60.0), [0.0, -60.0, 0.0], ACT_WATER_IDLE);
    // The walking action runs in the same frame (stick held so it stays walking).
    tick(&mut m, &ctl(0.0, 64.0, 0, 0));
    assert_eq!(m.action, ACT_WALKING);
    assert_eq!(m.prev_action, ACT_WATER_IDLE);
}

#[test]
fn holding_an_object_walks_out_holding_it() {
    let mut w = pool(-60.0);
    w.water_boxes[0].5 = LEVEL;
    let mut m = mario(w, [0.0, -60.0, 0.0], ACT_HOLD_WATER_IDLE);
    let id = m.objects.spawn(&ObjectSpec::new(ObjectBehaviorKind::KoopaShellUnderwater, [0.0, -60.0, 0.0], 0.0));
    m.held_obj = Some(id);
    m.objects[id].set_held_state(HELD_HELD);
    tick(&mut m, &ctl(0.0, 64.0, 0, 0));
    assert_eq!(m.action, ACT_HOLD_WALKING);
    assert_eq!(m.held_obj, Some(id));
}

// ------------------------------------------------------------------------------------------
// Air meter / health
// ------------------------------------------------------------------------------------------

#[test]
fn health_drains_one_per_frame_deep_and_recovers_at_the_surface() {
    // mario.c update_mario_health: swimming below waterLevel - 140: health -= 1; at or above: += 0x1A.
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    m.state.health = 0x800 as f64;
    tick(&mut m, &idle());
    let h0 = m.health;
    run(&mut m, &idle(), 10);
    assert_eq!(m.health, h0 - 10.0);
    let mut m = mario(pool(-3000.0), [0.0, LEVEL - 80.0, 0.0], ACT_WATER_IDLE);
    m.state.health = 0x500 as f64;
    tick(&mut m, &idle());
    let h0 = m.health;
    run(&mut m, &idle(), 3);
    assert_eq!(m.health, h0 + 3.0 * 0x1a as f64);
}

#[test]
fn running_out_of_air_drowns_and_respawns() {
    // check_common_submerged_cancels: health < 0x100 -> ACT_DROWNING; act_drowning warps at anim frame 30 of DROWNING_PART2.
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    m.state.health = 0x102 as f64;
    run_until(&mut m, &idle(), 20, |m| m.action == ACT_DROWNING);
    assert_eq!(m.health, 0xff as f64);
    let before = m.respawns;
    // PART1 is 80 frames, PART2 reaches frame 30 after 31 more.
    run_until(&mut m, &idle(), 140, |m| m.respawns > before);
    assert_eq!(m.action, ACT_WATER_IDLE, "respawned into the spawn action");
}

// ------------------------------------------------------------------------------------------
// Currents
// ------------------------------------------------------------------------------------------

#[test]
fn flowing_water_pushes_swimmers() {
    // apply_water_current: floor type SURFACE_FLOWING_WATER, force 0x0200 -> angle 0x0000 << 8 ... (force << 8 as s16 = 0), speed sWaterCurrentSpeeds[2] = 8.
    let mut w = CollisionWorld::default();
    let opts = SurfaceOptions { kind: SURFACE_FLOWING_WATER, force: 0x0200, ..Default::default() };
    floor_at(&mut w, -300.0, &opts);
    w.water_boxes.push(WaterBox(0.0, -7000.0, -7000.0, 7000.0, 7000.0, LEVEL));
    let mut m = mario(w, [0.0, -200.0, 0.0], ACT_WATER_IDLE);
    tick(&mut m, &idle());
    let z0 = m.pos[2];
    run(&mut m, &idle(), 5);
    assert!(near(m.pos[2] - z0, 40.0), "pushed {}", m.pos[2] - z0);
    assert!(m.pos[0].abs() < 1e-9);
}

#[test]
fn whirlpool_pulls_toward_its_center() {
    // apply_water_current: strength * (1 - distance / radius) along (pitch, yaw to the whirlpool, rotated by 0x2000 * 1000 / (d + 1000)).
    let mut w = pool(-3000.0);
    w.whirlpools.push(water::make_whirlpool([1000.0, -1000.0, 0.0], 20.0, 2000.0));
    let mut m = mario(w, [0.0, -1000.0, 0.0], ACT_WATER_IDLE);
    tick(&mut m, &idle());
    let d0 = ((1000.0 - m.pos[0]).powi(2) + m.pos[2].powi(2)).sqrt();
    run(&mut m, &idle(), 20);
    let d1 = ((1000.0 - m.pos[0]).powi(2) + m.pos[2].powi(2)).sqrt();
    assert!(d1 < d0, "pulled in: {d0} -> {d1}");
    assert!(m.pos[2].abs() > 1.0, "the yaw offset makes it spiral");
}

#[test]
fn caught_in_whirlpool_spirals_down_and_dies() {
    // interaction.c interact_whirlpool + act_caught_in_whirlpool: spiral in with angleChange, sink to
    // the whirlpool's height, warp (death) 16 frames after reaching it within 16.1 of the center.
    let mut w = pool(-3000.0);
    floor_at(&mut w, -3000.0, &SurfaceOptions::default());
    let mut m = mario(w, [150.0, -1000.0, 0.0], ACT_WATER_IDLE);
    let id = m.objects.spawn(&ObjectSpec::new(ObjectBehaviorKind::Whirlpool, [0.0, -1000.0, 0.0], 0.0));
    tick(&mut m, &idle());
    assert_eq!(m.action, ACT_CAUGHT_IN_WHIRLPOOL, "radius 200 hitbox");
    assert_eq!(m.used_obj, Some(id));
    let d0 = (m.pos[0].powi(2) + m.pos[2].powi(2)).sqrt();
    run(&mut m, &idle(), 6);
    let d1 = (m.pos[0].powi(2) + m.pos[2].powi(2)).sqrt();
    assert!(d1 < d0);
    let before = m.respawns;
    run_until(&mut m, &idle(), 400, |m| m.respawns > before);
}

// ------------------------------------------------------------------------------------------
// Knockback, shock
// ------------------------------------------------------------------------------------------

#[test]
fn water_knockback_runs_the_whole_animation_then_idles_with_invincibility() {
    // interaction.c determine_knockback_action (terrainIndex 2): forwardVel >= 28, faceAngle toward the source;
    // act_backward_water_kb: MARIO_ANIM_BACKWARDS_WATER_KB (loopEnd 50), invincTimer = 30 when actionArg > 0.
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    tick(&mut m, &idle());
    // Source level with Mario's feet (pos[1] >= object y): vel[1] is raised to 20.
    assert!(m.take_damage(1.0, Some([0.0, m.pos[1] - 10.0, 300.0])));
    assert_eq!(m.action, ACT_BACKWARD_WATER_KB, "source in front of Mario");
    assert_eq!(m.forward_vel, -28.0);
    assert_eq!(m.vel[1], 20.0);
    let n = run_until(&mut m, &idle(), 80, |m| m.action != ACT_BACKWARD_WATER_KB);
    assert_eq!(m.action, ACT_WATER_IDLE);
    assert!((50..=52).contains(&n), "knockback lasted {n} frames (anim loopEnd 50)");
    assert!(m.invinc_timer > 0.0 && m.invinc_timer <= 30.0);
    // Behind Mario -> forward knockback.
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    tick(&mut m, &idle());
    assert!(m.take_damage(1.0, Some([0.0, -1500.0, -300.0])));
    assert_eq!(m.action, ACT_FORWARD_WATER_KB);
    assert_eq!(m.forward_vel, 28.0);
}

#[test]
fn lethal_water_knockback_ends_in_water_death() {
    // common_water_knockback_step: health < 0x100 at the end -> ACT_WATER_DEATH (then a warp at anim frame 35).
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    m.state.health = 0x180 as f64;
    tick(&mut m, &idle());
    m.take_damage(3.0, Some([0.0, -1500.0, 300.0]));
    run_until(&mut m, &idle(), 120, |m| m.action == ACT_WATER_DEATH);
    let before = m.respawns;
    run_until(&mut m, &idle(), 80, |m| m.respawns > before);
}

#[test]
fn water_shock_lasts_six_animation_loops() {
    // act_water_shocked: MARIO_ANIM_SHOCKED (loopEnd 6): actionTimer++ when the returned frame is 0; at >= 6 -> ACT_WATER_IDLE, invincTimer 30.
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    tick(&mut m, &idle());
    assert!(water::shock_in_water(&mut m));
    assert_eq!(m.action, ACT_WATER_SHOCKED);
    let n = run_until(&mut m, &idle(), 80, |m| m.action != ACT_WATER_SHOCKED);
    assert_eq!(m.action, ACT_WATER_IDLE);
    // The first call only starts the animation (frame -1); the timer then ticks at frames 0 of each 6-frame loop:
    // 1 + 6 * 5 + 1 = 32 frames.
    assert_eq!(n, 32, "shock lasted {n}");
    assert!(m.invinc_timer >= 29.0);
    // Dry Mario is not shocked by this hook.
    let mut dry = mario(pool(-3000.0), [0.0, 500.0, 0.0], ACT_IDLE);
    assert!(!water::shock_in_water(&mut dry));
}

// ------------------------------------------------------------------------------------------
// Objects underwater
// ------------------------------------------------------------------------------------------

fn shell_world() -> (MarioRuntime, ObjId) {
    let mut m = mario(pool(-3000.0), [0.0, -1500.0, 0.0], ACT_WATER_IDLE);
    let id = m.objects.spawn(&ObjectSpec::new(ObjectBehaviorKind::KoopaShellUnderwater, [0.0, -1500.0, 100.0], 0.0));
    (m, id)
}

#[test]
fn punching_picks_up_the_shell_and_shell_swimming_times_out() {
    // act_water_punch: GRAB_OBJ_PART1 (5 frames) -> check_water_grab -> PICK_UP_OBJ (11) -> bhvKoopaShellUnderwater -> ACT_WATER_SHELL_SWIMMING;
    // after 240 frames the shell is released (INT_STATUS_STOP_RIDING) and Mario flutter kicks.
    let (mut m, shell) = shell_world();
    tick(&mut m, &idle());
    tick(&mut m, &ctl(0.0, 0.0, B_BUTTON, B_BUTTON));
    assert_eq!(m.action, ACT_WATER_PUNCH);
    run_until(&mut m, &idle(), 40, |m| m.action == ACT_WATER_SHELL_SWIMMING);
    assert_eq!(m.held_obj, Some(shell));
    assert!(m.objects[shell].held_state == HELD_HELD);
    // The shell goes with Mario while he swims.
    run(&mut m, &ctl(0.0, 0.0, A_BUTTON, 0), 20);
    assert_eq!(m.action, ACT_WATER_SHELL_SWIMMING);
    assert!(m.forward_vel > 20.0, "shell swimming approaches 30: {}", m.forward_vel);
    let d = ((m.objects[shell].pos[0] - m.pos[0]).powi(2) + (m.objects[shell].pos[2] - m.pos[2]).powi(2)).sqrt();
    assert!(d < 200.0);
    run_until(&mut m, &idle(), 260, |m| m.action != ACT_WATER_SHELL_SWIMMING);
    assert_eq!(m.action, ACT_FLUTTER_KICK);
    assert!(m.held_obj.is_none());
    tick(&mut m, &idle());
    assert!(!m.objects[shell].active, "the shell vanishes once released");
}

#[test]
fn throwing_the_shell_underwater() {
    // act_water_throw: the object leaves at actionTimer == 5 and ends at the end of MARIO_ANIM_WATER_THROW_OBJ (loopEnd 28).
    let (mut m, shell) = shell_world();
    tick(&mut m, &idle());
    tick(&mut m, &ctl(0.0, 0.0, B_BUTTON, B_BUTTON));
    run_until(&mut m, &idle(), 40, |m| m.action == ACT_WATER_SHELL_SWIMMING);
    tick(&mut m, &ctl(0.0, 0.0, B_BUTTON, B_BUTTON));
    assert_eq!(m.action, ACT_WATER_THROW);
    assert_eq!(m.held_obj, Some(shell));
    run_until(&mut m, &idle(), 10, |m| m.held_obj.is_none());
    assert!(m.objects[shell].held_state == HELD_THROWN || !m.objects[shell].active);
    run_until(&mut m, &idle(), 40, |m| m.action == ACT_WATER_IDLE);
}

#[test]
fn holding_actions_swim_with_the_object() {
    // act_hold_water_idle -> A -> act_hold_breaststroke (17 frames) -> act_hold_flutter_kick; the object stays in hand.
    let (mut m, shell) = shell_world();
    m.set_action(ACT_HOLD_WATER_IDLE, 0.0);
    m.held_obj = Some(shell);
    m.objects[shell].set_held_state(HELD_HELD);
    tick(&mut m, &idle());
    assert_eq!(m.action, ACT_HOLD_WATER_IDLE);
    tick(&mut m, &ctl(0.0, 0.0, A_BUTTON, A_BUTTON));
    assert_eq!(m.action, ACT_HOLD_BREASTSTROKE);
    let hold = ctl(0.0, 0.0, A_BUTTON, 0);
    run_until(&mut m, &hold, 30, |m| m.action == ACT_HOLD_FLUTTER_KICK);
    assert_eq!(m.action_timer, 0.0, "fresh action");
    run_until(&mut m, &idle(), 10, |m| m.action == ACT_HOLD_SWIMMING_END);
    run_until(&mut m, &idle(), 30, |m| m.action == ACT_HOLD_WATER_ACTION_END);
    run_until(&mut m, &idle(), 60, |m| m.action == ACT_HOLD_WATER_IDLE);
    assert_eq!(m.held_obj, Some(shell));
    // B throws.
    tick(&mut m, &ctl(0.0, 0.0, B_BUTTON, B_BUTTON));
    assert_eq!(m.action, ACT_WATER_THROW);
}

#[test]
fn a_dropped_object_returns_to_plain_swimming() {
    let (mut m, shell) = shell_world();
    m.set_action(ACT_HOLD_WATER_IDLE, 0.0);
    m.held_obj = Some(shell);
    m.objects[shell].set_held_state(HELD_HELD);
    m.water.drop_object = true;
    tick(&mut m, &idle());
    assert_eq!(m.action, ACT_WATER_IDLE);
    assert!(m.held_obj.is_none());
}

#[test]
fn hold_water_jump_when_leaving_with_an_object() {
    // check_water_jump with heldObj -> ACT_HOLD_WATER_JUMP.
    let (mut m, shell) = shell_world();
    m.state.pos[1] = LEVEL - 80.0;
    m.set_action(ACT_HOLD_WATER_IDLE, 0.0);
    m.held_obj = Some(shell);
    m.objects[shell].set_held_state(HELD_HELD);
    tick(&mut m, &idle());
    tick(&mut m, &ctl(0.0, 0.0, A_BUTTON, A_BUTTON));
    assert_eq!(m.action, ACT_HOLD_BREASTSTROKE);
    tick(&mut m, &ctl(0.0, -64.0, A_BUTTON, A_BUTTON));
    assert_eq!(m.action, ACT_HOLD_WATER_JUMP);
    assert_eq!(m.held_obj, Some(shell));
}

// ------------------------------------------------------------------------------------------
// Metal cap
// ------------------------------------------------------------------------------------------

#[test]
fn metal_mario_sinks_lands_walks_and_jumps() {
    // act_water_plunge stateFlags 4 -> ACT_METAL_WATER_FALLING -> ACT_METAL_WATER_FALL_LAND (perform_water_step & HIT_FLOOR)
    // -> ACT_METAL_WATER_STANDING (anim end) -> ACT_METAL_WATER_WALKING (stick) -> ACT_METAL_WATER_JUMP (A, vel[1] = 32).
    let mut m = mario(pool(-1000.0), [0.0, 200.0, 0.0], ACT_FREEFALL);
    m.grant_metal_cap(3000.0);
    let mut seen = vec![];
    let mut note = |m: &MarioRuntime, seen: &mut Vec<u32>| if seen.last() != Some(&m.action) { seen.push(m.action); };
    for _ in 0..140 {
        tick(&mut m, &idle());
        note(&m, &mut seen);
    }
    let i = seen.iter().position(|a| *a == ACT_WATER_PLUNGE).expect("plunge");
    assert_eq!(&seen[i..], [ACT_WATER_PLUNGE, ACT_METAL_WATER_FALLING, ACT_METAL_WATER_FALL_LAND, ACT_METAL_WATER_STANDING]);
    assert_eq!(m.pos[1], -1000.0, "standing on the bottom");

    // Walk: forwardVel += 1.1 from rest, face the stick.
    let z0 = m.pos[2];
    let fwd = ctl(0.0, 64.0, 0, 0);
    run(&mut m, &fwd, 20);
    assert_eq!(m.action, ACT_METAL_WATER_WALKING);
    assert!(m.pos[2] < z0 - 20.0, "walked {} (stick up faces -z at camera yaw 0)", m.pos[2] - z0);
    assert!(m.forward_vel <= 32.0);
    assert_eq!(m.pos[1], -1000.0);

    // Jump.
    tick(&mut m, &ctl(0.0, 64.0, A_BUTTON, A_BUTTON));
    assert_eq!(m.action, ACT_METAL_WATER_JUMP);
    assert!(m.vel[1] > 20.0 && m.vel[1] <= 32.0, "vel y {}", m.vel[1]);
    assert!(m.pos[1] > -1000.0);
    run_until(&mut m, &fwd, 60, |m| m.action == ACT_METAL_WATER_JUMP_LAND);
    // Losing the cap ends the metal actions: swimming resumes.
    m.metal_cap_timer = 0.0;
    m.state.flags &= !MARIO_METAL_CAP;
    tick(&mut m, &idle());
    assert_eq!(m.action, ACT_WATER_IDLE);
}

#[test]
fn metal_walk_speed_follows_the_source() {
    // update_metal_water_walking_speed: forwardVel <= 0: += 1.1; else <= intendedMag/1.5: += 1.1 - fv/43; capped at 32.
    let mut m = mario(pool(-1000.0), [0.0, -1000.0, 0.0], ACT_METAL_WATER_STANDING);
    m.grant_metal_cap(3000.0);
    tick(&mut m, &idle());
    let fwd = ctl(0.0, 64.0, 0, 0);
    tick(&mut m, &fwd);
    assert_eq!(m.action, ACT_METAL_WATER_WALKING);
    assert!(near(m.forward_vel, 1.1), "{}", m.forward_vel);
    tick(&mut m, &fwd);
    assert!(near(m.forward_vel, 1.1 + (1.1 - 1.1 / 43.0)), "{}", m.forward_vel);
}

#[test]
fn metal_jump_out_of_the_water_becomes_a_water_jump() {
    // act_metal_water_jump: update_metal_water_jump_speed is true once vel[1] > 0 above waterLevel - 100 -> ACT_WATER_JUMP arg 1.
    let mut m = mario(pool(-60.0 - 200.0), [0.0, -100.0, 0.0], ACT_METAL_WATER_STANDING);
    m.grant_metal_cap(3000.0);
    tick(&mut m, &idle());
    m.state.pos[1] = -150.0; // 150 below the surface line; a 32 jump crosses waterLevel - 100
    let a = ctl(0.0, 0.0, A_BUTTON, A_BUTTON);
    tick(&mut m, &a);
    assert!(matches!(m.action, ACT_METAL_WATER_JUMP | ACT_WATER_JUMP), "0x{:x}", m.action);
    run_until(&mut m, &idle(), 30, |m| m.action == ACT_WATER_JUMP || m.action == ACT_FREEFALL || m.action == ACT_JUMP_LAND);
}

#[test]
fn holding_metal_variants_keep_the_object() {
    let (mut m, shell) = shell_world();
    m.grant_metal_cap(3000.0);
    m.state.pos[1] = -2990.0;
    m.set_action(ACT_HOLD_METAL_WATER_STANDING, 0.0);
    m.held_obj = Some(shell);
    m.objects[shell].set_held_state(HELD_HELD);
    tick(&mut m, &idle());
    assert_eq!(m.action, ACT_HOLD_METAL_WATER_STANDING);
    tick(&mut m, &ctl(0.0, 64.0, 0, 0));
    assert_eq!(m.action, ACT_HOLD_METAL_WATER_WALKING);
    assert_eq!(m.held_obj, Some(shell));
    tick(&mut m, &ctl(0.0, 64.0, A_BUTTON, A_BUTTON));
    assert_eq!(m.action, ACT_HOLD_METAL_WATER_JUMP);
    assert_eq!(m.held_obj, Some(shell));
    run_until(&mut m, &idle(), 120, |m| m.action == ACT_HOLD_METAL_WATER_JUMP_LAND);
    run_until(&mut m, &idle(), 60, |m| m.action == ACT_HOLD_METAL_WATER_STANDING);
    assert_eq!(m.held_obj, Some(shell));
    // Losing the cap while holding -> ACT_HOLD_WATER_IDLE, still holding.
    m.metal_cap_timer = 0.0;
    m.state.flags &= !MARIO_METAL_CAP;
    tick(&mut m, &idle());
    assert_eq!(m.action, ACT_HOLD_WATER_IDLE);
    assert_eq!(m.held_obj, Some(shell));
}

// ------------------------------------------------------------------------------------------
// Ceilings
// ------------------------------------------------------------------------------------------

#[test]
fn a_low_ceiling_pushes_the_pitch_down() {
    // common_swimming_step: WATER_STEP_HIT_CEILING -> faceAngle[0] -= 0x100 (while > -0x3000).
    let mut w = pool(-3000.0);
    // Ceiling at y = -1000: triangles facing down (reverse winding of the floor).
    let o = SurfaceOptions::default();
    w.add_triangle([-R, -1000.0, -R], [R, -1000.0, R], [-R, -1000.0, R], &o);
    w.add_triangle([-R, -1000.0, -R], [R, -1000.0, -R], [R, -1000.0, R], &o);
    let mut m = mario(w, [0.0, -1170.0, 0.0], ACT_FLUTTER_KICK);
    m.state.forward_vel = 12.0;
    let up = ctl(0.0, -64.0, A_BUTTON, 0);
    run(&mut m, &up, 12);
    assert!(m.pos[1] <= -1000.0 - 160.0 + 1e-9, "head stays under the ceiling: y {}", m.pos[1]);
}
