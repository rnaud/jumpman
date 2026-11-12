//! Visual effects and look: particles, animated water, sky/fog, shadows, Mario cap/eye/metal
//! variants and display settings. One Bevy plugin per area, composed by [`FxPlugin`].
pub mod anchor;
pub mod display;
pub mod juice;
pub mod mario_looks;
pub mod particles;
pub mod pickup_fx;
pub mod render_scale;
pub mod shadows;
pub mod sky;
pub mod water;

use bevy::prelude::*;

/// World-space effect sprites (particles, sparkles, rings). They are root entities but not part of the course: the
/// experiments layer leaves them alone when a mode hides or rescales the course (`experiments::apply_course_state`).
#[derive(Component)]
pub struct FxSprite;

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((display::DisplayPlugin, render_scale::RenderScalePlugin, particles::ParticlesPlugin, water::WaterPlugin, sky::SkyPlugin, shadows::ShadowsPlugin, mario_looks::MarioLooksPlugin));
        app.add_plugins(juice::JuicePlugin);
        app.add_plugins(pickup_fx::PickupFxPlugin);
        app.add_systems(PostUpdate, matte_specular);
    }
}

/// Bevy 0.20 renders a fully rough (roughness 1) dielectric with a visible specular lobe at the default reflectance
/// of 0.5: Mario's overalls and cap picked up per-triangle highlights that the 0.16 build never showed (the N64 look
/// is purely diffuse). Matte, non-metallic materials get zero reflectance as they appear, which restores the old
/// smooth, darker shading (Mario, Link, Banjo and the glTF scenery all use roughness 1).
fn matte_specular(mut events: MessageReader<AssetEvent<StandardMaterial>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let ids: Vec<_> = events.read().filter_map(|e| match e {
        AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(*id),
        _ => None,
    }).collect();
    for id in ids {
        let Some(m) = materials.get(id) else { continue };
        if m.perceptual_roughness >= 0.99 && m.metallic < 0.01 && m.reflectance > 0.0 && !m.unlit {
            if let Some(mut m) = materials.get_mut(id) {
                m.reflectance = 0.0;
            }
        }
    }
}
