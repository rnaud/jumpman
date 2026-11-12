//! Star Swarm: an arena-survivor ("horde survivor") mode on the real physics of whoever you picked.
//!
//! You run, jump, dive and ground-pound through the sim as usual; your weapons fire on their own,
//! waves of foes pour in over a real stretch of a real course (Bob-omb Battlefield, Whomp's Fortress,
//! Hyrule Field, ...: see `swarm_levels.rs`), kills drop gems and chests, gems level you up, and
//! every level offers three upgrades, some of them evolutions. A storm ring closes in on the field;
//! stay inside it, survive to 7:00 and kill the Star Tyrant to win. The run also ends when your
//! health does.
//!
//! * The rules live in `swarm_core.rs` (deterministic, seeded, unit-tested); this file glues them
//!   to the host: it samples the course's ground, feeds the core the player's position every 30 Hz
//!   tick, applies the movement upgrades to the sim (extra run speed, higher jumps, ground-pound
//!   quake, dive strikes) and publishes render lists and HUD numbers through [`SwarmBridge`].
//! * `swarm_meta.rs` keeps the progression between runs (tiers, ranks, per-level bests).
//! * `swarm_view.rs` draws everything and owns the HUD, the briefing, the level-up cards and the
//!   run summary.
//!
//! The course itself bounds the fight (its cliffs, water and walls); the only addition is the storm
//! ring (no invisible walls). The player who falls off the world is put back on the last safe ground.
//!
//! Test hooks: `JUMPMAN_SWARM_SEED=<n>` fixes the run seed; `JUMPMAN_SWARM_LEVEL=<key>` and
//! `JUMPMAN_SWARM_TIER=<0..2>` pick the battlefield and difficulty; `JUMPMAN_SWARM_SKIPBRIEF=1`
//! starts the run without the briefing; `JUMPMAN_SWARM_START=<secs>` begins with the waves of that
//! time, `JUMPMAN_SWARM_XP=<n>` grants XP up front, `JUMPMAN_SWARM_BUILD=1` hands out a mid-run
//! build, `JUMPMAN_SWARM_GOD=1` keeps health full. Hooked runs never touch the records.
use super::foes_view::FoeItem;
use super::swarm_core::*;
use super::swarm_levels::{LevelDef, LEVELS};
use super::swarm_meta::{self, Meta};
use super::swarm_view::{BriefAction, Briefing, CardView, NumItem, RItem, Summary};
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::to_bevy;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::state::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;

/// Cell size of the terrain sampling grid.
const CELL: f32 = 100.0;
/// Pause between a card appearing and it accepting input (stray thumbs).
const CARD_LOCKOUT: f64 = 0.5;
const SUMMARY_LOCKOUT: f64 = 0.9;
/// Gems and shots beyond this distance from the player are not drawn.
const DRAW_RANGE: f32 = 4200.0;
/// Pillars drawn on the storm ring.
pub const RING_POSTS: usize = 64;
/// Tall light pillars over elites and chests.
pub const BEACON_SLOTS: usize = 12;

// ---------------------------------------------------------------------------------------------
// The picked battlefield and difficulty
// ---------------------------------------------------------------------------------------------

static LEVEL_PICK: AtomicUsize = AtomicUsize::new(0);
static TIER_PICK: AtomicUsize = AtomicUsize::new(0);
static LOADED_PICK: AtomicBool = AtomicBool::new(false);
static ENV_PICK: OnceLock<()> = OnceLock::new();

/// The last choice (the save file's, then the environment's) is read once, the first time anyone asks.
fn load_pick() {
    if LOADED_PICK.swap(true, Ordering::Relaxed) {
        return;
    }
    let m = load_meta();
    LEVEL_PICK.store(super::swarm_levels::index_of(&m.last_level), Ordering::Relaxed);
    TIER_PICK.store(m.last_tier.min(swarm_meta::TIERS - 1), Ordering::Relaxed);
    ENV_PICK.get_or_init(|| {
        if let Some(i) = std::env::var("JUMPMAN_SWARM_LEVEL").ok().and_then(|k| LEVELS.iter().position(|l| l.key == k)) {
            LEVEL_PICK.store(i, Ordering::Relaxed);
        }
        if let Some(t) = std::env::var("JUMPMAN_SWARM_TIER").ok().and_then(|v| v.parse::<usize>().ok()) {
            TIER_PICK.store(t.min(swarm_meta::TIERS - 1), Ordering::Relaxed);
        }
    });
}

pub fn level_index() -> usize {
    load_pick();
    LEVEL_PICK.load(Ordering::Relaxed).min(LEVELS.len() - 1)
}

pub fn level() -> &'static LevelDef {
    &LEVELS[level_index()]
}

pub fn tier_index() -> usize {
    load_pick();
    TIER_PICK.load(Ordering::Relaxed)
}

/// Per-kind display: model scale and hover height.
fn look(kind: EnemyKind) -> (f32, f32) {
    match kind {
        EnemyKind::Skitter => (2.0, 0.0),
        EnemyKind::Brute => (0.7, 0.0),
        EnemyKind::Dasher => (3.0, 0.0),
        EnemyKind::Wisp => (0.9, 50.0),
        EnemyKind::King => (2.4, 0.0),
        EnemyKind::Bomber => (1.8, 0.0),
        EnemyKind::Splitter => (1.3, 0.0),
        EnemyKind::Chomp => (1.6, 0.0),
        EnemyKind::Tyrant => (0.8, 0.0),
        // The ported species are drawn by `foes_view`.
        _ => (1.0, 0.0),
    }
}

/// The course's ground under the arena, sampled once on entering: heights for snapping everything
/// to the terrain and a walkable mask for the enemies.
pub struct Terrain {
    center: [f64; 2],
    half: f32,
    base: f32,
    max_rise: f32,
    /// (n + 1)^2 corner heights, row-major over z then x.
    heights: Vec<f32>,
    n: usize,
}

impl Terrain {
    /// Sample the ground of a disc of radius `half` around `center` from above (`top`).
    pub fn sample(world: &CollisionWorld, center: [f64; 2], half: f32, top: f64, max_rise: f32) -> Self {
        let n = (half * 2.0 / CELL) as usize;
        let mut heights = Vec::with_capacity((n + 1) * (n + 1));
        let mut found = vec![];
        for iz in 0..=n {
            for ix in 0..=n {
                found.push(super::swarm_levels::ground_at(world, center[0] + (-half + ix as f32 * CELL) as f64, center[1] + (-half + iz as f32 * CELL) as f64, top));
            }
        }
        // The level of the field: the median of what was found.
        let mut sorted: Vec<f32> = found.iter().flatten().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let base = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
        for h in &found {
            heights.push(h.unwrap_or(f32::NAN));
        }
        Self { center, half, base, max_rise, heights, n }
    }

