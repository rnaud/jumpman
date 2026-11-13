//! The N64 effect look for every minigame that has no hand-made `<mode>_fx.rs` restyle: one pass that needs no hooks in
//! the modes' own files.
//!
//! * **Particles.** The shared sprite pool (`fx::particles`: hit sparks, dust, landing puffs, box shards, splashes,
//!   coin sparkles, the bursts every mode fires through `Host::fx` / `host.particles`) is switched onto two tagged
//!   [`N64Fx`] materials (alpha and additive) while a minigame runs: chunky 3-tone puffs, spinning sparks, drops,
//!   shards and flame licks from the `PUFF` pattern. Each sprite's colour, fade, age, kind and seed travel in its
//!   `MeshTag`, so the whole pool shares two materials and batches.
//! * **Mirrored effect meshes.** A mode's own translucent unlit effect meshes (shockwave rings, hazard discs, portals,
//!   light columns, energy spheres, flames, storm walls) and its small unlit balls (shots, guide dots) are moved onto
//!   tagged materials picked by the mesh's shape (ring, disc, column, shell, ball sprite). The mode keeps animating its
//!   own `StandardMaterial` asset; every frame the colour and alpha are copied into the entity's tag, so fades, colour
//!   swaps and growth keep working untouched.
//!
//! Only entities of a running minigame (`XpEntity`, or a child of one) are touched, never the course, and the kart
//! (MK64's own textures) and Gem Clash (`brawl_fx`) are left out. With `JUMPMAN_FX_QUALITY=off` nothing runs.
use super::n64::{self, pack_tag, pat, Mount, N64Fx};
use super::particles::{Particles, ParticleSprite};
use crate::experiments::{ExperimentId, ExperimentSlot, XpEntity};
use bevy::mesh::{MeshTag, VertexAttributeValues};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub struct N64ModesPlugin;

impl Plugin for N64ModesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModesFx>().add_systems(PostUpdate, (switch, (particles, adopt, drive).chain().run_if(|s: Res<ModesFx>| s.on)).chain().before(bevy::camera::visibility::VisibilitySystems::CheckVisibility));
    }
}

/// Modes this pass styles (the kart keeps MK64's textures; Gem Clash and Star Swarm have their own restyles, but Swarm's
/// chests and particles still come through here).
fn styled(id: Option<ExperimentId>) -> bool {
    !matches!(id, None | Some(ExperimentId::Kart) | Some(ExperimentId::Brawl))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Shape {
    Ring,
    Disc,
    /// A tall thin column (light beams, pillars).
    Column,
    /// An open tube (storm walls).
    Tube,
    /// Any other closed 3D mesh (spheres, tori, cones).
    Shell,
    /// A small round mesh drawn opaque: becomes a shaded ball sprite.
    Ball,
}

#[derive(Clone, Copy, Debug)]
struct MeshInfo {
    shape: Shape,
    /// 1 / half extent along the axis the pattern reads.
    inv: f32,
    /// Flat in xz rather than xy (the pattern plane is swizzled).
    xz: bool,
    /// Radius (balls).
    radius: f32,
}

/// The source material a mirrored entity copies its colour and alpha from.
#[derive(Component)]
pub struct N64Mirror {
    src: Handle<StandardMaterial>,
    shape: Shape,
    sprite: bool,
}

/// The legacy particle material, kept to switch back when the minigame ends.
#[derive(Component)]
struct LegacyParticle(Handle<StandardMaterial>);

#[derive(Resource, Default)]
pub struct ModesFx {
    on: bool,
    /// The particle pool's materials (alpha, additive).
    puff: Option<(Handle<N64Fx>, Handle<N64Fx>)>,
    /// Per mesh: its class (None = left alone).
    class: HashMap<AssetId<Mesh>, Option<MeshInfo>>,
    /// Per (mesh, additive): the shared tagged material.
    mats: HashMap<(AssetId<Mesh>, bool), Handle<N64Fx>>,
    /// Quads for ball sprites, by radius in 1/100ths.
    quads: HashMap<u32, Handle<Mesh>>,
    /// Scan everything once after switching on (entities spawned before the mode was marked running).
    rescan: bool,
    /// Lit prop materials given a surface texture.
    textured: bevy::platform::collections::HashSet<AssetId<StandardMaterial>>,
    panel: Option<Handle<Image>>,
    speckle: Option<Handle<Image>>,
}

/// A 32-texel N64-style surface: lattice noise in four hard grey levels with a few dark specks, and on `panel` (boxes)
/// a bevelled rim, light on the top-left and dark on the bottom-right, so a plain cube reads as a built block. It
/// multiplies the prop's own colour, which keeps every mode's palette.
pub fn surface_texture(panel: bool) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: usize = 32;
    let hash = |x: i32, y: i32| {
        let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ 0x9e37_79b9;
        h ^= h >> 13;
        h = h.wrapping_mul(0x85eb_ca6b);
        h ^= h >> 16;
        (h & 0xffff) as f32 / 65535.0
    };
    // Value noise on an 8-cell lattice (tileable: the lattice wraps).
    let noise = |x: usize, y: usize| {
        let (fx, fy) = (x as f32 / 4.0, y as f32 / 4.0);
        let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
        let (tx, ty) = (fx - ix as f32, fy - iy as f32);
        let c = |a: i32, b: i32| hash(a.rem_euclid(8), b.rem_euclid(8));
        let top = c(ix, iy) + (c(ix + 1, iy) - c(ix, iy)) * tx;
        let bot = c(ix, iy + 1) + (c(ix + 1, iy + 1) - c(ix, iy + 1)) * tx;
        top + (bot - top) * ty
    };
    let mut data = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let n = noise(x, y) * 0.7 + hash(x as i32 * 3, y as i32 * 5) * 0.3;
            let mut v = 0.78 + (n * 4.0).floor() / 4.0 * 0.2;
            if hash(x as i32 + 77, y as i32 + 31) > 0.94 {
                v -= 0.16;
            }
            if panel {
                let (lo, hi) = (x.min(y), x.max(y));
                if lo < 2 && x + y < N {
                    v = 1.0;
                } else if hi >= N - 2 || lo < 2 {
                    v = 0.5;
                } else if lo < 3 || hi >= N - 3 {
                    v *= 0.88;
                }
            }
            let b = (v.clamp(0.0, 1.0) * 255.0) as u8;
            data.extend_from_slice(&[b, b, b, 255]);
        }
    }
    let mut img = Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    img.sampler = bevy::image::ImageSampler::nearest();
    img
}

