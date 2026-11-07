//! Rust port of the Jumpman SM64 simulation (src/sim in the TypeScript game).
//!
//! Everything uses `f64` like the TypeScript original so outputs can be compared
//! bit-for-bit against fixtures generated from it.
// Constants are copied verbatim from the C/TS originals (bit-for-bit goldens) and some branch
// shapes mirror the decomp, so these style lints are off.
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::needless_range_loop, clippy::if_same_then_else)]
pub mod actions;
pub mod behaviors;
pub mod characters;
pub mod collision;
pub mod loot_coin;
pub mod js_math;
pub mod mario_helpers;
pub mod mario_runtime;
pub mod mario_step;
pub mod math;
pub mod object;
pub mod object_manager;
pub mod spawned_star;
pub mod state;
pub mod state_consts;
pub mod surfaces;
