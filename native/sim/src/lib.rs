//! Rust port of the Jumpman SM64 simulation (src/sim in the TypeScript game).
//!
//! Everything uses `f64` like the TypeScript original so outputs can be compared
//! bit-for-bit against fixtures generated from it.
pub mod actions;
pub mod behaviors;
pub mod characters;
pub mod collision;
pub mod loot_coin;
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
pub mod v8_math;
