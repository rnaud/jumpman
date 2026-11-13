//! The shared N64-look effect system: one custom material (`n64.wesl`) behind every Star Swarm weapon and Gem Clash
//! attack visual, plus the low-poly meshes it likes and a quality switch.
//!
//! What it fakes, all procedurally (no art, no textures): vertices snapped to a coarse screen grid with the odd
//! jump at 15 fps, 16-texel noise "textures" with the 3-point filter flipped at 15 fps, hard colour-ramp steps with
//! a 4x4 ordered dither, camera-facing sprites and ribbons, additive or alpha blending. Plain vertex + fragment
//! code, no compute and no storage buffers, so it is the same on Metal, GLES and Vulkan.
//!
//! Quality (`JUMPMAN_FX_QUALITY=off|low|high`, default `high` on desktop and `low` on phones): `high` has the
//! vertex jitter, dither and the extra layers (stars behind flashes, twig lightning), `low` keeps the look but drops
//! them, `off` leaves the original flat unlit materials (the CPU-side fallback: nothing here is created).
use bevy::asset::{load_internal_asset, weak_handle};
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use std::sync::atomic::{AtomicU8, Ordering};

const N64_SHADER: Handle<Shader> = weak_handle!("3b7e5c10-9d42-4f18-a6e3-71c0d2b84e55");

/// Pattern numbers of `n64.wesl`.
pub mod pat {
    pub const RING: f32 = 0.0;
    pub const ZONE: f32 = 1.0;
    pub const SLASH: f32 = 2.0;
    pub const BURST: f32 = 3.0;
    pub const STAR: f32 = 4.0;
    pub const BLADE: f32 = 5.0;
    pub const ORB: f32 = 6.0;
    pub const BOOM: f32 = 7.0;
    pub const POP: f32 = 8.0;
    pub const BEAM: f32 = 9.0;
    pub const AURA: f32 = 10.0;
    pub const PILLAR: f32 = 11.0;
    /// Sprite particles: 0 dust, 1 smoke, 2 water drop, 3 spark, 4 shard, 5 flame.
    pub const PUFF: f32 = 12.0;
    /// Pickup sprites: 0 gem, 1 heart, 2 chest, 3 coin, 4 ball.
    pub const ICON: f32 = 13.0;
    /// Glowing 3D shells: 0 energy, 1 shield bubble, 2 flame, 3 storm wall.
    pub const SHELL: f32 = 14.0;
}

/// Per-entity effect state for a "tagged" material ([`N64Fx::tagged`]), packed into the entity's
/// `bevy::mesh::MeshTag` so a whole pool shares one material (and batches): sRGB colour at 5 bits a channel (the
/// console's RGBA5551), intensity and progress at 5 bits, a 3-bit variant and a 4-bit seed.
pub fn pack_tag(srgb: [f32; 3], intensity: f32, progress: f32, variant: u32, seed: f32) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 31.0).round() as u32;
    q(srgb[0]) | q(srgb[1]) << 5 | q(srgb[2]) << 10 | q(intensity) << 15 | q(progress) << 20 | (variant & 7) << 25 | (((seed.rem_euclid(1.0)) * 16.0) as u32 & 15) << 28
}

/// How the effects are drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    /// The original flat unlit materials.
    Off = 0,
    /// The look without the vertex jitter, dither and extra layers.
    Low = 1,
    High = 2,
}

static QUALITY: AtomicU8 = AtomicU8::new(2);

pub fn quality() -> Quality {
    match QUALITY.load(Ordering::Relaxed) {
        0 => Quality::Off,
        1 => Quality::Low,
        _ => Quality::High,
    }
}

/// Change the tier at run time (the next frame re-tints every material).
pub fn set_quality(q: Quality) {
    QUALITY.store(q as u8, Ordering::Relaxed);
}

/// Anything but `off`.
pub fn enabled() -> bool {
    quality() != Quality::Off
}

fn default_quality() -> Quality {
    match std::env::var("JUMPMAN_FX_QUALITY").ok().as_deref() {
        Some("off") | Some("0") => Quality::Off,
        Some("low") | Some("1") => Quality::Low,
        Some("high") | Some("2") => Quality::High,
        _ if cfg!(any(target_os = "ios", target_os = "android")) => Quality::Low,
        _ => Quality::High,
    }
}

