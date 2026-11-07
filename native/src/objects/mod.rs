//! Course objects glue (`courseObjects.ts` and the `src/game` object files): moving platforms,
//! object colliders, cannons, goombas, signposts and NPC dialogs. Each area is its own Bevy
//! plugin; the pure logic lives in `game/` (platform, goomba, ...) so it can be unit tested.
pub mod cannons;
pub mod colliders;
pub mod debug;
pub mod dialog;
pub mod goombas;
pub mod platforms;
pub mod wing_switch;

use bevy::prelude::*;

pub struct ObjectsPlugin;

impl Plugin for ObjectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((platforms::PlatformsPlugin, colliders::CollidersPlugin, goombas::GoombasPlugin, dialog::DialogPlugin, cannons::CannonsPlugin, debug::DebugHooksPlugin, wing_switch::WingSwitchPlugin));
    }
}
