//! Relic Descent, deterministic core: an original action-RPG "looter dungeon crawler" ruleset
//! with no engine dependency. The player moves with Mario's real physics (driven elsewhere);
//! this file owns everything else: combat (swing arc, dodge i-frames, four cooldown skills and a
//! twelve-node skill tree), six foe types plus elite modifiers and a three-phase boss, packs that
//! wake when a room is entered, loot drops, gold, potions, XP, floors and the stairs.
//!
//! All state lives in fixed-capacity slot arrays so a tick never allocates (except when an item
//! enters the bag) and a render slot index is stable for the life of an entity. Positions are in
//! SM64 units on the floor plane, relative to the dungeon's corner (tile 0,0); the Bevy layer adds
//! the origin. Time is counted in 30 Hz ticks and every random choice comes from the seeded
//! [`Rng`], so a run is reproducible from `(seed, inputs)`.
use super::relic_dungeon::*;
use super::relic_loot::*;
use super::swarm_core::Rng;

pub const TICKS_PER_SEC: u32 = 30;
pub const FLOORS: u32 = 3;
pub const ENEMY_SLOTS: usize = 80;
pub const SHOT_SLOTS: usize = 48;
pub const HAZARD_SLOTS: usize = 24;
pub const RING_SLOTS: usize = 16;
pub const DROP_SLOTS: usize = 64;
pub const FLOATER_SLOTS: usize = 32;
pub const CHEST_SLOTS: usize = 4;
pub const PLAYER_RADIUS: f32 = 45.0;
pub const MAX_LEVEL: u32 = 20;
pub const POTION_MAX: u32 = 5;
pub const START_POTIONS: u32 = 2;
pub const DODGE_COOLDOWN: u16 = 30;
pub const HIT_IFRAMES: u16 = 18;

/// Live-enemy cap: phones get fewer than desktops.
pub const fn enemy_cap() -> usize {
    if cfg!(any(target_os = "ios", target_os = "android")) { 64 } else { ENEMY_SLOTS }
}

// ---------------------------------------------------------------------------------------------
// Heroes and skills
// ---------------------------------------------------------------------------------------------

/// The playable characters, mapped from the app's character system by the host.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Hero {
    #[default]
    Mario,
    Link,
    YoungLink,
    Banjo,
}

#[derive(Clone, Copy, Debug)]
pub struct AttackShape {
    pub name: &'static str,
    /// Distance in front of the hero (plus the target's radius).
    pub reach: f32,
    /// Half of the cone's opening angle (radians).
    pub half_arc: f32,
    pub mult: f32,
    /// Ticks between swings at +0% attack speed.
    pub cd: u16,
    pub knock: f32,
}

