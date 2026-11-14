//! `marioCrowd.ts`: a crowd of full-physics Marios with simple, readable goals. The brain only
//! ever touches the stick and buttons; all movement is SM64's own.
//!
//! Differences from the TS: dropped "hot" coins and scripted tricks are not ported; the crowd
//! reads neighbours from a snapshot taken at the start of each tick (the TS reads live
//! positions mid-tick); no particles/sfx.
use super::crowd::{spawn_body, CrowdLook, CrowdMario, DriveInput};
use super::crowd_route::mario_route;
use super::Host;
use crate::render::{to_bevy, SCALE};
use bevy::prelude::*;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
use jumpman_sim::math::{atan2s, coss, sins, Vec3 as V3};
use jumpman_sim::state::*;
use jumpman_sim::surfaces::SURFACE_DEATH_PLANE;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Personality {
    Racer,
    Hunter,
    Follower,
    Brawler,
    Explorer,
}

pub const PERSONALITY_ORDER: [Personality; 5] = [Personality::Racer, Personality::Hunter, Personality::Follower, Personality::Brawler, Personality::Explorer];

pub struct PersonalityInfo {
    pub label: &'static str,
    pub color: u32,
    pub overalls: Option<u32>,
    pub blurb: &'static str,
}

impl Personality {
    pub fn info(self) -> PersonalityInfo {
        match self {
            Self::Racer => PersonalityInfo { label: "Racer", color: 0xe23b2e, overalls: None, blurb: "Runs the Koopa race route to the summit, lap after lap." },
            Self::Hunter => PersonalityInfo { label: "Coin hunter", color: 0xf5c518, overalls: None, blurb: "Sprints to the nearest coin before anyone else." },
            Self::Follower => PersonalityInfo { label: "Follower", color: 0xff6fb5, overalls: None, blurb: "Flocks around you and copies your jumps." },
            Self::Brawler => PersonalityInfo { label: "Brawler", color: 0x8c3fd9, overalls: Some(0x22222e), blurb: "Guards a patch of turf and punches intruders." },
            Self::Explorer => PersonalityInfo { label: "Explorer", color: 0x17b3a6, overalls: None, blurb: "Wanders anywhere, long-jumping and triple-jumping." },
        }
    }
    pub fn look(self) -> CrowdLook {
        let i = self.info();
        CrowdLook { cap: i.color, overalls: i.overalls }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrowdMix {
    Mixed,
    Race,
    Gold,
    Parade,
    Brawl,
}

impl CrowdMix {
    fn weights(self) -> &'static [(Personality, f64)] {
        use Personality::*;
        match self {
            Self::Mixed => &[(Racer, 0.3), (Hunter, 0.25), (Follower, 0.2), (Brawler, 0.1), (Explorer, 0.15)],
            Self::Race => &[(Racer, 1.0)],
            Self::Gold => &[(Hunter, 1.0)],
            Self::Parade => &[(Follower, 1.0)],
            Self::Brawl => &[(Brawler, 1.0)],
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Mixed => "mixed",
            Self::Race => "race day",
            Self::Gold => "gold rush",
            Self::Parade => "parade",
            Self::Brawl => "brawl",
        }
    }
}

/// Deterministic personality assignment for agent index i under a mix.
pub fn personality_for(mix: CrowdMix, index: usize, available: &[Personality]) -> Personality {
    let weights: Vec<(Personality, f64)> = mix.weights().iter().copied().filter(|(p, _)| available.contains(p)).collect();
    if weights.is_empty() {
        return available.first().copied().unwrap_or(Personality::Explorer);
    }
    let total: f64 = weights.iter().map(|(_, w)| w).sum();
    // Interleave so every slice of the crowd has the same blend.
    let slot = ((index as f64 * 0.61803398875) % 1.0) * total;
    let mut acc = 0.0;
    for (p, w) in &weights {
        acc += w;
        if slot < acc {
            return *p;
        }
    }
    weights.last().unwrap().0
}

/// xorshift32, as in the TS.
pub struct Rng(u32);
impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }
    pub fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f64 / 4294967296.0
    }
}

/// A standable floor point near (cx, cz), or None.
pub fn sample_floor(world: &CollisionWorld, cx: f64, cz: f64, radius: f64, rng: &mut Rng) -> Option<V3> {
    for _ in 0..40 {
        let angle = rng.next() * std::f64::consts::TAU;
        let r = rng.next().sqrt() * radius;
        let (x, z) = (cx + angle.cos() * r, cz + angle.sin() * r);
        if x.abs() > world.horizontal_boundary - 300.0 || z.abs() > world.horizontal_boundary - 300.0 {
            continue;
        }
        let floor = world.find_floor(x, 9000.0, z);
        let Some(s) = floor.surface.as_ref() else { continue };
        if s.normal[1] < 0.86 || s.kind == SURFACE_DEATH_PLANE {
            continue;
        }
        if world.water_level(x, z) > floor.height - 20.0 {
            continue;
        }
        return Some([x, floor.height, z]);
    }
    None
}

pub struct Coin {
    pub entity: Option<Entity>,
    pub pos: V3,
    pub active: bool,
    /// Ticks until a collected coin respawns; negative = a one-off dropped coin.
    pub respawn: i32,
    pub fall_vel: f64,
    /// Ticks a dropped coin keeps pulling every Mario toward it.
    pub hot: i32,
}

