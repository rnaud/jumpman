//! Alternate playable characters: Link (src/sim/linkController.ts) and Banjo
//! (src/sim/banjoController.ts). `step_link` / `step_banjo` return true when they handled
//! the frame (the normal action loop is skipped).
use crate::banjo_ext::Ability;
use crate::link::physics;
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::{MarioRuntime, MovementMode};
use crate::math::*;
use crate::state::*;

// ---------------------------------------------------------------------------
// Link
// ---------------------------------------------------------------------------

// Reuse walking and attacking IDs so course actors see Link through their usual interactions.
pub const LINK_BACKFLIP: f64 = 0x50 as f64;
pub const LINK_HOP_LEFT: f64 = 0x51 as f64;
pub const LINK_HOP_RIGHT: f64 = 0x52 as f64;
const SLASH: f64 = 0x53 as f64;
const FINISHER: f64 = 0x54 as f64;
const ROLL: f64 = 1.0;
const GUARD: f64 = 0x4c as f64;

pub fn link_finishing(m: &MarioRuntime) -> bool { m.action == ACT_PUNCHING && m.action_arg == FINISHER }

/// `swordReach(m)`: extra punch radius while Link slashes. Mario: 0.
pub fn sword_reach(m: &MarioRuntime) -> f64 {
    if m.action == ACT_PUNCHING && (m.action_arg == SLASH || m.action_arg == FINISHER) && m.flags & MARIO_PUNCHING != 0 { 82.0 } else { crate::link::moves::extra_reach(m) }
}

/// `linkGuarding(m)`: Link's shield is up (`ACT_IDLE` with `action_arg == 0x4c`).
pub fn link_guarding(m: &MarioRuntime) -> bool { m.action == ACT_IDLE && m.action_arg == GUARD }

pub fn link_rolling(m: &MarioRuntime) -> bool { m.action == ACT_WALKING && m.action_arg == ROLL }

fn js_sign(x: f64) -> f64 { if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { x } }

