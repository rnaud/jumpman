//! Star Swarm, deterministic core: an arena-survivor ("horde survivor") ruleset with no engine
//! dependency. The player moves (Mario's real physics, driven elsewhere); everything else here is
//! automatic: stars fire at the nearest foes, foes swarm in from the arena edge, kills drop gems,
//! gems level you up, each level offers three upgrades, and the run ends when health runs out.
//!
//! All state lives in fixed-capacity slot arrays (`enemies`, `bolts`, ...) so a tick never
//! allocates and a render slot index is stable for the life of an entity. Positions are in SM64
//! units on the arena plane, *relative to the arena centre*; the Bevy layer adds the origin.
//! Time is counted in 30 Hz ticks and every random choice comes from the seeded [`Rng`], so a
//! run is reproducible from `(seed, inputs)`.

#[path = "swarm_foes.rs"]
mod foes;
#[path = "swarm_arms.rs"]
mod arms;
pub use arms::{BoltKind, Zone, ZoneKind, ZONE_SLOTS};
pub use foes::PORTED;
use std::f32::consts::{PI, TAU};
pub use super::swarm_meta::{Perks, Tier};

pub use super::swarm_world::{ChestSite, ChestTier};

pub const TICKS_PER_SEC: u32 = 30;
pub const ENEMY_SLOTS: usize = 320;
pub const BOLT_SLOTS: usize = 128;
pub const ORB_SLOTS: usize = 64;
pub const GEM_SLOTS: usize = 320;
pub const RING_SLOTS: usize = 16;
pub const ORBIT_MAX: usize = 6;
pub const BOOM_SLOTS: usize = 8;
/// Half the side of the square arena.
pub const ARENA_HALF: f32 = 3700.0;
pub const PLAYER_RADIUS: f32 = 45.0;
pub const IFRAMES: u16 = 24;
pub const BASE_HP: f32 = 100.0;
pub const BOSS_PERIOD_SECS: f32 = 120.0;
/// An elite (a big, golden mini-boss that always drops a chest) walks in this often, from `ELITE_FIRST_SECS`.
pub const ELITE_PERIOD_SECS: f32 = 50.0;
pub const ELITE_FIRST_SECS: f32 = 45.0;
/// The final boss, the Star Tyrant, arrives at 7:00. Killing it wins the run.
pub const FINAL_SECS: f32 = 420.0;
/// The dawn: the clock the HUD counts to. A Tyrant still standing then is enraged.
pub const WIN_SECS: f32 = 480.0;
const SURGE_PERIOD_TICKS: u32 = 40 * TICKS_PER_SEC;
const STAMPEDE_PERIOD_TICKS: u32 = 75 * TICKS_PER_SEC;
const BOLT_SPEED: f32 = 42.0;
const BOLT_RANGE: f32 = 1900.0;
const DASH_SPEED: f32 = 34.0;
const ORBIT_RADIUS: f32 = 340.0;
/// The storm ring closes to this share of the arena by the time the Tyrant arrives.
pub const RING_MIN_FRACTION: f32 = 0.58;
/// Spatial hash cell edge: the biggest foes (kings) are about one cell wide.
const GRID_CELL: f32 = 256.0;
/// Splitter pieces, summons and elites may take the live count this far over the cap.
pub const OVERCAP: usize = 16;

/// Live-enemy cap: phones get fewer than desktops.
pub const fn enemy_cap() -> usize {
    if cfg!(any(target_os = "ios", target_os = "android")) { 190 } else { 300 }
}

// ---------------------------------------------------------------------------------------------
// Seeded RNG (splitmix64)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u32
    }
    /// Uniform in `[0, 1)`.
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { ((self.next_u32() as u64 * n as u64) >> 32) as u32 }
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
}

// ---------------------------------------------------------------------------------------------
// Progression and wave scaling
// ---------------------------------------------------------------------------------------------

/// XP needed to go from `level` to `level + 1` (levels start at 1).
pub fn xp_to_next(level: u32) -> u32 {
    5 + 3 * level + level * level / 2
}

fn minutes(secs: f32) -> f32 {
    secs / 60.0
}

/// Enemy hit-point multiplier after `secs` seconds.
pub fn hp_mult(secs: f32) -> f32 {
    let m = minutes(secs);
    1.0 + 0.15 * m + 0.03 * m * m + 0.4 * dawn(secs)
}

/// Minutes past the Tyrant's arrival (the dark grows while the player explores instead of finishing him), capped at 3.
pub fn dawn(secs: f32) -> f32 {
    ((secs - FINAL_SECS) / 60.0).clamp(0.0, 3.0)
}

pub fn speed_mult(secs: f32) -> f32 {
    1.0 + (0.04 * minutes(secs)).min(0.4) + 0.05 * dawn(secs)
}

pub fn damage_mult(secs: f32) -> f32 {
    1.0 + 0.08 * minutes(secs)
}

/// How many enemies the wave director tries to keep alive (`pressure` is the tier's spawn multiplier).
pub fn target_live(secs: f32, cap: usize) -> usize {
    target_live_at(secs, cap, 1.0)
}

pub fn target_live_at(secs: f32, cap: usize, pressure: f32) -> usize {
    (((10.0 + 0.36 * secs + 0.0005 * secs * secs) * pressure * (1.0 + 0.25 * dawn(secs))) as usize).min(cap)
}

/// The storm ring's radius `secs` into the run, for an arena of `radius`: it closes steadily until the
/// Tyrant arrives, then holds.
pub fn ring_radius(radius: f32, secs: f32) -> f32 {
    let t = (secs / FINAL_SECS).clamp(0.0, 1.0);
    radius * (1.0 - (1.0 - RING_MIN_FRACTION) * t)
}

