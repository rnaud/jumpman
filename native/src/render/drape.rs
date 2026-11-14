//! Ground-draped effect meshes: poison pools, telegraph rings, blob shadows, shockwaves, hazard circles...
//!
//! The minigames draw their ground effects as flat discs / rings / quads, scaled and placed by their
//! transform. On a slope or a stepped floor a flat shape sinks into the terrain on one side and floats
//! on the other. Tagging the entity with [`Drape`] swaps its flat mesh for a private, subdivided copy
//! whose every vertex sits on the ground (plus a small lift against z-fighting).
//!
//! The draped copy is built in the entity's own local frame: each grid vertex of the flat outline goes
//! to the world through the entity's `GlobalTransform`, takes the ground height there, and comes back
//! through the inverse. So the mode's placement code (position, yaw, the "lie flat" rotation, the
//! radius as scale) is unchanged, and the gameplay radius still matches the picture: the outline seen
//! from above is the same circle, only its heights follow the floor. The outline is read from the flat
//! mesh the entity carries ([`outline_of`]), so a pool that swaps disc / ring / slice meshes, or the N64
//! effect layer re-meshing it with its low-poly shapes, is followed automatically. The local plane
//! coordinates are kept, so the N64 effect patterns (drawn from local x/y) look the same.
//!
//! Heights march outward from the centre: each vertex may differ from its inner neighbour by a small step,
//! or more only along a continuous slope of at most 45 degrees (checked at the midpoint, so a pillar, a
//! post, a ledge or a wall top is a jump and is not climbed; a drop is bridged flat at the edge height).
//! Floors steeper than 45 degrees are ignored, and a vertex left standing above the plane of its
//! neighbours by more than a step (a one-sample post) is put back on it.
//! The ground comes from [`DrapeGround`], which each mode sets on entry (the same floor its actors use)
//! and the experiment host clears on exit. With no ground, or an effect well above the floor (an
//! airborne blast ring, a sword swing at hip height), the shape stays flat.
//!
//! Cost: a mesh is rebuilt only when its transform moved past a small threshold (static zones are built
//! once), and the grid is capped (9 rings x 48 segments, 12 x 12 for quads): a moving blob shadow is
//! ~100-150 floor queries a frame.
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::sync::Arc;

use super::SCALE;

/// Highest floor under world (x, z) at most about `max_y` (Bevy world units).
pub type GroundFn = Arc<dyn Fn(f32, f32, f32) -> Option<f32> + Send + Sync>;

/// The floor that [`Drape`] effects follow, set by the running mode.
#[derive(Resource, Default, Clone)]
pub struct DrapeGround(pub Option<GroundFn>);

impl DrapeGround {
    pub fn set(&mut self, f: impl Fn(f32, f32, f32) -> Option<f32> + Send + Sync + 'static) {
        self.0 = Some(Arc::new(f));
    }
    pub fn clear(&mut self) {
        if self.0.is_some() {
            self.0 = None;
        }
    }
    /// A collision world in SM64 units drawn at [`SCALE`] (what most modes play on).
    pub fn set_world(&mut self, world: Arc<jumpman_sim::collision::CollisionWorld>) {
        self.set(move |x, z, max_y| course_floor(&world, x, z, max_y));
    }
}

/// The topmost floor of a collision world at or just below `max_y`, in Bevy units.
pub fn course_floor(world: &jumpman_sim::collision::CollisionWorld, x: f32, z: f32, max_y: f32) -> Option<f32> {
    let (x, z, top) = (x as f64 / SCALE as f64, z as f64 / SCALE as f64, max_y as f64 / SCALE as f64);
    let mut q = top;
    for _ in 0..4 {
        let h = world.find_floor(x, q, z);
        let s = h.surface.as_ref()?;
        // Steeper than about 45 degrees (a pillar's flank, a cliff face the collision counts as floor) is not ground
        // to lie on: look for the floor under it, else the drape carries on flat from its neighbour.
        if h.height <= top + 0.01 && s.normal[1] >= MIN_NORMAL_Y {
            return Some(h.height as f32 * SCALE);
        }
        // find_floor accepts floors up to 78 above the query: step below the one it found.
        q = h.height - 79.0;
    }
    None
}

/// The outline of the flat mesh an effect was drawn with, in its local plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A filled circle (`Circle`).
    Disc { r: f32 },
    /// A ring (`Annulus`).
    Ring { inner: f32, outer: f32 },
    /// A pie slice (`CircularSector`, the relic fan): `mid` is the angle of its centre line in the plane.
    Sector { r: f32, half: f32, mid: f32 },
    /// A rectangle of half extents (hx, hy) (`Plane3d`, `Rectangle`).
    Rect { hx: f32, hy: f32 },
}

/// Which local plane the flat shape lies in: Bevy's 2D primitives are in XY (normal +Z); `Plane3d` is XZ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plane {
    XY,
    XZ,
}

/// How far a vertex may sit above / below its inner neighbour as a plain step (Bevy units). A bigger rise is
/// only followed when it is a continuous slope of at most 45 degrees (the floor halfway between the two is
/// halfway up); a jump (a pillar, a ledge, a wall top) is not climbed and a drop is bridged flat.
pub const STEP: f32 = 30.0 * SCALE;
pub const DROP: f32 = 30.0 * SCALE;
/// Tolerance of the slope test (how far the floor halfway may be off the straight line).
pub const SPIKE: f32 = 10.0 * SCALE;
/// Floors steeper than this (normal y; about 45 degrees) are not draped over.
pub const MIN_NORMAL_Y: f64 = 0.7;
/// An effect more than this above the floor under its centre is airborne and stays flat.
pub const AIRBORNE: f32 = 120.0 * SCALE;
/// Target spacing between samples (world units).
pub const SPACING: f32 = 45.0 * SCALE;
pub const MAX_RINGS: usize = 9;
pub const MAX_SEGS: usize = 48;
pub const MAX_GRID: usize = 12;

