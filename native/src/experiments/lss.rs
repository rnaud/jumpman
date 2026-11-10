//! Last Star Standing: an original last-one-standing match on Mario's real physics.
//!
//! You and a field of AI Marios glide down onto Bob-omb Battlefield, scavenge shells, bombs,
//! fire flowers, star bursts and caps from loot camps, chests and airdrops, and fight with the
//! moves Mario already has (punch, dive, ground pound, wall kick) while the Blight closes in from
//! the edge of the island. The last Mario standing wins.
//!
//! * The rules live in `lss_core.rs` (deterministic, seeded, unit-tested). This file glues them
//!   to the host: it measures the island, runs the player and the bots nearest to the action on
//!   the full SM64 physics (`CrowdMario`, the "A Hundred Marios" body) while the core moves the
//!   rest kinematically, applies knockbacks through `take_damage`, and publishes render lists
//!   and HUD numbers through [`LssBridge`].
//! * `lss_view.rs` draws pickups, shots, the Blight wall, the HUD, the minimap, the spectate bar
//!   and the end card.
//!
//! Test hooks (env): `JUMPMAN_LSS_SEED`, `_BOTS`, `_DIFF=easy|normal|hard`, `_START=<secs>`
//! (skip the drop and fast-forward the match clock), `_PHASE=<n>` (start in the shrink of phase
//! n), `_ALIVE=<n>`, `_GOD=1`, `_KIT=1` (full inventory), `_DIE=<secs>` (eliminate the player
//! that long after the start), `_FIGHT=1` (an armed rival a few steps away), `_LOOT=1` (start on
//! a loot camp). Runs using these hooks never write the saved records.
use super::crowd::{init_clips, spawn_body, CrowdLook, CrowdMario, DriveInput};
use super::lss_core::Event;
use super::lss_core::*;
use super::lss_view::*;
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::camera::rig::camera_ray;
use crate::game::sm64_animation::ClipMeta;
use crate::render::to_bevy;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
use jumpman_sim::math::{angle_to_radians, radians_to_angle};
use jumpman_sim::state::*;
use jumpman_sim::surfaces::SURFACE_DEATH_PLANE;
use std::sync::Arc;

#[cfg(any(target_os = "ios", target_os = "android"))]
const DEFAULT_BOTS: usize = 23;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const DEFAULT_BOTS: usize = 39;
/// Bots stepped on the full physics engine at once, and how near they must be.
#[cfg(any(target_os = "ios", target_os = "android"))]
const SIM_CAP: usize = 8;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const SIM_CAP: usize = 14;
const SIM_RADIUS: f32 = 3400.0;
/// Rivals beyond this distance from the camera are not drawn.
const DRAW_RANGE: f32 = 6500.0;
#[cfg(any(target_os = "ios", target_os = "android"))]
const MAX_DRAWN: usize = 14;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const MAX_DRAWN: usize = 26;
const PICKUP_RANGE: f32 = 4200.0;
/// The eliminated player is parked far above the island, out of everybody's way.
const PARK_HEIGHT: f64 = 40000.0;
const CARD_DELAY: f64 = 2.2;

/// Where a run's records live.
fn save_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("lss.json")
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Records {
    /// Best (lowest) placement, 0 = none yet.
    pub best_place: u32,
    pub wins: u32,
    pub difficulty: u8,
}

pub fn load_records() -> Records {
    read_records(&save_path())
}

fn read_records(path: &std::path::Path) -> Records {
    let v = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let get = |k: &str| v.as_ref().and_then(|v| v[k].as_u64()).unwrap_or(0) as u32;
    Records { best_place: get("best_place"), wins: get("wins"), difficulty: get("difficulty").min(2) as u8 }
}

fn save_records(r: Records) {
    write_records(&save_path(), r);
}

fn write_records(path: &std::path::Path, r: Records) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "best_place": r.best_place, "wins": r.wins, "difficulty": r.difficulty }).to_string());
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok().and_then(|s| s.parse().ok())
}

fn difficulty_of(i: u8) -> Difficulty {
    [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard][i.min(2) as usize]
}

// ---------------------------------------------------------------------------------------------
// Terrain adapter
// ---------------------------------------------------------------------------------------------

pub struct CourseTerrain(pub Arc<CollisionWorld>);

impl Terrain for CourseTerrain {
    fn ground(&self, x: f32, y: f32, z: f32) -> Option<Ground> {
        let hit = self.0.find_floor(x as f64, y as f64, z as f64);
        let s = hit.surface.as_ref()?;
        let surf = if s.kind == SURFACE_DEATH_PLANE || self.0.water_level(x as f64, z as f64) > hit.height + 30.0 {
            Surf::Hazard
        } else if s.normal[1] < 0.86 {
            Surf::Steep
        } else {
            Surf::Solid
        };
        Some(Ground { y: hit.height as f32, surf })
    }

    fn wall(&self, pos: [f32; 3]) -> [f32; 3] {
        let mut p = [pos[0] as f64, pos[1] as f64, pos[2] as f64];
        if self.0.f32_find_wall_collision(&mut p, 0.0, 55.0) > 0 {
            [p[0] as f32, p[1] as f32, p[2] as f32]
        } else {
            pos
        }
    }
}

/// Measure the island: the walkable area's centre and a radius that is mostly land.
pub fn detect_arena(world: &Arc<CollisionWorld>) -> Arena {
    let t = CourseTerrain(world.clone());
    let bound = world.horizontal_boundary.min(9000.0) as f32;
    let step = 300.0;
    let n = (bound * 2.0 / step) as i32;
    let (mut min_x, mut max_x, mut min_z, mut max_z) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    let (mut sum_y, mut count, mut min_y) = (0.0f64, 0usize, f32::MAX);
    for ix in 0..=n {
        for iz in 0..=n {
            let (x, z) = (-bound + ix as f32 * step, -bound + iz as f32 * step);
            if let Some(g) = t.ground(x, 12000.0, z) {
                if g.surf != Surf::Solid {
                    continue;
                }
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_z = min_z.min(z);
                max_z = max_z.max(z);
                sum_y += g.y as f64;
                min_y = min_y.min(g.y);
                count += 1;
            }
        }
    }
    if count == 0 {
        return Arena { cx: 0.0, cz: 0.0, r: 4000.0, top_y: 0.0, kill_y: -3000.0 };
    }
    let r = 0.5 * (max_x - min_x).min(max_z - min_z) * 0.94;
    Arena { cx: (min_x + max_x) / 2.0, cz: (min_z + max_z) / 2.0, r, top_y: (sum_y / count as f64) as f32, kill_y: min_y - 900.0 }
}

// ---------------------------------------------------------------------------------------------
// Mario body <-> core
// ---------------------------------------------------------------------------------------------

