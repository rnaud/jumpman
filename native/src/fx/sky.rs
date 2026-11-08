//! Sky dome (`world.ts addSky`), per-course distance fog and underwater tint (`updateEnvironment`).
use super::particles::{hex_rgb, Particles};
use crate::camera::MainCamera;
use crate::game::runner::GameSim;
use crate::game::world::CourseVisual;
use crate::render::SCALE;
use bevy::asset::{load_internal_asset, weak_handle};
use bevy::pbr::{DistanceFog, FogFalloff, Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin, NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError};

pub const TOP: u32 = 0x3f8fd6;
pub const MID: u32 = 0x9ed4f5;
pub const BOTTOM: u32 = 0xe8f6ff;
pub const FOG_COLOR: u32 = 0xc7e8ff;
pub const UNDERWATER_COLOR: u32 = 0x1d5878;

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Gradient stops of the dome (`SkyDome` of the experiments: each one picks its own).
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct SkyPalette {
    pub top: [f32; 3],
    pub mid: [f32; 3],
    pub bottom: [f32; 3],
    /// Cloud coverage of the animated fbm layer (`SkyDome.cloudAmount`); 0 = a plain gradient.
    pub clouds: f32,
}

impl Default for SkyPalette {
    fn default() -> Self {
        Self { top: hex_rgb(TOP), mid: hex_rgb(MID), bottom: hex_rgb(BOTTOM), clouds: 0.0 }
    }
}

impl SkyPalette {
    pub fn from_hex(top: u32, mid: u32, bottom: u32) -> Self {
        Self { top: hex_rgb(top), mid: hex_rgb(mid), bottom: hex_rgb(bottom), clouds: 0.0 }
    }
    pub fn with_clouds(mut self, clouds: f32) -> Self {
        self.clouds = clouds;
        self
    }
    /// Move towards `to` by `blend` (0..1), like `SkyDome.setColors`.
    pub fn lerp_to(&mut self, to: &SkyPalette, blend: f32) {
        let mix = |a: &mut [f32; 3], b: [f32; 3]| (0..3).for_each(|i| a[i] += (b[i] - a[i]) * blend);
        mix(&mut self.top, to.top);
        mix(&mut self.mid, to.mid);
        mix(&mut self.bottom, to.bottom);
        self.clouds += (to.clouds - self.clouds) * blend;
    }
}

/// Sky gradient for a unit direction's `y` (the TS fragment shader).
pub fn sky_color(h: f32) -> [f32; 3] {
    sky_color_with(&SkyPalette::default(), h)
}

pub fn sky_color_with(p: &SkyPalette, h: f32) -> [f32; 3] {
    let (top, mid, bottom) = (p.top, p.mid, p.bottom);
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

/// Set when something removed the camera fog and it should be rebuilt.
#[derive(Resource, Default)]
pub struct FogRefresh(pub bool);

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
        load_internal_asset!(app, SKY_SHADER, "sky_dome.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .init_resource::<Environment>()
            .init_resource::<FogRefresh>()
            .init_resource::<SkyPalette>()
            .add_systems(Update, update_dome)
            .add_systems(Startup, spawn_sky.in_set(crate::progress::PersistentSpawn))
            .add_systems(Update, (update_environment, follow_camera.after(crate::CameraSet).after(crate::experiments::apply_camera), debug_pitch.after(crate::CameraSet).after(crate::experiments::apply_camera).before(follow_camera)));
    }
}

fn is_pss(sim: &GameSim) -> bool {
    sim.course.request.id == "pss"
}

fn is_playground(sim: &GameSim) -> bool {
    matches!(sim.course.visual, CourseVisual::Playground(_))
}