/// OoT free movement adapted to Jumpman's units and 30 Hz collision steps.
pub fn step_link(m: &mut MarioRuntime) -> bool {
    if m.link.enabled && m.action & ACT_FLAG_AIR == 0 {
        m.link.phys.fall_start = m.pos[1];
    }
    if crate::link::moves::step(m) { return true; }
    if m.held_obj.is_some() || m.riding_shell.is_some() { return false; }
    let oot = m.link.enabled && !m.link.phys.legacy;
    if m.action == ACT_FREEFALL {
        m.action_timer += 1.0;
        if oot { return oot_air(m); }
        let vy = m.vel[1];
        let hit = m.step_air(AIR_STEP_CHECK_LEDGE_GRAB);
        if hit == AIR_STEP_LANDED as i32 {
            let fv = m.forward_vel;
            m.land_event(vy, fv);
            m.set_action(ACT_IDLE, 0.0);
        } else if hit == AIR_STEP_GRABBED_LEDGE as i32 {
            m.set_action(ACT_LEDGE_GRAB, 0.0);
        } else if hit == AIR_STEP_HIT_LAVA_WALL as i32 {
            m.set_action(ACT_LAVA_BOOST, 0.0);
        }
        return true;
    }
    if m.action != ACT_IDLE && m.action != ACT_WALKING && m.action != ACT_PUNCHING { return false; }

    if m.input & INPUT_OFF_FLOOR != 0 {
        m.set_action(ACT_FREEFALL, 0.0);
        return true;
    }
    if oot {
        if m.action == ACT_IDLE && m.action_arg == physics::ARG_STUN { return oot_stun(m); }
        if m.action != ACT_PUNCHING && !link_rolling(m) && steep_for_link(m) { return oot_slide(m); }
    }
    // Player_CalcSpeedAndYawFromControlStick's curved mode, scaled to course units.
    let magnitude = if m.movement_mode == MovementMode::Side { m.controller.stick_x.abs() } else { m.controller.stick_mag };
    let curve = 1.0 - coss((magnitude - 20.0) * 450.0);
    let target = if oot {
        physics::walk_target(magnitude, physics::uphill_sin(m, m.face_angle[1]), physics::run_cap(m.link.inv.boots))
    } else if magnitude < 20.0 { 0.0 } else { ((curve * curve * 30.0 + 7.0) * 0.56).min(32.0) };

    let mut reversed = false;
    if !link_rolling(m) && m.action != ACT_PUNCHING {
        if let Some(focus) = m.controller.focus_yaw {
            m.face_angle[1] = focus;
        } else if target > 0.0 && !link_guarding(m) {
            let turn = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
            let rate = if oot { physics::TURN_PER_TICK } else { 0x1600 as f64 };
            if oot && m.forward_vel >= 1.0 && turn.abs() > physics::REVERSE_YAW {
                // func_8083C484: a stick pushed back against the heading brings him to a stop first.
                mario_set_forward_vel(m, 0.0);
                reversed = true;
            }
            m.face_angle[1] = if m.forward_vel < 1.0 {
                m.intended_yaw
            } else {
                to_s16(m.face_angle[1] + clamp(turn, -rate, rate)) as f64
            };
        }
        let pressed = m.controller.button_pressed;
        if pressed & B_BUTTON != 0 {
            m.set_action(ACT_PUNCHING, SLASH);
        } else if m.controller.focus_yaw.is_some() && target > 0.0 && (pressed & A_BUTTON != 0)
            && (to_s16(m.intended_yaw - m.face_angle[1]) as f64).abs() > 0x2000 as f64
        {
            let delta = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
            let backwards = delta.abs() > 0x6000 as f64;
            let dodge_yaw = m.face_angle[1] + if backwards { 0x8000 as f64 } else { js_sign(delta) * 0x4000 as f64 };
            m.set_action(ACT_FREEFALL, if backwards { LINK_BACKFLIP } else if delta > 0.0 { LINK_HOP_LEFT } else { LINK_HOP_RIGHT });
            let (fv, vy) = if !oot { (27.0, if backwards { 38.0 } else { 28.0 }) } else if backwards { physics::backflip() } else { physics::side_hop() };
            m.forward_vel = fv;
            m.vel = [sins(dodge_yaw) * fv, vy, coss(dodge_yaw) * fv];
            return true;
        } else if m.controller.button_down & Z_TRIG != 0 {
            if !link_guarding(m) { m.set_action(ACT_IDLE, GUARD); }
        } else if target > 0.0 && pressed & A_BUTTON != 0 {
            m.set_action(ACT_WALKING, ROLL);
        } else {
            let action = if target > 0.0 || m.forward_vel > 2.0 { ACT_WALKING } else { ACT_IDLE };
            if m.action != action || link_guarding(m) || m.action_arg == physics::ARG_SLIDE { m.set_action(action, 0.0); }
        }
    }

    let mut speed = if link_guarding(m) || reversed { 0.0 } else { target };
    if m.action == ACT_PUNCHING {
        speed = 0.0;
        let finishing = link_finishing(m);
        if !finishing && m.action_timer >= 4.0 && m.controller.button_pressed & B_BUTTON != 0 { m.action_state = 1.0; }
        if m.action_timer >= 2.0 && m.action_timer < if finishing { 6.0 } else { 5.0 } { m.flags |= MARIO_PUNCHING; }
        m.action_timer += 1.0;
        if !finishing && m.action_state == 1.0 && m.action_timer >= 9.0 {
            m.set_action(ACT_PUNCHING, FINISHER);
        } else if m.action_timer >= if finishing { 25.0 } else { 13.0 } {
            m.set_action(ACT_IDLE, 0.0);
        }
    } else if link_rolling(m) {
        // Player_Action_Roll uses 1.5x stick speed and exits at animation frame 20.
        speed = if oot { physics::roll_target(target) } else { (target * 1.5).max(12.0) };
        if m.action_timer == 6.0 { m.invinc_timer = m.invinc_timer.max(if oot { physics::ROLL_INVINC_TICKS } else { 10.0 }); }
        m.action_timer += 1.0;
        if m.action_timer >= 16.0 { m.set_action(ACT_WALKING, 0.0); }
    }
    let fv = if oot { approach_f32(m.forward_vel, speed, physics::GROUND_ACCEL, physics::GROUND_DECEL) } else { approach_f32(m.forward_vel, speed, 4.0, 6.0) };
    mario_set_forward_vel(m, fv);
    if m.controller.focus_yaw.is_some() && m.action == ACT_WALKING && !link_rolling(m) {
        m.vel[0] = sins(m.intended_yaw) * m.forward_vel;
        m.vel[2] = coss(m.intended_yaw) * m.forward_vel;
    }
    let hit = m.step_ground();
    if hit == GROUND_STEP_LEFT_GROUND as i32 {
        let was_rolling = link_rolling(m);
        let fv = m.forward_vel;
        m.set_action(ACT_FREEFALL, 0.0);
        m.vel[1] = if !oot {
            if fv >= 12.0 { 38.0 } else { 0.0 }
        } else if fv > physics::ledge_jump_speed() && !was_rolling {
            // func_8083A4A8: the automatic jump off an edge.
            physics::ledge_jump_vy(fv)
        } else {
            0.0
        };
    } else if hit == GROUND_STEP_HIT_WALL as i32 && link_rolling(m) && (!oot || m.forward_vel >= physics::bonk_speed()) {
        m.set_action(ACT_IDLE, 0.0);
        mario_set_forward_vel(m, 0.0);
    }
    true
}