fn switch(mut state: ResMut<ModesFx>, slot: Res<ExperimentSlot>, mut commands: Commands, legacy: Query<(Entity, &LegacyParticle)>) {
    let want = n64::enabled() && styled(slot.id);
    if want == state.on {
        return;
    }
    state.on = want;
    state.rescan = want;
    if !want {
        // Particles go back to their soft sprites; mirrored entities leave with the minigame.
        for (e, l) in &legacy {
            commands.entity(e).remove::<(MeshMaterial3d<N64Fx>, LegacyParticle)>().insert(MeshMaterial3d(l.0.clone()));
        }
    }
}

/// The particle kind as the `PUFF` pattern draws it, read off the burst's look.
fn puff_kind(p: &super::particles::Particle) -> (u32, bool) {
    let [r, g, b] = p.color;
    if p.gravity >= 500.0 && p.size <= 24.0 {
        (3, true)
    } else if b > r + 0.12 && b > 0.6 {
        (2, false)
    } else if r.max(g).max(b) < 0.45 {
        (1, false)
    } else if r > 0.85 && g < 0.6 && b < 0.35 {
        (5, true)
    } else if p.gravity >= 400.0 {
        (4, false)
    } else {
        (0, false)
    }
}

fn particles(
    mut commands: Commands,
    mut state: ResMut<ModesFx>,
    pool: Res<Particles>,
    mut mats: ResMut<Assets<N64Fx>>,
    mut sprites: Query<(Entity, &ParticleSprite, Option<&MeshMaterial3d<StandardMaterial>>, Option<&MeshMaterial3d<N64Fx>>, Option<&mut MeshTag>, &Visibility)>,
) {
    let (soft, add) = state
        .puff
        .get_or_insert_with(|| {
            let mut m = |additive| mats.add(N64Fx::new(pat::PUFF, Color::WHITE, additive).mount(Mount::Sprite).snap(2.0).tagged());
            (m(false), m(true))
        })
        .clone();
    for (e, sprite, legacy, cur, tag, vis) in &mut sprites {
        let i = sprite.index();
        let Some(p) = pool.pool.get(i) else { continue };
        if !p.alive && *vis == Visibility::Hidden && cur.is_some() {
            continue;
        }
        let (variant, additive) = puff_kind(p);
        let want = if additive { &add } else { &soft };
        let value = pack_tag(p.color, (p.opacity / 0.85).min(1.0), 1.0 - p.life / p.max_life.max(0.001), variant, i as f32 * 0.618);
        if let Some(l) = legacy {
            commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((LegacyParticle(l.0.clone()), MeshMaterial3d(want.clone()), MeshTag::new(value)));
            continue;
        }
        if cur.is_some_and(|c| c.0 != *want) {
            commands.entity(e).insert(MeshMaterial3d(want.clone()));
        }
        if let Some(mut t) = tag {
            if t.value != value {
                t.value = value;
            }
        }
    }
}

