//! Port of `src/sim/actions/drowning.ts` (`ACT_DROWNING`, `ACT_WATER_DEATH`).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`SUBMERGED_ACTIONS`) to [`drowning_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `actDrowning` -> `act_drowning`
//! - `actWaterDeath` -> `act_water_death`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn drowning_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
