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
    /// Ticks (30 per second) between shells falling on the fighters once a round is under way; 0 = calm.
    pub barrage: i32,
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
        barrage: 150,
        spawns: [[-1000.0, 0.0], [1000.0, 0.0], [0.0, -1000.0], [0.0, 1000.0]],
        gems: &[[-500.0, 0.0], [500.0, 0.0], [0.0, -500.0], [0.0, 500.0], [-900.0, 700.0], [900.0, -700.0]],
        items: &[[-300.0, -300.0], [300.0, 300.0], [-300.0, 300.0], [300.0, -300.0], [-1100.0, -600.0], [1100.0, 600.0], [-600.0, -1100.0], [600.0, 1100.0]],
    },
    Region {
        key: "courtyard",
        name: "Castle Courtyard",
        blurb: "A walled garden plaza: a raised stone ring round the pond, lawns at the sides",
        course: "castle_courtyard",
        centre: [0.0, 600.0, -1800.0],
        floor: Some(0.0),
        half: [1100.0, 1650.0],
        void_y: -400.0,
        pits: &[[-650.0, -650.0, 650.0, 650.0]],
        barrage: 170,
        spawns: [[-950.0, -1500.0], [950.0, 1500.0], [950.0, -1500.0], [-950.0, 1500.0]],
        gems: &[[-700.0, 0.0], [700.0, 0.0], [0.0, -660.0], [0.0, 780.0], [-1000.0, -1100.0], [1000.0, 1100.0]],
        items: &[[-1000.0, 500.0], [1000.0, -500.0], [-700.0, -1500.0], [700.0, 1500.0], [-300.0, 1400.0], [300.0, -1400.0], [0.0, 1200.0], [0.0, -1200.0]],
    },
    Region {
        key: "kokiri",
        name: "Kokiri Forest",
        blurb: "A mossy clearing beside the pond, open and flat; the rim is invisible",
        course: "kokiri_forest",
        centre: [-8600.0, 3000.0, 100.0],
        floor: None,
        half: [1100.0, 1100.0],
        void_y: -500.0,
        pits: &[],
        barrage: 190,
        spawns: [[-750.0, -750.0], [750.0, 750.0], [750.0, -750.0], [-750.0, 750.0]],
        gems: &[[-300.0, 0.0], [300.0, 0.0], [0.0, 300.0], [0.0, -300.0], [-650.0, 250.0], [650.0, -250.0]],
        items: &[[-450.0, -450.0], [450.0, 450.0], [-450.0, 450.0], [450.0, -450.0], [0.0, 700.0], [0.0, -700.0], [-800.0, 0.0], [800.0, 0.0]],
    },
    Region {
        key: "whomp",
        name: "Whomp King's Arena",
        blurb: "The Whomp King's stone deck high above the fortress: cliffs on every side, and he watches",
        course: "wf",
        centre: [250.0, 4184.0, 500.0],
        floor: Some(3584.0),
        half: [1100.0, 900.0],
        void_y: -300.0,
        pits: &[],
        barrage: 150,
        spawns: [[-748.0, -558.0], [748.0, 558.0], [748.0, -558.0], [-748.0, 558.0]],
        gems: &[[-495.0, 0.0], [495.0, 0.0], [0.0, -405.0], [0.0, 405.0], [-825.0, 315.0], [825.0, -315.0]],
        items: &[[-330.0, -270.0], [330.0, 270.0], [-330.0, 270.0], [330.0, -270.0], [0.0, 720.0], [0.0, -720.0], [-935.0, 0.0], [935.0, 0.0]],
    },
    Region {
        key: "sand",
        name: "Shifting Sand Pit",
        blurb: "A walled sand pit in the shadow of the pyramid: stone walls on every side, so launches end in a splat",
        course: "ssl",
        centre: [-217.0, 345.0, -6166.0],
        floor: Some(-255.0),
        half: [680.0, 440.0],
        void_y: -500.0,
        pits: &[],
        barrage: 160,
        spawns: [[-462.0, -273.0], [462.0, 273.0], [462.0, -273.0], [-462.0, 273.0]],
        gems: &[[-306.0, 0.0], [306.0, 0.0], [0.0, -198.0], [0.0, 198.0], [-510.0, 154.0], [510.0, -154.0]],
        items: &[[-204.0, -132.0], [204.0, 132.0], [-204.0, 132.0], [204.0, -132.0], [0.0, 352.0], [0.0, -352.0], [-578.0, 0.0], [578.0, 0.0]],
    },
    Region {
        key: "island",
        name: "Tiny-Huge Island",
        blurb: "A grassy shelf of the island: open lawn and a fall to the water below",
        course: "thi",
        centre: [-3991.0, 89.0, 2509.0],
        floor: Some(-511.0),
        half: [1100.0, 1000.0],
        void_y: -500.0,
        pits: &[],
        barrage: 170,
        spawns: [[-748.0, -620.0], [748.0, 620.0], [748.0, -620.0], [-748.0, 620.0]],
        gems: &[[-495.0, 0.0], [495.0, 0.0], [0.0, -450.0], [0.0, 450.0], [-825.0, 350.0], [825.0, -350.0]],
        items: &[[-330.0, -300.0], [330.0, 300.0], [-330.0, 300.0], [330.0, -300.0], [0.0, 800.0], [0.0, -800.0], [-935.0, 0.0], [935.0, 0.0]],
    },
    Region {
        key: "rainbow",
        name: "Rainbow Ride Deck",
        blurb: "A checkered deck in the sky with a rainbow overhead: nothing under the edge but a very long fall",
        course: "rr",
        centre: [-4591.0, 3672.0, -4691.0],
        floor: Some(3072.0),
        half: [1100.0, 1000.0],
        void_y: -400.0,
        pits: &[],
        barrage: 140,
        spawns: [[-748.0, -620.0], [818.0, 620.0], [748.0, -620.0], [-748.0, 620.0]],
        gems: &[[-495.0, 0.0], [495.0, 0.0], [0.0, -450.0], [-63.0, 480.0], [-825.0, 350.0], [825.0, -350.0]],
        items: &[[-330.0, -300.0], [330.0, 300.0], [-330.0, 300.0], [330.0, -300.0], [-63.0, 830.0], [0.0, -800.0], [-935.0, 0.0], [935.0, 0.0]],
    },
    Region {
        key: "ranch",
        name: "Lon Lon Ranch",
        blurb: "The wide paddock: nowhere to hide, so every hit counts",
        course: "lon_lon_ranch",
        centre: [8920.0, 600.0, 11683.0],
        floor: Some(0.0),
        half: [1200.0, 1100.0],
        void_y: -500.0,
        pits: &[],
        barrage: 200,
        spawns: [[-816.0, -682.0], [816.0, 682.0], [816.0, -682.0], [-816.0, 682.0]],
        gems: &[[-540.0, 0.0], [540.0, 0.0], [0.0, -495.0], [0.0, 495.0], [-900.0, 385.0], [900.0, -385.0]],
        items: &[[-360.0, -330.0], [360.0, 330.0], [-360.0, 330.0], [360.0, -330.0], [0.0, 880.0], [0.0, -880.0], [-1020.0, 0.0], [1020.0, 0.0]],
    },
    Region {
        key: "graves",
        name: "Kakariko Graveyard",
        blurb: "Headstones and dry earth between the walls",
        course: "graveyard",
        centre: [573.0, 1132.0, -357.0],
        floor: Some(532.0),
        half: [800.0, 700.0],
        void_y: -500.0,
        pits: &[],
        barrage: 170,
        spawns: [[-544.0, -434.0], [544.0, 434.0], [544.0, -434.0], [-544.0, 434.0]],
        gems: &[[-360.0, 0.0], [360.0, 0.0], [0.0, -315.0], [0.0, 315.0], [-600.0, 245.0], [600.0, -245.0]],
        items: &[[-240.0, -210.0], [240.0, 210.0], [-240.0, 210.0], [240.0, -210.0], [0.0, 560.0], [0.0, -560.0], [-680.0, 0.0], [680.0, 0.0]],
    },
    Region {
        key: "whomp2",
        name: "Whomp's Fortress Courtyard",
        blurb: "The lower yard of the fortress: stone slabs, walls and a drop to the lawns",
        course: "wf",
        centre: [4209.0, 856.0, 4909.0],
        floor: Some(256.0),
        half: [800.0, 700.0],
        void_y: -400.0,
        pits: &[],
        barrage: 150,
        spawns: [[-544.0, -434.0], [544.0, 434.0], [544.0, -434.0], [-544.0, 434.0]],
        gems: &[[-360.0, 0.0], [360.0, 0.0], [0.0, -315.0], [0.0, 315.0], [-600.0, 245.0], [600.0, -245.0]],
        items: &[[-240.0, -210.0], [240.0, 210.0], [-240.0, 210.0], [240.0, -210.0], [0.0, 560.0], [0.0, -560.0], [-680.0, 0.0], [680.0, 0.0]],
    },
    Region {
        key: "bob2",
        name: "Bob-omb Ridge",
        blurb: "A grassy ridge on the mountain path, the slope falling away on both sides",
        course: "bob",
        centre: [-6391.0, 1624.0, -291.0],
        floor: Some(1024.0),
        half: [800.0, 700.0],
        void_y: -500.0,
        pits: &[],
        barrage: 150,
        spawns: [[-544.0, -434.0], [544.0, 434.0], [544.0, -434.0], [-544.0, 434.0]],
        gems: &[[-360.0, 0.0], [360.0, 0.0], [0.0, -315.0], [0.0, 315.0], [-600.0, 245.0], [600.0, -245.0]],
        items: &[[-240.0, -210.0], [240.0, 210.0], [-240.0, 210.0], [240.0, -210.0], [0.0, 560.0], [0.0, -560.0], [-680.0, 0.0], [680.0, 0.0]],
    },
    Region {
        key: "bob3",
        name: "Bob-omb Lower Slope",
        blurb: "The lower slope below the summit: open grass and chain-chomp country",
        course: "bob",
        centre: [-6391.0, 1368.0, 3309.0],
        floor: Some(768.0),
        half: [800.0, 700.0],
        void_y: -500.0,
        pits: &[],
        barrage: 160,
        spawns: [[-544.0, -434.0], [544.0, 434.0], [544.0, -434.0], [-544.0, 434.0]],
        gems: &[[-360.0, 0.0], [360.0, 0.0], [0.0, -315.0], [0.0, 315.0], [-600.0, 245.0], [600.0, -245.0]],
        items: &[[-240.0, -210.0], [240.0, 210.0], [-240.0, 210.0], [240.0, -210.0], [0.0, 560.0], [0.0, -560.0], [-680.0, 0.0], [680.0, 0.0]],
    },
    Region {
        key: "kokiri2",
        name: "Kokiri Hollow",
        blurb: "A quiet hollow among the trees, away from the pond",
        course: "kokiri_forest",
        centre: [-11423.0, 864.0, -2939.0],
        floor: Some(264.0),
        half: [800.0, 700.0],
        void_y: -500.0,
        pits: &[],
        barrage: 190,
        spawns: [[-544.0, -434.0], [544.0, 434.0], [544.0, -434.0], [-544.0, 434.0]],
        gems: &[[-360.0, 0.0], [360.0, 0.0], [0.0, -315.0], [0.0, 315.0], [-600.0, 245.0], [600.0, -245.0]],
        items: &[[-240.0, -210.0], [240.0, 210.0], [-240.0, 210.0], [240.0, -210.0], [0.0, 560.0], [0.0, -560.0], [-680.0, 0.0], [680.0, 0.0]],
    },
    Region {
        key: "rainbow2",
        name: "Rainbow Ride Island",
        blurb: "A second floating island in the sky: smaller, windier, just as far to fall",
        course: "rr",
        centre: [4809.0, 4419.0, 3709.0],
        floor: Some(3819.0),
        half: [800.0, 700.0],
        void_y: -400.0,
        pits: &[],
        barrage: 130,
        spawns: [[-474.0, -434.0], [544.0, 434.0], [544.0, -434.0], [-544.0, 434.0]],
        gems: &[[-360.0, 0.0], [360.0, 0.0], [0.0, -315.0], [0.0, 315.0], [-600.0, 245.0], [600.0, -245.0]],
        items: &[[-240.0, -210.0], [240.0, 210.0], [-240.0, 210.0], [240.0, -210.0], [0.0, 560.0], [0.0, -560.0], [-680.0, 0.0], [680.0, 0.0]],
    },
    Region {
        key: "sand2",
        name: "Shifting Sand Dunes",
        blurb: "A raised stretch of the dunes under the pyramid, ringed by drops",
        course: "ssl",
        centre: [5953.0, 1214.0, 3909.0],
        floor: Some(614.0),
        half: [800.0, 700.0],
        void_y: -500.0,
        pits: &[],
        barrage: 150,
        spawns: [[-544.0, -434.0], [544.0, 434.0], [544.0, -434.0], [-544.0, 434.0]],
        gems: &[[-360.0, 0.0], [360.0, 0.0], [0.0, -315.0], [0.0, 315.0], [-600.0, 245.0], [600.0, -245.0]],
        items: &[[-240.0, -210.0], [240.0, 210.0], [-240.0, 210.0], [240.0, -210.0], [0.0, 560.0], [0.0, -560.0], [-680.0, 0.0], [680.0, 0.0]],
    },
    Region {
        key: "island2",
        name: "Tiny-Huge Shore",
        blurb: "The island shore: soft sand and the water close by",
        course: "thi",
        centre: [-6391.0, -2369.0, 6309.0],
        floor: Some(-2969.0),
        half: [900.0, 800.0],
        void_y: -600.0,
        pits: &[],
        barrage: 170,
        spawns: [[-612.0, -496.0], [612.0, 496.0], [612.0, -496.0], [-612.0, 496.0]],
        gems: &[[-405.0, 0.0], [405.0, 0.0], [0.0, -360.0], [0.0, 360.0], [-675.0, 280.0], [675.0, -280.0]],
        items: &[[-270.0, -240.0], [270.0, 240.0], [-270.0, 240.0], [270.0, -240.0], [0.0, 640.0], [0.0, -640.0], [-765.0, 0.0], [765.0, 0.0]],
    },
    Region {
        key: "ranch2",
        name: "Lon Lon Back Field",
        blurb: "The back field of the ranch: wide open grass",
        course: "lon_lon_ranch",
        centre: [-8880.0, 600.0, 11683.0],
        floor: Some(0.0),
        half: [900.0, 800.0],
        void_y: -500.0,
        pits: &[],
        barrage: 190,
        spawns: [[-612.0, -496.0], [612.0, 496.0], [612.0, -496.0], [-612.0, 496.0]],
        gems: &[[-405.0, 0.0], [405.0, 0.0], [0.0, -360.0], [0.0, 360.0], [-675.0, 280.0], [675.0, -280.0]],
        items: &[[-270.0, -240.0], [270.0, 240.0], [-270.0, 240.0], [270.0, -240.0], [0.0, 640.0], [0.0, -640.0], [-765.0, 0.0], [765.0, 0.0]],
    },
    Region {
        key: "ranch3",
        name: "Lon Lon Far Field",
        blurb: "The far field of the ranch, under a big sky",
        course: "lon_lon_ranch",
        centre: [-8880.0, 600.0, -6317.0],
        floor: Some(0.0),
        half: [900.0, 800.0],
        void_y: -500.0,
        pits: &[],
        barrage: 190,
        spawns: [[-612.0, -496.0], [612.0, 496.0], [612.0, -496.0], [-612.0, 496.0]],
        gems: &[[-405.0, 0.0], [405.0, 0.0], [0.0, -360.0], [0.0, 360.0], [-675.0, 280.0], [675.0, -280.0]],
        items: &[[-270.0, -240.0], [270.0, 240.0], [-270.0, 240.0], [270.0, -240.0], [0.0, 640.0], [0.0, -640.0], [-765.0, 0.0], [765.0, 0.0]],
    },
];

