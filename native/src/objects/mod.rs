//! Course objects glue (`courseObjects.ts` and the `src/game` object files): moving platforms,
//! object colliders, cannons, goombas, signposts and NPC dialogs. Each area is its own Bevy
//! plugin; the pure logic lives in `game/` (platform, goomba, ...) so it can be unit tested.
pub mod cannons;
pub mod colliders;
pub mod debug;
pub mod dialog;
pub mod goombas;
pub mod mushroom;
pub mod platforms;

use crate::progress::CourseRebuild;
use bevy::prelude::*;

/// Register a `fn(&mut World)` that rebuilds a module's course state after each course switch
/// (`progress::CourseRebuild`); the module's `Startup` system covers the first course.
pub fn register_rebuild(app: &mut App, hook: fn(&mut World)) {
    app.init_resource::<CourseRebuild>();
    app.world_mut().resource_mut::<CourseRebuild>().push(hook);
}

pub struct ObjectsPlugin;

impl Plugin for ObjectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((platforms::PlatformsPlugin, colliders::CollidersPlugin, goombas::GoombasPlugin, dialog::DialogPlugin, cannons::CannonsPlugin, debug::DebugHooksPlugin, mushroom::MushroomPlugin));
    }
}
