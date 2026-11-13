//! `crowdRoute.ts`: plan routes Mario can actually run (grid A* over standable floors).
use jumpman_sim::collision::{CollisionWorld, Surface};
use jumpman_sim::math::Vec3;
use jumpman_sim::surfaces::SURFACE_DEATH_PLANE;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

const CELL: f64 = 150.0;
const STEP_UP: f64 = 95.0;
const JUMP_UP: f64 = 210.0;
const DROP: f64 = 700.0;

/// Minimum floor normal Mario can run up, by surface type.
/// The same surface classes as the physics (`mario_get_floor_class`): e.g. Bob-omb Battlefield's grassy ramps are
/// "noise slippery" (0x2a), which Mario slides down past 20 degrees, not 38.
pub fn walkable_normal(surface: &Surface) -> f64 {
    use jumpman_sim::surfaces::*;
    match surface.kind {
        SURFACE_VERY_SLIPPERY | SURFACE_ICE | SURFACE_HARD_VERY_SLIPPERY | SURFACE_NOISE_VERY_SLIPPERY_73 | SURFACE_NOISE_VERY_SLIPPERY_74 | SURFACE_NOISE_VERY_SLIPPERY | SURFACE_NO_CAM_COL_VERY_SLIPPERY => 0.9848, // 10 deg
        SURFACE_SLIPPERY | SURFACE_NOISE_SLIPPERY | SURFACE_HARD_SLIPPERY | SURFACE_NO_CAM_COL_SLIPPERY => 0.9397,                                                                                       // 20 deg
        SURFACE_NOT_SLIPPERY | SURFACE_HARD_NOT_SLIPPERY | SURFACE_SWITCH => 0.0,
        _ => 0.788, // 38 deg
    }
}

fn standable(world: &CollisionWorld, x: f64, z: f64, probe: f64) -> Option<f64> {
    let floor = world.find_floor(x, probe, z);
    let s = floor.surface.as_ref()?;
    if s.kind == SURFACE_DEATH_PLANE || s.normal[1] < walkable_normal(s) {
        return None;
    }
    if world.water_level(x, z) > floor.height - 30.0 {
        return None;
    }
    Some(floor.height)
}

/// Whether Mario can run the straight segment a->b without sliding or climbing walls.
pub fn segment_walkable(world: &CollisionWorld, a: Vec3, b: Vec3) -> bool {
    segment_walkable_drop(world, a, b, DROP)
}

/// [`segment_walkable`] allowing drops of up to `drop` units (a racer may hop off a ledge the crowd avoids).
pub fn segment_walkable_drop(world: &CollisionWorld, a: Vec3, b: Vec3, drop: f64) -> bool {
    let length = (b[0] - a[0]).hypot(b[2] - a[2]);
    let steps = ((length / 100.0).ceil() as usize).max(1);
    let Some(mut previous) = standable(world, a[0], a[2], a[1] + 300.0) else { return false };
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let Some(h) = standable(world, a[0] + (b[0] - a[0]) * t, a[2] + (b[2] - a[2]) * t, previous.max(a[1] + (b[1] - a[1]) * t) + 300.0) else {
            return false;
        };
        if h - previous > STEP_UP || previous - h > drop {
            return false;
        }
        previous = h;
    }
    true
}

#[derive(Default, Debug)]
pub struct PathStats {
    pub expansions: usize,
    pub start_ok: bool,
}

#[derive(PartialEq)]
struct Node(f64, usize);
impl Eq for Node {}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Node {
    // Min-heap on f.
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
    }
}

/// Grid A* over the course's top floors between two points; None when no path is found.
pub fn find_walk_path(world: &CollisionWorld, a: Vec3, b: Vec3, margin: f64, budget: usize, stats: Option<&mut PathStats>) -> Option<Vec<Vec3>> {
    find_walk_path_drop(world, a, b, margin, budget, stats, DROP)
}

