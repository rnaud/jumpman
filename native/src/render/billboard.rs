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
        app.add_systems(Update, face_camera.after(crate::CameraSet));
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
