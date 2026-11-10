//! Rust port of the Jumpman SM64 simulation (src/sim in the TypeScript game).
//!
//! Everything uses `f64` like the TypeScript original so outputs can be compared
//! bit-for-bit against fixtures generated from it.
// Constants are copied verbatim from the C/TS originals (bit-for-bit goldens) and some branch
// shapes mirror the decomp, so these style lints are off.
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::needless_range_loop, clippy::if_same_then_else)]
pub mod actions;
pub mod behaviors;
pub mod banjo_ext;
pub mod bk_enemy;
pub mod characters;
pub mod collision;
pub mod cutscene_ext;
pub mod mario_anim_meta;
pub mod sm64_dialogs;
pub mod loot_coin;
pub mod js_math;
pub mod link;
pub mod mario_helpers;
pub mod mario_r2a;
pub mod mario_runtime;
pub mod mario_step;
pub mod math;
pub mod object;
pub mod object_manager;
pub mod solids;
pub mod oot_blocks;
pub mod oot_doors;
pub mod oot_hazards;
pub mod oot_heat;
pub mod oot_movers;
pub mod oot_rooms;
pub mod oot_tags;
pub mod oot_water;
pub mod oot_enemy;
pub mod oot_kankyo;
pub mod sm64cam;
pub mod spawned_star;
pub mod state;
pub mod state_consts;
pub mod water_consts;
pub mod mario_anim;
pub mod mario_anim_data;
pub mod surfaces;
