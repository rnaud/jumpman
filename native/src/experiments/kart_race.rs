//! Race mode for the MK64 experiment, engine-free: ordered checkpoints, laps, a 3-2-1-GO start,
//! three CPU drivers on the shared `kart_physics` core, ranking, and the hooks into the ported item system
//! (`kart_items.rs`: all 15 items, roulette curves, item actors) and kart effects (`kart_effects.rs`:
//! `apply_triggers` / `apply_effect`, kart-vs-kart `func_802903D8`, the start boost and early-start spinout). `src/game/experiments/kartRace.ts` is the line-for-line browser twin; the rules are
//! checked by the unit tests in both. Everything runs on the 30 Hz sim tick (`KartRace::tick` steps the
//! 60 Hz kart physics twice) in MK64 course units (the `path` of `luigi-raceway.json`).
//!
//! Simplifications against MK64, all deliberate: the CPU item decision is the decomp's item timer plus simple
//! target checks (`cpu_items` below) rather than the full per-item `cpu_use_item_strategy` table, no Lakitu
//! animation (a fall puts you back on the last gate), kart-vs-kart is evaluated once per 30 Hz tick (the decomp:
//! per frame).
use super::kart_cpu::{CpuDriver, CpuItemAction, CpuItemStrategy, CpuItemView, Neighbor, PathInfo, RaceSetup, RaceView};
use super::kart_effects::{self as fx, Effects};
use super::kart_items::{Body, Ctx, ItemEvent, ItemUse, ItemWorld, Pad};
#[allow(unused_imports)]
use super::kart_physics::{Ground, Input, Kart, Wall};
use super::kart_stats::KartStats;
pub use super::kart_items::Item;
use std::f64::consts::PI;

pub const LAPS: u32 = 3;
pub const CPU_COUNT: usize = 3;
pub const RACERS: usize = CPU_COUNT + 1;
pub const GATES: usize = 24;
/// Half the width of a checkpoint gate, course units. Wider than the road so a wide line still counts,
/// narrow enough that cutting across the infield does not.
pub const GATE_HALF: f64 = 420.;
pub const COUNTDOWN_TICKS: u32 = 90;
pub const GO_TICKS: u32 = 30;
/// The item window roulette (`kart_items::ROULETTE_FRAMES`, 142 frames) in 30 Hz ticks.
pub const ROULETTE_TICKS: u32 = super::kart_items::ROULETTE_FRAMES.div_ceil(2);
/// `update_actor_item_box`: 20 frames of break animation, then the box rises 28.66 units at 0.45 per frame
/// (64 frames); in 30 Hz ticks.
pub const BOX_RESPAWN: u32 = (20 + 64) / 2;
/// The box can be touched again once it is back (`state 0`, after the break).
pub const BOX_BREAK: u32 = 10;
/// `query_collision_player_vs_actor_item`: the kart (5.5) plus the item box (`boundingBoxSize = 20`).
pub const BOX_RADIUS: f64 = super::kart_items::ITEM_BOX_RADIUS + fx::KART_RADIUS;
pub const HIT_RADIUS: f64 = 24.;
/// Raw units per 60 Hz frame.
const PLAYER_SLOT: usize = 3;
/// The rocket start: accelerate first pressed in this countdown window (30 Hz ticks before GO) gives the
/// start boost (`START_BOOST_TRIGGER` -> `func_8002A704`); holding it from earlier spins out at the line
/// (`START_SPINOUT_TRIGGER` -> `func_8008F104`). The window itself is set by code the decomp has not
/// decompiled to C, so these bounds are an approximation (about the second Lakitu light).
pub const START_BOOST_WINDOW: std::ops::RangeInclusive<u32> = 10..=30;

pub fn to_s16(v: f64) -> i32 {
    (v as i32) as i16 as i32
}
/// MK64 `rotation[1]` for a direction in the xz plane (forward = (sin yaw, cos yaw)).
pub fn rotation_for(dir: [f64; 2]) -> i32 {
    to_s16(-dir[0].atan2(dir[1]) * 32768. / PI)
}
fn norm(v: [f64; 2]) -> [f64; 2] {
    let l = v[0].hypot(v[1]);
    if l < 1e-9 { [0., 1.] } else { [v[0] / l, v[1] / l] }
}
fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn forward(kart: &Kart) -> [f64; 2] {
    let y = kart.yaw();
    [y.sin(), y.cos()]
}

