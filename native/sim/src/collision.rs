//! Triangle collision world, ported from src/engine/surface_collision.c via
//! src/sim/collision.ts. Floors sort highest-first, ceilings lowest-first and
//! walls keep insertion order; "first match wins" is load-bearing.
use crate::math::Vec3;
use crate::surfaces::*;
use std::collections::HashMap;
use std::sync::Arc;

pub const LEVEL_BOUNDARY_MAX: f64 = 8192.0;
pub const CELL_SIZE: f64 = 1024.0;
pub const CELL_HEIGHT_LIMIT: f64 = 20000.0;
pub const FLOOR_LOWER_LIMIT: f64 = -11000.0;
const MAX_WALLS: usize = 4;

#[derive(Clone, Debug, Default)]
pub struct SurfaceOptions {
    pub room: Option<i32>,
    pub kind: i32,
    pub force: i32,
    pub flags: i32,
    /// Index of the owning dynamic object, if any.
    pub object: Option<u32>,
}

#[derive(Debug)]
pub struct Surface {
    pub object: Option<u32>,
    pub room: Option<i32>,
    pub kind: i32,
    pub force: i32,
    pub flags: i32,
    pub lower_y: f64,
    pub upper_y: f64,
    pub v1: Vec3,
    pub v2: Vec3,
    pub v3: Vec3,
    pub normal: Vec3,
    pub origin_offset: f64,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub height: f64,
    pub surface: Option<Arc<Surface>>,
}

#[derive(Default)]
pub struct WallData {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub offset_y: f64,
    pub radius: f64,
    pub walls: Vec<Arc<Surface>>,
}

#[derive(Default)]
struct Cell {
    floors: Vec<Arc<Surface>>,
    ceils: Vec<Arc<Surface>>,
    walls: Vec<Arc<Surface>>,
}

pub struct WaterBox(pub f64, pub f64, pub f64, pub f64, pub f64, pub f64);

pub struct CollisionWorld {
    pub cell_size: f64,
    pub horizontal_boundary: f64,
    pub water_boxes: Vec<WaterBox>,
    pub slide_terrain: bool,
    pub snow_terrain: bool,
    pub ceiling_limit: f64,
    cells: HashMap<(i32, i32), Cell>,
    all: Vec<Arc<Surface>>,
}

impl Default for CollisionWorld {
    fn default() -> Self { Self::new(CELL_SIZE, LEVEL_BOUNDARY_MAX) }
}

fn cell_index(v: f64, size: f64) -> i32 { (v / size).floor() as i32 }

impl CollisionWorld {
    pub fn new(cell_size: f64, horizontal_boundary: f64) -> Self {
        Self {
            cell_size,
            horizontal_boundary,
            water_boxes: vec![],
            slide_terrain: false,
            snow_terrain: false,
            ceiling_limit: CELL_HEIGHT_LIMIT,
            cells: HashMap::new(),
            all: vec![],
        }
    }

