//! NPC dialog plumbing shared by Toad and Mips: `set_mario_npc_dialog` (mario.c),
//! `check_npc_talk` / `interact_text` (interaction.c) and `cur_obj_update_dialog_with_cutscene`
//! (object_helpers.c). The dialog box itself is the presentation layer's automatic dialog
//! (`CutCue::Dialog`, `cut.dialog_open`, see `cutscene_ext::act_reading_automatic_dialog`).
use super::*;
use crate::actions::cutscene::ready_for_npc_dialog;
use crate::cutscene_ext::{mario_ready_to_speak, CutCue};
use crate::state::*;

pub const MARIO_DIALOG_STOP: i32 = 0;
pub const MARIO_DIALOG_LOOK_FRONT: i32 = 1;
pub const MARIO_DIALOG_LOOK_UP: i32 = 2;
pub const MARIO_DIALOG_LOOK_DOWN: i32 = 3;

pub const MARIO_DIALOG_STATUS_NONE: i32 = 0;
pub const MARIO_DIALOG_STATUS_START: i32 = 1;
pub const MARIO_DIALOG_STATUS_SPEAK: i32 = 2;

/// `set_mario_npc_dialog(actionArg)` for the object `id` (`o` of the decomp call).
pub fn set_mario_npc_dialog(m: &mut MarioRuntime, id: ObjId, action_arg: i32) -> i32 {
    let mut state = MARIO_DIALOG_STATUS_NONE;
    if m.action == ACT_READING_NPC_DIALOG {
        if m.action_state < 8.0 {
            state = MARIO_DIALOG_STATUS_START;
        }
        if m.action_state == 8.0 {
            if action_arg == MARIO_DIALOG_STOP {
                m.action_state += 1.0;
            } else {
                state = MARIO_DIALOG_STATUS_SPEAK;
            }
        }
    } else if action_arg != 0 && mario_ready_to_speak(m) {
        m.used_obj = Some(id);
        m.set_action(ACT_READING_NPC_DIALOG, action_arg as f64);
        state = MARIO_DIALOG_STATUS_START;
    }
    state
}

/// `mario_can_talk(m, 1)`.
fn can_talk(m: &MarioRuntime) -> bool {
    m.action & ACT_FLAG_IDLE != 0 || m.action == ACT_WALKING
}

/// `check_npc_talk`: A or B near an `INT_SUBTYPE_NPC` object Mario faces (within a half turn).
/// Returns true when the talk starts (Mario waits for the dialog).
pub fn check_npc_talk(m: &mut MarioRuntime, id: ObjId) -> bool {
    let pressed = m.pending_pressed & (A_BUTTON | B_BUTTON) != 0;
    if pressed && can_talk(m) {
        let o = &m.objects[id];
        let to_object = atan2s(o.pos[2] - m.pos[2], o.pos[0] - m.pos[0]) as f64;
        let facing = to_s16(to_object - m.face_angle[1]);
        if (-0x4000..=0x4000).contains(&facing) {
            m.objects[id].interact_status = INT_STATUS_INTERACTED;
            m.interact_obj = Some(id);
            m.used_obj = Some(id);
            push_mario_out_of_object(m, id, -10.0);
            return m.set_action(ACT_WAITING_FOR_DIALOG, 0.0);
        }
    }
    push_mario_out_of_object(m, id, -10.0);
    false
}

pub const DIALOG_STATUS_ENABLE_TIME_STOP: i32 = 0;
pub const DIALOG_STATUS_INTERRUPT: i32 = 1;
pub const DIALOG_STATUS_START_DIALOG: i32 = 2;
pub const DIALOG_STATUS_STOP_DIALOG: i32 = 3;

/// `cur_obj_update_dialog_with_cutscene(actionArg, DIALOG_FLAG_TURN_TO_MARIO, CUTSCENE_DIALOG, dialogID)`.
/// `state_var` names the `oDialogState` var of the object. Returns true on the frame the whole
/// exchange is over (the original returns `DIALOG_RESPONSE_YES`/`NO`, nonzero).
pub fn update_dialog_with_cutscene(m: &mut MarioRuntime, id: ObjId, state_var: &str, action_arg: i32, turn_to_mario: bool, dialog_id: u32) -> bool {
    let state = m.objects[id].var(state_var) as i32;
    match state {
        DIALOG_STATUS_ENABLE_TIME_STOP => {
            // BUGFIX_DIALOG_TIME_STOP is off in the US build: only the health check.
            if m.health >= 0x100 as f64 {
                m.objects[id].set_var(state_var, 1.0);
                m.objects[id].set_var("dialogResponse", 0.0);
            }
        }
        DIALOG_STATUS_INTERRUPT => {
            let mut done_turning = true;
            if turn_to_mario {
                let to_mario = atan2s(m.pos[2] - m.objects[id].pos[2], m.pos[0] - m.objects[id].pos[0]) as f64;
                done_turning = crate::behaviors::g1::rotate_yaw_toward(&mut m.objects[id], to_mario, 0x800 as f64);
                // Failsafe if it takes more than 33 frames.
                if m.objects[id].var("dialogResponse") >= 33.0 {
                    done_turning = true;
                }
            }
            if set_mario_npc_dialog(m, id, action_arg) == MARIO_DIALOG_STATUS_SPEAK && done_turning {
                m.objects[id].set_var("dialogResponse", 0.0);
                m.objects[id].set_var(state_var, 2.0);
            } else {
                let r = m.objects[id].var("dialogResponse") + 1.0;
                m.objects[id].set_var("dialogResponse", r);
            }
        }
        DIALOG_STATUS_START_DIALOG => {
            // cutscene_object_with_dialog(CUTSCENE_DIALOG, o, dialogID): open the box, wait for it to close.
            if m.objects[id].var("dialogOpened") == 0.0 {
                m.objects[id].set_var("dialogOpened", 1.0);
                m.cut.dialog_open = true;
                m.cut.cue(CutCue::Dialog(dialog_id));
            } else if !m.cut.dialog_open {
                m.objects[id].set_var("dialogOpened", 0.0);
                m.objects[id].set_var(state_var, 3.0);
            }
        }
        _ => {
            // DIALOG_STATUS_STOP_DIALOG
            if m.action != ACT_READING_NPC_DIALOG {
                m.objects[id].set_var(state_var, 0.0);
                return true;
            }
            set_mario_npc_dialog(m, id, MARIO_DIALOG_STOP);
        }
    }
    false
}

