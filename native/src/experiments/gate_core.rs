//! Hold the Gate, deterministic core: an original tower-defence ruleset with no engine dependency.
//!
//! Waves of SM64 critters walk (or float) from the edge of a meadow toward a castle gate. You spend
//! coins on six kinds of towers, upgrade and sell them, and the player's own Mario assists: his
//! punch, dive and ground-pound hurt foes, he scoops up the coins they drop, and standing near a
//! tower rallies it (faster fire; a ground-pound near it triggers an overdrive and clears jams).
//!
//! Like the other cores, all state lives in fixed-capacity slot arrays (`enemies`, `towers`,
//! `projs`, ...) so a tick never allocates and a render slot index is stable for the life of an
//! entity. Positions are SM64 units on the arena plane, relative to the arena centre; the Bevy
//! layer adds the origin. Time is counted in 30 Hz ticks and every random choice comes from the
//! seeded [`Rng`], so a run is reproducible from `(seed, map, commands, hero frames)`.
pub use super::swarm_core::Rng;

pub const TICKS_PER_SEC: u32 = 30;
/// Side of one grid cell.
pub const TILE: f32 = 220.0;
pub const ENEMY_SLOTS: usize = 192;
pub const PROJ_SLOTS: usize = 96;
pub const TOWER_SLOTS: usize = 40;
pub const PICKUP_SLOTS: usize = 40;
pub const FX_SLOTS: usize = 48;
pub const BEAM_SLOTS: usize = 12;
pub const WAVES: u32 = 15;
pub const START_LIVES: i32 = 20;
pub const START_COINS: u32 = 400;
pub const FIRST_WAVE_SECS: u32 = 40;
pub const INTERWAVE_SECS: u32 = 24;
pub const SELL_PERCENT: u32 = 70;
/// Hero rally radius and the overdrive a ground-pound near a tower grants.
pub const RALLY_RADIUS: f32 = 250.0;
const OVERDRIVE_TICKS: u16 = 150;
const OVERDRIVE_COOLDOWN: u16 = 750;
const PICKUP_RADIUS: f32 = 120.0;
const PICKUP_TTL: u16 = 270;

/// Live-enemy cap: phones get fewer than desktops.
pub const fn enemy_cap() -> usize {
    if cfg!(any(target_os = "ios", target_os = "android")) { 150 } else { 180 }
}

// ---------------------------------------------------------------------------------------------
// Maps
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Grass,
    Path,
    Rock,
}

pub struct MapDef {
    pub name: &'static str,
    pub blurb: &'static str,
    pub w: i32,
    pub h: i32,
    /// Waypoints in cells, axis-aligned runs; the first point lies one cell outside the map.
    pub lanes: &'static [&'static [(i32, i32)]],
    /// Inclusive rectangles `(x0, y0, x1, y1)` of unbuildable rock.
    pub rocks: &'static [(i32, i32, i32, i32)],
}

pub const MAPS: [MapDef; 2] = [
    MapDef {
        name: "Meadow Pass",
        blurb: "One long winding road. Flyers cut straight across it.",
        w: 18,
        h: 11,
        lanes: &[&[(-1, 1), (4, 1), (4, 8), (9, 8), (9, 2), (13, 2), (13, 8), (17, 8)]],
        rocks: &[(0, 5, 2, 8), (6, 3, 7, 5), (15, 3, 16, 5), (16, 0, 17, 1), (11, 5, 11, 6)],
    },
    MapDef {
        name: "Twin Fords",
        blurb: "Two roads from two gates merge into one long, deadly road.",
        w: 18,
        h: 11,
        lanes: &[&[(-1, 2), (7, 2), (7, 5), (10, 5), (10, 9), (13, 9), (13, 1), (15, 1), (15, 5), (17, 5)], &[(-1, 8), (7, 8), (7, 5), (10, 5), (10, 9), (13, 9), (13, 1), (15, 1), (15, 5), (17, 5)]],
        rocks: &[(2, 4, 3, 6), (11, 3, 12, 3), (11, 7, 12, 7), (16, 0, 17, 1), (15, 9, 17, 10)],
    },
];

/// A walking route as a polyline with cumulative lengths.
#[derive(Clone, Debug)]
pub struct Lane {
    pub pts: Vec<[f32; 2]>,
    pub cum: Vec<f32>,
    pub len: f32,
}

impl Lane {
    pub fn new(pts: Vec<[f32; 2]>) -> Self {
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            let l = cum[cum.len() - 1] + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            cum.push(l);
        }
        let len = *cum.last().unwrap_or(&0.0);
        Self { pts, cum, len }
    }
    /// Position and unit direction at arclength `off` shifted sideways by `lat`; `seg` is the
    /// caller's cached segment index (it moves both ways, so knock-back works).
    pub fn sample(&self, seg: &mut u16, off: f32, lat: f32) -> ([f32; 2], [f32; 2]) {
        let last = self.pts.len() - 2;
        let mut s = (*seg as usize).min(last);
        while s < last && off > self.cum[s + 1] {
            s += 1;
        }
        while s > 0 && off < self.cum[s] {
            s -= 1;
        }
        *seg = s as u16;
        let (a, b) = (self.pts[s], self.pts[s + 1]);
        let l = (self.cum[s + 1] - self.cum[s]).max(1e-3);
        let t = ((off - self.cum[s]) / l).clamp(0.0, 1.0);
        let d = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
        ([a[0] + (b[0] - a[0]) * t - d[1] * lat, a[1] + (b[1] - a[1]) * t + d[0] * lat], d)
    }
}

pub struct Map {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Tile>,
    pub lanes: Vec<Lane>,
    /// Straight flight routes from every spawn to the gate.
    pub air: Vec<Lane>,
    pub gate: [f32; 2],
}

impl Map {
    pub fn new(id: usize) -> Self {
        let def = &MAPS[id % MAPS.len()];
        let mut m = Map { w: def.w, h: def.h, tiles: vec![Tile::Grass; (def.w * def.h) as usize], lanes: vec![], air: vec![], gate: [0.0; 2] };
        for r in def.rocks {
            for y in r.1..=r.3 {
                for x in r.0..=r.2 {
                    m.set(x, y, Tile::Rock);
                }
            }
        }
        for lane in def.lanes {
            for pair in lane.windows(2) {
                let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
                for y in y0.min(y1)..=y0.max(y1) {
                    for x in x0.min(x1)..=x0.max(x1) {
                        m.set(x, y, Tile::Path);
                    }
                }
            }
            let pts = lane.iter().map(|&(x, y)| m.center(x, y)).collect();
            m.lanes.push(Lane::new(pts));
        }
        m.gate = *m.lanes[0].pts.last().unwrap();
        for l in &m.lanes {
            m.air.push(Lane::new(vec![l.pts[0], m.gate]));
        }
        m
    }
    fn set(&mut self, x: i32, y: i32, t: Tile) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            let i = (y * self.w + x) as usize;
            self.tiles[i] = t;
        }
    }
    pub fn tile(&self, x: i32, y: i32) -> Option<Tile> {
        (x >= 0 && y >= 0 && x < self.w && y < self.h).then(|| self.tiles[(y * self.w + x) as usize])
    }
    pub fn center(&self, x: i32, y: i32) -> [f32; 2] {
        [(x as f32 + 0.5 - self.w as f32 / 2.0) * TILE, (y as f32 + 0.5 - self.h as f32 / 2.0) * TILE]
    }
    pub fn cell_at(&self, p: [f32; 2]) -> Option<(i32, i32)> {
        let (x, y) = ((p[0] / TILE + self.w as f32 / 2.0).floor() as i32, (p[1] / TILE + self.h as f32 / 2.0).floor() as i32);
        self.tile(x, y).map(|_| (x, y))
    }
    /// Half extents of the playfield.
    pub fn half(&self) -> [f32; 2] {
        [self.w as f32 * TILE / 2.0, self.h as f32 * TILE / 2.0]
    }
    /// How much road lies within `range` of a cell (used by the demo bot and tests).
    pub fn coverage(&self, x: i32, y: i32, range: f32) -> u32 {
        let c = self.center(x, y);
        let mut n = 0;
        for lane in &self.lanes {
            let mut d = 0.0;
            let mut seg = 0u16;
            while d < lane.len {
                let (p, _) = lane.sample(&mut seg, d, 0.0);
                if (p[0] - c[0]).hypot(p[1] - c[1]) <= range {
                    n += 1;
                }
                d += TILE / 2.0;
            }
        }
        n
    }
}

// ---------------------------------------------------------------------------------------------
// Enemies
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum EKind {
    /// Cheap swarmer.
    Goomba = 1,
    /// Fast runner.
    Bomb = 2,
    /// Armoured; shoves and jams nearby towers.
    Bully = 3,
    /// Slow, tough, fireproof.
    Spiny = 4,
    /// Flies straight over the rocks.
    Boo = 5,
    /// Flying healer.
    Lakitu = 6,
    /// Calls in reinforcements.
    Koopa = 7,
    /// Boss: calls goombas.
    KingBomb = 8,
    /// Boss: armoured, lunges.
    Chomp = 9,
    /// Boss: stomps jam towers.
    Whomp = 10,
}

pub const ALL_EKINDS: [EKind; 10] = [EKind::Goomba, EKind::Bomb, EKind::Bully, EKind::Spiny, EKind::Boo, EKind::Lakitu, EKind::Koopa, EKind::KingBomb, EKind::Chomp, EKind::Whomp];

impl EKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        ALL_EKINDS.get((v as usize).wrapping_sub(1)).copied()
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Goomba => "Goomba",
            Self::Bomb => "Bob-omb",
            Self::Bully => "Bully",
            Self::Spiny => "Spiny",
            Self::Boo => "Boo",
            Self::Lakitu => "Lakitu",
            Self::Koopa => "Koopa",
            Self::KingBomb => "King Bob-omb",
            Self::Chomp => "Chain Chomp",
            Self::Whomp => "Whomp",
        }
    }
    pub fn is_boss(self) -> bool {
        matches!(self, Self::KingBomb | Self::Chomp | Self::Whomp)
    }
    pub fn flying(self) -> bool {
        matches!(self, Self::Boo | Self::Lakitu)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EStats {
    pub hp: f32,
    /// Units per tick.
    pub speed: f32,
    pub radius: f32,
    pub armor: f32,
    pub bounty: u32,
    pub leak: i32,
    pub burn_immune: bool,
}

pub fn estats(k: EKind) -> EStats {
    let s = |hp, speed, radius, armor, bounty, leak, burn_immune| EStats { hp, speed, radius, armor, bounty, leak, burn_immune };
    match k {
        EKind::Goomba => s(42.0, 4.2, 40.0, 0.0, 6, 1, false),
        EKind::Bomb => s(30.0, 7.6, 36.0, 0.0, 7, 1, false),
        EKind::Bully => s(150.0, 3.3, 52.0, 4.0, 14, 1, false),
        EKind::Spiny => s(290.0, 2.4, 46.0, 1.0, 18, 2, true),
        EKind::Boo => s(80.0, 3.8, 44.0, 0.0, 11, 1, false),
        EKind::Lakitu => s(120.0, 3.0, 46.0, 0.0, 20, 2, false),
        EKind::Koopa => s(150.0, 3.2, 44.0, 0.0, 18, 2, false),
        EKind::KingBomb => s(1300.0, 2.1, 90.0, 2.0, 180, 5, false),
        EKind::Chomp => s(1900.0, 3.4, 84.0, 5.0, 280, 8, false),
        EKind::Whomp => s(3800.0, 1.7, 110.0, 5.0, 500, 20, false),
    }
}

/// Enemy health multiplier for a wave (1-based).
pub fn wave_hp_mult(wave: u32) -> f32 {
    let n = wave.max(1) as f32 - 1.0;
    1.0 + 0.12 * n + 0.008 * n * n
}

#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub kind: EKind,
    pub count: u16,
    /// Ticks between spawns.
    pub every: u16,
    /// Ticks after the wave begins.
    pub delay: u16,
}

