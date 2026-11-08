//! Port of `src/sim/actions/airborne.ts` (jumps, freefall, flying, knockback, ground pound, kicks,
//! riding-shell jump). `ACT_SHOT_FROM_CANNON` is routed to cannon.rs by the dispatcher.
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::surfaces::SURFACE_BURNING;
#[allow(unused_imports)]
use crate::{math::*, state::*};

const STEP_NONE: i32 = AIR_STEP_NONE as i32;
const STEP_LANDED: i32 = AIR_STEP_LANDED as i32;
const STEP_HIT_WALL: i32 = AIR_STEP_HIT_WALL as i32;
const STEP_GRABBED_LEDGE: i32 = AIR_STEP_GRABBED_LEDGE as i32;
const STEP_GRABBED_CEILING: i32 = AIR_STEP_GRABBED_CEILING as i32;
const STEP_HIT_LAVA_WALL: i32 = AIR_STEP_HIT_LAVA_WALL as i32;

fn sin(angle: f64) -> f64 {
    js_sin(to_s16(angle) as f64 / 32768.0 * std::f64::consts::PI)
}
fn cos(angle: f64) -> f64 {
    js_cos(to_s16(angle) as f64 / 32768.0 * std::f64::consts::PI)
}
fn s16(v: f64) -> f64 { to_s16(v) as f64 }
fn set_fwd(m: &mut MarioRuntime, v: f64) { mario_set_forward_vel(&mut m.state, v); }

// ---------------------------------------------------------------------------
// Air control
// ---------------------------------------------------------------------------

/// update_air_without_turn
pub fn update_air_without_turn(m: &mut MarioRuntime) {
    let drag_threshold = if m.action == ACT_LONG_JUMP { 48.0 } else { 32.0 };

    m.forward_vel = approach_f32(m.forward_vel, 0.0, 0.35, 0.35);

    let mut sideways_speed = 0.0;
    if m.input & INPUT_NONZERO_ANALOG != 0 {
        let intended_d_yaw = s16(m.intended_yaw - m.face_angle[1]);
        let intended_mag = m.intended_mag / 32.0;

        m.forward_vel += intended_mag * cos(intended_d_yaw) * 1.5;
        sideways_speed = intended_mag * sin(intended_d_yaw) * 10.0;
    }

    if m.forward_vel > drag_threshold { m.forward_vel -= 1.0; }
    if m.forward_vel < -16.0 { m.forward_vel += 2.0; }

    m.slide_vel_x = m.forward_vel * sin(m.face_angle[1]);
    m.slide_vel_z = m.forward_vel * cos(m.face_angle[1]);

    m.slide_vel_x += sideways_speed * sin(s16(m.face_angle[1] + 0x4000 as f64));
    m.slide_vel_z += sideways_speed * cos(s16(m.face_angle[1] + 0x4000 as f64));

    m.vel[0] = m.slide_vel_x;
    m.vel[2] = m.slide_vel_z;
}

/// update_air_with_turn
pub fn update_air_with_turn(m: &mut MarioRuntime) {
    let drag_threshold = if m.action == ACT_LONG_JUMP { 48.0 } else { 32.0 };

    m.forward_vel = approach_f32(m.forward_vel, 0.0, 0.35, 0.35);

    if m.input & INPUT_NONZERO_ANALOG != 0 {
        let intended_d_yaw = s16(m.intended_yaw - m.face_angle[1]);
        let intended_mag = m.intended_mag / 32.0;

        m.forward_vel += 1.5 * cos(intended_d_yaw) * intended_mag;
        m.face_angle[1] = s16(m.face_angle[1] + 512.0 * sin(intended_d_yaw) * intended_mag);
    }

    if m.forward_vel > drag_threshold { m.forward_vel -= 1.0; }
    if m.forward_vel < -16.0 { m.forward_vel += 2.0; }

    m.slide_vel_x = m.forward_vel * sin(m.face_angle[1]);
    m.vel[0] = m.slide_vel_x;
    m.slide_vel_z = m.forward_vel * cos(m.face_angle[1]);
    m.vel[2] = m.slide_vel_z;
}

// ---------------------------------------------------------------------------
// Common steps
// ---------------------------------------------------------------------------

