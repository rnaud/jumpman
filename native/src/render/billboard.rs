//! Camera-facing sprites (`objectBillboard` in the TS): coins use the camera's full rotation,
//! trees only rotate about Y.
use crate::camera::MainCamera;
use bevy::prelude::*;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Billboard {
    /// Copy the camera rotation (coins, stars).
    Full,
    /// Face the camera about the vertical axis (trees).
    Yaw,
}

pub struct BillboardPlugin;

impl Plugin for BillboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (face_camera, (mark_geo_billboards, face_geo_billboards).chain()).after(crate::CameraSet).after(crate::experiments::apply_camera));
    }
}

/// A glb node exported from `GEO_BILLBOARD` (`Sm64Billboards`): it always faces the camera and
/// keeps its size when an ancestor `GEO_SCALE` shrinks the skeleton.
#[derive(Component)]
pub struct GeoBillboard;

fn mark_geo_billboards(mut commands: Commands, new: Query<(Entity, &Name), Added<Name>>) {
    for (e, name) in &new {
        if name.as_str().starts_with("GEO_BILLBOARD") {
            commands.entity(e).insert(GeoBillboard);
        }
    }
}

fn face_geo_billboards(
    camera: Single<&GlobalTransform, With<MainCamera>>,
    mut nodes: Query<(&mut Transform, &ChildOf), With<GeoBillboard>>,
    parents: Query<(&GlobalTransform, &Transform, Option<&Name>, Option<&ChildOf>), Without<GeoBillboard>>,
) {
    let cam_rot = camera.rotation();
    for (mut tf, child_of) in &mut nodes {
        let mut geo_scale = 1.0f32;
        let mut parent_rot = None;
        let mut cur = Some(child_of.parent());
        while let Some(e) = cur {
            let Ok((global, local, name, up)) = parents.get(e) else { break };
            if parent_rot.is_none() {
                parent_rot = Some(global.rotation());
            }
            if name.is_some_and(|n| n.as_str().starts_with("GEO_SCALE")) {
                geo_scale *= local.scale.x;
            }
            cur = up.map(|c| c.parent());
        }
        tf.rotation = parent_rot.unwrap_or(Quat::IDENTITY).inverse() * cam_rot;
        if geo_scale != 0.0 {
            tf.scale = Vec3::splat(1.0 / geo_scale);
        }
    }
}

fn face_camera(camera: Single<&Transform, (With<MainCamera>, Without<Billboard>)>, mut sprites: Query<(&Billboard, &mut Transform)>) {
    for (kind, mut tf) in &mut sprites {
        match kind {
            Billboard::Full => tf.rotation = camera.rotation,
            Billboard::Yaw => {
                let d = camera.translation - tf.translation;
                tf.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
            }
        }
    }
}
