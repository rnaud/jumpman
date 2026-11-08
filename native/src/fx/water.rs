//! Animated translucent water planes over the collision world's water boxes (`waterSurfaces.ts`).
use crate::game::runner::GameSim;
use crate::render::SCALE;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use jumpman_sim::collision::WaterBox;

const SEGMENTS: usize = 18;
/// Water boxes of the Banjo-Kazooie worlds (`scripts/export-bk-levels.py`): swimmable, but the level
/// model already draws its own translucent water, so no extra plane.
pub const BK_WATER_ID: f64 = 45.0;

/// Vertical wave offset (SM64 units) at local `(x, z)` (`WaterSurface.update`).
pub fn wave_height(x: f32, z: f32, time: f32, phase: f32) -> f32 {
    (x * 0.012 + time * 1.4 + phase).sin() * 3.5 + (z * 0.009 + time * 0.9 + phase).cos() * 2.5
}

pub fn wave_opacity(time: f32, phase: f32) -> f32 {
    0.3 + (time * 1.2 + phase).sin() * 0.04
}

pub fn wave_phase(id: f64) -> f32 {
    ((id * 1.7) % (std::f64::consts::PI * 2.0)) as f32
}

#[derive(Component)]
pub struct WaterSurface {
    /// Grid vertices, local SM64 x/z.
    base: Vec<[f32; 2]>,
    phase: f32,
}

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_water).add_systems(Update, (animate_water, follow_water_levels));
        // Surfaces are course-scoped: rebuild them for the new course after a switch.
        app.init_resource::<crate::progress::CourseRebuild>();
        app.world_mut().resource_mut::<crate::progress::CourseRebuild>().push(water_rebuild);
    }
}

fn water_rebuild(world: &mut World) {
    let _ = bevy::ecs::system::RunSystemOnce::run_system_once(world, spawn_water);
}

fn plane_mesh(width: f32, depth: f32) -> (Mesh, Vec<[f32; 2]>) {
    let n = SEGMENTS + 1;
    let mut base = Vec::with_capacity(n * n);
    for iz in 0..n {
        for ix in 0..n {
            base.push([(ix as f32 / SEGMENTS as f32 - 0.5) * width, (iz as f32 / SEGMENTS as f32 - 0.5) * depth]);
        }
    }
    let mut idx = Vec::new();
    for iz in 0..SEGMENTS {
        for ix in 0..SEGMENTS {
            let a = (iz * n + ix) as u32;
            let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let positions: Vec<[f32; 3]> = base.iter().map(|p| [p[0] * SCALE, 0.0, p[1] * SCALE]).collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; positions.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(idx));
    (mesh, base)
}

/// Water boxes that get a surface: ids < 50 (others are other fluids; the BK id never), at least 2 units wide.
pub fn surface_boxes(boxes: &[WaterBox]) -> impl Iterator<Item = &WaterBox> {
    boxes.iter().filter(|WaterBox(id, x1, z1, x2, z2, _)| *id < 50.0 && *id != BK_WATER_ID && (x2 - x1).abs() >= 2.0 && (z2 - z1).abs() >= 2.0)
}

fn spawn_water(mut commands: Commands, sim: Res<GameSim>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (index, WaterBox(id, x1, z1, x2, z2, y)) in sim.course.world.water_boxes.iter().enumerate().filter(|(_, b)| surface_boxes(std::slice::from_ref(*b)).next().is_some()) {
        let (mesh, base) = plane_mesh((x2 - x1).abs() as f32, (z2 - z1).abs() as f32);
        let phase = wave_phase(*id);
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0x4f as f32 / 255.0, 0xb9 as f32 / 255.0, 0xdf as f32 / 255.0, 0.34),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.16,
            metallic: 0.05,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            WaterSurface { base, phase },
            WaterBoxIndex(index),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform::from_xyz(((x1 + x2) * 0.5) as f32 * SCALE, (*y as f32 - 4.0) * SCALE, ((z1 + z2) * 0.5) as f32 * SCALE),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

/// Index of the sim water box a surface draws (`gEnvironmentRegions` heights change: the castle moat drain).
#[derive(Component)]
pub struct WaterBoxIndex(pub usize);

/// Keep each surface at its water box's current height.
pub fn follow_water_levels(sim: Res<GameSim>, mut surfaces: Query<(&WaterBoxIndex, &mut Transform)>) {
    for (index, mut tf) in &mut surfaces {
        if let Some(b) = sim.mario.world.water_boxes.get(index.0) {
            let y = (b.5 as f32 - 4.0) * SCALE;
            if tf.translation.y != y {
                tf.translation.y = y;
            }
        }
    }
}

fn animate_water(time: Res<Time>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, surfaces: Query<(&WaterSurface, &Mesh3d, &MeshMaterial3d<StandardMaterial>)>) {
    let t = time.elapsed_secs();
    for (surface, mesh, material) in &surfaces {
        if let Some(mut mesh) = meshes.get_mut(&mesh.0) {
            if let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
                for (p, b) in pos.iter_mut().zip(&surface.base) {
                    p[1] = wave_height(b[0], b[1], t, surface.phase) * SCALE;
                }
            }
        }
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.base_color.set_alpha(wave_opacity(t, surface.phase));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_matches_ts_formula() {
        let h = wave_height(100.0, -50.0, 2.0, 0.5);
        let want = (100.0f32 * 0.012 + 2.8 + 0.5).sin() * 3.5 + (-50.0f32 * 0.009 + 1.8 + 0.5).cos() * 2.5;
        assert!((h - want).abs() < 1e-5);
        assert!(h.abs() <= 6.0);
    }

    #[test]
    fn opacity_band() {
        for i in 0..100 {
            let o = wave_opacity(i as f32 * 0.37, 1.0);
            assert!((0.26..=0.34).contains(&o));
        }
    }

    #[test]
    fn plane_grid_size() {
        let (mesh, base) = plane_mesh(100.0, 40.0);
        assert_eq!(base.len(), 19 * 19);
        assert_eq!(mesh.indices().unwrap().len(), 18 * 18 * 6);
        assert_eq!(base[0], [-50.0, -20.0]);
    }

    #[test]
    fn filters_non_water_and_tiny_boxes() {
        let boxes = vec![WaterBox(1.0, 0.0, 0.0, 100.0, 100.0, 0.0), WaterBox(50.0, 0.0, 0.0, 100.0, 100.0, 0.0), WaterBox(2.0, 0.0, 0.0, 1.0, 100.0, 0.0)];
        assert_eq!(surface_boxes(&boxes).count(), 1);
    }
}
