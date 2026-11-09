//! Course props of the kart experiment: the foliage `spawn_foliage` places (`racing/actors.c`), read from
//! `<course>-props.json` (written by `byor-rs` `mk64/props.rs`: the spawn rows, the tree display lists and the CI8 pictures).
//! Every tree is a vertical billboard: its model quad (the display list's four vertices) turned to the camera about the
//! up axis, standing on the course floor under its spawn point (`check_bounding_collision` + `calculate_surface_height`).
use super::Host;
use crate::button_marker::LoadWithSettings;
use crate::render::to_bevy;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use jumpman_sim::collision::CollisionWorld;
use serde::Deserialize;
use std::path::Path;

/// Course units -> world units (`kart.rs` `UNIT`, the MK64 export scale).
const UNIT: f64 = 8.;

#[derive(Clone, Debug, Deserialize)]
pub struct Image {
    pub file: String,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Part {
    pub image: usize,
    /// `[x, y, z, s, t]` (s / t in the display list's 10.5 fixed point texels).
    pub verts: Vec<[f64; 5]>,
    pub tris: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Model {
    pub id: i64,
    pub parts: Vec<Part>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Prop {
    pub model: i64,
    pub pos: [f64; 3],
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Props {
    pub images: Vec<Image>,
    pub models: Vec<Model>,
    pub props: Vec<Prop>,
}

impl Props {
    /// `<course>-props.json` of a decomp course directory, when the course has foliage.
    pub fn read(root: &Path, dir: &str) -> Option<Self> {
        let bytes = std::fs::read(root.join(format!("{}-props.json", dir.replace('_', "-")))).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

/// Mesh data of one model part in world units about the prop's origin: positions (Bevy axes), uvs, indices. `mirror` flips x
/// like the mirrored course does.
pub fn part_mesh(part: &Part, image: &Image, mirror: bool) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
    let flip = if mirror { -1. } else { 1. };
    let positions = part.verts.iter().map(|v| to_bevy([v[0] * UNIT * flip, v[1] * UNIT, v[2] * UNIT]).to_array()).collect();
    let uvs = part.verts.iter().map(|v| [(v[3] / (32. * image.width as f64)) as f32, (v[4] / (32. * image.height as f64)) as f32]).collect();
    let mut indices: Vec<u32> = part.tris.iter().flatten().copied().collect();
    if mirror {
        for t in indices.chunks_mut(3) {
            t.swap(1, 2);
        }
    }
    (positions, uvs, indices)
}

/// Where a prop stands: its spawn point on the course floor (the array's y is only a hint), in world units.
pub fn stand(world: &CollisionWorld, pos: [f64; 3], mirror: bool) -> [f64; 3] {
    let (x, z) = (pos[0] * UNIT * if mirror { -1. } else { 1. }, pos[2] * UNIT);
    let hint = pos[1] * UNIT;
    let floor = world.find_floor(x, hint + 800., z);
    [x, if floor.surface.is_some() { floor.height } else { hint }, z]
}

/// Marks a billboard that turns to the camera about the up axis each frame.
#[derive(Component)]
pub struct PropBillboard;

fn picture(host: &Host, file: &str) -> Handle<bevy::image::Image> {
    host.assets.load_builder_with_settings(format!("mk64://{file}"), |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::ClampToEdge, address_mode_v: ImageAddressMode::ClampToEdge, ..ImageSamplerDescriptor::linear() });
    })
}

/// Spawn every prop of the course. Returns how many were placed.
pub fn spawn(host: &mut Host, data: &Props, world: &CollisionWorld, mirror: bool) -> usize {
    let materials: Vec<Handle<StandardMaterial>> = data
        .images
        .iter()
        .map(|i| host.materials.add(StandardMaterial { base_color_texture: Some(picture(host, &i.file)), unlit: true, cull_mode: None, alpha_mode: AlphaMode::Mask(0.5), ..default() }))
        .collect();
    // One mesh per model part, shared by every instance.
    let meshes: Vec<(i64, Vec<(Handle<Mesh>, usize)>)> = data
        .models
        .iter()
        .map(|m| {
            let parts = m
                .parts
                .iter()
                .filter_map(|p| {
                    let image = data.images.get(p.image)?;
                    let (positions, uvs, indices) = part_mesh(p, image, mirror);
                    let n = positions.len();
                    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
                    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
                    mesh.insert_indices(Indices::U32(indices));
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 0., 1.]; n]);
                    Some((host.meshes.add(mesh), p.image))
                })
                .collect();
            (m.id, parts)
        })
        .collect();
    let mut placed = 0;
    for prop in &data.props {
        let Some((_, parts)) = meshes.iter().find(|(id, _)| *id == prop.model) else { continue };
        let at = stand(world, prop.pos, mirror);
        let parent = host.spawn((Transform::from_translation(to_bevy(at)), Visibility::Inherited, PropBillboard));
        for (mesh, image) in parts {
            let child = host.commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(materials[*image].clone()), Transform::default())).id();
            host.commands.entity(parent).add_child(child);
        }
        placed += 1;
    }
    placed
}

