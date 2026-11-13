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
//! build, `JUMPMAN_SWARM_GOD=1` keeps health full, `JUMPMAN_SWARM_AUTOPICK=1` takes the first card of every offer,
//! `JUMPMAN_SWARM_UPGRADES=sword:4,flame:3` (names or indices) preloads weapons and tomes. Hooked runs never touch the records.
use super::foes_view::FoeItem;
use super::swarm_core::*;
use super::swarm_levels::{LevelDef, LEVELS};
use super::swarm_meta::{self, Meta};
use super::swarm_chest_view::CompassPt;
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
        EnemyKind::Skitter => (1.8, 0.0),
        EnemyKind::Brute => (0.5, 0.0),
        EnemyKind::Dasher => (2.6, 0.0),
        EnemyKind::Wisp => (0.9, 50.0),
        EnemyKind::King => (2.4, 0.0),
        EnemyKind::Bomber => (1.5, 0.0),
        EnemyKind::Splitter => (1.1, 0.0),
        EnemyKind::Chomp => (1.4, 0.0),
        EnemyKind::Tyrant => (0.7, 0.0),
        // The ported species are drawn by `foes_view`.
        _ => (1.0, 0.0),
    }
}

/// One streamed square of ground: `CH x CH` cells of [`CELL`] units.
struct Chunk {
    /// Height of the player when it was sampled (floors far above that, roofs and ledges, are not sampled).
    ref_y: f32,
    /// (CH + 1)^2 corner heights, row-major over z then x; NaN where there is no walkable floor.
    heights: Vec<f32>,
}

const CH: i32 = CHUNK_CELLS;
/// Chunks within this of the player are kept loaded (spawns reach 2200, foes are recycled past 3000).
const LOAD_R: f32 = 3700.0;
/// A chunk whose sampling height differs from the player's by more than this is sampled again when he is near.
const STALE_DY: f32 = 450.0;

/// The course's ground around the player, streamed in chunks as he walks (no big grid for a big course): corner heights
/// for snapping everything to the terrain, and the walkable mask for the foes, filled into the core's [`WalkMask`].
pub struct Terrain {
    world: std::sync::Arc<CollisionWorld>,
    center: [f64; 2],
    chunks: std::collections::HashMap<(i32, i32), Chunk>,
    /// Ground height used where nothing is known (the player's last ground).
    fallback: f32,
}

impl Terrain {
    pub fn new(world: std::sync::Arc<CollisionWorld>, center: [f64; 2], fallback: f32) -> Self {
        Self { world, center, chunks: Default::default(), fallback }
    }

    pub fn center(&self) -> [f64; 2] {
        self.center
    }

    /// The live collision (the course's moving gates and solids join it after the first frames).
    pub fn set_world(&mut self, world: std::sync::Arc<CollisionWorld>) {
        self.world = world;
    }

    pub fn loaded(&self) -> usize {
        self.chunks.len()
    }

    /// Sample one chunk from `ref_y + 700` down; returns it and its walkable cells.
    fn build(&self, key: (i32, i32), ref_y: f32) -> (Chunk, Box<[bool]>) {
        let n = (CH + 1) as usize;
        let mut heights = Vec::with_capacity(n * n);
        let top = (ref_y + 700.0) as f64;
        for iz in 0..=CH {
            for ix in 0..=CH {
                let (x, z) = ((key.0 * CH + ix) as f64 * CELL as f64, (key.1 * CH + iz) as f64 * CELL as f64);
                heights.push(super::swarm_levels::ground_at(&self.world, self.center[0] + x, self.center[1] + z, top).unwrap_or(f32::NAN));
            }
        }
        let mut bits = vec![false; (CH * CH) as usize];
        for iz in 0..CH as usize {
            for ix in 0..CH as usize {
                let hs = [heights[iz * n + ix], heights[iz * n + ix + 1], heights[(iz + 1) * n + ix], heights[(iz + 1) * n + ix + 1]];
                if hs.iter().any(|h| h.is_nan()) {
                    continue;
                }
                let (lo, hi) = hs.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
                if hi - lo >= 110.0 {
                    continue;
                }
                // A wall (a fence, a gate, a house side) across the cell blocks it too.
                let mid = hs.iter().sum::<f32>() / 4.0;
                let (cx, cz) = (self.center[0] + ((key.0 * CH + ix as i32) as f32 + 0.5) as f64 * CELL as f64, self.center[1] + ((key.1 * CH + iz as i32) as f32 + 0.5) as f64 * CELL as f64);
                let mut w = jumpman_sim::collision::WallData { x: cx, y: mid as f64, z: cz, offset_y: 60.0, radius: 55.0, walls: vec![] };
                bits[iz * CH as usize + ix] = self.world.find_wall_collisions(&mut w, false) == 0;
            }
        }
        (Chunk { ref_y, heights }, bits.into_boxed_slice())
    }

    /// Load (or refresh) up to `budget` chunks around the player, nearest first, into the mask. Returns how many were sampled.
    pub fn stream(&mut self, player: [f32; 2], player_y: f32, budget: usize, mask: &mut WalkMask) -> usize {
        self.fallback = player_y;
        let span = CH as f32 * CELL;
        let (pc, rc) = (WalkMask::chunk_of(player), (LOAD_R / span).ceil() as i32);
        let mut want: Vec<(f32, (i32, i32))> = vec![];
        for cz in pc.1 - rc..=pc.1 + rc {
            for cx in pc.0 - rc..=pc.0 + rc {
                let mid = [(cx as f32 + 0.5) * span, (cz as f32 + 0.5) * span];
                let d = ((mid[0] - player[0]).powi(2) + (mid[1] - player[1]).powi(2)).sqrt();
                if d > LOAD_R + span * 0.5 {
                    continue;
                }
                let stale = self.chunks.get(&(cx, cz)).is_some_and(|c| (c.ref_y - player_y).abs() > STALE_DY && d < 2600.0);
                if !self.chunks.contains_key(&(cx, cz)) || stale {
                    want.push((d, (cx, cz)));
                }
            }
        }
        want.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut done = 0;
        for (_, key) in want.into_iter().take(budget) {
            let (chunk, bits) = self.build(key, player_y);
            self.chunks.insert(key, chunk);
            mask.set_chunk(key, bits);
            done += 1;
        }
        done
    }

