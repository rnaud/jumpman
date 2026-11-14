//! Last Star Standing, engine-free rules: an original last-one-standing match on Mario's
//! physics. You and a field of AI Marios drop onto an island, scavenge pickups, and fight while
//! the Blight (a corrupting fog) eats the island from the edge inward; the last Mario standing
//! wins.
//!
//! Everything here is deterministic from the seed and independent of Bevy: the experiment glue
//! (`lss.rs`) feeds in the real positions of the Marios it runs on the full physics engine
//! (the player and the bots nearest to the action) as [`Motion`]s, and this module simulates the
//! rest of the field with a cheap kinematic mover, resolves every hit, projectile, pickup and
//! elimination, runs the zone schedule and the bot brains, and reports what happened as
//! [`Event`]s. Terrain comes in through the [`Terrain`] trait so the whole match can be unit
//! tested on flat ground.
//!
//! Coordinates are SM64 units (x, y up, z). A heading `h` (radians) points along
//! `(sin h, cos h)` in the x/z plane, like SM64's `sins/coss` of the face yaw.

use std::f32::consts::{PI, TAU};

pub const TPS: u32 = 30;
pub const DT: f32 = 1.0 / TPS as f32;
pub const MAX_HP: f32 = 100.0;
/// Hard ceiling for fighters (player + bots); the render lists are sized from this.
pub const MAX_FIGHTERS: usize = 48;
pub const PICKUP_SLOTS: usize = 224;
pub const SHOT_SLOTS: usize = 48;
pub const RING_SLOTS: usize = 12;
/// Height above the island's highest floor where everybody starts the drop.
pub const DROP_HEIGHT: f32 = 3400.0;
pub const KIN_RUN: f32 = 24.0;

// ---------------------------------------------------------------------------------------------
// Random numbers
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Self(seed ^ 0x9e37_79b9_7f4a_7c15);
        r.next_u64();
        r
    }
    /// splitmix64.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { (self.next_u64() % n as u64) as u32 }
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
    /// Index chosen with probability proportional to the weights (all-zero picks the last).
    pub fn weighted(&mut self, weights: &[f32]) -> usize {
        let total: f32 = weights.iter().sum();
        let mut x = self.f() * total;
        for (i, w) in weights.iter().enumerate() {
            if x < *w {
                return i;
            }
            x -= w;
        }
        weights.len().saturating_sub(1)
    }
}

pub fn wrap_pi(mut a: f32) -> f32 {
    while a > PI {
        a -= TAU;
    }
    while a < -PI {
        a += TAU;
    }
    a
}

#[inline]
pub fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[inline]
fn heading_to(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[0] - from[0]).atan2(to[2] - from[2])
}

// ---------------------------------------------------------------------------------------------
// Terrain
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surf {
    /// Walkable.
    Solid,
    /// Too steep to stand on (bots treat it as a wall).
    Steep,
    /// Water, lava, death plane: ending up on it is an elimination.
    Hazard,
}

#[derive(Clone, Copy, Debug)]
pub struct Ground {
    pub y: f32,
    pub surf: Surf,
}

pub trait Terrain {
    /// The highest floor at or below `y` under (x, z); `None` over the void.
    fn ground(&self, x: f32, y: f32, z: f32) -> Option<Ground>;
    /// Push a body at `pos` out of walls (identity when there is no wall).
    fn wall(&self, pos: [f32; 3]) -> [f32; 3] {
        pos
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Arena {
    pub cx: f32,
    pub cz: f32,
    /// Radius of the playable disc.
    pub r: f32,
    /// Highest floor on the island (drop altitude is relative to it).
    pub top_y: f32,
    /// Below this height a Mario is gone.
    pub kill_y: f32,
}

// ---------------------------------------------------------------------------------------------
// The zone (the Blight)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub x: f32,
    pub z: f32,
    pub r: f32,
}

impl Circle {
    pub fn contains(&self, p: [f32; 3]) -> bool {
        (p[0] - self.x).hypot(p[2] - self.z) <= self.r
    }
    pub fn lerp(&self, to: &Circle, k: f32) -> Circle {
        Circle { x: self.x + (to.x - self.x) * k, z: self.z + (to.z - self.z) * k, r: self.r + (to.r - self.r) * k }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Phase {
    /// Seconds the circle sits still (the next one is already marked on the map).
    pub hold: f32,
    /// Seconds it takes to close in.
    pub shrink: f32,
    /// Radius of the next circle as a fraction of the arena.
    pub end_frac: f32,
    /// Health per second lost outside the circle while this phase is active.
    pub dps: f32,
}

/// Tuned for a ~4.5 minute match: on the real island a 40-fighter field used to stay nearly whole for the first
/// two minutes (the bots near you run on the full physics and loot slowly), which read as an empty game. The
/// first hold is now short and the Blight bites harder, so the circle herds everybody together sooner.
pub const PHASES: [Phase; 6] = [
    Phase { hold: 30.0, shrink: 28.0, end_frac: 0.58, dps: 2.5 },
    Phase { hold: 28.0, shrink: 26.0, end_frac: 0.36, dps: 4.0 },
    Phase { hold: 24.0, shrink: 22.0, end_frac: 0.22, dps: 6.0 },
    Phase { hold: 20.0, shrink: 18.0, end_frac: 0.11, dps: 9.0 },
    Phase { hold: 16.0, shrink: 16.0, end_frac: 0.05, dps: 13.0 },
    Phase { hold: 12.0, shrink: 14.0, end_frac: 0.0, dps: 18.0 },
];
/// Seconds of free flight before the first phase starts.
pub const DROP_SECS: f32 = 18.0;
/// After the last phase ends the Blight gets this hungry, so a match cannot stall.
pub const ENDGAME_DPS: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Before the first phase: everybody is still dropping in.
    Landing,
    Hold,
    Shrink,
    /// After the last shrink.
    Final,
}

#[derive(Clone, Copy, Debug)]
pub struct ZoneView {
    pub cur: Circle,
    pub next: Circle,
    /// 1-based active phase (0 while landing).
    pub phase: usize,
    pub stage: Stage,
    /// Seconds until the current stage ends.
    pub secs_left: f32,
    pub dps: f32,
}

#[derive(Clone, Debug)]
pub struct Zone {
    pub circles: [Circle; PHASES.len() + 1],
}

impl Zone {
    pub fn generate(arena: &Arena, rng: &mut Rng, terrain: &dyn Terrain) -> Self {
        let mut circles = [Circle { x: arena.cx, z: arena.cz, r: arena.r * 0.98 }; PHASES.len() + 1];
        for (i, ph) in PHASES.iter().enumerate() {
            let prev = circles[i];
            let r = (arena.r * ph.end_frac).max(40.0 * (ph.end_frac > 0.0) as u8 as f32);
            // Nested: the next circle fits inside the previous one. Try a few random offsets
            // for a centre that stands on solid ground.
            let slack = (prev.r - r).max(0.0);
            let mut best = Circle { x: prev.x, z: prev.z, r };
            for _ in 0..16 {
                let a = rng.range(0.0, TAU);
                let d = slack * rng.range(0.2, 0.92).sqrt();
                let c = Circle { x: prev.x + a.sin() * d, z: prev.z + a.cos() * d, r };
                if terrain.ground(c.x, arena.top_y + 500.0, c.z).is_some_and(|g| g.surf == Surf::Solid) {
                    best = c;
                    break;
                }
            }
            circles[i + 1] = best;
        }
        Self { circles }
    }

    pub fn total_secs() -> f32 {
        DROP_SECS + PHASES.iter().map(|p| p.hold + p.shrink).sum::<f32>()
    }

    /// Start time of phase `i` (0-based).
    pub fn phase_start(i: usize) -> f32 {
        DROP_SECS + PHASES.iter().take(i).map(|p| p.hold + p.shrink).sum::<f32>()
    }

