//! Static course presentation (`sm64World.ts` display side): level models, decoration props,
//! water, the procedural playground and room-model switching for interiors.
use super::billboard::Billboard;
use super::{scene, to_bevy, AssetRoot, SCALE};
use crate::game::playground::{Material, Piece, Shape};
use crate::game::runner::GameSim;
use crate::game::world::CourseVisual;
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use jumpman_sim::collision::WaterBox;

pub struct CourseViewPlugin;

impl Plugin for CourseViewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.48, 0.76, 0.96)))
            .insert_resource(AmbientLight { brightness: 320., ..default() })
            .add_systems(Startup, spawn_course)
            .add_systems(Update, select_room);
    }
}

/// One display model of a multi-room interior (castle).
#[derive(Component)]
pub struct RoomModel(pub usize);

fn spawn_course(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    sim: Res<GameSim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let course = &sim.course;
    match &course.visual {
        CourseVisual::Glb { models } => {
            for (index, path) in models.iter().enumerate() {
                if !root.exists(path) {
                    warn!("course model missing: {path}");
                    continue;
                }
                commands.spawn((
                    scene(&assets, path),
                    if index == 0 || models.len() == 1 { Visibility::Inherited } else { Visibility::Hidden },
                    RoomModel(index),
                ));
            }
        }
        CourseVisual::Playground(pieces) => spawn_playground(&mut commands, pieces, &mut meshes, &mut materials),
    }

    for prop in &course.props {
        if !root.exists(&prop.model) {
            warn!("prop model missing: {}", prop.model);
            continue;
        }
        let rotation = match prop.rotation {
            // (pitch, yaw, roll) s16, three.js 'YXZ'.
            Some(r) => Quat::from_euler(EulerRot::YXZ, (r[1] * std::f64::consts::PI / 32768.0) as f32, (r[0] * std::f64::consts::PI / 32768.0) as f32, (r[2] * std::f64::consts::PI / 32768.0) as f32),
            None => Quat::from_rotation_y(prop.yaw.to_radians() as f32),
        };
        let tf = Transform::from_translation(to_bevy(prop.pos)).with_rotation(rotation).with_scale(Vec3::splat(prop.scale as f32));
        let mut e = commands.spawn((scene(&assets, &prop.model), tf));
        if prop.billboard {
            e.insert(Billboard::Yaw);
        }
    }

    // Water surfaces (ids < 50 are water; the rest are other fluids).
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.23, 0.67, 0.82, 0.45),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for WaterBox(id, x1, z1, x2, z2, y) in &course.world.water_boxes {
        if *id >= 50.0 {
            continue;
        }
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(((x2 - x1) as f32) * SCALE, ((z2 - z1) as f32) * SCALE))),
            MeshMaterial3d(water.clone()),
            Transform::from_xyz(((x1 + x2) * 0.5) as f32 * SCALE, (*y as f32 - 4.0) * SCALE, ((z1 + z2) * 0.5) as f32 * SCALE),
        ));
    }

    commands.spawn((
        DirectionalLight { illuminance: 1800., shadows_enabled: true, ..default() },
        Transform::from_xyz(-5., 10., 5.).looking_at(Vec3::ZERO, Vec3::Y),
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
