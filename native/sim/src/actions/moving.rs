//! Port of `src/sim/actions/moving.ts` (walking, braking, sliding, landings, hold-walking, riding shell on the ground).
use crate::mario_helpers::*;
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::{GROUND_STEP_HIT_WALL_I, GROUND_STEP_LEFT_GROUND_I, GROUND_STEP_NONE_I};
use crate::object::INT_SUBTYPE_HOLDABLE_NPC;
use crate::surfaces::*;
use crate::{math::*, state::*};
use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// apply_slope_accel
pub fn apply_slope_accel(m: &mut MarioRuntime) {
    let Some(floor) = m.floor.clone() else { return };

    let steepness = (floor.normal[0] * floor.normal[0] + floor.normal[2] * floor.normal[2]).sqrt();
    let floor_d_yaw = to_s16(m.floor_angle - m.face_angle[1]);

    if mario_floor_is_slope(m) {
        let slope_accel = match m.floor_class() {
            SURFACE_CLASS_VERY_SLIPPERY => 5.3,
            SURFACE_CLASS_SLIPPERY => 2.7,
            SURFACE_CLASS_NOT_SLIPPERY => 0.0,
            _ => 1.7,
        };

        if floor_d_yaw > -0x4000 && floor_d_yaw < 0x4000 {
            m.forward_vel += slope_accel * steepness;
        } else {
            m.forward_vel -= slope_accel * steepness;
        }
    }

    m.slide_yaw = m.face_angle[1];
    m.slide_vel_x = m.forward_vel * js_sin(to_s16(m.face_angle[1]) as f64 / 32768.0 * PI);
    m.slide_vel_z = m.forward_vel * js_cos(to_s16(m.face_angle[1]) as f64 / 32768.0 * PI);
    m.vel[0] = m.slide_vel_x;
    m.vel[1] = 0.0;
    m.vel[2] = m.slide_vel_z;
}

/// apply_landing_accel
pub fn apply_landing_accel(m: &mut MarioRuntime, friction_factor: f64) -> bool {
    apply_slope_accel(m);

    let mut stopped = false;
    if !mario_floor_is_slope(m) {
        m.forward_vel *= friction_factor;
        if m.forward_vel * m.forward_vel < 1.0 {
            mario_set_forward_vel(m, 0.0);
            stopped = true;
        }
    }
    stopped
}

/// apply_slope_decel
pub fn apply_slope_decel(m: &mut MarioRuntime, decel_coef: f64) -> bool {
    let decel = match m.floor_class() {
        SURFACE_CLASS_VERY_SLIPPERY => decel_coef * 0.2,
        SURFACE_CLASS_SLIPPERY => decel_coef * 0.7,
        SURFACE_CLASS_NOT_SLIPPERY => decel_coef * 3.0,
        _ => decel_coef * 2.0,
    };

    let mut stopped = false;
    m.forward_vel = approach_f32(m.forward_vel, 0.0, decel, decel);
    if m.forward_vel == 0.0 { stopped = true; }

    apply_slope_accel(m);
    stopped
}

/// update_decelerating_speed
pub fn update_decelerating_speed(m: &mut MarioRuntime) -> bool {
    let mut stopped = false;
    m.forward_vel = approach_f32(m.forward_vel, 0.0, 1.0, 1.0);
    if m.forward_vel == 0.0 { stopped = true; }
    let v = m.forward_vel;
    mario_set_forward_vel(m, v);
    stopped
}

/// update_walking_speed
pub fn update_walking_speed(m: &mut MarioRuntime) {
    let max_target_speed = if m.floor.as_ref().is_some_and(|f| f.kind == SURFACE_SLOW) { 24.0 } else { 32.0 };
    let target_speed = m.intended_mag.min(max_target_speed);

    if m.forward_vel <= 0.0 {
        m.forward_vel += 1.1;
    } else if m.forward_vel <= target_speed {
        m.forward_vel += 1.1 - m.forward_vel / 43.0;
    } else if m.floor.as_ref().is_some_and(|f| f.normal[1] >= 0.95) {
        m.forward_vel -= 1.0;
    }

    if m.forward_vel > 48.0 { m.forward_vel = 48.0; }

    m.face_angle[1] = to_s16(
        m.intended_yaw - approach_s32(to_s16(m.intended_yaw - m.face_angle[1]) as f64, 0.0, 0x800 as f64, 0x800 as f64),
    ) as f64;
    apply_slope_accel(m);
}

/// should_begin_sliding
pub fn should_begin_sliding(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_ABOVE_SLIDE != 0 {
        let moving_backward = m.forward_vel <= -1.0;
        if m.slide_terrain || moving_backward || mario_facing_downhill(m, false) {
            return true;
        }
    }
    false
}

