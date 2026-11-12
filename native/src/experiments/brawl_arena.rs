//! Arena Brawl, arenas (engine-free): the seeded RNG, vector helpers and the six small
//! multi-level arenas with their set pieces (lifts, a lava tide, bumper pillars, a crumbling
//! bridge, gusting wind, a central cannon). All state is plain data advanced by [`Arena::step`];
//! the rules live in `brawl_core.rs`, the pictures in `brawl.rs` / `brawl_view.rs`.
//!
//! Units are SM64 units, +y up, the arena is centred on the origin and the main floor's top is
//! y = 0. A fighter stands on the highest solid top under its feet that is at most `STEP` above
//! them; taller solids are walls to anything standing lower (so ledges need a jump).

pub const DT: f32 = 1.0 / 30.0;
/// Highest ledge a walking fighter steps up (and the drop it follows down without falling).
pub const STEP: f32 = 34.0;

/// Splitmix64: tiny, fast, identical on every platform (determinism by seed).
#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }
    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u32
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn below(&mut self, n: usize) -> usize {
        self.next() as usize % n.max(1)
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
}

pub type V2 = [f32; 2];

pub fn len2(v: V2) -> f32 {
    v[0].hypot(v[1])
}
pub fn dist2(a: V2, b: V2) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
/// Unit vector, or `fallback` for a (near) zero vector.
pub fn norm2(v: V2, fallback: V2) -> V2 {
    let l = len2(v);
    if l < 1e-4 { fallback } else { [v[0] / l, v[1] / l] }
}
pub fn dir_of(yaw: f32) -> V2 {
    [yaw.sin(), yaw.cos()]
}
pub fn yaw_of(d: V2) -> f32 {
    d[0].atan2(d[1])
}
/// Shortest signed angle from `a` to `b`.
pub fn angle_diff(a: f32, b: f32) -> f32 {
    let mut d = (b - a) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}
