//! Port of `src/sim/actions/object.ts` (punching and the table entries for the pick up / throw / Bowser actions (those bodies live in holding.rs; the dispatcher also tries holding_action)).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`OBJECT_ACTIONS`) to [`object_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `setPunchAnimation` -> `set_punch_animation`
//! - `updatePunchSequence` -> `update_punch_sequence`
//! - `actPunching` -> `act_punching`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// Action table. STUB: no actions are handled yet.
pub fn object_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
