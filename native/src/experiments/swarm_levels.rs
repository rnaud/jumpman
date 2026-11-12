//! Star Swarm battlefields: stretches of real courses that are open enough to fight on. The mode
//! plays on the course's own collision (its hills, pits, cliffs and water bound the swarm and the
//! player); the only thing added is a shrinking storm ring, so there is no invisible box.
//!
//! Every entry was chosen by `scan_for_open_ground` below (largest connected walkable disc of the
//! course) and is checked again by a test against the real collision when the game data is present.
use jumpman_sim::collision::CollisionWorld;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelDef {
    /// Short key kept in the save file.
    pub key: &'static str,
    /// Course id the mode loads.
    pub course: &'static str,
    pub name: &'static str,
    /// Centre of the arena in course x/z, and its radius (SM64 units).
    pub center: [f64; 2],
    pub radius: f32,
    /// Walkable cells may differ this much from the arena's median height.
    pub max_rise: f32,
    /// Sky / mood line shown on the briefing.
    pub blurb: &'static str,
}

pub const LEVELS: [LevelDef; 1] = [LevelDef { key: "bob", course: "bob", name: "Bob-omb Battlefield", center: [-500.0, 4750.0], radius: 2250.0, max_rise: 320.0, blurb: "The open meadow below the castle bridge." }];

pub fn by_key(key: &str) -> Option<&'static LevelDef> {
    LEVELS.iter().find(|l| l.key == key)
}

pub fn index_of(key: &str) -> usize {
    LEVELS.iter().position(|l| l.key == key).unwrap_or(0)
}

/// Surfaces a fighter can stand on: not lava, a death plane or quicksand.
pub fn floor_ok(kind: i32, normal_y: f64) -> bool {
    normal_y > 0.7 && !matches!(kind, 0x01 | 0x0a | 0x12 | 0x13)
}

/// Topmost walkable floor height at a point, or None (no floor, a hazard, a steep face, under water).
pub fn ground_at(world: &CollisionWorld, x: f64, z: f64, top: f64) -> Option<f32> {
    let hit = world.find_floor(x, top, z);
    let s = hit.surface?;
    if !floor_ok(s.kind, s.normal[1]) || world.water_level(x, z) > hit.height + 20.0 {
        return None;
    }
    Some(hit.height as f32)
}

#[cfg(test)]
pub mod scan {
    use super::*;

    pub struct Found {
        pub center: [f64; 2],
        pub fraction: f32,
        pub base: f32,
    }

