//! Port of `src/sim/actions/moving.ts` (walking, braking, sliding, landings, hold-walking, riding shell on the ground).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`MOVING_ACTIONS`) to [`moving_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `applySlopeAccel` -> `apply_slope_accel`
//! - `applyLandingAccel` -> `apply_landing_accel`
//! - `applySlopeDecel` -> `apply_slope_decel`
//! - `updateDeceleratingSpeed` -> `update_decelerating_speed`
//! - `updateWalkingSpeed` -> `update_walking_speed`
//! - `shouldBeginSliding` -> `should_begin_sliding`
//! - `analogStickHeldBack` -> `analog_stick_held_back`
//! - `checkGroundDiveOrPunch` -> `check_ground_dive_or_punch`
//! - `beginBrakingAction` -> `begin_braking_action`
//! - `beginWalkingAction` -> `begin_walking_action`
//! - `slideBonk` -> `slide_bonk`
//! - `setTripleJumpAction` -> `set_triple_jump_action`
//! - `sin` -> `sin`
//! - `cos` -> `cos`
//! - `updateSlidingAngle` -> `update_sliding_angle`
//! - `updateSliding` -> `update_sliding`
//! - `commonSlideAction` -> `common_slide_action`
//! - `commonSlideActionWithJump` -> `common_slide_action_with_jump`
//! - `actWalking` -> `act_walking`
//! - `actMovePunching` -> `act_move_punching`
//! - `actTurningAround` -> `act_turning_around`
//! - `actFinishTurningAround` -> `act_finish_turning_around`
//! - `actBraking` -> `act_braking`
//! - `actDecelerating` -> `act_decelerating`
//! - `actCrawling` -> `act_crawling`
//! - `actCrouchSlide` -> `act_crouch_slide`
//! - `actButtSlide` -> `act_butt_slide`
//! - `actStomachSlide` -> `act_stomach_slide`
//! - `actDiveSlide` -> `act_dive_slide`
//! - `actSlideKickSlide` -> `act_slide_kick_slide`
//! - `commonLandingCancels` -> `common_landing_cancels`
//! - `commonLandingAction` -> `common_landing_action`
//! - `actJumpLand` -> `act_jump_land`
//! - `actFreefallLand` -> `act_freefall_land`
//! - `actSideFlipLand` -> `act_side_flip_land`
//! - `actDoubleJumpLand` -> `act_double_jump_land`
//! - `actTripleJumpLand` -> `act_triple_jump_land`
//! - `actLongJumpLand` -> `act_long_jump_land`
//! - `actBackflipLand` -> `act_backflip_land`
//! - `holdThrowAction` -> `hold_throw_action`
//! - `actHoldWalking` -> `act_hold_walking`
//! - `actHoldDecelerating` -> `act_hold_decelerating`
//! - `holdLanding` -> `hold_landing`
//! - `actHoldJumpLand` -> `act_hold_jump_land`
//! - `actHoldFreefallLand` -> `act_hold_freefall_land`
//! - `groundKnockback` -> `ground_knockback`
//! - `actRidingShellGround` -> `act_riding_shell_ground`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn moving_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