/// Link's SM64 steep floors: the slippery classes at SM64's own thresholds, ordinary floors from 45 degrees
/// (OoT walks anything its collision calls a floor and slides only on flagged surfaces).
fn steep_for_link(m: &MarioRuntime) -> bool {
    let Some(f) = m.floor.as_ref() else { return false };
    let limit = match crate::mario_helpers::mario_get_floor_class(&m.state) {
        crate::surfaces::SURFACE_CLASS_VERY_SLIPPERY => 0.9659258,
        crate::surfaces::SURFACE_CLASS_SLIPPERY => 0.9396926,
        _ => 0.7071068,
    };
    f.normal[1] <= limit && f.normal[1] > 0.0
}

/// `Player_Action_SlideOnSlope`: down the fall line at `(1 - ny) * 40` (at most 10 units per frame), stick ignored.
fn oot_slide(m: &mut MarioRuntime) -> bool {
    let n = m.floor.as_ref().map(|f| f.normal).unwrap_or([0.0, 1.0, 0.0]);
    let (target, incr) = physics::slide(n[1]);
    if m.action != ACT_WALKING || m.action_arg != physics::ARG_SLIDE { m.set_action(ACT_WALKING, physics::ARG_SLIDE); }
    m.face_angle[1] = atan2s(n[2], n[0]) as f64;
    let fv = approach_f32(m.forward_vel, target, incr, n[1] * 0.01);
    mario_set_forward_vel(m, fv);
    m.vel[0] = sins(m.face_angle[1]) * m.forward_vel;
    m.vel[2] = coss(m.face_angle[1]) * m.forward_vel;
    let hit = m.step_ground();
    if hit == GROUND_STEP_LEFT_GROUND as i32 {
        m.set_action(ACT_FREEFALL, 0.0);
        m.vel[1] = 0.0;
    }
    true
}

/// The stagger after a hard landing (`fallDamageStunTimer`): he cannot walk for 10 or 20 frames.
fn oot_stun(m: &mut MarioRuntime) -> bool {
    m.link.phys.stun -= 1.0;
    let fv = approach_f32(m.forward_vel, 0.0, physics::GROUND_DECEL, physics::GROUND_DECEL);
    mario_set_forward_vel(m, fv);
    let hit = m.step_ground();
    if hit == GROUND_STEP_LEFT_GROUND as i32 {
        m.set_action(ACT_FREEFALL, 0.0);
    } else if m.link.phys.stun <= 0.0 {
        m.set_action(ACT_IDLE, 0.0);
    }
    true
}