/// check_fall_damage (no snow/sand sticking)
pub fn check_fall_damage(m: &mut MarioRuntime) -> bool {
    if m.floor.is_none() { return false; }
    if m.flags & MARIO_METAL_CAP != 0 { return false; }
    if m.vel[1] < -55.0 {
        let fall_height = m.peak_height - m.pos[1];

        if fall_height > 3000.0 {
            m.hurt_counter += 16.0;
            m.health -= 16.0;
            m.squish_timer = 30.0;
        } else if fall_height > 1150.0 && !m.floor_is_slippery() {
            m.hurt_counter += 8.0;
            m.health -= 8.0;
            m.squish_timer = 30.0;
        }

        if m.health < 0x100 as f64 {
            m.respawn(false);
            return true;
        }
    }
    false
}

/// check_wall_kick
pub fn check_wall_kick(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_A_PRESSED != 0 && m.wall_kick_timer != 0.0 && m.prev_action == ACT_AIR_HIT_WALL {
        m.face_angle[1] = s16(m.face_angle[1] + 0x8000 as f64);
        return m.set_action(ACT_WALL_KICK_AIR, 0.0);
    }
    false
}

pub fn approach_s32(value: f64, target: f64, increment: f64, decrement: f64) -> f64 {
    if value < target { return target.min(value + increment); }
    if value > target { return target.max(value - decrement); }
    value
}

/// update_flying_pitch / update_flying_yaw / update_flying from mario_actions_airborne.c
pub fn update_flying(m: &mut MarioRuntime) {
    let pitch_target = -(((m.stick_y * (m.forward_vel / 5.0)).trunc() as i64 as i32) as f64);
    if pitch_target > 0.0 {
        if m.angle_vel[0] < 0.0 { m.angle_vel[0] = (0x20 as f64).min(m.angle_vel[0] + 0x40 as f64); }
        else { m.angle_vel[0] = approach_s32(m.angle_vel[0], pitch_target, 0x20 as f64, 0x40 as f64); }
    } else if pitch_target < 0.0 {
        if m.angle_vel[0] > 0.0 { m.angle_vel[0] = (-0x20 as f64).max(m.angle_vel[0] - 0x40 as f64); }
        else { m.angle_vel[0] = approach_s32(m.angle_vel[0], pitch_target, 0x40 as f64, 0x20 as f64); }
    } else {
        m.angle_vel[0] = approach_s32(m.angle_vel[0], 0.0, 0x40 as f64, 0x40 as f64);
    }

    let yaw_target = -(((m.stick_x * (m.forward_vel / 4.0)).trunc() as i64 as i32) as f64);
    if yaw_target > 0.0 {
        if m.angle_vel[1] < 0.0 { m.angle_vel[1] = (0x10 as f64).min(m.angle_vel[1] + 0x40 as f64); }
        else { m.angle_vel[1] = approach_s32(m.angle_vel[1], yaw_target, 0x10 as f64, 0x20 as f64); }
    } else if yaw_target < 0.0 {
        if m.angle_vel[1] > 0.0 { m.angle_vel[1] = (-0x10 as f64).max(m.angle_vel[1] - 0x40 as f64); }
        else { m.angle_vel[1] = approach_s32(m.angle_vel[1], yaw_target, 0x20 as f64, 0x10 as f64); }
    } else {
        m.angle_vel[1] = approach_s32(m.angle_vel[1], 0.0, 0x40 as f64, 0x40 as f64);
    }

    m.face_angle[1] = s16(m.face_angle[1] + m.angle_vel[1]);
    m.face_angle[2] = s16(20.0 * -m.angle_vel[1]);

    m.forward_vel -= 2.0 * (m.face_angle[0] / 0x4000 as f64) + 0.1;
    m.forward_vel -= 0.5 * (1.0 - coss(m.angle_vel[1]));
    if m.forward_vel < 0.0 { m.forward_vel = 0.0; }
    if m.forward_vel > 16.0 { m.face_angle[0] = s16(m.face_angle[0] + (m.forward_vel - 32.0) * 6.0); }
    else if m.forward_vel > 4.0 { m.face_angle[0] = s16(m.face_angle[0] + (m.forward_vel - 32.0) * 10.0); }
    else { m.face_angle[0] -= 0x400 as f64; }

    m.face_angle[0] += m.angle_vel[0];
    m.face_angle[0] = (-0x2aaa as f64).max((0x2aaa as f64).min(m.face_angle[0]));

    m.vel[0] = m.forward_vel * coss(m.face_angle[0]) * sins(m.face_angle[1]);
    m.vel[1] = m.forward_vel * sins(m.face_angle[0]);
    m.vel[2] = m.forward_vel * coss(m.face_angle[0]) * coss(m.face_angle[1]);
    m.slide_vel_x = m.vel[0];
    m.slide_vel_z = m.vel[2];
}