impl Hero {
    pub fn label(self) -> &'static str {
        match self {
            Hero::Mario => "Mario",
            Hero::Link => "Link",
            Hero::YoungLink => "Young Link",
            Hero::Banjo => "Banjo",
        }
    }
    /// Mario punches fast and wide, Link slashes long and heavy, Young Link jabs very quickly, Banjo claws hard.
    pub fn attack(self) -> AttackShape {
        match self {
            Hero::Mario => AttackShape { name: "Punch combo", reach: 200.0, half_arc: 1.2, mult: 1.0, cd: 10, knock: 16.0 },
            Hero::Link => AttackShape { name: "Sword slash", reach: 280.0, half_arc: 1.05, mult: 1.2, cd: 14, knock: 20.0 },
            Hero::YoungLink => AttackShape { name: "Quick stab", reach: 220.0, half_arc: 0.9, mult: 0.9, cd: 8, knock: 12.0 },
            Hero::Banjo => AttackShape { name: "Claw swipe", reach: 230.0, half_arc: 1.35, mult: 1.25, cd: 16, knock: 22.0 },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Skill {
    Slam = 0,
    Whirl = 1,
    Volley = 2,
    Aegis = 3,
}

pub const SKILLS: [Skill; 4] = [Skill::Slam, Skill::Whirl, Skill::Volley, Skill::Aegis];

impl Skill {
    pub fn name(self) -> &'static str {
        ["Quake Slam", "Whirl", "Star Volley", "Aegis"][self as usize]
    }
    pub fn short(self) -> &'static str {
        ["SLAM", "WHIRL", "STARS", "AEGIS"][self as usize]
    }
    pub fn cost(self) -> f32 {
        [0.0, 20.0, 25.0, 30.0][self as usize]
    }
    pub fn base_cooldown_secs(self) -> f32 {
        [4.0, 4.5, 5.0, 14.0][self as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Perk {
    QuakeForce = 0,
    Aftershock,
    Whirl,
    Cyclone,
    Gale,
    Volley,
    Barrage,
    Piercing,
    Aegis,
    Bulwark,
    Reprisal,
    Vigor,
}

pub const NODES: [Perk; 12] = [Perk::QuakeForce, Perk::Aftershock, Perk::Whirl, Perk::Cyclone, Perk::Gale, Perk::Volley, Perk::Barrage, Perk::Piercing, Perk::Aegis, Perk::Bulwark, Perk::Reprisal, Perk::Vigor];

#[derive(Clone, Copy, Debug)]
pub struct PerkInfo {
    pub name: &'static str,
    pub desc: &'static str,
    pub max: u8,
    /// The skill this perk belongs to (its column on the tree page).
    #[allow(dead_code)]
    pub skill: Option<Skill>,
    pub req: Option<Perk>,
}

impl Perk {
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn info(self) -> PerkInfo {
        use Perk::*;
        let n = |name, desc, max, skill, req| PerkInfo { name, desc, max, skill, req };
        match self {
            QuakeForce => n("Quake Force", "Slam hits 25% harder per rank.", 3, Some(Skill::Slam), None),
            Aftershock => n("Aftershock", "Slam rings are 15% wider per rank and stun.", 2, Some(Skill::Slam), Some(QuakeForce)),
            Whirl => n("Whirl", "Unlock: a spin that hits everything close.", 1, Some(Skill::Whirl), None),
            Cyclone => n("Cyclone", "Whirl hits 30% harder per rank.", 3, Some(Skill::Whirl), Some(Whirl)),
            Gale => n("Gale", "Whirl recharges 15% faster per rank.", 2, Some(Skill::Whirl), Some(Whirl)),
            Volley => n("Star Volley", "Unlock: fan of seeking stars.", 1, Some(Skill::Volley), None),
            Barrage => n("Barrage", "+2 stars per rank.", 2, Some(Skill::Volley), Some(Volley)),
            Piercing => n("Piercing", "Stars pass through foes.", 1, Some(Skill::Volley), Some(Volley)),
            Aegis => n("Aegis", "Unlock: a shield that soaks damage.", 1, Some(Skill::Aegis), None),
            Bulwark => n("Bulwark", "Shield is 40% bigger per rank.", 3, Some(Skill::Aegis), Some(Aegis)),
            Reprisal => n("Reprisal", "Damage the shield soaks bursts back at foes.", 1, Some(Skill::Aegis), Some(Aegis)),
            Vigor => n("Vigor", "+10% max health per rank.", 3, None, None),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    pub ranks: [u8; 12],
    pub points: u32,
}

impl Tree {
    pub fn rank(&self, n: Perk) -> u8 {
        self.ranks[n.index()]
    }
    pub fn can_rank(&self, n: Perk) -> bool {
        let info = n.info();
        self.points > 0 && self.rank(n) < info.max && info.req.is_none_or(|r| self.rank(r) > 0)
    }
    pub fn rank_up(&mut self, n: Perk) -> bool {
        if !self.can_rank(n) {
            return false;
        }
        self.ranks[n.index()] += 1;
        self.points -= 1;
        true
    }
    pub fn unlocked(&self, s: Skill) -> bool {
        match s {
            Skill::Slam => true,
            Skill::Whirl => self.rank(Perk::Whirl) > 0,
            Skill::Volley => self.rank(Perk::Volley) > 0,
            Skill::Aegis => self.rank(Perk::Aegis) > 0,
        }
    }
    pub fn spent(&self) -> u32 {
        self.ranks.iter().map(|r| *r as u32).sum()
    }
}

// ---------------------------------------------------------------------------------------------
// Progression and stats
// ---------------------------------------------------------------------------------------------

/// XP needed to go from `level` to `level + 1` (levels start at 1).
pub fn xp_to_next(level: u32) -> u32 {
    30 + 18 * level + 3 * level * level
}

/// Persistent meta bonus from banked gold: +2% damage and health per 500 gold, up to +20%.
pub fn legacy_bonus(stash: u32) -> f32 {
    ((stash / 500).min(10)) as f32 * 0.02
}

pub fn enemy_hp_mult(floor: u32) -> f32 {
    1.0 + 0.5 * floor.saturating_sub(1) as f32
}

pub fn enemy_dmg_mult(floor: u32) -> f32 {
    1.0 + 0.3 * floor.saturating_sub(1) as f32
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub max_hp: f32,
    /// Flat damage before multipliers (unarmed 6 + weapon + damage affixes).
    pub base_damage: f32,
    pub dmg_mult: f32,
    pub crit: f32,
    pub crit_mult: f32,
    pub aspd: f32,
    pub move_bonus: f32,
    pub lifesteal: f32,
    pub cdr: f32,
    pub dodge_ticks: u16,
    pub pickup: f32,
    pub thorns: f32,
    pub defence: f32,
    pub max_spark: f32,
    pub magpie: bool,
    pub tremor: bool,
    pub echo: bool,
    pub lastlight: bool,
}

pub fn derive_stats(level: u32, eq: &Totals, tree: &Tree, legacy: f32) -> Stats {
    let lv = level.max(1) - 1;
    let vigor = tree.rank(Perk::Vigor) as f32;
    let slip = eq.has(Legendary::Slipstream);
    Stats {
        max_hp: ((100.0 + 12.0 * lv as f32 + eq.get(Stat::MaxHp)) * (1.0 + 0.10 * vigor) * (1.0 + legacy)).round(),
        base_damage: 6.0 + eq.weapon_dmg + eq.get(Stat::Damage),
        dmg_mult: (1.0 + 0.08 * lv as f32) * (1.0 + legacy),
        crit: (0.05 + eq.get(Stat::CritChance) / 100.0).min(0.75),
        crit_mult: 1.75,
        aspd: 1.0 + eq.get(Stat::AttackSpeed) / 100.0,
        move_bonus: (eq.boots_move + eq.get(Stat::MoveSpeed)) / 100.0,
        lifesteal: eq.get(Stat::LifeSteal) / 100.0,
        cdr: (eq.get(Stat::CooldownReduction) / 100.0).min(0.5),
        dodge_ticks: 14 + eq.get(Stat::DodgeFrames) as u16 + if slip { 4 } else { 0 },
        pickup: 190.0 * (1.0 + eq.get(Stat::PickupRadius) / 100.0),
        thorns: eq.get(Stat::Thorns) * if eq.has(Legendary::Briar) { 2.0 } else { 1.0 },
        defence: eq.defence,
        max_spark: 60.0 + eq.max_spark,
        magpie: eq.has(Legendary::Magpie),
        tremor: eq.has(Legendary::Tremor),
        echo: eq.has(Legendary::Echo),
        lastlight: eq.has(Legendary::Lastlight),
    }
}

/// One attack's damage: the base scaled by `mult`, +-10% spread, crits multiply.
pub fn roll_damage(stats: &Stats, mult: f32, rng: &mut Rng) -> (f32, bool) {
    let mut d = stats.base_damage * stats.dmg_mult * mult * rng.range(0.9, 1.1);
    let crit = rng.chance(stats.crit);
    if crit {
        d *= stats.crit_mult;
    }
    (d, crit)
}

/// Armour as diminishing returns: 100 defence halves the damage.
pub fn mitigate(dmg: f32, defence: f32) -> f32 {
    dmg * 100.0 / (100.0 + defence.max(0.0))
}

// ---------------------------------------------------------------------------------------------
// Enemies
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum EnemyKind {
    /// Fast, fragile, comes in numbers (Goomba).
    Scamp = 1,
    /// Slow and heavy, slams the ground (Whomp).
    Bruiser = 2,
    /// Winds up, then charges in a line (Spiny).
    Lancer = 3,
    /// Keeps its distance and shoots (Boo).
    Wraith = 4,
    /// Runs in and blows up (Bob-omb).
    Sapper = 5,
    /// Hangs back and summons scamps (Mr. I).
    Seer = 6,
    /// The boss (King Bob-omb).
    King = 7,
}

impl EnemyKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Scamp,
            2 => Self::Bruiser,
            3 => Self::Lancer,
            4 => Self::Wraith,
            5 => Self::Sapper,
            6 => Self::Seer,
            7 => Self::King,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnemyStats {
    pub hp: f32,
    /// Units per tick.
    pub speed: f32,
    pub radius: f32,
    pub dmg: f32,
    pub xp: u32,
}

pub fn enemy_stats(kind: EnemyKind) -> EnemyStats {
    match kind {
        EnemyKind::Scamp => EnemyStats { hp: 26.0, speed: 11.0, radius: 55.0, dmg: 6.0, xp: 6 },
        EnemyKind::Bruiser => EnemyStats { hp: 130.0, speed: 6.0, radius: 100.0, dmg: 22.0, xp: 18 },
        EnemyKind::Lancer => EnemyStats { hp: 55.0, speed: 9.0, radius: 65.0, dmg: 16.0, xp: 12 },
        EnemyKind::Wraith => EnemyStats { hp: 42.0, speed: 8.0, radius: 60.0, dmg: 10.0, xp: 12 },
        EnemyKind::Sapper => EnemyStats { hp: 32.0, speed: 15.0, radius: 60.0, dmg: 28.0, xp: 10 },
        EnemyKind::Seer => EnemyStats { hp: 95.0, speed: 6.0, radius: 70.0, dmg: 8.0, xp: 20 },
        EnemyKind::King => EnemyStats { hp: 1500.0, speed: 7.0, radius: 190.0, dmg: 30.0, xp: 400 },
    }
}

/// Elite modifiers (bit mask on [`Enemy::mods`]).
pub const MOD_SWIFT: u8 = 1;
pub const MOD_STOUT: u8 = 2;
pub const MOD_LEECH: u8 = 4;
pub const MOD_VOLATILE: u8 = 8;
pub const MOD_WARDED: u8 = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum EState {
    Chase = 0,
    Windup = 1,
    Dash = 2,
    Recover = 3,
    Fuse = 4,
}

#[derive(Clone, Copy, Debug)]
pub struct Enemy {
    pub alive: bool,
    pub kind: EnemyKind,
    pub pos: [f32; 2],
    pub prev: [f32; 2],
    pub vel: [f32; 2],
    pub hp: f32,
    pub max_hp: f32,
    pub shield: f32,
    pub state: EState,
    pub timer: u16,
    pub cd: u16,
    pub stun: u16,
    pub flash: u8,
    pub mods: u8,
    pub room: u8,
    pub dir: [f32; 2],
    pub dmg: f32,
    pub speed: f32,
    pub radius: f32,
    pub phase: u8,
    pub invuln: u16,
    pub dive_stamp: u16,
    pub struck: bool,
}

impl Enemy {
    fn blank() -> Self {
        Self { alive: false, kind: EnemyKind::Scamp, pos: [0.0; 2], prev: [0.0; 2], vel: [0.0; 2], hp: 0.0, max_hp: 1.0, shield: 0.0, state: EState::Chase, timer: 0, cd: 0, stun: 0, flash: 0, mods: 0, room: 0, dir: [0.0, 1.0], dmg: 0.0, speed: 0.0, radius: 50.0, phase: 0, invuln: 0, dive_stamp: 0, struck: false }
    }
    pub fn is_elite(&self) -> bool {
        self.mods != 0
    }
    /// A telegraph pose the view can shake or flash.
    pub fn winding_up(&self) -> bool {
        matches!(self.state, EState::Windup | EState::Fuse)
    }
}

/// Boss phase from the hit-point fraction and the phase reached so far (phases never go back).
pub fn boss_phase(hp_frac: f32, current: u8) -> u8 {
    let p = if hp_frac > 0.66 {
        0
    } else if hp_frac > 0.33 {
        1
    } else {
        2
    };
    p.max(current)
}

/// What a room's pack holds: (kind, elite-modifier mask).
#[derive(Clone, Copy, Debug)]
pub struct PackSlot {
    pub kind: EnemyKind,
    pub mods: u8,
}

pub const PACK_MAX: usize = 20;

/// Pack for a room: more and tougher foes per floor, an elite sometimes, always in treasure rooms.
pub fn plan_pack(rng: &mut Rng, floor: u32, kind: RoomKind, area_tiles: usize, out: &mut [PackSlot; PACK_MAX]) -> usize {
    let mut n = 4 + 2 * floor as usize + rng.below(3) as usize + area_tiles / 60;
    if kind == RoomKind::Treasure {
        n += 2;
    }
    n = n.min(PACK_MAX - 2);
    let weights: [f32; 6] = [34.0, 11.0, 14.0, 13.0, 12.0, if floor >= 2 { 8.0 } else { 3.0 }];
    let total: f32 = weights.iter().sum();
    for slot in out.iter_mut().take(n) {
        let mut r = rng.f32() * total;
        let mut k = 0;
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                k = i;
                break;
            }
            r -= *w;
        }
        *slot = PackSlot { kind: EnemyKind::from_u8(k as u8 + 1).unwrap(), mods: 0 };
    }
    let elite_chance = 0.18 + 0.12 * floor as f32;
    if kind == RoomKind::Treasure || rng.chance(elite_chance) {
        let i = rng.below(n as u32) as usize;
        let m = [MOD_SWIFT, MOD_STOUT, MOD_LEECH, MOD_VOLATILE, MOD_WARDED][rng.below(5) as usize];
        out[i].mods = m;
    }
    n
}

// ---------------------------------------------------------------------------------------------
// Entities other than enemies
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub alive: bool,
    pub from_player: bool,
    pub pos: [f32; 2],
    pub prev: [f32; 2],
    pub vel: [f32; 2],
    pub life: u16,
    pub dmg: f32,
    pub radius: f32,
    pub pierce: bool,
    /// Wave shots (Echo) are drawn bigger.
    pub wave: bool,
}

impl Shot {
    fn blank() -> Self {
        Self { alive: false, from_player: false, pos: [0.0; 2], prev: [0.0; 2], vel: [0.0; 2], life: 0, dmg: 0.0, radius: 30.0, pierce: false, wave: false }
    }
}

/// A delayed area attack: shows as a growing red disc, then hurts the player if inside.
#[derive(Clone, Copy, Debug)]
pub struct Hazard {
    pub alive: bool,
    pub pos: [f32; 2],
    pub radius: f32,
    pub delay: u16,
    pub total: u16,
    pub dmg: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RingKind {
    Slash = 1,
    Slam = 2,
    Whirl = 3,
    Aegis = 4,
    Blast = 5,
}

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub alive: bool,
    pub kind: RingKind,
    pub center: [f32; 2],
    pub yaw: f32,
    pub half: f32,
    pub radius: f32,
    pub age: u16,
    pub life: u16,
}

impl Ring {
    pub fn fade(&self) -> f32 {
        1.0 - self.age as f32 / self.life.max(1) as f32
    }
    pub fn grow(&self) -> f32 {
        let t = self.age as f32 / self.life.max(1) as f32;
        0.35 + 0.65 * (1.0 - (1.0 - t) * (1.0 - t))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DropKind {
    Gold,
    Potion,
    Item,
}

#[derive(Clone, Copy, Debug)]
pub struct Drop {
    pub alive: bool,
    pub kind: DropKind,
    pub pos: [f32; 2],
    pub amount: u32,
    pub item: Item,
    pub age: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Chest {
    pub pos: [f32; 2],
    pub opened: bool,
    pub boss: bool,
}

/// Floating combat text: a number or a short tag, drawn above `pos`.
#[derive(Clone, Copy, Debug)]
pub struct Floater {
    pub alive: bool,
    pub pos: [f32; 2],
    pub height: f32,
    pub value: i32,
    /// 0 enemy hit, 1 crit, 2 player hurt, 3 heal, 4 gold, 5 xp-ish tag
    pub kind: u8,
    pub age: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Intent {
    pub attack: bool,
    pub dodge: bool,
    pub skill: [bool; 4],
    pub potion: bool,
}

/// What the host tells the core each tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerFrame {
    pub pos: [f32; 2],
    pub height: f32,
    /// Facing (radians, atan2(x, z)).
    pub face: f32,
    /// Ground pound hit the floor this tick.
    pub pound_land: bool,
    /// The physics put the hero into a dive on their own this tick (running + B).
    pub dive_started: bool,
    /// The physics started a punch/slash/claw this tick (native B).
    pub punch_started: bool,
    pub grounded: bool,
}

/// What happened during the last tick (sound, particles, toasts, physics requests).
#[derive(Clone, Copy, Debug, Default)]
pub struct Events {
    pub swing: bool,
    pub hit: u32,
    pub crit: bool,
    pub kills: u32,
    pub last_kill: [f32; 2],
    pub hurt: bool,
    pub dodged: bool,
    pub dodge_start: bool,
    pub slam: bool,
    pub whirl: bool,
    pub volley: bool,
    pub aegis: bool,
    pub level_up: bool,
    pub drop_rarity: Option<Rarity>,
    pub pickup_gold: u32,
    pub pickup_item: Option<Rarity>,
    pub pickup_potion: bool,
    pub potion_used: bool,
    pub chest: bool,
    pub boss_spawned: bool,
    pub boss_phase: Option<u8>,
    pub boss_killed: bool,
    pub room_cleared: bool,
    pub descended: bool,
    pub died: bool,
    pub won: bool,
    pub bag_full: bool,
    pub lastlight: bool,
    pub blast: bool,
    pub auto_equipped: bool,
    pub summoned: bool,
    /// Physics requests for the glue: a dive, a leap-and-pound, an immediate pound.
    pub want_dive: bool,
    pub want_leap: bool,
    pub want_pound: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub pos: [f32; 2],
    pub prev: [f32; 2],
    pub height: f32,
    pub face: f32,
    pub hp: f32,
    pub spark: f32,
    pub level: u32,
    pub xp: u32,
    pub iframes: u16,
    pub dodge_t: u16,
    pub dodge_cd: u16,
    pub atk_cd: u16,
    pub potion_cd: u16,
    pub skill_cd: [u16; 4],
    pub skill_cd_max: [u16; 4],
    pub shield: f32,
    pub shield_max: f32,
    pub shield_t: u16,
    pub gold: u32,
    pub potions: u32,
    pub swings: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RoomState {
    pub active: bool,
    pub cleared: bool,
    pub visited: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RunSummary {
    pub victory: bool,
    pub floor: u32,
    pub level: u32,
    pub kills: u32,
    pub gold: u32,
    pub items: u32,
    pub best_rarity: u8,
    pub secs: f32,
}

pub struct Relic {
    pub seed: u64,
    pub hero: Hero,
    pub floor: u32,
    pub dungeon: Dungeon,
    pub rooms: Vec<RoomState>,
    pub rng: Rng,
    pub tick: u32,
    pub player: Player,
    pub stats: Stats,
    pub tree: Tree,
    pub equipment: Equipment,
    pub bag: Vec<Item>,
    pub legacy: f32,
    pub filter_commons: bool,
    pub enemies: [Enemy; ENEMY_SLOTS],
    pub shots: [Shot; SHOT_SLOTS],
    pub hazards: [Hazard; HAZARD_SLOTS],
    pub rings: [Ring; RING_SLOTS],
    pub drops: [Drop; DROP_SLOTS],
    pub floaters: [Floater; FLOATER_SLOTS],
    pub chests: [Option<Chest>; CHEST_SLOTS],
    pub ev: Events,
    pub alive_enemies: usize,
    pub kills: u32,
    pub run_gold: u32,
    pub items_found: u32,
    pub best_rarity: u8,
    pub over: bool,
    pub won: bool,
    pub boss: Option<usize>,
    pub boss_dead_at: Option<u32>,
    pub stairs_t: u16,
    pub lastlight_used: bool,
    pub freeze: bool,
    /// Test hook: foes stand still (screenshots).
    pub hold_foes: bool,
    next_item: u32,
    cap: usize,
    dodge_id: u16,
}

impl Relic {
    pub fn new(seed: u64, hero: Hero, start_floor: u32, legacy_stash: u32) -> Self {
        Self::with_cap(seed, hero, start_floor, legacy_stash, enemy_cap())
    }

    pub fn with_cap(seed: u64, hero: Hero, start_floor: u32, legacy_stash: u32, cap: usize) -> Self {
        let floor = start_floor.clamp(1, FLOORS);
        let dungeon = generate(seed, floor, floor == FLOORS);
        let legacy = legacy_bonus(legacy_stash);
        let tree = Tree { points: 2, ..Default::default() };
        let eq = Equipment::default();
        let stats = derive_stats(1, &eq.totals(), &tree, legacy);
        let c = dungeon.rooms[dungeon.start].center();
        let mut r = Self {
            seed,
            hero,
            floor,
            rooms: vec![RoomState::default(); dungeon.rooms.len()],
            dungeon,
            rng: Rng::new(seed ^ 0x5EED),
            tick: 0,
            player: Player { pos: c, prev: c, height: 0.0, face: 0.0, hp: stats.max_hp, spark: stats.max_spark, level: 1, xp: 0, iframes: 0, dodge_t: 0, dodge_cd: 0, atk_cd: 0, potion_cd: 0, skill_cd: [0; 4], skill_cd_max: [0; 4], shield: 0.0, shield_max: 0.0, shield_t: 0, gold: 0, potions: START_POTIONS, swings: 0 },
            stats,
            tree,
            equipment: eq,
            bag: Vec::with_capacity(BAG_CAP),
            legacy,
            filter_commons: true,
            enemies: [Enemy::blank(); ENEMY_SLOTS],
            shots: [Shot::blank(); SHOT_SLOTS],
            hazards: [Hazard { alive: false, pos: [0.0; 2], radius: 0.0, delay: 0, total: 1, dmg: 0.0 }; HAZARD_SLOTS],
            rings: [Ring { alive: false, kind: RingKind::Slash, center: [0.0; 2], yaw: 0.0, half: 0.0, radius: 0.0, age: 0, life: 1 }; RING_SLOTS],
            drops: [Drop { alive: false, kind: DropKind::Gold, pos: [0.0; 2], amount: 0, item: Item { id: 0, slot: Slot::Weapon, rarity: Rarity::Common, ilvl: 1, base_idx: 0, base: 0.0, n: 0, affixes: [Affix { stat: Stat::Damage, value: 0.0 }; MAX_AFFIX], legendary: None }, age: 0 }; DROP_SLOTS],
            floaters: [Floater { alive: false, pos: [0.0; 2], height: 0.0, value: 0, kind: 0, age: 0 }; FLOATER_SLOTS],
            chests: [None; CHEST_SLOTS],
            ev: Events::default(),
            alive_enemies: 0,
            kills: 0,
            run_gold: 0,
            items_found: 0,
            best_rarity: 0,
            over: false,
            won: false,
            boss: None,
            boss_dead_at: None,
            stairs_t: 0,
            lastlight_used: false,
            freeze: false,
            hold_foes: false,
            next_item: 1,
            cap,
            dodge_id: 0,
        };
        r.setup_floor();
        r
    }

    pub fn secs(&self) -> f32 {
        self.tick as f32 / TICKS_PER_SEC as f32
    }

    pub fn start_pos(&self) -> [f32; 2] {
        self.dungeon.rooms[self.dungeon.start].center()
    }

    pub fn stairs_pos(&self) -> Option<[f32; 2]> {
        if self.dungeon.boss_floor { None } else { Some(self.dungeon.rooms[self.dungeon.exit].center()) }
    }

    #[allow(dead_code)]
    pub fn cap(&self) -> usize {
        self.cap
    }

    pub fn boss_enemy(&self) -> Option<&Enemy> {
        self.boss.map(|i| &self.enemies[i]).filter(|e| e.alive)
    }

    pub fn summary(&self) -> RunSummary {
        RunSummary { victory: self.won, floor: self.floor, level: self.player.level, kills: self.kills, gold: self.run_gold, items: self.items_found, best_rarity: self.best_rarity, secs: self.secs() }
    }

    /// Fresh floor state (called on creation and after the stairs).
    fn setup_floor(&mut self) {
        self.enemies = [Enemy::blank(); ENEMY_SLOTS];
        self.shots = [Shot::blank(); SHOT_SLOTS];
        for h in self.hazards.iter_mut() {
            h.alive = false;
        }
        for r in self.rings.iter_mut() {
            r.alive = false;
        }
        for d in self.drops.iter_mut() {
            d.alive = false;
        }
        for f in self.floaters.iter_mut() {
            f.alive = false;
        }
        self.rooms = vec![RoomState::default(); self.dungeon.rooms.len()];
        self.rooms[self.dungeon.start].visited = true;
        self.rooms[self.dungeon.start].cleared = true;
        self.rooms[self.dungeon.start].active = true;
        self.chests = [None; CHEST_SLOTS];
        let mut n = 0;
        for r in &self.dungeon.rooms {
            if r.kind == RoomKind::Treasure && n < CHEST_SLOTS {
                self.chests[n] = Some(Chest { pos: r.center(), opened: false, boss: false });
                n += 1;
            }
        }
        self.boss = None;
        self.boss_dead_at = None;
        self.stairs_t = 0;
        self.lastlight_used = false;
        self.alive_enemies = 0;
        let c = self.start_pos();
        self.player.pos = c;
        self.player.prev = c;
    }

    pub fn paused_for_ui(&self) -> bool {
        self.freeze
    }

    // -----------------------------------------------------------------------------------------
    // Player
    // -----------------------------------------------------------------------------------------

    fn refresh_stats(&mut self) {
        let old_max = self.stats.max_hp;
        self.stats = derive_stats(self.player.level, &self.equipment.totals(), &self.tree, self.legacy);
        let gain = self.stats.max_hp - old_max;
        if gain > 0.0 {
            self.player.hp += gain;
        }
        self.player.hp = self.player.hp.min(self.stats.max_hp);
        self.player.spark = self.player.spark.min(self.stats.max_spark);
    }

    pub fn add_xp(&mut self, amount: u32) {
        if self.player.level >= MAX_LEVEL {
            return;
        }
        self.player.xp += amount;
        while self.player.level < MAX_LEVEL && self.player.xp >= xp_to_next(self.player.level) {
            self.player.xp -= xp_to_next(self.player.level);
            self.player.level += 1;
            self.tree.points += 1;
            self.ev.level_up = true;
            let old = self.stats.max_hp;
            self.refresh_stats();
            // A level heals a little on top of the max-health gain.
            self.player.hp = (self.player.hp + self.stats.max_hp * 0.25).min(self.stats.max_hp);
            let _ = old;
        }
        if self.player.level >= MAX_LEVEL {
            self.player.xp = 0;
        }
    }

    fn cd_ticks(&self, secs: f32, extra_cut: f32) -> u16 {
        (secs * TICKS_PER_SEC as f32 * (1.0 - self.stats.cdr) * (1.0 - extra_cut).max(0.2)) as u16
    }

    /// Hurt the player. Returns whether damage landed (false during i-frames).
    pub fn hurt_player(&mut self, raw: f32) -> bool {
        if self.over {
            return false;
        }
        if self.player.iframes > 0 {
            if self.player.dodge_t > 0 {
                self.ev.dodged = true;
            }
            return false;
        }
        let mut dmg = mitigate(raw, self.stats.defence);
        if self.player.shield > 0.0 {
            let soak = dmg.min(self.player.shield);
            self.player.shield -= soak;
            dmg -= soak;
            if self.tree.rank(Perk::Reprisal) > 0 && soak > 0.0 {
                self.reprisal(soak);
            }
            if self.player.shield <= 0.0 {
                self.player.shield_t = 0;
            }
        }
        self.player.iframes = HIT_IFRAMES;
        if dmg <= 0.0 {
            self.add_floater(self.player.pos, 0.0, 0, 2);
            return true;
        }
        self.player.hp -= dmg;
        self.ev.hurt = true;
        self.add_floater(self.player.pos, 0.0, dmg.round() as i32, 2);
        if self.player.hp <= 0.0 {
            if self.stats.lastlight && !self.lastlight_used {
                self.lastlight_used = true;
                self.player.hp = self.stats.max_hp * 0.3;
                self.player.iframes = 60;
                self.ev.lastlight = true;
            } else {
                self.player.hp = 0.0;
                self.over = true;
                self.ev.died = true;
            }
        }
        true
    }

    fn reprisal(&mut self, soaked: f32) {
        let p = self.player.pos;
        for i in 0..ENEMY_SLOTS {
            let e = self.enemies[i];
            if e.alive && dist(e.pos, p) < 420.0 {
                self.damage_enemy(i, soaked * 1.5, p, false, 8.0);
            }
        }
        self.push_ring(RingKind::Blast, p, 0.0, 0.0, 420.0, 10);
    }

    pub fn heal(&mut self, amount: f32) {
        let before = self.player.hp;
        self.player.hp = (self.player.hp + amount).min(self.stats.max_hp);
        let got = (self.player.hp - before).round() as i32;
        if got > 0 {
            self.add_floater(self.player.pos, 0.0, got, 3);
        }
    }

    pub fn use_potion(&mut self) -> bool {
        if self.player.potions == 0 || self.player.potion_cd > 0 || self.player.hp >= self.stats.max_hp || self.over {
            return false;
        }
        self.player.potions -= 1;
        self.player.potion_cd = 45;
        self.heal(self.stats.max_hp * 0.45);
        self.ev.potion_used = true;
        true
    }

    pub fn skill_ready(&self, s: Skill) -> bool {
        self.tree.unlocked(s) && self.player.skill_cd[s as usize] == 0 && self.player.spark >= s.cost()
    }

    fn start_cooldown(&mut self, s: Skill, extra_cut: f32) {
        let t = self.cd_ticks(s.base_cooldown_secs(), extra_cut);
        self.player.skill_cd[s as usize] = t;
        self.player.skill_cd_max[s as usize] = t.max(1);
    }

    /// Cooldown fraction left (0 = ready).
    pub fn skill_cd_frac(&self, s: Skill) -> f32 {
        let i = s as usize;
        if self.player.skill_cd_max[i] == 0 { 0.0 } else { self.player.skill_cd[i] as f32 / self.player.skill_cd_max[i] as f32 }
    }

    fn nearest_enemy(&self, from: [f32; 2], max: f32) -> Option<usize> {
        let mut best = None;
        let mut bd = max;
        for (i, e) in self.enemies.iter().enumerate() {
            if e.alive {
                let d = dist(e.pos, from);
                if d < bd {
                    bd = d;
                    best = Some(i);
                }
            }
        }
        best
    }

    /// The core swing: a cone in front of the hero. Returns the number of foes hit.
    pub fn swing(&mut self) -> u32 {
        let shape = self.hero.attack();
        self.player.swings += 1;
        self.ev.swing = true;
        let face = self.player.face;
        let p = self.player.pos;
        self.push_ring(RingKind::Slash, p, face, shape.half_arc, shape.reach, 7);
        let mut hits = 0;
        for i in 0..ENEMY_SLOTS {
            let e = self.enemies[i];
            if !e.alive {
                continue;
            }
            let d = dist(e.pos, p);
            if d > shape.reach + e.radius {
                continue;
            }
            let to = (e.pos[0] - p[0]).atan2(e.pos[1] - p[1]);
            if d > e.radius + 60.0 && angle_diff(to, face).abs() > shape.half_arc {
                continue;
            }
            let (dmg, crit) = roll_damage(&self.stats, shape.mult, &mut self.rng);
            self.damage_enemy(i, dmg, p, crit, shape.knock);
            self.player.spark = (self.player.spark + 2.5).min(self.stats.max_spark);
            hits += 1;
        }
        if self.stats.echo && self.player.swings % 4 == 0 {
            let (dmg, _) = roll_damage(&self.stats, 1.1, &mut self.rng);
            self.spawn_shot(true, p, [face.sin() * 22.0, face.cos() * 22.0], dmg, 70.0, 26, true, true);
        }
        hits
    }

    /// Try the dodge: starts the i-frames; the host puts the hero into a dive.
    pub fn try_dodge(&mut self) -> bool {
        if self.player.dodge_cd > 0 || self.over {
            return false;
        }
        self.player.dodge_t = self.stats.dodge_ticks;
        self.player.iframes = self.player.iframes.max(self.stats.dodge_ticks);
        self.player.dodge_cd = if self.equipment_has_slipstream() { DODGE_COOLDOWN / 2 } else { DODGE_COOLDOWN } + self.stats.dodge_ticks / 2;
        self.dodge_id = self.dodge_id.wrapping_add(1);
        self.ev.dodge_start = true;
        true
    }

    fn equipment_has_slipstream(&self) -> bool {
        self.equipment.totals().has(Legendary::Slipstream)
    }

    /// The skill buttons. `Slam` is physical: grounded it asks for a leap, airborne a pound.
    pub fn use_skill(&mut self, s: Skill, grounded: bool) -> bool {
        if self.over || !self.skill_ready(s) {
            return false;
        }
        let p = self.player.pos;
        match s {
            Skill::Slam => {
                if grounded {
                    self.ev.want_leap = true;
                } else {
                    self.ev.want_pound = true;
                }
                return true;
            }
            Skill::Whirl => {
                self.player.spark -= s.cost();
                let r = 360.0;
                let mult = 1.6 * (1.0 + 0.3 * self.tree.rank(Perk::Cyclone) as f32);
                for i in 0..ENEMY_SLOTS {
                    let e = self.enemies[i];
                    if e.alive && dist(e.pos, p) < r + e.radius {
                        let (dmg, crit) = roll_damage(&self.stats, mult, &mut self.rng);
                        self.damage_enemy(i, dmg, p, crit, 22.0);
                    }
                }
                self.push_ring(RingKind::Whirl, p, 0.0, 0.0, r, 12);
                self.ev.whirl = true;
                self.start_cooldown(s, 0.15 * self.tree.rank(Perk::Gale) as f32);
            }
            Skill::Volley => {
                self.player.spark -= s.cost();
                let n = 5 + 2 * self.tree.rank(Perk::Barrage) as usize;
                let aim = self.nearest_enemy(p, 1800.0).map_or(self.player.face, |i| (self.enemies[i].pos[0] - p[0]).atan2(self.enemies[i].pos[1] - p[1]));
                let pierce = self.tree.rank(Perk::Piercing) > 0;
                for k in 0..n {
                    let a = aim + (k as f32 - (n as f32 - 1.0) / 2.0) * (0.9 / n.max(2) as f32 * 2.0).min(0.34);
                    let (dmg, _) = roll_damage(&self.stats, 0.9, &mut self.rng);
                    self.spawn_shot(true, p, [a.sin() * 26.0, a.cos() * 26.0], dmg, 50.0, 40, pierce, false);
                }
                self.ev.volley = true;
                self.start_cooldown(s, 0.0);
            }
            Skill::Aegis => {
                self.player.spark -= s.cost();
                let cap = self.stats.max_hp * 0.4 * (1.0 + 0.4 * self.tree.rank(Perk::Bulwark) as f32);
                self.player.shield = cap;
                self.player.shield_max = cap;
                self.player.shield_t = 6 * TICKS_PER_SEC as u16;
                self.push_ring(RingKind::Aegis, p, 0.0, 0.0, 200.0, 14);
                self.ev.aegis = true;
                self.start_cooldown(s, 0.0);
            }
        }
        true
    }

    /// The ground pound landed: the Quake Slam shockwave (when it is off cooldown).
    fn slam_land(&mut self) {
        if self.player.skill_cd[0] > 0 {
            return;
        }
        let p = self.player.pos;
        let aft = self.tree.rank(Perk::Aftershock) as f32;
        let trem = if self.stats.tremor { 1.4 } else { 1.0 };
        let r = (380.0 + 0.0 * aft) * (1.0 + 0.15 * aft) * if self.stats.tremor { 1.25 } else { 1.0 };
        let mult = 2.4 * (1.0 + 0.25 * self.tree.rank(Perk::QuakeForce) as f32) * trem;
        for i in 0..ENEMY_SLOTS {
            let e = self.enemies[i];
            if e.alive && dist(e.pos, p) < r + e.radius {
                let (dmg, crit) = roll_damage(&self.stats, mult, &mut self.rng);
                self.damage_enemy(i, dmg, p, crit, 30.0);
                if aft > 0.0 {
                    self.enemies[i].stun = 24 + 12 * aft as u16;
                }
            }
        }
        self.push_ring(RingKind::Slam, p, 0.0, 0.0, r, 16);
        self.ev.slam = true;
        self.start_cooldown(Skill::Slam, 0.0);
    }

    // -----------------------------------------------------------------------------------------
    // Damage and kills
    // -----------------------------------------------------------------------------------------

    /// Deal `dmg` to enemy `i`: shields, boss phase guard, flash, knockback away from `from`.
    pub fn damage_enemy(&mut self, i: usize, dmg: f32, from: [f32; 2], crit: bool, knock: f32) {
        let e = &mut self.enemies[i];
        if !e.alive || e.invuln > 0 {
            return;
        }
        let mut d = dmg;
        if e.shield > 0.0 {
            let soak = d.min(e.shield);
            e.shield -= soak;
            d -= soak;
        }
        e.hp -= d;
        e.flash = 4;
        if e.kind != EnemyKind::King {
            let dir = norm([e.pos[0] - from[0], e.pos[1] - from[1]]);
            e.vel[0] += dir[0] * knock;
            e.vel[1] += dir[1] * knock;
            if knock > 0.0 && e.stun < 6 {
                e.stun = 6;
            }
        }
        let pos = e.pos;
        let height = e.radius * 1.6;
        self.ev.hit += 1;
        self.ev.crit |= crit;
        self.add_floater(pos, height, dmg.round().max(1.0) as i32, if crit { 1 } else { 0 });
        let heal = dmg * self.stats.lifesteal;
        if heal > 0.0 {
            self.player.hp = (self.player.hp + heal).min(self.stats.max_hp);
        }
        let e = &mut self.enemies[i];
        if e.kind == EnemyKind::King {
            let frac = (e.hp / e.max_hp).max(0.0);
            let next = boss_phase(frac, e.phase);
            if next > e.phase && e.hp > 0.0 {
                e.phase = next;
                e.invuln = 36;
                e.state = EState::Chase;
                e.timer = 0;
                e.cd = 30;
                self.ev.boss_phase = Some(next);
                self.boss_phase_summons(i, next);
            }
        }
        if self.enemies[i].hp <= 0.0 {
            self.kill_enemy(i);
        }
    }

    fn boss_phase_summons(&mut self, boss: usize, phase: u8) {
        let pos = self.enemies[boss].pos;
        let room = self.enemies[boss].room;
        let kinds: &[EnemyKind] = if phase == 1 { &[EnemyKind::Scamp, EnemyKind::Scamp, EnemyKind::Scamp] } else { &[EnemyKind::Sapper, EnemyKind::Sapper, EnemyKind::Wraith, EnemyKind::Wraith] };
        for (k, kind) in kinds.iter().enumerate() {
            let a = k as f32 * 2.1 + 0.5;
            let p = [pos[0] + a.sin() * 420.0, pos[1] + a.cos() * 420.0];
            let p = if self.dungeon.circle_free(p, 70.0) { p } else { pos };
            self.spawn_enemy(*kind, p, room, 0);
        }
        self.ev.summoned = true;
    }

    fn kill_enemy(&mut self, i: usize) {
        let e = self.enemies[i];
        self.enemies[i].alive = false;
        self.alive_enemies = self.alive_enemies.saturating_sub(1);
        self.kills += 1;
        self.ev.kills += 1;
        self.ev.last_kill = e.pos;
        let xp_mult = 1.0 + 0.25 * self.floor.saturating_sub(1) as f32;
        let elite = if e.is_elite() { 3 } else { 1 };
        let xp = (enemy_stats(e.kind).xp as f32 * xp_mult) as u32 * elite;
        self.add_xp(xp);
        if e.mods & MOD_VOLATILE != 0 && e.kind != EnemyKind::King {
            self.add_hazard(e.pos, 280.0, 0, e.dmg * 1.2);
        }
        // Loot.
        let floor = self.floor;
        let level = self.player.level;
        let magpie = if self.stats.magpie { 1.5 } else { 1.0 };
        if e.kind == EnemyKind::King {
            self.ev.boss_killed = true;
            self.boss_dead_at = Some(self.tick);
            for k in 0..3 {
                let a = k as f32 * 2.0;
                let p = [e.pos[0] + a.sin() * 220.0, e.pos[1] + a.cos() * 220.0];
                self.drop_item(p, DropSource::Boss, floor, level);
            }
            let gold = ((120 + 60 * floor) as f32 * magpie) as u32;
            self.drop_gold(e.pos, gold);
            self.drop_potion(e.pos);
            self.drop_potion([e.pos[0] + 120.0, e.pos[1]]);
            // The relic chest.
            for c in self.chests.iter_mut() {
                if c.is_none() {
                    *c = Some(Chest { pos: [e.pos[0], e.pos[1] - 380.0], opened: false, boss: true });
                    break;
                }
            }
        } else {
            let g_chance = if e.is_elite() { 1.0 } else { 0.55 };
            if self.rng.chance(g_chance) {
                let amt = ((3 + self.rng.below(5) + 2 * floor) as f32 * if e.is_elite() { 3.0 } else { 1.0 } * magpie) as u32;
                self.drop_gold(e.pos, amt);
            }
            let item_chance = if e.is_elite() { 1.0 } else { 0.11 };
            if self.rng.chance(item_chance) {
                let src = if e.is_elite() { DropSource::Elite } else { DropSource::Normal };
                self.drop_item(e.pos, src, floor, level);
                if e.is_elite() && self.rng.chance(0.5) {
                    self.drop_item(e.pos, DropSource::Normal, floor, level);
                }
            }
            let p_chance = 0.06 * if self.stats.magpie { 1.8 } else { 1.0 } + if e.is_elite() { 0.2 } else { 0.0 };
            if self.rng.chance(p_chance) {
                self.drop_potion(e.pos);
            }
        }
        // Room cleared?
        let room = e.room as usize;
        if room < self.rooms.len() && self.rooms[room].active && !self.rooms[room].cleared && !self.enemies.iter().any(|x| x.alive && x.room as usize == room) {
            self.rooms[room].cleared = true;
            self.ev.room_cleared = true;
            let c = self.dungeon.rooms[room].center();
            let amt = ((10 + 5 * floor) as f32 * magpie) as u32;
            self.drop_gold(c, amt);
        }
    }

    fn free_slot<T>(list: &[T], alive: impl Fn(&T) -> bool) -> Option<usize> {
        list.iter().position(|x| !alive(x))
    }

    fn drop_gold(&mut self, pos: [f32; 2], amount: u32) {
        let Some(i) = Self::free_slot(&self.drops, |d| d.alive) else {
            self.player.gold += amount;
            self.run_gold += amount;
            return;
        };
        let jitter = self.scatter(60.0);
        self.drops[i].alive = true;
        self.drops[i].kind = DropKind::Gold;
        self.drops[i].pos = self.nudge(pos, jitter);
        self.drops[i].amount = amount;
        self.drops[i].age = 0;
    }

    fn drop_potion(&mut self, pos: [f32; 2]) {
        let Some(i) = Self::free_slot(&self.drops, |d| d.alive) else { return };
        let jitter = self.scatter(80.0);
        self.drops[i].alive = true;
        self.drops[i].kind = DropKind::Potion;
        self.drops[i].pos = self.nudge(pos, jitter);
        self.drops[i].amount = 1;
        self.drops[i].age = 0;
    }

    /// Roll an item and put it on the ground (commons become gold when the filter is on).
    pub fn drop_item(&mut self, pos: [f32; 2], source: DropSource, floor: u32, level: u32) {
        let id = self.next_item;
        self.next_item += 1;
        let item = roll_drop(&mut self.rng, id, source, floor, level);
        self.place_item(pos, item);
    }

    pub fn place_item(&mut self, pos: [f32; 2], item: Item) {
        if self.filter_commons && item.rarity == Rarity::Common {
            let g = item.sell_value();
            self.drop_gold(pos, g);
            return;
        }
        let Some(i) = Self::free_slot(&self.drops, |d| d.alive) else {
            // No room on the floor: it goes straight to the vendor.
            self.player.gold += item.sell_value();
            self.run_gold += item.sell_value();
            return;
        };
        let jitter = self.scatter(110.0);
        self.drops[i] = Drop { alive: true, kind: DropKind::Item, pos: self.nudge(pos, jitter), amount: 1, item, age: 0 };
        self.ev.drop_rarity = Some(self.ev.drop_rarity.map_or(item.rarity, |r| r.max(item.rarity)));
    }

    fn scatter(&mut self, r: f32) -> [f32; 2] {
        [self.rng.range(-r, r), self.rng.range(-r, r)]
    }

    fn nudge(&self, pos: [f32; 2], j: [f32; 2]) -> [f32; 2] {
        let p = [pos[0] + j[0], pos[1] + j[1]];
        if self.dungeon.circle_free(p, 30.0) { p } else { pos }
    }

    fn add_floater(&mut self, pos: [f32; 2], height: f32, value: i32, kind: u8) {
        let i = Self::free_slot(&self.floaters, |f| f.alive).unwrap_or((self.tick as usize) % FLOATER_SLOTS);
        self.floaters[i] = Floater { alive: true, pos, height, value, kind, age: 0 };
    }

    fn push_ring(&mut self, kind: RingKind, center: [f32; 2], yaw: f32, half: f32, radius: f32, life: u16) {
        let i = Self::free_slot(&self.rings, |r| r.alive).unwrap_or(0);
        self.rings[i] = Ring { alive: true, kind, center, yaw, half, radius, age: 0, life };
    }

    fn add_hazard(&mut self, pos: [f32; 2], radius: f32, delay: u16, dmg: f32) {
        if let Some(i) = Self::free_slot(&self.hazards, |h| h.alive) {
            self.hazards[i] = Hazard { alive: true, pos, radius, delay, total: delay.max(1), dmg };
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_shot(&mut self, from_player: bool, pos: [f32; 2], vel: [f32; 2], dmg: f32, radius: f32, life: u16, pierce: bool, wave: bool) {
        if let Some(i) = Self::free_slot(&self.shots, |s| s.alive) {
            self.shots[i] = Shot { alive: true, from_player, pos, prev: pos, vel, life, dmg, radius, pierce, wave };
        }
    }

    // -----------------------------------------------------------------------------------------
    // Spawning
    // -----------------------------------------------------------------------------------------

    /// Put an enemy in a free slot. `mods` are elite modifiers. Returns the slot.
    pub fn spawn_enemy(&mut self, kind: EnemyKind, pos: [f32; 2], room: u8, mods: u8) -> Option<usize> {
        if self.alive_enemies >= self.cap {
            return None;
        }
        let i = Self::free_slot(&self.enemies, |e| e.alive)?;
        let st = enemy_stats(kind);
        let hp_mult = enemy_hp_mult(self.floor) * if mods & MOD_STOUT != 0 { 2.0 } else { 1.0 };
        let hp = st.hp * hp_mult;
        let mut e = Enemy::blank();
        e.alive = true;
        e.kind = kind;
        e.pos = pos;
        e.prev = pos;
        e.hp = hp;
        e.max_hp = hp;
        e.mods = mods;
        e.room = room;
        e.dmg = st.dmg * enemy_dmg_mult(self.floor);
        e.speed = st.speed * if mods & MOD_SWIFT != 0 { 1.4 } else { 1.0 };
        e.radius = st.radius * if mods != 0 { 1.2 } else { 1.0 };
        e.cd = 20 + self.rng.below(40) as u16;
        if mods & MOD_WARDED != 0 {
            e.shield = hp * 0.6;
            e.max_hp = hp + e.shield;
            e.hp = hp;
        }
        self.enemies[i] = e;
        self.alive_enemies += 1;
        Some(i)
    }

    fn activate_room(&mut self, r: usize) {
        self.rooms[r].active = true;
        self.rooms[r].visited = true;
        let room = self.dungeon.rooms[r].clone();
        let mut rng = Rng::new(self.seed ^ (self.floor as u64) << 32 ^ (r as u64 + 1) * 0x9E37);
        if room.kind == RoomKind::Boss {
            let k = self.spawn_enemy(EnemyKind::King, room.center(), r as u8, 0);
            if let Some(i) = k {
                let e = &mut self.enemies[i];
                e.hp *= 1.0 + 0.0;
                self.boss = Some(i);
                self.ev.boss_spawned = true;
                e.cd = 45;
            }
            return;
        }
        let mut pack = [PackSlot { kind: EnemyKind::Scamp, mods: 0 }; PACK_MAX];
        let n = plan_pack(&mut rng, self.floor, room.kind, room.w * room.h, &mut pack);
        let me = self.player.pos;
        let c = room.center();
        let h = room.half();
        for slot in pack.iter().take(n) {
            // A free spot at least 650 units from the player.
            let mut placed = false;
            for _ in 0..12 {
                let p = [c[0] + rng.range(-h[0] + 180.0, h[0] - 180.0), c[1] + rng.range(-h[1] + 180.0, h[1] - 180.0)];
                if dist(p, me) > 650.0 && self.dungeon.circle_free(p, 90.0) {
                    self.spawn_enemy(slot.kind, p, r as u8, slot.mods);
                    placed = true;
                    break;
                }
            }
            if !placed {
                self.spawn_enemy(slot.kind, c, r as u8, slot.mods);
            }
        }
        if self.alive_enemies == 0 {
            self.rooms[r].cleared = true;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Inventory commands (called by the UI layer)
    // -----------------------------------------------------------------------------------------

    pub fn equip_from_bag(&mut self, id: u32) -> bool {
        let Some(pos) = self.bag.iter().position(|i| i.id == id) else { return false };
        let item = self.bag.remove(pos);
        if let Some(old) = self.equipment.equip(item) {
            self.bag.push(old);
        }
        self.refresh_stats();
        true
    }

    pub fn unequip(&mut self, slot: usize) -> bool {
        if slot >= EQUIP_SLOTS || self.bag.len() >= BAG_CAP {
            return false;
        }
        let Some(item) = self.equipment.slots[slot].take() else { return false };
        self.bag.push(item);
        self.refresh_stats();
        true
    }

    pub fn sell_from_bag(&mut self, id: u32) -> bool {
        let Some(pos) = self.bag.iter().position(|i| i.id == id) else { return false };
        let item = self.bag.remove(pos);
        self.player.gold += item.sell_value();
        self.run_gold += item.sell_value();
        true
    }

    /// Sell every bag item of `max` rarity or lower. Returns the gold made.
    pub fn sell_up_to(&mut self, max: Rarity) -> u32 {
        let mut gold = 0;
        self.bag.retain(|i| {
            if i.rarity <= max {
                gold += i.sell_value();
                false
            } else {
                true
            }
        });
        self.player.gold += gold;
        self.run_gold += gold;
        gold
    }

    pub fn rank_up(&mut self, n: Perk) -> bool {
        let ok = self.tree.rank_up(n);
        if ok {
            self.refresh_stats();
        }
        ok
    }

    /// Give an item straight to the bag (test hook and chest rewards); equips into an empty slot.
    pub fn give_item(&mut self, item: Item) -> bool {
        let target = self.equipment.target(item.slot);
        if self.equipment.slots[target].is_none() {
            self.equipment.slots[target] = Some(item);
            self.refresh_stats();
            self.ev.auto_equipped = true;
            return true;
        }
        if self.bag.len() >= BAG_CAP {
            return false;
        }
        self.bag.push(item);
        true
    }

    pub fn new_item_id(&mut self) -> u32 {
        let id = self.next_item;
        self.next_item += 1;
        id
    }

    // -----------------------------------------------------------------------------------------
    // Tick
    // -----------------------------------------------------------------------------------------

    pub fn tick(&mut self, f: &PlayerFrame, intent: &Intent) {
        self.ev = Events::default();
        if self.over {
            return;
        }
        self.tick += 1;
        let p = &mut self.player;
        p.prev = p.pos;
        p.pos = f.pos;
        p.height = f.height;
        p.face = f.face;
        for t in [&mut p.iframes, &mut p.dodge_t, &mut p.dodge_cd, &mut p.atk_cd, &mut p.potion_cd] {
            *t = t.saturating_sub(1);
        }
        for t in p.skill_cd.iter_mut() {
            *t = t.saturating_sub(1);
        }
        if p.shield_t > 0 {
            p.shield_t -= 1;
            if p.shield_t == 0 {
                p.shield = 0.0;
            }
        }
        p.spark = (p.spark + 5.0 / TICKS_PER_SEC as f32).min(self.stats.max_spark);

        // Intents.
        if intent.potion {
            self.use_potion();
        }
        for s in SKILLS {
            if intent.skill[s as usize] {
                self.use_skill(s, f.grounded);
            }
        }
        if intent.dodge || f.dive_started {
            if self.try_dodge() && intent.dodge && !f.dive_started {
                self.ev.want_dive = true;
            }
        }
        if (intent.attack || f.punch_started) && self.player.atk_cd == 0 {
            self.swing();
            self.player.atk_cd = ((self.hero.attack().cd as f32) / self.stats.aspd).max(3.0) as u16;
        }
        if f.pound_land {
            self.slam_land();
        }

        self.update_rooms();
        self.update_enemies();
        self.update_shots();
        self.update_hazards();
        self.update_effects();
        self.update_pickups();
        self.update_stairs();
        if let Some(t) = self.boss_dead_at {
            if self.tick >= t + 120 && !self.won {
                self.won = true;
                self.over = true;
                self.ev.won = true;
            }
        }
    }

    fn update_rooms(&mut self) {
        let p = self.player.pos;
        for r in 0..self.rooms.len() {
            if !self.rooms[r].active && self.dungeon.rooms[r].contains(p, -TILE * 0.6) {
                self.activate_room(r);
            }
            if self.dungeon.rooms[r].contains(p, 0.0) {
                self.rooms[r].visited = true;
            }
        }
    }

    fn update_effects(&mut self) {
        for r in self.rings.iter_mut().filter(|r| r.alive) {
            r.age += 1;
            if r.age >= r.life {
                r.alive = false;
            }
        }
        for f in self.floaters.iter_mut().filter(|f| f.alive) {
            f.age += 1;
            if f.age > 34 {
                f.alive = false;
            }
        }
        for d in self.drops.iter_mut().filter(|d| d.alive) {
            d.age = d.age.saturating_add(1);
        }
    }

    fn update_shots(&mut self) {
        for i in 0..SHOT_SLOTS {
            if !self.shots[i].alive {
                continue;
            }
            let s = &mut self.shots[i];
            s.prev = s.pos;
            s.pos[0] += s.vel[0];
            s.pos[1] += s.vel[1];
            s.life = s.life.saturating_sub(1);
            let (pos, radius, from_player, dmg, pierce) = (s.pos, s.radius, s.from_player, s.dmg, s.pierce);
            if s.life == 0 || !self.dungeon.walkable(pos) {
                self.shots[i].alive = false;
                continue;
            }
            if from_player {
                for j in 0..ENEMY_SLOTS {
                    let e = self.enemies[j];
                    if e.alive && dist(e.pos, pos) < radius + e.radius {
                        self.damage_enemy(j, dmg, pos, false, 8.0);
                        if !pierce {
                            self.shots[i].alive = false;
                            break;
                        }
                    }
                }
            } else if dist(pos, self.player.pos) < radius + PLAYER_RADIUS && self.hurt_player(dmg) {
                self.shots[i].alive = false;
            }
        }
    }

    fn update_hazards(&mut self) {
        for i in 0..HAZARD_SLOTS {
            if !self.hazards[i].alive {
                continue;
            }
            if self.hazards[i].delay > 0 {
                self.hazards[i].delay -= 1;
                continue;
            }
            let h = self.hazards[i];
            self.hazards[i].alive = false;
            self.push_ring(RingKind::Blast, h.pos, 0.0, 0.0, h.radius, 10);
            self.ev.blast = true;
            if dist(h.pos, self.player.pos) < h.radius + PLAYER_RADIUS {
                self.hurt_player(h.dmg);
            }
        }
    }

    fn update_stairs(&mut self) {
        let Some(s) = self.stairs_pos() else { return };
        if dist(s, self.player.pos) < 170.0 {
            self.stairs_t += 1;
            if self.stairs_t >= 24 {
                self.descend();
            }
        } else {
            self.stairs_t = 0;
        }
    }

    /// Take the stairs: a new floor (the next seed), same hero.
    pub fn descend(&mut self) {
        if self.floor >= FLOORS {
            return;
        }
        self.floor += 1;
        self.dungeon = generate(self.seed, self.floor, self.floor == FLOORS);
        self.setup_floor();
        self.ev.descended = true;
        self.player.iframes = 45;
    }

    fn update_pickups(&mut self) {
        let me = self.player.pos;
        let radius = self.stats.pickup;
        for i in 0..DROP_SLOTS {
            let d = self.drops[i];
            if !d.alive || d.age < 8 {
                continue;
            }
            let dd = dist(d.pos, me);
            if dd > radius {
                continue;
            }
            match d.kind {
                DropKind::Gold => {
                    self.player.gold += d.amount;
                    self.run_gold += d.amount;
                    self.ev.pickup_gold += d.amount;
                    self.add_floater(me, 40.0, d.amount as i32, 4);
                }
                DropKind::Potion => {
                    if self.player.potions >= POTION_MAX {
                        continue;
                    }
                    self.player.potions += 1;
                    self.ev.pickup_potion = true;
                }
                DropKind::Item => {
                    if !self.give_item(d.item) {
                        self.ev.bag_full = true;
                        continue;
                    }
                    self.items_found += 1;
                    self.best_rarity = self.best_rarity.max(d.item.rarity as u8);
                    self.ev.pickup_item = Some(self.ev.pickup_item.map_or(d.item.rarity, |r| r.max(d.item.rarity)));
                }
            }
            self.drops[i].alive = false;
        }
        // Chests open when walked into.
        for c in 0..CHEST_SLOTS {
            let Some(ch) = self.chests[c] else { continue };
            if ch.opened || dist(ch.pos, me) > 170.0 {
                continue;
            }
            // The treasure room's guards must be dead first.
            if let Some(r) = self.dungeon.room_at(ch.pos) {
                if !ch.boss && self.rooms[r].active && !self.rooms[r].cleared {
                    continue;
                }
            }
            self.chests[c] = Some(Chest { opened: true, ..ch });
            self.ev.chest = true;
            let (floor, level) = (self.floor, self.player.level);
            let n = if ch.boss { 2 } else { 2 + self.rng.below(2) };
            for k in 0..n {
                let a = k as f32 * 1.9 + 0.4;
                let p = [ch.pos[0] + a.sin() * 200.0, ch.pos[1] + a.cos() * 200.0];
                self.drop_item(p, if ch.boss { DropSource::Boss } else { DropSource::Chest }, floor, level);
            }
            let g = ((30 + 20 * floor) as f32 * if self.stats.magpie { 1.5 } else { 1.0 }) as u32;
            self.drop_gold(ch.pos, g);
            self.drop_potion(ch.pos);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Enemy AI
    // -----------------------------------------------------------------------------------------

    fn update_enemies(&mut self) {
        if self.freeze || self.hold_foes {
            return;
        }
        let me = self.player.pos;
        let dodge_id = self.dodge_id;
        let dodging = self.player.dodge_t > 0;
        for i in 0..ENEMY_SLOTS {
            if !self.enemies[i].alive {
                continue;
            }
            // Dive-through damage.
            if dodging && self.enemies[i].dive_stamp != dodge_id && dist(self.enemies[i].pos, me) < 150.0 + self.enemies[i].radius {
                self.enemies[i].dive_stamp = dodge_id;
                let (dmg, crit) = roll_damage(&self.stats, 0.6, &mut self.rng);
                self.damage_enemy(i, dmg, me, crit, 14.0);
                if !self.enemies[i].alive {
                    continue;
                }
            }
            self.enemies[i].prev = self.enemies[i].pos;
            let e = &mut self.enemies[i];
            e.flash = e.flash.saturating_sub(1);
            e.invuln = e.invuln.saturating_sub(1);
            e.cd = e.cd.saturating_sub(1);
            // Knockback glide.
            if e.vel[0].abs() + e.vel[1].abs() > 0.4 {
                let (pos, vel, r) = (e.pos, e.vel, e.radius.min(90.0));
                let np = self.dungeon.slide(pos, vel, r);
                let e = &mut self.enemies[i];
                e.pos = np;
                e.vel = [e.vel[0] * 0.76, e.vel[1] * 0.76];
            }
            let e = &mut self.enemies[i];
            if e.stun > 0 {
                e.stun -= 1;
                continue;
            }
            self.ai(i, me);
        }
        // Soft separation so packs do not stack on one point.
        for i in 0..ENEMY_SLOTS {
            if !self.enemies[i].alive || self.enemies[i].kind == EnemyKind::King {
                continue;
            }
            for j in i + 1..ENEMY_SLOTS {
                if !self.enemies[j].alive || self.enemies[j].kind == EnemyKind::King {
                    continue;
                }
                let (a, b) = (self.enemies[i].pos, self.enemies[j].pos);
                let min = (self.enemies[i].radius + self.enemies[j].radius) * 0.8;
                let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
                let d2 = dx * dx + dz * dz;
                if d2 < min * min && d2 > 0.01 {
                    let d = d2.sqrt();
                    let push = (min - d) * 0.25;
                    let (ux, uz) = (dx / d * push, dz / d * push);
                    let ra = self.enemies[i].radius.min(80.0);
                    let rb = self.enemies[j].radius.min(80.0);
                    self.enemies[i].pos = self.dungeon.slide(a, [-ux, -uz], ra);
                    self.enemies[j].pos = self.dungeon.slide(b, [ux, uz], rb);
                }
            }
        }
    }

    fn step_enemy(&mut self, i: usize, dir: [f32; 2], speed: f32) {
        let e = &self.enemies[i];
        let np = self.dungeon.slide(e.pos, [dir[0] * speed, dir[1] * speed], e.radius.min(90.0));
        self.enemies[i].pos = np;
    }

    fn melee_hit(&mut self, i: usize) {
        let (dmg, leech, pos) = (self.enemies[i].dmg, self.enemies[i].mods & MOD_LEECH != 0, self.enemies[i].pos);
        if self.hurt_player(dmg) {
            if leech {
                let e = &mut self.enemies[i];
                e.hp = (e.hp + dmg * 0.5).min(e.max_hp);
            }
            // Thorns.
            if self.stats.thorns > 0.0 {
                let t = self.stats.thorns;
                self.damage_enemy(i, t, pos, false, 0.0);
            }
        }
    }

    fn ai(&mut self, i: usize, me: [f32; 2]) {
        let e = self.enemies[i];
        let to = [me[0] - e.pos[0], me[1] - e.pos[1]];
        let d = (to[0] * to[0] + to[1] * to[1]).sqrt().max(0.001);
        let dir = [to[0] / d, to[1] / d];
        let reach = e.radius + PLAYER_RADIUS + 25.0;
        match e.kind {
            EnemyKind::Scamp => {
                if d > reach * 0.8 {
                    self.step_enemy(i, dir, e.speed);
                }
                if d < reach && e.cd == 0 {
                    self.melee_hit(i);
                    self.enemies[i].cd = 34;
                }
            }
            EnemyKind::Bruiser => match e.state {
                EState::Chase => {
                    self.step_enemy(i, dir, e.speed);
                    if d < 300.0 && e.cd == 0 {
                        let target = [e.pos[0] + dir[0] * 190.0, e.pos[1] + dir[1] * 190.0];
                        self.add_hazard(target, 270.0, 22, e.dmg);
                        let en = &mut self.enemies[i];
                        en.state = EState::Windup;
                        en.timer = 22;
                        en.dir = dir;
                    }
                }
                EState::Windup => {
                    let en = &mut self.enemies[i];
                    en.timer = en.timer.saturating_sub(1);
                    if en.timer == 0 {
                        en.state = EState::Recover;
                        en.timer = 24;
                    }
                }
                _ => {
                    let en = &mut self.enemies[i];
                    en.timer = en.timer.saturating_sub(1);
                    if en.timer == 0 {
                        en.state = EState::Chase;
                        en.cd = 60;
                    }
                }
            },
            EnemyKind::Lancer => match e.state {
                EState::Chase => {
                    if d > 600.0 || e.cd > 0 {
                        self.step_enemy(i, dir, e.speed);
                    }
                    if d < 800.0 && e.cd == 0 {
                        let en = &mut self.enemies[i];
                        en.state = EState::Windup;
                        en.timer = 16;
                        en.dir = dir;
                    }
                }
                EState::Windup => {
                    let en = &mut self.enemies[i];
                    en.timer = en.timer.saturating_sub(1);
                    if en.timer == 0 {
                        en.state = EState::Dash;
                        en.timer = 13;
                        en.struck = false;
                    }
                }
                EState::Dash => {
                    let dd = e.dir;
                    let before = e.pos;
                    self.step_enemy(i, dd, 36.0);
                    let after = self.enemies[i].pos;
                    if dist(before, after) < 5.0 {
                        self.enemies[i].timer = 1;
                    }
                    if !e.struck && dist(after, me) < reach {
                        self.melee_hit(i);
                        self.enemies[i].struck = true;
                    }
                    let en = &mut self.enemies[i];
                    en.timer = en.timer.saturating_sub(1);
                    if en.timer == 0 {
                        en.state = EState::Recover;
                        en.timer = 30;
                    }
                }
                _ => {
                    let en = &mut self.enemies[i];
                    en.timer = en.timer.saturating_sub(1);
                    if en.timer == 0 {
                        en.state = EState::Chase;
                        en.cd = 50;
                    }
                }
            },
            EnemyKind::Wraith | EnemyKind::Seer => {
                let (near, far) = if e.kind == EnemyKind::Wraith { (700.0, 1100.0) } else { (850.0, 1300.0) };
                if d < near {
                    self.step_enemy(i, [-dir[0], -dir[1]], e.speed);
                } else if d > far {
                    self.step_enemy(i, dir, e.speed);
                } else {
                    let side = if (i + (self.tick as usize / 90)) % 2 == 0 { 1.0 } else { -1.0 };
                    self.step_enemy(i, [-dir[1] * side, dir[0] * side], e.speed * 0.5);
                }
                if e.kind == EnemyKind::Wraith {
                    if e.state == EState::Windup {
                        let en = &mut self.enemies[i];
                        en.timer = en.timer.saturating_sub(1);
                        if en.timer == 0 {
                            en.state = EState::Chase;
                            en.cd = 75;
                            self.spawn_shot(false, e.pos, [dir[0] * 15.0, dir[1] * 15.0], e.dmg, 32.0, 80, false, false);
                        }
                    } else if e.cd == 0 && d < 1500.0 {
                        let en = &mut self.enemies[i];
                        en.state = EState::Windup;
                        en.timer = 9;
                    }
                } else if e.cd == 0 {
                    let near_minions = self.enemies.iter().filter(|m| m.alive && m.room == e.room && m.kind == EnemyKind::Scamp).count();
                    self.enemies[i].cd = 135;
                    if near_minions < 5 {
                        for k in 0..2 {
                            let a = k as f32 * 3.1 + self.tick as f32 * 0.1;
                            let p = [e.pos[0] + a.sin() * 160.0, e.pos[1] + a.cos() * 160.0];
                            let p = if self.dungeon.circle_free(p, 60.0) { p } else { e.pos };
                            self.spawn_enemy(EnemyKind::Scamp, p, e.room, 0);
                        }
                        self.ev.summoned = true;
                    }
                }
            }
            EnemyKind::Sapper => match e.state {
                EState::Fuse => {
                    let en = &mut self.enemies[i];
                    en.timer = en.timer.saturating_sub(1);
                    if en.timer == 0 {
                        let (pos, dmg) = (en.pos, en.dmg);
                        en.alive = false;
                        self.alive_enemies = self.alive_enemies.saturating_sub(1);
                        self.add_hazard(pos, 340.0, 0, dmg);
                        // Room clear check without credit.
                        let room = e.room as usize;
                        if room < self.rooms.len() && self.rooms[room].active && !self.rooms[room].cleared && !self.enemies.iter().any(|x| x.alive && x.room as usize == room) {
                            self.rooms[room].cleared = true;
                            self.ev.room_cleared = true;
                        }
                    }
                }
                _ => {
                    self.step_enemy(i, dir, e.speed);
                    if d < 210.0 {
                        let en = &mut self.enemies[i];
                        en.state = EState::Fuse;
                        en.timer = 22;
                    }
                }
            },
            EnemyKind::King => self.ai_king(i, me, dir, d),
        }
    }

    fn ai_king(&mut self, i: usize, me: [f32; 2], dir: [f32; 2], d: f32) {
        let e = self.enemies[i];
        let enraged = e.phase >= 2;
        let speed = e.speed * if enraged { 1.35 } else { 1.0 };
        match e.state {
            EState::Chase => {
                if d > 260.0 {
                    self.step_enemy(i, dir, speed);
                }
                if e.cd == 0 {
                    // Pick an attack: stomp always; bombs from phase 1; charge in phase 2.
                    let roll = self.rng.below(100);
                    let en = &mut self.enemies[i];
                    en.dir = dir;
                    if e.phase >= 2 && roll < 30 {
                        en.state = EState::Windup;
                        en.timer = 20;
                        en.phase = e.phase; // keep
                        en.struck = true; // charge flag
                    } else if e.phase >= 1 && roll < 55 {
                        // Bomb volley: three delayed blasts around the player.
                        for k in 0..3 {
                            let a = k as f32 * 2.1 + self.tick as f32 * 0.07;
                            let r = if k == 0 { 0.0 } else { 300.0 };
                            let p = [me[0] + a.sin() * r, me[1] + a.cos() * r];
                            let p = if self.dungeon.walkable(p) { p } else { me };
                            self.add_hazard(p, 260.0, 34 + k as u16 * 6, e.dmg * 0.8);
                        }
                        let en = &mut self.enemies[i];
                        en.state = EState::Recover;
                        en.timer = 38;
                        en.struck = false;
                    } else {
                        self.add_hazard(e.pos, 460.0, 26, e.dmg);
                        let en = &mut self.enemies[i];
                        en.state = EState::Windup;
                        en.timer = 26;
                        en.struck = false;
                    }
                }
            }
            EState::Windup => {
                let en = &mut self.enemies[i];
                en.timer = en.timer.saturating_sub(1);
                if en.timer == 0 {
                    if en.struck {
                        en.state = EState::Dash;
                        en.timer = 16;
                        en.struck = false;
                    } else {
                        en.state = EState::Recover;
                        en.timer = 20;
                    }
                }
            }
            EState::Dash => {
                let dd = e.dir;
                self.step_enemy(i, dd, 30.0);
                if !e.struck && dist(self.enemies[i].pos, me) < e.radius + PLAYER_RADIUS {
                    self.melee_hit(i);
                    self.enemies[i].struck = true;
                }
                let en = &mut self.enemies[i];
                en.timer = en.timer.saturating_sub(1);
                if en.timer == 0 {
                    en.state = EState::Recover;
                    en.timer = 30;
                }
            }
            _ => {
                let en = &mut self.enemies[i];
                en.timer = en.timer.saturating_sub(1);
                if en.timer == 0 {
                    en.state = EState::Chase;
                    en.cd = if enraged { 38 } else { 62 };
                }
            }
        }
        // Touching the king hurts.
        let e2 = self.enemies[i];
        if e2.state != EState::Dash && d < e2.radius + PLAYER_RADIUS && e2.stun == 0 {
            // Contact uses the king's own cooldown-free 1/3 damage tick.
            if self.tick % 20 == 0 {
                let dmg = e2.dmg * 0.3;
                self.hurt_player(dmg);
            }
        }
    }
}

pub fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

pub fn norm(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l < 1e-4 { [0.0, 1.0] } else { [v[0] / l, v[1] / l] }
}

/// Smallest signed difference between two angles (radians).
pub fn angle_diff(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

pub fn clock(secs: f32) -> String {
    let s = secs as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> PlayerFrame {
        PlayerFrame { grounded: true, ..Default::default() }
    }

    fn frame_at(r: &Relic) -> PlayerFrame {
        PlayerFrame { pos: r.player.pos, grounded: true, face: r.player.face, ..Default::default() }
    }

    /// A relic with one enemy next to the player, nothing else awake.
    fn duel(kind: EnemyKind) -> (Relic, usize) {
        let mut r = Relic::new(7, Hero::Mario, 1, 0);
        let c = r.start_pos();
        r.player.pos = c;
        let i = r.spawn_enemy(kind, [c[0], c[1] + 150.0], 0, 0).unwrap();
        (r, i)
    }

    #[test]
    fn xp_curve_is_increasing_and_levels_grant_points() {
        for l in 1..30 {
            assert!(xp_to_next(l + 1) > xp_to_next(l));
        }
        let mut r = Relic::new(1, Hero::Mario, 1, 0);
        let p0 = r.tree.points;
        r.add_xp(xp_to_next(1));
        assert_eq!(r.player.level, 2);
        assert_eq!(r.tree.points, p0 + 1);
        assert!(r.ev.level_up);
        r.add_xp(100_000);
        assert_eq!(r.player.level, MAX_LEVEL);
        assert!(r.stats.max_hp > 100.0);
    }

    #[test]
    fn damage_formula_crits_spread_and_armour() {
        let stats = Stats { base_damage: 10.0, dmg_mult: 1.0, crit: 0.0, crit_mult: 2.0, ..Default::default() };
        let mut rng = Rng::new(3);
        for _ in 0..200 {
            let (d, c) = roll_damage(&stats, 1.0, &mut rng);
            assert!(!c && (9.0..=11.0).contains(&d));
        }
        let always = Stats { crit: 1.0, ..stats };
        let (d, c) = roll_damage(&always, 1.0, &mut rng);
        assert!(c && (18.0..=22.0).contains(&d));
        let (d2, _) = roll_damage(&stats, 3.0, &mut rng);
        assert!(d2 > 26.0);
        assert!((mitigate(100.0, 100.0) - 50.0).abs() < 1e-3);
        assert_eq!(mitigate(40.0, 0.0), 40.0);
        // Crit rate is about what the stat says.
        let some = Stats { crit: 0.3, ..stats };
        let n = (0..10_000).filter(|_| roll_damage(&some, 1.0, &mut rng).1).count();
        assert!((2700..3300).contains(&n), "{n}");
    }

    #[test]
    fn dodge_iframes_block_damage_then_expire() {
        let (mut r, _) = duel(EnemyKind::Scamp);
        r.enemies = [Enemy::blank(); ENEMY_SLOTS];
        r.alive_enemies = 0;
        let hp = r.player.hp;
        assert!(r.try_dodge());
        assert!(r.player.iframes >= r.stats.dodge_ticks);
        assert!(!r.hurt_player(30.0));
        assert_eq!(r.player.hp, hp);
        // Cannot dodge again until the cooldown ends.
        assert!(!r.try_dodge());
        for _ in 0..r.stats.dodge_ticks {
            { let f = frame_at(&r); r.tick(&f, &Intent::default()); }
        }
        assert_eq!(r.player.iframes, 0);
        assert!(r.hurt_player(30.0));
        assert!(r.player.hp < hp);
        // Hits grant a short grace period but no more.
        assert!(!r.hurt_player(30.0));
        for _ in 0..HIT_IFRAMES {
            { let f = frame_at(&r); r.tick(&f, &Intent::default()); }
        }
        assert!(r.hurt_player(5.0));
    }

    #[test]
    fn swing_hits_the_cone_in_front_only() {
        let mut r = Relic::new(2, Hero::Link, 1, 0);
        let c = r.start_pos();
        r.player.pos = c;
        r.player.face = 0.0; // +z
        let front = r.spawn_enemy(EnemyKind::Bruiser, [c[0], c[1] + 200.0], 0, 0).unwrap();
        let behind = r.spawn_enemy(EnemyKind::Bruiser, [c[0], c[1] - 260.0], 0, 0).unwrap();
        let far = r.spawn_enemy(EnemyKind::Bruiser, [c[0], c[1] + 1000.0], 0, 0).unwrap();
        r.freeze = true;
        let hits = r.swing();
        assert_eq!(hits, 1);
        assert!(r.enemies[front].hp < r.enemies[front].max_hp);
        assert_eq!(r.enemies[behind].hp, r.enemies[behind].max_hp);
        assert_eq!(r.enemies[far].hp, r.enemies[far].max_hp);
        // Each hero's swing has its own shape.
        assert!(Hero::Link.attack().reach > Hero::Mario.attack().reach);
        assert!(Hero::YoungLink.attack().cd < Hero::Banjo.attack().cd);
    }

    #[test]
    fn skill_tree_rules() {
        let mut t = Tree { points: 3, ..Default::default() };
        assert!(!t.can_rank(Perk::Cyclone), "needs the Whirl unlock first");
        assert!(!t.unlocked(Skill::Whirl));
        assert!(t.rank_up(Perk::Whirl));
        assert!(t.unlocked(Skill::Whirl));
        assert!(!t.rank_up(Perk::Whirl), "unlock nodes have one rank");
        assert!(t.rank_up(Perk::Cyclone));
        assert!(t.rank_up(Perk::Cyclone));
        assert_eq!(t.points, 0);
        assert!(!t.rank_up(Perk::Cyclone), "out of points");
        t.points = 10;
        for _ in 0..5 {
            t.rank_up(Perk::Cyclone);
        }
        assert_eq!(t.rank(Perk::Cyclone), 3, "capped at max rank");
        assert_eq!(NODES.len(), 12);
        assert_eq!(t.spent(), 4);
        for s in SKILLS {
            assert!(NODES.iter().any(|n| n.info().skill == Some(s)), "{} has perks", s.name());
        }
        for n in NODES {
            let info = n.info();
            assert!(info.max >= 1 && info.name.is_ascii() && info.desc.is_ascii());
            if let Some(req) = info.req {
                assert_ne!(req, n);
            }
        }
        assert!(Tree::default().unlocked(Skill::Slam));
    }

    #[test]
    fn skills_cost_spark_and_cool_down() {
        let mut r = Relic::new(4, Hero::Mario, 1, 0);
        r.freeze = true;
        assert!(!r.use_skill(Skill::Whirl, true), "locked until the tree unlocks it");
        r.rank_up(Perk::Whirl);
        let spark = r.player.spark;
        assert!(r.use_skill(Skill::Whirl, true));
        assert_eq!(r.player.spark, spark - Skill::Whirl.cost());
        assert!(r.player.skill_cd[Skill::Whirl as usize] > 0);
        assert!(!r.use_skill(Skill::Whirl, true), "on cooldown");
        // Cooldown reduction shortens it.
        let base = r.player.skill_cd[1];
        let mut r2 = Relic::new(4, Hero::Mario, 1, 0);
        r2.rank_up(Perk::Whirl);
        r2.rank_up(Perk::Gale);
        r2.use_skill(Skill::Whirl, true);
        assert!(r2.player.skill_cd[1] < base);
        // Volley fires bolts, barrage adds more.
        let mut r3 = Relic::new(4, Hero::Mario, 1, 0);
        r3.tree.points = 4;
        r3.rank_up(Perk::Volley);
        r3.use_skill(Skill::Volley, true);
        let n1 = r3.shots.iter().filter(|s| s.alive).count();
        let mut r4 = Relic::new(4, Hero::Mario, 1, 0);
        r4.tree.points = 4;
        r4.rank_up(Perk::Volley);
        r4.rank_up(Perk::Barrage);
        r4.use_skill(Skill::Volley, true);
        assert!(r4.shots.iter().filter(|s| s.alive).count() == n1 + 2);
        // The slam asks the physics for a leap, or a pound when airborne.
        let mut r5 = Relic::new(4, Hero::Mario, 1, 0);
        r5.use_skill(Skill::Slam, true);
        assert!(r5.ev.want_leap && !r5.ev.want_pound);
        let mut r6 = Relic::new(4, Hero::Mario, 1, 0);
        r6.use_skill(Skill::Slam, false);
        assert!(r6.ev.want_pound);
    }

    #[test]
    fn slam_landing_hits_in_a_ring_and_goes_on_cooldown() {
        let (mut r, i) = duel(EnemyKind::Bruiser);
        r.freeze = true;
        let hp = r.enemies[i].hp;
        let mut f = frame_at(&r);
        f.pound_land = true;
        r.tick(&f, &Intent::default());
        assert!(r.ev.slam && r.enemies[i].hp < hp);
        assert!(r.player.skill_cd[0] > 0);
        // A second landing right away does nothing.
        let hp2 = r.enemies[i].hp;
        r.tick(&f, &Intent::default());
        assert!(!r.ev.slam && r.enemies[i].hp == hp2);
    }

    #[test]
    fn aegis_soaks_damage_and_expires() {
        let mut r = Relic::new(5, Hero::Mario, 1, 0);
        r.rank_up(Perk::Aegis);
        assert!(r.use_skill(Skill::Aegis, true));
        let hp = r.player.hp;
        assert!(r.player.shield > 20.0);
        assert!(r.hurt_player(10.0));
        assert_eq!(r.player.hp, hp, "shield soaked it");
        assert!(r.player.shield < r.player.shield_max);
        r.player.shield_t = 1;
        { let f = frame_at(&r); r.tick(&f, &Intent::default()); }
        assert_eq!(r.player.shield, 0.0);
    }

    #[test]
    fn scaling_by_floor_and_pack_plans() {
        assert!(enemy_hp_mult(3) > enemy_hp_mult(2) && enemy_hp_mult(2) > enemy_hp_mult(1));
        assert!(enemy_dmg_mult(3) > enemy_dmg_mult(1));
        let size = |floor: u32| {
            let mut tot = 0;
            for s in 0..60u64 {
                let mut pack = [PackSlot { kind: EnemyKind::Scamp, mods: 0 }; PACK_MAX];
                tot += plan_pack(&mut Rng::new(s), floor, RoomKind::Combat, 80, &mut pack);
            }
            tot
        };
        assert!(size(3) > size(1));
        let mut elites = 0;
        for s in 0..200u64 {
            let mut pack = [PackSlot { kind: EnemyKind::Scamp, mods: 0 }; PACK_MAX];
            let n = plan_pack(&mut Rng::new(s), 2, RoomKind::Combat, 80, &mut pack);
            assert!(n <= PACK_MAX);
            assert!(pack[..n].iter().all(|p| p.kind != EnemyKind::King));
            assert!(pack[..n].iter().filter(|p| p.mods != 0).count() <= 1);
            elites += pack[..n].iter().filter(|p| p.mods != 0).count();
            let mut t = [PackSlot { kind: EnemyKind::Scamp, mods: 0 }; PACK_MAX];
            let m = plan_pack(&mut Rng::new(s), 1, RoomKind::Treasure, 80, &mut t);
            assert!(t[..m].iter().any(|p| p.mods != 0), "treasure rooms always have an elite");
        }
        assert!(elites > 20 && elites < 200);
    }

    #[test]
    fn boss_phases_are_monotone_and_fire_once() {
        assert_eq!(boss_phase(1.0, 0), 0);
        assert_eq!(boss_phase(0.66, 0), 1);
        assert_eq!(boss_phase(0.5, 0), 1);
        assert_eq!(boss_phase(0.33, 0), 2);
        assert_eq!(boss_phase(0.9, 2), 2, "never goes back");
        let mut r = Relic::new(9, Hero::Mario, 3, 0);
        r.freeze = true;
        let i = r.spawn_enemy(EnemyKind::King, r.start_pos(), 0, 0).unwrap();
        r.boss = Some(i);
        let max = r.enemies[i].max_hp;
        let mut seen = vec![];
        for _ in 0..40 {
            r.enemies[i].invuln = 0;
            r.damage_enemy(i, max * 0.05, [0.0; 2], false, 0.0);
            if let Some(p) = r.ev.boss_phase.take() {
                seen.push(p);
            }
            if !r.enemies[i].alive {
                break;
            }
        }
        assert_eq!(seen, vec![1, 2]);
        assert!(!r.enemies[i].alive && r.ev.boss_killed);
        // Phase summons appeared (and their count is bounded by the cap).
        assert!(r.alive_enemies >= 5 && r.alive_enemies <= r.cap());
    }

    #[test]
    fn boss_dies_into_victory_with_guaranteed_loot() {
        let mut r = Relic::new(10, Hero::Mario, 3, 0);
        r.freeze = true;
        let i = r.spawn_enemy(EnemyKind::King, r.start_pos(), 0, 0).unwrap();
        r.boss = Some(i);
        r.enemies[i].hp = 1.0;
        r.damage_enemy(i, 50.0, [0.0; 2], false, 0.0);
        assert!(r.ev.boss_killed);
        let items: Vec<_> = r.drops.iter().filter(|d| d.alive && matches!(d.kind, DropKind::Item)).collect();
        assert!(items.len() >= 3, "boss drops items");
        assert!(items.iter().all(|d| d.item.rarity >= Rarity::Rare));
        assert!(r.chests.iter().flatten().any(|c| c.boss));
        for _ in 0..130 {
            let f = frame_at(&r);
            r.tick(&f, &Intent::default());
        }
        assert!(r.won && r.over);
    }

    #[test]
    fn rooms_wake_on_entry_and_the_boss_room_spawns_the_king() {
        let mut r = Relic::new(11, Hero::Mario, 1, 0);
        assert_eq!(r.alive_enemies, 0);
        let target = (0..r.dungeon.rooms.len()).find(|&i| r.dungeon.rooms[i].kind == RoomKind::Combat).unwrap();
        r.player.pos = r.dungeon.rooms[target].center();
        let mut f = idle();
        f.pos = r.player.pos;
        r.tick(&f, &Intent::default());
        assert!(r.alive_enemies >= 5, "{}", r.alive_enemies);
        assert!(r.rooms[target].active);
        assert!(r.enemies.iter().filter(|e| e.alive).all(|e| e.room as usize == target));
        let mut b = Relic::new(11, Hero::Mario, 3, 0);
        f.pos = b.dungeon.rooms[b.dungeon.exit].center();
        b.tick(&f, &Intent::default());
        assert!(b.ev.boss_spawned && b.boss_enemy().is_some_and(|e| e.kind == EnemyKind::King));
        assert!(b.stairs_pos().is_none());
    }

    #[test]
    fn stairs_descend_to_a_new_floor() {
        let mut r = Relic::new(12, Hero::Mario, 1, 0);
        let s = r.stairs_pos().unwrap();
        let mut f = idle();
        f.pos = s;
        let tiles = r.dungeon.tiles.clone();
        for _ in 0..30 {
            r.tick(&f, &Intent::default());
            if r.ev.descended {
                break;
            }
        }
        assert_eq!(r.floor, 2);
        assert_ne!(r.dungeon.tiles, tiles);
        assert!(r.ev.descended);
        assert_eq!(r.player.pos, r.start_pos());
        assert!(r.enemies.iter().all(|e| !e.alive));
    }

    #[test]
    fn loot_filter_turns_commons_into_gold() {
        let mut r = Relic::new(13, Hero::Mario, 1, 0);
        let mut rng = Rng::new(1);
        let common = roll_item(&mut rng, 900, Slot::Weapon, Rarity::Common, 5);
        r.filter_commons = true;
        r.place_item([100.0, 100.0], common);
        assert!(r.drops.iter().all(|d| !(d.alive && matches!(d.kind, DropKind::Item))));
        assert!(r.drops.iter().any(|d| d.alive && matches!(d.kind, DropKind::Gold) && d.amount == common.sell_value()));
        r.filter_commons = false;
        r.place_item([100.0, 100.0], common);
        assert!(r.drops.iter().any(|d| d.alive && matches!(d.kind, DropKind::Item)));
    }

    #[test]
    fn pickup_equips_into_empty_slots_then_fills_the_bag() {
        let mut r = Relic::new(14, Hero::Mario, 1, 0);
        r.freeze = true;
        r.filter_commons = false;
        let mut rng = Rng::new(2);
        let w1 = roll_item(&mut rng, 1, Slot::Weapon, Rarity::Rare, 6);
        let w2 = roll_item(&mut rng, 2, Slot::Weapon, Rarity::Rare, 6);
        let dmg0 = r.stats.base_damage;
        let c = r.player.pos;
        r.place_item(c, w1);
        r.place_item(c, w2);
        for _ in 0..12 {
            let f = frame_at(&r);
            r.tick(&f, &Intent::default());
        }
        assert!(r.equipment.slots[0].is_some(), "first weapon auto-equips");
        assert_eq!(r.bag.len(), 1, "second one goes to the bag");
        assert!(r.stats.base_damage > dmg0);
        // Equip the bag item: the old one swaps back.
        let id = r.bag[0].id;
        assert!(r.equip_from_bag(id));
        assert_eq!(r.bag.len(), 1);
        // Sell it.
        let gold = r.player.gold;
        let id = r.bag[0].id;
        assert!(r.sell_from_bag(id));
        assert!(r.player.gold > gold && r.bag.is_empty());
        // Bag capacity blocks pickups.
        for k in 0..BAG_CAP {
            r.bag.push(roll_item(&mut rng, 100 + k as u32, Slot::Charm, Rarity::Rare, 4));
        }
        r.equipment.slots[3] = Some(roll_item(&mut rng, 5, Slot::Charm, Rarity::Rare, 4));
        r.equipment.slots[4] = Some(roll_item(&mut rng, 6, Slot::Charm, Rarity::Rare, 4));
        let c = r.player.pos;
        r.place_item(c, roll_item(&mut rng, 7, Slot::Charm, Rarity::Rare, 4));
        for _ in 0..12 {
            let f = frame_at(&r);
            r.tick(&f, &Intent::default());
        }
        assert_eq!(r.bag.len(), BAG_CAP);
        assert!(r.drops.iter().any(|d| d.alive && matches!(d.kind, DropKind::Item)), "item stays on the ground");
    }

    #[test]
    fn potions_heal_and_respect_limits() {
        let mut r = Relic::new(15, Hero::Mario, 1, 0);
        assert!(!r.use_potion(), "full health");
        r.player.hp = 10.0;
        assert!(r.use_potion());
        assert!(r.player.hp > 40.0);
        assert_eq!(r.player.potions, START_POTIONS - 1);
        assert!(!r.use_potion(), "cooldown");
    }

    #[test]
    fn lastlight_saves_once_per_floor() {
        let mut r = Relic::new(16, Hero::Mario, 1, 0);
        r.stats.lastlight = true;
        r.player.hp = 5.0;
        r.hurt_player(50.0);
        assert!(!r.over && r.ev.lastlight);
        assert!((r.player.hp - r.stats.max_hp * 0.3).abs() < 1.0);
        r.player.iframes = 0;
        r.player.hp = 5.0;
        r.hurt_player(50.0);
        assert!(r.over && r.ev.died);
    }

    #[test]
    fn dying_stops_the_world() {
        let mut r = Relic::new(17, Hero::Mario, 1, 0);
        r.player.hp = 1.0;
        r.hurt_player(100.0);
        assert!(r.over);
        let t = r.tick;
        r.tick(&idle(), &Intent::default());
        assert_eq!(r.tick, t);
    }

    #[test]
    fn enemies_chase_and_each_kind_attacks() {
        for kind in [EnemyKind::Scamp, EnemyKind::Bruiser, EnemyKind::Lancer, EnemyKind::Wraith, EnemyKind::Sapper, EnemyKind::Seer] {
            let (mut r, _) = duel(kind);
            r.player.hp = r.stats.max_hp;
            let hp0 = r.player.hp;
            let mut hurt = false;
            let mut shots = false;
            let mut spawned = false;
            for _ in 0..240 {
                let f = frame_at(&r);
                r.tick(&f, &Intent::default());
                hurt |= r.player.hp < hp0;
                shots |= r.shots.iter().any(|s| s.alive && !s.from_player);
                spawned |= r.alive_enemies > 1;
                r.player.iframes = 0;
            }
            match kind {
                EnemyKind::Seer => assert!(spawned, "seer summons"),
                EnemyKind::Wraith => assert!(shots || hurt, "wraith shoots"),
                _ => assert!(hurt, "{kind:?} should have hurt a standing target"),
            }
        }
    }

    #[test]
    fn sapper_explosion_has_a_telegraph_and_kills_the_sapper() {
        let (mut r, i) = duel(EnemyKind::Sapper);
        let mut fused = false;
        for _ in 0..80 {
            let f = frame_at(&r);
            r.tick(&f, &Intent::default());
            fused |= r.enemies[i].alive && r.enemies[i].state == EState::Fuse;
        }
        assert!(fused && !r.enemies[i].alive);
    }

    #[test]
    fn echo_and_thorns_and_lifesteal_stats_apply() {
        let mut rng = Rng::new(31);
        let mut item = roll_item(&mut rng, 1, Slot::Armor, Rarity::Legendary, 8);
        item.legendary = Some(Legendary::Briar);
        item.n = 1;
        item.affixes[0] = Affix { stat: Stat::Thorns, value: 4.0 };
        let mut t = Totals::default();
        let mut eq = Equipment::default();
        eq.equip(item);
        t = eq.totals();
        let s = derive_stats(1, &t, &Tree::default(), 0.0);
        assert_eq!(s.thorns, 8.0, "Briar doubles thorns");
        let _ = &mut t;
        let s0 = derive_stats(1, &Totals::default(), &Tree::default(), 0.0);
        let s1 = derive_stats(1, &Totals::default(), &Tree::default(), legacy_bonus(2500));
        assert!(s1.max_hp > s0.max_hp && s1.dmg_mult > s0.dmg_mult);
        assert_eq!(legacy_bonus(100_000), 0.2f32.min(legacy_bonus(100_000)));
    }

    #[test]
    fn determinism_same_seed_same_inputs_same_world() {
        let run = || {
            let mut r = Relic::new(77, Hero::Link, 2, 0);
            let target = (0..r.dungeon.rooms.len()).find(|&i| r.dungeon.rooms[i].kind == RoomKind::Combat).unwrap();
            let c = r.dungeon.rooms[target].center();
            let mut trace = vec![];
            for t in 0..400u32 {
                let mut f = PlayerFrame { pos: c, grounded: true, ..Default::default() };
                f.face = (t as f32 * 0.07).sin();
                let intent = Intent { attack: t % 5 == 0, skill: [t % 90 == 40, false, false, false], ..Default::default() };
                r.tick(&f, &intent);
                r.player.hp = r.stats.max_hp;
                trace.push((r.kills, r.alive_enemies, (r.player.xp, r.player.gold)));
            }
            (trace, r.enemies.iter().filter(|e| e.alive).map(|e| e.pos.map(|v| v.to_bits())).collect::<Vec<_>>())
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn the_cap_bounds_live_enemies_and_a_full_wave_ticks_fast() {
        let mut r = Relic::with_cap(3, Hero::Mario, 3, 0, 64);
        let c = r.start_pos();
        for k in 0..200 {
            r.spawn_enemy(EnemyKind::Scamp, [c[0] + (k % 5) as f32 * 40.0, c[1] + (k / 5) as f32 * 10.0 + 400.0], 0, 0);
        }
        assert_eq!(r.alive_enemies, 64);
        let start = std::time::Instant::now();
        for _ in 0..300 {
            let mut f = frame_at(&r);
            f.pos = c;
            r.tick(&f, &Intent { attack: true, ..Default::default() });
            r.player.hp = r.stats.max_hp;
        }
        let per_tick = start.elapsed().as_secs_f64() / 300.0;
        assert!(per_tick < 0.004, "tick took {:.3} ms with a full crowd", per_tick * 1000.0);
    }

    #[test]
    fn a_scripted_bot_can_clear_a_room_and_loot_it() {
        let mut r = Relic::new(21, Hero::Mario, 1, 0);
        r.filter_commons = false;
        let target = (0..r.dungeon.rooms.len()).find(|&i| r.dungeon.rooms[i].kind == RoomKind::Combat).unwrap();
        let c = r.dungeon.rooms[target].center();
        r.player.pos = c;
        for _ in 0..3000 {
            if r.rooms[target].cleared {
                break;
            }
            // Face and chase the nearest enemy, swing constantly, quaff when low.
            let mut pos = r.player.pos;
            let mut face = r.player.face;
            if let Some(i) = r.nearest_enemy(pos, 5000.0) {
                let e = r.enemies[i].pos;
                face = (e[0] - pos[0]).atan2(e[1] - pos[1]);
                let d = norm([e[0] - pos[0], e[1] - pos[1]]);
                pos = r.dungeon.slide(pos, [d[0] * 20.0, d[1] * 20.0], 45.0);
            }
            let f = PlayerFrame { pos, face, grounded: true, ..Default::default() };
            r.tick(&f, &Intent { attack: true, potion: r.player.hp < 40.0, ..Default::default() });
            if r.over {
                break;
            }
        }
        assert!(r.rooms[target].cleared && !r.over, "bot should beat a floor-1 room (hp {})", r.player.hp);
        assert!(r.kills >= 5 && r.player.xp > 0 || r.player.level > 1);
    }
}
