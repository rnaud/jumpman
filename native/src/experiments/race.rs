//! Summit Race (`race.ts`): you against seven full-physics rivals up Bob-omb Battlefield's race
//! route (the real course, the route Koopa the Quick runs, re-planned so Mario can run it).
//!
//! * **Rivals** are crowd Marios on the real physics, steered by [`RivalBrain`]: they chase a point a
//!   little ahead on the route, long-jump the straights (the fast way up the mountain), shove past each
//!   other, and a Lakitu-style pick-up puts a rival that fell off the mountain back on the route (it
//!   loses a couple of seconds, like the player would). A rival far ahead of you eases off and one far
//!   behind pushes harder (a soft rubber band), so the race stays a race.
//! * **Lakitu lifts**: in this port's collision the meadow and the mountain path are joined only by slippery
//!   grass every Mario slides back down (measured, see `race_course`), so the plan marks that stretch as a lift:
//!   whoever reaches its foot, player or rival, is carried to the top and loses the same 1.7 s.
//! * **Classes**: Rookie, Pro and Champion rivals ([`RaceClass`]) differ in speed, long-jump habit and
//!   how rough they play (Champions throw punches when you run alongside). Winning a class unlocks the
//!   next; the best time and the best place per class are saved.
//! * **Feedback**: a three-light countdown, a toast when you gain or lose a place, split times at each
//!   quarter of the route against your best, the leader's gap, and a podium card at the end.
//!
//! You can punch rivals too (they are knocked back like any Mario). Keys: `R` race again, `C` change
//! class (on the grid or after the finish). Test hooks: `JUMPMAN_RACE_CLASS=0|1|2`,
//! `JUMPMAN_RACE_AUTO=1` (the player's stick is driven by a rival brain: soak tests and screenshots).
use super::crowd::{init_clips, DriveInput};
use super::mario_crowd::*;
use super::rail_path::{RailHit, RailPath};
use super::{store, Experiment, ExperimentId, Host, StepOverride};
use crate::render::SCALE;
use bevy::prelude::*;
use jumpman_sim::behaviors::koopa_the_quick::KOOPA_RACE_ROUTE;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::math::{atan2s, Vec3 as V3};
use jumpman_sim::state::ControllerSnapshot;

const RIVALS: usize = 7;
const COUNTDOWN: u32 = 3 * 30 + 15;
const BEST_KEY: &str = "jumpman.race.best";
const CLASS_KEY: &str = "jumpman.race.classes";
const ORDINALS: [&str; 8] = ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th"];
const RIVAL_COLORS: [u32; 7] = [0x17b3a6, 0xf5c518, 0x1fa3e0, 0x8c3fd9, 0xff6fb5, 0xff8a1f, 0xeeeeee];
const RIVAL_NAMES: [&str; 7] = ["Acqua", "Oro", "Azul", "Viola", "Rosa", "Arancia", "Bianco"];
/// Largest drop off a ledge the race plan takes (Koopa's own route hops off the 768-unit plateau; Mario
/// takes no fall damage below 1150).
pub const RACE_DROP: f64 = 820.0;
/// Split points as fractions of the route.
const SPLITS: [f64; 3] = [0.25, 0.5, 0.75];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RaceState {
    Grid,
    Racing,
    Finished,
}

/// The rival field's level. Each is unlocked by winning the one before.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RaceClass {
    Rookie,
    Pro,
    Champion,
}

impl RaceClass {
    pub const ALL: [RaceClass; 3] = [RaceClass::Rookie, RaceClass::Pro, RaceClass::Champion];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn from_index(i: usize) -> Self {
        Self::ALL[i.min(2)]
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Rookie => "ROOKIE",
            Self::Pro => "PRO",
            Self::Champion => "CHAMPION",
        }
    }
    /// The racing habits of rival `i` (0..7) in this class: a spread, so the field strings out.
    pub fn skill(self, i: usize) -> RivalSkill {
        // Rank 0 is the class's fastest rival, 6 the slowest.
        let rank = (i * 3) % RIVALS;
        let k = rank as f64 / (RIVALS - 1) as f64;
        match self {
            Self::Rookie => RivalSkill { mag: 58.0 - 8.0 * k, long_jump: 0.0, rough: 0.0 },
            Self::Pro => RivalSkill { mag: 64.0 - 6.0 * k, long_jump: 0.05 - 0.04 * k, rough: 0.0 },
            Self::Champion => RivalSkill { mag: 64.0 - 2.0 * k, long_jump: 0.14 - 0.08 * k, rough: 0.02 },
        }
    }
}

/// How one rival races.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RivalSkill {
    /// Stick magnitude on the straights (64 = full run, 32 units a frame).
    pub mag: f64,
    /// Chance per eligible tick (running fast, a straight flat stretch ahead) to long-jump.
    pub long_jump: f64,
    /// Chance per tick to throw a punch at a racer running alongside.
    pub rough: f64,
}

/// Which places and times a player has earned, per class (saved).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ClassRecord {
    /// Best finishing place, 1-based (0 = never finished).
    pub place: u8,
    /// Best time in seconds (0 = none).
    pub time: f64,
    /// Best split times (seconds at each of [`SPLITS`]), 0 = none.
    pub splits: [f64; 3],
}

/// Class `c` is open when it is the first one or the class below was won.
pub fn class_unlocked(records: &[ClassRecord; 3], c: RaceClass) -> bool {
    c.index() == 0 || records[c.index() - 1].place == 1
}

/// A racer as the brain sees it.
#[derive(Clone, Copy, Debug)]
pub struct RacerView {
    pub pos: V3,
    pub grounded: bool,
    pub forward_vel: f64,
    pub against_wall: bool,
    /// Knocked back / intangible: the brain waits.
    pub stunned: bool,
}

/// What the brain wants this tick.
#[derive(Clone, Debug, Default)]
pub struct Steer {
    pub input: DriveInput,
    /// Put the racer back on the route here (position, face yaw): the Lakitu pick-up.
    pub respawn: Option<(V3, f64)>,
}

/// Seeded splitmix64 for the brains (deterministic races).
#[derive(Clone, Copy, Debug)]
pub struct BrainRng(u64);
impl BrainRng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15)
    }
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Seconds a rival needs off the route (or without progress) before the pick-up.
const OFF_TICKS: u32 = 40;
const STALL_TICKS: u32 = 120;
const LOOK_AHEAD: f64 = 300.0;

