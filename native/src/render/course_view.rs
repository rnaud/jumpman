//! Static course presentation (`sm64World.ts` display side): level models, decoration props,
//! water, the procedural playground and room-model switching for interiors.
use super::billboard::Billboard;
use super::{scene, to_bevy, AssetRoot, SCALE};
use crate::game::playground::{Material, Piece, Shape};
use crate::game::runner::GameSim;
use crate::game::world::CourseVisual;
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;

pub struct CourseViewPlugin;

impl Plugin for CourseViewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.48, 0.76, 0.96)))
            .insert_resource(GlobalAmbientLight { brightness: 320., ..default() })
            .add_systems(Startup, spawn_course)
            .add_systems(Update, (select_room, animate_props));
    }
}

/// One display model of a multi-room interior (castle).
#[derive(Component)]
pub struct RoomModel(pub usize);

/// A decoration drawn from `LoadedCourse::props[i]` (the link prop layer hides it when its prop is destroyed).
#[derive(Component)]
pub struct StaticProp(pub usize);

pub fn spawn_course(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    sim: Res<GameSim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let course = &sim.course;
    // Banjo-Kazooie worlds bring their own sky gradient; every other course keeps the daylight default.
    commands.insert_resource(match course.sky {
        Some(s) => crate::fx::sky::SkyPalette { top: s.top, mid: s.mid, bottom: s.bottom, clouds: 0.0 },
        None => match crate::game::mk64::look(&root.0, &course.request.id) {
            Some(l) => crate::fx::sky::SkyPalette::from_hex(l.sky[0], l.sky[1], l.sky[2]),
            None => crate::fx::sky::SkyPalette::default(),
        },
    });
    match &course.visual {
        CourseVisual::Glb { models } => {
            for (index, path) in models.iter().enumerate() {
                if !root.exists(path) {
                    warn!("course model missing: {path}");
                    continue;
                }
                commands.spawn((
                    scene(&assets, path),
                    Transform::from_scale(Vec3::splat(crate::zelda::glb::bevy_scale(&root.0.join(path)))),
                    if index == 0 || models.len() == 1 { Visibility::Inherited } else { Visibility::Hidden },
                    RoomModel(index),
                    super::decal::DecalScene,
                ));
            }
        }
        CourseVisual::Playground(pieces) => spawn_playground(&mut commands, pieces, &mut meshes, &mut materials),
    }

    for (prop_index, prop) in course.props.iter().enumerate() {
        if !root.exists(&prop.model) {
            warn!("prop model missing: {}", prop.model);
            continue;
        }
        let rotation = match prop.rotation {
            // (pitch, yaw, roll) s16, three.js 'YXZ'.
            Some(r) => Quat::from_euler(EulerRot::YXZ, (r[1] * std::f64::consts::PI / 32768.0) as f32, (r[0] * std::f64::consts::PI / 32768.0) as f32, (r[2] * std::f64::consts::PI / 32768.0) as f32),
            None => Quat::from_rotation_y(prop.yaw.to_radians() as f32),
        };
        let tf = Transform::from_translation(to_bevy(prop.pos)).with_rotation(rotation).with_scale(Vec3::splat(prop.scale as f32 * crate::zelda::glb::bevy_scale(&root.0.join(&prop.model))));
        let mut e = commands.spawn((scene(&assets, &prop.model), tf, super::decal::DecalScene, StaticProp(prop_index)));
        if prop.billboard {
            e.insert(Billboard::Yaw);
        }
        if prop.scroll_t != 0.0 {
            e.insert(ScrollTex { speed: prop.scroll_t as f32, materials: None });
        }
        if prop.spin_z != 0.0 {
            e.insert(SpinZ { rate: prop.spin_z as f32, base: rotation });
        }
    }

    // Water surfaces live in `fx/water.rs`.

    commands.spawn((
        DirectionalLight { illuminance: 1800., shadow_maps_enabled: true, ..default() },
        crate::lighting::sun_cascades(),
        // `JUMPMAN_SUN=x,y,z` places the sun for captures (a low sun makes cast shadows long enough to judge).
        std::env::var("JUMPMAN_SUN").ok().and_then(|v| {
            let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            (p.len() == 3).then(|| Transform::from_xyz(p[0], p[1], p[2]).looking_at(Vec3::ZERO, Vec3::Y))
        }).unwrap_or_else(|| Transform::from_xyz(-5., 10., 5.).looking_at(Vec3::ZERO, Vec3::Y)),
    ));
}

