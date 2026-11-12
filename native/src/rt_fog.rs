//! Distance fog for the ray-traced tier (see `rt_fog.wesl`): Solari has no fog, so the camera's `DistanceFog` is mirrored
//! into a screen-space overlay that blends the fog colour by the depth-prepass distance. Only added when ray tracing is
//! active; the other tiers keep Bevy's own per-material fog.
use crate::camera::MainCamera;
use bevy::asset::{load_internal_asset, weak_handle};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{DistanceFog, FogFalloff, Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, CompareFunction, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

const FOG_SHADER: Handle<Shader> = weak_handle!("0b6d3c52-7a55-4f0e-8d64-2a1c5f9e7b31");

/// The sky dome sits at 300 world units; anything farther than this is sky and never fogged.
const SKY_DISTANCE: f32 = 150.0;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, PartialEq)]
pub struct RtFogMaterial {
    #[uniform(0)]
    color: Vec4,
    #[uniform(1)]
    params: Vec4,
}

impl RtFogMaterial {
    /// The overlay equivalent of a camera's fog (`None`: no fog).
    pub fn from_fog(fog: Option<&DistanceFog>) -> Self {
        let Some(fog) = fog else { return Self { color: Vec4::ZERO, params: Vec4::new(0.0, 1.0, SKY_DISTANCE, 0.0) } };
        let c = fog.color.to_linear();
        let (start, end) = match fog.falloff {
            FogFalloff::Linear { start, end } => (start, end),
            // Visibility distance -> where the exponential reaches ~95%: a linear ramp over the same span.
            FogFalloff::Exponential { density } => (0.0, 3.0 / density.max(1e-4)),
            FogFalloff::ExponentialSquared { density } => (0.0, 2.45 / density.max(1e-4)),
            FogFalloff::Atmospheric { extinction, .. } => (0.0, 3.0 / extinction.x.max(1e-4)),
        };
        Self { color: Vec4::new(c.red, c.green, c.blue, 1.0), params: Vec4::new(start, end, SKY_DISTANCE, 1.0) }
    }
}

impl Material for RtFogMaterial {
    fn vertex_shader() -> ShaderRef {
        FOG_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        FOG_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(false);
            ds.depth_compare = Some(CompareFunction::Always);
        }
        Ok(())
    }
}

#[derive(Component)]
struct FogOverlay;

pub struct RtFogPlugin;

impl Plugin for RtFogPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, FOG_SHADER, "rt_fog.wesl", Shader::from_wesl);
        app.add_plugins(MaterialPlugin::<RtFogMaterial>::default()).add_systems(Startup, spawn_overlay).add_systems(PostUpdate, sync_overlay);
    }
}

fn spawn_overlay(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<RtFogMaterial>>) {
    commands.spawn((
        FogOverlay,
        Mesh3d(meshes.add(Triangle3d::new(Vec3::ZERO, Vec3::X, Vec3::Y))),
        MeshMaterial3d(materials.add(RtFogMaterial::from_fog(None))),
        // Transparent items sort far to near by distance to the entity: parked far ahead of the camera the overlay draws
        // before every blended sprite, which then fogs itself.
        Transform::default(),
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn sync_overlay(
    camera: Single<(&GlobalTransform, Option<&DistanceFog>), With<MainCamera>>,
    mut overlay: Single<(&mut Transform, &MeshMaterial3d<RtFogMaterial>), With<FogOverlay>>,
    mut materials: ResMut<Assets<RtFogMaterial>>,
) {
    overlay.0.translation = camera.0.translation() + camera.0.forward().as_vec3() * 2000.0;
    let next = RtFogMaterial::from_fog(camera.1);
    if materials.get(&overlay.1 .0).is_some_and(|m| *m != next) {
        if let Some(mut m) = materials.get_mut(&overlay.1 .0) {
            *m = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_fog_maps_to_the_ramp() {
        let fog = DistanceFog { color: Color::srgb(1.0, 1.0, 1.0), falloff: FogFalloff::Linear { start: 100.0, end: 260.0 }, ..default() };
        let m = RtFogMaterial::from_fog(Some(&fog));
        assert_eq!((m.params.x, m.params.y, m.params.w), (100.0, 260.0, 1.0));
        assert!(m.color.x > 0.99);
    }

    #[test]
    fn no_fog_means_no_overlay() {
        assert_eq!(RtFogMaterial::from_fog(None).params.w, 0.0);
    }
}
