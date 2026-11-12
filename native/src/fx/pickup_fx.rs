//! Collectible presentation shared by the Zelda drops (rupees, hearts, ammo) and Banjo's pickups (notes, eggs,
//! feathers, honeycombs, Jiggies): the item shapes, generated in code, and the feedback when one is taken.
//!
//! * [`Look`] names every collectible; [`PickupLooks`] holds one mesh and one material per look (built once at
//!   start-up, shared by every instance).
//! * [`PickupFx`] is a small fixed pool of effects: the *pop* of a collected item (it jumps up, spins up and shrinks
//!   away), four-point *sparkles* (additive, twinkling), tumbling *shards* (cut grass, pot and crate pieces) and
//!   camera-facing *rings*. Everything is bounded and allocation-free per frame; nothing is drawn while idle.
//!
//! The shapes, the pool and the animation curves are engine-free and unit tested.
use crate::camera::MainCamera;
use crate::fx::particles::Rng;
use crate::render::SCALE;
use bevy::asset::RenderAssetUsages;
use jumpman_sim::collectible_theme::Skin;
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::{PI, TAU};

pub struct PickupFxPlugin;

impl Plugin for PickupFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PickupFx>()
            .add_systems(Startup, setup.in_set(crate::progress::PersistentSpawn))
            .add_systems(Update, (step, draw).chain().after(crate::CameraSet).after(crate::experiments::apply_camera));
    }
}

// ---------------------------------------------------------------------------------------------
// Shapes

/// A flat-shaded triangle soup.
#[derive(Default, Clone, Debug)]
pub struct Tris {
    pub pos: Vec<[f32; 3]>,
    pub nor: Vec<[f32; 3]>,
}

impl Tris {
    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3) {
        let n = (b - a).cross(c - a);
        if n.length_squared() < 1e-12 {
            return;
        }
        let n = n.normalize().to_array();
        self.pos.extend([a.to_array(), b.to_array(), c.to_array()]);
        self.nor.extend([n, n, n]);
    }

    /// A faceted lens: the `outline` (counter-clockwise seen from +z, star-shaped about `centre`) is the rim at
    /// z = 0; the front and back faces are the outline shrunk by `inset` towards `centre` at z = +-`depth`.
    /// `inset` 1 gives a plain prism.
    pub fn lens(&mut self, outline: &[Vec2], centre: Vec2, depth: f32, inset: f32) {
        let n = outline.len();
        for side in [1.0f32, -1.0] {
            let z = depth * side;
            let c = centre.extend(z);
            for i in 0..n {
                let (o0, o1) = (outline[i], outline[(i + 1) % n]);
                let (i0, i1) = ((centre + (o0 - centre) * inset).extend(z), (centre + (o1 - centre) * inset).extend(z));
                let (r0, r1) = (o0.extend(0.0), o1.extend(0.0));
                if side > 0.0 {
                    self.tri(c, i0, i1);
                    self.tri(i0, r0, r1);
                    self.tri(i0, r1, i1);
                } else {
                    self.tri(c, i1, i0);
                    self.tri(i0, r1, r0);
                    self.tri(i0, i1, r1);
                }
            }
        }
    }

    /// A smooth ellipsoid.
    pub fn ellipsoid(&mut self, centre: Vec3, radii: Vec3, rot: Quat, segs: usize, rings: usize) {
        let at = |i: usize, j: usize| {
            let (th, ph) = (i as f32 / segs as f32 * TAU, j as f32 / rings as f32 * PI);
            let d = Vec3::new(ph.sin() * th.cos(), ph.cos(), ph.sin() * th.sin());
            (centre + rot * (d * radii), (rot * (d / radii)).normalize())
        };
        for j in 0..rings {
            for i in 0..segs {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                for (p, q, r) in [(a, b, c), (a, c, d)] {
                    if (q.0 - p.0).cross(r.0 - p.0).length_squared() < 1e-12 {
                        continue;
                    }
                    self.pos.extend([p.0.to_array(), q.0.to_array(), r.0.to_array()]);
                    self.nor.extend([p.1.to_array(), q.1.to_array(), r.1.to_array()]);
                }
            }
        }
    }

    /// Move the bounding box's centre to the origin and scale uniformly so its half-height is one.
    pub fn centre_unit(&mut self) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &self.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if self.pos.is_empty() || hi[1] <= lo[1] {
            return;
        }
        let k = 2.0 / (hi[1] - lo[1]);
        for p in &mut self.pos {
            for a in 0..3 {
                p[a] = (p[a] - (lo[a] + hi[a]) * 0.5) * k;
            }
        }
    }

    pub fn mesh(self) -> Mesh {
        // MAIN_WORLD must be kept: ray-traced lighting (and any later edit) reads the CPU copy; a RENDER_WORLD-only mesh
        // is emptied once extracted and every access to its attributes panics ("Mesh has been extracted to RenderWorld").
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        let uv = vec![[0.0f32, 0.0]; self.pos.len()];
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        m
    }
}

/// The rupee: a tall six-sided gem.
pub fn gem_outline() -> Vec<Vec2> {
    [(0.0, 1.0), (-0.5, 0.6), (-0.5, -0.6), (0.0, -1.0), (0.5, -0.6), (0.5, 0.6)].iter().map(|p| Vec2::new(p.0, p.1)).collect()
}

pub fn heart_outline(n: usize) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let t = -(i as f32 / n as f32) * TAU;
            let x = 16.0 * t.sin().powi(3);
            let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
            Vec2::new(x, y + 2.5) / 16.0
        })
        .collect()
}

pub fn polygon_outline(n: usize, r: f32, turn: f32) -> Vec<Vec2> {
    (0..n).map(|i| Vec2::from_angle(turn + i as f32 / n as f32 * TAU) * r).collect()
}