    pub fn at(&self, secs: f32) -> ZoneView {
        if secs < DROP_SECS {
            return ZoneView { cur: self.circles[0], next: self.circles[0], phase: 0, stage: Stage::Landing, secs_left: DROP_SECS - secs, dps: 0.0 };
        }
        let mut t = secs - DROP_SECS;
        for (i, ph) in PHASES.iter().enumerate() {
            if t < ph.hold {
                return ZoneView { cur: self.circles[i], next: self.circles[i + 1], phase: i + 1, stage: Stage::Hold, secs_left: ph.hold - t, dps: ph.dps };
            }
            t -= ph.hold;
            if t < ph.shrink {
                let k = t / ph.shrink;
                return ZoneView { cur: self.circles[i].lerp(&self.circles[i + 1], k), next: self.circles[i + 1], phase: i + 1, stage: Stage::Shrink, secs_left: ph.shrink - t, dps: ph.dps };
            }
            t -= ph.shrink;
        }
        let last = self.circles[PHASES.len()];
        ZoneView { cur: last, next: last, phase: PHASES.len(), stage: Stage::Final, secs_left: 0.0, dps: ENDGAME_DPS }
    }
}

// ---------------------------------------------------------------------------------------------
// Items and loot
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Rarity {
    Common,
    Rare,
    Epic,
}

impl Rarity {
    /// Damage / effect multiplier.
    pub fn power(self) -> f32 {
        match self {
            Rarity::Common => 1.0,
            Rarity::Rare => 1.25,
            Rarity::Epic => 1.5,
        }
    }
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn label(self) -> &'static str {
        ["COMMON", "RARE", "EPIC"][self as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    /// Thrown green shell that bounces along the ground.
    Shell,
    /// Lit bob-omb: lobbed, fuse, big blast.
    Bomb,
    /// Fire flower: rapid bouncing fireballs.
    Flower,
    /// Star power burst: radial blast plus a moment of invincibility.
    StarBurst,
    /// Charges your next ground pound(s) into a shockwave.
    Quake,
    /// Shield cap: soaks damage for a while.
    Cap,
    /// Speed boots.
    Boots,
    /// Heart: heals.
    Heart,
    /// Lucky shroom: full heal.
    OneUp,
}

pub const ALL_KINDS: [Kind; 9] = [Kind::Shell, Kind::Bomb, Kind::Flower, Kind::StarBurst, Kind::Quake, Kind::Cap, Kind::Boots, Kind::Heart, Kind::OneUp];

impl Kind {
    pub fn is_weapon(self) -> bool {
        matches!(self, Kind::Shell | Kind::Bomb | Kind::Flower | Kind::StarBurst | Kind::Quake)
    }
    pub fn name(self) -> &'static str {
        match self {
            Kind::Shell => "Bounce Shell",
            Kind::Bomb => "Lit Bob-omb",
            Kind::Flower => "Fire Flower",
            Kind::StarBurst => "Star Burst",
            Kind::Quake => "Quake Charge",
            Kind::Cap => "Shield Cap",
            Kind::Boots => "Swift Boots",
            Kind::Heart => "Heart",
            Kind::OneUp => "Lucky Shroom",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Kind::Shell => "SHELL",
            Kind::Bomb => "BOMB",
            Kind::Flower => "FIRE",
            Kind::StarBurst => "STAR",
            Kind::Quake => "QUAKE",
            Kind::Cap => "CAP",
            Kind::Boots => "BOOTS",
            Kind::Heart => "HEART",
            Kind::OneUp => "SHROOM",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Kind::Shell => "Throw a shell that bounces along the ground",
            Kind::Bomb => "Lob a lit bob-omb; it blows after a short fuse",
            Kind::Flower => "Rapid-fire bouncing fireballs",
            Kind::StarBurst => "Blast everything around you and shrug off hits briefly",
            Kind::Quake => "Your next ground pound lands as a huge shockwave",
            Kind::Cap => "A shield that soaks damage for a while",
            Kind::Boots => "Run faster for a while",
            Kind::Heart => "Restore health",
            Kind::OneUp => "Restore all health",
        }
    }
    pub fn min_rarity(self) -> Rarity {
        match self {
            Kind::StarBurst | Kind::OneUp => Rarity::Rare,
            _ => Rarity::Common,
        }
    }
    /// Uses (weapons) by rarity.
    pub fn ammo(self, r: Rarity) -> u8 {
        let t: [u8; 3] = match self {
            Kind::Shell => [3, 4, 6],
            Kind::Bomb => [2, 3, 4],
            Kind::Flower => [8, 12, 18],
            Kind::StarBurst => [1, 1, 2],
            Kind::Quake => [2, 3, 4],
            _ => [1, 1, 1],
        };
        t[r.idx()]
    }
    /// Ticks between uses.
    pub fn cooldown(self) -> u32 {
        match self {
            Kind::Shell => 14,
            Kind::Bomb => 24,
            Kind::Flower => 7,
            Kind::StarBurst => 30,
            Kind::Quake => 20,
            _ => 10,
        }
    }
    /// Consumable strength by rarity: shield points, boost seconds, heal points.
    pub fn potency(self, r: Rarity) -> f32 {
        let t: [f32; 3] = match self {
            Kind::Cap => [25.0, 40.0, 60.0],
            Kind::Boots => [10.0, 14.0, 20.0],
            Kind::Heart => [25.0, 40.0, 60.0],
            Kind::OneUp => [100.0, 100.0, 100.0],
            _ => [0.0; 3],
        };
        t[r.idx()]
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Item {
    pub kind: Kind,
    pub rarity: Rarity,
    pub ammo: u8,
}

impl Item {
    pub fn new(kind: Kind, rarity: Rarity) -> Self {
        Self { kind, rarity: rarity.max(kind.min_rarity()), ammo: kind.ammo(rarity.max(kind.min_rarity())) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Floor,
    Chest,
    Airdrop,
}

/// Relative weight of each kind (order of [`ALL_KINDS`]) by source.
fn kind_weights(source: Source) -> [f32; 9] {
    match source {
        //            shell bomb flower star quake cap boots heart oneup
        Source::Floor => [22.0, 9.0, 16.0, 3.0, 6.0, 12.0, 12.0, 18.0, 2.0],
        Source::Chest => [14.0, 14.0, 14.0, 10.0, 12.0, 10.0, 6.0, 10.0, 6.0],
        Source::Airdrop => [6.0, 22.0, 14.0, 26.0, 22.0, 0.0, 0.0, 0.0, 10.0],
    }
}

fn rarity_weights(source: Source) -> [f32; 3] {
    match source {
        Source::Floor => [68.0, 26.0, 6.0],
        Source::Chest => [25.0, 50.0, 25.0],
        Source::Airdrop => [0.0, 40.0, 60.0],
    }
}

pub fn roll_item(rng: &mut Rng, source: Source) -> Item {
    let kind = ALL_KINDS[rng.weighted(&kind_weights(source))];
    let r = [Rarity::Common, Rarity::Rare, Rarity::Epic][rng.weighted(&rarity_weights(source))];
    Item::new(kind, r)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PickKind {
    Item,
    /// Opens on touch and spills two items.
    Chest { opened: bool },
    /// Falls from the sky, then opens on touch.
    Crate { drop_y: f32, opened: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Pickup {
    pub alive: bool,
    pub pos: [f32; 3],
    pub item: Item,
    pub kind: PickKind,
    /// Ticks since it was spawned or opened (spill animation, cleanup).
    pub age: u32,
}

impl Pickup {
    const DEAD: Pickup = Pickup { alive: false, pos: [0.0; 3], item: Item { kind: Kind::Heart, rarity: Rarity::Common, ammo: 0 }, kind: PickKind::Item, age: 0 };
    pub fn is_container(&self) -> bool {
        !matches!(self.kind, PickKind::Item)
    }
    pub fn opened(&self) -> bool {
        matches!(self.kind, PickKind::Chest { opened: true } | PickKind::Crate { opened: true, .. })
    }
}

// ---------------------------------------------------------------------------------------------
// Fighters
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "easy" => Some(Self::Easy),
            "normal" => Some(Self::Normal),
            "hard" => Some(Self::Hard),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Easy => "EASY",
            Self::Normal => "NORMAL",
            Self::Hard => "HARD",
        }
    }
    /// Radians of aim error (1 sigma-ish).
    fn aim_error(self) -> f32 {
        match self {
            Self::Easy => 0.34,
            Self::Normal => 0.18,
            Self::Hard => 0.07,
        }
    }
    /// Damage multiplier for bot attacks.
    fn damage(self) -> f32 {
        match self {
            Self::Easy => 0.75,
            Self::Normal => 1.0,
            Self::Hard => 1.15,
        }
    }
    fn flee_hp(self) -> f32 {
        match self {
            Self::Easy => 0.42,
            Self::Normal => 0.3,
            Self::Hard => 0.2,
        }
    }
    /// Ticks between brain updates while fighting.
    fn reaction(self) -> u32 {
        match self {
            Self::Easy => 7,
            Self::Normal => 5,
            Self::Hard => 3,
        }
    }
    fn skill(self) -> f32 {
        match self {
            Self::Easy => 0.35,
            Self::Normal => 0.6,
            Self::Hard => 0.9,
        }
    }
}

/// What a Mario body is doing, as far as the rules care.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mv {
    #[default]
    Idle,
    Move,
    Air,
    Dive,
    Pound,
    PoundLand,
    Punch,
    Knock,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Atk {
    #[default]
    None,
    Punch,
    Dive,
}

/// Controls for one tick. For bots the glue turns these into stick and buttons.
#[derive(Clone, Copy, Debug, Default)]
pub struct Intent {
    pub heading: f32,
    /// 0..1 stick magnitude.
    pub mag: f32,
    pub jump: bool,
    pub hold_a: bool,
    pub attack: Atk,
    pub pound: bool,
}

/// The real body of a Mario run by the physics engine this tick.
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub yaw: f32,
    pub grounded: bool,
    pub mv: Mv,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Gliding down from the launch height.
    Drop,
    Ground,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BotState {
    Drop,
    Loot,
    Rotate,
    Engage,
    Flee,
}

/// The few fields of a fighter a brain reads while it mutates other state.
#[derive(Clone, Copy)]
struct Me {
    pos: [f32; 3],
    hp: f32,
    weapons: usize,
    cons: Option<Item>,
}

#[derive(Clone, Debug)]
pub struct Brain {
    pub state: BotState,
    /// 0..1: how readily this bot picks fights.
    pub aggression: f32,
    pub goal: [f32; 3],
    pub goal_pickup: Option<usize>,
    pub target: Option<usize>,
    /// Spot inside the next circle this bot is heading for.
    pub offset: [f32; 2],
    pub think_at: u32,
    pub stuck_t: u32,
    pub stuck_dir: f32,
    last_check: [f32; 3],
    pub aim_bias: f32,
    pub pound_t: u32,
    pub landing: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct Fighter {
    pub id: usize,
    pub name: String,
    pub is_player: bool,
    pub alive: bool,
    pub hp: f32,
    pub shield: f32,
    pub shield_t: u32,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub yaw: f32,
    pub grounded: bool,
    pub mv: Mv,
    prev_mv: Mv,
    pub mode: Mode,
    pub iframes: u32,
    pub slots: [Option<Item>; 2],
    pub cons: Option<Item>,
    pub cd: [u32; 3],
    pub boost_t: u32,
    pub quake: u8,
    pub star_t: u32,
    pub kills: u32,
    /// Final placement once eliminated (1 = winner).
    pub place: u32,
    pub died_tick: u32,
    pub last_hit: Option<(usize, u32)>,
    /// Run on the physics engine this tick (set by [`Lss::choose_sim`]).
    pub sim: bool,
    /// Became `sim` this tick: the glue must (re)place the body.
    pub sim_started: bool,
    pub intent: Intent,
    pub brain: Brain,
    /// Pending knockback: velocity per tick (x, y, z) plus the damage that came with it.
    pub knock: [f32; 3],
    pub knock_dmg: f32,
    pub knock_from: [f32; 3],
    pub knocked: bool,
    /// Asked the glue for a stomp bounce.
    pub bounce: bool,
    atk_cd: u32,
    dive_t: u32,
    punch_t: u32,
    pub dealt: f32,
}

impl Fighter {
    fn new(id: usize, name: String, rng: &mut Rng) -> Self {
        Self {
            id,
            name,
            is_player: id == 0,
            alive: true,
            hp: MAX_HP,
            shield: 0.0,
            shield_t: 0,
            pos: [0.0; 3],
            vel: [0.0; 3],
            yaw: 0.0,
            grounded: false,
            mv: Mv::Air,
            prev_mv: Mv::Air,
            mode: Mode::Drop,
            iframes: 0,
            slots: [None; 2],
            cons: None,
            cd: [0; 3],
            boost_t: 0,
            quake: 0,
            star_t: 0,
            kills: 0,
            place: 0,
            died_tick: 0,
            last_hit: None,
            sim: false,
            sim_started: false,
            intent: Intent::default(),
            brain: Brain {
                state: BotState::Drop,
                aggression: 0.15 + 0.8 * rng.f().powf(1.2),
                goal: [0.0; 3],
                goal_pickup: None,
                target: None,
                offset: [0.0; 2],
                think_at: id as u32 % 5,
                stuck_t: 0,
                stuck_dir: 0.0,
                last_check: [0.0; 3],
                aim_bias: rng.range(-1.0, 1.0),
                pound_t: 0,
                landing: [0.0; 3],
            },
            knock: [0.0; 3],
            knock_dmg: 0.0,
            knock_from: [0.0; 3],
            knocked: false,
            bounce: false,
            atk_cd: 0,
            dive_t: 0,
            punch_t: 0,
            dealt: 0.0,
        }
    }

    pub fn defenseless(&self) -> bool {
        self.mode == Mode::Drop || !self.alive
    }

    pub fn weapon_count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}

// ---------------------------------------------------------------------------------------------
// Events, shots, rings
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cause {
    /// Health reached zero from fighting.
    Fight,
    /// Knocked off the island or into a hazard.
    Fall,
    /// Consumed by the Blight.
    Blight,
}

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Hit { target: usize, by: Option<usize>, dmg: f32 },
    Elim { victim: usize, killer: Option<usize>, cause: Cause, place: u32 },
    Pickup { who: usize, kind: Kind },
    ChestOpen { pos: [f32; 3] },
    AirdropCall { pos: [f32; 3] },
    AirdropLand { pos: [f32; 3] },
    Phase { phase: usize, shrinking: bool },
    Explosion { pos: [f32; 3], r: f32 },
    Shock { pos: [f32; 3], r: f32 },
    Used { who: usize, kind: Kind, pos: [f32; 3] },
    Winner { who: usize },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShotKind {
    Shell,
    Bomb,
    Fire,
}

#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub alive: bool,
    pub kind: ShotKind,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub owner: usize,
    pub life: u32,
    pub bounces: u8,
    pub power: f32,
    pub spin: f32,
}

impl Shot {
    const DEAD: Shot = Shot { alive: false, kind: ShotKind::Shell, pos: [0.0; 3], vel: [0.0; 3], owner: 0, life: 0, bounces: 0, power: 1.0, spin: 0.0 };
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RingKind {
    Blast,
    Shock,
    Star,
}

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub alive: bool,
    pub kind: RingKind,
    pub pos: [f32; 3],
    pub r: f32,
    pub age: u32,
}

impl Ring {
    const DEAD: Ring = Ring { alive: false, kind: RingKind::Blast, pos: [0.0; 3], r: 0.0, age: 0 };
    pub const LIFE: u32 = 14;
    pub fn radius(&self) -> f32 {
        self.r * (0.25 + 0.75 * (self.age as f32 / Self::LIFE as f32).min(1.0))
    }
    pub fn fade(&self) -> f32 {
        1.0 - (self.age as f32 / Self::LIFE as f32).min(1.0)
    }
}

// ---------------------------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------------------------

pub const NAMES: [&str; 64] = [
    "Waddle", "Captain Crumb", "Dr. Zoomies", "Sir Bonk", "Noodle", "Big Pickle", "Turbo Tilly", "Moss", "Gizmo Gus", "Lady Biscuit",
    "Professor Plop", "Rascal", "Tater", "Mighty Muffin", "Baron Bounce", "Pip", "Zig Zag", "Grandma Gale", "Fuzzbucket", "Wobbles",
    "Count Chonk", "Dash Hound", "Marzipan", "Skip", "Bumble", "Jelly Roll", "Clover", "Sprocket", "Lord Lunge", "Nacho",
    "Poppy", "Rusty", "Thistle", "Mr. Mumble", "Pebble", "Kazoo", "Biscotti", "Zesty", "Admiral Fluff", "Doodle",
    "Humphrey", "Twiggy", "Snickers Jr.", "Plum", "Gadget", "Boomer", "Sunny", "Ripley", "Quill", "Tumble",
    "Fandango", "Oddball", "Mochi", "Cricket", "Blaze Bailey", "Pogo", "Walnut", "Yodel", "Sprinkles", "Nimbus",
    "Bartholomew", "Crash Test", "Dizzy Dan", "Peanut",
];

// ---------------------------------------------------------------------------------------------
// The match
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Config {
    pub seed: u64,
    pub bots: usize,
    pub difficulty: Difficulty,
    pub arena: Arena,
    /// Loot camps (clusters of items, half with a chest) and loose singles.
    pub camps: usize,
    pub singles: usize,
}

impl Config {
    pub fn new(seed: u64, bots: usize, arena: Arena) -> Self {
        Self { seed, bots: bots.min(MAX_FIGHTERS - 1), difficulty: Difficulty::Normal, arena, camps: 26, singles: 18 }
    }
}

pub struct Lss {
    pub cfg: Config,
    pub arena: Arena,
    pub zone: Zone,
    pub zv: ZoneView,
    pub tick: u32,
    rng: Rng,
    pub fighters: Vec<Fighter>,
    pub pickups: Vec<Pickup>,
    pub shots: Vec<Shot>,
    pub rings: Vec<Ring>,
    pub events: Vec<Event>,
    pub alive: usize,
    pub over: bool,
    pub winner: Option<usize>,
    /// Player's final placement once known (0 while alive).
    pub player_place: u32,
    pub camps: Vec<[f32; 3]>,
    airdrops: u8,
    prev_phase: (usize, Stage),
    pub god: bool,
    pub freeze_bots: bool,
    /// Test/demo: fighter 0 is driven by the bot brain like everybody else (never set in the game).
    pub autopilot: bool,
}

impl Lss {
    pub fn new(cfg: Config, terrain: &dyn Terrain) -> Self {
        let mut rng = Rng::new(cfg.seed);
        let arena = cfg.arena;
        let zone = Zone::generate(&arena, &mut rng, terrain);
        let zv = zone.at(0.0);
        let n = cfg.bots + 1;
        // Distinct silly names, a different draw per seed.
        let mut order: Vec<usize> = (0..NAMES.len()).collect();
        for i in (1..order.len()).rev() {
            let j = rng.below(i as u32 + 1) as usize;
            order.swap(i, j);
        }
        let mut fighters: Vec<Fighter> = (0..n)
            .map(|i| {
                let name = if i == 0 { "YOU".to_string() } else { NAMES[order[(i - 1) % NAMES.len()]].to_string() };
                Fighter::new(i, name, &mut rng)
            })
            .collect();
        let mut me = Self {
            cfg,
            arena,
            zone,
            zv,
            tick: 0,
            rng,
            alive: n,
            fighters: vec![],
            pickups: vec![Pickup::DEAD; PICKUP_SLOTS],
            shots: vec![Shot::DEAD; SHOT_SLOTS],
            rings: vec![Ring::DEAD; RING_SLOTS],
            events: Vec::with_capacity(64),
            over: false,
            winner: None,
            player_place: 0,
            camps: vec![],
            airdrops: 0,
            prev_phase: (0, Stage::Landing),
            god: false,
            freeze_bots: false,
            autopilot: false,
        };
        me.generate_loot(terrain);
        me.place_drop(&mut fighters);
        me.fighters = fighters;
        me
    }

    /// Everybody starts spread along a line across the island, high up.
    fn place_drop(&mut self, fighters: &mut [Fighter]) {
        let a = self.rng.range(0.0, TAU);
        let (dx, dz) = (a.sin(), a.cos());
        let n = fighters.len();
        let arena = self.arena;
        for (i, f) in fighters.iter_mut().enumerate() {
            let t = (i as f32 + self.rng.range(0.1, 0.9)) / n as f32 * 2.0 - 1.0;
            let side = self.rng.range(-250.0, 250.0);
            f.pos = [arena.cx + dx * t * arena.r * 0.85 + dz * side, arena.top_y + DROP_HEIGHT, arena.cz + dz * t * arena.r * 0.85 - dx * side];
            f.yaw = a;
            f.vel = [0.0, -15.0, 0.0];
            f.mode = Mode::Drop;
            f.mv = Mv::Air;
            f.brain.state = BotState::Drop;
            f.brain.offset = [self.rng.range(0.0, TAU), self.rng.f().sqrt() * 0.6];
        }
        // Each bot picks a landing camp near its start (with noise, so they spread out).
        let camps = self.camps.clone();
        for f in fighters.iter_mut() {
            if f.is_player || camps.is_empty() {
                f.brain.landing = [f.pos[0], 0.0, f.pos[2]];
                continue;
            }
            let mut best = (f32::MAX, camps[0]);
            for c in &camps {
                let d = dist2(f.pos, *c) * self.rng.range(0.7, 1.5);
                if d < best.0 {
                    best = (d, *c);
                }
            }
            let jitter = self.rng.range(0.0, TAU);
            f.brain.landing = [best.1[0] + jitter.sin() * 150.0, best.1[1], best.1[2] + jitter.cos() * 150.0];
        }
    }

    fn free_pickup(&mut self) -> Option<usize> {
        self.pickups.iter().position(|p| !p.alive)
    }

    pub fn add_pickup(&mut self, pos: [f32; 3], item: Item, kind: PickKind) -> Option<usize> {
        let i = self.free_pickup()?;
        self.pickups[i] = Pickup { alive: true, pos, item, kind, age: 0 };
        Some(i)
    }

    fn random_floor(&mut self, terrain: &dyn Terrain, max_r: f32) -> Option<[f32; 3]> {
        for _ in 0..30 {
            let a = self.rng.range(0.0, TAU);
            let d = self.rng.f().sqrt() * max_r;
            let (x, z) = (self.arena.cx + a.sin() * d, self.arena.cz + a.cos() * d);
            if let Some(g) = terrain.ground(x, self.arena.top_y + 400.0, z) {
                if g.surf == Surf::Solid {
                    return Some([x, g.y, z]);
                }
            }
        }
        None
    }

    fn generate_loot(&mut self, terrain: &dyn Terrain) {
        let r = self.arena.r * 0.94;
        for c in 0..self.cfg.camps {
            let Some(center) = self.random_floor(terrain, r) else { continue };
            self.camps.push(center);
            let chest = c % 2 == 0;
            let items = 2 + self.rng.below(3);
            if chest {
                self.add_pickup(center, Item::new(Kind::Heart, Rarity::Common), PickKind::Chest { opened: false });
            }
            for _ in 0..items {
                let a = self.rng.range(0.0, TAU);
                let d = self.rng.range(120.0, 330.0);
                let (x, z) = (center[0] + a.sin() * d, center[2] + a.cos() * d);
                let Some(g) = terrain.ground(x, center[1] + 300.0, z).filter(|g| g.surf == Surf::Solid) else { continue };
                let item = roll_item(&mut self.rng, Source::Floor);
                self.add_pickup([x, g.y, z], item, PickKind::Item);
            }
        }
        for _ in 0..self.cfg.singles {
            let Some(p) = self.random_floor(terrain, r) else { continue };
            let item = roll_item(&mut self.rng, Source::Floor);
            self.add_pickup(p, item, PickKind::Item);
        }
    }

    /// Index of the first fighter the bot brain drives (the player is 0 unless on autopilot).
    fn first_bot(&self) -> usize {
        if self.autopilot { 0 } else { 1 }
    }

    pub fn player(&self) -> &Fighter {
        &self.fighters[0]
    }

    pub fn secs(&self) -> f32 {
        self.tick as f32 * DT
    }

    pub fn alive_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.fighters.iter().filter(|f| f.alive).map(|f| f.id)
    }

    // -----------------------------------------------------------------------------------------
    // Damage
    // -----------------------------------------------------------------------------------------

    /// Apply a hit from `by` (or the environment). `dir` is the horizontal knockback direction
    /// (unit), `force` its speed per tick, `lift` the upward kick. Returns the damage dealt.
    #[allow(clippy::too_many_arguments)]
    fn hit(&mut self, target: usize, by: Option<usize>, raw: f32, dir: [f32; 2], force: f32, lift: f32, source: [f32; 3]) -> f32 {
        let tick = self.tick;
        let diff = self.cfg.difficulty;
        let by_bot = by.is_some_and(|b| b != 0);
        let f = &mut self.fighters[target];
        if !f.alive || f.defenseless() || f.iframes > 0 || f.star_t > 0 || self.over || (self.god && target == 0) {
            return 0.0;
        }
        let mut dmg = raw * if by_bot && !f.is_player { 1.0 } else if by_bot { diff.damage() } else { 1.0 };
        if f.shield > 0.0 {
            let soak = f.shield.min(dmg);
            f.shield -= soak;
            dmg -= soak;
            if f.shield <= 0.0 {
                f.shield_t = 0;
            }
        }
        f.hp -= dmg;
        f.iframes = 18;
        f.last_hit = by.map(|b| (b, tick));
        f.knock = [dir[0] * force, lift, dir[1] * force];
        f.knock_dmg = raw;
        f.knock_from = source;
        f.knocked = true;
        let _ = f;
        if let Some(b) = by {
            self.fighters[b].dealt += dmg;
        }
        self.events.push(Event::Hit { target, by, dmg });
        dmg
    }

    fn knock_dir(&self, from: [f32; 3], to: [f32; 3]) -> [f32; 2] {
        let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
        let l = dx.hypot(dz);
        if l < 1.0 { [0.0, 1.0] } else { [dx / l, dz / l] }
    }

    // -----------------------------------------------------------------------------------------
    // Items
    // -----------------------------------------------------------------------------------------

    /// Best auto-aim heading for `who`: the nearest enemy inside a cone around `facing`, else
    /// `facing` itself. Returns (heading, target).
    pub fn aim(&self, who: usize, facing: f32, cone: f32, range: f32) -> (f32, Option<usize>) {
        let me = &self.fighters[who];
        let mut best: Option<(f32, usize, f32)> = None;
        for o in &self.fighters {
            if o.id == who || !o.alive || o.mode == Mode::Drop {
                continue;
            }
            let d = dist2(me.pos, o.pos);
            if d > range || d < 1.0 {
                continue;
            }
            let h = heading_to(me.pos, o.pos);
            if wrap_pi(h - facing).abs() > cone {
                continue;
            }
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, o.id, h));
            }
        }
        match best {
            Some((_, id, h)) => (h, Some(id)),
            None => (facing, None),
        }
    }

