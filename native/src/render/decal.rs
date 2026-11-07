//! N64 decal render modes (`LAYER_TRANSPARENT_DECAL`: the castle lobby sun rug, tree shadows,
//! floor emblems) draw coplanar overlays with a depth-compare slack. glTF has no such field and
//! Bevy's `depth_bias` only reorders sorting, so alpha-blended level geometry is swapped to a
//! `StandardMaterial` extended with a real raster depth bias. Without it the blended quad
//! z-fights the floor it lies on and drops out in bands (worst on mobile GPUs).
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{AsBindGroup, DepthBiasState, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use std::collections::HashMap;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct DecalExt {}

impl MaterialExtension for DecalExt {
    fn specialize(_: &MaterialExtensionPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialExtensionKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            // Reverse-Z: larger depth is nearer, so a positive bias pulls the decal forward.
            ds.bias = decal_bias();
        }
        Ok(())
    }
}

pub fn decal_bias() -> DepthBiasState {
    DepthBiasState { constant: 64, slope_scale: 4.0, clamp: 0.0 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn decal_bias_pulls_toward_the_camera_under_reverse_z() {
        let b = super::decal_bias();
        assert!(b.constant > 0 && b.slope_scale > 0.0);
    }
}

pub type DecalMaterial = ExtendedMaterial<StandardMaterial, DecalExt>;

/// Marks a spawned level scene whose blended materials should be biased.
#[derive(Component)]
pub struct DecalScene;

#[derive(Component)]
struct DecalDone;

pub struct DecalPlugin;

impl Plugin for DecalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<DecalMaterial>::default()).add_systems(Update, bias_decals);
    }
}

fn bias_decals(
    scenes: Query<Entity, (With<DecalScene>, Without<DecalDone>)>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    std: Res<Assets<StandardMaterial>>,
    mut ext: ResMut<Assets<DecalMaterial>>,
    mut commands: Commands,
    mut cache: Local<HashMap<AssetId<StandardMaterial>, Handle<DecalMaterial>>>,
) {
    for scene in &scenes {
        let mut seen = false;
        for c in children.iter_descendants(scene) {
            let Ok(h) = handles.get(c) else { continue };
            seen = true;
            let Some(m) = std.get(&h.0) else { continue };
            if !matches!(m.alpha_mode, AlphaMode::Blend) {
                continue;
            }
            let new = cache.entry(h.0.id()).or_insert_with(|| ext.add(DecalMaterial { base: m.clone(), extension: DecalExt {} })).clone();
            commands.entity(c).remove::<MeshMaterial3d<StandardMaterial>>().insert(MeshMaterial3d(new));
        }
        if seen {
            commands.entity(scene).insert(DecalDone);
        }
    }
}
