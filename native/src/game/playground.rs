//! Port of `playgroundLayout.ts` / `sm64Playground.ts`: the hand-built movement playground.
//! The layout is expressed as [`Piece`]s that feed both the collision world and the renderer.
use super::world::{CourseRequest, CourseVisual, LoadedCourse, Prop};
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::surfaces::*;
use std::sync::Arc;

pub const PLAYGROUND_SIZE: f64 = 12000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Material {
    Grass,
    Stone,
    Wood,
    Ice,
}

#[derive(Clone, Copy, Debug)]
pub enum Shape {
    /// Axis-aligned box standing on y=0: (x, z, width, height, depth).
    Box { x: f64, z: f64, w: f64, h: f64, d: f64 },
    /// Wedge rising toward +Z: (x, z, width, length, height).
    Ramp { x: f64, z: f64, w: f64, l: f64, h: f64 },
    /// Axis-aligned box with its top at `top` (floating blocks, ground slabs): (x, z, width, top, height, depth).
    Slab { x: f64, z: f64, w: f64, top: f64, h: f64, d: f64 },
    /// Octagonal collision-only trunk.
    Trunk { x: f64, z: f64 },
    Ground,
}

#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub shape: Shape,
    pub material: Material,
    pub surface: i32,
    /// Collision only (no mesh).
    pub hidden: bool,
}

pub fn zone(x: f64, z: f64) -> &'static str {
    if x > 1800.0 && z < -1800.0 {
        "STAR STAIRCASE"
    } else if x < -1800.0 && z < 0.0 {
        "WALL KICK CANYON"
    } else if x > 1800.0 && z >= -1800.0 && z < 1800.0 {
        "SLIDE RINK"
    } else if z < -2800.0 && x.abs() < 1300.0 {
        "LONG JUMP GAP"
    } else if z < -800.0 && x.abs() < 1300.0 {
        "JUMP GARDEN"
    } else if z > 2000.0 && x < -1600.0 {
        "STAIR & PYRAMID"
    } else if z > 2000.0 && x > -1000.0 {
        "SLOPE LAB"
    } else {
        "OPEN MEADOW"
    }
}

#[derive(Default)]
pub struct Layout {
    pub pieces: Vec<Piece>,
    pub coins: Vec<[f64; 3]>,
    pub trees: Vec<[f64; 2]>,
    pub star: [f64; 3],
}