/// Linear colour as a uniform.
pub fn tint(c: Color) -> Vec4 {
    let l = c.to_linear();
    Vec4::new(l.red, l.green, l.blue, 1.0)
}

/// The effect material. `tint`/`progress`/`intensity` change per effect, the rest is the look.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, PartialEq)]
pub struct N64Fx {
    #[uniform(0)]
    pub tint: Vec4,
    /// x pattern, y progress through the effect's life, z seed, w intensity.
    #[uniform(1)]
    pub p0: Vec4,
    /// x quality, y billboard (0 none, 1 sprite, 2 ribbon), z vertex snap in pixels, w 1 / half extent.
    #[uniform(2)]
    pub p1: Vec4,
    /// x variant, y spin (rad/s), z 1 = tagged (state comes from the entity's `MeshTag`), w additive.
    #[uniform(3)]
    pub p2: Vec4,
}

/// How a pattern is placed: sprite kind and blending.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mount {
    /// A mesh in the world (flat ring, disc, sphere, pillar).
    Mesh,
    /// A camera-facing quad.
    Sprite,
    /// A camera-facing strip along the entity's z axis.
    Ribbon,
}

impl N64Fx {
    pub fn new(pattern: f32, color: Color, additive: bool) -> Self {
        Self { tint: tint(color), p0: Vec4::new(pattern, 0.0, 0.0, 1.0), p1: Vec4::new(quality() as u8 as f32, 0.0, 0.0, 1.0), p2: Vec4::new(0.0, 0.0, 0.0, if additive { 1.0 } else { 0.0 }) }
    }
    pub fn mount(mut self, m: Mount) -> Self {
        self.p1.y = match m {
            Mount::Mesh => 0.0,
            Mount::Sprite => 1.0,
            Mount::Ribbon => 2.0,
        };
        self
    }
    /// Vertex snap cell in pixels (0 = none).
    pub fn snap(mut self, px: f32) -> Self {
        self.p1.z = px;
        self
    }
    /// 1 / half extent of the mesh along the axis the pattern reads (pillars).
    pub fn extent(mut self, inv: f32) -> Self {
        self.p1.w = inv;
        self
    }
    pub fn variant(mut self, v: u32) -> Self {
        self.p2.x = v as f32;
        self
    }
    pub fn spin(mut self, rad_per_sec: f32) -> Self {
        self.p2.y = rad_per_sec;
        self
    }
    /// Read colour, intensity, progress, variant and seed per entity from its `MeshTag` ([`pack_tag`]); the
    /// material's tint then multiplies the tag colour (leave it white).
    pub fn tagged(mut self) -> Self {
        self.p2.z = 1.0;
        self
    }
    pub fn seed(mut self, s: f32) -> Self {
        self.p0.z = s;
        self
    }
    /// Push one frame's state; true when something changed (so the caller only touches the asset then).
    pub fn set(&mut self, color: [f32; 3], progress: f32, intensity: f32) -> bool {
        let t = tint(Color::srgb(color[0], color[1], color[2]));
        let (k, i) = (progress.clamp(0.0, 1.0), intensity.clamp(0.0, 1.0));
        // Whole 1/64ths of progress are plenty for a 15 fps look and keep the asset from churning.
        let k = (k * 64.0).round() / 64.0;
        let i = (i * 32.0).round() / 32.0;
        if t == self.tint && k == self.p0.y && i == self.p0.w {
            return false;
        }
        self.tint = t;
        self.p0.y = k;
        self.p0.w = i;
        true
    }
    pub fn is_additive(&self) -> bool {
        self.p2.w > 0.5
    }
}

