//! The 3D half of the v3 front end: flat/gouraud-shaded low-poly props rendered by a private camera
//! into a small image (see `mod.rs`). Holds the original hero (a fox in an aviator helmet), the
//! title backdrop and gem, and the two hub rooms (a ring of doors each).
use super::fb::Rgba;
use super::icons;
use super::model::{Menu, Page, Wing};
use crate::shell::ExperimentId;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use std::f32::consts::{PI, TAU};

pub const LAYER: usize = 7;
pub const FOV: f32 = 52.0 * PI / 180.0;

// ------------------------------------------------------------------------------------------
// Geometry helpers
// ------------------------------------------------------------------------------------------

fn lin(c: Rgba) -> [f32; 4] {
    Color::srgb_u8(c[0], c[1], c[2]).to_linear().to_f32_array()
}

#[derive(Default)]
pub struct MeshBuilder {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl MeshBuilder {
    /// One flat-shaded triangle with a colour per corner (gouraud across the face).
    pub fn tri_cols(&mut self, a: Vec3, b: Vec3, c: Vec3, ca: Rgba, cb: Rgba, cc: Rgba) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        let base = self.pos.len() as u32;
        for (p, col) in [(a, ca), (b, cb), (c, cc)] {
            self.pos.push(p.to_array());
            self.nor.push(n.to_array());
            self.col.push(lin(col));
        }
        self.idx.extend([base, base + 1, base + 2]);
    }

    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: Rgba) {
        self.tri_cols(a, b, c, col, col, col);
    }

    /// Triangle wound so its front faces away from `behind`.
    pub fn tri_away(&mut self, a: Vec3, b: Vec3, c: Vec3, col: Rgba, behind: Vec3) {
        let n = (b - a).cross(c - a);
        if n.dot((a + b + c) / 3.0 - behind) >= 0.0 {
            self.tri(a, b, c, col);
        } else {
            self.tri(a, c, b, col);
        }
    }

    pub fn quad_cols(&mut self, p: [Vec3; 4], c: [Rgba; 4]) {
        self.tri_cols(p[0], p[1], p[2], c[0], c[1], c[2]);
        self.tri_cols(p[0], p[2], p[3], c[0], c[2], c[3]);
    }

    pub fn quad(&mut self, p: [Vec3; 4], col: Rgba) {
        self.quad_cols(p, [col; 4]);
    }

    /// Quad wound to face away from `behind` (corners in loop order).
    pub fn quad_away(&mut self, p: [Vec3; 4], c: [Rgba; 4], behind: Vec3) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        if n.dot((p[0] + p[1] + p[2] + p[3]) / 4.0 - behind) >= 0.0 {
            self.quad_cols(p, c);
        } else {
            self.quad_cols([p[0], p[3], p[2], p[1]], [c[0], c[3], c[2], c[1]]);
        }
    }

    /// Oriented box from three basis vectors (right x up = forward) and half sizes along them.
    pub fn obox(&mut self, center: Vec3, r: Vec3, u: Vec3, f: Vec3, half: Vec3, col: Rgba) {
        let (hr, hu, hf) = (r * half.x, u * half.y, f * half.z);
        let faces = [(f, hf, r, hr, u, hu), (-f, -hf, u, hu, r, hr), (r, hr, u, hu, f, hf), (-r, -hr, f, hf, u, hu), (u, hu, f, hf, r, hr), (-u, -hu, r, hr, f, hf)];
        for (_, off, _, t1, _, t2) in faces {
            let c = center + off;
            self.quad([c - t1 - t2, c + t1 - t2, c + t1 + t2, c - t1 + t2], col);
        }
    }

    /// Axis-aligned box; `col_top` lights the top face a little differently.
    pub fn cube(&mut self, center: Vec3, size: Vec3, col: Rgba) {
        self.obox(center, Vec3::X, Vec3::Y, Vec3::Z, size / 2.0, col);
    }

    /// Pyramid / cone with `seg` sides on a base at `base` growing along `axis` (length = height).
    pub fn cone(&mut self, base: Vec3, axis: Vec3, radius: f32, seg: usize, col: Rgba) {
        let up = axis.normalize();
        let e1 = if up.x.abs() < 0.9 { up.cross(Vec3::X) } else { up.cross(Vec3::Z) }.normalize();
        let e2 = up.cross(e1);
        let apex = base + axis;
        let inside = base + axis * 0.3;
        for i in 0..seg {
            let (a0, a1) = (i as f32 / seg as f32 * TAU, (i + 1) as f32 / seg as f32 * TAU);
            let p0 = base + (e1 * a0.cos() + e2 * a0.sin()) * radius;
            let p1 = base + (e1 * a1.cos() + e2 * a1.sin()) * radius;
            self.tri_away(p0, p1, apex, col, inside);
            self.tri_away(p0, p1, base, col, inside);
        }
    }

    pub fn build(self) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        m.insert_indices(Indices::U32(self.idx));
        m
    }
}