pub fn act_flying(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 1.0); }
    if m.flags & MARIO_WING_CAP == 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.action_state == 0.0 {
        m.anim_length = (if m.action_arg != 0.0 { 42.0 } else { 10.0 }) - 1.0;
        if m.anim_frame >= m.anim_length { m.action_state = 1.0; }
    }

    let start_pitch = m.face_angle[0];
    update_flying(m);
    match m.step_air(0) {
        STEP_NONE => { m.action_timer = 0.0; }
        STEP_LANDED => {
            m.land_event(m.vel[1], m.forward_vel);
            m.face_angle[0] = 0.0;
            m.set_action(ACT_DIVE_SLIDE, 0.0);
        }
        STEP_HIT_WALL => {
            if m.wall.is_some() {
                m.events.push(MarioEvent::Bonk { speed: m.forward_vel });
                set_fwd(m, -16.0);
                m.face_angle[0] = 0.0;
                m.vel[1] = (0.0f64).min(m.vel[1]);
                m.particle_flags |= 2;
                m.set_action(ACT_BACKWARD_AIR_KB, 0.0);
            } else {
                // The air step also reports leaving the collision mesh as a wall hit.
                let prev = m.action_timer;
                m.action_timer += 1.0;
                if prev == 0.0 { m.events.push(MarioEvent::FlightBoundary); }
                if m.action_timer == 30.0 { m.action_timer = 0.0; }
                m.face_angle[0] = (-0x2aaa as f64).max(m.face_angle[0] - 0x200 as f64);
            }
        }
        STEP_HIT_LAVA_WALL => {
            if let Some(w) = m.wall.clone() { m.face_angle[1] = atan2s(w.normal[2], w.normal[0]) as f64; }
            m.forward_vel = (24.0f64).max(m.forward_vel);
            if m.flags & MARIO_METAL_CAP == 0 {
                m.hurt_counter += if m.flags & MARIO_CAP_ON_HEAD != 0 { 12.0 } else { 18.0 };
            }
            m.set_action(ACT_LAVA_BOOST, 1.0);
            m.vel[1] = 84.0;
            m.events.push(MarioEvent::FlightBurn);
        }
        _ => {}
    }
    if m.face_angle[0] > 0x800 as f64 && m.forward_vel >= 48.0 { m.particle_flags |= 1; }
    let pull_up = start_pitch <= 0.0 && m.face_angle[0] > 0.0 && m.forward_vel >= 48.0;
    m.events.push(MarioEvent::WingFlight { pull_up });
    false
}

pub fn act_riding_shell_jump(m: &mut MarioRuntime) -> bool {
    update_air_without_turn(m);
    match m.step_air(0) {
        STEP_LANDED => { m.set_action(ACT_RIDING_SHELL_GROUND, 1.0); }
        STEP_HIT_WALL => set_fwd(m, 0.0),
        STEP_HIT_LAVA_WALL => { m.set_action(ACT_LAVA_BOOST, 0.0); }
        _ => {}
    }
    false
}

