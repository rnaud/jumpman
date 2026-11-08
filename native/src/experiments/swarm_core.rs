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

pub const TICKS_PER_SEC: u32 = 30;
pub const ENEMY_SLOTS: usize = 224;
pub const BOLT_SLOTS: usize = 96;
pub const ORB_SLOTS: usize = 64;
pub const GEM_SLOTS: usize = 320;
pub const RING_SLOTS: usize = 8;
pub const ORBIT_MAX: usize = 6;
/// Half the side of the square arena.
pub const ARENA_HALF: f32 = 3700.0;
pub const PLAYER_RADIUS: f32 = 45.0;
pub const IFRAMES: u16 = 24;
pub const BASE_HP: f32 = 100.0;
pub const BOSS_PERIOD_SECS: f32 = 120.0;
/// Surviving this long wins the run (the third King arrives at 6:00, the dawn comes at 8:00).
pub const WIN_SECS: f32 = 480.0;
const SURGE_PERIOD_TICKS: u32 = 40 * TICKS_PER_SEC;
const BOLT_SPEED: f32 = 42.0;
const BOLT_RANGE: f32 = 1900.0;
const DASH_SPEED: f32 = 34.0;
const ORBIT_RADIUS: f32 = 340.0;

/// Live-enemy cap: phones get fewer than desktops.
pub const fn enemy_cap() -> usize {
    if cfg!(any(target_os = "ios", target_os = "android")) { 160 } else { 220 }
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
    1.0 + 0.15 * m + 0.03 * m * m
}

pub fn speed_mult(secs: f32) -> f32 {
    1.0 + (0.04 * minutes(secs)).min(0.4)
}

pub fn damage_mult(secs: f32) -> f32 {
    1.0 + 0.08 * minutes(secs)
}

/// How many enemies the wave director tries to keep alive.
pub fn target_live(secs: f32, cap: usize) -> usize {
    ((8.0 + 0.30 * secs + 0.0006 * secs * secs) as usize).min(cap)
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
}

impl EnemyKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Skitter,
            2 => Self::Brute,
            3 => Self::Dasher,
            4 => Self::Wisp,
            5 => Self::King,
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
    }
}

/// Spawn weights for [skitter, brute, dasher, wisp] at `secs`; new types phase in over time.
pub fn kind_weights(secs: f32) -> [f32; 4] {
    [10.0, if secs < 45.0 { 0.0 } else { ((secs - 45.0) / 20.0).min(6.0) }, if secs < 75.0 { 0.0 } else { ((secs - 75.0) / 20.0).min(5.0) }, if secs < 120.0 { 0.0 } else { ((secs - 120.0) / 25.0).min(4.0) }]
}

// ---------------------------------------------------------------------------------------------
// Upgrades
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Upgrade {
    Damage,
    FireRate,
    Bolts,
    Area,
    Pierce,
    Speed,
    Jump,
    Quake,
    Vitality,
    Regen,
    Magnet,
    Crit,
    Nova,
    Orbit,
    SecondChance,
}

pub const UPGRADE_COUNT: usize = 15;

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
            Upgrade::Damage => ("Star Power", "Stars hit 20% harder.", 8, 10, 1, false),
            Upgrade::FireRate => ("Rapid Fire", "Fire 12% faster.", 8, 10, 1, false),
            Upgrade::Bolts => ("Twin Stars", "Fire one more star per volley.", 5, 6, 2, false),
            Upgrade::Area => ("Big Bang", "Stars, pulses and orbits grow 25%.", 5, 8, 1, false),
            Upgrade::Pierce => ("Piercing Light", "Stars pass through one more foe.", 4, 7, 2, false),
            Upgrade::Speed => ("Swift Boots", "Run and fly 7% faster.", 5, 8, 1, false),
            Upgrade::Jump => ("Spring Legs", "Jump 10% higher.", 4, 6, 1, false),
            Upgrade::Quake => ("Ground Quake", "Ground-pound shockwave hits harder and wider.", 4, 5, 2, false),
            Upgrade::Vitality => ("Hearty", "+30 max health and a small heal.", 6, 10, 1, false),
            Upgrade::Regen => ("Mending", "Recover 0.8 health per second.", 5, 7, 1, false),
            Upgrade::Magnet => ("Gem Magnet", "Gems fly to you from 30% further.", 5, 8, 1, false),
            Upgrade::Crit => ("Lucky Star", "+8% chance for double damage.", 5, 8, 2, false),
            Upgrade::Nova => ("Nova Pulse", "A ring of light bursts around you every few seconds.", 4, 3, 4, true),
            Upgrade::Orbit => ("Orbit Stars", "Stars circle you and slice what they touch.", 4, 3, 5, true),
            Upgrade::SecondChance => ("Second Chance", "Once per run, get back up at half health.", 1, 2, 6, true),
        };
        UpgradeInfo { name, desc, max, weight, min_level, rare }
    }
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Upgrades {
    pub levels: [u8; UPGRADE_COUNT],
}

