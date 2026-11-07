//! Sky dome (`world.ts addSky`), per-course distance fog and underwater tint (`updateEnvironment`).
use super::particles::{hex_rgb, Particles};
use crate::camera::MainCamera;
use crate::game::runner::GameSim;
use crate::game::world::CourseVisual;
use crate::render::SCALE;
use bevy::pbr::{DistanceFog, FogFalloff, NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;

pub const TOP: u32 = 0x3f8fd6;
pub const MID: u32 = 0x9ed4f5;
pub const BOTTOM: u32 = 0xe8f6ff;
pub const FOG_COLOR: u32 = 0xc7e8ff;
pub const UNDERWATER_COLOR: u32 = 0x1d5878;

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Sky gradient for a unit direction's `y` (the TS fragment shader).
pub fn sky_color(h: f32) -> [f32; 3] {
    let (top, mid, bottom) = (hex_rgb(TOP), hex_rgb(MID), hex_rgb(BOTTOM));
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
    let c = mix(bottom, mid, smoothstep(-0.05, 0.22, h));
    mix(c, top, smoothstep(0.2, 0.75, h))
}

/// `(near, far)` fog distances in SM64 units.
pub fn fog_range(playground: bool) -> (f32, f32) {
    if playground {
        (6500.0, 16000.0)
    } else {
        (10000.0, 26000.0)
    }
}

#[derive(Component)]
pub struct SkyDome;

/// Camera-environment state other systems read.
#[derive(Resource, Default)]
pub struct Environment {
    pub underwater: bool,
    /// Fog pushed back by this many units while a telephoto experiment camera (2D side/top) is active.
    pub fog_shift: f32,
}

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Environment>()
            .add_systems(Startup, spawn_sky)
            .add_systems(Update, (update_environment, follow_camera.after(crate::CameraSet).after(crate::experiments::apply_camera)));
    }
}

fn is_pss(sim: &GameSim) -> bool {
    sim.course.request.id == "pss"
}

fn is_playground(sim: &GameSim) -> bool {
    matches!(sim.course.visual, CourseVisual::Playground(_))
}

fn spawn_sky(mut commands: Commands, sim: Res<GameSim>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if is_pss(&sim) {
        commands.insert_resource(ClearColor(Color::BLACK));
        return;
    }
    let radius = if is_playground(&sim) { 20000.0 } else { 30000.0 } * SCALE;
    let mut mesh: Mesh = Sphere::new(radius).mesh().uv(32, 16).into();
    if let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        let colors: Vec<[f32; 4]> = pos
            .iter()
            .map(|p| {
                let c = sky_color(Vec3::from(*p).normalize().y);
                let l = Color::srgb(c[0], c[1], c[2]).to_linear();
                [l.red, l.green, l.blue, 1.0]
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }
    commands.spawn((
        SkyDome,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial { unlit: true, cull_mode: None, fog_enabled: false, ..default() })),
        Transform::default(),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn follow_camera(camera: Single<&Transform, (With<MainCamera>, Without<SkyDome>)>, mut sky: Query<&mut Transform, With<SkyDome>>) {
    for mut t in &mut sky {
        t.translation = camera.translation;
    }
}

fn linear_fog(color: u32, start: f32, end: f32) -> DistanceFog {
    let c = hex_rgb(color);
    DistanceFog { color: Color::srgb(c[0], c[1], c[2]), falloff: FogFalloff::Linear { start: start * SCALE, end: end * SCALE }, ..default() }
}

/// Underwater: dark teal fog 120..9000 and a splash on entering/leaving the water volume.
fn update_environment(
    mut commands: Commands,
    sim: Res<GameSim>,
    mut env: ResMut<Environment>,
    mut particles: ResMut<Particles>,
    cameras: Query<Entity, With<MainCamera>>,
    mut inserted: Local<bool>,
    camera_override: Res<crate::experiments::CameraOverride>,
) {
    let pos = sim.mario.pos;
    let water = sim.mario.world.water_level(pos[0], pos[2]);
    let underwater = water.is_finite() && pos[1] < water - 40.0;
    let mut changed = underwater != env.underwater;
    if changed {
        env.underwater = underwater;
        particles.splash(pos);
    }
    // A far-away flat camera (Dimensions 2D) would sit inside the fog: measure fog from the focus instead.
    let shift = camera_override.pose.map_or(0.0, |p| ((p.eye.distance(p.look) - 3000.0).max(0.0) / 500.0).round() as f32 * 500.0);
    if shift != env.fog_shift {
        env.fog_shift = shift;
        changed = true;
    }
    if is_pss(&sim) {
        return;
    }
    // Insert on the first frame the camera exists and whenever the state flips.
    let (near, far) = fog_range(is_playground(&sim));
    let fog = if underwater { linear_fog(UNDERWATER_COLOR, 120.0, 9000.0) } else { linear_fog(FOG_COLOR, near + env.fog_shift, far + env.fog_shift) };
    for e in &cameras {
        if changed || !*inserted {
            commands.entity(e).insert(fog.clone());
            *inserted = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_endpoints() {
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
        assert!(close(sky_color(1.0), hex_rgb(TOP)));
        assert!(close(sky_color(-1.0), hex_rgb(BOTTOM)));
        let mid = sky_color(0.22);
        let want = hex_rgb(MID);
        assert!((0..3).all(|i| (mid[i] - want[i]).abs() < 0.02));
    }

    #[test]
    fn fog_ranges() {
        assert_eq!(fog_range(false), (10000.0, 26000.0));
        assert_eq!(fog_range(true), (6500.0, 16000.0));
    }
}