    /// Use the item in slot `slot` (0, 1 = weapons, 2 = consumable). `heading` is where to
    /// throw/fire. Returns whether anything happened.
    pub fn use_item(&mut self, who: usize, slot: usize, heading: f32) -> bool {
        if who >= self.fighters.len() || slot > 2 || self.over {
            return false;
        }
        let f = &self.fighters[who];
        if !f.alive || f.cd[slot] > 0 || (f.mode == Mode::Drop && slot < 2) {
            return false;
        }
        let item = if slot < 2 { f.slots[slot] } else { f.cons };
        let Some(item) = item else { return false };
        let pos = f.pos;
        let power = item.rarity.power();
        let mut dm = self.diff_of(who).damage();
        dm *= power;
        let (sh, ch) = (heading.sin(), heading.cos());
        match item.kind {
            Kind::Shell => self.spawn_shot(ShotKind::Shell, who, [pos[0] + sh * 70.0, pos[1] + 50.0, pos[2] + ch * 70.0], [sh * 26.0, 4.0, ch * 26.0], dm),
            Kind::Flower => self.spawn_shot(ShotKind::Fire, who, [pos[0] + sh * 60.0, pos[1] + 70.0, pos[2] + ch * 60.0], [sh * 24.0, 4.0, ch * 24.0], dm),
            Kind::Bomb => self.spawn_shot(ShotKind::Bomb, who, [pos[0] + sh * 60.0, pos[1] + 80.0, pos[2] + ch * 60.0], [sh * 17.0 + self.fighters[who].vel[0] * 0.3, 17.0, ch * 17.0 + self.fighters[who].vel[2] * 0.3], dm),
            Kind::StarBurst => {
                let r = 430.0;
                self.add_ring(RingKind::Star, pos, r);
                self.events.push(Event::Shock { pos, r });
                self.aoe(who, pos, r, 24.0 * dm, 17.0, 14.0);
                self.fighters[who].star_t = 75;
            }
            Kind::Quake => {
                self.fighters[who].quake = self.fighters[who].quake.saturating_add(1).min(3);
            }
            Kind::Cap => {
                let f = &mut self.fighters[who];
                f.shield = f.shield.max(item.kind.potency(item.rarity));
                f.shield_t = 15 * TPS;
            }
            Kind::Boots => {
                let f = &mut self.fighters[who];
                f.boost_t = (item.kind.potency(item.rarity) * TPS as f32) as u32;
            }
            Kind::Heart | Kind::OneUp => {
                let f = &mut self.fighters[who];
                f.hp = (f.hp + item.kind.potency(item.rarity)).min(MAX_HP);
            }
        }
        self.events.push(Event::Used { who, kind: item.kind, pos });
        let f = &mut self.fighters[who];
        f.cd[slot] = item.kind.cooldown();
        let used_up = {
            let it = if slot < 2 { f.slots[slot].as_mut() } else { f.cons.as_mut() };
            let it = it.unwrap();
            it.ammo = it.ammo.saturating_sub(1);
            it.ammo == 0
        };
        if used_up {
            if slot < 2 {
                f.slots[slot] = None;
            } else {
                f.cons = None;
            }
        }
        true
    }

    fn spawn_shot(&mut self, kind: ShotKind, owner: usize, pos: [f32; 3], vel: [f32; 3], power: f32) {
        let life = match kind {
            ShotKind::Shell => 110,
            ShotKind::Bomb => 48,
            ShotKind::Fire => 60,
        };
        let slot = self.shots.iter().position(|s| !s.alive).unwrap_or(0);
        self.shots[slot] = Shot { alive: true, kind, pos, vel, owner, life, bounces: 0, power, spin: 0.0 };
    }

    fn add_ring(&mut self, kind: RingKind, pos: [f32; 3], r: f32) {
        let slot = self.rings.iter().position(|s| !s.alive).unwrap_or(0);
        self.rings[slot] = Ring { alive: true, kind, pos, r, age: 0 };
    }

    /// Area damage around `pos`, falling off with distance. The owner is spared.
    fn aoe(&mut self, owner: usize, pos: [f32; 3], r: f32, dmg: f32, force: f32, lift: f32) {
        for i in 0..self.fighters.len() {
            if i == owner || !self.fighters[i].alive {
                continue;
            }
            let t = self.fighters[i].pos;
            let d = dist2(pos, t);
            if d > r || (t[1] - pos[1]).abs() > r * 0.7 {
                continue;
            }
            let fall = 1.0 - 0.55 * (d / r);
            let dir = self.knock_dir(pos, t);
            self.hit(i, Some(owner), dmg * fall, dir, force * fall.max(0.5), lift, pos);
        }
    }

    // -----------------------------------------------------------------------------------------
    // The tick
    // -----------------------------------------------------------------------------------------