pub struct CrowdAgent {
    pub id: u32,
    pub name: String,
    pub body: CrowdMario,
    pub personality: Personality,
    pub goal: Option<V3>,
    goal_coin: Option<usize>,
    pub label: String,
    pub waypoint: usize,
    pub laps: u32,
    pub coins: u32,
    pub hits: u32,
    home: V3,
    goal_timer: f64,
    celebrate: i32,
    hold_a: i32,
    jump_at: i64,
    cooldown: i32,
    stuck: i32,
    stuck_check: V3,
    stuck_ticks: i32,
    recoveries: i32,
    best_distance: f64,
    no_progress: i32,
    stalls: i32,
    progress_goal: Option<V3>,
    unstick: i32,
    unstick_yaw: f64,
    flair: Flair,
    combo: i32,
    pub start_tick: u64,
    pub best_lap: f64,
    /// Goals this Mario gave up on (unreachable coins), until the given tick.
    avoid: HashMap<usize, u64>,
    /// Steering set by the owning mode (Summit Race's racer brain): when present the crowd brain is skipped and this
    /// input drives the body every step (knock-backs still play out first).
    pub manual: Option<DriveInput>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flair {
    None,
    LongJump,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct CrowdStats {
    pub summits: u32,
    pub coins: u32,
    pub hits: u32,
    pub best_lap: Option<f64>,
}

pub struct CrowdAgentDebug {
    pub id: u32,
    pub name: String,
    pub personality: Personality,
    pub label: String,
    pub action: String,
    pub pos: V3,
    pub speed: f64,
    pub coins: u32,
    pub laps: u32,
    pub hits: u32,
    pub best_lap: Option<f64>,
}

pub struct MarioCrowdOptions {
    pub world: Arc<CollisionWorld>,
    /// Waypoints for racers; racers become explorers without one.
    pub route: Option<Vec<V3>>,
    /// Whether followers flock to a leader (the player).
    pub has_leader: bool,
    pub coin_count: usize,
    /// Centre and radius used to sample spawn, coin and wander points.
    pub center: V3,
    pub radius: f64,
    pub seed: u32,
    /// Racers stop at the summit instead of starting another lap.
    pub single_lap: bool,
}

const SEPARATION: f64 = 120.0;
const GRID: f64 = 256.0;

#[derive(Clone, Copy)]
struct Snap {
    pos: V3,
    personality: Personality,
    index: usize,
}

struct Grid {
    cells: HashMap<(i32, i32), Vec<Snap>>,
}

impl Grid {
    fn build(agents: &[CrowdAgent]) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<Snap>> = HashMap::new();
        for (index, a) in agents.iter().enumerate() {
            let p = a.body.pos();
            cells.entry(((p[0] / GRID).floor() as i32, (p[2] / GRID).floor() as i32)).or_default().push(Snap { pos: p, personality: a.personality, index });
        }
        Self { cells }
    }

    /// Every snapshot within `radius` horizontally (and 200 vertically) with its distance.
    fn near(&self, pos: V3, radius: f64) -> Vec<(Snap, f64)> {
        let mut out = vec![];
        let (x0, x1) = (((pos[0] - radius) / GRID).floor() as i32, ((pos[0] + radius) / GRID).floor() as i32);
        let (z0, z1) = (((pos[2] - radius) / GRID).floor() as i32, ((pos[2] + radius) / GRID).floor() as i32);
        for x in x0..=x1 {
            for z in z0..=z1 {
                let Some(list) = self.cells.get(&(x, z)) else { continue };
                for s in list {
                    let d = (s.pos[0] - pos[0]).hypot(s.pos[2] - pos[2]);
                    if d <= radius && (s.pos[1] - pos[1]).abs() < 200.0 {
                        out.push((*s, d));
                    }
                }
            }
        }
        out
    }
}

pub struct LeaderInfo {
    pub pos: V3,
}

pub struct MarioCrowd {
    pub agents: Vec<CrowdAgent>,
    pub coins: Vec<Coin>,
    pub speed: f64,
    pub mix: CrowdMix,
    pub tick: u64,
    accumulator: f64,
    next_id: u32,
    rng: Rng,
    leader_was_air: bool,
    fx: Vec<(&'static str, V3)>,
    pub stats: CrowdStats,
    pub selected: Option<u32>,
    /// The racers' route, re-planned so Mario can actually run it.
    pub route: Option<Vec<V3>>,
    opts: MarioCrowdOptions,
    meta: Option<Arc<Vec<crate::game::sm64_animation::ClipMeta>>>,
}

impl MarioCrowd {
    pub fn new(opts: MarioCrowdOptions, meta: Option<Arc<Vec<crate::game::sm64_animation::ClipMeta>>>, host: Option<&mut Host>) -> Self {
        let route = opts.route.as_ref().filter(|r| !r.is_empty()).map(|r| mario_route(&opts.world, r));
        let mut me = Self {
            agents: vec![],
            coins: vec![],
            speed: 1.0,
            mix: CrowdMix::Mixed,
            tick: 0,
            accumulator: 0.0,
            next_id: 1,
            rng: Rng::new(opts.seed),
            leader_was_air: false,
            fx: vec![],
            stats: CrowdStats::default(),
            selected: None,
            route,
            opts,
            meta,
        };
        me.scatter_coins(host);
        me
    }

    pub fn available(&self) -> Vec<Personality> {
        PERSONALITY_ORDER.iter().copied().filter(|p| (*p != Personality::Racer || self.route.is_some()) && (*p != Personality::Follower || self.opts.has_leader)).collect()
    }

    fn make_agent(&mut self, pos: V3, face_yaw: f64, personality: Personality, name: String, host: Option<&mut Host>) -> CrowdAgent {
        let spawn = SpawnPoint { action: Some(ACT_FREEFALL), pos, face_yaw };
        let mut body = CrowdMario::new(self.opts.world.clone(), spawn, self.meta.as_ref().map(|m| m.as_slice()));
        body.runtime.reset_to_spawn();
        body.sync_previous();
        if let Some(host) = host {
            body.entity = Some(spawn_body(host.commands, host.assets, personality.look()));
        }
        let id = self.next_id;
        self.next_id += 1;
        CrowdAgent {
            id,
            name,
            body,
            personality,
            goal: None,
            goal_coin: None,
            label: "Dropping in".into(),
            waypoint: 0,
            laps: 0,
            coins: 0,
            hits: 0,
            home: pos,
            goal_timer: 0.0,
            celebrate: 0,
            hold_a: 0,
            jump_at: -1,
            cooldown: 0,
            stuck: 0,
            stuck_check: pos,
            stuck_ticks: 0,
            recoveries: 0,
            best_distance: f64::INFINITY,
            no_progress: 0,
            stalls: 0,
            progress_goal: None,
            unstick: 0,
            unstick_yaw: 0.0,
            flair: Flair::None,
            combo: 0,
            start_tick: self.tick,
            best_lap: f64::INFINITY,
            avoid: HashMap::new(),
            manual: None,
        }
    }

