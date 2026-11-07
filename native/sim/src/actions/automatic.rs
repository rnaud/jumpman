//! Port of `src/sim/actions/automatic.ts` (ledge grab/climb, hanging, poles (`ACT_IN_CANNON` is routed to cannon.rs by the dispatcher)).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`AUTOMATIC_ACTIONS`) to [`automatic_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `sin` -> `sin`
//! - `cos` -> `cos`
//! - `letGoOfLedge` -> `let_go_of_ledge`
//! - `actLedgeGrab` -> `act_ledge_grab`
//! - `actLedgeClimbSlow` -> `act_ledge_climb_slow`
//! - `actLedgeClimbFast` -> `act_ledge_climb_fast`
//! - `canHang` -> `can_hang`
//! - `actStartHanging` -> `act_start_hanging`
//! - `actHanging` -> `act_hanging`
//! - `actHangMoving` -> `act_hang_moving`
//! - `poleTop` -> `pole_top`
//! - `setPolePosition` -> `set_pole_position`
//! - `leavePole` -> `leave_pole`
//! - `actGrabPole` -> `act_grab_pole`
//! - `actHoldingPole` -> `act_holding_pole`
//! - `actClimbingPole` -> `act_climbing_pole`
//! - `actTopOfPoleTransition` -> `act_top_of_pole_transition`
//! - `actTopOfPole` -> `act_top_of_pole`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn automatic_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