/// common_air_action_step
pub fn common_air_action_step(m: &mut MarioRuntime, land_action: u32, step_arg: u32) -> i32 {
    update_air_without_turn(m);

    let step_result = m.step_air(step_arg);

    match step_result {
        STEP_NONE => {}

        STEP_LANDED => {
            m.land_event(m.vel[1], m.forward_vel);
            if !check_fall_damage(m) {
                m.set_action(land_action, 0.0);
            }
        }

        STEP_HIT_WALL => {
            if m.forward_vel > 16.0 {
                m.bonk_reflection(false);
                m.face_angle[1] = s16(m.face_angle[1] + 0x8000 as f64);

                if m.wall.is_some() {
                    m.set_action(ACT_AIR_HIT_WALL, 0.0);
                } else {
                    if m.vel[1] > 0.0 { m.vel[1] = 0.0; }
                    if m.forward_vel >= 38.0 {
                        m.particle_flags |= 2;
                        m.set_action(ACT_SOFT_BONK, 0.0);
                        m.forward_vel = -m.forward_vel;
                    } else {
                        if m.forward_vel > 8.0 { set_fwd(m, -8.0); }
                        m.set_action(ACT_SOFT_BONK, 0.0);
                    }
                }
            } else {
                set_fwd(m, 0.0);
            }
        }

        STEP_GRABBED_LEDGE => { m.drop_and_set_action(ACT_LEDGE_GRAB, 0.0); }

        STEP_GRABBED_CEILING => { m.set_action(ACT_START_HANGING, 0.0); }

        _ => {}
    }

    step_result
}

/// common_air_knockback_step
pub fn common_air_knockback_step(m: &mut MarioRuntime, land_action: u32, speed: f64) -> i32 {
    set_fwd(m, speed);

    let step_result = m.step_air(0);

    match step_result {
        STEP_LANDED => {
            m.land_event(m.vel[1], m.forward_vel);
            if !check_fall_damage(m) {
                let arg = m.action_arg;
                m.set_action(land_action, arg);
            }
        }

        STEP_HIT_WALL => {
            m.bonk_reflection(false);
            if m.vel[1] > 0.0 { m.vel[1] = 0.0; }
            set_fwd(m, -speed);
        }

        _ => {}
    }

    step_result
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// check_kick_or_dive_in_air
pub fn check_kick_or_dive_in_air(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 {
        return m.set_action(if m.forward_vel > 28.0 { ACT_DIVE } else { ACT_JUMP_KICK }, 0.0);
    }
    false
}

pub fn act_lava_boost(m: &mut MarioRuntime) -> bool {
    m.anim_length = 20.0;
    if m.input & INPUT_NONZERO_ANALOG == 0 { m.forward_vel *= 0.9; }

    let result = m.step_air(0);
    if result == STEP_LANDED {
        if m.floor.as_ref().is_some_and(|f| f.kind == SURFACE_BURNING) {
            m.action_state = 0.0;
            m.hurt_counter += 18.0;
            m.vel[1] = 84.0;
        } else if m.action_state < 2.0 && m.vel[1] < 0.0 {
            m.vel[1] = -m.vel[1] * 0.4;
            let v = m.forward_vel * 0.5;
            set_fwd(m, v);
            m.action_state += 1.0;
        } else {
            return m.set_action(ACT_LAVA_BOOST_LAND, 0.0);
        }
    } else if result == STEP_HIT_WALL {
        let v = -m.forward_vel;
        set_fwd(m, v);
    }
    false
}

pub fn act_jump(m: &mut MarioRuntime) -> bool {
    if check_kick_or_dive_in_air(m) { return true; }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }

    let arg = m.air_step_arg() | AIR_STEP_CHECK_HANG;
    common_air_action_step(m, ACT_JUMP_LAND, arg);
    false
}

pub fn act_double_jump(m: &mut MarioRuntime) -> bool {
    if check_kick_or_dive_in_air(m) { return true; }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }

    let arg = m.air_step_arg() | AIR_STEP_CHECK_HANG;
    common_air_action_step(m, ACT_DOUBLE_JUMP_LAND, arg);
    false
}

pub fn act_triple_jump(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_DIVE, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }

    common_air_action_step(m, ACT_TRIPLE_JUMP_LAND, 0);
    false
}

pub fn act_backflip(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }
    common_air_action_step(m, ACT_BACKFLIP_LAND, 0);
    false
}

pub fn act_freefall(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_DIVE, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }

    let arg = m.air_step_arg();
    common_air_action_step(m, ACT_FREEFALL_LAND, arg);
    false
}

pub fn act_side_flip(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_DIVE, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }

    let arg = m.air_step_arg();
    common_air_action_step(m, ACT_SIDE_FLIP_LAND, arg);
    false
}

pub fn act_wall_kick_air(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_DIVE, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }

    let arg = m.air_step_arg();
    common_air_action_step(m, ACT_JUMP_LAND, arg);
    false
}