/// One airborne tick with OoT's gravity (`Player_Action_8084411C`): ballistic, with the faint steering of
/// `func_8083DFE0` for a plain fall or a ledge jump, and the landing rules of `func_80843E64`.
fn oot_air(m: &mut MarioRuntime) -> bool {
    let arg = m.action_arg;
    let hop = arg == LINK_BACKFLIP || arg == LINK_HOP_LEFT || arg == LINK_HOP_RIGHT || arg == crate::link::moves::ARG_JUMP_FORWARD;
    if !hop {
        let cap = physics::run_cap(m.link.inv.boots);
        let mag = m.controller.stick_mag.min(physics::STICK_MAX);
        let (mut fv, mut yaw) = (m.forward_vel.clamp(-cap, cap), m.face_angle[1]);
        if mag > 0.0 {
            let diff = to_s16(m.intended_yaw - yaw) as f64;
            if diff.abs() > physics::REVERSE_YAW {
                fv = approach_f32(fv, 0.0, crate::link::combat::accel(1.0), crate::link::combat::accel(1.0));
                if fv == 0.0 { yaw = m.intended_yaw; }
            } else {
                let target = (mag * 0.8 * 0.14 * crate::link::combat::speed(1.0)).min(cap);
                fv = approach_f32(fv, target, crate::link::combat::accel(0.05), crate::link::combat::accel(0.1));
                yaw = to_s16(yaw + clamp(diff, -200.0 / 1.5, 200.0 / 1.5)) as f64;
            }
        }
        m.forward_vel = fv;
        m.face_angle[1] = yaw;
        m.vel[0] = sins(yaw) * fv;
        m.vel[2] = coss(yaw) * fv;
    }
    let locked = m.controller.focus_yaw.is_some();
    let hit = physics::air_step(m, locked);
    if hit == AIR_STEP_LANDED as i32 {
        let (fv, vy) = (m.forward_vel, m.vel[1]);
        m.land_event(vy, fv);
        let fall = m.link.phys.fall_start - m.pos[1];
        let forward = m.controller.stick_mag > 20.0 && (to_s16(m.intended_yaw - m.face_angle[1]) as f64).abs() <= 0x2000 as f64;
        match if hop { physics::Landing::Soft } else { physics::landing(fall, forward) } {
            physics::Landing::Roll => {
                m.set_action(ACT_WALKING, ROLL);
                m.forward_vel = fv.max(physics::ledge_jump_speed());
                m.action_timer = 0.0;
            }
            physics::Landing::Hurt { damage, stun } => {
                if m.link.nayru <= 0.0 {
                    m.link.inv.health = (m.link.inv.health - damage).max(0);
                    if m.link.inv.health == 0 { m.state.health = 0xff as f64; }
                }
                m.invinc_timer = m.invinc_timer.max(physics::FALL_INVINC_TICKS);
                m.events.push(crate::state::MarioEvent::Hurt { damage: damage as f64 / 8.0 });
                m.link.phys.stun = physics::stun_ticks(stun);
                m.set_action(ACT_IDLE, physics::ARG_STUN);
            }
            _ => { m.set_action(ACT_IDLE, 0.0); }
        }
    } else if hit == AIR_STEP_GRABBED_LEDGE as i32 {
        m.set_action(ACT_LEDGE_GRAB, 0.0);
    } else if hit == AIR_STEP_HIT_LAVA_WALL as i32 {
        m.set_action(ACT_LAVA_BOOST, 0.0);
    }
    true
}

// ---------------------------------------------------------------------------
// Banjo
// ---------------------------------------------------------------------------
//
// Banjo-Kazooie's moveset from src/core2/bs/*.c in n64decomp/banjo-kazooie. Each move borrows
// the SM64 action ID with the same role; the action argument marks the BK version.
// Rare's units per second are converted to Jumpman's 30 Hz frames at Banjo's 1.25 model scale.
const SCALE: f64 = 1.25;
pub(crate) const fn speed(units_per_second: f64) -> f64 { units_per_second * SCALE / 30.0 }
pub(crate) const fn accel(units_per_second2: f64) -> f64 { units_per_second2 * SCALE / 900.0 }

pub const BANJO_FLAP: f64 = 0x60 as f64;
pub const BANJO_JUMP: f64 = 0x61 as f64;
pub const BANJO_FLIP: f64 = 0x62 as f64;
pub const BANJO_CLAW: f64 = 0x63 as f64;
pub const BANJO_ROLL: f64 = 0x64 as f64;
pub const BANJO_BARGE: f64 = 0x65 as f64;
pub const BANJO_PECK: f64 = 0x66 as f64;

pub fn banjo_flapping(m: &MarioRuntime) -> bool { m.action == ACT_FREEFALL && m.action_arg == BANJO_FLAP }
pub fn banjo_move(m: &MarioRuntime) -> f64 {
    if m.action_arg >= BANJO_FLAP && m.action_arg <= crate::banjo_ext::BANJO_LAST { m.action_arg } else { 0.0 }
}

