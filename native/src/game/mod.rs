//! Engine-free game layer: course catalogue/loading and the fixed-step runner. Nothing in here
//! touches Bevy types except `runner` (a thin `Resource` wrapper), so the logic is unit-testable.
#![allow(dead_code)] // public surface for features that land later (warps, doors, save data)
pub mod course_data;
pub mod cannons;
pub mod collider;
pub mod signpost;
pub mod course_extras;
pub mod goomba;
pub mod courses;
pub mod platform;
pub mod three_math;
pub mod mk64;
pub mod mushroom2d;
pub mod native_collision;
#[cfg(test)]
mod hookshot_marks_tests;
pub mod playground;
pub mod runner;
pub mod sm64_animation;
pub mod sm64_upgrade;
pub mod sm64_areas;
pub mod world;
#[cfg(test)]
mod longrun_tests;
#[cfg(test)]
mod game_tests;
#[cfg(test)]
mod game2_tests;
#[cfg(test)]
mod warps_tests;
#[cfg(test)]
mod g2_tests;
#[cfg(test)]
mod part_b_tests;
#[cfg(test)]
mod last_stars_tests;
#[cfg(test)]
mod bk_tests;
#[cfg(test)]
mod bk_warp_tests;
#[cfg(test)]
mod sm64_warp_tests;
#[cfg(test)]
mod sm64_world_tests;
#[cfg(test)]
mod bk_totals_tests;
#[cfg(test)]
mod bk_debug_tests;
