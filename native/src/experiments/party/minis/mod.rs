//! The mini-games: every one is played by four real Marios on a real spot of Bob-omb Battlefield (the summit, the
//! meadows, the bridge field...). Each game is an engine-free module with a deterministic seeded core and a bot policy:
//! the core reads where the four bodies are ([`Body`]), turns the controls into [`Drive`]s, and asks for knockbacks and
//! pick-ups ([`MiniEv`]); the app plays them on the full SM64 physics (`CrowdMario`), while tests and the board's headless
//! mode use [`Kin`], a small kinematic stand-in on the same [`Field`].
use super::common::*;
use std::sync::Arc;

pub mod bomb_rain;
pub mod coin_scramble;
pub mod field;
pub mod goomba_stomp;
pub mod star_tag;
pub mod summit_crown;

pub use field::*;

/// How a game is controlled (shown on the intro card).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Run with the stick, A jumps.
    Run,
    /// Run, A jumps, B punches (a punch in the air is a dive).
    Fight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mini {
    CoinScramble,
    SummitCrown,
    BombRain,
    StarTag,
    GoombaStomp,
}

pub const ALL_MINIS: [Mini; 5] = [Mini::CoinScramble, Mini::SummitCrown, Mini::BombRain, Mini::StarTag, Mini::GoombaStomp];

pub struct MiniInfo {
    pub key: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub control: Control,
    /// What the score means, for the results list.
    pub unit: &'static str,
    /// Where on the battlefield it is played.
    pub place: &'static str,
}

impl Mini {
    pub fn info(self) -> MiniInfo {
        use Control::*;
        let (key, name, blurb, control, unit, place) = match self {
            Mini::CoinScramble => ("coins", "Coin Scramble", "Coins rain on the hills. Grab the most: gold coins are worth three, and some sit on the ledges.", Run, "coins", "the hills east of the mountain"),
            Mini::SummitCrown => ("crown", "Summit Crown", "Hold the crown circle on the summit alone to score. Punch rivals out of it, and off the mountain.", Fight, "seconds", "the summit"),
            Mini::BombRain => ("bombs", "Bob-omb Rain", "Bob-ombs fall where the red rings grow. Dodge the blasts: three hits and you are out.", Run, "points", "the field of wooden posts north of the start"),
            Mini::StarTag => ("star", "Star Tag", "Grab the star and keep it: it scores while you hold it. Punch the holder to steal it.", Fight, "seconds", "the floating island"),
            Mini::GoombaStomp => ("stomp", "Goomba Stomp", "Goombas pour in and chase you. Jump on them to score; bounce from one onto the next for double. Walking into one hurts.", Run, "points", "the high meadow in the north-west"),
        };
        MiniInfo { key, name, blurb, control, unit, place }
    }

    pub fn from_key(s: &str) -> Option<Mini> {
        let s = s.to_ascii_lowercase();
        ALL_MINIS.iter().copied().find(|m| m.info().key == s || m.info().name.to_ascii_lowercase().replace([' ', '-'], "") == s.replace(['_', '-', ' '], ""))
    }

    pub fn idx(self) -> usize {
        ALL_MINIS.iter().position(|m| *m == self).unwrap()
    }

    /// Radius of the arena (SM64 units).
    pub fn radius(self) -> f64 {
        match self {
            Mini::CoinScramble => 950.0,
            Mini::SummitCrown => 700.0,
            Mini::BombRain => 800.0,
            Mini::StarTag => 800.0,
            Mini::GoombaStomp => 850.0,
        }
    }

    /// Build the game for the `active` players on `field`.
    pub fn make(self, seed: u64, active: [bool; 4], field: Arc<Field>) -> Box<dyn MiniGame> {
        match self {
            Mini::CoinScramble => Box::new(coin_scramble::CoinScramble::new(seed, active, field)),
            Mini::SummitCrown => Box::new(summit_crown::SummitCrown::new(seed, active, field)),
            Mini::BombRain => Box::new(bomb_rain::BombRain::new(seed, active, field)),
            Mini::StarTag => Box::new(star_tag::StarTag::new(seed, active, field)),
            Mini::GoombaStomp => Box::new(goomba_stomp::GoombaStomp::new(seed, active, field)),
        }
    }
}

/// Level discs of each game's size: the arenas without game data (the board core's own headless games, unit tests).
pub fn stand_in_fields() -> Vec<Arc<Field>> {
    ALL_MINIS.iter().map(|m| Arc::new(Field::flat([0.0; 3], m.radius()))).collect()
}

/// Fixed simulation step of every mini-game: one SM64 frame.
pub const DT: f32 = 1.0 / 30.0;

