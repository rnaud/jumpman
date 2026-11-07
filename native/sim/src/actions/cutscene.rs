//! Port of `src/sim/actions/cutscene.ts` (deaths, spawn/exit airborne, star dance, signs (`ACT_END_PEACH_CUTSCENE` / `ACT_JUMBO_STAR_CUTSCENE` are routed to ending.rs by the dispatcher)).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`CUTSCENE_ACTIONS`) to [`cutscene_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `deathFrame` -> `death_frame`
//! - `actQuicksandDeath` -> `act_quicksand_death`
//! - `actStandingDeath` -> `act_standing_death`
//! - `actDeathOnBack` -> `act_death_on_back`
//! - `actDeathOnStomach` -> `act_death_on_stomach`
//! - `spawnSpinAirborne` -> `spawn_spin_airborne`
//! - `starExitAirborne` -> `star_exit_airborne`
//! - `deathExit` -> `death_exit`
//! - `warpLanding` -> `warp_landing`
//! - `readyForNpcDialog` -> `ready_for_npc_dialog`
//! - `readingNpcDialog` -> `reading_npc_dialog`
//! - `groundStarDance` -> `ground_star_dance`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn cutscene_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