/// Drape this entity's flat mesh over the ground (see the module docs). `lift` is the height above the
/// floor in Bevy units. Systems that swap the mesh should compare against [`Drape::shows`] rather than
/// the entity's current `Mesh3d` (which is the draped copy).
#[derive(Component, Clone)]
#[require(NoFrustumCulling)]
pub struct Drape {
    pub lift: f32,
    source: Option<Handle<Mesh>>,
    outline: Option<(Shape, Plane)>,
    state: Option<State>,
}

#[derive(Clone)]
struct State {
    own: Handle<Mesh>,
    at: Mat4,
}

impl Drape {
    pub fn new(lift: f32) -> Self {
        Self { lift, source: None, outline: None, state: None }
    }
    /// The flat mesh this drape copies (None until first built).
    pub fn source(&self) -> Option<AssetId<Mesh>> {
        self.source.as_ref().map(|h| h.id())
    }
    /// Does an entity showing `current` show `mesh` (directly, or as the source of its draped copy)?
    pub fn shows(drape: Option<&Drape>, current: &Handle<Mesh>, mesh: &Handle<Mesh>) -> bool {
        current == mesh || drape.is_some_and(|d| d.source() == Some(mesh.id()))
    }
    /// The outline read from the source mesh (tests).
    #[cfg(test)]
    pub fn outline(&self) -> Option<(Shape, Plane)> {
        self.outline
    }
}

/// Read the outline of a flat mesh: which plane it lies in, and whether it is a disc, ring, pie slice or
/// rectangle. None for anything that is not flat (spheres, cones) or has no positions.
pub fn outline_of(mesh: &Mesh) -> Option<(Shape, Plane)> {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    if p.len() < 3 {
        return None;
    }
    let ext = p.iter().fold(Vec3::ZERO, |m, v| m.max(Vec3::from_array(*v).abs()));
    let eps = 1e-4 * ext.max_element().max(1e-6);
    let (plane, q): (Plane, Vec<Vec2>) = if ext.z <= eps {
        (Plane::XY, p.iter().map(|v| Vec2::new(v[0], v[1])).collect())
    } else if ext.y <= eps {
        (Plane::XZ, p.iter().map(|v| Vec2::new(v[0], -v[2])).collect())
    } else {
        return None;
    };
    let (hx, hy) = q.iter().fold((0.0f32, 0.0f32), |(x, y), v| (x.max(v.x.abs()), y.max(v.y.abs())));
    let tol = 1e-3 * hx.max(hy);
    let corner = |c: Vec2| q.iter().any(|v| v.distance(c) <= tol);
    if [Vec2::new(hx, hy), Vec2::new(-hx, hy), Vec2::new(hx, -hy), Vec2::new(-hx, -hy)].into_iter().all(corner) {
        return Some((Shape::Rect { hx, hy }, plane));
    }
    let r: Vec<f32> = q.iter().map(|v| v.length()).collect();
    let outer = r.iter().copied().fold(0.0, f32::max);
    let inner = r.iter().copied().fold(f32::MAX, f32::min);
    if outer <= 0.0 {
        return None;
    }
    // Bevy's `Circle` is a rim-only fan (every vertex on the rim): a disc, not a ring.
    if inner > 0.02 * outer && inner < 0.98 * outer {
        return Some((Shape::Ring { inner, outer }, plane));
    }
    // A disc or a slice: a slice leaves one wide gap between the angles of its rim.
    let mut a: Vec<f32> = q.iter().zip(&r).filter(|(_, r)| **r > 0.5 * outer).map(|(v, _)| v.y.atan2(v.x)).collect();
    a.sort_by(f32::total_cmp);
    a.dedup_by(|x, y| (*x - *y).abs() < 1e-4);
    if a.len() < 3 {
        return None;
    }
    let tau = std::f32::consts::TAU;
    let mut gaps: Vec<(f32, usize)> = (0..a.len()).map(|i| (if i + 1 < a.len() { a[i + 1] - a[i] } else { a[0] + tau - a[i] }, i)).collect();
    gaps.sort_by(|x, y| x.0.total_cmp(&y.0));
    let (wide, at) = gaps[gaps.len() - 1];
    let typical = gaps[gaps.len() / 2].0;
    if wide > 0.3 && wide > 3.0 * typical {
        let half = (tau - wide) * 0.5;
        // The arc runs from the angle after the gap round to the one before it.
        let start = a[(at + 1) % a.len()];
        return Some((Shape::Sector { r: outer, half, mid: start + half }, plane));
    }
    Some((Shape::Disc { r: outer }, plane))
}

/// A flat 2D grid over a shape: plane coordinates, texture coordinates, triangle indices, and for each
/// vertex its neighbour one step nearer the centre (None: the centre itself is next).
pub struct Grid {
    pub uv: Vec<Vec2>,
    pub tex: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub parent: Vec<Option<u32>>,
}