fn classify(mesh: &Mesh) -> Option<MeshInfo> {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let n = pos.len();
    if n <= 4 || n == 24 {
        // Quads, planes and boxes are panels or props, not effects.
        return None;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in pos {
        let v = Vec3::from(*p);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let ext = hi - lo;
    let big = ext.max_element().max(1e-6);
    let flat_z = ext.z < big * 0.01;
    let flat_y = ext.y < big * 0.01;
    if flat_z || flat_y {
        let radial = |p: &[f32; 3]| if flat_z { Vec2::new(p[0], p[1]).length() } else { Vec2::new(p[0], p[2]).length() };
        let rmax = pos.iter().map(radial).fold(0.0f32, f32::max).max(1e-6);
        let rmin = pos.iter().map(radial).fold(f32::MAX, f32::min);
        // An annulus has an inner rim well off the centre; a disc or fan is all rim (or has its centre point).
        let ring = rmin > rmax * 0.3 && pos.iter().any(|p| radial(p) < rmax * 0.97);
        return Some(MeshInfo { shape: if ring { Shape::Ring } else { Shape::Disc }, inv: 1.0 / rmax, xz: flat_y, radius: rmax });
    }
    let half = ext * 0.5;
    let round_xz = (ext.x - ext.z).abs() < ext.x.max(ext.z) * 0.08;
    if round_xz && ext.y > ext.x * 2.5 {
        return Some(MeshInfo { shape: Shape::Column, inv: 1.0 / half.y, xz: false, radius: half.x });
    }
    if round_xz {
        // An open tube: every vertex sits on the wall.
        let rmax = half.x.max(1e-6);
        let rmin = pos.iter().map(|p| Vec2::new(p[0], p[2]).length()).fold(f32::MAX, f32::min);
        if rmin > rmax * 0.9 && ext.y < ext.x {
            return Some(MeshInfo { shape: Shape::Tube, inv: 1.0 / half.y.max(1e-6), xz: false, radius: rmax });
        }
    }
    let sphere = (ext.x - ext.y).abs() < big * 0.08 && round_xz;
    Some(MeshInfo { shape: if sphere { Shape::Ball } else { Shape::Shell }, inv: 1.0 / half.max_element().max(1e-6), xz: false, radius: half.max_element() })
}

fn under_xp(e: Entity, parents: &Query<&ChildOf>, xp: &Query<(), With<XpEntity>>) -> bool {
    let mut at = e;
    for _ in 0..3 {
        if xp.contains(at) {
            return true;
        }
        match parents.get(at) {
            Ok(c) => at = c.parent(),
            Err(_) => return false,
        }
    }
    false
}

/// The shared tagged material for a mesh class.
fn class_material(info: &MeshInfo, additive: bool, ball_sprite: bool) -> N64Fx {
    let (pattern, mount) = match info.shape {
        Shape::Ring => (pat::RING, Mount::Mesh),
        Shape::Disc => (pat::ZONE, Mount::Mesh),
        Shape::Column => (pat::PILLAR, Mount::Mesh),
        Shape::Ball if ball_sprite => (pat::ICON, Mount::Sprite),
        Shape::Tube | Shape::Shell | Shape::Ball => (pat::SHELL, Mount::Mesh),
    };
    let mut fx = N64Fx::new(pattern, Color::WHITE, additive).mount(mount).snap(2.0).tagged();
    if mount == Mount::Mesh {
        fx = fx.extent(info.inv);
    }
    if info.xz {
        // The pattern plane is the mesh's xz (`n64.wesl` swizzles on tint.w).
        fx.tint.w = 2.0;
    }
    fx
}

#[allow(clippy::too_many_arguments)]
fn adopt_one(
    commands: &mut Commands,
    state: &mut ModesFx,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    mats: &mut Assets<N64Fx>,
    e: Entity,
    mesh: &Mesh3d,
    src: &MeshMaterial3d<StandardMaterial>,
) {
    let Some(m) = std_mats.get(&src.0) else { return };
    if m.base_color_texture.is_some() {
        return;
    }
    if !m.unlit {
        // A flat lit prop of the mode's own (blocks, pieces, stage shapes, chests): a low-res procedural texture.
        if matches!(m.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)) && state.textured.insert(src.0.id()) {
            let boxy = meshes.get(&mesh.0).is_some_and(|m| m.count_vertices() == 24);
            let tex = if boxy { state.panel.get_or_insert_with(|| images.add(surface_texture(true))).clone() } else { state.speckle.get_or_insert_with(|| images.add(surface_texture(false))).clone() };
            if let Some(mut m) = std_mats.get_mut(&src.0) {
                m.base_color_texture = Some(tex);
            }
        }
        return;
    }
    let translucent = matches!(m.alpha_mode, AlphaMode::Blend | AlphaMode::Add | AlphaMode::Premultiplied);
    let id = mesh.0.id();
    let info = match state.class.get(&id) {
        Some(c) => *c,
        None => {
            let c = meshes.get(id).and_then(classify);
            state.class.insert(id, c);
            c
        }
    };
    let Some(info) = info else { return };
    // Opaque unlit meshes: only small balls (shots, guide dots, orbs) are effects; coins, posts and props stay.
    let ball_sprite = !translucent && info.shape == Shape::Ball && info.radius < 1.5;
    if !translucent && !ball_sprite {
        return;
    }
    let additive = m.alpha_mode == AlphaMode::Add;
    let key = (id, additive);
    let mat = match state.mats.get(&key) {
        Some(h) => h.clone(),
        None => {
            let fx = class_material(&info, additive, ball_sprite);
            let h = mats.add(fx);
            state.mats.insert(key, h.clone());
            h
        }
    };
    let mut ec = commands.entity(e);
    ec.remove::<MeshMaterial3d<StandardMaterial>>().insert((MeshMaterial3d(mat), MeshTag::new(0), N64Mirror { src: src.0.clone(), shape: if ball_sprite { Shape::Ball } else { info.shape }, sprite: ball_sprite }));
    if ball_sprite {
        // A camera-facing quad as big as the ball.
        let r = (info.radius * 100.0).round() as u32;
        let quad = state.quads.entry(r).or_insert_with(|| meshes.add(Rectangle::new(2.0 * info.radius, 2.0 * info.radius))).clone();
        ec.insert(Mesh3d(quad));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn adopt(
    mut commands: Commands,
    mut state: ResMut<ModesFx>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<N64Fx>>,
    changed: Query<(Entity, &Mesh3d, &MeshMaterial3d<StandardMaterial>), (Changed<MeshMaterial3d<StandardMaterial>>, Without<ParticleSprite>, Without<super::FxSprite>)>,
    all: Query<(Entity, &Mesh3d, &MeshMaterial3d<StandardMaterial>), (Without<ParticleSprite>, Without<super::FxSprite>)>,
    parents: Query<&ChildOf>,
    xp: Query<(), With<XpEntity>>,
) {
    let state = &mut *state;
    if std::mem::take(&mut state.rescan) {
        for (e, mesh, src) in &all {
            if under_xp(e, &parents, &xp) {
                adopt_one(&mut commands, state, &mut meshes, &mut std_mats, &mut images, &mut mats, e, mesh, src);
            }
        }
    } else {
        for (e, mesh, src) in &changed {
            if under_xp(e, &parents, &xp) {
                adopt_one(&mut commands, state, &mut meshes, &mut std_mats, &mut images, &mut mats, e, mesh, src);
            }
        }
    }
}

/// The disc look by colour: red-orange danger, green poison, blue-purple portal, else danger in its own colour.
fn disc_variant(c: [f32; 3]) -> u32 {
    let [r, g, b] = c;
    if g > r * 1.15 && g > b * 1.15 {
        1
    } else if b > r * 0.9 && b > g * 1.1 {
        3
    } else {
        4
    }
}

/// Copy each mirrored entity's live colour and alpha into its tag.
fn drive(std_mats: Res<Assets<StandardMaterial>>, mut q: Query<(Entity, &N64Mirror, &mut MeshTag)>) {
    for (e, m, mut tag) in &mut q {
        let Some(src) = std_mats.get(&m.src) else { continue };
        let c = src.base_color.to_srgba();
        let e_lin = src.emissive;
        // A dark base lit by emission (flames) shows its emissive colour.
        let rgb = if c.red.max(c.green).max(c.blue) < 0.2 && e_lin.red.max(e_lin.green).max(e_lin.blue) > 0.2 {
            let s = Color::LinearRgba(LinearRgba::rgb(e_lin.red.min(1.0), e_lin.green.min(1.0), e_lin.blue.min(1.0))).to_srgba();
            [s.red, s.green, s.blue]
        } else {
            [c.red, c.green, c.blue]
        };
        let hot = rgb[0] > 0.8 && rgb[1] < 0.75 && rgb[2] < 0.4;
        let (variant, intensity) = match m.shape {
            Shape::Ring => (0, (c.alpha * 1.4).min(1.0)),
            Shape::Disc => (disc_variant(rgb), (c.alpha * 1.6).min(1.0)),
            Shape::Column => (0, (c.alpha * 1.7).min(1.0)),
            Shape::Tube => (3, (c.alpha * 1.6).min(1.0)),
            Shape::Shell => (if hot { 2 } else { 0 }, (c.alpha * 1.5).min(1.0)),
            Shape::Ball if m.sprite => (4, c.alpha),
            Shape::Ball => (if hot { 2 } else { 0 }, (c.alpha * 1.5).min(1.0)),
        };
        let value = pack_tag(rgb, intensity, 0.0, variant, (e.index_u32() as f32 * 0.618).fract());
        if tag.value != value {
            tag.value = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(m: impl Into<Mesh>) -> Option<Shape> {
        classify(&m.into()).map(|i| i.shape)
    }

    #[test]
    fn meshes_are_told_apart() {
        assert_eq!(shape(Annulus::new(0.84, 1.0)), Some(Shape::Ring));
        assert_eq!(shape(Circle::new(1.0)), Some(Shape::Disc));
        assert_eq!(shape(Cylinder::new(0.26, 13.0)), Some(Shape::Column));
        assert_eq!(shape(Sphere::new(0.4).mesh().ico(1).unwrap()), Some(Shape::Ball));
        assert_eq!(shape(Torus::new(0.6, 1.0)), Some(Shape::Shell));
        assert_eq!(shape(Cuboid::new(1.0, 2.0, 1.0)), None, "boxes are props");
        assert_eq!(shape(Rectangle::new(1.0, 1.0)), None, "quads are panels");
        let flat = classify(&Mesh::from(Circle::new(2.0)).rotated_by(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))).unwrap();
        assert!(flat.xz && (flat.inv - 0.5).abs() < 1e-3, "a disc lying in xz reads its pattern in xz");
    }

    #[test]
    fn particles_pick_a_kind_from_their_burst() {
        let mut p = Particles::new(8);
        p.sparks([0.0; 3], 1);
        assert_eq!(puff_kind(&p.pool[0]), (3, true));
        p.splash([0.0; 3]);
        assert_eq!(puff_kind(&p.pool[1]).0, 2);
        let mut d = Particles::new(2);
        d.dust([0.0; 3], 1);
        assert_eq!(puff_kind(&d.pool[0]), (0, false));
    }

    #[test]
    fn discs_pick_a_look_by_colour() {
        assert_eq!(disc_variant([1.0, 0.3, 0.2]), 4);
        assert_eq!(disc_variant([0.3, 1.0, 0.3]), 1);
        assert_eq!(disc_variant([0.5, 0.3, 1.0]), 3);
    }

    #[test]
    fn surface_textures_tile_and_bevel() {
        let panel = surface_texture(true);
        let d = panel.data.as_ref().unwrap();
        assert_eq!(d.len(), 32 * 32 * 4);
        let at = |x: usize, y: usize| d[(y * 32 + x) * 4];
        assert!(at(0, 5) > at(31, 20), "light top-left rim, dark bottom-right rim");
        assert!(surface_texture(false).data.as_ref().unwrap().chunks(4).all(|p| p[0] > 90));
    }

    #[test]
    fn only_running_minigames_are_styled() {
        assert!(!styled(None));
        assert!(!styled(Some(ExperimentId::Kart)));
        assert!(!styled(Some(ExperimentId::Brawl)));
        assert!(styled(Some(ExperimentId::Relic)) && styled(Some(ExperimentId::Party)));
    }
}