/// `selectRoom`: show the room model the floor under Mario belongs to.
fn select_room(sim: Res<GameSim>, mut rooms: Query<(&RoomModel, &mut Visibility)>, mut current: Local<Option<usize>>) {
    if rooms.iter().count() < 2 {
        return;
    }
    let floor_room = sim.mario.floor.as_ref().and_then(|f| f.room).filter(|r| *r > 0);
    let selected = floor_room.map_or(current.unwrap_or(0), |r| (r - 1) as usize);
    if *current == Some(selected) {
        return;
    }
    *current = Some(selected);
    for (room, mut vis) in &mut rooms {
        *vis = if room.0 == selected { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn material_color(m: Material) -> Color {
    match m {
        Material::Grass => Color::srgb(0.38, 0.62, 0.25),
        Material::Stone => Color::srgb(0.78, 0.70, 0.54),
        Material::Wood => Color::srgb(0.62, 0.43, 0.26),
        Material::Ice => Color::srgb(0.66, 0.90, 1.0),
    }
}

fn spawn_playground(commands: &mut Commands, pieces: &[Piece], meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let mut mats = std::collections::HashMap::new();
    let mut material = |m: Material, materials: &mut Assets<StandardMaterial>| {
        mats.entry(format!("{m:?}"))
            .or_insert_with(|| materials.add(StandardMaterial { base_color: material_color(m), perceptual_roughness: 1.0, ..default() }))
            .clone()
    };
    for p in pieces.iter().filter(|p| !p.hidden) {
        let mat = material(p.material, materials);
        match p.shape {
            Shape::Ground => {
                commands.spawn((
                    Mesh3d(meshes.add(Plane3d::default().mesh().size(crate::game::playground::PLAYGROUND_SIZE as f32 * SCALE, crate::game::playground::PLAYGROUND_SIZE as f32 * SCALE))),
                    MeshMaterial3d(mat),
                    Transform::default(),
                ));
            }
            Shape::Box { x, z, w, h, d } => {
                commands.spawn((
                    Mesh3d(meshes.add(Cuboid::new(w as f32 * SCALE, h as f32 * SCALE, d as f32 * SCALE))),
                    MeshMaterial3d(mat),
                    Transform::from_translation(to_bevy([x, h / 2.0, z])),
                ));
            }
            Shape::Slab { x, z, w, top, h, d } => {
                commands.spawn((
                    Mesh3d(meshes.add(Cuboid::new(w as f32 * SCALE, h as f32 * SCALE, d as f32 * SCALE))),
                    MeshMaterial3d(mat),
                    Transform::from_translation(to_bevy([x, top - h / 2.0, z])),
                ));
            }
            Shape::Ramp { x, z, w, l, h } => {
                let (hw, hl, h) = (w as f32 * SCALE / 2.0, l as f32 * SCALE / 2.0, h as f32 * SCALE);
                let v = [[-hw, 0., -hl], [hw, 0., -hl], [-hw, 0., hl], [hw, 0., hl], [-hw, h, hl], [hw, h, hl]];
                let tris = [[0, 4, 5], [0, 5, 1], [2, 3, 5], [2, 5, 4], [0, 2, 4], [1, 5, 3], [0, 1, 3], [0, 3, 2]];
                let positions: Vec<[f32; 3]> = tris.iter().flatten().map(|&i| v[i]).collect();
                let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                mesh.compute_flat_normals();
                commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), Transform::from_translation(to_bevy([x, 0.0, z]))));
            }
            Shape::Trunk { .. } => {}
        }
    }
}

/// `scrollT`: scrolling texture (`scrollingTextures`, `offset.y = (time * speed) % 1`).
#[derive(Component)]
pub struct ScrollTex {
    speed: f32,
    /// Private material copies, made once the scene has spawned.
    materials: Option<Vec<Handle<StandardMaterial>>>,
}

/// `spinZ`: the prop turns about its local Z axis.
#[derive(Component)]
pub struct SpinZ {
    rate: f32,
    base: Quat,
}

pub fn animate_props(
    time: Res<Time>,
    mut scrolls: Query<(Entity, &mut ScrollTex)>,
    mut spins: Query<(&SpinZ, &mut Transform)>,
    children: Query<&Children>,
    mats: Query<&MeshMaterial3d<StandardMaterial>>,
    mut commands: Commands,
    mut assets: ResMut<Assets<StandardMaterial>>,
) {
    let t = time.elapsed_secs();
    for (s, mut tf) in &mut spins {
        tf.rotation = s.base * Quat::from_rotation_z(t * s.rate);
    }
    for (entity, mut scroll) in &mut scrolls {
        if scroll.materials.is_none() {
            let meshes: Vec<Entity> = children.iter_descendants(entity).filter(|e| mats.contains(*e)).collect();
            if meshes.is_empty() {
                continue;
            }
            let mut handles = vec![];
            for e in meshes {
                let Ok(m) = mats.get(e) else { continue };
                let Some(base) = assets.get(&m.0).cloned() else { continue };
                let handle = assets.add(base);
                commands.entity(e).try_insert(MeshMaterial3d(handle.clone()));
                handles.push(handle);
            }
            scroll.materials = Some(handles);
        }
        let offset = (t * scroll.speed).rem_euclid(1.0);
        for h in scroll.materials.iter().flatten() {
            if let Some(mut m) = assets.get_mut(h) {
                m.uv_transform = bevy::math::Affine2::from_translation(Vec2::new(0.0, offset));
            }
        }
    }
}