const fn g(kind: EKind, count: u16, every: u16, delay: u16) -> Group {
    Group { kind, count, every, delay }
}

use EKind::*;
pub const WAVE_TABLE: [&[Group]; WAVES as usize] = [
    &[g(Goomba, 10, 40, 0)],
    &[g(Goomba, 14, 30, 0), g(Bomb, 4, 60, 300)],
    &[g(Goomba, 12, 26, 0), g(Bomb, 8, 36, 200)],
    &[g(Bully, 3, 100, 0), g(Goomba, 14, 30, 60)],
    &[g(KingBomb, 1, 1, 90), g(Goomba, 16, 26, 0)],
    &[g(Boo, 8, 50, 0), g(Goomba, 12, 30, 120)],
    &[g(Spiny, 5, 90, 0), g(Bomb, 10, 30, 150)],
    &[g(Koopa, 2, 200, 0), g(Goomba, 16, 26, 60), g(Bully, 4, 80, 100)],
    &[g(Boo, 12, 36, 0), g(Lakitu, 2, 150, 200), g(Spiny, 4, 100, 60)],
    &[g(Chomp, 1, 1, 150), g(Bully, 6, 70, 0), g(Bomb, 12, 24, 300), g(Lakitu, 2, 150, 120)],
    &[g(Koopa, 4, 140, 0), g(Goomba, 24, 20, 40), g(Boo, 8, 50, 100)],
    &[g(Spiny, 8, 70, 0), g(Lakitu, 3, 130, 100), g(Bully, 6, 60, 60)],
    &[g(Bomb, 20, 16, 0), g(Boo, 14, 30, 60), g(Koopa, 3, 150, 100)],
    &[g(Bully, 10, 50, 0), g(Spiny, 8, 60, 40), g(Lakitu, 4, 100, 80), g(Goomba, 24, 18, 120)],
    &[g(Whomp, 1, 1, 120), g(Koopa, 4, 160, 0), g(Boo, 12, 40, 60), g(Bully, 6, 60, 100), g(Spiny, 5, 80, 160), g(Goomba, 26, 14, 200)],
];

/// What the wave-preview panel shows.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Preview {
    pub n: usize,
    pub kinds: [u8; 6],
    pub counts: [u16; 6],
    pub boss: bool,
    pub air: bool,
}

/// Preview of wave `wave` (1-based); empty past the last wave.
pub fn preview(wave: u32) -> Preview {
    let mut p = Preview::default();
    let Some(groups) = WAVE_TABLE.get((wave as usize).wrapping_sub(1)) else { return p };
    for gr in groups.iter().take(6) {
        p.kinds[p.n] = gr.kind as u8;
        p.counts[p.n] = gr.count;
        p.n += 1;
        p.boss |= gr.kind.is_boss();
        p.air |= gr.kind.flying();
    }
    p
}

#[derive(Clone, Copy, Debug)]
pub struct Enemy {
    pub alive: bool,
    pub kind: EKind,
    pub uid: u32,
    pub lane: u8,
    pub air: bool,
    pub seg: u16,
    pub off: f32,
    pub lat: f32,
    pub pos: [f32; 2],
    pub prev: [f32; 2],
    pub dir: [f32; 2],
    pub hp: f32,
    pub max_hp: f32,
    pub armor: f32,
    pub speed: f32,
    pub radius: f32,
    /// Speed multiplier (1 = none) and ticks it lasts.
    pub slow: f32,
    pub slow_t: u16,
    /// Burn damage per tick and ticks left.
    pub burn: f32,
    pub burn_t: u16,
    pub stun_t: u16,
    /// Ticks until the next special ability.
    pub cd: u16,
    pub dash_t: u16,
    pub flash: u8,
}

impl Default for Enemy {
    fn default() -> Self {
        Self { alive: false, kind: EKind::Goomba, uid: 0, lane: 0, air: false, seg: 0, off: 0.0, lat: 0.0, pos: [0.0; 2], prev: [0.0; 2], dir: [1.0, 0.0], hp: 0.0, max_hp: 1.0, armor: 0.0, speed: 0.0, radius: 40.0, slow: 1.0, slow_t: 0, burn: 0.0, burn_t: 0, stun_t: 0, cd: 0, dash_t: 0, flash: 0 }
    }
}

// ---------------------------------------------------------------------------------------------
// Towers
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TKind {
    /// Lobbed shell, splash. Ground only.
    Bombard = 0,
    /// Stomps the road around it and stuns. Ground only.
    Slab = 1,
    /// Fireballs that burn (burn ignores armour).
    Bloom = 2,
    /// Slowing aura, hits flyers too.
    Eye = 3,
    /// Piercing beam through everything in a line.
    Prism = 4,
    /// Pays coins over time.
    Spout = 5,
}

pub const ALL_TKINDS: [TKind; 6] = [TKind::Bombard, TKind::Slab, TKind::Bloom, TKind::Eye, TKind::Prism, TKind::Spout];

#[derive(Clone, Copy, Debug)]
pub struct TSpec {
    /// Coins to build (level 1) or to upgrade into this level.
    pub cost: u32,
    pub range: f32,
    /// Ticks between shots.
    pub interval: f32,
    pub damage: f32,
    /// Splash / slam / beam half-width.
    pub radius: f32,
    /// Slow fraction (Eye), burn damage per second (Bloom), stun ticks (Slab), coins per payout (Spout).
    pub effect: f32,
}