/// `buildNativePlayground`'s layout, in the same order as the TS.
pub fn layout() -> Layout {
    use Material::*;
    let mut l = Layout::default();
    let mut obstacles: Vec<[f64; 4]> = vec![]; // minx, maxx, minz, maxz
    let mut add = |l: &mut Layout, shape: Shape, material: Material, surface: i32, hidden: bool, bounds: Option<[f64; 4]>| {
        l.pieces.push(Piece { shape, material, surface, hidden });
        if let Some(b) = bounds {
            obstacles.push(b);
        }
    };
    let bx = |x: f64, z: f64, w: f64, d: f64| [x - w / 2.0, x + w / 2.0, z - d / 2.0, z + d / 2.0];
    macro_rules! boxp {
        ($l:expr, $x:expr, $z:expr, $w:expr, $h:expr, $d:expr, $m:expr, $s:expr) => {
            add($l, Shape::Box { x: $x, z: $z, w: $w, h: $h, d: $d }, $m, $s, false, Some(bx($x, $z, $w, $d)))
        };
    }
    add(&mut l, Shape::Ground, Grass, 0, false, None);
    for (i, size) in [100.0, 160.0, 220.0, 280.0].into_iter().enumerate() {
        let z = -1000.0 - i as f64 * 450.0;
        boxp!(&mut l, 0.0, z, size, size, size, Wood, 0);
        l.coins.push([0.0, size + 70.0, z]);
    }
    boxp!(&mut l, 0.0, -3400.0, 700.0, 180.0, 900.0, Wood, 0);
    boxp!(&mut l, 0.0, -4500.0, 700.0, 180.0, 600.0, Wood, 0);
    l.coins.push([0.0, 250.0, -4500.0]);
    for (i, h) in [60.0, 120.0, 180.0].into_iter().enumerate() {
        boxp!(&mut l, -280.0 + i as f64 * 140.0, -2880.0, 140.0, h, 140.0, Stone, 0);
    }
    boxp!(&mut l, -3350.0, -1100.0, 180.0, 600.0, 1400.0, Stone, 0);
    boxp!(&mut l, -2870.0, -1100.0, 180.0, 600.0, 1400.0, Stone, 0);
    l.coins.push([-3110.0, 100.0, -650.0]);
    l.coins.push([-3110.0, 420.0, -1450.0]);
    add(&mut l, Shape::Ramp { x: 3100.0, z: 300.0, w: 900.0, l: 1600.0, h: 480.0 }, Ice, SURFACE_ICE, false, Some(bx(3100.0, 300.0, 900.0, 1600.0)));
    boxp!(&mut l, 3100.0, 1400.0, 900.0, 480.0, 600.0, Stone, 0);
    boxp!(&mut l, 3100.0, -1300.0, 900.0, 8.0, 1600.0, Ice, SURFACE_ICE);
    l.coins.push([3100.0, 550.0, 1400.0]);
    l.coins.push([3100.0, 78.0, -1700.0]);
    for i in 0..12 {
        boxp!(&mut l, 2900.0, -2200.0 - i as f64 * 150.0, 500.0, (i + 1) as f64 * 60.0, 150.0, Stone, 0);
    }
    boxp!(&mut l, 2900.0, -4200.0, 800.0, 720.0, 550.0, Stone, 0);
    l.coins.push([2900.0, 430.0, -2950.0]);
    l.star = [2900.0, 820.0, -4200.0];
    for i in 0..8 {
        boxp!(&mut l, -3100.0, 2200.0 + i as f64 * 180.0, 700.0, (i + 1) as f64 * 50.0, 180.0, Stone, 0);
    }
    boxp!(&mut l, -3100.0, 3980.0, 1100.0, 400.0, 860.0, Stone, 0);
    l.coins.push([-3100.0, 470.0, 3980.0]);
    let surfaces = [SURFACE_NOT_SLIPPERY, SURFACE_SLIPPERY, SURFACE_VERY_SLIPPERY, SURFACE_ICE];
    for (i, degrees) in [20.0f64, 30.0, 45.0, 60.0].into_iter().enumerate() {
        let x = -300.0 + i as f64 * 850.0;
        let height = 600.0 * degrees.to_radians().tan();
        add(
            &mut l,
            Shape::Ramp { x, z: 3200.0, w: 480.0, l: 600.0, h: height },
            if i == 3 { Ice } else { Stone },
            surfaces[i],
            false,
            Some(bx(x, 3200.0, 480.0, 600.0)),
        );
        l.coins.push([x, height + 70.0, 3480.0]);
    }
    // Deterministic peripheral groves.
    let mut trees: Vec<[f64; 2]> = vec![];
    for row in -5i32..=5 {
        for col in -5i32..=5 {
            if row * row + col * col < 14 || (row + col) % 3 != 0 {
                continue;
            }
            let x = col as f64 * 1000.0 + (row % 2) as f64 * 160.0;
            let z = row as f64 * 1000.0;
            if obstacles.iter().any(|b| x >= b[0] - 450.0 && x <= b[1] + 450.0 && z >= b[2] - 450.0 && z <= b[3] + 450.0) {
                continue;
            }
            if trees.iter().any(|p| ((p[0] - x).powi(2) + (p[1] - z).powi(2)).sqrt() < 650.0) {
                continue;
            }
            trees.push([x, z]);
            l.pieces.push(Piece { shape: Shape::Trunk { x, z }, material: Wood, surface: 0, hidden: true });
        }
    }
    l.trees = trees;
    l
}

fn add_oriented_quad(world: &mut CollisionWorld, q: [[f64; 3]; 4], inside: [f64; 3], opts: &SurfaceOptions) {
    // Choose the winding whose normal points away from `inside`.
    let n = {
        let (a, b, c) = (q[0], q[1], q[2]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
        [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
    };
    let d = [q[0][0] - inside[0], q[0][1] - inside[1], q[0][2] - inside[2]];
    if n[0] * d[0] + n[1] * d[1] + n[2] * d[2] >= 0.0 {
        world.add_quad(q[0], q[1], q[2], q[3], opts);
    } else {
        world.add_quad(q[3], q[2], q[1], q[0], opts);
    }
}

pub fn add_piece_collision(world: &mut CollisionWorld, p: &Piece) {
    let opts = SurfaceOptions { kind: p.surface, ..Default::default() };
    match p.shape {
        Shape::Ground => {
            let r = PLAYGROUND_SIZE / 2.0;
            world.add_triangle([-r, 0.0, -r], [-r, 0.0, r], [r, 0.0, r], &opts);
            world.add_triangle([-r, 0.0, -r], [r, 0.0, r], [r, 0.0, -r], &opts);
        }
        Shape::Box { x, z, w, h, d } => add_prism(world, &[[x - w / 2.0, z - d / 2.0], [x + w / 2.0, z - d / 2.0], [x + w / 2.0, z + d / 2.0], [x - w / 2.0, z + d / 2.0]], 0.0, h, &opts),
        Shape::Slab { x, z, w, top, h, d } => add_prism(world, &[[x - w / 2.0, z - d / 2.0], [x + w / 2.0, z - d / 2.0], [x + w / 2.0, z + d / 2.0], [x - w / 2.0, z + d / 2.0]], top - h, top, &opts),
        Shape::Trunk { x, z } => {
            let ring: Vec<[f64; 2]> = (0..8)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 8.0;
                    [x + 14.0 * a.sin(), z + 14.0 * a.cos()]
                })
                .collect();
            add_prism(world, &ring, 0.0, 160.0, &opts);
        }
        Shape::Ramp { x, z, w, l, h } => {
            let v = [
                [x - w / 2.0, 0.0, z - l / 2.0],
                [x + w / 2.0, 0.0, z - l / 2.0],
                [x - w / 2.0, 0.0, z + l / 2.0],
                [x + w / 2.0, 0.0, z + l / 2.0],
                [x - w / 2.0, h, z + l / 2.0],
                [x + w / 2.0, h, z + l / 2.0],
            ];
            for t in [[0, 4, 5], [0, 5, 1], [2, 3, 5], [2, 5, 4], [0, 2, 4], [1, 5, 3], [0, 1, 3], [0, 3, 2]] {
                world.add_triangle(v[t[0]], v[t[1]], v[t[2]], &opts);
            }
        }
    }
}