/// analog_stick_held_back
pub fn analog_stick_held_back(m: &MarioRuntime) -> bool {
    let intended_d_yaw = to_s16(m.intended_yaw - m.face_angle[1]);
    intended_d_yaw < -0x471c || intended_d_yaw > 0x471c
}

/// check_ground_dive_or_punch
pub fn check_ground_dive_or_punch(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 {
        if m.forward_vel >= 29.0 && m.controller.stick_mag > 48.0 {
            m.vel[1] = 20.0;
            return m.set_action(ACT_DIVE, 1.0);
        }
        return m.set_action(ACT_MOVE_PUNCHING, 0.0);
    }
    false
}

/// begin_braking_action
pub fn begin_braking_action(m: &mut MarioRuntime) -> bool {
    if m.forward_vel >= 16.0 && m.floor.as_ref().is_some_and(|f| f.normal[1] >= 0.17364818) {
        return m.set_action(ACT_BRAKING, 0.0);
    }
    m.set_action(ACT_DECELERATING, 0.0)
}

/// begin_walking_action
pub fn begin_walking_action(m: &mut MarioRuntime, forward_vel: f64, action: u32) -> bool {
    m.face_angle[1] = m.intended_yaw;
    mario_set_forward_vel(m, forward_vel);
    m.set_action(action, 0.0)
}

/// slide_bonk
pub fn slide_bonk(m: &mut MarioRuntime, slow_action: u32) {
    if m.forward_vel > 16.0 {
        m.bonk_reflection(true);
        // ACT_GROUND_BONK is a knockback action; decelerating is the closest
        // non-damage equivalent in this port.
        m.set_action(ACT_DECELERATING, 0.0);
    } else {
        mario_set_forward_vel(m, 0.0);
        m.set_action(slow_action, 0.0);
    }
}

/// set_triple_jump_action
pub fn set_triple_jump_action(m: &mut MarioRuntime) -> bool {
    if m.forward_vel > 20.0 {
        return m.set_action(ACT_TRIPLE_JUMP, 0.0);
    }
    m.set_action(ACT_JUMP, 0.0)
}

pub fn sin(angle: f64) -> f64 { js_sin(to_s16(angle) as f64 / 32768.0 * PI) }
pub fn cos(angle: f64) -> f64 { js_cos(to_s16(angle) as f64 / 32768.0 * PI) }

// ---------------------------------------------------------------------------
// Sliding
// ---------------------------------------------------------------------------

/// update_sliding_angle
pub fn update_sliding_angle(m: &mut MarioRuntime, accel: f64, loss_factor: f64) {
    let Some(floor) = m.floor.clone() else { return };

    let slope_angle = atan2s(floor.normal[2], floor.normal[0]) as f64;
    let steepness = (floor.normal[0] * floor.normal[0] + floor.normal[2] * floor.normal[2]).sqrt();

    m.slide_vel_x += accel * steepness * sin(slope_angle);
    m.slide_vel_z += accel * steepness * cos(slope_angle);
    m.slide_vel_x *= loss_factor;
    m.slide_vel_z *= loss_factor;

    m.slide_yaw = atan2s(m.slide_vel_z, m.slide_vel_x) as f64;

    let facing_d_yaw = to_s16(m.face_angle[1] - m.slide_yaw) as f64;
    let mut new_facing_d_yaw = facing_d_yaw;

    if new_facing_d_yaw > 0.0 && new_facing_d_yaw <= 16384.0 {
        new_facing_d_yaw -= 512.0;
        if new_facing_d_yaw < 0.0 { new_facing_d_yaw = 0.0; }
    } else if new_facing_d_yaw > -16384.0 && new_facing_d_yaw < 0.0 {
        new_facing_d_yaw += 512.0;
        if new_facing_d_yaw > 0.0 { new_facing_d_yaw = 0.0; }
    } else if new_facing_d_yaw > 16384.0 && new_facing_d_yaw < 32768.0 {
        new_facing_d_yaw += 512.0;
        if new_facing_d_yaw > 32768.0 { new_facing_d_yaw = 32768.0; }
    } else if new_facing_d_yaw > -32768.0 && new_facing_d_yaw < -16384.0 {
        new_facing_d_yaw -= 512.0;
        if new_facing_d_yaw < -32768.0 { new_facing_d_yaw = -32768.0; }
    }

    m.face_angle[1] = to_s16(m.slide_yaw + new_facing_d_yaw) as f64;

    m.vel[0] = m.slide_vel_x;
    m.vel[1] = 0.0;
    m.vel[2] = m.slide_vel_z;

    m.forward_vel = (m.slide_vel_x * m.slide_vel_x + m.slide_vel_z * m.slide_vel_z).sqrt();
    if m.forward_vel > 100.0 {
        m.slide_vel_x = (m.slide_vel_x * 100.0) / m.forward_vel;
        m.slide_vel_z = (m.slide_vel_z * 100.0) / m.forward_vel;
    }

    if new_facing_d_yaw < -16384.0 || new_facing_d_yaw > 16384.0 {
        m.forward_vel *= -1.0;
    }
}