/// Storm damage per second outside the ring.
pub fn storm_dps(secs: f32) -> f32 {
    6.0 + 0.04 * secs
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum EnemyKind {
    /// Fast, fragile, comes in numbers.
    Skitter = 1,
    /// Slow and tanky.
    Brute = 2,
    /// Winds up, then lunges.
    Dasher = 3,
    /// Keeps its distance and shoots.
    Wisp = 4,
    /// Periodic mini-boss.
    King = 5,
    // The ported Ocarina of Time / Banjo-Kazooie species (`swarm_foes.rs`).
    Tektite = 6,
    Bat = 7,
    Wolfos = 8,
    Peahat = 9,
    SirSlush = 10,
    Bigbutt = 11,
    /// Runs up, flashes, and blows up: telegraphed area damage.
    Bomber = 12,
    /// Bursts into three skitters when it dies.
    Splitter = 13,
    /// Boss: a long wind-up and a very fast charge.
    Chomp = 14,
    /// The final boss.
    Tyrant = 15,
}

impl EnemyKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Skitter,
            2 => Self::Brute,
            3 => Self::Dasher,
            4 => Self::Wisp,
            5 => Self::King,
            12 => Self::Bomber,
            13 => Self::Splitter,
            14 => Self::Chomp,
            15 => Self::Tyrant,
            _ => return Self::from_u8_ported(v),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnemyStats {
    pub hp: f32,
    /// Units per tick.
    pub speed: f32,
    pub radius: f32,
    pub height: f32,
    pub contact: f32,
    pub xp: u16,
}

pub fn enemy_stats(kind: EnemyKind) -> EnemyStats {
    match kind {
        EnemyKind::Skitter => EnemyStats { hp: 14.0, speed: 15.0, radius: 55.0, height: 90.0, contact: 5.0, xp: 1 },
        EnemyKind::Brute => EnemyStats { hp: 70.0, speed: 8.0, radius: 100.0, height: 170.0, contact: 12.0, xp: 5 },
        EnemyKind::Dasher => EnemyStats { hp: 30.0, speed: 11.0, radius: 65.0, height: 110.0, contact: 10.0, xp: 3 },
        EnemyKind::Wisp => EnemyStats { hp: 26.0, speed: 10.0, radius: 60.0, height: 130.0, contact: 4.0, xp: 4 },
        EnemyKind::King => EnemyStats { hp: 900.0, speed: 7.0, radius: 190.0, height: 330.0, contact: 22.0, xp: 60 },
        EnemyKind::Bomber => EnemyStats { hp: 22.0, speed: 12.0, radius: 60.0, height: 100.0, contact: 6.0, xp: 4 },
        EnemyKind::Splitter => EnemyStats { hp: 60.0, speed: 8.5, radius: 90.0, height: 150.0, contact: 10.0, xp: 6 },
        EnemyKind::Chomp => EnemyStats { hp: 1400.0, speed: 8.0, radius: 160.0, height: 260.0, contact: 26.0, xp: 80 },
        EnemyKind::Tyrant => EnemyStats { hp: 9000.0, speed: 6.5, radius: 260.0, height: 420.0, contact: 30.0, xp: 400 },
        other => foes::stats(other),
    }
}

/// Spawn weights for [skitter, brute, dasher, wisp] at `secs`; new types phase in over time.
pub fn kind_weights(secs: f32) -> [f32; 4] {
    [10.0, if secs < 45.0 { 0.0 } else { ((secs - 45.0) / 20.0).min(6.0) }, if secs < 75.0 { 0.0 } else { ((secs - 75.0) / 20.0).min(5.0) }, if secs < 120.0 { 0.0 } else { ((secs - 120.0) / 25.0).min(4.0) }]
}

/// Spawn weights of the two crowd-breakers [bomber, splitter].
pub fn special_weights(secs: f32) -> [f32; 2] {
    [if secs < 80.0 { 0.0 } else { ((secs - 80.0) / 20.0).min(4.0) }, if secs < 110.0 { 0.0 } else { ((secs - 110.0) / 25.0).min(3.5) }]
}

// ---------------------------------------------------------------------------------------------
// Weapons, tomes and evolutions (see `SWARM_WEAPONS.md`)
// ---------------------------------------------------------------------------------------------

/// Weapons you can carry at once.
pub const WEAPON_SLOTS: usize = 5;
/// Level cap of every weapon.
pub const WEAPON_MAX: u8 = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Upgrade {
    // Tomes: passive boosts that apply to every weapon.
    Damage,
    FireRate,
    Bolts,
    Area,
    Pierce,
    Speed,
    Jump,
    /// Technique (no slot): the ground pound's shockwave.
    Quake,
    Vitality,
    Regen,
    Magnet,
    Crit,
    // Weapons (take a slot).
    /// An aura: a ring of light bursts around you.
    Nova,
    /// Blades circle you and slice what they touch.
    Orbit,
    SecondChance,
    /// A boomerang star that flies out and comes back, cutting everything on both legs.
    Boomerang,
    /// Lightning strikes random foes near you and arcs to their neighbours.
    Lightning,
    /// A spin slash around you every moment.
    Whirl,
    Armor,
    Growth,
    /// Technique (no slot): dives, slide kicks and jump kicks hurt what they plough through.
    Dive,
    /// The star shot: stars fly at the nearest foes.
    Stars,
    /// A sword swing in an arc that hits everything in front (and a full spin every third swing).
    Sword,
    /// A trail of fire on the ground behind you.
    Flame,
    /// Fireballs that explode and set foes alight.
    Fireball,
    /// A freezing ring: foes stop moving and take extra damage.
    Frost,
    /// Flasks that leave a poison cloud.
    Poison,
    /// A gravity well that drags foes together, then implodes.
    Gravity,
    /// Homing missiles.
    Missiles,
    // Evolutions: each needs two other upgrades at a given level and then offers one big step up.
    StarStorm,
    Supernova,
    Cyclone,
    ThunderGod,
    Comet,
    Earthshaker,
    MasterSword,
    Inferno,
    Singularity,
}

pub const UPGRADE_COUNT: usize = 38;

pub const ALL_UPGRADES: [Upgrade; UPGRADE_COUNT] = [
    Upgrade::Damage,
    Upgrade::FireRate,
    Upgrade::Bolts,
    Upgrade::Area,
    Upgrade::Pierce,
    Upgrade::Speed,
    Upgrade::Jump,
    Upgrade::Quake,
    Upgrade::Vitality,
    Upgrade::Regen,
    Upgrade::Magnet,
    Upgrade::Crit,
    Upgrade::Nova,
    Upgrade::Orbit,
    Upgrade::SecondChance,
    Upgrade::Boomerang,
    Upgrade::Lightning,
    Upgrade::Whirl,
    Upgrade::Armor,
    Upgrade::Growth,
    Upgrade::Dive,
    Upgrade::Stars,
    Upgrade::Sword,
    Upgrade::Flame,
    Upgrade::Fireball,
    Upgrade::Frost,
    Upgrade::Poison,
    Upgrade::Gravity,
    Upgrade::Missiles,
    Upgrade::StarStorm,
    Upgrade::Supernova,
    Upgrade::Cyclone,
    Upgrade::ThunderGod,
    Upgrade::Comet,
    Upgrade::Earthshaker,
    Upgrade::MasterSword,
    Upgrade::Inferno,
    Upgrade::Singularity,
];

#[derive(Clone, Copy, Debug)]
pub struct UpgradeInfo {
    pub name: &'static str,
    pub desc: &'static str,
    pub max: u8,
    pub weight: u32,
    /// Not offered before the player reaches this level.
    pub min_level: u32,
    pub rare: bool,
}

impl Upgrade {
    pub fn info(self) -> UpgradeInfo {
        let (name, desc, max, weight, min_level, rare) = match self {
            Upgrade::Damage => ("Power Tome", "All weapons hit 20% harder.", 8, 10, 1, false),
            Upgrade::FireRate => ("Haste Tome", "All weapons recharge 12% faster.", 8, 10, 1, false),
            Upgrade::Bolts => ("Quantity Tome", "One more star, boomerang, missile or strike.", 5, 6, 2, false),
            Upgrade::Area => ("Area Tome", "Blasts, auras, zones and stars grow 25%.", 5, 8, 1, false),
            Upgrade::Pierce => ("Piercing Tome", "Stars, beams and missiles pass through one more foe.", 4, 7, 2, false),
            Upgrade::Speed => ("Swift Boots", "Run and fly 7% faster.", 5, 8, 1, false),
            Upgrade::Jump => ("Spring Legs", "Jump 10% higher.", 4, 6, 1, false),
            Upgrade::Quake => ("Ground Quake", "Technique: ground-pound shockwave hits harder and wider.", 4, 5, 2, false),
            Upgrade::Vitality => ("Hearty Tome", "+30 max health and a small heal.", 6, 10, 1, false),
            Upgrade::Regen => ("Mending Tome", "Recover 0.8 health per second.", 5, 7, 1, false),
            Upgrade::Magnet => ("Gem Magnet", "Gems fly to you from 30% further.", 5, 8, 1, false),
            Upgrade::Crit => ("Luck Tome", "+8% chance for double damage.", 5, 8, 2, false),
            Upgrade::Nova => ("Nova Aura", "A ring of light bursts around you every few seconds.", WEAPON_MAX, 7, 1, false),
            Upgrade::Orbit => ("Orbit Blades", "Blades circle you and slice what they touch.", WEAPON_MAX, 7, 1, false),
            Upgrade::SecondChance => ("Second Chance", "Once per run, get back up at half health.", 1, 2, 6, true),
            Upgrade::Boomerang => ("Star Boomerang", "A star flies out and back, hitting everything twice.", WEAPON_MAX, 7, 1, false),
            Upgrade::Lightning => ("Thunder Strike", "Lightning hits random foes near you and arcs to more.", WEAPON_MAX, 7, 1, false),
            Upgrade::Whirl => ("Spin Slash", "A spinning slash cuts everything around you.", WEAPON_MAX, 7, 1, false),
            Upgrade::Armor => ("Thick Hide", "Take 8% less damage.", 5, 7, 2, false),
            Upgrade::Growth => ("Quick Study", "Gems give 12% more experience.", 5, 8, 1, false),
            Upgrade::Dive => ("Dive Strike", "Technique: dives, slide kicks and jump kicks smash foes in your path.", 4, 5, 2, false),
            Upgrade::Stars => ("Star Shot", "Stars fly at the nearest foes. Levels add a second star and pierce.", WEAPON_MAX, 8, 1, false),
            Upgrade::Sword => ("Hero Sword", "A wide sword swing hits everything in front; every third swing is a full spin.", WEAPON_MAX, 9, 1, false),
            Upgrade::Flame => ("Flamewalker", "Leave a trail of fire that burns whatever walks through.", WEAPON_MAX, 7, 1, false),
            Upgrade::Fireball => ("Fire Staff", "Fireballs explode on impact and set foes alight.", WEAPON_MAX, 7, 1, false),
            Upgrade::Frost => ("Frost Ring", "Freezes foes solid; frozen foes take extra damage from everything.", WEAPON_MAX, 6, 1, false),
            Upgrade::Poison => ("Poison Flask", "Flasks leave a poison cloud that eats and slows foes.", WEAPON_MAX, 6, 1, false),
            Upgrade::Gravity => ("Black Hole", "A gravity well drags foes together, then implodes.", WEAPON_MAX, 5, 2, false),
            Upgrade::Missiles => ("Homing Missiles", "Missiles hunt down foes and explode.", WEAPON_MAX, 6, 1, false),
            Upgrade::StarStorm => ("EVOLVE: Star Storm", "Every fourth volley also bursts into a ring of stars.", 1, 1, 1, true),
            Upgrade::Supernova => ("EVOLVE: Supernova", "Nova Aura becomes huge, fast and hits much harder.", 1, 1, 1, true),
            Upgrade::Cyclone => ("EVOLVE: Cyclone", "Spin Slash twice as often, reaching much further.", 1, 1, 1, true),
            Upgrade::ThunderGod => ("EVOLVE: Thunder God", "Two more strikes each time, arcing four times.", 1, 1, 1, true),
            Upgrade::Comet => ("EVOLVE: Comet Return", "Two more boomerangs, much bigger.", 1, 1, 1, true),
            Upgrade::Earthshaker => ("EVOLVE: Earthshaker", "Ground Quake is huge and rolls a second time.", 1, 1, 1, true),
            Upgrade::MasterSword => ("EVOLVE: Master Sword", "Every swing also throws a piercing sword beam.", 1, 1, 1, true),
            Upgrade::Inferno => ("EVOLVE: Inferno", "Bigger, longer fires, and fireballs leave flames.", 1, 1, 1, true),
            Upgrade::Singularity => ("EVOLVE: Singularity", "The black hole is huge, pulls hard and implodes violently.", 1, 1, 1, true),
        };
        UpgradeInfo { name, desc, max, weight, min_level, rare }
    }
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn is_evolution(self) -> bool {
        self.index() >= Upgrade::StarStorm.index()
    }
    /// A weapon takes one of the [`WEAPON_SLOTS`]; everything else (tomes, techniques, evolutions) does not.
    pub fn is_weapon(self) -> bool {
        matches!(self, Upgrade::Nova | Upgrade::Orbit | Upgrade::Boomerang | Upgrade::Lightning | Upgrade::Whirl | Upgrade::Stars | Upgrade::Sword | Upgrade::Flame | Upgrade::Fireball | Upgrade::Frost | Upgrade::Poison | Upgrade::Gravity | Upgrade::Missiles)
    }
    /// Evolutions open once two other upgrades are high enough.
    pub fn requirement(self) -> Option<[(Upgrade, u8); 2]> {
        Some(match self {
            Upgrade::StarStorm => [(Upgrade::Stars, 3), (Upgrade::FireRate, 2)],
            Upgrade::Supernova => [(Upgrade::Nova, 3), (Upgrade::Area, 2)],
            Upgrade::Cyclone => [(Upgrade::Whirl, 3), (Upgrade::Speed, 2)],
            Upgrade::ThunderGod => [(Upgrade::Lightning, 3), (Upgrade::Crit, 2)],
            Upgrade::Comet => [(Upgrade::Boomerang, 3), (Upgrade::Magnet, 2)],
            Upgrade::Earthshaker => [(Upgrade::Quake, 2), (Upgrade::Jump, 2)],
            Upgrade::MasterSword => [(Upgrade::Sword, 4), (Upgrade::Crit, 2)],
            Upgrade::Inferno => [(Upgrade::Fireball, 3), (Upgrade::Flame, 3)],
            Upgrade::Singularity => [(Upgrade::Gravity, 3), (Upgrade::Magnet, 2)],
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Upgrades {
    pub levels: [u8; UPGRADE_COUNT],
}

impl Default for Upgrades {
    fn default() -> Self {
        Self { levels: [0; UPGRADE_COUNT] }
    }
}

impl Upgrades {
    pub fn level(&self, u: Upgrade) -> u8 {
        self.levels[u.index()]
    }
    pub fn maxed(&self, u: Upgrade) -> bool {
        self.level(u) >= u.info().max
    }
    /// Take one level; false at the cap (or when a new weapon has no free slot).
    pub fn take(&mut self, u: Upgrade) -> bool {
        if self.maxed(u) || (u.is_weapon() && self.level(u) == 0 && self.free_slots() == 0) {
            return false;
        }
        self.levels[u.index()] += 1;
        true
    }
    /// Weapons carried.
    pub fn weapons(&self) -> usize {
        ALL_UPGRADES.iter().filter(|u| u.is_weapon() && self.level(**u) > 0).count()
    }
    pub fn free_slots(&self) -> usize {
        WEAPON_SLOTS.saturating_sub(self.weapons())
    }
    pub fn evolution_ready(&self, u: Upgrade) -> bool {
        u.requirement().is_some_and(|r| r.iter().all(|(need, lv)| self.level(*need) >= *lv))
    }
    pub fn eligible(&self, u: Upgrade, player_level: u32) -> bool {
        !self.maxed(u) && player_level >= u.info().min_level && (!u.is_evolution() || self.evolution_ready(u)) && (!u.is_weapon() || self.level(u) > 0 || self.free_slots() > 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choices {
    pub items: [Upgrade; 3],
    pub len: usize,
    /// Opened from a chest rather than a level-up.
    pub chest: bool,
}

impl Choices {
    pub fn as_slice(&self) -> &[Upgrade] {
        &self.items[..self.len]
    }
}

/// Weighted random draw of up to three distinct, non-maxed upgrades the player qualifies for.
pub fn roll_choices(rng: &mut Rng, owned: &Upgrades, player_level: u32) -> Choices {
    roll_choices_ex(rng, owned, player_level, false)
}

/// As [`roll_choices`]; a chest rolls rare cards three times as often and always offers a ready evolution.
/// Early on (few weapons, free slots) most cards are new weapons; later it is levels, tomes and evolutions.
pub fn roll_choices_ex(rng: &mut Rng, owned: &Upgrades, player_level: u32, chest: bool) -> Choices {
    let mut pool = [Upgrade::Damage; UPGRADE_COUNT];
    let mut evos = [Upgrade::Damage; UPGRADE_COUNT];
    let (mut n, mut ne) = (0, 0);
    for u in ALL_UPGRADES {
        if !owned.eligible(u, player_level) {
            continue;
        }
        if u.is_evolution() {
            evos[ne] = u;
            ne += 1;
        } else {
            pool[n] = u;
            n += 1;
        }
    }
    let mut out = Choices { items: [Upgrade::Damage; 3], len: 0, chest };
    // A ready evolution is the best card in the game: it leads most level-ups and every chest.
    if ne > 0 && rng.chance(if chest { 1.0 } else { 0.7 }) {
        out.items[0] = evos[rng.below(ne as u32) as usize];
        out.len = 1;
    }
    // New weapons first: with fewer than three weapons two of the cards are new weapons, with a free slot
    // (and a young build) one still is.
    let want_new = if owned.weapons() < 3 { 2 } else if owned.free_slots() > 0 && player_level < 14 { 1 } else { 0 };
    let mut new_taken = 0;
    while new_taken < want_new && out.len < 3 {
        let is_new = |u: &Upgrade| u.is_weapon() && owned.level(*u) == 0;
        let total: u32 = pool[..n].iter().filter(|u| is_new(u)).map(|u| u.info().weight).sum();
        if total == 0 {
            break;
        }
        let mut roll = rng.below(total);
        let mut pick = usize::MAX;
        for (i, u) in pool[..n].iter().enumerate() {
            if is_new(u) {
                let w = u.info().weight;
                if roll < w {
                    pick = i;
                    break;
                }
                roll -= w;
            }
        }
        if pick == usize::MAX {
            break;
        }
        out.items[out.len] = pool[pick];
        out.len += 1;
        pool[pick] = pool[n - 1];
        n -= 1;
        new_taken += 1;
    }
    let weight = |u: Upgrade| u.info().weight * if chest && u.info().rare { 3 } else { 1 };
    while out.len < 3 && n > 0 {
        let total: u32 = pool[..n].iter().map(|u| weight(*u)).sum();
        let mut roll = rng.below(total);
        let mut pick = n - 1;
        for (i, u) in pool[..n].iter().enumerate() {
            let w = weight(*u);
            if roll < w {
                pick = i;
                break;
            }
            roll -= w;
        }
        out.items[out.len] = pool[pick];
        out.len += 1;
        pool[pick] = pool[n - 1];
        n -= 1;
    }
    // Out of ordinary cards: fill with the remaining evolutions.
    for u in evos[..ne].iter() {
        if out.len < 3 && !out.items[..out.len].contains(u) {
            out.items[out.len] = *u;
            out.len += 1;
        }
    }
    out
}

/// Everything the upgrades add up to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub damage: f32,
    pub interval: u32,
    /// Every weapon's cooldown is multiplied by this (the Haste Tome).
    pub cd: f32,
    /// Star shot damage multiplier from its level.
    pub star_dmg: f32,
    /// Sword level (0 = none).
    pub sword: u8,
    pub bolts: u32,
    pub area: f32,
    pub pierce: u8,
    pub move_bonus: f32,
    pub jump_bonus: f32,
    pub quake: u8,
    pub max_hp: f32,
    pub regen_per_tick: f32,
    pub magnet: f32,
    pub crit: f32,
    pub nova: u8,
    pub orbit: u32,
    pub revive: bool,
    /// Boomerangs per throw (0 = none), ticks between throws.
    pub boomerangs: u32,
    pub boom_interval: u32,
    /// Lightning: strikes per cast, arcs per strike, ticks between casts.
    pub strikes: u32,
    pub arcs: u32,
    pub strike_interval: u32,
    pub whirl: u8,
    pub whirl_interval: u32,
    pub whirl_radius: f32,
    /// Damage taken is multiplied by this.
    pub armor: f32,
    pub xp_mult: f32,
    pub dive: u8,
    pub star_storm: bool,
    pub supernova: bool,
    pub earthshaker: bool,
    pub comet: bool,
}

pub fn derive_stats(u: &Upgrades) -> Stats {
    let l = |x: Upgrade| u.level(x) as f32;
    let has = |x: Upgrade| u.level(x) > 0;
    let (boom, strike, whirl) = (u.level(Upgrade::Boomerang) as u32, u.level(Upgrade::Lightning) as u32, u.level(Upgrade::Whirl));
    let (stars, nova, qty) = (u.level(Upgrade::Stars) as u32, u.level(Upgrade::Nova) as u32, u.level(Upgrade::Bolts) as u32);
    let area = 1.0 + 0.25 * l(Upgrade::Area);
    let cyclone = has(Upgrade::Cyclone);
    let cd = 0.88f32.powf(l(Upgrade::FireRate));
    Stats {
        damage: 14.0 * (1.0 + 0.2 * l(Upgrade::Damage)),
        interval: ((22.0 * cd * 0.95f32.powf(stars.saturating_sub(1) as f32)).round() as u32).max(6),
        cd,
        star_dmg: 1.0 + 0.1 * stars.saturating_sub(1) as f32,
        sword: u.level(Upgrade::Sword),
        bolts: if stars == 0 { 0 } else { 1 + qty + (stars >= 2) as u32 + (stars >= 5) as u32 },
        area,
        pierce: u.level(Upgrade::Pierce) + (stars >= 3) as u8 + (stars >= 5) as u8,
        move_bonus: 0.07 * l(Upgrade::Speed),
        jump_bonus: 0.10 * l(Upgrade::Jump),
        quake: u.level(Upgrade::Quake),
        max_hp: BASE_HP + 30.0 * l(Upgrade::Vitality),
        regen_per_tick: 0.8 * l(Upgrade::Regen) / TICKS_PER_SEC as f32,
        magnet: 220.0 * (1.0 + 0.3 * l(Upgrade::Magnet)),
        crit: 0.05 + 0.08 * l(Upgrade::Crit),
        nova: nova as u8,
        orbit: if u.level(Upgrade::Orbit) == 0 { 0 } else { 1 + u.level(Upgrade::Orbit) as u32 },
        revive: u.level(Upgrade::SecondChance) > 0,
        boomerangs: if boom == 0 { 0 } else { 1 + boom / 2 + qty / 2 + if has(Upgrade::Comet) { 2 } else { 0 } },
        boom_interval: (((70 - 6 * boom.min(6)) as f32 * cd) as u32).max(16),
        strikes: if strike == 0 { 0 } else { 1 + strike / 2 + qty / 2 + if has(Upgrade::ThunderGod) { 2 } else { 0 } },
        arcs: if strike == 0 { 0 } else { 2 + if has(Upgrade::ThunderGod) { 2 } else { 0 } },
        strike_interval: (((100 - 10 * strike.min(6)) as f32 * cd) as u32).max(20),
        whirl,
        whirl_interval: (((56.0 - 6.0 * whirl as f32) * cd * if cyclone { 0.5 } else { 1.0 }) as u32).max(10),
        whirl_radius: (290.0 + 35.0 * whirl as f32) * area.sqrt() * if cyclone { 1.45 } else { 1.0 },
        armor: 0.92f32.powf(l(Upgrade::Armor)),
        xp_mult: 1.0 + 0.12 * l(Upgrade::Growth),
        dive: u.level(Upgrade::Dive),
        star_storm: has(Upgrade::StarStorm),
        supernova: has(Upgrade::Supernova),
        earthshaker: has(Upgrade::Earthshaker),
        comet: has(Upgrade::Comet),
    }
}

// ---------------------------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Enemy {
    pub alive: bool,
    pub kind: EnemyKind,
    pub pos: [f32; 2],
    pub prev: [f32; 2],
    /// Last step taken (for facing).
    pub vel: [f32; 2],
    pub hp: f32,
    pub max_hp: f32,
    pub contact: f32,
    pub speed: f32,
    pub radius: f32,
    pub flash: u8,
    /// A golden mini-boss: bigger, tougher, and it always drops a chest.
    pub elite: bool,
    state: u8,
    timer: i16,
    dash: [f32; 2],
    orbit_cd: u8,
    boom_cd: u8,
    aux: u16,
    /// Status effects, in ticks left: burning and poisoned foes take damage over time, frozen ones cannot move.
    pub burn: u8,
    pub poison: u8,
    pub freeze: u8,
}

impl Enemy {
    const EMPTY: Enemy = Enemy { alive: false, kind: EnemyKind::Skitter, pos: [0.0; 2], prev: [0.0; 2], vel: [0.0; 2], hp: 0.0, max_hp: 0.0, contact: 0.0, speed: 0.0, radius: 0.0, flash: 0, elite: false, state: 0, timer: 0, dash: [0.0; 2], orbit_cd: 0, boom_cd: 0, aux: 0, burn: 0, poison: 0, freeze: 0 };
    /// Winding up a lunge, a fuse or a charge (the renderer shakes it).
    pub fn winding_up(&self) -> bool {
        (matches!(self.kind, EnemyKind::Dasher | EnemyKind::Chomp) && self.state == 1) || (self.kind == EnemyKind::Bomber && self.state == 1) || (self.kind == EnemyKind::Tyrant && self.state == 1) || self.winding_up_ported()
    }
    /// A bomber's fuse is burning: 0..1 up to the blast.
    pub fn fuse(&self) -> f32 {
        if self.kind == EnemyKind::Bomber && self.state == 1 { 1.0 - self.timer.max(0) as f32 / BOMB_FUSE as f32 } else { 0.0 }
    }
    /// Animation pose of a ported species, for the view.
    pub fn pose(&self) -> super::foes::Pose {
        foes::pose_of(self)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Bolt {
    pub alive: bool,
    pub pos: [f32; 2],
    pub prev: [f32; 2],
    pub vel: [f32; 2],
    pub life: u16,
    pub dmg: f32,
    pub pierce: u8,
    pub radius: f32,
    pub crit: bool,
    pub kind: BoltKind,
    /// Explosion radius on impact (fireballs, missiles); 0 = none.
    pub blast: f32,
    /// Missile target slot.
    pub aim: u16,
    recent: [u16; 3],
}

impl Bolt {
    const EMPTY: Bolt = Bolt { alive: false, pos: [0.0; 2], prev: [0.0; 2], vel: [0.0; 2], life: 0, dmg: 0.0, pierce: 0, radius: 0.0, crit: false, kind: BoltKind::Star, blast: 0.0, aim: u16::MAX, recent: [u16::MAX; 3] };
}

/// A thrown boomerang star: out, then home to the player.
#[derive(Clone, Copy, Debug)]
pub struct Boom {
    pub alive: bool,
    pub pos: [f32; 2],
    pub prev: [f32; 2],
    pub vel: [f32; 2],
    pub age: u16,
    pub dmg: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Orb {
    pub alive: bool,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub life: u16,
    pub dmg: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GemKind {
    Xp,
    Heart,
    /// Walk into it: a free upgrade pick, richer than a level-up.
    Chest,
    /// Pulls every gem on the field to you.
    Magnet,
    /// A star bomb: damages everything near.
    Bomb,
}

#[derive(Clone, Copy, Debug)]
pub struct Gem {
    pub alive: bool,
    pub kind: GemKind,
    pub pos: [f32; 2],
    pub value: u16,
    pub magnet: bool,
    speed: f32,
}

impl Gem {
    /// 0 small (gold), 1 medium (blue), 2 large (red).
    pub fn tier(&self) -> u8 {
        match self.value {
            0..=2 => 0,
            3..=9 => 1,
            _ => 2,
        }
    }
    /// What the renderer draws: gem tiers 0..=2, then heart, chest, magnet, bomb.
    pub fn look(&self) -> u8 {
        match self.kind {
            GemKind::Xp => self.tier(),
            GemKind::Heart => 3,
            GemKind::Chest => 4,
            GemKind::Magnet => 5,
            GemKind::Bomb => 6,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RingKind {
    Nova,
    Quake,
    Revive,
    /// A lightning strike's footprint.
    Strike,
    Slash,
    /// An explosion (bomber, star bomb).
    Blast,
    /// A sword swing: a pie slice facing `dir`.
    Arc,
    /// The frost ring.
    Frost,
}

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub alive: bool,
    pub kind: RingKind,
    pub center: [f32; 2],
    pub max: f32,
    pub age: u16,
    pub life: u16,
    /// Arc facing and half-width (radians); unused by the round rings.
    pub dir: f32,
    pub half: f32,
}

impl Ring {
    pub fn radius(&self) -> f32 {
        let t = (self.age as f32 / self.life.max(1) as f32).clamp(0.0, 1.0);
        self.max * (1.0 - (1.0 - t) * (1.0 - t))
    }
    pub fn fade(&self) -> f32 {
        1.0 - (self.age as f32 / self.life.max(1) as f32).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerState {
    pub pos: [f32; 2],
    /// Height of the feet above the arena floor.
    pub height: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub iframes: u16,
    pub xp: u32,
    pub level: u32,
    pub revived: bool,
}

/// What the host tells the core each tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerFrame {
    pub pos: [f32; 2],
    pub height: f32,
    /// Absolute height of the feet (course units): chests are reached by height too.
    pub y: f32,
    /// Ground pound hit the floor this tick.
    pub pound_land: bool,
    /// Diving, slide-kicking or jump-kicking this tick (the movement that Dive Strike turns into a weapon).
    pub rush: bool,
}

/// A damage number for the view: where, how much, and what kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitNum {
    pub pos: [f32; 2],
    pub amount: f32,
    pub crit: bool,
    /// 0 a foe was hit, 1 the player was hurt, 2 the player healed.
    pub kind: u8,
}

pub const NUMS_MAX: usize = 40;

/// What happened during the last tick (sound, particles, toasts).
#[derive(Clone, Copy, Debug, Default)]
pub struct Events {
    pub kills: u16,
    pub last_kill: [f32; 2],
    pub hit: bool,
    pub level_up: bool,
    pub gems: u16,
    pub hearts: u16,
    pub fired: bool,
    pub nova: bool,
    pub quake: bool,
    pub boss_spawned: bool,
    pub boss_killed: bool,
    pub surge: bool,
    pub died: bool,
    pub won: bool,
    pub revived: bool,
    pub slash: bool,
    pub thunder: bool,
    pub strike_pos: [[f32; 2]; 3],
    pub strike_n: u8,
    pub boom_throw: bool,
    pub rush_hit: bool,
    pub chest_opened: bool,
    pub elite_spawned: bool,
    pub elite_killed: bool,
    pub final_spawned: bool,
    pub stampede: bool,
    pub vacuum: bool,
    pub bomb: bool,
    pub blast: bool,
    pub blast_pos: [f32; 2],
    pub storm: bool,
    /// A world chest was opened this tick (its tier), a key was found, a golden fight began, one was won or lost.
    pub chest_tier: Option<ChestTier>,
    pub key_found: bool,
    pub challenge_started: bool,
    pub challenge_won: bool,
    pub challenge_lost: bool,
    pub split: bool,
    pub crit: bool,
}

// ---------------------------------------------------------------------------------------------
// Spatial hash
// ---------------------------------------------------------------------------------------------

const NIL: u16 = u16::MAX;

/// A uniform grid over the arena with linked cell lists: rebuilt every tick in O(live foes), queried
/// by position and radius. Fixed-size arrays, no allocation after construction.
#[derive(Clone, Debug)]
struct Grid {
    n: usize,
    head: Vec<u16>,
    next: Vec<u16>,
    /// Largest radius among the foes in the grid (the query padding).
    max_r: f32,
    /// Centre of the covered square (the player in explore mode, the arena centre otherwise).
    origin: [f32; 2],
}

impl Grid {
    fn new() -> Self {
        let n = (ARENA_HALF * 2.0 / GRID_CELL).ceil() as usize + 2;
        Self { n, head: vec![NIL; n * n], next: vec![NIL; ENEMY_SLOTS], max_r: 0.0, origin: [0.0; 2] }
    }

    fn cell(&self, v: f32, axis: usize) -> usize {
        (((v - self.origin[axis] + ARENA_HALF) / GRID_CELL).floor().max(0.0) as usize).min(self.n - 1)
    }

    fn rebuild(&mut self, enemies: &[Enemy], idx: &[u16], origin: [f32; 2]) {
        self.origin = origin;
        self.head.fill(NIL);
        self.max_r = 0.0;
        for &i in idx {
            let e = &enemies[i as usize];
            let c = self.cell(e.pos[1], 1) * self.n + self.cell(e.pos[0], 0);
            self.next[i as usize] = self.head[c];
            self.head[c] = i;
            self.max_r = self.max_r.max(e.radius);
        }
    }

    /// Every foe whose cell the disc of `r` around `p` touches (a superset of the foes within `r`).
    fn near(&self, p: [f32; 2], r: f32, out: &mut Vec<u16>) {
        out.clear();
        let (x0, x1, z0, z1) = (self.cell(p[0] - r, 0), self.cell(p[0] + r, 0), self.cell(p[1] - r, 1), self.cell(p[1] + r, 1));
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                let mut i = self.head[cz * self.n + cx];
                while i != NIL {
                    out.push(i);
                    i = self.next[i as usize];
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The game
// ---------------------------------------------------------------------------------------------

/// Cells per side of one streamed chunk of the explore-mode walk mask.
pub const CHUNK_CELLS: i32 = 16;
/// Cell size (SM64 units) of the explore-mode walk mask.
pub const WALK_CELL: f32 = 100.0;

/// Which ground cells can be walked on (the real course's terrain is not a flat square): enemies do not spawn
/// on or step into blocked cells. Two kinds: a dense disc built in one go (`build`, the fixed arena of the
/// unit tests) or a sparse set of streamed chunks (`streamed`) that the host fills as the player walks, so a
/// whole course needs no big grid. A cell in an absent chunk counts as blocked.
#[derive(Clone, Debug)]
pub struct WalkMask {
    pub half: f32,
    pub cell: f32,
    pub n: usize,
    pub ok: Vec<bool>,
    chunks: std::collections::HashMap<(i32, i32), Box<[bool]>>,
    streamed: bool,
}

impl WalkMask {
    /// Build by asking `walkable(x, z)` (arena-local coordinates) at every cell centre.
    pub fn build(half: f32, cell: f32, mut walkable: impl FnMut(f32, f32) -> bool) -> Self {
        let n = (half * 2.0 / cell).ceil() as usize;
        let mut ok = Vec::with_capacity(n * n);
        for iz in 0..n {
            for ix in 0..n {
                ok.push(walkable(-half + (ix as f32 + 0.5) * cell, -half + (iz as f32 + 0.5) * cell));
            }
        }
        Self { half, cell, n, ok, chunks: Default::default(), streamed: false }
    }
    /// An empty streamed mask (chunks of [`CHUNK_CELLS`] cells of [`WALK_CELL`] units, origin at the local 0,0).
    pub fn streamed() -> Self {
        Self { half: 0.0, cell: WALK_CELL, n: 0, ok: vec![], chunks: Default::default(), streamed: true }
    }
    /// Chunk key of a local point.
    pub fn chunk_of(p: [f32; 2]) -> (i32, i32) {
        let c = WALK_CELL * CHUNK_CELLS as f32;
        ((p[0] / c).floor() as i32, (p[1] / c).floor() as i32)
    }
    /// Store (or replace) a chunk: `CHUNK_CELLS^2` flags, row-major over z then x.
    pub fn set_chunk(&mut self, key: (i32, i32), bits: Box<[bool]>) {
        debug_assert_eq!(bits.len(), (CHUNK_CELLS * CHUNK_CELLS) as usize);
        self.chunks.insert(key, bits);
    }
    pub fn has_chunk(&self, key: (i32, i32)) -> bool {
        self.chunks.contains_key(&key)
    }
    pub fn at(&self, p: [f32; 2]) -> bool {
        if self.streamed {
            let (cx, cz) = ((p[0] / WALK_CELL).floor() as i32, (p[1] / WALK_CELL).floor() as i32);
            let key = (cx.div_euclid(CHUNK_CELLS), cz.div_euclid(CHUNK_CELLS));
            return self.chunks.get(&key).is_some_and(|b| b[(cz.rem_euclid(CHUNK_CELLS) * CHUNK_CELLS + cx.rem_euclid(CHUNK_CELLS)) as usize]);
        }
        let (ix, iz) = (((p[0] + self.half) / self.cell).floor(), ((p[1] + self.half) / self.cell).floor());
        if ix < 0.0 || iz < 0.0 || ix >= self.n as f32 || iz >= self.n as f32 {
            return false;
        }
        self.ok[iz as usize * self.n + ix as usize]
    }
    pub fn fraction(&self) -> f32 {
        if self.streamed {
            let (mut ok, mut all) = (0usize, 0usize);
            for c in self.chunks.values() {
                ok += c.iter().filter(|b| **b).count();
                all += c.len();
            }
            return ok as f32 / all.max(1) as f32;
        }
        self.ok.iter().filter(|b| **b).count() as f32 / self.ok.len().max(1) as f32
    }
    /// Is every cell on the straight line from `a` to `b` walkable? (Continuous ground between the two: no wall, drop or pit.)
    pub fn line_clear(&self, a: [f32; 2], b: [f32; 2]) -> bool {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let n = ((dx * dx + dz * dz).sqrt() / (self.cell * 0.8)).ceil().max(1.0) as i32;
        (0..=n).all(|i| {
            let t = i as f32 / n as f32;
            self.at([a[0] + dx * t, a[1] + dz * t])
        })
    }
}

/// The characters differ in what they start with: a first weapon (or two) at level one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Hero {
    /// No head start (the plain rules).
    #[default]
    Plain,
    /// Mario: the dive and the ground pound.
    Mario,
    /// Link: the sword.
    Link,
    /// Banjo: a boomerang.
    Banjo,
}

impl Hero {
    pub fn starters(self) -> &'static [Upgrade] {
        match self {
            Hero::Plain => &[Upgrade::Stars],
            Hero::Mario => &[Upgrade::Stars, Upgrade::Stars, Upgrade::Stars, Upgrade::Dive, Upgrade::Quake],
            Hero::Link => &[Upgrade::Sword, Upgrade::Sword],
            Hero::Banjo => &[Upgrade::Stars, Upgrade::Boomerang, Upgrade::Boomerang],
        }
    }
}

/// A chest of the level (see `swarm_world.rs`): where it lies, how rich it is, and what the player has done to it.
#[derive(Clone, Copy, Debug)]
pub struct WorldChest {
    pub site: ChestSite,
    pub opened: bool,
    /// The player has come close enough to know it is there (it stays on the compass).
    pub revealed: bool,
}

/// A golden chest's timed fight: survive it and the chest opens.
#[derive(Clone, Copy, Debug)]
pub struct Challenge {
    pub chest: usize,
    pub ticks_left: u32,
}

/// Distance at which a chest is noticed (revealed, its beacon lit).
pub const REVEAL_R: f32 = 1300.0;
/// Distance at which a hidden chest sparkles and chimes.
pub const HINT_R: f32 = 2300.0;
/// Walk into a chest this close (and no more than `CHEST_REACH_Y` above or below) to open it.
pub const CHEST_R: f32 = 140.0;
pub const CHEST_REACH_Y: f32 = 260.0;
/// Length of a golden chest's fight.
pub const CHALLENGE_TICKS: u32 = 20 * TICKS_PER_SEC;
/// Foes farther than this from the player are recycled (explore mode); spawns happen well inside it.
pub const DESPAWN_R: f32 = 3000.0;

const BOMB_FUSE: i16 = 22;
const NUM_STRIKES: usize = 3;

pub struct Swarm {
    /// Radius of the arena disc (the plaza default; a real course uses a smaller one).
    pub half: f32,
    pub mask: Option<WalkMask>,
    /// Explore mode: no arena, no storm; foes spawn around the player on streamed walkable ground and are recycled
    /// when left far behind.
    pub explore: bool,
    /// The level's chests (planned by the host from the course's collision).
    pub wchests: Vec<WorldChest>,
    pub keys: u32,
    /// Gold from chests (turned into star bits at the end of the run).
    pub gold: u32,
    /// Height of the player's feet above sea level of the course (the host sets it each tick for chest reach).
    pub player_floor_y: f32,
    pub challenge: Option<Challenge>,
    challenge_cooldown: u32,
    pub rng: Rng,
    pub tick: u32,
    pub player: PlayerState,
    pub up: Upgrades,
    pub stats: Stats,
    pub tier: Tier,
    pub perks: Perks,
    pub hero: Hero,
    pub enemies: Vec<Enemy>,
    pub bolts: Vec<Bolt>,
    pub booms: Vec<Boom>,
    pub orbs: Vec<Orb>,
    pub gems: Vec<Gem>,
    pub rings: [Ring; RING_SLOTS],
    pub zones: [Zone; ZONE_SLOTS],
    pub orbit_pos: [[f32; 2]; ORBIT_MAX],
    pub orbit_count: usize,
    pub choices: Option<Choices>,
    pub pending_levels: u32,
    pub pending_chests: u32,
    pub over: bool,
    /// The run ended by killing the Star Tyrant rather than by dying.
    pub won: bool,
    pub kills: u32,
    pub bosses: u32,
    pub elites: u32,
    pub chests_opened: u32,
    pub final_spawned: bool,
    /// The closing storm: radius of the safe disc, and whether it is on at all.
    pub ring_r: f32,
    pub storm: bool,
    pub ev: Events,
    /// Damage numbers of the last tick.
    pub nums: Vec<HitNum>,
    /// The boss on the field with the most health: its kind and fraction left.
    pub boss_bar: Option<(EnemyKind, f32)>,
    pub alive_enemies: usize,
    /// Test hook: enemies stand still.
    pub freeze: bool,
    cap: usize,
    fire_cd: u32,
    nova_cd: u32,
    whirl_cd: u32,
    boom_cd: u32,
    strike_cd: u32,
    sword_cd: u32,
    sword_n: u32,
    flame_cd: u32,
    flame_last: [f32; 2],
    fire_cd2: u32,
    frost_cd: u32,
    poison_cd: u32,
    grav_cd: u32,
    missile_cd: u32,
    burn_dmg: f32,
    poison_dmg: f32,
    volley_n: u32,
    echo: (u8, [f32; 2], f32, f32),
    tyrant_dead: bool,
    orbit_angle: f32,
    alive_idx: Vec<u16>,
    grid: Grid,
    near: Vec<u16>,
}

impl Swarm {
    pub fn new(seed: u64) -> Self {
        Self::with_cap(seed, enemy_cap())
    }

    /// A run for a character on a difficulty, with the rank's perks.
    pub fn start(seed: u64, cap: usize, hero: Hero, tier: Tier, perks: Perks) -> Self {
        let mut s = Self::with_cap(seed, cap);
        s.hero = hero;
        s.tier = tier;
        s.perks = perks;
        s.up = Upgrades::default();
        for u in hero.starters() {
            s.up.take(*u);
        }
        s.stats = derive_stats(&s.up);
        s.apply_perks();
        s.player.hp = s.player.max_hp;
        s
    }

    fn apply_perks(&mut self) {
        self.stats.damage *= self.perks.damage;
        self.stats.max_hp += self.perks.max_hp;
        self.stats.xp_mult *= self.perks.xp;
        self.player.max_hp = self.stats.max_hp;
    }

    /// Recompute the stats from the upgrades (and the rank's perks), keeping health in step.
    pub fn refresh_stats(&mut self) {
        self.stats = derive_stats(&self.up);
        self.apply_perks();
    }

    pub fn with_cap(seed: u64, cap: usize) -> Self {
        let mut up = Upgrades::default();
        up.levels[Upgrade::Stars.index()] = 1;
        let stats = derive_stats(&up);
        Self {
            half: ARENA_HALF,
            mask: None,
            explore: false,
            wchests: vec![],
            keys: 0,
            gold: 0,
            player_floor_y: 0.0,
            challenge: None,
            challenge_cooldown: 0,
            rng: Rng::new(seed),
            tick: 0,
            player: PlayerState { pos: [0.0; 2], height: 0.0, hp: stats.max_hp, max_hp: stats.max_hp, iframes: 0, xp: 0, level: 1, revived: false },
            up,
            stats,
            tier: super::swarm_meta::tier(0),
            perks: Perks { damage: 1.0, max_hp: 0.0, xp: 1.0 },
            hero: Hero::Plain,
            enemies: vec![Enemy::EMPTY; ENEMY_SLOTS],
            bolts: vec![Bolt::EMPTY; BOLT_SLOTS],
            booms: vec![Boom { alive: false, pos: [0.0; 2], prev: [0.0; 2], vel: [0.0; 2], age: 0, dmg: 0.0, radius: 0.0 }; BOOM_SLOTS],
            orbs: vec![Orb { alive: false, pos: [0.0; 2], vel: [0.0; 2], life: 0, dmg: 0.0 }; ORB_SLOTS],
            gems: vec![Gem { alive: false, kind: GemKind::Xp, pos: [0.0; 2], value: 0, magnet: false, speed: 0.0 }; GEM_SLOTS],
            zones: [Zone::EMPTY; ZONE_SLOTS],
            rings: [Ring { alive: false, kind: RingKind::Nova, center: [0.0; 2], max: 0.0, age: 0, life: 1, dir: 0.0, half: 0.0 }; RING_SLOTS],
            orbit_pos: [[0.0; 2]; ORBIT_MAX],
            orbit_count: 0,
            choices: None,
            pending_levels: 0,
            pending_chests: 0,
            over: false,
            won: false,
            kills: 0,
            bosses: 0,
            elites: 0,
            chests_opened: 0,
            final_spawned: false,
            ring_r: ARENA_HALF,
            storm: true,
            ev: Events::default(),
            nums: Vec::with_capacity(NUMS_MAX),
            boss_bar: None,
            alive_enemies: 0,
            freeze: false,
            cap: cap.min(ENEMY_SLOTS),
            fire_cd: 20,
            nova_cd: 60,
            whirl_cd: 30,
            boom_cd: 40,
            strike_cd: 50,
            sword_cd: 20,
            sword_n: 0,
            flame_cd: 0,
            flame_last: [1e9; 2],
            fire_cd2: 25,
            frost_cd: 40,
            poison_cd: 30,
            grav_cd: 60,
            missile_cd: 30,
            burn_dmg: 0.0,
            poison_dmg: 0.0,
            volley_n: 0,
            echo: (0, [0.0; 2], 0.0, 0.0),
            tyrant_dead: false,
            orbit_angle: 0.0,
            alive_idx: Vec::with_capacity(ENEMY_SLOTS),
            grid: Grid::new(),
            near: Vec::with_capacity(ENEMY_SLOTS),
        }
    }

    pub fn secs(&self) -> f32 {
        self.tick as f32 / TICKS_PER_SEC as f32
    }

    pub fn paused(&self) -> bool {
        self.over || self.choices.is_some()
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    fn num(&mut self, pos: [f32; 2], amount: f32, crit: bool, kind: u8) {
        if self.nums.len() < NUMS_MAX && amount >= 0.5 {
            self.nums.push(HitNum { pos, amount, crit, kind });
        }
    }

    /// Damage the player unless the i-frames are up (armor applies). Returns whether it landed.
    pub fn hurt_player(&mut self, amount: f32) -> bool {
        if self.player.iframes > 0 || self.over || amount <= 0.0 {
            return false;
        }
        let amount = amount * self.stats.armor;
        self.player.hp -= amount;
        self.player.iframes = IFRAMES;
        self.ev.hit = true;
        let p = self.player.pos;
        self.num(p, amount, false, 1);
        true
    }

    pub fn heal(&mut self, amount: f32) {
        self.player.hp = (self.player.hp + amount).min(self.player.max_hp);
    }

    /// Apply the chosen card. Returns false when there is nothing to choose.
    pub fn pick(&mut self, index: usize) -> bool {
        let Some(c) = self.choices else { return false };
        if index >= c.len {
            return false;
        }
        let u = c.items[index];
        self.up.take(u);
        let old_max = self.player.max_hp;
        self.refresh_stats();
        if self.stats.max_hp > old_max {
            // Vitality pays out the new health plus a little more.
            self.player.hp = (self.player.hp + (self.stats.max_hp - old_max) + 0.1 * self.stats.max_hp).min(self.stats.max_hp);
        }
        self.choices = None;
        if c.chest {
            self.pending_chests = self.pending_chests.saturating_sub(1);
        } else {
            self.pending_levels = self.pending_levels.saturating_sub(1);
        }
        self.offer();
        true
    }

    /// Roll the next card set when a level-up or a chest is waiting (chests first).
    fn offer(&mut self) {
        while (self.pending_levels > 0 || self.pending_chests > 0) && self.choices.is_none() {
            let chest = self.pending_chests > 0;
            let c = roll_choices_ex(&mut self.rng, &self.up, self.player.level, chest);
            if c.len == 0 {
                // Everything is maxed: take a heal instead.
                self.heal(0.25 * self.player.max_hp);
                if chest {
                    self.pending_chests -= 1;
                } else {
                    self.pending_levels -= 1;
                }
            } else {
                self.choices = Some(c);
            }
        }
    }

    pub fn add_xp(&mut self, amount: u32) {
        self.player.xp += amount;
        while self.player.xp >= xp_to_next(self.player.level) {
            self.player.xp -= xp_to_next(self.player.level);
            self.player.level += 1;
            self.pending_levels += 1;
            self.ev.level_up = true;
        }
        self.offer();
    }

    // ---- main step ---------------------------------------------------------------------------

    pub fn tick(&mut self, f: &PlayerFrame) {
        self.ev = Events::default();
        self.nums.clear();
        if self.paused() {
            return;
        }
        self.player.pos = f.pos;
        self.player.height = f.height;
        self.player_floor_y = f.y;
        self.tick += 1;
        let secs = self.secs();
        self.player.iframes = self.player.iframes.saturating_sub(1);
        if self.stats.regen_per_tick > 0.0 {
            self.heal(self.stats.regen_per_tick);
        }
        self.update_storm(secs);
        for e in self.enemies.iter_mut().filter(|e| e.alive) {
            e.prev = e.pos;
            e.flash = e.flash.saturating_sub(1);
            e.boom_cd = e.boom_cd.saturating_sub(1);
        }
        for b in self.bolts.iter_mut().filter(|b| b.alive) {
            b.prev = b.pos;
        }
        for b in self.booms.iter_mut().filter(|b| b.alive) {
            b.prev = b.pos;
        }
        self.spawn(secs);
        self.move_enemies();
        if self.explore {
            self.recycle(secs);
        }
        let origin = if self.explore { self.player.pos } else { [0.0; 2] };
        self.grid.rebuild(&self.enemies, &self.alive_idx, origin);
        self.separate();
        self.attack();
        self.attack_arms();
        self.update_bolts();
        self.update_zones();
        self.update_statuses();
        self.update_booms();
        self.update_orbit();
        self.rush(f.rush);
        if f.pound_land {
            self.quake();
        }
        self.update_echo();
        self.update_orbs();
        self.contact(secs);
        self.update_gems();
        if !self.wchests.is_empty() {
            self.update_world_chests();
        }
        self.update_rings();
        self.update_boss_bar();
        // Knockbacks may shove a foe over an edge: it goes back to where it stood at the start of the tick.
        if self.mask.is_some() {
            for k in 0..self.alive_idx.len() {
                let i = self.alive_idx[k] as usize;
                let e = &self.enemies[i];
                if e.alive && !self.walkable(e.pos) && self.walkable(e.prev) {
                    self.enemies[i].pos = self.enemies[i].prev;
                }
            }
        }
        if self.player.hp <= 0.0 {
            self.die();
        }
        if !self.over && self.tyrant_dead {
            self.won = true;
            self.over = true;
            self.ev.won = true;
        }
    }

    /// The storm ring closes in; standing outside it hurts steadily (armor does not help).
    fn update_storm(&mut self, secs: f32) {
        if self.explore {
            self.storm = false;
            return;
        }
        self.ring_r = if self.storm { ring_radius(self.half, secs) } else { self.half * 2.0 };
        if self.storm && dist2(self.player.pos, [0.0, 0.0]) > self.ring_r * self.ring_r {
            self.ev.storm = true;
            self.player.hp -= storm_dps(secs) / TICKS_PER_SEC as f32;
            if self.tick % TICKS_PER_SEC == 0 {
                let p = self.player.pos;
                self.num(p, storm_dps(secs), false, 1);
            }
        }
    }

    fn die(&mut self) {
        if self.stats.revive && !self.player.revived {
            self.player.revived = true;
            self.player.hp = 0.5 * self.player.max_hp;
            self.player.iframes = 3 * TICKS_PER_SEC as u16;
            self.ev.revived = true;
            let c = self.player.pos;
            self.add_ring(RingKind::Revive, c, 900.0, 14);
            let dmg = 6.0 * self.stats.damage;
            for i in 0..ENEMY_SLOTS {
                if self.enemies[i].alive && dist2(self.enemies[i].pos, c) < 900.0 * 900.0 && !self.enemies[i].kind.is_boss() && !self.enemies[i].elite {
                    self.damage_enemy(i, dmg);
                }
            }
        } else {
            self.player.hp = 0.0;
            self.over = true;
            self.ev.died = true;
        }
    }

    // ---- spawning ----------------------------------------------------------------------------

    fn free_enemy(&self) -> Option<usize> {
        self.enemies.iter().position(|e| !e.alive)
    }

    pub fn spawn_enemy(&mut self, kind: EnemyKind, pos: [f32; 2], secs: f32) -> bool {
        if self.alive_enemies >= self.cap && !kind.is_boss() {
            return false;
        }
        self.spawn_enemy_force(kind, pos, secs)
    }

    /// Spawn ignoring the cap (bosses, the pieces of a splitter): only a full slot table refuses.
    fn spawn_enemy_force(&mut self, kind: EnemyKind, pos: [f32; 2], secs: f32) -> bool {
        self.spawn_at(kind, pos, secs).is_some()
    }

    /// As [`Self::spawn_enemy_force`], returning the slot used.
    fn spawn_at(&mut self, kind: EnemyKind, pos: [f32; 2], secs: f32) -> Option<usize> {
        if !pos[0].is_finite() || !pos[1].is_finite() {
            return None;
        }
        if !kind.is_boss() && self.alive_enemies >= self.cap + OVERCAP {
            return None;
        }
        let slot = self.free_enemy()?;
        let s = enemy_stats(kind);
        let boss_scale = match kind {
            EnemyKind::King | EnemyKind::Chomp => 1.0 + 0.4 * self.bosses as f32,
            _ => 1.0,
        };
        let hp = s.hp * hp_mult(secs) * boss_scale * self.tier.hp;
        let mut seed = 0;
        let timer = match kind {
            EnemyKind::Wisp => 40 + self.rng.below(30) as i16,
            EnemyKind::King => 90,
            EnemyKind::Chomp => 120,
            EnemyKind::Tyrant => 100,
            EnemyKind::Dasher => 20 + self.rng.below(20) as i16,
            k if k.species().is_some() => {
                let (t, sd) = foes::init(k, &mut self.rng);
                seed = sd;
                t
            }
            _ => 0,
        };
        self.enemies[slot] = Enemy { alive: true, kind, pos, prev: pos, vel: [0.0; 2], hp, max_hp: hp, contact: s.contact * damage_mult(secs) * self.tier.damage, speed: s.speed * speed_mult(secs) * self.tier.speed, radius: s.radius, flash: 0, elite: false, state: 0, timer, dash: [0.0; 2], orbit_cd: seed, boom_cd: 0, aux: 0, burn: 0, poison: 0, freeze: 0 };
        self.alive_enemies += 1;
        Some(slot)
    }

    /// An elite: a big, tough, golden foe of an ordinary kind. It always drops a chest.
    pub fn spawn_elite(&mut self, kind: EnemyKind, pos: [f32; 2], secs: f32) -> bool {
        let Some(slot) = self.spawn_at(kind, pos, secs) else { return false };
        let e = &mut self.enemies[slot];
        e.elite = true;
        e.hp *= 7.0;
        e.max_hp = e.hp;
        e.radius *= 1.35;
        e.contact *= 1.4;
        e.speed *= 0.92;
        self.ev.elite_spawned = true;
        true
    }

    /// Can an enemy stand at `p`?
    pub fn walkable(&self, p: [f32; 2]) -> bool {
        dist2(p, [0.0, 0.0]) < self.half * self.half && self.mask.as_ref().is_none_or(|m| m.at(p))
    }

    /// A point on a ring around the player, kept inside the safe disc (and on walkable ground).
    fn edge_point(&mut self, min_d: f32, max_d: f32) -> [f32; 2] {
        let p = self.player.pos;
        if self.explore {
            // Around the player on ground that is walkable and joined to him (no wall or pit in a straight line).
            for attempt in 0..48 {
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let d = self.rng.range(min_d, max_d);
                let q = [p[0] + a.cos() * d, p[1] + a.sin() * d];
                if self.walkable(q) && (attempt >= 36 || self.mask.as_ref().is_none_or(|m| m.line_clear(p, q))) {
                    return q;
                }
            }
            return [f32::NAN; 2];
        }
        let lim = (self.ring_r.min(self.half) - 200.0).max(300.0);
        for _ in 0..24 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = self.rng.range(min_d, max_d);
            let q = [p[0] + a.cos() * d, p[1] + a.sin() * d];
            if dist2(q, [0.0, 0.0]) < lim * lim && self.walkable(q) {
                return q;
            }
        }
        // Cornered: any walkable point on the far side of the arena.
        for _ in 0..64 {
            let q = [self.rng.range(-lim, lim), self.rng.range(-lim, lim)];
            if dist2(q, [0.0, 0.0]) < lim * lim && self.walkable(q) && dist2(q, p) > min_d * 0.6 {
                return q;
            }
        }
        [if p[0] > 0.0 { -lim * 0.7 } else { lim * 0.7 }, if p[1] > 0.0 { -lim * 0.7 } else { lim * 0.7 }]
    }

    fn spawn(&mut self, secs: f32) {
        // A mini-boss every two minutes (King, Chomp, King), then the Star Tyrant.
        if secs >= BOSS_PERIOD_SECS * (self.bosses + 1) as f32 && secs < FINAL_SECS - 20.0 {
            let pos = self.edge_point(1700.0, 2100.0);
            let kind = if self.bosses % 2 == 0 { EnemyKind::King } else { EnemyKind::Chomp };
            if self.spawn_enemy(kind, pos, secs) {
                self.bosses += 1;
                self.ev.boss_spawned = true;
            }
        }
        if secs >= FINAL_SECS && !self.final_spawned {
            let pos = self.edge_point(1500.0, 1900.0);
            if self.spawn_enemy(EnemyKind::Tyrant, pos, secs) {
                self.final_spawned = true;
                self.ev.final_spawned = true;
                self.ev.boss_spawned = true;
            }
        }
        // Elites walk in on a schedule; later ones come in pairs.
        if secs >= ELITE_FIRST_SECS + ELITE_PERIOD_SECS * self.elites as f32 && !(secs >= FINAL_SECS - 10.0 && !self.final_spawned) {
            self.elites += 1;
            let n = 1 + (secs / 150.0) as usize;
            for _ in 0..n {
                let kinds = [EnemyKind::Brute, EnemyKind::Dasher, EnemyKind::Splitter, EnemyKind::Bomber, EnemyKind::Wolfos];
                let k = kinds[self.rng.below(if secs < 80.0 { 2 } else { kinds.len() as u32 }) as usize];
                let pos = self.edge_point(1500.0, 2000.0);
                self.spawn_elite(k, pos, secs);
            }
        }
        // A ring of skitters closes in every 40 seconds.
        if self.tick >= SURGE_PERIOD_TICKS && self.tick % SURGE_PERIOD_TICKS == 0 {
            self.ev.surge = true;
            let n = 10 + (secs / 15.0) as usize;
            let c = self.player.pos;
            let lim = self.ring_r.min(self.half) - 150.0;
            for i in 0..n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let mut q = [c[0] + a.cos() * 1500.0, c[1] + a.sin() * 1500.0];
                let d = dist2(q, [0.0, 0.0]).sqrt();
                if d > lim && !self.explore {
                    q = [q[0] * lim / d, q[1] * lim / d];
                }
                if !self.walkable(q) {
                    q = self.edge_point(1200.0, 1700.0);
                }
                if !self.spawn_enemy(EnemyKind::Skitter, q, secs) {
                    break;
                }
            }
        }
        // A stampede: a line of dashers charging in from one side.
        if secs >= 150.0 && self.tick % STAMPEDE_PERIOD_TICKS == 0 {
            self.ev.stampede = true;
            let n = 8 + (secs / 45.0) as usize;
            let base = self.edge_point(1500.0, 1900.0);
            let p = self.player.pos;
            let away = [base[0] - p[0], base[1] - p[1]];
            let len = (away[0] * away[0] + away[1] * away[1]).sqrt().max(1.0);
            let side = [-away[1] / len, away[0] / len];
            for i in 0..n {
                let off = (i as f32 - (n as f32 - 1.0) / 2.0) * 150.0;
                let q = [base[0] + side[0] * off, base[1] + side[1] * off];
                if !self.walkable(q) {
                    continue;
                }
                if self.alive_enemies < self.cap {
                    if let Some(slot) = self.spawn_at(EnemyKind::Dasher, q, secs) {
                        self.enemies[slot].state = 1;
                        self.enemies[slot].timer = 14 + (i % 4) as i16;
                    }
                }
            }
        }
        let want = target_live_at(secs, self.cap, self.tier.spawn);
        let deficit = want.saturating_sub(self.alive_enemies);
        let mut budget = 3 + (deficit / 14).min(5);
        while self.alive_enemies < want && budget > 0 {
            budget -= 1;
            let kind = self.pick_kind(secs);
            let pos = self.edge_point(1600.0, 2200.0);
            if !self.spawn_enemy(kind, pos, secs) {
                break;
            }
        }
    }

    /// Explore mode: foes left far behind are recycled (the director spawns fresh ones near the player), so the live
    /// count stays under the cap however far the player runs. Bosses and elites are moved instead.
    fn recycle(&mut self, secs: f32) {
        let p = self.player.pos;
        let far2 = DESPAWN_R * DESPAWN_R;
        for i in 0..ENEMY_SLOTS {
            let e = &self.enemies[i];
            if !e.alive || dist2(e.pos, p) <= far2 {
                continue;
            }
            if e.kind.is_boss() || e.elite {
                let q = self.edge_point(1500.0, 2000.0);
                if q[0].is_finite() {
                    self.enemies[i].pos = q;
                    self.enemies[i].prev = q;
                }
            } else {
                self.enemies[i].alive = false;
                self.alive_enemies = self.alive_enemies.saturating_sub(1);
            }
        }
        // Gems left far behind are forgotten (the field would fill with them over a long walk).
        let gem_far2 = (DESPAWN_R * 2.2) * (DESPAWN_R * 2.2);
        if self.tick % 30 == 0 {
            for g in self.gems.iter_mut().filter(|g| g.alive && g.kind != GemKind::Chest) {
                if dist2(g.pos, p) > gem_far2 {
                    g.alive = false;
                }
            }
        }
        let _ = secs;
    }

    /// Chests of the level: reveal when near, open on touch (golden ones need a key or a survived fight).
    fn update_world_chests(&mut self) {
        let p = self.player.pos;
        let h = self.player.height;
        let _ = h;
        self.challenge_cooldown = self.challenge_cooldown.saturating_sub(1);
        // A golden fight in progress.
        if let Some(mut c) = self.challenge {
            let site = self.wchests[c.chest].site;
            c.ticks_left = c.ticks_left.saturating_sub(1);
            let d2 = dist2(site.pos, p);
            if d2 > 1500.0 * 1500.0 {
                self.challenge = None;
                self.challenge_cooldown = 8 * TICKS_PER_SEC;
                self.ev.challenge_lost = true;
            } else if c.ticks_left == 0 {
                self.challenge = None;
                self.ev.challenge_won = true;
                self.open_world_chest(c.chest, 2);
            } else {
                // Reinforcements every two seconds while it lasts.
                if c.ticks_left % 60 == 0 && self.alive_enemies < self.cap + OVERCAP {
                    let secs = self.secs();
                    for k in 0..4 {
                        let a = self.rng.range(0.0, std::f32::consts::TAU) + k as f32;
                        let q = [site.pos[0] + a.cos() * 900.0, site.pos[1] + a.sin() * 900.0];
                        if self.walkable(q) {
                            let kind = self.pick_kind(secs);
                            self.spawn_enemy(kind, q, secs);
                        }
                    }
                }
                self.challenge = Some(c);
            }
        }
        for i in 0..self.wchests.len() {
            let w = self.wchests[i];
            if w.opened {
                continue;
            }
            let d2 = dist2(w.site.pos, p);
            if d2 < REVEAL_R * REVEAL_R {
                self.wchests[i].revealed = true;
            }
            let reach = d2 < CHEST_R * CHEST_R && (self.player_floor_y - w.site.y).abs() < CHEST_REACH_Y;
            if !reach {
                continue;
            }
            if w.site.tier != ChestTier::Golden {
                self.open_world_chest(i, 1);
            } else if self.keys > 0 {
                self.keys -= 1;
                self.open_world_chest(i, 2);
            } else if self.challenge.is_none() && self.challenge_cooldown == 0 {
                self.challenge = Some(Challenge { chest: i, ticks_left: CHALLENGE_TICKS });
                self.ev.challenge_started = true;
                // The guardians: an elite and a ring of foes.
                let secs = self.secs();
                let kinds = [EnemyKind::Brute, EnemyKind::Dasher, EnemyKind::Splitter];
                let k = kinds[self.rng.below(3) as usize];
                let q = [w.site.pos[0] + 700.0, w.site.pos[1]];
                let q = if self.walkable(q) { q } else { w.site.pos };
                self.spawn_elite(k, q, secs);
            }
        }
    }

    fn open_world_chest(&mut self, i: usize, richness: u32) {
        let tier = self.wchests[i].site.tier;
        self.wchests[i].opened = true;
        self.wchests[i].revealed = true;
        self.ev.chest_opened = true;
        self.ev.chest_tier = Some(tier);
        self.chests_opened += 1;
        // Contents: a card pick (two for a golden or a keyed one), healing and gold for the better ones.
        let picks = match tier {
            ChestTier::Common => 1,
            ChestTier::Rare => 1,
            ChestTier::Golden => 2,
        } + (richness > 1 && tier != ChestTier::Golden) as u32;
        self.pending_chests += picks;
        let gold = match tier {
            ChestTier::Common => 12,
            ChestTier::Rare => 35,
            ChestTier::Golden => 90,
        } * richness;
        self.gold += gold;
        if tier != ChestTier::Common {
            self.heal(0.3 * self.player.max_hp);
        }
        self.offer();
    }

    fn pick_kind(&mut self, secs: f32) -> EnemyKind {
        let w = kind_weights(secs);
        let sw = special_weights(secs);
        let pw = foes::weights(secs);
        let total: f32 = w.iter().sum::<f32>() + sw.iter().sum::<f32>() + pw.iter().sum::<f32>();
        let mut r = self.rng.f32() * total;
        for (i, k) in [EnemyKind::Skitter, EnemyKind::Brute, EnemyKind::Dasher, EnemyKind::Wisp].into_iter().enumerate() {
            if r < w[i] {
                return k;
            }
            r -= w[i];
        }
        for (i, k) in [EnemyKind::Bomber, EnemyKind::Splitter].into_iter().enumerate() {
            if r < sw[i] {
                return k;
            }
            r -= sw[i];
        }
        for (i, k) in PORTED.into_iter().enumerate() {
            if r < pw[i] {
                return k;
            }
            r -= pw[i];
        }
        EnemyKind::Skitter
    }

    // ---- enemies -----------------------------------------------------------------------------

    fn move_enemies(&mut self) {
        let p = self.player.pos;
        let secs = self.secs();
        let mask = self.mask.take();
        let lim = self.half - 100.0;
        self.alive_idx.clear();
        for i in 0..ENEMY_SLOTS {
            if !self.enemies[i].alive {
                continue;
            }
            self.alive_idx.push(i as u16);
            let mut fire: Option<(f32, usize)> = None; // (dmg, ring size or 0 = aimed)
            let mut blast = false;
            let mut summon = 0usize;
            let mut extra_aimed = false;
            if self.freeze {
                continue;
            }
            let big = self.enemies[i].kind.is_boss() || self.enemies[i].elite;
            if self.enemies[i].freeze > 0 {
                self.enemies[i].freeze -= 1;
                if !big {
                    self.enemies[i].vel = [0.0; 2];
                    continue;
                }
            }
            let e = &mut self.enemies[i];
            let to = [p[0] - e.pos[0], p[1] - e.pos[1]];
            let d = (to[0] * to[0] + to[1] * to[1]).sqrt().max(0.001);
            let dir = [to[0] / d, to[1] / d];
            let mut step = [0.0f32; 2];
            match e.kind {
                EnemyKind::Skitter | EnemyKind::Brute | EnemyKind::Splitter => {
                    step = [dir[0] * e.speed, dir[1] * e.speed];
                }
                EnemyKind::Dasher => match e.state {
                    0 => {
                        step = [dir[0] * e.speed, dir[1] * e.speed];
                        if e.timer > 0 {
                            e.timer -= 1;
                        } else if d < 850.0 {
                            e.state = 1;
                            e.timer = 12;
                        }
                    }
                    1 => {
                        e.timer -= 1;
                        if e.timer <= 0 {
                            e.state = 2;
                            e.timer = 14;
                            e.dash = dir;
                        }
                    }
                    _ => {
                        step = [e.dash[0] * DASH_SPEED, e.dash[1] * DASH_SPEED];
                        e.timer -= 1;
                        if e.timer <= 0 {
                            e.state = 0;
                            e.timer = 45;
                        }
                    }
                },
                EnemyKind::Bomber => {
                    if e.state == 0 {
                        step = [dir[0] * e.speed, dir[1] * e.speed];
                        if d < 250.0 {
                            e.state = 1;
                            e.timer = BOMB_FUSE;
                        }
                    } else {
                        e.timer -= 1;
                        if e.timer <= 0 {
                            blast = true;
                        }
                    }
                }
                EnemyKind::Wisp => {
                    if d > 1000.0 {
                        step = [dir[0] * e.speed, dir[1] * e.speed];
                    } else if d < 650.0 {
                        step = [-dir[0] * e.speed * 0.8, -dir[1] * e.speed * 0.8];
                    } else {
                        step = [-dir[1] * e.speed * 0.5, dir[0] * e.speed * 0.5];
                    }
                    e.timer -= 1;
                    if e.timer <= 0 {
                        e.timer = 75;
                        fire = Some((7.0, 0));
                    }
                }
                EnemyKind::King => {
                    step = [dir[0] * e.speed, dir[1] * e.speed];
                    e.timer -= 1;
                    if e.timer <= 0 {
                        e.timer = 100;
                        fire = Some((9.0, 10));
                    }
                }
                EnemyKind::Chomp => match e.state {
                    0 => {
                        step = [dir[0] * e.speed, dir[1] * e.speed];
                        e.timer -= 1;
                        if e.timer <= 0 && d < 1300.0 {
                            e.state = 1;
                            e.timer = 30;
                        }
                    }
                    1 => {
                        e.timer -= 1;
                        if e.timer <= 0 {
                            e.state = 2;
                            e.timer = 26;
                            e.dash = dir;
                        }
                    }
                    2 => {
                        step = [e.dash[0] * 40.0, e.dash[1] * 40.0];
                        e.timer -= 1;
                        if e.timer <= 0 {
                            e.state = 3;
                            e.timer = 40;
                            fire = Some((8.0, 8));
                        }
                    }
                    _ => {
                        e.timer -= 1;
                        if e.timer <= 0 {
                            e.state = 0;
                            e.timer = 70;
                        }
                    }
                },
                EnemyKind::Tyrant => {
                    let f = e.hp / e.max_hp.max(1.0);
                    let phase = if f > 0.66 { 0 } else if f > 0.33 { 1 } else { 2 };
                    let rage = if secs >= WIN_SECS { 1.5 } else { 1.0 };
                    e.aux = e.aux.wrapping_add(1);
                    match e.state {
                        0 => {
                            step = [dir[0] * e.speed * (1.0 + 0.2 * phase as f32) * rage, dir[1] * e.speed * (1.0 + 0.2 * phase as f32) * rage];
                            e.timer -= 1;
                            if e.timer <= 0 {
                                e.timer = (110 - 25 * phase) as i16;
                                fire = Some((11.0, 12 + 4 * phase as usize));
                                extra_aimed = phase >= 1;
                            }
                            if phase >= 1 && e.aux % 160 == 0 {
                                summon = 5;
                            }
                            if phase >= 2 && e.aux % 210 == 0 {
                                e.state = 1;
                                e.timer = 26;
                            }
                        }
                        1 => {
                            e.timer -= 1;
                            if e.timer <= 0 {
                                e.state = 2;
                                e.timer = 20;
                                e.dash = dir;
                            }
                        }
                        _ => {
                            step = [e.dash[0] * 30.0, e.dash[1] * 30.0];
                            e.timer -= 1;
                            if e.timer <= 0 {
                                e.state = 0;
                                e.timer = 50;
                                fire = Some((11.0, 16));
                            }
                        }
                    }
                }
                _ => {
                    let (st, f) = foes::step(e, dir, d, self.tick);
                    step = st;
                    fire = f;
                }
            }
            // Frozen bosses and poisoned foes crawl.
            if (e.freeze > 0 && big) || e.poison > 0 && !big {
                step = [step[0] * 0.5, step[1] * 0.5];
            }
            e.vel = step;
            // Blocked ground (a pit, a cliff): slide along it on one axis or stay put.
            let from = e.pos;
            let next = [(from[0] + step[0]).clamp(-lim, lim), (from[1] + step[1]).clamp(-lim, lim)];
            let blocked = |q: [f32; 2]| mask.as_ref().is_some_and(|m| !m.at(q));
            e.pos = if !blocked(next) {
                next
            } else if !blocked([next[0], from[1]]) {
                [next[0], from[1]]
            } else if !blocked([from[0], next[1]]) {
                [from[0], next[1]]
            } else {
                from
            };
            let (origin, radius, contact, elite) = (e.pos, e.radius, e.contact, e.elite);
            let dmg_mult = damage_mult(secs) * self.tier.damage;
            if let Some((dmg, ring)) = fire {
                if ring == 0 {
                    self.fire_orb(origin, dir, 13.0, dmg * dmg_mult);
                } else {
                    let phase = self.rng.range(0.0, std::f32::consts::TAU);
                    for k in 0..ring {
                        let a = phase + k as f32 / ring as f32 * std::f32::consts::TAU;
                        self.fire_orb(origin, [a.cos(), a.sin()], 9.0, dmg * dmg_mult);
                    }
                    if extra_aimed {
                        for s in [-0.2f32, 0.0, 0.2] {
                            let (sn, cs) = s.sin_cos();
                            self.fire_orb(origin, [dir[0] * cs - dir[1] * sn, dir[0] * sn + dir[1] * cs], 13.0, dmg * dmg_mult);
                        }
                    }
                }
            }
            for k in 0..summon {
                let a = k as f32 / summon as f32 * std::f32::consts::TAU;
                let q = [origin[0] + a.cos() * (radius + 120.0), origin[1] + a.sin() * (radius + 120.0)];
                if self.walkable(q) {
                    self.spawn_enemy_force(EnemyKind::Skitter, q, secs);
                }
            }
            if blast {
                self.explode(i, contact, elite);
            }
        }
        self.mask = mask;
    }

    /// A bomber goes off: damage to the player in reach, a ring, and the bomber is gone (no reward).
    fn explode(&mut self, i: usize, contact: f32, elite: bool) {
        let pos = self.enemies[i].pos;
        let radius = if elite { 420.0 } else { 310.0 };
        self.enemies[i].alive = false;
        self.alive_enemies = self.alive_enemies.saturating_sub(1);
        self.add_ring(RingKind::Blast, pos, radius, 10);
        self.ev.blast = true;
        self.ev.blast_pos = pos;
        let reach = radius + PLAYER_RADIUS;
        if self.player.height < 200.0 && dist2(self.player.pos, pos) < reach * reach {
            self.hurt_player(contact * 2.6);
        }
    }

    fn fire_orb(&mut self, pos: [f32; 2], dir: [f32; 2], speed: f32, dmg: f32) {
        if let Some(o) = self.orbs.iter_mut().find(|o| !o.alive) {
            *o = Orb { alive: true, pos, vel: [dir[0] * speed, dir[1] * speed], life: 110, dmg };
        }
    }

    /// Push overlapping enemies apart (heavier ones move less). Uses the spatial hash: each foe only
    /// looks at the ones in the cells around it.
    fn separate(&mut self) {
        let mut near = std::mem::take(&mut self.near);
        let max_r = self.grid.max_r;
        for a in 0..self.alive_idx.len() {
            let i = self.alive_idx[a] as usize;
            let (pi, mi) = (self.enemies[i].pos, self.enemies[i].radius);
            self.grid.near(pi, (mi + max_r) * 0.8 + 8.0, &mut near);
            for &jj in near.iter() {
                let j = jj as usize;
                if j <= i || !self.enemies[j].alive {
                    continue;
                }
                let (pi, pj) = (self.enemies[i].pos, self.enemies[j].pos);
                let (dx, dz) = (pj[0] - pi[0], pj[1] - pi[1]);
                let mj = self.enemies[j].radius;
                let mi = self.enemies[i].radius;
                let reach = (mi + mj) * 0.8;
                let d2 = dx * dx + dz * dz;
                if d2 >= reach * reach {
                    continue;
                }
                let d = d2.sqrt();
                let (nx, nz) = if d > 0.01 { (dx / d, dz / d) } else { (1.0, 0.0) };
                let push = (reach - d) * 0.35;
                let (fi, fj) = (mj / (mi + mj), mi / (mi + mj));
                let (ni, nj) = ([pi[0] - nx * push * fi * 2.0, pi[1] - nz * push * fi * 2.0], [pj[0] + nx * push * fj * 2.0, pj[1] + nz * push * fj * 2.0]);
                // Nobody is shoved over a cliff or out of the arena.
                if self.walkable(ni) {
                    self.enemies[i].pos = ni;
                }
                if self.walkable(nj) {
                    self.enemies[j].pos = nj;
                }
            }
        }
        self.near = near;
    }

    // ---- attacks -----------------------------------------------------------------------------

    fn attack(&mut self) {
        let p = self.player.pos;
        // Star volley at the nearest foes.
        self.fire_cd = self.fire_cd.saturating_sub(1);
        if self.fire_cd == 0 && self.stats.bolts == 0 {
            self.fire_cd = 10;
        }
        if self.fire_cd == 0 {
            let mut best = [(f32::INFINITY, u16::MAX); 6];
            let mut found = 0;
            for &i in &self.alive_idx {
                let e = &self.enemies[i as usize];
                if !e.alive {
                    continue;
                }
                let d = dist2(e.pos, p);
                if d > BOLT_RANGE * BOLT_RANGE || d >= best[best.len() - 1].0 {
                    continue;
                }
                let mut k = best.len() - 1;
                while k > 0 && best[k - 1].0 > d {
                    best[k] = best[k - 1];
                    k -= 1;
                }
                best[k] = (d, i);
                found = (found + 1).min(best.len());
            }
            if found > 0 {
                let volley = (self.stats.bolts as usize).min(BOLT_SLOTS);
                let crit_roll = self.stats.crit;
                for n in 0..volley {
                    let target = best[n % found].1 as usize;
                    let t = self.enemies[target].pos;
                    let mut a = (t[1] - p[1]).atan2(t[0] - p[0]);
                    // Extra stars on a shared target fan out.
                    let extra = n / found;
                    if extra > 0 {
                        a += (if extra % 2 == 1 { 1.0 } else { -1.0 }) * 0.12 * ((extra + 1) / 2) as f32;
                    }
                    let crit = self.rng.chance(crit_roll);
                    let dmg = self.stats.damage * self.stats.star_dmg * if crit { 2.0 } else { 1.0 };
                    self.fire_bolt(p, a, dmg, crit);
                }
                self.volley_n += 1;
                if self.stats.star_storm && self.volley_n % 4 == 0 {
                    let phase = self.rng.range(0.0, std::f32::consts::TAU);
                    for k in 0..10 {
                        let a = phase + k as f32 / 10.0 * std::f32::consts::TAU;
                        self.fire_bolt(p, a, 0.6 * self.stats.damage, false);
                    }
                }
                self.fire_cd = self.stats.interval;
                self.ev.fired = true;
            } else {
                self.fire_cd = 4;
            }
        }
        // Nova pulse.
        if self.stats.nova > 0 {
            self.nova_cd = self.nova_cd.saturating_sub(1);
            if self.nova_cd == 0 {
                let lvl = self.stats.nova as f32;
                let sn = self.stats.supernova;
                self.nova_cd = ((130.0 - 18.0 * lvl) * if sn { 0.7 } else { 1.0 }) as u32;
                let radius = (380.0 + 50.0 * lvl) * self.stats.area * if sn { 1.6 } else { 1.0 };
                let dmg = 0.9 * self.stats.damage * (1.0 + 0.25 * lvl) * if sn { 1.5 } else { 1.0 };
                self.add_ring(RingKind::Nova, p, radius, 12);
                self.ev.nova = true;
                self.area_damage(p, radius, dmg, if sn { 50.0 } else { 0.0 });
            }
        }
        // Spin slash.
        if self.stats.whirl > 0 {
            self.whirl_cd = self.whirl_cd.saturating_sub(1);
            if self.whirl_cd == 0 {
                self.whirl_cd = self.stats.whirl_interval.max(12);
                let r = self.stats.whirl_radius;
                let dmg = self.stats.damage * (1.2 + 0.2 * self.stats.whirl as f32);
                self.add_ring(RingKind::Slash, p, r, 7);
                self.ev.slash = true;
                self.area_damage(p, r, dmg, 45.0);
            }
        }
        // Boomerangs.
        if self.stats.boomerangs > 0 {
            self.boom_cd = self.boom_cd.saturating_sub(1);
            if self.boom_cd == 0 {
                self.boom_cd = self.stats.boom_interval.max(20);
                self.throw_boomerangs();
            }
        }
        // Lightning.
        if self.stats.strikes > 0 {
            self.strike_cd = self.strike_cd.saturating_sub(1);
            if self.strike_cd == 0 {
                self.strike_cd = self.stats.strike_interval.max(30);
                self.thunder();
            }
        }
    }

    fn throw_boomerangs(&mut self) {
        let p = self.player.pos;
        // Aim at the nearest foe; with no foe in range the throw is saved for the next tick.
        let mut best = (f32::INFINITY, [0.0f32; 2]);
        for &i in &self.alive_idx {
            let e = &self.enemies[i as usize];
            let d = dist2(e.pos, p);
            if e.alive && d < best.0 {
                best = (d, e.pos);
            }
        }
        if !best.0.is_finite() || best.0 > 1700.0 * 1700.0 {
            self.boom_cd = 6;
            return;
        }
        let base = (best.1[1] - p[1]).atan2(best.1[0] - p[0]);
        let n = self.stats.boomerangs as usize;
        let size = if self.stats.comet { 1.5 } else { 1.0 };
        let dmg = 0.9 * self.stats.damage * (1.0 + 0.15 * self.up.level(Upgrade::Boomerang) as f32);
        for k in 0..n {
            let spread = if n == 1 { 0.0 } else { (k as f32 / (n - 1) as f32 - 0.5) * 1.1 };
            let a = base + spread;
            if let Some(b) = self.booms.iter_mut().find(|b| !b.alive) {
                *b = Boom { alive: true, pos: p, prev: p, vel: [a.cos() * 62.0, a.sin() * 62.0], age: 0, dmg, radius: 85.0 * self.stats.area.sqrt() * size };
            }
        }
        self.ev.boom_throw = true;
    }

    fn update_booms(&mut self) {
        let p = self.player.pos;
        let mut near = std::mem::take(&mut self.near);
        for bi in 0..BOOM_SLOTS {
            if !self.booms[bi].alive {
                continue;
            }
            {
                let b = &mut self.booms[bi];
                b.age += 1;
                if b.age < 20 {
                    b.vel[0] *= 0.955;
                    b.vel[1] *= 0.955;
                } else {
                    let to = [p[0] - b.pos[0], p[1] - b.pos[1]];
                    let d = (to[0] * to[0] + to[1] * to[1]).sqrt().max(1.0);
                    if d < 90.0 && b.age > 24 || b.age > 150 {
                        b.alive = false;
                        continue;
                    }
                    b.vel[0] += to[0] / d * 4.0;
                    b.vel[1] += to[1] / d * 4.0;
                    let sp = (b.vel[0] * b.vel[0] + b.vel[1] * b.vel[1]).sqrt();
                    if sp > 52.0 {
                        b.vel[0] *= 52.0 / sp;
                        b.vel[1] *= 52.0 / sp;
                    }
                }
                b.pos[0] += b.vel[0];
                b.pos[1] += b.vel[1];
            }
            let b = self.booms[bi];
            self.grid.near(b.pos, b.radius + self.grid.max_r + 70.0, &mut near);
            for &ii in near.iter() {
                let i = ii as usize;
                let e = &self.enemies[i];
                if !e.alive || e.boom_cd > 0 {
                    continue;
                }
                let reach = e.radius + b.radius;
                if dist2(e.pos, b.pos) > reach * reach {
                    continue;
                }
                self.enemies[i].boom_cd = 8;
                self.damage_enemy(i, b.dmg);
            }
        }
        self.near = near;
    }

    /// Lightning: strike random foes near the player, each arcing on to its neighbours.
    fn thunder(&mut self) {
        let p = self.player.pos;
        let mut near = std::mem::take(&mut self.near);
        self.grid.near(p, 1500.0, &mut near);
        near.retain(|&i| self.enemies[i as usize].alive && dist2(self.enemies[i as usize].pos, p) < 1500.0 * 1500.0);
        let mut struck = 0;
        if !near.is_empty() {
            for _ in 0..self.stats.strikes {
                let i = near[self.rng.below(near.len() as u32) as usize] as usize;
                if !self.enemies[i].alive {
                    continue;
                }
                let crit = self.rng.chance(self.stats.crit);
                let dmg = 2.6 * self.stats.damage * if crit { 2.0 } else { 1.0 };
                let at = self.enemies[i].pos;
                if (struck as usize) < NUM_STRIKES {
                    self.ev.strike_pos[struck as usize] = at;
                    struck += 1;
                }
                self.add_ring(RingKind::Strike, at, 170.0, 8);
                self.damage_enemy_ex(i, dmg, crit);
                // Arcs: the nearest others within reach take half.
                let mut left = self.stats.arcs;
                let mut tries = 0;
                while left > 0 && tries < 6 && !near.is_empty() {
                    tries += 1;
                    let j = near[self.rng.below(near.len() as u32) as usize] as usize;
                    if j != i && self.enemies[j].alive && dist2(self.enemies[j].pos, at) < 420.0 * 420.0 {
                        left -= 1;
                        let at2 = self.enemies[j].pos;
                        self.add_ring(RingKind::Strike, at2, 110.0, 6);
                        self.damage_enemy_ex(j, dmg * 0.5, false);
                    }
                }
            }
        }
        self.ev.strike_n = struck;
        self.ev.thunder = struck > 0;
        self.near = near;
    }

    /// Dive Strike: while diving, slide-kicking or jump-kicking, foes in the way take damage and a shove.
    fn rush(&mut self, rushing: bool) {
        if !rushing || self.stats.dive == 0 || self.tick % 3 != 0 {
            return;
        }
        let lvl = self.stats.dive as f32;
        let dmg = self.stats.damage * (1.4 + 0.7 * lvl);
        let p = self.player.pos;
        self.ev.rush_hit = true;
        self.area_damage(p, (200.0 + 20.0 * lvl) * self.stats.area.sqrt(), dmg, 60.0);
    }

    fn fire_bolt(&mut self, from: [f32; 2], angle: f32, dmg: f32, crit: bool) {
        let spec = arms::BoltSpec { kind: BoltKind::Star, dmg, speed: BOLT_SPEED, life: (BOLT_RANGE / BOLT_SPEED) as u16, pierce: self.stats.pierce, radius: 42.0 * self.stats.area.sqrt(), blast: 0.0, crit, aim: u16::MAX };
        self.spawn_bolt(from, angle, spec);
    }

    fn update_bolts(&mut self) {
        let half = self.half;
        let mut near = std::mem::take(&mut self.near);
        for bi in 0..BOLT_SLOTS {
            if !self.bolts[bi].alive {
                continue;
            }
            if self.bolts[bi].kind == BoltKind::Missile {
                self.steer_missile(bi);
            }
            {
                let b = &mut self.bolts[bi];
                b.pos[0] += b.vel[0];
                b.pos[1] += b.vel[1];
                b.life = b.life.saturating_sub(1);
                if b.life == 0 || b.pos[0].abs() > half || b.pos[1].abs() > half {
                    b.alive = false;
                    let (blast, pos, dmg, kind) = (b.blast, b.pos, b.dmg, b.kind);
                    // A fireball or missile that finds nothing still goes off where it ends.
                    if blast > 0.0 && pos[0].abs() <= half && pos[1].abs() <= half {
                        self.bolt_blast(pos, blast, dmg, kind);
                    }
                    continue;
                }
            }
            let b = self.bolts[bi];
            // Look in the cells the bolt's step crosses (it moves up to 42 units a tick).
            self.grid.near(b.pos, b.radius + self.grid.max_r + 70.0, &mut near);
            for &ii in near.iter() {
                let i = ii as usize;
                let b = self.bolts[bi];
                let e = &self.enemies[i];
                if !self.bolts[bi].alive || !e.alive || b.recent.contains(&(i as u16)) {
                    continue;
                }
                let reach = e.radius + b.radius;
                if dist2(e.pos, b.pos) > reach * reach {
                    continue;
                }
                if b.blast > 0.0 {
                    self.bolts[bi].alive = false;
                    self.bolt_blast(b.pos, b.blast, b.dmg, b.kind);
                    break;
                }
                self.damage_enemy_ex(i, b.dmg, b.crit);
                let e = &mut self.enemies[i];
                if e.alive && !e.kind.is_boss() && b.kind != BoltKind::Beam {
                    e.pos[0] += b.vel[0] * 0.2;
                    e.pos[1] += b.vel[1] * 0.2;
                }
                let bolt = &mut self.bolts[bi];
                if bolt.pierce == 0 {
                    bolt.alive = false;
                    break;
                }
                bolt.pierce -= 1;
                bolt.recent.rotate_right(1);
                bolt.recent[0] = i as u16;
            }
        }
        self.near = near;
    }

    fn update_orbit(&mut self) {
        let n = (self.stats.orbit as usize).min(ORBIT_MAX);
        self.orbit_count = n;
        if n == 0 {
            return;
        }
        self.orbit_angle += 0.13;
        let p = self.player.pos;
        let radius = ORBIT_RADIUS * self.stats.area.sqrt();
        let hit_r = 62.0 * self.stats.area.sqrt();
        let dmg = 0.6 * self.stats.damage;
        let mut near = std::mem::take(&mut self.near);
        for e in self.enemies.iter_mut().filter(|e| e.alive) {
            e.orbit_cd = e.orbit_cd.saturating_sub(1);
        }
        for k in 0..n {
            let a = self.orbit_angle + k as f32 / n as f32 * std::f32::consts::TAU;
            let q = [p[0] + a.cos() * radius, p[1] + a.sin() * radius];
            self.orbit_pos[k] = q;
            self.grid.near(q, hit_r + self.grid.max_r + 40.0, &mut near);
            for &ii in near.iter() {
                let i = ii as usize;
                let e = &self.enemies[i];
                if !e.alive || e.orbit_cd > 0 {
                    continue;
                }
                let reach = e.radius + hit_r;
                if dist2(e.pos, q) < reach * reach {
                    self.enemies[i].orbit_cd = 8;
                    self.damage_enemy(i, dmg);
                }
            }
        }
        self.near = near;
    }

    fn quake(&mut self) {
        let lvl = self.stats.quake as f32;
        let es = self.stats.earthshaker;
        let radius = (420.0 + 80.0 * lvl) * if es { 1.6 } else { 1.0 };
        let dmg = self.stats.damage * (2.2 + 0.8 * lvl);
        let c = self.player.pos;
        self.add_ring(RingKind::Quake, c, radius, 10);
        self.ev.quake = true;
        self.area_damage(c, radius, dmg, 70.0);
        if es {
            self.echo = (9, c, radius * 1.15, dmg * 0.7);
        }
    }

    /// Earthshaker's second roll of the quake.
    fn update_echo(&mut self) {
        if self.echo.0 == 0 {
            return;
        }
        self.echo.0 -= 1;
        if self.echo.0 == 0 {
            let (_, c, r, d) = self.echo;
            self.add_ring(RingKind::Quake, c, r, 10);
            self.ev.quake = true;
            self.area_damage(c, r, d, 80.0);
        }
    }

    /// Damage everything within `radius` of `c`, shoving survivors outward by `knock`.
    fn area_damage(&mut self, c: [f32; 2], radius: f32, dmg: f32, knock: f32) {
        for k in 0..self.alive_idx.len() {
            let i = self.alive_idx[k] as usize;
            let e = &self.enemies[i];
            if !e.alive {
                continue;
            }
            let reach = radius + e.radius * 0.5;
            let d2 = dist2(e.pos, c);
            if d2 > reach * reach {
                continue;
            }
            self.damage_enemy(i, dmg);
            if knock > 0.0 && self.enemies[i].alive && !self.enemies[i].kind.is_boss() && !self.enemies[i].elite {
                let d = d2.sqrt().max(1.0);
                let e = &mut self.enemies[i];
                e.pos[0] += (e.pos[0] - c[0]) / d * knock;
                e.pos[1] += (e.pos[1] - c[1]) / d * knock;
            }
        }
    }

    fn add_ring(&mut self, kind: RingKind, center: [f32; 2], max: f32, life: u16) {
        if let Some(r) = self.rings.iter_mut().find(|r| !r.alive) {
            *r = Ring { alive: true, kind, center, max, age: 0, life, dir: 0.0, half: 0.0 };
        }
    }

    fn update_rings(&mut self) {
        for r in self.rings.iter_mut().filter(|r| r.alive) {
            r.age += 1;
            if r.age >= r.life {
                r.alive = false;
            }
        }
    }

    fn update_boss_bar(&mut self) {
        let mut best: Option<(EnemyKind, f32, f32)> = None;
        for &i in &self.alive_idx {
            let e = &self.enemies[i as usize];
            if e.alive && e.kind.is_boss() && best.is_none_or(|b| e.max_hp > b.2) {
                best = Some((e.kind, (e.hp / e.max_hp.max(1.0)).clamp(0.0, 1.0), e.max_hp));
            }
        }
        self.boss_bar = best.map(|b| (b.0, b.1));
    }

    /// Hit an enemy; handles the kill and its drops.
    pub fn damage_enemy(&mut self, i: usize, dmg: f32) {
        self.damage_enemy_ex(i, dmg, false);
    }

    /// Damage over time: no number, no flash.
    pub(super) fn damage_quiet(&mut self, i: usize, dmg: f32) {
        let e = &mut self.enemies[i];
        if !e.alive {
            return;
        }
        e.hp -= dmg * if e.freeze > 0 { arms::SHATTER } else { 1.0 };
        if e.hp <= 0.0 {
            self.kill(i);
        }
    }

    pub fn damage_enemy_ex(&mut self, i: usize, dmg: f32, crit: bool) {
        let e = &mut self.enemies[i];
        if !e.alive {
            return;
        }
        // Shatter: frozen foes take extra damage from everything.
        let dmg = if e.freeze > 0 { dmg * arms::SHATTER } else { dmg };
        e.hp -= dmg;
        e.flash = 4;
        let at = e.pos;
        let dead = e.hp <= 0.0;
        if crit {
            self.ev.crit = true;
        }
        self.num(at, dmg, crit, 0);
        if !dead {
            return;
        }
        self.kill(i);
    }

    fn kill(&mut self, i: usize) {
        let e = &mut self.enemies[i];
        e.alive = false;
        let (kind, pos, elite) = (e.kind, e.pos, e.elite);
        self.alive_enemies = self.alive_enemies.saturating_sub(1);
        self.kills += 1;
        self.ev.kills += 1;
        self.ev.last_kill = pos;
        let mut xp = enemy_stats(kind).xp;
        if elite {
            xp = xp.saturating_mul(6).max(18);
        }
        match kind {
            EnemyKind::King | EnemyKind::Chomp | EnemyKind::Tyrant => {
                self.ev.boss_killed = true;
                for k in 0..5 {
                    let a = k as f32 * 1.2566;
                    self.drop_gem(GemKind::Xp, [pos[0] + a.cos() * 140.0, pos[1] + a.sin() * 140.0], xp / 5);
                }
                self.drop_gem(GemKind::Heart, pos, 0);
                self.drop_special(GemKind::Chest, [pos[0], pos[1] + 120.0]);
                if kind == EnemyKind::Tyrant {
                    self.tyrant_dead = true;
                    self.drop_special(GemKind::Chest, [pos[0] + 160.0, pos[1]]);
                }
            }
            _ if elite => {
                self.ev.elite_killed = true;
                if self.explore {
                    self.keys += 1;
                    self.ev.key_found = true;
                }
                for k in 0..3 {
                    let a = k as f32 * 2.0944;
                    self.drop_gem(GemKind::Xp, [pos[0] + a.cos() * 90.0, pos[1] + a.sin() * 90.0], xp / 3);
                }
                self.drop_special(GemKind::Chest, pos);
                if self.rng.chance(0.4) {
                    self.drop_special(GemKind::Heart, [pos[0] + 60.0, pos[1]]);
                }
            }
            _ => {
                self.drop_gem(GemKind::Xp, pos, xp);
                let r = self.rng.f32();
                if r < 0.02 {
                    self.drop_special(GemKind::Heart, [pos[0] + 40.0, pos[1]]);
                } else if r < 0.026 {
                    self.drop_special(GemKind::Magnet, [pos[0], pos[1] + 40.0]);
                } else if r < 0.031 {
                    self.drop_special(GemKind::Bomb, [pos[0] - 40.0, pos[1]]);
                }
            }
        }
        if kind == EnemyKind::Splitter {
            self.ev.split = true;
            let secs = self.secs();
            for k in 0..3 {
                let a = k as f32 * 2.0944 + 0.5;
                let q = [pos[0] + a.cos() * 80.0, pos[1] + a.sin() * 80.0];
                if self.walkable(q) {
                    self.spawn_enemy_force(EnemyKind::Skitter, q, secs);
                }
            }
        }
    }

    // ---- hazards, gems -----------------------------------------------------------------------

    fn update_orbs(&mut self) {
        let p = self.player.pos;
        let h = self.player.height;
        let half = self.half;
        let mut hit = 0.0;
        for o in self.orbs.iter_mut().filter(|o| o.alive) {
            o.pos[0] += o.vel[0];
            o.pos[1] += o.vel[1];
            o.life = o.life.saturating_sub(1);
            if o.life == 0 || o.pos[0].abs() > half + 200.0 || o.pos[1].abs() > half + 200.0 {
                o.alive = false;
                continue;
            }
            let reach = 40.0 + PLAYER_RADIUS;
            if h < 130.0 && dist2(o.pos, p) < reach * reach {
                o.alive = false;
                hit += o.dmg;
            }
        }
        if hit > 0.0 {
            self.hurt_player(hit);
        }
    }

    fn contact(&mut self, _secs: f32) {
        if self.player.iframes > 0 {
            return;
        }
        let p = self.player.pos;
        let h = self.player.height;
        let mut dmg = 0.0f32;
        for &i in &self.alive_idx {
            let e = &self.enemies[i as usize];
            if !e.alive || (e.freeze > 0 && !e.kind.is_boss() && !e.elite) {
                continue;
            }
            let s = enemy_stats(e.kind);
            let reach = e.radius.max(s.radius) + PLAYER_RADIUS;
            if h < s.height * 0.85 && dist2(e.pos, p) < reach * reach {
                dmg = dmg.max(e.contact);
            }
        }
        if dmg > 0.0 {
            self.hurt_player(dmg);
        }
    }

    fn drop_gem(&mut self, kind: GemKind, pos: [f32; 2], value: u16) {
        let value = if kind == GemKind::Xp { value.max(1) } else { 0 };
        if let Some(g) = self.gems.iter_mut().find(|g| !g.alive) {
            *g = Gem { alive: true, kind, pos, value, magnet: false, speed: 10.0 };
            return;
        }
        // Full: fold the XP into the farthest gem instead of dropping it.
        if kind == GemKind::Xp {
            let p = self.player.pos;
            let mut far = (0.0f32, usize::MAX);
            for (i, g) in self.gems.iter().enumerate() {
                if g.kind == GemKind::Xp {
                    let d = dist2(g.pos, p);
                    if d > far.0 {
                        far = (d, i);
                    }
                }
            }
            if far.1 != usize::MAX {
                let g = &mut self.gems[far.1];
                g.value = g.value.saturating_add(value);
            }
        }
    }

    /// A chest, heart, magnet or bomb: these must not be lost when the field is full of gems, so
    /// the farthest XP gem is merged into another one to make room.
    fn drop_special(&mut self, kind: GemKind, pos: [f32; 2]) {
        if !self.gems.iter().any(|g| !g.alive) {
            let p = self.player.pos;
            let mut far = (0.0f32, usize::MAX);
            let mut other = usize::MAX;
            for (i, g) in self.gems.iter().enumerate() {
                if g.kind == GemKind::Xp {
                    let d = dist2(g.pos, p);
                    if d > far.0 {
                        far = (d, i);
                    }
                    other = i;
                }
            }
            if far.1 == usize::MAX {
                return;
            }
            let v = self.gems[far.1].value;
            self.gems[far.1].alive = false;
            if other != far.1 && other != usize::MAX {
                self.gems[other].value = self.gems[other].value.saturating_add(v);
            }
        }
        if let Some(g) = self.gems.iter_mut().find(|g| !g.alive) {
            *g = Gem { alive: true, kind, pos, value: 0, magnet: false, speed: 10.0 };
        }
    }

    fn update_gems(&mut self) {
        let p = self.player.pos;
        let magnet = self.stats.magnet;
        let mut xp = 0u32;
        let mut hearts = 0u16;
        let mut picked = 0u16;
        let (mut chests, mut vacuum, mut bomb) = (0u32, false, false);
        for g in self.gems.iter_mut().filter(|g| g.alive) {
            let d2 = dist2(g.pos, p);
            if g.kind == GemKind::Chest {
                // A chest does not fly to you: you walk into it.
                if d2 < 130.0 * 130.0 {
                    g.alive = false;
                    chests += 1;
                }
                continue;
            }
            if !g.magnet && d2 < magnet * magnet {
                g.magnet = true;
            }
            if !g.magnet {
                continue;
            }
            let d = d2.sqrt();
            g.speed = (g.speed + 2.5).min(55.0);
            if d <= g.speed.max(70.0) {
                g.alive = false;
                picked += 1;
                match g.kind {
                    GemKind::Xp => xp += g.value as u32,
                    GemKind::Heart => hearts += 1,
                    GemKind::Magnet => vacuum = true,
                    GemKind::Bomb => bomb = true,
                    GemKind::Chest => {}
                }
            } else {
                g.pos[0] += (p[0] - g.pos[0]) / d * g.speed;
                g.pos[1] += (p[1] - g.pos[1]) / d * g.speed;
            }
        }
        self.ev.gems = picked;
        self.ev.hearts = hearts;
        if vacuum {
            // Every gem on the field flies to the player, faster and faster.
            self.ev.vacuum = true;
            for g in self.gems.iter_mut().filter(|g| g.alive && g.kind != GemKind::Chest) {
                g.magnet = true;
                g.speed = g.speed.max(28.0);
            }
        }
        if bomb {
            self.ev.bomb = true;
            let dmg = (6.0 * self.stats.damage).max(60.0);
            self.add_ring(RingKind::Blast, p, 1100.0, 12);
            self.area_damage(p, 1100.0, dmg, 140.0);
        }
        for _ in 0..hearts {
            self.heal(0.25 * self.player.max_hp);
            let at = self.player.pos;
            self.num(at, 0.25 * self.player.max_hp, false, 2);
        }
        if chests > 0 {
            self.ev.chest_opened = true;
            self.chests_opened += chests;
            self.pending_chests += chests;
        }
        if xp > 0 {
            let gain = ((xp as f32) * self.stats.xp_mult * self.tier.xp).round().max(1.0) as u32;
            self.add_xp(gain);
        } else if chests > 0 {
            self.offer();
        }
    }
}

impl EnemyKind {
    /// Bosses ignore the live-foe cap and are never shoved around.
    pub fn is_boss(self) -> bool {
        matches!(self, EnemyKind::King | EnemyKind::Chomp | EnemyKind::Tyrant)
    }
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dz) = (a[0] - b[0], a[1] - b[1]);
    dx * dx + dz * dz
}

/// Format seconds as `M:SS`.
pub fn clock(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at(x: f32, z: f32) -> PlayerFrame {
        PlayerFrame { pos: [x, z], height: 0.0, pound_land: false, ..Default::default() }
    }

    #[test]
    fn rng_is_reproducible_and_uniform_enough() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        let xs: Vec<u32> = (0..32).map(|_| a.next_u32()).collect();
        assert_eq!(xs, (0..32).map(|_| b.next_u32()).collect::<Vec<_>>());
        assert_ne!(Rng::new(8).next_u32(), xs[0]);
        let mut r = Rng::new(1);
        let mean: f32 = (0..4000).map(|_| r.f32()).sum::<f32>() / 4000.0;
        assert!((mean - 0.5).abs() < 0.03, "{mean}");
        for _ in 0..1000 {
            assert!(r.below(5) < 5);
        }
    }

    #[test]
    fn xp_curve_grows_and_matches_the_formula() {
        assert_eq!(xp_to_next(1), 8);
        assert_eq!(xp_to_next(2), 13);
        assert_eq!(xp_to_next(10), 85);
        for l in 1..60 {
            assert!(xp_to_next(l + 1) > xp_to_next(l));
        }
    }

    #[test]
    fn wave_scaling_escalates_and_respects_the_cap() {
        assert!(hp_mult(300.0) > hp_mult(60.0) && hp_mult(60.0) > hp_mult(0.0));
        assert_eq!(hp_mult(0.0), 1.0);
        assert!((hp_mult(300.0) - 2.5).abs() < 1e-3);
        assert!(speed_mult(3600.0) <= 1.4 + 0.15 + 1e-6);
        assert!(damage_mult(600.0) > damage_mult(0.0));
        assert_eq!(target_live(0.0, 200), 10);
        assert!(target_live(120.0, 200) > target_live(60.0, 200));
        assert_eq!(target_live(10_000.0, 160), 160);
        // New enemy types phase in.
        assert_eq!(kind_weights(10.0), [10.0, 0.0, 0.0, 0.0]);
        let late = kind_weights(300.0);
        assert!(late.iter().all(|w| *w > 0.0));
    }

    #[test]
    fn rolls_are_distinct_unmaxed_and_gated() {
        let mut rng = Rng::new(3);
        let mut owned = Upgrades::default();
        for _ in 0..200 {
            let c = roll_choices(&mut rng, &owned, 1);
            assert_eq!(c.len, 3);
            let s = c.as_slice();
            assert!(s[0] != s[1] && s[1] != s[2] && s[0] != s[2]);
            for u in s {
                assert!(u.info().min_level <= 1, "{u:?} offered too early");
            }
        }
        // Rare ones appear once the player is strong enough.
        let mut seen_rare = false;
        for _ in 0..400 {
            if roll_choices(&mut rng, &owned, 10).as_slice().iter().any(|u| u.info().rare) {
                seen_rare = true;
            }
        }
        assert!(seen_rare);
        // Maxed upgrades never come back.
        for _ in 0..8 {
            owned.take(Upgrade::Damage);
        }
        assert!(owned.maxed(Upgrade::Damage));
        assert!(!owned.take(Upgrade::Damage));
        for _ in 0..200 {
            assert!(!roll_choices(&mut rng, &owned, 10).as_slice().contains(&Upgrade::Damage));
        }
    }

    #[test]
    fn rolls_are_deterministic_and_run_dry_gracefully() {
        let owned = Upgrades::default();
        let a: Vec<_> = (0..20).map(|_| roll_choices(&mut Rng::new(9), &owned, 8)).collect();
        assert!(a.windows(2).all(|w| w[0] == w[1]));
        let mut all = Upgrades::default();
        for u in ALL_UPGRADES {
            while all.take(u) {}
        }
        assert_eq!(roll_choices(&mut Rng::new(1), &all, 99).len, 0);
        // One eligible upgrade left: a single card.
        let mut one = all;
        one.levels[Upgrade::Magnet.index()] = 0;
        let c = roll_choices(&mut Rng::new(1), &one, 99);
        assert_eq!((c.len, c.items[0]), (1, Upgrade::Magnet));
    }

    #[test]
    fn upgrades_stack_with_caps() {
        let mut one = Upgrades::default();
        one.levels[Upgrade::Stars.index()] = 1;
        let base = derive_stats(&one);
        assert_eq!((base.bolts, base.interval, base.orbit, base.nova), (1, 22, 0, 0));
        assert_eq!(derive_stats(&Upgrades::default()).bolts, 0, "no star shot without the weapon");
        // Slots limit what you can take, so fill the table by hand to look at the maxed numbers.
        let mut u = Upgrades::default();
        for x in ALL_UPGRADES {
            u.levels[x.index()] = x.info().max;
        }
        let max = derive_stats(&u);
        assert_eq!(max.bolts, 1 + 5 + 2);
        assert!(max.interval >= 6 && max.interval < base.interval);
        assert!(max.damage > 2.5 * base.damage);
        assert_eq!(max.orbit, 7);
        assert!(max.revive && max.pierce == 4 + 2 && max.quake == 4);
        assert!((max.max_hp - 280.0).abs() < 1e-3);
        assert!(max.cd < 0.4 && max.boom_interval >= 16 && max.whirl_interval >= 10 && max.strike_interval >= 20);
        for x in ALL_UPGRADES {
            let i = x.info();
            assert!(i.max >= 1 && i.weight > 0 && !i.name.is_empty() && !i.desc.is_empty());
        }
    }

    #[test]
    fn weapon_slots_cap_the_build_and_cards_follow_them() {
        let mut u = Upgrades::default();
        let weapons: Vec<Upgrade> = ALL_UPGRADES.iter().copied().filter(|x| x.is_weapon()).collect();
        assert!(weapons.len() >= 12 && WEAPON_SLOTS == 5);
        for w in &weapons {
            u.take(*w);
        }
        assert_eq!(u.weapons(), WEAPON_SLOTS, "only five weapons fit");
        assert_eq!(u.free_slots(), 0);
        // An owned weapon still levels; a sixth cannot be taken.
        let owned = weapons.iter().copied().find(|w| u.level(*w) > 0).unwrap();
        let spare = weapons.iter().copied().find(|w| u.level(*w) == 0).unwrap();
        assert!(u.take(owned) && !u.take(spare));
        assert!(!u.eligible(spare, 99) && u.eligible(owned, 99));
        for seed in 0..60 {
            let c = roll_choices(&mut Rng::new(seed), &u, 30);
            assert!(c.as_slice().iter().all(|x| !x.is_weapon() || u.level(*x) > 0), "a full build is offered a new weapon: {c:?}");
        }
        // A young build with free slots is mostly offered new weapons.
        let young = Upgrades::default();
        for seed in 0..60 {
            let c = roll_choices(&mut Rng::new(seed), &young, 2);
            assert_eq!(c.len, 3);
            assert!(c.as_slice().iter().filter(|x| x.is_weapon()).count() >= 2, "early cards are weapons: {c:?}");
        }
        // Tomes and techniques are not weapons and never use a slot.
        for t in [Upgrade::Damage, Upgrade::FireRate, Upgrade::Quake, Upgrade::Dive, Upgrade::StarStorm] {
            assert!(!t.is_weapon());
        }
    }

    /// A small stage: no director spawns (cap 0), the star shot off, foes placed by hand.
    fn lab(weapons: &[(Upgrade, u8)]) -> Swarm {
        let mut s = Swarm::with_cap(5, 0);
        s.up = Upgrades::default();
        for (u, n) in weapons {
            s.up.levels[u.index()] = *n;
        }
        s.refresh_stats();
        s.storm = false;
        s
    }

    fn foe(s: &mut Swarm, kind: EnemyKind, x: f32, z: f32) -> usize {
        s.spawn_at(kind, [x, z], 0.0).expect("slot")
    }

    fn run_ticks(s: &mut Swarm, n: u32) {
        for _ in 0..n {
            s.player.hp = s.player.max_hp;
            s.tick(&frame_at(0.0, 0.0));
        }
    }

    #[test]
    fn the_sword_cuts_an_arc_in_front_and_every_third_swing_spins() {
        let mut s = lab(&[(Upgrade::Sword, 1)]);
        s.freeze = true;
        let front = foe(&mut s, EnemyKind::Brute, 300.0, 0.0);
        let behind = foe(&mut s, EnemyKind::Brute, -420.0, 0.0);
        let far = foe(&mut s, EnemyKind::Brute, 1500.0, 0.0);
        let hp0 = s.enemies[front].hp;
        run_ticks(&mut s, 22);
        assert!(s.enemies[front].hp < hp0, "the foe in front is cut");
        assert_eq!(s.enemies[behind].hp, hp0, "an arc does not reach behind at level 1");
        assert_eq!(s.enemies[far].hp, hp0, "nor past its reach");
        // Swing two and three (the third is a spin) get to the one behind.
        run_ticks(&mut s, 80);
        assert!(s.enemies[behind].hp < hp0 || !s.enemies[behind].alive, "the third swing spins");
        // The swing is one arc ring, drawn facing the foe.
        let mut s = lab(&[(Upgrade::Sword, 2)]);
        s.freeze = true;
        foe(&mut s, EnemyKind::Brute, 300.0, 0.0);
        run_ticks(&mut s, 22);
        let arc = s.rings.iter().find(|r| r.alive && r.kind == RingKind::Arc).expect("an arc");
        assert!(arc.dir.abs() < 0.2 && arc.half > 1.0, "{arc:?}");
    }

    #[test]
    fn a_sword_at_level_four_cuts_behind_as_well() {
        let mut s = lab(&[(Upgrade::Sword, 4)]);
        s.freeze = true;
        let front = foe(&mut s, EnemyKind::Brute, 300.0, 0.0);
        let behind = foe(&mut s, EnemyKind::Brute, -300.0, 0.0);
        let hp0 = s.enemies[front].hp;
        run_ticks(&mut s, 22);
        assert!(s.enemies[front].hp < hp0 && s.enemies[behind].hp < hp0, "both sides are cut at level 4");
    }

    #[test]
    fn master_sword_throws_a_piercing_beam_each_swing() {
        let mut s = lab(&[(Upgrade::Sword, 4), (Upgrade::MasterSword, 1)]);
        s.freeze = true;
        let a = foe(&mut s, EnemyKind::Brute, 900.0, 0.0);
        let b = foe(&mut s, EnemyKind::Brute, 1250.0, 0.0);
        let hp0 = s.enemies[a].hp;
        run_ticks(&mut s, 60);
        // Out of the arc's reach: only the beam can have touched them.
        assert!(s.enemies[a].hp < hp0 && s.enemies[b].hp < hp0, "the beam pierced both");
    }

    #[test]
    fn fireballs_explode_ignite_and_the_fire_spreads() {
        let mut s = lab(&[(Upgrade::Fireball, 1)]);
        s.freeze = true;
        let a = foe(&mut s, EnemyKind::Brute, 800.0, 0.0);
        let b = foe(&mut s, EnemyKind::Brute, 800.0, 140.0);
        let far = foe(&mut s, EnemyKind::Brute, 800.0, 900.0);
        let hp0 = s.enemies[a].hp;
        let mut blasted = false;
        for _ in 0..70 {
            run_ticks(&mut s, 1);
            blasted |= s.ev.blast;
            if blasted {
                break;
            }
        }
        assert!(blasted, "a fireball exploded");
        assert!(s.enemies[a].hp < hp0 && s.enemies[b].hp < hp0, "the blast hits the neighbours too");
        assert!(s.enemies[a].burn > 0, "and sets them alight");
        assert_eq!(s.enemies[far].hp, hp0);
        // The burn keeps hurting after the blast.
        let after = s.enemies[a].hp;
        run_ticks(&mut s, 18);
        assert!(s.enemies[a].hp < after || !s.enemies[a].alive, "burning damage over time");
    }

    #[test]
    fn inferno_fireballs_leave_flames_behind() {
        let mut s = lab(&[(Upgrade::Fireball, 3), (Upgrade::Flame, 3), (Upgrade::Inferno, 1)]);
        s.freeze = true;
        foe(&mut s, EnemyKind::Brute, 800.0, 0.0);
        run_ticks(&mut s, 80);
        assert!(s.zones.iter().any(|z| z.alive && z.kind == ZoneKind::Fire), "a fire zone at the blast");
    }

    #[test]
    fn the_flame_trail_burns_what_stands_on_it() {
        let mut s = lab(&[(Upgrade::Flame, 2)]);
        s.freeze = true;
        let on_path = foe(&mut s, EnemyKind::Brute, 1200.0, 0.0);
        let off_path = foe(&mut s, EnemyKind::Brute, 1200.0, 900.0);
        let hp0 = s.enemies[on_path].hp;
        let mut zones_seen = 0;
        for t in 0..80 {
            s.player.hp = s.player.max_hp;
            s.tick(&PlayerFrame { pos: [t as f32 * 40.0, 0.0], ..Default::default() });
            zones_seen = zones_seen.max(s.zones.iter().filter(|z| z.alive).count());
        }
        assert!(zones_seen >= 8, "a trail of {zones_seen} patches");
        assert!(s.enemies[on_path].hp < hp0 || !s.enemies[on_path].alive, "the foe on the trail burned");
        assert_eq!(s.enemies[off_path].hp, hp0);
        // Standing still lights nothing new.
        let n = s.zones.iter().filter(|z| z.alive).count();
        for _ in 0..10 {
            s.tick(&PlayerFrame { pos: [79.0 * 40.0, 0.0], ..Default::default() });
        }
        assert!(s.zones.iter().filter(|z| z.alive).count() <= n);
    }

    #[test]
    fn frost_freezes_foes_slows_bosses_and_shatters() {
        let mut s = lab(&[(Upgrade::Frost, 1)]);
        s.frost_cd = 1;
        let a = foe(&mut s, EnemyKind::Brute, 330.0, 0.0);
        let king = foe(&mut s, EnemyKind::King, 0.0, -360.0);
        run_ticks(&mut s, 2);
        assert!(s.enemies[a].freeze > 0 && s.enemies[king].freeze > 0);
        let (pa, pk) = (s.enemies[a].pos, s.enemies[king].pos);
        run_ticks(&mut s, 12);
        assert_eq!(s.enemies[a].pos, pa, "a frozen foe does not move");
        assert!(s.enemies[king].pos != pk, "a frozen boss still creeps");
        assert!(dist2(s.enemies[king].pos, pk).sqrt() < 12.0 * s.enemies[king].speed * 0.6, "but slowly");
        // Shatter: the same hit does 40% more to a frozen foe.
        let mut t = lab(&[]);
        let x = foe(&mut t, EnemyKind::Brute, 900.0, 0.0);
        let y = foe(&mut t, EnemyKind::Brute, -900.0, 0.0);
        t.enemies[y].freeze = 30;
        t.damage_enemy(x, 10.0);
        t.damage_enemy(y, 10.0);
        let (dx, dy) = (t.enemies[x].max_hp - t.enemies[x].hp, t.enemies[y].max_hp - t.enemies[y].hp);
        assert!((dy / dx - arms::SHATTER).abs() < 1e-3, "{dx} {dy}");
        // It thaws.
        run_ticks(&mut t, 40);
        assert_eq!(t.enemies[y].freeze, 0);
    }

    #[test]
    fn poison_clouds_poison_and_slow_foes() {
        let mut s = lab(&[(Upgrade::Poison, 1)]);
        s.poison_cd = 1;
        let a = foe(&mut s, EnemyKind::Brute, 600.0, 0.0);
        let hp0 = s.enemies[a].hp;
        run_ticks(&mut s, 3);
        assert!(s.zones.iter().any(|z| z.alive && z.kind == ZoneKind::Poison), "a cloud");
        run_ticks(&mut s, 30);
        assert!(s.enemies[a].hp < hp0 || !s.enemies[a].alive, "poison eats");
        assert!(s.enemies[a].poison > 0 || !s.enemies[a].alive);
    }

    #[test]
    fn a_black_hole_drags_foes_together_and_collapses() {
        let mut s = lab(&[(Upgrade::Gravity, 2)]);
        s.grav_cd = 1;
        s.freeze = true;
        let a = foe(&mut s, EnemyKind::Brute, 700.0, 0.0);
        let b = foe(&mut s, EnemyKind::Brute, 700.0, 280.0);
        let gap0 = dist2(s.enemies[a].pos, s.enemies[b].pos).sqrt();
        let hp0 = s.enemies[a].hp;
        run_ticks(&mut s, 12);
        assert!(s.zones.iter().any(|z| z.alive && z.kind == ZoneKind::Gravity));
        let gap1 = dist2(s.enemies[a].pos, s.enemies[b].pos).sqrt();
        assert!(gap1 < gap0 * 0.8, "pulled together: {gap0} -> {gap1}");
        run_ticks(&mut s, 100);
        assert!(s.enemies[a].hp < hp0 || !s.enemies[a].alive, "the collapse hurts");
    }

    #[test]
    fn missiles_home_in_on_foes_they_were_not_aimed_at() {
        let mut s = lab(&[(Upgrade::Missiles, 2)]);
        s.freeze = true;
        let a = foe(&mut s, EnemyKind::Brute, 900.0, 300.0);
        let hp0 = s.enemies[a].hp;
        run_ticks(&mut s, 110);
        assert!(s.enemies[a].hp < hp0 || !s.enemies[a].alive, "the missiles found it");
    }

    /// The renderer reads rings, zones and bolts by slot: the lists do not grow and nothing allocates mid-run.
    #[test]
    fn weapon_effects_stay_in_their_pools() {
        let mut s = lab(&[(Upgrade::Sword, 5), (Upgrade::Flame, 5), (Upgrade::Fireball, 5), (Upgrade::Missiles, 5), (Upgrade::Gravity, 5)]);
        s.tick = 200 * TICKS_PER_SEC;
        s.cap = 200;
        // Warm up one tick so the scratch lists have their working size, then watch them.
        s.tick(&frame_at(0.0, 0.0));
        let caps = (s.near.capacity(), s.alive_idx.capacity(), s.nums.capacity(), s.bolts.len(), s.rings.len(), s.zones.len(), s.enemies.len());
        for t in 0..900 {
            s.player.hp = 1e9;
            s.player.max_hp = 1e9;
            let a = t as f32 * 0.03;
            s.tick(&PlayerFrame { pos: [a.cos() * 800.0, a.sin() * 800.0], ..Default::default() });
            while s.choices.is_some() {
                s.pick(0);
            }
            assert_eq!(s.zones.len(), ZONE_SLOTS);
            assert!(s.zones.iter().filter(|z| z.alive).all(|z| z.age <= z.life && z.radius > 0.0));
        }
        assert_eq!(caps, (s.near.capacity(), s.alive_idx.capacity(), s.nums.capacity(), s.bolts.len(), s.rings.len(), s.zones.len(), s.enemies.len()), "no pool or scratch list grew");
        assert!(s.kills > 50, "the weapons killed {}", s.kills);
    }

    #[test]
    fn full_weapon_builds_are_deterministic_and_cheap_at_the_cap() {
        let builds: [&[(Upgrade, u8)]; 2] = [
            &[(Upgrade::Sword, 5), (Upgrade::Flame, 5), (Upgrade::Fireball, 5), (Upgrade::Missiles, 5), (Upgrade::Gravity, 5), (Upgrade::Damage, 4), (Upgrade::Area, 3), (Upgrade::Bolts, 3), (Upgrade::FireRate, 4)],
            &[(Upgrade::Frost, 5), (Upgrade::Poison, 5), (Upgrade::Lightning, 5), (Upgrade::Nova, 5), (Upgrade::Orbit, 5), (Upgrade::Damage, 4), (Upgrade::Area, 3), (Upgrade::Bolts, 3), (Upgrade::FireRate, 4)],
        ];
        for (n, b) in builds.iter().enumerate() {
            let go = || {
                let mut s = lab(b);
                s.cap = enemy_cap();
                s.tick = 400 * TICKS_PER_SEC;
                s.bosses = 3;
                let t0 = std::time::Instant::now();
                for t in 0..600 {
                    s.player.hp = 1e9;
                    s.player.max_hp = 1e9;
                    let a = t as f32 * 0.02;
                    s.tick(&PlayerFrame { pos: [a.cos() * 900.0, a.sin() * 900.0], ..Default::default() });
                    while s.choices.is_some() {
                        s.pick(0);
                    }
                }
                (s.kills, s.alive_enemies, t0.elapsed().as_secs_f64() * 1e6 / 600.0)
            };
            let (k1, l1, us) = go();
            let (k2, l2, _) = go();
            eprintln!("build {n}: {k1} kills, {l1} live at the end, {us:.0} us per tick");
            assert_eq!((k1, l1), (k2, l2), "same seed, same run");
            assert!(l1 >= 100.min(enemy_cap()), "the crowd stayed at the cap ({l1})");
            if !cfg!(debug_assertions) {
                assert!(us < 1500.0, "{us} us per tick with build {n}");
            }
        }
    }

    #[test]
    fn heroes_start_with_a_strong_signature_weapon() {
        let mk = |h| Swarm::start(1, 160, h, super::super::swarm_meta::tier(0), Perks { damage: 1.0, max_hp: 0.0, xp: 1.0 });
        let link = mk(Hero::Link);
        assert!(link.up.level(Upgrade::Sword) >= 2 && link.stats.sword >= 2);
        let mario = mk(Hero::Mario);
        assert!(mario.up.level(Upgrade::Stars) >= 1 && mario.up.level(Upgrade::Dive) > 0 && mario.up.level(Upgrade::Quake) > 0);
        let banjo = mk(Hero::Banjo);
        assert!(banjo.up.level(Upgrade::Boomerang) >= 2 && banjo.stats.boomerangs >= 1);
        for s in [&link, &mario, &banjo] {
            assert!(s.up.weapons() <= WEAPON_SLOTS && s.up.weapons() >= 1);
        }
    }

    #[test]
    fn damage_respects_iframes_and_death() {
        let mut s = Swarm::with_cap(1, 50);
        assert!(s.hurt_player(10.0));
        assert_eq!(s.player.hp, 90.0);
        assert!(!s.hurt_player(10.0), "i-frames block a second hit");
        assert_eq!(s.player.hp, 90.0);
        for _ in 0..IFRAMES {
            s.tick(&frame_at(0.0, 0.0));
        }
        assert_eq!(s.player.iframes, 0);
        assert!(s.hurt_player(95.0));
        s.tick(&frame_at(0.0, 0.0));
        assert!(s.over && s.ev.died && s.player.hp == 0.0);
        // A finished run no longer changes.
        let t = s.tick;
        s.tick(&frame_at(0.0, 0.0));
        assert_eq!(s.tick, t);
    }

    #[test]
    fn second_chance_revives_once() {
        let mut s = Swarm::with_cap(1, 50);
        s.up.take(Upgrade::SecondChance);
        s.stats = derive_stats(&s.up);
        s.player.hp = 1.0;
        s.hurt_player(50.0);
        s.tick(&frame_at(0.0, 0.0));
        assert!(!s.over && s.ev.revived);
        assert_eq!(s.player.hp, 50.0);
        assert!(s.player.iframes > 60);
        s.player.iframes = 0;
        s.hurt_player(500.0);
        s.tick(&frame_at(0.0, 0.0));
        assert!(s.over);
    }

    #[test]
    fn contact_needs_overlap_and_a_jump_clears_small_foes() {
        let mut s = Swarm::with_cap(1, 50);
        s.spawn_enemy(EnemyKind::Skitter, [30.0, 0.0], 0.0);
        s.alive_idx.push(0);
        s.contact(0.0);
        assert!(s.player.hp < BASE_HP, "standing in a skitter hurts");
        let mut s = Swarm::with_cap(1, 50);
        s.spawn_enemy(EnemyKind::Skitter, [30.0, 0.0], 0.0);
        s.alive_idx.push(0);
        s.player.height = 120.0;
        s.contact(0.0);
        assert_eq!(s.player.hp, BASE_HP, "jumping over a skitter is safe");
        let mut s = Swarm::with_cap(1, 50);
        s.spawn_enemy(EnemyKind::Skitter, [900.0, 0.0], 0.0);
        s.alive_idx.push(0);
        s.contact(0.0);
        assert_eq!(s.player.hp, BASE_HP);
    }

    #[test]
    fn stars_kill_gems_level_and_pause() {
        let mut s = Swarm::with_cap(5, 50);
        // A lone skitter right next to the player dies to the first volley and drops a gem.
        s.spawn_enemy(EnemyKind::Skitter, [300.0, 0.0], 0.0);
        s.enemies[0].hp = 5.0;
        let mut guard = 0;
        while s.kills == 0 && guard < 90 {
            s.tick(&frame_at(0.0, 0.0));
            guard += 1;
        }
        assert_eq!(s.kills, 1, "volley should land within 3 s");
        assert!(s.player.xp > 0 || s.gems.iter().any(|g| g.alive), "the kill pays out XP");
        // Gems fly in and level the player up.
        s.player.xp = xp_to_next(1) - 1;
        s.drop_gem(GemKind::Xp, [10.0, 0.0], 1);
        for _ in 0..60 {
            s.tick(&frame_at(0.0, 0.0));
            if s.choices.is_some() {
                break;
            }
        }
        assert_eq!(s.player.level, 2);
        let c = s.choices.expect("level-up offers cards");
        assert_eq!(c.len, 3);
        // The game stands still while choosing.
        let t = s.tick;
        s.tick(&frame_at(0.0, 0.0));
        assert_eq!(s.tick, t);
        assert!(s.pick(1));
        assert!(s.choices.is_none());
        assert_eq!(s.up.level(c.items[1]), 1);
        assert!(!s.pick(0), "no cards left to pick");
    }

    #[test]
    fn stacked_level_ups_queue_cards() {
        let mut s = Swarm::with_cap(5, 50);
        s.add_xp(200);
        assert!(s.player.level >= 4);
        let mut picks = 0;
        while s.choices.is_some() {
            assert!(s.pick(0));
            picks += 1;
        }
        assert_eq!(picks, s.player.level - 1);
        assert_eq!(s.pending_levels, 0);
    }

    #[test]
    fn gems_merge_when_the_arena_is_full() {
        let mut s = Swarm::with_cap(1, 50);
        let total = |s: &Swarm| s.gems.iter().filter(|g| g.alive).map(|g| g.value as u32).sum::<u32>();
        for i in 0..GEM_SLOTS {
            s.drop_gem(GemKind::Xp, [1000.0 + i as f32, 1000.0], 1);
        }
        assert_eq!(s.gems.iter().filter(|g| g.alive).count(), GEM_SLOTS);
        s.drop_gem(GemKind::Xp, [0.0, 0.0], 4);
        assert_eq!(s.gems.iter().filter(|g| g.alive).count(), GEM_SLOTS);
        assert_eq!(total(&s), GEM_SLOTS as u32 + 4, "no XP is lost");
    }

    #[test]
    fn boss_arrives_every_two_minutes() {
        let mut s = Swarm::with_cap(2, 40);
        let mut boss_at = vec![];
        for _ in 0..(BOSS_PERIOD_SECS as u32 * 2 + 5) * TICKS_PER_SEC {
            s.player.hp = s.player.max_hp;
            s.tick(&frame_at(0.0, 0.0));
            if s.ev.boss_spawned {
                boss_at.push(s.secs());
            }
            while s.choices.is_some() {
                s.pick(0);
            }
        }
        assert_eq!(boss_at.len(), 2, "{boss_at:?}");
        assert!((boss_at[0] - 120.0).abs() < 1.0 && (boss_at[1] - 240.0).abs() < 1.0);
    }

    /// A long scripted run: the player circles the arena, always taking the first card.
    fn run(seed: u64, secs: u32) -> (u32, u32, u32, f32, usize, usize) {
        let mut s = Swarm::with_cap(seed, 160);
        let mut peak = 0;
        for t in 0..secs * TICKS_PER_SEC {
            let a = t as f32 * 0.01;
            s.player.hp = s.player.max_hp;
            s.tick(&PlayerFrame { pos: [a.cos() * 1500.0, a.sin() * 1500.0], height: 0.0, pound_land: t % 200 == 0, ..Default::default() });
            while s.choices.is_some() {
                s.pick(0);
            }
            peak = peak.max(s.alive_enemies);
            assert!(s.alive_enemies <= s.cap() + OVERCAP + 4, "live {} over the cap", s.alive_enemies);
            let counted = s.enemies.iter().filter(|e| e.alive).count();
            assert_eq!(counted, s.alive_enemies, "enemy counter drifted");
            for e in s.enemies.iter().filter(|e| e.alive) {
                assert!(e.pos[0].abs() <= ARENA_HALF && e.pos[1].abs() <= ARENA_HALF);
            }
            if s.over {
                break;
            }
        }
        (s.tick, s.kills, s.player.level, s.player.hp, peak, s.gems.iter().filter(|g| g.alive).count())
    }

    #[test]
    fn long_runs_are_deterministic_and_bounded() {
        let a = run(11, 420);
        eprintln!("run(11): {a:?}");
        let b = run(11, 420);
        assert_eq!(a, b, "same seed, same run");
        assert!(a.1 > 100, "an auto-attacker should rack up kills: {a:?}");
        assert!(a.2 >= 5, "and level up: {a:?}");
        assert!(a.4 <= 160 + OVERCAP + 4, "{a:?}");
        let c = run(12, 420);
        assert_ne!(a, c, "different seeds diverge");
    }

    #[test]
    fn standing_still_eventually_loses() {
        let mut s = Swarm::with_cap(4, 160);
        for _ in 0..30 * 60 * TICKS_PER_SEC {
            s.tick(&frame_at(0.0, 0.0));
            while s.choices.is_some() {
                s.pick(0);
            }
            if s.over {
                break;
            }
        }
        assert!(s.over && !s.won && s.player.hp == 0.0, "the swarm wins against an idle player (hp {})", s.player.hp);
        // A finished run is inert: no more ticks, no revival, no second ending.
        let t = s.tick;
        s.tick(&frame_at(0.0, 0.0));
        assert!(s.tick == t && !s.ev.died && !s.ev.won);
    }

    /// Peak load (the enemy cap, bosses, orbs, a full volley) must stay cheap: the sim tick has to
    /// fit in a small slice of a 33 ms phone frame. Debug builds are far slower, so only the
    /// optimised build asserts a bound.
    #[test]
    fn tick_cost_at_peak_load_is_small() {
        let mut s = Swarm::with_cap(7, enemy_cap());
        s.tick = 420 * TICKS_PER_SEC;
        s.bosses = 3;
        s.up.levels[Upgrade::Bolts.index()] = 5;
        s.up.levels[Upgrade::Nova.index()] = 3;
        s.up.levels[Upgrade::Orbit.index()] = 3;
        s.stats = derive_stats(&s.up);
        let mut peak = 0;
        let start = std::time::Instant::now();
        let ticks = 900;
        for t in 0..ticks {
            s.player.hp = 1e9;
            s.player.max_hp = 1e9;
            let a = t as f32 * 0.02;
            s.tick(&PlayerFrame { pos: [a.cos() * 900.0, a.sin() * 900.0], height: 0.0, pound_land: false, ..Default::default() });
            while s.choices.is_some() {
                s.pick(0);
            }
            peak = peak.max(s.alive_enemies);
        }
        let per_tick = start.elapsed().as_secs_f64() * 1e6 / ticks as f64;
        eprintln!("peak {peak} enemies, {per_tick:.0} us per tick");
        assert!(peak >= enemy_cap().min(100));
        if !cfg!(debug_assertions) {
            assert!(per_tick < 1500.0, "{per_tick} us per tick");
        }
    }

    /// A scripted human stand-in: kites away from the crowd (strongest push from the nearest foes,
    /// a pull toward the middle so it never pins itself on a wall), jumps the odd skitter, ground
    /// pounds now and then, and always takes the most useful card. Returns the finished core.
    fn kite(seed: u64, hp_cheat: bool) -> Swarm {
        kite_from(Swarm::with_cap(seed, 160), WIN_SECS + 90.0, hp_cheat)
    }

    fn kite_from(s: Swarm, until_secs: f32, hp_cheat: bool) -> Swarm {
        kite_skill(s, until_secs, hp_cheat, 1.0)
    }

    /// `speed_k` below 1 is a clumsier player who runs slower and turns late (a casual human).
    fn kite_skill(mut s: Swarm, until_secs: f32, hp_cheat: bool, speed_k: f32) -> Swarm {
        const PRIORITY: &[Upgrade] = &[
            Upgrade::StarStorm,
            Upgrade::Supernova,
            Upgrade::Cyclone,
            Upgrade::ThunderGod,
            Upgrade::Comet,
            Upgrade::Earthshaker,
            Upgrade::MasterSword,
            Upgrade::Inferno,
            Upgrade::Singularity,
            Upgrade::Sword,
            Upgrade::Stars,
            Upgrade::Fireball,
            Upgrade::Missiles,
            Upgrade::Vitality,
            Upgrade::Regen,
            Upgrade::Boomerang,
            Upgrade::Lightning,
            Upgrade::Nova,
            Upgrade::Whirl,
            Upgrade::Frost,
            Upgrade::Flame,
            Upgrade::Gravity,
            Upgrade::Orbit,
            Upgrade::Poison,
            Upgrade::Damage,
            Upgrade::FireRate,
            Upgrade::Bolts,
            Upgrade::Armor,
            Upgrade::Speed,
            Upgrade::Area,
            Upgrade::Pierce,
            Upgrade::SecondChance,
            Upgrade::Crit,
            Upgrade::Magnet,
            Upgrade::Quake,
            Upgrade::Jump,
            Upgrade::Armor,
            Upgrade::Growth,
            Upgrade::Dive,
        ];
        let mut pos = [0.0f32; 2];
        let mut heading = 0.0f32;
        for t in 0..(until_secs as u32) * TICKS_PER_SEC {
            // Steering: repulsion from every foe within 700, weighted by closeness.
            let mut push = [0.0f32; 2];
            for e in s.enemies.iter().filter(|e| e.alive) {
                let d = [pos[0] - e.pos[0], pos[1] - e.pos[1]];
                let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1.0);
                if l < 700.0 {
                    let w = (700.0 - l) / 700.0;
                    push[0] += d[0] / l * w * w;
                    push[1] += d[1] / l * w * w;
                }
            }
            // Gems (and chests) pull the player in when no foe is close: a human collects them.
            let nearest_foe = s.enemies.iter().filter(|e| e.alive).map(|e| dist2(e.pos, pos)).fold(f32::MAX, f32::min);
            if nearest_foe > 450.0 * 450.0 {
                let mut best = (f32::MAX, [0.0f32; 2]);
                for g in s.gems.iter().filter(|g| g.alive) {
                    let d = dist2(g.pos, pos);
                    if d < best.0 {
                        best = (d, g.pos);
                    }
                }
                if best.0 < 1400.0 * 1400.0 {
                    let d = best.0.sqrt().max(1.0);
                    push[0] += (best.1[0] - pos[0]) / d * 0.9;
                    push[1] += (best.1[1] - pos[1]) / d * 0.9;
                }
            }
            // Walls and the middle: a gentle pull to the centre that grows near the edge.
            let safe = s.ring_r - 150.0;
            let edge = ((pos[0] * pos[0] + pos[1] * pos[1]).sqrt() / safe).powi(3);
            push[0] -= pos[0] / safe * 2.0 * edge;
            push[1] -= pos[1] / safe * 2.0 * edge;
            let pl = (push[0] * push[0] + push[1] * push[1]).sqrt();
            let speed = (24.0 * (1.0 + s.stats.move_bonus)).min(40.0) * speed_k;
            if pl > 0.05 {
                let want = push[1].atan2(push[0]);
                let mut d = want - heading;
                while d > std::f32::consts::PI {
                    d -= std::f32::consts::TAU;
                }
                while d < -std::f32::consts::PI {
                    d += std::f32::consts::TAU;
                }
                heading += d.clamp(-0.35, 0.35) * if speed_k < 1.0 { 0.4 } else { 1.0 };
                pos[0] = (pos[0] + heading.cos() * speed).clamp(-ARENA_HALF + 90.0, ARENA_HALF - 90.0);
                pos[1] = (pos[1] + heading.sin() * speed).clamp(-ARENA_HALF + 90.0, ARENA_HALF - 90.0);
            }
            if hp_cheat {
                s.player.hp = s.player.max_hp;
            }
            let pounds = !matches!(s.hero, Hero::Link | Hero::Banjo);
            s.tick(&PlayerFrame { pos, height: 0.0, pound_land: pounds && t % 240 == 0, ..Default::default() });
            while s.choices.is_some() {
                let c = s.choices.unwrap();
                let best = (0..c.len).min_by_key(|&i| PRIORITY.iter().position(|u| *u == c.items[i]).unwrap_or(99)).unwrap();
                s.pick(best);
            }
            if s.over {
                break;
            }
        }
        s
    }

    /// The run is survivable to the Tyrant: a kiting player who takes sensible cards (and never ducks a shot or
    /// thinks about the storm) is alive when the final boss walks in on most seeds.
    #[test]
    fn a_kiting_player_reaches_the_final_boss() {
        let mut reached = 0;
        for seed in 1..=8u64 {
            let s = kite_from(Swarm::with_cap(seed, 160), FINAL_SECS + 5.0, false);
            eprintln!("kite seed {seed}: over={} secs={:.0} level={} kills={} hp={:.0}/{:.0} elites={} bosses={}", s.over, s.secs(), s.player.level, s.kills, s.player.hp, s.player.max_hp, s.elites, s.bosses);
            if !s.over && s.final_spawned {
                reached += 1;
            }
            assert!(s.bosses >= 1 || s.over, "seed {seed}: no boss by {}", s.secs());
        }
        assert!(reached >= 5, "only {reached} of 8 kiting players reached the Tyrant");
    }

    /// And it can be beaten: a player with a mid-run build (what seven minutes of picks and chests buy) kills it.
    #[test]
    fn a_geared_player_kills_the_tyrant_and_wins() {
        for seed in 1..=4u64 {
            let mut s = Swarm::with_cap(seed, 160);
            for (u, n) in [(Upgrade::Damage, 6), (Upgrade::FireRate, 6), (Upgrade::Bolts, 4), (Upgrade::Pierce, 2), (Upgrade::Area, 3), (Upgrade::Vitality, 6), (Upgrade::Regen, 3), (Upgrade::Armor, 3), (Upgrade::Boomerang, 4), (Upgrade::Lightning, 3), (Upgrade::Whirl, 3), (Upgrade::Speed, 3), (Upgrade::Magnet, 3)] {
                for _ in 0..n {
                    s.up.take(u);
                }
            }
            s.refresh_stats();
            s.player.hp = s.player.max_hp;
            s.player.level = 30;
            s.tick = (FINAL_SECS * TICKS_PER_SEC as f32) as u32;
            s.bosses = 3;
            let s = kite_from(s, FINAL_SECS + 150.0, false);
            eprintln!("geared seed {seed}: over={} won={} secs={:.0} hp={:.0}/{:.0} boss={:?}", s.over, s.won, s.secs(), s.player.hp, s.player.max_hp, s.boss_bar);
            assert!(s.won && s.player.hp > 0.0, "seed {seed}: the geared player did not beat the Tyrant");
        }
    }

    #[test]
    fn killing_the_tyrant_wins_once_and_death_beats_a_simultaneous_win() {
        let tyrant_run = || {
            let mut s = Swarm::with_cap(3, 20);
            s.tick = (FINAL_SECS * TICKS_PER_SEC as f32) as u32 - 1;
            s.tick(&frame_at(0.0, 0.0));
            assert!(s.final_spawned && s.ev.final_spawned && !s.over, "the Tyrant arrives at 7:00");
            let t = s.enemies.iter().position(|e| e.alive && e.kind == EnemyKind::Tyrant).expect("the tyrant");
            assert!(s.boss_bar.is_some() || { s.tick(&frame_at(0.0, 0.0)); s.boss_bar.is_some() }, "the boss bar shows it");
            (s, t)
        };
        let (mut s, t) = tyrant_run();
        s.damage_enemy(t, 1e9);
        s.tick(&frame_at(0.0, 0.0));
        assert!(s.over && s.won && s.ev.won && !s.ev.died);
        s.tick(&frame_at(0.0, 0.0));
        assert!(!s.ev.won, "the win event fires once");
        let (mut d, t) = tyrant_run();
        d.damage_enemy(t, 1e9);
        d.player.hp = 0.0;
        d.tick(&frame_at(0.0, 0.0));
        assert!(d.over && !d.won && d.ev.died);
        // The dawn alone is not a win: the Tyrant has to fall.
        let (mut w, _) = tyrant_run();
        w.tick = (WIN_SECS * TICKS_PER_SEC as f32) as u32 + 5;
        w.player.hp = w.player.max_hp;
        w.tick(&frame_at(0.0, 0.0));
        assert!(!w.over && !w.won);
    }

    /// The all-pairs separation the mode used before the spatial hash (kept here to time the change).
    fn separate_all_pairs(s: &mut Swarm) {
        let n = s.alive_idx.len();
        for a in 0..n {
            let i = s.alive_idx[a] as usize;
            for b in a + 1..n {
                let j = s.alive_idx[b] as usize;
                let (pi, pj) = (s.enemies[i].pos, s.enemies[j].pos);
                let (dx, dz) = (pj[0] - pi[0], pj[1] - pi[1]);
                let (mi, mj) = (s.enemies[i].radius, s.enemies[j].radius);
                let reach = (mi + mj) * 0.8;
                let d2 = dx * dx + dz * dz;
                if d2 >= reach * reach {
                    continue;
                }
                let d = d2.sqrt();
                let (nx, nz) = if d > 0.01 { (dx / d, dz / d) } else { (1.0, 0.0) };
                let push = (reach - d) * 0.35;
                let (fi, fj) = (mj / (mi + mj), mi / (mi + mj));
                s.enemies[i].pos = [pi[0] - nx * push * fi * 2.0, pi[1] - nz * push * fi * 2.0];
                s.enemies[j].pos = [pj[0] + nx * push * fj * 2.0, pj[1] + nz * push * fj * 2.0];
            }
        }
    }

    /// `cargo test --release -p jumpman bench_swarm -- --ignored --nocapture`: the crowd step at the cap, hash against all pairs.
    #[test]
    #[ignore]
    fn bench_swarm_crowd_step() {
        for cap in [160usize, 300] {
            let mut s = Swarm::with_cap(7, cap);
            s.tick = 420 * TICKS_PER_SEC;
            s.final_spawned = true;
            s.bosses = 3;
            for t in 0..600 {
                s.player.hp = 1e9;
                s.player.max_hp = 1e9;
                let a = t as f32 * 0.02;
                s.tick(&PlayerFrame { pos: [a.cos() * 900.0, a.sin() * 900.0], ..Default::default() });
                while s.choices.is_some() {
                    s.pick(0);
                }
            }
            let live = s.alive_enemies;
            let time = |f: &mut dyn FnMut()| {
                let t0 = std::time::Instant::now();
                for _ in 0..2000 {
                    f();
                }
                t0.elapsed().as_secs_f64() * 1e6 / 2000.0
            };
            let hash = time(&mut || s.separate());
            let pairs = time(&mut || separate_all_pairs(&mut s));
            let whole = {
                let t0 = std::time::Instant::now();
                for t in 0..600 {
                    s.player.hp = 1e9;
                    let a = t as f32 * 0.02;
                    s.tick(&PlayerFrame { pos: [a.cos() * 900.0, a.sin() * 900.0], ..Default::default() });
                    while s.choices.is_some() {
                        s.pick(0);
                    }
                }
                t0.elapsed().as_secs_f64() * 1e6 / 600.0
            };
            eprintln!("cap {cap} ({live} live): separation with the hash {hash:.1} us, all pairs {pairs:.1} us; whole tick {whole:.1} us");
        }
    }

    #[test]
    #[ignore]
    fn debug_kill_rate() {
        let mut s = Swarm::with_cap(1, 160);
        for t in 0..240 * TICKS_PER_SEC {
            s.player.hp = s.player.max_hp;
            s.tick(&frame_at(0.0, 0.0));
            while s.choices.is_some() {
                s.pick(0);
            }
            if t % (30 * TICKS_PER_SEC) == 0 {
                eprintln!("t={} kills={} lvl={} alive={} bolts={} gems={}", t / 30, s.kills, s.player.level, s.alive_enemies, s.bolts.iter().filter(|b| b.alive).count(), s.gems.iter().filter(|g| g.alive).count());
            }
        }
    }

    /// Per-hero survival with the kiting stand-in (no health cheat): `cargo test --release -p jumpman hero_survival -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hero_survival() {
        for (label, k) in [("skilled kiter", 1.0f32), ("casual (half speed, late turns)", 0.5)] {
            for hero in [Hero::Mario, Hero::Link, Hero::Banjo] {
                let (mut secs, mut kills, mut lvl, mut reached, mut k120) = (0.0, 0.0, 0.0, 0, 0.0);
                for seed in 1..=8u64 {
                    let s = Swarm::start(seed, 300, hero, super::super::swarm_meta::tier(0), Perks { damage: 1.0, max_hp: 0.0, xp: 1.0 });
                    let s = kite_skill(s, 120.0, false, k);
                    let at120 = s.kills;
                    let s = kite_skill(s, WIN_SECS + 90.0, false, k);
                    secs += s.secs() as f64;
                    kills += s.kills as f64;
                    lvl += s.player.level as f64;
                    k120 += at120 as f64;
                    if s.won || s.secs() >= FINAL_SECS {
                        reached += 1;
                    }
                }
                eprintln!("HERO {hero:?} [{label}]: mean survival {:.0}s, kills@2:00 {:.0}, total kills {:.0}, level {:.1}, reached 7:00 in {reached}/8", secs / 8.0, k120 / 8.0, kills / 8.0, lvl / 8.0);
            }
        }
    }

    /// Link's sword must carry the early game as well as Mario's stars (the complaint was dying fast with the sword).
    #[test]
    fn link_survives_and_kills_as_well_as_mario_early() {
        if cfg!(debug_assertions) {
            return;
        }
        let early = |hero: Hero, k: f32| {
            let (mut kills, mut alive) = (0u32, 0u32);
            for seed in 1..=6u64 {
                let s = Swarm::start(seed, 300, hero, super::super::swarm_meta::tier(0), Perks { damage: 1.0, max_hp: 0.0, xp: 1.0 });
                let s = kite_skill(s, 180.0, false, k);
                kills += s.kills;
                alive += !s.over as u32;
            }
            (kills / 6, alive)
        };
        for k in [1.0, 0.5] {
            let (mario, link, banjo) = (early(Hero::Mario, k), early(Hero::Link, k), early(Hero::Banjo, k));
            eprintln!("skill {k}: mean kills by 3:00 and runs alive of 6 - Mario {mario:?}, Link {link:?}, Banjo {banjo:?}");
            assert!(link.0 * 10 >= mario.0 * 7, "Link kills {link:?} against Mario {mario:?}");
            assert!(link.1 + 1 >= mario.1 && banjo.1 + 1 >= mario.1, "survival {link:?} {banjo:?} {mario:?}");
            assert!(link.1 >= 4, "Link survives the early game: {link:?}");
        }
    }

    #[test]
    fn clock_format() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(61.9), "1:01");
        assert_eq!(clock(600.0), "10:00");
    }

    // ---- explore mode ------------------------------------------------------------------------

    /// A flat, fully walkable streamed world (every chunk near the origin marked walkable).
    fn open_world() -> Swarm {
        let mut s = Swarm::with_cap(9, 120);
        s.explore = true;
        s.storm = false;
        s.half = 1.0e6;
        let mut m = WalkMask::streamed();
        for cz in -12..12 {
            for cx in -12..12 {
                m.set_chunk((cx, cz), vec![true; (CHUNK_CELLS * CHUNK_CELLS) as usize].into_boxed_slice());
            }
        }
        s.mask = Some(m);
        s
    }

    #[test]
    fn streamed_masks_answer_by_chunk_and_treat_missing_chunks_as_blocked() {
        let mut m = WalkMask::streamed();
        assert!(!m.at([0.0, 0.0]));
        let mut bits = vec![false; (CHUNK_CELLS * CHUNK_CELLS) as usize];
        bits[0] = true; // cell (0, 0) of chunk (0, 0)
        bits[(CHUNK_CELLS * CHUNK_CELLS) as usize - 1] = true;
        m.set_chunk((0, 0), bits.into_boxed_slice());
        assert!(m.at([10.0, 10.0]) && !m.at([150.0, 10.0]));
        let span = WALK_CELL * CHUNK_CELLS as f32;
        assert!(m.at([span - 10.0, span - 10.0]));
        assert!(!m.at([span + 10.0, 10.0]) && !m.at([-10.0, 10.0]));
        assert_eq!(WalkMask::chunk_of([-1.0, span + 1.0]), (-1, 1));
        let mut m2 = WalkMask::streamed();
        m2.set_chunk((-1, -1), vec![true; (CHUNK_CELLS * CHUNK_CELLS) as usize].into_boxed_slice());
        assert!(m2.at([-5.0, -5.0]) && m2.line_clear([-100.0, -100.0], [-900.0, -400.0]) && !m2.line_clear([-100.0, -100.0], [900.0, 400.0]));
    }

    #[test]
    fn explore_mode_has_no_storm_and_no_edge() {
        let mut s = open_world();
        let far = [9000.0, -7000.0];
        s.mask.as_mut().unwrap().set_chunk(WalkMask::chunk_of(far), vec![true; (CHUNK_CELLS * CHUNK_CELLS) as usize].into_boxed_slice());
        for _ in 0..(40 * TICKS_PER_SEC) {
            s.player.hp = s.player.max_hp;
            s.tick(&PlayerFrame { pos: far, ..Default::default() });
            assert!(!s.ev.storm);
        }
        assert!(s.alive_enemies > 5, "foes gather around a player far from the origin ({})", s.alive_enemies);
    }

    #[test]
    fn foes_spawn_around_the_player_on_walkable_ground_and_far_ones_are_recycled() {
        let mut s = open_world();
        // Restrict the walkable ground to a corridor: foes must only appear on it.
        let mut m = WalkMask::streamed();
        for cz in -1..=0 {
            for cx in -12..12 {
                m.set_chunk((cx, cz), vec![true; (CHUNK_CELLS * CHUNK_CELLS) as usize].into_boxed_slice());
            }
        }
        s.mask = Some(m);
        for t in 0..(60 * TICKS_PER_SEC) {
            s.player.hp = s.player.max_hp;
            s.tick(&PlayerFrame { pos: [t as f32 * 4.0, 0.0], ..Default::default() });
            while s.choices.is_some() {
                s.pick(0);
            }
            for e in s.enemies.iter().filter(|e| e.alive) {
                assert!(e.pos[1].abs() < 1700.0 && s.walkable(e.pos), "{:?}", e.pos);
            }
        }
        assert!(s.alive_enemies <= s.cap() + OVERCAP);
        // A foe left far behind is recycled; a boss is moved to the player's side instead.
        let mut s = open_world();
        let far = s.spawn_enemy(EnemyKind::Skitter, [-8000.0, 0.0], 0.0);
        let boss = s.spawn_enemy(EnemyKind::King, [8000.0, 0.0], 0.0);
        assert!(far && boss);
        s.player.hp = 1e6;
        s.tick(&frame_at(0.0, 0.0));
        let king = s.enemies.iter().find(|e| e.alive && e.kind == EnemyKind::King).expect("the boss stays");
        assert!(dist2(king.pos, [0.0, 0.0]).sqrt() < 2600.0, "{:?}", king.pos);
        assert!(s.enemies.iter().all(|e| !(e.alive && e.pos[0] < -4000.0)));
    }

    #[test]
    fn the_dark_grows_after_the_tyrant_arrives() {
        assert_eq!(dawn(100.0), 0.0);
        assert!(dawn(FINAL_SECS + 60.0) > 0.99 && dawn(FINAL_SECS + 6000.0) == 3.0);
        assert!(hp_mult(FINAL_SECS + 120.0) > hp_mult(FINAL_SECS) * 1.2);
        assert!(target_live_at(FINAL_SECS + 120.0, 300, 1.0) > target_live_at(FINAL_SECS, 300, 1.0));
        assert!(speed_mult(FINAL_SECS + 120.0) > speed_mult(FINAL_SECS));
    }

    fn site(x: f32, z: f32, y: f32, tier: ChestTier) -> WorldChest {
        WorldChest { site: ChestSite { pos: [x, z], y, tier, hide: 1 }, opened: false, revealed: false }
    }

    #[test]
    fn world_chests_reveal_when_near_and_open_on_touch_with_tiered_rewards() {
        let mut s = open_world();
        s.freeze = true;
        s.wchests = vec![site(1000.0, 0.0, 0.0, ChestTier::Common), site(0.0, 1000.0, 0.0, ChestTier::Rare), site(-2000.0, 0.0, 300.0, ChestTier::Common)];
        s.player.hp = 40.0;
        s.tick(&PlayerFrame { pos: [0.0, 0.0], ..Default::default() });
        assert!(s.wchests[0].revealed && s.wchests[1].revealed && !s.wchests[2].revealed, "only the near ones are noticed");
        // Touching the common chest: a card pick.
        s.tick(&PlayerFrame { pos: [1000.0, 0.0], ..Default::default() });
        assert!(s.wchests[0].opened && s.ev.chest_tier == Some(ChestTier::Common) && s.ev.chest_opened);
        assert!(s.choices.is_some_and(|c| c.chest), "a chest card set is on offer");
        s.pick(0);
        // The rare one heals and pays gold.
        s.player.hp = 30.0;
        s.tick(&PlayerFrame { pos: [0.0, 1000.0], ..Default::default() });
        assert!(s.wchests[1].opened && s.ev.chest_tier == Some(ChestTier::Rare) && s.gold >= 35 && s.player.hp > 55.0, "hp {}", s.player.hp);
        // A chest on a ledge three hundred up cannot be taken from the ground below it.
        s.pick(0);
        s.tick(&PlayerFrame { pos: [-2000.0, 0.0], y: 0.0, ..Default::default() });
        assert!(!s.wchests[2].opened, "out of reach in height");
        s.tick(&PlayerFrame { pos: [-2000.0, 0.0], y: 300.0, ..Default::default() });
        assert!(s.wchests[2].opened);
        assert_eq!(s.chests_opened, 3);
        // Opened chests never open twice.
        let before = s.pending_chests;
        s.tick(&PlayerFrame { pos: [-2000.0, 0.0], y: 300.0, ..Default::default() });
        assert_eq!(s.pending_chests, before);
    }

    #[test]
    fn a_golden_chest_needs_a_key_or_a_survived_fight() {
        // With a key (an elite's drop): opens at once for two picks.
        let mut s = open_world();
        s.freeze = true;
        s.wchests = vec![site(800.0, 0.0, 0.0, ChestTier::Golden)];
        s.keys = 1;
        s.tick(&PlayerFrame { pos: [800.0, 0.0], ..Default::default() });
        assert!(s.wchests[0].opened && s.keys == 0 && s.pending_chests >= 2 && s.gold >= 90);
        // Without: a timed fight starts, an elite guards it, and surviving 20 seconds opens it.
        let mut s = open_world();
        s.wchests = vec![site(800.0, 0.0, 0.0, ChestTier::Golden)];
        s.tick(&PlayerFrame { pos: [800.0, 0.0], ..Default::default() });
        assert!(s.ev.challenge_started && s.challenge.is_some() && !s.wchests[0].opened);
        assert!(s.enemies.iter().any(|e| e.alive && e.elite), "a guardian appears");
        let mut won = false;
        for _ in 0..(CHALLENGE_TICKS + 5) {
            s.player.hp = s.player.max_hp;
            s.tick(&PlayerFrame { pos: [800.0, 0.0], ..Default::default() });
            while s.choices.is_some() {
                s.pick(0);
            }
            won |= s.ev.challenge_won;
        }
        assert!(won && s.wchests[0].opened && s.challenge.is_none());
        // Running away from the fight cancels it and the chest stays shut.
        let mut s = open_world();
        s.freeze = true;
        s.wchests = vec![site(800.0, 0.0, 0.0, ChestTier::Golden)];
        s.tick(&PlayerFrame { pos: [800.0, 0.0], ..Default::default() });
        assert!(s.challenge.is_some());
        s.tick(&PlayerFrame { pos: [3000.0, 0.0], ..Default::default() });
        assert!(s.ev.challenge_lost && s.challenge.is_none() && !s.wchests[0].opened);
    }

    #[test]
    fn elites_drop_keys_in_explore_mode() {
        let mut s = open_world();
        s.freeze = true;
        assert!(s.spawn_elite(EnemyKind::Brute, [300.0, 0.0], 60.0));
        let i = s.enemies.iter().position(|e| e.alive && e.elite).unwrap();
        s.damage_enemy(i, 1.0e9);
        assert_eq!(s.keys, 1);
        assert!(s.ev.key_found);
    }
}