/// A jigsaw piece: a square with a knob on the right and top edges and a matching bite out of the bottom and left.
pub fn jigsaw_outline() -> Vec<Vec2> {
    const R: f32 = 0.34;
    const N: usize = 7;
    let mut out = vec![Vec2::new(-1.0, -1.0)];
    let arc = |out: &mut Vec<Vec2>, c: Vec2, a0: f32, a1: f32| {
        for i in 0..=N {
            let a = a0 + (a1 - a0) * i as f32 / N as f32;
            out.push(c + Vec2::from_angle(a) * R);
        }
    };
    // the bites stop where the arc turns back on the centre (asin R), so the fan from the middle never folds over
    const LIP: f32 = 0.36;
    arc(&mut out, Vec2::new(0.0, -1.0), PI - LIP, LIP);
    out.push(Vec2::new(1.0, -1.0));
    arc(&mut out, Vec2::new(1.0, 0.0), -PI / 2.0, PI / 2.0);
    out.push(Vec2::new(1.0, 1.0));
    arc(&mut out, Vec2::new(0.0, 1.0), 0.0, PI);
    out.push(Vec2::new(-1.0, 1.0));
    arc(&mut out, Vec2::new(-1.0, 0.0), PI / 2.0 - LIP, -PI / 2.0 + LIP);
    out.iter().map(|p| *p / (1.0 + R)).collect()
}

/// A feather: a slim leaf with a slight bend, quill down.
pub fn feather_outline(n: usize) -> Vec<Vec2> {
    let side = |t: f32| 0.36 * (PI * t.powf(0.75)).sin().max(0.0) + 0.03;
    let bend = |t: f32| 0.14 * (PI * t).sin();
    let mut out = vec![];
    for i in 0..=n {
        let t = i as f32 / n as f32;
        out.push(Vec2::new(bend(t) + side(t), -1.0 + 2.0 * t));
    }
    for i in (0..=n).rev() {
        let t = i as f32 / n as f32;
        out.push(Vec2::new(bend(t) - side(t), -1.0 + 2.0 * t));
    }
    out
}

/// Twice the signed area (positive = counter-clockwise).
pub fn signed_area(outline: &[Vec2]) -> f32 {
    (0..outline.len()).map(|i| outline[i].perp_dot(outline[(i + 1) % outline.len()])).sum()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Shape {
    Gem,
    Heart,
    Note,
    Egg,
    Feather,
    Honeycomb,
    Jiggy,
    Coin,
    Orb,
    Jar,
}

pub const SHAPES: [Shape; 10] = [Shape::Gem, Shape::Heart, Shape::Note, Shape::Egg, Shape::Feather, Shape::Honeycomb, Shape::Jiggy, Shape::Coin, Shape::Orb, Shape::Jar];

/// The triangles of a shape, about one unit from the centre to the top.
pub fn shape_tris(shape: Shape) -> Tris {
    let mut t = Tris::default();
    match shape {
        Shape::Gem => t.lens(&gem_outline(), Vec2::ZERO, 0.26, 0.55),
        Shape::Heart => t.lens(&heart_outline(28), Vec2::ZERO, 0.34, 0.6),
        Shape::Note => {
            // an eighth note: the tilted head, the stem and the flag
            t.ellipsoid(Vec3::new(-0.2, -0.68, 0.0), Vec3::new(0.46, 0.31, 0.3), Quat::from_rotation_z(0.38), 12, 8);
            let stem = [Vec2::new(0.14, -0.62), Vec2::new(0.28, -0.62), Vec2::new(0.28, 1.0), Vec2::new(0.14, 1.0)];
            t.lens(&stem, Vec2::new(0.21, 0.2), 0.1, 1.0);
            let flag = [Vec2::new(0.28, 1.0), Vec2::new(0.28, 0.5), Vec2::new(0.52, 0.42), Vec2::new(0.78, 0.1), Vec2::new(0.72, 0.52)];
            t.lens(&flag, Vec2::new(0.5, 0.56), 0.09, 1.0);
        }
        Shape::Egg => {
            // blunt below, pointed above
            t.ellipsoid(Vec3::new(0.0, -0.1, 0.0), Vec3::new(0.72, 0.9, 0.72), Quat::IDENTITY, 14, 10);
            t.ellipsoid(Vec3::new(0.0, 0.32, 0.0), Vec3::new(0.56, 0.68, 0.56), Quat::IDENTITY, 14, 8);
        }
        Shape::Feather => {
            t.lens(&feather_outline(8), Vec2::ZERO, 0.07, 0.35);
        }
        Shape::Honeycomb => t.lens(&polygon_outline(6, 1.0, 0.0), Vec2::ZERO, 0.3, 0.72),
        Shape::Jiggy => t.lens(&jigsaw_outline(), Vec2::ZERO, 0.2, 0.86),
        Shape::Coin => t.lens(&polygon_outline(12, 1.0, 0.0), Vec2::ZERO, 0.16, 0.8),
        Shape::Orb => t.ellipsoid(Vec3::ZERO, Vec3::splat(0.9), Quat::IDENTITY, 14, 10),
        Shape::Jar => {
            t.ellipsoid(Vec3::new(0.0, -0.25, 0.0), Vec3::new(0.7, 0.72, 0.7), Quat::IDENTITY, 12, 8);
            t.lens(&polygon_outline(8, 0.36, 0.0), Vec2::ZERO, 0.0, 1.0);
            let neck: Vec<Vec2> = [(-0.3, 0.3), (0.3, 0.3), (0.38, 0.95), (-0.38, 0.95)].iter().map(|p| Vec2::new(p.0, p.1)).collect();
            t.lens(&neck, Vec2::new(0.0, 0.6), 0.3, 0.9);
        }
    }
    // the currency skins turn about, and are placed by, their centre (`fx::anchor`): centred on the origin, one unit of half-height
    if matches!(shape, Shape::Gem | Shape::Note | Shape::Coin) {
        t.centre_unit();
    }
    t
}

/// Every collectible the two modes draw.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Look {
    RupeeGreen,
    RupeeBlue,
    RupeeRed,
    RupeePurple,
    RupeeGold,
    /// The 100-rupee silver one (seen only as Link's version of a big coin).
    RupeeSilver,
    /// The SM64 coin in its three colours, drawn for Mario in the Zelda and Banjo worlds.
    CoinYellow,
    CoinRed,
    CoinBlue,
    Heart,
    HeartPiece,
    Fairy,
    Magic,
    Ammo,
    Bomb,
    Key,
    Token,
    Note,
    Egg,
    RedFeather,
    GoldFeather,
    Honeycomb,
    Jiggy,
    MumboToken,
}