    /// Ground height at a local point (bilinear; unknown corners take their neighbours' mean, else the fallback).
    pub fn height(&self, p: [f32; 2]) -> f32 {
        let span = CH as f32 * CELL;
        let key = WalkMask::chunk_of(p);
        let Some(c) = self.chunks.get(&key) else { return self.fallback };
        let (fx, fz) = ((p[0] - key.0 as f32 * span) / CELL, (p[1] - key.1 as f32 * span) / CELL);
        let (ix, iz) = ((fx.floor() as usize).min(CH as usize - 1), (fz.floor() as usize).min(CH as usize - 1));
        let (tx, tz) = ((fx - ix as f32).clamp(0.0, 1.0), (fz - iz as f32).clamp(0.0, 1.0));
        let n = (CH + 1) as usize;
        let h = [c.heights[iz * n + ix], c.heights[iz * n + ix + 1], c.heights[(iz + 1) * n + ix], c.heights[(iz + 1) * n + ix + 1]];
        let valid: Vec<f32> = h.iter().copied().filter(|v| !v.is_nan()).collect();
        if valid.is_empty() {
            return self.fallback;
        }
        let mean = valid.iter().sum::<f32>() / valid.len() as f32;
        let g = |v: f32| if v.is_nan() { mean } else { v };
        let (a, b) = (g(h[0]) + (g(h[1]) - g(h[0])) * tx, g(h[2]) + (g(h[3]) - g(h[2])) * tx);
        a + (b - a) * tz
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
        Hero::Mario => "MARIO: STAR SHOT, DIVE + QUAKE",
        Hero::Link => "LINK: HERO SWORD",
        Hero::Banjo => "BANJO: BOOMERANG + STARS",
    }
}

/// Test hooks that skip the records.
fn cheating() -> bool {
    ["JUMPMAN_SWARM_START", "JUMPMAN_SWARM_XP", "JUMPMAN_SWARM_GOD", "JUMPMAN_SWARM_UPGRADES", "JUMPMAN_SWARM_BUILD", "JUMPMAN_SWARM_AUTOPICK", "JUMPMAN_SWARM_TP"].iter().any(|k| std::env::var_os(k).is_some())
}

/// The course's collision as the chest planner sees it.
pub struct CourseProbe<'a>(pub &'a CollisionWorld);

impl super::swarm_world::Probe for CourseProbe<'_> {
    fn floors(&self, x: f32, z: f32, out: &mut [f32; super::swarm_world::LAYERS]) -> usize {
        let (x, z) = (x as f64, z as f64);
        let mut top = 9000.0;
        let mut n = 0;
        for _ in 0..8 {
            let hit = self.0.find_floor(x, top, z);
            let Some(s) = hit.surface else { break };
            if hit.height < -9000.0 {
                break;
            }
            if super::swarm_levels::stand_ok(s.kind, s.normal[1]) && self.0.water_level(x, z) <= hit.height + 20.0 {
                out[n] = hit.height as f32;
                n += 1;
                if n == out.len() {
                    break;
                }
                top = hit.height - super::swarm_world::HEADROOM as f64 - 80.0;
            } else {
                top = hit.height - 90.0;
            }
        }
        n
    }
    fn wall_near(&self, x: f32, y: f32, z: f32, r: f32) -> bool {
        let mut w = jumpman_sim::collision::WallData { x: x as f64, y: y as f64, z: z as f64, offset_y: 0.0, radius: r as f64, walls: vec![] };
        self.0.find_wall_collisions(&mut w, false) > 0
    }
}

