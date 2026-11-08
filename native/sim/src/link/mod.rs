//! Link (Ocarina of Time) beyond the legacy `characters::step_link`: inventory, weapons,
//! projectiles, the ocarina and the faithful move set. See `native/LINK_CONTROLLER.md`.
pub mod combat;
pub mod items;
pub mod moves;
pub mod ocarina;
pub mod projectiles;

pub use moves::{LinkEvent, LinkExt, Mv};
