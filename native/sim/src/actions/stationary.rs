//! Port of `src/sim/actions/stationary.ts` (idle, crouching, landing stops, hold-idle, sleeping).
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::stationary_ground_step;
use crate::object::INT_SUBTYPE_HOLDABLE_NPC;
use crate::state::*;

fn ground_step(m: &mut MarioRuntime) {
    stationary_ground_step(&mut m.state, &m.world);
}

fn floor_too_steep(m: &MarioRuntime) -> bool {
    m.floor.as_ref().is_some_and(|f| f.normal[1] < 0.29237169)
}

/// check_common_idle_cancels (no objects or first-person mode).
pub fn check_common_idle_cancels(m: &mut MarioRuntime) -> bool {
    if floor_too_steep(m) {
        return m.push_off_steep_floor(ACT_FREEFALL, 0.0);
    }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_JUMP, 0.0); }
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 {
        m.face_angle[1] = m.intended_yaw;
        return m.set_action(ACT_WALKING, 0.0);
    }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_PUNCHING, 0.0); }
    if m.input & INPUT_Z_DOWN != 0 { return m.set_action(ACT_START_CROUCHING, 0.0); }
    false
}

pub fn act_lava_boost_land(m: &mut MarioRuntime) -> bool {
    m.anim_length = 20.0;
    m.forward_vel = 0.0;
    m.step_ground();
    if m.anim_frame >= 20.0 { return m.set_action(ACT_IDLE, 0.0); }
    false
}

/// act_idle, including the source's 10 head-turn cycles before sleep.
pub fn act_idle(m: &mut MarioRuntime) -> bool {
    if check_common_idle_cancels(m) { return true; }

    if m.action_state < 3.0 {
        m.action_timer += 1.0;
        if m.action_timer >= 60.0 {
            m.action_state += 1.0;
            m.action_timer = 0.0;
        }
    } else {
        let behind = m.pos[1] - m.floor_height_relative_polar(-0x8000 as f64, 60.0);
        if behind.abs() > 24.0 {
            m.action_state = 0.0;
            m.action_timer = 0.0;
        } else {
            m.action_timer += 1.0;
            if m.action_timer >= 600.0 {
                return m.set_action(ACT_START_SLEEPING, 0.0);
            }
        }
    }

    m.anim_length = 60.0;
    ground_step(m);
    false
}

pub fn act_start_sleeping(m: &mut MarioRuntime) -> bool {
    if check_common_idle_cancels(m) { return true; }

    m.action_timer += 1.0;
    m.anim_length = 20.0;
    if m.action_timer >= 20.0 {
        m.action_timer = 0.0;
        m.action_state += 1.0;
        if m.action_state >= 4.0 { return m.set_action(ACT_SLEEPING, 0.0); }
    }

    ground_step(m);
    false
}

pub fn act_sleeping(m: &mut MarioRuntime) -> bool {
    if m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE | INPUT_B_PRESSED | INPUT_Z_DOWN) != 0 {
        let arg = m.action_state;
        return m.set_action(ACT_WAKING_UP, arg);
    }

    m.action_timer += 1.0;
    m.anim_length = 120.0;
    if m.action_timer >= 120.0 {
        m.action_timer = 0.0;
        if m.action_state < 2.0 { m.action_state += 1.0; }
    }

    ground_step(m);
    false
}

pub fn act_waking_up(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }

    m.action_timer += 1.0;
    if m.action_timer >= 20.0 { return m.set_action(ACT_IDLE, 0.0); }
    m.anim_length = 20.0;
    ground_step(m);
    false
}

pub fn act_crouching(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_BACKFLIP, 0.0); }
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }
    if m.input & INPUT_Z_DOWN == 0 { return m.set_action(ACT_STOP_CROUCHING, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_START_CRAWLING, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_PUNCHING, 9.0); }

    ground_step(m);
    m.anim_length = 30.0;
    false
}

pub fn act_start_crouching(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_BACKFLIP, 0.0); }
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }

    ground_step(m);
    m.action_timer += 1.0;
    if m.action_timer >= 6.0 { return m.set_action(ACT_CROUCHING, 0.0); }
    false
}

pub fn act_stop_crouching(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_BACKFLIP, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }

    ground_step(m);
    m.action_timer += 1.0;
    if m.action_timer >= 6.0 { return m.set_action(ACT_IDLE, 0.0); }
    false
}