impl TKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        ALL_TKINDS.get(v as usize).copied()
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Bombard => "Bombard",
            Self::Slab => "Crush Slab",
            Self::Bloom => "Cinder Bloom",
            Self::Eye => "Dread Eye",
            Self::Prism => "Prism Star",
            Self::Spout => "Coin Spout",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Bombard => "Lobs shells with splash. Ground only.",
            Self::Slab => "Slams the road and stuns. Ground only.",
            Self::Bloom => "Fireballs that burn through armor.",
            Self::Eye => "Slows everything in range, even flyers.",
            Self::Prism => "A beam that pierces a whole line.",
            Self::Spout => "Pays coins over time.",
        }
    }
    pub fn ground_only(self) -> bool {
        matches!(self, Self::Bombard | Self::Slab)
    }
    pub fn spec(self, level: u8) -> TSpec {
        let l = (level as usize).min(2);
        let s = |cost: [u32; 3], range: [f32; 3], interval: [f32; 3], damage: [f32; 3], radius: [f32; 3], effect: [f32; 3]| TSpec { cost: cost[l], range: range[l], interval: interval[l], damage: damage[l], radius: radius[l], effect: effect[l] };
        match self {
            Self::Bombard => s([100, 85, 150], [600.0, 640.0, 700.0], [60.0, 54.0, 48.0], [30.0, 48.0, 72.0], [120.0, 140.0, 170.0], [0.0; 3]),
            Self::Slab => s([120, 100, 170], [330.0, 350.0, 380.0], [96.0, 84.0, 72.0], [32.0, 52.0, 80.0], [330.0, 350.0, 380.0], [24.0, 36.0, 54.0]),
            Self::Bloom => s([90, 75, 130], [520.0, 560.0, 600.0], [20.0, 18.0, 16.0], [8.0, 12.0, 18.0], [0.0; 3], [8.0, 13.0, 20.0]),
            Self::Eye => s([110, 90, 150], [420.0, 470.0, 520.0], [30.0, 30.0, 30.0], [0.0, 0.0, 6.0], [0.0; 3], [0.28, 0.38, 0.5]),
            Self::Prism => s([160, 130, 220], [720.0, 780.0, 850.0], [66.0, 58.0, 48.0], [24.0, 38.0, 62.0], [30.0, 30.0, 34.0], [0.0; 3]),
            Self::Spout => s([80, 70, 120], [0.0; 3], [90.0, 90.0, 90.0], [0.0; 3], [0.0; 3], [2.0, 4.0, 7.0]),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Target {
    First = 0,
    Last = 1,
    Strongest = 2,
    Closest = 3,
}

impl Target {
    pub fn next(self) -> Self {
        match self {
            Self::First => Self::Last,
            Self::Last => Self::Strongest,
            Self::Strongest => Self::Closest,
            Self::Closest => Self::First,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::First => "First",
            Self::Last => "Last",
            Self::Strongest => "Strongest",
            Self::Closest => "Closest",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Tower {
    pub alive: bool,
    pub kind: TKind,
    pub level: u8,
    pub cell: (i32, i32),
    pub pos: [f32; 2],
    pub cd: f32,
    pub mode: Target,
    /// Coins invested (build + upgrades); sell refunds [`SELL_PERCENT`] of it.
    pub spent: u32,
    pub jam: u16,
    pub rally: bool,
    pub od_t: u16,
    pub od_cd: u16,
    pub dealt: f32,
    pub kills: u32,
    /// View: yaw toward the last target and a short "just fired" counter.
    pub aim: f32,
    pub fired: u8,
}

impl Default for Tower {
    fn default() -> Self {
        Self { alive: false, kind: TKind::Bombard, level: 0, cell: (0, 0), pos: [0.0; 2], cd: 0.0, mode: Target::First, spent: 0, jam: 0, rally: false, od_t: 0, od_cd: 0, dealt: 0.0, kills: 0, aim: 0.0, fired: 0 }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Proj {
    pub alive: bool,
    /// 1 shell, 2 fireball.
    pub kind: u8,
    pub pos: [f32; 2],
    pub prev: [f32; 2],
    pub dest: [f32; 2],
    pub target: u32,
    pub t: u16,
    pub total: u16,
    pub damage: f32,
    pub radius: f32,
    pub burn: f32,
    pub src: u8,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Pickup {
    pub alive: bool,
    pub pos: [f32; 2],
    pub value: u32,
    pub ttl: u16,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Beam {
    pub alive: bool,
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub ttl: u8,
}

/// Short-lived ground effect for the view: 1 splash, 2 slam, 3 hero pound.
#[derive(Clone, Copy, Default, Debug)]
pub struct Fx {
    pub alive: bool,
    pub kind: u8,
    pub pos: [f32; 2],
    pub radius: f32,
    pub ttl: u8,
    pub max: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    NotBuildable,
    Occupied,
    NoCoins,
    TowerCap,
    Over,
}

impl BuildError {
    pub fn text(self) -> &'static str {
        match self {
            Self::NotBuildable => "Can't build there",
            Self::Occupied => "Already a tower here",
            Self::NoCoins => "Not enough coins",
            Self::TowerCap => "Tower limit reached",
            Self::Over => "The run is over",
        }
    }
}

pub const MAX_TOWERS: usize = 36;

/// The hero's frame, fed every tick.
#[derive(Clone, Copy, Default, Debug)]
pub struct HeroFrame {
    pub pos: [f32; 2],
    pub height: f32,
    pub face: f32,
    pub punch_started: bool,
    pub dive_started: bool,
    pub pound_land: bool,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Events {
    /// Where the last splash / slam / stomp / overdrive happened.
    pub at: [f32; 2],
    pub kills: u32,
    pub last_kill: [f32; 2],
    pub leaked: u32,
    pub splash: bool,
    pub slam: bool,
    pub beam: bool,
    pub shot: bool,
    /// Bit `1 << TKind as u8` for each tower kind that fired a shot / beam / slam / pulse this tick (for per-kind sound).
    pub fired_kinds: u8,
    pub hero_hit: bool,
    pub pickup: bool,
    pub income: bool,
    pub wave_started: bool,
    pub wave_cleared: bool,
    pub boss_spawned: bool,
    pub boss_killed: bool,
    pub overdrive: bool,
    pub stomp: bool,
    pub jammed: bool,
    pub won: bool,
    pub lost: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Playing,
    Won,
    Lost,
}

pub struct Gate {
    pub map: Map,
    pub rng: Rng,
    pub tick: u32,
    pub phase: Phase,
    pub coins: u32,
    pub lives: i32,
    pub max_lives: i32,
    /// Waves started so far (0..=WAVES).
    pub wave: u32,
    pub waves_cleared: u32,
    /// Ticks until the next wave starts (counts only while nothing is spawning).
    pub timer: u32,
    pub spawning: bool,
    wave_tick: u32,
    spawned: [u16; 6],
    pub speed: u8,
    pub paused: bool,
    pub god: bool,
    pub enemies: Vec<Enemy>,
    pub towers: Vec<Tower>,
    pub projs: Vec<Proj>,
    pub pickups: Vec<Pickup>,
    pub beams: Vec<Beam>,
    pub fx: Vec<Fx>,
    fx_next: usize,
    pub alive_enemies: usize,
    next_uid: u32,
    pub kills: u32,
    pub earned: u32,
    pub ev: Events,
    pub cap: usize,
    spawn_count: u32,
}

const NO_SRC: usize = usize::MAX;

impl Gate {
    pub fn new(seed: u64, map: usize) -> Self {
        Self {
            map: Map::new(map),
            rng: Rng::new(seed),
            tick: 0,
            phase: Phase::Playing,
            coins: START_COINS,
            lives: START_LIVES,
            max_lives: START_LIVES,
            wave: 0,
            waves_cleared: 0,
            timer: FIRST_WAVE_SECS * TICKS_PER_SEC,
            spawning: false,
            wave_tick: 0,
            spawned: [0; 6],
            speed: 1,
            paused: false,
            god: false,
            enemies: vec![Enemy::default(); ENEMY_SLOTS],
            towers: vec![Tower::default(); TOWER_SLOTS],
            projs: vec![Proj::default(); PROJ_SLOTS],
            pickups: vec![Pickup::default(); PICKUP_SLOTS],
            beams: vec![Beam::default(); BEAM_SLOTS],
            fx: vec![Fx::default(); FX_SLOTS],
            fx_next: 0,
            alive_enemies: 0,
            next_uid: 1,
            kills: 0,
            earned: 0,
            ev: Events::default(),
            cap: enemy_cap(),
            spawn_count: 0,
        }
    }

    pub fn over(&self) -> bool {
        self.phase != Phase::Playing
    }
    pub fn tower_count(&self) -> usize {
        self.towers.iter().filter(|t| t.alive).count()
    }
    pub fn tower_at(&self, cell: (i32, i32)) -> Option<usize> {
        self.towers.iter().position(|t| t.alive && t.cell == cell)
    }
    /// Wave number the preview panel should show (the one that comes next, else the current).
    pub fn next_wave(&self) -> u32 {
        (self.wave + 1).min(WAVES)
    }
    pub fn hero_damage_mult(&self) -> f32 {
        1.0 + 0.10 * (self.wave.max(1) - 1) as f32
    }

    // ----- player commands ------------------------------------------------------------------

    pub fn can_build(&self, kind: TKind, cell: (i32, i32)) -> Result<(), BuildError> {
        if self.over() {
            return Err(BuildError::Over);
        }
        if self.map.tile(cell.0, cell.1) != Some(Tile::Grass) {
            return Err(BuildError::NotBuildable);
        }
        if self.tower_at(cell).is_some() {
            return Err(BuildError::Occupied);
        }
        if self.tower_count() >= MAX_TOWERS {
            return Err(BuildError::TowerCap);
        }
        if self.coins < kind.spec(0).cost {
            return Err(BuildError::NoCoins);
        }
        Ok(())
    }

    pub fn build(&mut self, kind: TKind, cell: (i32, i32)) -> Result<usize, BuildError> {
        self.can_build(kind, cell)?;
        let i = self.towers.iter().position(|t| !t.alive).ok_or(BuildError::TowerCap)?;
        let cost = kind.spec(0).cost;
        self.coins -= cost;
        self.towers[i] = Tower { alive: true, kind, cell, pos: self.map.center(cell.0, cell.1), spent: cost, cd: kind.spec(0).interval * 0.5, ..Tower::default() };
        Ok(i)
    }

    /// Cost of the next level, or `None` at the top level.
    pub fn upgrade_cost(&self, i: usize) -> Option<u32> {
        let t = self.towers.get(i).filter(|t| t.alive)?;
        (t.level < 2).then(|| t.kind.spec(t.level + 1).cost)
    }

    pub fn upgrade(&mut self, i: usize) -> Result<(), BuildError> {
        if self.over() {
            return Err(BuildError::Over);
        }
        let cost = self.upgrade_cost(i).ok_or(BuildError::NotBuildable)?;
        if self.coins < cost {
            return Err(BuildError::NoCoins);
        }
        self.coins -= cost;
        let t = &mut self.towers[i];
        t.level += 1;
        t.spent += cost;
        Ok(())
    }

    pub fn sell_value(&self, i: usize) -> u32 {
        self.towers.get(i).filter(|t| t.alive).map_or(0, |t| t.spent * SELL_PERCENT / 100)
    }

    pub fn sell(&mut self, i: usize) -> u32 {
        let v = self.sell_value(i);
        if self.towers.get(i).is_some_and(|t| t.alive) {
            self.coins += v;
            self.towers[i].alive = false;
        }
        v
    }

    pub fn cycle_mode(&mut self, i: usize) {
        if let Some(t) = self.towers.get_mut(i).filter(|t| t.alive) {
            t.mode = t.mode.next();
        }
    }

    /// Start the next wave now (only between waves); pays a small bonus for the time saved.
    pub fn call_wave(&mut self) -> bool {
        if self.over() || self.spawning || self.wave >= WAVES {
            return false;
        }
        let bonus = self.timer / (3 * TICKS_PER_SEC);
        self.coins += bonus;
        self.earned += bonus;
        self.timer = 0;
        self.start_wave();
        true
    }

    // ----- spawning -------------------------------------------------------------------------

    fn start_wave(&mut self) {
        self.wave += 1;
        self.spawning = true;
        self.wave_tick = 0;
        self.spawned = [0; 6];
        self.ev.wave_started = true;
    }

    pub fn spawn_enemy(&mut self, kind: EKind, lane: usize, off: f32, lat: f32) -> Option<usize> {
        let i = self.enemies.iter().position(|e| !e.alive)?;
        let st = estats(kind);
        let air = kind.flying();
        let lanes = if air { &self.map.air } else { &self.map.lanes };
        let lane = lane % lanes.len();
        let mut seg = 0;
        let (pos, dir) = lanes[lane].sample(&mut seg, off, lat);
        let hp = st.hp * wave_hp_mult(self.wave);
        self.enemies[i] = Enemy { alive: true, kind, uid: self.next_uid, lane: lane as u8, air, seg, off, lat, pos, prev: pos, dir, hp, max_hp: hp, armor: st.armor, speed: st.speed, radius: st.radius, cd: ability_period(kind), ..Enemy::default() };
        self.next_uid = self.next_uid.wrapping_add(1).max(1);
        self.alive_enemies += 1;
        if kind.is_boss() {
            self.ev.boss_spawned = true;
        }
        Some(i)
    }

    fn run_spawns(&mut self) {
        let groups = WAVE_TABLE[(self.wave - 1) as usize];
        let mut pending = false;
        for (gi, gr) in groups.iter().enumerate().take(6) {
            if self.spawned[gi] >= gr.count {
                continue;
            }
            pending = true;
            let due = gr.delay as u32 + self.spawned[gi] as u32 * gr.every as u32;
            if self.wave_tick >= due && self.alive_enemies < self.cap {
                let lat = if gr.kind.is_boss() { 0.0 } else { self.rng.range(-55.0, 55.0) };
                let lane = self.spawn_count as usize;
                self.spawn_count = self.spawn_count.wrapping_add(1);
                if self.spawn_enemy(gr.kind, lane, 0.0, lat).is_some() {
                    self.spawned[gi] += 1;
                }
            }
        }
        self.wave_tick += 1;
        if !pending {
            self.spawning = false;
            if self.wave < WAVES {
                self.timer = INTERWAVE_SECS * TICKS_PER_SEC;
            }
        }
    }

    // ----- damage ---------------------------------------------------------------------------

    fn fx_push(&mut self, kind: u8, pos: [f32; 2], radius: f32, ttl: u8) {
        let i = self.fx_next;
        self.fx_next = (self.fx_next + 1) % FX_SLOTS;
        self.fx[i] = Fx { alive: true, kind, pos, radius, ttl, max: ttl };
    }

    /// Hurt enemy `i`; returns whether it died. `src` is the tower index credited (or `NO_SRC`).
    pub fn damage(&mut self, i: usize, dmg: f32, ignore_armor: bool, src: usize) -> bool {
        let e = &mut self.enemies[i];
        if !e.alive {
            return false;
        }
        let eff = if ignore_armor { dmg } else { (dmg - e.armor).max(dmg * 0.2) };
        e.hp -= eff;
        e.flash = 4;
        if src != NO_SRC {
            self.towers[src].dealt += eff;
        }
        if self.enemies[i].hp <= 0.0 {
            self.kill(i, src);
            return true;
        }
        false
    }

    fn kill(&mut self, i: usize, src: usize) {
        let e = self.enemies[i];
        self.enemies[i].alive = false;
        self.alive_enemies -= 1;
        self.kills += 1;
        let bounty = estats(e.kind).bounty;
        self.coins += bounty;
        self.earned += bounty;
        self.ev.kills += 1;
        self.ev.last_kill = e.pos;
        if e.kind.is_boss() {
            self.ev.boss_killed = true;
        }
        if src != NO_SRC {
            self.towers[src].kills += 1;
        }
        // A bonus coin the hero can scoop up (never required).
        if bounty >= 4 || self.kills.is_multiple_of(2) {
            if let Some(p) = self.pickups.iter().position(|p| !p.alive) {
                self.pickups[p] = Pickup { alive: true, pos: e.pos, value: (bounty / 2).max(1), ttl: PICKUP_TTL };
            }
        }
    }

    fn set_slow(&mut self, i: usize, frac: f32) {
        let e = &mut self.enemies[i];
        let frac = if e.kind.is_boss() { frac * 0.5 } else { frac };
        let mult = 1.0 - frac;
        if mult < e.slow || e.slow_t == 0 {
            e.slow = mult;
        }
        e.slow_t = 4;
    }

    fn apply_burn(&mut self, i: usize, dps: f32) {
        let e = &mut self.enemies[i];
        if estats(e.kind).burn_immune {
            return;
        }
        e.burn = e.burn.max(dps / TICKS_PER_SEC as f32);
        e.burn_t = 90;
    }

    fn apply_stun(&mut self, i: usize, ticks: f32) {
        let e = &mut self.enemies[i];
        let t = if e.kind.is_boss() { ticks * 0.4 } else { ticks } as u16;
        e.stun_t = e.stun_t.max(t);
    }

    // ----- targeting ------------------------------------------------------------------------

    fn progress(&self, e: &Enemy) -> f32 {
        let lanes = if e.air { &self.map.air } else { &self.map.lanes };
        e.off / lanes[e.lane as usize].len.max(1.0)
    }

    /// The enemy tower `ti` would shoot, by its targeting mode (ties go to the lowest slot).
    pub fn pick_target(&self, ti: usize) -> Option<usize> {
        let t = &self.towers[ti];
        let range = t.kind.spec(t.level).range;
        let ground_only = t.kind.ground_only();
        let mut best: Option<(usize, f32)> = None;
        for (i, e) in self.enemies.iter().enumerate() {
            if !e.alive || (ground_only && e.air) {
                continue;
            }
            let d = (e.pos[0] - t.pos[0]).hypot(e.pos[1] - t.pos[1]);
            if d > range + e.radius {
                continue;
            }
            let score = match t.mode {
                Target::First => self.progress(e),
                Target::Last => -self.progress(e),
                Target::Strongest => e.hp,
                Target::Closest => -d,
            };
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((i, score));
            }
        }
        best.map(|b| b.0)
    }

    // ----- the tick -------------------------------------------------------------------------

    pub fn tick(&mut self, hero: &HeroFrame) {
        self.ev = Events::default();
        if self.over() || self.paused {
            return;
        }
        self.tick += 1;
        for e in self.enemies.iter_mut() {
            e.prev = e.pos;
        }
        for p in self.projs.iter_mut() {
            p.prev = p.pos;
        }
        // Wave director.
        if self.spawning {
            self.run_spawns();
        } else if self.wave < WAVES {
            self.timer = self.timer.saturating_sub(1);
            if self.timer == 0 {
                self.start_wave();
                self.run_spawns();
            }
        }
        self.update_towers(hero);
        self.update_projs();
        self.update_enemies();
        self.update_hero(hero);
        self.update_misc();
        // Wave clear, win and lose.
        if !self.spawning && self.alive_enemies == 0 && self.waves_cleared < self.wave {
            self.waves_cleared = self.wave;
            self.ev.wave_cleared = true;
            let bonus = 20 + 6 * self.wave;
            self.coins += bonus;
            self.earned += bonus;
        }
        if self.lives <= 0 && !self.god {
            self.phase = Phase::Lost;
            self.ev.lost = true;
        } else if self.wave >= WAVES && !self.spawning && self.alive_enemies == 0 {
            self.phase = Phase::Won;
            self.ev.won = true;
        }
    }

    fn update_towers(&mut self, hero: &HeroFrame) {
        let overdrive = hero.pound_land;
        for ti in 0..TOWER_SLOTS {
            if !self.towers[ti].alive {
                continue;
            }
            let (kind, level) = (self.towers[ti].kind, self.towers[ti].level);
            let spec = kind.spec(level);
            let pos = self.towers[ti].pos;
            let hd = (pos[0] - hero.pos[0]).hypot(pos[1] - hero.pos[1]);
            {
                let t = &mut self.towers[ti];
                t.rally = hd < RALLY_RADIUS && hero.height < 400.0;
                t.fired = t.fired.saturating_sub(1);
                t.od_t = t.od_t.saturating_sub(1);
                t.od_cd = t.od_cd.saturating_sub(1);
                if overdrive && hd < RALLY_RADIUS + 60.0 {
                    t.jam = 0;
                    if t.od_cd == 0 {
                        t.od_t = OVERDRIVE_TICKS;
                        t.od_cd = OVERDRIVE_COOLDOWN;
                        self.ev.overdrive = true;
                    self.ev.at = pos;
                    }
                }
                if t.jam > 0 {
                    // The hero standing close repairs a jam four times as fast.
                    t.jam = t.jam.saturating_sub(if t.rally { 4 } else { 1 });
                    continue;
                }
            }
            let rate = (if self.towers[ti].rally { 1.3 } else { 1.0 }) * (if self.towers[ti].od_t > 0 { 1.6 } else { 1.0 });
            self.towers[ti].cd -= rate;
            if self.towers[ti].cd > 0.0 {
                continue;
            }
            match kind {
                TKind::Spout => {
                    let pay = (spec.effect * if self.towers[ti].rally { 1.5 } else { 1.0 }).round() as u32;
                    self.towers[ti].cd += spec.interval;
                    self.coins += pay;
                    self.earned += pay;
                    self.ev.income = true;
                    self.towers[ti].fired = 6;
                }
                TKind::Eye => {
                    // Aura: slow everyone in range every tick; chip damage on the interval at level 3.
                    let chip = spec.damage > 0.0;
                    let slow = spec.effect + if self.towers[ti].rally { 0.05 } else { 0.0 };
                    let mut any = false;
                    for i in 0..ENEMY_SLOTS {
                        let e = &self.enemies[i];
                        if !e.alive || (e.pos[0] - pos[0]).hypot(e.pos[1] - pos[1]) > spec.range + e.radius {
                            continue;
                        }
                        any = true;
                        self.set_slow(i, slow);
                        if chip {
                            self.damage(i, spec.damage, true, ti);
                        }
                    }
                    // Level 3 pulses its chip damage on the interval; the slow is applied every tick.
                    self.towers[ti].cd = if chip { self.towers[ti].cd + spec.interval } else { 0.0 };
                    if any {
                        self.towers[ti].fired = 2;
                    }
                }
                TKind::Slab => {
                    let any = self.enemies.iter().any(|e| e.alive && !e.air && (e.pos[0] - pos[0]).hypot(e.pos[1] - pos[1]) <= spec.range + e.radius);
                    if !any {
                        self.towers[ti].cd = 0.0;
                        continue;
                    }
                    for i in 0..ENEMY_SLOTS {
                        let e = &self.enemies[i];
                        if !e.alive || e.air || (e.pos[0] - pos[0]).hypot(e.pos[1] - pos[1]) > spec.range + e.radius {
                            continue;
                        }
                        self.damage(i, spec.damage, false, ti);
                        if self.enemies[i].alive {
                            self.apply_stun(i, spec.effect);
                        }
                    }
                    self.towers[ti].cd += spec.interval;
                    self.towers[ti].fired = 10;
                    self.ev.slam = true;
                    self.ev.fired_kinds |= 1 << TKind::Slab as u8;
                    self.ev.at = pos;
                    self.fx_push(2, pos, spec.range, 10);
                }
                TKind::Bombard | TKind::Bloom | TKind::Prism => {
                    let Some(target) = self.pick_target(ti) else {
                        self.towers[ti].cd = 0.0;
                        continue;
                    };
                    let e = self.enemies[target];
                    let to = [e.pos[0] - pos[0], e.pos[1] - pos[1]];
                    let dist = to[0].hypot(to[1]).max(1.0);
                    self.towers[ti].aim = to[0].atan2(to[1]);
                    self.towers[ti].cd += spec.interval;
                    self.towers[ti].fired = 6;
                    self.ev.shot = true;
                    self.ev.fired_kinds |= 1 << kind as u8;
                    match kind {
                        TKind::Bombard => {
                            let flight = (dist / 24.0).clamp(8.0, 34.0);
                            let v = e.speed * e.slow * if e.stun_t > 0 { 0.0 } else { 1.0 };
                            let dest = [e.pos[0] + e.dir[0] * v * flight, e.pos[1] + e.dir[1] * v * flight];
                            self.spawn_proj(Proj { alive: true, kind: 1, pos, prev: pos, dest, target: 0, t: flight as u16, total: flight as u16, damage: spec.damage, radius: spec.radius, burn: 0.0, src: ti as u8 });
                        }
                        TKind::Bloom => {
                            self.spawn_proj(Proj { alive: true, kind: 2, pos, prev: pos, dest: e.pos, target: e.uid, t: 40, total: 40, damage: spec.damage, radius: 0.0, burn: spec.effect, src: ti as u8 });
                        }
                        _ => {
                            let d = [to[0] / dist, to[1] / dist];
                            let end = [pos[0] + d[0] * spec.range, pos[1] + d[1] * spec.range];
                            for i in 0..ENEMY_SLOTS {
                                let e = &self.enemies[i];
                                if !e.alive {
                                    continue;
                                }
                                let r = [e.pos[0] - pos[0], e.pos[1] - pos[1]];
                                let along = r[0] * d[0] + r[1] * d[1];
                                let across = (r[0] * d[1] - r[1] * d[0]).abs();
                                if along > 0.0 && along <= spec.range + e.radius && across <= spec.radius + e.radius {
                                    self.damage(i, spec.damage, false, ti);
                                }
                            }
                            if let Some(b) = self.beams.iter().position(|b| !b.alive) {
                                self.beams[b] = Beam { alive: true, from: pos, to: end, ttl: 6 };
                            }
                            self.ev.beam = true;
                        }
                    }
                }
            }
        }
    }

    fn spawn_proj(&mut self, p: Proj) {
        if let Some(i) = self.projs.iter().position(|p| !p.alive) {
            self.projs[i] = p;
        }
    }

    fn update_projs(&mut self) {
        for pi in 0..PROJ_SLOTS {
            let p = self.projs[pi];
            if !p.alive {
                continue;
            }
            if p.kind == 1 {
                // Shell: straight to the landing point over `total` ticks.
                let left = p.t.saturating_sub(1);
                let k = 1.0 - left as f32 / p.total.max(1) as f32;
                let start = self.towers[p.src as usize].pos;
                self.projs[pi].pos = [start[0] + (p.dest[0] - start[0]) * k, start[1] + (p.dest[1] - start[1]) * k];
                self.projs[pi].t = left;
                if left == 0 {
                    self.projs[pi].alive = false;
                    self.ev.splash = true;
                    self.ev.at = p.dest;
                    self.fx_push(1, p.dest, p.radius, 8);
                    for i in 0..ENEMY_SLOTS {
                        let e = &self.enemies[i];
                        if !e.alive || e.air {
                            continue;
                        }
                        let d = (e.pos[0] - p.dest[0]).hypot(e.pos[1] - p.dest[1]);
                        if d <= p.radius + e.radius {
                            let fall = 1.0 - 0.5 * (d / (p.radius + e.radius)).min(1.0);
                            self.damage(i, p.damage * fall, false, p.src as usize);
                        }
                    }
                }
            } else {
                // Fireball: homes on its target while it lives, else flies on to the last known spot.
                let tgt = self.enemies.iter().position(|e| e.alive && e.uid == p.target);
                let dest = tgt.map_or(p.dest, |i| self.enemies[i].pos);
                let d = [dest[0] - p.pos[0], dest[1] - p.pos[1]];
                let dist = d[0].hypot(d[1]);
                let step = 26.0;
                if dist <= step + 12.0 {
                    self.projs[pi].alive = false;
                    if let Some(i) = tgt {
                        self.damage(i, p.damage, false, p.src as usize);
                        if self.enemies[i].alive {
                            self.apply_burn(i, p.burn);
                        }
                    }
                } else {
                    self.projs[pi].pos = [p.pos[0] + d[0] / dist * step, p.pos[1] + d[1] / dist * step];
                    self.projs[pi].dest = dest;
                }
            }
        }
    }

    fn update_enemies(&mut self) {
        for i in 0..ENEMY_SLOTS {
            if !self.enemies[i].alive {
                continue;
            }
            // Status effects.
            {
                let e = &mut self.enemies[i];
                e.flash = e.flash.saturating_sub(1);
                if e.slow_t > 0 {
                    e.slow_t -= 1;
                    if e.slow_t == 0 {
                        e.slow = 1.0;
                    }
                }
            }
            if self.enemies[i].burn_t > 0 {
                self.enemies[i].burn_t -= 1;
                let b = self.enemies[i].burn;
                if self.damage(i, b, true, NO_SRC) {
                    continue;
                }
            }
            // Abilities.
            let kind = self.enemies[i].kind;
            if ability_period(kind) > 0 {
                if self.enemies[i].cd > 0 {
                    self.enemies[i].cd -= 1;
                } else {
                    self.enemies[i].cd = ability_period(kind);
                    self.ability(i);
                }
            }
            // Movement.
            let e = &mut self.enemies[i];
            if e.dash_t > 0 {
                e.dash_t -= 1;
            }
            if e.stun_t > 0 {
                e.stun_t -= 1;
            } else {
                let boost = if e.dash_t > 0 { 2.3 } else { 1.0 };
                e.off += e.speed * e.slow * boost;
            }
            let lanes = if e.air { &self.map.air } else { &self.map.lanes };
            let lane = &lanes[e.lane as usize];
            if e.off >= lane.len {
                let cost = estats(e.kind).leak;
                e.alive = false;
                self.alive_enemies -= 1;
                if !self.god {
                    self.lives -= cost;
                }
                self.ev.leaked += 1;
                continue;
            }
            let (pos, dir) = lane.sample(&mut e.seg, e.off, e.lat);
            e.pos = pos;
            e.dir = dir;
        }
    }

    fn ability(&mut self, i: usize) {
        let e = self.enemies[i];
        match e.kind {
            EKind::Koopa | EKind::KingBomb => {
                for k in 0..2 {
                    if self.alive_enemies < self.cap {
                        self.spawn_enemy(EKind::Goomba, e.lane as usize, (e.off - 70.0 - 55.0 * k as f32).max(0.0), if k == 0 { -40.0 } else { 40.0 });
                    }
                }
            }
            EKind::Lakitu => {
                for j in 0..ENEMY_SLOTS {
                    let o = &self.enemies[j];
                    if o.alive && (o.pos[0] - e.pos[0]).hypot(o.pos[1] - e.pos[1]) <= 360.0 {
                        let amt = o.max_hp * if o.kind.is_boss() { 0.02 } else { 0.07 };
                        let o = &mut self.enemies[j];
                        o.hp = (o.hp + amt).min(o.max_hp);
                    }
                }
            }
            EKind::Bully => {
                // Shove: jam the nearest tower in reach.
                let mut best: Option<(usize, f32)> = None;
                for (ti, t) in self.towers.iter().enumerate() {
                    let d = (t.pos[0] - e.pos[0]).hypot(t.pos[1] - e.pos[1]);
                    if t.alive && t.jam == 0 && d <= 300.0 && best.is_none_or(|b| d < b.1) {
                        best = Some((ti, d));
                    }
                }
                if let Some((ti, _)) = best {
                    self.towers[ti].jam = 75;
                    self.ev.jammed = true;
                }
            }
            EKind::Whomp => {
                self.ev.stomp = true;
                self.ev.at = e.pos;
                self.fx_push(2, e.pos, 420.0, 10);
                for t in self.towers.iter_mut() {
                    if t.alive && (t.pos[0] - e.pos[0]).hypot(t.pos[1] - e.pos[1]) <= 420.0 {
                        t.jam = 90;
                        self.ev.jammed = true;
                    }
                }
            }
            EKind::Chomp => self.enemies[i].dash_t = 40,
            _ => {}
        }
    }

    fn update_hero(&mut self, hero: &HeroFrame) {
        let hm = self.hero_damage_mult();
        let air = hero.height > 50.0;
        let face = [hero.face.sin(), hero.face.cos()];
        let hit = |g: &mut Gate, reach: f32, front: bool, dmg: f32, stun: f32, push: f32| {
            for i in 0..ENEMY_SLOTS {
                let e = &g.enemies[i];
                if !e.alive || (e.air && !air) {
                    continue;
                }
                let r = [e.pos[0] - hero.pos[0], e.pos[1] - hero.pos[1]];
                let d = r[0].hypot(r[1]);
                if d > reach + e.radius || (front && d > 1.0 && (r[0] * face[0] + r[1] * face[1]) / d < -0.2) {
                    continue;
                }
                let boss = if e.kind.is_boss() { 0.5 } else { 1.0 };
                g.ev.hero_hit = true;
                if g.damage(i, dmg * hm * boss, false, NO_SRC) {
                    continue;
                }
                if stun > 0.0 {
                    g.apply_stun(i, stun);
                }
                if !g.enemies[i].kind.is_boss() {
                    g.enemies[i].off = (g.enemies[i].off - push).max(0.0);
                }
            }
        };
        if hero.punch_started {
            hit(self, 170.0, true, 16.0, 0.0, 50.0);
        }
        if hero.dive_started {
            hit(self, 200.0, false, 22.0, 12.0, 30.0);
        }
        if hero.pound_land {
            self.fx_push(3, hero.pos, 300.0, 10);
            hit(self, 300.0, false, 45.0, 36.0, 0.0);
        }
        // Coins.
        for p in self.pickups.iter_mut() {
            if p.alive && (p.pos[0] - hero.pos[0]).hypot(p.pos[1] - hero.pos[1]) <= PICKUP_RADIUS && hero.height < 160.0 {
                p.alive = false;
                self.coins += p.value;
                self.earned += p.value;
                self.ev.pickup = true;
            }
        }
    }

    fn update_misc(&mut self) {
        for p in self.pickups.iter_mut() {
            if p.alive {
                p.ttl = p.ttl.saturating_sub(1);
                p.alive = p.ttl > 0;
            }
        }
        for b in self.beams.iter_mut() {
            if b.alive {
                b.ttl = b.ttl.saturating_sub(1);
                b.alive = b.ttl > 0;
            }
        }
        for f in self.fx.iter_mut() {
            if f.alive {
                f.ttl = f.ttl.saturating_sub(1);
                f.alive = f.ttl > 0;
            }
        }
    }

    // ----- demo / test bot ------------------------------------------------------------------

    /// A simple automatic builder (screenshots, soak tests): every call spends coins on the best
    /// uncovered spot, upgrading when the map is full of towers or coins pile up.
    pub fn bot_step(&mut self) {
        let want = [TKind::Bombard, TKind::Bloom, TKind::Prism, TKind::Bloom, TKind::Slab, TKind::Eye, TKind::Prism, TKind::Bombard, TKind::Spout, TKind::Bloom];
        let n = self.tower_count();
        let kind = want[n % want.len()];
        let target = (5 + self.wave as usize * 4 / 5).min(20);
        if n < target {
            if self.coins < kind.spec(0).cost {
                return;
            }
            let range = kind.spec(1).range.max(300.0);
            let mut best: Option<((i32, i32), u32)> = None;
            for y in 0..self.map.h {
                for x in 0..self.map.w {
                    if self.can_build(kind, (x, y)).is_ok() {
                        let mut score = self.map.coverage(x, y, range * 0.9);
                        if kind == TKind::Spout {
                            score = 1000 - self.map.coverage(x, y, 600.0).min(900);
                        }
                        // Prefer spreading out.
                        score = score * 4 + (x as u32 + y as u32) % 3;
                        if best.is_none_or(|b| score > b.1) {
                            best = Some(((x, y), score));
                        }
                    }
                }
            }
            if let Some((cell, _)) = best {
                let _ = self.build(kind, cell);
                return;
            }
        }
        // Otherwise upgrade the cheapest upgrade available.
        let mut pick: Option<(usize, u32)> = None;
        for i in 0..TOWER_SLOTS {
            if let Some(c) = self.upgrade_cost(i) {
                if self.coins >= c && pick.is_none_or(|p| c < p.1) {
                    pick = Some((i, c));
                }
            }
        }
        if let Some((i, _)) = pick {
            let _ = self.upgrade(i);
        }
    }
}

/// Ticks between special abilities (0 = none).
fn ability_period(k: EKind) -> u16 {
    match k {
        EKind::Koopa => 150,
        EKind::KingBomb => 180,
        EKind::Lakitu => 45,
        EKind::Bully => 150,
        EKind::Whomp => 200,
        EKind::Chomp => 150,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> HeroFrame {
        HeroFrame { pos: [1.0e6, 1.0e6], ..Default::default() }
    }

    fn run(g: &mut Gate, ticks: u32) {
        let h = quiet();
        for _ in 0..ticks {
            g.tick(&h);
        }
    }

    /// The `skip`-th best buildable cell by road coverage.
    fn spot(g: &Gate, skip: usize) -> (i32, i32) {
        let mut cells = vec![];
        for y in 0..g.map.h {
            for x in 0..g.map.w {
                if g.map.tile(x, y) == Some(Tile::Grass) {
                    cells.push(((x, y), g.map.coverage(x, y, 500.0)));
                }
            }
        }
        cells.sort_by_key(|c| std::cmp::Reverse(c.1));
        cells[skip].0
    }

    #[test]
    fn maps_have_roads_rocks_and_a_gate() {
        for id in 0..MAPS.len() {
            let m = Map::new(id);
            assert!(!m.lanes.is_empty() && m.lanes.len() == m.air.len());
            assert!(m.tiles.iter().filter(|t| **t == Tile::Path).count() > 20);
            assert!(m.tiles.iter().any(|t| *t == Tile::Rock));
            for lane in &m.lanes {
                assert!(lane.len > 3000.0);
                assert_eq!(*lane.pts.last().unwrap(), m.gate);
                for p in &lane.pts[1..] {
                    let (x, y) = m.cell_at(*p).unwrap();
                    assert_eq!(m.tile(x, y), Some(Tile::Path));
                }
            }
            let c = m.center(5, 4);
            assert_eq!(m.cell_at(c), Some((5, 4)));
        }
    }

    #[test]
    fn lane_sampling_follows_the_polyline_and_steps_back() {
        let lane = Lane::new(vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]]);
        let mut seg = 0;
        let (p, d) = lane.sample(&mut seg, 50.0, 0.0);
        assert_eq!((p, d, seg), ([50.0, 0.0], [1.0, 0.0], 0));
        let (p, _) = lane.sample(&mut seg, 150.0, 0.0);
        assert_eq!((p[0].round(), p[1].round(), seg), (100.0, 50.0, 1));
        let (p, _) = lane.sample(&mut seg, 20.0, 0.0);
        assert_eq!((p, seg), ([20.0, 0.0], 0));
        let (p, _) = lane.sample(&mut seg, 50.0, 10.0);
        assert_eq!(p, [50.0, 10.0]);
    }

    #[test]
    fn enemies_walk_the_road_and_leak_costing_lives() {
        let mut g = Gate::new(1, 0);
        let i = g.spawn_enemy(EKind::Goomba, 0, 0.0, 0.0).unwrap();
        let start = g.enemies[i].pos;
        run(&mut g, 60);
        let moved = (g.enemies[i].pos[0] - start[0]).hypot(g.enemies[i].pos[1] - start[1]);
        assert!(moved > 150.0, "{moved}");
        let len = g.map.lanes[0].len;
        g.enemies[i].off = len - 1.0;
        run(&mut g, 2);
        assert!(!g.enemies[i].alive);
        assert_eq!(g.lives, START_LIVES - 1);
        assert_eq!(g.alive_enemies, 0);
    }

    #[test]
    fn boss_leak_costs_more_and_zero_lives_loses() {
        let mut g = Gate::new(1, 0);
        g.lives = 3;
        let i = g.spawn_enemy(EKind::KingBomb, 0, 0.0, 0.0).unwrap();
        g.enemies[i].off = g.map.lanes[0].len - 1.0;
        run(&mut g, 2);
        assert_eq!(g.phase, Phase::Lost);
        assert!(g.lives <= 0);
        let t = g.tick;
        run(&mut g, 10);
        assert_eq!(g.tick, t, "frozen after the end");
    }

    #[test]
    fn flyers_cross_obstacles_on_a_straight_line() {
        let mut g = Gate::new(1, 0);
        let b = g.spawn_enemy(EKind::Boo, 0, 0.0, 0.0).unwrap();
        let w = g.spawn_enemy(EKind::Goomba, 0, 0.0, 0.0).unwrap();
        assert!(g.enemies[b].air && !g.enemies[w].air);
        assert!(g.map.air[0].len < g.map.lanes[0].len * 0.6);
        let mut seg = 0;
        let mut over_rock = false;
        let mut d = 0.0;
        while d < g.map.air[0].len {
            let (p, _) = g.map.air[0].sample(&mut seg, d, 0.0);
            if let Some((x, y)) = g.map.cell_at(p) {
                over_rock |= g.map.tile(x, y) == Some(Tile::Rock);
            }
            d += 40.0;
        }
        assert!(over_rock);
    }

    #[test]
    fn build_validates_tiles_coins_and_occupancy() {
        let mut g = Gate::new(1, 0);
        let road = g.map.cell_at(g.map.lanes[0].pts[1]).unwrap();
        assert_eq!(g.can_build(TKind::Bombard, road), Err(BuildError::NotBuildable));
        assert_eq!(g.can_build(TKind::Bombard, (0, 5)), Err(BuildError::NotBuildable), "rock");
        assert_eq!(g.can_build(TKind::Bombard, (-3, 2)), Err(BuildError::NotBuildable));
        let c = spot(&g, 0);
        assert!(g.build(TKind::Bombard, c).is_ok());
        assert_eq!(g.coins, START_COINS - 100);
        assert_eq!(g.can_build(TKind::Bloom, c), Err(BuildError::Occupied));
        g.coins = 10;
        assert_eq!(g.can_build(TKind::Bloom, spot(&g, 1)), Err(BuildError::NoCoins));
    }

    #[test]
    fn upgrade_and_sell_economy() {
        let mut g = Gate::new(1, 0);
        g.coins = 1000;
        let c = spot(&g, 0);
        let t = g.build(TKind::Prism, c).unwrap();
        assert_eq!(g.upgrade_cost(t), Some(130));
        g.upgrade(t).unwrap();
        g.upgrade(t).unwrap();
        assert_eq!(g.towers[t].level, 2);
        assert_eq!(g.upgrade_cost(t), None);
        assert_eq!(g.upgrade(t), Err(BuildError::NotBuildable));
        assert_eq!(g.coins, 1000 - 160 - 130 - 220);
        assert_eq!(g.towers[t].spent, 510);
        assert_eq!(g.sell_value(t), 357);
        let before = g.coins;
        assert_eq!(g.sell(t), 357);
        assert_eq!(g.coins, before + 357);
        assert!(!g.towers[t].alive);
        assert!(g.can_build(TKind::Prism, c).is_ok());
        let t = g.build(TKind::Prism, c).unwrap();
        g.coins = 5;
        assert_eq!(g.upgrade(t), Err(BuildError::NoCoins));
    }

    #[test]
    fn tower_cap_is_enforced() {
        let mut g = Gate::new(1, 0);
        g.coins = 100_000;
        let mut placed = 0;
        for y in 0..g.map.h {
            for x in 0..g.map.w {
                if g.build(TKind::Spout, (x, y)).is_ok() {
                    placed += 1;
                }
            }
        }
        assert_eq!(placed, MAX_TOWERS);
    }

    fn put(g: &mut Gate, kind: EKind, off: f32, hp: f32) -> usize {
        let i = g.spawn_enemy(kind, 0, off, 0.0).unwrap();
        g.enemies[i].hp = hp;
        g.enemies[i].max_hp = hp;
        g.enemies[i].speed = 0.0;
        i
    }

    #[test]
    fn targeting_modes_pick_first_last_strongest_closest() {
        let mut g = Gate::new(1, 0);
        g.coins = 1000;
        let c = g.map.cell_at([g.map.lanes[0].pts[1][0] + 200.0, g.map.lanes[0].pts[1][1] + 220.0]).unwrap();
        let t = g.build(TKind::Prism, c).unwrap();
        g.towers[t].level = 2;
        let front = put(&mut g, EKind::Goomba, 1000.0, 1.0e6);
        let back = put(&mut g, EKind::Goomba, 800.0, 1.0e6);
        let tank = put(&mut g, EKind::Spiny, 900.0, 2.0e6);
        let pos = g.towers[t].pos;
        let d = |g: &Gate, i: usize| (g.enemies[i].pos[0] - pos[0]).hypot(g.enemies[i].pos[1] - pos[1]);
        let closest = [front, back, tank].into_iter().min_by(|a, b| d(&g, *a).partial_cmp(&d(&g, *b)).unwrap()).unwrap();
        assert!(d(&g, front) < 850.0 && d(&g, back) < 850.0 && d(&g, tank) < 850.0);
        g.towers[t].mode = Target::First;
        assert_eq!(g.pick_target(t), Some(front));
        g.towers[t].mode = Target::Last;
        assert_eq!(g.pick_target(t), Some(back));
        g.towers[t].mode = Target::Strongest;
        assert_eq!(g.pick_target(t), Some(tank));
        g.towers[t].mode = Target::Closest;
        assert_eq!(g.pick_target(t), Some(closest));
        g.towers[t].pos = [1.0e5, 1.0e5];
        assert_eq!(g.pick_target(t), None, "out of range");
        assert_eq!(Target::First.next().next().next().next(), Target::First);
    }

    #[test]
    fn ground_only_towers_ignore_flyers() {
        let mut g = Gate::new(1, 0);
        g.coins = 1000;
        let bomb = g.build(TKind::Bombard, spot(&g, 0)).unwrap();
        let bloom = g.build(TKind::Bloom, spot(&g, 1)).unwrap();
        let b = g.spawn_enemy(EKind::Boo, 0, 0.0, 0.0).unwrap();
        g.enemies[b].pos = g.towers[bomb].pos;
        g.towers[bloom].pos = g.towers[bomb].pos;
        assert_eq!(g.pick_target(bomb), None);
        assert_eq!(g.pick_target(bloom), Some(b));
    }

    #[test]
    fn armor_reduces_hits_but_never_below_twenty_percent_and_burn_ignores_it() {
        let mut g = Gate::new(1, 0);
        let i = g.spawn_enemy(EKind::Bully, 0, 0.0, 0.0).unwrap();
        let hp = g.enemies[i].hp;
        g.damage(i, 10.0, false, NO_SRC);
        assert!((hp - g.enemies[i].hp - 6.0).abs() < 1e-4);
        let hp = g.enemies[i].hp;
        g.damage(i, 3.0, false, NO_SRC);
        assert!((hp - g.enemies[i].hp - 0.6).abs() < 1e-4, "floor of 20%");
        let hp = g.enemies[i].hp;
        g.damage(i, 3.0, true, NO_SRC);
        assert!((hp - g.enemies[i].hp - 3.0).abs() < 1e-4);
    }

    #[test]
    fn splash_hits_clusters_with_falloff_and_skips_flyers() {
        let mut g = Gate::new(1, 0);
        g.coins = 1000;
        let t = g.build(TKind::Bombard, spot(&g, 0)).unwrap();
        g.towers[t].cd = 1.0e9;
        let a = put(&mut g, EKind::Bully, 900.0, 5000.0);
        let b = put(&mut g, EKind::Bully, 960.0, 5000.0);
        let far = put(&mut g, EKind::Bully, 2400.0, 5000.0);
        let fly = put(&mut g, EKind::Boo, 600.0, 5000.0);
        let dest = g.enemies[a].pos;
        g.enemies[fly].pos = dest;
        let shell = Proj { alive: true, kind: 1, pos: dest, prev: dest, dest, target: 0, t: 1, total: 1, damage: 100.0, radius: 120.0, burn: 0.0, src: t as u8 };
        g.spawn_proj(shell);
        g.tick(&quiet());
        assert!(g.ev.splash);
        assert!(g.enemies[a].hp < g.enemies[b].hp + 1.0 && g.enemies[b].hp < 5000.0, "both in the blast, centre hit hardest");
        assert!(g.enemies[a].hp < 5000.0 - 90.0 + 10.0, "armor 4 trims a 100 hit: {}", g.enemies[a].hp);
        assert_eq!(g.enemies[far].hp, 5000.0);
        assert_eq!(g.enemies[fly].hp, 5000.0, "flyers are not splashed");
    }

    #[test]
    fn dread_eye_slows_and_bosses_resist() {
        let mut g = Gate::new(1, 0);
        g.coins = 1000;
        let t = g.build(TKind::Eye, spot(&g, 0)).unwrap();
        let w = put(&mut g, EKind::Goomba, 0.0, 100.0);
        let k = put(&mut g, EKind::KingBomb, 0.0, 100.0);
        g.enemies[w].pos = g.towers[t].pos;
        g.enemies[k].pos = g.towers[t].pos;
        g.towers[t].cd = 0.0;
        g.tick(&quiet());
        assert!((g.enemies[w].slow - 0.72).abs() < 1e-4, "{}", g.enemies[w].slow);
        assert!((g.enemies[k].slow - 0.86).abs() < 1e-4);
        g.enemies[w].pos = [1.0e5, 1.0e5];
        run(&mut g, 8);
        assert_eq!(g.enemies[w].slow, 1.0, "wears off out of range");
    }

    #[test]
    fn burn_damages_over_time_and_spares_the_fireproof() {
        let mut g = Gate::new(1, 0);
        let a = put(&mut g, EKind::Goomba, 100.0, 1000.0);
        let s = put(&mut g, EKind::Spiny, 100.0, 1000.0);
        for i in [a, s] {
            g.apply_burn(i, 30.0);
        }
        run(&mut g, 90);
        assert!((1000.0 - g.enemies[a].hp - 90.0).abs() < 1.0, "{}", g.enemies[a].hp);
        assert_eq!(g.enemies[s].hp, 1000.0);
        run(&mut g, 30);
        assert!((1000.0 - g.enemies[a].hp - 90.0).abs() < 1.0, "burn expired");
    }

    #[test]
    fn crush_slab_stuns_ground_but_not_air() {
        let mut g = Gate::new(1, 0);
        g.coins = 1000;
        let t = g.build(TKind::Slab, spot(&g, 0)).unwrap();
        let w = put(&mut g, EKind::Bully, 100.0, 1000.0);
        let f = put(&mut g, EKind::Boo, 100.0, 1000.0);
        g.enemies[w].pos = g.towers[t].pos;
        g.enemies[f].pos = g.towers[t].pos;
        g.towers[t].cd = 0.0;
        g.tick(&quiet());
        assert!(g.ev.slam);
        assert!(g.enemies[w].hp < 1000.0 && g.enemies[w].stun_t > 10);
        assert_eq!(g.enemies[f].hp, 1000.0);
        g.enemies[w].speed = 3.0;
        let off = g.enemies[w].off;
        run(&mut g, 5);
        assert_eq!(g.enemies[w].off, off, "a stunned enemy does not advance");
    }

    #[test]
    fn prism_pierces_every_enemy_on_the_line() {
        let mut g = Gate::new(1, 1);
        g.coins = 1000;
        // West of the merge, in line with the long straight road (row 5).
        let t = g.build(TKind::Prism, (5, 5)).unwrap();
        g.towers[t].level = 2;
        g.towers[t].cd = 0.0;
        let mut ids = vec![];
        for k in 0..3 {
            let i = g.spawn_enemy(EKind::Goomba, 0, 2420.0 + 220.0 * k as f32, 0.0).unwrap();
            g.enemies[i].hp = 5000.0;
            g.enemies[i].max_hp = 5000.0;
            g.enemies[i].speed = 0.0;
            ids.push(i);
        }
        let off_line = g.spawn_enemy(EKind::Goomba, 1, 1500.0, 0.0).unwrap();
        g.enemies[off_line].hp = 5000.0;
        g.enemies[off_line].speed = 0.0;
        g.tick(&quiet());
        assert!(g.ev.beam && g.beams.iter().any(|b| b.alive));
        for i in ids {
            assert!(g.enemies[i].hp < 5000.0, "slot {i} pierced");
        }
        assert_eq!(g.enemies[off_line].hp, 5000.0);
    }

    #[test]
    fn each_tower_kind_reports_its_own_shot_for_sound() {
        let mut g = Gate::new(1, 1);
        g.coins = 5000;
        let prism = g.build(TKind::Prism, (5, 5)).unwrap();
        g.towers[prism].cd = 0.0;
        let i = g.spawn_enemy(EKind::Goomba, 0, 2420.0, 0.0).unwrap();
        let ep = g.enemies[i].pos;
        let range = TKind::Slab.spec(0).range;
        let cell = (0..g.map.h)
            .flat_map(|y| (0..g.map.w).map(move |x| (x, y)))
            .find(|&(x, y)| g.can_build(TKind::Slab, (x, y)).is_ok() && (g.map.center(x, y)[0] - ep[0]).hypot(g.map.center(x, y)[1] - ep[1]) < range * 0.8)
            .expect("a grass cell beside the road");
        let slab = g.build(TKind::Slab, cell).unwrap();
        g.towers[slab].cd = 0.0;
        g.enemies[i].hp = 5000.0;
        g.enemies[i].max_hp = 5000.0;
        g.enemies[i].speed = 0.0;
        g.tick(&quiet());
        assert_ne!(g.ev.fired_kinds & (1 << TKind::Prism as u8), 0);
        assert_ne!(g.ev.fired_kinds & (1 << TKind::Slab as u8), 0);
        assert_eq!(g.ev.fired_kinds & (1 << TKind::Bombard as u8), 0, "no bombard was built");
        // Events are per tick: an idle tick reports nothing.
        let mut idle = Gate::new(1, 1);
        idle.tick(&quiet());
        assert_eq!(idle.ev.fired_kinds, 0);
    }

    #[test]
    fn coin_spout_pays_over_time_and_rally_boosts_it() {
        let mut g = Gate::new(1, 0);
        let t = g.build(TKind::Spout, spot(&g, 0)).unwrap();
        g.towers[t].cd = 0.0;
        let c0 = g.coins;
        run(&mut g, 91);
        assert_eq!(g.coins - c0, 2 * 2, "two payouts of two");
        let hero = HeroFrame { pos: g.towers[t].pos, ..Default::default() };
        let c1 = g.coins;
        for _ in 0..100 {
            g.tick(&hero);
        }
        assert!(g.coins - c1 >= 3, "rallied payout is 3 (1.5x)");
    }

    #[test]
    fn kills_pay_bounty_drop_coins_and_the_hero_collects_them() {
        let mut g = Gate::new(1, 0);
        let i = g.spawn_enemy(EKind::Bully, 0, 300.0, 0.0).unwrap();
        let pos = g.enemies[i].pos;
        let c0 = g.coins;
        assert!(g.damage(i, 1.0e6, false, NO_SRC));
        assert_eq!(g.coins, c0 + 14);
        assert_eq!(g.kills, 1);
        assert!(g.pickups.iter().any(|p| p.alive && p.value == 7));
        let hero = HeroFrame { pos, ..Default::default() };
        g.tick(&hero);
        assert!(g.ev.pickup);
        assert_eq!(g.coins, c0 + 14 + 7);
        assert!(!g.pickups.iter().any(|p| p.alive));
        let i = g.spawn_enemy(EKind::Bully, 0, 300.0, 0.0).unwrap();
        g.damage(i, 1.0e6, false, NO_SRC);
        run(&mut g, PICKUP_TTL as u32 + 2);
        assert!(!g.pickups.iter().any(|p| p.alive), "uncollected coins expire");
    }

    #[test]
    fn hero_punch_dive_and_pound_hurt_nearby_foes_and_flyers_need_height() {
        let mut g = Gate::new(1, 0);
        g.wave = 1;
        let a = put(&mut g, EKind::Bully, 300.0, 1000.0);
        let f = put(&mut g, EKind::Boo, 300.0, 1000.0);
        let p = g.enemies[a].pos;
        g.enemies[f].pos = p;
        let hero = HeroFrame { pos: [p[0], p[1] - 80.0], face: 0.0, punch_started: true, ..Default::default() };
        g.tick(&hero);
        assert!(g.enemies[a].hp < 1000.0 && g.ev.hero_hit);
        assert_eq!(g.enemies[f].hp, 1000.0, "flyer out of reach on the ground");
        let p = g.enemies[a].pos;
        let hp = g.enemies[a].hp;
        let behind = HeroFrame { pos: [p[0], p[1] - 80.0], face: std::f32::consts::PI, punch_started: true, ..Default::default() };
        g.tick(&behind);
        assert_eq!(g.enemies[a].hp, hp, "behind the hero");
        let pf = g.enemies[f].pos;
        let pound = HeroFrame { pos: pf, height: 80.0, pound_land: true, ..Default::default() };
        g.tick(&pound);
        assert!(g.enemies[a].stun_t > 0 || g.enemies[a].hp < hp);
        assert!(g.enemies[f].hp < 1000.0, "airborne pound reaches flyers");
    }

    #[test]
    fn rally_speeds_fire_and_pound_overdrives_and_unjams() {
        let mut g = Gate::new(1, 0);
        g.coins = 1000;
        let t = g.build(TKind::Bloom, spot(&g, 0)).unwrap();
        let pos = g.towers[t].pos;
        g.towers[t].jam = 300;
        let near = HeroFrame { pos, ..Default::default() };
        g.tick(&near);
        assert!(g.towers[t].rally);
        assert_eq!(g.towers[t].jam, 296, "four times as fast");
        let pound = HeroFrame { pos, pound_land: true, ..Default::default() };
        g.tick(&pound);
        assert_eq!(g.towers[t].jam, 0);
        assert!(g.towers[t].od_t > 100 && g.ev.overdrive);
        g.towers[t].od_t = 0;
        g.tick(&pound);
        assert_eq!(g.towers[t].od_t, 0, "overdrive is on cooldown");
        g.tick(&quiet());
        assert!(!g.towers[t].rally);
    }

    #[test]
    fn bully_jams_towers_and_jammed_towers_do_not_fire() {
        let mut g = Gate::new(1, 0);
        g.coins = 1000;
        let t = g.build(TKind::Bloom, spot(&g, 0)).unwrap();
        let b = put(&mut g, EKind::Bully, 100.0, 1.0e6);
        g.enemies[b].pos = g.towers[t].pos;
        g.enemies[b].cd = 0;
        g.towers[t].cd = 0.0;
        g.tick(&quiet());
        assert!(g.towers[t].jam > 0 && g.ev.jammed);
        let before = g.projs.iter().filter(|p| p.alive).count();
        g.tick(&quiet());
        assert_eq!(g.projs.iter().filter(|p| p.alive).count(), before, "no new shots while jammed");
    }

    #[test]
    fn healer_heals_allies_and_summoner_calls_goombas() {
        let mut g = Gate::new(1, 0);
        let l = put(&mut g, EKind::Lakitu, 100.0, 100.0);
        let w = put(&mut g, EKind::Bully, 100.0, 1000.0);
        g.enemies[w].pos = g.enemies[l].pos;
        g.enemies[w].hp = 10.0;
        g.enemies[l].cd = 0;
        g.tick(&quiet());
        assert!(g.enemies[w].hp > 10.0);
        let k = put(&mut g, EKind::Koopa, 400.0, 1.0e6);
        g.enemies[k].cd = 0;
        let before = g.alive_enemies;
        g.tick(&quiet());
        assert_eq!(g.alive_enemies, before + 2);
        g.cap = g.alive_enemies;
        g.enemies[k].cd = 0;
        g.tick(&quiet());
        assert_eq!(g.alive_enemies, before + 2, "never above the cap");
    }

    #[test]
    fn wave_table_is_complete_and_bosses_arrive_every_fifth() {
        assert_eq!(WAVE_TABLE.len(), 15);
        for w in 1..=WAVES {
            let p = preview(w);
            assert!(p.n >= 1 && p.n <= 6, "wave {w}");
            assert_eq!(p.boss, w % 5 == 0, "wave {w}");
        }
        assert_eq!(preview(16), Preview::default());
        assert!(preview(6).air && !preview(1).air);
        let total = |w: u32| -> f32 { WAVE_TABLE[w as usize - 1].iter().map(|g| g.count as f32 * estats(g.kind).hp).sum::<f32>() * wave_hp_mult(w) };
        assert!(total(4) > total(1) && total(9) > total(4) && total(14) > total(9));
        assert!(wave_hp_mult(15) > 3.5 && wave_hp_mult(1) == 1.0);
    }

    #[test]
    fn waves_start_on_the_timer_spawn_in_groups_and_clear_pays() {
        let mut g = Gate::new(7, 0);
        g.god = true;
        assert_eq!(g.wave, 0);
        run(&mut g, FIRST_WAVE_SECS * TICKS_PER_SEC - 1);
        assert_eq!(g.wave, 0);
        run(&mut g, 1);
        assert_eq!(g.wave, 1);
        assert!(g.spawning);
        run(&mut g, 10 * 40);
        assert!(!g.spawning);
        assert!(g.timer <= INTERWAVE_SECS * TICKS_PER_SEC && g.timer > 600);
        for i in 0..ENEMY_SLOTS {
            if g.enemies[i].alive {
                g.damage(i, 1.0e6, true, NO_SRC);
            }
        }
        let c = g.coins;
        run(&mut g, 1);
        assert_eq!(g.waves_cleared, 1);
        assert!(g.ev.wave_cleared);
        assert_eq!(g.coins, c + 26);
    }

    #[test]
    fn calling_the_wave_early_pays_a_bonus_once() {
        let mut g = Gate::new(7, 0);
        let c = g.coins;
        assert!(g.call_wave());
        assert_eq!(g.wave, 1);
        assert_eq!(g.coins, c + 40 / 3);
        assert!(!g.call_wave(), "not while spawning");
    }

    #[test]
    fn no_towers_loses_in_bounded_time() {
        let mut g = Gate::new(3, 0);
        let h = quiet();
        let mut t = 0;
        while !g.over() && t < 30 * 60 * 20 {
            g.tick(&h);
            t += 1;
        }
        assert_eq!(g.phase, Phase::Lost);
        assert!(g.wave >= 2 && g.wave < WAVES, "wave {}", g.wave);
    }

    fn bot_run(seed: u64, map: usize) -> Gate {
        let mut g = Gate::new(seed, map);
        let h = quiet();
        let mut t = 0u32;
        while !g.over() && t < 30 * 60 * 25 {
            if t % 20 == 0 {
                g.bot_step();
            }
            if t % 30 == 0 && !g.spawning && g.timer > 150 && g.alive_enemies == 0 {
                g.call_wave();
            }
            g.tick(&h);
            t += 1;
        }
        g
    }

    #[test]
    fn a_simple_bot_wins_every_map_without_the_hero() {
        for map in 0..MAPS.len() {
            // Several seeds per map: sensible play (no hero help) must always be enough, with a few lives to spare.
            for seed in [11u64, 12, 13, 14] {
                let g = bot_run(seed, map);
                assert_eq!(g.phase, Phase::Won, "map {map} seed {seed}: wave {} cleared {} lives {} towers {}", g.wave, g.waves_cleared, g.lives, g.tower_count());
                assert_eq!(g.waves_cleared, WAVES);
                assert!(g.lives >= 3, "map {map} seed {seed}: only {} lives left", g.lives);
                assert!(g.kills > 400);
            }
        }
    }

    #[test]
    fn every_map_can_be_lost_too() {
        for map in 0..MAPS.len() {
            let mut g = Gate::new(5, map);
            let h = quiet();
            let mut t = 0u32;
            // A bot that stops building after the first waves is overrun and the run ends in Lost.
            while !g.over() && t < 30 * 60 * 25 {
                if t % 20 == 0 && g.wave < 4 {
                    g.bot_step();
                }
                if t % 30 == 0 && !g.spawning && g.timer > 150 && g.alive_enemies == 0 {
                    g.call_wave();
                }
                g.tick(&h);
                t += 1;
            }
            assert_eq!(g.phase, Phase::Lost, "map {map}: wave {}", g.wave);
            assert!(g.lives <= 0);
        }
    }

    /// `cargo test --release balance_report -- --ignored --nocapture`: bot results over several seeds per map.
    #[test]
    #[ignore]
    fn balance_report() {
        for map in 0..2 {
            let mut won = 0;
            let (mut lives, mut cleared) = (0, 0);
            for seed in 1..=12u64 {
                let g = bot_run(seed, map);
                won += (g.phase == Phase::Won) as i32;
                lives += g.lives.max(0);
                cleared += g.waves_cleared;
            }
            let mut at14 = (0, 0);
            {
                let mut g = Gate::new(1, map);
                let h = quiet();
                let mut t = 0u32;
                while !g.over() && g.wave < 15 && t < 30 * 60 * 25 {
                    if t % 20 == 0 {
                        g.bot_step();
                    }
                    if t % 30 == 0 && !g.spawning && g.timer > 150 && g.alive_enemies == 0 {
                        g.call_wave();
                    }
                    g.tick(&h);
                    t += 1;
                }
                at14 = (g.lives, g.coins);
            }
            println!("  seed 1 lives/coins when wave 15 starts: {at14:?}");
            let one = bot_run(1, map);
            println!("map {map}: won {won}/12, avg lives left {:.1}, avg waves cleared {:.1}; seed 1 ended wave {} with {} towers, {} lives", lives as f64 / 12.0, cleared as f64 / 12.0, one.wave, one.tower_count(), one.lives);
        }
    }


    #[test]
    fn the_final_waves_are_not_free() {
        let mut g = Gate::new(5, 0);
        let h = quiet();
        let mut t = 0u32;
        while !g.over() && t < 30 * 60 * 25 {
            if t % 20 == 0 && g.wave < 6 {
                g.bot_step();
            }
            g.tick(&h);
            t += 1;
        }
        assert_eq!(g.phase, Phase::Lost, "wave {}", g.wave);
    }

    fn fingerprint(g: &Gate) -> (u32, u32, i32, u32, u32, u32, u64) {
        let hp: f64 = g.enemies.iter().filter(|e| e.alive).map(|e| e.hp as f64 + e.off as f64).sum();
        (g.tick, g.wave, g.lives, g.coins, g.kills, g.alive_enemies as u32, hp.to_bits())
    }

    #[test]
    fn a_full_simulated_run_is_deterministic_and_seed_dependent() {
        let a = bot_run(21, 1);
        let b = bot_run(21, 1);
        assert_eq!(fingerprint(&a), fingerprint(&b));
        let c = bot_run(22, 1);
        assert_ne!(fingerprint(&a), fingerprint(&c));
        let hero_run = |seed| {
            let mut g = Gate::new(seed, 0);
            g.timer = 30;
            for t in 0..4000u32 {
                if t % 20 == 0 {
                    g.bot_step();
                }
                let a = t as f32 * 0.02;
                g.tick(&HeroFrame { pos: [a.cos() * 900.0, a.sin() * 600.0], punch_started: t % 37 == 0, pound_land: t % 211 == 0, ..Default::default() });
            }
            fingerprint(&g)
        };
        assert_eq!(hero_run(9), hero_run(9));
    }

    #[test]
    fn pausing_freezes_the_core_and_god_mode_never_loses() {
        let mut g = Gate::new(1, 0);
        g.paused = true;
        run(&mut g, 100);
        assert_eq!(g.tick, 0);
        g.paused = false;
        g.god = true;
        g.lives = 1;
        let i = g.spawn_enemy(EKind::Goomba, 0, 0.0, 0.0).unwrap();
        g.enemies[i].off = g.map.lanes[0].len - 1.0;
        run(&mut g, 3);
        assert_eq!(g.phase, Phase::Playing);
    }

    #[test]
    fn enemy_cap_holds_while_spawning() {
        let mut g = Gate::new(1, 0);
        g.cap = 5;
        g.god = true;
        g.timer = 1;
        run(&mut g, 400);
        assert!(g.alive_enemies <= 5);
    }

    #[test]
    fn tick_cost_with_150_enemies_and_30_towers_is_small() {
        let mut g = Gate::new(1, 1);
        g.coins = 1_000_000;
        g.god = true;
        g.cap = 190;
        let kinds = [TKind::Bombard, TKind::Slab, TKind::Bloom, TKind::Eye, TKind::Prism, TKind::Spout];
        let mut n = 0;
        'o: for y in 0..g.map.h {
            for x in 0..g.map.w {
                if g.map.coverage(x, y, 500.0) > 6 && g.build(kinds[n % 6], (x, y)).is_ok() {
                    let ti = g.tower_at((x, y)).unwrap();
                    g.towers[ti].level = (n % 3) as u8;
                    n += 1;
                    if n == 30 {
                        break 'o;
                    }
                }
            }
        }
        assert_eq!(n, 30);
        let refill = |g: &mut Gate| {
            let mut k = 0;
            while g.alive_enemies < 150 {
                let kind = [EKind::Goomba, EKind::Bully, EKind::Spiny, EKind::Boo][k % 4];
                let off = (k as f32 * 37.0) % 3000.0;
                if let Some(i) = g.spawn_enemy(kind, k, off, 0.0) {
                    g.enemies[i].hp = 1.0e9;
                    g.enemies[i].max_hp = 1.0e9;
                }
                k += 1;
            }
        };
        refill(&mut g);
        let h = quiet();
        let start = std::time::Instant::now();
        let ticks = 300;
        for _ in 0..ticks {
            refill(&mut g);
            g.tick(&h);
        }
        let per = start.elapsed().as_secs_f64() * 1000.0 / ticks as f64;
        assert!(per < 4.0, "tick cost {per} ms");
    }
}
