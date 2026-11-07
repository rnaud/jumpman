//! Port of `src/sim/actions/stationary.ts` (idle, crouching, landing stops, hold-idle, sleeping).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`STATIONARY_ACTIONS`) to [`stationary_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `checkCommonIdleCancels` -> `check_common_idle_cancels`
//! - `actLavaBoostLand` -> `act_lava_boost_land`
//! - `actIdle` -> `act_idle`
//! - `actStartSleeping` -> `act_start_sleeping`
//! - `actSleeping` -> `act_sleeping`
//! - `actWakingUp` -> `act_waking_up`
//! - `actCrouching` -> `act_crouching`
//! - `actStartCrouching` -> `act_start_crouching`
//! - `actStopCrouching` -> `act_stop_crouching`
//! - `actStartCrawling` -> `act_start_crawling`
//! - `actStopCrawling` -> `act_stop_crawling`
//! - `stoppingStep` -> `stopping_step`
//! - `actBrakingStop` -> `act_braking_stop`
//! - `actButtSlideStop` -> `act_butt_slide_stop`
//! - `actSlideKickSlideStop` -> `act_slide_kick_slide_stop`
//! - `checkCommonLandingCancels` -> `check_common_landing_cancels`
//! - `landingStep` -> `landing_step`
//! - `actJumpLandStop` -> `act_jump_land_stop`
//! - `actDoubleJumpLandStop` -> `act_double_jump_land_stop`
//! - `actFreefallLandStop` -> `act_freefall_land_stop`
//! - `actSideFlipLandStop` -> `act_side_flip_land_stop`
//! - `actTripleJumpLandStop` -> `act_triple_jump_land_stop`
//! - `actBackflipLandStop` -> `act_backflip_land_stop`
//! - `actLongJumpLandStop` -> `act_long_jump_land_stop`
//! - `actGroundPoundLand` -> `act_ground_pound_land`
//! - `checkCommonHoldIdleCancels` -> `check_common_hold_idle_cancels`
//! - `actHoldIdle` -> `act_hold_idle`
//! - `checkCommonHoldActionExits` -> `check_common_hold_action_exits`
//! - `holdLandingStop` -> `hold_landing_stop`
//! - `actHoldJumpLandStop` -> `act_hold_jump_land_stop`
//! - `actHoldFreefallLandStop` -> `act_hold_freefall_land_stop`
//! - `actAirThrowLand` -> `act_air_throw_land`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn stationary_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