pub fn act_long_jump(m: &mut MarioRuntime) -> bool {
    let arg = m.air_step_arg();
    common_air_action_step(m, ACT_LONG_JUMP_LAND, arg);
    false
}

pub fn act_dive(m: &mut MarioRuntime) -> bool {
    update_air_without_turn(m);

    let step_result = m.step_air(0);

    match step_result {
        STEP_NONE => {
            if m.vel[1] < 0.0 && m.face_angle[0] > -0x2aaa as f64 {
                m.face_angle[0] -= 0x200 as f64;
                if m.face_angle[0] < -0x2aaa as f64 { m.face_angle[0] = -0x2aaa as f64; }
            }
        }

        STEP_LANDED => {
            m.land_event(m.vel[1], m.forward_vel);
            m.face_angle[0] = 0.0;
            if !check_fall_damage(m) {
                m.set_action(ACT_DIVE_SLIDE, 0.0);
            }
        }

        STEP_HIT_WALL => {
            m.bonk_reflection(true);
            m.face_angle[0] = 0.0;
            if m.vel[1] > 0.0 { m.vel[1] = 0.0; }
            m.particle_flags |= 2;
            set_fwd(m, -16.0);
            m.set_action(ACT_SOFT_BONK, 0.0);
        }

        _ => {}
    }

    false
}

pub fn act_ground_pound(m: &mut MarioRuntime) -> bool {
    if m.action_state == 0.0 {
        if m.action_timer < 10.0 {
            let y_offset = 20.0 - 2.0 * m.action_timer;
            if m.pos[1] + y_offset + 160.0 < m.ceil_height {
                m.pos[1] += y_offset;
                m.peak_height = m.pos[1];
            }
        }

        m.vel[1] = -50.0;
        set_fwd(m, 0.0);

        m.action_timer += 1.0;
        if m.action_timer >= (if m.action_arg == 0.0 { 11.0 } else { 16.0 }) + 4.0 {
            m.events.push(MarioEvent::GroundpoundDrop);
            m.action_state = 1.0;
        }
        return false;
    }

    let step_result = m.step_air(0);

    if step_result == STEP_LANDED {
        m.land_event(m.vel[1], m.forward_vel);
        if !check_fall_damage(m) {
            m.particle_flags |= 3;
            m.set_action(ACT_GROUND_POUND_LAND, 0.0);
        }
    } else if step_result == STEP_HIT_WALL {
        set_fwd(m, -16.0);
        if m.vel[1] > 0.0 { m.vel[1] = 0.0; }
        m.particle_flags |= 2;
        m.set_action(ACT_SOFT_BONK, 0.0);
    }

    false
}

pub fn act_steep_jump(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_DIVE, 0.0); }

    let v = 0.98 * m.forward_vel;
    set_fwd(m, v);

    match m.step_air(0) {
        STEP_LANDED => {
            m.land_event(m.vel[1], m.forward_vel);
            m.face_angle[0] = 0.0;
            if !check_fall_damage(m) {
                m.set_action(if m.forward_vel < 0.0 { ACT_BEGIN_SLIDING } else { ACT_JUMP_LAND }, 0.0);
            }
        }

        STEP_HIT_WALL => set_fwd(m, 0.0),

        _ => {}
    }

    false
}

pub fn act_air_hit_wall(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    if m.action_timer <= 2.0 {
        if m.input & INPUT_A_PRESSED != 0 {
            m.vel[1] = 52.0;
            m.face_angle[1] = s16(m.face_angle[1] + 0x8000 as f64);
            return m.set_action(ACT_WALL_KICK_AIR, 0.0);
        }
    } else if m.forward_vel >= 38.0 {
        m.wall_kick_timer = 5.0;
        if m.vel[1] > 0.0 { m.vel[1] = 0.0; }
        m.particle_flags |= 2;
        let v = -m.forward_vel;
        set_fwd(m, v);
        return m.set_action(ACT_SOFT_BONK, 0.0);
    } else {
        m.wall_kick_timer = 5.0;
        if m.vel[1] > 0.0 { m.vel[1] = 0.0; }
        if m.forward_vel > 8.0 { set_fwd(m, -8.0); }
        return m.set_action(ACT_SOFT_BONK, 0.0);
    }

    false
}