/// update_sliding
pub fn update_sliding(m: &mut MarioRuntime, stop_speed: f64) -> bool {
    let intended_d_yaw = to_s16(m.intended_yaw - m.slide_yaw) as f64;
    let mut forward = cos(intended_d_yaw);
    let sideward = sin(intended_d_yaw);

    if forward < 0.0 && m.forward_vel >= 0.0 {
        forward *= 0.5 + (0.5 * m.forward_vel) / 100.0;
    }

    let (accel, loss_factor) = match m.floor_class() {
        SURFACE_CLASS_VERY_SLIPPERY => (10.0, (m.intended_mag / 32.0) * forward * 0.02 + 0.98),
        SURFACE_CLASS_SLIPPERY => (8.0, (m.intended_mag / 32.0) * forward * 0.02 + 0.96),
        SURFACE_CLASS_NOT_SLIPPERY => (5.0, (m.intended_mag / 32.0) * forward * 0.02 + 0.92),
        _ => (7.0, (m.intended_mag / 32.0) * forward * 0.02 + 0.92),
    };

    let old_speed = (m.slide_vel_x * m.slide_vel_x + m.slide_vel_z * m.slide_vel_z).sqrt();

    m.slide_vel_x += m.slide_vel_z * (m.intended_mag / 32.0) * sideward * 0.05;
    m.slide_vel_z -= m.slide_vel_x * (m.intended_mag / 32.0) * sideward * 0.05;

    let new_speed = (m.slide_vel_x * m.slide_vel_x + m.slide_vel_z * m.slide_vel_z).sqrt();
    if old_speed > 0.0 && new_speed > 0.0 {
        m.slide_vel_x = (m.slide_vel_x * old_speed) / new_speed;
        m.slide_vel_z = (m.slide_vel_z * old_speed) / new_speed;
    }

    update_sliding_angle(m, accel, loss_factor);

    let mut stopped = false;
    if !mario_floor_is_slope(m) && m.forward_vel * m.forward_vel < stop_speed * stop_speed {
        mario_set_forward_vel(m, 0.0);
        stopped = true;
    }
    stopped
}

/// common_slide_action
pub fn common_slide_action(m: &mut MarioRuntime, end_action: u32, air_action: u32) {
    let step = m.step_ground();

    match step {
        GROUND_STEP_LEFT_GROUND_I => {
            m.set_action(air_action, 0.0);
        }
        GROUND_STEP_NONE_I => {
            if m.floor.is_some() { m.pos[1] = m.floor_height; }
        }
        GROUND_STEP_HIT_WALL_I => {
            if !mario_floor_is_slippery(m) {
                slide_bonk(m, end_action);
            } else if let Some(wall) = m.wall.clone() {
                let wall_angle = atan2s(wall.normal[2], wall.normal[0]) as f64;
                let mut slide_speed = (m.slide_vel_x * m.slide_vel_x + m.slide_vel_z * m.slide_vel_z).sqrt();
                slide_speed *= 0.9;
                if slide_speed < 4.0 { slide_speed = 4.0; }

                m.slide_yaw = to_s16(wall_angle - to_s16(m.slide_yaw - wall_angle) as f64 + 32768.0) as f64;
                m.slide_vel_x = slide_speed * sin(m.slide_yaw);
                m.vel[0] = m.slide_vel_x;
                m.slide_vel_z = slide_speed * cos(m.slide_yaw);
                m.vel[2] = m.slide_vel_z;
            }
            if m.floor.is_some() { m.pos[1] = m.floor_height; }
        }
        _ => {}
    }
}

/// common_slide_action_with_jump
pub fn common_slide_action_with_jump(m: &mut MarioRuntime, stop_action: u32, jump_action: u32, air_action: u32) -> bool {
    if m.action_timer == 5.0 {
        if m.input & INPUT_A_PRESSED != 0 {
            return m.set_jumping_action(jump_action, 0.0);
        }
    } else {
        m.action_timer += 1.0;
    }

    if update_sliding(m, 4.0) {
        return m.set_action(stop_action, 0.0);
    }

    common_slide_action(m, stop_action, air_action);
    false
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

pub fn act_walking(m: &mut MarioRuntime) -> bool {
    if should_begin_sliding(m) { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }

    if m.input & INPUT_A_PRESSED != 0 { return m.set_jump_from_landing(); }
    if check_ground_dive_or_punch(m) { return true; }
    if m.input & INPUT_UNKNOWN_5 != 0 { return begin_braking_action(m); }
    if analog_stick_held_back(m) && m.forward_vel >= 16.0 {
        return m.set_action(ACT_TURNING_AROUND, 0.0);
    }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_CROUCH_SLIDE, 0.0); }

    m.action_state = 0.0;
    update_walking_speed(m);

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => {
            m.set_action(ACT_FREEFALL, 0.0);
        }
        GROUND_STEP_NONE_I => {
            if m.intended_mag - m.forward_vel > 16.0 { m.particle_flags |= 1; }
        }
        GROUND_STEP_HIT_WALL_I => {
            if m.forward_vel > 6.0 { mario_set_forward_vel(m, 6.0); }
            m.action_timer = 0.0;
        }
        _ => {}
    }

    false
}