/// Polar or square grid over a shape; `k` is the local-to-world length factor (picks the resolution).
pub fn grid(shape: Shape, k: f32) -> Grid {
    let mut g = Grid { uv: vec![], tex: vec![], indices: vec![], parent: vec![] };
    let tau = std::f32::consts::TAU;
    match shape {
        Shape::Disc { .. } | Shape::Ring { .. } | Shape::Sector { .. } => {
            let (inner, outer, a0, span, closed) = match shape {
                Shape::Ring { inner, outer } => (inner.clamp(0.0, outer * 0.999), outer, 0.0, tau, true),
                Shape::Sector { r, half, mid } => (0.0, r, mid - half, 2.0 * half, false),
                Shape::Disc { r } => (0.0, r, 0.0, tau, true),
                Shape::Rect { .. } => unreachable!(),
            };
            let band = (outer - inner) * k;
            let disc = inner == 0.0;
            let rings = ((band / SPACING).ceil() as usize).clamp(if disc { 2 } else { 1 }, MAX_RINGS);
            let full = (tau * outer * k / SPACING).ceil().clamp(16.0, MAX_SEGS as f32);
            let segs = ((full * span / tau).ceil() as usize).max(if closed { 16 } else { 4 });
            let cols = if closed { segs } else { segs + 1 };
            if disc {
                g.uv.push(Vec2::ZERO);
                g.parent.push(None);
            }
            let first = if disc { 1 } else { 0 };
            let base = if disc { 1 } else { 0 };
            let at = |ring: usize, s: usize| -> u32 { (base + (ring - first) * cols + s % cols) as u32 };
            for ring in first..=rings {
                let rad = inner + (outer - inner) * ring as f32 / rings as f32;
                for s in 0..cols {
                    let a = a0 + span * s as f32 / segs as f32;
                    g.uv.push(Vec2::new(a.cos(), a.sin()) * rad);
                    g.parent.push(if ring == first { if disc { Some(0) } else { None } } else { Some(at(ring - 1, s)) });
                }
            }
            if disc {
                for s in 0..segs {
                    g.indices.extend_from_slice(&[0, at(1, s), at(1, s + 1)]);
                }
            }
            for ring in first..rings {
                for s in 0..segs {
                    let (a, b, c, d) = (at(ring, s), at(ring, s + 1), at(ring + 1, s), at(ring + 1, s + 1));
                    g.indices.extend_from_slice(&[a, c, d, a, d, b]);
                }
            }
            g.tex = g.uv.iter().map(|p| [0.5 + 0.5 * p.x / outer, 0.5 - 0.5 * p.y / outer]).collect();
        }
        Shape::Rect { hx, hy } => {
            let n = |h: f32| ((2.0 * h * k / SPACING).ceil() as usize).clamp(2, MAX_GRID);
            let (nx, ny) = (n(hx), n(hy));
            let at = |i: usize, j: usize| (j * (nx + 1) + i) as u32;
            // One step toward the middle on each axis that is at least a step away from it.
            let toward = |i: usize, n: usize| -> usize {
                let d = i as f32 - n as f32 * 0.5;
                if d >= 1.0 {
                    i - 1
                } else if d <= -1.0 {
                    i + 1
                } else {
                    i
                }
            };
            for j in 0..=ny {
                for i in 0..=nx {
                    g.uv.push(Vec2::new(-hx + 2.0 * hx * i as f32 / nx as f32, -hy + 2.0 * hy * j as f32 / ny as f32));
                    let (pi, pj) = (toward(i, nx), toward(j, ny));
                    g.parent.push(if (pi, pj) == (i, j) { None } else { Some(at(pi, pj)) });
                }
            }
            for j in 0..ny {
                for i in 0..nx {
                    let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
                    g.indices.extend_from_slice(&[a, b, d, a, d, c]);
                }
            }
            g.tex = g.uv.iter().map(|p| [0.5 + 0.5 * p.x / hx, 0.5 - 0.5 * p.y / hy]).collect();
        }
    }
    g
}

fn local(plane: Plane, p: Vec2) -> Vec3 {
    match plane {
        Plane::XY => Vec3::new(p.x, p.y, 0.0),
        // Plane3d: the 2D "y" runs along -Z.
        Plane::XZ => Vec3::new(p.x, 0.0, -p.y),
    }
}

/// The draped grid of a shape placed by `to_world`, positions in local space. Flat (the plain grid) when
/// the effect is airborne or there is no ground under its centre.
pub fn drape_positions(shape: Shape, plane: Plane, lift: f32, to_world: Mat4, ground: &dyn Fn(f32, f32, f32) -> Option<f32>) -> (Grid, Vec<Vec3>) {
    let k = to_world.transform_vector3(local(plane, Vec2::X)).length().max(to_world.transform_vector3(local(plane, Vec2::Y)).length());
    let g = grid(shape, k);
    let flat: Vec<Vec3> = g.uv.iter().map(|p| local(plane, *p)).collect();
    let centre = to_world.transform_point3(Vec3::ZERO);
    let cg = ground(centre.x, centre.z, centre.y + STEP);
    let Some(cg) = cg.filter(|cg| centre.y - cg <= AIRBORNE && to_world.determinant().abs() > 1e-12) else {
        return (g, flat);
    };
    let world: Vec<Vec3> = flat.iter().map(|v| to_world.transform_point3(*v)).collect();
    // Nearest the centre first, so every vertex's inner neighbour is known before it.
    let mut order: Vec<usize> = (0..world.len()).collect();
    order.sort_by(|a, b| g.uv[*a].length_squared().total_cmp(&g.uv[*b].length_squared()));
    let mut h = vec![cg; world.len()];
    for i in order {
        let w = world[i];
        let (from, d) = match g.parent[i] {
            Some(p) => (h[p as usize], Vec2::new(w.x - world[p as usize].x, w.z - world[p as usize].z).length()),
            None => (cg, Vec2::new(w.x - centre.x, w.z - centre.z).length()),
        };
        h[i] = match ground(w.x, w.z, from + STEP + d) {
            // A small step either way.
            Some(y) if (y - from).abs() <= STEP => y,
            // More than a step, within 45 degrees: followed only if it is a slope, i.e. the floor halfway is about
            // halfway up. A jump (pillar, ledge, wall top, cliff) has the midpoint level with one end: not followed.
            Some(y) if y >= from - DROP - d => {
                let (mx, mz) = match g.parent[i] {
                    Some(p) => ((w.x + world[p as usize].x) * 0.5, (w.z + world[p as usize].z) * 0.5),
                    None => ((w.x + centre.x) * 0.5, (w.z + centre.z) * 0.5),
                };
                let half = (y + from) * 0.5;
                match ground(mx, mz, from.max(y) + STEP) {
                    // (A long span, the centre to an annulus' first ring, may bend a little more.)
                    Some(m) if (m - half).abs() <= 0.25 * (y - from).abs() + 0.5 * SPIKE + 0.3 * (d - 2.0 * SPACING).max(0.0) => y,
                    _ => from,
                }
            }
            _ => from,
        };
    }
    despike(&g, &world, &mut h);
    let inv = to_world.inverse();
    let pos = world.iter().zip(&h).map(|(w, y)| inv.transform_point3(Vec3::new(w.x, y + lift, w.z))).collect();
    (g, pos)
}