fn c(hex: u32) -> Rgba {
    super::fb::rgb(hex)
}

fn shade(col: Rgba, f: f32) -> Rgba {
    [(col[0] as f32 * f).min(255.0) as u8, (col[1] as f32 * f).min(255.0) as u8, (col[2] as f32 * f).min(255.0) as u8, 255]
}

// ------------------------------------------------------------------------------------------
// The hero: an original fox in an aviator helmet, goggles and a scarf
// ------------------------------------------------------------------------------------------

const TEAL: u32 = 0x2a9aa3;
const TEAL_DK: u32 = 0x1d6f7a;
const CREAM: u32 = 0xe8e2d0;
const BROWN: u32 = 0x7a5230;
const ORANGE: u32 = 0xe8742a;
const HELMET: u32 = 0xe8b830;

fn hero_body() -> Mesh {
    let mut b = MeshBuilder::default();
    for s in [-1.0f32, 1.0] {
        b.cube(Vec3::new(s * 0.17, 0.09, 0.05), Vec3::new(0.22, 0.18, 0.36), c(0x6b4226));
        b.cube(Vec3::new(s * 0.17, 0.21, 0.0), Vec3::new(0.23, 0.06, 0.24), c(0xd8d8d0));
        b.cube(Vec3::new(s * 0.17, 0.48, 0.0), Vec3::new(0.2, 0.46, 0.2), c(TEAL_DK));
        // Arms and gloves.
        b.cube(Vec3::new(s * 0.38, 1.06, 0.0), Vec3::new(0.16, 0.42, 0.18), c(TEAL));
        b.cube(Vec3::new(s * 0.38, 0.8, 0.02), Vec3::new(0.2, 0.2, 0.2), c(0x5a3a22));
    }
    b.cube(Vec3::new(0.0, 0.76, 0.0), Vec3::new(0.6, 0.08, 0.36), c(BROWN));
    b.cube(Vec3::new(0.0, 0.76, 0.19), Vec3::new(0.1, 0.08, 0.02), c(0xffc933));
    b.cube(Vec3::new(0.0, 1.04, 0.0), Vec3::new(0.58, 0.5, 0.34), c(TEAL));
    b.cube(Vec3::new(0.0, 1.0, 0.175), Vec3::new(0.26, 0.34, 0.02), c(CREAM));
    // Scarf: a collar and a tail that streams behind.
    b.cube(Vec3::new(0.0, 1.32, 0.0), Vec3::new(0.52, 0.12, 0.4), c(ORANGE));
    b.cube(Vec3::new(0.2, 1.12, -0.22), Vec3::new(0.14, 0.4, 0.06), c(0xc85a1a));
    b.build()
}