pub fn act_start_crawling(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }

    ground_step(m);
    m.action_timer += 1.0;
    if m.action_timer >= 8.0 { return m.set_action(ACT_CRAWLING, 0.0); }
    false
}

pub fn act_stop_crawling(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BEGIN_SLIDING, 0.0); }

    ground_step(m);
    m.action_timer += 1.0;
    if m.action_timer >= 8.0 { return m.set_action(ACT_CROUCHING, 0.0); }
    false
}

/// stopping_step
pub fn stopping_step(m: &mut MarioRuntime, frames: f64, action: u32) -> bool {
    ground_step(m);
    m.action_timer += 1.0;
    if m.action_timer >= frames { return m.set_action(action, 0.0); }
    false
}

pub fn act_braking_stop(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_PUNCHING, 0.0); }
    if m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE) != 0 {
        return m.check_common_action_exits();
    }
    stopping_step(m, 8.0, ACT_IDLE)
}

pub fn act_butt_slide_stop(m: &mut MarioRuntime) -> bool {
    if m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE) != 0 {
        return m.check_common_action_exits();
    }
    stopping_step(m, 8.0, ACT_IDLE)
}

pub fn act_slide_kick_slide_stop(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    stopping_step(m, 8.0, ACT_CROUCHING)
}

/// check_common_landing_cancels
pub fn check_common_landing_cancels(m: &mut MarioRuntime, action: u32) -> bool {
    if m.input & INPUT_A_PRESSED != 0 {
        if action == 0 { return m.set_jump_from_landing(); }
        return m.set_jumping_action(action, 0.0);
    }
    if m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE) != 0 {
        return m.check_common_action_exits();
    }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_PUNCHING, 0.0); }
    false
}

/// landing_step
pub fn landing_step(m: &mut MarioRuntime, frames: f64, action: u32) -> bool {
    ground_step(m);
    m.action_timer += 1.0;
    if m.action_timer >= frames { return m.set_action(action, 0.0); }
    false
}

pub fn act_jump_land_stop(m: &mut MarioRuntime) -> bool {
    if check_common_landing_cancels(m, 0) { return true; }
    landing_step(m, 8.0, ACT_IDLE)
}

pub fn act_double_jump_land_stop(m: &mut MarioRuntime) -> bool {
    if check_common_landing_cancels(m, 0) { return true; }
    landing_step(m, 8.0, ACT_IDLE)
}

pub fn act_freefall_land_stop(m: &mut MarioRuntime) -> bool {
    if check_common_landing_cancels(m, 0) { return true; }
    landing_step(m, 8.0, ACT_IDLE)
}

pub fn act_side_flip_land_stop(m: &mut MarioRuntime) -> bool {
    if check_common_landing_cancels(m, 0) { return true; }
    if landing_step(m, 8.0, ACT_IDLE) { return true; }
    false
}

pub fn act_triple_jump_land_stop(m: &mut MarioRuntime) -> bool {
    if check_common_landing_cancels(m, ACT_JUMP) { return true; }
    landing_step(m, 8.0, ACT_IDLE)
}

pub fn act_backflip_land_stop(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_Z_DOWN == 0 { m.input &= !INPUT_A_PRESSED; }
    if check_common_landing_cancels(m, ACT_BACKFLIP) { return true; }
    landing_step(m, 8.0, ACT_IDLE)
}

pub fn act_long_jump_land_stop(m: &mut MarioRuntime) -> bool {
    m.input &= !INPUT_B_PRESSED;
    if check_common_landing_cancels(m, ACT_JUMP) { return true; }
    landing_step(m, 8.0, ACT_CROUCHING)
}

pub fn act_ground_pound_land(m: &mut MarioRuntime) -> bool {
    m.action_state = 1.0;
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_BUTT_SLIDE, 0.0); }
    landing_step(m, 14.0, ACT_BUTT_SLIDE_STOP)
}

/// check_common_hold_idle_cancels
pub fn check_common_hold_idle_cancels(m: &mut MarioRuntime) -> bool {
    let Some(held) = m.held_obj else { return m.set_action(ACT_IDLE, 0.0) };

    if floor_too_steep(m) { return m.push_off_steep_floor(ACT_HOLD_FREEFALL, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_HOLD_JUMP, 0.0); }
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_HOLD_FREEFALL, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_HOLD_BEGIN_SLIDING, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 {
        m.face_angle[1] = m.intended_yaw;
        return m.set_action(ACT_HOLD_WALKING, 0.0);
    }
    if m.input & INPUT_B_PRESSED != 0 {
        let a = if m.objects[held].interact_subtype & INT_SUBTYPE_HOLDABLE_NPC != 0 { ACT_PLACING_DOWN } else { ACT_THROWING };
        return m.set_action(a, 0.0);
    }
    if m.input & INPUT_Z_DOWN != 0 {
        m.drop_held_object();
        return m.set_action(ACT_START_CROUCHING, 0.0);
    }
    false
}