    /// One 30 Hz step. `motions[i]` is the physics engine's body for fighter `i` when
    /// `fighters[i].sim` is set, and always for the player (fighter 0) when present.
    pub fn tick(&mut self, terrain: &dyn Terrain, motions: &[Option<Motion>]) {
        self.events.clear();
        if self.over {
            return;
        }
        self.tick += 1;
        let secs = self.secs();
        self.zv = self.zone.at(secs);
        let key = (self.zv.phase, self.zv.stage);
        if key != self.prev_phase {
            if self.zv.phase > 0 && matches!(self.zv.stage, Stage::Hold | Stage::Shrink) {
                self.events.push(Event::Phase { phase: self.zv.phase, shrinking: self.zv.stage == Stage::Shrink });
            }
            self.prev_phase = key;
        }
        self.schedule_airdrops(terrain);

        // 1. Bodies: physics-driven ones report in, the rest move kinematically.
        for i in 0..self.fighters.len() {
            if !self.fighters[i].alive {
                continue;
            }
            // The player's Mario is always a physics body: the host hands its motion in every
            // tick (choose_sim only ever flags bots, so the player must not wait on `sim`, or his
            // core position would stay at the drop point and he could never collect anything).
            let physics = self.fighters[i].sim || self.fighters[i].is_player;
            let motion = if physics { motions.get(i).copied().flatten() } else { None };
            let f = &mut self.fighters[i];
            f.prev_mv = f.mv;
            f.sim_started = false;
            f.knocked_clear_if_unused();
            match motion {
                Some(m) => {
                    f.pos = m.pos;
                    f.vel = m.vel;
                    f.yaw = m.yaw;
                    f.grounded = m.grounded;
                    f.mv = m.mv;
                    if f.mode == Mode::Drop && m.grounded {
                        f.mode = Mode::Ground;
                        f.iframes = f.iframes.max(30);
                    }
                }
                None => self.kin_step(i, terrain),
            }
        }
        // Taps (jump, attacks, pound) last one tick; the brain re-issues them while it wants them.
        for f in self.fighters.iter_mut() {
            f.intent.jump = false;
            f.intent.attack = Atk::None;
            f.intent.pound = false;
        }
        // 2. Melee and item effects that come from what the bodies did.
        for i in 0..self.fighters.len() {
            if self.fighters[i].alive {
                self.melee(i);
            }
        }
        // 3. Bots use items.
        for i in self.first_bot()..self.fighters.len() {
            self.bot_items(i);
        }
        self.step_shots(terrain);
        self.collect(terrain);
        // 4. Environment.
        self.environment(terrain);
        // 5. Timers.
        for f in self.fighters.iter_mut().filter(|f| f.alive) {
            f.iframes = f.iframes.saturating_sub(1);
            f.star_t = f.star_t.saturating_sub(1);
            f.boost_t = f.boost_t.saturating_sub(1);
            for c in f.cd.iter_mut() {
                *c = c.saturating_sub(1);
            }
            f.atk_cd = f.atk_cd.saturating_sub(1);
            if f.shield_t > 0 {
                f.shield_t -= 1;
                if f.shield_t == 0 {
                    f.shield = 0.0;
                }
            }
            if f.mode == Mode::Ground && f.mv == Mv::Idle && f.hp < MAX_HP && f.is_player {
                // (no passive regen: health comes from pickups)
            }
        }
        for r in self.rings.iter_mut().filter(|r| r.alive) {
            r.age += 1;
            if r.age > Ring::LIFE {
                r.alive = false;
            }
        }
        // 6. Brains decide the next tick's intents.
        if !self.freeze_bots {
            for i in self.first_bot()..self.fighters.len() {
                if self.fighters[i].alive {
                    self.think(i, terrain);
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Kinematic movement for the bots the physics engine is not running
    // -----------------------------------------------------------------------------------------

    fn kin_step(&mut self, i: usize, terrain: &dyn Terrain) {
        let arena = self.arena;
        let freeze = self.freeze_bots && i != 0;
        let autopilot = self.autopilot;
        let f = &mut self.fighters[i];
        if f.is_player && !f.sim && !autopilot {
            return;
        }
        if f.mode == Mode::Drop {
            // Glide toward the chosen landing spot, steeply.
            let t = f.brain.landing;
            let d = dist2(f.pos, t);
            let alt = f.pos[1] - (t[1] + 40.0);
            let steps_left = (alt / 14.0).max(1.0);
            let speed = (d / steps_left).min(34.0);
            let h = heading_to(f.pos, t);
            if d > 30.0 {
                f.pos[0] += h.sin() * speed;
                f.pos[2] += h.cos() * speed;
                f.yaw = h;
            }
            f.pos[1] -= 14.0;
            f.vel = [h.sin() * speed, -14.0, h.cos() * speed];
            f.mv = Mv::Air;
            f.grounded = false;
            if let Some(g) = terrain.ground(f.pos[0], f.pos[1] + 60.0, f.pos[2]) {
                if f.pos[1] <= g.y + 30.0 {
                    f.pos[1] = g.y;
                    f.mode = Mode::Ground;
                    f.mv = Mv::Idle;
                    f.grounded = true;
                    f.vel = [0.0; 3];
                    f.iframes = f.iframes.max(24);
                }
            }
            return;
        }
        let intent = if freeze { Intent::default() } else { f.intent };
        // Lunges and pounds override walking.
        let mut vx = 0.0;
        let mut vz = 0.0;
        if f.dive_t > 0 {
            f.dive_t -= 1;
            f.mv = Mv::Dive;
            vx = f.yaw.sin() * 46.0;
            vz = f.yaw.cos() * 46.0;
        } else if f.mv == Mv::Pound {
            // handled below by the vertical step
        } else if f.punch_t > 0 {
            f.punch_t -= 1;
        } else if f.knock[0].abs() + f.knock[2].abs() > 0.6 || !f.grounded {
            // ballistic / knocked
        } else if intent.mag > 0.05 {
            let boost = if f.boost_t > 0 { 1.3 } else { 1.0 };
            let s = KIN_RUN * intent.mag * boost;
            vx = intent.heading.sin() * s;
            vz = intent.heading.cos() * s;
            f.yaw = intent.heading;
        }
        // Knockback velocity decays.
        let kx = f.knock[0];
        let kz = f.knock[2];
        f.knock[0] *= 0.86;
        f.knock[2] *= 0.86;
        if !f.grounded {
            vx += f.vel[0] * 0.0;
        }
        let mut nx = f.pos[0] + vx + kx;
        let mut nz = f.pos[2] + vz + kz;
        let mut ny = f.pos[1];
        // Jumping, falling and diving down.
        if f.grounded && (intent.jump) && f.mv != Mv::Dive {
            f.vel[1] = if intent.hold_a { 46.0 } else { 40.0 };
            f.grounded = false;
        }
        if f.knock[1] > 0.0 {
            f.vel[1] = f.knock[1];
            f.knock[1] = 0.0;
            f.grounded = false;
        }
        if f.grounded {
            f.vel[1] = 0.0;
        } else {
            f.vel[1] = (f.vel[1] - 4.0).max(-75.0);
            if intent.pound && f.vel[1] < 20.0 && f.mv != Mv::Pound && f.mv != Mv::Dive && !f.grounded {
                f.mv = Mv::Pound;
            }
            if f.mv == Mv::Pound {
                f.vel[1] = -62.0;
                nx = f.pos[0] + kx;
                nz = f.pos[2] + kz;
            }
            ny += f.vel[1];
        }
        // Walls first, then the floor under the new spot.
        let p = terrain.wall([nx, ny + 40.0, nz]);
        nx = p[0];
        nz = p[2];
        let probe = f.pos[1] + 90.0;
        let was_ground = f.grounded;
        match terrain.ground(nx, probe.max(ny + 60.0), nz) {
            None => {
                // Void: only a knockback or a dive carries a bot over an edge.
                if f.knock[0].abs() + f.knock[2].abs() > 2.0 || f.dive_t > 0 || !was_ground {
                    f.pos = [nx, ny - if was_ground { 30.0 } else { 0.0 }, nz];
                    f.grounded = false;
                }
            }
            Some(g) => {
                let step_up = g.y - f.pos[1];
                let blocked_ground = was_ground && ((g.surf != Surf::Solid && f.knock[0].abs() + f.knock[2].abs() < 2.0 && f.dive_t == 0) || step_up > 95.0);
                if blocked_ground {
                    // stay put (the brain's stuck logic steers around it)
                } else {
                    f.pos[0] = nx;
                    f.pos[2] = nz;
                    if !was_ground || ny > g.y {
                        f.pos[1] = ny.max(g.y);
                    } else if f.pos[1] - g.y > 60.0 {
                        // walked off a ledge: start falling
                        f.pos[1] = ny;
                        f.grounded = false;
                    } else {
                        f.pos[1] = g.y;
                    }
                    if f.pos[1] <= g.y + 0.5 {
                        f.pos[1] = g.y;
                        if !f.grounded && f.vel[1] <= 0.0 {
                            f.grounded = true;
                            f.vel[1] = 0.0;
                        }
                    }
                }
            }
        }
        if f.grounded {
            if f.mv == Mv::Pound {
                f.mv = Mv::PoundLand;
            } else if f.mv == Mv::PoundLand || f.mv == Mv::Dive || f.mv == Mv::Punch || f.mv == Mv::Air || f.mv == Mv::Knock {
                f.mv = Mv::Idle;
            }
            f.mv = if f.mv == Mv::PoundLand { f.mv } else if vx.abs() + vz.abs() > 0.5 { Mv::Move } else { Mv::Idle };
        } else if f.mv != Mv::Pound && f.mv != Mv::Dive {
            f.mv = Mv::Air;
        }
        // Ground attacks requested by the brain.
        if f.grounded || f.mv == Mv::Air {
            match intent.attack {
                Atk::Punch if f.atk_cd == 0 && f.mv != Mv::Dive => {
                    f.mv = Mv::Punch;
                    f.punch_t = 6;
                    f.atk_cd = 12;
                }
                Atk::Dive if f.atk_cd == 0 && f.dive_t == 0 => {
                    f.dive_t = 7;
                    f.atk_cd = 70;
                    f.mv = Mv::Dive;
                }
                _ => {}
            }
        }
        let _ = arena;
    }

    // -----------------------------------------------------------------------------------------
    // Melee
    // -----------------------------------------------------------------------------------------

    fn melee(&mut self, a: usize) {
        let (mv, prev, pos, yaw, mode) = {
            let f = &self.fighters[a];
            (f.mv, f.prev_mv, f.pos, f.yaw, f.mode)
        };
        if mode == Mode::Drop || self.over {
            return;
        }
        let diff = self.diff_of(a).damage();
        match mv {
            Mv::Punch if prev != Mv::Punch => {
                let mut best: Option<(f32, usize)> = None;
                for o in &self.fighters {
                    if o.id == a || !o.alive || o.mode == Mode::Drop {
                        continue;
                    }
                    let d = dist2(pos, o.pos);
                    if d < 170.0 && (o.pos[1] - pos[1]).abs() < 130.0 && wrap_pi(heading_to(pos, o.pos) - yaw).abs() < 1.25 && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, o.id));
                    }
                }
                if let Some((_, t)) = best {
                    let dir = self.knock_dir(pos, self.fighters[t].pos);
                    self.hit(t, Some(a), 7.0 * diff, dir, 11.0, 12.0, pos);
                }
            }
            Mv::Dive => {
                for t in 0..self.fighters.len() {
                    if t == a {
                        continue;
                    }
                    let o = &self.fighters[t];
                    if !o.alive || o.mode == Mode::Drop || o.iframes > 0 {
                        continue;
                    }
                    let d = dist2(pos, o.pos);
                    if d < 150.0 && (o.pos[1] - pos[1]).abs() < 150.0 {
                        let dir = self.knock_dir(pos, o.pos);
                        let o_pos = o.pos;
                        self.hit(t, Some(a), 15.0 * diff, dir, 22.0, 15.0, pos);
                        let _ = o_pos;
                        self.fighters[a].dive_t = 0;
                    }
                }
            }
            Mv::Pound => {
                // Stomp: coming down on a head.
                for t in 0..self.fighters.len() {
                    if t == a {
                        continue;
                    }
                    let o = &self.fighters[t];
                    if !o.alive || o.mode == Mode::Drop || o.iframes > 0 {
                        continue;
                    }
                    if dist2(pos, o.pos) < 110.0 && pos[1] > o.pos[1] + 30.0 && pos[1] < o.pos[1] + 220.0 {
                        let dir = self.knock_dir(pos, o.pos);
                        if self.hit(t, Some(a), 12.0 * diff, dir, 8.0, 16.0, pos) > 0.0 {
                            self.fighters[a].bounce = true;
                            self.fighters[a].mv = Mv::Air;
                            self.fighters[a].vel[1] = 38.0;
                        }
                    }
                }
            }
            Mv::PoundLand if prev != Mv::PoundLand => {
                let charged = self.fighters[a].quake > 0;
                let (r, dmg, force, lift) = if charged { (520.0, 30.0 * diff, 20.0, 18.0) } else { (230.0, 11.0 * diff, 13.0, 14.0) };
                if charged {
                    self.fighters[a].quake -= 1;
                }
                self.add_ring(RingKind::Shock, pos, r);
                self.events.push(Event::Shock { pos, r });
                // Shockwaves only hurt things standing on the ground (jump to dodge).
                for t in 0..self.fighters.len() {
                    if t == a || !self.fighters[t].alive || self.fighters[t].mode == Mode::Drop {
                        continue;
                    }
                    let o = &self.fighters[t];
                    let d = dist2(pos, o.pos);
                    if d < r && o.grounded && (o.pos[1] - pos[1]).abs() < 120.0 {
                        let fall = 1.0 - 0.5 * d / r;
                        let dir = self.knock_dir(pos, o.pos);
                        self.hit(t, Some(a), dmg * fall, dir, force * fall, lift, pos);
                    }
                }
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------------------------
    // Projectiles
    // -----------------------------------------------------------------------------------------

    fn step_shots(&mut self, terrain: &dyn Terrain) {
        for s in 0..self.shots.len() {
            if !self.shots[s].alive {
                continue;
            }
            let mut sh = self.shots[s];
            sh.life = sh.life.saturating_sub(1);
            sh.spin += 0.5;
            sh.vel[1] -= match sh.kind {
                ShotKind::Fire => 2.4,
                ShotKind::Shell => 2.2,
                ShotKind::Bomb => 3.0,
            };
            let old = sh.pos;
            sh.pos = [old[0] + sh.vel[0], old[1] + sh.vel[1], old[2] + sh.vel[2]];
            let mut explode = sh.kind == ShotKind::Bomb && sh.life == 0;
            // Walls: bounce shells, stop bombs, kill fireballs.
            let w = terrain.wall([sh.pos[0], sh.pos[1] + 30.0, sh.pos[2]]);
            if (w[0] - sh.pos[0]).abs() + (w[2] - sh.pos[2]).abs() > 0.5 {
                match sh.kind {
                    ShotKind::Shell => {
                        sh.vel[0] = -sh.vel[0];
                        sh.vel[2] = -sh.vel[2];
                        sh.pos = [old[0], sh.pos[1], old[2]];
                        sh.bounces += 1;
                    }
                    ShotKind::Bomb => {
                        sh.vel[0] = 0.0;
                        sh.vel[2] = 0.0;
                        sh.pos = [old[0], sh.pos[1], old[2]];
                    }
                    ShotKind::Fire => sh.life = 0,
                }
            }
            match terrain.ground(sh.pos[0], old[1].max(sh.pos[1]) + 50.0, sh.pos[2]) {
                Some(g) if sh.pos[1] <= g.y + 20.0 && sh.vel[1] < 0.0 => {
                    sh.pos[1] = g.y + 20.0;
                    match sh.kind {
                        ShotKind::Shell => {
                            // Hops on hard landings, otherwise rolls along the ground.
                            if sh.vel[1] < -6.0 {
                                sh.vel[1] = -sh.vel[1] * 0.55;
                                sh.bounces += 1;
                            } else {
                                sh.vel[1] = 0.0;
                            }
                        }
                        ShotKind::Bomb => {
                            sh.vel[1] = (-sh.vel[1] * 0.3).min(7.0);
                            sh.vel[0] *= 0.55;
                            sh.vel[2] *= 0.55;
                        }
                        ShotKind::Fire => {
                            sh.vel[1] = 11.0;
                            sh.bounces += 1;
                        }
                    }
                }
                None if sh.pos[1] < self.arena.kill_y => sh.life = 0,
                _ => {}
            }
            // Fighters.
            let mut hit_one = false;
            if !explode {
                for t in 0..self.fighters.len() {
                    let o = &self.fighters[t];
                    if t == sh.owner || !o.alive || o.mode == Mode::Drop {
                        continue;
                    }
                    let dy = sh.pos[1] - o.pos[1];
                    if dist2(sh.pos, o.pos) < 80.0 && dy > -30.0 && dy < 170.0 {
                        match sh.kind {
                            ShotKind::Bomb => explode = true,
                            ShotKind::Shell => {
                                let l = sh.vel[0].hypot(sh.vel[2]).max(1.0);
                                let dir = [sh.vel[0] / l, sh.vel[2] / l];
                                if self.hit(t, Some(sh.owner), 16.0 * sh.power, dir, 13.0, 12.0, old) > 0.0 {
                                    hit_one = true;
                                    sh.bounces += 2;
                                }
                            }
                            ShotKind::Fire => {
                                let l = sh.vel[0].hypot(sh.vel[2]).max(1.0);
                                let dir = [sh.vel[0] / l, sh.vel[2] / l];
                                if self.hit(t, Some(sh.owner), 7.0 * sh.power, dir, 6.0, 6.0, old) > 0.0 {
                                    hit_one = true;
                                    sh.life = 0;
                                }
                            }
                        }
                        if explode || hit_one {
                            break;
                        }
                    }
                }
            }
            if explode {
                let r = 320.0;
                self.add_ring(RingKind::Blast, sh.pos, r);
                self.events.push(Event::Explosion { pos: sh.pos, r });
                // The thrower is not spared by their own bomb, but takes less.
                let owner = sh.owner;
                for t in 0..self.fighters.len() {
                    let o = &self.fighters[t];
                    if !o.alive || o.mode == Mode::Drop {
                        continue;
                    }
                    let d = dist2(sh.pos, o.pos);
                    if d > r || (o.pos[1] - sh.pos[1]).abs() > 260.0 {
                        continue;
                    }
                    let fall = 1.0 - 0.6 * (d / r);
                    let mult = if t == owner { 0.4 } else { 1.0 };
                    let dir = self.knock_dir(sh.pos, o.pos);
                    self.hit(t, Some(owner), 38.0 * sh.power * fall * mult, dir, 22.0 * fall.max(0.5), 17.0, sh.pos);
                }
                sh.alive = false;
            } else if sh.life == 0 || sh.bounces >= 5 {
                sh.alive = false;
            }
            self.shots[s] = sh;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Pickups
    // -----------------------------------------------------------------------------------------

    fn airdrops_due(&self) -> u8 {
        match self.zv.phase {
            0 => 0,
            p if self.zv.stage == Stage::Hold => (p as u8).min(5).saturating_sub(1),
            p => (p as u8).min(5),
        }
    }

    fn schedule_airdrops(&mut self, terrain: &dyn Terrain) {
        // One crate whenever a phase starts shrinking, aimed inside the circle that is coming.
        let want = self.airdrops_due();
        if self.airdrops < want && self.airdrops < 5 && self.zv.phase > 0 {
            self.airdrops += 1;
            let c = self.zv.next;
            for _ in 0..12 {
                let a = self.rng.range(0.0, TAU);
                let d = self.rng.f().sqrt() * c.r * 0.7;
                let (x, z) = (c.x + a.sin() * d, c.z + a.cos() * d);
                if let Some(g) = terrain.ground(x, self.arena.top_y + 400.0, z).filter(|g| g.surf == Surf::Solid) {
                    let item = roll_item(&mut self.rng, Source::Airdrop);
                    let pos = [x, g.y, z];
                    if self.add_pickup(pos, item, PickKind::Crate { drop_y: g.y + 3000.0, opened: false }).is_some() {
                        self.events.push(Event::AirdropCall { pos });
                    }
                    break;
                }
            }
        }
    }

    fn collect(&mut self, terrain: &dyn Terrain) {
        for p in 0..self.pickups.len() {
            if !self.pickups[p].alive {
                continue;
            }
            let mut pk = self.pickups[p];
            pk.age += 1;
            if let PickKind::Crate { drop_y, opened } = pk.kind {
                if drop_y > pk.pos[1] {
                    let nd = (drop_y - 16.0).max(pk.pos[1]);
                    pk.kind = PickKind::Crate { drop_y: nd, opened };
                    if nd <= pk.pos[1] {
                        self.events.push(Event::AirdropLand { pos: pk.pos });
                    }
                    self.pickups[p] = pk;
                    continue;
                }
            }
            // Opened containers linger briefly, then go.
            if pk.opened() {
                if pk.age > 150 {
                    pk.alive = false;
                }
                self.pickups[p] = pk;
                continue;
            }
            let reach = if pk.is_container() { 150.0 } else { 105.0 };
            let mut taker: Option<usize> = None;
            for f in &self.fighters {
                if f.alive && f.mode == Mode::Ground && dist2(f.pos, pk.pos) < reach && (f.pos[1] - pk.pos[1]).abs() < 170.0 && (pk.age > 12 || pk.is_container()) {
                    taker = Some(f.id);
                    break;
                }
            }
            let Some(t) = taker else {
                self.pickups[p] = pk;
                continue;
            };
            match pk.kind {
                PickKind::Item => {
                    if self.give(t, pk.item) {
                        pk.alive = false;
                        self.events.push(Event::Pickup { who: t, kind: pk.item.kind });
                    }
                }
                PickKind::Chest { .. } | PickKind::Crate { .. } => {
                    pk.age = 0;
                    pk.kind = match pk.kind {
                        PickKind::Chest { .. } => PickKind::Chest { opened: true },
                        PickKind::Crate { drop_y, .. } => PickKind::Crate { drop_y, opened: true },
                        k => k,
                    };
                    self.events.push(Event::ChestOpen { pos: pk.pos });
                    let air = matches!(pk.kind, PickKind::Crate { .. });
                    let src = if air { Source::Airdrop } else { Source::Chest };
                    let n = if air { 3 } else { 2 };
                    for k in 0..n {
                        let item = if air && k == 0 { pk.item } else { roll_item(&mut self.rng, src) };
                        let a = self.rng.range(0.0, TAU) + k as f32 * 2.1;
                        let d = self.rng.range(90.0, 190.0);
                        let (x, z) = (pk.pos[0] + a.sin() * d, pk.pos[2] + a.cos() * d);
                        let y = terrain.ground(x, pk.pos[1] + 200.0, z).map_or(pk.pos[1], |g| g.y);
                        self.add_pickup([x, y, z], item, PickKind::Item);
                    }
                }
            }
            self.pickups[p] = pk;
        }
    }

    /// Put `item` in `who`'s inventory. Weapons take a free slot, merge into a matching one, or
    /// replace a worse one (which drops to the floor). Returns false when it did not fit.
    pub fn give(&mut self, who: usize, item: Item) -> bool {
        let pos = self.fighters[who].pos;
        let f = &mut self.fighters[who];
        if !item.kind.is_weapon() {
            match f.cons {
                None => {
                    f.cons = Some(item);
                    return true;
                }
                Some(c) if item.rarity > c.rarity => {
                    f.cons = Some(item);
                    self.drop_item(pos, c);
                    return true;
                }
                _ => return false,
            }
        }
        // Same kind and rarity: top up.
        for s in f.slots.iter_mut().flatten() {
            if s.kind == item.kind && s.rarity == item.rarity {
                s.ammo = (s.ammo + item.ammo).min(item.kind.ammo(item.rarity) * 2);
                return true;
            }
        }
        if let Some(s) = f.slots.iter_mut().find(|s| s.is_none()) {
            *s = Some(item);
            return true;
        }
        // Full: swap out the lowest rarity if the new one is better.
        let (worst, wr) = f.slots.iter().enumerate().filter_map(|(i, s)| s.map(|it| (i, it))).min_by_key(|(_, it)| it.rarity).map(|(i, it)| (i, it)).unwrap();
        if item.rarity > wr.rarity {
            f.slots[worst] = Some(item);
            self.drop_item(pos, wr);
            return true;
        }
        false
    }

    fn drop_item(&mut self, pos: [f32; 3], item: Item) {
        if let Some(i) = self.free_pickup() {
            // A little ahead so it is not instantly picked up again (pickups wait 12 ticks).
            self.pickups[i] = Pickup { alive: true, pos, item, kind: PickKind::Item, age: 0 };
        }
    }

    // -----------------------------------------------------------------------------------------
    // Environment: the Blight, hazards, eliminations
    // -----------------------------------------------------------------------------------------

    fn environment(&mut self, terrain: &dyn Terrain) {
        let dps = self.zv.dps;
        let cur = self.zv.cur;
        let kill_y = self.arena.kill_y;
        let mut dead: Vec<(usize, Cause)> = vec![];
        for f in self.fighters.iter_mut() {
            if !f.alive || f.mode == Mode::Drop {
                continue;
            }
            if dps > 0.0 && !cur.contains(f.pos) && !(self.god && f.is_player) {
                f.hp -= dps * DT;
                if f.hp <= 0.0 {
                    dead.push((f.id, Cause::Blight));
                    continue;
                }
            }
            if f.hp <= 0.0 {
                dead.push((f.id, Cause::Fight));
                continue;
            }
            let fell = f.pos[1] < kill_y;
            let hazard = f.grounded && terrain.ground(f.pos[0], f.pos[1] + 40.0, f.pos[2]).is_some_and(|g| g.surf == Surf::Hazard && f.pos[1] <= g.y + 5.0);
            if fell || hazard {
                dead.push((f.id, Cause::Fall));
            }
        }
        for (id, cause) in dead {
            if self.god && id == 0 {
                self.fighters[0].hp = MAX_HP;
                if cause == Cause::Fall {
                    // Test hook: god mode also catches falls.
                    let a = self.arena;
                    self.fighters[0].pos = [a.cx, a.top_y + 400.0, a.cz];
                }
                continue;
            }
            self.eliminate(id, cause);
        }
    }

    pub fn eliminate(&mut self, id: usize, cause: Cause) {
        if self.over || !self.fighters[id].alive {
            return;
        }
        let tick = self.tick;
        let place = self.alive as u32;
        let killer = self.fighters[id].last_hit.filter(|(k, t)| *k != id && tick.saturating_sub(*t) <= 6 * TPS && self.fighters[*k].alive).map(|(k, _)| k);
        {
            let f = &mut self.fighters[id];
            f.alive = false;
            f.hp = 0.0;
            f.place = place;
            f.died_tick = tick;
            f.sim = false;
        }
        self.alive -= 1;
        if let Some(k) = killer {
            self.fighters[k].kills += 1;
        }
        if id == 0 {
            self.player_place = place;
        }
        // Their stuff spills where they fell (if they are still over land).
        let pos = self.fighters[id].pos;
        let spill: Vec<Item> = self.fighters[id].slots.iter().flatten().copied().chain(self.fighters[id].cons).collect();
        if cause != Cause::Fall {
            for (k, it) in spill.into_iter().enumerate() {
                let a = k as f32 * 2.1;
                let p = [pos[0] + a.sin() * 110.0, pos[1], pos[2] + a.cos() * 110.0];
                if let Some(i) = self.free_pickup() {
                    self.pickups[i] = Pickup { alive: true, pos: p, item: it, kind: PickKind::Item, age: 0 };
                }
            }
        }
        self.events.push(Event::Elim { victim: id, killer, cause, place });
        if self.alive <= 1 {
            self.over = true;
            if let Some(w) = self.fighters.iter().find(|f| f.alive).map(|f| f.id) {
                self.winner = Some(w);
                self.fighters[w].place = 1;
                if w == 0 {
                    self.player_place = 1;
                }
                self.events.push(Event::Winner { who: w });
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Bots
    // -----------------------------------------------------------------------------------------

    fn bot_items(&mut self, i: usize) {
        if !self.fighters[i].alive || self.fighters[i].mode == Mode::Drop {
            return;
        }
        let f = &self.fighters[i];
        let hp = f.hp;
        let threat = f.brain.target.filter(|t| self.fighters[*t].alive).map(|t| dist2(f.pos, self.fighters[t].pos));
        // Consumables.
        if let Some(c) = f.cons {
            let want = match c.kind {
                Kind::Heart | Kind::OneUp => hp < 55.0 && threat.is_none_or(|d| d > 250.0) || hp < 30.0,
                Kind::Cap => threat.is_some_and(|d| d < 800.0) && f.shield <= 0.0,
                Kind::Boots => f.brain.state == BotState::Rotate && !self.zv.cur.contains(f.pos) && f.boost_t == 0,
                _ => false,
            };
            if want && f.cd[2] == 0 {
                self.use_item(i, 2, 0.0);
            }
        }
        // Weapons: fire at the target when lined up.
        let f = &self.fighters[i];
        let Some(t) = f.brain.target.filter(|t| self.fighters[*t].alive && self.fighters[*t].mode != Mode::Drop) else { return };
        let d = dist2(f.pos, self.fighters[t].pos);
        let aim = heading_to(f.pos, self.fighters[t].pos);
        let level = self.diff_of(i);
        let err = level.aim_error() * f.brain.aim_bias;
        let state = f.brain.state;
        if state != BotState::Engage && state != BotState::Flee {
            return;
        }
        let nearby = self.fighters.iter().filter(|o| o.id != i && o.alive && o.mode != Mode::Drop && dist2(o.pos, f.pos) < 420.0).count();
        for slot in 0..2 {
            let f = &self.fighters[i];
            let Some(it) = f.slots[slot] else { continue };
            if f.cd[slot] > 0 {
                continue;
            }
            let go = match it.kind {
                Kind::Shell => (200.0..1300.0).contains(&d),
                Kind::Flower => (150.0..1000.0).contains(&d),
                Kind::Bomb => (350.0..1100.0).contains(&d),
                Kind::StarBurst => nearby >= 2 || (d < 380.0 && hp < 45.0),
                Kind::Quake => d < 520.0 && f.quake == 0,
                _ => false,
            };
            if go && self.rng.chance(0.28 + 0.4 * level.skill()) {
                // Lead the shot a little at long range, plus aim error.
                let noise = err + self.rng.range(-1.0, 1.0) * level.aim_error() * 0.5;
                self.use_item(i, slot, aim + noise);
                break;
            }
        }
    }

    /// Choose the nearest `cap` bots within `radius` of `focus` to run on the physics engine.
    /// Returns nothing; flags are in `fighters[i].sim` / `sim_started`.
    pub fn choose_sim(&mut self, focus: [f32; 3], radius: f32, cap: usize) {
        // Nearest candidates first, in a fixed-size scratch array (no allocation per tick).
        let mut cands = [(0.0f32, 0usize); MAX_FIGHTERS];
        let mut n = 0;
        for f in &self.fighters {
            if f.is_player || !f.alive {
                continue;
            }
            // Dropping bots stay kinematic until they are about to land.
            let low = f.mode == Mode::Ground || f.pos[1] < self.arena.top_y + 500.0;
            let d = dist2(f.pos, focus);
            if low && d < radius && n < MAX_FIGHTERS {
                cands[n] = (d, f.id);
                n += 1;
            }
        }
        let cands = &mut cands[..n];
        cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        for f in self.fighters.iter_mut() {
            if f.is_player {
                continue;
            }
            f.sim_started = false;
            let rank = cands.iter().position(|c| c.1 == f.id);
            let want = rank.is_some_and(|r| r < cap);
            // Hysteresis so a bot on the boundary does not flip every tick.
            let keep = f.sim && f.alive && f.mode == Mode::Ground && rank.is_some_and(|r| r < cap + 3);
            let on = want || keep;
            if on && !f.sim {
                f.sim_started = true;
            }
            f.sim = on;
        }
    }

    /// How well fighter `who` fights right now. The chosen difficulty is how dangerous the bots are *to you*:
    /// a bot going after the player uses it, bots fighting each other always fight at Normal, so an Easy match
    /// is not also a slow one (on Easy the field used to stay whole for the first two minutes).
    fn diff_of(&self, who: usize) -> Difficulty {
        if who == 0 && !self.autopilot {
            Difficulty::Normal
        } else if self.fighters[who].brain.target == Some(0) {
            self.cfg.difficulty
        } else {
            Difficulty::Normal
        }
    }

    fn think(&mut self, i: usize, terrain: &dyn Terrain) {
        let diff = self.diff_of(i);
        let tick = self.tick;
        let zv = self.zv;
        let state = self.fighters[i].brain.state;
        if self.fighters[i].mode == Mode::Drop {
            return;
        }
        let engaged = matches!(state, BotState::Engage | BotState::Flee);
        let interval = if engaged { diff.reaction() } else { 9 };
        if tick < self.fighters[i].brain.think_at {
            // Keep walking toward the goal between decisions, but steer around walls each tick.
            self.steer(i, terrain);
            return;
        }
        self.fighters[i].brain.think_at = tick + interval;
        let me = Me { pos: self.fighters[i].pos, hp: self.fighters[i].hp, weapons: self.fighters[i].weapon_count(), cons: self.fighters[i].cons };
        let hp_frac = me.hp / MAX_HP;
        // Nearest visible enemy.
        let mut target: Option<(f32, usize)> = None;
        let mut near_threats = 0;
        for o in &self.fighters {
            if o.id == i || !o.alive || o.mode == Mode::Drop {
                continue;
            }
            let d = dist2(me.pos, o.pos);
            if d < 1900.0 && (o.pos[1] - me.pos[1]).abs() < 900.0 {
                if target.is_none_or(|(bd, _)| d < bd) {
                    target = Some((d, o.id));
                }
                if d < 600.0 {
                    near_threats += 1;
                }
            }
        }
        let outside_now = !zv.cur.contains(me.pos) || dist2(me.pos, [zv.cur.x, 0.0, zv.cur.z]) > zv.cur.r * 0.94;
        let next_c = zv.next;
        let closing = zv.stage == Stage::Shrink || (zv.stage == Stage::Hold && zv.secs_left < 22.0) || zv.stage == Stage::Final;
        let outside_next = dist2(me.pos, [next_c.x, 0.0, next_c.z]) > next_c.r * 0.85;
        let must_rotate = outside_now || (closing && outside_next);
        let has_attack = me.weapons > 0;
        let b = &mut self.fighters[i].brain;
        b.target = target.map(|t| t.1);
        let td = target.map_or(f32::MAX, |t| t.0);
        // Decide.
        let flee = hp_frac < diff.flee_hp() && td < 1000.0 && me.cons.is_none_or(|c| !matches!(c.kind, Kind::Heart | Kind::OneUp));
        let want_fight = td < 300.0 + 1300.0 * b.aggression && (has_attack || td < 600.0) && (b.aggression > 0.3 || td < 400.0);
        let new_state = if flee {
            BotState::Flee
        } else if must_rotate && !(td < 260.0 && b.aggression > 0.5) {
            BotState::Rotate
        } else if want_fight && hp_frac > 0.2 {
            BotState::Engage
        } else {
            BotState::Loot
        };
        b.state = new_state;
        let _ = near_threats;
        match new_state {
            BotState::Flee => {
                if let Some((_, t)) = target {
                    let away = heading_to(self.fighters[t].pos, me.pos);
                    let toward_zone = heading_to(me.pos, [zv.next.x, 0.0, zv.next.z]);
                    // Run away, bent toward safety.
                    let h = away + wrap_pi(toward_zone - away) * 0.35;
                    self.fighters[i].brain.goal = [me.pos[0] + h.sin() * 900.0, me.pos[1], me.pos[2] + h.cos() * 900.0];
                }
            }
            BotState::Rotate => {
                let c = zv.next;
                let inner = c.r * self.fighters[i].brain.offset[1].min(0.7);
                let a = self.fighters[i].brain.offset[0];
                self.fighters[i].brain.goal = [c.x + a.sin() * inner, me.pos[1], c.z + a.cos() * inner];
            }
            BotState::Engage => {
                if let Some((_, t)) = target {
                    self.fighters[i].brain.goal = self.fighters[t].pos;
                }
            }
            BotState::Loot => {
                // The nearest pickup worth having; otherwise drift toward the safe area.
                let want_weapon = me.weapons < 2;
                let want_cons = me.cons.is_none();
                let hurt = me.hp < 70.0;
                let mut best: Option<(f32, usize)> = None;
                for (pi, p) in self.pickups.iter().enumerate() {
                    if !p.alive || p.opened() {
                        continue;
                    }
                    if let PickKind::Crate { drop_y, .. } = p.kind {
                        if drop_y > p.pos[1] + 50.0 {
                            continue;
                        }
                    }
                    let useful = p.is_container() || (p.item.kind.is_weapon() && (want_weapon || p.item.rarity > Rarity::Common)) || (!p.item.kind.is_weapon() && (want_cons || (hurt && matches!(p.item.kind, Kind::Heart | Kind::OneUp))));
                    if !useful {
                        continue;
                    }
                    let d = dist2(me.pos, p.pos);
                    // Stay out of the Blight when shopping.
                    let safe = zv.next.contains(p.pos) || zv.cur.contains(p.pos);
                    let score = d + if safe { 0.0 } else { 2500.0 } - if p.is_container() { 150.0 } else { 0.0 };
                    if d < 3200.0 && best.is_none_or(|(bs, _)| score < bs) {
                        best = Some((score, pi));
                    }
                }
                match best {
                    Some((_, pi)) => {
                        self.fighters[i].brain.goal_pickup = Some(pi);
                        self.fighters[i].brain.goal = self.pickups[pi].pos;
                    }
                    None => {
                        self.fighters[i].brain.goal_pickup = None;
                        let c = zv.next;
                        let a = self.fighters[i].brain.offset[0];
                        let inner = c.r * self.fighters[i].brain.offset[1];
                        self.fighters[i].brain.goal = [c.x + a.sin() * inner, me.pos[1], c.z + a.cos() * inner];
                        self.fighters[i].brain.state = BotState::Rotate;
                    }
                }
            }
            BotState::Drop => {}
        }
        self.act(i, td, target.map(|t| t.1), terrain);
        self.steer(i, terrain);
    }

    /// Turn the current goal and fight state into this tick's controls.
    fn act(&mut self, i: usize, td: f32, target: Option<usize>, _terrain: &dyn Terrain) {
        let diff = self.cfg.difficulty;
        let skill = diff.skill();
        let me_pos = self.fighters[i].pos;
        let grounded = self.fighters[i].grounded;
        let state = self.fighters[i].brain.state;
        let goal = self.fighters[i].brain.goal;
        let mut it = Intent { heading: heading_to(me_pos, goal), mag: 1.0, ..Default::default() };
        let d_goal = dist2(me_pos, goal);
        match state {
            BotState::Engage => {
                if let Some(t) = target {
                    let tp = self.fighters[t].pos;
                    let tair = !self.fighters[t].grounded;
                    it.heading = heading_to(me_pos, tp);
                    it.mag = if td < 110.0 { 0.0 } else { 1.0 };
                    if td < 140.0 && self.rng.chance(0.55 + 0.4 * skill) {
                        it.attack = Atk::Punch;
                    } else if (230.0..560.0).contains(&td) && grounded && self.rng.chance(0.18 + 0.4 * skill) {
                        it.attack = Atk::Dive;
                    } else if td < 420.0 && !grounded && me_pos[1] > tp[1] + 100.0 && self.rng.chance(0.4 + 0.5 * skill) {
                        it.pound = true;
                    } else if grounded && td < 600.0 && !tair && self.rng.chance(0.1 * skill + 0.05) {
                        // A hop to set up a pound or dodge a shell.
                        it.jump = true;
                        it.hold_a = true;
                    } else if td > 400.0 && grounded && self.rng.chance(0.06) {
                        it.jump = true;
                    }
                    // Circle-strafe while waiting.
                    if td < 330.0 && self.rng.chance(0.35) {
                        it.heading += self.fighters[i].brain.aim_bias.signum() * 1.2;
                        it.mag = 0.8;
                    }
                }
            }
            BotState::Flee => {
                if grounded && self.rng.chance(0.12) {
                    it.jump = true;
                    it.hold_a = true;
                }
            }
            _ => {
                if d_goal < 60.0 {
                    it.mag = 0.0;
                }
                if grounded && self.rng.chance(0.015) {
                    it.jump = true;
                }
            }
        }
        // Stay out of the void: do not run toward a hazard edge (handled by terrain checks).
        self.fighters[i].intent = it;
    }

    /// Per-tick steering: unstick, and bend around obstacles.
    fn steer(&mut self, i: usize, terrain: &dyn Terrain) {
        let tick = self.tick;
        let f = &mut self.fighters[i];
        if f.mode == Mode::Drop {
            return;
        }
        if tick % 15 == 0 {
            let moved = dist2(f.pos, f.brain.last_check);
            f.brain.last_check = f.pos;
            if moved < 55.0 && f.intent.mag > 0.1 && f.grounded {
                f.brain.stuck_t = 24;
                f.brain.stuck_dir = if (tick / 15 + i as u32) % 2 == 0 { 1.2 } else { -1.2 };
            }
        }
        if f.brain.stuck_t > 0 {
            f.brain.stuck_t -= 1;
            f.intent.heading += f.brain.stuck_dir;
            f.intent.mag = 1.0;
            if f.brain.stuck_t % 8 == 0 {
                f.intent.jump = true;
                f.intent.hold_a = true;
            }
        }
        // Look ahead: no running off edges or into hazards unless fleeing for its life.
        if f.grounded && f.intent.mag > 0.0 && f.dive_t == 0 {
            let h = f.intent.heading;
            let probe = [f.pos[0] + h.sin() * 130.0, f.pos[1], f.pos[2] + h.cos() * 130.0];
            let bad = match terrain.ground(probe[0], f.pos[1] + 120.0, probe[2]) {
                None => true,
                Some(g) => g.surf == Surf::Hazard || f.pos[1] - g.y > 260.0,
            };
            if bad {
                // Try to slide along the edge.
                for turn in [0.8f32, -0.8, 1.6, -1.6] {
                    let h2 = h + turn;
                    let p2 = [f.pos[0] + h2.sin() * 130.0, f.pos[1], f.pos[2] + h2.cos() * 130.0];
                    if terrain.ground(p2[0], f.pos[1] + 120.0, p2[2]).is_some_and(|g| g.surf == Surf::Solid && f.pos[1] - g.y < 200.0) {
                        f.intent.heading = h2;
                        return;
                    }
                }
                f.intent.mag = 0.0;
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Test and screenshot helpers
    // -----------------------------------------------------------------------------------------

    /// Skip ahead: set the clock, land everybody on the ground near their landing camp with a
    /// starter kit. Used by the screenshot hooks and tests.
    pub fn fast_forward(&mut self, secs: f32, terrain: &dyn Terrain) {
        self.tick = (secs * TPS as f32) as u32;
        self.zv = self.zone.at(self.secs());
        self.prev_phase = (self.zv.phase, self.zv.stage);
        self.airdrops = self.airdrops_due().saturating_sub(1);
        for i in 0..self.fighters.len() {
            if self.fighters[i].mode == Mode::Drop {
                let l = self.fighters[i].brain.landing;
                let a = (i as f32) * 2.399;
                let c = self.zv.cur;
                // Land inside the circle that is currently safe.
                let (mut x, mut z) = (l[0], l[2]);
                if !c.contains([x, 0.0, z]) {
                    let d = c.r * 0.8 * ((i % 7) as f32 / 7.0 + 0.1);
                    x = c.x + a.sin() * d;
                    z = c.z + a.cos() * d;
                }
                if let Some(g) = terrain.ground(x, self.arena.top_y + 500.0, z) {
                    let f = &mut self.fighters[i];
                    f.pos = [x, g.y, z];
                    f.mode = Mode::Ground;
                    f.grounded = true;
                    f.mv = Mv::Idle;
                    f.vel = [0.0; 3];
                    f.iframes = 0;
                }
            }
            if i > 0 {
                for _ in 0..2 {
                    let it = roll_item(&mut self.rng, Source::Floor);
                    let _ = self.give(i, it);
                }
            }
        }
    }

    /// Quietly remove bots until `alive` remain (screenshot hook); placements stay consistent.
    pub fn cull_to(&mut self, alive: usize) {
        let ids: Vec<usize> = self.fighters.iter().rev().filter(|f| f.alive && !f.is_player).map(|f| f.id).collect();
        for id in ids {
            if self.alive <= alive.max(2) {
                break;
            }
            // Quietly: no killer credit and nothing spilled.
            self.fighters[id].last_hit = None;
            self.fighters[id].slots = [None; 2];
            self.fighters[id].cons = None;
            self.eliminate(id, Cause::Fight);
        }
        self.events.clear();
    }

    /// Give `who` a full kit (screenshot hook).
    pub fn give_kit(&mut self, who: usize) {
        self.fighters[who].slots = [Some(Item::new(Kind::Shell, Rarity::Rare)), Some(Item::new(Kind::Bomb, Rarity::Epic))];
        self.fighters[who].cons = Some(Item::new(Kind::Heart, Rarity::Rare));
    }
}

impl Fighter {
    /// Knockback is consumed by whoever moves the body; if nobody did this tick, drop it so it
    /// cannot fire late.
    fn knocked_clear_if_unused(&mut self) {
        if !self.sim && !self.is_player {
            self.knocked = false;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A disc of flat ground; beyond `edge` is the void; `hazard` rings are water.
    pub struct Flat {
        pub edge: f32,
        pub hazard: Option<(f32, f32)>,
    }

    impl Terrain for Flat {
        fn ground(&self, x: f32, y: f32, z: f32) -> Option<Ground> {
            let d = x.hypot(z);
            if d > self.edge || y < 0.0 {
                return None;
            }
            let surf = match self.hazard {
                Some((a, b)) if d > a && d < b => Surf::Hazard,
                _ => Surf::Solid,
            };
            Some(Ground { y: 0.0, surf })
        }
        fn wall(&self, pos: [f32; 3]) -> [f32; 3] {
            pos
        }
    }

    fn arena() -> Arena {
        Arena { cx: 0.0, cz: 0.0, r: 6000.0, top_y: 0.0, kill_y: -600.0 }
    }

    fn flat() -> Flat {
        Flat { edge: 6300.0, hazard: None }
    }

    fn new_match(seed: u64, bots: usize) -> (Lss, Flat) {
        let t = flat();
        (Lss::new(Config::new(seed, bots, arena()), &t), t)
    }

    /// Land everybody on the ground at spread-out spots and run `n` ticks with bots thinking.
    fn run(l: &mut Lss, t: &Flat, ticks: u32) {
        let m = vec![None; l.fighters.len()];
        for _ in 0..ticks {
            l.tick(t, &m);
        }
    }

    #[test]
    fn rng_is_deterministic_and_uniform() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        assert_eq!((0..50).map(|_| a.next_u64()).collect::<Vec<_>>(), (0..50).map(|_| b.next_u64()).collect::<Vec<_>>());
        let mut r = Rng::new(1);
        let mean: f32 = (0..20000).map(|_| r.f()).sum::<f32>() / 20000.0;
        assert!((mean - 0.5).abs() < 0.01, "{mean}");
        assert!((0..2000).all(|_| (0.0..1.0).contains(&r.f())));
    }

    #[test]
    fn zone_schedule_nests_shrinks_and_ends_at_the_total() {
        let t = flat();
        let mut rng = Rng::new(3);
        let z = Zone::generate(&arena(), &mut rng, &t);
        for i in 0..PHASES.len() {
            let (a, b) = (z.circles[i], z.circles[i + 1]);
            assert!(b.r <= a.r + 1e-3, "phase {i} grows");
            // Nested: the next circle lies fully inside the previous one.
            assert!((a.x - b.x).hypot(a.z - b.z) + b.r <= a.r + 1.0, "phase {i} not nested");
        }
        let total = Zone::total_secs();
        assert!((240.0..330.0).contains(&total), "match length {total}");
        // Radius is non-increasing over the whole timeline.
        let mut last = f32::MAX;
        let mut s = 0.0;
        while s < total + 5.0 {
            let v = z.at(s);
            assert!(v.cur.r <= last + 1e-3);
            last = v.cur.r;
            s += 0.5;
        }
        assert_eq!(z.at(0.0).stage, Stage::Landing);
        assert_eq!(z.at(0.0).dps, 0.0);
        assert_eq!(z.at(DROP_SECS + 1.0).stage, Stage::Hold);
        assert_eq!(z.at(DROP_SECS + 1.0).phase, 1);
        let shrink_mid = DROP_SECS + PHASES[0].hold + PHASES[0].shrink / 2.0;
        let v = z.at(shrink_mid);
        assert_eq!(v.stage, Stage::Shrink);
        assert!(v.cur.r < z.circles[0].r && v.cur.r > z.circles[1].r);
        assert_eq!(z.at(total + 1.0).stage, Stage::Final);
        assert!(z.at(total + 1.0).cur.r < 60.0);
    }

    #[test]
    fn blight_damage_scales_with_phase_and_only_hits_outside() {
        let (mut l, t) = new_match(5, 6);
        l.fast_forward(Zone::phase_start(2) + 1.0, &t);
        let c = l.zone.at(l.secs()).cur;
        let inside = [c.x, 0.0, c.z];
        let outside = [c.x + c.r + 500.0, 0.0, c.z];
        l.fighters[0].pos = inside;
        l.fighters[1].pos = outside;
        l.freeze_bots = true;
        let hp0 = (l.fighters[0].hp, l.fighters[1].hp);
        run(&mut l, &t, 30);
        assert_eq!(l.fighters[0].hp, hp0.0, "inside is safe");
        let lost = hp0.1 - l.fighters[1].hp;
        let dps = PHASES[2].dps;
        assert!((lost - dps).abs() < 0.2 * dps + 0.1, "lost {lost}, expected about {dps}");
        // A later phase hurts more.
        let (mut l2, t2) = new_match(5, 6);
        l2.fast_forward(Zone::phase_start(5) + 1.0, &t2);
        l2.freeze_bots = true;
        let c2 = l2.zone.at(l2.secs()).cur;
        l2.fighters[1].pos = [c2.x + c2.r + 300.0, 0.0, c2.z];
        l2.fighters[0].pos = [c2.x, 0.0, c2.z];
        run(&mut l2, &t2, 30);
        assert!(100.0 - l2.fighters[1].hp > lost * 3.0);
    }

    #[test]
    fn loot_tables_respect_rarity_and_source() {
        let mut r = Rng::new(11);
        let mut counts = std::collections::HashMap::new();
        let mut rar = [[0u32; 3]; 3];
        for (si, src) in [Source::Floor, Source::Chest, Source::Airdrop].into_iter().enumerate() {
            for _ in 0..6000 {
                let it = roll_item(&mut r, src);
                *counts.entry((si, it.kind)).or_insert(0u32) += 1;
                rar[si][it.rarity.idx()] += 1;
                assert!(it.rarity >= it.kind.min_rarity());
                assert_eq!(it.ammo, it.kind.ammo(it.rarity));
            }
        }
        // Floor loot is mostly common, chests mostly rare, airdrops never common.
        assert!(rar[0][0] > rar[0][1] * 2 && rar[0][1] > rar[0][2]);
        assert!(rar[1][1] > rar[1][0] && rar[1][1] > rar[1][2]);
        assert_eq!(rar[2][0], 0);
        assert!(rar[2][2] > rar[2][1]);
        // Airdrops carry no cheap consumables; the star burst is far rarer on the floor.
        assert!(counts.get(&(2, Kind::Heart)).is_none() && counts.get(&(2, Kind::Cap)).is_none());
        assert!(counts[&(0, Kind::Shell)] > counts[&(0, Kind::StarBurst)] * 4);
        assert!(counts[&(2, Kind::StarBurst)] > counts[&(0, Kind::StarBurst)]);
        // The min-rarity kinds never appear common.
        let it = Item::new(Kind::StarBurst, Rarity::Common);
        assert_eq!(it.rarity, Rarity::Rare);
    }

    #[test]
    fn inventory_fills_merges_and_swaps_by_rarity() {
        let (mut l, _t) = new_match(1, 2);
        assert!(l.give(0, Item::new(Kind::Shell, Rarity::Common)));
        assert!(l.give(0, Item::new(Kind::Flower, Rarity::Common)));
        // Same kind and rarity tops up instead of taking a slot.
        let before = l.fighters[0].slots[0].unwrap().ammo;
        assert!(l.give(0, Item::new(Kind::Shell, Rarity::Common)));
        assert_eq!(l.fighters[0].slots[0].unwrap().ammo, before + Kind::Shell.ammo(Rarity::Common));
        // Full of commons: a common bomb does not fit, an epic one replaces the worst.
        assert!(!l.give(0, Item::new(Kind::Bomb, Rarity::Common)));
        assert!(l.give(0, Item::new(Kind::Bomb, Rarity::Epic)));
        assert!(l.fighters[0].slots.iter().flatten().any(|i| i.kind == Kind::Bomb));
        // One consumable slot; a better one swaps, a worse one is refused.
        assert!(l.give(0, Item::new(Kind::Heart, Rarity::Rare)));
        assert!(!l.give(0, Item::new(Kind::Cap, Rarity::Common)));
        assert!(l.give(0, Item::new(Kind::Cap, Rarity::Epic)));
        assert_eq!(l.fighters[0].cons.unwrap().kind, Kind::Cap);
    }

    #[test]
    fn item_use_spends_ammo_and_applies_effects() {
        let (mut l, t) = new_match(2, 2);
        l.fast_forward(40.0, &t);
        l.freeze_bots = true;
        l.fighters[0].slots = [Some(Item::new(Kind::Shell, Rarity::Common)), None];
        let n = l.fighters[0].slots[0].unwrap().ammo;
        assert!(l.use_item(0, 0, 0.0));
        assert_eq!(l.fighters[0].slots[0].unwrap().ammo, n - 1);
        assert_eq!(l.shots.iter().filter(|s| s.alive).count(), 1);
        // Cooldown blocks an immediate second use; nothing in an empty slot.
        assert!(!l.use_item(0, 0, 0.0));
        assert!(!l.use_item(0, 1, 0.0));
        // Last charge empties the slot.
        l.fighters[0].slots[0] = Some(Item { kind: Kind::Flower, rarity: Rarity::Common, ammo: 1 });
        l.fighters[0].cd = [0; 3];
        assert!(l.use_item(0, 0, 0.0));
        assert!(l.fighters[0].slots[0].is_none());
        // Consumables.
        l.fighters[0].hp = 40.0;
        l.fighters[0].cons = Some(Item::new(Kind::Heart, Rarity::Rare));
        assert!(l.use_item(0, 2, 0.0));
        assert!((l.fighters[0].hp - 80.0).abs() < 1e-3);
        l.fighters[0].cons = Some(Item::new(Kind::Cap, Rarity::Rare));
        l.fighters[0].cd = [0; 3];
        l.use_item(0, 2, 0.0);
        assert_eq!(l.fighters[0].shield, 40.0);
    }

    #[test]
    fn damage_shield_iframes_and_knockback() {
        let (mut l, t) = new_match(4, 2);
        l.fast_forward(40.0, &t);
        l.freeze_bots = true;
        l.fighters[0].pos = [0.0, 0.0, 0.0];
        l.fighters[1].pos = [300.0, 0.0, 0.0];
        let d = l.hit(1, Some(0), 20.0, [1.0, 0.0], 10.0, 8.0, [0.0; 3]);
        assert_eq!(d, 20.0);
        assert_eq!(l.fighters[1].hp, 80.0);
        assert!(l.fighters[1].knocked && l.fighters[1].knock[0] == 10.0 && l.fighters[1].knock[1] == 8.0);
        // i-frames: an immediate second hit does nothing.
        assert_eq!(l.hit(1, Some(0), 20.0, [1.0, 0.0], 10.0, 8.0, [0.0; 3]), 0.0);
        for _ in 0..18 {
            l.fighters[1].iframes = l.fighters[1].iframes.saturating_sub(1);
        }
        // A shield soaks first.
        l.fighters[1].shield = 15.0;
        l.hit(1, Some(0), 20.0, [1.0, 0.0], 10.0, 8.0, [0.0; 3]);
        assert_eq!(l.fighters[1].shield, 0.0);
        assert_eq!(l.fighters[1].hp, 75.0);
        // Star power shrugs hits off; a dropping Mario is untouchable.
        l.fighters[1].iframes = 0;
        l.fighters[1].star_t = 10;
        assert_eq!(l.hit(1, Some(0), 20.0, [1.0, 0.0], 10.0, 8.0, [0.0; 3]), 0.0);
        l.fighters[1].star_t = 0;
        l.fighters[1].mode = Mode::Drop;
        assert_eq!(l.hit(1, Some(0), 20.0, [1.0, 0.0], 10.0, 8.0, [0.0; 3]), 0.0);
    }

    #[test]
    fn melee_punch_dive_and_pound_hit_with_the_right_shape() {
        let (mut l, t) = new_match(6, 3);
        l.fast_forward(40.0, &t);
        l.freeze_bots = true;
        let setup = |l: &mut Lss| {
            for f in l.fighters.iter_mut() {
                f.pos = [4000.0 + f.id as f32 * 900.0, 0.0, 4000.0];
                f.iframes = 0;
                f.knock = [0.0; 3];
                f.vel = [0.0; 3];
                f.hp = 100.0;
                f.grounded = true;
                f.mv = Mv::Idle;
            }
            l.fighters[0].pos = [0.0, 0.0, 0.0];
            l.fighters[1].pos = [120.0, 0.0, 0.0];
            l.fighters[0].yaw = PI / 2.0; // facing +x
        };
        let step = |l: &mut Lss, t: &Flat, mv: Mv| {
            let m = vec![
                Some(Motion { pos: l.fighters[0].pos, vel: [0.0; 3], yaw: l.fighters[0].yaw, grounded: mv != Mv::Dive, mv }),
                None,
                None,
                None,
            ];
            l.fighters[0].sim = true;
            l.tick(t, &m);
        };
        // Punch in front lands; behind does not.
        setup(&mut l);
        step(&mut l, &t, Mv::Punch);
        assert_eq!(l.fighters[1].hp, 93.0);
        setup(&mut l);
        l.fighters[0].yaw = -PI / 2.0;
        step(&mut l, &t, Mv::Punch);
        assert_eq!(l.fighters[1].hp, 100.0, "punching away from the target");
        // Dive hits at range and hurts more than a punch.
        setup(&mut l);
        l.fighters[1].pos = [140.0, 0.0, 0.0];
        step(&mut l, &t, Mv::Dive);
        assert!(l.fighters[1].hp <= 85.5);
        // Ground pound shockwave: grounded targets in range only.
        setup(&mut l);
        l.fighters[1].pos = [200.0, 0.0, 0.0];
        l.fighters[2].pos = [150.0, 0.0, 0.0];
        l.fighters[2].grounded = false; // jumped over the wave
        l.fighters[2].pos[1] = 200.0;
        l.fighters[2].vel[1] = 30.0;
        l.fighters[0].mv = Mv::Pound;
        step(&mut l, &t, Mv::PoundLand);
        assert!(l.fighters[1].hp < 100.0);
        assert_eq!(l.fighters[2].hp, 100.0);
        // A charged quake reaches much further.
        setup(&mut l);
        l.fighters[1].pos = [450.0, 0.0, 0.0];
        step(&mut l, &t, Mv::Idle);
        step(&mut l, &t, Mv::PoundLand);
        assert_eq!(l.fighters[1].hp, 100.0, "uncharged pound is short ranged");
        setup(&mut l);
        l.fighters[0].quake = 1;
        l.fighters[1].pos = [450.0, 0.0, 0.0];
        step(&mut l, &t, Mv::Idle);
        step(&mut l, &t, Mv::PoundLand);
        assert!(l.fighters[1].hp < 100.0 && l.fighters[0].quake == 0);
    }

    #[test]
    fn shell_bounces_and_hits_bomb_explodes_fire_hurts() {
        let (mut l, t) = new_match(8, 3);
        l.fast_forward(40.0, &t);
        l.freeze_bots = true;
        for f in l.fighters.iter_mut() {
            f.pos = [4500.0 + f.id as f32 * 500.0, 0.0, 4500.0];
            f.iframes = 0;
        }
        l.fighters[0].pos = [0.0, 0.0, 0.0];
        l.fighters[1].pos = [700.0, 0.0, 0.0];
        l.fighters[0].slots = [Some(Item::new(Kind::Shell, Rarity::Common)), Some(Item::new(Kind::Flower, Rarity::Common))];
        l.use_item(0, 0, PI / 2.0);
        let mut min_y = f32::MAX;
        let mut max_y: f32 = 0.0;
        for _ in 0..40 {
            run(&mut l, &t, 1);
            if let Some(s) = l.shots.iter().find(|s| s.alive) {
                min_y = min_y.min(s.pos[1]);
                max_y = max_y.max(s.pos[1]);
            }
        }
        assert!(l.fighters[1].hp <= 84.5, "shell should have landed: {}", l.fighters[1].hp);
        assert!(max_y > min_y + 5.0 || l.fighters[1].hp < 100.0);
        // Bomb: explodes after its fuse near the target, damaging it and knocking it away.
        l.fighters[1].hp = 100.0;
        l.fighters[1].iframes = 0;
        l.fighters[0].slots[0] = Some(Item::new(Kind::Bomb, Rarity::Common));
        l.fighters[0].cd = [0; 3];
        l.fighters[1].pos = [600.0, 0.0, 0.0];
        l.use_item(0, 0, PI / 2.0);
        let mut boom = false;
        for _ in 0..80 {
            run(&mut l, &t, 1);
            boom |= l.events.iter().any(|e| matches!(e, Event::Explosion { .. }));
        }
        assert!(boom, "the bomb must explode");
        assert!(l.fighters[1].hp < 100.0);
        // Fire: a hit does less than the shell.
        l.fighters[1].hp = 100.0;
        l.fighters[1].iframes = 0;
        l.fighters[1].pos = [500.0, 0.0, 0.0];
        l.fighters[0].slots[1] = Some(Item::new(Kind::Flower, Rarity::Common));
        l.fighters[0].cd = [0; 3];
        l.use_item(0, 1, PI / 2.0);
        run(&mut l, &t, 40);
        assert!(l.fighters[1].hp < 100.0 && l.fighters[1].hp >= 90.0);
    }

    #[test]
    fn knockoffs_into_the_void_and_hazards_are_eliminations_with_credit() {
        let (mut l, t) = new_match(9, 3);
        l.fast_forward(40.0, &t);
        l.freeze_bots = true;
        for f in l.fighters.iter_mut() {
            f.pos = [100.0 * f.id as f32, 0.0, 0.0];
        }
        // Player hits bot 1, then bot 1 falls off the world within the credit window.
        l.fighters[1].pos = [0.0, 0.0, 300.0];
        l.hit(1, Some(0), 5.0, [0.0, 1.0], 20.0, 10.0, [0.0; 3]);
        l.fighters[1].pos = [0.0, -700.0, 3000.0];
        run(&mut l, &t, 1);
        assert!(!l.fighters[1].alive);
        let elim = l.events.iter().find_map(|e| if let Event::Elim { victim, killer, cause, place } = e { Some((*victim, *killer, *cause, *place)) } else { None });
        assert_eq!(elim, Some((1, Some(0), Cause::Fall, 4)));
        assert_eq!(l.fighters[0].kills, 1);
        // Water counts: a grounded Mario standing in the hazard ring is out.
        let h = Flat { edge: 6300.0, hazard: Some((1000.0, 2000.0)) };
        l.fighters[2].pos = [1500.0, 0.0, 0.0];
        l.fighters[2].grounded = true;
        l.fighters[2].last_hit = None;
        l.tick(&h, &vec![None; l.fighters.len()]);
        assert!(!l.fighters[2].alive);
        assert!(l.events.iter().any(|e| matches!(e, Event::Elim { victim: 2, killer: None, cause: Cause::Fall, .. })));
        // Too late a hit earns no credit.
        let (mut l, t) = new_match(9, 3);
        l.fast_forward(40.0, &t);
        l.freeze_bots = true;
        l.fighters[1].last_hit = Some((0, l.tick.saturating_sub(10 * TPS)));
        l.fighters[1].pos = [0.0, -900.0, 0.0];
        run(&mut l, &t, 1);
        assert!(l.events.iter().any(|e| matches!(e, Event::Elim { victim: 1, killer: None, .. })));
    }

    #[test]
    fn placements_are_unique_and_the_last_one_standing_wins() {
        let (mut l, t) = new_match(10, 5);
        l.fast_forward(40.0, &t);
        l.freeze_bots = true;
        for id in [3usize, 5, 1] {
            l.eliminate(id, Cause::Fight);
        }
        assert_eq!((l.fighters[3].place, l.fighters[5].place, l.fighters[1].place), (6, 5, 4));
        assert!(!l.over && l.alive == 3);
        l.eliminate(0, Cause::Blight);
        assert_eq!(l.player_place, 3);
        l.eliminate(2, Cause::Fight);
        assert!(l.over);
        assert_eq!(l.winner, Some(4));
        assert_eq!(l.fighters[2].place, 2);
        assert_eq!(l.fighters[4].place, 1);
        let mut places: Vec<u32> = l.fighters.iter().map(|f| f.place).collect();
        places.sort();
        assert_eq!(places, vec![1, 2, 3, 4, 5, 6]);
        // The player winning is placement 1.
        let (mut l, t) = new_match(10, 2);
        l.fast_forward(40.0, &t);
        l.eliminate(1, Cause::Fight);
        l.eliminate(2, Cause::Fight);
        assert!(l.over && l.winner == Some(0) && l.player_place == 1);
    }

    #[test]
    fn bots_drop_land_loot_and_head_into_the_zone() {
        let (mut l, t) = new_match(12, 12);
        // Everyone starts high and lands by itself.
        assert!(l.fighters.iter().all(|f| f.mode == Mode::Drop && f.pos[1] > DROP_HEIGHT - 1.0));
        l.fighters[0].sim = false;
        // The player is not a bot: park him on the ground so he does not distort the test.
        l.fighters[0].mode = Mode::Ground;
        l.fighters[0].pos = [0.0, 0.0, 0.0];
        l.fighters[0].grounded = true;
        run(&mut l, &t, 20 * TPS);
        let bots_landed = l.fighters.iter().filter(|f| !f.is_player && f.alive && f.mode == Mode::Ground).count();
        assert!(bots_landed >= 10, "{bots_landed}");
        // Some of them picked up weapons.
        run(&mut l, &t, 40 * TPS);
        let armed = l.fighters.iter().filter(|f| !f.is_player && f.alive && f.weapon_count() > 0).count();
        assert!(armed >= 2, "armed {armed}");
        // Late in the match living bots are inside (or hurrying into) the circle.
        l.fast_forward(Zone::phase_start(2) + 20.0, &t);
        run(&mut l, &t, 25 * TPS);
        let zv = l.zv;
        let inside = l.fighters.iter().filter(|f| f.alive && !f.is_player).filter(|f| zv.cur.contains(f.pos)).count();
        let alive = l.fighters.iter().filter(|f| f.alive && !f.is_player).count();
        assert!(inside * 10 >= alive * 7, "{inside} of {alive} inside");
    }

    #[test]
    fn bot_state_transitions_flee_engage_rotate() {
        let (mut l, t) = new_match(14, 3);
        l.fast_forward(Zone::phase_start(0) + 5.0, &t);
        let zv = l.zv;
        for f in l.fighters.iter_mut() {
            f.pos = [zv.cur.x + 40.0 * f.id as f32, 0.0, zv.cur.z + 3000.0];
            f.brain.think_at = 0;
        }
        l.fighters[1].brain.aggression = 0.95;
        l.fighters[1].pos = [zv.cur.x, 0.0, zv.cur.z];
        l.fighters[2].pos = [zv.cur.x + 250.0, 0.0, zv.cur.z];
        l.fighters[3].pos = [zv.cur.x + 3500.0, 0.0, zv.cur.z - 3000.0];
        l.fighters[0].pos = [zv.cur.x - 5000.0, 0.0, zv.cur.z];
        l.fighters[0].iframes = 999;
        l.fighters[1].slots[0] = Some(Item::new(Kind::Shell, Rarity::Common));
        // An aggressive armed bot with an enemy next door engages.
        l.think(1, &t);
        assert_eq!(l.fighters[1].brain.state, BotState::Engage);
        assert_eq!(l.fighters[1].brain.target, Some(2));
        // Low health with the enemy close: it flees.
        l.fighters[1].hp = 15.0;
        l.fighters[1].cons = None;
        l.fighters[1].brain.think_at = 0;
        l.think(1, &t);
        assert_eq!(l.fighters[1].brain.state, BotState::Flee);
        // Outside the circle with nobody around: rotate toward the next one.
        l.fighters[3].brain.think_at = 0;
        l.fighters[3].pos = [zv.cur.x + zv.cur.r + 800.0, 0.0, zv.cur.z];
        l.fighters[1].pos = [zv.cur.x - 6000.0, 0.0, zv.cur.z - 6000.0];
        l.fighters[2].pos = [zv.cur.x - 6000.0, 0.0, zv.cur.z + 5000.0];
        l.think(3, &t);
        assert_eq!(l.fighters[3].brain.state, BotState::Rotate);
        let g = l.fighters[3].brain.goal;
        assert!(dist2(g, [zv.next.x, 0.0, zv.next.z]) <= zv.next.r + 1.0);
    }

    #[test]
    fn kinematic_bots_follow_the_ground_and_stop_at_edges() {
        let (mut l, t) = new_match(15, 2);
        l.fast_forward(40.0, &t);
        l.freeze_bots = true;
        let c = l.zv.cur;
        let b = &mut l.fighters[1];
        b.pos = [c.x, 0.0, c.z];
        b.grounded = true;
        b.intent = Intent { heading: PI / 2.0, mag: 1.0, ..Default::default() };
        for _ in 0..30 {
            l.freeze_bots = false;
            l.fighters[1].intent = Intent { heading: PI / 2.0, mag: 1.0, ..Default::default() };
            l.fighters[1].brain.think_at = u32::MAX;
            l.kin_step(1, &t);
        }
        let x = l.fighters[1].pos[0];
        assert!(x - c.x > 500.0, "ran {}", x - c.x);
        // Running at the island's edge it stops instead of walking into the void.
        l.fighters[1].pos = [t.edge - 150.0, 0.0, 0.0];
        for _ in 0..30 {
            l.fighters[1].intent = Intent { heading: PI / 2.0, mag: 1.0, ..Default::default() };
            l.kin_step(1, &t);
        }
        assert!(l.fighters[1].pos[0] <= t.edge + 1.0 && l.fighters[1].pos[1] > -1.0);
        // Jumping leaves the ground and comes back.
        l.fighters[1].pos = [0.0, 0.0, 0.0];
        l.fighters[1].grounded = true;
        l.fighters[1].intent = Intent { jump: true, ..Default::default() };
        l.kin_step(1, &t);
        assert!(l.fighters[1].pos[1] > 10.0 && !l.fighters[1].grounded);
        l.fighters[1].intent = Intent::default();
        for _ in 0..40 {
            l.kin_step(1, &t);
        }
        assert!(l.fighters[1].grounded && l.fighters[1].pos[1].abs() < 1.0);
    }

    #[test]
    fn sim_selection_picks_the_nearest_bots_with_a_cap() {
        let (mut l, t) = new_match(16, 12);
        l.fast_forward(40.0, &t);
        for (i, f) in l.fighters.iter_mut().enumerate() {
            f.pos = [i as f32 * 400.0, 0.0, 0.0];
            f.grounded = true;
        }
        l.choose_sim([0.0, 0.0, 0.0], 3000.0, 4);
        let sim: Vec<usize> = l.fighters.iter().filter(|f| f.sim).map(|f| f.id).collect();
        assert_eq!(sim, vec![1, 2, 3, 4]);
        assert!(l.fighters.iter().filter(|f| f.sim_started).count() == 4);
        // Next tick: unchanged, nothing newly started.
        l.choose_sim([0.0, 0.0, 0.0], 3000.0, 4);
        assert!(l.fighters.iter().all(|f| !f.sim_started));
        // Dropping bots are never sim until close to the ground.
        l.fighters[1].mode = Mode::Drop;
        l.fighters[1].pos[1] = DROP_HEIGHT;
        l.choose_sim([0.0, 0.0, 0.0], 3000.0, 4);
        assert!(!l.fighters[1].sim);
    }

    #[test]
    fn a_whole_match_is_deterministic_and_finishes_with_a_winner() {
        let play = |seed: u64| {
            let (mut l, t) = new_match(seed, 23);
            // The player stands in the middle and does nothing; the bots do everything.
            l.fighters[0].mode = Mode::Ground;
            l.fighters[0].pos = [l.zv.cur.x, 0.0, l.zv.cur.z];
            l.fighters[0].grounded = true;
            let m = vec![None; l.fighters.len()];
            let mut guard = 0;
            while !l.over && guard < 20 * 60 * TPS {
                l.tick(&t, &m);
                guard += 1;
            }
            let digest: Vec<(u32, u32)> = l.fighters.iter().map(|f| (f.place, f.kills)).collect();
            (l.over, l.winner, l.tick, digest)
        };
        let a = play(77);
        let b = play(77);
        assert_eq!(a, b, "same seed, same match");
        assert!(a.0 && a.1.is_some(), "match must end");
        assert!(a.2 < (Zone::total_secs() + 150.0) as u32 * TPS, "match took {} ticks", a.2);
        let c = play(78);
        assert_ne!(a.3, c.3, "different seeds differ");
        // Placements form a permutation.
        let mut p: Vec<u32> = a.3.iter().map(|x| x.0).collect();
        p.sort();
        assert_eq!(p, (1..=24).collect::<Vec<u32>>());
        // Somebody actually got knocked out by somebody.
        assert!(a.3.iter().map(|x| x.1).sum::<u32>() > 0, "bots never fought");
    }

    /// Run a match to its end with fighter 0 on the bot brain. Returns the finished match.
    fn autopilot_match(seed: u64, bots: usize) -> Lss {
        let (mut l, t) = new_match(seed, bots);
        l.autopilot = true;
        l.fast_forward(12.0, &t);
        let m = vec![None; l.fighters.len()];
        let mut guard = 0;
        while !l.over && guard < 20 * 60 * TPS {
            l.tick(&t, &m);
            guard += 1;
        }
        l
    }

    /// The goal is reachable: with the player fighting like a bot, matches end and the player
    /// takes first place in some of them (a small field, so luck is not the only way in).
    #[test]
    fn a_player_on_the_bot_brain_can_win_a_match() {
        let mut wins = 0;
        let mut placed = vec![];
        for seed in 1..=24u64 {
            let l = autopilot_match(seed, 5);
            assert!(l.over && l.winner.is_some(), "seed {seed}: match never ended");
            assert!(l.player_place >= 1 && l.player_place <= 6, "seed {seed}: place {}", l.player_place);
            if l.winner == Some(0) {
                assert_eq!(l.player_place, 1);
                assert!(l.events.iter().any(|e| matches!(e, Event::Winner { who: 0 })) || l.fighters[0].alive);
                wins += 1;
            }
            placed.push(l.player_place);
        }
        eprintln!("autopilot wins {wins}/24, places {placed:?}");
        assert!(wins >= 1, "the player never wins: {placed:?}");
    }

    /// A player lost to the Blight (stands outside the safe zone) is eliminated, takes a place
    /// below the survivors', and the match still plays out to a single winner.
    #[test]
    fn losing_to_the_blight_still_ends_in_a_finished_match() {
        for seed in [31u64, 32, 33] {
            let (mut l, t) = new_match(seed, 11);
            l.fast_forward(40.0, &t);
            // Park the idle player on the rim, far from where the zone will end up.
            let c = l.zone.at(Zone::total_secs()).cur;
            let a = (c.z - l.arena.cz).atan2(c.x - l.arena.cx) + std::f32::consts::PI;
            l.fighters[0].mode = Mode::Ground;
            l.fighters[0].grounded = true;
            l.fighters[0].pos = [l.arena.cx + a.cos() * 5900.0, 0.0, l.arena.cz + a.sin() * 5900.0];
            let m = vec![None; l.fighters.len()];
            let mut guard = 0;
            let mut out_at = None;
            while !l.over && guard < 20 * 60 * TPS {
                l.tick(&t, &m);
                if out_at.is_none() && !l.fighters[0].alive {
                    out_at = Some(l.tick);
                }
                guard += 1;
            }
            assert!(l.over && l.winner.is_some_and(|w| w != 0), "seed {seed}: over {} winner {:?}", l.over, l.winner);
            assert!(out_at.is_some() && l.player_place > 1, "seed {seed}: the idle player on the rim should be eliminated");
            assert!(l.fighters.iter().filter(|f| f.alive).count() <= 1);
        }
    }

    /// Pacing of a full desktop field (39 bots, the player on the bot brain): the field must thin out from the
    /// first minute on (a quiet first two minutes is what made the mode feel empty) and the match must fit in
    /// about five minutes.
    #[test]
    fn a_full_field_thins_out_steadily() {
        let mut curves = vec![];
        for seed in [3u64, 4, 5] {
            let (mut l, t) = new_match(seed, 39);
            l.autopilot = true;
            let m = vec![None; l.fighters.len()];
            let mut marks = vec![];
            while !l.over && l.tick < 20 * 60 * TPS {
                l.tick(&t, &m);
                if l.tick % (30 * TPS) == 0 {
                    marks.push(l.alive);
                }
            }
            eprintln!("seed {seed}: alive every 30 s {marks:?}, over at {:.0}s", l.tick as f32 / TPS as f32);
            curves.push((marks, l.tick));
        }
        for (marks, end) in &curves {
            assert!(marks.get(1).copied().unwrap_or(0) <= 34, "after a minute at least 6 of 40 are out: {marks:?}");
            assert!(marks.get(3).copied().unwrap_or(0) <= 24, "after two minutes the field has shrunk by 40%: {marks:?}");
            assert!(*end < 330 * TPS, "the match fits in about five minutes: {end}");
        }
    }

    #[test]
    fn ticking_forty_bots_is_cheap() {
        let (mut l, t) = new_match(21, 39);
        l.god = true;
        l.fast_forward(60.0, &t);
        let m = vec![None; l.fighters.len()];
        let start = std::time::Instant::now();
        let n = 600;
        for _ in 0..n {
            l.tick(&t, &m);
            l.choose_sim([0.0, 0.0, 0.0], 3000.0, 12);
        }
        let per = start.elapsed().as_secs_f64() / n as f64;
        // Budget on a phone is a few hundred microseconds for the whole core; allow headroom
        // for unoptimised test builds.
        let limit = if cfg!(debug_assertions) { 3e-3 } else { 4e-4 };
        assert!(per < limit, "{:.1} us per tick", per * 1e6);
    }

    #[test]
    fn airdrops_fall_land_and_spill_loot() {
        let (mut l, t) = new_match(22, 4);
        l.fighters[0].mode = Mode::Ground;
        l.fighters[0].grounded = true;
        l.fast_forward(Zone::phase_start(1) + 1.0, &t);
        l.freeze_bots = true;
        let m = vec![None; l.fighters.len()];
        let before = l.pickups.iter().filter(|p| matches!(p.kind, PickKind::Crate { .. })).count();
        let mut called = false;
        for _ in 0..4 {
            l.tick(&t, &m);
            called |= l.events.iter().any(|e| matches!(e, Event::AirdropCall { .. }));
        }
        assert!(called, "a crate is announced when a phase starts");
        assert!(l.pickups.iter().filter(|p| matches!(p.kind, PickKind::Crate { .. })).count() > before);
        let mut landed = false;
        for _ in 0..260 {
            l.tick(&t, &m);
            landed |= l.events.iter().any(|e| matches!(e, Event::AirdropLand { .. }));
        }
        assert!(landed);
        // Opening it spills items.
        let crate_pos = l.pickups.iter().find(|p| matches!(p.kind, PickKind::Crate { .. }) && p.alive).unwrap().pos;
        let items_before = l.pickups.iter().filter(|p| p.alive && !p.is_container()).count();
        l.fighters[0].pos = crate_pos;
        l.tick(&t, &m);
        assert!(l.pickups.iter().filter(|p| p.alive && !p.is_container()).count() >= items_before + 3);
    }

    /// Walk the player's physics body (the host's Motion, as the game hands it in) from `from` to
    /// `to` at running speed; returns the events of every tick.
    fn walk_player(l: &mut Lss, t: &Flat, from: [f32; 3], to: [f32; 3]) -> Vec<Event> {
        let mut seen = vec![];
        let steps = (dist2(from, to) / 24.0).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let a = k as f32 / steps as f32;
            let pos = [from[0] + (to[0] - from[0]) * a, from[1] + (to[1] - from[1]) * a, from[2] + (to[2] - from[2]) * a];
            let mut m = vec![None; l.fighters.len()];
            m[0] = Some(Motion { pos, vel: [0.0; 3], yaw: heading_to(from, to), grounded: true, mv: Mv::Move });
            l.tick(t, &m);
            seen.extend(l.events.iter().copied());
        }
        seen
    }

    #[test]
    fn player_collects_every_pickup_by_walking_into_it() {
        let (mut l, t) = new_match(31, 3);
        // The bots stand frozen far away so they cannot interfere.
        l.freeze_bots = true;
        for (k, f) in l.fighters.iter_mut().enumerate().skip(1) {
            f.mode = Mode::Ground;
            f.pos = [-3000.0 + k as f32 * 100.0, 0.0, -3000.0];
        }
        for p in l.pickups.iter_mut() {
            p.alive = false;
        }
        // The player lands through his physics body only (the core never moves him itself; nobody
        // flags him `sim`): the landing motion must put him on the ground where Mario stands.
        assert!(!l.fighters[0].sim && l.fighters[0].mode == Mode::Drop);
        let air = Motion { pos: [0.0, 300.0, 0.0], vel: [0.0, -15.0, 0.0], yaw: 0.0, grounded: false, mv: Mv::Air };
        let mut m = vec![None; l.fighters.len()];
        m[0] = Some(air);
        l.tick(&t, &m);
        assert_eq!(l.fighters[0].pos, air.pos, "the core follows the player's Mario");
        m[0] = Some(Motion { pos: [0.0, 0.0, 0.0], grounded: true, mv: Mv::Idle, ..air });
        l.tick(&t, &m);
        assert_eq!(l.fighters[0].mode, Mode::Ground);

        let mut here = [0.0f32, 0.0, 0.0];
        for (n, &kind) in ALL_KINDS.iter().enumerate() {
            let f = &mut l.fighters[0];
            f.slots = [None; 2];
            f.cons = None;
            f.cd = [0; 3];
            f.hp = 30.0;
            let spot = [here[0] + 400.0, 0.0, here[2] + if n % 2 == 0 { 150.0 } else { -150.0 }];
            let pi = l.add_pickup(spot, Item::new(kind, Rarity::Rare), PickKind::Item).unwrap();
            l.pickups[pi].age = 20;
            let events = walk_player(&mut l, &t, here, spot);
            here = spot;
            assert!(!l.pickups[pi].alive, "{kind:?} was not picked up: f0 {:?} alive {} mode {:?} over {} pk {:?}", l.fighters[0].pos, l.fighters[0].alive, l.fighters[0].mode, l.over, l.pickups[pi]);
            assert!(events.iter().any(|e| matches!(e, Event::Pickup { who: 0, kind: k } if *k == kind)), "{kind:?}: no pickup event");
            let f = &l.fighters[0];
            let slot = if kind.is_weapon() {
                assert_eq!(f.slots[0].map(|i| i.kind), Some(kind));
                0
            } else {
                assert_eq!(f.cons.map(|i| i.kind), Some(kind));
                2
            };
            // And it does what it says once used.
            let shots = l.shots.iter().filter(|s| s.alive).count();
            assert!(l.use_item(0, slot, 0.0), "{kind:?} could not be used");
            let f = &l.fighters[0];
            match kind {
                Kind::Shell | Kind::Bomb | Kind::Flower => assert!(l.shots.iter().filter(|s| s.alive).count() > shots, "{kind:?}"),
                Kind::StarBurst => assert!(f.star_t > 0),
                Kind::Quake => assert!(f.quake > 0),
                Kind::Cap => assert!(f.shield > 0.0),
                Kind::Boots => assert!(f.boost_t > 0),
                Kind::Heart | Kind::OneUp => assert!(f.hp > 30.0, "{kind:?} hp {}", f.hp),
            }
        }

        // Containers: a chest and a landed airdrop crate open when walked into and spill loot.
        for kind in [PickKind::Chest { opened: false }, PickKind::Crate { drop_y: 0.0, opened: false }] {
            let spot = [here[0] + 450.0, 0.0, here[2]];
            let ci = l.add_pickup(spot, Item::new(Kind::Heart, Rarity::Common), kind).unwrap();
            let items = l.pickups.iter().filter(|p| p.alive && !p.is_container()).count();
            let events = walk_player(&mut l, &t, here, [spot[0] - 60.0, 0.0, spot[2]]);
            here = [spot[0] - 60.0, 0.0, spot[2]];
            assert!(l.pickups[ci].opened(), "{kind:?} did not open");
            assert!(events.iter().any(|e| matches!(e, Event::ChestOpen { .. })));
            // Spilled items count even if the player then scooped one up (he is standing there).
            let picked = events.iter().filter(|e| matches!(e, Event::Pickup { who: 0, .. })).count();
            assert!(l.pickups.iter().filter(|p| p.alive && !p.is_container()).count() + picked >= items + 2, "{kind:?} spilled nothing");
        }
    }

    /// The research checklist for Last Star Standing (`MINIGAMES_RESEARCH.md` section 5), measured on a full desktop field
    /// (39 bots, the player on the bot brain): the player's first exchange of blows after landing, the longest stretch with
    /// no rival within view range after minute 1, the elimination curve, the airdrop interval and how close the final five
    /// stand. `-- --nocapture` prints the numbers `MINIGAMES_AUDIT_3.md` quotes.
    #[test]
    fn last_star_standing_pace_report() {
        const VIEW: f32 = 3000.0;
        let mut firsts = vec![];
        let mut worst_gaps = vec![];
        for seed in [3u64, 4, 5, 6, 7, 8] {
            let (mut l, t) = new_match(seed, 39);
            l.autopilot = true;
            let m = vec![None; l.fighters.len()];
            let (mut landed, mut first, mut gap, mut worst_gap) = (None::<u32>, None::<u32>, 0u32, 0u32);
            let (mut half_at, mut five_at, mut five_spread) = (None::<u32>, None::<u32>, 0.0f32);
            let mut drops = vec![];
            while !l.over && l.tick < 20 * 60 * TPS {
                l.tick(&t, &m);
                let me = &l.fighters[0];
                if landed.is_none() && me.mode == Mode::Ground {
                    landed = Some(l.tick);
                }
                for e in &l.events {
                    match *e {
                        Event::Hit { target, by, dmg } if dmg > 0.0 && (target == 0 || by == Some(0)) && first.is_none() => first = Some(l.tick),
                        Event::AirdropCall { .. } => drops.push(l.tick),
                        _ => {}
                    }
                }
                if me.alive && l.tick > 60 * TPS {
                    let near = l.fighters.iter().skip(1).any(|f| f.alive && (f.pos[0] - me.pos[0]).hypot(f.pos[2] - me.pos[2]) < VIEW);
                    gap = if near { 0 } else { gap + 1 };
                    worst_gap = worst_gap.max(gap);
                }
                if half_at.is_none() && l.alive <= 20 {
                    half_at = Some(l.tick);
                }
                if five_at.is_none() && l.alive <= 5 {
                    five_at = Some(l.tick);
                    let alive: Vec<[f32; 3]> = l.fighters.iter().filter(|f| f.alive).map(|f| f.pos).collect();
                    for a in &alive {
                        for b in &alive {
                            five_spread = five_spread.max((a[0] - b[0]).hypot(a[2] - b[2]));
                        }
                    }
                }
            }
            let s = |t: Option<u32>| t.map_or(-1.0, |t| t as f32 / TPS as f32);
            let fight = match (landed, first) {
                (Some(a), Some(b)) => (b.saturating_sub(a)) as f32 / TPS as f32,
                _ => -1.0,
            };
            let gaps: Vec<f32> = drops.windows(2).map(|w| (w[1] - w[0]) as f32 / TPS as f32).collect();
            eprintln!(
                "LSS seed {seed}: landed {:.0}s, first player fight {fight:.0}s after landing, worst no-rival-in-{VIEW} stretch {:.0}s, half the field out {:.0}s, final five {:.0}s (spread {five_spread:.0}), airdrops at {:?} (gaps {gaps:?}), place {} of 40, over {:.0}s",
                s(landed),
                worst_gap as f32 / TPS as f32,
                s(half_at),
                s(five_at),
                drops.iter().map(|t| t / TPS).collect::<Vec<_>>(),
                l.player_place,
                l.tick as f32 / TPS as f32
            );
            firsts.push(fight);
            worst_gaps.push(worst_gap as f32 / TPS as f32);
            assert!(fight <= 45.0, "seed {seed}: the player's first fight came {fight:.0}s after landing");
            assert!(worst_gap <= 30 * TPS, "seed {seed}: {}s with no rival in view", worst_gap / TPS);
            assert!(gaps.iter().all(|g| *g <= 60.0), "seed {seed}: airdrops {gaps:?}");
        }
        eprintln!("LSS first fights {firsts:?}, worst gaps {worst_gaps:?}");
    }
}