/// Level every vertex that stands out above the plane through its grid neighbours by more than a step + a
/// quarter of the distance to the nearest: a one-sample post, not terrain to follow (anything lower is a
/// bump the shape may lie over).
fn despike(g: &Grid, world: &[Vec3], h: &mut [f32]) {
    let mut near: Vec<Vec<u32>> = vec![vec![]; h.len()];
    for t in g.indices.chunks(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if !near[a as usize].contains(&b) {
                near[a as usize].push(b);
                near[b as usize].push(a);
            }
        }
    }
    for _ in 0..2 {
        for i in 0..h.len() {
            let n = &near[i];
            if n.is_empty() {
                continue;
            }
            // The plane through the neighbours (least squares, relative to this vertex) predicts its height: right on a
            // slope, at a rim (where every neighbour is on one side) and on a hill; a post stands far above it.
            let (mut m, mut r, mut reach) = (Mat3::ZERO, Vec3::ZERO, f32::MAX);
            for j in n {
                let j = *j as usize;
                let (dx, dz) = (world[j].x - world[i].x, world[j].z - world[i].z);
                let v = Vec3::new(1.0, dx, dz);
                m += Mat3::from_cols(v * v.x, v * v.y, v * v.z);
                r += v * h[j];
                reach = reach.min(Vec2::new(dx, dz).length());
            }
            if n.len() < 3 || m.determinant().abs() < 1e-9 {
                continue;
            }
            let predicted = (m.inverse() * r).x;
            // Only upward: a post poking through the effect is what shows; a vertex sunk in a hole is hidden by it.
            if h[i] > predicted + STEP + 0.25 * reach {
                h[i] = predicted;
            }
        }
    }
}

fn build_mesh(g: Grid, pos: Vec<Vec3>, plane: Plane) -> Mesh {
    let n = match plane {
        Plane::XY => [0.0, 0.0, 1.0],
        Plane::XZ => [0.0, 1.0, 0.0],
    };
    let count = pos.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos.into_iter().map(|p| p.to_array()).collect::<Vec<_>>())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![n; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, g.tex)
        .with_inserted_indices(Indices::U32(g.indices))
}

/// Has the transform moved enough since the last build to need a new drape?
pub fn moved(a: &Mat4, b: &Mat4) -> bool {
    let (sa, ra, ta) = a.to_scale_rotation_translation();
    let (sb, rb, tb) = b.to_scale_rotation_translation();
    ta.distance(tb) > 2.0 * SCALE || (sa - sb).abs().max_element() > 0.01 * sa.abs().max_element().max(1e-6) || ra.angle_between(rb) > 0.01
}

pub struct DrapePlugin;

impl Plugin for DrapePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrapeGround>().add_systems(
            PostUpdate,
            drape_meshes.after(bevy::transform::TransformSystems::Propagate).after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
        );
    }
}

fn drape_meshes(ground: Res<DrapeGround>, mut meshes: ResMut<Assets<Mesh>>, mut q: Query<(&mut Drape, &mut Mesh3d, &GlobalTransform, &InheritedVisibility)>) {
    let none = |_: f32, _: f32, _: f32| None;
    let gf: &dyn Fn(f32, f32, f32) -> Option<f32> = match &ground.0 {
        Some(f) => f.as_ref(),
        None => &none,
    };
    // `JUMPMAN_NO_DRAPE=1` leaves every effect flat (A/B screenshots).
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *OFF.get_or_init(|| std::env::var_os("JUMPMAN_NO_DRAPE").is_some()) {
        return;
    }
    let regrounded = ground.is_changed();
    for (mut drape, mut mesh, gt, vis) in &mut q {
        if !vis.get() {
            continue;
        }
        let current = mesh.0.id();
        let resourced = drape.state.as_ref().is_none_or(|s| s.own.id() != current);
        if resourced {
            // A new flat mesh was put on the entity (first frame, or another system re-meshed it): read its outline.
            if drape.source() == Some(current) && drape.outline.is_none() {
                continue; // not a flat shape: left alone
            }
            let Some(src) = meshes.get(&mesh.0) else { continue };
            let outline = outline_of(src);
            drape.source = Some(mesh.0.clone());
            drape.outline = outline;
        }
        let Some((shape, plane)) = drape.outline else { continue };
        let at = gt.to_matrix();
        if !resourced && !regrounded && drape.state.as_ref().is_some_and(|s| !moved(&s.at, &at)) {
            continue;
        }
        let (g, pos) = drape_positions(shape, plane, drape.lift, at, gf);
        let built = build_mesh(g, pos, plane);
        let own = match drape.state.as_ref().map(|s| s.own.clone()) {
            Some(own) if meshes.get(&own).is_some() => {
                if let Some(mut m) = meshes.get_mut(&own) {
                    *m = built;
                }
                own
            }
            _ => meshes.add(built),
        };
        if mesh.0.id() != own.id() {
            mesh.0 = own.clone();
        }
        drape.state = Some(State { own, at });
    }
}