fn f3(p: [f64; 3]) -> [f32; 3] {
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

fn d3(p: [f32; 3]) -> [f64; 3] {
    [p[0] as f64, p[1] as f64, p[2] as f64]
}

fn motion_of(m: &MarioRuntime) -> Motion {
    let a = m.action;
    let air = a & ACT_FLAG_AIR != 0;
    let mv = match a {
        ACT_DIVE | ACT_SLIDE_KICK | ACT_JUMP_KICK => Mv::Dive,
        ACT_GROUND_POUND => Mv::Pound,
        ACT_GROUND_POUND_LAND => Mv::PoundLand,
        ACT_PUNCHING | ACT_MOVE_PUNCHING => Mv::Punch,
        ACT_BACKWARD_AIR_KB | ACT_FORWARD_AIR_KB | ACT_HARD_BACKWARD_AIR_KB | ACT_HARD_FORWARD_AIR_KB | ACT_BACKWARD_GROUND_KB | ACT_FORWARD_GROUND_KB | ACT_SOFT_BACKWARD_GROUND_KB | ACT_SOFT_FORWARD_GROUND_KB | ACT_HARD_BACKWARD_GROUND_KB | ACT_HARD_FORWARD_GROUND_KB => Mv::Knock,
        _ if air => Mv::Air,
        _ if m.forward_vel.abs() > 3.0 => Mv::Move,
        _ => Mv::Idle,
    };
    Motion { pos: f3(m.pos), vel: f3(m.vel), yaw: angle_to_radians(m.face_angle[1]) as f32, grounded: !air, mv }
}

/// Send a core knockback into a physics body: the real knockback action, then a harder shove.
fn apply_knock(m: &mut MarioRuntime, f: &mut Fighter) {
    if !f.knocked {
        return;
    }
    f.knocked = false;
    let kn = std::mem::take(&mut f.knock);
    let big = f.knock_dmg >= 14.0;
    m.invinc_timer = 0.0;
    let ok = m.take_damage(if big { 4.0 } else { 2.0 }, Some(d3(f.knock_from)));
    let h = (kn[0].hypot(kn[2]) * 1.15) as f64;
    if ok {
        m.forward_vel = m.forward_vel.signum() * m.forward_vel.abs().max(h);
        if big || m.action & ACT_FLAG_AIR != 0 {
            m.vel[1] = m.vel[1].max(kn[1] as f64 * 1.5 + 10.0);
            if m.action & ACT_FLAG_AIR == 0 {
                m.set_action(if m.forward_vel < 0.0 { ACT_BACKWARD_AIR_KB } else { ACT_FORWARD_AIR_KB }, 0.0);
            }
        }
    }
    m.health = 0x880 as f64;
    m.hurt_counter = 0.0;
}

fn drive_of(i: &Intent) -> DriveInput {
    DriveInput {
        yaw: (i.mag > 0.05).then_some(i.heading as f64 / std::f64::consts::PI * 32768.0),
        mag: Some((i.mag * 64.0) as f64),
        jump: i.jump,
        hold_a: i.hold_a || i.jump,
        punch: i.attack != Atk::None,
        crouch: i.pound,
        hold_z: false,
    }
}

// ---------------------------------------------------------------------------------------------
// The experiment
// ---------------------------------------------------------------------------------------------

struct Hooks {
    bots: usize,
    diff: Option<Difficulty>,
    start: Option<f32>,
    alive: Option<usize>,
    god: bool,
    kit: bool,
    die: Option<f32>,
    fight: bool,
    loot: bool,
    spectate: bool,
    edge: bool,
}

impl Hooks {
    fn from_env() -> Self {
        let phase = env_f32("JUMPMAN_LSS_PHASE").map(|n| {
            let i = (n as usize).clamp(1, PHASES.len()) - 1;
            Zone::phase_start(i) + PHASES[i].hold + 4.0
        });
        Self {
            bots: std::env::var("JUMPMAN_LSS_BOTS").ok().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_BOTS),
            diff: std::env::var("JUMPMAN_LSS_DIFF").ok().and_then(|s| Difficulty::parse(&s)),
            start: env_f32("JUMPMAN_LSS_START").or(phase),
            alive: std::env::var("JUMPMAN_LSS_ALIVE").ok().and_then(|s| s.parse().ok()),
            god: std::env::var_os("JUMPMAN_LSS_GOD").is_some(),
            kit: std::env::var_os("JUMPMAN_LSS_KIT").is_some(),
            die: env_f32("JUMPMAN_LSS_DIE"),
            fight: std::env::var_os("JUMPMAN_LSS_FIGHT").is_some(),
            loot: std::env::var_os("JUMPMAN_LSS_LOOT").is_some(),
            spectate: std::env::var_os("JUMPMAN_LSS_SPECTATE").is_some(),
            edge: std::env::var_os("JUMPMAN_LSS_EDGE").is_some(),
        }
    }
    fn cheats(&self) -> bool {
        self.start.is_some() || self.alive.is_some() || self.god || self.die.is_some() || self.fight
    }
}

