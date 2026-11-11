//! Gem Clash, real-world arenas (engine-free): a [`Region`] is a hand-picked stretch of one of the
//! real courses (Bob-omb Battlefield's summit, the castle courtyard, Kokiri Forest's clearing ...)
//! that the Gem Clash rules play on instead of a made-up box arena. The floors and walls come from
//! the course's own `CollisionWorld` (the same one Mario, Link and Banjo walk on) through
//! [`WorldGround`], which implements the arena's [`Ground`] seam; gems, items and spawns are
//! validated against it at build time.
//!
//! Coordinates: the core works in arena-local units (origin = the region's centre, y = 0 the
//! height of the floor there). `WorldGround` adds `origin` to get course coordinates, so the rules,
//! the bots and the digest never see the offset; only the presentation (`brawl.rs`) adds it back.
use super::brawl_arena::*;
use jumpman_sim::collision::{CollisionWorld, Hit};
use jumpman_sim::surfaces::*;
use std::sync::Arc;

/// One playable stretch of a real course. Spot lists are arena-local (x, z); heights are read off
/// the course when the arena is built.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    /// Short key for the menu persistence and `JUMPMAN_BRAWL_REGION`.
    pub key: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    /// Course id (`courses_table`) the region lives in.
    pub course: &'static str,
    /// Course (x, z) of the arena centre and a height above the floor there to search down from.
    pub centre: [f64; 3],
    /// Exact course height of the arena floor (local y = 0); None finds the floor under `centre` (searching down from its y).
    pub floor: Option<f64>,
    /// Half extents of the playable rectangle (an invisible rim keeps fighters inside it).
    pub half: V2,
    /// Local height under which a fighter is out of the world (the floor at the centre is 0).
    pub void_y: f32,
    /// Sunken basins (x0, z0, x1, z1, arena-local) whose low floor does not count: whoever steps in falls through
    /// and is put back on a spawn (a pond nobody can climb out of would trap the bots).
    pub pits: &'static [[f32; 4]],
    pub spawns: [V2; 4],
    pub gems: &'static [V2],
    pub items: &'static [V2],
}

/// The real-world arenas, in menu order (after the six classic ones).
pub const REGIONS: &[Region] = &[
    Region {
        key: "bob",
        name: "Bob-omb Summit",
        blurb: "The round mountain top: no walls, ring-outs over the cliff, one ramp down",
        course: "bob",
        centre: [2950.0, 8000.0, -4850.0],
        floor: None,
        half: [1900.0, 1900.0],
        void_y: -500.0,
        pits: &[],
        spawns: [[-1000.0, 0.0], [1000.0, 0.0], [0.0, -1000.0], [0.0, 1000.0]],
        gems: &[[-500.0, 0.0], [500.0, 0.0], [0.0, -500.0], [0.0, 500.0], [-900.0, 700.0], [900.0, -700.0]],
        items: &[[-300.0, -300.0], [300.0, 300.0], [-300.0, 300.0], [300.0, -300.0], [-1100.0, -600.0], [1100.0, 600.0], [-600.0, -1100.0], [600.0, 1100.0]],
    },
    Region {
        key: "courtyard",
        name: "Castle Courtyard",
        blurb: "A walled garden: a raised ring around the pond, hedged lawn outside",
        course: "castle_courtyard",
        centre: [0.0, 600.0, -1800.0],
        floor: Some(0.0),
        half: [1100.0, 1650.0],
        void_y: -400.0,
        pits: &[[-650.0, -650.0, 650.0, 650.0]],
        spawns: [[-950.0, -1500.0], [950.0, 1500.0], [950.0, -1500.0], [-950.0, 1500.0]],
        gems: &[[-700.0, 0.0], [700.0, 0.0], [0.0, -660.0], [0.0, 780.0], [-1000.0, -1100.0], [1000.0, 1100.0]],
        items: &[[-1000.0, 500.0], [1000.0, -500.0], [-700.0, -1500.0], [700.0, 1500.0], [-300.0, 1400.0], [300.0, -1400.0], [0.0, 1200.0], [0.0, -1200.0]],
    },
    Region {
        key: "kokiri",
        name: "Kokiri Forest",
        blurb: "The village green under the trees, open and flat",
        course: "kokiri_forest",
        centre: [-8600.0, 3000.0, 100.0],
        floor: None,
        half: [1100.0, 1100.0],
        void_y: -500.0,
        pits: &[],
        spawns: [[-750.0, -750.0], [750.0, 750.0], [750.0, -750.0], [-750.0, 750.0]],
        gems: &[[-300.0, 0.0], [300.0, 0.0], [0.0, 300.0], [0.0, -300.0], [-650.0, 250.0], [650.0, -250.0]],
        items: &[[-450.0, -450.0], [450.0, 450.0], [-450.0, 450.0], [450.0, -450.0], [0.0, 700.0], [0.0, -700.0], [-800.0, 0.0], [800.0, 0.0]],
    },
];