// baphysics.c and bsjump.c
pub(crate) const GRAVITY: f64 = accel(2700.0);
pub(crate) const TERMINAL: f64 = -75.0;
pub(crate) const JUMP_VELOCITY: f64 = speed(710.0);
const JUMP_HELD_GRAVITY: f64 = accel(1350.0);
pub(crate) const MAX_WALK: f64 = speed(500.0);
// bsbflap.c
const FLAP_IMPULSE: f64 = speed(280.0);
const FLAP_GRAVITY: f64 = accel(1100.0);
const FLAP_TERMINAL: f64 = speed(-399.9);
const FLAP_FRAMES: f64 = 75.0; // seconds(2.5)
const FLAP_PERIOD: f64 = 9.0;
// bsbflip.c
const FLIP_VELOCITY: f64 = speed(920.0);
const FLIP_GRAVITY: f64 = accel(1200.0);
const FLIP_TERMINAL: f64 = speed(-533.3);
const FLIP_FORWARD: f64 = speed(200.0);
const FLIP_WINDUP: f64 = 5.0;
// bsclaw.c: 1.3 s swipe, three hitbox windows, lunge at 160 until 0.5788.
const CLAW_FRAMES: f64 = 39.0; // seconds(1.3)
const CLAW_LUNGE: f64 = speed(160.0);
// bstwirl.c (BS_31_ROLL): 0.9 s, 600 units/s, hitbox until 0.8011.
const ROLL_FRAMES: f64 = 27.0; // seconds(0.9)
const ROLL_SPEED: f64 = speed(600.0);
// bsbbarge.c: wind up, charge at 850 (B held) or 500, shed 80 per frame below the hold, stop at 200.
const BARGE_WINDUP: f64 = 4.0;
const BARGE_FULL: f64 = speed(850.0);
const BARGE_SHORT: f64 = speed(500.0);
const BARGE_DECAY: f64 = speed(80.0);
const BARGE_END: f64 = speed(200.0);
const BARGE_FRAMES: f64 = 30.0; // seconds(1.0)
// bsbpeck.c: gravity -1400, 120 units/s hop per peck, 0.5 s of pecking, then a 0.2 s recoil.
const PECK_GRAVITY: f64 = accel(1400.0);
const PECK_HOP: f64 = speed(120.0);
const PECK_FRAMES: f64 = 15.0; // seconds(0.5)
const PECK_END: f64 = 21.0; // seconds(0.7)
const PECK_LOOP: f64 = 11.0; // seconds(0.35) = Math.round(10.5)

pub(crate) const FLAP_FROM: [u32; 10] = [ACT_JUMP, ACT_DOUBLE_JUMP, ACT_TRIPLE_JUMP, ACT_FREEFALL, ACT_SIDE_FLIP,
    ACT_BACKFLIP, ACT_STEEP_JUMP, ACT_WALL_KICK_AIR, ACT_LONG_JUMP, ACT_JUMP_KICK];

pub(crate) fn is_crouched(action: u32) -> bool {
    action == ACT_CROUCHING || action == ACT_START_CROUCHING || action == ACT_STOP_CROUCHING
}

/// A plain standing, walking or landing action where BK's button moves may start.
pub(crate) fn on_basic_ground(m: &MarioRuntime) -> bool {
    let group = m.action & ACT_GROUP_MASK;
    (group == ACT_GROUP_STATIONARY || group == ACT_GROUP_MOVING)
        && m.action & (ACT_FLAG_AIR | ACT_FLAG_SWIMMING | ACT_FLAG_INVULNERABLE | ACT_FLAG_INTANGIBLE | ACT_FLAG_METAL_WATER
            | ACT_FLAG_BUTT_OR_STOMACH_SLIDE | ACT_FLAG_DIVING | ACT_FLAG_RIDING_SHELL) == 0
        && banjo_move(m) == 0.0
}

pub(crate) fn start(m: &mut MarioRuntime, action: u32, arg: f64) {
    let forward = m.forward_vel;
    let vy = m.vel[1];
    m.set_action(action, arg);
    // set_action seeds SM64's own launch velocities; BK moves set theirs explicitly.
    m.forward_vel = forward;
    m.vel[1] = vy;
}

/// Steer toward the stick the way baphysics' airborne mode chases a target speed.
pub(crate) fn air_control(m: &mut MarioRuntime, max: f64, rate: f64) {
    let target = if m.intended_mag > 0.0 { max.min(m.intended_mag / 32.0 * max) } else { 0.0 };
    if m.intended_mag > 0.0 {
        let turn = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
        m.face_angle[1] = to_s16(m.face_angle[1] + clamp(turn, -0x800 as f64, 0x800 as f64)) as f64;
    }
    m.forward_vel = approach_f32(m.forward_vel, target, rate, rate);
    m.vel[0] = sins(m.face_angle[1]) * m.forward_vel;
    m.vel[2] = coss(m.face_angle[1]) * m.forward_vel;
}

/// One airborne frame with BK gravity; SM64's apply_gravity after the step is undone.
pub(crate) fn air_step(m: &mut MarioRuntime, gravity: f64, terminal: f64) -> bool {
    m.vel[1] = terminal.max(m.vel[1] - gravity);
    let vy = m.vel[1];
    let hit = m.step_air(AIR_STEP_CHECK_LEDGE_GRAB);
    if hit != AIR_STEP_LANDED as i32 { m.vel[1] = vy; }
    if hit == AIR_STEP_LANDED as i32 {
        let fv = m.forward_vel;
        m.land_event(vy, fv);
        m.set_action(if m.action_arg == BANJO_JUMP { ACT_JUMP_LAND } else { ACT_FREEFALL_LAND }, 0.0);
        return true;
    }
    if hit == AIR_STEP_GRABBED_LEDGE as i32 {
        m.set_action(ACT_LEDGE_GRAB, 0.0);
    } else if hit == AIR_STEP_HIT_LAVA_WALL as i32 {
        m.set_action(ACT_LAVA_BOOST, 0.0);
    }
    hit == AIR_STEP_GRABBED_LEDGE as i32 || hit == AIR_STEP_HIT_LAVA_WALL as i32
}

