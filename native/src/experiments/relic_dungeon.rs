//! Relic Descent floor generator: a lattice of rooms joined by three-tile corridors, on a tile
//! grid. Engine-free and deterministic in `(seed, floor)`.
//!
//! Layout: the floor is `COLS x ROWS` lattice cells of `CELL` tiles. A random walk picks 6-8
//! cells, each gets a room, a spanning tree plus some extra edges join them with L-shaped
//! corridors. The room farthest (in links) from the start holds the stairs, or the boss on the
//! last floor; a dead-end room holds the treasure.
use super::swarm_core::Rng;

pub const TILE: f32 = 220.0;
pub const CELL: usize = 15;
pub const COLS: usize = 4;
pub const ROWS: usize = 3;
pub const W: usize = COLS * CELL;
pub const H: usize = ROWS * CELL;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Tile {
    Void = 0,
    Corridor = 1,
    Room = 2,
    BossRoom = 3,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoomKind {
    Start,
    Combat,
    Treasure,
    Stairs,
    Boss,
}

#[derive(Clone, Debug)]
pub struct Room {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub kind: RoomKind,
    pub cell: (usize, usize),
    pub links: Vec<usize>,
}

impl Room {
    /// Centre in world units (floor-plane x, z).
    pub fn center(&self) -> [f32; 2] {
        [(self.x as f32 + self.w as f32 / 2.0) * TILE, (self.y as f32 + self.h as f32 / 2.0) * TILE]
    }
    /// Half extents in world units.
    pub fn half(&self) -> [f32; 2] {
        [self.w as f32 * TILE / 2.0, self.h as f32 * TILE / 2.0]
    }
    pub fn contains(&self, p: [f32; 2], margin: f32) -> bool {
        let (c, h) = (self.center(), self.half());
        (p[0] - c[0]).abs() <= h[0] + margin && (p[1] - c[1]).abs() <= h[1] + margin
    }
}

#[derive(Clone, Debug)]
pub struct Dungeon {
    pub tiles: Vec<Tile>,
    pub rooms: Vec<Room>,
    pub start: usize,
    /// The stairs room (or the boss room on the last floor).
    pub exit: usize,
    pub boss_floor: bool,
}

/// Axis-aligned wall box in tile units.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WallBox {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Dungeon {
    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
            Tile::Void
        } else {
            self.tiles[y as usize * W + x as usize]
        }
    }
    pub fn is_floor_tile(&self, x: i32, y: i32) -> bool {
        self.tile(x, y) != Tile::Void
    }
    /// World position (x, z) -> tile coordinates.
    pub fn tile_of(p: [f32; 2]) -> (i32, i32) {
        ((p[0] / TILE).floor() as i32, (p[1] / TILE).floor() as i32)
    }
    pub fn walkable(&self, p: [f32; 2]) -> bool {
        let (x, y) = Self::tile_of(p);
        self.is_floor_tile(x, y)
    }
    /// A circle fits when its four extreme points are on floor.
    pub fn circle_free(&self, p: [f32; 2], r: f32) -> bool {
        self.walkable([p[0] - r, p[1] - r]) && self.walkable([p[0] + r, p[1] - r]) && self.walkable([p[0] - r, p[1] + r]) && self.walkable([p[0] + r, p[1] + r])
    }
    /// Move a circle by `d` sliding along walls.
    pub fn slide(&self, p: [f32; 2], d: [f32; 2], r: f32) -> [f32; 2] {
        let mut q = p;
        if self.circle_free([p[0] + d[0], p[1]], r) {
            q[0] += d[0];
        }
        if self.circle_free([q[0], p[1] + d[1]], r) {
            q[1] += d[1];
        }
        q
    }
    pub fn room_at(&self, p: [f32; 2]) -> Option<usize> {
        self.rooms.iter().position(|r| r.contains(p, 0.0))
    }
    /// Boxes (in tile units) covering every void tile touching a floor tile, merged into
    /// rectangles so the collision world and the wall mesh stay small.
    pub fn wall_boxes(&self) -> Vec<WallBox> {
        let wall = |x: i32, y: i32| -> bool {
            if self.is_floor_tile(x, y) {
                return false;
            }
            (-1..=1).any(|dy| (-1..=1).any(|dx| self.is_floor_tile(x + dx, y + dy)))
        };
        // Horizontal runs per row.
        let mut runs: Vec<(usize, usize, usize)> = vec![]; // (y, x0, len)
        for y in -1..=(H as i32) {
            let mut x = -1;
            while x <= W as i32 {
                if wall(x, y) {
                    let x0 = x;
                    while x <= W as i32 && wall(x, y) {
                        x += 1;
                    }
                    runs.push(((y + 1) as usize, (x0 + 1) as usize, (x - x0) as usize));
                } else {
                    x += 1;
                }
            }
        }
        // Stack equal runs of consecutive rows (coordinates are shifted by +1 so they stay unsigned).
    let mut out: Vec<WallBox> = vec![];
        let mut open: Vec<WallBox> = vec![];
        for y in 0..=H + 1 {
            let this: Vec<(usize, usize)> = runs.iter().filter(|r| r.0 == y).map(|r| (r.1, r.2)).collect();
            let mut next_open = vec![];
            for (x0, len) in &this {
                if let Some(i) = open.iter().position(|b| b.x == *x0 && b.w == *len && b.y + b.h == y) {
                    let mut b = open.remove(i);
                    b.h += 1;
                    next_open.push(b);
                } else {
                    next_open.push(WallBox { x: *x0, y, w: *len, h: 1 });
                }
            }
            out.extend(open.drain(..));
            open = next_open;
        }
        out.extend(open);
        // Back to unshifted tile coordinates (may be -1: callers use signed math through `i32`).
        out
    }
}