    /// Replace the crowd with `count` Marios that rain in around `origin`.
    pub fn populate(&mut self, count: usize, origin: V3, spread: f64, mut host: Option<&mut Host>) {
        self.clear_agents(host.as_deref_mut());
        let available = self.available();
        for i in 0..count {
            let personality = personality_for(self.mix, i, &available);
            let world = self.opts.world.clone();
            let spot = sample_floor(&world, origin[0], origin[2], spread + (i as f64 / count as f64) * spread, &mut self.rng)
                .or_else(|| sample_floor(&world, self.opts.center[0], self.opts.center[2], self.opts.radius, &mut self.rng));
            let Some(spot) = spot else { continue };
            let drop = [spot[0], spot[1] + 500.0 + self.rng.next() * 1100.0, spot[2]];
            let yaw = (self.rng.next() * 65536.0).floor() - 32768.0;
            let id = self.next_id;
            let mut agent = self.make_agent(drop, yaw, personality, format!("Mario #{id}"), host.as_deref_mut());
            agent.home = spot;
            if personality == Personality::Racer {
                self.start_race(&mut agent, true);
            }
            self.agents.push(agent);
        }
    }

    /// Place one Mario per spot, standing (a starting grid rather than a drop-in).
    pub fn populate_at(&mut self, spots: &[V3], personality: Personality, face_yaw: f64, mut host: Option<&mut Host>) {
        self.clear_agents(host.as_deref_mut());
        for (n, spot) in spots.iter().enumerate() {
            let mut agent = self.make_agent(*spot, face_yaw, personality, format!("Rival #{}", n + 1), host.as_deref_mut());
            agent.label = "On the grid".into();
            // Standing start: the spawn action should be idle, not a drop.
            agent.body.runtime.spawn.action = None;
            agent.body.runtime.reset_to_spawn();
            agent.body.sync_previous();
            if personality == Personality::Racer {
                self.start_race(&mut agent, true);
            }
            self.agents.push(agent);
        }
    }

    pub fn set_mix(&mut self, mix: CrowdMix, host: Option<&mut Host>) {
        self.mix = mix;
        let available = self.available();
        let mut host = host;
        for index in 0..self.agents.len() {
            let p = personality_for(mix, index, &available);
            self.set_personality(index, p, host.as_deref_mut());
        }
    }

    pub fn set_personality(&mut self, index: usize, personality: Personality, host: Option<&mut Host>) {
        if !self.available().contains(&personality) {
            return;
        }
        let mut agent = std::mem::replace(&mut self.agents[index], placeholder());
        agent.personality = personality;
        agent.goal = None;
        agent.goal_timer = 0.0;
        agent.home = agent.body.pos();
        if let (Some(host), Some(e)) = (host, agent.body.entity) {
            host.commands.entity(e).insert((super::crowd::CrowdColors(personality.look()), super::crowd::NeedsTint));
        }
        if personality == Personality::Racer {
            self.start_race(&mut agent, true);
        }
        self.agents[index] = agent;
    }

    pub fn cycle_personality(&mut self, index: usize, host: Option<&mut Host>) -> Personality {
        let available = self.available();
        let cur = available.iter().position(|p| *p == self.agents[index].personality).unwrap_or(0);
        let next = available[(cur + 1) % available.len()];
        self.set_personality(index, next, host);
        next
    }

    fn scatter_coins(&mut self, mut host: Option<&mut Host>) {
        let (center, radius) = (self.opts.center, self.opts.radius);
        let world = self.opts.world.clone();
        let mesh = host.as_deref_mut().map(|h| h.meshes.add(Cylinder::new(45.0 * SCALE, 12.0 * SCALE)));
        let mat = host.as_deref_mut().map(|h| h.unlit(Color::srgb(1.0, 0.85, 0.1)));
        for _ in 0..self.opts.coin_count {
            let Some(spot) = sample_floor(&world, center[0], center[2], radius, &mut self.rng) else { continue };
            let pos = [spot[0], spot[1] + 70.0, spot[2]];
            let entity = match (host.as_deref_mut(), &mesh, &mat) {
                (Some(h), Some(mesh), Some(mat)) => Some(h.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(to_bevy(pos))))),
                _ => None,
            };
            self.coins.push(Coin { entity, pos, active: true, respawn: 0, fall_vel: 0.0, hot: 0 });
        }
    }

    fn start_race(&self, agent: &mut CrowdAgent, nearest: bool) {
        let Some(route) = self.route.as_ref() else { return };
        let mut best = 0;
        if nearest {
            let mut distance = f64::INFINITY;
            let p = agent.body.pos();
            for (index, point) in route.iter().enumerate() {
                let d = (point[0] - p[0]).hypot((point[1] - p[1]) * 2.0).hypot(point[2] - p[2]);
                if d < distance && index + 2 < route.len() {
                    distance = d;
                    best = index;
                }
            }
        }
        agent.waypoint = best;
        agent.start_tick = self.tick;
    }

    /// Advance the crowd by one game tick, scaled by `speed`.
    /// Sounds/particles raised since the last call (`playNear` in the TS), for the host to play.
    pub fn take_fx(&mut self) -> Vec<(&'static str, V3)> {
        std::mem::take(&mut self.fx)
    }

    pub fn update(&mut self, leader: Option<&mut MarioRuntime>) {
        self.accumulator += self.speed;
        let mut steps = 0;
        let mut leader = leader;
        while self.accumulator >= 1.0 && steps < 4 {
            self.accumulator -= 1.0;
            self.step_once(leader.as_deref_mut());
            steps += 1;
        }
    }

    fn step_once(&mut self, mut leader: Option<&mut MarioRuntime>) {
        self.tick += 1;
        let mut agents = std::mem::take(&mut self.agents);
        let grid = Grid::build(&agents);
        let info = leader.as_deref().map(|l| LeaderInfo { pos: l.pos });
        if let Some(l) = leader.as_deref() {
            self.echo_leader_jump(&mut agents, l);
        }
        for i in 0..agents.len() {
            self.think(&mut agents[i], &grid, info.as_ref());
        }
        self.resolve_hits(&mut agents, &grid, leader.as_deref_mut());
        self.step_coins(&mut agents, &grid);
        self.agents = agents;
    }

    fn echo_leader_jump(&mut self, agents: &mut [CrowdAgent], leader: &MarioRuntime) {
        let air = leader.action & ACT_FLAG_AIR != 0;
        if air && !self.leader_was_air && leader.vel[1] > 20.0 {
            for a in agents.iter_mut().filter(|a| a.personality == Personality::Follower) {
                let p = a.body.pos();
                let distance = (p[0] - leader.pos[0]).hypot(p[2] - leader.pos[2]);
                if distance > 2500.0 {
                    continue;
                }
                // A jump wave ripples outward from the player.
                a.jump_at = self.tick as i64 + 2 + (distance / 70.0).floor() as i64;
            }
        }
        self.leader_was_air = air;
    }