pub fn act_hold_idle(m: &mut MarioRuntime) -> bool {
    if check_common_hold_idle_cancels(m) { return true; }
    ground_step(m);
    m.anim_length = 90.0;
    false
}

/// check_common_hold_action_exits
pub fn check_common_hold_action_exits(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_A_PRESSED != 0 { return m.set_jumping_action(ACT_HOLD_JUMP, 0.0); }
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_HOLD_FREEFALL, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_HOLD_WALKING, 0.0); }
    if m.input & INPUT_ABOVE_SLIDE != 0 { return m.set_action(ACT_HOLD_BEGIN_SLIDING, 0.0); }
    false
}

pub fn hold_landing_stop(m: &mut MarioRuntime) -> bool {
    if check_common_hold_action_exits(m) { return true; }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_THROWING, 0.0); }
    super::moving::apply_landing_accel(m, 0.9);
    landing_step(m, 6.0, ACT_HOLD_IDLE)
}

pub fn act_hold_jump_land_stop(m: &mut MarioRuntime) -> bool { hold_landing_stop(m) }

pub fn act_hold_freefall_land_stop(m: &mut MarioRuntime) -> bool { hold_landing_stop(m) }

pub fn act_air_throw_land(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    m.action_timer += 1.0;
    if m.action_timer == 4.0 && m.held_obj.is_some() { m.throw_held_object(); }
    m.anim_length = 15.0;
    if m.action_timer >= 15.0 { return m.set_action(ACT_IDLE, 0.0); }
    ground_step(m);
    false
}

fn act_hold_butt_slide_stop(m: &mut MarioRuntime) -> bool {
    if check_common_hold_action_exits(m) { return true; }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_THROWING, 0.0); }
    stopping_step(m, 14.0, ACT_HOLD_IDLE)
}

/// Action table (`STATIONARY_ACTIONS`).
pub fn stationary_action(m: &mut MarioRuntime) -> Option<bool> {
    Some(match m.action {
        ACT_IDLE => act_idle(m),
        ACT_LAVA_BOOST_LAND => act_lava_boost_land(m),
        ACT_CROUCHING => act_crouching(m),
        ACT_START_CROUCHING => act_start_crouching(m),
        ACT_STOP_CROUCHING => act_stop_crouching(m),
        ACT_START_CRAWLING => act_start_crawling(m),
        ACT_STOP_CRAWLING => act_stop_crawling(m),
        ACT_BRAKING_STOP => act_braking_stop(m),
        ACT_BUTT_SLIDE_STOP => act_butt_slide_stop(m),
        ACT_SLIDE_KICK_SLIDE_STOP => act_slide_kick_slide_stop(m),
        ACT_JUMP_LAND_STOP => act_jump_land_stop(m),
        ACT_DOUBLE_JUMP_LAND_STOP => act_double_jump_land_stop(m),
        ACT_FREEFALL_LAND_STOP => act_freefall_land_stop(m),
        ACT_SIDE_FLIP_LAND_STOP => act_side_flip_land_stop(m),
        ACT_TRIPLE_JUMP_LAND_STOP => act_triple_jump_land_stop(m),
        ACT_BACKFLIP_LAND_STOP => act_backflip_land_stop(m),
        ACT_LONG_JUMP_LAND_STOP => act_long_jump_land_stop(m),
        ACT_GROUND_POUND_LAND => act_ground_pound_land(m),
        ACT_HOLD_BUTT_SLIDE_STOP => act_hold_butt_slide_stop(m),
        ACT_HOLD_IDLE => act_hold_idle(m),
        ACT_HOLD_JUMP_LAND_STOP => act_hold_jump_land_stop(m),
        ACT_HOLD_FREEFALL_LAND_STOP => act_hold_freefall_land_stop(m),
        ACT_AIR_THROW_LAND => act_air_throw_land(m),
        ACT_START_SLEEPING => act_start_sleeping(m),
        ACT_SLEEPING => act_sleeping(m),
        ACT_WAKING_UP => act_waking_up(m),
        _ => return None,
    })
}