/// Wall boxes are stored with a +1 shift; this gives the real tile origin.
pub fn box_origin(b: &WallBox) -> (i32, i32) {
    (b.x as i32 - 1, b.y as i32 - 1)
}

fn carve_rect(tiles: &mut [Tile], x: usize, y: usize, w: usize, h: usize, t: Tile) {
    for yy in y..y + h {
        for xx in x..x + w {
            if xx < W && yy < H {
                tiles[yy * W + xx] = t;
            }
        }
    }
}

fn carve_corridor(tiles: &mut [Tile], a: (usize, usize), b: (usize, usize), horizontal_first: bool) {
    let (ax, ay) = (a.0 as i32, a.1 as i32);
    let (bx, by) = (b.0 as i32, b.1 as i32);
    let mut put = |x: i32, y: i32| {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (xx, yy) = (x + dx, y + dy);
                if xx >= 0 && yy >= 0 && (xx as usize) < W && (yy as usize) < H && tiles[yy as usize * W + xx as usize] == Tile::Void {
                    tiles[yy as usize * W + xx as usize] = Tile::Corridor;
                }
            }
        }
    };
    let (corner_x, corner_y) = if horizontal_first { (bx, ay) } else { (ax, by) };
    let walk = |from: (i32, i32), to: (i32, i32), put: &mut dyn FnMut(i32, i32)| {
        let (mut x, mut y) = from;
        put(x, y);
        while (x, y) != to {
            x += (to.0 - x).signum();
            y += (to.1 - y).signum();
            put(x, y);
        }
    };
    walk((ax, ay), (corner_x, corner_y), &mut put);
    walk((corner_x, corner_y), (bx, by), &mut put);
}