pub const LOOKS: [Look; 24] = [
    Look::RupeeGreen,
    Look::RupeeBlue,
    Look::RupeeRed,
    Look::RupeePurple,
    Look::RupeeGold,
    Look::RupeeSilver,
    Look::CoinYellow,
    Look::CoinRed,
    Look::CoinBlue,
    Look::Heart,
    Look::HeartPiece,
    Look::Fairy,
    Look::Magic,
    Look::Ammo,
    Look::Bomb,
    Look::Key,
    Look::Token,
    Look::Note,
    Look::Egg,
    Look::RedFeather,
    Look::GoldFeather,
    Look::Honeycomb,
    Look::Jiggy,
    Look::MumboToken,
];

impl Look {
    pub fn shape(self) -> Shape {
        use Look::*;
        match self {
            RupeeGreen | RupeeBlue | RupeeRed | RupeePurple | RupeeGold | RupeeSilver => Shape::Gem,
            CoinYellow | CoinRed | CoinBlue => Shape::Coin,
            Heart | HeartPiece => Shape::Heart,
            Fairy | Ammo | Bomb => Shape::Orb,
            Magic => Shape::Jar,
            Key | Token | MumboToken => Shape::Coin,
            Note => Shape::Note,
            Egg => Shape::Egg,
            RedFeather | GoldFeather => Shape::Feather,
            Honeycomb => Shape::Honeycomb,
            Jiggy => Shape::Jiggy,
        }
    }

    pub fn rgb(self) -> [f32; 3] {
        use Look::*;
        match self {
            RupeeGreen => [0.1, 0.85, 0.25],
            RupeeBlue => [0.15, 0.4, 1.0],
            RupeeRed => [1.0, 0.15, 0.12],
            RupeePurple => [0.7, 0.2, 0.95],
            RupeeGold => [1.0, 0.75, 0.1],
            RupeeSilver => [0.82, 0.86, 0.95],
            CoinYellow => [1.0, 0.82, 0.1],
            CoinRed => [1.0, 0.16, 0.1],
            CoinBlue => [0.18, 0.42, 1.0],
            Heart => [0.95, 0.12, 0.18],
            HeartPiece => [1.0, 0.45, 0.6],
            Fairy => [1.0, 0.7, 0.9],
            Magic => [0.2, 0.85, 0.35],
            Ammo => [0.82, 0.66, 0.3],
            Bomb => [0.16, 0.2, 0.42],
            Key => [0.8, 0.82, 0.86],
            Token => [1.0, 0.8, 0.15],
            Note => [1.0, 0.8, 0.12],
            Egg => [0.62, 0.8, 1.0],
            RedFeather => [0.95, 0.14, 0.12],
            GoldFeather => [1.0, 0.76, 0.1],
            Honeycomb => [1.0, 0.6, 0.08],
            Jiggy => [1.0, 0.78, 0.1],
            MumboToken => [0.86, 0.86, 0.78],
        }
    }

    /// Centre-to-top size in sim units.
    pub fn size(self) -> f32 {
        use Look::*;
        match self {
            RupeeGreen | RupeeBlue | RupeeRed => 19.0,
            RupeePurple | RupeeGold | RupeeSilver => 24.0,
            CoinYellow | CoinRed | CoinBlue => 22.0,
            Heart => 15.0,
            HeartPiece => 24.0,
            Fairy => 11.0,
            Magic => 15.0,
            Ammo | Bomb => 13.0,
            Key | Token => 17.0,
            Note => 24.0,
            Egg => 19.0,
            RedFeather | GoldFeather => 30.0,
            Honeycomb => 26.0,
            Jiggy => 34.0,
            MumboToken => 22.0,
        }
    }

    /// The picture of a themed currency pickup (`jumpman_sim::collectible_theme`).
    pub fn of_skin(skin: Skin) -> Look {
        match skin {
            Skin::CoinYellow => Look::CoinYellow,
            Skin::CoinRed => Look::CoinRed,
            Skin::CoinBlue => Look::CoinBlue,
            Skin::RupeeGreen => Look::RupeeGreen,
            Skin::RupeeBlue => Look::RupeeBlue,
            Skin::RupeeRed => Look::RupeeRed,
            Skin::RupeePurple => Look::RupeePurple,
            Skin::RupeeSilver => Look::RupeeSilver,
            Skin::RupeeGold => Look::RupeeGold,
            Skin::Note => Look::Note,
        }
    }

    /// The rare ones get the long pop, a ring and more sparkles.
    pub fn big(self) -> bool {
        matches!(self, Look::HeartPiece | Look::Token | Look::Key | Look::Jiggy | Look::Honeycomb | Look::MumboToken | Look::RupeePurple | Look::RupeeGold | Look::RupeeSilver)
    }

    /// The glint and sparkle colour: the item's own colour pulled towards white.
    pub fn glint(self) -> [f32; 3] {
        let c = self.rgb();
        [0.5 + 0.5 * c[0], 0.5 + 0.5 * c[1], 0.5 + 0.5 * c[2]]
    }
}