    pub fn center(&self) -> [f64; 2] {
        self.center
    }

    fn corner(&self, ix: usize, iz: usize) -> f32 {
        self.heights[iz.min(self.n) * (self.n + 1) + ix.min(self.n)]
    }

    /// Ground height at an arena-local point (bilinear; blocked corners count as the field's level).
    pub fn height(&self, p: [f32; 2]) -> f32 {
        let fx = ((p[0] + self.half) / CELL).clamp(0.0, self.n as f32);
        let fz = ((p[1] + self.half) / CELL).clamp(0.0, self.n as f32);
        let (ix, iz) = (fx.floor() as usize, fz.floor() as usize);
        let (tx, tz) = (fx - ix as f32, fz - iz as f32);
        let c = |x, z| {
            let h = self.corner(x, z);
            if h.is_nan() { self.base } else { h }
        };
        let a = c(ix, iz) + (c(ix + 1, iz) - c(ix, iz)) * tx;
        let b = c(ix, iz + 1) + (c(ix + 1, iz + 1) - c(ix, iz + 1)) * tx;
        a + (b - a) * tz
    }

    /// Can something walk on cell (ix, iz)? Needs ground at all four corners, near the field's
    /// level, without a cliff step between neighbours.
    fn cell_ok(&self, ix: usize, iz: usize) -> bool {
        let hs = [self.corner(ix, iz), self.corner(ix + 1, iz), self.corner(ix, iz + 1), self.corner(ix + 1, iz + 1)];
        if hs.iter().any(|h| h.is_nan() || (h - self.base).abs() > self.max_rise) {
            return false;
        }
        let (lo, hi) = hs.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
        hi - lo < 110.0
    }

    /// The walkable cells inside the arena disc.
    pub fn mask(&self) -> WalkMask {
        WalkMask::build(self.half, CELL, |x, z| {
            if x * x + z * z > self.half * self.half {
                return false;
            }
            let (ix, iz) = (((x + self.half) / CELL) as usize, ((z + self.half) / CELL) as usize);
            self.cell_ok(ix, iz)
        })
    }

    pub fn base(&self) -> f32 {
        self.base
    }
}

/// Frame-rate independent smoothing of the point the camera looks at. The player is already
/// interpolated to the render time; this takes the last bit of unevenness out (landings, hits,
/// the running bob) without trailing noticeably: at 12 /s the lag behind a full-speed run is a
/// tenth of the screen's height at most.
#[derive(Clone, Copy, Debug)]
pub struct Follow {
    pub focus: DVec3,
    primed: bool,
}

impl Follow {
    pub const RATE_XZ: f64 = 14.0;
    pub const RATE_Y: f64 = 5.0;

    pub fn new() -> Self {
        Self { focus: DVec3::ZERO, primed: false }
    }
    /// Jump straight to `target` (a restart, a teleport).
    pub fn snap(&mut self) {
        self.primed = false;
    }
    pub fn update(&mut self, target: DVec3, dt: f64) -> DVec3 {
        if !self.primed || self.focus.distance(target) > 2500.0 {
            self.focus = target;
            self.primed = true;
        } else {
            let kxz = 1.0 - (-Self::RATE_XZ * dt).exp();
            let ky = 1.0 - (-Self::RATE_Y * dt).exp();
            self.focus.x += (target.x - self.focus.x) * kxz;
            self.focus.z += (target.z - self.focus.z) * kxz;
            self.focus.y += (target.y - self.focus.y) * ky;
        }
        self.focus
    }
}

fn best_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("swarm.json")
}

/// The progression file (the old single-record format is read as the Bob-omb Battlefield record).
pub fn load_meta() -> Meta {
    std::fs::read_to_string(best_path()).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).map(|v| Meta::from_json(&v)).unwrap_or_default()
}

fn save_meta(m: &Meta) {
    let path = best_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, m.to_json().to_string());
}

fn run_seed() -> u64 {
    std::env::var("JUMPMAN_SWARM_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64))
}

fn hero_of(c: crate::characters::Character) -> Hero {
    use crate::characters::Character as C;
    match c {
        C::Mario => Hero::Mario,
        C::Link | C::YoungLink => Hero::Link,
        C::Banjo => Hero::Banjo,
    }
}

fn hero_name(h: Hero) -> &'static str {
    match h {
        Hero::Plain => "HERO",
        Hero::Mario => "MARIO: DIVE STRIKE + GROUND QUAKE",
        Hero::Link => "LINK: SPIN SLASH",
        Hero::Banjo => "BANJO: STAR BOOMERANG",
    }
}

/// Test hooks that skip the records.
fn cheating() -> bool {
    ["JUMPMAN_SWARM_START", "JUMPMAN_SWARM_XP", "JUMPMAN_SWARM_GOD", "JUMPMAN_SWARM_UPGRADES", "JUMPMAN_SWARM_BUILD"].iter().any(|k| std::env::var_os(k).is_some())
}

pub struct StarSwarmExperiment {
    core: Swarm,
    seed: u64,
    runs: u32,
    /// Seconds since entering (render clock, keeps running while paused).
    t: f64,
    last_action: u32,
    was_air: bool,
    meta: Meta,
    level: &'static LevelDef,
    tier_i: usize,
    new_best: bool,
    briefing: bool,
    card_gen_shown: u32,
    cards_at: f64,
    over_at: f64,
    sfx_tick: u32,
    yaws: Vec<f32>,
    first_run: bool,
    /// Where the arena sits on the course (centre x/z, floor level) and its sampled ground.
    center: [f64; 3],
    terrain: Option<Terrain>,
    follow: Follow,
    /// Camera shake for hits, novas, quakes and kings.
    shake: super::feel::Shake,
    /// The slow-motion beat after a big moment lasts until this render time.
    hitstop_until: f64,
    /// The last standing-on-solid-ground spot inside the safe disc (a fall puts the player back here).
    safe: [f64; 3],
    upgrades_seen: [u8; UPGRADE_COUNT],
    build_gen: u32,
}