/// Menu entries: one per real-world region (the six made-up box arenas are gone from the menus).
pub fn arena_count() -> usize {
    REGIONS.len()
}

/// The region a menu arena index picks.
pub fn region_of(arena: u8) -> Option<&'static Region> {
    REGIONS.get(arena as usize)
}

pub fn arena_name(arena: u8) -> &'static str {
    REGIONS[arena as usize % REGIONS.len()].name
}

pub fn arena_blurb(arena: u8) -> &'static str {
    REGIONS[arena as usize % REGIONS.len()].blurb
}

/// The regions of the course `arena` lives in, the picked one first: the stages of a Gauntlet there (stage `k` plays on
/// element `k % len`).
pub fn course_regions(arena: u8) -> Vec<&'static Region> {
    let Some(first) = region_of(arena) else { return vec![] };
    let mut v = vec![first];
    v.extend(REGIONS.iter().filter(|r| r.course == first.course && r.key != first.key));
    v
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
        a.barrage = self.barrage;
        a.barrage_cd = self.barrage;
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

    /// Exploration aid (ignored): rank flat, walkable rectangles of a course that make a good arena (flat floor, edges that are
    /// cliffs or walls). `FIND_COURSE=ccm FIND_HX=1200 FIND_HZ=1100 cargo test find_arenas -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn find_arenas() {
        let env = |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let id = std::env::var("FIND_COURSE").unwrap_or_else(|_| "bob".into());
        let Some(world) = course_world(&id) else { return };
        let (hx, hz) = (env("FIND_HX", 1200.0), env("FIND_HZ", 1100.0));
        let (mut x0, mut x1, mut z0, mut z1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for s in world.surfaces() {
            for v in [s.v1, s.v2, s.v3] {
                x0 = x0.min(v[0]);
                x1 = x1.max(v[0]);
                z0 = z0.min(v[2]);
                z1 = z1.max(v[2]);
            }
        }
        let g = WorldGround::new(world.clone(), [0.0; 3]);
        let step = 200.0;
        let cell = 100.0;
        let mut found: Vec<(f64, f64, f64, f64, f64)> = vec![];
        let (mut cx, mut cz);
        cz = z0 + hz;
        while cz < z1 - hz {
            cx = x0 + hx;
            while cx < x1 - hx {
                // Every floor level under the centre is a candidate reference height.
                let mut q = 20000.0;
                for _ in 0..6 {
                    let Some(h) = g.course_hit(cx, cz, q) else { break };
                    q = h.height - 79.0;
                    let y0 = h.height;
                    if h.surface.as_ref().is_none_or(|s| s.normal[1] < WALKABLE_NY || matches!(s.kind, SURFACE_BURNING | SURFACE_DEATH_PLANE)) {
                        continue;
                    }
                    let (mut ok, mut total, mut edge_ok, mut edge_total) = (0, 0, 0, 0);
                    let (nx, nz) = ((hx / cell) as i32 + 2, (hz / cell) as i32 + 2);
                    for iz in -nz..=nz {
                        for ix in -nx..=nx {
                            let (x, z) = (cx + ix as f64 * cell, cz + iz as f64 * cell);
                            let inside = ix.abs() as f64 * cell <= hx && iz.abs() as f64 * cell <= hz;
                            let near_edge = (ix.abs() as f64 * cell - hx).abs() <= cell && iz.abs() as f64 * cell <= hz + cell || (iz.abs() as f64 * cell - hz).abs() <= cell && ix.abs() as f64 * cell <= hx + cell;
                            let f = g.course_hit(x, z, y0 + 60.0);
                            let flat = f.as_ref().is_some_and(|h| (h.height - y0).abs() <= 40.0 && h.surface.as_ref().is_some_and(|s| s.normal[1] >= WALKABLE_NY && !matches!(s.kind, SURFACE_BURNING | SURFACE_DEATH_PLANE)));
                            if inside {
                                total += 1;
                                if flat {
                                    ok += 1;
                                }
                            } else if near_edge {
                                edge_total += 1;
                                // A cliff (nothing, or far lower) or a wall counts as a good edge.
                                let drop = f.as_ref().is_none_or(|h| h.height < y0 - 150.0);
                                let mut w = [x, y0 + 40.0, z];
                                let wall = g.world.resolve_and_return_wall_collisions(&mut w, WALL_OFFSET, 35.0).is_some();
                                if drop || wall {
                                    edge_ok += 1;
                                }
                            }
                        }
                    }
                    let cover = ok as f64 / total.max(1) as f64;
                    let edge = edge_ok as f64 / edge_total.max(1) as f64;
                    if cover >= 0.9 && edge >= 0.5 {
                        found.push((cover * edge, cx, cz, y0, cover));
                    }
                }
                cx += step;
            }
            cz += step;
        }
        found.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let mut shown: Vec<(f64, f64, f64)> = vec![];
        for (sc, x, z, y, cov) in found {
            if shown.iter().any(|s| (s.0 - x).abs() < 2.0 * hx && (s.1 - z).abs() < 2.0 * hz && (s.2 - y).abs() < 300.0) {
                continue;
            }
            println!("{id}: centre ({x:.0}, {z:.0}) floor {y:.0} score {sc:.2} flat {cov:.2}");
            shown.push((x, z, y));
            if shown.len() >= 8 {
                break;
            }
        }
    }

    /// Exploration aid (ignored): the spawn / gem / item lists of a new region, laid out on a standard pattern and snapped to
    /// standable ground. `SUG_COURSE=wf SUG_X=209 SUG_Z=509 SUG_FLOOR=3584 SUG_HX=1100 SUG_HZ=900 cargo test suggest_region -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn suggest_region() {
        let env = |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let id = std::env::var("SUG_COURSE").unwrap_or_else(|_| "bob".into());
        let Some(world) = course_world(&id) else { return };
        let (cx, cz, fl, hx, hz) = (env("SUG_X", 0.0), env("SUG_Z", 0.0), env("SUG_FLOOR", 0.0), env("SUG_HX", 1100.0) as f32, env("SUG_HZ", 1000.0) as f32);
        let mut g = WorldGround::new(world, [cx, fl + 100.0, cz]);
        g.origin[1] = fl;
        let snap = |x: f32, z: f32| -> Option<V2> {
            for ring in 0..8 {
                for k in 0..(1 + ring * 6) {
                    let a = k as f32 / (1 + ring * 6) as f32 * std::f32::consts::TAU;
                    let (px, pz) = (x + a.cos() * ring as f32 * 70.0, z + a.sin() * ring as f32 * 70.0);
                    if px.abs() > hx * 0.95 || pz.abs() > hz * 0.95 {
                        continue;
                    }
                    let Some(y) = g.floor(px, pz, 600.0) else { continue };
                    if y.abs() <= 60.0 && g.standable(px, y, pz) {
                        return Some([px.round(), pz.round()]);
                    }
                }
            }
            None
        };
        let list = |name: &str, pts: &[[f32; 2]]| {
            let v: Vec<String> = pts.iter().map(|p| match snap(p[0] * hx, p[1] * hz) { Some(s) => format!("[{:.1}, {:.1}]", s[0], s[1]), None => "[MISSING]".into() }).collect();
            println!("        {name}: &[{}],", v.join(", "));
        };
        println!("        centre: [{cx:.1}, {:.1}, {cz:.1}], floor: Some({fl:.1}), half: [{hx:.1}, {hz:.1}],", fl + 600.0);
        list("spawns", &[[-0.68, -0.62], [0.68, 0.62], [0.68, -0.62], [-0.68, 0.62]]);
        list("gems", &[[-0.45, 0.0], [0.45, 0.0], [0.0, -0.45], [0.0, 0.45], [-0.75, 0.35], [0.75, -0.35]]);
        list("items", &[[-0.3, -0.3], [0.3, 0.3], [-0.3, 0.3], [0.3, -0.3], [0.0, 0.8], [0.0, -0.8], [-0.85, 0.0], [0.85, 0.0]]);
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
