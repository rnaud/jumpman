//! Port of `src/sim/actions/cannon.ts` (`ACT_IN_CANNON` (in_cannon) and `ACT_SHOT_FROM_CANNON` (shot_from_cannon)).
//!
//! Fill this file in without touching `mario_runtime.rs`: add one `ACT_*` arm per entry of the
//! TS action table (`(entries of AUTOMATIC_ACTIONS / AIRBORNE_ACTIONS)`) to [`cannon_action`] and port the helpers below as plain
//! `pub fn xxx(m: &mut MarioRuntime) -> bool` (snake_case of the TS names). Return
//! `Some(changed)` for handled actions; `None` means "no handler here" and the dispatcher in
//! `actions/mod.rs` falls back exactly like the TS `runAction`.
//!
//! TS functions to port (source order):
//! - `inCannon` -> `in_cannon`
//! - `shotFromCannon` -> `shot_from_cannon`
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// The cannon aiming state (TS `CannonSeat`). Defined here because `MarioRuntime::cannon` holds it.
#[derive(Clone, Debug, Default)]
pub struct CannonSeat {
    pub home: Vec3,
    pub base_yaw: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub rise: f64,
    pub shake: f64,
    pub input_yaw: f64,
    pub cooldown: f64,
}

/// Action table. STUB: no actions are handled yet.
pub fn cannon_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        // ACT_EXAMPLE => Some(act_example(m)),
        _ => None,
    }
}