    #[allow(clippy::cognitive_complexity)]
    fn think(&mut self, agent: &mut CrowdAgent, grid: &Grid, leader: Option<&LeaderInfo>) {
        let (pos, action, forward_vel) = (agent.body.pos(), agent.body.runtime.action, agent.body.runtime.forward_vel);
        let grounded = action & ACT_FLAG_AIR == 0;
        agent.goal_timer += 1.0;
        if agent.cooldown > 0 {
            agent.cooldown -= 1;
        }
        // Knockbacks and other intangible states play out untouched.
        if action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE) != 0 {
            agent.label = "Knocked back".into();
            agent.body.drive(&DriveInput::default());
            return;
        }
        if let Some(input) = agent.manual.clone() {
            agent.body.drive(&input);
            return;
        }
        if agent.label == "Dropping in" && grounded {
            agent.label.clear();
        }

        let (mut mag, mut jump, mut punch, mut crouch) = (64.0, false, false, false);
        if let Some(h) = self.hottest_coin(pos) {
            agent.goal = Some(h);
            agent.label = "Chasing a dropped coin".into();
        } else {
            match agent.personality {
                Personality::Racer => {
                    let route = self.route.clone().unwrap_or_default();
                    if agent.celebrate > 0 {
                        agent.celebrate -= 1;
                        agent.label = "Summit! Celebrating".into();
                        agent.goal = None;
                        if agent.celebrate % 24 == 0 && grounded {
                            jump = true;
                        }
                        if self.opts.single_lap {
                            agent.celebrate = agent.celebrate.max(2);
                        } else if agent.celebrate == 0 {
                            agent.laps += 1;
                            let world = self.opts.world.clone();
                            let start = sample_floor(&world, route[0][0], route[0][2], 260.0, &mut self.rng).unwrap_or(route[0]);
                            agent.body.teleport([start[0], start[1] + 300.0, start[2]], None);
                            agent.waypoint = 1;
                            agent.start_tick = self.tick;
                        }
                    } else {
                        let target = route[agent.waypoint.min(route.len() - 1)];
                        agent.goal = Some(target);
                        let (dx, dz) = (target[0] - pos[0], target[2] - pos[2]);
                        // Jump where a runner would: a checkpoint up ahead, or a ledge, a step or a gap just in front
                        // (without this a racer only jumped once it was stuck against a wall, and needed four minutes).
                        if grounded && agent.cooldown == 0 && agent.unstick == 0 {
                            let up = target[1] > pos[1] + 120.0 && dx.hypot(dz) < 500.0;
                            let yaw = atan2s(dz, dx) as f64;
                            let look = (forward_vel * 1.6).max(50.0);
                            let (px, pz) = (pos[0] + sins(yaw) * look, pos[2] + coss(yaw) * look);
                            let f = self.opts.world.find_floor(px, pos[1] + 700.0, pz);
                            let ledge = forward_vel > 8.0 && (f.surface.is_none() || f.height > pos[1] + 60.0 || f.height < pos[1] - 100.0);
                            if up || ledge {
                                jump = true;
                                agent.hold_a = 14;
                                agent.cooldown = 12;
                            }
                        }
                        if dx.hypot(dz) < 240.0 && (target[1] - pos[1]).abs() < 450.0 {
                            agent.waypoint += 1;
                            agent.recoveries = 0;
                            agent.stuck = 0;
                            // Respawn on the last checkpoint when the race knocks a Mario off the mountain.
                            let yaw = agent.body.runtime.face_angle[1];
                            agent.body.runtime.spawn = SpawnPoint { action: None, pos: target, face_yaw: yaw };
                            if agent.waypoint >= route.len() {
                                agent.celebrate = 90;
                                let lap = (self.tick - agent.start_tick) as f64 / 30.0;
                                agent.best_lap = agent.best_lap.min(lap);
                                self.stats.summits += 1;
                                self.stats.best_lap = Some(self.stats.best_lap.map_or(lap, |b| b.min(lap)));
                            }
                        }
                        agent.label = format!("Racing - checkpoint {}/{}", (agent.waypoint + 1).min(route.len()), route.len());
                    }
                }
                Personality::Hunter => {
                    if let Some(ci) = self.nearest_coin(pos, 7000.0, Some(agent)) {
                        let cp = self.coins[ci].pos;
                        agent.goal = Some(cp);
                        agent.goal_coin = Some(ci);
                        let d = (cp[0] - pos[0]).hypot(cp[2] - pos[2]);
                        agent.label = format!("Hunting a coin - {} m", (d / 100.0).round());
                        // Leap for coins that float above head height.
                        if d < 160.0 && cp[1] - pos[1] > 110.0 && grounded {
                            jump = true;
                        }
                    } else {
                        self.explore(agent, "Looking for coins");
                    }
                }
                Personality::Follower => match leader {
                    None => self.explore(agent, "Wandering"),
                    Some(l) => {
                        let angle = agent.id as f64 * 2.39996;
                        let radius = 280.0 + (agent.id % 7) as f64 * 110.0;
                        let goal = [l.pos[0] + angle.cos() * radius, l.pos[1], l.pos[2] + angle.sin() * radius];
                        agent.goal = Some(goal);
                        let d = (goal[0] - pos[0]).hypot(goal[2] - pos[2]);
                        mag = if d < 60.0 { 0.0 } else if d < 400.0 { 24.0 + d / 10.0 } else { 64.0 };
                        agent.label = if d > 3000.0 { "Catching up with you" } else { "Following you" }.into();
                        if agent.jump_at == self.tick as i64 && grounded {
                            jump = true;
                            agent.hold_a = 10;
                        }
                    }
                },
                Personality::Brawler => {
                    let mut intruder: Option<(V3, f64)> = None;
                    for (other, distance) in grid.near(pos, 900.0) {
                        if other.index_is(agent.body.pos()) || other.personality == Personality::Brawler {
                            continue;
                        }
                        if (other.pos[0] - agent.home[0]).hypot(other.pos[2] - agent.home[2]) > 900.0 {
                            continue;
                        }
                        if intruder.map_or(true, |(_, d)| distance < d) {
                            intruder = Some((other.pos, distance));
                        }
                    }
                    if let Some(l) = leader {
                        if (l.pos[0] - agent.home[0]).hypot(l.pos[2] - agent.home[2]) < 700.0 {
                            let distance = (l.pos[0] - pos[0]).hypot(l.pos[2] - pos[2]);
                            if intruder.map_or(true, |(_, d)| distance < d) {
                                intruder = Some((l.pos, distance));
                            }
                        }
                    }
                    if let Some((ip, distance)) = intruder {
                        agent.goal = Some(ip);
                        agent.label = "Chasing an intruder".into();
                        if distance < 150.0 && grounded && agent.cooldown == 0 {
                            punch = true;
                            agent.cooldown = 20;
                        }
                    } else {
                        let reached = agent.goal.map_or(true, |g| (g[0] - pos[0]).hypot(g[2] - pos[2]) < 120.0);
                        if agent.goal.is_none() || agent.goal_timer > 150.0 || reached {
                            let world = self.opts.world.clone();
                            agent.goal = Some(sample_floor(&world, agent.home[0], agent.home[2], 600.0, &mut self.rng).unwrap_or(agent.home));
                            agent.goal_timer = 0.0;
                        }
                        mag = 34.0;
                        agent.label = "Guarding turf".into();
                    }
                }
                Personality::Explorer => {
                    self.explore(agent, "Exploring");
                    // Show off: long jumps on long straights, chained triple jumps.
                    let far = agent.goal.map_or(false, |g| (g[0] - pos[0]).hypot(g[2] - pos[2]) > 1400.0);
                    if grounded && agent.flair == Flair::None && forward_vel > 26.0 && far && self.rng.next() < 0.02 {
                        agent.flair = Flair::LongJump;
                        crouch = true;
                    } else if agent.flair == Flair::LongJump {
                        jump = true;
                        agent.hold_a = 6;
                        agent.flair = Flair::None;
                    } else if grounded && agent.body.landed_ticks == 1 && agent.combo > 0 && agent.combo < 3 {
                        jump = true;
                        agent.hold_a = 12;
                        agent.combo += 1;
                    } else if grounded && forward_vel > 22.0 && agent.combo == 0 && self.rng.next() < 0.012 {
                        jump = true;
                        agent.hold_a = 12;
                        agent.combo = 1;
                    } else if grounded && agent.body.landed_ticks > 3 {
                        agent.combo = 0;
                    }
                }
            }
        }