impl StarSwarmExperiment {
    /// Particles and camera shake for the events of the tick that just ran (bounded: a handful of particles a tick).
    fn feel(&mut self, host: &mut Host, mario_pos: [f64; 3]) {
        use crate::fx::particles::BurstOptions;
        let ev = self.core.ev;
        let at = |s: &Self, p: [f32; 2], h: f64| [s.center[0] + p[0] as f64, s.ground(p) as f64 + h, s.center[2] + p[1] as f64];
        let up = |h: f64| [mario_pos[0], mario_pos[1] + h, mario_pos[2]];
        if ev.kills > 0 {
            let n = (3 + ev.kills as usize * 2).min(12);
            host.particles.burst(at(self, ev.last_kill, 60.0), BurstOptions { color: 0xfff1b8, count: n, speed: 260.0, life: 0.4, gravity: -500.0, size: 46.0, upward: 160.0, spread: 1.0, ..Default::default() });
        }
        if ev.gems > 0 {
            host.particles.burst(up(90.0), BurstOptions { color: 0x7fe6ff, count: 3, speed: 120.0, life: 0.35, gravity: 120.0, size: 30.0, upward: 220.0, spread: 1.0, ..Default::default() });
        }
        if ev.hit {
            self.shake.add(0.45);
            host.particles.burst(up(80.0), BurstOptions { color: 0xff5a4a, count: 10, speed: 320.0, life: 0.4, gravity: -600.0, size: 44.0, upward: 200.0, spread: 1.0, ..Default::default() });
        }
        if ev.nova {
            self.shake.add(0.3);
            host.particles.burst(up(60.0), BurstOptions { color: 0xbfe9ff, count: 18, speed: 620.0, life: 0.45, gravity: 0.0, size: 50.0, upward: 0.0, spread: 1.0, ..Default::default() });
        }
        if ev.slash {
            host.particles.burst(up(70.0), BurstOptions { color: 0xe8f4ff, count: 14, speed: 540.0, life: 0.3, gravity: 0.0, size: 40.0, upward: 0.0, spread: 1.0, ..Default::default() });
        }
        if ev.thunder {
            self.shake.add(0.2);
            for k in 0..(ev.strike_n as usize).min(3) {
                host.particles.burst(at(self, ev.strike_pos[k], 40.0), BurstOptions { color: 0xfff3a0, count: 8, speed: 260.0, life: 0.35, gravity: -300.0, size: 52.0, upward: 520.0, spread: 0.5, ..Default::default() });
            }
        }
        if ev.rush_hit {
            host.particles.burst(up(40.0), BurstOptions { color: 0xffc27a, count: 3, speed: 200.0, life: 0.3, gravity: -200.0, size: 38.0, upward: 40.0, spread: 1.0, ..Default::default() });
        }
        if ev.blast {
            self.shake.add(0.5);
            host.particles.burst(at(self, ev.blast_pos, 60.0), BurstOptions { color: 0xff9a3a, count: 16, speed: 480.0, life: 0.5, gravity: -200.0, size: 70.0, upward: 200.0, spread: 1.0, ..Default::default() });
        }
        if ev.bomb {
            self.shake.add(0.8);
            host.particles.burst(up(60.0), BurstOptions { color: 0xff7a3a, count: 24, speed: 900.0, life: 0.6, gravity: 0.0, size: 80.0, upward: 0.0, spread: 1.0, ..Default::default() });
        }
        if ev.quake {
            self.shake.add(0.5);
            host.particles.burst(mario_pos, BurstOptions { color: 0xd8c39a, count: 16, speed: 520.0, life: 0.5, gravity: -300.0, size: 60.0, upward: 90.0, spread: 1.0, ..Default::default() });
        }
        if ev.level_up {
            host.particles.burst(up(60.0), BurstOptions { color: 0xffe066, count: 16, speed: 240.0, life: 0.7, gravity: 80.0, size: 44.0, upward: 420.0, spread: 1.0, ..Default::default() });
        }
        if ev.chest_opened {
            host.particles.burst(up(80.0), BurstOptions { color: 0xffd24a, count: 26, speed: 520.0, life: 0.8, gravity: -300.0, size: 64.0, upward: 460.0, spread: 1.0, ..Default::default() });
        }
        if ev.vacuum {
            host.particles.burst(up(60.0), BurstOptions { color: 0x6ff0ff, count: 20, speed: 760.0, life: 0.5, gravity: 0.0, size: 50.0, upward: 0.0, spread: 1.0, ..Default::default() });
        }
        if ev.boss_spawned {
            self.shake.add(0.6);
        }
        if ev.elite_killed {
            self.shake.add(0.5);
            host.particles.burst(at(self, ev.last_kill, 100.0), BurstOptions { color: 0xffd24a, count: 18, speed: 560.0, life: 0.7, gravity: -400.0, size: 70.0, upward: 300.0, spread: 1.0, ..Default::default() });
        }
        if ev.boss_killed {
            self.shake.add(1.0);
            for (color, speed) in [(0xffd24au32, 700.0f32), (0xffffffu32, 420.0)] {
                host.particles.burst(at(self, ev.last_kill, 110.0), BurstOptions { color, count: 20, speed, life: 0.8, gravity: -420.0, size: 80.0, upward: 320.0, spread: 1.0, ..Default::default() });
            }
        }
        if ev.revived {
            self.shake.add(0.5);
            host.particles.burst(up(80.0), BurstOptions { color: 0xffffff, count: 20, speed: 520.0, life: 0.6, gravity: 0.0, size: 54.0, upward: 120.0, spread: 1.0, ..Default::default() });
        }
        // The slow beat after the big moments.
        let beat = if ev.final_spawned || (ev.boss_killed && self.core.won) { 0.4 } else if ev.boss_killed { 0.22 } else if ev.elite_killed || ev.chest_opened || ev.revived { 0.1 } else { 0.0 };
        if beat > 0.0 {
            self.hitstop_until = self.hitstop_until.max(self.t + beat);
        }
    }

    pub fn new() -> Self {
        let seed = run_seed();
        let level = level();
        Self { core: Swarm::new(seed), seed, runs: 0, t: 0.0, last_action: 0, was_air: false, meta: Meta::default(), level, tier_i: tier_index(), new_best: false, briefing: false, card_gen_shown: 0, cards_at: 0.0, over_at: 0.0, sfx_tick: 0, yaws: vec![0.0; ENEMY_SLOTS], first_run: true, center: [level.center[0], 0.0, level.center[1]], terrain: None, follow: Follow::new(), shake: Default::default(), hitstop_until: 0.0, safe: [0.0; 3], upgrades_seen: [0; UPGRADE_COUNT], build_gen: 0 }
    }

