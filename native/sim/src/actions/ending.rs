//! Port of `src/sim/actions/ending.ts` (`ACT_JUMBO_STAR_CUTSCENE`, `ACT_END_PEACH_CUTSCENE`).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`(entries of CUTSCENE_ACTIONS)`) to [`ending_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `splineWeights` -> `spline_weights`
//! - `flightPath` -> `flight_path`
//! - `advance` -> `advance`
//! - `wingLanding` -> `wing_landing`
//! - `jumboStarCutscene` -> `jumbo_star_cutscene`
//! - `endPeachCutscene` -> `end_peach_cutscene`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn ending_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