/// Every vertex of `mesh` (taken through `to_world`) and its height above `ground` (tests).
#[cfg(test)]
pub fn heights_above(mesh: &Mesh, to_world: Mat4, ground: &dyn Fn(f32, f32) -> f32) -> Vec<f32> {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return vec![] };
    p.iter()
        .map(|v| {
            let w = to_world.transform_point3(Vec3::from_array(*v));
            w.y - ground(w.x, w.z)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIFT: f32 = 3.0 * SCALE;

    fn slope(x: f32, z: f32) -> f32 {
        0.35 * x + 0.1 * z + 2.0
    }
    fn steep(x: f32, z: f32) -> f32 {
        // Nearly 45 degrees: still followed all the way up and down.
        0.9 * x - 0.2 * z
    }
    fn stepped(x: f32, z: f32) -> f32 {
        // 18-unit steps every 60 units across x and 10 every 90 across z: terraces under the step, not walls.
        (x / 0.6).floor() * 0.18 + (z / 0.9).floor() * 0.1
    }
    fn wavy(x: f32, z: f32) -> f32 {
        0.3 * (x * 1.7).sin() + 0.2 * (z * 2.3).cos()
    }

    fn placed(at: Vec3, radius: f32, yaw: f32) -> Mat4 {
        // How the modes place a unit Circle / Annulus: lie it flat, yaw it, scale to the radius.
        Mat4::from_scale_rotation_translation(Vec3::splat(radius), Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2), at)
    }

    fn check(name: &str, shape: Shape, plane: Plane, to_world: Mat4, h: fn(f32, f32) -> f32) {
        let g = move |x: f32, z: f32, top: f32| Some(h(x, z)).filter(|y| *y <= top + 1e-4);
        let (grid, pos) = drape_positions(shape, plane, LIFT, to_world, &g);
        let mesh = build_mesh(grid, pos, plane);
        let up = heights_above(&mesh, to_world, &h);
        assert!(!up.is_empty());
        for d in &up {
            assert!(*d >= LIFT - 1e-3, "{name} {shape:?}: a vertex is below the lifted ground: {d}");
            assert!(*d <= LIFT + 1e-3, "{name} {shape:?}: a vertex floats: {d}");
        }
    }

    #[test]
    fn every_vertex_hugs_slopes_terraces_and_waves() {
        for (name, h) in [("slope", slope as fn(f32, f32) -> f32), ("steep", steep), ("stepped", stepped), ("wavy", wavy)] {
            for shape in [Shape::Disc { r: 1.0 }, Shape::Ring { inner: 0.8, outer: 1.0 }, Shape::Sector { r: 1.0, half: 1.1, mid: 1.5 }, Shape::Ring { inner: 1.5, outer: 1.85 }] {
                for (at, r, yaw) in [(Vec3::new(0.3, 0.0, -0.2), 1.2, 0.0), (Vec3::new(-3.0, 0.0, 4.0), 4.0, 0.9), (Vec3::new(1.0, 0.0, 1.0), 0.3, 2.0)] {
                    let at = at.with_y(h(at.x, at.z) + 0.05);
                    check(name, shape, Plane::XY, placed(at, r, yaw), h);
                }
            }
            let quad = Mat4::from_scale_rotation_translation(Vec3::new(2.0, 1.0, 3.0), Quat::from_rotation_y(0.4), Vec3::new(0.5, h(0.5, 0.5), 0.5));
            check(name, Shape::Rect { hx: 0.5, hy: 0.5 }, Plane::XZ, quad, h);
        }
    }

    #[test]
    fn the_outline_seen_from_above_keeps_the_gameplay_radius() {
        let to_world = placed(Vec3::new(1.0, slope(1.0, 2.0), 2.0), 2.5, 0.3);
        let g = |x: f32, z: f32, _: f32| Some(slope(x, z));
        let (_, pos) = drape_positions(Shape::Disc { r: 1.0 }, Plane::XY, LIFT, to_world, &g);
        let max = pos.iter().map(|p| to_world.transform_point3(*p)).map(|w| Vec2::new(w.x - 1.0, w.z - 2.0).length()).fold(0.0f32, f32::max);
        assert!((max - 2.5).abs() < 1e-3, "outer radius {max}");
    }

    #[test]
    fn triangle_midpoints_stay_close_on_a_crease() {
        // A roof ridge (the worst case for a chord): the grid is fine enough that edge midpoints stay within the lift.
        let ridge = |x: f32, _z: f32| 1.0 - 0.5 * x.abs();
        let to_world = placed(Vec3::new(0.0, 1.0, 0.0), 1.5, 0.0);
        let g = move |x: f32, z: f32, _: f32| Some(ridge(x, z));
        let (grid, pos) = drape_positions(Shape::Disc { r: 1.0 }, Plane::XY, LIFT, to_world, &g);
        let w: Vec<Vec3> = pos.iter().map(|p| to_world.transform_point3(*p)).collect();
        for t in grid.indices.chunks(3) {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let m = (w[a as usize] + w[b as usize]) * 0.5;
                assert!(m.y - ridge(m.x, m.z) > -LIFT, "an edge cuts {} into the ridge", ridge(m.x, m.z) - m.y);
            }
        }
    }

    #[test]
    fn airborne_or_groundless_effects_stay_flat() {
        let to_world = placed(Vec3::new(0.0, 5.0, 0.0), 1.0, 0.0);
        let g = |_: f32, _: f32, _: f32| Some(0.0);
        let (_, pos) = drape_positions(Shape::Disc { r: 1.0 }, Plane::XY, LIFT, to_world, &g);
        assert!(pos.iter().all(|p| p.z == 0.0), "an effect high above the floor is not pulled down to it");
        let none = |_: f32, _: f32, _: f32| None;
        let (_, pos) = drape_positions(Shape::Ring { inner: 0.5, outer: 1.0 }, Plane::XY, LIFT, to_world.mul_mat4(&Mat4::IDENTITY), &none);
        assert!(pos.iter().all(|p| p.z == 0.0));
    }

    #[test]
    fn over_a_cliff_edge_the_shape_carries_on_flat() {
        let cliff = |x: f32, _z: f32| if x > 0.5 { -10.0 } else { 0.0 };
        let to_world = placed(Vec3::new(0.0, 0.03, 0.0), 1.5, 0.0);
        let g = move |x: f32, z: f32, _: f32| Some(cliff(x, z));
        let (_, pos) = drape_positions(Shape::Disc { r: 1.0 }, Plane::XY, LIFT, to_world, &g);
        let low = pos.iter().map(|p| to_world.transform_point3(*p).y).fold(f32::MAX, f32::min);
        assert!((low - LIFT).abs() < 1e-4, "dropped to {low}");
    }

    #[test]
    fn a_wall_or_high_ledge_is_not_climbed() {
        // A ledge 3 units (300 SM64 units) up from x = 0.5: its top is above every vertex's step allowance.
        let ledge = |x: f32, top: f32| if x > 0.5 && top >= 3.0 { 3.0 } else { 0.0 };
        let to_world = placed(Vec3::new(0.0, 0.03, 0.0), 1.5, 0.0);
        let g = move |x: f32, _z: f32, top: f32| Some(ledge(x, top));
        let (_, pos) = drape_positions(Shape::Disc { r: 1.0 }, Plane::XY, LIFT, to_world, &g);
        assert!(pos.iter().all(|p| to_world.transform_point3(*p).y < 0.1));
    }

    /// Regression (Star Swarm on Tiny-Huge Island: a yellow ring climbing a pillar, a green spike in a poison pool):
    /// pillars, posts and one-sample spikes inside an effect are never climbed, on flat ground or on a slope.
    #[test]
    fn pillars_posts_and_one_sample_spikes_are_not_climbed() {
        let tol = LIFT + SPIKE + 1e-3;
        for base in [(|_: f32, _: f32| 0.0) as fn(f32, f32) -> f32, |x: f32, z: f32| 0.3 * x + 0.1 * z] {
            for (pillar, radius, height) in [
                // A thin post (narrower than the spacing), a pillar two samples wide, a short block just over a step.
                (0.12f32, 1.5f32, 4.0f32),
                (0.6, 2.0, 0.9),
                (0.5, 1.5, 0.4),
                (0.35, 1.0, 2.5),
            ] {
                let at = Vec2::new(0.55, 0.2);
                let floor = move |x: f32, z: f32, top: f32| {
                    let b = base(x, z);
                    let inside = Vec2::new(x, z).distance(at) < pillar;
                    if inside && b + height <= top { b + height } else { b }
                };
                let to_world = placed(Vec3::new(0.0, base(0.0, 0.0) + 0.03, 0.0), radius, 0.4);
                let g = move |x: f32, z: f32, top: f32| Some(floor(x, z, top));
                let (_, pos) = drape_positions(Shape::Disc { r: 1.0 }, Plane::XY, LIFT, to_world, &g);
                for p in &pos {
                    let w = to_world.transform_point3(*p);
                    let above = w.y - base(w.x, w.z);
                    assert!(above <= tol, "pillar r {pillar} h {height}: a vertex climbs {above} above the floor round it");
                }
            }
            // A one-sample spike exactly on a grid vertex (a post, a rock), the centre included: at most a step tall in
            // the drape, however tall it is.
            let to_world = placed(Vec3::new(0.0, base(0.0, 0.0) + 0.03, 0.0), 1.2, 0.0);
            let (grid, _) = drape_positions(Shape::Disc { r: 1.0 }, Plane::XY, LIFT, to_world, &|x, z, _| Some(base(x, z)));
            for k in [0, grid.uv.len() / 2, grid.uv.len() - 1] {
                let spot = to_world.transform_point3(local(Plane::XY, grid.uv[k]));
                for tall in [0.25f32, 0.5, 2.0] {
                    let spiky = move |x: f32, z: f32, top: f32| {
                        let b = base(x, z);
                        Some(if Vec2::new(x - spot.x, z - spot.z).length() < 1e-3 && b + tall <= top { b + tall } else { b })
                    };
                    let (_, pos) = drape_positions(Shape::Disc { r: 1.0 }, Plane::XY, LIFT, to_world, &spiky);
                    for p in &pos {
                        let w = to_world.transform_point3(*p);
                        let above = w.y - base(w.x, w.z);
                        assert!(above <= LIFT + STEP + 1e-3, "a {tall} spike at vertex {k} stands {above} above");
                    }
                }
            }
        }
    }

    #[test]
    fn outlines_are_read_from_the_flat_meshes_the_modes_use() {
        let o = |m: Mesh| outline_of(&m);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert!(matches!(o(Circle::new(1.0).into()), Some((Shape::Disc { r }, Plane::XY)) if near(r, 1.0)));
        assert!(matches!(o(Circle::new(1.0).mesh().resolution(14).build()), Some((Shape::Disc { .. }, Plane::XY))));
        assert!(matches!(o(Annulus::new(0.86, 1.0).into()), Some((Shape::Ring { inner, outer }, Plane::XY)) if near(inner, 0.86) && near(outer, 1.0)));
        assert!(matches!(o(Annulus::new(150.0 * SCALE, 185.0 * SCALE).into()), Some((Shape::Ring { inner, outer }, _)) if near(inner, 1.5) && near(outer, 1.85)));
        assert!(matches!(o(Annulus::new(0.78, 1.0).mesh().resolution(20).build()), Some((Shape::Ring { .. }, Plane::XY))));
        for (half, res) in [(1.15, 32), (1.15, 8), (0.5, 12)] {
            match o(CircularSector::new(1.0, half).mesh().resolution(res).build()) {
                Some((Shape::Sector { r, half: h, mid }, Plane::XY)) => {
                    assert!(near(r, 1.0) && near(h, half) && near(mid, std::f32::consts::FRAC_PI_2), "{h} {mid}");
                }
                other => panic!("sector {half} {res}: {other:?}"),
            }
        }
        assert!(matches!(o(Plane3d::default().mesh().size(1.0, 1.0).build()), Some((Shape::Rect { hx, hy }, Plane::XZ)) if near(hx, 0.5) && near(hy, 0.5)));
        assert!(matches!(o(Plane3d::new(Vec3::Y, Vec2::splat(0.7)).into()), Some((Shape::Rect { .. }, Plane::XZ))));
        assert!(matches!(o(Rectangle::new(2.0, 1.0).into()), Some((Shape::Rect { hx, hy }, Plane::XY)) if near(hx, 1.0) && near(hy, 0.5)));
        assert!(o(Sphere::new(1.0).mesh().ico(1).unwrap()).is_none());
        assert!(o(Cylinder::new(1.0, 1.0).into()).is_none());
    }

    #[test]
    fn a_fan_in_the_xz_plane_is_read_as_a_slice_pointing_along_it() {
        // Relic Descent's slash fan: XZ plane, centred on +Z.
        let half = 0.8f32;
        let mut pos = vec![[0.0, 0.0, 0.0]];
        for i in 0..=14 {
            let a = -half + 2.0 * half * i as f32 / 14.0;
            pos.push([a.sin(), 0.0, a.cos()]);
        }
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()).with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        let Some((Shape::Sector { half: h, mid, .. }, Plane::XZ)) = outline_of(&mesh) else { panic!() };
        assert!((h - half).abs() < 1e-3);
        let dir = local(Plane::XZ, Vec2::new(mid.cos(), mid.sin()));
        assert!(dir.distance(Vec3::Z) < 1e-3, "{dir}");
    }

    #[test]
    fn grids_stay_small_for_ios() {
        for shape in [Shape::Disc { r: 1.0 }, Shape::Ring { inner: 0.9, outer: 1.0 }, Shape::Sector { r: 1.0, half: 1.0, mid: 0.0 }] {
            let g = grid(shape, 50.0);
            assert!(g.uv.len() <= 1 + (MAX_RINGS + 1) * (MAX_SEGS + 1), "{shape:?}: {}", g.uv.len());
            let small = grid(shape, 0.4);
            assert!(small.uv.len() < 120, "{shape:?} small: {}", small.uv.len());
        }
        assert!(grid(Shape::Rect { hx: 0.5, hy: 0.5 }, 100.0).uv.len() <= (MAX_GRID + 1) * (MAX_GRID + 1));
    }

    #[test]
    fn indices_are_in_range_triangles_face_up_and_parents_lead_inward() {
        for shape in [Shape::Disc { r: 1.0 }, Shape::Ring { inner: 0.7, outer: 1.0 }, Shape::Sector { r: 1.0, half: 0.6, mid: 1.0 }, Shape::Rect { hx: 1.0, hy: 0.5 }] {
            let g = grid(shape, 3.0);
            assert_eq!(g.indices.len() % 3, 0);
            assert_eq!(g.parent.len(), g.uv.len());
            for t in g.indices.chunks(3) {
                assert!(t.iter().all(|i| (*i as usize) < g.uv.len()));
                let [a, b, c] = [g.uv[t[0] as usize], g.uv[t[1] as usize], g.uv[t[2] as usize]];
                // Counter-clockwise in the plane = facing +Z like Bevy's Circle (so culling matches).
                assert!((b - a).perp_dot(c - a) > 0.0, "{shape:?} winding");
            }
            for (i, p) in g.parent.iter().enumerate() {
                if let Some(p) = p {
                    assert!(g.uv[*p as usize].length() < g.uv[i].length(), "{shape:?}: parent of {i} is not nearer the centre");
                }
            }
        }
    }

    #[test]
    fn moving_past_the_threshold_rebuilds() {
        let a = placed(Vec3::ZERO, 1.0, 0.0);
        assert!(!moved(&a, &placed(Vec3::new(0.001, 0.0, 0.0), 1.0, 0.0)));
        assert!(moved(&a, &placed(Vec3::new(0.1, 0.0, 0.0), 1.0, 0.0)));
        assert!(moved(&a, &placed(Vec3::ZERO, 1.2, 0.0)));
        assert!(moved(&a, &placed(Vec3::ZERO, 1.0, 0.3)));
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Assets<Mesh>>().init_resource::<DrapeGround>().add_systems(Update, drape_meshes);
        app.world_mut().resource_mut::<DrapeGround>().set(|x, z, _| Some(slope(x, z)));
        app
    }

    /// The system: a tagged entity's shared flat mesh is swapped for its own draped one, an untagged one is
    /// left alone, and a re-mesh by another system (the N64 effect layer) is followed without churning assets.
    #[test]
    fn the_system_drapes_tagged_entities_and_follows_a_re_mesh() {
        let mut app = app();
        let flat = app.world_mut().resource_mut::<Assets<Mesh>>().add(Circle::new(1.0));
        let at = placed(Vec3::new(1.0, slope(1.0, 1.0) + 0.05, 1.0), 2.0, 0.0);
        let gt = GlobalTransform::from(Transform::from_matrix(at));
        let tagged = app.world_mut().spawn((Mesh3d(flat.clone()), gt, InheritedVisibility::VISIBLE, Drape::new(LIFT))).id();
        let plain = app.world_mut().spawn((Mesh3d(flat.clone()), gt, InheritedVisibility::VISIBLE)).id();
        app.update();
        let own = app.world().get::<Mesh3d>(tagged).unwrap().0.clone();
        assert_ne!(own.id(), flat.id());
        assert_eq!(app.world().get::<Mesh3d>(plain).unwrap().0.id(), flat.id());
        let heights = |app: &App, h: &Handle<Mesh>| heights_above(app.world().resource::<Assets<Mesh>>().get(h).unwrap(), at, &slope);
        assert!(heights(&app, &own).iter().all(|d| (d - LIFT).abs() < 1e-3));
        assert!(app.world().entity(tagged).contains::<NoFrustumCulling>());
        let d = app.world().get::<Drape>(tagged).unwrap();
        assert!(Drape::shows(Some(d), &own, &flat), "a mesh swapper sees the source as already shown");

        // Another system swaps in a ring: the drape reads the new outline and reuses its own mesh asset.
        let ring = app.world_mut().resource_mut::<Assets<Mesh>>().add(Annulus::new(0.78, 1.0).mesh().resolution(20).build());
        let count = app.world().resource::<Assets<Mesh>>().len();
        app.world_mut().get_mut::<Mesh3d>(tagged).unwrap().0 = ring.clone();
        app.update();
        assert_eq!(app.world().get::<Mesh3d>(tagged).unwrap().0.id(), own.id());
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), count);
        assert!(matches!(app.world().get::<Drape>(tagged).unwrap().outline(), Some((Shape::Ring { .. }, Plane::XY))));
        assert!(heights(&app, &own).iter().all(|d| (d - LIFT).abs() < 1e-3));

        // Standing still: nothing is rebuilt.
        app.world_mut().resource_mut::<Assets<Mesh>>().get_mut(&own).unwrap().insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; 1]);
        app.update();
        assert!(app.world().resource::<Assets<Mesh>>().get(&own).unwrap().attribute(Mesh::ATTRIBUTE_COLOR).is_some());
    }

    /// Every minigame with ground effects tags them with `Drape` and hands its floor to `DrapeGround` (and the N64
    /// effect layers, which re-mesh pooled rings, compare against the drape's source so they do not fight it).
    #[test]
    fn every_mode_drapes_its_ground_effects_and_sets_its_floor() {
        let modes: [(&str, &[(&str, &str, usize)], &str); 9] = [
            // (mode, [(view file, what it drapes, at least this many tags)], file that sets the floor)
            ("Gem Clash", &[("brawl_view.rs", include_str!("../experiments/brawl_view.rs"), 4), ("brawl_hazard_view.rs", include_str!("../experiments/brawl_hazard_view.rs"), 1)], include_str!("../experiments/brawl.rs")),
            ("Star Swarm", &[("swarm_view.rs", include_str!("../experiments/swarm_view.rs"), 2), ("swarm_shrine_view.rs", include_str!("../experiments/swarm_shrine_view.rs"), 2)], include_str!("../experiments/swarm.rs")),
            ("Hold the Gate", &[("gate_view.rs", include_str!("../experiments/gate_view.rs"), 5)], include_str!("../experiments/gate.rs")),
            ("Last Star Standing", &[("lss_view.rs", include_str!("../experiments/lss_view.rs"), 1)], include_str!("../experiments/lss.rs")),
            ("Relic Descent", &[("relic_view.rs", include_str!("../experiments/relic_view.rs"), 4)], include_str!("../experiments/relic.rs")),
            ("Relic Descent floor circles", &[("relic.rs", include_str!("../experiments/relic.rs"), 3)], include_str!("../experiments/relic.rs")),
            ("Painted Halls", &[("halls_view.rs", include_str!("../experiments/halls_view.rs"), 2)], include_str!("../experiments/halls.rs")),
            ("Star Deck", &[("deck.rs", include_str!("../experiments/deck.rs"), 1)], include_str!("../experiments/deck.rs")),
            ("N64 effect layers", &[("brawl_fx.rs", include_str!("../experiments/brawl_fx.rs"), 0), ("swarm_fx.rs", include_str!("../experiments/swarm_fx.rs"), 0)], include_str!("../experiments/brawl.rs")),
        ];
        for (mode, views, setter) in modes {
            for (file, src, n) in views {
                let tags = src.matches("Drape::new(").count();
                assert!(tags >= *n, "{mode}: {file} drapes {tags} effects, expected at least {n}");
                if *n == 0 {
                    assert!(src.contains("Drape::shows("), "{mode}: {file} re-meshes pooled effects without checking the drape's source");
                }
            }
            assert!(setter.contains("host.drape.set"), "{mode}: never sets the floor its effects drape over");
        }
        // The host clears the floor when a mode exits.
        assert!(include_str!("../experiments/mod.rs").contains("p.drape.clear()"));
    }

    #[test]
    fn non_flat_meshes_and_hidden_entities_are_left_alone() {
        let mut app = app();
        let ball = app.world_mut().resource_mut::<Assets<Mesh>>().add(Sphere::new(1.0).mesh().ico(1).unwrap());
        let flat = app.world_mut().resource_mut::<Assets<Mesh>>().add(Circle::new(1.0));
        let gt = GlobalTransform::from(Transform::from_matrix(placed(Vec3::new(0.0, 2.0, 0.0), 1.0, 0.0)));
        let a = app.world_mut().spawn((Mesh3d(ball.clone()), gt, InheritedVisibility::VISIBLE, Drape::new(LIFT))).id();
        let b = app.world_mut().spawn((Mesh3d(flat.clone()), gt, InheritedVisibility::HIDDEN, Drape::new(LIFT))).id();
        app.update();
        app.update();
        assert_eq!(app.world().get::<Mesh3d>(a).unwrap().0.id(), ball.id());
        assert_eq!(app.world().get::<Mesh3d>(b).unwrap().0.id(), flat.id());
    }
}