/// Where one player's Mario is this step (from the physics body, or from [`Kin`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    pub pos: [f64; 3],
    /// Facing, SM64 angle units.
    pub yaw: f64,
    pub grounded: bool,
    /// Rising fast or falling fast (stomps need a falling body).
    pub vy: f64,
    /// Punching, kicking or diving: the moves that hit.
    pub attack: bool,
}

/// What a player (person or bot) asks of their Mario this step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    /// World direction (x, z) to run toward; None lets go of the stick.
    pub dir: Option<[f64; 2]>,
    /// Stick magnitude 0..1.
    pub mag: f64,
    /// A pressed this step.
    pub jump: bool,
    /// A held (a higher jump).
    pub hold: bool,
    /// B pressed this step: punch on the ground, dive in the air.
    pub punch: bool,
}

/// What a game asks of the bodies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MiniEv {
    /// Knock player `p` away from `from` (a real knockback on the physics body).
    Knock { p: usize, from: [f64; 3], power: f64 },
    /// Put player `p` here (a fall off the arena, a start position).
    Place { p: usize, pos: [f64; 3] },
    /// Bounce player `p` up off something they stomped.
    Bounce { p: usize },
    /// A sound at a place (`SfxCue::Effect` kind).
    Cue { kind: &'static str, pos: [f64; 3] },
}

/// The mini-game contract: a deterministic core stepped once per SM64 frame.
pub trait MiniGame: Send + Sync {
    /// Where player `p` starts.
    fn start(&self, p: usize) -> [f64; 3];
    fn step(&mut self, dt: f32, bodies: &[Body; 4], drives: &[Drive; 4]);
    /// The CPU policy for player `p`, called once per step before [`MiniGame::step`].
    fn bot(&mut self, p: usize, lvl: Level, bodies: &[Body; 4]) -> Drive;
    /// Requests since the last call.
    fn events(&mut self) -> Vec<MiniEv>;
    fn done(&self) -> bool;
    fn time_left(&self) -> f32;
    /// Higher is better; inactive players report `f32::NEG_INFINITY`.
    fn scores(&self) -> [f32; 4];
    fn scene(&self, out: &mut Stage);
    /// Speed factor for a player (the star holder runs a little slower).
    fn speed(&self, _p: usize) -> f64 {
        1.0
    }
    /// Players who are out of the game (their Mario is hidden).
    fn out(&self, _p: usize) -> bool {
        false
    }
}

// -- shared helpers ---------------------------------------------------------------------------

pub fn hdist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// Unit direction (x, z) from `a` to `b`, None when they are on top of each other.
pub fn toward(a: [f64; 3], b: [f64; 3]) -> Option<[f64; 2]> {
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    let d = dx.hypot(dz);
    (d > 1.0).then(|| [dx / d, dz / d])
}

/// SM64 yaw (0 = +z, 0x4000 = +x) as a unit (x, z) vector.
pub fn yaw_dir(yaw: f64) -> [f64; 2] {
    let r = yaw / 32768.0 * std::f64::consts::PI;
    [r.sin(), r.cos()]
}

/// Whether `a` (facing `yaw`) has `b` roughly in front of it.
pub fn facing(a: &Body, b: [f64; 3], cos_limit: f64) -> bool {
    let Some(d) = toward(a.pos, b) else { return true };
    let f = yaw_dir(a.yaw);
    d[0] * f[0] + d[1] * f[1] >= cos_limit
}

/// The standard hit test: `a` attacks and `v` is close, at about the same height and in front of `a`.
pub fn hits(a: &Body, v: &Body, reach: f64) -> bool {
    a.attack && hdist(a.pos, v.pos) < reach && (a.pos[1] - v.pos[1]).abs() < 170.0 && facing(a, v.pos, 0.2)
}

/// The seats in the order they get first claim this step: rotating every step, so when two Marios reach a coin (or
/// each other) on the same frame no seat always wins the tie.
pub fn seat_order(t: f32) -> [usize; 4] {
    let s = (t * 30.0).round() as usize % 4;
    [s, (s + 1) % 4, (s + 2) % 4, (s + 3) % 4]
}

/// Rotate a direction by `turn` radians.
pub fn rotate(d: [f64; 2], turn: f64) -> [f64; 2] {
    let (s, c) = turn.sin_cos();
    [d[0] * c - d[1] * s, d[0] * s + d[1] * c]
}