/// One grounded frame of a BK move at `forward` speed. Returns false once the move left the ground or hit a wall.
pub(crate) fn ground_step(m: &mut MarioRuntime, forward: f64) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 {
        m.set_action(ACT_FREEFALL, 0.0);
        return false;
    }
    mario_set_forward_vel(m, forward);
    let hit = m.step_ground();
    if hit == GROUND_STEP_LEFT_GROUND as i32 {
        m.set_action(ACT_FREEFALL, 0.0);
        return false;
    }
    if hit == GROUND_STEP_HIT_WALL as i32 && forward > 8.0 {
        mario_set_forward_vel(m, 0.0);
        m.set_action(ACT_IDLE, 0.0);
        return false;
    }
    true
}

/// Banjo's moveset. Returns true when it consumed this simulation frame.
pub fn step_banjo(m: &mut MarioRuntime) -> bool {
    if m.action & ACT_FLAG_AIR == 0 { m.banjo_flap = 0.0; }
    crate::banjo_ext::tick(m);
    if m.banjo.form != crate::banjo_ext::Form::Banjo {
        crate::banjo_ext::form_step(m);
        return false;
    }
    if m.held_obj.is_some() || m.riding_shell.is_some() || m.flags & MARIO_WING_CAP != 0 { return false; }
    if let Some(consumed) = crate::banjo_ext::step(m) { return consumed; }
    let pressed = m.controller.button_pressed;

    let mv = banjo_move(m);
    if mv == BANJO_FLAP {
        return step_flap(m);
    } else if mv == BANJO_JUMP {
        if m.action == ACT_JUMP { return step_jump(m, false); }
    } else if mv == BANJO_FLIP {
        if m.action == ACT_BACKFLIP { return step_flip(m); }
    } else if mv == BANJO_CLAW {
        if m.action == ACT_PUNCHING { return step_claw(m); }
    } else if mv == BANJO_ROLL {
        if m.action == ACT_MOVE_PUNCHING { return step_roll(m); }
    } else if mv == BANJO_BARGE {
        if m.action == ACT_MOVE_PUNCHING { return step_barge(m); }
    } else if mv == BANJO_PECK
        && m.action == ACT_JUMP_KICK { return step_peck(m); }

    if m.action & ACT_FLAG_AIR != 0 {
        if !FLAP_FROM.contains(&m.action) { return false; }
        // Rat-a-tat Rap (B), Feathery Flap (A once per airtime); Z falls through to SM64's ground pound.
        if pressed & B_BUTTON != 0 && m.banjo.has(Ability::RatATatRap) {
            start(m, ACT_JUMP_KICK, BANJO_PECK);
            m.vel[1] = PECK_HOP;
            return step_peck(m);
        }
        if m.banjo_flap == 0.0 && (pressed & A_BUTTON != 0) && m.vel[1] < 12.0 && m.banjo.has(Ability::FeatheryFlap) {
            start(m, ACT_FREEFALL, BANJO_FLAP);
            m.banjo_flap = FLAP_FRAMES;
            m.vel[1] = 0.0;
            return step_flap(m);
        }
        return false;
    }

    if !on_basic_ground(m) { return false; }
    let crouched = is_crouched(m.action);
    if crouched && pressed & A_BUTTON != 0 && m.banjo.has(Ability::FlapFlip) {
        start(m, ACT_BACKFLIP, BANJO_FLIP);
        mario_set_forward_vel(m, 0.0);
        m.vel[1] = 0.0;
        return step_flip(m);
    }
    if crouched && pressed & B_BUTTON != 0 && m.banjo.has(Ability::Barge) {
        start(m, ACT_MOVE_PUNCHING, BANJO_BARGE);
        return step_barge(m);
    }
    if crouched { return false; }
    if pressed & A_BUTTON != 0 {
        if m.intended_mag > 0.0 { m.face_angle[1] = m.intended_yaw; }
        start(m, ACT_JUMP, BANJO_JUMP);
        m.vel[1] = JUMP_VELOCITY;
        m.jump_takeoff_speed = MAX_WALK.max(m.forward_vel.abs());
        let max = m.jump_takeoff_speed;
        air_control(m, max, 1.5);
        return step_jump(m, true);
    }
    if pressed & B_BUTTON != 0 {
        // func_802B6F20: fast walking rolls, otherwise Banjo swipes.
        if m.forward_vel.abs() > speed(225.0) && m.banjo.has(Ability::Roll) {
            start(m, ACT_MOVE_PUNCHING, BANJO_ROLL);
            return step_roll(m);
        }
        if m.banjo.has(Ability::ClawSwipe) {
            start(m, ACT_PUNCHING, BANJO_CLAW);
            return step_claw(m);
        }
    }
    if pressed & Z_TRIG != 0 {
        mario_set_forward_vel(m, 0.0);
        m.set_action(ACT_START_CROUCHING, 0.0);
        return false;
    }
    false
}

