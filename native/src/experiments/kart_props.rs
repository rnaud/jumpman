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
    /// The quad's s runs past the picture with a mirrored wrap (the Piranha Plant).
    #[serde(default, rename = "mirrorU")]
    pub mirror_u: bool,
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
    /// Animated models (the Piranha Plant): the pictures to step through, the camera distance (course units) inside
    /// which it animates, and the ticks (30 Hz) per picture.
    #[serde(default)]
    pub frames: Vec<usize>,
    #[serde(default, rename = "animDistance")]
    pub anim_distance: f64,
    #[serde(default, rename = "frameTicks")]
    pub frame_ticks: u32,
    /// Trees turn to face the camera (`D_801502C0`); the Jungle Parkway's palm is a crossed-quad model that stands still.
    #[serde(default = "yes")]
    pub billboard: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
pub struct Prop {
    pub model: i64,
    pub pos: [f64; 3],
    /// The decomp keeps the spawn height as written (`render_palm_trees` draws the Jungle Parkway trees at `pos[1]`).
    #[serde(default)]
    pub fixed: bool,
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
    place(world, &Prop { model: 0, pos, fixed: false }, mirror)
}

/// Where a prop stands: its own height when the decomp draws it there (`fixed`), else the floor under it.
pub fn place(world: &CollisionWorld, prop: &Prop, mirror: bool) -> [f64; 3] {
    let pos = prop.pos;
    let (x, z) = (pos[0] * UNIT * if mirror { -1. } else { 1. }, pos[2] * UNIT);
    let hint = pos[1] * UNIT;
    if prop.fixed {
        return [x, hint, z];
    }
    let floor = world.find_floor(x, hint + 800., z);
    [x, if floor.surface.is_some() { floor.height } else { hint }, z]
}

/// Marks a billboard that turns to the camera about the up axis each frame.
#[derive(Component)]
pub struct PropBillboard;

/// `update_actor_piranha_plant`: the timer counts up while a camera is within range (60 ticks, then back to 6) and drops to
/// 0 out of range; the picture is `timer / frameTicks` (at most the last). Engine-free.
pub fn piranha_step(timer: u32, in_range: bool) -> u32 {
    if !in_range {
        return 0;
    }
    let t = timer + 1;
    if t > 60 {
        6
    } else {
        t
    }
}

/// The picture a plant shows for its timer.
pub fn piranha_frame(timer: u32, frame_ticks: u32, frames: usize) -> usize {
    ((timer / frame_ticks.max(1)) as usize).min(frames.saturating_sub(1))
}

/// An animated plant: the materials to swap on its quad, the child that holds the material, the timer and the range.
#[derive(Component)]
pub struct Piranha {
    pub materials: Vec<Handle<StandardMaterial>>,
    pub quad: Entity,
    pub timer: u32,
    pub frame_ticks: u32,
    /// Squared range in world units.
    pub range2: f32,
    pub acc: f32,
}

fn picture(host: &Host, file: &str, mirror_u: bool) -> Handle<bevy::image::Image> {
    host.assets.load_builder_with_settings(format!("mk64://{file}"), move |s: &mut ImageLoaderSettings| {
        let u = if mirror_u { ImageAddressMode::MirrorRepeat } else { ImageAddressMode::ClampToEdge };
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: u, address_mode_v: ImageAddressMode::ClampToEdge, ..ImageSamplerDescriptor::linear() });
    })
}

/// Spawn every prop of the course. Returns how many were placed.
pub fn spawn(host: &mut Host, data: &Props, world: &CollisionWorld, mirror: bool) -> usize {
    let materials: Vec<Handle<StandardMaterial>> = data
        .images
        .iter()
        .map(|i| host.materials.add(StandardMaterial { base_color_texture: Some(picture(host, &i.file, i.mirror_u)), unlit: true, cull_mode: None, alpha_mode: AlphaMode::Mask(0.5), ..default() }))
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
        let at = place(world, prop, mirror);
        let facing = data.models.iter().find(|m| m.id == prop.model).is_none_or(|m| m.billboard);
        let parent = host.spawn((Transform::from_translation(to_bevy(at)), Visibility::Inherited));
        if facing {
            host.commands.entity(parent).insert(PropBillboard);
        }
        let mut quad = None;
        for (mesh, image) in parts {
            let child = host.commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(materials[*image].clone()), Transform::default())).id();
            host.commands.entity(parent).add_child(child);
            quad = Some(child);
        }
        if let (Some(model), Some(quad)) = (data.models.iter().find(|m| m.id == prop.model && !m.frames.is_empty()), quad) {
            let range = (model.anim_distance * UNIT) as f32 * crate::render::SCALE;
            host.commands.entity(parent).insert(Piranha {
                materials: model.frames.iter().filter_map(|f| materials.get(*f).cloned()).collect(),
                quad,
                timer: 0,
                frame_ticks: model.frame_ticks,
                range2: range * range,
                acc: 0.,
            });
        }
        placed += 1;
    }
    placed
}