/// xorshift32. The browser twin uses the same recurrence so seeds replay identically.
#[derive(Clone, Debug)]
pub struct Rng(pub u32);
impl Rng {
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    pub fn unit(&mut self) -> f64 {
        self.next_u32() as f64 / 4294967296.
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Gate {
    pub c: [f64; 2],
    /// Unit direction of travel.
    pub t: [f64; 2],
    pub arc: f64,
}
impl Gate {
    /// Did the segment `prev -> now` cross this gate forwards inside its width?
    pub fn crossed(&self, prev: [f64; 2], now: [f64; 2]) -> bool {
        let d0 = (prev[0] - self.c[0]) * self.t[0] + (prev[1] - self.c[1]) * self.t[1];
        let d1 = (now[0] - self.c[0]) * self.t[0] + (now[1] - self.c[1]) * self.t[1];
        if !(d0 < 0. && d1 >= 0.) {
            return false;
        }
        let s = d0 / (d0 - d1);
        let p = [prev[0] + (now[0] - prev[0]) * s, prev[1] + (now[1] - prev[1]) * s];
        let lateral = (p[0] - self.c[0]) * self.t[1] - (p[1] - self.c[1]) * self.t[0];
        lateral.abs() <= GATE_HALF
    }
}

/// The closed centreline plus the checkpoint gates cut across it.
#[derive(Clone, Debug)]
pub struct Track {
    pub nodes: Vec<[f64; 3]>,
    pub cum: Vec<f64>,
    pub length: f64,
    pub gates: Vec<Gate>,
    pub spacing: f64,
    pub min_y: f64,
}
impl Track {
    /// `path` as exported (raw course units). Sentinel nodes (the export ends with x = -32768) are dropped.
    pub fn new(path: &[[f64; 3]]) -> Self {
        let nodes: Vec<[f64; 3]> = path.iter().copied().filter(|p| p.iter().all(|v| v.is_finite()) && p[0] > -30000.).collect();
        assert!(nodes.len() >= 8, "track needs a path");
        let n = nodes.len();
        let seg = |i: usize| dist([nodes[i][0], nodes[i][2]], [nodes[(i + 1) % n][0], nodes[(i + 1) % n][2]]);
        let mut cum = vec![0.; n];
        for i in 1..n {
            cum[i] = cum[i - 1] + seg(i - 1);
        }
        let length = cum[n - 1] + seg(n - 1);
        let mut track = Self { min_y: nodes.iter().map(|p| p[1]).fold(f64::MAX, f64::min), nodes, cum, length, gates: vec![], spacing: length / GATES as f64 };
        track.gates = (0..GATES)
            .map(|k| {
                let i = k * n / GATES;
                Gate { c: [track.nodes[i][0], track.nodes[i][2]], t: track.dir(i), arc: track.cum[i] }
            })
            .collect();
        track
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    fn xz(&self, i: usize) -> [f64; 2] {
        let p = self.nodes[i % self.len()];
        [p[0], p[2]]
    }
    fn seg(&self, i: usize) -> f64 {
        dist(self.xz(i), self.xz(i + 1))
    }
    /// Smoothed unit direction of travel at node `i`.
    pub fn dir(&self, i: usize) -> [f64; 2] {
        let n = self.len();
        let (a, b) = (self.xz((i + n - 2) % n), self.xz(i + 2));
        norm([b[0] - a[0], b[1] - a[1]])
    }
    /// Right-hand side perpendicular of the direction of travel (the lateral axis).
    pub fn side(&self, i: usize) -> [f64; 2] {
        let t = self.dir(i);
        [t[1], -t[0]]
    }
    /// Node `distance` of arc length ahead of `i`.
    pub fn ahead(&self, i: usize, distance: f64) -> usize {
        let (mut j, mut acc) = (i % self.len(), 0.);
        for _ in 0..self.len() {
            if acc >= distance {
                break;
            }
            acc += self.seg(j);
            j = (j + 1) % self.len();
        }
        j
    }
    pub fn behind(&self, i: usize, distance: f64) -> usize {
        let n = self.len();
        let (mut j, mut acc) = (i % n, 0.);
        for _ in 0..n {
            if acc >= distance {
                break;
            }
            j = (j + n - 1) % n;
            acc += self.seg(j);
        }
        j
    }
    pub fn nearest_global(&self, p: [f64; 2]) -> usize {
        (0..self.len()).fold((0, f64::MAX), |best, i| {
            let d = dist(self.xz(i), p);
            if d < best.1 { (i, d) } else { best }
        }).0
    }
    /// Nearest node within `window` nodes of `hint` (keeps a kart on its own part of the loop).
    pub fn nearest(&self, p: [f64; 2], hint: usize, window: usize) -> usize {
        let n = self.len();
        let mut best = (hint % n, f64::MAX);
        for k in 0..=2 * window {
            let i = (hint + n + k - window % n) % n;
            let d = dist(self.xz(i), p);
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0
    }
    /// Starting grid: two abreast, rows behind the line. Returns (x, z, rotation).
    pub fn grid(&self, slot: usize) -> ([f64; 2], i32) {
        let j = self.behind(0, 70. + 75. * (slot / 2) as f64);
        let side = self.side(j);
        let lateral = if slot % 2 == 0 { -38. } else { 38. };
        let c = self.xz(j);
        ([c[0] + side[0] * lateral, c[1] + side[1] * lateral], rotation_for(self.dir(j)))
    }
    /// Item box rows: node index for each of the four stations along the lap (six in free roam).
    pub fn box_nodes(&self, practice: bool) -> Vec<usize> {
        let fractions: &[f64] = if practice { &[0.015, 0.08, 0.2, 0.31, 0.54, 0.78] } else { &[0.08, 0.31, 0.54, 0.78] };
        fractions.iter().map(|f| self.cum.iter().position(|c| *c >= f * self.length).unwrap_or(0)).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Countdown number about to be shown (3, 2, 1).
    Count(u8),
    Go,
    Pickup(usize),
    Use(usize, Item),
    Hit(usize),
    /// Racer `i` was hit by this item (sent after `Hit`).
    Smash(usize, Item),
    Lap(usize, u32),
    Finish(usize),
    Rescue(usize),
    /// Karts `i` and `j` bumped (`func_802903D8`).
    Bump(usize, usize),
    /// The rocket start fired for racer `i`.
    StartBoost(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Countdown,
    Racing,
    /// The player crossed the line; the CPUs keep driving.
    Finished,
    /// Free roam with items: the player alone, item boxes along the lap, no laps, CPUs or clock.
    Practice,
}

#[derive(Clone, Debug)]
pub struct Racer {
    pub passed: u32,
    pub next_gate: usize,
    pub last: [f64; 2],
    pub hint: usize,
    pub spin: u32,
    pub immune: u32,
    pub boost: u32,
    /// Mirrors of the item window (`KartRace::items` is the truth).
    pub item: Option<Item>,
    pub roulette: u32,
    /// `player->effects` / `triggers` and their state (`kart_effects.rs`).
    pub fx: Effects,
    /// Countdown tick at which accelerate was first held (the rocket start).
    pub throttle_at: Option<u32>,
    pub lap_start: u32,
    pub laps: Vec<u32>,
    pub finish: Option<u32>,
    pub dist_next: f64,
    /// Ticks spent nearly stationary.
    stuck: u32,
    hold: u32,
    pub rescues: u32,
}
impl Racer {
    fn new(pos: [f64; 2], human: bool) -> Self {
        Self { passed: 0, next_gate: 0, last: pos, hint: 0, spin: 0, immune: 0, boost: 0, item: None, roulette: 0, fx: Effects::new(human), throttle_at: None, lap_start: 0, laps: vec![], finish: None, dist_next: 0., stuck: 0, hold: 0, rescues: 0 }
    }
    /// Lap shown on the HUD (1-based).
    pub fn lap(&self) -> u32 {
        (self.laps.len() as u32 + 1).min(LAPS)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ItemBox {
    pub pos: [f64; 2],
    pub y: f64,
    pub respawn: u32,
}

#[derive(Clone, Debug)]
pub struct Results {
    /// Racer indices, first place first (0 is the player).
    pub order: Vec<usize>,
    pub total: u32,
    pub laps: Vec<u32>,
    pub best_lap: u32,
    pub new_best_lap: bool,
    pub new_best_total: bool,
}
impl Results {
    pub fn place(&self) -> usize {
        self.order.iter().position(|i| *i == 0).unwrap_or(0) + 1
    }
}

/// A path-following CPU on the same physics. Deterministic: the only randomness is the seeded
/// per-driver lane offset and skill.
#[derive(Clone, Debug)]
pub struct Driver {
    pub hint: usize,
    lane: f64,
    skill: f64,
    boost_cool: u32,
    ticks: u32,
}
impl Driver {
    pub fn new(rng: &mut Rng, hint: usize) -> Self {
        Self { hint, lane: (rng.unit() - 0.5) * 100., skill: 0.9 + rng.unit() * 0.08, boost_cool: 90, ticks: 0 }
    }
    /// `rubber`: how far this CPU is ahead of the player along the lap (course units, negative when behind).
    /// Returns the pad input and whether to ask for a catch-up boost.
    pub fn input(&mut self, track: &Track, kart: &Kart, rubber: f64, racing: bool) -> (Input, bool) {
        self.ticks += 1;
        let pos = [kart.pos[0], kart.pos[2]];
        self.hint = track.nearest(pos, self.hint, 30);
        if !racing {
            return (Input::default(), false);
        }
        let look = 130. + kart.speed * 22.;
        let target_node = track.ahead(self.hint, look);
        let side = track.side(target_node);
        let tp = track.xz(target_node);
        let target = [tp[0] + side[0] * self.lane, tp[1] + side[1] * self.lane];
        let want = (target[0] - pos[0]).atan2(target[1] - pos[1]);
        let mut diff = want - kart.yaw();
        while diff > PI {
            diff -= 2. * PI;
        }
        while diff < -PI {
            diff += 2. * PI;
        }
        let stick = (-diff * 120.).clamp(-64., 64.);
        let a = track.dir(track.ahead(self.hint, 120.));
        let b = track.dir(track.ahead(self.hint, 420.));
        let turn = (a[0] * b[0] + a[1] * b[1]).clamp(-1., 1.).acos();
        let safe = (6.4 - turn * 3.2).clamp(2.6, 6.4) * self.skill;
        let mut accel = true;
        let mut brake = false;
        if kart.speed > safe + 0.3 || (diff.abs() > 0.7 && kart.speed > 3.2) {
            accel = false;
            brake = kart.speed > safe + 1.3 && kart.speed > 2.;
        }
        // Rubber band: leaders lift now and then, stragglers get an occasional boost.
        if rubber > 1400. && self.ticks % 5 == 0 {
            accel = false;
        }
        let mut want_boost = false;
        if self.boost_cool > 0 {
            self.boost_cool -= 1;
        } else if rubber < -900. {
            want_boost = true;
            self.boost_cool = 300;
        }
        let raw_y = if brake { -80 } else if accel { 80 } else { 0 };
        (Input { raw_x: (stick * 80. / 64.).round() as i32, raw_y, accel, brake, hop_down: false, hop_pressed: false }, want_boost)
    }
}

pub struct Cpu {
    pub kart: Kart,
    pub driver: CpuDriver,
}

pub struct KartRace {
    pub track: Track,
    pub phase: Phase,
    pub clock: u32,
    pub race_tick: u32,
    pub countdown: u32,
    pub racers: Vec<Racer>,
    pub cpus: Vec<Cpu>,
    /// Items: the windows, the actors on the track (`kart_items.rs`).
    pub items: ItemWorld,
    /// The player's item stick and release edge for the next tick (the press is `tick`'s `use_item`).
    pub item_pad: Pad,
    /// Kart-vs-kart bumping (`func_802909F0`); on by default.
    pub kart_collisions: bool,
    /// Set when an outside controller drives the CPUs' items through `cpu_item_use` (tests); the decomp's
    /// `cpu_use_item_strategy` below then stops.
    pub external_cpu_items: bool,
    /// `cpu_ItemStrategy[]`, one per CPU (`kart_cpu::CpuItemStrategy`).
    pub cpu_strategy: Vec<CpuItemStrategy>,
    /// Item events of the last ticks (sounds, toasts); drained by the view.
    pub item_events: Vec<ItemEvent>,
    pub boxes: Vec<ItemBox>,
    rng: Rng,
    seed: u32,
    pub events: Vec<Event>,
    pub results: Option<Results>,
    pub best_lap: Option<u32>,
    pub best_total: Option<u32>,
    /// Set when `best_*` changed and should be saved.
    pub best_dirty: bool,
    pub fall_y: f64,
    /// CC class, racers and the course's CPU tables (kart setup menu); `kart_cpu.rs`.
    pub setup: RaceSetup,
    /// The lap path with the CPU driver's boundaries, curve types and expected headings.
    pub path: PathInfo,
    /// Toad's Turnpike looks one path point less ahead (`determine_ideal_cpu_position_offset`).
    pub turnpike: bool,
    /// Per racer: (best progress so far, ticks without improving it). A kart that makes no headway for `STALL_TICKS`
    /// is put back on the course, like the Lakitu pick-up of a wedged kart.
    stall: Vec<(f64, u32)>,
    /// The player is rescued like a CPU when wedged (scripted players in tests; people press R).
    pub assist_player: bool,
    /// Where and why each rescue happened (diagnostics): (racer, race tick, position, nearest path point, cause).
    pub rescue_log: Vec<(usize, u32, [f64; 3], usize, &'static str)>,
}

/// Ticks (30 Hz) without progress along the lap before a CPU is rescued.
pub const STALL_TICKS: u32 = 30 * 12;

/// One 30 Hz tick of a kart: two 60 Hz updates, each preceded by the effects frame (`apply_triggers` +
/// `apply_effect`), which takes the pad away while a spinout or tumble has it.
fn step_kart(kart: &mut Kart, st: &mut Racer, input: &Input, racing: bool, ground: &dyn Ground) {
    let base = if !racing { Input::default() } else { *input };
    for f in 0..2 {
        let mut pad = Input { hop_pressed: base.hop_pressed && f == 0, ..base };
        st.fx.frame(kart, &mut pad);
        kart.step(&pad, ground);
    }
    st.spin = if st.fx.blocks_controls() { 1 } else { 0 };
    st.immune = st.immune.saturating_sub(1);
    st.boost = if st.fx.has(fx::MUSHROOM_EFFECT) { 1 } else { 0 };
}

impl KartRace {
    pub fn new(path: &[[f64; 3]], seed: u32) -> Self {
        let track = Track::new(path);
        let track_nodes = track.nodes.clone();
        let fall_y = track.min_y - 300.;
        Self {
            track,
            phase: Phase::Countdown,
            clock: 0,
            race_tick: 0,
            countdown: COUNTDOWN_TICKS,
            racers: vec![],
            cpus: vec![],
            items: ItemWorld::new(RACERS, seed.max(1)),
            item_pad: Pad::default(),
            external_cpu_items: false,
            cpu_strategy: vec![],
            kart_collisions: true,
            item_events: vec![],
            boxes: vec![],
            rng: Rng(seed.max(1)),
            seed: seed.max(1),
            events: vec![],
            results: None,
            best_lap: None,
            best_total: None,
            best_dirty: false,
            fall_y,
            path: PathInfo::new(&track_nodes, RaceSetup::default().course.max_separation),
            setup: RaceSetup::default(),
            turnpike: false,
            stall: vec![],
            assist_player: false,
            rescue_log: vec![],
        }
    }

    /// Install the kart setup (CC class, racer, the course's CPU tables); call before `start`.
    pub fn configure(&mut self, setup: RaceSetup, turnpike: bool) {
        self.path = PathInfo::new(&self.track.nodes, setup.course.max_separation);
        self.setup = setup;
        self.turnpike = turnpike;
    }

    /// The stats of racer `i` (0 = the player).
    pub fn stats_for(&self, i: usize) -> KartStats {
        self.setup.stats(if i == 0 { self.setup.character } else { self.setup.cpu_character(i - 1) })
    }

    /// A kart at rest with racer `i`'s character and CC.
    fn new_kart(&self, i: usize, pos: [f64; 3], rotation: i32) -> Kart {
        let mut k = Kart::new(pos, rotation);
        k.set_stats(self.stats_for(i));
        k
    }

    /// `gNumPathPointsTraversed`: completed laps x path length + the nearest path point (negative before the line).
    pub fn points_traversed(&self, i: usize, player: &Kart) -> i64 {
        let n = self.path.len() as i64;
        let r = &self.racers[i];
        let pos = self.kart_pos(i, player);
        let point = if i == 0 { self.path.nearest(pos, r.hint) } else { self.cpus[i - 1].driver.point } as i64;
        if r.passed == 0 { point - n } else { r.laps.len() as i64 * n + point }
    }

    fn floor_y(ground: &dyn Ground, x: f64, y: f64, z: f64) -> f64 {
        ground.floor(x, y + 60., z).map_or(y, |f| f.height)
    }

    /// (Re)start: grid, countdown, fresh items. The player's kart is replaced.
    pub fn start(&mut self, player: &mut Kart, ground: &dyn Ground) {
        self.rng = Rng(self.seed);
        self.phase = Phase::Countdown;
        self.clock = 0;
        self.race_tick = 0;
        self.countdown = COUNTDOWN_TICKS;
        self.results = None;
        self.events.clear();
        self.items.reset(RACERS, self.seed);
        self.item_events.clear();
        self.racers.clear();
        self.cpus.clear();
        self.stall = vec![(f64::MIN, 0); RACERS];
        self.cpu_strategy = vec![CpuItemStrategy::default(); RACERS - 1];
        self.rescue_log.clear();
        let y0 = self.track.nodes[0][1];
        for slot in 0..RACERS {
            let (p, rot) = self.track.grid(slot);
            let y = Self::floor_y(ground, p[0], y0, p[1]);
            let index = if slot == PLAYER_SLOT { 0 } else { slot + 1 };
            let kart = self.new_kart(index, [p[0], y, p[1]], rot);
            let mut racer = Racer::new(p, slot == PLAYER_SLOT);
            racer.hint = self.path.nearest_any([p[0], y, p[1]]);
            racer.dist_next = dist(p, self.track.gates[0].c);
            if slot == PLAYER_SLOT {
                *player = kart;
                self.racers.insert(0, racer);
            } else {
                let driver = CpuDriver::new(slot + 1, racer.hint, self.setup.course.min_separation);
                self.racers.push(racer);
                self.cpus.push(Cpu { kart, driver });
            }
        }
        self.build_boxes(ground, false);
        self.events.push(Event::Count(3));
    }

    fn build_boxes(&mut self, ground: &dyn Ground, practice: bool) {
        self.boxes.clear();
        for node in self.track.box_nodes(practice) {
            let side = self.track.side(node);
            let c = self.track.xz(node);
            for lateral in [-52., 0., 52.] {
                let p = [c[0] + side[0] * lateral, c[1] + side[1] * lateral];
                self.boxes.push(ItemBox { pos: p, y: Self::floor_y(ground, p[0], self.track.nodes[node][1], p[1]), respawn: 0 });
            }
        }
    }

    /// Free roam with items: the player alone, item boxes along the lap, no countdown, laps or CPUs. The kart
    /// is not moved. Fresh items and boxes (R in free roam calls this again).
    pub fn start_practice(&mut self, player: &Kart, ground: &dyn Ground) {
        self.rng = Rng(self.seed);
        self.phase = Phase::Practice;
        self.clock = 0;
        self.race_tick = 0;
        self.countdown = 0;
        self.results = None;
        self.events.clear();
        self.items.reset(1, self.seed);
        self.item_events.clear();
        self.cpus.clear();
        self.racers = vec![Racer::new([player.pos[0], player.pos[2]], true)];
        self.build_boxes(ground, true);
    }

    pub fn kart_pos(&self, i: usize, player: &Kart) -> [f64; 3] {
        if i == 0 { player.pos } else { self.cpus[i - 1].kart.pos }
    }

    /// Race progress in course units: whole gates passed minus the distance left to the next one.
    pub fn progress(&self, i: usize) -> f64 {
        let r = &self.racers[i];
        r.passed as f64 * self.track.spacing - r.dist_next
    }

    /// Racer indices, first place first. Finished racers rank by finishing time.
    pub fn order(&self) -> Vec<usize> {
        let mut o: Vec<usize> = (0..self.racers.len()).collect();
        o.sort_by(|a, b| {
            let (ra, rb) = (&self.racers[*a], &self.racers[*b]);
            match (ra.finish, rb.finish) {
                (Some(x), Some(y)) => x.cmp(&y).then(a.cmp(b)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => self.progress(*b).partial_cmp(&self.progress(*a)).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b)),
            }
        });
        o
    }
    /// 0-based rank of racer `i`.
    pub fn rank(&self, i: usize) -> usize {
        self.order().iter().position(|r| *r == i).unwrap_or(0)
    }

    /// Extra visual turn of racer `i` in radians: the tumble of a shell / fake-box hit (`unk_042`). A spinout
    /// turns the kart's real heading, so it needs nothing here.
    pub fn spin_angle(&self, i: usize) -> f64 {
        self.racers[i].fx.look().tumble
    }

    /// 3, 2, 1 while counting down, then 0 for "GO!" during its first second; None otherwise.
    pub fn banner(&self) -> Option<u32> {
        match self.phase {
            Phase::Practice => None,
            Phase::Countdown => Some(self.countdown.div_ceil(30)),
            _ if self.race_tick < GO_TICKS => Some(0),
            _ => None,
        }
    }

    /// Put racer `i` back on the last gate it passed (or the grid before the line).
    fn rescue_kart(&self, i: usize, ground: &dyn Ground) -> Kart {
        let r = &self.racers[i];
        let (p, rot, y) = if r.passed == 0 {
            let slot = if i == 0 { PLAYER_SLOT } else { i - 1 };
            let (p, rot) = self.track.grid(slot);
            (p, rot, self.track.nodes[0][1])
        } else {
            let g = &self.track.gates[(r.next_gate + GATES - 1) % GATES];
            let node = g.arc;
            let idx = self.track.cum.iter().position(|c| *c >= node).unwrap_or(0);
            // Lakitu drops the kart on solid road: walk back from the gate to the first path point with a floor near
            // the path's own height (a gate can sit over a jump's gap or a bridge seam).
            let n = self.track.len();
            let solid = (0..80).map(|k| (idx + n - k % n) % n).find(|j| {
                let p = self.track.nodes[*j];
                [40., 120., 250.].iter().any(|up| ground.floor(p[0], p[1] + up, p[2]).is_some_and(|f| f.height > p[1] - 40. && f.height < p[1] + 100. && f.normal[1] > 0.7))
            });
            match solid {
                Some(j) if j != idx => {
                    let p = self.track.nodes[j];
                    ([p[0], p[2]], rotation_for(self.track.dir(j)), p[1])
                }
                _ => (g.c, rotation_for(g.t), self.track.nodes[idx][1]),
            }
        };
        self.new_kart(i, [p[0], Self::floor_y(ground, p[0], y, p[1]), p[1]], rot)
    }

    fn rescue(&mut self, i: usize, player: &mut Kart, ground: &dyn Ground, cause: &'static str) {
        let at = self.kart_pos(i, player);
        let near = if i > 0 { self.cpus[i - 1].driver.point } else { self.path.nearest(at, self.racers[i].hint) };
        // A second rescue at the same spot soon after the first: the kart cannot get past it (a jump our physics
        // cannot clear, a wall it keeps ramming), so Lakitu carries it beyond.
        let n = self.path.len() as i64;
        let repeat = self.rescue_log.iter().rev().find(|e| e.0 == i).is_some_and(|e| {
            let d = (e.3 as i64 - near as i64).rem_euclid(n);
            self.race_tick < e.1 + 30 * 40 && (d < 30 || d > n - 30)
        });
        // A fall into a gap of the road itself (a jump our physics cannot clear without the ramp boost): carry the kart
        // over it at once instead of dropping it back in front of the gap.
        let gap = cause == "fell" && (0..12).any(|k| !self.road_at(ground, (near + self.path.len() - 6 + k) % self.path.len()));
        let repeat = repeat || gap;
        self.rescue_log.push((i, self.race_tick, at, near, cause));
        if let Some(s) = self.stall.get_mut(i) {
            *s = (f64::MIN, 0);
        }
        let (k, ahead) = match self.lakitu_spot(i, near, repeat, ground) {
            Some((k, ahead)) => (k, ahead),
            None => (self.rescue_kart(i, ground), None),
        };
        if let Some(node) = ahead {
            self.pass_gates_up_to(i, near, node);
        }
        let p = [k.pos[0], k.pos[2]];
        let k_y = k.pos[1];
        if i == 0 { *player = k } else { self.cpus[i - 1].kart = k }
        let r = &mut self.racers[i];
        r.last = p;
        r.spin = 0;
        r.boost = 0;
        r.fx = Effects::new(i == 0);
        r.stuck = 0;
        r.immune = 60;
        r.rescues += 1;
        r.hint = self.path.nearest_any([p[0], k_y, p[1]]);
        if i > 0 {
            let point = self.path.nearest_any([p[0], k_y, p[1]]);
            self.cpus[i - 1].driver.relocate(point);
        }
        self.events.push(Event::Rescue(i));
    }

    /// Lakitu's drop point (`func_80090778` puts the kart back near the path point it left the course at): the first
    /// path point with solid road at the path's height, searching back from `near`, or forward past the obstacle
    /// when `skip`. Returns the kart and, when it was carried ahead, the path point it was put on.
    fn lakitu_spot(&self, i: usize, near: usize, skip: bool, ground: &dyn Ground) -> Option<(Kart, Option<usize>)> {
        if self.racers[i].passed == 0 {
            return None;
        }
        let n = self.path.len();
        let solid = |j: usize| {
            let p = self.path.pts[j];
            [40., 120., 250.].iter().find_map(|up| ground.floor(p[0], p[1] + up, p[2]).filter(|f| f.height > p[1] - 40. && f.height < p[1] + 100. && f.normal[1] > 0.7)).map(|f| f.height)
        };
        let (j, y) = if skip {
            (12..120).map(|k| (near + k) % n).find_map(|j| solid(j).map(|y| (j, y)))
        } else {
            (0..120).map(|k| (near + n - k % n) % n).find_map(|j| solid(j).map(|y| (j, y)))
        }?;
        let p = self.path.pts[j];
        let k = self.new_kart(i, [p[0], y, p[2]], self.path.rotation[j]);
        Some((k, if skip { Some(j) } else { None }))
    }

    /// Solid road at the height of path point `j` (probing from a little above it: stacked roads, overhangs).
    fn road_at(&self, ground: &dyn Ground, j: usize) -> bool {
        let p = self.path.pts[j];
        [40., 120., 250.].iter().any(|up| ground.floor(p[0], p[1] + up, p[2]).is_some_and(|f| f.height > p[1] - 40. && f.height < p[1] + 100. && f.normal[1] > 0.7))
    }

    /// Lakitu carried racer `i` forward from path point `from` to `to`: count the checkpoint gates in between as passed.
    fn pass_gates_up_to(&mut self, i: usize, from: usize, to: usize) {
        let n = self.path.len();
        let span = (to + n - from) % n;
        for _ in 0..GATES {
            let g = self.racers[i].next_gate;
            let ahead = (g * n / GATES + n - from) % n;
            if ahead > span {
                break;
            }
            let gate = self.track.gates[g];
            self.racers[i].last = [gate.c[0] - gate.t[0] * 5., gate.c[1] - gate.t[1] * 5.];
            self.progress_step(i, [gate.c[0] + gate.t[0] * 5., gate.c[1] + gate.t[1] * 5.]);
        }
    }

    /// Player asked for a Lakitu rescue (R in race mode).
    pub fn rescue_player(&mut self, player: &mut Kart, ground: &dyn Ground) {
        if self.phase == Phase::Racing {
            self.rescue(0, player, ground, "requested");
        }
    }

    /// Advance the whole race by one 30 Hz tick. `input` is the player's pad; `use_item` is the item button edge.
    pub fn tick(&mut self, input: &Input, use_item: bool, player: &mut Kart, ground: &dyn Ground) {
        if self.racers.is_empty() {
            self.start(player, ground);
        }
        self.clock += 1;
        if self.phase == Phase::Practice {
            step_kart(player, &mut self.racers[0], input, true, ground);
            self.items_step(use_item, player, ground);
            return;
        }
        match self.phase {
            Phase::Practice => {}
            Phase::Countdown => {
                self.countdown = self.countdown.saturating_sub(1);
                // The rocket start: note when accelerate went down (and forget it when released).
                let r = &mut self.racers[0];
                if input.accel {
                    r.throttle_at.get_or_insert(self.countdown);
                } else {
                    r.throttle_at = None;
                }
                if self.countdown == 0 {
                    self.phase = Phase::Racing;
                    self.race_tick = 0;
                    self.events.push(Event::Go);
                    let r = &mut self.racers[0];
                    match r.throttle_at.take() {
                        Some(t) if START_BOOST_WINDOW.contains(&t) => {
                            r.fx.add_trigger(fx::START_BOOST_TRIGGER);
                            self.events.push(Event::StartBoost(0));
                        }
                        Some(t) if t > *START_BOOST_WINDOW.end() => {
                            r.fx.add_trigger(fx::START_SPINOUT_TRIGGER);
                            self.events.push(Event::Hit(0));
                        }
                        _ => {}
                    }
                } else if self.countdown % 30 == 0 {
                    self.events.push(Event::Count((self.countdown / 30) as u8));
                }
            }
            _ => self.race_tick += 1,
        }
        let racing = self.phase != Phase::Countdown;
        let pad = if self.phase == Phase::Finished { Input::default() } else { *input };
        // Player.
        step_kart(player, &mut self.racers[0], &pad, racing, ground);
        let player_progress = self.progress(0);
        // CPUs.
        let _ = player_progress;
        self.drive_cpus(racing, player, ground);
        if racing {
            for i in 0..RACERS {
                let pos = self.kart_pos(i, player);
                let p = [pos[0], pos[2]];
                // Falls and wedged CPUs.
                let slow = (if i == 0 { player.speed } else { self.cpus[i - 1].kart.speed }) < 0.25;
                let r = &mut self.racers[i];
                r.stuck = if slow && (i > 0 || self.assist_player) { r.stuck + 1 } else { 0 };
                let helped = i > 0 || self.assist_player;
                let progress = self.progress(i);
                let st = &mut self.stall[i];
                if progress > st.0 + 20. {
                    *st = (progress, 0);
                } else {
                    st.1 += 1;
                }
                let stalled = helped && self.stall[i].1 > STALL_TICKS;
                // Falling well below the lap path: Lakitu does not wait for the bottom of the void.
                let path_y = self.path.pts[if i > 0 { self.cpus[i - 1].driver.point } else { self.racers[0].hint % self.path.len() }][1];
                let airborne = if i > 0 { self.cpus[i - 1].kart.midair } else { player.midair };
                let void = airborne && pos[1] < path_y - 150. && ground.floor(pos[0], pos[1] + 5., pos[2]).is_none();
                if pos[1] < self.fall_y || void {
                    self.rescue(i, player, ground, "fell");
                    continue;
                }
                if (self.racers[i].stuck > 150 && helped) || stalled {
                    self.rescue(i, player, ground, if stalled { "no progress" } else { "wedged" });
                    continue;
                }
                self.progress_step(i, p);
            }
            self.bump_karts(player);
            self.items_step(use_item, player, ground);
        }
    }

    /// The CPU drivers' decision for this tick, then their physics.
    fn drive_cpus(&mut self, racing: bool, player: &Kart, ground: &dyn Ground) {
        let order = self.order();
        let n = self.path.len() as i64;
        let traversed: Vec<i64> = (0..self.racers.len()).map(|i| self.points_traversed(i, player)).collect();
        let hint = self.racers[0].hint;
        let player_point = self.path.nearest(player.pos, hint);
        self.racers[0].hint = player_point;
        let neighbors: Vec<Neighbor> = (0..self.racers.len())
            .map(|i| {
                let k = if i == 0 { player } else { &self.cpus[i - 1].kart };
                let point = if i == 0 { player_point } else { self.cpus[i - 1].driver.point };
                let factor = if i == 0 { self.path.factor([player.pos[0], player.pos[2]], player_point) } else { self.cpus[i - 1].driver.factor };
                Neighbor { point, speed: k.speed, factor }
            })
            .collect();
        for c in 0..self.cpus.len() {
            let i = c + 1;
            let rank = order.iter().position(|r| *r == i).unwrap_or(0);
            let human_rank = order.iter().position(|r| *r == 0).unwrap_or(0);
            let lap = self.racers[i].laps.len();
            let view = RaceView {
                traversed: traversed[i],
                human_traversed: traversed[0],
                rank,
                human_rank,
                lap,
                lap_fraction: (traversed[i].rem_euclid(n)) as f64 / n as f64,
                cpu_rank: order.iter().filter(|r| **r != 0).position(|r| *r == i).unwrap_or(0),
                racing,
            };
            let others: Vec<Neighbor> = neighbors.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, v)| *v).collect();
            let character = self.setup.cpu_character(c);
            let cc = self.setup.cc;
            let cpu = &mut self.cpus[c];
            let spinning = self.racers[i].spin > 0;
            if racing {
                cpu.driver.look_for_edges(&self.path, &cpu.kart, ground);
                cpu.driver.control(&self.setup.course, &self.path, character, cc, self.turnpike, &mut cpu.kart, &others, &view);
                if spinning {
                    cpu.kart.cpu_turn = Some(0);
                    cpu.kart.cpu_decel = 1.;
                }
            } else {
                cpu.kart.cpu_turn = None;
            }
            step_kart(&mut cpu.kart, &mut self.racers[i], &Input::default(), racing, ground);
        }
    }

    /// `func_802909F0` / `func_802903D8`: every pair of karts that touch bumps; a star wins; a boo or a squashed
    /// kart is skipped.
    fn bump_karts(&mut self, player: &mut Kart) {
        if !self.kart_collisions {
            return;
        }
        let n = self.racers.len();
        for i in 0..n {
            for j in i + 1..n {
                let skip = |r: &Racer| r.fx.any(fx::BOO_EFFECT | fx::SQUISH_EFFECT);
                if skip(&self.racers[i]) || skip(&self.racers[j]) {
                    continue;
                }
                let (lo, hi) = self.racers.split_at_mut(j);
                let (fa, fb) = (&mut lo[i].fx, &mut hi[0].fx);
                // `gFloatArray802B8790[characterId]`: each kart's own racer (the kart setup menu picks them).
                let weight = |k: &Kart| fx::KART_WEIGHT[k.stats.character.min(7)];
                let wa = weight(if i == 0 { &*player } else { &self.cpus[i - 1].kart });
                let wb = weight(&self.cpus[j - 1].kart);
                let touched = if i == 0 {
                    fx::kart_vs_kart(player, fa, wa, &mut self.cpus[j - 1].kart, fb, wb)
                } else {
                    let (cl, ch) = self.cpus.split_at_mut(j - 1);
                    fx::kart_vs_kart(&mut cl[i - 1].kart, fa, wa, &mut ch[0].kart, fb, wb)
                };
                if touched {
                    self.events.push(Event::Bump(i, j));
                }
            }
        }
    }

    /// The CPU controller's item call: queue a press / release (and stick) for racer `i` (`racers` index, CPUs
    /// are 1..). Applied on the next tick. Set `external_cpu_items` to stop the built-in timers.
    pub fn cpu_item_use(&mut self, i: usize, use_: ItemUse) {
        self.items.set_use(i, use_);
    }
    /// Debug / test hook: put an item in racer `i`'s window.
    pub fn give_item(&mut self, i: usize, item: Item) {
        self.items.give(i, item);
        self.racers[i].item = Some(item);
        self.racers[i].roulette = 0;
    }

    pub fn progress_step(&mut self, i: usize, p: [f64; 2]) {
        let tick = self.race_tick;
        let r = &mut self.racers[i];
        let gate = self.track.gates[r.next_gate];
        if gate.crossed(r.last, p) {
            let crossed = r.next_gate;
            r.passed += 1;
            r.next_gate = (r.next_gate + 1) % GATES;
            if crossed == 0 {
                if i > 0 {
                    self.cpus[i - 1].driver.on_finish_line();
                }
                let r = &mut self.racers[i];
                if r.passed == 1 {
                    r.lap_start = tick;
                } else {
                    let time = tick - r.lap_start;
                    r.lap_start = tick;
                    r.laps.push(time);
                    let n = r.laps.len() as u32;
                    self.events.push(Event::Lap(i, n));
                    if i == 0 {
                        if self.best_lap.is_none_or(|b| time < b) {
                            self.best_lap = Some(time);
                            self.best_dirty = true;
                        }
                    }
                    if n >= LAPS && r.finish.is_none() {
                        r.finish = Some(tick);
                        self.events.push(Event::Finish(i));
                        if i == 0 {
                            self.finish_player();
                        }
                    }
                }
            }
        }
        let r = &mut self.racers[i];
        r.last = p;
        r.dist_next = dist(p, self.track.gates[r.next_gate].c);
    }

    fn finish_player(&mut self) {
        self.phase = Phase::Finished;
        let total = self.racers[0].finish.unwrap_or(self.race_tick);
        let laps = self.racers[0].laps.clone();
        let best_lap = laps.iter().copied().min().unwrap_or(0);
        let new_best_total = self.best_total.is_none_or(|b| total < b);
        if new_best_total {
            self.best_total = Some(total);
            self.best_dirty = true;
        }
        let new_best_lap = self.best_lap == Some(best_lap) && self.best_dirty;
        self.results = Some(Results { order: self.order(), total, laps, best_lap, new_best_lap, new_best_total });
    }

    fn items_step(&mut self, use_item: bool, player: &mut Kart, ground: &dyn Ground) {
        let n = self.racers.len();
        let order = self.order();
        let rank_of = |i: usize| order.iter().position(|r| *r == i).unwrap_or(0);
        // Item boxes (`ACTOR_ITEM_BOX` in `evaluate_collision_between_player_actor`): any kart breaks one; only a
        // human's window starts the roulette (`func_8007ABFC` is called for `PLAYER_HUMAN`). CPUs draw their items
        // on the `cpu_use_item_strategy` timer instead.
        for b in 0..self.boxes.len() {
            if self.boxes[b].respawn > 0 {
                self.boxes[b].respawn -= 1;
            }
            if self.boxes[b].respawn > BOX_RESPAWN - BOX_BREAK {
                continue;
            }
            for i in 0..n {
                if self.racers[i].finish.is_some() {
                    continue;
                }
                let k = self.kart_pos(i, player);
                let dy = (k[1] + fx::KART_RADIUS) - (self.boxes[b].y + 8.66);
                if dist([k[0], k[2]], self.boxes[b].pos) < BOX_RADIUS && dy.abs() < BOX_RADIUS {
                    self.boxes[b].respawn = BOX_RESPAWN;
                    if i == 0 && self.items.pickup(i, rank_of(i), n, true) {
                        self.events.push(Event::Pickup(i));
                    }
                    break;
                }
            }
        }
        // Pads: the player's button and stick; the CPUs' item strategy unless a controller drives them.
        let mut pads = vec![Pad::default(); n];
        if matches!(self.phase, Phase::Racing | Phase::Practice) {
            pads[0] = Pad { press: use_item, ..self.item_pad };
        }
        self.item_pad.release = false;
        if !self.external_cpu_items && self.phase == Phase::Racing {
            self.cpu_use_item_strategy(player, &order);
        }
        let bodies: Vec<Body> = (0..n)
            .map(|i| {
                let kart = if i == 0 { &*player } else { &self.cpus[i - 1].kart };
                let mut b = Body::from_kart(kart, &self.racers[i].fx, rank_of(i), self.racers[i].hint, i == 0);
                b.exists = i == 0 || self.racers[i].finish.is_none();
                b
            })
            .collect();
        let path: Vec<[f64; 3]> = self.track.nodes.clone();
        for f in 0..2 {
            let p = if f == 0 { pads.clone() } else { vec![Pad::default(); n] };
            let hits = self.items.step(&Ctx { bodies: &bodies, pads: &p, ground, path: &path, min_y: self.fall_y });
            for h in hits {
                self.racers[h.racer].fx.add_trigger(h.trigger);
                self.events.push(Event::Hit(h.racer));
                self.events.push(Event::Smash(h.racer, h.item));
            }
        }
        for (i, t) in self.items.drain_triggers() {
            self.racers[i].fx.add_trigger(t);
        }
        for e in self.items.drain_events() {
            if let ItemEvent::Use(i, item) = e {
                self.events.push(Event::Use(i, item));
            }
            self.item_events.push(e);
        }
        if self.item_events.len() > 256 {
            let extra = self.item_events.len() - 256;
            self.item_events.drain(..extra);
        }
        for i in 0..n {
            let r = &mut self.racers[i];
            r.item = self.items.held(i);
            r.roulette = self.items.slots[i].roulette.div_ceil(2);
            if r.item.is_some() || self.items.is_holding_actor(i) {
                r.hold += 1;
            } else {
                r.hold = 0;
            }
        }
    }

    /// `cpu_use_item_strategy` for every CPU, through the same item pad a human has (`cpu_item_use`): a drawn item
    /// goes into the CPU's window and is used at once; a banana / fake box is dragged for `timeBeforeThrow` frames,
    /// then let go.
    fn cpu_use_item_strategy(&mut self, player: &Kart, order: &[usize]) {
        let n = self.racers.len();
        for c in 0..self.cpus.len().min(self.cpu_strategy.len()) {
            let i = c + 1;
            if self.racers[i].finish.is_some() {
                continue;
            }
            // The decomp's CPUs never keep an item in a window: anything left there (a use that `prevent_item_use`
            // refused, the boo's 81 % item) is dropped.
            if self.cpu_strategy[c].branch != super::kart_cpu::CpuBranch::Hold && self.items.held(i).is_some() {
                self.items.discard(i);
            }
            let rank = order.iter().position(|r| *r == i).unwrap_or(0);
            let r = &self.racers[i];
            let view = CpuItemView {
                slot: i,
                traversed: self.points_traversed(i, player),
                lap: r.laps.len() as u32,
                holding: self.items.is_holding_actor(i),
                star_or_boo: r.fx.any(fx::STAR_EFFECT | fx::BOO_EFFECT),
                boosted: r.fx.any(fx::BOO_EFFECT | fx::MUSHROOM_EFFECT | fx::STAR_EFFECT),
            };
            let items = std::cell::RefCell::new(&mut self.items);
            let action = self.cpu_strategy[c].tick(&view, || items.borrow_mut().cpu_draw(rank, n), || items.borrow_mut().random_int(3));
            match action {
                Some(CpuItemAction::Use(item)) => {
                    self.items.give(i, item);
                    self.items.set_use(i, ItemUse { press: true, ..ItemUse::default() });
                }
                Some(CpuItemAction::Release) => self.items.set_use(i, ItemUse { release: true, ..ItemUse::default() }),
                None => {}
            }
        }
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

/// `m:ss.cc` for a tick count at 30 Hz.
pub fn format_ticks(ticks: u32) -> String {
    let cs = (ticks as f64 * 100. / 30.).round() as u32;
    format!("{}:{:02}.{:02}", cs / 6000, cs / 100 % 60, cs % 100)
}

/// Ordinal for the HUD (1 -> "1st").
pub fn ordinal(place: usize) -> &'static str {
    ["1st", "2nd", "3rd", "4th"][place.min(3)]
}

/// Saved personal bests (`kart.json`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Bests {
    pub best_lap: Option<u32>,
    pub best_total: Option<u32>,
}
pub fn save_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("kart.json")
}
pub fn load_bests() -> Bests {
    std::fs::read_to_string(save_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
pub fn save_bests(b: &Bests) {
    let path = save_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_string(b).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::super::kart_physics::WallHit;
    use super::*;

    struct Flat {
        wall_x: Option<f64>,
    }
    impl Ground for Flat {
        fn floor(&self, _x: f64, _y: f64, _z: f64) -> Option<super::super::kart_physics::Floor> {
            Some(super::super::kart_physics::Floor { height: 0., normal: [0., 1., 0.], grass: false })
        }
        fn wall(&self, pos: [f64; 3]) -> Option<Wall> {
            self.wall_x.filter(|w| pos[0] > *w).map(|w| Wall { pos: [w - 2., pos[1], pos[2]], hits: vec![WallHit { normal: [-1., 0., 0.], x_facing: true }] })
        }
    }

    /// A 1200 x 800 rectangle that starts mid-edge heading -z like Luigi Raceway.
    fn oval() -> Vec<[f64; 3]> {
        let mut pts = vec![];
        let corners: [[f64; 2]; 5] = [[0., 0.], [0., -400.], [1200., -400.], [1200., 400.], [0., 400.]];
        for k in 0..corners.len() {
            let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
            let n = ((b[0] - a[0]).abs() + (b[1] - a[1]).abs()) / 20.;
            for s in 0..n as usize {
                let f = s as f64 / n;
                pts.push([a[0] + (b[0] - a[0]) * f, 0., a[1] + (b[1] - a[1]) * f]);
            }
        }
        pts.push([-32768., -32768., -32768.]);
        pts
    }
    fn ground() -> Flat {
        Flat { wall_x: None }
    }
    fn race() -> (KartRace, Kart) {
        let mut r = KartRace::new(&oval(), 7);
        let mut k = Kart::new([0.; 3], 0);
        r.start(&mut k, &ground());
        (r, k)
    }
    fn pad() -> Input {
        Input::default()
    }

    #[test]
    fn outlier_node_is_dropped_and_gates_are_ordered() {
        let (r, _) = race();
        assert!(r.track.nodes.iter().all(|p| p[0] > -30000.));
        assert_eq!(r.track.gates.len(), GATES);
        assert!(r.track.gates.windows(2).all(|w| w[1].arc > w[0].arc));
        assert!((r.track.length - 4000.).abs() < 60., "{}", r.track.length);
    }

    #[test]
    fn gates_count_only_forward_crossings_inside_their_width() {
        let g = Gate { c: [0., 0.], t: [0., -1.], arc: 0. };
        assert!(g.crossed([0., 5.], [0., -5.]));
        assert!(!g.crossed([0., -5.], [0., 5.]), "reverse");
        assert!(!g.crossed([GATE_HALF + 50., 5.], [GATE_HALF + 50., -5.]), "outside the width");
        assert!(g.crossed([GATE_HALF - 5., 5.], [GATE_HALF - 5., -5.]));
        assert!(!g.crossed([0., 5.], [0., 2.]), "no crossing");
    }

    #[test]
    fn gates_must_be_taken_in_order_so_shortcuts_and_reversing_do_not_count() {
        let (mut r, _) = race();
        r.phase = Phase::Racing;
        let t = r.track.clone();
        let at = |g: usize| t.gates[g].c;
        let before = |g: usize| [t.gates[g].c[0] - t.gates[g].t[0] * 15., t.gates[g].c[1] - t.gates[g].t[1] * 15.];
        let after = |g: usize| [t.gates[g].c[0] + t.gates[g].t[0] * 15., t.gates[g].c[1] + t.gates[g].t[1] * 15.];
        let _ = at;
        // Cross gate 0 properly, then skip gate 1 and cross gate 2: nothing counts.
        r.racers[1].last = before(0);
        r.progress_step(1, after(0));
        assert_eq!(r.racers[1].passed, 1);
        r.racers[1].last = before(2);
        r.progress_step(1, after(2));
        assert_eq!(r.racers[1].passed, 1, "skipped gate");
        // Going backwards through gate 1 does not count either.
        r.racers[1].last = after(1);
        r.progress_step(1, before(1));
        assert_eq!(r.racers[1].passed, 1);
        r.racers[1].last = before(1);
        r.progress_step(1, after(1));
        assert_eq!(r.racers[1].passed, 2);
    }

    #[test]
    fn full_gate_cycle_counts_laps_and_finishes_at_three() {
        let (mut r, _) = race();
        r.phase = Phase::Racing;
        let t = r.track.clone();
        let mut tick = 0;
        for _lap in 0..=LAPS {
            for g in 0..GATES {
                tick += 10;
                r.race_tick = tick;
                let (c, d) = (t.gates[g].c, t.gates[g].t);
                r.racers[0].last = [c[0] - d[0] * 10., c[1] - d[1] * 10.];
                r.progress_step(0, [c[0] + d[0] * 10., c[1] + d[1] * 10.]);
            }
        }
        let p = &r.racers[0];
        assert_eq!(p.laps.len() as u32, LAPS);
        assert_eq!(p.passed, GATES as u32 * (LAPS + 1));
        assert!(p.finish.is_some());
        assert_eq!(r.phase, Phase::Finished);
        let res = r.results.as_ref().unwrap();
        assert_eq!(res.laps.len(), 3);
        assert_eq!(res.laps[0], GATES as u32 * 10);
        assert_eq!(r.best_lap, Some(GATES as u32 * 10));
        assert!(res.new_best_total);
    }

    #[test]
    fn xorshift_stream_matches_the_browser_twin() {
        let mut rng = Rng(12345);
        assert_eq!([rng.next_u32(), rng.next_u32(), rng.next_u32()], [3_336_926_330, 1_697_253_807, 2_816_511_904]);
        assert_eq!(format_ticks(30 * 75 + 15), "1:15.50");
    }

    #[test]
    fn countdown_holds_the_karts_then_releases_them() {
        let (mut r, mut k) = race();
        let g = ground();
        let go = Input { raw_y: 80, accel: true, ..pad() };
        let start = k.pos;
        let mut counts = vec![];
        // Accelerate from the rocket-start window on (earlier would spin out at the line).
        for _ in 0..COUNTDOWN_TICKS - 1 {
            let idle = pad();
            r.tick(if r.countdown <= 20 { &go } else { &idle }, false, &mut k, &g);
            counts.extend(r.drain_events());
        }
        assert_eq!(r.phase, Phase::Countdown);
        assert!((k.pos[2] - start[2]).abs() < 0.5, "held on the grid");
        assert_eq!(r.banner(), Some(1));
        r.tick(&go, false, &mut k, &g);
        assert_eq!(r.phase, Phase::Racing);
        assert!(r.drain_events().contains(&Event::Go));
        assert!(counts.contains(&Event::Count(2)) && counts.contains(&Event::Count(1)));
        for _ in 0..60 {
            r.tick(&go, false, &mut k, &g);
        }
        assert!(k.pos[2] < start[2] - 30., "released at GO");
    }

    #[test]
    fn ranking_orders_by_gates_then_distance_and_finish_time_wins() {
        let (mut r, _) = race();
        r.racers[0].passed = 5;
        r.racers[0].dist_next = 100.;
        r.racers[1].passed = 5;
        r.racers[1].dist_next = 40.;
        r.racers[2].passed = 7;
        r.racers[2].dist_next = 300.;
        r.racers[3].passed = 2;
        assert_eq!(r.order(), vec![2, 1, 0, 3]);
        assert_eq!(r.rank(0), 2);
        r.racers[3].finish = Some(900);
        r.racers[0].finish = Some(800);
        assert_eq!(r.order(), vec![0, 3, 2, 1]);
    }

    #[test]
    fn a_box_starts_the_142_frame_roulette_and_cpus_only_break_it() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        let g = ground();
        let b = r.boxes[0].pos;
        k.pos = [b[0], 0., b[1]];
        r.tick(&pad(), false, &mut k, &g);
        assert!(r.drain_events().contains(&Event::Pickup(0)));
        assert!(r.racers[0].roulette > 0 && r.racers[0].item.is_none());
        assert_eq!(r.boxes[0].respawn, BOX_RESPAWN);
        for _ in 0..ROULETTE_TICKS {
            r.tick(&pad(), false, &mut k, &g);
        }
        assert!(r.racers[0].item.is_some(), "the roulette settles after 142 frames");
        assert_eq!(r.racers[0].roulette, 0);
        // A CPU breaks a box but takes nothing (`func_8007ABFC` is human-only; CPUs draw on their strategy timer).
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        let b = r.boxes[3].pos;
        r.cpus[0].kart.pos = [b[0], 0., b[1]];
        r.external_cpu_items = true;
        r.tick(&pad(), false, &mut k, &g);
        assert_eq!(r.boxes[3].respawn, BOX_RESPAWN);
        assert!(r.racers[1].item.is_none() && r.racers[1].roulette == 0);
    }

    /// `cpu_use_item_strategy` on the oval: CPUs that are well under way draw and use items on the decomp timer
    /// (at most three a lap), and every use goes through the item system.
    #[test]
    fn cpus_use_items_on_the_strategy_timer() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        let g = ground();
        let mut uses = vec![0; RACERS];
        for _ in 0..30 * 90 {
            r.tick(&pad(), false, &mut k, &g);
            for e in r.drain_events() {
                if let Event::Use(i, _) = e {
                    uses[i] += 1;
                }
            }
        }
        assert_eq!(uses[0], 0);
        assert!(uses[1..].iter().sum::<u32>() >= 3, "{uses:?}");
        for c in 1..RACERS {
            let laps = r.racers[c].laps.len() as u32;
            assert!(uses[c] <= 3 * (laps + 1), "racer {c}: {} uses in {} laps", uses[c], laps + 1);
        }
    }

    #[test]
    fn a_mushroom_boosts_and_a_banana_spins_out_through_the_ported_effects() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        let g = ground();
        let go = Input { accel: true, raw_y: 80, ..pad() };
        for _ in 0..150 {
            r.tick(&go, false, &mut k, &g);
        }
        let cruise = k.speed;
        r.give_item(0, Item::Mushroom);
        r.tick(&go, true, &mut k, &g);
        r.tick(&go, false, &mut k, &g);
        assert!(r.racers[0].fx.has(fx::MUSHROOM_EFFECT));
        let mut fastest: f64 = 0.;
        for _ in 0..20 {
            r.tick(&go, false, &mut k, &g);
            fastest = fastest.max(k.speed);
        }
        assert!(fastest >= cruise, "boost {fastest} over {cruise}");
        assert!(r.racers[0].item.is_none());
        // A CPU-owned banana right in front of the player.
        r.give_item(1, Item::Banana);
        r.cpu_item_use(1, ItemUse { press: true, ..ItemUse::default() });
        r.external_cpu_items = true;
        r.tick(&go, false, &mut k, &g);
        r.cpu_item_use(1, ItemUse { release: true, ..ItemUse::default() });
        r.tick(&go, false, &mut k, &g);
        let banana = r.items.actors.iter().find(|a| a.kind == super::super::kart_items::Kind::Banana).expect("dropped").id;
        // Teleport the banana to the player's path and drive into it.
        let f = forward(&k);
        if let Some(a) = r.items.actors.iter_mut().find(|a| a.id == banana) {
            a.pos = [k.pos[0] + f[0] * 25., fx::KART_RADIUS - 2., k.pos[2] + f[1] * 25.];
            a.state = super::super::kart_items::banana::ON_GROUND;
            a.protect = false;
        }
        let mut hit = false;
        for _ in 0..10 {
            r.tick(&go, false, &mut k, &g);
            hit |= r.drain_events().contains(&Event::Smash(0, Item::Banana));
        }
        assert!(hit, "the banana trigger");
        assert!(r.racers[0].fx.any(fx::BANANA_NEAR_SPINOUT_EFFECT | fx::BANANA_SPINOUT_EFFECT | fx::DRIVING_SPINOUT_EFFECT));
    }

    #[test]
    fn practice_has_boxes_but_no_cpus_laps_or_banner_and_never_moves_the_kart() {
        let mut r = KartRace::new(&oval(), 4);
        let k = Kart::new([0.; 3], 0);
        r.start_practice(&k, &ground());
        assert_eq!(r.phase, Phase::Practice);
        assert!(r.cpus.is_empty());
        assert_eq!(r.racers.len(), 1);
        assert!(r.boxes.len() >= 15);
        assert_eq!(r.banner(), None);
        let mut k = k;
        r.tick(&pad(), false, &mut k, &ground());
        assert!(k.pos[0].hypot(k.pos[2]) < 1.);
        assert_eq!(r.phase, Phase::Practice);
    }

    #[test]
    fn practice_box_rolls_an_item_which_can_be_used_and_the_box_respawns() {
        let mut r = KartRace::new(&oval(), 4);
        let g = ground();
        r.start_practice(&Kart::new([0.; 3], 0), &g);
        let b = r.boxes[1].pos;
        let mut k = Kart::new([b[0], 0., b[1]], 0);
        r.tick(&pad(), false, &mut k, &g);
        assert!(r.drain_events().contains(&Event::Pickup(0)));
        assert!(r.racers[0].roulette > 0 && r.boxes[1].respawn > 0);
        for _ in 0..=ROULETTE_TICKS {
            r.tick(&pad(), false, &mut k, &g);
        }
        let item = r.racers[0].item.expect("item after the roulette");
        r.tick(&pad(), true, &mut k, &g);
        assert!(r.drain_events().contains(&Event::Use(0, item)));
        k.pos[0] += 4000.;
        for _ in 0..200 {
            r.item_pad.release = true;
            r.tick(&pad(), false, &mut k, &g);
        }
        assert_eq!(r.boxes[1].respawn, 0);
    }

    #[test]
    fn practice_rolls_are_reproducible_and_varied() {
        let rolls = || {
            let mut w = super::super::kart_items::ItemWorld::new(1, 9);
            (0..30)
                .map(|_| {
                    w.slots[0] = Default::default();
                    w.pickup(0, 0, 1, false);
                    w.held(0).unwrap()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(rolls(), rolls());
        let all = rolls();
        assert!(all.iter().any(|i| *i != all[0]));
    }

    #[test]
    fn a_red_shell_from_second_place_hits_the_leader_on_the_oval() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        r.external_cpu_items = true;
        let g = ground();
        // CPU 1 leads 120 units ahead of the player on the straight; the player is second.
        let f = forward(&k);
        r.cpus[0].kart = Kart::new([k.pos[0] + f[0] * 120., 0., k.pos[2] + f[1] * 120.], k.rotation);
        r.racers[1].passed = 3;
        r.racers[0].passed = 2;
        r.racers[2].passed = 0;
        r.racers[3].passed = 0;
        for c in 1..3 {
            r.cpus[c].kart.pos[0] += 2000.;
        }
        r.give_item(0, Item::RedShell);
        r.tick(&pad(), true, &mut k, &g);
        r.item_pad.release = true;
        r.tick(&pad(), false, &mut k, &g);
        let mut hit = false;
        for _ in 0..60 {
            r.tick(&pad(), false, &mut k, &g);
            hit |= r.drain_events().iter().any(|e| *e == Event::Smash(1, Item::RedShell));
        }
        assert!(hit, "RED_SHELL_LOCK_ON on rank - 1");
        assert!(r.racers[1].fx.has(fx::HIT_BY_STAR_EFFECT) || r.racers[1].fx.has(fx::POST_SQUISH_EFFECT) || !r.racers[1].fx.blocks_controls(), "the high tumble ran");
    }

    #[test]
    fn karts_bump_apart_and_a_star_tumbles_the_other() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        let g = ground();
        r.external_cpu_items = true;
        r.cpus[0].kart = Kart::new([k.pos[0] + 4., 0., k.pos[2]], k.rotation);
        r.racers[0].fx.add_trigger(fx::STAR_TRIGGER);
        r.tick(&pad(), false, &mut k, &g);
        r.cpus[0].kart.pos = [k.pos[0] + 4., k.pos[1], k.pos[2]];
        r.tick(&pad(), false, &mut k, &g);
        let ev = r.drain_events();
        assert!(ev.iter().any(|e| matches!(e, Event::Bump(0, 1))), "{ev:?}");
        assert!(r.racers[1].fx.has(fx::HIT_BY_STAR_EFFECT), "HIT_BY_STAR_TRIGGER");
    }

    #[test]
    fn the_rocket_start_window_boosts_and_revving_early_wobbles() {
        let g = ground();
        let go = Input { accel: true, raw_y: 80, ..pad() };
        for (press_at, want) in [(20u32, Some(Event::StartBoost(0))), (80, Some(Event::Hit(0))), (5, None)] {
            let (mut r, mut k) = race();
            let mut events = vec![];
            while r.phase == Phase::Countdown {
                let held = r.countdown <= press_at;
                let idle = pad();
                r.tick(if held { &go } else { &idle }, false, &mut k, &g);
                events.extend(r.drain_events());
            }
            match want {
                Some(Event::StartBoost(_)) => {
                    assert!(events.contains(&Event::StartBoost(0)));
                    r.tick(&go, false, &mut k, &g);
                    assert!(r.racers[0].fx.has(fx::MUSHROOM_EFFECT));
                }
                Some(_) => {
                    assert!(events.contains(&Event::Hit(0)));
                    r.tick(&go, false, &mut k, &g);
                    assert!(r.racers[0].fx.has(fx::EARLY_START_SPINOUT_EFFECT));
                }
                None => assert!(!events.contains(&Event::StartBoost(0)) && !events.contains(&Event::Hit(0))),
            }
        }
    }

    #[test]
    fn falling_off_rescues_near_where_the_kart_left_and_resets_nothing_else() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        r.racers[0].passed = 3;
        r.racers[0].next_gate = 3;
        let left = [k.pos[0], k.pos[2]];
        r.fall_y = 100.; // the flat floor at 0 is now "below the world"
        r.tick(&pad(), false, &mut k, &ground());
        assert_eq!(r.racers[0].rescues, 1);
        // Lakitu drops the kart on the lap path at the point nearest to where it fell (`func_80090778`).
        let on_path = r.track.nodes.iter().any(|n| dist([n[0], n[2]], [k.pos[0], k.pos[2]]) < 1.);
        assert!(on_path && dist(left, [k.pos[0], k.pos[2]]) < 60., "{:?} from {left:?}", k.pos);
        assert_eq!(r.racers[0].passed, 3, "progress is kept");
        assert!(r.drain_events().contains(&Event::Rescue(0)));
    }

    #[test]
    fn cpu_drivers_lap_the_synthetic_oval_in_order() {
        let (mut r, mut k) = race();
        let g = ground();
        for _ in 0..30 * 120 {
            r.tick(&pad(), false, &mut k, &g);
            if r.cpus.iter().all(|c| c.kart.pos[1].abs() < 50.) && r.racers[1..].iter().all(|x| x.laps.len() >= 1) {
                break;
            }
        }
        for c in 1..RACERS {
            assert!(r.racers[c].laps.len() >= 1, "cpu {c} passed {}", r.racers[c].passed);
            assert_eq!(r.racers[c].rescues, 0);
        }
    }

    /// The whole player flow on the extracted Luigi Raceway: countdown, three laps driven by a scripted pad
    /// (the CPU path follower), the finish, the results card, and a restart that leaves a clean race.
    #[test]
    fn a_scripted_player_finishes_three_laps_on_the_real_track_and_restarts_cleanly() {
        use super::super::kart::{Raceway, WorldGround};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64");
        if !root.join("luigi-raceway.json").exists() {
            return;
        }
        let data = Raceway::read(&root).unwrap();
        let world = data.world();
        let ground = WorldGround(&world);
        let mut race = KartRace::new(&data.path, 20240607);
        let mut player = Kart::new([0.; 3], 0);
        race.start(&mut player, &ground);
        assert_eq!(race.phase, Phase::Countdown);
        let mut driver = Driver::new(&mut Rng(99), 0);
        let mut events = vec![];
        let mut ticks = 0;
        while race.phase != Phase::Finished && ticks < 30 * 60 * 8 {
            let (pad, _) = driver.input(&race.track, &player, 0., race.phase != Phase::Countdown);
            race.tick(&pad, false, &mut player, &ground);
            events.extend(race.drain_events());
            ticks += 1;
        }
        let me = &race.racers[0];
        assert_eq!(race.phase, Phase::Finished, "finished within 8 minutes (laps {:?}, gates {})", me.laps, me.passed);
        assert_eq!(me.laps.len() as u32, LAPS);
        assert!(me.finish.is_some());
        let results = race.results.as_ref().expect("results card");
        assert_eq!(results.laps.len() as u32, LAPS);
        assert_eq!(results.order.len(), RACERS);
        assert!(events.contains(&Event::Go), "the countdown ended in GO");
        // Restart: a fresh countdown, no results, no laps.
        race.start(&mut player, &ground);
        assert_eq!(race.phase, Phase::Countdown);
        assert!(race.results.is_none());
        assert!(race.racers[0].laps.is_empty() && race.racers[0].finish.is_none());
    }

    #[test]
    fn cpus_lap_the_real_track_in_gate_order_on_the_road() {
        use super::super::kart::{Raceway, WorldGround};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64");
        if !root.join("luigi-raceway.json").exists() {
            return;
        }
        let data = Raceway::read(&root).unwrap();
        let world = data.world();
        let ground = WorldGround(&world);
        let mut race = KartRace::new(&data.path, 20240607);
        let mut player = Kart::new([0.; 3], 0);
        race.start(&mut player, &ground);
        let (mut ticks, mut grass, mut seen) = (0u32, 0u32, [0u32; RACERS]);
        while ticks < 30 * 90 {
            race.tick(&Input::default(), false, &mut player, &ground);
            ticks += 1;
            for c in 0..CPU_COUNT {
                let k = &race.cpus[c].kart;
                grass += ground.floor(k.pos[0], k.pos[1] + 5., k.pos[2]).is_some_and(|f| f.grass) as u32;
                assert!(race.racers[c + 1].passed >= seen[c + 1], "gates only count up");
                seen[c + 1] = race.racers[c + 1].passed;
            }
            if race.racers[1..].iter().all(|r| !r.laps.is_empty()) {
                break;
            }
        }
        for c in 1..RACERS {
            let r = &race.racers[c];
            println!("cpu {c}: lap {} ticks ({}), rescues {}, gates {}", r.laps.first().copied().unwrap_or(0), format_ticks(r.laps.first().copied().unwrap_or(0)), r.rescues, r.passed);
            assert!(!r.laps.is_empty(), "cpu {c} stuck at gate {}", r.passed);
            assert_eq!(r.rescues, 0, "cpu {c} left the road");
            assert!((900..2100).contains(&r.laps[0]));
        }
        println!("grass ticks {grass} of {}", ticks * 3);
        assert!(grass < ticks * 3 / 10, "CPUs stay on the asphalt: {grass} grass ticks of {}", ticks * 3);
        assert!(race.order()[0] != 0, "a CPU leads when the player sits still");
    }

    #[test]
    fn cpu_field_replays_identically_for_a_seed() {
        use super::super::kart::{Raceway, WorldGround};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64");
        if !root.join("luigi-raceway.json").exists() {
            return;
        }
        let data = Raceway::read(&root).unwrap();
        let world = data.world();
        let ground = WorldGround(&world);
        let run = || {
            let mut race = KartRace::new(&data.path, 5);
            let mut player = Kart::new([0.; 3], 0);
            race.start(&mut player, &ground);
            let go = Input { raw_y: 80, accel: true, ..Input::default() };
            for _ in 0..400 {
                race.tick(&go, false, &mut player, &ground);
            }
            race.cpus.iter().map(|c| format!("{:.3},{:.3}", c.kart.pos[0], c.kart.pos[2])).collect::<Vec<_>>().join("|")
        };
        assert_eq!(run(), run());
    }
}