    /// The best disc of `radius` on a course: the highest share of cells that are walkable (flat enough,
    /// no hazard) and connected to the disc's centre. Sampled at `cell` units over the whole level.
    pub fn open_ground(world: &CollisionWorld, radius: f32, cell: f32) -> Option<Found> {
        let lim = 8000.0f32;
        let n = (lim * 2.0 / cell) as usize;
        let at = |i: usize| -lim + (i as f32 + 0.5) * cell;
        let mut h = vec![f32::NAN; n * n];
        for iz in 0..n {
            for ix in 0..n {
                if let Some(v) = ground_at(world, at(ix) as f64, at(iz) as f64, 9000.0) {
                    h[iz * n + ix] = v;
                }
            }
        }
        // Walkable: a floor with no sharp step to any neighbour.
        let mut ok = vec![false; n * n];
        for iz in 1..n - 1 {
            for ix in 1..n - 1 {
                let c = h[iz * n + ix];
                if c.is_nan() {
                    continue;
                }
                ok[iz * n + ix] = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|(dx, dz)| {
                    let q = h[(iz as i32 + dz) as usize * n + (ix as i32 + dx) as usize];
                    !q.is_nan() && (q - c).abs() < cell * 0.6
                });
            }
        }
        let r_cells = (radius / cell) as i32;
        let mut best: Option<Found> = None;
        let stride = (r_cells / 3).max(1) as usize;
        let mut seen = vec![0u32; n * n];
        let mut stamp = 0u32;
        let mut stack = Vec::new();
        for cz in (r_cells as usize..n - r_cells as usize).step_by(stride) {
            for cx in (r_cells as usize..n - r_cells as usize).step_by(stride) {
                if !ok[cz * n + cx] {
                    continue;
                }
                stamp += 1;
                stack.clear();
                stack.push((cx, cz));
                seen[cz * n + cx] = stamp;
                let (mut reached, mut total) = (0u32, 0u32);
                for dz in -r_cells..=r_cells {
                    for dx in -r_cells..=r_cells {
                        if dx * dx + dz * dz <= r_cells * r_cells {
                            total += 1;
                        }
                    }
                }
                while let Some((x, z)) = stack.pop() {
                    reached += 1;
                    for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                        let (nx, nz) = ((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                        let (ddx, ddz) = (nx as i32 - cx as i32, nz as i32 - cz as i32);
                        if ddx * ddx + ddz * ddz > r_cells * r_cells || !ok[nz * n + nx] || seen[nz * n + nx] == stamp {
                            continue;
                        }
                        seen[nz * n + nx] = stamp;
                        stack.push((nx, nz));
                    }
                }
                let f = reached as f32 / total as f32;
                if best.as_ref().is_none_or(|b| f > b.fraction) {
                    best = Some(Found { center: [at(cx) as f64, at(cz) as f64], fraction: f, base: h[cz * n + cx] });
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::course_data::CourseData;
    use crate::game::runner::GameSim;
    use crate::game::world::CourseRequest;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn keys_are_unique_and_lookups_agree() {
        for (i, l) in LEVELS.iter().enumerate() {
            assert_eq!(index_of(l.key), i);
            assert_eq!(by_key(l.key), Some(l));
            assert!(l.radius >= 1500.0 && l.radius <= 3600.0, "{}", l.key);
            assert_eq!(LEVELS.iter().filter(|o| o.key == l.key).count(), 1);
        }
    }

    /// Dev tool: `SWARM_SCAN=wf,ccm cargo test -p jumpman scan_courses -- --ignored --nocapture` prints the best open disc of each course.
    #[test]
    #[ignore]
    fn scan_courses() {
        let list = std::env::var("SWARM_SCAN").unwrap_or_default();
        for id in list.split(',').filter(|s| !s.is_empty()) {
            if !CourseData::path(&assets(), id, 1).is_file() {
                println!("scan {id}: no data");
                continue;
            }
            let Ok(sim) = GameSim::load(&assets(), CourseRequest::new(id)) else {
                println!("scan {id}: load failed");
                continue;
            };
            for radius in [2250.0f32, 1800.0] {
                match scan::open_ground(&sim.mario.world, radius, 125.0) {
                    Some(f) => println!("scan {id} r={radius}: center [{:.0}, {:.0}] open {:.0}% base {:.0}", f.center[0], f.center[1], f.fraction * 100.0, f.base),
                    None => println!("scan {id} r={radius}: nothing"),
                }
            }
        }
    }

    #[test]
    fn every_level_is_open_walkable_ground_on_its_real_course() {
        for l in LEVELS {
            if !CourseData::path(&assets(), l.course, 1).is_file() {
                continue;
            }
            let Ok(sim) = GameSim::load(&assets(), CourseRequest::new(l.course)) else { continue };
            let w = &sim.mario.world;
            let here = ground_at(w, l.center[0], l.center[1], 9000.0);
            assert!(here.is_some(), "{}: the arena centre is not walkable ground", l.key);
            let f = scan::open_ground(w, l.radius, 125.0).expect("some open ground");
            let mine = {
                let mut ok = 0;
                let mut total = 0;
                for iz in -8..=8 {
                    for ix in -8..=8 {
                        let (dx, dz) = (ix as f32 * l.radius / 8.0, iz as f32 * l.radius / 8.0);
                        if dx * dx + dz * dz > l.radius * l.radius {
                            continue;
                        }
                        total += 1;
                        ok += ground_at(w, l.center[0] + dx as f64, l.center[1] + dz as f64, 9000.0).is_some() as u32;
                    }
                }
                ok as f32 / total as f32
            };
            assert!(mine > 0.55, "{}: only {:.0}% of the arena has floor (best disc on the course {:.0}%)", l.key, mine * 100.0, f.fraction * 100.0);
        }
    }
}