pub fn act_move_punching(m: &mut MarioRuntime) -> bool {
    if should_begin_sliding(m) { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }
    if m.action_state == 0.0 && (m.input & INPUT_A_DOWN != 0) {
        return m.set_action(ACT_JUMP_KICK, 0.0);
    }

    m.action_state = 1.0;
    if super::object::update_punch_sequence(m, ACT_WALKING, ACT_CROUCH_SLIDE) { return true; }

    if m.forward_vel >= 0.0 {
        apply_slope_decel(m, 0.5);
    } else {
        m.forward_vel += 8.0;
        if m.forward_vel >= 0.0 { m.forward_vel = 0.0; }
        apply_slope_accel(m);
    }

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => {
            m.set_action(ACT_FREEFALL, 0.0);
        }
        GROUND_STEP_NONE_I => m.particle_flags |= 1,
        _ => {}
    }

    false
}

pub fn act_turning_around(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_SIDE_FLIP, 0.0); }
    if m.input & INPUT_UNKNOWN_5 != 0 { return m.set_action(ACT_BRAKING, 0.0); }
    if !analog_stick_held_back(m) { return m.set_action(ACT_WALKING, 0.0); }

    if apply_slope_decel(m, 2.0) {
        return begin_walking_action(m, 8.0, ACT_FINISH_TURNING_AROUND);
    }

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => {
            m.set_action(ACT_FREEFALL, 0.0);
        }
        GROUND_STEP_NONE_I => m.particle_flags |= 1,
        _ => {}
    }

    if m.forward_vel >= 18.0 {
        m.anim_length = 12.0;
    } else {
        m.anim_length = 18.0;
        m.action_timer += 1.0;
        if m.action_timer > 18.0 {
            if m.forward_vel > 0.0 {
                let v = -m.forward_vel;
                begin_walking_action(m, v, ACT_WALKING);
            } else {
                begin_walking_action(m, 8.0, ACT_WALKING);
            }
        }
    }

    false
}

pub fn act_finish_turning_around(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_SIDE_FLIP, 0.0); }

    update_walking_speed(m);

    if m.step_ground() == GROUND_STEP_LEFT_GROUND_I {
        m.set_action(ACT_FREEFALL, 0.0);
    }

    m.action_timer += 1.0;
    if m.action_timer >= 12.0 {
        return m.set_action(ACT_WALKING, 0.0);
    }
    false
}

pub fn act_braking(m: &mut MarioRuntime) -> bool {
    if m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE) != 0 {
        return m.check_common_action_exits();
    }

    if apply_slope_decel(m, 2.0) {
        return m.set_action(ACT_BRAKING_STOP, 0.0);
    }

    if m.input & INPUT_B_PRESSED != 0 {
        return m.set_action(ACT_MOVE_PUNCHING, 0.0);
    }

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => {
            m.set_action(ACT_FREEFALL, 0.0);
        }
        GROUND_STEP_NONE_I => m.particle_flags |= 1,
        GROUND_STEP_HIT_WALL_I => slide_bonk(m, ACT_BRAKING_STOP),
        _ => {}
    }

    m.anim_length = 20.0;
    false
}

pub fn act_decelerating(m: &mut MarioRuntime) -> bool {
    let slope_class = m.floor_class();

    if should_begin_sliding(m) { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jump_from_landing(); }
    if check_ground_dive_or_punch(m) { return true; }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_WALKING, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_CROUCH_SLIDE, 0.0); }

    if update_decelerating_speed(m) {
        return m.set_action(ACT_IDLE, 0.0);
    }

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => {
            m.set_action(ACT_FREEFALL, 0.0);
        }
        GROUND_STEP_HIT_WALL_I => {
            if slope_class == SURFACE_CLASS_VERY_SLIPPERY {
                m.bonk_reflection(true);
            } else {
                mario_set_forward_vel(m, 0.0);
            }
        }
        _ => {}
    }

    if slope_class == SURFACE_CLASS_VERY_SLIPPERY {
        m.particle_flags |= 1;
    }
    m.anim_length = 30.0;
    false
}