/// `object_facing_mario`-free helper kept for the talk range checks of NPC hooks.
pub fn ready(m: &MarioRuntime) -> bool {
    ready_for_npc_dialog(m)
}

/// `DIALOG_FLAG_*` (object_constants.h).
pub const DIALOG_FLAG_TEXT_DEFAULT: u32 = 1 << 1;
pub const DIALOG_FLAG_TEXT_RESPONSE: u32 = 1 << 2;
pub const DIALOG_FLAG_TIME_STOP_ENABLED: u32 = 1 << 4;
pub const DIALOG_STATUS_DISABLE_TIME_STOP: i32 = 4;
/// `DIALOG_RESPONSE_NOT_DEFINED`: what a plain (choice-less) dialog reports.
pub const DIALOG_RESPONSE_NOT_DEFINED: i32 = 3;

/// object_helpers.c `cur_obj_update_dialog(actionArg, dialogFlags, dialogID, 0)` for object `id`, its
/// `oDialogState` / `oDialogResponse` kept in the vars `dialogState` / `dialogResult`. Returns the response
/// (nonzero) on the frame the exchange is over: `DIALOG_RESPONSE_YES` / `_NO` for a
/// [`DIALOG_FLAG_TEXT_RESPONSE`] box, [`DIALOG_RESPONSE_NOT_DEFINED`] otherwise. Time stop is not modelled
/// (as in [`update_dialog_with_cutscene`]); the US build's `DIALOG_STATUS_ENABLE_TIME_STOP` only checks health.
pub fn update_dialog(m: &mut MarioRuntime, id: ObjId, action_arg: i32, flags: u32, dialog_id: u32) -> i32 {
    use crate::cutscene_ext::DIALOG_RESPONSE_NONE;
    let state = m.objects[id].var("dialogState") as i32;
    let mut response = DIALOG_RESPONSE_NONE;
    let end = |m: &mut MarioRuntime, result: i32| {
        // cur_obj_end_dialog
        m.objects[id].set_var("dialogResult", result as f64);
        m.objects[id].set_var("dialogState", DIALOG_STATUS_DISABLE_TIME_STOP as f64);
        if flags & DIALOG_FLAG_TIME_STOP_ENABLED == 0 {
            set_mario_npc_dialog(m, id, MARIO_DIALOG_STOP);
        }
    };
    match state {
        DIALOG_STATUS_ENABLE_TIME_STOP => {
            if m.health >= 0x100 as f64 {
                m.objects[id].set_var("dialogState", DIALOG_STATUS_INTERRUPT as f64);
            }
        }
        DIALOG_STATUS_INTERRUPT => {
            if set_mario_npc_dialog(m, id, action_arg) == MARIO_DIALOG_STATUS_SPEAK {
                m.objects[id].set_var("dialogState", DIALOG_STATUS_START_DIALOG as f64);
            }
        }
        DIALOG_STATUS_START_DIALOG => {
            if flags & DIALOG_FLAG_TEXT_RESPONSE != 0 {
                m.cut.dialog_response = DIALOG_RESPONSE_NONE;
                m.cut.dialog_open = true;
                m.cut.cue(CutCue::DialogWithResponse(dialog_id));
            } else if flags & DIALOG_FLAG_TEXT_DEFAULT != 0 {
                m.cut.dialog_open = true;
                m.cut.cue(CutCue::Dialog(dialog_id));
            }
            m.objects[id].set_var("dialogState", DIALOG_STATUS_STOP_DIALOG as f64);
        }
        DIALOG_STATUS_STOP_DIALOG => {
            if flags & DIALOG_FLAG_TEXT_RESPONSE != 0 {
                if m.cut.dialog_response != DIALOG_RESPONSE_NONE {
                    let r = m.cut.dialog_response;
                    end(m, r);
                }
            } else if flags & DIALOG_FLAG_TEXT_DEFAULT != 0 {
                if !m.cut.dialog_open {
                    end(m, DIALOG_RESPONSE_NOT_DEFINED);
                }
            } else {
                end(m, DIALOG_RESPONSE_NOT_DEFINED);
            }
        }
        _ => {
            // DIALOG_STATUS_DISABLE_TIME_STOP
            if m.action != ACT_READING_NPC_DIALOG || flags & DIALOG_FLAG_TIME_STOP_ENABLED != 0 {
                response = m.objects[id].var("dialogResult") as i32;
                m.objects[id].set_var("dialogState", DIALOG_STATUS_ENABLE_TIME_STOP as f64);
            }
        }
    }
    response
}