    pub fn surfaces(&self) -> &[Arc<Surface>] { &self.all }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.all.clear();
    }

    pub fn water_level(&self, x: f64, z: f64) -> f64 {
        for WaterBox(id, x1, z1, x2, z2, y) in &self.water_boxes {
            if *id < 50.0 && x > *x1 && x < *x2 && z > *z1 && z < *z2 {
                return *y;
            }
        }
        f64::NEG_INFINITY
    }

    pub fn remove_surfaces(&mut self, removed: &dyn Fn(&Surface) -> bool) {
        for cell in self.cells.values_mut() {
            cell.floors.retain(|s| !removed(s));
            cell.walls.retain(|s| !removed(s));
            cell.ceils.retain(|s| !removed(s));
        }
        self.all.retain(|s| !removed(s));
    }

    /// Add a triangle using the decompilation's normal: n = (v2 - v1) x (v3 - v2).
    pub fn add_triangle(&mut self, v1: Vec3, v2: Vec3, v3: Vec3, options: &SurfaceOptions) -> Option<Arc<Surface>> {
        let [x1, y1, z1] = v1;
        let [x2, y2, z2] = v2;
        let [x3, y3, z3] = v3;
        let mut nx = (y2 - y1) * (z3 - z2) - (z2 - z1) * (y3 - y2);
        let mut ny = (z2 - z1) * (x3 - x2) - (x2 - x1) * (z3 - z2);
        let mut nz = (x2 - x1) * (y3 - y2) - (y2 - y1) * (x3 - x2);
        let mag = (nx * nx + ny * ny + nz * nz).sqrt();
        if mag < 0.0001 {
            return None;
        }
        let inv = 1.0 / mag;
        nx *= inv;
        ny *= inv;
        nz *= inv;

        let mut flags = options.flags;
        let list = if ny > 0.01 {
            0
        } else if ny < -0.01 {
            1
        } else {
            if nx < -0.707 || nx > 0.707 {
                flags |= SURFACE_FLAG_X_PROJECTION;
            }
            2
        };
        let surface = Arc::new(Surface {
            object: options.object,
            room: options.room,
            kind: options.kind,
            force: options.force,
            flags,
            lower_y: y1.min(y2).min(y3) - 5.0,
            upper_y: y1.max(y2).max(y3) + 5.0,
            v1,
            v2,
            v3,
            normal: [nx, ny, nz],
            origin_offset: -(nx * x1 + ny * y1 + nz * z1),
        });

        // Cell range grows by 50 units so wall radii straddling a boundary still hit.
        const PAD: f64 = 50.0;
        let size = self.cell_size;
        let min_x = cell_index(x1.min(x2).min(x3) - PAD, size);
        let max_x = cell_index(x1.max(x2).max(x3) + PAD, size);
        let min_z = cell_index(z1.min(z2).min(z3) - PAD, size);
        let max_z = cell_index(z1.max(z2).max(z3) + PAD, size);
        for cz in min_z..=max_z {
            for cx in min_x..=max_x {
                let cell = self.cells.entry((cx, cz)).or_default();
                match list {
                    // Stable insertion equals SM64's sort: highest first, ties keep order.
                    0 => {
                        let at = cell.floors.partition_point(|s| s.v1[1] >= v1[1]);
                        cell.floors.insert(at, surface.clone());
                    }
                    1 => {
                        let at = cell.ceils.partition_point(|s| s.v1[1] <= v1[1]);
                        cell.ceils.insert(at, surface.clone());
                    }
                    _ => cell.walls.push(surface.clone()),
                }
            }
        }
        self.all.push(surface.clone());
        Some(surface)
    }

    pub fn add_quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, options: &SurfaceOptions) -> Vec<Arc<Surface>> {
        [self.add_triangle(a, b, c, options), self.add_triangle(a, c, d, options)].into_iter().flatten().collect()
    }

    /// Kept for API parity: cells are always sorted on insertion.
    pub fn finalize(&mut self) {}

    fn cell_at(&self, x: f64, z: f64) -> Option<&Cell> {
        self.cells.get(&(cell_index(x, self.cell_size), cell_index(z, self.cell_size)))
    }

    fn skip(surf: &Surface, for_camera: bool) -> bool {
        if for_camera { surf.flags & SURFACE_FLAG_NO_CAM_COLLISION != 0 } else { surf.kind == SURFACE_CAMERA_BOUNDARY }
    }

    pub fn find_floor(&self, x: f64, y: f64, z: f64) -> Hit {
        self.find_floor_ex(x, y, z, false, false)
    }

    /// find_floor, including its 78-unit buffer and truncated integer triangle test.
    pub fn find_floor_ex(&self, x: f64, y: f64, z: f64, include_intangible: bool, for_camera: bool) -> Hit {
        let (qx, qy, qz) = (x.trunc(), y.trunc(), z.trunc());
        let none = Hit { height: FLOOR_LOWER_LIMIT, surface: None };
        let Some(cell) = self.cell_at(qx, qz) else { return none };
        for surf in &cell.floors {
            if Self::skip(surf, for_camera) {
                continue;
            }
            let (x1, z1) = (surf.v1[0].trunc(), surf.v1[2].trunc());
            let (x2, z2) = (surf.v2[0].trunc(), surf.v2[2].trunc());
            if (z1 - qz) * (x2 - x1) - (x1 - qx) * (z2 - z1) < 0.0 { continue; }
            let (x3, z3) = (surf.v3[0].trunc(), surf.v3[2].trunc());
            if (z2 - qz) * (x3 - x2) - (x2 - qx) * (z3 - z2) < 0.0 { continue; }
            if (z3 - qz) * (x1 - x3) - (x3 - qx) * (z1 - z3) < 0.0 { continue; }
            let [nx, ny, nz] = surf.normal;
            if ny == 0.0 { continue; }
            let height = -(qx * nx + nz * qz + surf.origin_offset) / ny;
            if qy - (height - 78.0) < 0.0 { continue; }
            if !include_intangible && surf.kind == 0x12 {
                return self.find_floor_ex(x, height - 200.0, z, true, for_camera);
            }
            return Hit { height, surface: Some(surf.clone()) };
        }
        none
    }

    pub fn find_floor_height(&self, x: f64, y: f64, z: f64) -> f64 { self.find_floor(x, y, z).height }

    /// find_ceil: first ceiling at least 78 units above the point.
    pub fn find_ceil(&self, x: f64, y: f64, z: f64) -> Hit {
        let (qx, qy, qz) = (x.trunc(), y.trunc(), z.trunc());
        let none = Hit { height: self.ceiling_limit, surface: None };
        let Some(cell) = self.cell_at(qx, qz) else { return none };
        for surf in &cell.ceils {
            if Self::skip(surf, false) { continue; }
            let (x1, z1) = (surf.v1[0].trunc(), surf.v1[2].trunc());
            let (x2, z2) = (surf.v2[0].trunc(), surf.v2[2].trunc());
            if (z1 - qz) * (x2 - x1) - (x1 - qx) * (z2 - z1) > 0.0 { continue; }
            let (x3, z3) = (surf.v3[0].trunc(), surf.v3[2].trunc());
            if (z2 - qz) * (x3 - x2) - (x2 - qx) * (z3 - z2) > 0.0 { continue; }
            if (z3 - qz) * (x1 - x3) - (x3 - qx) * (z1 - z3) > 0.0 { continue; }
            let [nx, ny, nz] = surf.normal;
            if ny == 0.0 { continue; }
            let height = -(qx * nx + nz * qz + surf.origin_offset) / ny;
            if qy - (height + 78.0) > 0.0 { continue; }
            return Hit { height, surface: Some(surf.clone()) };
        }
        none
    }

    /// find_wall_collisions: push a cylinder out of every wall it touches. Returns hit count.
    pub fn find_wall_collisions(&self, data: &mut WallData, for_camera: bool) -> usize {
        data.walls.clear();
        let radius = data.radius.min(200.0);
        let Some(cell) = self.cell_at(data.x, data.z) else { return 0 };
        let y = data.y + data.offset_y;
        let mut cols = 0;
        for surf in &cell.walls {
            if y < surf.lower_y || y > surf.upper_y { continue; }
            let offset = surf.normal[0] * data.x + surf.normal[1] * y + surf.normal[2] * data.z + surf.origin_offset;
            if offset < -radius || offset > radius { continue; }
            if Self::skip(surf, for_camera) || (!for_camera && surf.kind == SURFACE_VANISH_CAP_WALLS) { continue; }

            let (px, pz) = (data.x, data.z);
            let (w1, w2, w3, y1, y2, y3, p, positive);
            if surf.flags & SURFACE_FLAG_X_PROJECTION != 0 {
                w1 = -surf.v1[2]; w2 = -surf.v2[2]; w3 = -surf.v3[2];
                p = -pz;
                positive = surf.normal[0] > 0.0;
            } else {
                w1 = surf.v1[0]; w2 = surf.v2[0]; w3 = surf.v3[0];
                p = px;
                positive = surf.normal[2] > 0.0;
            }
            y1 = surf.v1[1]; y2 = surf.v2[1]; y3 = surf.v3[1];
            let e1 = (y1 - y) * (w2 - w1) - (w1 - p) * (y2 - y1);
            let e2 = (y2 - y) * (w3 - w2) - (w2 - p) * (y3 - y2);
            let e3 = (y3 - y) * (w1 - w3) - (w3 - p) * (y1 - y3);
            if positive {
                if e1 > 0.0 || e2 > 0.0 || e3 > 0.0 { continue; }
            } else if e1 < 0.0 || e2 < 0.0 || e3 < 0.0 {
                continue;
            }

            data.x += surf.normal[0] * (radius - offset);
            data.z += surf.normal[2] * (radius - offset);
            if data.walls.len() < MAX_WALLS {
                data.walls.push(surf.clone());
            }
            cols += 1;
        }
        cols
    }

    pub fn f32_find_wall_collision(&self, pos: &mut Vec3, offset_y: f64, radius: f64) -> usize {
        let mut d = WallData { x: pos[0], y: pos[1], z: pos[2], offset_y, radius, walls: vec![] };
        let n = self.find_wall_collisions(&mut d, false);
        *pos = [d.x, d.y, d.z];
        n
    }

    /// Wall push that also returns the most recent wall.
    pub fn resolve_and_return_wall_collisions(&self, pos: &mut Vec3, offset_y: f64, radius: f64) -> Option<Arc<Surface>> {
        let mut d = WallData { x: pos[0], y: pos[1], z: pos[2], offset_y, radius, walls: vec![] };
        let wall = if self.find_wall_collisions(&mut d, false) > 0 { d.walls.last().cloned() } else { None };
        *pos = [d.x, d.y, d.z];
        wall
    }

    pub fn vec3f_find_ceil(&self, pos: &Vec3, height: f64) -> Hit {
        self.find_ceil(pos[0], height + 80.0, pos[2])
    }
}

impl CollisionWorld {
    /// Virtual floor used when a shell rider skims the water surface (very slippery, flat).
    pub fn shell_water_floor(&self, height: f64) -> Arc<Surface> {
        Arc::new(Surface {
            object: None, room: None, kind: SURFACE_VERY_SLIPPERY, force: 0, flags: 0, lower_y: 0.0, upper_y: 0.0,
            v1: [0.0; 3], v2: [0.0; 3], v3: [0.0; 3], normal: [0.0, 1.0, 0.0], origin_offset: height,
        })
    }
}