pub fn act_crawling(m: &mut MarioRuntime) -> bool {
    if should_begin_sliding(m) { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_JUMP, 0.0); }
    if check_ground_dive_or_punch(m) { return true; }
    if m.input & INPUT_UNKNOWN_5 != 0 { return m.set_action(ACT_STOP_CRAWLING, 0.0); }
    if m.input & INPUT_Z_DOWN == 0 { return m.set_action(ACT_STOP_CRAWLING, 0.0); }

    m.intended_mag *= 0.1;
    update_walking_speed(m);

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => {
            m.set_action(ACT_FREEFALL, 0.0);
        }
        GROUND_STEP_HIT_WALL_I => {
            if m.forward_vel > 10.0 { mario_set_forward_vel(m, 10.0); }
        }
        _ => {}
    }

    m.anim_length = 20.0;
    false
}

pub fn act_crouch_slide(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BUTT_SLIDE, 0.0); }

    if m.action_timer < 30.0 {
        m.action_timer += 1.0;
        if m.input & INPUT_A_PRESSED != 0 && m.forward_vel > 10.0 {
            return m.set_jumping_action(ACT_LONG_JUMP, 0.0);
        }
    }

    if m.input & INPUT_B_PRESSED != 0 {
        if m.forward_vel >= 10.0 {
            return m.set_action(ACT_SLIDE_KICK, 0.0);
        }
        return m.set_action(ACT_MOVE_PUNCHING, 0x0009 as f64);
    }

    if m.input & INPUT_A_PRESSED != 0 {
        return m.set_jumping_action(ACT_JUMP, 0.0);
    }

    common_slide_action_with_jump(m, ACT_CROUCHING, ACT_JUMP, ACT_FREEFALL)
}

pub fn act_butt_slide(m: &mut MarioRuntime) -> bool {
    common_slide_action_with_jump(m, ACT_BUTT_SLIDE_STOP, ACT_JUMP, ACT_BUTT_SLIDE_AIR)
}

pub fn act_stomach_slide(m: &mut MarioRuntime) -> bool {
    if m.action_timer == 5.0 {
        if m.input & INPUT_ABOVE_SLIDE == 0 && (m.input & (INPUT_A_PRESSED | INPUT_B_PRESSED) != 0) {
            // Forward rollouts are not ported; popping into a crouch keeps the
            // player in control without inventing new physics.
            return m.set_action(ACT_CROUCHING, 0.0);
        }
    } else {
        m.action_timer += 1.0;
    }

    let held = m.action == ACT_HOLD_STOMACH_SLIDE;
    let stop = if held { ACT_DIVE_PICKING_UP } else { ACT_CROUCHING };
    if update_sliding(m, 4.0) { return m.set_action(stop, 0.0); }
    common_slide_action(m, stop, if held { ACT_HOLD_FREEFALL } else { ACT_FREEFALL });
    false
}

pub fn act_dive_slide(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_ABOVE_SLIDE == 0 && (m.input & (INPUT_A_PRESSED | INPUT_B_PRESSED) != 0) {
        return m.set_action(ACT_CROUCHING, 0.0);
    }

    if update_sliding(m, 8.0) {
        m.action_timer += 1.0;
        if m.action_timer > 8.0 {
            mario_set_forward_vel(m, 0.0);
            return m.set_action(ACT_CROUCHING, 0.0);
        }
    }

    common_slide_action(m, ACT_CROUCHING, ACT_FREEFALL);
    false
}

pub fn act_slide_kick_slide(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_A_PRESSED != 0 {
        return m.set_jumping_action(ACT_FREEFALL, 2.0);
    }

    m.action_timer += 1.0;
    if m.action_timer > 12.0 && m.forward_vel < 1.0 {
        return m.set_action(ACT_SLIDE_KICK_SLIDE_STOP, 0.0);
    }

    update_sliding(m, 1.0);

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => {
            m.set_action(ACT_FREEFALL, 2.0);
        }
        GROUND_STEP_HIT_WALL_I => {
            m.bonk_reflection(true);
            m.particle_flags |= 2;
            m.set_action(ACT_DECELERATING, 0.0);
        }
        _ => {}
    }

    m.particle_flags |= 1;
    false
}

// ---------------------------------------------------------------------------
// Landing actions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct LandingAction {
    pub num_frames: f64,
    pub very_steep_action: u32,
    pub end_action: u32,
    pub off_floor_action: u32,
    pub slide_action: u32,
    pub a_pressed_action: u32,
    pub a_press: fn(&mut MarioRuntime) -> bool,
}

fn jump_land_action() -> LandingAction {
    LandingAction {
        num_frames: 4.0,
        very_steep_action: ACT_FREEFALL,
        end_action: ACT_JUMP_LAND_STOP,
        off_floor_action: ACT_FREEFALL,
        slide_action: ACT_BEGIN_SLIDING,
        a_pressed_action: ACT_DOUBLE_JUMP,
        a_press: |m| m.set_jumping_action(ACT_DOUBLE_JUMP, 0.0),
    }
}

fn freefall_land_action() -> LandingAction { LandingAction { end_action: ACT_FREEFALL_LAND_STOP, ..jump_land_action() } }