fn run_seed() -> u64 {
    std::env::var("JUMPMAN_LSS_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64))
}

pub struct LastStarStandingExperiment {
    core: Option<Lss>,
    terrain: Option<CourseTerrain>,
    arena: Arena,
    seed: u64,
    runs: u32,
    t: f64,
    hooks: Hooks,
    records: Records,
    meta: Option<Arc<Vec<ClipMeta>>>,
    bodies: Vec<Option<CrowdMario>>,
    placed: Vec<bool>,
    drawn: Vec<bool>,
    motions: Vec<Option<Motion>>,
    shot_prev: Vec<([f32; 3], bool)>,
    /// Which fighter the camera follows after the player is out.
    spectating: Option<usize>,
    spec_pos: DVec3,
    spec_look: DVec3,
    dead_at: Option<f64>,
    over_at: Option<f64>,
    card_shown: bool,
    started: f64,
    fast_fall: bool,
    last_player_air: bool,
    feed_t: f32,
    first_run: bool,
    last_phase: usize,
    step_us: (f64, f64, u32),
    new_best: bool,
    recorded: bool,
    final_card: bool,
    card_at: f64,
}

impl LastStarStandingExperiment {
    pub fn new() -> Self {
        Self {
            core: None,
            terrain: None,
            arena: Arena { cx: 0.0, cz: 0.0, r: 4000.0, top_y: 0.0, kill_y: -3000.0 },
            seed: run_seed(),
            runs: 0,
            t: 0.0,
            hooks: Hooks::from_env(),
            records: Records::default(),
            meta: None,
            bodies: vec![],
            placed: vec![],
            drawn: vec![],
            motions: vec![],
            shot_prev: vec![([0.0; 3], false); SHOT_SLOTS],
            spectating: None,
            spec_pos: DVec3::ZERO,
            spec_look: DVec3::ZERO,
            dead_at: None,
            over_at: None,
            card_shown: false,
            started: 0.0,
            fast_fall: false,
            last_player_air: true,
            feed_t: 0.0,
            first_run: true,
            last_phase: 0,
            step_us: (0.0, 0.0, 0),
            new_best: false,
            recorded: false,
            final_card: false,
            card_at: 0.0,
        }
    }

    fn core(&self) -> &Lss {
        self.core.as_ref().unwrap()
    }

    fn start_match(&mut self, host: &mut Host) {
        self.runs += 1;
        // Free the previous match's bodies.
        for b in self.bodies.drain(..).flatten() {
            if let Some(e) = b.entity {
                host.commands.entity(e).despawn();
            }
        }
        let seed = self.seed.wrapping_add(self.runs as u64 * 7919);
        let terrain = self.terrain.take().unwrap_or_else(|| CourseTerrain(host.course_world()));
        let mut cfg = Config::new(seed, self.hooks.bots, self.arena);
        cfg.difficulty = self.hooks.diff.unwrap_or(difficulty_of(self.records.difficulty));
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            cfg.camps = 20;
            cfg.singles = 12;
        }
        let mut core = Lss::new(cfg, &terrain);
        core.god = self.hooks.god;
        let n = core.fighters.len();
        if let Some(secs) = self.hooks.start {
            core.fast_forward(secs, &terrain);
        }
        if let Some(a) = self.hooks.alive {
            core.cull_to(a);
        }
        if self.hooks.kit {
            core.give_kit(0);
        }
        // Bodies: one CrowdMario per bot; the entity appears when it is first drawn.
        let world = host.course_world();
        self.bodies = (0..n)
            .map(|i| {
                (i > 0).then(|| {
                    let f = &core.fighters[i];
                    let spawn = SpawnPoint { action: Some(ACT_FREEFALL), pos: d3(f.pos), face_yaw: radians_to_angle(f.yaw as f64) as f64 };
                    let mut b = CrowdMario::new(world.clone(), spawn, self.meta.as_ref().map(|m| m.as_slice()));
                    b.runtime.death_height = -1e9;
                    b.runtime.reset_to_spawn();
                    b.sync_previous();
                    b
                })
            })
            .collect();
        self.placed = vec![false; n];
        self.drawn = vec![false; n];
        self.motions = vec![None; n];
        self.shot_prev = vec![([0.0; 3], false); SHOT_SLOTS];
        self.spectating = None;
        self.dead_at = None;
        self.over_at = None;
        self.card_shown = false;
        self.new_best = false;
        self.recorded = false;
        self.final_card = false;
        self.card_at = 0.0;
        self.started = self.t;
        self.last_phase = 0;
        self.fast_fall = false;
        self.last_player_air = true;
        // The player starts at core fighter 0's drop position (or on the ground after a skip).
        let (f0pos, f0mode, f0yaw) = (core.fighters[0].pos, core.fighters[0].mode, core.fighters[0].yaw);
        let mut pos = d3(f0pos);
        if self.hooks.loot {
            if let Some(c) = core.camps.first() {
                pos = [c[0] as f64 - 300.0, c[1] as f64, c[2] as f64];
            }
        }
        if self.hooks.fight && self.hooks.start.is_some() {
            // A rival armed with a shell and a flower a few steps ahead.
            let id = 1;
            core.fighters[id].pos = [pos[0] as f32, pos[1] as f32, pos[2] as f32 - 420.0];
            core.fighters[id].slots = [Some(Item::new(Kind::Shell, Rarity::Rare)), Some(Item::new(Kind::Flower, Rarity::Common))];
            core.fighters[id].brain.aggression = 1.0;
        }
        let grounded = f0mode == Mode::Ground;
        let mut yaw = if grounded { std::f64::consts::PI } else { f0yaw as f64 };
        if self.hooks.edge {
            // Test hook: stand at the rim of the safe circle looking out at the Blight.
            let c = core.zv.cur;
            for k in 0..24 {
                let a = k as f32 * 0.5;
                let (x, z) = (c.x + a.sin() * (c.r - 500.0), c.z + a.cos() * (c.r - 500.0));
                if let Some(g) = terrain.ground(x, self.arena.top_y + 2000.0, z).filter(|g| g.surf == Surf::Solid && (g.y - self.arena.top_y).abs() < 250.0) {
                    pos = [x as f64, g.y as f64, z as f64];
                    yaw = a as f64;
                    core.fighters[0].pos = f3(pos);
                    break;
                }
            }
        }
        let face = radians_to_angle(yaw) as f64;
        host.place_mario(pos, Some(face));
        host.set_player_visible(true);
        let mut names = Vec::with_capacity(n);
        for f in &core.fighters {
            names.push(f.name.clone());
        }
        host.bridge_lss().reset(names);
        let b = host.bridge_lss();
        b.active = true;
        b.hud.hint = true;
        b.hud.total = n as u32;
        self.core = Some(core);
        self.terrain = Some(terrain);
        self.hooks.fight = false;
        host.toast(if self.first_run { "DROP IN! LAST STAR STANDING WINS" } else { "AGAIN!" }, 2.2);
        self.first_run = false;
    }

    /// Hand the player's Mario to the core as a Motion and run the glide while dropping.
    fn player_step(&mut self, host: &mut Host) {
        let core = self.core.as_mut().unwrap();
        if !core.fighters[0].alive {
            self.motions[0] = None;
            return;
        }
        let drop = core.fighters[0].mode == Mode::Drop;
        let mario = &mut host.sim.mario;
        if drop {
            // Glide: steer where the stick points (any speed), fall slowly, hold A to dive down.
            let air = mario.action & ACT_FLAG_AIR != 0;
            let height = mario.pos[1] - mario.floor_height;
            if air && height > 320.0 && !matches!(mario.action, ACT_DIVE | ACT_GROUND_POUND) {
                let fall = if self.fast_fall { -46.0 } else { -15.0 };
                mario.vel[1] = fall;
                mario.peak_height = mario.pos[1];
                let yaw = angle_to_radians(mario.intended_yaw);
                let k = (mario.intended_mag / 32.0).clamp(0.0, 1.0) * 34.0;
                host.shift_mario([yaw.sin() * k, 0.0, yaw.cos() * k]);
            }
        }
        let mario = &mut host.sim.mario;
        mario.health = 0x880 as f64;
        mario.hurt_counter = 0.0;
        // Swift Boots: extra distance on top of the physics.
        if core.fighters[0].boost_t > 0 && mario.action & ACT_FLAG_AIR == 0 {
            let yaw = angle_to_radians(mario.face_angle[1]);
            let v = mario.forward_vel.max(0.0) * 0.3;
            host.shift_mario([yaw.sin() * v, 0.0, yaw.cos() * v]);
        }
        self.motions[0] = Some(motion_of(&host.sim.mario));
    }

    /// Step every bot the core has flagged for the physics engine; keep the rest displayable.
    fn bots_step(&mut self, host: &mut Host) {
        let core = self.core.as_mut().unwrap();
        for i in 1..core.fighters.len() {
            self.motions[i] = None;
            let f = &core.fighters[i];
            if !f.alive {
                continue;
            }
            let Some(body) = self.bodies[i].as_mut() else { continue };
            if f.sim {
                if f.sim_started || !self.placed[i] {
                    let spawn = SpawnPoint { action: if f.grounded { None } else { Some(ACT_FREEFALL) }, pos: d3(f.pos), face_yaw: radians_to_angle(f.yaw as f64) as f64 };
                    body.runtime.spawn = spawn;
                    body.runtime.reset_to_spawn();
                    body.runtime.death_height = -1e9;
                    body.sync_previous();
                    self.placed[i] = true;
                }
                body.drive(&drive_of(&f.intent));
                self.motions[i] = Some(motion_of(&body.runtime));
            } else {
                // Kinematic: mirror the core's state into the (unstepped) body so it can be drawn.
                self.placed[i] = false;
                let near = {
                    let me = host.sim.mario.pos;
                    ((f.pos[0] as f64 - me[0]).hypot(f.pos[2] as f64 - me[2])) < DRAW_RANGE as f64
                };
                body.sync_previous();
                let rt = &mut body.runtime;
                rt.pos = d3(f.pos);
                rt.vel = d3(f.vel);
                rt.face_angle[1] = radians_to_angle(f.yaw as f64) as f64;
                let speed = (f.vel[0].hypot(f.vel[2]) as f64).max(if f.mv == Mv::Move { 24.0 } else { 0.0 });
                rt.forward_vel = if f.mv == Mv::Move { 26.0 * f.intent.mag as f64 } else { speed.min(30.0) };
                rt.intended_mag = if f.mv == Mv::Move { 32.0 * f.intent.mag as f64 } else { 0.0 };
                if near {
                    let want = match (f.mode, f.mv) {
                        (Mode::Drop, _) => ACT_FREEFALL,
                        (_, Mv::Dive) => ACT_DIVE,
                        (_, Mv::Pound) => ACT_GROUND_POUND,
                        (_, Mv::PoundLand) => ACT_GROUND_POUND_LAND,
                        (_, Mv::Punch) => ACT_PUNCHING,
                        (_, Mv::Air) | (_, Mv::Knock) if !f.grounded => ACT_FREEFALL,
                        (_, Mv::Move) => ACT_WALKING,
                        _ => ACT_IDLE,
                    };
                    if rt.action != want {
                        rt.set_action(want, 0.0);
                    }
                    rt.action_timer += 1.0;
                    body.update_pose();
                }
            }
        }
    }

    fn finish_player(&mut self, host: &mut Host) {
        // The player is out: hide him, park him, pick someone to watch.
        host.set_player_visible(false);
        let a = self.arena;
        host.place_mario([a.cx as f64, a.top_y as f64 + PARK_HEIGHT, a.cz as f64], None);
        self.dead_at = Some(self.t);
        let place = self.core().player_place;
        self.record_result(place, place == 1);
        let core = self.core();
        let by = core.events.iter().find_map(|e| match e {
            Event::Elim { victim: 0, killer: Some(k), .. } => Some(*k),
            _ => None,
        });
        let died_at = core.fighters[0].pos;
        let target = by.filter(|k| core.fighters[*k].alive).or_else(|| self.nearest_alive(died_at));
        self.spectating = target;
        host.fx("bonk", f64_pos(died_at));
    }

    /// `JUMPMAN_LSS_LOG=1`: average and worst cost of the experiment's 30 Hz step, every 5 s.
    fn log_step(&mut self, t0: std::time::Instant) {
        let us = t0.elapsed().as_secs_f64() * 1e6;
        self.step_us.0 += us;
        self.step_us.1 = self.step_us.1.max(us);
        self.step_us.2 += 1;
        if self.step_us.2 >= 150 {
            if std::env::var_os("JUMPMAN_LSS_LOG").is_some() {
                let c = self.core();
                let sim = c.fighters.iter().filter(|f| f.sim).count();
                eprintln!("[lss] t={:.0}s alive={} phase={} sim_bots={} step avg {:.0} us max {:.0} us", c.secs(), c.alive, c.zv.phase, sim, self.step_us.0 / self.step_us.2 as f64, self.step_us.1);
            }
            self.step_us = (0.0, 0.0, 0);
        }
    }

    fn nearest_alive(&self, from: [f32; 3]) -> Option<usize> {
        let core = self.core();
        core.fighters.iter().filter(|f| f.alive && !f.is_player).min_by(|a, b| dist2(a.pos, from).partial_cmp(&dist2(b.pos, from)).unwrap().then(a.id.cmp(&b.id))).map(|f| f.id)
    }

    fn step_spectate(&mut self, dir: i8) {
        let core = self.core.as_ref().unwrap();
        let ids: Vec<usize> = core.fighters.iter().filter(|f| f.alive && !f.is_player).map(|f| f.id).collect();
        if ids.is_empty() {
            self.spectating = None;
            return;
        }
        let cur = self.spectating.and_then(|s| ids.iter().position(|i| *i == s)).unwrap_or(0);
        let n = ids.len() as i32;
        self.spectating = Some(ids[((cur as i32 + dir as i32).rem_euclid(n)) as usize]);
    }

    fn handle_events(&mut self, host: &mut Host) {
        let core = self.core.as_ref().unwrap();
        let me_pos = host.sim.mario.pos;
        let name = |i: usize| -> String { core.fighters[i].name.clone() };
        let mut lines: Vec<(String, bool)> = vec![];
        let mut toasts: Vec<(&'static str, f32)> = vec![];
        let mut fx: Vec<(&'static str, [f64; 3])> = vec![];
        // Particle bursts (position, colour, count, speed, size), capped per tick below.
        let mut puffs: Vec<([f64; 3], u32, usize, f32, f32)> = vec![];
        let up = |p: [f32; 3], h: f64| {
            let q = f64_pos(p);
            [q[0], q[1] + h, q[2]]
        };
        for e in &core.events {
            match *e {
                Event::Elim { victim, killer, cause, .. } => {
                    let line = match (killer, cause) {
                        (Some(k), _) => format!("{} knocked {} out", name(k), name(victim)),
                        (None, Cause::Blight) => format!("{} was lost to the Blight", name(victim)),
                        (None, Cause::Fall) => format!("{} took a long fall", name(victim)),
                        (None, Cause::Fight) => format!("{} was knocked out", name(victim)),
                    };
                    lines.push((line, victim == 0 || killer == Some(0)));
                    fx.push(("break_box", f64_pos(core.fighters[victim].pos)));
                    puffs.push((up(core.fighters[victim].pos, 60.0), if killer == Some(0) { 0xffd24a } else { 0xfff1b8 }, 14, 260.0, 20.0));
                    if killer == Some(0) {
                        toasts.push(("KNOCKOUT!", 1.4));
                    }
                }
                Event::Pickup { who, kind } if who == 0 => {
                    fx.push(("coin", me_pos));
                    let r = core.fighters[0].slots.iter().flatten().chain(core.fighters[0].cons.iter()).find(|i| i.kind == kind).map_or(Rarity::Common, |i| i.rarity);
                    let hex = [0xe8e8f0u32, 0x5aa8ff, 0xc77dff][r.idx()];
                    puffs.push(([me_pos[0], me_pos[1] + 80.0, me_pos[2]], hex, 10, 150.0, 13.0));
                    lines.push((format!("Picked up {} ({})", kind.name(), r.label()), true));
                }
                Event::ChestOpen { pos } => {
                    fx.push(("star_sound", f64_pos(pos)));
                    puffs.push((up(pos, 50.0), 0xffe066, 14, 220.0, 15.0));
                }
                Event::AirdropCall { .. } => {
                    toasts.push(("AIRDROP INCOMING!", 1.8));
                    lines.push(("An airdrop crate is falling!".to_string(), false));
                }
                Event::Phase { phase, shrinking } => {
                    if shrinking {
                        toasts.push(("THE BLIGHT IS CLOSING IN!", 2.0));
                    } else if phase > 0 {
                        toasts.push(("NEW SAFE ZONE MARKED", 2.0));
                    }
                }
                Event::Explosion { pos, .. } => {
                    fx.push(("bonk", f64_pos(pos)));
                    puffs.push((up(pos, 30.0), 0xff9a3c, 16, 420.0, 26.0));
                }
                Event::Shock { pos, .. } => {
                    fx.push(("bonk", f64_pos(pos)));
                    puffs.push((up(pos, 10.0), 0xd8c39a, 12, 380.0, 22.0));
                }
                Event::Hit { target, dmg, .. } if dmg > 0.0 => {
                    if target == 0 {
                        fx.push(("bonk", me_pos));
                        puffs.push(([me_pos[0], me_pos[1] + 80.0, me_pos[2]], 0xff5a4a, 8, 240.0, 14.0));
                    } else {
                        puffs.push((up(core.fighters[target].pos, 80.0), 0xffffff, 5, 220.0, 12.0));
                    }
                }
                Event::Winner { who } => {
                    lines.push((format!("{} is the last star standing!", name(who)), who == 0));
                    if who == 0 {
                        toasts.push(("LAST STAR STANDING!", 3.0));
                    }
                }
                _ => {}
            }
        }
        for (t, s) in toasts {
            host.toast(t, s);
        }
        for (k, p) in fx {
            host.fx(k, p);
        }
        // Nearest first would need a sort; the cap alone keeps a 30-fighter brawl from flooding the pool.
        for (pos, color, count, speed, size) in puffs.into_iter().take(8) {
            host.particles.burst(pos, crate::fx::particles::BurstOptions { color: color, count: count, speed: speed, life: 0.45, gravity: -500.0, size: size, upward: 180.0, spread: 1.0, ..Default::default() });
        }
        let b = host.bridge_lss();
        for (text, mine) in lines {
            b.feed.push(FeedLine { text, age: 0.0, mine });
        }
        if b.feed.len() > 12 {
            let extra = b.feed.len() - 12;
            b.feed.drain(..extra);
        }
    }

    /// Write the player's result once (never for cheat-hook runs).
    fn record_result(&mut self, place: u32, won: bool) {
        if self.hooks.cheats() || self.recorded {
            return;
        }
        self.recorded = true;
        let mut r = self.records;
        if r.best_place == 0 || place < r.best_place {
            r.best_place = place;
            self.new_best = true;
        }
        if won {
            r.wins += 1;
        }
        self.records = r;
        save_records(r);
    }

    fn make_card(&self, kind: CardKind) -> Card {
        let core = self.core.as_ref().unwrap();
        let won = kind == CardKind::Win;
        Card {
            kind,
            place: core.player_place.max(1),
            total: core.fighters.len() as u32,
            kills: core.fighters[0].kills,
            secs: if won { core.secs() } else { core.fighters[0].died_tick as f32 / TPS as f32 },
            best_place: self.records.best_place,
            wins: self.records.wins,
            new_best: self.new_best,
            winner: core.winner.map(|w| core.fighters[w].name.clone()).unwrap_or_default(),
            can_spectate: kind == CardKind::Out && core.alive > 1,
            difficulty: self.records.difficulty,
        }
    }

    /// The "eliminated" card a moment after you go out, and the final card when the match ends.
    fn end_cards(&mut self, host: &mut Host) {
        let (over, alive, player_place) = {
            let c = self.core.as_ref().unwrap();
            (c.over, c.alive, c.player_place)
        };
        if over && self.over_at.is_none() {
            self.over_at = Some(self.t);
            self.record_result(player_place, player_place == 1);
        }
        let current = host.bridge_lss().card.as_ref().map(|c| c.kind);
        let mut show: Option<CardKind> = None;
        if over {
            let delay = if player_place == 1 { 1.8 } else { 1.4 };
            if !self.final_card && self.over_at.is_some_and(|t| self.t - t > delay) && matches!(current, None | Some(CardKind::Out)) {
                show = Some(if player_place == 1 { CardKind::Win } else { CardKind::End });
                self.final_card = true;
            }
        } else if let Some(d) = self.dead_at {
            if !self.card_shown && self.t - d > CARD_DELAY && alive > 1 {
                show = Some(CardKind::Out);
            }
        }
        if show == Some(CardKind::Out) && self.hooks.spectate {
            // Test hook: skip the card and go straight to spectating.
            self.card_shown = true;
            show = None;
        }
        if let Some(kind) = show {
            let card = self.make_card(kind);
            let b = host.bridge_lss();
            b.card = Some(card);
            b.card_gen = b.card_gen.wrapping_add(1);
            self.card_shown = true;
            self.card_at = self.t;
        }
        let ready = self.t - self.card_at > 0.7;
        let b = host.bridge_lss();
        if b.card_ready != (b.card.is_some() && ready) {
            b.card_ready = b.card.is_some() && ready;
        }
    }
    fn camera(&mut self, host: &mut Host, dt: f64) {
        let Some(target) = self.spectating else {
            return;
        };
        let core = self.core.as_ref().unwrap();
        let f = &core.fighters[target];
        let p = if f.sim { self.bodies[target].as_ref().map_or(d3(f.pos), |b| b.interpolated(host.alpha).0) } else { d3(f.pos) };
        let yaw = f.yaw as f64;
        let head = DVec3::new(p[0], p[1] + 120.0, p[2]);
        let mut desired = DVec3::new(p[0] - yaw.sin() * 760.0, p[1] + 380.0, p[2] - yaw.cos() * 760.0);
        let toward = desired - head;
        let length = toward.length();
        if length > 1.0 {
            if let Some(hit) = camera_ray(&host.course_world(), head, toward / length, length) {
                desired = head + toward / length * (hit - 60.0).max(120.0);
            }
        }
        let k = 1.0 - (-dt * 3.0).exp();
        if self.spec_pos == DVec3::ZERO || self.spec_pos.distance(desired) > 5000.0 {
            self.spec_pos = desired;
            self.spec_look = DVec3::new(p[0], p[1] + 100.0, p[2]);
        }
        self.spec_pos = self.spec_pos.lerp(desired, k);
        self.spec_look = self.spec_look.lerp(DVec3::new(p[0], p[1] + 100.0, p[2]), (k * 2.5).min(1.0));
        host.cam.pose = Some(CamPose { eye: self.spec_pos, look: self.spec_look, up: DVec3::Y, fov: 55.0, near: 10.0, far: 60000.0 });
    }

    /// Fill the bridge's render lists and HUD from the core.
    fn publish(&mut self, host: &mut Host) {
        let a = host.alpha.clamp(0.0, 1.0) as f32;
        let host_alpha = host.alpha;
        let focus_id = self.spectating.unwrap_or(0);
        let core = self.core.as_ref().unwrap();
        let focus = if focus_id == 0 { f3(host.render_pos()) } else { core.fighters[focus_id].pos };
        let t = self.t as f32;
        let zv = core.zv;
        let me = &core.fighters[0];
        let lerp3 = |p: [f32; 3], q: [f32; 3]| [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a, p[2] + (q[2] - p[2]) * a];
        let bp = |p: [f32; 3]| to_bevy(d3(p));
        let touch_dt = (t - self.feed_t).max(0.0);
        self.feed_t = t;
        let b = host.bridge_lss();
        b.time = t;
        for l in b.feed.iter_mut() {
            l.age += touch_dt;
        }
        b.feed.retain(|l| l.age < 7.5);
        // Pickups near the camera.
        for (i, p) in core.pickups.iter().enumerate() {
            let item = &mut b.pickups[i];
            if !p.alive || dist2(p.pos, focus) > PICKUP_RANGE {
                item.kind = 0;
                continue;
            }
            let fade_in = (p.age as f32 / 10.0).min(1.0);
            match p.kind {
                PickKind::Item => {
                    let k = ALL_KINDS.iter().position(|k| *k == p.item.kind).unwrap_or(0) as u8;
                    // Spilled items hop out of their container.
                    let hop = if p.age < 12 { (p.age as f32 / 12.0 * std::f32::consts::PI).sin() * 90.0 } else { 0.0 };
                    *item = RItem { kind: k + 1, pos: bp([p.pos[0], p.pos[1] + hop, p.pos[2]]), yaw: 0.0, scale: 0.7 + 0.3 * fade_in + 0.15 * p.item.rarity.idx() as f32, aux: p.item.rarity.idx() as f32 };
                }
                PickKind::Chest { opened } => *item = RItem { kind: if opened { 11 } else { 10 }, pos: bp(p.pos), yaw: (p.pos[0] * 0.013).sin() * 3.0, scale: 1.0, aux: 2.0 },
                PickKind::Crate { drop_y, opened } => {
                    let h = (drop_y - p.pos[1]).max(0.0);
                    *item = RItem { kind: if opened { 13 } else { 12 }, pos: bp([p.pos[0], p.pos[1] + h, p.pos[2]]), yaw: t * 0.8, scale: 1.0, aux: h };
                }
            }
        }
        for (i, s) in core.shots.iter().enumerate() {
            let item = &mut b.shots[i];
            if !s.alive {
                item.kind = 0;
                self.shot_prev[i].1 = false;
                continue;
            }
            let prev = if self.shot_prev[i].1 { self.shot_prev[i].0 } else { s.pos };
            let p = lerp3(prev, s.pos);
            let (k, scale) = match s.kind {
                ShotKind::Shell => (1, 1.0),
                ShotKind::Bomb => (2, 1.0 + 0.12 * ((s.life as f32) * 0.8).sin().abs()),
                ShotKind::Fire => (3, 1.0),
            };
            *item = RItem { kind: k, pos: bp(p), yaw: s.spin, scale, aux: 0.0 };
        }
        for (i, r) in core.rings.iter().enumerate() {
            let item = &mut b.rings[i];
            if r.alive {
                let k = match r.kind {
                    RingKind::Blast => 1,
                    RingKind::Shock => 2,
                    RingKind::Star => 3,
                };
                *item = RItem { kind: k, pos: bp(r.pos), yaw: 0.0, scale: r.radius() * crate::render::SCALE, aux: r.fade() };
            } else {
                item.kind = 0;
            }
        }
        // HUD.
        let me_p = if self.spectating.is_none() { f3(host.render_pos()) } else { me.pos };
        let b = host.bridge_lss();
        let core = self.core.as_ref().unwrap();
        b.hud = Hud {
            hp: me.hp,
            shield: me.shield,
            alive: core.alive as u32,
            total: core.fighters.len() as u32,
            kills: me.kills,
            secs: core.secs(),
            phase: zv.phase as u8,
            stage: match zv.stage {
                Stage::Landing => 0,
                Stage::Hold => 1,
                Stage::Shrink => 2,
                Stage::Final => 3,
            },
            zone_secs: zv.secs_left,
            zone_dist: dist2(me_p, [zv.next.x, 0.0, zv.next.z]),
            outside: me.alive && me.mode == Mode::Ground && !zv.cur.contains(me_p) && zv.dps > 0.0,
            dropping: me.mode == Mode::Drop,
            quake: me.quake,
            boost: me.boost_t > 0,
            hint: core.secs() < 14.0 && me.alive,
            god: core.god,
        };
        for (i, slot) in b.slots.iter_mut().enumerate() {
            let it = if i < 2 { me.slots[i] } else { me.cons };
            *slot = match it {
                Some(it) => SlotView { kind: ALL_KINDS.iter().position(|k| *k == it.kind).unwrap_or(0) as u8 + 1, rarity: it.rarity.idx() as u8, ammo: it.ammo, cooldown: me.cd[i] as f32 / it.kind.cooldown().max(1) as f32 },
                None => SlotView::default(),
            };
        }
        b.map = MapView { arena: [core.arena.cx, core.arena.cz, core.arena.r], cur: [zv.cur.x, zv.cur.z, zv.cur.r], next: [zv.next.x, zv.next.z, zv.next.r], me: [me_p[0], me_p[2]], show_next: zv.phase > 0 && zv.stage != Stage::Final };
        b.zone = ZoneVis { cur: [zv.cur.x, zv.cur.z, zv.cur.r], next: [zv.next.x, zv.next.z, zv.next.r], show_next: zv.phase > 0 && zv.stage != Stage::Final, base_y: core.arena.top_y, active: true };
        // Name tags: the nearest drawn rivals.
        b.tags.clear();
        let mut near = [(0.0f32, 0usize); MAX_FIGHTERS];
        let mut n = 0;
        for f in core.fighters.iter().filter(|f| f.alive && f.id != focus_id && self.drawn[f.id]) {
            let d = dist2(f.pos, focus);
            if d < 2200.0 && n < MAX_FIGHTERS {
                near[n] = (d, f.id);
                n += 1;
            }
        }
        let near = &mut near[..n];
        near.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap().then(x.1.cmp(&y.1)));
        // No tags while gliding in (everybody is a speck overhead).
        let tag_count = if me.mode == Mode::Drop && focus_id == 0 { 0 } else { 6 };
        for (_, id) in near.iter().copied().take(tag_count) {
            let f = &core.fighters[id];
            let p = match (f.sim, self.bodies[id].as_ref()) {
                (true, Some(body)) => f3(body.interpolated(host_alpha).0),
                _ => f.pos,
            };
            b.tags.push(Tag { id, pos: to_bevy(d3([p[0], p[1] + 210.0, p[2]])), hp: f.hp / MAX_HP });
        }
        match self.spectating.filter(|_| self.dead_at.is_some() && b.card.is_none() && self.card_shown) {
            Some(s) => {
                let name = &core.fighters[s].name;
                if b.spectate.as_ref() != Some(name) {
                    b.spectate = Some(name.clone());
                }
            }
            None => b.spectate = None,
        }
    }

    /// Draw the Marios: nearest first, up to the budget; the rest are hidden.
    fn render_bots(&mut self, host: &mut Host, dt: f64) {
        let core = self.core.as_ref().unwrap();
        let focus_id = self.spectating.unwrap_or(0);
        let eye = if focus_id == 0 { host.render_pos() } else { d3(core.fighters[focus_id].pos) };
        // Nearest first, in fixed scratch arrays (nothing is allocated per frame).
        let mut order = [(0.0f64, 0usize); MAX_FIGHTERS];
        let mut n = 0;
        for i in 1..core.fighters.len().min(MAX_FIGHTERS) {
            if core.fighters[i].alive {
                let p = core.fighters[i].pos;
                order[n] = (((p[0] as f64 - eye[0]).powi(2) + (p[1] as f64 - eye[1]).powi(2) + (p[2] as f64 - eye[2]).powi(2)).sqrt(), i);
                n += 1;
            }
        }
        let order = &mut order[..n];
        order.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        let mut show = [false; MAX_FIGHTERS];
        let mut dist = [0.0f64; MAX_FIGHTERS];
        for (d, i) in order.iter().take(MAX_DRAWN) {
            show[*i] = *d < DRAW_RANGE as f64;
            dist[*i] = *d;
        }
        for i in 1..core.fighters.len().min(MAX_FIGHTERS) {
            let Some(body) = self.bodies[i].as_mut() else { continue };
            if show[i] {
                if body.entity.is_none() {
                    let look = CrowdLook { cap: cap_color(i), overalls: (i % 4 == 1).then_some(0x2b2b3a) };
                    body.entity = Some(spawn_body(host.commands, host.assets, look));
                }
                if !self.drawn[i] {
                    host.commands.entity(body.entity.unwrap()).insert(Visibility::Inherited);
                    self.drawn[i] = true;
                }
                body.render(host.commands, host.alpha, dt, dist[i]);
            } else if self.drawn[i] {
                if let Some(e) = body.entity {
                    host.commands.entity(e).insert(Visibility::Hidden);
                }
                self.drawn[i] = false;
            }
        }
    }
}

fn f64_pos(p: [f32; 3]) -> [f64; 3] {
    d3(p)
}

impl Host<'_, '_, '_> {
    fn bridge_lss(&mut self) -> &mut LssBridge {
        self.lss
    }
}