impl Upgrades {
    pub fn level(&self, u: Upgrade) -> u8 {
        self.levels[u.index()]
    }
    pub fn maxed(&self, u: Upgrade) -> bool {
        self.level(u) >= u.info().max
    }
    /// Take one level; false at the cap.
    pub fn take(&mut self, u: Upgrade) -> bool {
        if self.maxed(u) {
            return false;
        }
        self.levels[u.index()] += 1;
        true
    }
    pub fn eligible(&self, u: Upgrade, player_level: u32) -> bool {
        !self.maxed(u) && player_level >= u.info().min_level
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choices {
    pub items: [Upgrade; 3],
    pub len: usize,
}

impl Choices {
    pub fn as_slice(&self) -> &[Upgrade] {
        &self.items[..self.len]
    }
}

/// Weighted random draw of up to three distinct, non-maxed upgrades the player qualifies for.
pub fn roll_choices(rng: &mut Rng, owned: &Upgrades, player_level: u32) -> Choices {
    let mut pool = [Upgrade::Damage; UPGRADE_COUNT];
    let mut n = 0;
    for u in ALL_UPGRADES {
        if owned.eligible(u, player_level) {
            pool[n] = u;
            n += 1;
        }
    }
    let mut out = Choices { items: [Upgrade::Damage; 3], len: 0 };
    while out.len < 3 && n > 0 {
        let total: u32 = pool[..n].iter().map(|u| u.info().weight).sum();
        let mut roll = rng.below(total);
        let mut pick = n - 1;
        for (i, u) in pool[..n].iter().enumerate() {
            let w = u.info().weight;
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
    out
}

/// Everything the upgrades add up to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub damage: f32,
    pub interval: u32,
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
}

pub fn derive_stats(u: &Upgrades) -> Stats {
    let l = |x: Upgrade| u.level(x) as f32;
    Stats {
        damage: 10.0 * (1.0 + 0.2 * l(Upgrade::Damage)),
        interval: ((26.0 * 0.88f32.powf(l(Upgrade::FireRate))).round() as u32).max(6),
        bolts: 1 + u.level(Upgrade::Bolts) as u32,
        area: 1.0 + 0.25 * l(Upgrade::Area),
        pierce: u.level(Upgrade::Pierce),
        move_bonus: 0.07 * l(Upgrade::Speed),
        jump_bonus: 0.10 * l(Upgrade::Jump),
        quake: u.level(Upgrade::Quake),
        max_hp: BASE_HP + 30.0 * l(Upgrade::Vitality),
        regen_per_tick: 0.8 * l(Upgrade::Regen) / TICKS_PER_SEC as f32,
        magnet: 220.0 * (1.0 + 0.3 * l(Upgrade::Magnet)),
        crit: 0.05 + 0.08 * l(Upgrade::Crit),
        nova: u.level(Upgrade::Nova),
        orbit: if u.level(Upgrade::Orbit) == 0 { 0 } else { 1 + u.level(Upgrade::Orbit) as u32 },
        revive: u.level(Upgrade::SecondChance) > 0,
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
    state: u8,
    timer: i16,
    dash: [f32; 2],
    orbit_cd: u8,
}

impl Enemy {
    const EMPTY: Enemy = Enemy { alive: false, kind: EnemyKind::Skitter, pos: [0.0; 2], prev: [0.0; 2], vel: [0.0; 2], hp: 0.0, max_hp: 0.0, contact: 0.0, speed: 0.0, radius: 0.0, flash: 0, state: 0, timer: 0, dash: [0.0; 2], orbit_cd: 0 };
    /// Winding up a lunge (the renderer shakes it).
    pub fn winding_up(&self) -> bool {
        self.kind == EnemyKind::Dasher && self.state == 1
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
    recent: [u16; 3],
}

impl Bolt {
    const EMPTY: Bolt = Bolt { alive: false, pos: [0.0; 2], prev: [0.0; 2], vel: [0.0; 2], life: 0, dmg: 0.0, pierce: 0, radius: 0.0, recent: [u16::MAX; 3] };
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
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RingKind {
    Nova,
    Quake,
    Revive,
}

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub alive: bool,
    pub kind: RingKind,
    pub center: [f32; 2],
    pub max: f32,
    pub age: u16,
    pub life: u16,
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
    /// Ground pound hit the floor this tick.
    pub pound_land: bool,
}

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
}

// ---------------------------------------------------------------------------------------------
// The game
// ---------------------------------------------------------------------------------------------

pub struct Swarm {
    pub rng: Rng,
    pub tick: u32,
    pub player: PlayerState,
    pub up: Upgrades,
    pub stats: Stats,
    pub enemies: Vec<Enemy>,
    pub bolts: Vec<Bolt>,
    pub orbs: Vec<Orb>,
    pub gems: Vec<Gem>,
    pub rings: [Ring; RING_SLOTS],
    pub orbit_pos: [[f32; 2]; ORBIT_MAX],
    pub orbit_count: usize,
    pub choices: Option<Choices>,
    pub pending_levels: u32,
    pub over: bool,
    /// The run ended by surviving to [`WIN_SECS`] rather than by dying.
    pub won: bool,
    pub kills: u32,
    pub bosses: u32,
    pub ev: Events,
    pub alive_enemies: usize,
    /// Test hook: enemies stand still.
    pub freeze: bool,
    cap: usize,
    fire_cd: u32,
    nova_cd: u32,
    orbit_angle: f32,
    alive_idx: Vec<u16>,
}

impl Swarm {
    pub fn new(seed: u64) -> Self {
        Self::with_cap(seed, enemy_cap())
    }

    pub fn with_cap(seed: u64, cap: usize) -> Self {
        let up = Upgrades::default();
        let stats = derive_stats(&up);
        Self {
            rng: Rng::new(seed),
            tick: 0,
            player: PlayerState { pos: [0.0; 2], height: 0.0, hp: stats.max_hp, max_hp: stats.max_hp, iframes: 0, xp: 0, level: 1, revived: false },
            up,
            stats,
            enemies: vec![Enemy::EMPTY; ENEMY_SLOTS],
            bolts: vec![Bolt::EMPTY; BOLT_SLOTS],
            orbs: vec![Orb { alive: false, pos: [0.0; 2], vel: [0.0; 2], life: 0, dmg: 0.0 }; ORB_SLOTS],
            gems: vec![Gem { alive: false, kind: GemKind::Xp, pos: [0.0; 2], value: 0, magnet: false, speed: 0.0 }; GEM_SLOTS],
            rings: [Ring { alive: false, kind: RingKind::Nova, center: [0.0; 2], max: 0.0, age: 0, life: 1 }; RING_SLOTS],
            orbit_pos: [[0.0; 2]; ORBIT_MAX],
            orbit_count: 0,
            choices: None,
            pending_levels: 0,
            over: false,
            won: false,
            kills: 0,
            bosses: 0,
            ev: Events::default(),
            alive_enemies: 0,
            freeze: false,
            cap: cap.min(ENEMY_SLOTS),
            fire_cd: 20,
            nova_cd: 60,
            orbit_angle: 0.0,
            alive_idx: Vec::with_capacity(ENEMY_SLOTS),
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

    /// Damage the player unless the i-frames are up. Returns whether it landed.
    pub fn hurt_player(&mut self, amount: f32) -> bool {
        if self.player.iframes > 0 || self.over || amount <= 0.0 {
            return false;
        }
        self.player.hp -= amount;
        self.player.iframes = IFRAMES;
        self.ev.hit = true;
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
        self.stats = derive_stats(&self.up);
        let old_max = self.player.max_hp;
        self.player.max_hp = self.stats.max_hp;
        if self.stats.max_hp > old_max {
            // Vitality pays out the new health plus a little more.
            self.player.hp = (self.player.hp + (self.stats.max_hp - old_max) + 0.1 * self.stats.max_hp).min(self.stats.max_hp);
        }
        self.choices = None;
        self.pending_levels = self.pending_levels.saturating_sub(1);
        self.offer();
        true
    }

    /// Roll the next card set when a level-up is waiting.
    fn offer(&mut self) {
        while self.pending_levels > 0 && self.choices.is_none() {
            let c = roll_choices(&mut self.rng, &self.up, self.player.level);
            if c.len == 0 {
                // Everything is maxed: take a heal instead.
                self.heal(0.25 * self.player.max_hp);
                self.pending_levels -= 1;
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
        if self.paused() {
            return;
        }
        self.player.pos = f.pos;
        self.player.height = f.height;
        self.tick += 1;
        let secs = self.secs();
        self.player.iframes = self.player.iframes.saturating_sub(1);
        if self.stats.regen_per_tick > 0.0 {
            self.heal(self.stats.regen_per_tick);
        }
        for e in self.enemies.iter_mut().filter(|e| e.alive) {
            e.prev = e.pos;
            e.flash = e.flash.saturating_sub(1);
        }
        for b in self.bolts.iter_mut().filter(|b| b.alive) {
            b.prev = b.pos;
        }
        self.spawn(secs);
        self.move_enemies();
        self.separate();
        self.attack();
        self.update_bolts();
        self.update_orbit();
        if f.pound_land {
            self.quake();
        }
        self.update_orbs();
        self.contact(secs);
        self.update_gems();
        self.update_rings();
        if self.player.hp <= 0.0 {
            self.die();
        }
        if !self.over && secs >= WIN_SECS {
            self.won = true;
            self.over = true;
            self.ev.won = true;
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
                if self.enemies[i].alive && dist2(self.enemies[i].pos, c) < 900.0 * 900.0 && self.enemies[i].kind != EnemyKind::King {
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
        if self.alive_enemies >= self.cap && kind != EnemyKind::King {
            return false;
        }
        let Some(slot) = self.free_enemy() else { return false };
        let s = enemy_stats(kind);
        let boss_scale = if kind == EnemyKind::King { 1.0 + 0.5 * self.bosses as f32 } else { 1.0 };
        let hp = s.hp * hp_mult(secs) * boss_scale;
        let timer = match kind {
            EnemyKind::Wisp => 40 + self.rng.below(30) as i16,
            EnemyKind::King => 90,
            EnemyKind::Dasher => 20 + self.rng.below(20) as i16,
            _ => 0,
        };
        self.enemies[slot] = Enemy { alive: true, kind, pos, prev: pos, vel: [0.0; 2], hp, max_hp: hp, contact: s.contact * damage_mult(secs), speed: s.speed * speed_mult(secs), radius: s.radius, flash: 0, state: 0, timer, dash: [0.0; 2], orbit_cd: 0 };
        self.alive_enemies += 1;
        true
    }

    /// A point on a ring around the player, kept inside the arena.
    fn edge_point(&mut self, min_d: f32, max_d: f32) -> [f32; 2] {
        let p = self.player.pos;
        let lim = ARENA_HALF - 150.0;
        for _ in 0..8 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = self.rng.range(min_d, max_d);
            let q = [p[0] + a.cos() * d, p[1] + a.sin() * d];
            if q[0].abs() < lim && q[1].abs() < lim {
                return q;
            }
        }
        // Cornered: pick the farthest corner-ish point.
        [if p[0] > 0.0 { -lim } else { lim }, if p[1] > 0.0 { -lim } else { lim }]
    }

    fn spawn(&mut self, secs: f32) {
        // Mini-boss every two minutes.
        if secs >= BOSS_PERIOD_SECS * (self.bosses + 1) as f32 {
            let pos = self.edge_point(1700.0, 2100.0);
            if self.spawn_enemy(EnemyKind::King, pos, secs) {
                self.bosses += 1;
                self.ev.boss_spawned = true;
            }
        }
        // A ring of skitters closes in every 40 seconds.
        if self.tick >= SURGE_PERIOD_TICKS && self.tick % SURGE_PERIOD_TICKS == 0 {
            self.ev.surge = true;
            let n = 10 + (secs / 15.0) as usize;
            let c = self.player.pos;
            let lim = ARENA_HALF - 150.0;
            for i in 0..n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let q = [(c[0] + a.cos() * 1500.0).clamp(-lim, lim), (c[1] + a.sin() * 1500.0).clamp(-lim, lim)];
                if !self.spawn_enemy(EnemyKind::Skitter, q, secs) {
                    break;
                }
            }
        }
        let want = target_live(secs, self.cap);
        let mut budget = 3;
        while self.alive_enemies < want && budget > 0 {
            budget -= 1;
            let kind = self.pick_kind(secs);
            let pos = self.edge_point(1600.0, 2200.0);
            if !self.spawn_enemy(kind, pos, secs) {
                break;
            }
        }
    }

    fn pick_kind(&mut self, secs: f32) -> EnemyKind {
        let w = kind_weights(secs);
        let total: f32 = w.iter().sum();
        let mut r = self.rng.f32() * total;
        for (i, k) in [EnemyKind::Skitter, EnemyKind::Brute, EnemyKind::Dasher, EnemyKind::Wisp].into_iter().enumerate() {
            if r < w[i] {
                return k;
            }
            r -= w[i];
        }
        EnemyKind::Skitter
    }

    // ---- enemies -----------------------------------------------------------------------------

    fn move_enemies(&mut self) {
        let p = self.player.pos;
        let lim = ARENA_HALF - 100.0;
        self.alive_idx.clear();
        for i in 0..ENEMY_SLOTS {
            if !self.enemies[i].alive {
                continue;
            }
            self.alive_idx.push(i as u16);
            let mut fire: Option<(f32, usize)> = None; // (dmg, ring size or 0 = aimed)
            if self.freeze {
                continue;
            }
            let e = &mut self.enemies[i];
            let to = [p[0] - e.pos[0], p[1] - e.pos[1]];
            let d = (to[0] * to[0] + to[1] * to[1]).sqrt().max(0.001);
            let dir = [to[0] / d, to[1] / d];
            let mut step = [0.0f32; 2];
            match e.kind {
                EnemyKind::Skitter | EnemyKind::Brute => {
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
            }
            e.vel = step;
            e.pos[0] = (e.pos[0] + step[0]).clamp(-lim, lim);
            e.pos[1] = (e.pos[1] + step[1]).clamp(-lim, lim);
            if let Some((dmg, ring)) = fire {
                let origin = e.pos;
                if ring == 0 {
                    self.fire_orb(origin, dir, 13.0, dmg * damage_mult(self.secs()));
                } else {
                    let phase = self.rng.range(0.0, std::f32::consts::TAU);
                    for k in 0..ring {
                        let a = phase + k as f32 / ring as f32 * std::f32::consts::TAU;
                        self.fire_orb(origin, [a.cos(), a.sin()], 9.0, dmg * damage_mult(self.secs()));
                    }
                }
            }
        }
    }

    fn fire_orb(&mut self, pos: [f32; 2], dir: [f32; 2], speed: f32, dmg: f32) {
        if let Some(o) = self.orbs.iter_mut().find(|o| !o.alive) {
            *o = Orb { alive: true, pos, vel: [dir[0] * speed, dir[1] * speed], life: 110, dmg };
        }
    }

    /// Push overlapping enemies apart (heavier ones move less).
    fn separate(&mut self) {
        let n = self.alive_idx.len();
        for a in 0..n {
            let i = self.alive_idx[a] as usize;
            for b in a + 1..n {
                let j = self.alive_idx[b] as usize;
                let (pi, pj) = (self.enemies[i].pos, self.enemies[j].pos);
                let (dx, dz) = (pj[0] - pi[0], pj[1] - pi[1]);
                let (mi, mj) = (self.enemies[i].radius, self.enemies[j].radius);
                let reach = (mi + mj) * 0.8;
                let d2 = dx * dx + dz * dz;
                if d2 >= reach * reach {
                    continue;
                }
                let d = d2.sqrt();
                let (nx, nz) = if d > 0.01 { (dx / d, dz / d) } else { (1.0, 0.0) };
                let push = (reach - d) * 0.35;
                let (fi, fj) = (mj / (mi + mj), mi / (mi + mj));
                self.enemies[i].pos = [pi[0] - nx * push * fi * 2.0, pi[1] - nz * push * fi * 2.0];
                self.enemies[j].pos = [pj[0] + nx * push * fj * 2.0, pj[1] + nz * push * fj * 2.0];
            }
        }
    }

    // ---- attacks -----------------------------------------------------------------------------

    fn attack(&mut self) {
        let p = self.player.pos;
        // Star volley at the nearest foes.
        self.fire_cd = self.fire_cd.saturating_sub(1);
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
                    let dmg = self.stats.damage * if crit { 2.0 } else { 1.0 };
                    self.fire_bolt(p, a, dmg);
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
                self.nova_cd = (130.0 - 18.0 * lvl) as u32;
                let radius = (380.0 + 50.0 * lvl) * self.stats.area;
                let dmg = 0.9 * self.stats.damage * (1.0 + 0.25 * lvl);
                self.add_ring(RingKind::Nova, p, radius, 12);
                self.ev.nova = true;
                self.area_damage(p, radius, dmg, 0.0);
            }
        }
    }

    fn fire_bolt(&mut self, from: [f32; 2], angle: f32, dmg: f32) {
        if let Some(b) = self.bolts.iter_mut().find(|b| !b.alive) {
            let v = [angle.cos() * BOLT_SPEED, angle.sin() * BOLT_SPEED];
            *b = Bolt { alive: true, pos: from, prev: from, vel: v, life: (BOLT_RANGE / BOLT_SPEED) as u16, dmg, pierce: self.stats.pierce, radius: 42.0 * self.stats.area.sqrt(), recent: [u16::MAX; 3] };
        }
    }

    fn update_bolts(&mut self) {
        for bi in 0..BOLT_SLOTS {
            if !self.bolts[bi].alive {
                continue;
            }
            {
                let b = &mut self.bolts[bi];
                b.pos[0] += b.vel[0];
                b.pos[1] += b.vel[1];
                b.life = b.life.saturating_sub(1);
                if b.life == 0 || b.pos[0].abs() > ARENA_HALF || b.pos[1].abs() > ARENA_HALF {
                    b.alive = false;
                    continue;
                }
            }
            for k in 0..self.alive_idx.len() {
                let i = self.alive_idx[k] as usize;
                let b = self.bolts[bi];
                let e = &self.enemies[i];
                if !e.alive || b.recent.contains(&(i as u16)) {
                    continue;
                }
                let reach = e.radius + b.radius;
                if dist2(e.pos, b.pos) > reach * reach {
                    continue;
                }
                self.damage_enemy(i, b.dmg);
                let e = &mut self.enemies[i];
                if e.alive {
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
        for e in self.enemies.iter_mut().filter(|e| e.alive) {
            e.orbit_cd = e.orbit_cd.saturating_sub(1);
        }
        for k in 0..n {
            let a = self.orbit_angle + k as f32 / n as f32 * std::f32::consts::TAU;
            let q = [p[0] + a.cos() * radius, p[1] + a.sin() * radius];
            self.orbit_pos[k] = q;
            for idx in 0..self.alive_idx.len() {
                let i = self.alive_idx[idx] as usize;
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
    }

    fn quake(&mut self) {
        let lvl = self.stats.quake as f32;
        let radius = 420.0 + 80.0 * lvl;
        let dmg = self.stats.damage * (2.2 + 0.8 * lvl);
        let c = self.player.pos;
        self.add_ring(RingKind::Quake, c, radius, 10);
        self.ev.quake = true;
        self.area_damage(c, radius, dmg, 70.0);
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
            if knock > 0.0 && self.enemies[i].alive && self.enemies[i].kind != EnemyKind::King {
                let d = d2.sqrt().max(1.0);
                let e = &mut self.enemies[i];
                e.pos[0] += (e.pos[0] - c[0]) / d * knock;
                e.pos[1] += (e.pos[1] - c[1]) / d * knock;
            }
        }
    }

    fn add_ring(&mut self, kind: RingKind, center: [f32; 2], max: f32, life: u16) {
        if let Some(r) = self.rings.iter_mut().find(|r| !r.alive) {
            *r = Ring { alive: true, kind, center, max, age: 0, life };
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

    /// Hit an enemy; handles the kill and its drops.
    pub fn damage_enemy(&mut self, i: usize, dmg: f32) {
        let e = &mut self.enemies[i];
        if !e.alive {
            return;
        }
        e.hp -= dmg;
        e.flash = 4;
        if e.hp > 0.0 {
            return;
        }
        e.alive = false;
        let (kind, pos) = (e.kind, e.pos);
        self.alive_enemies = self.alive_enemies.saturating_sub(1);
        self.kills += 1;
        self.ev.kills += 1;
        self.ev.last_kill = pos;
        let xp = enemy_stats(kind).xp;
        if kind == EnemyKind::King {
            self.ev.boss_killed = true;
            for k in 0..5 {
                let a = k as f32 * 1.2566;
                self.drop_gem(GemKind::Xp, [pos[0] + a.cos() * 140.0, pos[1] + a.sin() * 140.0], xp / 5);
            }
            self.drop_gem(GemKind::Heart, pos, 0);
        } else {
            self.drop_gem(GemKind::Xp, pos, xp);
            if self.rng.chance(0.02) {
                self.drop_gem(GemKind::Heart, [pos[0] + 40.0, pos[1]], 0);
            }
        }
    }

    // ---- hazards, gems -----------------------------------------------------------------------

    fn update_orbs(&mut self) {
        let p = self.player.pos;
        let h = self.player.height;
        let mut hit = 0.0;
        for o in self.orbs.iter_mut().filter(|o| o.alive) {
            o.pos[0] += o.vel[0];
            o.pos[1] += o.vel[1];
            o.life = o.life.saturating_sub(1);
            if o.life == 0 || o.pos[0].abs() > ARENA_HALF + 200.0 || o.pos[1].abs() > ARENA_HALF + 200.0 {
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
            if !e.alive {
                continue;
            }
            let s = enemy_stats(e.kind);
            let reach = s.radius + PLAYER_RADIUS;
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

    fn update_gems(&mut self) {
        let p = self.player.pos;
        let magnet = self.stats.magnet;
        let mut xp = 0u32;
        let mut hearts = 0u16;
        let mut picked = 0u16;
        for g in self.gems.iter_mut().filter(|g| g.alive) {
            let d2 = dist2(g.pos, p);
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
                }
            } else {
                g.pos[0] += (p[0] - g.pos[0]) / d * g.speed;
                g.pos[1] += (p[1] - g.pos[1]) / d * g.speed;
            }
        }
        self.ev.gems = picked;
        self.ev.hearts = hearts;
        for _ in 0..hearts {
            self.heal(0.25 * self.player.max_hp);
        }
        if xp > 0 {
            self.add_xp(xp);
        }
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
        PlayerFrame { pos: [x, z], height: 0.0, pound_land: false }
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
        assert!(speed_mult(3600.0) <= 1.4 + 1e-6);
        assert!(damage_mult(600.0) > damage_mult(0.0));
        assert_eq!(target_live(0.0, 200), 8);
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
        let base = derive_stats(&Upgrades::default());
        assert_eq!((base.bolts, base.interval, base.orbit, base.nova), (1, 26, 0, 0));
        let mut u = Upgrades::default();
        for x in ALL_UPGRADES {
            while u.take(x) {}
        }
        let max = derive_stats(&u);
        assert_eq!(max.bolts, 6);
        assert!(max.interval >= 6 && max.interval < base.interval);
        assert!(max.damage > 2.5 * base.damage);
        assert_eq!(max.orbit, 5);
        assert!(max.revive && max.pierce == 4 && max.quake == 4);
        assert!((max.max_hp - 280.0).abs() < 1e-3);
        for x in ALL_UPGRADES {
            let i = x.info();
            assert!(i.max >= 1 && i.weight > 0 && !i.name.is_empty() && !i.desc.is_empty());
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
            s.tick(&PlayerFrame { pos: [a.cos() * 1500.0, a.sin() * 1500.0], height: 0.0, pound_land: t % 200 == 0 });
            while s.choices.is_some() {
                s.pick(0);
            }
            peak = peak.max(s.alive_enemies);
            assert!(s.alive_enemies <= s.cap() + 4, "live {} over the cap", s.alive_enemies);
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
        assert!(a.4 <= 164, "{a:?}");
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
            s.tick(&PlayerFrame { pos: [a.cos() * 900.0, a.sin() * 900.0], height: 0.0, pound_land: false });
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
        const PRIORITY: [Upgrade; 15] = [
            Upgrade::Bolts,
            Upgrade::Damage,
            Upgrade::FireRate,
            Upgrade::Vitality,
            Upgrade::Regen,
            Upgrade::Speed,
            Upgrade::Area,
            Upgrade::Pierce,
            Upgrade::Nova,
            Upgrade::Orbit,
            Upgrade::SecondChance,
            Upgrade::Crit,
            Upgrade::Magnet,
            Upgrade::Quake,
            Upgrade::Jump,
        ];
        let mut s = Swarm::with_cap(seed, 160);
        let mut pos = [0.0f32; 2];
        let mut heading = 0.0f32;
        for t in 0..(WIN_SECS as u32 + 30) * TICKS_PER_SEC {
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
            // Walls and the middle: a gentle pull to the centre that grows near the edge.
            let edge = (pos[0].abs().max(pos[1].abs()) / ARENA_HALF).powi(3);
            push[0] -= pos[0] / ARENA_HALF * 2.0 * edge;
            push[1] -= pos[1] / ARENA_HALF * 2.0 * edge;
            let pl = (push[0] * push[0] + push[1] * push[1]).sqrt();
            let speed = (24.0 * (1.0 + s.stats.move_bonus)).min(40.0);
            if pl > 0.05 {
                let want = push[1].atan2(push[0]);
                let mut d = want - heading;
                while d > std::f32::consts::PI {
                    d -= std::f32::consts::TAU;
                }
                while d < -std::f32::consts::PI {
                    d += std::f32::consts::TAU;
                }
                heading += d.clamp(-0.35, 0.35);
                pos[0] = (pos[0] + heading.cos() * speed).clamp(-ARENA_HALF + 90.0, ARENA_HALF - 90.0);
                pos[1] = (pos[1] + heading.sin() * speed).clamp(-ARENA_HALF + 90.0, ARENA_HALF - 90.0);
            }
            if hp_cheat {
                s.player.hp = s.player.max_hp;
            }
            s.tick(&PlayerFrame { pos, height: 0.0, pound_land: t % 240 == 0 });
            while s.choices.is_some() {
                let c = s.choices.unwrap();
                let best = (0..c.len).min_by_key(|&i| PRIORITY.iter().position(|u| *u == c.items[i]).unwrap()).unwrap();
                s.pick(best);
            }
            if s.over {
                break;
            }
        }
        s
    }

    /// The goal is reachable: a kiting player who takes sensible cards survives to the dawn on
    /// every seed, and the run then ends as a win (not a death) with health left.
    #[test]
    fn a_kiting_player_survives_to_the_dawn_and_wins() {
        for seed in 1..=8u64 {
            let s = kite(seed, false);
            assert!(s.over && s.won && s.player.hp > 0.0, "seed {seed}: won={} secs={:.0} hp={}", s.won, s.secs(), s.player.hp);
            assert!(s.secs() >= WIN_SECS && s.secs() < WIN_SECS + 1.0, "seed {seed}: ended at {}", s.secs());
            assert!(s.bosses >= 3, "seed {seed}: kings seen {}", s.bosses);
        }
    }

    #[test]
    fn the_win_fires_once_and_death_beats_a_simultaneous_dawn() {
        let mut s = Swarm::with_cap(3, 20);
        s.tick = (WIN_SECS * TICKS_PER_SEC as f32) as u32 - 1;
        s.tick(&frame_at(0.0, 0.0));
        assert!(s.over && s.won && s.ev.won && !s.ev.died);
        s.tick(&frame_at(0.0, 0.0));
        assert!(!s.ev.won, "the win event fires once");
        let mut d = Swarm::with_cap(3, 20);
        d.tick = (WIN_SECS * TICKS_PER_SEC as f32) as u32 - 1;
        d.player.hp = 0.0;
        d.tick(&frame_at(0.0, 0.0));
        assert!(d.over && !d.won && d.ev.died);
    }

    #[test]
    fn clock_format() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(61.9), "1:01");
        assert_eq!(clock(600.0), "10:00");
    }
}
