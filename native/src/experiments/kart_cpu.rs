//! Port of Mario Kart 64's CPU driver (n64decomp/mk64 `src/cpu_vehicles_camera_path.c` and its includes
//! `cpu_vehicles_camera_path/{path_calc,path_utils,behaviour_utils,cpu_speed_control}.inc.c`). Engine-free.
//!
//! The game drives its CPU karts from the lap path (`d_course_*_track_path`): every path point gets a left and a
//! right boundary at `cpu_CourseMaximumSeparation`, the kart steers at a point `lookAhead` path points ahead at a
//! lateral "track position factor" (-1 left edge .. +1 right edge), the target speed depends on whether the point is
//! in a curve (`gTrackConsecutiveCurveCounts`), the course's `CPUBehaviour` list (read from the ROM by
//! `scripts/export-mk64-cpu.py`) makes it drift / hop / change lane, and `func_800088D8` is the rubber band against
//! the human player. All of that is `CpuDriver::control`, which runs once per 30 Hz tick (the decomp decides on every
//! other 60 Hz frame and repeats the last steering in between).
//!
//! Differences from the game, all noted where they happen: one path (Yoshi Valley's three side paths and Kalimari's
//! trains are not modelled), the nearest path point is searched by distance alone (the game also filters by collision
//! "track section id"), and the Lakitu rescue is the host's (`kart_race.rs`).

use serde::Deserialize;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::Path;

use super::kart_physics::{Ground, Input, Kart};
use super::kart_stats::{Cc, KartStats, KartTables};

/// 1.5 degrees: the steering clamp of `update_player`.
const MIN_ANGLE: i32 = 273;

// ---------------------------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------------------------