impl Experiment for LastStarStandingExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::LastStar
    }
    fn course(&self) -> Option<&'static str> {
        Some("bob")
    }
    /// An eliminated player stops simulating (he is parked and hidden).
    fn freezes_player(&self) -> bool {
        self.dead_at.is_some()
    }

    fn enter(&mut self, host: &mut Host) {
        host.shell.plain_camera = true;
        host.rig.distance = 1100.0;
        host.rig.pitch = 0.5;
        self.meta = init_clips(host);
        let world = host.course_world();
        self.arena = detect_arena(&world);
        eprintln!("[lss] arena {:?}", self.arena);
        self.terrain = Some(CourseTerrain(world));
        self.records = load_records();
        self.start_match(host);
    }

    fn exit(&mut self, host: &mut Host) {
        let b = host.bridge_lss();
        b.active = false;
        b.card = None;
        b.zone.active = false;
        host.set_player_visible(true);
        host.cam.pose = None;
        host.sim.mario.reset_to_spawn();
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if self.core.is_none() {
            return false;
        }
        let b = host.bridge_lss();
        // The slot keys also arrive here (scripted presses in screenshot runs).
        match key {
            KeyCode::Digit1 => b.slot_press[0] = true,
            KeyCode::Digit2 => b.slot_press[1] = true,
            KeyCode::Digit3 => b.slot_press[2] = true,
            _ => {}
        }
        if self.spectating.is_some() && b.card.is_none() {
            match key {
                KeyCode::ArrowRight | KeyCode::KeyE => {
                    self.step_spectate(1);
                    return true;
                }
                KeyCode::ArrowLeft | KeyCode::KeyQ => {
                    self.step_spectate(-1);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn before_step(&mut self, _host: &mut Host, controller: &ControllerSnapshot) -> StepOverride {
        self.fast_fall = controller.button_down & A_BUTTON != 0;
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        let t0 = std::time::Instant::now();
        if self.core.is_none() {
            return;
        }
        // Pointers into the core can't be held across the helper calls, so work in phases.
        for (i, s) in self.core.as_ref().unwrap().shots.iter().enumerate() {
            self.shot_prev[i] = (s.pos, s.alive);
        }
        // Inputs from the UI.
        {
            let presses = std::mem::take(&mut host.bridge_lss().slot_press);
            let core = self.core.as_mut().unwrap();
            if core.fighters[0].alive {
                let yaw = angle_to_radians(host.sim.mario.face_angle[1]) as f32;
                for (slot, p) in presses.iter().enumerate() {
                    if *p {
                        let (h, _) = core.aim(0, yaw, 0.7, 1600.0);
                        core.use_item(0, slot, h);
                    }
                }
            }
        }
        self.player_step(host);
        self.bots_step(host);
        // Scripted elimination for screenshots.
        if let Some(secs) = self.hooks.die {
            let core = self.core.as_mut().unwrap();
            if core.fighters[0].alive && self.t - self.started >= secs as f64 {
                core.eliminate(0, Cause::Fight);
            }
        }
        {
            let terrain = self.terrain.as_ref().unwrap();
            let core = self.core.as_mut().unwrap();
            core.tick(terrain, &self.motions);
            // Knockbacks go into the physics bodies.
            if core.fighters[0].alive {
                apply_knock(&mut host.sim.mario, &mut core.fighters[0]);
                if core.fighters[0].bounce {
                    core.fighters[0].bounce = false;
                    host.sim.mario.vel[1] = host.sim.mario.vel[1].max(36.0);
                }
            }
            for i in 1..core.fighters.len() {
                if core.fighters[i].sim && core.fighters[i].alive {
                    if let Some(b) = self.bodies[i].as_mut() {
                        apply_knock(&mut b.runtime, &mut core.fighters[i]);
                        if core.fighters[i].bounce {
                            core.fighters[i].bounce = false;
                            b.runtime.vel[1] = b.runtime.vel[1].max(36.0);
                        }
                    }
                }
            }
            // Next tick's physics set: the bots near whoever the camera follows.
            let focus = match self.spectating {
                Some(s) => core.fighters[s].pos,
                None => core.fighters[0].pos,
            };
            core.choose_sim(focus, SIM_RADIUS, SIM_CAP);
        }
        let was_dead = self.dead_at.is_some();
        self.handle_events(host);
        if !was_dead && !self.core().fighters[0].alive {
            self.finish_player(host);
        }
        // Keep the phase tracking for the toast hooks.
        self.last_phase = self.core().zv.phase;
        self.log_step(t0);
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        if self.core.is_none() {
            return;
        }
        self.t += dt;
        // UI requests.
        {
            let b = host.bridge_lss();
            if b.retry {
                b.retry = false;
                host.cam.pose = None;
                self.start_match(host);
                return;
            }
            if std::mem::take(&mut b.diff_press) {
                // Cycle the bots' difficulty for the next match (saved).
                self.records.difficulty = (self.records.difficulty + 1) % 3;
                save_records(self.records);
                let d = self.records.difficulty;
                if let Some(c) = b.card.as_mut() {
                    c.difficulty = d;
                }
                b.card_gen = b.card_gen.wrapping_add(1);
            }
            if std::mem::take(&mut b.spectate_start) {
                b.card = None;
                b.card_gen = b.card_gen.wrapping_add(1);
                self.card_shown = true;
            }
            let step = std::mem::take(&mut b.spec_step);
            if step != 0 {
                self.step_spectate(step);
            }
        }
        if let Some(s) = self.spectating {
            if !self.core().fighters[s].alive {
                let next = self.nearest_alive(self.core().fighters[s].pos);
                self.spectating = next;
            }
        }
        self.end_cards(host);
        // Blink while protected.
        let core = self.core.as_ref().unwrap();
        let me = &core.fighters[0];
        let blink = me.alive && me.iframes > 0 && me.mode == Mode::Ground && (self.t * 18.0) as i64 % 2 == 0;
        if me.alive {
            host.set_player_visible(!blink);
        }
        self.render_bots(host, dt);
        self.publish(host);
        if std::env::var_os("JUMPMAN_LSS_TOPDOWN").is_some() {
            // Test hook: an overview from straight above the island.
            let a = self.arena;
            let h = std::env::var("JUMPMAN_LSS_TOPDOWN").ok().and_then(|s| s.parse::<f64>().ok()).filter(|h| *h > 100.0).unwrap_or(20000.0);
            host.cam.pose = Some(CamPose { eye: DVec3::new(a.cx as f64, a.top_y as f64 + h, a.cz as f64 + 1.0), look: DVec3::new(a.cx as f64, a.top_y as f64, a.cz as f64), up: DVec3::Y, fov: 60.0, near: 50.0, far: 90000.0 });
        } else if self.spectating.is_some() && self.dead_at.is_some() {
            self.camera(host, dt);
        } else {
            host.cam.pose = None;
        }
    }

    fn debug(&self) -> String {
        match &self.core {
            Some(c) => format!("t={:.0} alive={} phase={} hp={:.0} kills={}", c.secs(), c.alive, c.zv.phase, c.fighters[0].hp, c.fighters[0].kills),
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::geometry::add_box_collision;
    use jumpman_sim::collision::SurfaceOptions;

    /// A 6000 x 6000 slab centred on the origin, top at y = 100.
    fn slab_world() -> Arc<CollisionWorld> {
        let mut world = CollisionWorld::new(1024.0, 1e9);
        let mut faces = vec![];
        add_box_collision(&mut world, [0.0, -100.0, 0.0], [6000.0, 400.0, 6000.0], 0.0, &mut faces, &SurfaceOptions::default());
        world.finalize();
        Arc::new(world)
    }

    fn runtime(world: &Arc<CollisionWorld>) -> MarioRuntime {
        let mut m = MarioRuntime::new(world.clone(), SpawnPoint { action: None, pos: [0.0, 100.0, 0.0], face_yaw: 0.0 }, -1e9);
        m.reset_to_spawn();
        m
    }

    #[test]
    fn the_island_is_measured_from_the_collision() {
        let world = slab_world();
        let a = detect_arena(&world);
        assert!(a.cx.abs() < 400.0 && a.cz.abs() < 400.0, "{a:?}");
        assert!((2400.0..3100.0).contains(&a.r), "radius {}", a.r);
        assert!((a.top_y - 100.0).abs() < 5.0 && a.kill_y < a.top_y - 800.0);
        let t = CourseTerrain(world);
        let g = t.ground(100.0, 600.0, 100.0).expect("floor under the slab top");
        assert!((g.y - 100.0).abs() < 1.0 && g.surf == Surf::Solid);
        assert!(t.ground(9000.0, 600.0, 0.0).is_none(), "void past the edge");
    }

    #[test]
    fn mario_actions_map_to_the_rules_moves() {
        let world = slab_world();
        let mut m = runtime(&world);
        let cases = [(ACT_DIVE, Mv::Dive, false), (ACT_GROUND_POUND, Mv::Pound, false), (ACT_GROUND_POUND_LAND, Mv::PoundLand, true), (ACT_PUNCHING, Mv::Punch, true), (ACT_BACKWARD_AIR_KB, Mv::Knock, false), (ACT_FREEFALL, Mv::Air, false), (ACT_IDLE, Mv::Idle, true)];
        for (action, mv, grounded) in cases {
            m.action = action;
            let got = motion_of(&m);
            assert_eq!((got.mv, got.grounded), (mv, grounded), "action {action:#x}");
        }
        m.action = ACT_WALKING;
        m.forward_vel = 20.0;
        assert_eq!(motion_of(&m).mv, Mv::Move);
    }

    #[test]
    fn bot_intents_become_stick_and_buttons() {
        let d = drive_of(&Intent { heading: std::f32::consts::FRAC_PI_2, mag: 1.0, jump: true, ..Default::default() });
        assert!((d.yaw.unwrap() - 16384.0).abs() < 1.0 && d.mag == Some(64.0) && d.jump && d.hold_a);
        let idle = drive_of(&Intent::default());
        assert!(idle.yaw.is_none() && !idle.jump && !idle.punch && !idle.crouch);
        let atk = drive_of(&Intent { attack: Atk::Dive, pound: true, ..Default::default() });
        assert!(atk.punch && atk.crouch);
    }

    #[test]
    fn knockback_puts_the_body_into_a_real_knockback_action_and_keeps_health_full() {
        let world = slab_world();
        let mut m = runtime(&world);
        let mut fighter = {
            let t = CourseTerrain(world.clone());
            let a = detect_arena(&world);
            let mut l = Lss::new(Config::new(5, 2, a), &t);
            l.fighters[0].knocked = true;
            l.fighters[0].knock = [14.0, 10.0, 0.0];
            l.fighters[0].knock_dmg = 16.0;
            l.fighters[0].knock_from = [-300.0, 100.0, 0.0];
            l.fighters.remove(0)
        };
        apply_knock(&mut m, &mut fighter);
        assert!(!fighter.knocked);
        assert!(m.action & ACT_FLAG_AIR != 0, "a big hit launches into an air knockback, action {:#x}", m.action);
        assert!(matches!(motion_of(&m).mv, Mv::Knock));
        assert_eq!(m.health, 0x880 as f64);
        assert!(m.vel[1] > 10.0);
    }

    #[test]
    fn records_round_trip_and_difficulty_maps() {
        let dir = std::env::temp_dir().join(format!("lss-records-{}", std::process::id()));
        let path = dir.join("lss.json");
        assert_eq!(read_records(&path), Records::default());
        write_records(&path, Records { best_place: 4, wins: 2, difficulty: 2 });
        assert_eq!(read_records(&path), Records { best_place: 4, wins: 2, difficulty: 2 });
        // A junk difficulty is clamped.
        std::fs::write(&path, r#"{"best_place":1,"difficulty":9}"#).unwrap();
        assert_eq!(read_records(&path), Records { best_place: 1, wins: 0, difficulty: 2 });
        assert_eq!(difficulty_of(0), Difficulty::Easy);
        assert_eq!(difficulty_of(9), Difficulty::Hard);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