/// Turn every billboard to face the camera about the up axis (a vertical billboard, like the sprites).
pub fn face_camera(camera: Query<&GlobalTransform, With<Camera3d>>, mut props: Query<&mut Transform, With<PropBillboard>>) {
    let Some(eye) = camera.iter().next().map(|c| c.translation()) else { return };
    for mut t in &mut props {
        let flat = Vec3::new(t.translation.x - eye.x, 0., t.translation.z - eye.z);
        if flat.length_squared() > 1e-6 {
            t.rotation = Transform::default().looking_to(flat, Vec3::Y).rotation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (Part, Image) {
        (
            Part { image: 0, verts: vec![[40., 10., 0., 1536., 1706.], [0., 100., 0., 512., -341.], [-40., 10., 0., -512., 1706.], [0., -10., 0., 512., 2161.]], tris: vec![[0, 1, 2], [0, 2, 3]] },
            Image { file: "prop-mario-raceway-0.png".into(), width: 32., height: 64. },
        )
    }

    #[test]
    fn the_quad_is_scaled_to_world_units_and_the_picture_is_clamped_in_texels() {
        let (p, i) = tree();
        let (pos, uv, idx) = part_mesh(&p, &i, false);
        assert_eq!(idx, vec![0, 1, 2, 0, 2, 3]);
        // The top vertex: 100 course units high, centre column of the 32 x 64 picture (s 512 = 16 texels = 0.5).
        assert!((pos[1][1] - to_bevy([0., 100. * UNIT, 0.]).y).abs() < 1e-3);
        assert!((uv[1][0] - 0.5).abs() < 1e-6);
        assert!((uv[1][1] - (-341. / 32. / 64.) as f32).abs() < 1e-6);
    }

    #[test]
    fn mirror_mode_flips_x_and_the_winding() {
        let (p, i) = tree();
        let (a, _, ia) = part_mesh(&p, &i, false);
        let (b, _, ib) = part_mesh(&p, &i, true);
        assert!((a[0][0] + b[0][0]).abs() < 1e-3, "x mirrored");
        assert_eq!(ib[..3], [ia[0], ia[2], ia[1]]);
    }

    #[test]
    fn a_prop_stands_on_the_floor_below_its_spawn_point() {
        let mut world = CollisionWorld::default();
        let o = jumpman_sim::collision::SurfaceOptions::default();
        world.add_triangle([-9000., 40., -9000.], [-9000., 40., 9000.], [9000., 40., 9000.], &o);
        world.add_triangle([-9000., 40., -9000.], [9000., 40., 9000.], [9000., 40., -9000.], &o);
        let at = stand(&world, [100., 0., 50.], false);
        assert_eq!(at, [800., 40., 400.]);
        assert_eq!(stand(&world, [100., 0., 50.], true)[0], -800.);
    }
}