/// A drive running toward `to` (stopping inside `stop`).
pub fn run_to(from: [f64; 3], to: [f64; 3], stop: f64) -> Drive {
    let d = hdist(from, to);
    match toward(from, to) {
        Some(dir) if d > stop => Drive { dir: Some(dir), mag: if d > 220.0 { 1.0 } else { (d / 220.0).max(0.45) }, ..Default::default() },
        _ => Drive::default(),
    }
}

// -- the session --------------------------------------------------------------------------------

/// Seconds a bot must fail to make headway before it jumps to get unstuck.
const STUCK_SECS: f32 = 0.7;

/// A running mini-game with its players: people bring their own [`Drive`], bots fill in the rest. The headless path
/// ([`Session::step_kin`]) moves [`Kin`] bodies; the app reads physics bodies and calls [`Session::plan`] and
/// [`Session::advance`] itself.
pub struct Session {
    pub kind: Mini,
    pub game: Box<dyn MiniGame>,
    pub active: [bool; 4],
    pub human: [bool; 4],
    pub levels: [Level; 4],
    pub steps: u32,
    pub field: Arc<Field>,
    pub kin: Kin,
    stuck: [(f32, [f64; 3]); 4],
    rng: Rng,
}

impl Session {
    pub fn new(kind: Mini, seed: u64, active: [bool; 4], human: [bool; 4], levels: [Level; 4], field: Arc<Field>) -> Self {
        let game = kind.make(seed, active, field.clone());
        let kin = Kin::new(field.clone(), std::array::from_fn(|p| game.start(p)));
        let stuck = std::array::from_fn(|p| (0.0, game.start(p)));
        Session { kind, game, active, human, levels, steps: 0, field, kin, stuck, rng: Rng::new(seed ^ 0x5eed) }
    }

    /// Every seat's drive: people's from `human_in`, bots' from the game's policy (with a jump when they are stuck),
    /// scaled by the game's speed factor; nothing for inactive or knocked-out seats.
    pub fn plan(&mut self, bodies: &[Body; 4], human_in: &[Drive; 4]) -> [Drive; 4] {
        let mut out = [Drive::default(); 4];
        for p in 0..4 {
            if !self.active[p] || self.game.out(p) {
                continue;
            }
            let mut d = if self.human[p] { human_in[p] } else { self.game.bot(p, self.levels[p], bodies) };
            if !self.human[p] {
                let (t, at) = &mut self.stuck[p];
                if d.dir.is_some() && d.mag > 0.5 {
                    if hdist(*at, bodies[p].pos) > 60.0 {
                        *t = 0.0;
                        *at = bodies[p].pos;
                    } else {
                        *t += DT;
                        if *t > STUCK_SECS {
                            *t = 0.0;
                            d.jump = true;
                            d.hold = true;
                            let turn = if self.rng.chance(0.5) { 0.7 } else { -0.7 };
                            d.dir = d.dir.map(|v| rotate(v, turn));
                        }
                    }
                } else {
                    *t = 0.0;
                    *at = bodies[p].pos;
                }
            }
            d.mag *= self.game.speed(p);
            out[p] = d;
        }
        out
    }

    /// One rules step with these bodies and drives; returns the game's requests.
    pub fn advance(&mut self, bodies: &[Body; 4], drives: &[Drive; 4]) -> Vec<MiniEv> {
        self.game.step(DT, bodies, drives);
        self.steps += 1;
        self.game.events()
    }

    /// One whole headless step on the kinematic bodies (tests, the board's autoplay).
    pub fn step_kin(&mut self, human_in: &[Drive; 4]) {
        let bodies = self.kin.bodies();
        let drives = self.plan(&bodies, human_in);
        self.kin.step(&drives);
        let bodies = self.kin.bodies();
        for ev in self.advance(&bodies, &drives) {
            self.kin.apply(&ev);
        }
    }

    pub fn done(&self) -> bool {
        self.game.done()
    }

    pub fn ranks(&self) -> [u8; 4] {
        ranks(&self.game.scores(), &self.active)
    }
}

/// Play a game to the end with bots in every active seat on the kinematic bodies (tests, balance runs, the board's
/// headless mode).
pub fn run_bots(kind: Mini, seed: u64, active: [bool; 4], levels: [Level; 4], field: &Arc<Field>) -> ([f32; 4], f32) {
    let mut s = Session::new(kind, seed, active, [false; 4], levels, field.clone());
    let none = [Drive::default(); 4];
    // Every game lasts well under a minute; the cap guards against a stuck core.
    while !s.done() && s.steps < 30 * 90 {
        s.step_kin(&none);
    }
    assert!(s.done(), "{:?} did not terminate", kind);
    (s.game.scores(), s.steps as f32 * DT)
}

#[cfg(test)]
mod tests;