/// Menu entries: 0..6 are the classic arenas, then one per real-world region.
pub const CLASSIC: usize = 6;

pub fn arena_count() -> usize {
    CLASSIC + REGIONS.len()
}

/// The region a menu arena index picks (None for a classic arena).
pub fn region_of(arena: u8) -> Option<&'static Region> {
    (arena as usize).checked_sub(CLASSIC).and_then(|i| REGIONS.get(i))
}

pub fn arena_name(arena: u8) -> &'static str {
    region_of(arena).map_or_else(|| ARENAS[arena as usize % CLASSIC].name(), |r| r.name)
}

pub fn arena_blurb(arena: u8) -> &'static str {
    region_of(arena).map_or_else(|| ARENAS[arena as usize % CLASSIC].hazard(), |r| r.blurb)
}

fn choice_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("gemclash_arena.txt")
}

/// The arena picked last time (menu index), 0 when never saved or no longer there.
pub fn load_choice() -> u8 {
    let v = std::fs::read_to_string(choice_path()).ok().and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(0);
    if v < arena_count() { v as u8 } else { 0 }
}

pub fn save_choice(arena: u8) {
    let path = choice_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, arena.to_string());
}

impl Region {
    pub fn parse(s: &str) -> Option<usize> {
        let s = s.to_ascii_lowercase();
        REGIONS.iter().position(|r| r.key == s)
    }

    /// Build the arena on `world` (the course `self.course`, already loaded).
    pub fn build(&self, world: Arc<CollisionWorld>) -> Arena {
        let mut ground = WorldGround::new(world, self.centre);
        // Local y = 0 is the floor at the centre.
        ground.origin[1] = self.floor.or_else(|| ground.course_floor(self.centre[0], self.centre[2], self.centre[1])).unwrap_or(self.centre[1]);
        ground.pits = self.pits.to_vec();
        let g = Arc::new(ground);
        let mut a = Arena::build(ArenaKind::Field);
        a.half = self.half;
        a.bounds = Some(self.half);
        a.void_y = self.void_y;
        a.ceil_y = 900.0;
        a.spawns = self.spawns;
        let lift = |sp: &[V2]| -> Vec<[f32; 3]> { sp.iter().map(|s| [s[0], g.floor(s[0], s[1], 900.0).unwrap_or(0.0), s[1]]).collect() };
        a.gem_spots = lift(self.gems);
        a.item_spots = lift(self.items);
        a.ground = Some(g);
        a
    }
}

/// A `CollisionWorld` seen through arena-local coordinates.
pub struct WorldGround {
    world: Arc<CollisionWorld>,
    /// Course coordinates of the arena origin.
    pub origin: [f64; 3],
    /// See [`Region::pits`].
    pub pits: Vec<[f32; 4]>,
}

/// How far under the arena floor a pit's floor must be to count as the basin.
const PIT_DEPTH: f32 = -15.0;

impl std::fmt::Debug for WorldGround {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorldGround(origin {:?}, {} surfaces)", self.origin, self.world.surfaces().len())
    }
}

/// Steepest floor (normal.y) something may rest on; about 45 degrees.
pub const WALKABLE_NY: f64 = 0.7;
/// Offset above the feet at which walls are tested (a ledge lower than this is stepped over).
const WALL_OFFSET: f64 = 40.0;

impl WorldGround {
    pub fn new(world: Arc<CollisionWorld>, centre: [f64; 3]) -> Self {
        Self { world, origin: [centre[0], 0.0, centre[2]], pits: vec![] }
    }

    /// The highest floor under course (x, z) that is at most `max_y` (course coordinates).
    fn course_hit(&self, x: f64, z: f64, max_y: f64) -> Option<Hit> {
        let mut q = max_y;
        for _ in 0..8 {
            let h = self.world.find_floor(x, q, z);
            h.surface.as_ref()?;
            if h.height <= max_y + 0.01 {
                return Some(h);
            }
            // find_floor accepts floors up to 78 above the query: step below the one it found.
            q = h.height - 79.0;
        }
        None
    }

    pub fn course_floor(&self, x: f64, z: f64, max_y: f64) -> Option<f64> {
        self.course_hit(x, z, max_y).map(|h| h.height)
    }
}