/// [`find_walk_path`] allowing drops of up to `drop` units.
pub fn find_walk_path_drop(world: &CollisionWorld, a: Vec3, b: Vec3, margin: f64, budget: usize, mut stats: Option<&mut PathStats>, drop: f64) -> Option<Vec<Vec3>> {
    let min_x = a[0].min(b[0]) - margin;
    let min_z = a[2].min(b[2]) - margin;
    let cols = (((a[0] - b[0]).abs() + margin * 2.0) / CELL).ceil() as usize;
    let rows = (((a[2] - b[2]).abs() + margin * 2.0) / CELL).ceil() as usize;
    // Probe just above the height the segment would have here, so stacked terrain resolves to
    // the layer being walked.
    let (sx, sz) = (b[0] - a[0], b[2] - a[2]);
    let length2 = (sx * sx + sz * sz).max(1.0);
    let probe_at = |x: f64, z: f64| {
        let t = (((x - a[0]) * sx + (z - a[2]) * sz) / length2).clamp(0.0, 1.0);
        a[1] + (b[1] - a[1]) * t + 700.0
    };
    let n = cols * rows;
    let mut heights = vec![f64::NAN; n];
    let mut known = vec![false; n];
    let mut height = |i: usize, heights: &mut Vec<f64>| -> f64 {
        if !known[i] {
            known[i] = true;
            let x = min_x + ((i % cols) as f64 + 0.5) * CELL;
            let z = min_z + ((i / cols) as f64 + 0.5) * CELL;
            heights[i] = standable(world, x, z, probe_at(x, z)).unwrap_or(f64::NAN);
        }
        heights[i]
    };
    let cell_of = |p: Vec3| ((((p[2] - min_z) / CELL).floor() as i64) * cols as i64 + ((p[0] - min_x) / CELL).floor() as i64) as usize;
    let start = cell_of(a);
    let goal = cell_of(b);
    if start >= n || goal >= n {
        return None;
    }
    let (gx, gz) = ((goal % cols) as i64, (goal / cols) as i64);
    let mut g = vec![f64::INFINITY; n];
    let mut came = vec![-1i64; n];
    let mut closed = vec![false; n];
    let mut heap = BinaryHeap::new();
    let start_ok = !height(start, &mut heights).is_nan();
    if let Some(s) = stats.as_deref_mut() {
        s.start_ok = start_ok;
    }
    if !start_ok {
        return None;
    }
    g[start] = 0.0;
    heap.push(Node(0.0, start));
    let mut expansions = 0usize;
    let mut found: Option<usize> = None;
    while let Some(Node(_, current)) = heap.pop() {
        if expansions >= budget {
            break;
        }
        expansions += 1;
        if closed[current] {
            continue;
        }
        closed[current] = true;
        let (cx, cz) = ((current % cols) as i64, (current / cols) as i64);
        let h0 = height(current, &mut heights);
        if (cx - gx).abs() <= 1 && (cz - gz).abs() <= 1 && (h0 - b[1]).abs() < 250.0 {
            found = Some(current);
            break;
        }
        for dz in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let (nx, nz) = (cx + dx, cz + dz);
                if nx < 0 || nz < 0 || nx >= cols as i64 || nz >= rows as i64 {
                    continue;
                }
                let next = nz as usize * cols + nx as usize;
                if closed[next] {
                    continue;
                }
                let h1 = height(next, &mut heights);
                if h1.is_nan() {
                    continue;
                }
                let rise = h1 - h0;
                if rise > JUMP_UP || -rise > drop {
                    continue;
                }
                let step = (if dx != 0 && dz != 0 { 1.414 } else { 1.0 }) * CELL + rise.max(0.0) * 1.5 + if rise > STEP_UP { 600.0 } else { 0.0 };
                let cost = g[current] + step;
                if cost >= g[next] {
                    continue;
                }
                g[next] = cost;
                came[next] = current as i64;
                heap.push(Node(cost + ((nx - gx) as f64).hypot((nz - gz) as f64) * CELL, next));
            }
        }
    }
    if let Some(s) = stats.as_deref_mut() {
        s.expansions = expansions;
    }
    let found = found?;
    let mut cells = vec![];
    let mut i = found as i64;
    while i >= 0 {
        cells.push(i as usize);
        i = came[i as usize];
    }
    cells.reverse();
    let points: Vec<Vec3> = cells.iter().map(|&i| [min_x + ((i % cols) as f64 + 0.5) * CELL, heights[i], min_z + ((i / cols) as f64 + 0.5) * CELL]).collect();
    // Keep only the corners: drop points whose shortcut is still walkable.
    let mut simplified = vec![points[0]];
    let mut anchor = 0;
    for i in 2..points.len() {
        if !segment_walkable_drop(world, points[anchor], points[i], drop) || (points[i][0] - points[anchor][0]).hypot(points[i][2] - points[anchor][2]) > 1400.0 {
            simplified.push(points[i - 1]);
            anchor = i - 1;
        }
    }
    simplified.push(b);
    Some(simplified)
}