fn side_flip_land_action() -> LandingAction { LandingAction { end_action: ACT_SIDE_FLIP_LAND_STOP, ..jump_land_action() } }

fn double_jump_land_action() -> LandingAction {
    LandingAction {
        num_frames: 4.0,
        very_steep_action: ACT_FREEFALL,
        end_action: ACT_DOUBLE_JUMP_LAND_STOP,
        off_floor_action: ACT_FREEFALL,
        slide_action: ACT_BEGIN_SLIDING,
        a_pressed_action: ACT_JUMP,
        a_press: set_triple_jump_action,
    }
}

fn triple_jump_land_action() -> LandingAction {
    LandingAction {
        end_action: ACT_TRIPLE_JUMP_LAND_STOP,
        a_pressed_action: 0,
        a_press: |m| m.set_jumping_action(0, 0.0),
        ..double_jump_land_action()
    }
}

fn long_jump_land_action() -> LandingAction {
    LandingAction {
        num_frames: 6.0,
        end_action: ACT_LONG_JUMP_LAND_STOP,
        a_pressed_action: ACT_JUMP,
        a_press: |m| m.set_jumping_action(ACT_JUMP, 0.0),
        ..jump_land_action()
    }
}

fn backflip_land_action() -> LandingAction {
    LandingAction {
        end_action: ACT_BACKFLIP_LAND_STOP,
        a_pressed_action: ACT_BACKFLIP,
        a_press: |m| m.set_jumping_action(ACT_BACKFLIP, 0.0),
        ..jump_land_action()
    }
}

/// common_landing_cancels
pub fn common_landing_cancels(m: &mut MarioRuntime, landing: &LandingAction) -> bool {
    if m.floor.as_ref().is_some_and(|f| f.normal[1] < 0.2923717) {
        return m.push_off_steep_floor(landing.very_steep_action, 0.0);
    }

    m.double_jump_timer = 5.0;

    if should_begin_sliding(m) {
        return m.set_action(landing.slide_action, 0.0);
    }

    m.action_timer += 1.0;
    if m.action_timer >= landing.num_frames {
        return m.set_action(landing.end_action, 0.0);
    }

    if m.input & INPUT_A_PRESSED != 0 {
        return (landing.a_press)(m);
    }

    if m.input & INPUT_OFF_FLOOR != 0 {
        return m.set_action(landing.off_floor_action, 0.0);
    }

    false
}

/// common_landing_action
pub fn common_landing_action(m: &mut MarioRuntime, air_action: u32) {
    if m.input & INPUT_NONZERO_ANALOG != 0 {
        apply_landing_accel(m, 0.98);
    } else if m.forward_vel >= 16.0 {
        apply_slope_decel(m, 2.0);
    } else {
        m.vel[1] = 0.0;
    }

    let step_result = m.step_ground();

    if step_result == GROUND_STEP_LEFT_GROUND_I {
        m.set_action(air_action, 0.0);
    }

    if m.forward_vel > 16.0 { m.particle_flags |= 1; }
}

pub fn act_jump_land(m: &mut MarioRuntime) -> bool {
    if common_landing_cancels(m, &jump_land_action()) { return true; }
    common_landing_action(m, ACT_FREEFALL);
    false
}

pub fn act_freefall_land(m: &mut MarioRuntime) -> bool {
    if common_landing_cancels(m, &freefall_land_action()) { return true; }
    common_landing_action(m, ACT_FREEFALL);
    false
}

pub fn act_side_flip_land(m: &mut MarioRuntime) -> bool {
    if common_landing_cancels(m, &side_flip_land_action()) { return true; }
    common_landing_action(m, ACT_FREEFALL);
    false
}

pub fn act_double_jump_land(m: &mut MarioRuntime) -> bool {
    if common_landing_cancels(m, &double_jump_land_action()) { return true; }
    common_landing_action(m, ACT_FREEFALL);
    false
}

pub fn act_triple_jump_land(m: &mut MarioRuntime) -> bool {
    m.input &= !INPUT_A_PRESSED;
    if common_landing_cancels(m, &triple_jump_land_action()) { return true; }
    common_landing_action(m, ACT_FREEFALL);
    false
}

pub fn act_long_jump_land(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_Z_DOWN == 0 {
        m.input &= !INPUT_A_PRESSED;
    }
    if common_landing_cancels(m, &long_jump_land_action()) { return true; }
    common_landing_action(m, ACT_FREEFALL);
    false
}

pub fn act_backflip_land(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_Z_DOWN == 0 {
        m.input &= !INPUT_A_PRESSED;
    }
    if common_landing_cancels(m, &backflip_land_action()) { return true; }
    common_landing_action(m, ACT_FREEFALL);
    false
}