impl Material for N64Fx {
    fn vertex_shader() -> ShaderRef {
        N64_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        N64_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.is_additive() {
            AlphaMode::Add
        } else {
            AlphaMode::Blend
        }
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Shared low-poly meshes (faceted, as the console would draw them).
#[derive(Resource, Clone)]
pub struct N64Meshes {
    /// Camera-facing quad, half extent 1.
    pub quad: Handle<Mesh>,
    /// Beam strip: x -1..1 across, y -0.5..0.5 along.
    pub ribbon: Handle<Mesh>,
    /// Thin flat ring (inner 0.78), 20 sides.
    pub ring: Handle<Mesh>,
    /// Flat disc, 14 sides.
    pub disc: Handle<Mesh>,
    /// A 132 degree pie slice pointing +y, 8 steps.
    pub sector: Handle<Mesh>,
    /// A 80-triangle sphere of radius 1.
    pub sphere: Handle<Mesh>,
    /// A 6-sided pillar, radius 1, height 1.
    pub pillar: Handle<Mesh>,
}

impl FromWorld for N64Meshes {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        Self {
            quad: meshes.add(Rectangle::new(2.0, 2.0)),
            ribbon: meshes.add(Rectangle::new(2.0, 1.0)),
            ring: meshes.add(Annulus::new(0.78, 1.0).mesh().resolution(20)),
            disc: meshes.add(Circle::new(1.0).mesh().resolution(14)),
            sector: meshes.add(CircularSector::new(1.0, 1.15).mesh().resolution(8)),
            sphere: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
            pillar: meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(6)),
        }
    }
}

pub struct N64Plugin;

impl Plugin for N64Plugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, N64_SHADER, "n64.wesl", Shader::from_wesl);
        set_quality(default_quality());
        app.add_plugins(MaterialPlugin::<N64Fx>::default()).init_resource::<N64Meshes>().add_systems(Update, apply_quality);
    }
}

/// Re-tier every material when [`set_quality`] changed the switch.
fn apply_quality(mut last: Local<Option<Quality>>, mut materials: ResMut<Assets<N64Fx>>) {
    let now = quality();
    let prev = last.replace(now);
    if prev.is_none() || prev == Some(now) {
        return;
    }
    let ids: Vec<_> = materials.ids().collect();
    for id in ids {
        if let Some(mut m) = materials.get_mut(id) {
            m.p1.x = now as u8 as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_only_reports_real_changes() {
        let mut m = N64Fx::new(pat::RING, Color::WHITE, true);
        assert!(m.set([1.0, 0.5, 0.2], 0.5, 1.0));
        assert!(!m.set([1.0, 0.5, 0.2], 0.5, 1.0), "same state is not a change");
        assert!(!m.set([1.0, 0.5, 0.2], 0.5 + 1.0 / 400.0, 1.0), "sub-step progress is quantised away");
        assert!(m.set([1.0, 0.5, 0.2], 0.9, 1.0));
        assert!(m.is_additive());
    }

    #[test]
    fn tags_pack_every_field() {
        let t = pack_tag([1.0, 0.0, 0.5], 1.0, 0.0, 5, 0.5);
        assert_eq!(t & 31, 31);
        assert_eq!((t >> 5) & 31, 0);
        assert_eq!((t >> 10) & 31, 16);
        assert_eq!((t >> 15) & 31, 31);
        assert_eq!((t >> 20) & 31, 0);
        assert_eq!((t >> 25) & 7, 5);
        assert_eq!(t >> 28, 8);
        assert_eq!(pack_tag([2.0, -1.0, 0.0], 0.5, 1.0, 9, 1.25) >> 25 & 7, 1, "variant wraps, values clamp");
    }

    #[test]
    fn quality_switch_round_trips() {
        let before = quality();
        set_quality(Quality::Off);
        assert!(!enabled());
        set_quality(Quality::Low);
        assert_eq!(quality(), Quality::Low);
        set_quality(before);
    }

    #[test]
    fn shader_has_every_pattern() {
        let src = include_str!("n64.wesl");
        for (name, id) in [("RING", pat::RING), ("ZONE", pat::ZONE), ("SLASH", pat::SLASH), ("BURST", pat::BURST), ("STAR", pat::STAR), ("BLADE", pat::BLADE), ("ORB", pat::ORB), ("BOOM", pat::BOOM), ("POP", pat::POP), ("BEAM", pat::BEAM), ("AURA", pat::AURA), ("PILLAR", pat::PILLAR), ("PUFF", pat::PUFF), ("ICON", pat::ICON), ("SHELL", pat::SHELL)] {
            assert!(src.contains(&format!("case {}:", id as u32)), "pattern {name} is not dispatched");
        }
    }
}