/// One mesh and one material per [`Look`], shared by every instance.
#[derive(Resource)]
pub struct PickupLooks {
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
}

impl PickupLooks {
    pub fn mesh(&self, look: Look) -> Handle<Mesh> {
        self.meshes[SHAPES.iter().position(|s| *s == look.shape()).unwrap_or(0)].clone()
    }
    pub fn material(&self, look: Look) -> Handle<StandardMaterial> {
        self.materials[LOOKS.iter().position(|l| *l == look).unwrap_or(0)].clone()
    }
}

/// The transform of a collectible resting at `at` (bevy units): a bob and a spin, each item out of step with its
/// neighbours, and a springy pop-in over its first third of a second (`age` seconds since it appeared).
pub fn idle_transform(look: Look, at: Vec3, t: f32, age: f32) -> Transform {
    let phase = (at.x * 12.9898 + at.z * 78.233).sin() * 43758.547;
    let phase = (phase - phase.floor()) * TAU;
    let size = look.size() * SCALE * pop_in(age / 0.32);
    let flat = matches!(look.shape(), Shape::Gem | Shape::Heart | Shape::Feather | Shape::Honeycomb | Shape::Jiggy | Shape::Coin);
    let spin = t * if flat { 2.6 } else { 1.8 } + phase;
    let tilt = if look.shape() == Shape::Feather { Quat::from_rotation_z(0.35) } else { Quat::IDENTITY };
    Transform { translation: at + Vec3::Y * ((t * 2.4 + phase).sin() * 0.05), rotation: Quat::from_rotation_y(spin) * tilt, scale: Vec3::splat(size.max(1e-4)) }
}

/// Ease-out-back from 0 to 1 (overshoots by a fifth).
pub fn pop_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + 2.2 * t * t * t + 1.2 * t * t
}

// ---------------------------------------------------------------------------------------------
// The effect pool

pub const POPS: usize = 12;
pub const SPARKS: usize = 80;
pub const SHARDS: usize = 56;
pub const RINGS: usize = 6;

#[derive(Clone, Copy, Debug)]
pub struct Pop {
    pub look: Look,
    pub at: Vec3,
    pub age: f32,
    pub life: f32,
    pub turn: f32,
}