fn step_jump(m: &mut MarioRuntime, launched: bool) -> bool {
    let pressed = m.controller.button_pressed;
    if !launched {
        if pressed & Z_TRIG != 0 && m.banjo.has(Ability::BeakBuster) { m.set_action(ACT_GROUND_POUND, 0.0); return false; }
        if pressed & B_BUTTON != 0 && m.banjo.has(Ability::RatATatRap) {
            start(m, ACT_JUMP_KICK, BANJO_PECK);
            m.vel[1] = PECK_HOP;
            return step_peck(m);
        }
        if pressed & A_BUTTON != 0 && m.banjo_flap == 0.0 && m.vel[1] < 12.0 && m.banjo.has(Ability::FeatheryFlap) {
            start(m, ACT_FREEFALL, BANJO_FLAP);
            m.banjo_flap = FLAP_FRAMES;
            m.vel[1] = 0.0;
            return step_flap(m);
        }
        let max = m.jump_takeoff_speed;
        air_control(m, max, 1.5);
    }
    m.action_timer += 1.0;
    // bsjump_update: releasing A while rising restores full gravity.
    let held = m.controller.button_down & A_BUTTON != 0 && m.vel[1] > 0.0 && m.banjo.has(Ability::HoldAJumpHigher);
    air_step(m, if held { JUMP_HELD_GRAVITY } else { GRAVITY }, TERMINAL);
    true
}

fn step_flap(m: &mut MarioRuntime) -> bool {
    if m.controller.button_pressed & Z_TRIG != 0 {
        m.banjo_flap = -1.0;
        m.set_action(ACT_GROUND_POUND, 0.0);
        return false;
    }
    // `!(down & A) || --flap <= 0`: the counter only drops while A is held.
    let spent = if m.controller.button_down & A_BUTTON == 0 { true } else { m.banjo_flap -= 1.0; m.banjo_flap <= 0.0 };
    if spent {
        // Kazooie is spent until Banjo lands again.
        m.banjo_flap = -1.0;
        m.set_action(ACT_FREEFALL, 0.0);
        return false;
    }
    let max = MAX_WALK.max(m.forward_vel);
    air_control(m, max, 1.0);
    let t = m.action_timer;
    m.action_timer += 1.0;
    if t % FLAP_PERIOD == 0.0 { m.vel[1] = FLAP_IMPULSE + FLAP_GRAVITY; }
    air_step(m, FLAP_GRAVITY, FLAP_TERMINAL);
    true
}

fn step_flip(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    if m.action_timer <= FLIP_WINDUP {
        // Banjo crouches while Kazooie braces; the launch happens at anim 0.1837.
        if m.action_timer == FLIP_WINDUP {
            if m.intended_mag > 0.0 { m.face_angle[1] = m.intended_yaw; }
            m.forward_vel = if m.intended_mag > 0.0 { FLIP_FORWARD } else { 0.0 };
            m.vel[1] = FLIP_VELOCITY + FLIP_GRAVITY;
            m.vel[0] = sins(m.face_angle[1]) * m.forward_vel;
            m.vel[2] = coss(m.face_angle[1]) * m.forward_vel;
        } else {
            m.step_ground();
            return true;
        }
    } else {
        if m.controller.button_pressed & Z_TRIG != 0 { m.set_action(ACT_GROUND_POUND, 0.0); return false; }
        let max = FLIP_FORWARD.max(m.forward_vel);
        air_control(m, max, 0.5);
    }
    // Holding A keeps the slow fall until release (bsbflip case 2).
    let terminal = if m.controller.button_down & A_BUTTON != 0 { FLIP_TERMINAL } else { TERMINAL };
    air_step(m, FLIP_GRAVITY, terminal);
    true
}