/// A rival's racing brain: pure pursuit along the route plus long jumps, shoves and the pick-up.
#[derive(Clone, Copy, Debug, Default)]
pub struct RivalBrain {
    pub hint: Option<usize>,
    /// Furthest progress along the route (the pick-up goes back a little behind it).
    pub best_s: f64,
    pub s: f64,
    stall: u32,
    off: u32,
    crouched: bool,
    hold_a: u8,
    /// Ticks of no input after a pick-up (the rival is dizzy; this is the time lost).
    dizzy: u32,
    pub pickups: u32,
    /// Where the last pick-up happened and how many in a row came at that spot: a rival that keeps failing the same
    /// ledge is set down just past it the third time (the race must go on).
    pick_at: f64,
    pick_repeats: u8,
}

/// How far before the foot of a lift (along the route) Lakitu picks a racer up, and how long the ride takes.
pub const LIFT_REACH: f64 = 260.0;
pub const LIFT_TICKS: u32 = 50;

/// Lakitu: the top (route distance) of the lift whose foot a racer with progress `best_s` has reached.
pub fn lift_for(best_s: f64, lifts: &[(f64, f64)]) -> Option<f64> {
    lifts.iter().find(|(foot, top)| best_s >= foot - LIFT_REACH && best_s < top - 100.0).map(|(_, top)| *top)
}

/// Lift spots on the rail: `(foot s, top s)` per lift.
pub fn lift_spans(rail: &RailPath, lifts: &[(V3, V3)]) -> Vec<(f64, f64)> {
    lifts.iter().map(|(a, b)| (rail.nearest(a, None, 0).s, rail.nearest(b, None, 0).s)).filter(|(a, b)| b > a).collect()
}

impl RivalBrain {
    /// Carried to `top` (route distance) by Lakitu: the time lost is the ride.
    pub fn lifted(&mut self, rail: &RailPath, top: f64) {
        let at = rail.at(top);
        self.hint = Some(at.index);
        self.best_s = top;
        self.s = top;
        self.stall = 0;
        self.off = 0;
        self.crouched = false;
        self.dizzy = LIFT_TICKS;
    }

    /// One 30 Hz decision. `others` are the other racers' positions (the player included), `player_s`
    /// the player's progress (for the rubber band).
    pub fn steer(&mut self, me: &RacerView, rail: &RailPath, skill: RivalSkill, others: &[V3], player_s: f64, rng: &mut BrainRng) -> Steer {
        let hit = rail.nearest(&me.pos, self.hint, 40);
        self.hint = Some(hit.index);
        self.s = hit.s;
        if hit.s > self.best_s + 30.0 {
            self.best_s = hit.s;
            self.stall = 0;
        } else {
            self.stall += 1;
        }
        if self.dizzy > 0 {
            self.dizzy -= 1;
            self.stall = 0;
            return Steer::default();
        }
        if me.stunned {
            self.stall = 0;
            return Steer::default();
        }
        let lateral = (me.pos[0] - hit.point[0]).hypot(me.pos[2] - hit.point[2]);
        let below = hit.point[1] - me.pos[1];
        let off = lateral > 700.0 || (below > 380.0 && lateral > 150.0);
        self.off = if off { self.off + 1 } else { 0 };
        if self.off > OFF_TICKS || self.stall > STALL_TICKS {
            self.pick_repeats = if (self.best_s - self.pick_at).abs() < 300.0 { self.pick_repeats + 1 } else { 0 };
            self.pick_at = self.best_s;
            let back = if self.pick_repeats >= 2 {
                self.pick_repeats = 0;
                self.best_s += 350.0;
                rail.at(self.best_s)
            } else {
                rail.at((self.best_s - 160.0).max(0.0))
            };
            self.off = 0;
            self.stall = 0;
            self.dizzy = 20;
            self.pickups += 1;
            self.crouched = false;
            let yaw = atan2s(back.tangent[1], back.tangent[0]) as f64;
            return Steer { input: DriveInput::default(), respawn: Some(([back.point[0], back.point[1] + 160.0, back.point[2]], yaw)) };
        }
        // Pure pursuit: aim at a point a little further along the route.
        let target = rail.at(hit.s + LOOK_AHEAD).point;
        let (mut vx, mut vz) = (target[0] - me.pos[0], target[2] - me.pos[2]);
        let d = vx.hypot(vz).max(1.0);
        vx /= d;
        vz /= d;
        // Shoulder past racers alongside.
        let mut rough_target = false;
        for o in others {
            let (dx, dz) = (me.pos[0] - o[0], me.pos[2] - o[2]);
            let od = dx.hypot(dz);
            if od < 1.0 || od > 170.0 || (o[1] - me.pos[1]).abs() > 160.0 {
                continue;
            }
            let push = (170.0 - od) / 170.0;
            vx += dx / od * push * 0.5;
            vz += dz / od * push * 0.5;
            rough_target |= od < 130.0;
        }
        let yaw = atan2s(vz, vx) as f64;
        // Rubber band: a rival far ahead of the player eases off, one far behind runs flat out.
        let gap = hit.s - player_s;
        let mut mag = skill.mag;
        if gap > 2500.0 {
            mag *= if gap > 6000.0 { 0.78 } else { 0.88 };
        } else if gap < -3500.0 {
            mag = 64.0;
        }
        let mut input = DriveInput { yaw: Some(yaw), mag: Some(mag), ..Default::default() };
        if self.hold_a > 0 {
            self.hold_a -= 1;
            input.hold_a = true;
        }
        if self.crouched {
            // Second half of the long jump: A while crouch-sliding.
            self.crouched = false;
            input.jump = true;
            input.hold_a = true;
            self.hold_a = 8;
            return Steer { input, respawn: None };
        }
        if me.grounded && me.against_wall && me.forward_vel < 10.0 {
            input.jump = true;
            input.hold_a = true;
            self.hold_a = 12;
        } else if me.grounded && me.forward_vel > 26.0 && gap < 2500.0 {
            let lj = skill.long_jump * if gap < -3500.0 { 2.0 } else { 1.0 };
            if lj > 0.0 && straight_ahead(rail, hit, me.pos[1]) && rng.next() < lj {
                self.crouched = true;
                input.crouch = true;
            } else if rough_target && skill.rough > 0.0 && rng.next() < skill.rough {
                input.punch = true;
            }
        }
        Steer { input, respawn: None }
    }
}

/// Whether the route ahead is straight and level enough for a long jump (no bends, no climbs or drops).
pub fn straight_ahead(rail: &RailPath, hit: RailHit, y: f64) -> bool {
    let near = rail.at(hit.s + 500.0);
    let far = rail.at(hit.s + 1000.0);
    let dot = hit.tangent[0] * far.tangent[0] + hit.tangent[1] * far.tangent[1];
    let dot2 = hit.tangent[0] * near.tangent[0] + hit.tangent[1] * near.tangent[1];
    dot > 0.97 && dot2 > 0.97 && (far.point[1] - y).abs() < 90.0 && (near.point[1] - y).abs() < 60.0
}

