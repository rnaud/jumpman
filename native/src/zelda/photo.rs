//! Fixed-camera rooms (`prepareKokiriBackground`): the ROM draws the room into the depth buffer
//! and lays the original photograph over its colour. The room meshes get [`PhotoMaterial`], an
//! unlit material that samples the photograph in screen space, so actors are occluded by the
//! real room geometry.
use crate::camera::MainCamera;
use crate::render::course_view::RoomModel;
use bevy::asset::{load_internal_asset, weak_handle};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::shader::ShaderRef;
use bevy::render::render_resource::{AsBindGroup, };

const PHOTO_SHADER: Handle<Shader> = weak_handle!("3b8f6d1a-77c2-4e0b-a1d4-5c9e2f0a6b31");

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct PhotoMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub photo: Handle<Image>,
    #[uniform(2)]
    pub aspect: Vec4,
}

impl Material for PhotoMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PHOTO_SHADER)
    }
}

/// Present while the current course is a fixed-camera room with a background photograph.
#[derive(Resource)]
pub struct PhotoRoom(pub Handle<PhotoMaterial>);

#[derive(Component)]
struct Swapped;

pub struct PhotoPlugin;

impl Plugin for PhotoPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, PHOTO_SHADER, "photo.wesl", Shader::from_wesl);
        app.add_plugins(MaterialPlugin::<PhotoMaterial>::default()).add_systems(PostUpdate, (swap_room_materials, update_aspect));
    }
}

fn swap_room_materials(
    room: Option<Res<PhotoRoom>>,
    mut commands: Commands,
    rooms: Query<Entity, With<RoomModel>>,
    children: Query<&Children>,
    meshes: Query<Entity, (With<MeshMaterial3d<StandardMaterial>>, Without<Swapped>)>,
) {
    let Some(room) = room else { return };
    for r in &rooms {
        for e in children.iter_descendants(r) {
            if meshes.contains(e) {
                commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((MeshMaterial3d(room.0.clone()), NotShadowCaster, Swapped));
            }
        }
    }
}

fn update_aspect(room: Option<Res<PhotoRoom>>, mut materials: ResMut<Assets<PhotoMaterial>>, camera: Single<&Camera, With<MainCamera>>) {
    let (Some(room), Some(size)) = (room, camera.logical_viewport_size()) else { return };
    let aspect = size.x / size.y.max(1.0) / (4.0 / 3.0);
    if let Some(mut m) = materials.get_mut(&room.0) {
        if (m.aspect.x - aspect).abs() > 1e-4 {
            m.aspect = Vec4::new(aspect, 0.0, 0.0, 0.0);
        }
    }
}