pub fn act_soft_bonk(m: &mut MarioRuntime) -> bool {
    if check_wall_kick(m) { return true; }

    let speed = m.forward_vel;
    common_air_knockback_step(m, ACT_FREEFALL_LAND, speed);
    false
}

pub fn act_slide_kick(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    if m.action_timer > 30.0 && m.pos[1] - m.floor_height > 500.0 {
        return m.set_action(ACT_FREEFALL, 2.0);
    }

    update_air_without_turn(m);

    match m.step_air(0) {
        STEP_LANDED => {
            if m.action_state == 0.0 && m.vel[1] < 0.0 {
                m.vel[1] = -m.vel[1] / 2.0;
                m.action_state = 1.0;
                m.action_timer = 0.0;
                m.land_event(0.0, m.forward_vel);
            } else {
                m.land_event(m.vel[1], m.forward_vel);
                m.set_action(ACT_SLIDE_KICK_SLIDE, 0.0);
            }
        }

        STEP_HIT_WALL => {
            if m.vel[1] > 0.0 { m.vel[1] = 0.0; }
            m.particle_flags |= 2;
            set_fwd(m, -16.0);
            m.set_action(ACT_SOFT_BONK, 0.0);
        }

        _ => {}
    }

    false
}

pub fn act_jump_kick(m: &mut MarioRuntime) -> bool {
    update_air_without_turn(m);

    match m.step_air(0) {
        STEP_LANDED => {
            m.land_event(m.vel[1], m.forward_vel);
            if !check_fall_damage(m) {
                m.set_action(ACT_FREEFALL_LAND, 0.0);
            }
        }

        STEP_HIT_WALL => set_fwd(m, 0.0),

        _ => {}
    }

    false
}

pub fn act_butt_slide_air(m: &mut MarioRuntime) -> bool {
    let held = m.action == ACT_HOLD_BUTT_SLIDE_AIR;
    m.action_timer += 1.0;
    if m.action_timer > 30.0 && m.pos[1] - m.floor_height > 500.0 {
        return m.set_action(if held { ACT_HOLD_FREEFALL } else { ACT_FREEFALL }, 1.0);
    }

    update_air_with_turn(m);

    match m.step_air(0) {
        STEP_LANDED => {
            if m.action_state == 0.0 && m.vel[1] < 0.0 && m.floor.as_ref().is_some_and(|f| f.normal[1] >= 0.9848077) {
                m.vel[1] = -m.vel[1] / 2.0;
                m.action_state = 1.0;
                m.land_event(0.0, m.forward_vel);
            } else {
                m.land_event(m.vel[1], m.forward_vel);
                m.set_action(if held { ACT_HOLD_BUTT_SLIDE } else { ACT_BUTT_SLIDE }, 0.0);
            }
        }

        STEP_HIT_WALL => {
            if m.vel[1] > 0.0 { m.vel[1] = 0.0; }
            m.particle_flags |= 2;
            set_fwd(m, -8.0);
            m.set_action(if held { ACT_BACKWARD_AIR_KB } else { ACT_SOFT_BONK }, 0.0);
        }

        _ => {}
    }

    false
}

pub fn air_knockback(m: &mut MarioRuntime) -> bool {
    let hard = m.action == ACT_HARD_BACKWARD_AIR_KB || m.action == ACT_HARD_FORWARD_AIR_KB;
    let backward = m.action == ACT_BACKWARD_AIR_KB || m.action == ACT_HARD_BACKWARD_AIR_KB;
    let speed = if backward { -16.0 } else { 16.0 };
    set_fwd(m, speed);
    match m.step_air(0) {
        STEP_LANDED => {
            let arg = m.action_arg;
            m.set_action(
                if backward {
                    if hard { ACT_HARD_BACKWARD_GROUND_KB } else { ACT_BACKWARD_GROUND_KB }
                } else if hard { ACT_HARD_FORWARD_GROUND_KB } else { ACT_FORWARD_GROUND_KB },
                arg,
            );
        }
        STEP_HIT_WALL => {
            m.bonk_reflection(false);
            m.vel[1] = (0.0f64).min(m.vel[1]);
            set_fwd(m, -speed);
        }
        _ => {}
    }
    false
}