/// The race on the real course: Koopa the Quick's checkpoints re-planned for Mario, plus the Lakitu lifts.
pub struct RaceCourse {
    /// The dense route (see [`densify`]).
    pub route: Vec<V3>,
    /// Stretches no Mario can run in this port's collision (Bob-omb Battlefield's meadow and the mountain path
    /// are joined only by slippery grass every Mario slides back down): `(foot, top)`. A racer that reaches the
    /// foot is carried to the top by Lakitu and loses the same time whoever it is.
    pub lifts: Vec<(V3, V3)>,
}

/// Plan the race on `world` (deterministic).
pub fn race_course(world: &CollisionWorld) -> RaceCourse {
    // Checkpoint 12 sits on the slippery foot of the mountain; snapped to standable ground it lands on the
    // plateau above, which leads nowhere. Without it the plan runs meadow, lift, mountain path.
    let koopa: Vec<V3> = KOOPA_RACE_ROUTE.iter().enumerate().filter(|(i, _)| *i != 12).map(|(_, p)| *p).collect();
    let plan = super::crowd_route::mario_route_drop(world, &koopa, RACE_DROP);
    let lifts = plan.windows(2).filter(|w| w[1][1] - w[0][1] > 150.0 && !super::crowd_route::segment_walkable_drop(world, w[0], w[1], RACE_DROP)).map(|w| (w[0], w[1])).collect();
    RaceCourse { route: densify(&plan, 200.0), lifts }
}

/// Extra points every `step` units along each straight of the plan, so the smoothed rail hugs the checked
/// polyline instead of bowing off narrow ledges between far-apart corners.
pub fn densify(route: &[V3], step: f64) -> Vec<V3> {
    let mut out: Vec<V3> = vec![];
    for w in route.windows(2) {
        let (a, b) = (w[0], w[1]);
        let n = ((b[0] - a[0]).hypot(b[2] - a[2]) / step).ceil().max(1.0) as usize;
        for k in 0..n {
            let t = k as f64 / n as f64;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]);
        }
    }
    if let Some(l) = route.last() {
        out.push(*l);
    }
    out
}

struct Runner {
    name: String,
    player: bool,
    /// Agent id for rivals.
    agent: Option<u32>,
    hit: Option<RailHit>,
    finish_ticks: Option<u32>,
    brain: RivalBrain,
    skill: RivalSkill,
}

pub struct SummitRaceExperiment {
    crowd: Option<MarioCrowd>,
    rail: Option<RailPath>,
    runners: Vec<Runner>,
    state: RaceState,
    ticks: u32,
    race_ticks: u32,
    best: f64,
    guide: Vec<Entity>,
    result: String,
    class: RaceClass,
    records: [ClassRecord; 3],
    rng: BrainRng,
    races: u32,
    /// The player's place last tick (for the "passed" toasts) and when the last such toast showed.
    last_place: usize,
    place_toast_tick: u32,
    splits_done: usize,
    split_times: [f64; 3],
    /// `JUMPMAN_RACE_AUTO`: a brain drives the player too.
    auto: Option<RivalBrain>,
    cheated: bool,
    /// Lakitu lifts as route distances, and the player's ride (ticks left, top).
    lifts: Vec<(f64, f64)>,
    player_lift: Option<(u32, f64)>,
}

pub fn format_time(seconds: f64) -> String {
    let m = (seconds / 60.0).floor();
    format!("{}:{:04.1}", m, seconds - m * 60.0)
}

/// Whether a runner standing at `pos` (projected to `hit` on a rail of length `total`) has made
/// the summit: within the last 260 units of the route and at the rail's height (not on a
/// different layer of the mountain). Shared by every Bob-omb Battlefield summit goal.
pub fn at_finish(total: f64, hit: &RailHit, pos: &V3) -> bool {
    hit.s > total - 260.0 && (pos[1] - hit.point[1]).abs() < 500.0
}

/// Rank by finish time, then by progress along the rail. Returns indices best first.
fn standings(finish: &[Option<u32>], progress: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..finish.len()).collect();
    order.sort_by(|&a, &b| match (finish[a], finish[b]) {
        (None, None) => progress[b].partial_cmp(&progress[a]).unwrap(),
        (fa, fb) => fa.unwrap_or(u32::MAX).cmp(&fb.unwrap_or(u32::MAX)),
    });
    order
}

fn load_records() -> [ClassRecord; 3] {
    let mut out = [ClassRecord::default(); 3];
    if let Some(v) = store::get(CLASS_KEY).and_then(|v| v.as_array().cloned()) {
        for (i, r) in v.iter().take(3).enumerate() {
            out[i].place = r.get("place").and_then(|p| p.as_u64()).unwrap_or(0) as u8;
            out[i].time = r.get("time").and_then(|p| p.as_f64()).unwrap_or(0.0);
            if let Some(s) = r.get("splits").and_then(|s| s.as_array()) {
                for (j, t) in s.iter().take(3).enumerate() {
                    out[i].splits[j] = t.as_f64().unwrap_or(0.0);
                }
            }
        }
    }
    out
}

fn save_records(r: &[ClassRecord; 3]) {
    let v: Vec<serde_json::Value> = r.iter().map(|c| serde_json::json!({ "place": c.place, "time": c.time, "splits": c.splits })).collect();
    store::set(CLASS_KEY, serde_json::Value::Array(v));
}

/// Fold a finished race into a class record; returns (new best place, new best time).
pub fn record_result(rec: &mut ClassRecord, place: u8, time: f64, splits: [f64; 3]) -> (bool, bool) {
    let better_place = rec.place == 0 || place < rec.place;
    let better_time = rec.time <= 0.0 || time < rec.time;
    if better_place {
        rec.place = place;
    }
    if better_time {
        rec.time = time;
        rec.splits = splits;
    }
    (better_place, better_time)
}

impl SummitRaceExperiment {
    pub fn new() -> Self {
        Self {
            crowd: None,
            rail: None,
            runners: vec![],
            state: RaceState::Grid,
            ticks: 0,
            race_ticks: 0,
            best: f64::INFINITY,
            guide: vec![],
            result: String::new(),
            class: RaceClass::Rookie,
            records: [ClassRecord::default(); 3],
            rng: BrainRng::new(0x5eed),
            races: 0,
            last_place: 7,
            place_toast_tick: 0,
            splits_done: 0,
            split_times: [0.0; 3],
            auto: None,
            cheated: false,
            lifts: vec![],
            player_lift: None,
        }
    }

