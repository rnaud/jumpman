//! Port of `src/sim/actions/holding.ts` (picking up, placing down, throwing, Bowser grab. Dispatched from the object group).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`(entries of OBJECT_ACTIONS)`) to [`holding_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `actPickingUp` -> `act_picking_up`
//! - `actPlacingDown` -> `act_placing_down`
//! - `actThrowing` -> `act_throwing`
//! - `actPickingUpBowser` -> `act_picking_up_bowser`
//! - `actHoldingBowser` -> `act_holding_bowser`
//! - `actReleasingBowser` -> `act_releasing_bowser`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn holding_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
