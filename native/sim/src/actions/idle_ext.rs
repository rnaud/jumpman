//! Stationary actions that were missing from the idle set (`mario_actions_stationary.c`): panting at
//! low health, coughing in poison gas, shivering in snow, sinking in quicksand and standing against a
//! wall. Animation-driven timers use the `AnimClock` of `cutscene_ext`.
//!
//! `INPUT_STOMPED` (the `ACT_SHOCKWAVE_BOUNCE` trigger) is never raised in this sim, so those branches
//! of the originals are left out.
use crate::actions::stationary::check_common_idle_cancels;
use crate::cutscene_ext::CutCue;
use crate::mario_anim_meta::*;
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::stationary_ground_step;
use crate::state::*;

fn step(m: &mut MarioRuntime) {
    stationary_ground_step(&mut m.state, &m.world);
}

fn sound(m: &mut MarioRuntime, name: &'static str) {
    m.cut.cue(CutCue::Sound(name));
}

/// `PARTICLE_BREATH`.
pub const PARTICLE_BREATH: u32 = 1 << 17;

/// The idle entries that come before `check_common_idle_cancels` in `act_idle`: quicksand, poison gas
/// and the low-health pant (`actionArg & 1` is the standing-against-wall flag).
pub fn idle_prechecks(m: &mut MarioRuntime) -> bool {
    if m.quicksand_depth > 30.0 {
        return m.set_action(ACT_IN_QUICKSAND, 0.0);
    }
    if m.input & INPUT_IN_POISON_GAS != 0 {
        return m.set_action(ACT_COUGHING, 0.0);
    }
    if (m.action_arg as i32) & 1 == 0 && m.health < 0x300 as f64 {
        return m.set_action(ACT_PANTING, 0.0);
    }
    false
}

pub fn act_panting(m: &mut MarioRuntime) -> bool {
    if m.health >= 0x500 as f64 {
        return m.set_action(ACT_IDLE, 0.0);
    }
    if check_common_idle_cancels(m) {
        return true;
    }
    if m.cut.clock.set(MARIO_ANIM_WALK_PANTING) == 1 {
        sound(m, "MARIO_PANTING");
    }
    step(m);
    false
}

pub fn act_coughing(m: &mut MarioRuntime) -> bool {
    if check_common_idle_cancels(m) {
        return true;
    }
    step(m);
    let frame = m.cut.clock.set(MARIO_ANIM_COUGHING);
    if frame == 25 || frame == 35 {
        sound(m, "MARIO_COUGHING3");
    }
    if frame == 50 || frame == 58 {
        sound(m, "MARIO_COUGHING2");
    }
    if frame == 71 || frame == 80 {
        sound(m, "MARIO_COUGHING1");
    }
    false
}

pub fn act_shivering(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 {
        return m.set_action(ACT_FREEFALL, 0.0);
    }
    if m.input & INPUT_ABOVE_SLIDE != 0 {
        return m.set_action(ACT_BEGIN_SLIDING, 0.0);
    }
    if m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_FIRST_PERSON | INPUT_B_PRESSED | INPUT_Z_PRESSED) != 0 {
        m.state.action_state = 2.0;
    }
    step(m);
    match m.action_state as i32 {
        0 => {
            let frame = m.cut.clock.set(MARIO_ANIM_SHIVERING_WARMING_HAND);
            if frame == 49 {
                m.state.particle_flags |= PARTICLE_BREATH;
                sound(m, "MARIO_PANTING_COLD");
            }
            if frame == 7 || frame == 81 {
                sound(m, "ACTION_CLAP_HANDS_COLD");
            }
            if m.cut.clock.past_end() {
                m.state.action_state = 1.0;
            }
        }
        1 => {
            let frame = m.cut.clock.set(MARIO_ANIM_SHIVERING);
            if frame == 9 || frame == 25 || frame == 44 {
                sound(m, "ACTION_CLAP_HANDS_COLD");
            }
        }
        _ => {
            m.cut.clock.set(MARIO_ANIM_SHIVERING_RETURN_TO_IDLE);
            if m.cut.clock.past_end() {
                m.set_action(ACT_IDLE, 0.0);
            }
        }
    }
    false
}

pub fn act_in_quicksand(m: &mut MarioRuntime) -> bool {
    if m.quicksand_depth < 30.0 {
        return m.set_action(ACT_IDLE, 0.0);
    }
    if check_common_idle_cancels(m) {
        return true;
    }
    m.cut.clock.set(if m.quicksand_depth > 70.0 { MARIO_ANIM_DYING_IN_QUICKSAND } else { MARIO_ANIM_IDLE_IN_QUICKSAND });
    step(m);
    false
}

pub fn act_standing_against_wall(m: &mut MarioRuntime) -> bool {
    if m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE) != 0 {
        return m.check_common_action_exits();
    }
    if m.input & INPUT_B_PRESSED != 0 {
        return m.set_action(ACT_PUNCHING, 0.0);
    }
    m.cut.clock.set(MARIO_ANIM_STAND_AGAINST_WALL);
    step(m);
    false
}

/// Dispatch for the new actions; the animation clock ticks once per rendered frame.
pub fn idle_ext_action(m: &mut MarioRuntime) -> Option<bool> {
    if !matches!(m.action, ACT_PANTING | ACT_COUGHING | ACT_SHIVERING | ACT_IN_QUICKSAND | ACT_STANDING_AGAINST_WALL) {
        return None;
    }
    if m.action_loops <= 1 {
        m.cut.clock.advance();
    }
    Some(match m.action {
        ACT_PANTING => act_panting(m),
        ACT_COUGHING => act_coughing(m),
        ACT_SHIVERING => act_shivering(m),
        ACT_IN_QUICKSAND => act_in_quicksand(m),
        _ => act_standing_against_wall(m),
    })
}