pub fn hold_throw_action(m: &mut MarioRuntime) -> bool {
    let npc = m.held_obj.is_some_and(|id| m.objects[id].interact_subtype & INT_SUBTYPE_HOLDABLE_NPC != 0);
    m.set_action(if npc { ACT_PLACING_DOWN } else { ACT_THROWING }, 0.0)
}

pub fn act_hold_walking(m: &mut MarioRuntime) -> bool {
    if m.held_obj.is_none() { return m.set_action(ACT_WALKING, 0.0); }
    if should_begin_sliding(m) { return m.set_action(ACT_HOLD_BEGIN_SLIDING, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return hold_throw_action(m); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_HOLD_JUMP, 0.0); }
    if m.input & INPUT_UNKNOWN_5 != 0 { return m.set_action(ACT_HOLD_DECELERATING, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 {
        m.drop_held_object();
        return m.set_action(ACT_CROUCH_SLIDE, 0.0);
    }

    m.intended_mag *= 0.4;
    update_walking_speed(m);

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => return m.set_action(ACT_HOLD_FREEFALL, 0.0),
        GROUND_STEP_HIT_WALL_I => {
            if m.forward_vel > 16.0 { mario_set_forward_vel(m, 16.0); }
        }
        _ => {}
    }
    false
}

pub fn act_hold_decelerating(m: &mut MarioRuntime) -> bool {
    if m.held_obj.is_none() { return m.set_action(ACT_DECELERATING, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_HOLD_JUMP, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return hold_throw_action(m); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_HOLD_WALKING, 0.0); }
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_HOLD_FREEFALL, 0.0); }
    if should_begin_sliding(m) { return m.set_action(ACT_HOLD_BEGIN_SLIDING, 0.0); }

    let stopped = apply_landing_accel(m, 0.95);
    if m.step_ground() == GROUND_STEP_LEFT_GROUND_I {
        return m.set_action(ACT_HOLD_FREEFALL, 0.0);
    }
    if stopped { return m.set_action(ACT_HOLD_IDLE, 0.0); }
    false
}

pub fn hold_landing(m: &mut MarioRuntime, fallback: u32) -> bool {
    if m.held_obj.is_none() { return m.set_action(fallback, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_HOLD_JUMP, 0.0); }
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_HOLD_FREEFALL, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_HOLD_WALKING, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return hold_throw_action(m); }

    apply_landing_accel(m, 0.9);
    if m.step_ground() == GROUND_STEP_LEFT_GROUND_I { return m.set_action(ACT_HOLD_FREEFALL, 0.0); }
    m.action_timer += 1.0;
    if m.action_timer >= 6.0 { return m.set_action(ACT_HOLD_IDLE, 0.0); }
    false
}

pub fn act_hold_jump_land(m: &mut MarioRuntime) -> bool { hold_landing(m, ACT_JUMP_LAND) }

pub fn act_hold_freefall_land(m: &mut MarioRuntime) -> bool { hold_landing(m, ACT_FREEFALL_LAND) }

pub fn ground_knockback(m: &mut MarioRuntime) -> bool {
    let soft = m.action == ACT_SOFT_BACKWARD_GROUND_KB || m.action == ACT_SOFT_FORWARD_GROUND_KB;
    let hard = m.action == ACT_HARD_BACKWARD_GROUND_KB || m.action == ACT_HARD_FORWARD_GROUND_KB;
    let backward = m.action == ACT_SOFT_BACKWARD_GROUND_KB
        || m.action == ACT_BACKWARD_GROUND_KB
        || m.action == ACT_HARD_BACKWARD_GROUND_KB;
    let duration = if soft { 24.0 } else if hard { if backward { 70.0 } else { 30.0 } } else { 30.0 };
    m.forward_vel = (-32.0f64).max(32.0f64.min(m.forward_vel));
    if soft || m.action_timer < (if backward { 22.0 } else { 20.0 }) {
        apply_landing_accel(m, 0.9);
    } else {
        let v = if m.forward_vel < 0.0 { -0.1 } else { 0.1 };
        mario_set_forward_vel(m, v);
    }
    if m.step_ground() == GROUND_STEP_LEFT_GROUND_I {
        let arg = m.action_arg;
        if m.forward_vel >= 0.0 {
            m.set_action(if hard { ACT_HARD_FORWARD_AIR_KB } else { ACT_FORWARD_AIR_KB }, arg);
        } else {
            m.set_action(if hard { ACT_HARD_BACKWARD_AIR_KB } else { ACT_BACKWARD_AIR_KB }, arg);
        }
    } else {
        m.action_timer += 1.0;
        if m.action_timer >= duration {
            if m.action_arg > 0.0 { m.invinc_timer = 30.0; }
            m.set_action(ACT_IDLE, 0.0);
        }
    }
    false
}