    fn start_run(&mut self, host: &mut Host) {
        self.runs += 1;
        let seed = self.seed.wrapping_add(self.runs as u64 * 7919);
        let hero = hero_of(host.bridge.character);
        let tier = swarm_meta::tier(self.tier_i);
        let perks = swarm_meta::perks(self.meta.rank());
        self.core = Swarm::start(seed, enemy_cap(), hero, tier, perks);
        self.core.half = self.level.radius;
        if let Some(t) = &self.terrain {
            self.core.mask = Some(t.mask());
        }
        self.follow.snap();
        if let Some(secs) = std::env::var("JUMPMAN_SWARM_START").ok().and_then(|v| v.parse::<f32>().ok()) {
            self.core.tick = (secs * TICKS_PER_SEC as f32) as u32;
            self.core.bosses = ((secs / BOSS_PERIOD_SECS) as u32).min(3);
            self.core.elites = ((secs - ELITE_FIRST_SECS).max(0.0) / ELITE_PERIOD_SECS) as u32 + 1;
        }
        // Test hook: a mid-run build (the stars, a boomerang, lightning, a spin slash, armor and an evolution).
        if std::env::var_os("JUMPMAN_SWARM_BUILD").is_some() {
            for (u, n) in [(Upgrade::Damage, 4), (Upgrade::FireRate, 4), (Upgrade::Bolts, 3), (Upgrade::Area, 2), (Upgrade::Boomerang, 3), (Upgrade::Lightning, 2), (Upgrade::Whirl, 2), (Upgrade::Orbit, 2), (Upgrade::Nova, 2), (Upgrade::Vitality, 4), (Upgrade::Armor, 2), (Upgrade::StarStorm, 1)] {
                for _ in 0..n {
                    self.core.up.take(u);
                }
            }
            self.core.refresh_stats();
            self.core.player.hp = self.core.player.max_hp;
            self.core.player.level = 12;
        }
        // Test hook: `JUMPMAN_SWARM_UPGRADES=12:3,13:2` preloads upgrade levels by index.
        if let Ok(list) = std::env::var("JUMPMAN_SWARM_UPGRADES") {
            for (i, n) in list.split(',').filter_map(|p| p.split_once(':')).filter_map(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<u8>().ok()?))) {
                if let Some(u) = ALL_UPGRADES.get(i) {
                    for _ in 0..n {
                        self.core.up.take(*u);
                    }
                }
            }
            self.core.refresh_stats();
            self.core.player.hp = self.core.player.max_hp;
        }
        if let Some(xp) = std::env::var("JUMPMAN_SWARM_XP").ok().and_then(|v| v.parse::<u32>().ok()) {
            self.core.add_xp(xp);
        }
        self.new_best = false;
        self.last_action = 0;
        self.was_air = false;
        self.cards_at = 0.0;
        self.card_gen_shown = 0;
        self.hitstop_until = 0.0;
        self.upgrades_seen = [0; UPGRADE_COUNT];
        self.yaws.iter_mut().for_each(|y| *y = 0.0);
        if std::env::var_os("JUMPMAN_SWARM_SHOWCASE").is_some() {
            // Test hook: one of each foe in a row ahead of the player, for scale checks.
            let kinds = [EnemyKind::Skitter, EnemyKind::Brute, EnemyKind::Dasher, EnemyKind::Wisp, EnemyKind::King, EnemyKind::Bomber, EnemyKind::Splitter, EnemyKind::Chomp, EnemyKind::Tyrant];
            for (i, k) in kinds.into_iter().enumerate() {
                self.core.spawn_enemy(k, [900.0 - i as f32 * 300.0, 900.0], 0.0);
            }
            self.core.freeze = true;
        }
        // Start in the middle of the field, facing the middle of the action.
        let start_y = self.ground([0.0, 0.0]) as f64;
        host.place_mario([self.center[0], start_y, self.center[2]], Some(0.0));
        self.safe = [self.center[0], start_y, self.center[2]];
        host.set_player_visible(true);
        host.bridge.reset();
        host.bridge.active = true;
        host.bridge.hud.hint = true;
        host.bridge.hud.max_hp = self.core.player.max_hp;
        if !self.briefing {
            host.toast(if self.first_run { "SURVIVE THE SWARM!" } else { "AGAIN!" }, 1.6);
        }
        self.first_run = false;
    }

    fn briefing(&self, host: &Host) -> Briefing {
        let m = &self.meta;
        let rank = m.rank();
        let rec = m.record(self.level.key);
        let t = swarm_meta::tier(self.tier_i);
        Briefing {
            level: self.level_index(),
            level_count: LEVELS.len(),
            level_name: self.level.name,
            blurb: self.level.blurb,
            tier: self.tier_i,
            tier_name: t.name,
            tier_locked: [!m.tier_unlocked(0), !m.tier_unlocked(1), !m.tier_unlocked(2)],
            best_secs: rec.best_secs,
            wins: rec.wins,
            rank,
            bits: m.bits,
            next_bits: if rank >= swarm_meta::MAX_RANK { 0 } else { swarm_meta::bits_for_rank(rank + 1) },
            hero: hero_name(hero_of(host.bridge.character)),
        }
    }

    fn level_index(&self) -> usize {
        LEVELS.iter().position(|l| l.key == self.level.key).unwrap_or(0)
    }

    fn finish(&mut self, host: &mut Host) {
        let secs = self.core.secs();
        let res = self.meta.finish_run(self.level.key, self.tier_i, secs, self.core.kills, self.core.won, cheating());
        self.new_best = res.new_best;
        if !cheating() {
            self.meta.last_level = self.level.key.to_string();
            self.meta.last_tier = self.tier_i;
            save_meta(&self.meta);
        }
        let rec = self.meta.record(self.level.key);
        self.over_at = self.t;
        host.bridge.summary = Some(Summary {
            secs,
            level: self.core.player.level,
            kills: self.core.kills,
            best: rec.best_secs,
            new_best: self.new_best,
            won: self.core.won,
            bits: res.bits,
            rank_before: res.rank_before,
            rank_after: res.rank_after,
            level_name: self.level.name,
            tier_name: swarm_meta::tier(self.tier_i).name,
            chests: self.core.chests_opened,
            elites: self.core.elites,
            bosses: self.core.bosses + self.core.final_spawned as u32,
            unlocked_tier: self.core.won && self.tier_i + 1 < swarm_meta::TIERS && self.meta.wins_by_tier[self.tier_i] == 1,
        });
        host.bridge.summary_ready = false;
        host.set_player_visible(true);
    }

    /// Ground height under an arena-local point.
    fn ground(&self, p: [f32; 2]) -> f32 {
        self.terrain.as_ref().map_or(0.0, |t| t.height(p))
    }

    /// World position (SM64 units, Bevy-mapped) of an arena-plane point at `height` above the ground.
    fn at(&self, p: [f32; 2], height: f32) -> Vec3 {
        to_bevy([self.center[0] + p[0] as f64, (self.ground(p) + height) as f64, self.center[2] + p[1] as f64])
    }

    /// A chase camera above the field that ignores the walls and the swarm; the follow rig still
    /// supplies the zoom and the yaw the player drags (its automatic turn-behind-you is off, see
    /// `enter`), and the look-at point is eased by [`Follow`].
    fn camera(&mut self, host: &mut Host, dt: f64) {
        let target = DVec3::from(host.render_pos()) + DVec3::new(0.0, 92.0, 0.0);
        let focus = self.follow.update(target, dt);
        let (yaw, pitch, dist) = (host.rig.yaw, 0.85f64, 2000.0f64);
        let eye = focus + DVec3::new(yaw.sin() * pitch.cos() * dist, pitch.sin() * dist, yaw.cos() * pitch.cos() * dist);
        let mut pose = CamPose { eye, look: focus, up: DVec3::Y, fov: 55.0, near: 20.0, far: 40000.0 };
        self.shake.update(dt as f32);
        self.shake.apply(&mut pose, 26.0);
        host.cam.pose = Some(pose);
    }

    fn publish(&mut self, host: &mut Host) {
        let a = host.alpha.clamp(0.0, 1.0) as f32;
        let t = self.t as f32;
        let me = self.core.player.pos;
        let lerp = |p: [f32; 2], q: [f32; 2]| [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a];
        let core = &self.core;
        let b = &mut *host.bridge;
        b.time = t;
        b.foes.clear();
        let mut beacons = 0usize;
        for it in b.beacons.iter_mut() {
            it.kind = 0;
        }
        for (i, e) in core.enemies.iter().enumerate() {
            let item = &mut b.enemies[i];
            if !e.alive {
                item.kind = 0;
                continue;
            }
            let (scale, lift) = look(e.kind);
            let elite_k = if e.elite { 1.4 } else { 1.0 };
            let p = lerp(e.prev, e.pos);
            if e.elite && beacons < BEACON_SLOTS {
                b.beacons[beacons] = RItem { kind: 1, pos: self.at(p, 0.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
                beacons += 1;
            }
            if let Some(sp) = e.kind.species() {
                if e.vel[0].abs() + e.vel[1].abs() > 0.5 {
                    self.yaws[i] = e.vel[0].atan2(e.vel[1]);
                } else {
                    self.yaws[i] = (me[0] - p[0]).atan2(me[1] - p[1]);
                }
                let shake = if e.winding_up() { (t * 70.0).sin() * 14.0 } else { 0.0 };
                let flies = if sp.flies() { 130.0 + (t * 3.0 + i as f32).sin() * 14.0 } else { 0.0 };
                let hop = e.hop_height();
                let pulse = (1.0 + 0.12 * e.flash as f32 / 4.0) * elite_k;
                b.foes.push(FoeItem { id: i as u32, species: sp, pos: self.at([p[0] + shake, p[1]], flies + hop), yaw: self.yaws[i], radius: e.radius * 0.95, pose: e.pose(), pulse, age: 0.0 });
                item.kind = 0;
                continue;
            }
            if e.vel[0].abs() + e.vel[1].abs() > 0.5 {
                self.yaws[i] = e.vel[0].atan2(e.vel[1]);
            } else {
                self.yaws[i] = (me[0] - p[0]).atan2(me[1] - p[1]);
            }
            let hop = if matches!(e.kind, EnemyKind::Skitter | EnemyKind::Dasher | EnemyKind::Bomber) { (t * 11.0 + i as f32).sin().abs() * 14.0 } else { 0.0 };
            let bob = if e.kind == EnemyKind::Wisp { (t * 2.5 + i as f32).sin() * 16.0 } else { 0.0 };
            let shake = if e.winding_up() { (t * 70.0).sin() * 14.0 } else { 0.0 };
            // A lit bomber swells and throbs until it blows.
            let fuse = 1.0 + e.fuse() * 0.35 * (1.0 + (t * 40.0).sin() * 0.4);
            item.kind = e.kind as u8;
            item.pos = self.at([p[0] + shake, p[1]], lift + hop + bob);
            item.yaw = self.yaws[i];
            item.scale = scale * elite_k * fuse * (1.0 + 0.12 * e.flash as f32 / 4.0);
            item.aux = if e.elite { 1.0 } else { 0.0 };
        }
        for (i, bolt) in core.bolts.iter().enumerate() {
            let item = &mut b.bolts[i];
            if bolt.alive {
                let p = lerp(bolt.prev, bolt.pos);
                *item = RItem { kind: 1, pos: self.at(p, 85.0), yaw: 0.0, scale: bolt.radius / 42.0, aux: if bolt.crit { 1.0 } else { 0.0 } };
            } else {
                item.kind = 0;
            }
        }
        for (i, bm) in core.booms.iter().enumerate() {
            let item = &mut b.booms[i];
            if bm.alive {
                let p = lerp(bm.prev, bm.pos);
                *item = RItem { kind: 1, pos: self.at(p, 90.0), yaw: t * 18.0 + i as f32, scale: bm.radius / 60.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        for (i, o) in core.orbs.iter().enumerate() {
            let item = &mut b.orbs[i];
            if o.alive {
                *item = RItem { kind: 1, pos: self.at(o.pos, 75.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        for (i, g) in core.gems.iter().enumerate() {
            let item = &mut b.gems[i];
            let d2 = (g.pos[0] - me[0]).powi(2) + (g.pos[1] - me[1]).powi(2);
            if g.alive && d2 < DRAW_RANGE * DRAW_RANGE {
                let look = g.look();
                let scale = match look {
                    3 => 1.0,
                    4 => 1.5,
                    5 | 6 => 1.2,
                    x => 0.8 + 0.3 * x as f32,
                };
                *item = RItem { kind: 1, pos: self.at(g.pos, 45.0 + (t * 3.0 + i as f32).sin() * 8.0), yaw: 0.0, scale, aux: look as f32 };
            } else {
                item.kind = 0;
            }
            // A chest is lit by a beacon, however far away.
            if g.alive && look_is_chest(g) && beacons < BEACON_SLOTS {
                b.beacons[beacons] = RItem { kind: 1, pos: self.at(g.pos, 0.0), yaw: 0.0, scale: 1.0, aux: 1.0 };
                beacons += 1;
            }
        }
        for (i, r) in core.rings.iter().enumerate() {
            let item = &mut b.rings[i];
            if r.alive {
                let kind = match r.kind {
                    RingKind::Nova => 1,
                    RingKind::Quake => 2,
                    RingKind::Revive => 3,
                    RingKind::Strike => 4,
                    RingKind::Slash => 5,
                    RingKind::Blast => 6,
                };
                *item = RItem { kind, pos: self.at(r.center, 10.0), yaw: 0.0, scale: (r.radius() * crate::render::SCALE).max(0.01), aux: r.fade() };
            } else {
                item.kind = 0;
            }
        }
        for i in 0..ORBIT_MAX {
            let item = &mut b.orbit[i];
            if i < core.orbit_count {
                *item = RItem { kind: 1, pos: self.at(core.orbit_pos[i], 95.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        // The storm ring: pillars standing on the ground, closer together as it closes.
        for (i, item) in b.posts.iter_mut().enumerate() {
            let ang = i as f32 / RING_POSTS as f32 * std::f32::consts::TAU;
            let p = [ang.cos() * core.ring_r, ang.sin() * core.ring_r];
            let d2 = (p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2);
            if core.storm && d2 < 6500.0 * 6500.0 {
                *item = RItem { kind: 1, pos: self.at(p, 0.0), yaw: 0.0, scale: core.ring_r / self.level.radius, aux: (t * 2.0 + i as f32 * 0.3).sin() * 0.5 + 0.5 };
            } else {
                item.kind = 0;
            }
        }
        b.hud.hp = core.player.hp;
        b.hud.max_hp = core.player.max_hp;
        b.hud.xp = core.player.xp;
        b.hud.xp_need = xp_to_next(core.player.level);
        b.hud.level = core.player.level;
        b.hud.secs = core.secs();
        b.hud.kills = core.kills;
        b.hud.hint = core.secs() < 9.0 && core.choices.is_none();
        b.hud.storm = core.ev.storm || (core.storm && (core.player.pos[0].powi(2) + core.player.pos[1].powi(2)) > core.ring_r * core.ring_r);
        b.hud.boss = core.boss_bar.map(|(k, f)| (k as u8, f));
        b.hud.chests = core.pending_chests;
        // The build list follows the upgrades.
        if self.upgrades_seen != core.up.levels {
            self.upgrades_seen = core.up.levels;
            b.build.clear();
            for u in ALL_UPGRADES {
                let lv = core.up.level(u);
                if lv > 0 {
                    b.build.push((u.info().name, lv, u.is_evolution()));
                }
            }
            self.build_gen = self.build_gen.wrapping_add(1);
            b.build_gen = self.build_gen;
        }
        // Cards follow the core's offer.
        let want = core.choices;
        let shown = !b.cards.is_empty();
        if want.is_some() != shown || (want.is_some() && self.card_gen_shown == 0) {
            b.cards.clear();
            b.cards_chest = false;
            if let Some(c) = want {
                for u in c.as_slice() {
                    let info = u.info();
                    let lv = core.up.level(*u);
                    b.cards.push(CardView { name: info.name, desc: info.desc, from: lv, to: lv + 1, rare: info.rare, evo: u.is_evolution() });
                }
                b.cards_chest = c.chest;
                b.focus = 0;
                self.cards_at = self.t;
                self.card_gen_shown = 1;
            } else {
                self.card_gen_shown = 0;
            }
            b.card_gen = b.card_gen.wrapping_add(1);
        }
        b.cards_ready = want.is_some() && self.t - self.cards_at >= CARD_LOCKOUT;
        b.summary_ready = b.summary.is_some() && self.t - self.over_at >= SUMMARY_LOCKOUT;
    }
}

fn look_is_chest(g: &Gem) -> bool {
    g.kind == GemKind::Chest
}

impl Experiment for StarSwarmExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Swarm
    }
    /// The picked battlefield's course, kept visible: the swarm fights on its ground.
    fn course(&self) -> Option<&'static str> {
        Some(level().course)
    }
    /// The game stands still while you choose an upgrade or read the briefing, and slows for a beat on the big moments.
    fn time_scale(&self) -> f64 {
        if self.core.choices.is_some() || self.briefing {
            0.0
        } else if self.t < self.hitstop_until {
            0.12
        } else {
            1.0
        }
    }

    fn enter(&mut self, host: &mut Host) {
        host.shell.plain_camera = true;
        // A top-down camera must not swing round to follow the player's heading.
        host.rig.auto_follow_yaw = false;
        self.level = level();
        self.tier_i = tier_index();
        let world = host.course_world();
        let terrain = Terrain::sample(&world, self.level.center, self.level.radius, 9000.0, self.level.max_rise);
        self.center = [self.level.center[0], 0.0, self.level.center[1]];
        info!("swarm arena {}: base {:.0}, ground at centre {:.0}, walkable {:.0}%", self.level.key, terrain.base, terrain.height([0.0, 0.0]), terrain.mask().fraction() * 100.0);
        self.terrain = Some(terrain);
        host.rig.distance = 1300.0;
        host.rig.pitch = 0.72;
        self.meta = load_meta();
        // A tier the save has not unlocked falls back to the highest one it has.
        while self.tier_i > 0 && !self.meta.tier_unlocked(self.tier_i) {
            self.tier_i -= 1;
        }
        self.briefing = std::env::var_os("JUMPMAN_SWARM_SKIPBRIEF").is_none();
        self.start_run(host);
        if self.briefing {
            host.bridge.briefing = Some(self.briefing(host));
        }
    }

    fn exit(&mut self, host: &mut Host) {
        host.bridge.active = false;
        host.bridge.summary = None;
        host.bridge.briefing = None;
        host.bridge.cards.clear();
        host.rig.auto_follow_yaw = true;
        host.sim.mario.reset_to_spawn();
        host.set_player_visible(true);
        host.cam.pose = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if self.briefing {
            match key {
                KeyCode::ArrowLeft | KeyCode::KeyA => host.bridge.brief_action = Some(BriefAction::Level(-1)),
                KeyCode::ArrowRight | KeyCode::KeyD => host.bridge.brief_action = Some(BriefAction::Level(1)),
                KeyCode::ArrowUp | KeyCode::KeyW => host.bridge.brief_action = Some(BriefAction::Tier(1)),
                KeyCode::ArrowDown | KeyCode::KeyS => host.bridge.brief_action = Some(BriefAction::Tier(-1)),
                KeyCode::Enter | KeyCode::Space => host.bridge.brief_action = Some(BriefAction::Start),
                _ => return false,
            }
            return true;
        }
        if self.core.over {
            if matches!(key, KeyCode::Space | KeyCode::Enter | KeyCode::KeyR) && host.bridge.summary_ready {
                host.bridge.retry = true;
                return true;
            }
            return false;
        }
        let slot = match key {
            KeyCode::Digit1 => 0,
            KeyCode::Digit2 => 1,
            KeyCode::Digit3 => 2,
            _ => return false,
        };
        if self.core.choices.is_some() {
            host.bridge.pick = Some(slot);
            return true;
        }
        false
    }

    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        if self.core.over {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        if self.core.paused() || self.briefing {
            return;
        }
        if std::env::var_os("JUMPMAN_SWARM_GOD").is_some() {
            self.core.player.hp = self.core.player.max_hp;
        }
        let stats = self.core.stats;
        let mario = &mut host.sim.mario;
        let air = mario.action & ACT_FLAG_AIR != 0;
        // Spring Legs: a bigger launch on takeoff.
        if air && !self.was_air && mario.vel[1] > 25.0 && stats.jump_bonus > 0.0 {
            mario.vel[1] *= 1.0 + stats.jump_bonus as f64;
        }
        self.was_air = air;
        let pound_land = mario.action == ACT_GROUND_POUND_LAND && self.last_action != ACT_GROUND_POUND_LAND;
        let rush = matches!(mario.action, ACT_DIVE | ACT_DIVE_SLIDE | ACT_SLIDE_KICK | ACT_SLIDE_KICK_SLIDE | ACT_JUMP_KICK | ACT_LONG_JUMP);
        self.last_action = mario.action;
        // Swift Boots: extra distance on top of whatever the physics produced this step.
        if stats.move_bonus > 0.0 {
            let (vx, vz) = if air {
                (mario.vel[0], mario.vel[2])
            } else {
                let yaw = angle_to_radians(mario.face_angle[1]);
                (yaw.sin() * mario.forward_vel.max(0.0), yaw.cos() * mario.forward_vel.max(0.0))
            };
            let k = stats.move_bonus as f64;
            host.shift_mario([vx * k, 0.0, vz * k]);
        }
        // Course creatures cannot hurt the player (the swarm does).
        host.sim.mario.health = 0x880 as f64;
        host.sim.mario.hurt_counter = 0.0;
        let m = &host.sim.mario;
        let local = [(m.pos[0] - self.center[0]) as f32, (m.pos[2] - self.center[2]) as f32];
        let on_ground = m.action & ACT_FLAG_AIR == 0;
        // Fell off the world (a pit, the sea)? Back to the last safe ground, with a scratch.
        let floor = self.terrain.as_ref().map_or(0.0, |t| t.base()) as f64;
        if m.pos[1] < floor - 1400.0 || m.pos[1] < -9000.0 {
            let safe = self.safe;
            host.place_mario(safe, None);
            host.toast("BACK ON YOUR FEET!", 1.0);
            self.core.hurt_player(8.0);
            self.shake.add(0.5);
        } else if on_ground && local[0] * local[0] + local[1] * local[1] < self.core.ring_r * self.core.ring_r && self.core.mask.as_ref().is_none_or(|k| k.at(local)) {
            self.safe = [m.pos[0], m.pos[1], m.pos[2]];
        }
        let m = &host.sim.mario;
        let local = [(m.pos[0] - self.center[0]) as f32, (m.pos[2] - self.center[2]) as f32];
        let frame = PlayerFrame { pos: local, height: (m.pos[1] as f32 - self.ground(local)).max(0.0), pound_land, rush };
        let mario_pos = m.pos;
        self.core.tick(&frame);
        for n in 0..self.core.nums.len() {
            let h = self.core.nums[n];
            let item = NumItem { pos: self.at(h.pos, 150.0), amount: h.amount, crit: h.crit, kind: h.kind };
            if host.bridge.nums.len() < 96 {
                host.bridge.nums.push(item);
            }
        }
        let ev = self.core.ev;
        let tick = self.core.tick;
        self.feel(host, mario_pos);
        let fx_at = |s: &Self, p: [f32; 2]| [s.center[0] + p[0] as f64, s.ground(p) as f64 + 60.0, s.center[2] + p[1] as f64];
        if ev.kills > 0 && tick >= self.sfx_tick + 3 {
            self.sfx_tick = tick;
            host.fx("break_box", fx_at(self, ev.last_kill));
        }
        if ev.gems > 0 && tick >= self.sfx_tick + 2 {
            host.fx("coin", mario_pos);
        }
        if ev.hit {
            host.fx("bonk", mario_pos);
        }
        if ev.nova || ev.slash {
            host.fx("teleport", mario_pos);
        }
        if ev.quake || ev.blast || ev.bomb {
            host.fx("bonk", mario_pos);
        }
        if ev.thunder {
            host.fx("sparks", fx_at(self, ev.strike_pos[0]));
        }
        if ev.level_up || ev.chest_opened || ev.vacuum {
            host.fx("star_sound", mario_pos);
        }
        if ev.surge {
            host.toast("THE SWARM CLOSES IN!", 1.8);
        }
        if ev.stampede {
            host.toast("STAMPEDE!", 1.8);
        }
        if ev.elite_spawned && !ev.boss_spawned {
            host.toast("AN ELITE APPROACHES - IT GUARDS A CHEST!", 2.2);
        }
        if ev.final_spawned {
            host.toast("THE STAR TYRANT ARRIVES!", 3.0);
        } else if ev.boss_spawned {
            host.toast(if self.core.bosses % 2 == 1 { "A KING APPROACHES!" } else { "A CHOMP BREAKS LOOSE!" }, 2.2);
        }
        if ev.boss_killed && !self.core.won {
            host.toast("BOSS DEFEATED!", 2.0);
        }
        if ev.chest_opened {
            host.toast("CHEST! PICK A REWARD", 1.6);
        }
        if ev.vacuum {
            host.toast("MAGNET!", 1.2);
        }
        if ev.revived {
            host.toast("SECOND CHANCE!", 2.0);
        }
        if ev.died || ev.won {
            self.finish(host);
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.t += dt;
        // The briefing: pick the battlefield and the difficulty, then go.
        if let Some(action) = host.bridge.brief_action.take() {
            if self.briefing {
                self.brief(host, action);
            }
        }
        if host.bridge.change_setup {
            host.bridge.change_setup = false;
            if self.core.over {
                self.briefing = true;
                host.bridge.summary = None;
                host.bridge.briefing = Some(self.briefing(host));
            }
        }
        if host.bridge.retry {
            host.bridge.retry = false;
            if self.core.over {
                self.start_run(host);
            }
        }
        if let Some(i) = host.bridge.pick.take() {
            if self.core.choices.is_some() && self.t - self.cards_at >= CARD_LOCKOUT && self.core.pick(i) {
                host.fx("star_sound", host.sim.mario.pos);
                // The next queued level-up (if any) shows a fresh set.
                self.card_gen_shown = 0;
            }
        }
        // Brief flicker while invulnerable.
        let blink = self.core.player.iframes > 0 && !self.core.over && (self.t * 18.0) as i64 % 2 == 0;
        host.set_player_visible(!blink);
        self.publish(host);
        self.camera(host, dt);
    }

    fn debug(&self) -> String {
        format!("t={} level={} hp={:.0} live={} kills={} gems={}", self.core.secs(), self.core.player.level, self.core.player.hp, self.core.alive_enemies, self.core.kills, self.core.gems.iter().filter(|g| g.alive).count())
    }
}

impl StarSwarmExperiment {
    fn brief(&mut self, host: &mut Host, action: BriefAction) {
        match action {
            BriefAction::Level(d) => {
                let i = (self.level_index() as i32 + d).rem_euclid(LEVELS.len() as i32) as usize;
                if LEVELS[i].key != self.level.key {
                    LEVEL_PICK.store(i, Ordering::Relaxed);
                    self.meta.last_level = LEVELS[i].key.to_string();
                    self.meta.last_tier = self.tier_i;
                    save_meta(&self.meta);
                    // The course changes under the fight: reload it fresh (the briefing comes back with it).
                    host.shell.reenter = true;
                    self.briefing = true;
                }
            }
            BriefAction::Tier(d) => {
                let want = (self.tier_i as i32 + d).clamp(0, swarm_meta::TIERS as i32 - 1) as usize;
                if self.meta.tier_unlocked(want) {
                    self.tier_i = want;
                    TIER_PICK.store(want, Ordering::Relaxed);
                } else {
                    host.toast("WIN ON THE TIER BELOW TO UNLOCK THIS ONE", 1.8);
                }
                host.bridge.briefing = Some(self.briefing(host));
            }
            BriefAction::Start => {
                self.briefing = false;
                host.bridge.briefing = None;
                self.meta.last_level = self.level.key.to_string();
                self.meta.last_tier = self.tier_i;
                if !cheating() {
                    save_meta(&self.meta);
                }
                self.start_run(host);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::course_data::CourseData;
    use crate::game::runner::GameSim;
    use crate::game::world::CourseRequest;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    /// Every battlefield with the game data present: its course loaded and its ground sampled.
    fn terrains() -> Vec<(&'static LevelDef, GameSim, Terrain)> {
        let mut out = vec![];
        for l in LEVELS.iter() {
            if !CourseData::path(&assets(), l.course, 1).is_file() {
                continue;
            }
            let Ok(sim) = GameSim::load(&assets(), CourseRequest::new(l.course)) else { continue };
            let t = Terrain::sample(&sim.mario.world, l.center, l.radius, 9000.0, l.max_rise);
            out.push((l, sim, t));
        }
        out
    }

    #[test]
    fn every_arena_sits_on_real_walkable_ground() {
        for (l, sim, t) in terrains() {
            let mask = t.mask();
            assert!(mask.fraction() > 0.4, "{}: only {:.0}% of the arena is walkable", l.key, mask.fraction() * 100.0);
            // The start point and its surroundings are open ground, and the sampled height is the real floor.
            for (x, z) in [(0.0f32, 0.0f32), (250.0, 0.0), (-250.0, 0.0), (0.0, 250.0), (0.0, -250.0)] {
                assert!(mask.at([x, z]), "{}: ({x},{z}) is blocked", l.key);
                let real = super::super::swarm_levels::ground_at(&sim.mario.world, l.center[0] + x as f64, l.center[1] + z as f64, 9000.0).expect("floor");
                assert!((t.height([x, z]) - real).abs() < 60.0, "{}: ground {} vs floor {real}", l.key, t.height([x, z]));
            }
        }
    }

    #[test]
    fn enemies_spawn_and_walk_only_on_walkable_ground_of_the_course() {
        for (l, _, t) in terrains() {
            for seed in 1..=2u64 {
                let mut s = Swarm::with_cap(seed, 160);
                s.half = l.radius;
                s.mask = Some(t.mask());
                let mut worst = 0usize;
                for tick in 0..(150 * TICKS_PER_SEC) {
                    // The player strolls a small circle in the open middle.
                    let a = tick as f32 * 0.01;
                    s.player.hp = s.player.max_hp;
                    s.tick(&PlayerFrame { pos: [a.cos() * 500.0, a.sin() * 500.0], ..Default::default() });
                    while s.choices.is_some() {
                        s.pick(0);
                    }
                    for e in s.enemies.iter().filter(|e| e.alive) {
                        // Pushing apart may nudge a foe a hair over a cell edge; never into the next cell over.
                        let near = [[0.0, 0.0], [40.0, 0.0], [-40.0, 0.0], [0.0, 40.0], [0.0, -40.0]].iter().any(|d| s.walkable([e.pos[0] + d[0], e.pos[1] + d[1]]));
                        assert!(near, "{} seed {seed} tick {tick}: {:?} at {:?} is off the ground", l.key, e.kind, e.pos);
                    }
                    worst = worst.max(s.alive_enemies);
                    if s.over {
                        break;
                    }
                }
                assert!(worst > 20, "{}: the swarm should fill the arena (peak {worst})", l.key);
            }
        }
    }

    /// The camera focus filter: Mario runs a straight line at a steady speed; frames arrive unevenly
    /// (a 120 Hz display with hitches) and his height bobs. The eased focus must move at an even speed.
    #[test]
    fn camera_follow_is_smooth_through_frame_jitter_and_bobbing() {
        let mut f = Follow::new();
        let mut t = 0.0f64;
        let mut prev: Option<DVec3> = None;
        let mut speeds = vec![];
        let mut raw_speeds = vec![];
        let mut prev_raw: Option<DVec3> = None;
        for n in 0..1200 {
            let dt = if n % 7 == 0 { 1.0 / 40.0 } else { 1.0 / 120.0 };
            t += dt;
            let bob = if n % 3 == 0 { 9.0 } else { 0.0 };
            let target = DVec3::new(960.0 * t, 92.0 + bob, 0.0);
            let out = f.update(target, dt);
            if n > 200 {
                if let Some(p) = prev {
                    speeds.push(p.distance(out) / dt);
                }
                if let Some(p) = prev_raw {
                    raw_speeds.push(p.distance(target) / dt);
                }
            }
            prev = Some(out);
            prev_raw = Some(target);
        }
        let cv = |v: &[f64]| {
            let m = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt() / m
        };
        assert!((cv(&speeds)) < 0.08, "filtered cv {}", cv(&speeds));
        assert!(cv(&speeds) < cv(&raw_speeds) * 0.5, "filter should calm the bob: {} vs {}", cv(&speeds), cv(&raw_speeds));
    }

    #[test]
    fn heroes_map_to_their_starting_weapons() {
        use crate::characters::Character as C;
        assert_eq!(hero_of(C::Mario), Hero::Mario);
        assert_eq!(hero_of(C::YoungLink), Hero::Link);
        assert_eq!(hero_of(C::Banjo), Hero::Banjo);
        for h in [Hero::Mario, Hero::Link, Hero::Banjo] {
            assert!(!hero_name(h).is_empty());
        }
    }
}