/// `[x0, z0, x1, z1]` around every floor a person could stand on in the course (clamped to the course's own boundary).
pub fn walkable_bounds(world: &CollisionWorld) -> [f32; 4] {
    let (mut x0, mut z0, mut x1, mut z1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for s in world.surfaces() {
        if !super::swarm_levels::floor_ok(s.kind, s.normal[1]) {
            continue;
        }
        for v in [s.v1, s.v2, s.v3] {
            x0 = x0.min(v[0]);
            x1 = x1.max(v[0]);
            z0 = z0.min(v[2]);
            z1 = z1.max(v[2]);
        }
    }
    let lim = world.horizontal_boundary.min(8192.0);
    if x0 > x1 {
        return [-lim as f32, -lim as f32, lim as f32, lim as f32];
    }
    [x0.max(-lim) as f32 - 50.0, z0.max(-lim) as f32 - 50.0, x1.min(lim) as f32 + 50.0, z1.min(lim) as f32 + 50.0]
}

/// The chest-walking test bot's state.
struct Bot {
    nav: Option<super::swarm_bot::Nav>,
    chest: usize,
    start_t: f64,
    reached: u32,
    failed: u32,
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
    /// The safe spot before `safe` (a fall returns here: `safe` may sit on a cliff edge).
    safe_prev: [f64; 3],
    safe_tick: u32,
    /// Camera distance (eased) and the last shown challenge countdown.
    cam_dist: f64,
    chests_planned: bool,
    plan_wait: u32,
    chest_log_done: bool,
    /// `JUMPMAN_SWARM_BOT=1`: a scripted player that walks to every chest over the planner's route (checks the real physics).
    bot: Option<Bot>,
    last_count: u32,
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
        Self { core: Swarm::new(seed), seed, runs: 0, t: 0.0, last_action: 0, was_air: false, meta: Meta::default(), level, tier_i: tier_index(), new_best: false, briefing: false, card_gen_shown: 0, cards_at: 0.0, over_at: 0.0, sfx_tick: 0, yaws: vec![0.0; ENEMY_SLOTS], first_run: true, center: [level.center[0], 0.0, level.center[1]], terrain: None, follow: Follow::new(), shake: Default::default(), hitstop_until: 0.0, safe: [0.0; 3], safe_prev: [0.0; 3], safe_tick: 0, cam_dist: 0.0, chests_planned: false, plan_wait: 0, chest_log_done: false, bot: None, last_count: 0, upgrades_seen: [0; UPGRADE_COUNT], build_gen: 0 }
    }

    fn start_run(&mut self, host: &mut Host) {
        self.runs += 1;
        let seed = self.seed.wrapping_add(self.runs as u64 * 7919);
        let hero = hero_of(host.bridge.character);
        let tier = swarm_meta::tier(self.tier_i);
        let perks = swarm_meta::perks(self.meta.rank());
        // Test hook: `JUMPMAN_SWARM_CAP=<n>` sets the live-foe cap (frame-time measurements at a fixed crowd size).
        let cap = std::env::var("JUMPMAN_SWARM_CAP").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or_else(enemy_cap);
        self.core = Swarm::start(seed, cap, hero, tier, perks);
        // Explore mode: the whole course, no arena edge or storm; the walk mask is streamed in as the player moves.
        self.core.explore = true;
        self.core.storm = false;
        self.core.half = 1.0e6;
        let mut mask = WalkMask::streamed();
        let start_y = self.ground_probe(host);
        if let Some(t) = &mut self.terrain {
            *t = Terrain::new(host.sim.mario.world.clone(), self.level.center, start_y);
            t.stream([0.0, 0.0], start_y, 64, &mut mask);
        }
        self.core.mask = Some(mask);
        self.chests_planned = false;
        self.follow.snap();
        self.cam_dist = 0.0;
        if let Some(secs) = std::env::var("JUMPMAN_SWARM_START").ok().and_then(|v| v.parse::<f32>().ok()) {
            self.core.tick = (secs * TICKS_PER_SEC as f32) as u32;
            self.core.bosses = ((secs / BOSS_PERIOD_SECS) as u32).min(3);
            self.core.elites = ((secs - ELITE_FIRST_SECS).max(0.0) / ELITE_PERIOD_SECS) as u32 + 1;
        }
        // Test hook: a mid-run build (the stars, a boomerang, lightning, a spin slash, armor and an evolution).
        if std::env::var_os("JUMPMAN_SWARM_BUILD").is_some() {
            for (u, n) in [(Upgrade::Damage, 4), (Upgrade::FireRate, 4), (Upgrade::Bolts, 3), (Upgrade::Area, 2), (Upgrade::Stars, 4), (Upgrade::Boomerang, 3), (Upgrade::Lightning, 3), (Upgrade::Orbit, 2), (Upgrade::Nova, 2), (Upgrade::Vitality, 4), (Upgrade::Armor, 2), (Upgrade::StarStorm, 1)] {
                for _ in 0..n {
                    self.core.up.take(u);
                }
            }
            self.core.refresh_stats();
            self.core.player.hp = self.core.player.max_hp;
            self.core.player.level = 12;
        }
        // Test hook: `JUMPMAN_SWARM_UPGRADES=12:3,13:2` or `sword:4,flame:3,damage:2` preloads upgrade levels by index
        // or by name (the `Upgrade` variant, any case). Weapons beyond the slots are refused like in a run.
        if let Ok(list) = std::env::var("JUMPMAN_SWARM_UPGRADES") {
            for (u, n) in list.split(',').filter_map(|p| p.split_once(':')).filter_map(|(a, b)| {
                let u = match a.parse::<usize>() {
                    Ok(i) => *ALL_UPGRADES.get(i)?,
                    Err(_) => *ALL_UPGRADES.iter().find(|u| format!("{u:?}").eq_ignore_ascii_case(a))?,
                };
                Some((u, b.parse::<u8>().ok()?))
            }) {
                for _ in 0..n {
                    self.core.up.take(u);
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
        let start_y = start_y as f64;
        host.place_mario([self.center[0], start_y, self.center[2]], Some(0.0));
        self.safe = [self.center[0], start_y, self.center[2]];
        self.safe_prev = self.safe;
        // The chests are planned a few steps later, once the course's own objects (gates, platforms) have put their collision in.
        self.plan_wait = 12;
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

    /// Ground height at the level's start (where the run begins).
    fn ground_probe(&self, host: &Host) -> f32 {
        super::swarm_levels::ground_at(&host.course_world(), self.level.center[0], self.level.center[1], 9000.0).unwrap_or(0.0)
    }

    /// Hide the level's chests (deterministic for the run's seed), from the course's real collision.
    fn plan_chests(&mut self, host: &Host, start_y: f32) {
        let world = host.sim.mario.world.clone();
        let t0 = std::time::Instant::now();
        let bounds = walkable_bounds(&world);
        let (cx, cz) = (self.level.center[0] as f32, self.level.center[1] as f32);
        let sites = super::swarm_world::plan(&CourseProbe(&world), [cx, start_y, cz], bounds, [cx, cz], self.core_seed());
        info!("swarm chests {}: {} planned in {:.0} ms over {:?}", self.level.key, sites.len(), t0.elapsed().as_secs_f64() * 1000.0, bounds);
        self.core.wchests = sites.into_iter().map(|site| WorldChest { site, opened: false, revealed: false }).collect();
        self.chests_planned = true;
    }

    /// Headless hooks for screenshots: `JUMPMAN_SWARM_TP=chest:<n>` (450 units before that chest, facing it; `chest:<n>:<dist>`
    /// changes the distance) or `JUMPMAN_SWARM_TP=<x>,<z>` (local offset from the start); `JUMPMAN_SWARM_CAM=<yaw>,<pitch>,<distance>`;
    /// `JUMPMAN_SWARM_CHESTS=1` prints the planned chests.
    fn test_hooks(&mut self, host: &mut Host) {
        if std::env::var_os("JUMPMAN_SWARM_CHESTS").is_some() {
            for (i, w) in self.core.wchests.iter().enumerate() {
                info!("swarm chest {i}: {:?} at local {:?} y {:.0} hide {:#06b}", w.site.tier, w.site.pos, w.site.y, w.site.hide);
            }
        }
        if let Ok(v) = std::env::var("JUMPMAN_SWARM_TP") {
            let mut at: Option<([f32; 2], f32, f64)> = None;
            if let Some(rest) = v.strip_prefix("chest:") {
                let mut it = rest.split(':');
                let i = it.next().and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
                let d = it.next().and_then(|n| n.parse::<f32>().ok()).unwrap_or(450.0);
                if let Some(w) = self.core.wchests.get(i) {
                    at = Some(([w.site.pos[0], w.site.pos[1] - d], w.site.y, 0.0));
                }
            } else if let Some((x, z)) = v.split_once(',') {
                if let (Ok(x), Ok(z)) = (x.parse::<f32>(), z.parse::<f32>()) {
                    at = Some(([x, z], f32::NAN, 0.0));
                }
            }
            if let Some((p, y, yaw)) = at {
                if let (Some(t), Some(m)) = (self.terrain.as_mut(), self.core.mask.as_mut()) {
                    t.stream(p, if y.is_nan() { self.safe[1] as f32 } else { y }, 200, m);
                }
                let y = if y.is_nan() { self.ground(p) } else { y.max(self.ground(p)) };
                host.place_mario([self.center[0] + p[0] as f64, y as f64, self.center[2] + p[1] as f64], Some(yaw));
                self.safe = [self.center[0] + p[0] as f64, y as f64, self.center[2] + p[1] as f64];
                self.safe_prev = self.safe;
                self.follow.snap();
            }
        }
        if let Ok(v) = std::env::var("JUMPMAN_SWARM_CAM") {
            let n: Vec<f64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            if n.len() == 3 {
                host.rig.yaw = n[0];
                host.rig.pitch = n[1];
                host.rig.distance = n[2];
            }
        }
    }

    /// One step of the chest-walking bot: follow the planner's route to the next unopened chest, jumping up ledges.
    fn bot_step(&mut self, host: &mut Host) -> StepOverride {
        let pos = host.sim.mario.pos;
        let air = host.sim.mario.action & ACT_FLAG_AIR != 0;
        let idle = StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        if self.core.challenge.is_some() || self.core.choices.is_some() {
            return idle;
        }
        let bot = self.bot.get_or_insert(Bot { nav: None, chest: usize::MAX, start_t: 0.0, reached: 0, failed: 0 });
        if bot.chest != usize::MAX && self.core.wchests.get(bot.chest).is_some_and(|w| w.opened) {
            bot.reached += 1;
            info!("swarm bot: chest {} opened after {:.0} s (reached {}, failed {})", bot.chest, self.t - bot.start_t, bot.reached, bot.failed);
            bot.chest = usize::MAX;
        }
        if bot.chest == usize::MAX {
            let Some(next) = self.core.wchests.iter().position(|w| !w.opened) else { return idle };
            bot.chest = next;
            bot.start_t = self.t;
            let w = self.core.wchests[next];
            let world = host.sim.mario.world.clone();
            let (cx, cz) = (self.center[0] as f32, self.center[2] as f32);
            let route = super::swarm_world::route(&CourseProbe(&world), [pos[0] as f32, pos[1] as f32, pos[2] as f32], walkable_bounds(&world), [w.site.pos[0] + cx, w.site.y, w.site.pos[1] + cz]);
            let bot = self.bot.as_mut().unwrap();
            match route {
                Some(r) => {
                    info!("swarm bot: walking to chest {next} ({:?}, hide {:#06b}) over {} nodes", w.site.tier, w.site.hide, r.len());
                    bot.nav = Some(super::swarm_bot::Nav::new(r));
                }
                None => {
                    info!("swarm bot: chest {next} has no route from here");
                    bot.failed += 1;
                    self.core.wchests[next].opened = true;
                    bot.chest = usize::MAX;
                }
            }
            return idle;
        }
        // Give up on a chest after 90 s.
        if self.t - bot.start_t > 90.0 {
            bot.failed += 1;
            info!("swarm bot: gave up on chest {} near {:?}", bot.chest, [pos[0] as i32, pos[1] as i32, pos[2] as i32]);
            let c = bot.chest;
            bot.chest = usize::MAX;
            self.core.wchests[c].opened = true;
            return idle;
        }
        let Some(nav) = bot.nav.as_mut() else { return idle };
        let (c, cam) = nav.step(pos, air);
        StepOverride { controller: Some(c), camera_yaw: Some(cam) }
    }

    fn core_seed(&self) -> u64 {
        self.seed.wrapping_add(self.runs as u64 * 7919)
    }

    /// A subtle hint for hidden chests near the player: a faint sparkle (and a soft chime up close), through walls.
    fn chest_hints(&mut self, host: &mut Host, me: [f64; 3]) {
        use crate::fx::particles::BurstOptions;
        let tick = self.core.tick;
        if tick % 24 != 0 {
            return;
        }
        let p = self.core.player.pos;
        for w in self.core.wchests.iter().filter(|w| !w.opened) {
            let d2 = (w.site.pos[0] - p[0]).powi(2) + (w.site.pos[1] - p[1]).powi(2);
            if d2 > HINT_R * HINT_R {
                continue;
            }
            let at = [self.center[0] + w.site.pos[0] as f64, w.site.y as f64 + 70.0, self.center[2] + w.site.pos[1] as f64];
            let color = match w.site.tier {
                ChestTier::Common => 0xffe9a8,
                ChestTier::Rare => 0xa8dcff,
                ChestTier::Golden => 0xffd24a,
            };
            host.particles.burst(at, BurstOptions { color, count: 2, speed: 40.0, life: 1.1, gravity: 20.0, size: 34.0, upward: 120.0, spread: 1.0, ..Default::default() });
            if d2 < 1100.0 * 1100.0 && tick % 96 == 0 {
                host.fx("coin_bounce", at);
            }
        }
        let _ = me;
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
        let mut res = self.meta.finish_run(self.level.key, self.tier_i, secs, self.core.kills, self.core.won, cheating());
        // Gold from chests is paid out as star bits on top of the run's own.
        if !cheating() && self.core.gold > 0 {
            let extra = self.core.gold / 12;
            self.meta.bits += extra;
            res.bits += extra;
            res.rank_after = self.meta.rank();
        }
        self.new_best = res.new_best;
        if !cheating() {
            self.meta.last_level = self.level.key.to_string();
            self.meta.last_tier = self.tier_i;
            save_meta(&self.meta);
        }
        let rec = self.meta.record(self.level.key);
        self.over_at = self.t;
        info!("swarm run over: hero {:?}, {}, {:.1} s, {} kills, level {}, course {}", self.core.hero, if self.core.won { "WON" } else { "died" }, secs, self.core.kills, self.core.player.level, self.level.key);
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

    /// A third-person orbit camera behind the hero: the shell's rig supplies the yaw, pitch and zoom the player drags
    /// (right stick, touch drag, mouse); the hero moves relative to that yaw. The distance grows slightly with the
    /// crowd, the course's walls and hills pull the camera in, and the look-at point (eased by [`Follow`]) sits above the
    /// hero so he rides in the lower part of the frame.
    fn camera(&mut self, host: &mut Host, dt: f64) {
        use super::swarm_cam as cam;
        let target = DVec3::from(host.render_pos()) + DVec3::new(0.0, 110.0, 0.0);
        let focus = self.follow.update(target, dt);
        let (yaw, pitch) = (host.rig.yaw, host.rig.pitch.clamp(cam::MIN_PITCH, cam::MAX_PITCH));
        let wanted = cam::distance_for(host.rig.distance, self.core.alive_enemies);
        let f = [focus.x, focus.y, focus.z];
        let want_eye = cam::eye_of(f, yaw, pitch, wanted);
        let world = host.sim.mario.world.clone();
        let clear = cam::clear_distance(&world, [target.x, target.y, target.z], cam::eye_of([target.x, target.y, target.z], yaw, pitch, wanted), cam::MIN_DISTANCE);
        let _ = want_eye;
        self.cam_dist = if self.cam_dist <= 0.0 { clear } else { cam::ease_distance(self.cam_dist, clear, dt) }.min(clear);
        let dist = self.cam_dist.max(cam::MIN_DISTANCE.min(wanted));
        let eye = cam::lift_above_ground(&world, cam::eye_of(f, yaw, pitch, dist), focus.y);
        // Look a little above the hero (more when the camera is close) so he rides low in the frame.
        let look = focus + DVec3::new(0.0, 60.0 + 0.1 * dist, 0.0);
        let mut pose = CamPose { eye: DVec3::from(eye), look, up: DVec3::Y, fov: 58.0, near: 12.0, far: 40000.0 };
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
                *item = RItem { kind: 1 + bolt.kind as u8, pos: self.at(p, 85.0), yaw: 0.0, scale: bolt.radius / 42.0, aux: if bolt.crit { 1.0 } else { 0.0 } };
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
            // A chest dropped by an elite is lit by a beacon once the player is near (not across the map).
            if g.alive && look_is_chest(g) && d2 < REVEAL_R * REVEAL_R && beacons < BEACON_SLOTS {
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
                    RingKind::Arc => 7,
                    RingKind::Frost => 8,
                };
                // An arc is a pie slice: its yaw turns the slice (pointing along -Z when flat) onto the swing direction.
                let yaw = if r.kind == RingKind::Arc { (-r.dir.cos()).atan2(-r.dir.sin()) } else { 0.0 };
                *item = RItem { kind, pos: self.at(r.center, 10.0), yaw, scale: (r.radius() * crate::render::SCALE).max(0.01), aux: r.fade() };
                if r.kind == RingKind::Arc {
                    item.pos.y += 2.0;
                }
            } else {
                item.kind = 0;
            }
        }
        // Ground zones ride on the end of the ring list: fire, poison clouds, black holes.
        for (i, z) in core.zones.iter().enumerate() {
            let item = &mut b.rings[RING_SLOTS + i];
            if z.alive {
                let kind = match z.kind {
                    ZoneKind::Fire => 10,
                    ZoneKind::Poison => 11,
                    ZoneKind::Gravity => 12,
                };
                *item = RItem { kind, pos: self.at(z.pos, 4.0 + i as f32 * 0.5), yaw: 0.0, scale: (z.radius * crate::render::SCALE).max(0.01), aux: z.fade() };
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
        // A bright ring under the player so he can be found in the crowd.
        b.marker = RItem { kind: 1, pos: self.at(me, 30.0), yaw: 0.0, scale: (105.0 + (t * 5.0).sin() * 8.0) * crate::render::SCALE, aux: 1.0 };
        // No storm ring any more: the pillars stay hidden.
        for item in b.posts.iter_mut() {
            item.kind = 0;
        }
        // The level's chests: those within sight range are drawn, the lit ones carry a beam; the compass lists the revealed ones.
        let (cs, sn) = ((host.rig.yaw as f32).cos(), (host.rig.yaw as f32).sin());
        b.world.items.clear();
        b.world.compass.clear();
        b.world.keys = core.keys;
        b.world.fight = core.challenge.map_or(0.0, |c| c.ticks_left as f32 / TICKS_PER_SEC as f32);
        b.world.total = core.wchests.len() as u32;
        b.world.opened = core.wchests.iter().filter(|w| w.opened).count() as u32;
        let mut pts: Vec<(f32, CompassPt)> = vec![];
        for (ci, w) in core.wchests.iter().enumerate() {
            let d = [w.site.pos[0] - me[0], w.site.pos[1] - me[1]];
            let d2 = d[0] * d[0] + d[1] * d[1];
            if d2 < DRAW_RANGE * DRAW_RANGE && b.world.items.len() < super::swarm_chest_view::DRAWN {
                let near = d2 < REVEAL_R * REVEAL_R;
                let pos = to_bevy([self.center[0] + w.site.pos[0] as f64, w.site.y as f64, self.center[2] + w.site.pos[1] as f64]);
                let busy = core.challenge.is_some_and(|c| c.chest == ci);
                b.world.items.push(super::swarm_chest_view::ChestItem { pos, tier: w.site.tier as u8, opened: w.opened, beam: near && !w.opened, busy });
            }
            if w.revealed && !w.opened {
                // The camera looks along (-sin yaw, -cos yaw); its right is (cos yaw, -sin yaw).
                let right = d[0] * cs - d[1] * sn;
                let ahead = -d[0] * sn - d[1] * cs;
                pts.push((d2, CompassPt { right, ahead, tier: w.site.tier as u8 }));
            }
        }
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        b.world.compass.extend(pts.into_iter().take(super::swarm_chest_view::DOTS).map(|p| p.1));
        b.hud.hp = core.player.hp;
        b.hud.max_hp = core.player.max_hp;
        b.hud.xp = core.player.xp;
        b.hud.xp_need = xp_to_next(core.player.level);
        b.hud.level = core.player.level;
        b.hud.secs = core.secs();
        b.hud.kills = core.kills;
        b.hud.hint = core.secs() < 9.0 && core.choices.is_none();
        b.hud.storm = false;
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
        self.center = [self.level.center[0], 0.0, self.level.center[1]];
        self.terrain = Some(Terrain::new(host.sim.mario.world.clone(), self.level.center, 0.0));
        host.rig.distance = super::swarm_cam::START_DISTANCE;
        host.rig.pitch = super::swarm_cam::START_PITCH;
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

    fn before_step(&mut self, host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        if self.core.over {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        if self.chests_planned && !self.briefing && std::env::var_os("JUMPMAN_SWARM_BOT").is_some() {
            return self.bot_step(host);
        }
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        // Test hook: `JUMPMAN_SWARM_AUTOPICK=1` takes the first card of every offer (headless bot runs).
        if self.core.choices.is_some() && std::env::var_os("JUMPMAN_SWARM_AUTOPICK").is_some() {
            while self.core.choices.is_some() {
                self.core.pick(0);
            }
        }
        if !self.chests_planned && !self.briefing {
            if self.plan_wait > 0 {
                self.plan_wait -= 1;
            } else {
                let y = self.safe[1] as f32;
                self.plan_chests(host, y);
                self.test_hooks(host);
            }
        }
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
        let ground_kind = m.floor.as_ref().map_or(0, |f| f.kind);
        let ground_ny = m.floor.as_ref().map_or(1.0, |f| f.normal[1]);
        let pos_now = m.pos;
        let floor_now = m.floor_height;
        // Fell off the world (a pit, the sea, a death plane)? Back to the last safe ground, with a scratch.
        let fell = pos_now[1] < self.safe[1] - 2600.0 || pos_now[1] < -9000.0 || (ground_kind == 0x0a && pos_now[1] < floor_now + 60.0);
        if fell {
            let safe = self.safe_prev;
            host.place_mario(safe, None);
            host.toast("BACK ON YOUR FEET!", 1.0);
            self.core.hurt_player(8.0);
            self.shake.add(0.5);
            self.follow.snap();
            self.cam_dist = 0.0;
        } else if on_ground && pos_now[1] - floor_now < 8.0 && super::swarm_levels::floor_ok(ground_kind, ground_ny) && self.core.mask.as_ref().is_some_and(|k| k.at(local) && k.at([local[0] + 160.0, local[1]]) && k.at([local[0] - 160.0, local[1]]) && k.at([local[0], local[1] + 160.0]) && k.at([local[0], local[1] - 160.0])) {
            // Safe ground: standing on a walkable floor with room around it. The spot before is kept too (a fall goes there).
            if self.safe_tick == 0 {
                self.safe_prev = self.safe;
                self.safe = pos_now;
                self.safe_tick = 45;
            }
        }
        self.safe_tick = self.safe_tick.saturating_sub(1);
        // Lava burns (the sim itself only plays the hop).
        if ground_kind == 0x01 && pos_now[1] - floor_now < 60.0 {
            if self.core.hurt_player(14.0 * self.core.tier.damage) {
                self.shake.add(0.4);
            }
        }
        // Stream the ground around the player (a couple of chunks a frame at most) into the foes' walk mask.
        if let (Some(t), Some(mask)) = (self.terrain.as_mut(), self.core.mask.as_mut()) {
            let m = &host.sim.mario;
            let local = [(m.pos[0] - self.center[0]) as f32, (m.pos[2] - self.center[2]) as f32];
            let prof = super::swarm_view::prof_on().then(std::time::Instant::now);
            t.set_world(host.sim.mario.world.clone());
            t.stream(local, m.floor_height as f32, 2, mask);
            if let Some(p) = prof {
                super::swarm_view::prof_add(0, p);
            }
        }
        let m = &host.sim.mario;
        let local = [(m.pos[0] - self.center[0]) as f32, (m.pos[2] - self.center[2]) as f32];
        let frame = PlayerFrame { pos: local, height: (m.pos[1] as f32 - self.ground(local)).max(0.0), y: m.floor_height as f32, pound_land, rush };
        let mario_pos = m.pos;
        let prof = super::swarm_view::prof_on().then(std::time::Instant::now);
        self.core.tick(&frame);
        if let Some(t) = prof {
            super::swarm_view::prof_add(0, t);
        }
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
        match ev.chest_tier {
            Some(ChestTier::Golden) => host.toast("GOLDEN CHEST! TWO REWARDS", 2.0),
            Some(ChestTier::Rare) => host.toast("RARE CHEST! PICK A REWARD", 1.8),
            Some(ChestTier::Common) => host.toast("CHEST! PICK A REWARD", 1.6),
            None if ev.chest_opened => host.toast("CHEST! PICK A REWARD", 1.6),
            None => {}
        }
        if ev.key_found {
            host.toast("A KEY! IT OPENS A GOLDEN CHEST", 1.8);
        }
        if ev.challenge_started {
            host.toast("GOLDEN CHEST: HOLD OUT FOR 20 SECONDS!", 2.4);
            self.last_count = 20;
        }
        if ev.challenge_won {
            host.toast("CHALLENGE COMPLETE!", 1.6);
        }
        if ev.challenge_lost {
            host.toast("YOU WENT TOO FAR - THE CHEST RESETS", 2.0);
        }
        if let Some(c) = self.core.challenge {
            let left = c.ticks_left / TICKS_PER_SEC;
            if left != self.last_count && matches!(left, 15 | 10 | 5 | 3) {
                self.last_count = left;
                host.toast(&format!("{left}..."), 0.9);
            }
        }
        self.chest_hints(host, mario_pos);
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
        let prof = super::swarm_view::prof_on().then(std::time::Instant::now);
        self.publish(host);
        self.camera(host, dt);
        if let Some(t) = prof {
            super::swarm_view::prof_add(1, t);
        }
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

    /// Every level with the game data present: its course loaded.
    fn courses() -> Vec<(&'static LevelDef, GameSim)> {
        let mut out = vec![];
        for l in LEVELS.iter() {
            if !CourseData::path(&assets(), l.course, 1).is_file() {
                continue;
            }
            let Ok(mut sim) = GameSim::load(&assets(), CourseRequest::new(l.course)) else { continue };
            // A few steps, as in the game: the course's objects put their solids into the collision.
            for _ in 0..15 {
                sim.tick(&jumpman_sim::state::ControllerSnapshot::default());
            }
            out.push((l, sim));
        }
        out
    }

    fn start_y(l: &LevelDef, sim: &GameSim) -> f32 {
        super::super::swarm_levels::ground_at(&sim.mario.world, l.center[0], l.center[1], 9000.0).expect("floor at the start")
    }

    #[test]
    fn every_level_streams_real_walkable_ground_around_the_start() {
        for (l, sim) in courses() {
            let y = start_y(l, &sim);
            let mut t = Terrain::new(sim.mario.world.clone(), l.center, y);
            let mut mask = WalkMask::streamed();
            let n = t.stream([0.0, 0.0], y, 1000, &mut mask);
            assert!(n > 10 && t.loaded() == n, "{}: {n} chunks", l.key);
            assert!(mask.fraction() > 0.2, "{}: only {:.0}% of the start's surroundings is walkable", l.key, mask.fraction() * 100.0);
            for (x, z) in [(0.0f32, 0.0f32), (250.0, 0.0), (-250.0, 0.0), (0.0, 250.0), (0.0, -250.0)] {
                assert!(mask.at([x, z]), "{}: ({x},{z}) is blocked", l.key);
                let real = super::super::swarm_levels::ground_at(&sim.mario.world, l.center[0] + x as f64, l.center[1] + z as f64, 9000.0).expect("floor");
                assert!((t.height([x, z]) - real).abs() < 60.0, "{}: ground {} vs floor {real}", l.key, t.height([x, z]));
            }
            // Nothing more to load until the player moves; then a few chunks arrive per call.
            assert_eq!(t.stream([0.0, 0.0], y, 1000, &mut mask), 0);
            assert!(t.stream([2500.0, 0.0], y, 3, &mut mask) <= 3);
        }
    }

    /// The player walks a long straight line across each real course (the mask streams in behind him); foes spawn only on
    /// walkable ground, stay near him, and the live count never passes the cap however far he goes.
    #[test]
    fn foes_follow_a_player_across_the_whole_course_and_stay_capped() {
        for (l, sim) in courses() {
            let y = start_y(l, &sim);
            let cap = 160;
            let mut t = Terrain::new(sim.mario.world.clone(), l.center, y);
            let mut s = Swarm::with_cap(3, cap);
            s.explore = true;
            s.storm = false;
            s.half = 1.0e6;
            let mut mask = WalkMask::streamed();
            t.stream([0.0, 0.0], y, 1000, &mut mask);
            s.mask = Some(mask);
            let (mut worst, mut peak_far) = (0usize, 0.0f32);
            let mut pos = [0.0f32, 0.0f32];
            let dir = [0.8f32, 0.6f32];
            for tick in 0..(180 * TICKS_PER_SEC) {
                // Walk 30 units a tick along a bent line (turn at random walkable obstacles: step only onto walkable cells).
                let next = [pos[0] + dir[0] * 30.0, pos[1] + dir[1] * 30.0];
                if s.mask.as_ref().unwrap().at(next) {
                    pos = next;
                } else {
                    let side = [pos[0] - dir[1] * 30.0, pos[1] + dir[0] * 30.0];
                    if s.mask.as_ref().unwrap().at(side) {
                        pos = side;
                    }
                }
                t.stream(pos, y, 2, s.mask.as_mut().unwrap());
                s.player.hp = s.player.max_hp;
                s.tick(&PlayerFrame { pos, y, ..Default::default() });
                while s.choices.is_some() {
                    s.pick(0);
                }
                for e in s.enemies.iter().filter(|e| e.alive) {
                    let near = [[0.0, 0.0], [40.0, 0.0], [-40.0, 0.0], [0.0, 40.0], [0.0, -40.0]].iter().any(|d| s.walkable([e.pos[0] + d[0], e.pos[1] + d[1]]));
                    assert!(near, "{} tick {tick}: {:?} at {:?} is off the ground", l.key, e.kind, e.pos);
                    let d = ((e.pos[0] - pos[0]).powi(2) + (e.pos[1] - pos[1]).powi(2)).sqrt();
                    peak_far = peak_far.max(d);
                }
                worst = worst.max(s.alive_enemies);
                assert!(s.alive_enemies <= cap + OVERCAP, "{}: {} live foes", l.key, s.alive_enemies);
                if s.over {
                    break;
                }
            }
            assert!(worst > 20, "{}: the swarm should follow the player (peak {worst})", l.key);
            assert!(peak_far < DESPAWN_R + 800.0, "{}: a foe was left {peak_far} behind", l.key);
        }
    }

    /// Independent check of the chest planner on the real courses: a finer flood fill over the course's own floors (walk steps,
    /// jump-ups) from the start must reach every chest; each chest stands on a real floor, clear of walls.
    #[test]
    fn every_chest_of_every_level_is_on_reachable_floor() {
        use super::super::swarm_world::{plan, ChestTier, Probe};
        for (l, sim) in courses() {
            let world = &*sim.mario.world;
            let y = start_y(l, &sim);
            let probe = CourseProbe(world);
            let bounds = walkable_bounds(world);
            let (cx, cz) = (l.center[0] as f32, l.center[1] as f32);
            let sites = plan(&probe, [cx, y, cz], bounds, [cx, cz], 11);
            assert!(sites.len() >= 10, "{}: only {} chests", l.key, sites.len());
            let n = |t: ChestTier| sites.iter().filter(|s| s.tier == t).count();
            assert!(n(ChestTier::Golden) >= 1 && n(ChestTier::Rare) >= 1 && n(ChestTier::Common) >= 1, "{}: tiers", l.key);
            let hidden = sites.iter().filter(|s| s.hide != 0).count();
            println!("{}: hidden {hidden}/{} (nook {}, ledge {}, cave {}, dead end {})", l.key, sites.len(), sites.iter().filter(|s| s.hide & 1 != 0).count(), sites.iter().filter(|s| s.hide & 2 != 0).count(), sites.iter().filter(|s| s.hide & 4 != 0).count(), sites.iter().filter(|s| s.hide & 8 != 0).count());
            assert!(hidden >= 4 && hidden * 5 >= sites.len(), "{}: hidden share", l.key);
            let again = plan(&probe, [cx, y, cz], bounds, [cx, cz], 11);
            assert_eq!(sites.len(), again.len());
            // A finer flood fill (60-unit cells) over every standable floor, jumps up to 210 and drops to 400.
            let cell = 60.0f32;
            let (nx, nz) = (((bounds[2] - bounds[0]) / cell) as usize + 1, ((bounds[3] - bounds[1]) / cell) as usize + 1);
            let key = |x: f32, z: f32| (((x - bounds[0]) / cell) as usize, ((z - bounds[1]) / cell) as usize);
            let mut seen = std::collections::HashSet::<(usize, usize, i32)>::new();
            let mut stack = vec![];
            let (sx, sz) = key(cx, cz);
            stack.push((sx, sz, y));
            seen.insert((sx, sz, (y / 40.0).round() as i32));
            let mut guard = 0;
            while let Some((ix, iz, h)) = stack.pop() {
                guard += 1;
                if guard > 3_000_000 {
                    break;
                }
                // Steps of up to two cells: a thin ridge or a stair that the planner's grid lands on is still walkable.
                for k in 0..24usize {
                    let all: [(i32, i32); 24] = [(-2, -2), (-1, -2), (0, -2), (1, -2), (2, -2), (-2, -1), (-1, -1), (0, -1), (1, -1), (2, -1), (-2, 0), (-1, 0), (1, 0), (2, 0), (-2, 1), (-1, 1), (0, 1), (1, 1), (2, 1), (-2, 2), (-1, 2), (0, 2), (1, 2), (2, 2)];
                    let (dx, dz) = all[k];
                    let (qx, qz) = ((ix as i32 + dx) as usize, (iz as i32 + dz) as usize);
                    if qx >= nx || qz >= nz {
                        continue;
                    }
                    let (x, z) = (bounds[0] + (qx as f32 + 0.5) * cell, bounds[1] + (qz as f32 + 0.5) * cell);
                    let mut f = [f32::NAN; super::super::swarm_world::LAYERS];
                    let cnt = super::super::swarm_world::Probe::floors(&probe, x, z, &mut f);
                    for &fh in f.iter().take(cnt) {
                        if fh - h > 230.0 || h - fh > 420.0 || seen.contains(&(qx, qz, (fh / 40.0).round() as i32)) {
                            continue;
                        }
                        if super::super::swarm_world::Probe::wall_near(&probe, x, fh + 60.0, z, 30.0) {
                            continue;
                        }
                        seen.insert((qx, qz, (fh / 40.0).round() as i32));
                        stack.push((qx, qz, fh));
                    }
                }
            }
            for s in &sites {
                let (x, z) = (s.pos[0] + cx, s.pos[1] + cz);
                let hit = world.find_floor(x as f64, s.y as f64 + 40.0, z as f64);
                assert!(hit.surface.is_some() && (hit.height as f32 - s.y).abs() < 30.0, "{}: {s:?} floats ({})", l.key, hit.height);
                assert!(!Probe::wall_near(&probe, x, s.y + 60.0, z, 40.0), "{}: {s:?} is inside a wall", l.key);
                let (qx, qz) = key(x, z);
                let found = (-1i32..=1).any(|dx| (-1i32..=1).any(|dz| (-2i32..=2).any(|dh| seen.contains(&((qx as i32 + dx) as usize, (qz as i32 + dz) as usize, (s.y / 40.0).round() as i32 + dh))))) ;
                assert!(found, "{}: {s:?} cannot be reached from the start", l.key);
            }
            println!("{}: {} chests (golden {}, rare {}, common {})", l.key, sites.len(), n(ChestTier::Golden), n(ChestTier::Rare), n(ChestTier::Common));
        }
    }

    /// Steps the real simulation: a scripted walker follows the planner's route to chests on every level (a spread of the tiers,
    /// ledges and caves included) and must stand on them. A failure here means the planner put a chest where a player cannot go,
    /// or the bot cannot follow; both are worth a look. `SWARM_BOT_LEVELS=bob,island` and `SWARM_BOT_CHESTS=<n>` narrow it.
    #[test]
    fn a_scripted_walker_reaches_chests_on_the_real_courses() {
        use super::super::swarm_bot::Nav;
        use super::super::swarm_world::{plan, route};
        let only = std::env::var("SWARM_BOT_LEVELS").unwrap_or_default();
        let per_level: usize = std::env::var("SWARM_BOT_CHESTS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
        let (mut tried, mut reached) = (0, 0);
        let mut fails = vec![];
        for (l, mut sim) in courses() {
            if !only.is_empty() && !only.split(',').any(|k| k == l.key) {
                continue;
            }
            let world = sim.mario.world.clone();
            let y = start_y(l, &sim);
            let probe = CourseProbe(&world);
            let bounds = walkable_bounds(&world);
            let (cx, cz) = (l.center[0] as f32, l.center[1] as f32);
            let sites = plan(&probe, [cx, y, cz], bounds, [0.0, 0.0], 11);
            if std::env::var_os("SWARM_BOT_KINDS").is_some() {
                let mut h = std::collections::BTreeMap::<i32, (usize, f64, f64)>::new();
                for sf in world.surfaces().iter().filter(|sf| sf.normal[1] > 0.7) {
                    let e = h.entry(sf.kind).or_insert((0, f64::MAX, f64::MIN));
                    e.0 += 1;
                    e.1 = e.1.min(sf.v1[1]);
                    e.2 = e.2.max(sf.v1[1]);
                }
                println!("{} floor kinds (kind: count, min y, max y): {h:?}", l.key);
            }
            // A spread of chests: every n-th, so tiers and hiding styles are all sampled.
            let step = (sites.len() / per_level).max(1);
            for (i, site) in sites.iter().enumerate().step_by(step).take(per_level) {
                // Back to the start for each chest (so a failure does not carry over).
                let m = &mut sim.mario;
                m.pos = [cx as f64, y as f64, cz as f64];
                m.vel = [0.0; 3];
                m.forward_vel = 0.0;
                m.set_action(ACT_FREEFALL, 0.0);
                let Some(r) = route(&probe, [cx, y, cz], bounds, [site.pos[0], site.y, site.pos[1]]) else {
                    fails.push(format!("{} chest {i}: no route", l.key));
                    tried += 1;
                    continue;
                };
                let mut nav = Nav::new(r);
                tried += 1;
                let mut ok = false;
                let mut best = f64::MAX;
                let mut nav_ticks = 0u32;
                for _ in 0..(150 * 30) {
                    let m = &sim.mario;
                    let air = m.action & ACT_FLAG_AIR != 0;
                    let (c, cam) = nav.step(m.pos, air);
                    sim.camera_yaw = cam;
                    sim.tick(&c);
                    if std::env::var_os("SWARM_BOT_TRACE").is_some() && nav_ticks % 45 == 0 {
                        let m = &sim.mario;
                        println!("  {} chest {i}: t{nav_ticks} pos {:?} act {:#x} floor {:?} fh {:.0} vel {:?} idx {}/{} tgt {:?} stick cam {cam}", l.key, [m.pos[0] as i32, m.pos[1] as i32, m.pos[2] as i32], m.action, m.floor.as_ref().map(|f| (f.kind, (f.normal[1] * 100.0) as i32)), m.floor_height, [m.vel[0] as i32, m.vel[1] as i32, m.vel[2] as i32], nav.idx, nav.route.len(), nav.route[(nav.idx + 1).min(nav.route.len() - 1)]);
                    }
                    nav_ticks += 1;
                    let m = &sim.mario;
                    let d = (m.pos[0] - site.pos[0] as f64).hypot(m.pos[2] - site.pos[1] as f64);
                    if d < 140.0 && (m.pos[1] - site.y as f64).abs() < 260.0 {
                        ok = true;
                        break;
                    }
                    if m.pos[1] < -8000.0 {
                        break;
                    }
                    best = best.min(d);
                }
                if ok {
                    reached += 1;
                } else {
                    let m = &sim.mario;
                    if std::env::var_os("SWARM_BOT_TRACE").is_some() {
                        for o in m.objects.objects.iter().filter(|o| o.active && (o.pos[0] - m.pos[0]).hypot(o.pos[2] - m.pos[2]) < 500.0) {
                            println!("    near object {:?} at {:?}", o.behavior, [o.pos[0] as i32, o.pos[1] as i32, o.pos[2] as i32]);
                        }
                    }
                    fails.push(format!("{} chest {i} ({:?} hide {:#06b} at {:?} y {:.0}): ended at {:?}, route {}/{}, closest {best:.0}, action {:#x}, next {:?}", l.key, site.tier, site.hide, site.pos, site.y, [m.pos[0] as i32, m.pos[1] as i32, m.pos[2] as i32], nav.idx, nav.route.len(), m.action, &nav.route[nav.idx..(nav.idx + 4).min(nav.route.len())]));
                }
            }
        }
        println!("scripted walker: {reached}/{tried} chests reached");
        for f in &fails {
            println!("  FAILED {f}");
        }
        assert!(tried == 0 || reached * 10 >= tried * 8, "the walker reached only {reached} of {tried} chests: {fails:#?}");
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
