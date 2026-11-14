//! Engine-free building blocks shared by the party boards and mini-games: a seeded RNG, bot skill
//! levels, the per-player input record, the draw list the view renders, and ranking helpers.

/// Small deterministic generator (xorshift64*), seeded per game / mini-game.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng(seed ^ 0x9e37_79b9_7f4a_7c15);
        if r.0 == 0 {
            r.0 = 1;
        }
        for _ in 0..3 {
            r.next_u64();
        }
        r
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

/// CPU skill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Level {
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Easy, Level::Normal, Level::Hard];
    pub fn label(self) -> &'static str {
        match self {
            Level::Easy => "Easy",
            Level::Normal => "Normal",
            Level::Hard => "Hard",
        }
    }
    pub fn from_name(s: &str) -> Option<Level> {
        match s.to_ascii_lowercase().as_str() {
            "easy" | "0" => Some(Level::Easy),
            "normal" | "1" => Some(Level::Normal),
            "hard" | "2" => Some(Level::Hard),
            _ => None,
        }
    }
    /// Probability-like skill in 0..1 (accuracy of aim / timing).
    pub fn skill(self) -> f32 {
        match self {
            Level::Easy => 0.5,
            Level::Normal => 0.75,
            Level::Hard => 0.93,
        }
    }
    /// Reaction delay in seconds.
    pub fn react(self) -> f32 {
        match self {
            Level::Easy => 0.55,
            Level::Normal => 0.32,
            Level::Hard => 0.18,
        }
    }
}

/// One player's input for one step. `act` is an edge (pressed this step); `held` the level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadIn {
    pub x: f32,
    /// +y is "down the screen" (toward the camera), matching the stage's +z.
    pub y: f32,
    pub act: bool,
    pub held: bool,
    /// B pressed this step (punch).
    pub b: bool,
    /// Index of a tapped [`Tile`].
    pub pick: Option<usize>,
}

pub type Inputs = [PadIn; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sh {
    Box,
    /// A cylinder around the vertical axis: `s = [radius, height, radius]` (a zero third radius copies the first).
    Disc,
    Ball,
    Cone,
    /// A coin standing on its edge, spinning by `yaw`: `s = [radius, thickness, _]`.
    Coin,
}

/// A coloured primitive at a course position (SM64 units); sizes in stage units (one = 100 SM64 units).
#[derive(Clone, Debug)]
pub struct Shape {
    pub sh: Sh,
    pub p: [f64; 3],
    pub s: [f32; 3],
    pub yaw: f32,
    pub col: u32,
}

/// A tappable screen tile (fractions of the playfield, origin top-left). No current mini-game uses tiles; the UI keeps
/// the pool for touch menus.
#[derive(Clone, Debug)]
pub struct Tile {
    pub id: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub col: u32,
    pub text: String,
}

/// What a running mini-game shows: primitives and models at course positions, plus the HUD text.
#[derive(Clone, Debug, Default)]
pub struct Stage {
    pub shapes: Vec<Shape>,
    pub tiles: Vec<Tile>,
    /// Goombas: (position, heading in radians, scale).
    pub goombas: Vec<([f64; 3], f32, f32)>,
    /// Falling Bob-ombs.
    pub bombs: Vec<[f64; 3]>,
    /// A Power Star sitting here...
    pub star_at: Option<[f64; 3]>,
    /// ...or carried over this player's head.
    pub star_on: Option<usize>,
    /// The player wearing the crown.
    pub crown: Option<usize>,
    /// Large centred text ("GO!", "3").
    pub banner: String,
    /// Short status per player shown on its chip ("x3", "OUT").
    pub tags: [String; 4],
}

impl Stage {
    pub fn shape_w(&mut self, sh: Sh, p: [f64; 3], s: [f32; 3], col: u32) {
        self.shapes.push(Shape { sh, p, s, yaw: 0.0, col });
    }
    /// A spinning coin of radius `r` (stage units).
    pub fn coin(&mut self, p: [f64; 3], r: f32, spin: f32, col: u32) {
        self.shapes.push(Shape { sh: Sh::Coin, p, s: [r, 0.08, r], yaw: spin, col });
    }
}

/// Player colours (cap tints): red, blue, green, gold.
pub const PLAYER_COLORS: [u32; 4] = [0xe23b2e, 0x3d7bff, 0x2fbf5a, 0xf5c518];
pub const PLAYER_NAMES: [&str; 4] = ["You", "Rosa", "Teo", "Mina"];

/// Competition ranking (1 = best, ties share) among the active players; inactive get 0.
pub fn ranks(scores: &[f32; 4], active: &[bool; 4]) -> [u8; 4] {
    let mut r = [0u8; 4];
    for p in 0..4 {
        if !active[p] {
            continue;
        }
        r[p] = 1 + (0..4).filter(|&q| active[q] && scores[q] > scores[p] + 1e-4).count() as u8;
    }
    r
}

/// Shared bookkeeping for the active mask.
#[derive(Clone, Copy, Debug)]
pub struct Roster {
    pub active: [bool; 4],
}

impl Roster {
    pub fn new(active: [bool; 4]) -> Self {
        Roster { active }
    }
    pub fn ids(&self) -> impl Iterator<Item = usize> + '_ {
        (0..4).filter(|&p| self.active[p])
    }
    pub fn count(&self) -> usize {
        self.active.iter().filter(|a| **a).count()
    }
    /// Scores with inactive players masked.
    pub fn mask(&self, mut s: [f32; 4]) -> [f32; 4] {
        for p in 0..4 {
            if !self.active[p] {
                s[p] = f32::NEG_INFINITY;
            }
        }
        s
    }
}