        let mut yaw: Option<f64> = None;
        if let Some(goal) = agent.goal {
            let (mut vx, mut vz) = (goal[0] - pos[0], goal[2] - pos[2]);
            let distance = vx.hypot(vz);
            if distance < 40.0 && agent.personality != Personality::Racer {
                mag = 0.0;
            }
            if distance > 0.01 {
                vx /= distance;
                vz /= distance;
            }
            // Crowd avoidance: steer away from Marios that are too close.
            let (mut sx, mut sz) = (0.0, 0.0);
            for (other, d) in grid.near(pos, SEPARATION) {
                if other.index_is(pos) || d < 0.01 {
                    continue;
                }
                let push = (SEPARATION - d) / SEPARATION;
                sx += (pos[0] - other.pos[0]) / d * push;
                sz += (pos[2] - other.pos[2]) / d * push;
            }
            vx += sx * 1.4;
            vz += sz * 1.4;
            if mag > 0.0 {
                yaw = Some(atan2s(vz, vx) as f64);
            }
        }

        // Unsticking: jump at walls, then try another heading, then give up and respawn.
        let wall = agent.body.runtime.wall.is_some();
        if agent.unstick > 0 {
            agent.unstick -= 1;
            yaw = Some(agent.unstick_yaw);
            mag = 64.0;
            agent.label = "Finding another way".into();
            if agent.unstick == 12 && grounded {
                jump = true;
                agent.hold_a = 12;
            }
        } else if yaw.is_some() && grounded && wall && forward_vel < 12.0 && agent.cooldown == 0 {
            jump = true;
            agent.hold_a = 14;
            agent.cooldown = 12;
        }
        // Goal progress: oscillating in place (jumping at a wall) still counts as stuck.
        if let Some(goal) = agent.goal {
            if matches!(agent.personality, Personality::Racer | Personality::Hunter | Personality::Explorer) && agent.celebrate == 0 {
                if agent.progress_goal != Some(goal) {
                    agent.progress_goal = Some(goal);
                    agent.best_distance = f64::INFINITY;
                    agent.no_progress = 0;
                    agent.stalls = 0;
                }
                let d = (goal[0] - pos[0]).hypot((goal[1] - pos[1]) * 0.5).hypot(goal[2] - pos[2]);
                if d < agent.best_distance - 80.0 {
                    agent.best_distance = d;
                    agent.no_progress = 0;
                } else {
                    agent.no_progress += 1;
                    if agent.no_progress > 150 && agent.unstick == 0 {
                        agent.no_progress = 0;
                        agent.best_distance = d;
                        agent.stalls += 1;
                        if agent.stalls >= 3 {
                            agent.stalls = 0;
                            self.recover(agent);
                            return;
                        }
                        if let Some(ci) = agent.goal_coin {
                            agent.avoid.insert(ci, self.tick + 30 * 25);
                        }
                        if agent.personality == Personality::Explorer {
                            agent.goal_timer = 1e9;
                        }
                        agent.unstick = 28;
                        let side = if self.rng.next() < 0.5 { 1.0 } else { -1.0 };
                        agent.unstick_yaw = jumpman_sim::math::to_s16(atan2s(goal[2] - pos[2], goal[0] - pos[0]) as f64 + 0x4000 as f64 * side) as f64;
                    }
                }
            }
        }
        agent.stuck_ticks += 1;
        if agent.stuck_ticks >= 24 {
            agent.stuck_ticks = 0;
            let moved = (pos[0] - agent.stuck_check[0]).hypot(pos[2] - agent.stuck_check[2]);
            agent.stuck_check = pos;
            if yaw.is_some() && mag > 30.0 && moved < 50.0 && agent.celebrate == 0 {
                agent.stuck += 1;
                if agent.stuck == 1 && grounded {
                    jump = true;
                    agent.hold_a = 14;
                } else if agent.stuck >= 2 && agent.stuck < 7 {
                    // Give up on a goal that keeps blocking us; pick something else for a while.
                    if agent.stuck >= 3 && agent.goal.is_some() {
                        if let Some(ci) = agent.goal_coin {
                            agent.avoid.insert(ci, self.tick + 30 * 25);
                        }
                        if matches!(agent.personality, Personality::Explorer | Personality::Brawler) {
                            agent.goal_timer = 1e9;
                        }
                    }
                    agent.unstick = 28;
                    let side = if self.rng.next() < 0.5 { 1.0 } else { -1.0 };
                    agent.unstick_yaw = jumpman_sim::math::to_s16(yaw.unwrap() + 0x4000 as f64 * side + ((self.rng.next() - 0.5) * 0x3000 as f64).floor()) as f64;
                } else if agent.stuck >= 7 {
                    self.recover(agent);
                    return;
                }
            } else {
                agent.stuck = (agent.stuck - 1).max(0);
            }
        }