    /// Line everyone up on a starting grid at the foot of the route.
    fn restart(&mut self, host: &mut Host) {
        let (Some(crowd), Some(rail)) = (self.crowd.as_mut(), self.rail.as_ref()) else { return };
        let start = rail.at(250.0);
        let [tx, tz] = start.tangent;
        let yaw = atan2s(tz, tx) as f64;
        let world = host.course_world();
        let mut spots: Vec<V3> = vec![];
        for i in 0..RIVALS + 1 {
            let (row, col) = ((i / 4) as f64, (i % 4) as f64 - 1.5);
            let x = start.point[0] - tx * row * 220.0 - tz * col * 170.0;
            let z = start.point[2] - tz * row * 220.0 + tx * col * 170.0;
            let floor = world.find_floor(x, start.point[1] + 400.0, z);
            spots.push([x, if floor.surface.is_some() { floor.height } else { start.point[1] }, z]);
        }
        // The player starts on the back row: there is a field to beat.
        let player_spot = spots.remove(5);
        host.place_mario(player_spot, Some(yaw));
        self.state = RaceState::Grid;
        self.ticks = 0;
        self.race_ticks = 0;
        self.result.clear();
        self.races += 1;
        self.rng = BrainRng::new(0x5eed ^ (self.races as u64) << 8);
        self.last_place = 7;
        self.place_toast_tick = 0;
        self.splits_done = 0;
        self.split_times = [0.0; 3];
        self.player_lift = None;
        crowd.populate_at(&spots, Personality::Racer, yaw, Some(&mut *host));
        let mut runners = vec![Runner { name: "You".into(), player: true, agent: None, hit: None, finish_ticks: None, brain: RivalBrain::default(), skill: self.class.skill(0) }];
        for (i, a) in crowd.agents.iter_mut().enumerate() {
            a.name = RIVAL_NAMES.get(i).copied().unwrap_or("Rival").into();
            a.manual = Some(DriveInput::default());
            // Cap colour per rival.
            if let Some(e) = a.body.entity {
                host.commands.entity(e).insert((super::crowd::CrowdColors(super::crowd::CrowdLook { cap: RIVAL_COLORS[i % RIVAL_COLORS.len()], overalls: None }), super::crowd::NeedsTint));
            }
            runners.push(Runner { name: a.name.clone(), player: false, agent: Some(a.id), hit: None, finish_ticks: None, brain: RivalBrain::default(), skill: self.class.skill(i) });
        }
        self.runners = runners;
        if self.auto.is_some() {
            self.auto = Some(RivalBrain::default());
        }
        host.toast(&format!("{} CLASS - ON YOUR MARKS...", self.class.name()), 1.4);
    }

    fn position_of(&self, host: &Host, runner: &Runner) -> V3 {
        match runner.agent {
            None => host.sim.mario.pos,
            Some(id) => self.crowd.as_ref().and_then(|c| c.agents.iter().find(|a| a.id == id)).map_or([0.0; 3], |a| a.body.pos()),
        }
    }

    /// Every rival's brain picks its input for the next crowd step.
    fn drive_rivals(&mut self, player_pos: V3) {
        let (Some(crowd), Some(rail)) = (self.crowd.as_mut(), self.rail.as_ref()) else { return };
        let player_s = self.runners[0].hit.map_or(0.0, |h| h.s);
        let positions: Vec<V3> = crowd.agents.iter().map(|a| a.body.pos()).chain(std::iter::once(player_pos)).collect();
        for (i, a) in crowd.agents.iter_mut().enumerate() {
            let Some(r) = self.runners.iter_mut().find(|r| r.agent == Some(a.id)) else { continue };
            if r.finish_ticks.is_some() {
                // Celebrate on the summit.
                a.manual = Some(DriveInput { jump: self.race_ticks % 40 == i as u32 * 5, ..Default::default() });
                continue;
            }
            let m = &a.body.runtime;
            let view = RacerView { pos: m.pos, grounded: m.action & jumpman_sim::state::ACT_FLAG_AIR == 0, forward_vel: m.forward_vel, against_wall: m.wall.is_some(), stunned: m.action & (jumpman_sim::state::ACT_FLAG_INTANGIBLE | jumpman_sim::state::ACT_FLAG_INVULNERABLE) != 0 };
            let others: Vec<V3> = positions.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| *p).collect();
            let steer = r.brain.steer(&view, rail, r.skill, &others, player_s, &mut self.rng);
            if let Some((p, yaw)) = steer.respawn {
                a.body.teleport(p, Some(yaw));
            }
            a.manual = Some(steer.input);
            if let Some(top) = lift_for(r.brain.best_s, &self.lifts) {
                let at = rail.at(top + 60.0);
                a.body.teleport([at.point[0], at.point[1] + 120.0, at.point[2]], Some(atan2s(at.tangent[1], at.tangent[0]) as f64));
                r.brain.lifted(rail, top);
            }
        }
    }

    fn finish(&mut self, host: &mut Host) {
        self.state = RaceState::Finished;
        let finish: Vec<_> = self.runners.iter().map(|r| r.finish_ticks).collect();
        let progress: Vec<_> = self.runners.iter().map(|r| r.hit.map_or(0.0, |h| h.s)).collect();
        let place = standings(&finish, &progress).iter().position(|&i| self.runners[i].player).unwrap_or(0);
        let seconds = self.race_ticks as f64 / 30.0;
        let mut head = if place == 0 { "YOU WIN!".to_string() } else { format!("{} PLACE", ORDINALS[place].to_uppercase()) };
        let mut lines = vec![];
        if !self.cheated {
            let ci = self.class.index();
            let was_open = self.records.map(|_| ()).iter().enumerate().map(|(i, _)| class_unlocked(&self.records, RaceClass::from_index(i))).collect::<Vec<_>>();
            let (new_place, new_time) = record_result(&mut self.records[ci], place as u8 + 1, seconds, self.split_times);
            save_records(&self.records);
            if seconds < self.best {
                self.best = seconds;
                store::set(BEST_KEY, serde_json::json!(seconds));
            }
            if new_time {
                lines.push("NEW BEST TIME!".to_string());
            } else if new_place {
                lines.push("BEST FINISH IN THIS CLASS!".to_string());
            }
            if place == 0 {
                if let Some(next) = RaceClass::ALL.get(ci + 1) {
                    if !was_open[ci + 1] {
                        lines.push(format!("{} CLASS UNLOCKED - press C", next.name()));
                        host.fx("star_sound", host.sim.mario.pos);
                    }
                } else {
                    head = "CHAMPION OF THE SUMMIT!".into();
                }
            }
        }
        let rec = self.records[self.class.index()];
        let again = host.prompt("R race again, C class", "tap RACE AGAIN", "RB to race again");
        let best = if rec.time > 0.0 { format_time(rec.time) } else { "-".into() };
        self.result = format!("{head}  {}  ({} best {})  {}  -  {again}", format_time(seconds), self.class.name(), best, lines.join("  "));
        host.toast(&head, 2.5);
        if place < 3 {
            host.fx("star_sound", host.sim.mario.pos);
        }
    }

    fn update_guide(&self, host: &mut Host, hit: RailHit) {
        let rail = self.rail.as_ref().unwrap();
        let world = host.course_world();
        let render_y = host.render_pos()[1];
        let t = host.time();
        let mut n = 0;
        for i in 1..=self.guide.len() {
            let s = hit.s + i as f64 * 160.0;
            if s > rail.total {
                break;
            }
            let p = rail.at(s).point;
            let floor = world.find_floor(p[0], p[1].max(render_y) + 600.0, p[2]);
            if floor.surface.is_none() {
                continue;
            }
            let pulse = 1.0 + 0.3 * (t * 6.0 - i as f64 * 0.7).sin();
            let tf = Transform::from_xyz(p[0] as f32 * SCALE, (floor.height as f32 + 16.0) * SCALE, p[2] as f32 * SCALE).with_scale(Vec3::splat(pulse as f32));
            host.commands.entity(self.guide[n]).insert((tf, Visibility::Inherited));
            n += 1;
        }
        for e in &self.guide[n..] {
            host.commands.entity(*e).insert(Visibility::Hidden);
        }
    }

    fn cycle_class(&mut self, host: &mut Host) {
        if self.state == RaceState::Racing {
            return;
        }
        let mut next = self.class;
        for _ in 0..3 {
            next = RaceClass::from_index((next.index() + 1) % 3);
            if class_unlocked(&self.records, next) {
                break;
            }
        }
        if next == self.class {
            host.toast("WIN A RACE TO UNLOCK THE NEXT CLASS", 1.6);
            return;
        }
        self.class = next;
        self.restart(host);
    }

    /// Splits, place changes and the leader's gap while racing.
    fn race_feedback(&mut self, host: &mut Host, place: usize) {
        let Some(rail) = self.rail.as_ref() else { return };
        let me = self.runners[0].hit.map_or(0.0, |h| h.s);
        while self.splits_done < SPLITS.len() && me > rail.total * SPLITS[self.splits_done] {
            let t = self.race_ticks as f64 / 30.0;
            let best = self.records[self.class.index()].splits[self.splits_done];
            self.split_times[self.splits_done] = t;
            let delta = if best > 0.0 { format!("  {}{:.1}", if t <= best { "-" } else { "+" }, (t - best).abs()) } else { String::new() };
            host.toast(&format!("SPLIT {}  {}{delta}  {}", self.splits_done + 1, format_time(t), ORDINALS[place.min(7)]), 1.6);
            self.splits_done += 1;
            self.place_toast_tick = self.race_ticks;
        }
        if place != self.last_place && self.race_ticks > 90 && self.race_ticks > self.place_toast_tick + 45 {
            let up = place < self.last_place;
            host.toast(&format!("{} {}", if up { "UP TO" } else { "DOWN TO" }, ORDINALS[place.min(7)].to_uppercase()), 0.9);
            if up {
                host.fx("coin", host.sim.mario.pos);
            }
            self.place_toast_tick = self.race_ticks;
        }
        self.last_place = place;
    }
}