/// Turn a route authored for another walker (e.g. Koopa the Quick) into one Mario can actually
/// run, detouring where needed.
pub fn mario_route(world: &CollisionWorld, route: &[Vec3]) -> Vec<Vec3> {
    mario_route_drop(world, route, DROP)
}

/// [`mario_route`] with the largest drop off a ledge the plan may take.
pub fn mario_route_drop(world: &CollisionWorld, route: &[Vec3], drop: f64) -> Vec<Vec3> {
    // Waypoints sitting on slopes Mario slides down move to the nearest standable spot.
    let points: Vec<Vec3> = route.iter().map(|&p| nearest_standable(world, p, 1500.0).unwrap_or(p)).collect();
    let mut out = vec![points[0]];
    for &b in &points[1..] {
        // Reach b from the latest route point that can; backtracking skips dead ends.
        let mut joined = false;
        let lowest = out.len().saturating_sub(24);
        let mut j = out.len() as i64 - 1;
        while j >= lowest as i64 && !joined {
            let a = out[j as usize];
            if segment_walkable_drop(world, a, b, drop) {
                out.truncate(j as usize + 1);
                out.push(b);
                joined = true;
                break;
            }
            if let Some(detour) = find_walk_path_drop(world, a, b, 3200.0, 150000, None, drop) {
                out.truncate(j as usize + 1);
                out.extend_from_slice(&detour[1..]);
                joined = true;
            }
            j -= 1;
        }
        // No walkable plan: keep the checkpoint; racers jump and, if needed, warp past it.
        if !joined {
            out.push(b);
        }
    }
    out
}

/// The closest point near `p` where Mario can stand, preferring similar height.
pub fn nearest_standable(world: &CollisionWorld, p: Vec3, radius: f64) -> Option<Vec3> {
    let probe = p[1] + 400.0;
    if let Some(here) = standable(world, p[0], p[2], probe) {
        return Some([p[0], here, p[2]]);
    }
    let mut best: Option<Vec3> = None;
    let mut score = f64::INFINITY;
    let mut r = 100.0;
    while r <= radius {
        let samples = ((std::f64::consts::PI * 2.0 * r) / 100.0).ceil() as usize;
        for i in 0..samples {
            let a = i as f64 / samples as f64 * std::f64::consts::PI * 2.0;
            let (x, z) = (p[0] + a.cos() * r, p[2] + a.sin() * r);
            let Some(h) = standable(world, x, z, probe) else { continue };
            let s = r + (h - p[1]).abs() * 1.5;
            if s < score {
                score = s;
                best = Some([x, h, z]);
            }
        }
        if best.is_some() && score < r + 200.0 {
            break;
        }
        r += 100.0;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::SurfaceOptions;

    fn ground(world: &mut CollisionWorld, size: f64) {
        let h = size / 2.0;
        world.add_quad([-h, 0., -h], [-h, 0., h], [h, 0., h], [h, 0., -h], &SurfaceOptions::default());
        world.finalize();
    }

    #[test]
    fn finds_a_straight_path_on_flat_ground() {
        let mut world = CollisionWorld::default();
        ground(&mut world, 8000.0);
        assert!(segment_walkable(&world, [-1720., 0., 120.], [-2720., 0., 1120.]));
        assert!(find_walk_path(&world, [-1720., 0., 120.], [-2720., 0., 1120.], 1500.0, 40000, None).is_some());
        assert!(find_walk_path(&world, [1000., 0., 120.], [-2720., 0., -1120.], 1500.0, 40000, None).is_some());
    }

    #[test]
    fn detours_around_a_slippery_slope_mario_would_slide_down() {
        let mut world = CollisionWorld::default();
        ground(&mut world, 8000.0);
        let slippery = SurfaceOptions { kind: 0x14, ..Default::default() };
        let normal = SurfaceOptions::default();
        world.add_quad([-300., 0., 600.], [-300., 400., 1200.], [300., 400., 1200.], [300., 0., 600.], &slippery);
        world.add_quad([-300., 400., 1200.], [-300., 400., 1800.], [300., 400., 1800.], [300., 400., 1200.], &normal);
        world.add_quad([300., 400., 1200.], [300., 400., 1800.], [1500., 0., 1800.], [1500., 0., 1200.], &normal);
        world.finalize();
        assert!(!segment_walkable(&world, [0., 0., 0.], [0., 400., 1500.]));
        let route = mario_route(&world, &[[0., 0., 0.], [0., 400., 1500.]]);
        assert!(route.len() > 2);
        for i in 1..route.len() {
            assert!(segment_walkable(&world, route[i - 1], route[i]), "segment {i}");
        }
    }
}
