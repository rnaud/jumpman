//! Lighting quality tiers. `Standard` is the classic look; `Enhanced` adds screen-space ambient
//! occlusion, soft (Gaussian-filtered) shadows with a larger shadow map, and FXAA (SSAO needs MSAA
//! off). This is NOT ray tracing. Hardware ray tracing is not possible in this build: Bevy 0.16.1 has
//! no `bevy_solari`, and wgpu 24's Metal backend stubs out ray queries and acceleration structures
//! (see `native/README.md`, "Ray tracing"). The UI therefore never offers a ray-tracing tier.
//!
//! Test hook: `JUMPMAN_RT=off|enhanced|rt` overrides the setting without persisting it. `rt` asks
//! for hardware ray tracing, logs why it is unavailable and falls back to `enhanced`.
use crate::camera::MainCamera;
use crate::shell::settings::{Lighting, Settings};
use bevy::core_pipeline::fxaa::Fxaa;
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::pbr::{DirectionalLightShadowMap, ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel, ShadowFilteringMethod};
use bevy::prelude::*;

/// One-line status shown under the lighting row in Settings.
pub const RAY_TRACING_NOTE: &str = "Hardware ray tracing: unavailable (Bevy 0.16 / wgpu 24 Metal has no ray queries).";

/// Why hardware ray tracing is unavailable (logged when `JUMPMAN_RT=rt`).
pub const RAY_TRACING_REASON: &str = "no bevy_solari in Bevy 0.16.1 and wgpu-hal 24 Metal does not implement EXPERIMENTAL_RAY_QUERY / acceleration structures";

/// Parse `JUMPMAN_RT`. `true` in the second slot means ray tracing was requested.
pub fn parse_env(v: &str) -> Option<(Lighting, bool)> {
    match v.trim().to_ascii_lowercase().as_str() {
        "off" | "standard" | "0" => Some((Lighting::Standard, false)),
        "enhanced" | "on" | "1" => Some((Lighting::Enhanced, false)),
        "rt" | "raytracing" | "ray" => Some((Lighting::Enhanced, true)),
        _ => None,
    }
}

/// The tier to render with: the environment hook if set, else the persisted setting.
pub fn effective(settings: &Settings) -> Lighting {
    std::env::var("JUMPMAN_RT").ok().and_then(|v| parse_env(&v)).map_or(settings.lighting, |(l, _)| l)
}

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("JUMPMAN_RT").ok().and_then(|v| parse_env(&v)).is_some_and(|(_, rt)| rt) {
            warn!("JUMPMAN_RT=rt: hardware ray tracing is unavailable ({RAY_TRACING_REASON}); using enhanced lighting");
        }
        app.add_systems(Update, apply);
    }
}

fn apply(
    mut commands: Commands,
    settings: Res<Settings>,
    cameras: Query<(Entity, &Msaa, Has<ScreenSpaceAmbientOcclusion>), With<MainCamera>>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut saved_msaa: Local<Option<Msaa>>,
) {
    let want = effective(&settings);
    let size = if want == Lighting::Enhanced { 4096 } else { 2048 };
    if shadow_map.size != size {
        shadow_map.size = size;
    }
    for (e, msaa, has) in &cameras {
        match (want, has) {
            (Lighting::Enhanced, false) => {
                saved_msaa.get_or_insert(*msaa);
                commands.entity(e).insert((
                    Msaa::Off,
                    DepthPrepass,
                    NormalPrepass,
                    ScreenSpaceAmbientOcclusion { quality_level: ScreenSpaceAmbientOcclusionQualityLevel::High, constant_object_thickness: 0.5 },
                    ShadowFilteringMethod::Gaussian,
                    Fxaa::default(),
                ));
            }
            (Lighting::Standard, true) => {
                commands.entity(e).remove::<(DepthPrepass, NormalPrepass, ScreenSpaceAmbientOcclusion, Fxaa)>().insert((saved_msaa.take().unwrap_or(Msaa::Sample4), ShadowFilteringMethod::default()));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_values_parse_and_rt_falls_back_to_enhanced() {
        assert_eq!(parse_env("off"), Some((Lighting::Standard, false)));
        assert_eq!(parse_env("Enhanced"), Some((Lighting::Enhanced, false)));
        assert_eq!(parse_env("rt"), Some((Lighting::Enhanced, true)));
        assert_eq!(parse_env("bogus"), None);
    }
}