pub fn act_hold_jump(m: &mut MarioRuntime) -> bool {
    if m.held_obj.is_none() { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_AIR_THROW, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 {
        m.drop_held_object();
        return m.set_action(ACT_GROUND_POUND, 0.0);
    }
    common_air_action_step(m, ACT_HOLD_JUMP_LAND, AIR_STEP_CHECK_LEDGE_GRAB);
    false
}

pub fn act_hold_freefall(m: &mut MarioRuntime) -> bool {
    if m.held_obj.is_none() { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_AIR_THROW, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 {
        m.drop_held_object();
        return m.set_action(ACT_GROUND_POUND, 0.0);
    }
    common_air_action_step(m, ACT_HOLD_FREEFALL_LAND, AIR_STEP_CHECK_LEDGE_GRAB);
    false
}

pub fn act_air_throw(m: &mut MarioRuntime) -> bool {
    if m.held_obj.is_none() { return m.set_action(ACT_FREEFALL, 0.0); }
    m.action_timer += 1.0;
    if m.action_timer == 4.0 { m.throw_held_object(); }
    common_air_action_step(m, ACT_AIR_THROW_LAND, 0);
    false
}

fn act_flying_triple_jump(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_DIVE, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }
    m.action_timer += 1.0;
    if m.action_timer >= 14.0 { m.action_state = 1.0; }
    if m.vel[1] < 4.0 {
        let v = (32.0f64).max(m.forward_vel);
        set_fwd(m, v);
        m.set_action(ACT_FLYING, 1.0);
    }
    update_air_without_turn(m);
    match m.step_air(0) {
        STEP_LANDED => { m.set_action(ACT_DOUBLE_JUMP_LAND, 0.0); }
        STEP_HIT_WALL => m.bonk_reflection(false),
        STEP_HIT_LAVA_WALL => { m.set_action(ACT_LAVA_BOOST, 0.0); }
        _ => {}
    }
    false
}

fn act_top_of_pole_jump(m: &mut MarioRuntime) -> bool {
    common_air_action_step(m, ACT_FREEFALL_LAND, AIR_STEP_CHECK_LEDGE_GRAB);
    false
}

/// Action table (`AIRBORNE_ACTIONS`). `ACT_SHOT_FROM_CANNON` is handled by cannon.rs.
pub fn airborne_action(m: &mut MarioRuntime) -> Option<bool> {
    Some(match m.action {
        ACT_BACKWARD_AIR_KB | ACT_FORWARD_AIR_KB | ACT_HARD_BACKWARD_AIR_KB | ACT_HARD_FORWARD_AIR_KB => air_knockback(m),
        ACT_FLYING => act_flying(m),
        ACT_RIDING_SHELL_FALL | ACT_RIDING_SHELL_JUMP => act_riding_shell_jump(m),
        ACT_JUMP => act_jump(m),
        ACT_DOUBLE_JUMP => act_double_jump(m),
        ACT_FLYING_TRIPLE_JUMP => act_flying_triple_jump(m),
        ACT_TRIPLE_JUMP => act_triple_jump(m),
        ACT_BACKFLIP => act_backflip(m),
        ACT_FREEFALL => act_freefall(m),
        ACT_SIDE_FLIP => act_side_flip(m),
        ACT_TOP_OF_POLE_JUMP => act_top_of_pole_jump(m),
        ACT_WALL_KICK_AIR => act_wall_kick_air(m),
        ACT_LONG_JUMP => act_long_jump(m),
        ACT_DIVE => act_dive(m),
        ACT_GROUND_POUND => act_ground_pound(m),
        ACT_LAVA_BOOST => act_lava_boost(m),
        ACT_STEEP_JUMP => act_steep_jump(m),
        ACT_AIR_HIT_WALL => act_air_hit_wall(m),
        ACT_SOFT_BONK => act_soft_bonk(m),
        ACT_SLIDE_KICK => act_slide_kick(m),
        ACT_JUMP_KICK => act_jump_kick(m),
        ACT_HOLD_BUTT_SLIDE_AIR | ACT_BUTT_SLIDE_AIR => act_butt_slide_air(m),
        ACT_HOLD_JUMP => act_hold_jump(m),
        ACT_HOLD_FREEFALL => act_hold_freefall(m),
        ACT_AIR_THROW => act_air_throw(m),
        _ => return None,
    })
}