impl Experiment for SummitRaceExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Race
    }
    fn course(&self) -> Option<&'static str> {
        Some("bob")
    }

    fn enter(&mut self, host: &mut Host) {
        self.best = store::get_f64(BEST_KEY).filter(|b| *b > 0.0).unwrap_or(f64::INFINITY);
        self.records = load_records();
        self.cheated = std::env::var_os("JUMPMAN_RACE_CLASS").is_some() || std::env::var_os("JUMPMAN_RACE_AUTO").is_some();
        // The highest class you have opened is where you start.
        self.class = RaceClass::ALL.iter().rev().copied().find(|c| class_unlocked(&self.records, *c)).unwrap_or(RaceClass::Rookie);
        if let Some(c) = std::env::var("JUMPMAN_RACE_CLASS").ok().and_then(|v| v.parse::<usize>().ok()) {
            self.class = RaceClass::from_index(c);
        }
        if std::env::var_os("JUMPMAN_RACE_AUTO").is_some() {
            self.auto = Some(RivalBrain::default());
        }
        host.card(
            "06",
            "Summit Race",
            "Beat seven rivals up Bob-omb Battlefield. Long jumps (run, Z then A) are the fast way up; punch a rival who blocks you. Win to unlock the next class.",
            &[("WASD Space", "run / jump"), ("Gold dots", "the race route"), ("R", "race again"), ("C", "rival class"), ("X", "leave")],
        );
        host.set_dock(vec![super::dock::DockItem::new("Race again", KeyCode::KeyR, 0).primary(), super::dock::DockItem::new("Class", KeyCode::KeyC, 0)]);
        let mesh = host.meshes.add(Sphere::new(13.0 * SCALE));
        let mat = host.unlit(Color::srgb(1.0, 0.82, 0.29));
        for _ in 0..24 {
            let e = host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden));
            self.guide.push(e);
        }
        let meta = init_clips(host);
        let world = host.course_world();
        let course = race_course(&world);
        let opts = MarioCrowdOptions { world, route: None, has_leader: true, coin_count: 0, center: [0.0; 3], radius: 6000.0, seed: 0x5eed, single_lap: true };
        let mut crowd = MarioCrowd::new(opts, meta, Some(&mut *host));
        let rail = RailPath::new(&course.route);
        self.lifts = lift_spans(&rail, &course.lifts);
        self.rail = Some(rail);
        crowd.route = Some(course.route);
        self.crowd = Some(crowd);
        self.restart(host);
    }

    fn exit(&mut self, host: &mut Host) {
        if let Some(mut c) = self.crowd.take() {
            c.dispose(host);
        }
        host.cam.pose = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        match key {
            KeyCode::KeyR => {
                self.restart(host);
                true
            }
            KeyCode::KeyC => {
                self.cycle_class(host);
                true
            }
            _ => false,
        }
    }

    fn before_step(&mut self, host: &mut Host, controller: &ControllerSnapshot) -> StepOverride {
        if self.state == RaceState::Grid || self.player_lift.is_some_and(|(left, _)| left > 0) {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        // `JUMPMAN_RACE_AUTO`: the Champion's brain runs the player (stick from its yaw, camera-relative).
        if let (Some(mut brain), Some(rail), RaceState::Racing) = (self.auto, self.rail.as_ref(), self.state) {
            let m = &host.sim.mario;
            let view = RacerView { pos: m.pos, grounded: m.action & jumpman_sim::state::ACT_FLAG_AIR == 0, forward_vel: m.forward_vel, against_wall: m.wall.is_some(), stunned: false };
            let steer = brain.steer(&view, rail, RaceClass::Champion.skill(0), &[], brain.s, &mut self.rng);
            self.auto = Some(brain);
            if let Some((p, yaw)) = steer.respawn {
                host.place_mario(p, Some(yaw));
            }
            use jumpman_sim::state::{A_BUTTON, Z_TRIG};
            let mut c = controller.clone();
            if let Some(yaw) = steer.input.yaw {
                // With the step's camera yaw forced to 0, SM64's intended yaw is atan2s(-stick_y, stick_x).
                let a = yaw * std::f64::consts::PI / 32768.0;
                let mag = steer.input.mag.unwrap_or(64.0);
                c.stick_x = a.sin() * mag;
                c.stick_y = -a.cos() * mag;
                c.stick_mag = mag;
            }
            if steer.input.jump {
                c.button_pressed |= A_BUTTON;
            }
            if steer.input.jump || steer.input.hold_a {
                c.button_down |= A_BUTTON;
            }
            if steer.input.crouch {
                c.button_pressed |= Z_TRIG;
                c.button_down |= Z_TRIG;
            }
            return StepOverride { controller: Some(c), camera_yaw: Some(0) };
        }
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        if self.crowd.is_none() || self.rail.is_none() {
            return;
        }
        self.ticks += 1;
        if self.state == RaceState::Grid {
            let left = COUNTDOWN as i64 - self.ticks as i64;
            if left % 30 == 0 && left > 0 {
                host.toast(&(left / 30).to_string(), 0.7);
                host.fx("bonk", host.sim.mario.pos);
            }
            if left <= 0 {
                self.state = RaceState::Racing;
                host.toast("GO!", 0.9);
                host.fx("star_sound", host.sim.mario.pos);
            }
            return;
        }
        self.race_ticks += 1;
        let player_pos = host.sim.mario.pos;
        if self.state != RaceState::Grid {
            self.drive_rivals(player_pos);
        }
        self.crowd.as_mut().unwrap().update(Some(&mut host.sim.mario));
        let fx = self.crowd.as_mut().unwrap().take_fx();
        host.play_fx(fx);
        let total = self.rail.as_ref().unwrap().total;
        let mut player_finished = false;
        for i in 0..self.runners.len() {
            let pos = self.position_of(host, &self.runners[i]);
            let rail = self.rail.as_ref().unwrap();
            let hit = rail.nearest(&pos, self.runners[i].hit.map(|h| h.index), 60);
            self.runners[i].hit = Some(hit);
            if self.runners[i].finish_ticks.is_none() && at_finish(total, &hit, &pos) && (self.state == RaceState::Racing || !self.runners[i].player) {
                self.runners[i].finish_ticks = Some(self.race_ticks);
                player_finished |= self.runners[i].player;
                if !self.runners[i].player {
                    let n = self.runners.iter().filter(|r| r.finish_ticks.is_some()).count();
                    if n == 1 {
                        host.toast(&format!("{} TAKES THE SUMMIT!", self.runners[i].name.to_uppercase()), 1.4);
                    }
                }
            }
        }
        // The player's own Lakitu ride: the same lifts and the same time lost as the rivals.
        if self.state == RaceState::Racing {
            if let Some(h) = self.runners[0].hit {
                let me = &mut self.runners[0].brain;
                me.best_s = me.best_s.max(h.s);
            }
            match self.player_lift {
                Some((left, top)) if left > 0 => self.player_lift = Some((left - 1, top)),
                Some(_) => self.player_lift = None,
                None => {
                    if let (Some(top), Some(rail)) = (lift_for(self.runners[0].brain.best_s, &self.lifts), self.rail.as_ref()) {
                        let at = rail.at(top + 60.0);
                        host.place_mario([at.point[0], at.point[1] + 120.0, at.point[2]], Some(atan2s(at.tangent[1], at.tangent[0]) as f64));
                        self.runners[0].brain.lifted(rail, top);
                        self.runners[0].hit = Some(at);
                        self.player_lift = Some((LIFT_TICKS, top));
                        host.toast("LAKITU LIFT!", 1.4);
                        host.fx("teleport", host.sim.mario.pos);
                    }
                }
            }
        }
        if player_finished {
            self.finish(host);
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        let (Some(_), Some(_)) = (self.crowd.as_ref(), self.rail.as_ref()) else { return };
        let eye = host.rig.eye;
        let alpha = host.alpha;
        self.crowd.as_mut().unwrap().render(host, alpha, dt, [eye.x, eye.y, eye.z]);
        let rail = self.rail.as_ref().unwrap();
        let player_hit = self.runners.first().and_then(|r| r.hit);
        let hit = player_hit.unwrap_or_else(|| rail.nearest(&host.sim.mario.pos, None, 40));
        self.update_guide(host, hit);
        let rail_total = self.rail.as_ref().unwrap().total;
        let finish: Vec<_> = self.runners.iter().map(|r| r.finish_ticks).collect();
        let progress: Vec<_> = self.runners.iter().map(|r| r.hit.map_or(0.0, |h| h.s)).collect();
        let order = standings(&finish, &progress);
        let place = order.iter().position(|&i| self.runners[i].player).unwrap_or(0);
        if self.state == RaceState::Racing {
            self.race_feedback(host, place);
        }
        let shown = self.runners.first().and_then(|r| r.finish_ticks).unwrap_or(self.race_ticks);
        let rec = self.records[self.class.index()];
        let mut text = format!(
            "{} / {}   {}   time {}   to summit {} m   best {}",
            ORDINALS[place.min(7)],
            self.runners.len(),
            self.class.name(),
            format_time(shown as f64 / 30.0),
            ((rail_total - hit.s) / 100.0).max(0.0).round(),
            if rec.time > 0.0 { format_time(rec.time) } else { "-".into() }
        );
        if self.state == RaceState::Finished {
            text.push_str(&format!("\n{}", self.result));
        } else if self.state == RaceState::Grid {
            text.push_str(&format!("\nClass {} - C to change - long jump the straights!", self.class.name()));
        } else {
            let me = progress[0];
            let leader = order[0];
            let gap = if self.runners[leader].player {
                let second = order.get(1).map_or(me, |&i| progress[i]);
                format!("lead +{} m", ((me - second) / 100.0).round())
            } else {
                format!("{} leads by {} m", self.runners[leader].name, ((progress[leader] - me) / 100.0).max(0.0).round())
            };
            let board: Vec<String> = order
                .iter()
                .enumerate()
                .take(4)
                .map(|(rank, &i)| {
                    let r = &self.runners[i];
                    format!("{} {} {}", ORDINALS[rank], if r.player { "YOU" } else { r.name.as_str() }, if r.finish_ticks.is_some() { "*".to_string() } else { format!("{}%", (100.0 * r.hit.map_or(0.0, |h| h.s) / rail_total).round()) })
                })
                .collect();
            text.push_str(&format!("\n{gap}   |   {}", board.join("  |  ")));
        }
        host.status(text);
    }

    fn debug(&self) -> String {
        format!(
            "state={:?} class={:?} race_s={:.1} route={:.0} runners={:?}",
            self.state,
            self.class,
            self.race_ticks as f64 / 30.0,
            self.rail.as_ref().map_or(0.0, |r| r.total),
            self.runners.iter().map(|r| (r.name.as_str(), r.hit.map_or(0, |h| h.s.round() as i64), r.finish_ticks, r.brain.pickups)).collect::<Vec<_>>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standings_rank_finishers_by_time_then_progress() {
        let finish = [None, Some(300), Some(250), None];
        let progress = [900.0, 3000.0, 3000.0, 1200.0];
        assert_eq!(standings(&finish, &progress), vec![2, 1, 3, 0]);
    }

    #[test]
    fn the_finish_needs_the_end_of_the_route_at_the_routes_height() {
        let rail = RailPath::new(&[[0.0, 0.0, 0.0], [2000.0, 0.0, 0.0], [4000.0, 0.0, 0.0]]);
        let near_end = [rail.total - 100.0, 0.0, 0.0];
        let hit = rail.nearest(&near_end, None, 40);
        assert!(at_finish(rail.total, &hit, &near_end));
        assert!(!at_finish(rail.total, &hit, &[near_end[0], 900.0, 0.0]), "a different layer of the mountain is not the finish");
        let mid = [rail.total / 2.0, 0.0, 0.0];
        let hit = rail.nearest(&mid, None, 40);
        assert!(!at_finish(rail.total, &hit, &mid));
    }

    #[test]
    fn time_formatting() {
        assert_eq!(format_time(75.34), "1:15.3");
        assert_eq!(format_time(5.0), "0:05.0");
    }

    #[test]
    fn classes_unlock_in_order_and_records_keep_the_best() {
        let mut r = [ClassRecord::default(); 3];
        assert!(class_unlocked(&r, RaceClass::Rookie));
        assert!(!class_unlocked(&r, RaceClass::Pro));
        assert_eq!(record_result(&mut r[0], 3, 80.0, [20.0, 40.0, 60.0]), (true, true));
        assert!(!class_unlocked(&r, RaceClass::Pro), "third place does not open Pro");
        assert_eq!(record_result(&mut r[0], 1, 85.0, [21.0, 41.0, 61.0]), (true, false));
        assert_eq!(r[0].time, 80.0, "a slower win keeps the best time");
        assert_eq!(r[0].splits, [20.0, 40.0, 60.0]);
        assert!(class_unlocked(&r, RaceClass::Pro));
        assert!(!class_unlocked(&r, RaceClass::Champion));
    }

    #[test]
    fn harder_classes_field_faster_rougher_rivals() {
        for i in 0..RIVALS {
            let (a, b, c) = (RaceClass::Rookie.skill(i), RaceClass::Pro.skill(i), RaceClass::Champion.skill(i));
            assert!(a.mag < b.mag && b.mag <= c.mag, "rival {i}");
            assert!(a.long_jump <= b.long_jump && b.long_jump < c.long_jump, "rival {i}");
            assert!(c.rough > 0.0 && a.rough == 0.0);
        }
        // The field is spread: not seven clones.
        let mags: Vec<f64> = (0..RIVALS).map(|i| RaceClass::Pro.skill(i).mag).collect();
        assert!(mags.iter().cloned().fold(0.0, f64::max) - mags.iter().cloned().fold(99.0, f64::min) > 4.0);
    }

    #[test]
    fn a_rival_off_the_route_is_picked_up_behind_its_best_progress() {
        let rail = RailPath::new(&[[0.0, 0.0, 0.0], [3000.0, 0.0, 0.0], [6000.0, 0.0, 0.0]]);
        let mut brain = RivalBrain::default();
        let mut rng = BrainRng::new(1);
        let skill = RaceClass::Pro.skill(0);
        let on = RacerView { pos: [1200.0, 0.0, 0.0], grounded: true, forward_vel: 30.0, against_wall: false, stunned: false };
        let s = brain.steer(&on, &rail, skill, &[], 1200.0, &mut rng);
        assert!(s.respawn.is_none());
        assert!(s.input.yaw.is_some());
        // Fallen far below and to the side of the route: picked up after a moment.
        let fallen = RacerView { pos: [1200.0, -900.0, 900.0], ..on };
        let mut picked = None;
        for t in 0..OFF_TICKS + 5 {
            if let Some(r) = brain.steer(&fallen, &rail, skill, &[], 1200.0, &mut rng).respawn {
                picked = Some((t, r));
                break;
            }
        }
        let (_, (p, _)) = picked.expect("the pick-up never came");
        assert!(p[0] < 1200.0 && p[0] > 900.0, "back on the route just behind the best progress: {p:?}");
        assert!(p[2].abs() < 1.0);
        assert_eq!(brain.pickups, 1);
        // Dizzy for a moment afterwards (the time lost), then racing again.
        assert!(brain.steer(&on, &rail, skill, &[], 1200.0, &mut rng).input.yaw.is_none());
    }

    #[test]
    fn the_rubber_band_eases_leaders_and_pushes_stragglers() {
        let rail = RailPath::new(&[[0.0, 0.0, 0.0], [20000.0, 0.0, 0.0]]);
        let skill = RaceClass::Rookie.skill(3);
        let me = RacerView { pos: [8000.0, 0.0, 0.0], grounded: true, forward_vel: 20.0, against_wall: false, stunned: false };
        let mut rng = BrainRng::new(2);
        let level = RivalBrain::default().steer(&me, &rail, skill, &[], 8000.0, &mut rng).input.mag.unwrap();
        let ahead = RivalBrain::default().steer(&me, &rail, skill, &[], 1000.0, &mut rng).input.mag.unwrap();
        let behind = RivalBrain::default().steer(&me, &rail, skill, &[], 14000.0, &mut rng).input.mag.unwrap();
        assert!(ahead < level && level < behind, "{ahead} {level} {behind}");
    }

    /// The rivals on the real course: the race brain gets a whole Pro field up Bob-omb Battlefield's
    /// mountain in a sensible time (a good human run is about 1:10), strung out rather than bunched.
    #[test]
    fn a_pro_field_climbs_the_real_bob_route() {
        let Some(times) = bob_field_times(RaceClass::Pro, 30 * 200) else { return };
        eprintln!("pro field finish times: {:?}", times.iter().map(|t| t.map(|t| format!("{:.1}", t as f64 / 30.0))).collect::<Vec<_>>());
        let done: Vec<u32> = times.iter().flatten().copied().collect();
        assert!(done.len() >= 6, "most of the field reaches the summit: {times:?}");
        let first = *done.iter().min().unwrap() as f64 / 30.0;
        assert!(first < 120.0, "the winner needs {first:.1}s");
    }

    /// The classes are a ladder on the real course: Rookies are beatable by a clean run, Champions are not.
    #[test]
    fn rookie_fields_are_slower_than_champion_fields() {
        let (Some(r), Some(c)) = (bob_field_times(RaceClass::Rookie, 30 * 200), bob_field_times(RaceClass::Champion, 30 * 200)) else { return };
        let best = |t: &[Option<u32>]| t.iter().flatten().min().copied().unwrap_or(u32::MAX) as f64 / 30.0;
        let mean = |t: &[Option<u32>]| t.iter().map(|x| x.unwrap_or(30 * 200) as f64).sum::<f64>() / t.len() as f64 / 30.0;
        eprintln!("rookie best {:.1} mean {:.1} / champion best {:.1} mean {:.1}", best(&r), mean(&r), best(&c), mean(&c));
        assert!(mean(&r) > mean(&c) + 3.0, "the Rookie field must be clearly slower");
        assert!(best(&c) < 75.0, "a Champion takes the summit quickly");
    }

    /// Simulates a field of seven rivals of `class` (and no player: the rubber band sees the player at the
    /// leader's progress) on the real bob course. None when the game data is absent.
    pub(super) fn bob_field_times(class: RaceClass, ticks: u32) -> Option<Vec<Option<u32>>> {
        use crate::game::world::{CourseRequest, LoadedCourse};
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if !assets.join("courses/bob.json").exists() {
            eprintln!("no game data: skipped");
            return None;
        }
        let course = LoadedCourse::load(&assets, CourseRequest::new("bob")).expect("bob loads");
        let rc = race_course(&course.world);
        let route = rc.route.clone();
        let rail = RailPath::new(&route);
        let lifts = lift_spans(&rail, &rc.lifts);
        eprintln!("lifts {:?} -> {:?}", rc.lifts, lifts);
        let opts = MarioCrowdOptions { world: course.world.clone(), route: None, has_leader: false, coin_count: 0, center: [0.0; 3], radius: 6000.0, seed: 0x5eed, single_lap: true };
        let mut crowd = MarioCrowd::new(opts, None, None);
        let start = rail.at(250.0);
        let yaw = atan2s(start.tangent[1], start.tangent[0]) as f64;
        let spots: Vec<V3> = (0..RIVALS)
            .map(|i| {
                let (row, col) = ((i / 4) as f64, (i % 4) as f64 - 1.5);
                let (tx, tz) = (start.tangent[0], start.tangent[1]);
                let (x, z) = (start.point[0] - tx * row * 220.0 - tz * col * 170.0, start.point[2] - tz * row * 220.0 + tx * col * 170.0);
                [x, course.world.find_floor(x, start.point[1] + 400.0, z).height, z]
            })
            .collect();
        crowd.populate_at(&spots, Personality::Racer, yaw, None);
        let mut brains = vec![RivalBrain::default(); RIVALS];
        let mut finish = vec![None; RIVALS];
        let mut rng = BrainRng::new(7);
        let trace = std::env::var_os("RACE_TRACE").is_some();
        if std::env::var("RACE_TRACE").as_deref() == Ok("2") {
            for p in &route {
                let h = rail.nearest(p, None, 0);
                eprintln!("route s={:.0} p=({:.0},{:.0},{:.0})", h.s, p[0], p[1], p[2]);
            }
        }
        for tick in 0..ticks {
            let positions: Vec<V3> = crowd.agents.iter().map(|a| a.body.pos()).collect();
            let lead = brains.iter().map(|b| b.s).fold(0.0, f64::max);
            for (i, a) in crowd.agents.iter_mut().enumerate() {
                if finish[i].is_some() {
                    a.manual = Some(DriveInput::default());
                    continue;
                }
                let m = &a.body.runtime;
                let view = RacerView { pos: m.pos, grounded: m.action & jumpman_sim::state::ACT_FLAG_AIR == 0, forward_vel: m.forward_vel, against_wall: m.wall.is_some(), stunned: m.action & (jumpman_sim::state::ACT_FLAG_INTANGIBLE | jumpman_sim::state::ACT_FLAG_INVULNERABLE) != 0 };
                let others: Vec<V3> = positions.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| *p).collect();
                let steer = brains[i].steer(&view, &rail, class.skill(i), &others, lead, &mut rng);
                if let Some((p, y)) = steer.respawn {
                    a.body.teleport(p, Some(y));
                }
                a.manual = Some(steer.input);
                if let Some(top) = lift_for(brains[i].best_s, &lifts) {
                    let at = rail.at(top + 60.0);
                    a.body.teleport([at.point[0], at.point[1] + 120.0, at.point[2]], None);
                    brains[i].lifted(&rail, top);
                }
            }
            crowd.update(None);
            for (i, a) in crowd.agents.iter().enumerate() {
                let pos = a.body.pos();
                let hit = rail.nearest(&pos, brains[i].hint, 60);
                if finish[i].is_none() && at_finish(rail.total, &hit, &pos) {
                    finish[i] = Some(tick);
                }
                if trace && Some(i) == std::env::var("RACE_IDX").ok().and_then(|v| v.parse().ok()).or(Some(0)) && tick % 60 == 0 {
                    eprintln!("t={:.0} s={:.0}/{:.0} pos=({:.0},{:.0},{:.0}) act={:x} fv={:.0} pickups={}", tick as f64 / 30.0, hit.s, rail.total, pos[0], pos[1], pos[2], a.body.runtime.action, a.body.runtime.forward_vel, brains[i].pickups);
                }
            }
            if finish.iter().all(|f| f.is_some()) {
                break;
            }
        }
        eprintln!("route {:.0}, pick-ups {:?}", rail.total, brains.iter().map(|b| b.pickups).collect::<Vec<_>>());
        Some(finish)
    }
}
