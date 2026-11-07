//! Port of `src/sim/actions/drowning.ts` (`ACT_DROWNING`, `ACT_WATER_DEATH`).
use crate::mario_runtime::MarioRuntime;
use crate::state::*;

pub fn act_drowning(m: &mut MarioRuntime) -> bool {
    m.forward_vel *= 0.9;
    if m.action_state == 0.0 {
        m.anim_length = 79.0;
        if m.anim_frame >= 79.0 {
            m.action_state = 1.0;
            m.anim_frame = -1.0;
        }
    } else {
        m.anim_length = 69.0;
        if m.anim_frame >= 30.0 {
            m.respawn(false);
        }
    }
    false
}

pub fn act_water_death(m: &mut MarioRuntime) -> bool {
    m.forward_vel *= 0.9;
    m.anim_length = 35.0;
    if m.anim_frame >= 35.0 {
        m.respawn(false);
    }
    false
}

/// Action table (`SUBMERGED_ACTIONS`).
pub fn drowning_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        ACT_DROWNING => Some(act_drowning(m)),
        ACT_WATER_DEATH => Some(act_water_death(m)),
        _ => None,
    }
}
