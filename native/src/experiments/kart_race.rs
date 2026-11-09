//! Race mode for the MK64 experiment, engine-free: ordered checkpoints, laps, a 3-2-1-GO start,
//! three CPU drivers on the shared `kart_physics` core, ranking and the item rules (mushroom, banana,
//! green shell). `src/game/experiments/kartRace.ts` is the line-for-line browser twin; the rules are
//! checked by the unit tests in both. Everything runs on the 30 Hz sim tick (`KartRace::tick` steps the
//! 60 Hz kart physics twice) in MK64 course units (the `path` of `luigi-raceway.json`).
//!
//! Simplifications against MK64, all deliberate: no start-line early-boost spinout, no kart-to-kart
//! collision (karts pass through each other), no item blocks for the CPU's decision tree beyond a timer
//! and a cone check, no Lakitu animation (a fall puts you back on the last gate), and a spinout is a
//! speed loss plus a visual wobble rather than the decomp's tumble.
use super::kart_cpu::{CpuDriver, Neighbor, PathInfo, RaceSetup, RaceView};
use super::kart_physics::{Ground, Input, Kart, Wall};
use super::kart_stats::KartStats;
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
pub const SPIN_TICKS: u32 = 36;
pub const IMMUNE_TICKS: u32 = 54;
pub const BOOST_TICKS: u32 = 42;
pub const ROULETTE_TICKS: u32 = 45;
pub const BOX_RESPAWN: u32 = 150;
pub const BOX_RADIUS: f64 = 30.;
pub const HIT_RADIUS: f64 = 24.;
/// Raw units per 60 Hz frame.
pub const SHELL_SPEED: f64 = 15.;
pub const SHELL_LIFE: u32 = 240;
pub const MAX_BANANAS: usize = 12;
const SPIN_DAMP: f64 = 0.94;
const PLAYER_SLOT: usize = 3;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Mushroom,
    Banana,
    Shell,
}
impl Item {
    pub const ALL: [Item; 3] = [Item::Mushroom, Item::Banana, Item::Shell];
    pub fn name(self) -> &'static str {
        match self {
            Item::Mushroom => "Mushroom",
            Item::Banana => "Banana",
            Item::Shell => "Green shell",
        }
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
    pub item: Option<Item>,
    pub roulette: u32,
    pending: Item,
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
    fn new(pos: [f64; 2]) -> Self {
        Self { passed: 0, next_gate: 0, last: pos, hint: 0, spin: 0, immune: 0, boost: 0, item: None, roulette: 0, pending: Item::Mushroom, lap_start: 0, laps: vec![], finish: None, dist_next: 0., stuck: 0, hold: 0, rescues: 0 }
    }
    /// Lap shown on the HUD (1-based).
    pub fn lap(&self) -> u32 {
        (self.laps.len() as u32 + 1).min(LAPS)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Banana {
    pub pos: [f64; 2],
    pub y: f64,
    pub owner: usize,
    pub age: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct Shell {
    pub pos: [f64; 2],
    pub y: f64,
    pub dir: [f64; 2],
    pub bounces: u32,
    pub life: u32,
    pub owner: usize,
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

/// Roulette weights per rank (0 = leading): [mushroom, banana, shell].
const WEIGHTS: [[u32; 3]; RACERS] = [[2, 5, 3], [4, 4, 3], [5, 2, 3], [6, 1, 3]];

pub struct KartRace {
    pub track: Track,
    pub phase: Phase,
    pub clock: u32,
    pub race_tick: u32,
    pub countdown: u32,
    pub racers: Vec<Racer>,
    pub cpus: Vec<Cpu>,
    pub bananas: Vec<Banana>,
    pub shells: Vec<Shell>,
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

/// One 30 Hz tick of a kart: spin/boost on top of the physics core, two 60 Hz updates.
fn step_kart(kart: &mut Kart, st: &mut Racer, input: &Input, racing: bool, ground: &dyn Ground) {
    let pad = if !racing || st.spin > 0 { Input::default() } else { *input };
    for f in 0..2 {
        if st.boost > 0 {
            // The mushroom reuses the mini-turbo thrust: keep the flag raised for its duration.
            kart.mini_turbo = true;
        }
        kart.step(&Input { hop_pressed: pad.hop_pressed && f == 0, ..pad }, ground);
        if st.spin > 0 {
            for v in kart.velocity.iter_mut() {
                *v *= SPIN_DAMP;
            }
            kart.current_speed *= SPIN_DAMP;
        }
    }
    st.spin = st.spin.saturating_sub(1);
    st.immune = st.immune.saturating_sub(1);
    st.boost = st.boost.saturating_sub(1);
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
            bananas: vec![],
            shells: vec![],
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
        self.bananas.clear();
        self.shells.clear();
        self.racers.clear();
        self.cpus.clear();
        self.stall = vec![(f64::MIN, 0); RACERS];
        self.rescue_log.clear();
        let y0 = self.track.nodes[0][1];
        for slot in 0..RACERS {
            let (p, rot) = self.track.grid(slot);
            let y = Self::floor_y(ground, p[0], y0, p[1]);
            let index = if slot == PLAYER_SLOT { 0 } else { slot + 1 };
            let kart = self.new_kart(index, [p[0], y, p[1]], rot);
            let mut racer = Racer::new(p);
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
        self.bananas.clear();
        self.shells.clear();
        self.cpus.clear();
        self.racers = vec![Racer::new([player.pos[0], player.pos[2]])];
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

    /// Visual spin of racer `i` in radians (0 when not spinning).
    pub fn spin_angle(&self, i: usize) -> f64 {
        let s = self.racers[i].spin;
        if s == 0 { 0. } else { (1. - s as f64 / SPIN_TICKS as f64) * 2. * PI }
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

    fn roll(&mut self, rank: usize) -> Item {
        // Alone in free roam there is no rank: use the middle of the pack.
        let w = WEIGHTS[if self.phase == Phase::Practice { 1 } else { rank.min(RACERS - 1) }];
        let total: u32 = w.iter().sum();
        let mut pick = (self.rng.unit() * total as f64) as u32;
        for (k, weight) in w.iter().enumerate() {
            if pick < *weight {
                return Item::ALL[k];
            }
            pick -= weight;
        }
        Item::Mushroom
    }

    fn hit(&mut self, i: usize, player: &mut Kart) {
        let r = &mut self.racers[i];
        if r.spin > 0 || r.immune > 0 || r.finish.is_some() && i != 0 {
            return;
        }
        r.spin = SPIN_TICKS;
        r.immune = SPIN_TICKS + IMMUNE_TICKS;
        r.boost = 0;
        let kart = if i == 0 { player } else { &mut self.cpus[i - 1].kart };
        for v in kart.velocity.iter_mut() {
            *v *= 0.35;
        }
        kart.current_speed *= 0.35;
        self.events.push(Event::Hit(i));
    }

    fn use_item(&mut self, i: usize, player: &Kart, ground: &dyn Ground) {
        let Some(item) = self.racers[i].item.take() else { return };
        let kart = if i == 0 { player } else { &self.cpus[i - 1].kart };
        let (f, p, y) = (forward(kart), [kart.pos[0], kart.pos[2]], kart.pos[1]);
        match item {
            Item::Mushroom => self.racers[i].boost = BOOST_TICKS,
            Item::Banana => {
                if self.bananas.len() >= MAX_BANANAS {
                    self.bananas.remove(0);
                }
                let q = [p[0] - f[0] * 30., p[1] - f[1] * 30.];
                self.bananas.push(Banana { pos: q, y: Self::floor_y(ground, q[0], y, q[1]), owner: i, age: 0 });
            }
            Item::Shell => {
                let q = [p[0] + f[0] * 28., p[1] + f[1] * 28.];
                self.shells.push(Shell { pos: q, y, dir: f, bounces: 0, life: 0, owner: i });
            }
        }
        self.events.push(Event::Use(i, item));
    }

    /// Straight-line items, one tick: shells advance (bouncing off one wall), bananas age.
    fn step_projectiles(&mut self, ground: &dyn Ground) {
        for b in self.bananas.iter_mut() {
            b.age += 1;
        }
        let mut shells = std::mem::take(&mut self.shells);
        shells.retain_mut(|s| {
            s.life += 1;
            if s.life > SHELL_LIFE {
                return false;
            }
            for _ in 0..4 {
                let step = SHELL_SPEED * 2. / 4.;
                let np = [s.pos[0] + s.dir[0] * step, s.pos[1] + s.dir[1] * step];
                let Some(floor) = ground.floor(np[0], s.y + 30., np[1]) else { return false };
                s.y = floor.height;
                if let Some(w) = ground.wall([np[0], s.y, np[1]]) {
                    if s.bounces >= 1 {
                        return false;
                    }
                    s.bounces += 1;
                    // The wall's own normal (pointing out of it) when the physics ground reports one, else the push-out direction.
                    let flat = w.hits.first().map_or(0., |h| h.normal[0].hypot(h.normal[2]));
                    let n = match w.hits.first() {
                        Some(h) if flat > 0.1 => norm([h.normal[0], h.normal[2]]),
                        _ => norm([w.pos[0] - np[0], w.pos[2] - np[1]]),
                    };
                    let dot = s.dir[0] * n[0] + s.dir[1] * n[1];
                    s.dir = norm([s.dir[0] - 2. * dot * n[0], s.dir[1] - 2. * dot * n[1]]);
                    s.pos = [w.pos[0], w.pos[2]];
                    continue;
                }
                s.pos = np;
            }
            true
        });
        self.shells = shells;
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
                if self.countdown == 0 {
                    self.phase = Phase::Racing;
                    self.race_tick = 0;
                    self.events.push(Event::Go);
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
        // Boxes and roulette.
        for b in 0..self.boxes.len() {
            if self.boxes[b].respawn > 0 {
                self.boxes[b].respawn -= 1;
                continue;
            }
            for i in 0..self.racers.len() {
                let r = &self.racers[i];
                if r.item.is_some() || r.roulette > 0 || r.finish.is_some() {
                    continue;
                }
                let k = self.kart_pos(i, player);
                if dist([k[0], k[2]], self.boxes[b].pos) < BOX_RADIUS {
                    self.boxes[b].respawn = BOX_RESPAWN;
                    let rank = self.rank(i);
                    let item = self.roll(rank);
                    let r = &mut self.racers[i];
                    r.roulette = ROULETTE_TICKS;
                    r.pending = item;
                    r.hold = 0;
                    self.events.push(Event::Pickup(i));
                    break;
                }
            }
        }
        for i in 0..self.racers.len() {
            let r = &mut self.racers[i];
            if r.roulette > 0 {
                r.roulette -= 1;
                if r.roulette == 0 {
                    r.item = Some(r.pending);
                }
            } else if r.item.is_some() {
                r.hold += 1;
            }
        }
        // Use.
        if use_item && matches!(self.phase, Phase::Racing | Phase::Practice) && self.racers[0].spin == 0 {
            self.use_item(0, player, ground);
        }
        for c in 0..self.cpus.len() {
            let i = c + 1;
            let Some(item) = self.racers[i].item else { continue };
            if self.racers[i].spin > 0 {
                continue;
            }
            let hold = self.racers[i].hold;
            let go = match item {
                Item::Mushroom => hold > 20 + 7 * c as u32,
                Item::Banana => hold > 45 + 20 * c as u32,
                Item::Shell => {
                    let me = &self.cpus[c].kart;
                    let (f, p) = (forward(me), [me.pos[0], me.pos[2]]);
                    let target = (0..self.racers.len()).filter(|j| *j != i).any(|j| {
                        let k = self.kart_pos(j, player);
                        let d = [k[0] - p[0], k[2] - p[1]];
                        let l = d[0].hypot(d[1]);
                        l > 40. && l < 900. && (d[0] * f[0] + d[1] * f[1]) / l > 0.96
                    });
                    target || hold > 300
                }
            };
            if go {
                self.use_item(i, player, ground);
            }
        }
        self.step_projectiles(ground);
        // Hits.
        let mut hits: Vec<(usize, Item)> = vec![];
        for i in 0..self.racers.len() {
            let k = self.kart_pos(i, player);
            let p = [k[0], k[2]];
            if self.racers[i].spin > 0 || self.racers[i].immune > 0 {
                continue;
            }
            if let Some(b) = self.bananas.iter().position(|b| b.age >= if b.owner == i { 24 } else { 4 } && dist(b.pos, p) < HIT_RADIUS) {
                self.bananas.remove(b);
                hits.push((i, Item::Banana));
                continue;
            }
            if let Some(s) = self.shells.iter().position(|s| (s.owner != i || s.life > 10) && dist(s.pos, p) < HIT_RADIUS + 6.) {
                self.shells.remove(s);
                hits.push((i, Item::Shell));
            }
        }
        for (i, item) in hits {
            self.hit(i, player);
            self.events.push(Event::Smash(i, item));
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
        for _ in 0..COUNTDOWN_TICKS - 1 {
            r.tick(&go, false, &mut k, &g);
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
    fn roulette_pays_out_after_the_spin_and_weights_follow_rank() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        let g = ground();
        let b = r.boxes[0].pos;
        k.pos = [b[0], 0., b[1]];
        r.tick(&pad(), false, &mut k, &g);
        assert!(r.racers[0].roulette > 0 && r.racers[0].item.is_none());
        assert!(r.drain_events().contains(&Event::Pickup(0)));
        assert!(r.boxes[0].respawn > 0);
        for _ in 0..ROULETTE_TICKS {
            r.tick(&pad(), false, &mut k, &g);
        }
        assert!(r.racers[0].item.is_some());
        // Holding an item, driving over another box gives nothing new.
        let held = r.racers[0].item;
        let b2 = r.boxes[1].pos;
        k.pos = [b2[0], 0., b2[1]];
        r.tick(&pad(), false, &mut k, &g);
        assert_eq!(r.racers[0].item, held);
        assert_eq!(r.racers[0].roulette, 0);
        let mut last = KartRace::new(&oval(), 1);
        last.rng = Rng(99);
        let (mut m_first, mut m_last) = (0, 0);
        for _ in 0..400 {
            if last.roll(0) == Item::Mushroom { m_first += 1 }
            if last.roll(3) == Item::Mushroom { m_last += 1 }
        }
        assert!(m_last > m_first * 2, "back markers get mushrooms: {m_first} vs {m_last}");
    }

    #[test]
    fn mushroom_boosts_banana_spins_and_a_spun_kart_loses_speed() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        let g = ground();
        let go = Input { raw_y: 80, accel: true, ..pad() };
        for _ in 0..180 {
            r.tick(&go, false, &mut k, &g);
        }
        let cruise = k.speed;
        r.racers[0].item = Some(Item::Mushroom);
        let mut fastest = 0f64;
        r.tick(&go, true, &mut k, &g);
        for _ in 0..BOOST_TICKS {
            r.tick(&go, false, &mut k, &g);
            fastest = fastest.max(k.speed);
        }
        assert!(fastest > cruise + 0.4, "boost {fastest} over {cruise}");
        assert!(r.racers[0].item.is_none());
        // A banana dropped by CPU 1 right in the player's path.
        for _ in 0..80 {
            r.tick(&go, false, &mut k, &g);
        }
        let speed_before = k.speed;
        let f = forward(&k);
        r.bananas.push(Banana { pos: [k.pos[0] + f[0] * 12., k.pos[2] + f[1] * 12.], y: 0., owner: 1, age: 10 });
        r.racers[0].immune = 0;
        r.tick(&go, false, &mut k, &g);
        assert!(r.racers[0].spin > 0, "banana spins the kart");
        assert!(r.bananas.is_empty(), "banana is used up");
        assert!(r.drain_events().contains(&Event::Hit(0)));
        for _ in 0..8 {
            r.tick(&go, false, &mut k, &g);
        }
        assert!(k.speed < speed_before * 0.5, "{} vs {speed_before}", k.speed);
        assert!(r.spin_angle(0) > 0.);
        // Spin ends, then a short grace period before it can happen again.
        for _ in 0..SPIN_TICKS {
            r.tick(&go, false, &mut k, &g);
        }
        assert_eq!(r.racers[0].spin, 0);
        assert!(r.racers[0].immune > 0);
    }

    #[test]
    fn own_banana_is_safe_at_first_and_dropping_puts_it_behind() {
        let (mut r, mut k) = race();
        r.phase = Phase::Racing;
        let g = ground();
        let go = Input { raw_y: 80, accel: true, ..pad() };
        for _ in 0..90 {
            r.tick(&go, false, &mut k, &g);
        }
        r.racers[0].item = Some(Item::Banana);
        r.tick(&go, true, &mut k, &g);
        let b = r.bananas[0];
        let f = forward(&k);
        assert!((b.pos[0] - k.pos[0]) * f[0] + (b.pos[1] - k.pos[2]) * f[1] < 0., "behind the kart");
        assert_eq!(r.racers[0].spin, 0);
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
        assert!(r.racers[0].item.is_none());
        assert!(r.drain_events().contains(&Event::Use(0, item)));
        k.pos[0] += 4000.;
        for _ in 0..200 {
            r.tick(&pad(), false, &mut k, &g);
        }
        assert_eq!(r.boxes[1].respawn, 0);
    }

    #[test]
    fn practice_rolls_are_reproducible_and_varied() {
        let rolls = || {
            let mut r = KartRace::new(&oval(), 9);
            r.start_practice(&Kart::new([0.; 3], 0), &ground());
            (0..30).map(|_| r.roll(0)).collect::<Vec<_>>()
        };
        assert_eq!(rolls(), rolls());
        let all = rolls();
        assert!(all.iter().any(|i| *i != all[0]));
    }

    #[test]
    fn a_banana_you_drive_back_over_spins_you_out_and_reports_what_hit() {
        let mut r = KartRace::new(&oval(), 4);
        let g = ground();
        let mut k = Kart::new([0.; 3], 0);
        r.start_practice(&k, &g);
        r.bananas.push(Banana { pos: [k.pos[0], k.pos[2]], y: 0., owner: 0, age: 40 });
        r.tick(&pad(), false, &mut k, &g);
        assert!(r.racers[0].spin > 0);
        assert!(r.drain_events().contains(&Event::Smash(0, Item::Banana)));
    }

    #[test]
    fn green_shell_flies_straight_bounces_once_then_breaks_and_hits() {
        let mut r = KartRace::new(&oval(), 3);
        let g = Flat { wall_x: Some(160.) };
        let mut shell = Shell { pos: [0., 0.], y: 0., dir: [1., 0.], bounces: 0, life: 0, owner: 0 };
        r.shells.push(shell);
        let mut xs = vec![];
        for _ in 0..40 {
            r.step_projectiles(&g);
            if let Some(s) = r.shells.first() {
                xs.push((s.pos[0], s.dir[0], s.bounces));
            }
        }
        assert!(xs[0].0 > 25. && xs[0].0 < 35., "{:?}", xs[0]);
        assert!(xs.iter().any(|x| x.2 == 1 && x.1 < 0.), "bounced back from the wall");
        let flew_back = xs.iter().filter(|x| x.2 == 1).last().unwrap();
        assert!(flew_back.0 < 160.);
        // Back across the track it meets the wall of... nothing (flat floor): it lives to expire.
        for _ in 0..SHELL_LIFE {
            r.step_projectiles(&g);
        }
        assert!(r.shells.is_empty(), "shell expires");
        // Second wall contact destroys it.
        shell.pos = [150., 0.];
        shell.bounces = 1;
        r.shells.push(shell);
        for _ in 0..4 {
            r.step_projectiles(&g);
        }
        assert!(r.shells.is_empty());
        // And a shell that reaches a kart spins it.
        let (mut race, mut k) = race();
        race.phase = Phase::Racing;
        let f = forward(&k);
        race.shells.push(Shell { pos: [k.pos[0] - f[0] * 60., k.pos[2] - f[1] * 60.], y: 0., dir: f, bounces: 0, life: 20, owner: 1 });
        race.racers[0].immune = 0;
        for _ in 0..4 {
            race.tick(&pad(), false, &mut k, &ground());
        }
        assert!(race.racers[0].spin > 0, "shell hit");
        assert!(race.shells.is_empty());
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