fn hero_head() -> Mesh {
    let mut b = MeshBuilder::default();
    // Origin at the neck.
    b.cube(Vec3::new(0.0, 0.26, 0.0), Vec3::new(0.56, 0.46, 0.5), c(TEAL));
    b.cube(Vec3::new(0.0, 0.14, 0.3), Vec3::new(0.26, 0.17, 0.16), c(CREAM));
    b.cube(Vec3::new(0.0, 0.18, 0.39), Vec3::new(0.08, 0.06, 0.05), c(0x14202a));
    for s in [-1.0f32, 1.0] {
        b.cube(Vec3::new(s * 0.2, 0.1, 0.27), Vec3::new(0.14, 0.14, 0.1), c(CREAM));
        b.cube(Vec3::new(s * 0.14, 0.27, 0.255), Vec3::new(0.08, 0.13, 0.02), c(0x14202a));
        b.cube(Vec3::new(s * 0.125, 0.31, 0.262), Vec3::new(0.03, 0.04, 0.02), c(0xffffff));
        // Ears through the helmet.
        b.cone(Vec3::new(s * 0.19, 0.55, -0.02), Vec3::new(s * 0.05, 0.38, 0.0), 0.13, 4, c(TEAL));
        b.cone(Vec3::new(s * 0.19, 0.56, 0.05), Vec3::new(s * 0.05, 0.28, 0.0), 0.06, 4, c(CREAM));
        // Helmet flaps and goggle lenses pushed up on the forehead.
        b.cube(Vec3::new(s * 0.3, 0.34, -0.05), Vec3::new(0.06, 0.28, 0.3), c(HELMET));
        b.cube(Vec3::new(s * 0.14, 0.52, 0.27), Vec3::new(0.2, 0.15, 0.04), c(BROWN));
        b.cube(Vec3::new(s * 0.14, 0.52, 0.3), Vec3::new(0.15, 0.1, 0.03), c(0xa9dcff));
    }
    b.cube(Vec3::new(0.0, 0.52, 0.0), Vec3::new(0.6, 0.18, 0.54), c(HELMET));
    b.cube(Vec3::new(0.0, 0.36, -0.26), Vec3::new(0.6, 0.34, 0.1), c(HELMET));
    b.cube(Vec3::new(0.0, 0.52, 0.0), Vec3::new(0.62, 0.05, 0.56), c(BROWN));
    b.cube(Vec3::new(0.0, 0.6, 0.0), Vec3::new(0.14, 0.05, 0.4), c(0xc89a20));
    b.build()
}

fn hero_tail() -> Mesh {
    let mut b = MeshBuilder::default();
    b.cube(Vec3::new(0.0, 0.0, -0.14), Vec3::new(0.16, 0.16, 0.3), c(TEAL));
    b.cube(Vec3::new(0.0, 0.07, -0.4), Vec3::new(0.2, 0.2, 0.28), c(TEAL));
    b.cube(Vec3::new(0.0, 0.15, -0.64), Vec3::new(0.18, 0.18, 0.22), c(CREAM));
    b.build()
}

fn gem_mesh() -> Mesh {
    let mut b = MeshBuilder::default();
    let (top, bot) = (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, -1.0, 0.0));
    let ring: Vec<Vec3> = (0..6).map(|i| { let a = i as f32 / 6.0 * TAU; Vec3::new(a.cos() * 0.55, 0.0, a.sin() * 0.55) }).collect();
    for i in 0..6 {
        let (p, q) = (ring[i], ring[(i + 1) % 6]);
        let tone = 0.75 + 0.25 * ((i as f32 * 1.7).sin() * 0.5 + 0.5);
        b.tri_away(p, q, top, shade(c(0xffa020), tone), Vec3::ZERO);
        b.tri_away(p, q, bot, shade(c(0xd06a10), tone), Vec3::ZERO);
    }
    b.build()
}

fn gradient_quad(w: f32, h: f32, top: Rgba, bottom: Rgba) -> Mesh {
    let mut b = MeshBuilder::default();
    let (hw, hh) = (w / 2.0, h / 2.0);
    b.quad_cols([Vec3::new(-hw, -hh, 0.0), Vec3::new(hw, -hh, 0.0), Vec3::new(hw, hh, 0.0), Vec3::new(-hw, hh, 0.0)], [bottom, bottom, top, top]);
    b.build()
}

// ------------------------------------------------------------------------------------------
// Rooms
// ------------------------------------------------------------------------------------------

pub fn ring_radius(wing: Wing) -> f32 {
    match wing {
        Wing::Games => 8.6,
        Wing::Lab => 6.6,
    }
}

pub fn door_angle(i: usize, n: usize) -> f32 {
    i as f32 / n.max(1) as f32 * TAU
}

pub fn dir_of(angle: f32) -> Vec3 {
    Vec3::new(angle.sin(), 0.0, angle.cos())
}

/// World position above a door where its sign hangs.
pub fn sign_anchor(wing: Wing, i: usize, n: usize) -> Vec3 {
    dir_of(door_angle(i, n)) * (ring_radius(wing) - 0.3) + Vec3::Y * if wing == Wing::Games { 4.9 } else { 4.5 }
}