/// Generate one floor.
pub fn generate(seed: u64, floor: u32, boss_floor: bool) -> Dungeon {
    let mut rng = Rng::new(seed ^ (floor as u64).wrapping_mul(0xA24B_AED4_963E_E407) ^ 0x51ED);
    let target = 6 + rng.below(3) as usize;
    // Random walk over lattice cells.
    let mut cells: Vec<(usize, usize)> = vec![(rng.below(COLS as u32) as usize, rng.below(ROWS as u32) as usize)];
    let mut links: Vec<(usize, usize)> = vec![];
    while cells.len() < target {
        let from = rng.below(cells.len() as u32) as usize;
        let (cx, cy) = cells[from];
        let dir = rng.below(4);
        let (nx, ny) = match dir {
            0 => (cx as i32 + 1, cy as i32),
            1 => (cx as i32 - 1, cy as i32),
            2 => (cx as i32, cy as i32 + 1),
            _ => (cx as i32, cy as i32 - 1),
        };
        if nx < 0 || ny < 0 || nx >= COLS as i32 || ny >= ROWS as i32 {
            continue;
        }
        let n = (nx as usize, ny as usize);
        if cells.contains(&n) {
            continue;
        }
        cells.push(n);
        links.push((from, cells.len() - 1));
    }
    // Extra loops between adjacent unlinked rooms.
    for i in 0..cells.len() {
        for j in i + 1..cells.len() {
            let d = (cells[i].0 as i32 - cells[j].0 as i32).abs() + (cells[i].1 as i32 - cells[j].1 as i32).abs();
            if d == 1 && !links.contains(&(i, j)) && !links.contains(&(j, i)) && rng.chance(0.3) {
                links.push((i, j));
            }
        }
    }
    let mut tiles = vec![Tile::Void; W * H];
    let mut rooms: Vec<Room> = vec![];
    for &(cx, cy) in cells.iter() {
        let max = CELL - 4;
        let (w, h) = (7 + rng.below(max as u32 - 6) as usize, 7 + rng.below(max as u32 - 6) as usize);
        let x = cx * CELL + 2 + rng.below((max - w) as u32 + 1) as usize;
        let y = cy * CELL + 2 + rng.below((max - h) as u32 + 1) as usize;
        rooms.push(Room { x, y, w, h, kind: RoomKind::Combat, cell: (cx, cy), links: vec![] });
    }
    for &(a, b) in &links {
        rooms[a].links.push(b);
        rooms[b].links.push(a);
    }
    // Kinds: BFS depth from room 0 (the walk's first room) picks the farthest as exit.
    let mut depth = vec![usize::MAX; rooms.len()];
    depth[0] = 0;
    let mut queue = vec![0usize];
    let mut qi = 0;
    while qi < queue.len() {
        let r = queue[qi];
        qi += 1;
        for &n in &rooms[r].links.clone() {
            if depth[n] == usize::MAX {
                depth[n] = depth[r] + 1;
                queue.push(n);
            }
        }
    }
    let exit = (1..rooms.len()).max_by_key(|&i| (depth[i], i)).unwrap_or(0);
    // The boss arena fills its whole lattice cell interior.
    if boss_floor {
        let max = CELL - 4;
        let r = &mut rooms[exit];
        r.w = max;
        r.h = max;
        r.x = r.cell.0 * CELL + 2;
        r.y = r.cell.1 * CELL + 2;
    }
    rooms[0].kind = RoomKind::Start;
    rooms[exit].kind = if boss_floor { RoomKind::Boss } else { RoomKind::Stairs };
    let treasure = (1..rooms.len()).filter(|&i| i != exit && rooms[i].links.len() == 1).max_by_key(|&i| (depth[i], i)).or_else(|| (1..rooms.len()).filter(|&i| i != exit).max_by_key(|&i| (depth[i], i)));
    if let Some(t) = treasure {
        rooms[t].kind = RoomKind::Treasure;
    }
    for r in rooms.iter() {
        let t = if r.kind == RoomKind::Boss { Tile::BossRoom } else { Tile::Room };
        carve_rect(&mut tiles, r.x, r.y, r.w, r.h, t);
    }
    // Corridors after rooms so that they only fill void tiles.
    for &(a, b) in &links {
        let ca = (rooms[a].x + rooms[a].w / 2, rooms[a].y + rooms[a].h / 2);
        let cb = (rooms[b].x + rooms[b].w / 2, rooms[b].y + rooms[b].h / 2);
        let horizontal = rooms[a].cell.1 == rooms[b].cell.1;
        carve_corridor(&mut tiles, ca, cb, horizontal);
    }
    Dungeon { tiles, rooms, start: 0, exit, boss_floor }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flood(d: &Dungeon, from: (i32, i32)) -> Vec<bool> {
        let mut seen = vec![false; W * H];
        let mut stack = vec![from];
        while let Some((x, y)) = stack.pop() {
            if !d.is_floor_tile(x, y) || seen[y as usize * W + x as usize] {
                continue;
            }
            seen[y as usize * W + x as usize] = true;
            stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
        }
        seen
    }

    #[test]
    fn every_floor_is_connected_for_many_seeds() {
        for seed in 0..120u64 {
            for floor in 1..=3 {
                let d = generate(seed, floor, floor == 3);
                let c = d.rooms[d.start].center();
                let seen = flood(&d, Dungeon::tile_of(c));
                for (i, r) in d.rooms.iter().enumerate() {
                    let rc = Dungeon::tile_of(r.center());
                    assert!(seen[rc.1 as usize * W + rc.0 as usize], "seed {seed} floor {floor}: room {i} unreachable");
                }
                // No floor tile is cut off from the start.
                let total = d.tiles.iter().filter(|t| **t != Tile::Void).count();
                assert_eq!(seen.iter().filter(|s| **s).count(), total, "seed {seed}: orphan floor tiles");
            }
        }
    }

    #[test]
    fn generation_is_deterministic_and_seeds_differ() {
        let a = generate(42, 2, false);
        let b = generate(42, 2, false);
        assert_eq!(a.tiles, b.tiles);
        assert_eq!(a.exit, b.exit);
        assert_ne!(generate(43, 2, false).tiles, a.tiles);
        assert_ne!(generate(42, 3, false).tiles, a.tiles);
    }

    #[test]
    fn rooms_do_not_overlap_and_stay_inside_the_grid() {
        for seed in 0..60u64 {
            let d = generate(seed, 1, false);
            assert!((6..=8).contains(&d.rooms.len()));
            for (i, a) in d.rooms.iter().enumerate() {
                assert!(a.x >= 1 && a.y >= 1 && a.x + a.w < W && a.y + a.h < H);
                for b in &d.rooms[i + 1..] {
                    assert!(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y, "overlap in seed {seed}");
                }
                assert!(!a.links.is_empty());
            }
        }
    }

    #[test]
    fn exit_start_and_boss_roles() {
        for seed in 0..60u64 {
            let d = generate(seed, 1, false);
            assert_eq!(d.rooms[d.start].kind, RoomKind::Start);
            assert_eq!(d.rooms[d.exit].kind, RoomKind::Stairs);
            assert_ne!(d.exit, d.start);
            assert!(d.rooms.iter().filter(|r| r.kind == RoomKind::Treasure).count() <= 1);
            let b = generate(seed, 3, true);
            assert_eq!(b.rooms[b.exit].kind, RoomKind::Boss);
            assert!(b.boss_floor && b.rooms.iter().filter(|r| r.kind == RoomKind::Stairs).count() == 0);
            let r = &b.rooms[b.exit];
            assert!(r.w >= CELL - 4 && r.h >= CELL - 4);
        }
    }

    #[test]
    fn walls_surround_the_floor_without_covering_it() {
        let d = generate(7, 1, false);
        let boxes = d.wall_boxes();
        assert!(!boxes.is_empty() && boxes.len() < 400, "{} boxes", boxes.len());
        let mut covered = vec![false; (W + 4) * (H + 4)];
        for b in &boxes {
            let (x0, y0) = box_origin(b);
            for y in y0..y0 + b.h as i32 {
                for x in x0..x0 + b.w as i32 {
                    assert!(!d.is_floor_tile(x, y), "wall over floor at {x},{y}");
                    covered[(y + 1) as usize * (W + 4) + (x + 1) as usize] = true;
                }
            }
        }
        // Every void tile that touches floor is walled.
        for y in 0..H as i32 {
            for x in 0..W as i32 {
                if d.is_floor_tile(x, y) {
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (nx, ny) = (x + dx, y + dy);
                        if !d.is_floor_tile(nx, ny) {
                            assert!(covered[(ny + 1) as usize * (W + 4) + (nx + 1) as usize], "open edge at {nx},{ny}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn circle_movement_slides_along_walls() {
        let d = generate(3, 1, false);
        let c = d.rooms[d.start].center();
        assert!(d.circle_free(c, 60.0));
        // Walk far in one direction: the circle always stays on floor.
        let mut p = c;
        for _ in 0..400 {
            p = d.slide(p, [30.0, 12.0], 60.0);
            assert!(d.circle_free(p, 60.0));
        }
    }
}