fn step_claw(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer / CLAW_FRAMES;
    m.action_timer += 1.0;
    if m.controller.button_pressed & A_BUTTON != 0 {
        start(m, ACT_JUMP, BANJO_JUMP);
        m.vel[1] = JUMP_VELOCITY;
        m.jump_takeoff_speed = MAX_WALK;
        return step_jump(m, true);
    }
    // Three swipes: 0.05-0.25, 0.23-0.43 and 0.48-0.68 of the animation.
    if (t > 0.0488 && t < 0.2488) || (t > 0.2288 && t < 0.4288) || (t > 0.4788 && t < 0.6788) {
        if m.mario_check_object_grab() { return true; }
        m.flags |= MARIO_PUNCHING;
    }
    if m.intended_mag > 0.0 && t < 0.5788 {
        let turn = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
        m.face_angle[1] = to_s16(m.face_angle[1] + clamp(turn, -0x400 as f64, 0x400 as f64)) as f64;
    }
    let target = if t < 0.5788 { CLAW_LUNGE } else { 0.0 };
    let fwd = approach_f32(m.forward_vel, target, 1.5, 1.5);
    if !ground_step(m, fwd) { return true; }
    if m.action_timer >= CLAW_FRAMES { m.set_action(ACT_IDLE, 0.0); }
    true
}

fn step_roll(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer / ROLL_FRAMES;
    m.action_timer += 1.0;
    if m.controller.button_pressed & A_BUTTON != 0 {
        start(m, ACT_JUMP, BANJO_JUMP);
        m.vel[1] = JUMP_VELOCITY;
        m.jump_takeoff_speed = MAX_WALK.max(m.forward_vel);
        return step_jump(m, true);
    }
    let rolling = t < 0.8011;
    if rolling { m.flags |= MARIO_TRIPPING; }
    if m.intended_mag > 0.0 {
        let turn = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
        m.face_angle[1] = to_s16(m.face_angle[1] + clamp(turn, -0x300 as f64, 0x300 as f64)) as f64;
    }
    let fwd = approach_f32(m.forward_vel, if rolling { ROLL_SPEED } else { 0.0 }, 2.0, if rolling { 2.0 } else { 3.0 });
    if !ground_step(m, fwd) { return true; }
    if m.action_timer >= ROLL_FRAMES { m.set_action(if m.forward_vel > 1.0 { ACT_WALKING } else { ACT_IDLE }, 0.0); }
    true
}

fn step_barge(m: &mut MarioRuntime) -> bool {
    let timer = m.action_timer;
    m.action_timer += 1.0;
    // action_state: 0 wind-up, 1 charging (B held for the full 850), 2 sliding to a stop.
    if m.controller.button_down & B_BUTTON == 0 && m.action_state == 0.0 { m.action_state = -1.0; }
    let mut forward = m.forward_vel;
    if timer < BARGE_WINDUP {
        forward = approach_f32(forward, 0.0, 3.0, 3.0);
    } else if timer == BARGE_WINDUP {
        if m.intended_mag > 0.0 { m.face_angle[1] = m.intended_yaw; }
        forward = if m.action_state == -1.0 { BARGE_SHORT } else { BARGE_FULL };
        m.action_state = 1.0;
    } else if forward > BARGE_END {
        forward = BARGE_END.max(forward - if timer > BARGE_WINDUP + 3.0 { BARGE_DECAY } else { 0.0 });
    } else {
        forward = approach_f32(forward, 0.0, 1.0, 1.0);
    }
    if timer >= BARGE_WINDUP && forward > BARGE_END { m.flags |= MARIO_PUNCHING; }
    if !ground_step(m, forward) { return true; }
    if m.action_timer >= BARGE_FRAMES { m.set_action(ACT_IDLE, 0.0); }
    true
}

fn step_peck(m: &mut MarioRuntime) -> bool {
    let timer = m.action_timer;
    m.action_timer += 1.0;
    // Two pecks per 0.35 s loop of BSBPECK_ENTER, each with a little hop.
    if timer < PECK_FRAMES {
        m.flags |= MARIO_KICKING;
        let lp = (timer % PECK_LOOP) / PECK_LOOP;
        if timer > 0.0 && ((lp - 0.1621).abs() < 0.05 || (lp - 0.7).abs() < 0.05) { m.vel[1] = PECK_HOP + PECK_GRAVITY; }
    }
    let max = (m.forward_vel * 0.1).max(1.0);
    air_control(m, max, 2.0);
    if timer >= PECK_END {
        m.set_action(ACT_FREEFALL, 0.0);
        return false;
    }
    air_step(m, PECK_GRAVITY, TERMINAL);
    true
}