fn spawn_sky(mut commands: Commands, sim: Res<GameSim>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<SkyMaterial>>) {
    if is_pss(&sim) {
        commands.insert_resource(ClearColor(Color::BLACK));
        return;
    }
    let radius = if is_playground(&sim) { 20000.0 } else { 30000.0 } * SCALE;
    let mesh: Mesh = Sphere::new(radius).mesh().uv(48, 24).into();
    commands.spawn((
        SkyDome,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(SkyMaterial::new(&SkyPalette::default(), 0.0))),
        Transform::default(),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

const SKY_SHADER: Handle<Shader> = weak_handle!("6f0c3b1e-5a1d-4c7e-9e42-5d7a1b9c2f10");

/// The dome's gradient (+ optional animated clouds) fragment shader, `sky_dome.wgsl`. Linear
/// colours; the dome is a double-sided sphere centred on the camera, so the view direction is
/// `world_position - camera`, which makes it independent of the dome's scale.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct SkyMaterial {
    #[uniform(0)]
    top: Vec4,
    #[uniform(1)]
    mid: Vec4,
    #[uniform(2)]
    bottom: Vec4,
    /// x: cloud amount, y: time (s).
    #[uniform(3)]
    params: Vec4,
}

fn linear(c: [f32; 3]) -> Vec4 {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    Vec4::new(l.red, l.green, l.blue, 1.0)
}

impl SkyMaterial {
    fn new(p: &SkyPalette, time: f32) -> Self {
        Self { top: linear(p.top), mid: linear(p.mid), bottom: linear(p.bottom), params: Vec4::new(p.clouds, time, 0.0, 0.0) }
    }
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        SKY_SHADER.into()
    }

    fn specialize(_: &MaterialPipeline<Self>, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        // Seen from inside.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Push the palette (and the cloud clock) into the dome's material.
fn update_dome(palette: Res<SkyPalette>, time: Res<Time>, domes: Query<&MeshMaterial3d<SkyMaterial>, With<SkyDome>>, mut materials: ResMut<Assets<SkyMaterial>>, mut forced: Local<Option<Option<f32>>>) {
    let forced = *forced.get_or_insert_with(|| std::env::var("JUMPMAN_SKY_CLOUDS").ok().and_then(|v| v.parse::<f32>().ok()));
    let mut palette = *palette;
    if let Some(c) = forced {
        palette.clouds = c;
    }
    for m in &domes {
        // Don't touch the asset (and re-upload) when nothing moves.
        let Some(mat) = materials.get(&m.0) else { continue };
        let next = SkyMaterial::new(&palette, if palette.clouds > 0.0 { time.elapsed_secs() } else { mat.params.y });
        if next.top != mat.top || next.mid != mat.mid || next.bottom != mat.bottom || next.params != mat.params {
            if let Some(mat) = materials.get_mut(&m.0) {
                *mat = next;
            }
        }
    }
}

/// Screenshot hook: `JUMPMAN_SKY_PITCH=<degrees>` tilts the camera up so the sky fills the frame;
/// `JUMPMAN_SKY_CLOUDS=<0..1>` forces the cloud coverage (see `update_dome`).
fn debug_pitch(mut camera: Single<&mut Transform, With<MainCamera>>, mut pitch: Local<Option<Option<f32>>>) {
    let deg = pitch.get_or_insert_with(|| std::env::var("JUMPMAN_SKY_PITCH").ok().and_then(|v| v.parse::<f32>().ok()));
    if let Some(deg) = *deg {
        let fwd = camera.forward().as_vec3();
        let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::NEG_Z);
        let a = deg.to_radians();
        let dir = flat * a.cos() + Vec3::Y * a.sin();
        camera.look_to(dir, Vec3::Y);
    }
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
pub fn update_environment(
    mut commands: Commands,
    sim: Res<GameSim>,
    mut env: ResMut<Environment>,
    mut particles: ResMut<Particles>,
    cameras: Query<Entity, With<MainCamera>>,
    mut inserted: Local<bool>,
    mut refresh: ResMut<FogRefresh>,
    camera_override: Res<crate::experiments::CameraOverride>,
) {
    let pos = sim.mario.pos;
    let water = sim.mario.world.water_level(pos[0], pos[2]);
    let underwater = water.is_finite() && pos[1] < water - 40.0;
    let mut changed = underwater != env.underwater || std::mem::take(&mut refresh.0);
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
    let fog = if underwater {
        linear_fog(UNDERWATER_COLOR, 120.0, 9000.0)
    } else if let Some(s) = sim.course.sky {
        let c = s.fog;
        DistanceFog { color: Color::srgb(c[0], c[1], c[2]), falloff: FogFalloff::Linear { start: (s.fog_start + env.fog_shift) * SCALE, end: (s.fog_end + env.fog_shift) * SCALE }, ..default() }
    } else {
        linear_fog(FOG_COLOR, near + env.fog_shift, far + env.fog_shift)
    };
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
