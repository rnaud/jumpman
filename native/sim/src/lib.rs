//! Rust port of the Jumpman SM64 simulation (src/sim in the TypeScript game).
//!
//! Everything uses `f64` like the TypeScript original so outputs can be compared
//! bit-for-bit against fixtures generated from it.
pub mod collision;
pub mod math;
pub mod surfaces;
