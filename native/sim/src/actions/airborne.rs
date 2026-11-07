//! Port of `src/sim/actions/airborne.ts` (jumps, freefall, flying, knockback, ground pound, kicks, riding-shell jump (`ACT_SHOT_FROM_CANNON` is routed to cannon.rs by the dispatcher)).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`AIRBORNE_ACTIONS`) to [`airborne_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `sin` -> `sin`
//! - `cos` -> `cos`
//! - `updateAirWithoutTurn` -> `update_air_without_turn`
//! - `updateAirWithTurn` -> `update_air_with_turn`
//! - `checkFallDamage` -> `check_fall_damage`
//! - `checkWallKick` -> `check_wall_kick`
//! - `approachS32` -> `approach_s32`
//! - `updateFlying` -> `update_flying`
//! - `actFlying` -> `act_flying`
//! - `actRidingShellJump` -> `act_riding_shell_jump`
//! - `commonAirActionStep` -> `common_air_action_step`
//! - `commonAirKnockbackStep` -> `common_air_knockback_step`
//! - `checkKickOrDiveInAir` -> `check_kick_or_dive_in_air`
//! - `actLavaBoost` -> `act_lava_boost`
//! - `actJump` -> `act_jump`
//! - `actDoubleJump` -> `act_double_jump`
//! - `actTripleJump` -> `act_triple_jump`
//! - `actBackflip` -> `act_backflip`
//! - `actFreefall` -> `act_freefall`
//! - `actSideFlip` -> `act_side_flip`
//! - `actWallKickAir` -> `act_wall_kick_air`
//! - `actLongJump` -> `act_long_jump`
//! - `actDive` -> `act_dive`
//! - `actGroundPound` -> `act_ground_pound`
//! - `actSteepJump` -> `act_steep_jump`
//! - `actAirHitWall` -> `act_air_hit_wall`
//! - `actSoftBonk` -> `act_soft_bonk`
//! - `actSlideKick` -> `act_slide_kick`
//! - `actJumpKick` -> `act_jump_kick`
//! - `actButtSlideAir` -> `act_butt_slide_air`
//! - `airKnockback` -> `air_knockback`
//! - `actHoldJump` -> `act_hold_jump`
//! - `actHoldFreefall` -> `act_hold_freefall`
//! - `actAirThrow` -> `act_air_throw`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn airborne_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
