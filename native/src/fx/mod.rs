//! Visual effects and look: particles, animated water, sky/fog, shadows, Mario cap/eye/metal
//! variants and display settings. One Bevy plugin per area, composed by [`FxPlugin`].
pub mod display;
pub mod mario_looks;
pub mod particles;
pub mod shadows;
pub mod sky;
pub mod water;

use bevy::prelude::*;

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((display::DisplayPlugin, particles::ParticlesPlugin, water::WaterPlugin, sky::SkyPlugin, shadows::ShadowsPlugin, mario_looks::MarioLooksPlugin));
    }
}