pub fn approach(v: f32, target: f32, step: f32) -> f32 {
    if v < target { (v + step).min(target) } else { (v - step).max(target) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArenaKind {
    Gearworks,
    Ember,
    Bumpers,
    Bridge,
    Gale,
    Cannon,
    /// A stretch of a real course (see `brawl_region.rs`); the floors and walls come from its collision.
    Field,
}

pub const ARENAS: [ArenaKind; 6] = [ArenaKind::Gearworks, ArenaKind::Ember, ArenaKind::Bumpers, ArenaKind::Bridge, ArenaKind::Gale, ArenaKind::Cannon];

impl ArenaKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Gearworks => "Gearworks",
            Self::Ember => "Ember Basin",
            Self::Bumpers => "Bumper Garden",
            Self::Bridge => "Splitrock Bridge",
            Self::Gale => "Gale Summit",
            Self::Cannon => "Cannon Court",
            Self::Field => "Real world",
        }
    }
    pub fn hazard(self) -> &'static str {
        match self {
            Self::Gearworks => "Moving lifts and sliders; no walls, mind the drop",
            Self::Ember => "A lava tide rises and falls, stand on the rocks",
            Self::Bumpers => "Bumper pillars fling fighters and items away",
            Self::Bridge => "The plank bridge crumbles under your feet",
            Self::Gale => "Gusting wind shoves everything toward the edge",
            Self::Cannon => "The central cannon shells whoever is losing",
            Self::Field => "A real place: its own ground, walls and ledges",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Gearworks => "gear",
            Self::Ember => "ember",
            Self::Bumpers => "bumper",
            Self::Bridge => "bridge",
            Self::Gale => "gale",
            Self::Cannon => "cannon",
            Self::Field => "field",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.to_ascii_lowercase();
        ARENAS.iter().copied().find(|a| a.key() == s) .or_else(|| s.parse::<usize>().ok().and_then(|i| ARENAS.get(i).copied()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mover {
    None,
    /// top = base + amp * (1 + sin) / 2 ... a vertical lift between `base_top` and `base_top + amp`.
    Lift { amp: f32, period: f32, phase: f32 },
    /// Slides along `axis` by +-amp around its base position.
    Slide { axis: V2, amp: f32, period: f32, phase: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plank {
    Solid,
    /// Shaking before it drops (ticks left).
    Shaking(u16),
    /// Gone; ticks until it is back.
    Fallen(u16),
}

#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub x: f32,
    pub z: f32,
    pub hx: f32,
    pub hz: f32,
    pub top: f32,
    pub base_x: f32,
    pub base_z: f32,
    pub base_top: f32,
    pub mover: Mover,
    pub crumble: bool,
    pub plank: Plank,
    /// Ticks something has stood on it (crumbling planks).
    pub stood: u16,
    /// Motion since the last tick (carries riders along).
    pub d: [f32; 3],
    /// Visual style: 0 stone floor, 1 brick, 2 planks, 3 metal/gold, 4 rock, 5 sand, 6 tiles.
    pub style: u8,
}

impl Solid {
    pub fn new(x: f32, z: f32, hx: f32, hz: f32, top: f32, style: u8) -> Self {
        Self { x, z, hx, hz, top, base_x: x, base_z: z, base_top: top, mover: Mover::None, crumble: false, plank: Plank::Solid, stood: 0, d: [0.0; 3], style }
    }
    pub fn active(&self) -> bool {
        !matches!(self.plank, Plank::Fallen(_))
    }
    fn with(mut self, m: Mover) -> Self {
        self.mover = m;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Bumper {
    pub x: f32,
    pub z: f32,
    pub r: f32,
    pub h: f32,
    /// Ticks of "just hit" flash.
    pub flash: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LavaPhase {
    Low,
    /// Glowing, about to rise.
    Warn,
    Rising,
    High,
    Falling,
}

/// The ground of an arena that is a stretch of a real course instead of a set of boxes. All
/// coordinates are arena-local (origin = the region's centre, y = 0 its main floor); the
/// implementation translates to the course. Engine-free: `brawl_region::WorldGround` wraps a
/// `CollisionWorld`, the tests use plain heightfields.
pub trait Ground: Send + Sync + std::fmt::Debug {
    /// Height of the highest floor under (x, z) that is at most `max_y`.
    fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<f32>;
    /// Push a standing circle (radius `r`, feet at `p[1]`) out of every wall.
    fn push(&self, p: &mut [f32; 3], r: f32);
    /// Can something rest at (x, y, z)? Flat enough, dry, not lava, death or quicksand.
    fn standable(&self, x: f32, y: f32, z: f32) -> bool;
    /// Where the arena origin sits in the world the picture is drawn in (zero for the box arenas).
    fn origin(&self) -> [f32; 3] {
        [0.0; 3]
    }
}

#[derive(Clone, Debug)]
pub struct Arena {
    /// The real ground, when this arena is a region of a course (then `solids` is empty).
    pub ground: Option<std::sync::Arc<dyn Ground>>,
    /// Highest local height that counts as "the floor" when looking for it from above.
    pub ceil_y: f32,
    pub kind: ArenaKind,
    pub solids: Vec<Solid>,
    pub bumpers: Vec<Bumper>,
    pub spawns: [V2; 4],
    pub item_spots: Vec<[f32; 3]>,
    pub gem_spots: Vec<[f32; 3]>,
    /// Invisible rim: positions are clamped inside +-bounds (None = open edges, you can fall off).
    pub bounds: Option<V2>,
    /// Half extents of the main floor (camera framing, random drops).
    pub half: V2,
    pub void_y: f32,
    pub t: f32,
    pub lava: f32,
    pub lava_phase: LavaPhase,
    /// Wind acceleration direction (unit) and strength 0..1 (0 calm), plus the warning 0..1 before a gust.
    pub wind_dir: V2,
    pub wind: f32,
    pub wind_warn: f32,
    wind_t: f32,
    pub cannon_yaw: f32,
    /// Ticks until the cannon fires.
    pub cannon_cd: i32,
    pub has_cannon: bool,
    /// Real-world regions: ticks between shell strikes on the fighters once the round is under way (0 = calm), and the countdown.
    pub barrage: i32,
    pub barrage_cd: i32,
}

pub const LAVA_LOW: f32 = -70.0;
pub const LAVA_HIGH: f32 = 62.0;
pub const LAVA_CYCLE: f32 = 26.0;

impl Arena {
    pub fn build(kind: ArenaKind) -> Self {
        let mut a = Arena {
            ground: None,
            ceil_y: 1000.0,
            kind,
            solids: vec![],
            bumpers: vec![],
            spawns: [[0.0; 2]; 4],
            item_spots: vec![],
            gem_spots: vec![],
            bounds: None,
            half: [850.0, 850.0],
            void_y: -520.0,
            t: 0.0,
            lava: f32::NEG_INFINITY,
            lava_phase: LavaPhase::Low,
            wind_dir: [1.0, 0.0],
            wind: 0.0,
            wind_warn: 0.0,
            wind_t: 6.0,
            cannon_yaw: 0.0,
            cannon_cd: 120,
            has_cannon: false,
            barrage: 0,
            barrage_cd: 0,
        };
        let corners = |r: f32| [[-r, -r], [r, -r], [-r, r], [r, r]];
        match kind {
            ArenaKind::Gearworks => {
                a.half = [850.0, 850.0];
                a.solids.push(Solid::new(0.0, 0.0, 850.0, 850.0, 0.0, 6));
                // A mid island and two long gantries reached by lifts; sliders bridge the island.
                a.solids.push(Solid::new(0.0, 0.0, 190.0, 190.0, 130.0, 3));
                a.solids.push(Solid::new(0.0, -700.0, 780.0, 130.0, 300.0, 3));
                a.solids.push(Solid::new(0.0, 700.0, 780.0, 130.0, 300.0, 3));
                a.solids.push(Solid::new(-600.0, -450.0, 110.0, 110.0, 0.0, 1).with(Mover::Lift { amp: 300.0, period: 7.0, phase: 0.0 }));
                a.solids.push(Solid::new(600.0, 450.0, 110.0, 110.0, 0.0, 1).with(Mover::Lift { amp: 300.0, period: 7.0, phase: 3.5 }));
                a.solids.push(Solid::new(-400.0, 300.0, 110.0, 110.0, 130.0, 1).with(Mover::Slide { axis: [1.0, 0.0], amp: 250.0, period: 6.0, phase: 0.0 }));
                a.solids.push(Solid::new(400.0, -300.0, 110.0, 110.0, 130.0, 1).with(Mover::Slide { axis: [-1.0, 0.0], amp: 250.0, period: 6.0, phase: 0.0 }));
                a.spawns = [[-650.0, 0.0], [650.0, 0.0], [0.0, -420.0], [0.0, 420.0]];
                a.item_spots = vec![[-500.0, 0.0, 0.0], [500.0, 0.0, 0.0], [0.0, 0.0, 130.0], [-250.0, 0.0, -300.0], [250.0, 0.0, 300.0], [-300.0, 0.0, 580.0], [300.0, 0.0, -580.0], [0.0, 300.0, -700.0]];
                a.gem_spots = vec![[-300.0, 0.0, 0.0], [300.0, 0.0, 0.0], [0.0, 0.0, 130.0], [-640.0, 0.0, 300.0], [640.0, 0.0, -300.0], [0.0, 300.0, -700.0]];
            }
            ArenaKind::Ember => {
                a.half = [800.0, 800.0];
                a.bounds = Some([800.0, 800.0]);
                a.solids.push(Solid::new(0.0, 0.0, 800.0, 800.0, 0.0, 5));
                a.solids.push(Solid::new(0.0, 0.0, 160.0, 160.0, 210.0, 4));
                a.solids.push(Solid::new(0.0, -300.0, 130.0, 100.0, 105.0, 4));
                a.solids.push(Solid::new(0.0, 300.0, 130.0, 100.0, 105.0, 4));
                for c in corners(470.0) {
                    a.solids.push(Solid::new(c[0], c[1], 140.0, 140.0, 140.0, 4));
                }
                a.spawns = [[-640.0, -640.0], [640.0, 640.0], [640.0, -640.0], [-640.0, 640.0]];
                a.item_spots = vec![[-470.0, 140.0, -470.0], [470.0, 140.0, 470.0], [470.0, 140.0, -470.0], [-470.0, 140.0, 470.0], [0.0, 210.0, 0.0], [-650.0, 0.0, 0.0], [650.0, 0.0, 0.0], [0.0, 0.0, -650.0]];
                a.gem_spots = vec![[-470.0, 140.0, -470.0], [470.0, 140.0, 470.0], [0.0, 210.0, 0.0], [0.0, 105.0, -300.0], [0.0, 105.0, 300.0], [470.0, 140.0, -470.0]];
                a.lava = LAVA_LOW;
            }
            ArenaKind::Bumpers => {
                a.half = [850.0, 850.0];
                a.bounds = Some([850.0, 850.0]);
                a.solids.push(Solid::new(0.0, 0.0, 850.0, 850.0, 0.0, 0));
                for (x, z, hx, hz) in [(-740.0, 0.0, 110.0, 340.0), (740.0, 0.0, 110.0, 340.0), (0.0, -740.0, 340.0, 110.0), (0.0, 740.0, 340.0, 110.0)] {
                    a.solids.push(Solid::new(x, z, hx, hz, 110.0, 1));
                }
                a.bumpers.push(Bumper { x: 0.0, z: 0.0, r: 100.0, h: 150.0, flash: 0 });
                for c in corners(430.0) {
                    a.bumpers.push(Bumper { x: c[0], z: c[1], r: 80.0, h: 130.0, flash: 0 });
                }
                a.spawns = [[-560.0, 0.0], [560.0, 0.0], [0.0, -560.0], [0.0, 560.0]];
                a.item_spots = vec![[-740.0, 110.0, 0.0], [740.0, 110.0, 0.0], [0.0, 110.0, -740.0], [0.0, 110.0, 740.0], [-250.0, 0.0, 250.0], [250.0, 0.0, -250.0], [-250.0, 0.0, -250.0], [250.0, 0.0, 250.0]];
                a.gem_spots = vec![[-740.0, 110.0, 0.0], [740.0, 110.0, 0.0], [0.0, 110.0, -740.0], [0.0, 110.0, 740.0], [-250.0, 0.0, 0.0], [250.0, 0.0, 0.0]];
            }
            ArenaKind::Bridge => {
                a.half = [980.0, 420.0];
                a.bounds = Some([980.0, 420.0]);
                a.solids.push(Solid::new(-690.0, 0.0, 290.0, 420.0, 90.0, 4));
                a.solids.push(Solid::new(690.0, 0.0, 290.0, 420.0, 90.0, 4));
                a.solids.push(Solid::new(-860.0, 0.0, 120.0, 160.0, 230.0, 1));
                a.solids.push(Solid::new(860.0, 0.0, 120.0, 160.0, 230.0, 1));
                // Seven planks across the chasm.
                for i in 0..7 {
                    let mut p = Solid::new(-258.0 + i as f32 * 86.0, 0.0, 43.0, 100.0, 90.0, 2);
                    p.crumble = true;
                    a.solids.push(p);
                }
                a.spawns = [[-720.0, -300.0], [720.0, 300.0], [-720.0, 300.0], [720.0, -300.0]];
                a.item_spots = vec![[-700.0, 90.0, -250.0], [700.0, 90.0, 250.0], [-700.0, 90.0, 250.0], [700.0, 90.0, -250.0], [-860.0, 230.0, 0.0], [860.0, 230.0, 0.0], [-420.0, 90.0, 0.0], [420.0, 90.0, 0.0]];
                a.gem_spots = vec![[-860.0, 230.0, 0.0], [860.0, 230.0, 0.0], [0.0, 90.0, 0.0], [-520.0, 90.0, 0.0], [520.0, 90.0, 0.0], [-700.0, 90.0, 300.0]];
                a.void_y = -480.0;
            }
            ArenaKind::Gale => {
                a.half = [800.0, 800.0];
                a.solids.push(Solid::new(0.0, 0.0, 800.0, 800.0, 0.0, 0));
                a.solids.push(Solid::new(0.0, 0.0, 330.0, 330.0, 70.0, 5));
                a.solids.push(Solid::new(0.0, 0.0, 230.0, 230.0, 140.0, 5));
                a.solids.push(Solid::new(0.0, 0.0, 130.0, 130.0, 210.0, 1));
                a.spawns = [[-640.0, -640.0], [640.0, 640.0], [640.0, -640.0], [-640.0, 640.0]];
                a.item_spots = vec![[0.0, 0.0, 210.0], [-280.0, 70.0, 0.0], [280.0, 70.0, 0.0], [-600.0, 0.0, 0.0], [600.0, 0.0, 0.0], [0.0, 0.0, -600.0], [0.0, 0.0, 600.0], [-180.0, 140.0, 180.0]];
                a.gem_spots = vec![[0.0, 0.0, 210.0], [-600.0, 0.0, 400.0], [600.0, 0.0, -400.0], [-280.0, 70.0, 0.0], [280.0, 70.0, 0.0], [0.0, 0.0, 600.0]];
                a.wind_dir = [1.0, 0.0];
            }
            ArenaKind::Field => {
                // Filled in by `brawl_region`.
                a.item_spots = vec![[0.0; 3]];
                a.gem_spots = vec![[0.0; 3]];
            }
            ArenaKind::Cannon => {
                a.half = [850.0, 850.0];
                a.bounds = Some([850.0, 850.0]);
                a.has_cannon = true;
                a.solids.push(Solid::new(0.0, 0.0, 850.0, 850.0, 0.0, 1));
                a.solids.push(Solid::new(0.0, 0.0, 115.0, 115.0, 150.0, 3));
                for c in corners(600.0) {
                    a.solids.push(Solid::new(c[0], c[1], 190.0, 190.0, 110.0, 4));
                    a.solids.push(Solid::new(c[0] * 0.62, c[1] * 0.62, 60.0, 60.0, 55.0, 4));
                }
                a.spawns = [[-420.0, 0.0], [420.0, 0.0], [0.0, -420.0], [0.0, 420.0]];
                a.item_spots = vec![[-600.0, 110.0, -600.0], [600.0, 110.0, 600.0], [600.0, 110.0, -600.0], [-600.0, 110.0, 600.0], [-250.0, 0.0, 250.0], [250.0, 0.0, -250.0], [-250.0, 0.0, -250.0], [250.0, 0.0, 250.0]];
                a.gem_spots = vec![[-600.0, 110.0, -600.0], [600.0, 110.0, 600.0], [0.0, 150.0, 0.0], [-250.0, 0.0, 0.0], [250.0, 0.0, 0.0], [600.0, 110.0, -600.0]];
            }
        }
        a.step_movers(0.0);
        a
    }

    fn step_movers(&mut self, dt: f32) {
        let t = self.t;
        for s in &mut self.solids {
            let (ox, oy, oz) = (s.x, s.top, s.z);
            match s.mover {
                Mover::None => {}
                Mover::Lift { amp, period, phase } => {
                    s.top = s.base_top + amp * (1.0 + ((t + phase) / period * std::f32::consts::TAU).sin()) * 0.5;
                }
                Mover::Slide { axis, amp, period, phase } => {
                    let k = amp * ((t + phase) / period * std::f32::consts::TAU).sin();
                    s.x = s.base_x + axis[0] * k;
                    s.z = s.base_z + axis[1] * k;
                }
            }
            s.d = if dt > 0.0 { [s.x - ox, s.top - oy, s.z - oz] } else { [0.0; 3] };
        }
    }

    /// Advance set pieces by one tick. `stood` lists the solids currently carrying a fighter.
    pub fn step(&mut self, stood: &[bool]) {
        self.t += DT;
        self.step_movers(DT);
        for (i, s) in self.solids.iter_mut().enumerate() {
            if !s.crumble {
                continue;
            }
            let on = stood.get(i).copied().unwrap_or(false);
            match s.plank {
                Plank::Solid => {
                    if on {
                        s.stood += 1;
                        if s.stood >= 16 {
                            s.plank = Plank::Shaking(14);
                        }
                    } else if s.stood > 0 && self.t as i32 % 2 == 0 {
                        s.stood -= 1;
                    }
                }
                Plank::Shaking(n) => s.plank = if n <= 1 { Plank::Fallen(170) } else { Plank::Shaking(n - 1) },
                Plank::Fallen(n) => {
                    if n <= 1 {
                        s.plank = Plank::Solid;
                        s.stood = 0;
                    } else {
                        s.plank = Plank::Fallen(n - 1);
                    }
                }
            }
        }
        for b in &mut self.bumpers {
            b.flash = b.flash.saturating_sub(1);
        }
        if self.kind == ArenaKind::Ember {
            self.step_lava();
        }
        if self.kind == ArenaKind::Gale {
            self.step_wind();
        }
    }

    /// Where the tide is in its cycle: (level, phase). Pure function of the clock.
    pub fn lava_at(t: f32) -> (f32, LavaPhase) {
        let c = t % LAVA_CYCLE;
        let ease = |k: f32| k * k * (3.0 - 2.0 * k);
        if c < 9.0 {
            (LAVA_LOW, LavaPhase::Low)
        } else if c < 11.5 {
            (LAVA_LOW + 12.0 * ease((c - 9.0) / 2.5), LavaPhase::Warn)
        } else if c < 14.5 {
            let k = ease((c - 11.5) / 3.0);
            (LAVA_LOW + 12.0 + (LAVA_HIGH - LAVA_LOW - 12.0) * k, LavaPhase::Rising)
        } else if c < 21.5 {
            (LAVA_HIGH, LavaPhase::High)
        } else {
            let k = ease((c - 21.5) / 4.5);
            (LAVA_HIGH + (LAVA_LOW - LAVA_HIGH) * k, LavaPhase::Falling)
        }
    }

    fn step_lava(&mut self) {
        let (y, p) = Self::lava_at(self.t);
        self.lava = y;
        self.lava_phase = p;
    }

    fn step_wind(&mut self) {
        self.wind_t -= DT;
        // calm 5s, warning 1.6s, gust 5s; direction picked from the clock so it is reproducible.
        let c = self.t % 11.6;
        let n = (self.t / 11.6) as u32;
        let dirs: [V2; 4] = [[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], [0.0, -1.0]];
        self.wind_dir = dirs[((n.wrapping_mul(2654435761) >> 7) % 4) as usize];
        if c < 5.0 {
            self.wind = 0.0;
            self.wind_warn = 0.0;
        } else if c < 6.6 {
            self.wind = 0.0;
            self.wind_warn = (c - 5.0) / 1.6;
        } else {
            self.wind_warn = 0.0;
            let k = ((c - 6.6) / 0.5).min(1.0).min((11.6 - c) / 0.5);
            self.wind = k.clamp(0.0, 1.0);
        }
    }

    /// Highest active solid top under (x, z) that is at most `max_y`: (top, index).
    pub fn support(&self, x: f32, z: f32, max_y: f32) -> Option<(f32, usize)> {
        if let Some(g) = &self.ground {
            return g.floor(x, z, max_y).map(|y| (y, 0));
        }
        let mut best: Option<(f32, usize)> = None;
        for (i, s) in self.solids.iter().enumerate() {
            if !s.active() || s.top > max_y || (x - s.x).abs() > s.hx || (z - s.z).abs() > s.hz {
                continue;
            }
            if best.is_none_or(|(t, _)| s.top > t) {
                best = Some((s.top, i));
            }
        }
        best
    }

    /// Push a circle (radius `r`) standing at height `y` out of every solid that is a wall to it.
    pub fn push_out(&self, p: &mut [f32; 3], r: f32) {
        if let Some(g) = &self.ground {
            g.push(p, r);
        }
        for s in &self.solids {
            if !s.active() || s.top <= p[1] + STEP {
                continue;
            }
            let (dx, dz) = (p[0] - s.x, p[2] - s.z);
            let (px, pz) = (s.hx + r - dx.abs(), s.hz + r - dz.abs());
            if px > 0.0 && pz > 0.0 {
                if px < pz {
                    p[0] += px * if dx >= 0.0 { 1.0 } else { -1.0 };
                } else {
                    p[2] += pz * if dz >= 0.0 { 1.0 } else { -1.0 };
                }
            }
        }
        if let Some(b) = self.bounds {
            p[0] = p[0].clamp(-b[0] + r, b[0] - r);
            p[2] = p[2].clamp(-b[1] + r, b[1] - r);
        }
    }


    /// Is (x, z) over the footprint of any solid (a plank that has fallen still counts: the gap is
    /// part of the level)? Everything else is outside the playable area.
    pub fn over_floor(&self, x: f32, z: f32) -> bool {
        if let Some(g) = &self.ground {
            return self.bounds.is_none_or(|b| x.abs() <= b[0] && z.abs() <= b[1]) && g.floor(x, z, self.ceil_y).is_some();
        }
        self.solids.iter().any(|s| (x - s.x).abs() <= s.hx && (z - s.z).abs() <= s.hz)
    }

    /// The closest point over a solid's footprint, pulled `inset` units inside it.
    pub fn nearest_floor_point(&self, x: f32, z: f32, inset: f32) -> [f32; 2] {
        if let Some(g) = &self.ground {
            return self.ground_floor_point(g.as_ref(), x, z, inset);
        }
        let mut best = ([x, z], f32::MAX);
        for s in &self.solids {
            let (hx, hz) = ((s.hx - inset).max(0.0), (s.hz - inset).max(0.0));
            let p = [x.clamp(s.x - hx, s.x + hx), z.clamp(s.z - hz, s.z + hz)];
            let d = (p[0] - x).hypot(p[1] - z);
            if d < best.1 {
                best = (p, d);
            }
        }
        best.0
    }

    /// Whether a thing may rest at `p`: over the floor, standing on a surface at about its height,
    /// and clear of the lava, the bumper pillars and the arena's rim.
    pub fn valid_spot(&self, p: [f32; 3]) -> bool {
        if !self.over_floor(p[0], p[2]) || self.in_bumper(p[0], p[2], 0.0) {
            return false;
        }
        if let Some(g) = &self.ground {
            return g.floor(p[0], p[2], p[1] + 2.0).is_some_and(|top| p[1] - top < 2.0 && g.standable(p[0], top, p[2]));
        }
        if let Some(b) = self.bounds {
            if p[0].abs() > b[0] || p[2].abs() > b[1] {
                return false;
            }
        }
        // Lifts and sliders move: what counts is that some solid reaches the spot's height at some time.
        match self.support(p[0], p[2], p[1] + 2.0) {
            Some((top, _)) => p[1] - top < 2.0 || self.solids.iter().any(|s| s.mover != Mover::None),
            None => false,
        }
    }

    /// A spot on solid ground for something that has left the level (its home spot if that is valid).
    pub fn respawn_spot(&self, home: [f32; 3]) -> [f32; 3] {
        if self.valid_spot(home) && self.lava_floor() <= home[1] - 2.0 {
            return home;
        }
        let q = self.nearest_floor_point(home[0], home[2], 40.0);
        let top = self.support(q[0], q[1], 1000.0).map_or(0.0, |t| t.0);
        [q[0], top, q[1]]
    }

    /// A random spot on the main floor (or a ledge) where something can land, for sky drops.
    pub fn random_drop_spot(&self, rng: &mut Rng) -> [f32; 3] {
        if let Some(g) = &self.ground {
            for _ in 0..24 {
                let x = rng.range(-self.half[0] * 0.85, self.half[0] * 0.85);
                let z = rng.range(-self.half[1] * 0.85, self.half[1] * 0.85);
                if let Some(top) = g.floor(x, z, self.ceil_y) {
                    if top > self.void_y + 100.0 && g.standable(x, top, z) {
                        return [x, top, z];
                    }
                }
            }
            let s = self.item_spots[rng.below(self.item_spots.len())];
            return [s[0], s[1], s[2]];
        }
        for _ in 0..12 {
            let x = rng.range(-self.half[0] * 0.85, self.half[0] * 0.85);
            let z = rng.range(-self.half[1] * 0.85, self.half[1] * 0.85);
            if let Some((top, _)) = self.support(x, z, 1000.0) {
                if !self.in_bumper(x, z, 60.0) && top >= self.lava_floor() + 4.0 {
                    return [x, top, z];
                }
            }
        }
        let s = self.item_spots[rng.below(self.item_spots.len())];
        [s[0], s[1], s[2]]
    }

    /// Closest standable floor point to (x, z) at least `inset` inside the rim (rings outward).
    fn ground_floor_point(&self, g: &dyn Ground, x: f32, z: f32, inset: f32) -> [f32; 2] {
        let lim = self.bounds.map(|b| [(b[0] - inset).max(0.0), (b[1] - inset).max(0.0)]);
        let ok = |px: f32, pz: f32| lim.is_none_or(|l| px.abs() <= l[0] && pz.abs() <= l[1]) && g.floor(px, pz, self.ceil_y).is_some_and(|t| t > self.void_y + 100.0 && g.standable(px, t, pz));
        let (cx, cz) = match lim {
            Some(l) => (x.clamp(-l[0], l[0]), z.clamp(-l[1], l[1])),
            None => (x, z),
        };
        for ring in 0..40 {
            let r = ring as f32 * 30.0;
            let n = if ring == 0 { 1 } else { 8 + ring * 2 };
            for k in 0..n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                let (px, pz) = (cx + a.cos() * r, cz + a.sin() * r);
                if ok(px, pz) {
                    return [px, pz];
                }
            }
        }
        self.spawns[0]
    }

    /// World position of the arena-local origin (presentation only).
    pub fn origin(&self) -> [f32; 3] {
        self.ground.as_ref().map_or([0.0; 3], |g| g.origin())
    }

    /// How far above a gem's last height a floor still catches it (slopes rise under a rolling gem).
    pub fn gem_reach(&self) -> f32 {
        if self.ground.is_some() { 30.0 } else { 8.0 }
    }

    /// Is a point inside solid wall (real grounds only)? Used to stop shots in cliffs and trees.
    pub fn blocked_point(&self, pos: [f32; 3]) -> bool {
        match &self.ground {
            Some(g) => {
                let mut q = pos;
                g.push(&mut q, 12.0);
                (q[0] - pos[0]).hypot(q[2] - pos[2]) > 6.0
            }
            None => false,
        }
    }

    pub fn in_bumper(&self, x: f32, z: f32, pad: f32) -> bool {
        self.bumpers.iter().any(|b| (x - b.x).hypot(z - b.z) < b.r + pad)
    }

    /// The nearest tall platform (a place to stand while the lava is up), as (x, z).
    pub fn nearest_high_ground(&self, x: f32, z: f32, min_top: f32) -> Option<V2> {
        let mut best: Option<(f32, V2)> = None;
        for s in &self.solids {
            if !s.active() || s.top < min_top {
                continue;
            }
            let d = (x - s.x).hypot(z - s.z);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, [s.x, s.z]));
            }
        }
        best.map(|b| b.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_spread() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..50 {
            assert_eq!(a.next(), b.next());
        }
        let mut c = Rng::new(8);
        assert_ne!(Rng::new(7).next(), c.next());
        let mut r = Rng::new(1);
        let mean: f32 = (0..2000).map(|_| r.f()).sum::<f32>() / 2000.0;
        assert!((mean - 0.5).abs() < 0.05);
    }

    #[test]
    fn every_arena_has_floor_spawns_items_and_gems_on_something() {
        for k in ARENAS {
            let a = Arena::build(k);
            assert!(a.item_spots.len() >= 6 && a.gem_spots.len() >= 6, "{k:?}");
            for s in a.spawns {
                assert!(a.support(s[0], s[1], 1000.0).is_some(), "{k:?} spawn {s:?} has no floor");
            }
            for g in a.gem_spots.iter().chain(a.item_spots.iter()) {
                let (top, _) = a.support(g[0], g[2], 1000.0).unwrap_or_else(|| panic!("{k:?} spot {g:?} floats"));
                assert!(top >= g[1] - 1.0 || a.solids.iter().any(|s| s.mover != Mover::None), "{k:?} spot {g:?} is below ground {top}");
            }
        }
    }

    #[test]
    fn lift_moves_and_carries_a_delta() {
        let mut a = Arena::build(ArenaKind::Gearworks);
        let i = a.solids.iter().position(|s| matches!(s.mover, Mover::Lift { .. })).unwrap();
        let t0 = a.solids[i].top;
        for _ in 0..60 {
            a.step(&[]);
        }
        assert!((a.solids[i].top - t0).abs() > 20.0);
        assert!(a.solids[i].d[1].abs() < STEP, "a rider must be able to follow in one tick");
    }

    #[test]
    fn lava_cycles_between_low_and_high() {
        let (lo, p) = Arena::lava_at(0.0);
        assert_eq!(p, LavaPhase::Low);
        let (hi, p) = Arena::lava_at(16.0);
        assert_eq!(p, LavaPhase::High);
        assert!(hi > 40.0 && lo < 0.0, "the floor is dry at low tide and under at high tide");
        assert!(matches!(Arena::lava_at(10.0).1, LavaPhase::Warn));
    }

    #[test]
    fn planks_crumble_and_come_back() {
        let mut a = Arena::build(ArenaKind::Bridge);
        let i = a.solids.iter().position(|s| s.crumble).unwrap();
        let mut stood = vec![false; a.solids.len()];
        stood[i] = true;
        for _ in 0..40 {
            a.step(&stood);
        }
        assert!(matches!(a.solids[i].plank, Plank::Fallen(_)), "{:?}", a.solids[i].plank);
        assert!(a.support(a.solids[i].x, 0.0, 200.0).is_none());
        for _ in 0..200 {
            a.step(&[]);
        }
        assert_eq!(a.solids[i].plank, Plank::Solid);
    }

    #[test]
    fn walls_stop_walkers_but_not_jumpers() {
        let a = Arena::build(ArenaKind::Bumpers);
        let mut p = [-690.0, 0.0, 0.0];
        a.push_out(&mut p, 40.0);
        assert!(p[0] <= -850.0 + 110.0 * 0.0 - 0.0 || p[0] > -630.0 || p[0] < -690.0, "pushed out of the ledge: {p:?}");
        let mut high = [-700.0, 100.0, 0.0];
        a.push_out(&mut high, 40.0);
        assert_eq!(high[0], -700.0, "standing at ledge height is not blocked");
    }

    #[test]
    fn wind_gusts_after_a_calm() {
        let mut a = Arena::build(ArenaKind::Gale);
        let mut saw_warn = false;
        let mut saw_gust = false;
        for _ in 0..(12 * 30) {
            a.step(&[]);
            saw_warn |= a.wind_warn > 0.5;
            saw_gust |= a.wind > 0.9;
        }
        assert!(saw_warn && saw_gust);
    }
}