impl Ground for WorldGround {
    fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        let o = self.origin;
        let y = self.course_hit(x as f64 + o[0], z as f64 + o[2], max_y as f64 + o[1]).map(|h| (h.height - o[1]) as f32)?;
        if y < PIT_DEPTH && self.pits.iter().any(|p| x >= p[0] && x <= p[2] && z >= p[1] && z <= p[3]) {
            return None;
        }
        Some(y)
    }

    fn push(&self, p: &mut [f32; 3], r: f32) {
        let o = self.origin;
        for _ in 0..2 {
            let mut w = [p[0] as f64 + o[0], p[1] as f64 + o[1], p[2] as f64 + o[2]];
            if self.world.resolve_and_return_wall_collisions(&mut w, WALL_OFFSET, r as f64).is_none() {
                break;
            }
            p[0] = (w[0] - o[0]) as f32;
            p[2] = (w[2] - o[2]) as f32;
        }
    }

    fn origin(&self) -> [f32; 3] {
        [self.origin[0] as f32, self.origin[1] as f32, self.origin[2] as f32]
    }

    fn standable(&self, x: f32, y: f32, z: f32) -> bool {
        let o = self.origin;
        let (wx, wy, wz) = (x as f64 + o[0], y as f64 + o[1], z as f64 + o[2]);
        let Some(h) = self.course_hit(wx, wz, wy + 1.0) else { return false };
        let Some(s) = h.surface else { return false };
        let k = s.kind;
        let bad = matches!(k, SURFACE_BURNING | SURFACE_DEATH_PLANE | SURFACE_DEEP_QUICKSAND | SURFACE_SHALLOW_QUICKSAND);
        !bad && s.normal[1] >= WALKABLE_NY && self.world.water_level(wx, wz) < h.height + 12.0
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::game::world::{CourseRequest, LoadedCourse};

    pub fn assets() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    /// The course's collision, or None when the imported data is not on this machine.
    pub fn course_world(id: &str) -> Option<Arc<CollisionWorld>> {
        if !crate::game::course_data::CourseData::path(&assets(), id, 1).is_file() {
            return None;
        }
        LoadedCourse::load(&assets(), CourseRequest::new(id)).ok().map(|c| c.world)
    }

    /// Exploration aid (ignored): an ASCII height map of a course.
    /// `MAP_COURSE=bob MAP_X=0 MAP_Z=0 MAP_Y=4000 MAP_HALF=1500 MAP_STEP=50 cargo test map_dump -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn map_dump() {
        let env = |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let id = std::env::var("MAP_COURSE").unwrap_or_else(|_| "bob".into());
        let Some(world) = course_world(&id) else { return };
        let (cx, cz, cy, half, step) = (env("MAP_X", 0.0), env("MAP_Z", 0.0), env("MAP_Y", 4000.0), env("MAP_HALF", 1500.0), env("MAP_STEP", 50.0));
        let g = WorldGround::new(world.clone(), [cx, cy, cz]);
        let base = g.course_floor(cx, cz, cy).unwrap_or(0.0);
        println!("course {id} centre ({cx},{cz}) floor {base:.0}; cells = height relative to centre floor / 20 (0-9 a-z), '.' none, '#' steep, 'L' hazard");
        let n = (half / step) as i32;
        for iz in -n..=n {
            let mut row = String::new();
            for ix in -n..=n {
                let (x, z) = (cx + ix as f64 * step, cz + iz as f64 * step);
                let c = match g.course_hit(x, z, cy) {
                    None => '.',
                    Some(h) => {
                        let s = h.surface.as_ref().unwrap();
                        let d = ((h.height - base) / 20.0).round() as i32;
                        let mut w = [x, h.height, z];
                        g.world.resolve_and_return_wall_collisions(&mut w, WALL_OFFSET, 35.0);
                        if (w[0] - x).hypot(w[2] - z) > 3.0 {
                            'W'
                        } else if matches!(s.kind, SURFACE_BURNING | SURFACE_DEATH_PLANE) {
                            'L'
                        } else if s.normal[1] < WALKABLE_NY {
                            '#'
                        } else if d < 0 {
                            if d <= -10 { '_' } else { char::from(b'a' + (-d - 1) as u8).to_ascii_uppercase() }
                        } else if d < 10 {
                            char::from(b'0' + d as u8)
                        } else {
                            '+'
                        }
                    }
                };
                row.push(c);
            }
            println!("{:6.0} {row}", cz + iz as f64 * step);
        }
    }
}