        if agent.hold_a > 0 {
            agent.hold_a -= 1;
        }
        agent.body.drive(&DriveInput { yaw, mag: Some(mag), jump, hold_a: agent.hold_a > 0, punch, crouch, hold_z: false });
        if agent.body.runtime.respawns > 0.0 {
            agent.body.runtime.respawns = 0.0;
            agent.body.sync_previous();
        }
    }

    fn explore(&mut self, agent: &mut CrowdAgent, label: &str) {
        let pos = agent.body.pos();
        let reached = agent.goal.map_or(false, |g| (g[0] - pos[0]).hypot(g[2] - pos[2]) < 180.0);
        if agent.goal.is_none() || reached || agent.goal_timer > 30.0 * 12.0 {
            let roll = self.rng.next();
            let world = self.opts.world.clone();
            agent.goal = match self.route.as_ref() {
                Some(route) if roll < 0.3 && !route.is_empty() => Some(route[(self.rng.next() * route.len() as f64).floor() as usize % route.len()]),
                _ => sample_floor(&world, pos[0], pos[2], 3200.0, &mut self.rng).or_else(|| sample_floor(&world, self.opts.center[0], self.opts.center[2], self.opts.radius, &mut self.rng)),
            };
            agent.goal_coin = None;
            agent.goal_timer = 0.0;
        }
        agent.label = label.into();
    }

    fn recover(&mut self, agent: &mut CrowdAgent) {
        let mut anchor = agent.home;
        if let (Personality::Racer, Some(route)) = (agent.personality, self.route.as_ref()) {
            // Twice stuck on the same checkpoint: warp onto it instead of retrying forever.
            agent.recoveries += 1;
            if agent.recoveries >= 2 {
                agent.recoveries = 0;
                anchor = route[agent.waypoint.min(route.len() - 1)];
            } else {
                anchor = route[agent.waypoint.saturating_sub(1)];
            }
        }
        let world = self.opts.world.clone();
        let spot = sample_floor(&world, anchor[0], anchor[2], 300.0, &mut self.rng).unwrap_or(anchor);
        agent.body.teleport([spot[0], spot[1] + 200.0, spot[2]], None);
        agent.stuck = 0;
        agent.goal = None;
        agent.label = "Warped back".into();
    }

    fn hottest_coin(&self, pos: V3) -> Option<V3> {
        let mut best = None;
        let mut distance = 4200.0;
        for coin in &self.coins {
            if !coin.active || coin.hot <= 0 {
                continue;
            }
            let d = (coin.pos[0] - pos[0]).hypot(coin.pos[2] - pos[2]);
            if d < distance {
                distance = d;
                best = Some(coin.pos);
            }
        }
        best
    }

    /// Drop a coin from the sky; every Mario nearby races for it.
    pub fn drop_coin(&mut self, host: &mut Host, at: V3) -> bool {
        let floor = self.opts.world.find_floor(at[0], at[1] + 400.0, at[2]);
        if floor.surface.is_none() {
            return false;
        }
        let mesh = host.meshes.add(Cylinder::new(45.0 * SCALE, 12.0 * SCALE));
        let mat = host.unlit(Color::srgb(1.0, 0.85, 0.1));
        let pos = [at[0], floor.height + 1400.0, at[2]];
        let entity = Some(host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(to_bevy(pos)))));
        self.coins.push(Coin { entity, pos, active: true, respawn: -1, fall_vel: 0.0, hot: 30 * 20 });
        true
    }

    fn nearest_coin(&mut self, pos: V3, range: f64, agent: Option<&mut CrowdAgent>) -> Option<usize> {
        let mut best = None;
        let mut distance = range;
        let tick = self.tick;
        let mut avoid = agent;
        for (i, coin) in self.coins.iter().enumerate() {
            if !coin.active {
                continue;
            }
            if let Some(a) = avoid.as_deref_mut() {
                if let Some(&until) = a.avoid.get(&i) {
                    if until > tick {
                        continue;
                    }
                    a.avoid.remove(&i);
                }
            }
            let d = (coin.pos[0] - pos[0]).hypot((coin.pos[1] - pos[1]) * 1.5).hypot(coin.pos[2] - pos[2]);
            if d < distance {
                distance = d;
                best = Some(i);
            }
        }
        best
    }

    fn step_coins(&mut self, agents: &mut [CrowdAgent], grid: &Grid) {
        let (center, radius) = (self.opts.center, self.opts.radius);
        let world = self.opts.world.clone();
        for ci in 0..self.coins.len() {
            if !self.coins[ci].active {
                if self.coins[ci].respawn > 0 {
                    self.coins[ci].respawn -= 1;
                    if self.coins[ci].respawn == 0 {
                        match sample_floor(&world, center[0], center[2], radius, &mut self.rng) {
                            Some(spot) => {
                                self.coins[ci].pos = [spot[0], spot[1] + 70.0, spot[2]];
                                self.coins[ci].active = true;
                            }
                            None => self.coins[ci].respawn = 30,
                        }
                    }
                }
                continue;
            }
            if self.coins[ci].hot > 0 {
                self.coins[ci].hot -= 1;
            }
            if self.coins[ci].respawn < 0 {
                let c = &mut self.coins[ci];
                let rest = world.find_floor(c.pos[0], c.pos[1] + 50.0, c.pos[2]).height + 70.0;
                if c.pos[1] > rest {
                    c.fall_vel = (c.fall_vel - 3.0).max(-60.0);
                    c.pos[1] = rest.max(c.pos[1] + c.fall_vel);
                    if c.pos[1] == rest && c.hot > 0 && self.fx.len() < 24 {
                        self.fx.push(("coin_bounce", c.pos));
                    }
                }
            }
            let cpos = self.coins[ci].pos;
            // Any Mario that touches a coin takes it.
            if let Some((other, _)) = grid.near(cpos, 90.0).into_iter().find(|(s, _)| (s.pos[1] + 60.0 - cpos[1]).abs() <= 140.0) {
                self.coins[ci].active = false;
                if self.coins[ci].respawn >= 0 {
                    self.coins[ci].respawn = 30 * 18;
                }
                agents[other.index].coins += 1;
                self.stats.coins += 1;
                if self.fx.len() < 24 {
                    self.fx.push(("coin", cpos));
                }
            }
        }
    }

    /// Brawler punches, player punches and stomps all turn into real knockbacks.
    fn resolve_hits(&mut self, agents: &mut [CrowdAgent], grid: &Grid, mut leader: Option<&mut MarioRuntime>) {
        let attacking = |m: &MarioRuntime| m.flags & (MARIO_PUNCHING | MARIO_KICKING) != 0 || m.action & ACT_FLAG_ATTACKING != 0;
        let hit_from = |attacker_pos: V3, attacker_yaw: f64, victim_pos: V3, reach: f64| {
            let (dx, dz) = (victim_pos[0] - attacker_pos[0], victim_pos[2] - attacker_pos[2]);
            let d = dx.hypot(dz);
            if d > reach || (victim_pos[1] - attacker_pos[1]).abs() > 140.0 {
                return false;
            }
            let facing = (dx * sins(attacker_yaw) + dz * coss(attacker_yaw)) / d.max(1.0);
            d < 60.0 || facing > 0.25
        };
        for ai in 0..agents.len() {
            let (m_pos, m_yaw, is_attacking) = {
                let m = &agents[ai].body.runtime;
                (m.pos, m.face_angle[1], attacking(m))
            };
            if !is_attacking {
                continue;
            }
            for (other, _) in grid.near(m_pos, 170.0) {
                if other.index == ai || !hit_from(m_pos, m_yaw, agents[other.index].body.runtime.pos, 170.0) {
                    continue;
                }
                if agents[other.index].body.runtime.take_damage(2.0, Some(m_pos)) {
                    agents[ai].hits += 1;
                    self.stats.hits += 1;
                    if self.fx.len() < 24 {
                        self.fx.push(("bonk", agents[other.index].body.runtime.pos));
                    }
                }
            }
            if let Some(l) = leader.as_deref_mut() {
                if hit_from(m_pos, m_yaw, l.pos, 160.0) && l.take_damage(1.0, Some(m_pos)) {
                    agents[ai].hits += 1;
                    l.hurt_counter = 0.0;
                }
            }
        }
        let Some(l) = leader else { return };
        if attacking(l) {
            for (other, _) in grid.near(l.pos, 180.0) {
                if hit_from(l.pos, l.face_angle[1], agents[other.index].body.runtime.pos, 180.0) {
                    agents[other.index].body.runtime.take_damage(2.0, Some(l.pos));
                }
            }
        }
        // Stomp a Mario's head to bounce off it.
        if l.vel[1] < 0.0 && l.action & ACT_FLAG_AIR != 0 {
            for (other, _) in grid.near(l.pos, 80.0) {
                let head = other.pos[1] + 150.0;
                if l.pos[1] < head - 40.0 || l.pos[1] > head + 30.0 {
                    continue;
                }
                l.vel[1] = 52.0;
                l.pos[1] = head;
                l.set_action(ACT_FREEFALL, 0.0);
                l.vel[1] = 52.0;
                agents[other.index].body.runtime.squish_timer = 24.0;
            }
        }
    }

    /// Per-frame: push interpolated transforms/clips to the entities.
    pub fn render(&mut self, host: &mut Host, alpha: f64, dt: f64, eye: V3) {
        for agent in &mut self.agents {
            let p = agent.body.pos();
            let d = ((eye[0] - p[0]).powi(2) + (eye[1] - p[1]).powi(2) + (eye[2] - p[2]).powi(2)).sqrt();
            agent.body.render(host.commands, alpha, dt, d);
        }
        let spin = Quat::from_rotation_y(host.time() as f32 * 4.0) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        for coin in &self.coins {
            let Some(e) = coin.entity else { continue };
            let bob = (self.tick as f64 * 0.15 + coin.pos[0]).sin() * 4.0;
            host.commands
                .entity(e)
                .insert((Transform::from_translation(to_bevy([coin.pos[0], coin.pos[1] + bob, coin.pos[2]])).with_rotation(spin), if coin.active { Visibility::Inherited } else { Visibility::Hidden }));
        }
    }

    /// Screen-space pick by head position (cheap and forgiving for small targets): the nearest
    /// agent to the cursor, given each agent's projected screen position via `project`.
    pub fn pick(&self, cursor: Vec2, radius: f32, alpha: f64, mut project: impl FnMut(V3) -> Option<Vec2>) -> Option<u32> {
        let mut best = None;
        let mut best_d = radius;
        for agent in &self.agents {
            let (p, _) = agent.body.interpolated(alpha);
            for h in [40.0, 110.0] {
                let Some(s) = project([p[0], p[1] + h, p[2]]) else { continue };
                let d = s.distance(cursor);
                if d < best_d {
                    best_d = d;
                    best = Some(agent.id);
                }
            }
        }
        best
    }

    pub fn describe(&self, agent: &CrowdAgent) -> CrowdAgentDebug {
        let m = &agent.body.runtime;
        CrowdAgentDebug {
            id: agent.id,
            name: agent.name.clone(),
            personality: agent.personality,
            label: if agent.label.is_empty() { if m.action & ACT_FLAG_AIR != 0 { "Airborne".into() } else { "Idle".into() } } else { agent.label.clone() },
            action: crate::action_names::action_name(m.action).map_or_else(|| format!("0x{:x}", m.action & 0x1ff), str::to_string),
            pos: m.pos,
            speed: m.forward_vel,
            coins: agent.coins,
            laps: agent.laps,
            hits: agent.hits,
            best_lap: agent.best_lap.is_finite().then_some(agent.best_lap),
        }
    }

    pub fn counts(&self) -> HashMap<Personality, usize> {
        let mut c = HashMap::new();
        for a in &self.agents {
            *c.entry(a.personality).or_insert(0) += 1;
        }
        c
    }

    fn clear_agents(&mut self, host: Option<&mut Host>) {
        if let Some(host) = host {
            for a in &self.agents {
                if let Some(e) = a.body.entity {
                    host.commands.entity(e).despawn();
                }
            }
        }
        self.agents.clear();
        self.selected = None;
        self.stats = CrowdStats::default();
    }

    /// Despawn everything this crowd owns (the framework also despawns all `XpEntity`).
    pub fn dispose(&mut self, host: &mut Host) {
        self.clear_agents(Some(host));
        for c in self.coins.drain(..) {
            if let Some(e) = c.entity {
                host.commands.entity(e).despawn();
            }
        }
    }

    pub fn world(&self) -> &Arc<CollisionWorld> {
        &self.opts.world
    }
}