pub const DOOR_HALF_W: f32 = 1.35;
pub const DOOR_H: f32 = 3.7;

struct Palette {
    wall_lo: u32,
    wall_hi: u32,
    floor_a: u32,
    floor_b: u32,
    sky_top: u32,
    sky_low: u32,
    trim: u32,
}

fn palette(w: Wing) -> Palette {
    match w {
        Wing::Games => Palette { wall_lo: 0xb4ac9c, wall_hi: 0x7e7a78, floor_a: 0xcdb98c, floor_b: 0x8f8f94, sky_top: 0x1f1a5a, sky_low: 0xa5609a, trim: 0x5a5660 },
        Wing::Lab => Palette { wall_lo: 0x8a7ac0, wall_hi: 0x4a3a80, floor_a: 0x4a3a78, floor_b: 0x7660b0, sky_top: 0x0c0626, sky_low: 0x4a2a7a, trim: 0x2e2450 },
    }
}

#[derive(Component)]
pub struct DoorGlow {
    pub wing: Wing,
    pub index: usize,
    pub mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct RoomRoot(pub Wing);
#[derive(Component)]
pub struct TitleRoot;
#[derive(Component)]
pub struct Hero;
#[derive(Component)]
pub struct HeroHead;
#[derive(Component)]
pub struct HeroTail;
#[derive(Component)]
pub struct GemProp;
#[derive(Component)]
pub struct RetroCam;
#[derive(Component)]
pub struct Lamp(pub f32);

fn room_meshes(wing: Wing, doors: &[ExperimentId]) -> (Mesh, Mesh, Mesh) {
    let p = palette(wing);
    let r = ring_radius(wing);
    let n = doors.len().max(1);
    let (mut solid, mut sky, mut lamps) = (MeshBuilder::default(), MeshBuilder::default(), MeshBuilder::default());
    let wall_h = 6.2;
    // Floor: polar checkerboard, flat per cell.
    let (rings, segs) = (6usize, (n * 2).max(12));
    for k in 0..rings {
        let (r0, r1) = (r * k as f32 / rings as f32, r * (k + 1) as f32 / rings as f32);
        for j in 0..segs {
            let (a0, a1) = (j as f32 / segs as f32 * TAU, (j + 1) as f32 / segs as f32 * TAU);
            let col = c(if (k + j) % 2 == 0 { p.floor_a } else { p.floor_b });
            let pt = |rad: f32, a: f32| Vec3::new(a.sin() * rad, 0.0, a.cos() * rad);
            solid.quad_away([pt(r0, a0), pt(r0, a1), pt(r1, a1), pt(r1, a0)], [col; 4], Vec3::new(0.0, -5.0, 0.0));
        }
    }
    // Wall ring with a vertical gradient (lit near the floor), plus a cap and a plinth.
    let wsegs = (n * 4).max(24);
    for j in 0..wsegs {
        let (a0, a1) = (j as f32 / wsegs as f32 * TAU, (j + 1) as f32 / wsegs as f32 * TAU);
        let (d0, d1) = (dir_of(a0) * r, dir_of(a1) * r);
        let corners = [d0, d1, d1 + Vec3::Y * wall_h, d0 + Vec3::Y * wall_h];
        let (lo, hi) = (c(p.wall_lo), c(p.wall_hi));
        let mid = (d0 + d1) * 0.5;
        solid.quad_away(corners, [lo, lo, hi, hi], mid * 3.0);
        let (o0, o1) = (dir_of(a0) * (r + 0.7), dir_of(a1) * (r + 0.7));
        solid.quad_away([d0 + Vec3::Y * wall_h, d1 + Vec3::Y * wall_h, o1 + Vec3::Y * wall_h, o0 + Vec3::Y * wall_h], [c(p.trim); 4], Vec3::new(0.0, -20.0, 0.0));
        solid.quad_away([d0, d1, d1 + Vec3::Y * 0.5, d0 + Vec3::Y * 0.5], [shade(c(p.trim), 1.2); 4], mid * 3.0);
    }
    // Doors: frame posts, an accent lintel and a lamp on each pillar.
    for (i, id) in doors.iter().enumerate() {
        let a = door_angle(i, n);
        let f = dir_of(a);
        let right = Vec3::new(a.cos(), 0.0, -a.sin());
        let at = f * (r - 0.25);
        let trim = c(p.trim);
        for s in [-1.0f32, 1.0] {
            solid.obox(at + right * s * (DOOR_HALF_W + 0.3), right, Vec3::Y, f, Vec3::new(0.3, DOOR_H / 2.0 + 0.2, 0.3), shade(trim, 1.35));
        }
        solid.obox(at + Vec3::Y * (DOOR_H + 0.2), right, Vec3::Y, f, Vec3::new(DOOR_HALF_W + 0.6, 0.25, 0.32), icons::accent(*id));
        solid.obox(at + Vec3::Y * (DOOR_H + 0.55), right, Vec3::Y, f, Vec3::new(DOOR_HALF_W + 0.75, 0.1, 0.36), shade(trim, 1.5));
        // Pillar + lamp between this door and the next.
        let pa = a + std::f32::consts::PI / n as f32;
        let (pf, pr) = (dir_of(pa), Vec3::new(pa.cos(), 0.0, -pa.sin()));
        solid.obox(pf * (r - 0.3) + Vec3::Y * 2.2, pr, Vec3::Y, pf, Vec3::new(0.28, 2.2, 0.28), shade(trim, 1.1));
        lamps.obox(pf * (r - 0.62) + Vec3::Y * 3.6, pr, Vec3::Y, pf, Vec3::splat(0.16), c(0xffe8a0));
    }
    // Sky dome (inside-out, vertex-coloured).
    let (dsegs, drings) = (16usize, 4usize);
    for k in 0..drings {
        let (e0, e1) = (k as f32 / drings as f32 * 1.2, (k + 1) as f32 / drings as f32 * 1.2);
        for j in 0..dsegs {
            let (a0, a1) = (j as f32 / dsegs as f32 * TAU, (j + 1) as f32 / dsegs as f32 * TAU);
            let pt = |e: f32, a: f32| Vec3::new(a.sin() * e.cos(), e.sin(), a.cos() * e.cos()) * 60.0;
            let col = |e: f32| super::fb::mix(c(p.sky_low), c(p.sky_top), (e / 1.2).powf(0.7));
            let q = [pt(e0, a0), pt(e0, a1), pt(e1, a1), pt(e1, a0)];
            sky.quad_away(q, [col(e0), col(e0), col(e1), col(e1)], (q[0] + q[2]) * 1.5);
        }
    }
    (solid.build(), sky.build(), lamps.build())
}

#[derive(Resource)]
pub struct SceneAssets {
    pub lit: Handle<StandardMaterial>,
    pub unlit: Handle<StandardMaterial>,
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_scene(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    target: Handle<Image>,
    doors: &[Vec<ExperimentId>; 2],
) {
    let lit = mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 1.0, reflectance: 0.0, ..default() });
    let unlit = mats.add(StandardMaterial { base_color: Color::WHITE, unlit: true, cull_mode: None, ..default() });
    let layer = RenderLayers::layer(LAYER);
    commands.insert_resource(SceneAssets { lit: lit.clone(), unlit: unlit.clone() });
    commands.spawn((
        RetroCam,
        Camera3d::default(),
        Camera { order: -5, is_active: false, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderTarget::Image(target.into()),
        Msaa::Off,
        Tonemapping::None,
        // Bevy warns each time this camera is created when dithering is left on with no tonemapping.
        bevy::core_pipeline::tonemapping::DebandDither::Disabled,
        AmbientLight { color: Color::WHITE, brightness: 900.0, ..default() },
        Projection::Perspective(PerspectiveProjection { fov: FOV, near: 0.1, far: 200.0, ..default() }),
        Transform::from_xyz(0.0, 1.5, 6.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        layer.clone(),
    ));
    commands.spawn((DirectionalLight { illuminance: 12000.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 10.0, 0.0).looking_to(Vec3::new(-0.35, -1.0, -0.5), Vec3::Y), layer.clone()));

    // Hero (shared by the title and the hub).
    let body = meshes.add(hero_body());
    let head = meshes.add(hero_head());
    let tail = meshes.add(hero_tail());
    commands
        .spawn((Hero, Transform::default(), Visibility::Hidden, layer.clone()))
        .with_children(|h| {
            h.spawn((Mesh3d(body), MeshMaterial3d(lit.clone()), Transform::default(), layer.clone()));
            h.spawn((HeroHead, Mesh3d(head), MeshMaterial3d(lit.clone()), Transform::from_xyz(0.0, 1.36, 0.0), layer.clone()));
            h.spawn((HeroTail, Mesh3d(tail), MeshMaterial3d(lit.clone()), Transform::from_xyz(0.0, 0.8, -0.2), layer.clone()));
        });

    // Title: gradient backdrop and the gem.
    let back = meshes.add(gradient_quad(90.0, 70.0, c(0x050c3c), c(0x2a63e8)));
    commands.spawn((TitleRoot, Transform::from_xyz(0.0, 1.0, -30.0), Visibility::Hidden, layer.clone())).with_children(|t| {
        t.spawn((Mesh3d(back), MeshMaterial3d(unlit.clone()), Transform::default(), layer.clone()));
    });
    let gem = meshes.add(gem_mesh());
    commands.spawn((GemProp, Mesh3d(gem), MeshMaterial3d(lit.clone()), Transform::from_scale(Vec3::splat(0.9)), Visibility::Hidden, layer.clone()));

    // Hub rooms.
    for wing in [Wing::Games, Wing::Lab] {
        let list = &doors[wing.idx()];
        let (solid, sky, lamps) = room_meshes(wing, list);
        let (solid, sky, lamps) = (meshes.add(solid), meshes.add(sky), meshes.add(lamps));
        let n = list.len().max(1);
        commands.spawn((RoomRoot(wing), Transform::default(), Visibility::Hidden, layer.clone())).with_children(|room| {
            room.spawn((Mesh3d(solid), MeshMaterial3d(lit.clone()), Transform::default(), layer.clone()));
            room.spawn((Mesh3d(sky), MeshMaterial3d(unlit.clone()), Transform::default(), layer.clone()));
            room.spawn((Mesh3d(lamps), MeshMaterial3d(unlit.clone()), Transform::default(), layer.clone()));
            for i in 0..list.len() {
                let a = door_angle(i, n);
                let f = dir_of(a);
                let mat = mats.add(StandardMaterial { base_color: Color::srgb(0.03, 0.03, 0.06), unlit: true, cull_mode: None, ..default() });
                let quad = meshes.add(Rectangle::new(DOOR_HALF_W * 2.0, DOOR_H));
                // Rectangle faces +Z; turn it so it faces the room centre.
                room.spawn((
                    DoorGlow { wing, index: i, mat: mat.clone() },
                    Mesh3d(quad),
                    MeshMaterial3d(mat),
                    Transform::from_translation(f * (ring_radius(wing) - 0.12) + Vec3::Y * DOOR_H / 2.0).with_rotation(Quat::from_rotation_y(a + PI)),
                    layer.clone(),
                ));
            }
        });
    }
}

// ------------------------------------------------------------------------------------------
// Camera
// ------------------------------------------------------------------------------------------

/// The camera the overlay projects with (same maths as the real one).
#[derive(Resource, Clone, Copy, Debug)]
pub struct CamState {
    pub eye: Vec3,
    pub target: Vec3,
    pub aspect: f32,
    /// Hub orbit angle (the direction of the selected door), smoothed.
    pub yaw: f32,
    /// 0..1 intro dolly of the hub.
    pub intro: f32,
}

impl Default for CamState {
    fn default() -> Self {
        Self { eye: Vec3::new(0.0, 1.5, 6.0), target: Vec3::new(0.0, 1.0, 0.0), aspect: 4.0 / 3.0, yaw: 0.0, intro: 0.0 }
    }
}

impl CamState {
    /// World point -> framebuffer px (x, y) and the view depth; `None` behind the camera.
    pub fn project(&self, p: Vec3, w: f32, h: f32) -> Option<(f32, f32, f32)> {
        let view = Mat4::look_at_rh(self.eye, self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(FOV, self.aspect, 0.1, 200.0);
        let v = view * p.extend(1.0);
        if v.z >= -0.05 {
            return None;
        }
        let clip = proj * v;
        let ndc = clip.truncate() / clip.w;
        Some(((ndc.x * 0.5 + 0.5) * w, (0.5 - ndc.y * 0.5) * h, -v.z))
    }
}

pub fn shortest_angle(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Hub camera for an orbit angle: behind the hero, looking out toward the door.
pub fn hub_camera(wing: Wing, yaw: f32, intro: f32, enter: f32) -> (Vec3, Vec3) {
    let f = dir_of(yaw);
    let e = smooth(intro);
    let radius = 4.7 + (1.0 - e) * 5.0;
    let height = 1.9 + (1.0 - e) * 2.2;
    let door = f * (ring_radius(wing) - 0.5);
    let base = -f * radius + Vec3::Y * height;
    let look = f * 4.0 + Vec3::Y * 1.45;
    let p = smooth(enter);
    let eye = base.lerp(door + Vec3::Y * 1.7, p);
    let target = look.lerp(door * 1.5 + Vec3::Y * 1.7, p);
    (eye, target)
}

pub fn title_camera(aspect: f32) -> (Vec3, Vec3) {
    let _ = aspect;
    (Vec3::new(0.0, 1.25, 6.4), Vec3::new(0.0, 1.05, 0.0))
}

/// Half-width of the view at distance `d` for the given aspect.
pub fn half_view_width(d: f32, aspect: f32) -> f32 {
    d * (FOV / 2.0).tan() * aspect
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn animate_scene(
    time: Res<Time<Real>>,
    menu: Res<super::MenuRes>,
    ctx: Res<super::CtxRes>,
    mut cam_state: ResMut<CamState>,
    active: Res<super::RetroActive>,
    low: Res<super::LowRes>,
    mut cam: Query<(&mut Transform, &mut Camera, &mut Projection), With<RetroCam>>,
    mut hero: Query<(&mut Transform, &mut Visibility), (With<Hero>, Without<RetroCam>, Without<GemProp>, Without<HeroHead>, Without<HeroTail>)>,
    mut head: Query<&mut Transform, (With<HeroHead>, Without<Hero>, Without<RetroCam>, Without<HeroTail>, Without<GemProp>)>,
    mut tail: Query<&mut Transform, (With<HeroTail>, Without<Hero>, Without<RetroCam>, Without<HeroHead>, Without<GemProp>)>,
    mut gem: Query<(&mut Transform, &mut Visibility), (With<GemProp>, Without<Hero>, Without<RetroCam>, Without<HeroHead>, Without<HeroTail>)>,
    mut title: Query<&mut Visibility, (With<TitleRoot>, Without<Hero>, Without<GemProp>, Without<RoomRoot>)>,
    mut rooms: Query<(&RoomRoot, &mut Visibility), (Without<Hero>, Without<GemProp>, Without<TitleRoot>)>,
    glows: Query<&DoorGlow>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut last_page: Local<Option<(Page, Wing)>>,
) {
    let Ok((mut cam_tf, mut camera, mut proj)) = cam.single_mut() else { return };
    camera.is_active = active.0;
    if !active.0 {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();
    let m = &menu.0;
    let aspect = low.aspect();
    cam_state.aspect = aspect;
    if let Projection::Perspective(p) = &mut *proj {
        p.aspect_ratio = aspect;
    }
    // Page / wing switches: restart the intro dolly, snap the orbit.
    let key = (m.page, m.wing);
    let n = m.doors(&ctx.0).len().max(1);
    let want = door_angle(m.cur_door(), n);
    if *last_page != Some(key) {
        if m.page == Page::Hub {
            cam_state.intro = if last_page.is_some_and(|(p, _)| p == Page::Hub) { 1.0 } else { 0.0 };
            cam_state.yaw = want;
        }
        *last_page = Some(key);
    }
    let hub = m.page == Page::Hub;
    let select = m.page == Page::Select;
    cam_state.intro = (cam_state.intro + dt / 0.9).min(1.0);
    let diff = shortest_angle(cam_state.yaw, want);
    cam_state.yaw += diff * (1.0 - (-9.0 * dt).exp());

    for (room, mut vis) in &mut rooms {
        *vis = if hub && room.0 == m.wing { Visibility::Inherited } else { Visibility::Hidden };
    }
    for mut v in &mut title {
        *v = if hub || select { Visibility::Hidden } else { Visibility::Inherited };
    }

    let (eye, target) = if hub {
        hub_camera(m.wing, cam_state.yaw, cam_state.intro, m.enter_progress())
    } else if select {
        super::select::camera(low.w, low.h)
    } else {
        title_camera(aspect)
    };
    cam_state.eye = eye;
    cam_state.target = target;
    *cam_tf = Transform::from_translation(eye).looking_at(target, Vec3::Y);

    if let Ok((mut tf, mut vis)) = hero.single_mut() {
        *vis = if select { Visibility::Hidden } else { Visibility::Inherited };
        if hub {
            let facing = cam_state.yaw + PI + (t * 0.8).sin() * 0.12;
            *tf = Transform::from_translation(Vec3::new(0.0, (t * 2.4).sin() * 0.012, 0.0)).with_rotation(Quat::from_rotation_y(facing)).with_scale(Vec3::splat(0.92));
        } else {
            let d = 6.4;
            let x = -half_view_width(d, aspect) * 0.58;
            *tf = Transform::from_translation(Vec3::new(x, (t * 2.0).sin() * 0.015, 0.0)).with_rotation(Quat::from_rotation_y(0.5 + (t * 0.7).sin() * 0.28)).with_scale(Vec3::splat(1.25));
        }
    }
    if let Ok(mut h) = head.single_mut() {
        h.rotation = Quat::from_rotation_z((t * 1.3).sin() * 0.04) * Quat::from_rotation_y((t * 0.9).sin() * 0.12);
    }
    if let Ok(mut tl) = tail.single_mut() {
        tl.rotation = Quat::from_rotation_y((t * 3.0).sin() * 0.25);
    }
    if let Ok((mut tf, mut vis)) = gem.single_mut() {
        *vis = if hub || select { Visibility::Hidden } else { Visibility::Inherited };
        let x = -half_view_width(6.4, aspect) * 0.12;
        *tf = Transform::from_translation(Vec3::new(x, 1.35 + (t * 1.6).sin() * 0.12, 0.0)).with_rotation(Quat::from_rotation_y(t * 1.1)).with_scale(Vec3::splat(1.15));
    }
    // The selected door lights up warmly; the rest stay dark.
    for g in &glows {
        let selected = hub && g.wing == m.wing && g.index == m.cur_door();
        let pulse = 0.85 + 0.15 * (t * 4.0).sin();
        let target = if selected { Color::srgb(1.0 * pulse, 0.86 * pulse, 0.5 * pulse) } else { Color::srgb(0.03, 0.03, 0.07) };
        if let Some(mut mat) = mats.get_mut(&g.mat) {
            if mat.base_color != target {
                mat.base_color = target;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortest_angle_wraps_both_ways() {
        assert!((shortest_angle(0.1, TAU - 0.1) + 0.2).abs() < 1e-4);
        assert!((shortest_angle(TAU - 0.1, 0.1) - 0.2).abs() < 1e-4);
    }

    #[test]
    fn selected_door_projects_near_the_centre() {
        let (eye, target) = hub_camera(Wing::Games, door_angle(3, 11), 1.0, 0.0);
        let cam = CamState { eye, target, aspect: 4.0 / 3.0, ..Default::default() };
        let (x, _, d) = cam.project(sign_anchor(Wing::Games, 3, 11), 320.0, 240.0).expect("in front");
        assert!((x - 160.0).abs() < 2.0, "sign x {x}");
        assert!(d > 5.0);
        // The next door sits to one side, in view.
        let (nx, _, _) = cam.project(sign_anchor(Wing::Games, 4, 11), 320.0, 240.0).expect("in front");
        assert!((nx - 160.0).abs() > 50.0 && nx > 0.0 && nx < 320.0, "neighbour x {nx}");
    }

    #[test]
    fn builders_make_valid_meshes() {
        for m in [hero_body(), hero_head(), hero_tail(), gem_mesh()] {
            let n = m.count_vertices();
            assert!(n > 0 && n % 3 == 0);
        }
        let (a, b, c) = room_meshes(Wing::Lab, &[ExperimentId::God, ExperimentId::Diorama]);
        assert!(a.count_vertices() > 100 && b.count_vertices() > 0 && c.count_vertices() > 0);
    }
}