/// Turn every billboard to face the camera about the up axis (a vertical billboard, like the sprites).
pub fn face_camera(camera: Query<&GlobalTransform, With<crate::camera::MainCamera>>, mut props: Query<&mut Transform, With<PropBillboard>>) {
    let Some(eye) = camera.iter().next().map(|c| c.translation()) else { return };
    for mut t in &mut props {
        let flat = Vec3::new(t.translation.x - eye.x, 0., t.translation.z - eye.z);
        if flat.length_squared() > 1e-6 {
            t.rotation = Transform::default().looking_to(flat, Vec3::Y).rotation;
        }
    }
}

/// Step the plants at 30 Hz and show their picture (`render_actor_piranha_plant`).
pub fn animate_piranha(
    time: Res<Time>,
    camera: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    mut plants: Query<(&GlobalTransform, &mut Piranha)>,
    mut quads: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let Some(eye) = camera.iter().next().map(|c| c.translation()) else { return };
    for (at, mut p) in &mut plants {
        p.acc += time.delta_secs() * 30.;
        let in_range = at.translation().distance_squared(eye) < p.range2;
        while p.acc >= 1. {
            p.acc -= 1.;
            p.timer = piranha_step(p.timer, in_range);
        }
        let frame = piranha_frame(p.timer, p.frame_ticks, p.materials.len());
        if let (Some(m), Ok(mut slot)) = (p.materials.get(frame), quads.get_mut(p.quad)) {
            if slot.0 != *m {
                slot.0 = m.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (Part, Image) {
        (
            Part { image: 0, verts: vec![[40., 10., 0., 1536., 1706.], [0., 100., 0., 512., -341.], [-40., 10., 0., -512., 1706.], [0., -10., 0., 512., 2161.]], tris: vec![[0, 1, 2], [0, 2, 3]] },
            Image { file: "prop-mario-raceway-0.png".into(), width: 32., height: 64., mirror_u: false },
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
    fn a_plant_animates_near_the_camera_and_rests_far_away() {
        // 60 ticks of counting, then back to 6 (the loop skips the closed picture); out of range it resets.
        let mut t = 0;
        for _ in 0..61 {
            t = piranha_step(t, true);
        }
        assert_eq!(t, 6);
        assert_eq!(piranha_step(40, false), 0);
        assert_eq!(piranha_frame(0, 6, 9), 0);
        assert_eq!(piranha_frame(30, 6, 9), 5);
        assert_eq!(piranha_frame(60, 6, 9), 8);
        assert_eq!(piranha_frame(600, 6, 9), 8);
    }

    #[test]
    fn fixed_props_keep_their_spawn_height_and_the_others_settle_on_the_floor() {
        let mut world = CollisionWorld::default();
        let o = jumpman_sim::collision::SurfaceOptions::default();
        world.add_triangle([-9000., 40., -9000.], [-9000., 40., 9000.], [9000., 40., 9000.], &o);
        world.add_triangle([-9000., 40., -9000.], [9000., 40., 9000.], [9000., 40., -9000.], &o);
        let tree = Prop { model: 0, pos: [100., 10., 50.], fixed: true };
        assert_eq!(place(&world, &tree, false), [800., 80., 400.]);
        assert_eq!(place(&world, &Prop { fixed: false, ..tree }, false), [800., 40., 400.]);
    }

    #[test]
    fn models_face_the_camera_unless_the_data_says_otherwise() {
        let json = r#"{"images":[],"models":[{"id":1,"parts":[]},{"id":6,"billboard":false,"parts":[]}],"props":[{"model":6,"pos":[0,0,0],"fixed":true},{"model":1,"pos":[0,0,0]}]}"#;
        let p: Props = serde_json::from_str(json).unwrap();
        assert!(p.models[0].billboard && !p.models[1].billboard);
        assert!(p.props[0].fixed && !p.props[1].fixed);
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
