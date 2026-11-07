//! Port of `src/sim/actions/object.ts` (punching; table entries for pick up / throw / Bowser
//! live in holding.rs and are tried by the dispatcher after this table).
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::stationary_ground_step;
use crate::state::*;

const PUNCH_FORWARD_SPEEDS: [f64; 8] = [0.0, 1.0, 1.0, 2.0, 3.0, 5.0, 7.0, 10.0];

// Source clips: first punch 5 frames, first fast punch 10, second punch 4,
// second fast punch 13, ground kick 26, breakdance 24.
const PUNCH_FIRST: f64 = 5.0;
const PUNCH_FIRST_FAST: f64 = 10.0;
const PUNCH_SECOND: f64 = 4.0;
const PUNCH_SECOND_FAST: f64 = 13.0;
const PUNCH_KICK: f64 = 26.0;
const PUNCH_BREAKDANCE: f64 = 24.0;

pub fn set_punch_animation(m: &mut MarioRuntime, frames: f64) {
    m.anim_length = frames - 1.0;
}

/// mario_update_punch_sequence, driven by the simulation's animation frame.
pub fn update_punch_sequence(m: &mut MarioRuntime, end_action: u32, crouch_end_action: u32) -> bool {
    let arg = m.action_arg;
    if arg == 0.0 || arg == 1.0 {
        set_punch_animation(m, PUNCH_FIRST);
        if m.anim_frame >= 2.0 {
            if m.mario_check_object_grab() { return true; }
            m.flags |= MARIO_PUNCHING;
        }
        if m.anim_frame >= PUNCH_FIRST - 1.0 {
            m.action_arg = 2.0;
            m.anim_frame = -1.0;
        } else {
            m.action_arg = 1.0;
        }
    } else if arg == 2.0 {
        set_punch_animation(m, PUNCH_FIRST_FAST);
        if m.anim_frame <= 0.0 { m.flags |= MARIO_PUNCHING; }
        if m.input & INPUT_B_PRESSED != 0 {
            m.action_arg = 3.0;
            m.anim_frame = -1.0;
            return false;
        }
        if m.anim_frame >= PUNCH_FIRST_FAST - 1.0 {
            return m.set_action(end_action, 0.0);
        }
    } else if arg == 3.0 || arg == 4.0 {
        set_punch_animation(m, PUNCH_SECOND);
        if m.anim_frame > 0.0 { m.flags |= MARIO_PUNCHING; }
        if m.anim_frame >= PUNCH_SECOND - 1.0 {
            m.action_arg = 5.0;
            m.anim_frame = -1.0;
        } else {
            m.action_arg = 4.0;
        }
    } else if arg == 5.0 {
        set_punch_animation(m, PUNCH_SECOND_FAST);
        if m.anim_frame <= 0.0 { m.flags |= MARIO_PUNCHING; }
        if m.input & INPUT_B_PRESSED != 0 {
            m.action_arg = 6.0;
            m.anim_frame = -1.0;
            return false;
        }
        if m.anim_frame >= PUNCH_SECOND_FAST - 1.0 {
            return m.set_action(end_action, 0.0);
        }
    } else if arg == 6.0 {
        set_punch_animation(m, PUNCH_KICK);
        if m.anim_frame >= 0.0 && m.anim_frame < 8.0 { m.flags |= MARIO_KICKING; }
        if m.anim_frame >= PUNCH_KICK - 1.0 {
            return m.set_action(end_action, 0.0);
        }
    } else if arg == 9.0 {
        set_punch_animation(m, PUNCH_BREAKDANCE);
        if m.anim_frame >= 2.0 && m.anim_frame < 8.0 { m.flags |= MARIO_TRIPPING; }
        if m.anim_frame >= PUNCH_BREAKDANCE - 1.0 {
            return m.set_action(crouch_end_action, 0.0);
        }
    }
    false
}

/// act_punching
pub fn act_punching(m: &mut MarioRuntime) -> bool {
    if m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE) != 0 {
        return m.check_common_action_exits();
    }
    if m.action_state == 0.0 && m.input & INPUT_A_DOWN != 0 {
        return m.set_action(ACT_JUMP_KICK, 0.0);
    }
    m.action_state = 1.0;

    if m.action_arg == 0.0 && m.action_timer == 0.0 { m.action_timer = 7.0; }
    let idx = m.action_timer.min(7.0) as usize;
    mario_set_forward_vel(m, PUNCH_FORWARD_SPEEDS[idx]);
    if m.action_timer > 0.0 { m.action_timer -= 1.0; }

    if update_punch_sequence(m, ACT_IDLE, ACT_CROUCHING) { return true; }

    m.step_ground();
    false
}

/// Action table (`OBJECT_ACTIONS` rows whose bodies live here).
pub fn object_action(m: &mut MarioRuntime) -> Option<bool> {
    Some(match m.action {
        ACT_PUNCHING => act_punching(m),
        ACT_DIVE_PICKING_UP => {
            let w = m.world.clone();
            stationary_ground_step(&mut m.state, &w);
            m.action_timer += 1.0;
            m.action_timer >= 37.0 && m.set_action(ACT_HOLD_IDLE, 0.0)
        }
        _ => return None,
    })
}