/// update_shell_speed and act_riding_shell_ground.
pub fn act_riding_shell_ground(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_RIDING_SHELL_JUMP, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 {
        if m.forward_vel < 24.0 { mario_set_forward_vel(m, 24.0); }
        return m.set_action(ACT_CROUCH_SLIDE, 0.0);
    }
    let cap: f64 = if m.floor.as_ref().is_some_and(|f| f.kind == SURFACE_SLOW) { 48.0 } else { 64.0 };
    let target = 24.0f64.max(cap.min(m.intended_mag * 2.0));
    if m.forward_vel <= 0.0 {
        m.forward_vel += 1.1;
    } else if m.forward_vel <= target {
        m.forward_vel += 1.1 - m.forward_vel / 58.0;
    } else if m.floor.as_ref().is_some_and(|f| f.normal[1] >= 0.95) {
        m.forward_vel -= 1.0;
    }
    m.forward_vel = m.forward_vel.min(64.0);
    let start_yaw = m.face_angle[1];
    m.face_angle[1] = to_s16(m.intended_yaw - approach_s32(to_s16(m.intended_yaw - m.face_angle[1]) as f64, 0.0, 0x800 as f64, 0x800 as f64)) as f64;
    apply_slope_accel(m);
    let lean = -((to_s16(m.face_angle[1] - start_yaw) as f64 * m.forward_vel / 12.0).trunc());
    m.shell_roll = approach_s32(m.shell_roll, (-0x1800 as f64).max((0x1800 as f64).min(lean)), 0x200 as f64, 0x200 as f64);
    let step = m.step_ground();
    if step == GROUND_STEP_LEFT_GROUND_I { m.set_action(ACT_RIDING_SHELL_FALL, 0.0); }
    if step == GROUND_STEP_HIT_WALL_I {
        let speed = m.forward_vel;
        m.events.push(MarioEvent::Bonk { speed });
        m.set_action(ACT_BACKWARD_GROUND_KB, 0.0);
    }
    false
}

/// Action table (`MOVING_ACTIONS`).
pub fn moving_action(m: &mut MarioRuntime) -> Option<bool> {
    Some(match m.action {
        ACT_SOFT_BACKWARD_GROUND_KB | ACT_SOFT_FORWARD_GROUND_KB | ACT_BACKWARD_GROUND_KB | ACT_FORWARD_GROUND_KB
        | ACT_HARD_BACKWARD_GROUND_KB | ACT_HARD_FORWARD_GROUND_KB => ground_knockback(m),
        ACT_RIDING_SHELL_GROUND => act_riding_shell_ground(m),
        ACT_WALKING => act_walking(m),
        ACT_TURNING_AROUND => act_turning_around(m),
        ACT_FINISH_TURNING_AROUND => act_finish_turning_around(m),
        ACT_BRAKING => act_braking(m),
        ACT_DECELERATING => act_decelerating(m),
        ACT_CRAWLING => act_crawling(m),
        ACT_CROUCH_SLIDE => act_crouch_slide(m),
        ACT_HOLD_STOMACH_SLIDE => act_stomach_slide(m),
        ACT_HOLD_BUTT_SLIDE => common_slide_action_with_jump(m, ACT_HOLD_BUTT_SLIDE_STOP, ACT_HOLD_JUMP, ACT_HOLD_BUTT_SLIDE_AIR),
        ACT_BUTT_SLIDE => act_butt_slide(m),
        ACT_STOMACH_SLIDE => act_stomach_slide(m),
        ACT_DIVE_SLIDE => act_dive_slide(m),
        ACT_SLIDE_KICK_SLIDE => act_slide_kick_slide(m),
        ACT_DEATH_EXIT_LAND => {
            apply_landing_accel(m, 0.9);
            m.anim_length = 79.0;
            if m.action_timer == 54.0 { m.events.push(MarioEvent::MamaMia); }
            if m.action_timer == 68.0 { m.land_event(0.0, 0.0); }
            m.action_timer += 1.0;
            if m.action_timer == 80.0 { m.set_action(ACT_IDLE, 0.0); }
            false
        }
        ACT_MOVE_PUNCHING => act_move_punching(m),
        ACT_JUMP_LAND => act_jump_land(m),
        ACT_FREEFALL_LAND => act_freefall_land(m),
        ACT_DOUBLE_JUMP_LAND => act_double_jump_land(m),
        ACT_SIDE_FLIP_LAND => act_side_flip_land(m),
        ACT_TRIPLE_JUMP_LAND => act_triple_jump_land(m),
        ACT_LONG_JUMP_LAND => act_long_jump_land(m),
        ACT_BACKFLIP_LAND => act_backflip_land(m),
        ACT_HOLD_WALKING => act_hold_walking(m),
        ACT_HOLD_DECELERATING => act_hold_decelerating(m),
        ACT_HOLD_JUMP_LAND => act_hold_jump_land(m),
        ACT_HOLD_FREEFALL_LAND => act_hold_freefall_land(m),
        _ => return None,
    })
}