impl Snap {
    /// Whether this snapshot is the agent standing at `pos` (neighbour queries include self).
    fn index_is(&self, pos: V3) -> bool {
        self.pos == pos
    }
}

fn placeholder() -> CrowdAgent {
    let world = Arc::new(CollisionWorld::default());
    CrowdAgent {
        id: 0,
        name: String::new(),
        body: CrowdMario::new(world, SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, None),
        personality: Personality::Explorer,
        goal: None,
        goal_coin: None,
        label: String::new(),
        waypoint: 0,
        laps: 0,
        coins: 0,
        hits: 0,
        home: [0.0; 3],
        goal_timer: 0.0,
        celebrate: 0,
        hold_a: 0,
        jump_at: -1,
        cooldown: 0,
        stuck: 0,
        stuck_check: [0.0; 3],
        stuck_ticks: 0,
        recoveries: 0,
        best_distance: f64::INFINITY,
        no_progress: 0,
        stalls: 0,
        progress_goal: None,
        unstick: 0,
        unstick_yaw: 0.0,
        flair: Flair::None,
        combo: 0,
        start_tick: 0,
        best_lap: f64::INFINITY,
        avoid: HashMap::new(),
            manual: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::SurfaceOptions;

    fn ground() -> Arc<CollisionWorld> {
        let mut w = CollisionWorld::default();
        w.add_quad([-6000., 0., -6000.], [-6000., 0., 6000.], [6000., 0., 6000.], [6000., 0., -6000.], &SurfaceOptions::default());
        w.finalize();
        Arc::new(w)
    }

    fn crowd(mix: CrowdMix, count: usize, coins: usize, route: Option<Vec<V3>>) -> MarioCrowd {
        let opts = MarioCrowdOptions { world: ground(), route, has_leader: false, coin_count: coins, center: [0.0; 3], radius: 3000.0, seed: 0x5eed, single_lap: false };
        let mut c = MarioCrowd::new(opts, None, None);
        c.mix = mix;
        c.populate(count, [0.0; 3], 1600.0, None);
        c
    }

    #[test]
    fn personalities_interleave_by_mix() {
        let all = PERSONALITY_ORDER;
        let counts = |mix: CrowdMix| {
            let mut m: HashMap<Personality, usize> = HashMap::new();
            for i in 0..100 {
                *m.entry(personality_for(mix, i, &all)).or_default() += 1;
            }
            m
        };
        let mixed = counts(CrowdMix::Mixed);
        assert_eq!(mixed.values().sum::<usize>(), 100);
        assert_eq!(mixed.len(), 5);
        assert!((mixed[&Personality::Racer] as i32 - 30).abs() <= 3);
        assert_eq!(counts(CrowdMix::Gold)[&Personality::Hunter], 100);
        // Unavailable personalities are skipped.
        let no_racers: Vec<_> = all.iter().copied().filter(|p| *p != Personality::Racer).collect();
        assert_ne!(personality_for(CrowdMix::Mixed, 0, &no_racers), Personality::Racer);
        assert_eq!(personality_for(CrowdMix::Race, 0, &no_racers), Personality::Hunter);
    }

    #[test]
    fn rng_is_xorshift32() {
        let mut r = Rng::new(1);
        // 1 -> xorshift32 first output.
        let v = r.next();
        assert_eq!((v * 4294967296.0) as u32, 270369);
    }

    #[test]
    fn a_hundred_marios_run_land_and_collect_coins() {
        let mut c = crowd(CrowdMix::Gold, 100, 40, None);
        assert_eq!(c.agents.len(), 100);
        for _ in 0..300 {
            c.update(None);
        }
        // Everyone has landed on the ground and is alive and well.
        for a in &c.agents {
            let m = &a.body.runtime;
            assert!(m.pos[1].is_finite() && m.pos[1] >= -1.0 && m.pos[1] < 200.0, "{} {:?}", a.name, m.pos);
        }
        assert!(c.stats.coins > 0, "hunters should pick up coins");
        let moved = c.agents.iter().filter(|a| (a.body.pos()[0] - a.home[0]).hypot(a.body.pos()[2] - a.home[2]) > 100.0).count();
        assert!(moved > 50, "only {moved} moved");
    }

    #[test]
    fn racers_follow_the_route_to_the_summit() {
        let route = vec![[-2000., 0., 0.], [-1000., 0., 0.], [0., 0., 0.], [1000., 0., 0.], [2000., 0., 0.]];
        let opts = MarioCrowdOptions { world: ground(), route: Some(route.clone()), has_leader: false, coin_count: 0, center: [0.0; 3], radius: 3000.0, seed: 7, single_lap: true };
        let mut c = MarioCrowd::new(opts, None, None);
        c.mix = CrowdMix::Race;
        c.populate_at(&[[-2000., 0., 0.], [-2000., 0., 150.]], Personality::Racer, 0x4000 as f64, None);
        for _ in 0..600 {
            c.update(None);
        }
        assert!(c.stats.summits >= 1, "no racer reached the summit");
    }

    #[test]
    fn brawlers_punch_intruders() {
        let opts = MarioCrowdOptions { world: ground(), route: None, has_leader: false, coin_count: 0, center: [0.0; 3], radius: 3000.0, seed: 3, single_lap: false };
        let mut c = MarioCrowd::new(opts, None, None);
        c.populate_at(&[[0., 0., 0.], [0., 0., 110.]], Personality::Brawler, 0.0, None);
        // Settle on the ground, then the first Mario throws a punch at the one in front of it.
        for a in c.agents.iter_mut() {
            for _ in 0..5 {
                a.body.drive(&DriveInput::default());
            }
        }
        c.agents[0].body.drive(&DriveInput { punch: true, ..Default::default() });
        assert!(c.agents[0].body.runtime.flags & (MARIO_PUNCHING | MARIO_KICKING) != 0 || c.agents[0].body.runtime.action & ACT_FLAG_ATTACKING != 0, "action {:x}", c.agents[0].body.runtime.action);
        let mut agents = std::mem::take(&mut c.agents);
        let grid = Grid::build(&agents);
        c.resolve_hits(&mut agents, &grid, None);
        assert!(agents[0].hits > 0 && c.stats.hits > 0, "punch should connect");
        assert!(agents[1].body.runtime.action & ACT_FLAG_INTANGIBLE != 0 || agents[1].body.runtime.hurt_counter >= 0.0);
    }
}