/// Closed prism over a convex footprint, every face wound outward.
fn add_prism(world: &mut CollisionWorld, ring: &[[f64; 2]], y0: f64, y1: f64, opts: &SurfaceOptions) {
    let n = ring.len() as f64;
    let cx = ring.iter().map(|p| p[0]).sum::<f64>() / n;
    let cz = ring.iter().map(|p| p[1]).sum::<f64>() / n;
    let inside = [cx, (y0 + y1) / 2.0, cz];
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        add_oriented_quad(world, [[a[0], y0, a[1]], [b[0], y0, b[1]], [b[0], y1, b[1]], [a[0], y1, a[1]]], inside, opts);
        // Top / bottom fans.
        let c = [cx, cz];
        add_oriented_quad(world, [[a[0], y1, a[1]], [b[0], y1, b[1]], [c[0], y1, c[1]], [c[0], y1, c[1]]], inside, opts);
        add_oriented_quad(world, [[a[0], y0, a[1]], [b[0], y0, b[1]], [c[0], y0, c[1]], [c[0], y0, c[1]]], inside, opts);
    }
}

pub fn load(request: CourseRequest) -> LoadedCourse {
    let l = layout();
    let mut world = CollisionWorld::default();
    // Ground goes in last: floors with equal v1.y keep insertion order, and ramps must win ties
    // against the ground plane (the TS relies on float noise in its rotated plane instead).
    for p in l.pieces.iter().filter(|p| !matches!(p.shape, Shape::Ground)) {
        add_piece_collision(&mut world, p);
    }
    for p in l.pieces.iter().filter(|p| matches!(p.shape, Shape::Ground)) {
        add_piece_collision(&mut world, p);
    }
    let mut props: Vec<Prop> = l
        .trees
        .iter()
        .map(|t| Prop { model: "actors/tree/bubbly_tree_geo.glb".into(), pos: [t[0], 0.0, t[1]], yaw: 0.0, rotation: None, scale: 0.65, billboard: true, scroll_t: 0.0, spin_z: 0.0 })
        .collect();
    props.push(Prop { model: "actors/star/star_geo.glb".into(), pos: l.star, yaw: 0.0, rotation: None, scale: 0.5, billboard: false, scroll_t: 0.0, spin_z: 0.0 });
    LoadedCourse {
        request,
        name: "Movement Playground".into(),
        visual: CourseVisual::Playground(l.pieces.clone()),
        world: Arc::new(world),
        spawn: SpawnPoint { action: None, pos: [0.0, 10.0, 500.0], face_yaw: 0x8000 as f64 },
        death_height: -1600.0,
        sky: None,
        objects: ObjectManager::new(vec![], None, None),
        props,
        coins: l.coins,
        red_coins: vec![],
        doors: vec![],
        warp_spawns: Default::default(),
        fading_warps: vec![],
        death_warp: None,
        fall_warp: None,
        success_entry: None,
        success_area: None,
        skipped_behaviors: vec![],
        extras: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playground_has_floor_and_ramp() {
        let c = load(CourseRequest::new("playground"));
        let spawn = c.world.find_floor(0.0, 300.0, 500.0);
        assert!(spawn.height.abs() < 1e-9);
        // Top of the first crate (100 units at z=-1000).
        let crate_top = c.world.find_floor(0.0, 300.0, -1000.0);
        assert!((crate_top.height - 100.0).abs() < 1e-9);
        // Middle of the 45 degree ramp rises toward +Z.
        let ramp = c.world.find_floor(-300.0 + 2.0 * 850.0, 2000.0, 3200.0 + 299.0);
        assert!(ramp.height > 500.0 && ramp.height < 650.0, "{}", ramp.height);
        assert!(!c.coins.is_empty());
    }
}