/// One course's entry of `cpu-ai.json`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseAi {
    /// `cpu_CourseMaximumSeparation`: half the width the CPUs may use around the path.
    pub max_separation: f64,
    /// `cpu_CourseMinimumSeparation`: lane spacing at the start.
    pub min_separation: f64,
    /// `cpu_SteeringSensitivity`.
    pub steering: i32,
    /// `cpu_NormalTargetSpeed[cc]`, `cpu_CurveTargetSpeed`, `cpu_OffTrackTargetSpeed`, `D_0D0096B8`.
    pub normal_speed: [f64; 4],
    pub curve_speed: [f64; 4],
    pub off_track_speed: [f64; 4],
    pub slow_speed: [f64; 4],
    /// `CPUBehaviour`: `[pathPointStart, pathPointEnd, type]`.
    pub behaviour: Vec<[i32; 3]>,
}
impl CourseAi {
    /// Luigi Raceway's numbers (used when `cpu-ai.json` is missing, and by the synthetic-oval tests).
    pub fn fallback() -> Self {
        Self {
            max_separation: 50.,
            min_separation: 0.7,
            steering: 48,
            normal_speed: [3.75, 5.166_666_5, 5.75, 6.333_333_5],
            curve_speed: [4.166_666_5, 5.583_333_5, 6.166_666_5, 6.75],
            off_track_speed: [3.75, 5.166_666_5, 5.75, 6.333_333_5],
            slow_speed: [3.333_333_2, 3.916_666_7, 4.5, 5.083_333_5],
            behaviour: vec![[1, 2, 6]],
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct AiFile {
    courses: HashMap<String, CourseAi>,
    kart: KartTables,
}

/// Everything `cpu-ai.json` holds: the CPU tables per course and the racers' kart constants.
#[derive(Clone, Debug)]
pub struct CpuAi {
    courses: HashMap<String, CourseAi>,
    pub kart: KartTables,
}
impl CpuAi {
    /// `root` is the folder `scripts/export-mk64.py` writes (`public/mk64`).
    pub fn load(root: &Path) -> Option<Self> {
        let bytes = std::fs::read(root.join("cpu-ai.json")).ok()?;
        let file: AiFile = serde_json::from_slice(&bytes).ok()?;
        Some(Self { courses: file.courses, kart: file.kart })
    }
    pub fn course(&self, dir: &str) -> CourseAi {
        self.courses.get(dir).cloned().unwrap_or_else(CourseAi::fallback)
    }
}

/// The settings a race is started with (kart setup menu).
#[derive(Clone, Debug)]
pub struct RaceSetup {
    pub cc: Cc,
    /// The player's racer (`characterId`); the CPUs take the next three.
    pub character: usize,
    pub course: CourseAi,
    pub tables: Option<KartTables>,
}
impl Default for RaceSetup {
    fn default() -> Self {
        Self { cc: Cc::Cc150, character: 0, course: CourseAi::fallback(), tables: None }
    }
}
impl RaceSetup {
    pub fn stats(&self, character: usize) -> KartStats {
        match &self.tables {
            Some(t) => t.stats(character, self.cc),
            None => KartStats { character, cc: self.cc, ..KartStats::MARIO_150 },
        }
    }
    /// The racer of CPU `c` (0..3): the next characters after the player's, as the game fills the grid with the
    /// characters nobody picked.
    pub fn cpu_character(&self, c: usize) -> usize {
        // Luigi, Peach, Toad, Yoshi, Wario, Bowser, D.K., Mario: the first three nobody else picked.
        const ORDER: [usize; 8] = [1, 6, 3, 2, 5, 7, 4, 0];
        ORDER.iter().copied().filter(|r| *r != self.character).nth(c).unwrap_or(1)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The processed path (path_calc.inc.c)
// ---------------------------------------------------------------------------------------------------------------

/// `gTrackSectionTypes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Straight,
    RightLean,
    LeftLean,
    Right,
    Left,
}

#[derive(Clone, Debug)]
pub struct PathInfo {
    pub pts: Vec<[f64; 3]>,
    /// `gTrackLeftPaths` / `gTrackRightPaths` (x, z).
    pub left: Vec<[f64; 2]>,
    pub right: Vec<[f64; 2]>,
    pub section: Vec<Section>,
    /// `gPathExpectedRotation` (s16 as i32).
    pub rotation: Vec<i32>,
    /// `gTrackConsecutiveCurveCounts`.
    pub curves: Vec<i32>,
}

fn s16(v: f64) -> i32 {
    (v as i32) as i16 as i32
}
fn wrap16(v: i32) -> i32 {
    v as i16 as i32
}
/// `atan2s(dx, dz)` negated, i.e. the kart `rotation` that faces from `a` to `b`.
fn heading(a: [f64; 2], b: [f64; 2]) -> i32 {
    s16(-(b[0] - a[0]).atan2(b[1] - a[1]) * 32768. / PI)
}

impl PathInfo {
    /// `nodes` are the lap path points (x, y, z in course units, sentinel nodes already removed), `max_separation`
    /// the course's `cpu_CourseMaximumSeparation`.
    pub fn new(nodes: &[[f64; 3]], max_separation: f64) -> Self {
        let n = nodes.len();
        let xz = |i: usize| [nodes[i % n][0], nodes[i % n][2]];
        // calculate_track_boundaries
        let (mut left, mut right) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for i in 0..n {
            let (a, b) = (xz(i), xz(i + 1));
            let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
            let len = dx.hypot(dz);
            if len < 1e-9 {
                left.push(a);
                right.push(a);
                continue;
            }
            left.push([max_separation * dz / len + a[0], max_separation * (-dx) / len + a[1]]);
            right.push([max_separation * (-dz) / len + a[0], max_separation * dx / len + a[1]]);
        }
        // analyze_track_sections
        let curvature = |i: usize| -> f64 {
            let (p1, p2, p3) = (xz(i), xz(i + 1), xz(i + 2));
            let f = [(p2[0] + p3[0]) * 0.5 - p1[0], (p2[1] + p3[1]) * 0.5 - p1[1]];
            let (q1, q2, q3) = (xz(i + 3), xz(i + 4), xz(i + 5));
            let s = [(q2[0] + q3[0]) * 0.5 - q1[0], (q2[1] + q3[1]) * 0.5 - q1[1]];
            let (fl, sl) = (f[1].hypot(f[0]), s[1].hypot(s[0]));
            -((f[1] * s[0]) - (f[0] * s[1])) / (sl * fl)
        };
        let mut section: Vec<Section> = (0..n)
            .map(|i| {
                let c = curvature(i);
                if c > 0.1 {
                    Section::Right
                } else if c < -0.1 {
                    Section::Left
                } else {
                    Section::Straight
                }
            })
            .collect();
        let mut i = 0;
        while i < n {
            if section[i] == Section::Straight {
                let mut j = 1;
                while j < n {
                    match section[(i + j) % n] {
                        Section::RightLean | Section::Right => {
                            for k in 0..j {
                                section[(i + k) % n] = Section::RightLean;
                            }
                            i += j;
                            j = n;
                        }
                        Section::LeftLean | Section::Left => {
                            for k in 0..j {
                                section[(i + k) % n] = Section::LeftLean;
                            }
                            i += j;
                            j = n;
                        }
                        Section::Straight => {}
                    }
                    j += 1;
                }
            }
            i += 1;
        }
        // analyze_path_angle
        let rotation = (0..n).map(|i| heading(xz(i), xz(i + 1))).collect();
        // analyze_curved_path
        let curves = (0..n)
            .map(|i| {
                let mut count = 0;
                for j in 0..n {
                    if matches!(section[(i + j) % n], Section::LeftLean | Section::RightLean) {
                        count += 1;
                    } else {
                        break;
                    }
                }
                count
            })
            .collect();
        Self { pts: nodes.to_vec(), left, right, section, rotation, curves }
    }
    pub fn len(&self) -> usize {
        self.pts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pts.is_empty()
    }
    /// `calculate_track_position_factor`: -1 on the left boundary, 1 on the right one.
    pub fn factor(&self, p: [f64; 2], point: usize) -> f64 {
        let (l, r) = (self.left[point], self.right[point]);
        let sq = (r[0] - l[0]).powi(2) + (r[1] - l[1]).powi(2);
        if sq < 0.01 {
            return 0.;
        }
        (2. * ((r[0] - l[0]) * (p[0] - l[0]) + (r[1] - l[1]) * (p[1] - l[1]))) / sq - 1.
    }
    /// `set_track_offset_position`: the point between the boundaries at `offset` (-1 left .. 1 right), averaged over
    /// path points `point` and `point + 1`.
    pub fn offset_position(&self, point: usize, offset: f64) -> [f64; 2] {
        let n = self.len();
        let (a, b) = (point % n, (point + 1) % n);
        let t0 = 0.5 - offset / 2.;
        let t1 = 1. - t0;
        [
            (t0 * (self.left[a][0] + self.left[b][0])) / 2. + (t1 * (self.right[a][0] + self.right[b][0])) / 2.,
            (t0 * (self.left[a][1] + self.left[b][1])) / 2. + (t1 * (self.right[a][1] + self.right[b][1])) / 2.,
        ]
    }
    /// `update_path_index` (window of the path points around the last one, within 400 units), else the nearest point
    /// anywhere (the game then asks the collision "track section"; there is none here).
    pub fn nearest(&self, pos: [f64; 3], hint: usize) -> usize {
        let n = self.len() as i64;
        let mut best: Option<(usize, f64)> = None;
        for k in -3..7i64 {
            let i = ((hint as i64 + k + n) % n) as usize;
            let p = self.pts[i];
            let d = (p[0] - pos[0]).powi(2) + (p[1] - pos[1]).powi(2) + (p[2] - pos[2]).powi(2);
            if d < 400. * 400. && best.is_none_or(|b| d < b.1) {
                best = Some((i, d));
            }
        }
        best.map(|b| b.0).unwrap_or_else(|| self.nearest_any(pos))
    }
    /// Nearest path point anywhere, in 3D (stacked roads: Choco Mountain's start straight crosses over the lap).
    pub fn nearest_any(&self, pos: [f64; 3]) -> usize {
        (0..self.len())
            .map(|i| (i, (self.pts[i][0] - pos[0]).powi(2) + (self.pts[i][1] - pos[1]).powi(2) + (self.pts[i][2] - pos[2]).powi(2)))
            .fold((0, f64::MAX), |a, b| if b.1 < a.1 { b } else { a })
            .0
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The driver
// ---------------------------------------------------------------------------------------------------------------

/// `cpu_BehaviourState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BehaviourState {
    None,
    Start,
    Running,
}
/// `gSpeedCPUBehaviour`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpeedMode {
    Normal,
    Fast,
    Slow,
    Max,
}

/// What the driver knows about another kart (`gNearestPathPointByPlayerId`, speed, `gTrackPositionFactor`).
#[derive(Clone, Copy, Debug)]
pub struct Neighbor {
    pub point: usize,
    pub speed: f64,
    pub factor: f64,
}

/// Race state the speed regulation looks at.
#[derive(Clone, Copy, Debug, Default)]
pub struct RaceView {
    /// `gNumPathPointsTraversed` of this kart and of the best human.
    pub traversed: i64,
    pub human_traversed: i64,
    /// 0-based race rank of this kart (`gGPCurrentRaceRankByPlayerIdDup`).
    pub rank: usize,
    /// Rank of the human.
    pub human_rank: usize,
    /// Completed laps (`gLapCountByPlayerId`, clamped to 3).
    pub lap: usize,
    /// Fraction of the lap (`gLapCompletionPercentByPlayerId`).
    pub lap_fraction: f64,
    /// How many CPUs (including this one) are ahead of the human: the index among the CPUs.
    pub cpu_rank: usize,
    pub racing: bool,
}

/// What `control` decided this tick (informational: the kart was already steered).
#[derive(Clone, Copy, Debug, Default)]
#[allow(dead_code)] // `fast` / `decel` are read by tests and debug traces
pub struct Control {
    pub steer: i32,
    pub accelerate: bool,
    pub fast: bool,
    pub decel: f64,
}
impl Control {
    /// The same decision as a human pad (for the scripted player bot): the stick value goes through the deadzone
    /// that `get_clamped_stickX_with_deadzone` removes again.
    pub fn pad(&self) -> Input {
        let raw_x = match self.steer.signum() {
            0 => 0,
            s => (self.steer + 12 * s).clamp(-80, 80),
        };
        Input { raw_x, raw_y: if self.accelerate { 40 } else { 0 }, accel: self.accelerate, brake: self.decel >= 2., hop_down: false, hop_pressed: false }
    }
}

#[derive(Clone, Debug)]
pub struct CpuDriver {
    /// Index among the racers' player ids (the decomp's `playerId`, 1.. for CPUs).
    pub slot: usize,
    pub point: usize,
    pub factor: f64,
    instr_current: f64,
    instr_target: f64,
    instr_unk_c: f64,
    look_ahead: i32,
    prev_steer: i32,
    prev_offset: Option<[f64; 2]>,
    state: BehaviourState,
    cur_id: usize,
    prev_id: usize,
    /// `D_801630E8`: 0 none, +-1 drifting right/left, +-2 stay out of the drift, 3 too slow.
    drift: i32,
    speed_mode: SpeedMode,
    /// `D_80162FF8` / `D_80163010` / `D_80163090`: lane change to get past a slower kart.
    avoid: i32,
    avoid_timer: i32,
    avoid_factor: f64,
    pub target_speed: f64,
    pub last: Control,
    prev_pos: [f64; 3],
    /// Set by `look_for_edges`: the road ends just ahead to the side (see there).
    edge: bool,
}

impl CpuDriver {
    pub fn new(slot: usize, point: usize, min_separation: f64) -> Self {
        // init_players: lane `((i + 1) % 3) - 1` times the minimum separation.
        let lane = min_separation * (((slot + 1) % 3) as f64 - 1.);
        Self {
            slot,
            point,
            factor: 0.,
            instr_current: 0.,
            instr_target: lane,
            instr_unk_c: lane,
            look_ahead: 6,
            prev_steer: 0,
            prev_offset: None,
            state: BehaviourState::None,
            cur_id: 0,
            prev_id: 0,
            drift: 0,
            speed_mode: SpeedMode::Normal,
            avoid: 0,
            avoid_timer: 0,
            avoid_factor: 0.,
            target_speed: 0.,
            last: Control::default(),
            prev_pos: [0.; 3],
            edge: false,
        }
    }

    /// `reset_cpu_behaviour`: the kart crossed the finish line.
    pub fn on_finish_line(&mut self) {
        self.cur_id = 0;
        self.prev_id = 0;
        self.state = BehaviourState::Start;
    }

    /// Put the driver back on the path after a rescue.
    pub fn relocate(&mut self, point: usize) {
        self.point = point;
        self.drift = 0;
        self.prev_steer = 0;
        self.prev_offset = None;
        self.instr_current = 0.;
    }

    /// Not in the game: probe the floor a few frames ahead along the kart's motion. When the road ends there but the
    /// lap path itself continues on solid ground (a rail-less edge, not a jump), the next `control` aims back at the
    /// middle of the road. The decomp's CPUs turn their heading straight onto the aim point, which keeps them on
    /// narrow roads (Banshee Boardwalk, Rainbow Road, Sherbet Land's ice); ours steer through the human physics and slide.
    pub fn look_for_edges(&mut self, path: &PathInfo, kart: &Kart, ground: &dyn Ground) {
        self.edge = false;
        if kart.midair || path.is_empty() {
            return;
        }
        let v = [kart.velocity[0], kart.velocity[2]];
        let len = v[0].hypot(v[1]);
        if len < 0.5 {
            return;
        }
        let d = [v[0] / len, v[1] / len];
        let solid = |x: f64, y: f64, z: f64| ground.floor(x, y + 25., z).is_some_and(|f| f.height > y - 30.);
        let n = path.len();
        // Along the motion, and sideways towards the nearer boundary of the lane (`gTrackLeftPaths`/`RightPaths`).
        let edge = if self.factor >= 0. { path.right[self.point % n] } else { path.left[self.point % n] };
        let mid = path.pts[self.point % n];
        let side = {
            let (sx, sz) = (edge[0] - mid[0], edge[1] - mid[2]);
            let l = sx.hypot(sz).max(1e-6);
            [sx / l, sz / l]
        };
        let probes = [
            (d[0] * 12., d[1] * 12.),
            (d[0] * 24., d[1] * 24.),
            (d[0] * 40., d[1] * 40.),
            (d[0] * 16. + side[0] * 14., d[1] * 16. + side[1] * 14.),
            (d[0] * 30. + side[0] * 20., d[1] * 30. + side[1] * 20.),
        ];
        for (dx, dz) in probes {
            let (x, z) = (kart.pos[0] + dx, kart.pos[2] + dz);
            if solid(x, kart.pos[1], z) {
                continue;
            }
            // Is the road itself still there ahead (centre of the path a few points on)?
            let ahead = path.pts[(self.point + 3) % n];
            if solid(ahead[0], ahead[1], ahead[2]) {
                self.edge = true;
            }
            return;
        }
    }

    /// `cpu_track_position_factor`.
    fn step_instruction(&mut self) -> f64 {
        let step = 0.015;
        if self.instr_current < self.instr_target {
            self.instr_current = (self.instr_current + step).min(self.instr_target);
        } else if self.instr_target < self.instr_current {
            self.instr_current = (self.instr_current - step).max(self.instr_target);
        }
        self.instr_current
    }

    /// `cpu_behaviour` -> `cpu_behaviour_start` / `cpu_behaviour_end`, run when the nearest path point changed.
    fn behaviour(&mut self, ai: &CourseAi, path: &PathInfo, kart: &mut Kart) {
        match self.state {
            BehaviourState::None => {}
            BehaviourState::Start => {
                let Some(row) = ai.behaviour.get(self.cur_id).copied() else { return };
                let (start, kind) = (row[0], row[2]);
                if row[0] == -1 && row[1] == -1 {
                    self.cur_id = 0;
                    self.prev_id = 0;
                    self.state = BehaviourState::None;
                    return;
                }
                if self.point as i32 != start {
                    return;
                }
                self.state = BehaviourState::Running;
                self.prev_id = self.cur_id;
                self.cur_id += 1;
                match kind {
                    1 => self.start_drift(path, kart),
                    2 => {
                        kart.cpu_hop();
                        kart.cpu_end_drift();
                        self.drift = 0;
                    }
                    3 => self.instr_target = 0.,
                    4 => self.instr_target = -0.6,
                    5 => self.instr_target = 0.6,
                    6 => self.speed_mode = SpeedMode::Normal,
                    7 => self.speed_mode = SpeedMode::Fast,
                    8 => self.speed_mode = SpeedMode::Slow,
                    11 => self.speed_mode = SpeedMode::Max,
                    _ => {}
                }
            }
            BehaviourState::Running => {
                let Some(row) = ai.behaviour.get(self.prev_id).copied() else { return };
                if self.point as i32 >= row[1] {
                    match row[2] {
                        1 => {
                            kart.cpu_end_drift();
                            self.drift = 0;
                            self.state = BehaviourState::Start;
                        }
                        3 | 4 | 5 => {
                            self.instr_target = self.instr_unk_c;
                            self.state = BehaviourState::Start;
                        }
                        2 | 6 | 7 | 8 | 9 | 10 | 11 => self.state = BehaviourState::Start,
                        _ => {}
                    }
                }
            }
        }
    }

    /// `func_80011EC0`: BEHAVIOUR_1 starts a drift into the coming curve when fast enough.
    fn start_drift(&mut self, path: &PathInfo, kart: &mut Kart) {
        if kart.kmh() >= 45. && self.drift == 0 {
            let steer = kart.steer_position >> 16;
            match path.section[self.point] {
                Section::RightLean | Section::Right => {
                    if steer >= -9 && self.avoid == 0 && self.factor > -0.8 && self.factor < 0.5 {
                        kart.cpu_start_drift();
                        self.drift = 1;
                    } else {
                        self.drift = 2;
                    }
                }
                Section::LeftLean | Section::Left => {
                    if steer < 10 && self.avoid == 0 && self.factor > -0.5 && self.factor < 0.8 {
                        kart.cpu_start_drift();
                        self.drift = -1;
                    } else {
                        self.drift = -2;
                    }
                }
                Section::Straight => {}
            }
        } else {
            self.drift = 3;
        }
    }

    /// `determine_ideal_cpu_position_offset`: the point to aim at.
    fn ideal_offset(&mut self, ai: &CourseAi, path: &PathInfo, point: usize, toads_turnpike: bool) -> [f64; 2] {
        let n = path.len();
        let curve_count = path.curves[point];
        let mut look = 6;
        if toads_turnpike {
            look = 7;
        } else if curve_count < 6 {
            look = 8;
        } else if curve_count >= 0x15 {
            look = 20;
        }
        let _ = ai;
        if look >= 8 {
            if self.factor > 0.75 && path.section[point] == Section::RightLean {
                look = 7;
            }
            if self.factor < -0.75 && path.section[point] == Section::LeftLean {
                look = 7;
            }
        }
        if self.factor >= 1.1 || self.factor <= -1.1 {
            look = 5;
        }
        if self.look_ahead < look {
            self.look_ahead += 1;
        }
        if look < self.look_ahead {
            self.look_ahead -= 1;
        }
        let current = self.step_instruction();
        let p = (self.look_ahead as usize + point) % n;
        let a = path.offset_position(p, current);
        let b = path.offset_position((p + 1) % n, current);
        [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
    }

    /// `func_8000B140`: move over when a slower kart is just ahead in this lane.
    fn avoid_slower(&mut self, kart: &Kart, character: usize, path: &PathInfo, others: &[Neighbor]) {
        if kart.drifting || self.drift == 1 || self.drift == -1 || self.factor < -1. || self.factor > 1. {
            return;
        }
        // Heavyweights (D.K., Wario, Bowser) hold their line.
        if matches!(character, 4 | 5 | 7) {
            return;
        }
        let n = path.len() as i64;
        let my_kmh = kart.kmh();
        if self.avoid_timer > 0 {
            self.avoid_timer -= 1;
            if self.avoid_timer <= 0 {
                self.avoid = 0;
            }
        }
        let mut near: Vec<(i64, f64, f64)> = vec![]; // (points ahead, speed difference, their factor)
        for o in others {
            let kmh = o.speed / 18. * 216.;
            let gap = my_kmh - 5.;
            if kmh < gap {
                let ahead = (o.point as i64 - self.point as i64).rem_euclid(n);
                if ahead <= 20 {
                    near.push((ahead.max(1), gap - kmh, o.factor));
                }
            }
            if near.len() >= 2 {
                break;
            }
        }
        if near.is_empty() {
            return;
        }
        let (mut lo, mut hi) = (1., -1.);
        for (ahead, diff, f) in near {
            if f > -1. && f < 1. {
                let width = ((0.2 * (20. / (ahead as f64 + 20.))) * (diff + 10.)) / 20.;
                if lo == 1. && hi == -1. {
                    lo = f - width;
                    hi = f + width;
                } else {
                    let (a, b) = (f - width, f + width);
                    if a < lo && b > lo {
                        lo = a;
                    }
                    if b > hi && a < hi {
                        hi = b;
                    }
                }
            }
        }
        if hi < lo || self.factor < lo || hi < self.factor {
            return;
        }
        hi = hi.min(1.);
        lo = lo.max(-1.);
        self.avoid_timer = 60;
        if (lo + 1.) < (1. - hi) {
            self.avoid = 1;
            self.avoid_factor = hi;
        } else {
            self.avoid = 2;
            self.avoid_factor = lo;
        }
    }

    /// `func_800088D8` for a CPU in a Grand Prix with one human: true when the kart may speed up with the rubber-band
    /// thrust, false when it is close enough to the human to coast.
    fn may_speed_up(&self, kart: &Kart, cc: Cc, view: &RaceView) -> bool {
        if !view.racing {
            return true;
        }
        let lap = view.lap.min(3);
        let table = &RUBBER_BAND;
        let row = |slot: usize| -> f64 {
            let a = table[lap * 8 + slot.min(7)] as f64;
            if lap < 3 {
                let b = table[lap * 8 + 8 + slot.min(7)] as f64;
                b * view.lap_fraction + a * (1. - view.lap_fraction)
            } else {
                a
            }
        };
        let progress = (view.traversed - view.human_traversed).unsigned_abs() as f64;
        if view.rank == 0 {
            let limit = (cc.index() as f64 + 1.) * row(0);
            return !(limit < progress && kart.kmh() >= 20.);
        }
        let ahead_of_me = view.human_rank < view.rank;
        let slot = if ahead_of_me { view.cpu_rank } else { view.rank };
        row(slot) < progress
    }

    /// `regulate_cpu_speed` (race part): sets the throttle on the kart. Returns what it did.
    fn regulate(&mut self, kart: &mut Kart, target: f64, cc: Cc, view: &RaceView) -> (bool, f64, bool) {
        let speed = kart.speed;
        let floor = match cc {
            Cc::Cc50 => 2.5,
            Cc::Cc150 => 3.75,
            _ => 3.333_333_3,
        };
        let mut target = target;
        let (accel, fast, decel);
        if speed < floor {
            (accel, fast, decel) = (true, false, 0.);
        } else {
            match self.speed_mode {
                SpeedMode::Fast => (accel, fast, decel) = (true, false, 0.),
                SpeedMode::Max => (accel, fast, decel) = (true, true, 0.),
                mode => {
                    if mode == SpeedMode::Slow && speed / 18. * 216. > 20. {
                        target = 1.666_666_6;
                    }
                    if speed < target {
                        if self.may_speed_up(kart, cc, view) {
                            (accel, fast, decel) = (true, true, 0.);
                        } else {
                            (accel, fast, decel) = (false, false, 1.);
                        }
                    } else {
                        (accel, fast, decel) = (false, false, if target > 1. { 2. } else { 5. });
                    }
                }
            }
        }
        kart.cpu_fast = fast;
        kart.cpu_decel = if accel { 0. } else { decel };
        (accel, decel, fast)
    }

    /// One 30 Hz decision (`update_player`, CPU branch). `others` are all the other karts; the kart is steered and
    /// its throttle set through `Kart::{cpu_turn, cpu_decel, cpu_fast}`.
    #[allow(clippy::too_many_arguments)]
    pub fn control(
        &mut self,
        ai: &CourseAi,
        path: &PathInfo,
        character: usize,
        cc: Cc,
        toads_turnpike: bool,
        kart: &mut Kart,
        others: &[Neighbor],
        view: &RaceView,
    ) -> Control {
        let n = path.len();
        let pos = [kart.pos[0], kart.pos[2]];
        // update_player_path_completion
        let point = path.nearest(kart.pos, self.point);
        let new_point = point != self.point;
        self.point = point;
        self.factor = path.factor(pos, point);
        if new_point {
            self.behaviour(ai, path, kart);
        }
        // Drift bookkeeping: leave the drift once the heading has swung past the path's.
        if self.drift != 0 {
            let moved = [kart.pos[0] - self.prev_pos[0], kart.pos[2] - self.prev_pos[2]];
            if moved[0].hypot(moved[1]) > 1e-6 {
                let player_angle = heading([0., 0.], moved);
                let mut expected = path.rotation[(point + 2) % n] * 360 / 65535;
                let mut mine = player_angle * 360 / 65535;
                let fold = |a: &mut i32| {
                    if *a < -180 {
                        *a += 360;
                    }
                    if *a > 180 {
                        *a -= 360;
                    }
                };
                fold(&mut expected);
                fold(&mut mine);
                let mut diff = expected - mine;
                fold(&mut diff);
                if (self.drift == -1 && diff > 5) || (self.drift == 1 && diff < -5) {
                    self.drift = 0;
                    kart.cpu_end_drift();
                }
            }
        }
        self.prev_pos = kart.pos;
        let in_curve = path.curves[point] > 0;
        let mut offset = self.ideal_offset(ai, path, point, toads_turnpike);
        if view.traversed < 11 {
            // The start: aim eight (Toad's Turnpike twenty) points ahead at the current lane.
            if view.traversed > 0 && toads_turnpike {
                offset = path.offset_position((point + 20) % n, 0.);
                self.instr_target = 0.;
            } else {
                offset = path.offset_position((point + 8) % n, self.factor);
                self.instr_current = self.factor;
            }
        }
        self.avoid_slower(kart, character, path, others);
        if self.avoid > 0 {
            offset = path.offset_position((point + 5) % n, self.avoid_factor);
        }
        let prev = self.prev_offset.unwrap_or(offset);
        offset = [(prev[0] + offset[0]) * 0.5, (prev[1] + offset[1]) * 0.5];
        if self.edge {
            // Back towards the middle of the road (see `look_for_edges`).
            offset = path.offset_position((point + 4) % n, 0.);
            self.instr_current *= 0.5;
            self.avoid = 0;
            if self.drift == 1 || self.drift == -1 {
                self.drift = 0;
                kart.cpu_end_drift();
            }
        }
        self.prev_offset = Some(offset);
        // Steering: the angle to the aim point, clamped to 1.5 degrees, scaled by the course's sensitivity.
        let mut angle = wrap16(heading(pos, offset) - kart.rotation);
        angle = angle.clamp(-MIN_ANGLE, MIN_ANGLE);
        let mut sensitivity = ai.steering;
        // The game indexes the section type by player id here (not by path point); kept.
        match path.section[self.slot % n] {
            Section::Right => {
                if self.factor > 0.5 {
                    sensitivity = 0x14;
                }
                if self.factor < -0.5 {
                    sensitivity = 0x35;
                }
            }
            Section::Left => {
                if self.factor > 0.5 {
                    sensitivity = 0x35;
                }
                if self.factor < -0.5 {
                    sensitivity = 0x14;
                }
            }
            _ => {}
        }
        if self.state == BehaviourState::Running && (self.factor > 0.9 || self.factor < -0.9) {
            self.drift = 0;
            kart.cpu_end_drift();
        }
        let steer = if kart.hop {
            match self.drift {
                1 => 0x35,
                -1 => -0x35,
                _ => (self.prev_steer + (angle * sensitivity) / MIN_ANGLE) / 2,
            }
        } else if kart.midair {
            0
        } else {
            (self.prev_steer + (angle * sensitivity) / MIN_ANGLE) / 2
        };
        kart.cpu_turn = Some(steer);
        self.prev_steer = steer;
        // Target speed.
        let c = cc.index();
        let mut target = if in_curve || self.drift == 1 || self.drift == -1 || kart.midair { ai.curve_speed[c] } else { ai.normal_speed[c] };
        if self.factor > 0.9 || self.factor < -0.9 {
            target = ai.off_track_speed[c];
        }
        if self.drift == 2 || self.drift == -2 || self.drift == 3 {
            target = 3.333_333_3;
        }
        if self.edge {
            // Ease off near a rail-less edge (`D_0D0096B8`, the course's careful speed).
            target = target.min(ai.slow_speed[c]);
        }
        self.target_speed = target;
        let (accelerate, decel, fast) = self.regulate(kart, target, cc, view);
        self.last = Control { steer, accelerate, fast, decel };
        self.last
    }
}

/// `D_800DCAF4`: the rubber band thresholds (path points) per lap row and rank slot.
const RUBBER_BAND: [i16; 32] = [
    0x14, 0x05, 0x0a, 0x0f, 0x14, 0x19, 0x1e, 0x23, 0x1e, 0x19, 0x32, 0x4b, 0x64, 0x7d, 0x96, 0xaf, 0x28, 0x1e, 0x3c, 0x5a, 0x78, 0x96,
    0xb4, 0xd2, 0x32, 0x28, 0x50, 0x78, 0xa0, 0xc8, 0xf0, 0x118,
];

/// Item hook between the CPU drivers and the item system (the MK64 items work, part A, owns the items themselves).
/// `kart_race.rs` keeps its own three items until that system lands; an implementation of this trait replaces the
/// timer/cone rules there.
#[allow(dead_code)] // the hook the MK64 items work implements
pub trait CpuItemUse {
    /// Racer `racer` (1.. for CPUs) gets an item rolled for its lap and rank (`cpu_gen_random_item`).
    fn grant(&mut self, racer: usize, lap: u32, rank: usize);
    /// Use the held item now (`cpu_use_item_strategy` decided to); returns false when nothing was held.
    fn use_item(&mut self, racer: usize) -> bool;
}

/// `cpu_use_item_strategy`, branch `CPU_STRATEGY_WAIT_NEXT_ITEM`: MK64's CPUs do not take item boxes, they draw an item
/// on a timer: after `playerId * 20 + 100` path points of the race, `0x259` frames after the last one, at most three
/// per lap and never on the last lap. Counts 60 Hz frames like the game (two per 30 Hz tick).
#[derive(Clone, Debug, Default)]
pub struct CpuItemStrategy {
    timer: u32,
    uses: u32,
    lap: u32,
}
impl CpuItemStrategy {
    /// One 30 Hz tick for the CPU with player id `slot`; true when it draws an item now.
    pub fn tick(&mut self, slot: usize, traversed: i64, lap: u32) -> bool {
        if lap != self.lap {
            // update_player_path_completion: `cpu_ItemStrategy[playerId].numItemUse = 0` on every new lap.
            self.lap = lap;
            self.uses = 0;
        }
        self.timer += 2;
        if traversed > (slot as i64 * 20 + 100) && self.timer >= 0x259 && self.uses < 3 && lap < 3 {
            self.timer = 0;
            self.uses += 1;
            return true;
        }
        false
    }
}

/// A scripted human for tests and demos: the same path logic as the CPUs (aim point, lane, curve speeds) but through
/// the human pad and physics, always on the gas. This is the "skilled player" the lap-time check compares with.
#[derive(Clone, Debug)]
pub struct Bot {
    driver: CpuDriver,
}
impl Bot {
    pub fn new(point: usize) -> Self {
        Self { driver: CpuDriver::new(0, point, 0.) }
    }
    pub fn input(&mut self, ai: &CourseAi, path: &PathInfo, kart: &Kart, racing: bool) -> Input {
        if !racing {
            return Input::default();
        }
        let mut probe = kart.clone();
        probe.cpu_turn = None;
        let view = RaceView { traversed: 1000, human_traversed: 1000, racing, ..RaceView::default() };
        let c = self.driver.control(ai, path, kart.stats.character, kart.stats.cc, false, &mut probe, &[], &view);
        let mut pad = c.pad();
        pad.accel = true;
        pad.raw_y = 40;
        pad.brake = false;
        pad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1200 x 800 rectangle path, 20 units apart, like the kart_race oval.
    fn oval() -> Vec<[f64; 3]> {
        let corners: [[f64; 2]; 4] = [[0., 0.], [1200., 0.], [1200., 800.], [0., 800.]];
        let mut pts = vec![];
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            let n = ((b[0] - a[0]).abs() + (b[1] - a[1]).abs()) / 20.;
            for s in 0..n as usize {
                let f = s as f64 / n;
                pts.push([a[0] + (b[0] - a[0]) * f, 0., a[1] + (b[1] - a[1]) * f]);
            }
        }
        pts
    }

    #[test]
    fn boundaries_straddle_the_path_and_the_factor_maps_them_to_minus_one_and_one() {
        let path = PathInfo::new(&oval(), 50.);
        // Point 10 is on the first edge heading +x: left is -z? (the game's left is (z, -x) of the heading).
        let p = path.pts[10];
        let (l, r) = (path.left[10], path.right[10]);
        assert!((l[1] - p[2]).abs() > 49. && (r[1] - p[2]).abs() > 49. && (l[1] - p[2]).signum() != (r[1] - p[2]).signum());
        assert!((path.factor(l, 10) + 1.).abs() < 1e-9 && (path.factor(r, 10) - 1.).abs() < 1e-9);
        assert!(path.factor([p[0], p[2]], 10).abs() < 1e-9);
        let mid = path.offset_position(10, 0.);
        assert!((mid[1] - p[2]).abs() < 1e-6);
    }

    #[test]
    fn curves_are_found_and_the_approach_leans_into_them() {
        let path = PathInfo::new(&oval(), 50.);
        let strong: Vec<usize> = (0..path.len()).filter(|i| matches!(path.section[*i], Section::Right | Section::Left)).collect();
        assert!(!strong.is_empty(), "the corners are curves");
        // Straight points before a corner become leaning curves, and are counted.
        assert!(path.curves.iter().any(|c| *c > 3));
        assert!(path.section[5] != Section::Left);
    }

    #[test]
    fn expected_rotation_points_down_the_path() {
        let path = PathInfo::new(&oval(), 50.);
        // Heading +x: rotation_for([1, 0]) = -atan2(1, 0) = -90 degrees.
        assert_eq!(path.rotation[10], s16(-(PI / 2.) * 32768. / PI));
    }

    #[test]
    fn nearest_follows_the_window_then_falls_back_to_the_global_search() {
        let path = PathInfo::new(&oval(), 50.);
        assert_eq!(path.nearest([200., 0., 5.], 8), 10);
        assert_eq!(path.nearest([1200., 0., 400.], 0), path.nearest([1200., 0., 400.], 3), "far away: global nearest");
    }

    #[test]
    fn cpus_draw_at_most_three_items_a_lap_on_the_decomp_timer() {
        let mut s = CpuItemStrategy::default();
        assert_eq!((0..30 * 60).filter(|_| s.tick(1, 500, 0)).count(), 3, "three per lap");
        let mut s = CpuItemStrategy::default();
        assert!(!(0..400).any(|_| s.tick(1, 50, 0)), "not before 120 path points");
        let mut s = CpuItemStrategy::default();
        assert!(!(0..400).any(|_| s.tick(1, 5000, 3)), "nothing on the final lap");
    }

    #[test]
    fn the_rubber_band_table_is_the_decomp_s() {
        assert_eq!(RUBBER_BAND[0], 20);
        assert_eq!(RUBBER_BAND[15], 175);
    }
}