impl Pop {
    pub fn alive(&self) -> bool {
        self.age < self.life
    }
    /// (height risen in sim units, scale factor, spin angle).
    pub fn pose(&self) -> (f32, f32, f32) {
        let t = (self.age / self.life).clamp(0.0, 1.0);
        let rise = (1.0 - (1.0 - t) * (1.0 - t)) * if self.look.big() { 110.0 } else { 70.0 };
        let scale = if t < 0.3 { 1.0 + 0.45 * (t / 0.3) } else { 1.45 * (1.0 - ((t - 0.3) / 0.7).powi(2)) };
        // the spin winds up
        (rise, scale.max(0.0), self.turn + self.age * (6.0 + 26.0 * t))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Spark {
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f32,
    pub life: f32,
    /// Bevy units.
    pub size: f32,
    pub rgb: [f32; 3],
    pub phase: f32,
    pub gravity: f32,
    fresh: bool,
}

impl Spark {
    pub fn alive(&self) -> bool {
        self.age < self.life
    }
    /// Size envelope: quick in, slow out, with a twinkle.
    pub fn scale(&self) -> f32 {
        let t = (self.age / self.life).clamp(0.0, 1.0);
        let env = (t * 6.0).min(1.0) * (1.0 - t).powf(0.7);
        self.size * env * (0.72 + 0.28 * (self.age * 26.0 + self.phase).sin())
    }
}

/// Shard colours.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Debris {
    Grass,
    Clay,
    Wood,
    Stone,
    Shell,
    Web,
}

pub const DEBRIS: [Debris; 6] = [Debris::Grass, Debris::Clay, Debris::Wood, Debris::Stone, Debris::Shell, Debris::Web];

impl Debris {
    pub fn rgb(self, variant: usize) -> [f32; 3] {
        let (a, b) = match self {
            Debris::Grass => ([0.3, 0.72, 0.2], [0.62, 0.9, 0.35]),
            Debris::Clay => ([0.62, 0.33, 0.2], [0.78, 0.48, 0.3]),
            Debris::Wood => ([0.5, 0.33, 0.16], [0.68, 0.5, 0.28]),
            Debris::Stone => ([0.45, 0.44, 0.42], [0.66, 0.65, 0.62]),
            Debris::Shell => ([0.85, 0.92, 1.0], [1.0, 1.0, 1.0]),
            Debris::Web => ([0.8, 0.8, 0.76], [0.95, 0.95, 0.9]),
        };
        if variant % 2 == 0 {
            a
        } else {
            b
        }
    }
    /// (speed, upward speed, gravity, size) in sim units; grass floats, clay and stone drop.
    fn motion(self) -> (f32, f32, f32, f32) {
        match self {
            Debris::Grass => (210.0, 360.0, 560.0, 30.0),
            Debris::Clay => (300.0, 380.0, 1300.0, 14.0),
            Debris::Wood => (280.0, 400.0, 1200.0, 18.0),
            Debris::Stone => (260.0, 360.0, 1500.0, 16.0),
            Debris::Shell => (240.0, 260.0, 1100.0, 9.0),
            Debris::Web => (120.0, 160.0, 260.0, 16.0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Shard {
    pub pos: Vec3,
    pub vel: Vec3,
    pub axis: Vec3,
    pub spin: f32,
    pub age: f32,
    pub life: f32,
    pub size: f32,
    pub gravity: f32,
    pub material: usize,
    pub slim: bool,
}

impl Shard {
    pub fn alive(&self) -> bool {
        self.age < self.life
    }
    pub fn scale(&self) -> f32 {
        let t = (self.age / self.life).clamp(0.0, 1.0);
        self.size * (1.0 - ((t - 0.65) / 0.35).max(0.0).powi(2))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Halo {
    pub pos: Vec3,
    pub age: f32,
    pub life: f32,
    /// Final radius, bevy units.
    pub r1: f32,
    pub rgb: [f32; 3],
}

impl Halo {
    pub fn alive(&self) -> bool {
        self.age < self.life
    }
    pub fn radius(&self) -> f32 {
        let t = (self.age / self.life).clamp(0.0, 1.0);
        self.r1 * (1.0 - (1.0 - t).powi(3))
    }
    pub fn alpha(&self) -> f32 {
        0.85 * (1.0 - (self.age / self.life).clamp(0.0, 1.0)).powf(1.5)
    }
}

/// Fixed pools, overwritten oldest first.
#[derive(Resource)]
pub struct PickupFx {
    pub pops: Vec<Pop>,
    pub sparks: Vec<Spark>,
    pub shards: Vec<Shard>,
    pub halos: Vec<Halo>,
    cursor: [usize; 4],
    rng: Rng,
}

impl Default for PickupFx {
    fn default() -> Self {
        let dead_pop = Pop { look: Look::Note, at: Vec3::ZERO, age: 1.0, life: 0.0, turn: 0.0 };
        let dead_spark = Spark { pos: Vec3::ZERO, vel: Vec3::ZERO, age: 1.0, life: 0.0, size: 0.0, rgb: [1.0; 3], phase: 0.0, gravity: 0.0, fresh: false };
        let dead_shard = Shard { pos: Vec3::ZERO, vel: Vec3::ZERO, axis: Vec3::Y, spin: 0.0, age: 1.0, life: 0.0, size: 0.0, gravity: 0.0, material: 0, slim: false };
        let dead_halo = Halo { pos: Vec3::ZERO, age: 1.0, life: 0.0, r1: 0.0, rgb: [1.0; 3] };
        Self { pops: vec![dead_pop; POPS], sparks: vec![dead_spark; SPARKS], shards: vec![dead_shard; SHARDS], halos: vec![dead_halo; RINGS], cursor: [0; 4], rng: Rng::new(0x7ac0_11ec7) }
    }
}

impl PickupFx {
    fn slot(&mut self, pool: usize, len: usize) -> usize {
        let i = self.cursor[pool];
        self.cursor[pool] = (i + 1) % len;
        i
    }

    /// A unit vector spread over the sphere.
    fn dir(&mut self) -> Vec3 {
        let (a, y) = (self.rng.next() * TAU, self.rng.next() * 2.0 - 1.0);
        let r = (1.0 - y * y).max(0.0).sqrt();
        Vec3::new(a.cos() * r, y, a.sin() * r)
    }

    /// The item at `at` (bevy units) was taken: it pops, with sparkles in its colour.
    pub fn collect(&mut self, look: Look, at: Vec3) {
        let i = self.slot(0, POPS);
        let big = look.big();
        let turn = self.rng.next() * TAU;
        self.pops[i] = Pop { look, at, age: 0.0, life: if big { 0.85 } else { 0.42 }, turn };
        self.sparkle(at, look.glint(), if big { 16 } else { 8 }, if big { 280.0 } else { 210.0 }, if big { 60.0 } else { 44.0 });
        if big {
            self.halo(at, look.glint(), look.size() * 5.0, 0.5);
            self.halo(at, [1.0, 1.0, 1.0], look.size() * 2.6, 0.3);
        } else {
            self.halo(at, look.glint(), look.size() * 2.8, 0.28);
        }
    }

    /// `n` sparkles flying out of `at` (bevy units) at up to `speed` sim units a second, `size` sim units across.
    pub fn sparkle(&mut self, at: Vec3, rgb: [f32; 3], n: usize, speed: f32, size: f32) {
        for _ in 0..n.min(SPARKS) {
            let i = self.slot(1, SPARKS);
            let d = self.dir();
            let (v, l, s, ph) = (self.rng.next(), self.rng.next(), self.rng.next(), self.rng.next());
            self.sparks[i] = Spark {
                pos: at + d * 0.06,
                vel: (d * (0.35 + 0.65 * v) * speed + Vec3::Y * speed * 0.35) * SCALE,
                age: 0.0,
                life: 0.38 + 0.4 * l,
                size: size * (0.6 + 0.8 * s) * SCALE,
                rgb,
                phase: ph * TAU,
                gravity: 260.0 * SCALE,
                fresh: true,
            };
        }
    }

    /// One still twinkle (an idle glint on a waiting collectible).
    pub fn glint(&mut self, at: Vec3, rgb: [f32; 3], size: f32) {
        let i = self.slot(1, SPARKS);
        let ph = self.rng.next() * TAU;
        self.sparks[i] = Spark { pos: at, vel: Vec3::Y * 0.1, age: 0.0, life: 0.55, size: size * SCALE, rgb, phase: ph, gravity: 0.0, fresh: true };
    }

    /// `n` tumbling shards thrown up from `at` (bevy units).
    pub fn debris(&mut self, at: Vec3, kind: Debris, n: usize) {
        let (speed, up, gravity, size) = kind.motion();
        let base = DEBRIS.iter().position(|d| *d == kind).unwrap_or(0) * 2;
        for k in 0..n.min(SHARDS) {
            let i = self.slot(2, SHARDS);
            let d = self.dir();
            let axis = self.dir();
            let (v, u, l, s, sp) = (self.rng.next(), self.rng.next(), self.rng.next(), self.rng.next(), self.rng.next());
            self.shards[i] = Shard {
                pos: at + Vec3::new(d.x, d.y.abs() * 0.5, d.z) * 0.12,
                vel: Vec3::new(d.x * speed * (0.3 + 0.7 * v), up * (0.45 + 0.55 * u), d.z * speed * (0.3 + 0.7 * v)) * SCALE,
                axis,
                spin: 5.0 + 14.0 * sp,
                age: 0.0,
                life: 0.55 + 0.45 * l,
                size: size * (0.6 + 0.8 * s) * SCALE,
                gravity: gravity * SCALE,
                material: base + k % 2,
                slim: kind == Debris::Grass,
            };
        }
    }

    /// An expanding camera-facing ring at `at` (bevy units) growing to `radius` sim units.
    pub fn halo(&mut self, at: Vec3, rgb: [f32; 3], radius: f32, life: f32) {
        let i = self.slot(3, RINGS);
        self.halos[i] = Halo { pos: at, age: 0.0, life, r1: radius * SCALE, rgb };
    }

    pub fn update(&mut self, dt: f32) {
        for p in self.pops.iter_mut().filter(|p| p.alive()) {
            p.age += dt;
        }
        for s in self.sparks.iter_mut().filter(|s| s.alive()) {
            s.age += dt;
            s.vel.y -= s.gravity * dt;
            // air drag: they burst out and hang
            s.vel *= (1.0 - 3.2 * dt).max(0.0);
            s.pos += s.vel * dt;
        }
        for s in self.shards.iter_mut().filter(|s| s.alive()) {
            s.age += dt;
            s.vel.y -= s.gravity * dt;
            s.pos += s.vel * dt;
        }
        for h in self.halos.iter_mut().filter(|h| h.alive()) {
            h.age += dt;
        }
    }

    pub fn live(&self) -> usize {
        self.pops.iter().filter(|p| p.alive()).count() + self.sparks.iter().filter(|p| p.alive()).count() + self.shards.iter().filter(|p| p.alive()).count() + self.halos.iter().filter(|p| p.alive()).count()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing

#[derive(Component)]
struct PopEnt(usize);
#[derive(Component)]
struct SparkEnt(usize);
#[derive(Component)]
struct ShardEnt(usize);
#[derive(Component)]
struct HaloEnt(usize);

#[derive(Resource)]
struct ShardMaterials(Vec<Handle<StandardMaterial>>);

/// A 64x64 four-point star: a bright core with thin horizontal and vertical rays.
pub fn star_texture() -> Image {
    let size = 64usize;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = ((x as f32 + 0.5) / 32.0 - 1.0, (y as f32 + 0.5) / 32.0 - 1.0);
            let a = star_alpha(u, v);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: 64, height: 64, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

pub fn star_alpha(u: f32, v: f32) -> f32 {
    let r = (u * u + v * v).sqrt();
    let rays = (-(u.abs() * v.abs()) * 90.0).exp() * (1.0 - r).max(0.0).powf(1.6);
    let core = (-r * r * 22.0).exp();
    (rays + core).min(1.0) * (1.0 - r.powi(4)).max(0.0)
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let looks = PickupLooks {
        meshes: SHAPES.iter().map(|s| meshes.add(shape_tris(*s).mesh())).collect(),
        materials: LOOKS
            .iter()
            .map(|l| {
                let c = l.rgb();
                let gem = l.shape() == Shape::Gem;
                materials.add(StandardMaterial {
                    base_color: Color::srgb(c[0], c[1], c[2]),
                    // lifted out of the shade, and a clear highlight as they turn
                    emissive: LinearRgba::rgb(c[0] * 0.5, c[1] * 0.5, c[2] * 0.5),
                    perceptual_roughness: if gem { 0.18 } else { 0.38 },
                    metallic: if matches!(l, Look::Jiggy | Look::Note | Look::Token | Look::Key | Look::MumboToken | Look::CoinYellow | Look::CoinRed | Look::CoinBlue) { 0.35 } else { 0.0 },
                    reflectance: 0.6,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                })
            })
            .collect(),
    };
    let hidden = || (Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver);
    for i in 0..POPS {
        commands.spawn((PopEnt(i), Mesh3d(looks.mesh(Look::Note)), MeshMaterial3d(looks.material(Look::Note)), hidden()));
    }
    commands.insert_resource(looks);
    let star = images.add(star_texture());
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    for i in 0..SPARKS {
        let m = materials.add(StandardMaterial { base_color_texture: Some(star.clone()), base_color: Color::WHITE, unlit: true, alpha_mode: AlphaMode::Add, cull_mode: None, ..default() });
        commands.spawn((SparkEnt(i), Mesh3d(quad.clone()), MeshMaterial3d(m), hidden()));
    }
    // shards: a sliver and a chip, one material per colour
    let mut sliver = Tris::default();
    sliver.lens(&[Vec2::new(-0.3, -1.0), Vec2::new(0.3, -1.0), Vec2::new(0.0, 1.0)], Vec2::new(0.0, -0.3), 0.03, 1.0);
    let mut chip = Tris::default();
    chip.lens(&[Vec2::new(-0.8, -0.6), Vec2::new(0.9, -0.4), Vec2::new(0.5, 0.7), Vec2::new(-0.5, 0.8)], Vec2::ZERO, 0.14, 0.8);
    let (sliver, chip) = (meshes.add(sliver.mesh()), meshes.add(chip.mesh()));
    let mats: Vec<Handle<StandardMaterial>> = DEBRIS
        .iter()
        .flat_map(|d| [d.rgb(0), d.rgb(1)])
        .map(|c| materials.add(StandardMaterial { base_color: Color::srgb(c[0], c[1], c[2]), emissive: LinearRgba::rgb(c[0] * 0.12, c[1] * 0.12, c[2] * 0.12), perceptual_roughness: 1.0, cull_mode: None, double_sided: true, ..default() }))
        .collect();
    for i in 0..SHARDS {
        commands.spawn((ShardEnt(i), Mesh3d(if i % 2 == 0 { sliver.clone() } else { chip.clone() }), MeshMaterial3d(mats[0].clone()), hidden(), ShardMeshes(sliver.clone(), chip.clone())));
    }
    commands.insert_resource(ShardMaterials(mats));
    let ring = meshes.add(Annulus::new(0.84, 1.0).mesh().resolution(36));
    for i in 0..RINGS {
        let m = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.0), unlit: true, alpha_mode: AlphaMode::Add, cull_mode: None, ..default() });
        commands.spawn((HaloEnt(i), Mesh3d(ring.clone()), MeshMaterial3d(m), hidden()));
    }
}

#[derive(Component)]
struct ShardMeshes(Handle<Mesh>, Handle<Mesh>);

fn step(time: Res<Time>, mut fx: ResMut<PickupFx>) {
    if fx.live() > 0 {
        fx.update(time.delta_secs().min(0.1));
    }
}

/// (Takes the `Mut` itself: a `&mut Visibility` coerced from it would already have marked the component changed.)
fn show(vis: &mut Mut<Visibility>, on: bool) {
    vis.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
}

#[allow(clippy::type_complexity)]
fn draw(
    mut fx: ResMut<PickupFx>,
    looks: Option<Res<PickupLooks>>,
    shard_mats: Option<Res<ShardMaterials>>,
    camera: Single<&Transform, (With<MainCamera>, Without<PopEnt>, Without<SparkEnt>, Without<ShardEnt>, Without<HaloEnt>)>,
    mut pops: Query<(&PopEnt, &mut Transform, &mut Visibility, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>), (Without<SparkEnt>, Without<ShardEnt>, Without<HaloEnt>)>,
    mut sparks: Query<(&SparkEnt, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), (Without<PopEnt>, Without<ShardEnt>, Without<HaloEnt>)>,
    mut shards: Query<(&ShardEnt, &mut Transform, &mut Visibility, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>, &ShardMeshes), (Without<PopEnt>, Without<SparkEnt>, Without<HaloEnt>)>,
    mut halos: Query<(&HaloEnt, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), (Without<PopEnt>, Without<SparkEnt>, Without<ShardEnt>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(looks), Some(shard_mats)) = (looks, shard_mats) else { return };
    for (PopEnt(i), mut tf, mut vis, mut mesh, mut mat) in &mut pops {
        let p = fx.pops[*i];
        show(&mut vis, p.alive());
        if !p.alive() {
            continue;
        }
        let (rise, scale, turn) = p.pose();
        tf.translation = p.at + Vec3::Y * rise * SCALE;
        tf.rotation = Quat::from_rotation_y(turn);
        tf.scale = Vec3::splat((p.look.size() * SCALE * scale).max(1e-4));
        let (want_mesh, want_mat) = (looks.mesh(p.look), looks.material(p.look));
        if mesh.0 != want_mesh {
            mesh.0 = want_mesh;
        }
        if mat.0 != want_mat {
            mat.0 = want_mat;
        }
    }
    for (SparkEnt(i), mut tf, mut vis, mat) in &mut sparks {
        let s = fx.sparks[*i];
        show(&mut vis, s.alive());
        if !s.alive() {
            continue;
        }
        tf.translation = s.pos;
        tf.rotation = camera.rotation * Quat::from_rotation_z(s.phase * 0.25 + s.age * 1.5);
        tf.scale = Vec3::splat(s.scale().max(1e-4));
        if s.fresh {
            // the colour is set once; the fade is the shrinking star
            fx.sparks[*i].fresh = false;
            if let Some(mut m) = materials.get_mut(&mat.0) {
                m.base_color = Color::srgb(s.rgb[0], s.rgb[1], s.rgb[2]);
            }
        }
    }
    for (ShardEnt(i), mut tf, mut vis, mut mesh, mut mat, both) in &mut shards {
        let s = fx.shards[*i];
        show(&mut vis, s.alive());
        if !s.alive() {
            continue;
        }
        tf.translation = s.pos;
        tf.rotation = Quat::from_axis_angle(s.axis, s.age * s.spin);
        tf.scale = Vec3::splat(s.scale().max(1e-4));
        let want_mesh = if s.slim { &both.0 } else { &both.1 };
        if mesh.0 != *want_mesh {
            mesh.0 = want_mesh.clone();
        }
        if let Some(want) = shard_mats.0.get(s.material) {
            if mat.0 != *want {
                mat.0 = want.clone();
            }
        }
    }
    for (HaloEnt(i), mut tf, mut vis, mat) in &mut halos {
        let h = fx.halos[*i];
        show(&mut vis, h.alive());
        if !h.alive() {
            continue;
        }
        tf.translation = h.pos;
        tf.rotation = camera.rotation;
        tf.scale = Vec3::splat(h.radius().max(1e-4));
        if let Some(mut m) = materials.get_mut(&mat.0) {
            let a = h.alpha();
            m.base_color = Color::srgba(h.rgb[0] * a, h.rgb[1] * a, h.rgb[2] * a, a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlines_wind_counter_clockwise_and_fit_the_unit_box() {
        for (name, o) in [("gem", gem_outline()), ("heart", heart_outline(28)), ("hex", polygon_outline(6, 1.0, 0.0)), ("jigsaw", jigsaw_outline()), ("feather", feather_outline(8))] {
            assert!(signed_area(&o) > 0.1, "{name} is clockwise or empty");
            for p in &o {
                assert!(p.x.abs() <= 1.05 && p.y.abs() <= 1.05, "{name} leaves the box at {p:?}");
            }
        }
    }

    #[test]
    fn outlines_are_star_shaped_about_their_centre() {
        // every edge turns the same way seen from the fan centre, so the fan never folds over
        for (name, o) in [("gem", gem_outline()), ("heart", heart_outline(28)), ("jigsaw", jigsaw_outline()), ("feather", feather_outline(8))] {
            for i in 0..o.len() {
                let (a, b) = (o[i], o[(i + 1) % o.len()]);
                assert!(a.perp_dot(b) > -1e-4, "{name} folds at {i}: {a:?} -> {b:?}");
            }
        }
    }

    #[test]
    fn every_shape_builds_a_closed_looking_mesh_with_unit_normals() {
        for s in SHAPES {
            let t = shape_tris(s);
            assert!(t.pos.len() >= 36 && t.pos.len() % 3 == 0, "{s:?} has {} vertices", t.pos.len());
            assert!(t.pos.len() < 1500, "{s:?} is too heavy: {}", t.pos.len());
            assert_eq!(t.pos.len(), t.nor.len());
            for n in &t.nor {
                assert!((Vec3::from(*n).length() - 1.0).abs() < 1e-3);
            }
            let top = t.pos.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
            assert!((0.6..=1.05).contains(&top), "{s:?} top at {top}");
        }
    }

    #[test]
    fn convex_lens_faces_point_outwards() {
        let mut t = Tris::default();
        t.lens(&gem_outline(), Vec2::ZERO, 0.26, 0.55);
        for (p, n) in t.pos.chunks(3).zip(t.nor.chunks(3)) {
            let c = (Vec3::from(p[0]) + Vec3::from(p[1]) + Vec3::from(p[2])) / 3.0;
            assert!(c.dot(Vec3::from(n[0])) > 0.0, "inward face at {c:?}");
        }
    }

    #[test]
    fn every_look_has_a_mesh_a_colour_and_a_size() {
        for l in LOOKS {
            assert!(SHAPES.contains(&l.shape()));
            assert!((8.0..=40.0).contains(&l.size()));
            assert!(l.glint().iter().all(|c| (0.5..=1.0).contains(c)));
        }
        assert!(Look::Jiggy.big() && !Look::Note.big() && !Look::RupeeGreen.big());
    }

    #[test]
    fn pop_grows_then_vanishes() {
        let mut fx = PickupFx::default();
        assert_eq!(fx.live(), 0);
        fx.collect(Look::Note, Vec3::ZERO);
        let p = fx.pops[0];
        assert!((p.pose().1 - 1.0).abs() < 1e-5);
        let mut peak = 0.0f32;
        let mut last_rise = 0.0;
        for _ in 0..60 {
            fx.update(1.0 / 60.0);
            let (rise, scale, _) = fx.pops[0].pose();
            peak = peak.max(scale);
            assert!(rise >= last_rise - 1e-4);
            last_rise = rise;
        }
        assert!(peak > 1.3);
        assert!(!fx.pops[0].alive());
        assert!(fx.pops[0].pose().1 < 1e-3);
        for _ in 0..60 {
            fx.update(1.0 / 60.0);
        }
        assert_eq!(fx.live(), 0, "everything ends");
    }

    #[test]
    fn pools_are_bounded() {
        let mut fx = PickupFx::default();
        for i in 0..200 {
            fx.collect(LOOKS[i % LOOKS.len()], Vec3::splat(i as f32));
            fx.debris(Vec3::ZERO, DEBRIS[i % DEBRIS.len()], 30);
            fx.glint(Vec3::ZERO, [1.0; 3], 20.0);
        }
        assert_eq!((fx.pops.len(), fx.sparks.len(), fx.shards.len(), fx.halos.len()), (POPS, SPARKS, SHARDS, RINGS));
        assert_eq!(fx.live(), POPS + SPARKS + SHARDS + RINGS);
        fx.clear();
        assert_eq!(fx.live(), 0);
    }

    #[test]
    fn shards_fly_up_and_fall_and_sparkles_fade_out() {
        let mut fx = PickupFx::default();
        fx.debris(Vec3::ZERO, Debris::Clay, 8);
        fx.sparkle(Vec3::ZERO, [1.0; 3], 8, 200.0, 24.0);
        fx.update(0.05);
        assert!(fx.shards.iter().filter(|s| s.alive()).all(|s| s.pos.y > 0.0 && s.material / 2 == 1));
        assert!(fx.sparks.iter().filter(|s| s.alive()).all(|s| s.scale() > 0.0));
        for _ in 0..40 {
            fx.update(0.05);
        }
        assert_eq!(fx.live(), 0);
        let s = fx.shards[0];
        assert!(s.pos.y < 0.0, "gravity pulled it back down");
        assert!(s.scale().abs() < 1e-4 && fx.sparks[0].scale().abs() < 1e-4);
    }

    #[test]
    fn pop_in_overshoots_and_settles() {
        assert!(pop_in(0.0).abs() < 1e-5 && (pop_in(1.0) - 1.0).abs() < 1e-5);
        assert!((0..=20).map(|i| pop_in(i as f32 / 20.0)).fold(0.0f32, f32::max) > 1.05);
        let a = idle_transform(Look::Note, Vec3::new(1.0, 2.0, 3.0), 0.0, 10.0);
        let b = idle_transform(Look::Note, Vec3::new(4.0, 2.0, 3.0), 0.0, 10.0);
        assert!(a.rotation.angle_between(b.rotation) > 1e-3, "neighbours are out of step");
        assert!((a.scale.x - Look::Note.size() * SCALE).abs() < 1e-5);
    }

    #[test]
    fn star_is_bright_in_the_middle_and_dark_in_the_corners() {
        assert!(star_alpha(0.0, 0.0) > 0.95);
        assert!(star_alpha(0.5, 0.0) > star_alpha(0.35, 0.35) * 3.0);
        assert!(star_alpha(0.97, 0.97) < 0.01 && star_alpha(1.0, 0.0) < 0.01);
    }
}

#[cfg(test)]
pub(crate) fn test_sample_mesh() -> Mesh {
    let mut t = Tris::default();
    t.tri(Vec3::ZERO, Vec3::X, Vec3::Y);
    t.mesh()
}
