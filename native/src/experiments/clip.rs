//! Renderer clipping plane (`renderer.clippingPlanes` in the TS): while `CameraOverride::clip`
//! is set, every lit mesh is swapped to a `StandardMaterial` extended with a world-space plane
//! that discards fragments on the wrong side; the originals come back when it is cleared.
use super::CameraOverride;
use crate::fx::sky::SkyDome;
use bevy::asset::{load_internal_asset, weak_handle};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::shader::ShaderRef;
use bevy::render::render_resource::{AsBindGroup, };
use std::collections::HashMap;

const CLIP_SHADER: Handle<Shader> = weak_handle!("6a1c0e8e-52f4-4d6e-9b53-0c7b1a2f9d11");

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct ClipExt {
    /// (normal.xyz, constant) in Bevy units; fragments with `dot(n, p) + w < 0` are discarded.
    #[uniform(100)]
    pub plane: Vec4,
}

impl MaterialExtension for ClipExt {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(CLIP_SHADER)
    }
}

pub type ClipMaterial = ExtendedMaterial<StandardMaterial, ClipExt>;

/// Marks meshes the clip plane must leave alone (the cut profile itself, the sky).
#[derive(Component)]
pub struct NoClip;

/// Remembers the material to restore.
#[derive(Component)]
struct ClipOrig(Handle<StandardMaterial>);

pub struct ClipPlugin;

impl Plugin for ClipPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, CLIP_SHADER, "clip.wesl", Shader::from_wesl);
        app.add_plugins(MaterialPlugin::<ClipMaterial>::default()).add_systems(PostUpdate, apply_clip.before(bevy::transform::TransformSystems::Propagate));
    }
}

type Swappable = (Without<NoClip>, Without<SkyDome>, Without<ClipOrig>);

#[allow(clippy::too_many_arguments)]
fn apply_clip(
    cam: Res<CameraOverride>,
    mut commands: Commands,
    std_materials: Res<Assets<StandardMaterial>>,
    mut clip_materials: ResMut<Assets<ClipMaterial>>,
    fresh: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Swappable>,
    swapped: Query<(Entity, &ClipOrig)>,
    mut cache: Local<HashMap<AssetId<StandardMaterial>, Handle<ClipMaterial>>>,
    mut last: Local<Option<Vec4>>,
) {
    let Some((normal, constant)) = cam.clip else {
        if !swapped.is_empty() {
            for (e, orig) in &swapped {
                commands.entity(e).insert(MeshMaterial3d(orig.0.clone())).remove::<(MeshMaterial3d<ClipMaterial>, ClipOrig)>();
            }
            cache.clear();
        }
        *last = None;
        return;
    };
    let plane = Vec4::new(normal.x as f32, normal.y as f32, normal.z as f32, constant as f32 * crate::render::SCALE);
    if *last != Some(plane) {
        for handle in cache.values() {
            if let Some(mut m) = clip_materials.get_mut(handle) {
                m.extension.plane = plane;
            }
        }
        *last = Some(plane);
    }
    for (e, mat) in &fresh {
        let handle = match cache.get(&mat.0.id()) {
            Some(h) => h.clone(),
            None => {
                let Some(base) = std_materials.get(&mat.0) else { continue };
                let h = clip_materials.add(ClipMaterial { base: base.clone(), extension: ClipExt { plane } });
                cache.insert(mat.0.id(), h.clone());
                h
            }
        };
        commands.entity(e).insert((MeshMaterial3d(handle), ClipOrig(mat.0.clone()))).remove::<MeshMaterial3d<StandardMaterial>>();
    }
}
