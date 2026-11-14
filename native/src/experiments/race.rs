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
            Self::Rookie => RivalSkill { mag: 55.0 - 8.0 * k, long_jump: 0.0, rough: 0.0 },
            // A tighter spread than Rookie's so a Pro race finishes in a bunch (1st to 4th within 5 s with a good player).
            Self::Pro => RivalSkill { mag: 61.5 - 4.5 * k, long_jump: 0.05 - 0.04 * k, rough: 0.0 },
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

/// Ticks a rival needs off the route before the pick-up.
const OFF_TICKS: u32 = 40;
/// Ticks a rival may go without gaining [`STALL_GAIN`] of route progress before the pick-up. Counted on progress alone:
/// jumping at a wall and being knocked back (or punched about) is motion, not progress, and does not reset it.
const STALL_TICKS: u32 = 120;
const STALL_GAIN: f64 = 150.0;
const LOOK_AHEAD: f64 = 300.0;

/// A rival's racing brain: pure pursuit along the route plus long jumps, shoves and the pick-up.
#[derive(Clone, Copy, Debug, Default)]
pub struct RivalBrain {
    pub hint: Option<usize>,
    /// Furthest progress along the route (the pick-up goes back a little behind it).
    pub best_s: f64,
    pub s: f64,
    /// Ticks since the route progress last passed `mark` (see [`STALL_TICKS`]).
    stall: u32,
    mark: f64,
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
    /// Sidestep round something on the line: ticks left and which side (+1 left of the route, -1 right).
    side_t: u8,
    side: f64,
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

/// Lakitu's ride over a lift, the same for the player and every rival: he swoops down onto the racer at the
/// foot (`LIFT_PICK` ticks, the racer held still), carries him along a visible arc to the top (`LIFT_CARRY`),
/// lets go just above the path (`LIFT_DROP`, held there), then flies off (`LIFT_LEAVE`, visual only: the racer
/// is free again after `LIFT_TICKS`). The ride is the whole time lost, so it costs everyone the same 1.7 s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LakituRide {
    pub from: V3,
    pub to: V3,
    /// Ticks since the pick-up.
    pub tick: u32,
    /// Height of the arc over the straight line at its middle (see [`LakituRide::clear_ground`]).
    pub apex: f64,
}
pub const LIFT_PICK: u32 = 9;
pub const LIFT_CARRY: u32 = 35;
pub const LIFT_DROP: u32 = LIFT_TICKS - LIFT_PICK - LIFT_CARRY;
pub const LIFT_LEAVE: u32 = 24;
/// Height of Lakitu's cloud above the racer he holds (Mario hangs just under the cloud).
pub const LAKITU_HANG: f64 = 230.0;

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The inverse of [`smooth`] on 0..1 (bisection; only used to sample the arc evenly).
fn unsmooth(u: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..30 {
        let mid = 0.5 * (lo + hi);
        if smooth(mid) < u {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

impl LakituRide {
    /// A ride from the racer's spot at the foot to the drop point above the top of the lift.
    pub fn new(from: V3, rail: &RailPath, top: f64) -> Self {
        let at = rail.at(top + 60.0);
        let to = [at.point[0], at.point[1] + 120.0, at.point[2]];
        let run = (to[0] - from[0]).hypot(to[2] - from[2]);
        Self { from, to, tick: 0, apex: 280.0 + run * 0.18 }
    }
    /// Raise the arc until it passes at least 300 units over the course's ground everywhere between the ends (Lakitu
    /// carries the racer over the mountainside, never through it).
    pub fn clear_ground(mut self, world: &CollisionWorld) -> Self {
        let flat = Self { apex: 0.0, ..self };
        for k in 2..=22 {
            let u = k as f64 / 24.0;
            let t = LIFT_PICK as f64 + LIFT_CARRY as f64 * unsmooth(u);
            let p = flat.racer_at(t);
            let floor = world.find_floor(p[0], 20000.0, p[2]);
            if floor.surface.is_some() {
                let need = floor.height + 300.0 - p[1];
                self.apex = self.apex.max(need / (4.0 * u * (1.0 - u)));
            }
        }
        self
    }
    /// Still holding the racer (his input is ignored and his position is the ride's).
    pub fn carrying(&self) -> bool {
        self.tick < LIFT_TICKS
    }
    /// Lakitu is still on screen.
    pub fn visible(&self) -> bool {
        self.tick < LIFT_TICKS + LIFT_LEAVE
    }
    /// The held racer's position at (fractional) tick `t`.
    pub fn racer_at(&self, t: f64) -> V3 {
        if t <= LIFT_PICK as f64 {
            return self.from;
        }
        let u = smooth((t - LIFT_PICK as f64) / LIFT_CARRY as f64);
        let (a, b) = (self.from, self.to);
        // A parabola over the straight line: rises steeply off the meadow, settles onto the path.
        let lift = self.apex * 4.0 * u * (1.0 - u) + (b[1] - a[1]) * (u * (2.0 - u) - u);
        [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u + lift, a[2] + (b[2] - a[2]) * u]
    }
    /// Lakitu's (cloud) position at fractional tick `t`.
    pub fn lakitu_at(&self, t: f64) -> V3 {
        let hang = |p: V3| [p[0], p[1] + LAKITU_HANG, p[2]];
        if t < LIFT_PICK as f64 {
            // Swoops down from high above, a little behind.
            let u = smooth(t / LIFT_PICK as f64);
            let (dx, dz) = (self.to[0] - self.from[0], self.to[2] - self.from[2]);
            let d = dx.hypot(dz).max(1.0);
            let k = (1.0 - u) * 700.0;
            let p = hang(self.from);
            return [p[0] - dx / d * k, p[1] + (1.0 - u) * 900.0, p[2] - dz / d * k];
        }
        if t <= LIFT_TICKS as f64 {
            return hang(self.racer_at(t));
        }
        // Off he goes: up and onwards.
        let u = smooth((t - LIFT_TICKS as f64) / LIFT_LEAVE as f64);
        let (dx, dz) = (self.to[0] - self.from[0], self.to[2] - self.from[2]);
        let d = dx.hypot(dz).max(1.0);
        let p = hang(self.to);
        [p[0] + dx / d * u * 900.0, p[1] + u * 1100.0, p[2] + dz / d * u * 900.0]
    }
    /// Facing yaw (SM64 units) along the ride.
    pub fn yaw(&self) -> f64 {
        atan2s(self.to[2] - self.from[2], self.to[0] - self.from[0]) as f64
    }
}

impl RivalBrain {
    /// Picked up by Lakitu for a lift to `top` (route distance): the time lost is the ride itself, so the
    /// racer is not dizzy afterwards (the brain is not consulted while he is carried).
    pub fn lifted(&mut self, rail: &RailPath, top: f64) {
        let at = rail.at(top);
        self.hint = Some(at.index);
        self.best_s = top;
        self.s = top;
        self.mark = top;
        self.stall = 0;
        self.off = 0;
        self.crouched = false;
        self.dizzy = 0;
    }

    /// One 30 Hz decision. `others` are the other racers' positions (the player included), `player_s`
    /// the player's progress (for the rubber band).
    pub fn steer(&mut self, me: &RacerView, rail: &RailPath, skill: RivalSkill, others: &[V3], player_s: f64, rng: &mut BrainRng) -> Steer {
        let hit = rail.nearest(&me.pos, self.hint, 40);
        self.hint = Some(hit.index);
        self.s = hit.s;
        if hit.s > self.best_s + 30.0 {
            self.best_s = hit.s;
        }
        // Stall: progress over a window, whatever the rival is doing meanwhile (a knockback is not progress).
        if self.best_s > self.mark + STALL_GAIN {
            self.mark = self.best_s;
            self.stall = 0;
        } else {
            self.stall += 1;
        }
        if self.dizzy > 0 {
            self.dizzy -= 1;
            self.stall = 0;
            return Steer::default();
        }
        let lateral = (me.pos[0] - hit.point[0]).hypot(me.pos[2] - hit.point[2]);
        let below = hit.point[1] - me.pos[1];
        // Fallen under the route (a lower layer of the mountain right below the path counts: no way back up from there).
        let off = lateral > 700.0 || (below > 380.0 && lateral > 150.0) || (below > 600.0 && me.grounded);
        self.off = if off { self.off + 1 } else { 0 };
        if me.stunned && self.off <= OFF_TICKS && self.stall <= STALL_TICKS {
            return Steer::default();
        }
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
            self.mark = self.best_s;
            self.dizzy = 20;
            self.pickups += 1;
            self.crouched = false;
            let yaw = atan2s(back.tangent[1], back.tangent[0]) as f64;
            return Steer { input: DriveInput::default(), respawn: Some(([back.point[0], back.point[1] + 160.0, back.point[2]], yaw)) };
        }
        // Pure pursuit: aim at a point a little further along the route; closer in where the route bends (a long look-ahead
        // cuts the corner, off the narrow mountain path or into the mountainside), and back onto the line when pressed
        // against a wall off it.
        let bend = bend_ahead(rail, hit);
        let mut look = if bend > 0.95 { LOOK_AHEAD } else if bend > 0.8 { 240.0 } else { 200.0 };
        if me.against_wall && lateral > 35.0 {
            look = 60.0;
        }
        let mut target = rail.at(hit.s + look).point;
        // Pressed against something standing on the line itself (a post, a lip the jump bonks off) and getting nowhere: step
        // round it, trying one side and then the other.
        if self.side_t == 0 && me.against_wall && me.grounded && lateral <= 35.0 && self.stall > 30 && self.stall % 40 < 2 {
            self.side = if (self.stall / 40 + self.pickups) % 2 == 0 { 1.0 } else { -1.0 };
            self.side_t = 24;
        }
        if self.side_t > 0 {
            self.side_t -= 1;
            let at = rail.at(hit.s + 140.0);
            let n = [-at.tangent[1], at.tangent[0]];
            target = [at.point[0] + n[0] * self.side * 170.0, at.point[1], at.point[2] + n[1] * self.side * 170.0];
        }
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
        } else if gap > 1200.0 {
            // A soft band near the player keeps the pack close without a rival visibly braking.
            mag *= 0.95;
        } else if gap < -3500.0 {
            mag = 64.0;
        } else if gap < -1500.0 {
            mag = (mag + 3.0).min(64.0);
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
        if me.grounded && me.against_wall && me.forward_vel < 10.0 && lateral < 60.0 && self.side_t == 0 {
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

/// How straight the route runs over the next stretch: the smallest cosine between the tangent here and the tangents up to
/// 450 units on (1 = straight, 0 = a right angle).
pub fn bend_ahead(rail: &RailPath, hit: RailHit) -> f64 {
    [150.0, 300.0, 450.0].iter().map(|d| {
        let t = rail.at(hit.s + d).tangent;
        hit.tangent[0] * t[0] + hit.tangent[1] * t[1]
    }).fold(1.0, f64::min)
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

/// Koopa's checkpoints the race leaves out: his detour over the plateau (see [`race_course`]).
const SKIPPED_LOOP: std::ops::RangeInclusive<usize> = 1..=7;

/// Plan the race on `world` (deterministic).
pub fn race_course(world: &CollisionWorld) -> RaceCourse {
    // Checkpoint 12 sits on the slippery foot of the mountain; snapped to standable ground it lands on the
    // plateau above, which leads nowhere. Without it the plan runs meadow, lift, mountain path.
    // Checkpoints 1-7 are Koopa's own loop over the plateau east of the start: it comes back down to the meadow 4,000 units
    // south of the grid, and a Mario who simply runs down the slope from the start is there in 4.3 s against about 10 s round
    // the loop (`try_race_line`, `find_race_shortcuts`). Rivals followed the loop, so anyone who ran straight won by six
    // seconds; the race now starts down the slope like that runner.
    let koopa: Vec<V3> = KOOPA_RACE_ROUTE.iter().enumerate().filter(|(i, _)| *i != 12 && !SKIPPED_LOOP.contains(i)).map(|(_, p)| *p).collect();
    let plan = super::crowd_route::mario_route_drop(world, &koopa, RACE_DROP);
    let lifts = plan.windows(2).filter(|w| w[1][1] - w[0][1] > 150.0 && !super::crowd_route::segment_walkable_drop(world, w[0], w[1], RACE_DROP)).map(|w| (w[0], w[1])).collect();
    let mut route = densify(&plan, 200.0);
    clear_walls(world, &mut route);
    RaceCourse { route, lifts }
}

/// Body room the race line keeps from walls (Mario's own wall radius is 50: a line that grazes the mountain's corners leaves
/// every rival who follows it closely jammed against the tip, which is where they used to get stuck).
pub const LINE_CLEARANCE: f64 = 100.0;

/// Push each point of the race line out of the walls around it (at a step's and at a body's height), as Mario himself is
/// pushed, as long as it keeps standing on the same floor.
pub fn clear_walls(world: &CollisionWorld, route: &mut [V3]) {
    for p in route.iter_mut() {
        let mut q = *p;
        for _ in 0..3 {
            for off in [30.0, 60.0, 150.0] {
                world.f32_find_wall_collision(&mut q, off, LINE_CLEARANCE);
            }
        }
        let floor = world.find_floor(q[0], q[1] + 100.0, q[2]);
        if floor.surface.is_some() && (floor.height - p[1]).abs() < 60.0 && (q[0] - p[0]).hypot(q[2] - p[2]) < LINE_CLEARANCE {
            *p = [q[0], floor.height, q[2]];
        }
    }
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

/// Lakitu's model (the cameraman's cloud and all, from the player's own import) and its scale.
const LAKITU_MODEL: &str = "actors/lakitu_cameraman/lakitu_geo.glb";
const LAKITU_SCALE: f32 = 2.6;
/// Where the model sits on the cloud (SM64 units above the cloud's middle).
const LAKITU_RIDE_Y: f32 = 55.0;

/// One tick of a rival's Lakitu ride: while he is carried his body is held on the arc with no input.
/// Returns whether the rival is being carried (the brain is skipped). The ride is kept after the drop while
/// Lakitu flies off.
fn ride_rival(body: &mut super::crowd::CrowdMario, manual: &mut Option<DriveInput>, ride: &mut Option<LakituRide>) -> bool {
    let Some(r) = ride.as_mut() else { return false };
    if !r.visible() {
        *ride = None;
        return false;
    }
    let carrying = r.carrying();
    if carrying {
        let p = r.racer_at(r.tick as f64 + 1.0);
        let m = &mut body.runtime;
        m.pos = p;
        m.vel = [0.0; 3];
        m.forward_vel = 0.0;
        m.face_angle[1] = r.yaw();
        *manual = Some(DriveInput::default());
    }
    r.tick += 1;
    carrying
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
    /// Lakitu carrying this racer over a lift (and flying off afterwards).
    ride: Option<LakituRide>,
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
    /// Lakitu lifts as route distances.
    lifts: Vec<(f64, f64)>,
    /// One Lakitu (cloud and all) per racer, hidden unless that racer is on a ride.
    lakitus: Vec<Entity>,
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
            lakitus: vec![],
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
        crowd.populate_at(&spots, Personality::Racer, yaw, Some(&mut *host));
        let mut runners = vec![Runner { name: "You".into(), player: true, agent: None, hit: None, finish_ticks: None, brain: RivalBrain::default(), skill: self.class.skill(0), ride: None }];
        for (i, a) in crowd.agents.iter_mut().enumerate() {
            a.name = RIVAL_NAMES.get(i).copied().unwrap_or("Rival").into();
            a.manual = Some(DriveInput::default());
            // Cap colour per rival.
            if let Some(e) = a.body.entity {
                host.commands.entity(e).insert((super::crowd::CrowdColors(super::crowd::CrowdLook { cap: RIVAL_COLORS[i % RIVAL_COLORS.len()], overalls: None }), super::crowd::NeedsTint));
            }
            runners.push(Runner { name: a.name.clone(), player: false, agent: Some(a.id), hit: None, finish_ticks: None, brain: RivalBrain::default(), skill: self.class.skill(i), ride: None });
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
            if ride_rival(&mut a.body, &mut a.manual, &mut r.ride) {
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
                r.ride = Some(LakituRide::new(a.body.pos(), rail, top).clear_ground(&a.body.runtime.world));
                r.brain.lifted(rail, top);
                ride_rival(&mut a.body, &mut a.manual, &mut r.ride);
            }
        }
    }

    /// Place and turn every racer's Lakitu (interpolated between ticks).
    fn draw_lakitus(&self, host: &mut Host) {
        let alpha = host.alpha;
        for (i, e) in self.lakitus.iter().enumerate() {
            match self.runners.get(i).and_then(|r| r.ride) {
                Some(ride) if ride.visible() => {
                    let t = (ride.tick as f64 - 1.0 + alpha).max(0.0);
                    let p = ride.lakitu_at(t);
                    let bob = (t * 0.35).sin() * 6.0;
                    let yaw = ride.yaw() * std::f64::consts::PI / 32768.0;
                    let tf = Transform::from_translation(crate::render::to_bevy([p[0], p[1] + bob, p[2]])).with_rotation(Quat::from_rotation_y(yaw as f32));
                    host.commands.entity(*e).insert((tf, Visibility::Inherited));
                }
                _ => {
                    host.commands.entity(*e).insert(Visibility::Hidden);
                }
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
        // Lakitu: a low-poly cloud of our own (the cameraman's cloud is a separate object in the original), with the imported
        // cameraman on it, animated, when the player's data has him; otherwise an original shell-and-head stand-in.
        let puff = host.meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap());
        let cloud = host.materials.add(StandardMaterial { base_color: Color::srgb(0.97, 0.98, 1.0), emissive: LinearRgba::rgb(0.25, 0.26, 0.3), perceptual_roughness: 1.0, ..default() });
        let has_model = host.asset_root.exists(LAKITU_MODEL);
        let (shell, skin) = (host.unlit(Color::srgb(0.25, 0.7, 0.25)), host.unlit(Color::srgb(1.0, 0.85, 0.3)));
        for _ in 0..RIVALS + 1 {
            let e = host.spawn((Transform::default(), Visibility::Hidden));
            for (dx, dy, dz, r) in [(-60.0, 0.0, 0.0, 62.0), (60.0, 0.0, 0.0, 62.0), (0.0, 12.0, 45.0, 58.0), (0.0, 12.0, -45.0, 58.0), (0.0, 26.0, 0.0, 70.0), (-28.0, -18.0, 0.0, 50.0), (28.0, -18.0, 0.0, 50.0)] {
                let c = host.spawn((Mesh3d(puff.clone()), MeshMaterial3d(cloud.clone()), Transform::from_xyz(dx * SCALE, dy * SCALE, dz * SCALE).with_scale(Vec3::splat(r * SCALE)), Visibility::Inherited));
                host.commands.entity(e).add_child(c);
            }
            if has_model {
                let m = crate::render::object_view::spawn_model(host.commands, host.assets, host.asset_root, LAKITU_MODEL, true).insert((super::XpEntity, Transform::from_xyz(0.0, LAKITU_RIDE_Y * SCALE, 0.0).with_scale(Vec3::splat(LAKITU_SCALE)))).id();
                host.commands.entity(e).add_child(m);
            } else {
                for (dy, r, m) in [(70.0, 55.0, &shell), (135.0, 34.0, &skin)] {
                    let c = host.spawn((Mesh3d(puff.clone()), MeshMaterial3d(m.clone()), Transform::from_xyz(0.0, dy * SCALE, 0.0).with_scale(Vec3::splat(r * SCALE)), Visibility::Inherited));
                    host.commands.entity(e).add_child(c);
                }
            }
            self.lakitus.push(e);
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
        if let Some(ride) = self.runners.first().and_then(|r| r.ride).filter(|r| r.carrying()) {
            // Held by Lakitu: the step starts on the arc (the physics only adds a tick of gravity).
            let m = &mut host.sim.mario;
            m.pos = ride.racer_at(ride.tick as f64);
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.face_angle[1] = ride.yaw();
            if m.action & jumpman_sim::state::ACT_FLAG_AIR == 0 {
                m.set_action(jumpman_sim::state::ACT_FREEFALL, 0.0);
            }
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        if self.state == RaceState::Grid {
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
            let ride = self.runners[0].ride;
            match ride {
                Some(mut r) if r.visible() => {
                    r.tick += 1;
                    if r.tick == LIFT_TICKS {
                        // Let go just above the path.
                        let rail = self.rail.as_ref().unwrap();
                        self.runners[0].hit = Some(rail.at(self.runners[0].brain.best_s));
                        host.fx("bonk", r.to);
                        // The camera trailed the arc through the air: put it back behind the racer on the path.
                        let yaw = host.sim.mario.face_angle[1];
                        host.rig.snap_behind(yaw);
                    }
                    self.runners[0].ride = Some(r);
                }
                Some(_) => self.runners[0].ride = None,
                None => {
                    if let (Some(top), Some(rail)) = (lift_for(self.runners[0].brain.best_s, &self.lifts), self.rail.as_ref()) {
                        let ride = LakituRide::new(host.sim.mario.pos, rail, top).clear_ground(&host.sim.mario.world);
                        self.runners[0].brain.lifted(rail, top);
                        if let Some(a) = self.auto.as_mut() {
                            a.lifted(rail, top);
                        }
                        self.runners[0].ride = Some(ride);
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
        self.draw_lakitus(host);
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
    fn lakitu_carries_along_an_arc_and_costs_the_same_ride() {
        let rail = RailPath::new(&[[0.0, 0.0, 0.0], [2000.0, 900.0, 0.0], [4000.0, 900.0, 0.0]]);
        let ride = LakituRide::new([100.0, 0.0, 0.0], &rail, 2000.0);
        assert_eq!(LIFT_PICK + LIFT_CARRY + LIFT_DROP, LIFT_TICKS);
        // Held still while Lakitu swoops down, which starts high above.
        assert_eq!(ride.racer_at(3.0), ride.from);
        assert!(ride.lakitu_at(0.0)[1] > ride.from[1] + 800.0);
        assert!((ride.lakitu_at(LIFT_PICK as f64)[1] - (ride.from[1] + LAKITU_HANG)).abs() < 1.0);
        // The arc: continuous, above both the straight line and the end points, ending at the drop point.
        let mut last = ride.racer_at(LIFT_PICK as f64);
        for t in LIFT_PICK + 1..=LIFT_TICKS {
            let p = ride.racer_at(t as f64);
            let u = (p[0] - ride.from[0]) / (ride.to[0] - ride.from[0]);
            assert!(p[1] >= ride.from[1] + (ride.to[1] - ride.from[1]) * u - 1.0, "under the line at {t}");
            assert!((p[0] - last[0]).hypot(p[1] - last[1]) < 220.0, "a jump at {t}");
            last = p;
        }
        assert!((last[0] - ride.to[0]).abs() < 1e-6 && (last[1] - ride.to[1]).abs() < 1e-6);
        let mid = ride.racer_at((LIFT_PICK + LIFT_CARRY / 2) as f64);
        assert!(mid[1] > ride.to[1], "the arc rises over the top: {mid:?}");
        // Then Lakitu flies off and is gone.
        let mut r = ride;
        r.tick = LIFT_TICKS;
        assert!(!r.carrying() && r.visible());
        assert!(r.lakitu_at((LIFT_TICKS + LIFT_LEAVE) as f64)[1] > ride.to[1] + 1000.0);
        r.tick = LIFT_TICKS + LIFT_LEAVE;
        assert!(!r.visible());
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
        let Some(times) = bob_field_times(RaceClass::Pro, 30 * 200, 7) else { return };
        eprintln!("pro field finish times: {:?}", times.iter().map(|t| t.map(|t| format!("{:.1}", t as f64 / 30.0))).collect::<Vec<_>>());
        let done: Vec<u32> = times.iter().flatten().copied().collect();
        assert!(done.len() >= 6, "most of the field reaches the summit: {times:?}");
        let first = *done.iter().min().unwrap() as f64 / 30.0;
        assert!(first < 120.0, "the winner needs {first:.1}s");
    }

    /// The classes are a ladder on the real course: Rookies are beatable by a clean run, Champions are not.
    #[test]
    fn rookie_fields_are_slower_than_champion_fields() {
        let (Some(r), Some(c)) = (bob_field_times(RaceClass::Rookie, 30 * 200, 7), bob_field_times(RaceClass::Champion, 30 * 200, 7)) else { return };
        let best = |t: &[Option<u32>]| t.iter().flatten().min().copied().unwrap_or(u32::MAX) as f64 / 30.0;
        let mean = |t: &[Option<u32>]| t.iter().map(|x| x.unwrap_or(30 * 200) as f64).sum::<f64>() / t.len() as f64 / 30.0;
        eprintln!("rookie best {:.1} mean {:.1} / champion best {:.1} mean {:.1}", best(&r), mean(&r), best(&c), mean(&c));
        assert!(mean(&r) > mean(&c) + 3.0, "the Rookie field must be clearly slower");
        assert!(best(&c) < 75.0, "a Champion takes the summit quickly");
    }

    /// No rival is left behind: in every class and on several seeds the whole field reaches the summit within 200 s (a rival
    /// stuck on a ledge is picked up by Lakitu once it stops making progress along the route, whatever it is doing there).
    /// `RACE_SEEDS=n` widens the sweep.
    #[test]
    fn every_rival_of_every_class_finishes_within_200_s() {
        let seeds: u64 = std::env::var("RACE_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        let mut stuck = vec![];
        let only: Option<usize> = std::env::var("RACE_CLASS").ok().and_then(|v| v.parse().ok());
        for class in RaceClass::ALL.into_iter().filter(|c| only.is_none_or(|o| o == c.index())) {
            for seed in std::env::var("RACE_SEED").ok().and_then(|v| v.parse().ok()).map_or(1..=seeds, |s: u64| s..=s) {
                let Some(times) = bob_field_times(class, 30 * 200, seed) else { return };
                let shown: Vec<String> = times.iter().map(|t| t.map_or("DNF".to_string(), |t| format!("{:.1}", t as f64 / 30.0))).collect();
                eprintln!("FIELD {class:?} seed {seed}: {}", shown.join(" "));
                for (i, t) in times.iter().enumerate() {
                    if t.is_none() {
                        stuck.push((class, seed, i));
                    }
                }
            }
        }
        assert!(stuck.is_empty(), "rivals that never reached the summit (class, seed, rival): {stuck:?}");
    }

    /// The research checklist's race criteria (`MINIGAMES_RESEARCH.md` section 6) with a competent player (the Champion brain's
    /// fastest habits, as `JUMPMAN_RACE_AUTO`) on the back row: the gap from 1st to 4th, the player's place changes, how often
    /// a rival is within 15 m, the player's place and the race length. `RACE_SEEDS` (default 6), `RACE_CLASS`.
    /// `cargo test --release -p jumpman --lib audit_race -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn audit_race() {
        let seeds: u64 = std::env::var("RACE_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(6);
        let only: Option<usize> = std::env::var("RACE_CLASS").ok().and_then(|v| v.parse().ok());
        for class in RaceClass::ALL.into_iter().filter(|c| only.is_none_or(|o| o == c.index())) {
            let (mut close, mut gaps, mut changes, mut near, mut places, mut lens, mut n) = (0, 0.0, 0u32, 0.0, vec![], 0.0, 0);
            for seed in 1..=seeds {
                let Some(r) = bob_race(class, 30 * 200, seed, Some(RaceClass::Champion.skill(0))) else { return };
                let mut t: Vec<f64> = r.finish.iter().map(|f| f.map_or(200.0, |t| t as f64 / 30.0)).collect();
                let me = *t.last().unwrap();
                t.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let gap = t[3] - t[0];
                let place = t.iter().position(|x| *x == me).unwrap() + 1;
                eprintln!("RACE {class:?} seed {seed}: player {me:.1}s place {place}, 1st-4th {gap:.1}s, place changes {}, rival within 15 m {:.0}%, player pick-ups {}, times {:?}", r.place_changes, 100.0 * r.near, r.player_pickups, t.iter().map(|x| format!("{x:.1}")).collect::<Vec<_>>());
                close += (gap < 5.0) as u32;
                gaps += gap;
                changes += r.place_changes;
                near += r.near;
                places.push(place);
                lens += me;
                n += 1;
            }
            let nf = n as f64;
            eprintln!("AUDIT RACE {class:?}: 1st-4th under 5 s in {close}/{n} (mean {:.1}s), place changes {:.1}, rival within 15 m {:.0}%, player places {places:?}, player time {:.1}s", gaps / nf, changes as f64 / nf, 100.0 * near / nf, lens / nf);
        }
    }

    /// Lines that leave the race route and rejoin it further on with a saving: straight segments the planner's walk check passes
    /// (drops included), and ones with a single gap a long jump clears (`RACE_GAP` units, default 450). `cargo test --release -p
    /// jumpman --lib find_race_shortcuts -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn find_race_shortcuts() {
        use crate::game::world::{CourseRequest, LoadedCourse};
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let Ok(course) = LoadedCourse::load(&assets, CourseRequest::new("bob")) else { return };
        let world = &course.world;
        let rc = race_course(world);
        let rail = RailPath::new(&rc.route);
        let gap_max: f64 = std::env::var("RACE_GAP").ok().and_then(|v| v.parse().ok()).unwrap_or(450.0);
        let pts: Vec<(f64, V3)> = (0..(rail.total / 150.0) as usize).map(|k| { let a = rail.at(k as f64 * 150.0); (a.s, a.point) }).collect();
        let floor_at = |x: f64, z: f64, y: f64| -> Option<f64> {
            let f = world.find_floor(x, y + 300.0, z);
            f.surface.filter(|s| s.normal[1] > 0.7).map(|_| f.height)
        };
        let mut found: Vec<(f64, f64, f64, f64, &str)> = vec![];
        for i in 0..pts.len() {
            for j in i + 1..pts.len() {
                let (si, a) = pts[i];
                let (sj, b) = pts[j];
                let d = (b[0] - a[0]).hypot(b[2] - a[2]);
                let save = sj - si - d;
                if save < 500.0 || d > 4000.0 || b[1] - a[1] > 60.0 {
                    continue;
                }
                if super::super::crowd_route::segment_walkable_drop(world, a, b, RACE_DROP) {
                    found.push((save, si, sj, d, "walk"));
                    continue;
                }
                // One gap (no standable floor) no longer than a long jump, landing no higher than the take-off.
                let steps = (d / 50.0).ceil() as usize;
                let (mut prev, mut gap, mut gaps, mut ok, mut take_off) = (a[1], 0.0, 0, true, a[1]);
                for k in 1..=steps {
                    let t = k as f64 / steps as f64;
                    let (x, z) = (a[0] + (b[0] - a[0]) * t, a[2] + (b[2] - a[2]) * t);
                    match floor_at(x, z, prev.max(a[1] + (b[1] - a[1]) * t)) {
                        Some(h) if (gap == 0.0 && h - prev <= 40.0 && prev - h <= RACE_DROP) || (gap > 0.0 && h <= take_off + 20.0 && take_off - h <= RACE_DROP) => {
                            prev = h;
                            gap = 0.0;
                        }
                        _ => {
                            if gap == 0.0 {
                                gaps += 1;
                                take_off = prev;
                            }
                            gap += d / steps as f64;
                            if gap > gap_max || gaps > 1 {
                                ok = false;
                                break;
                            }
                        }
                    }
                }
                if ok && gaps == 1 && gap == 0.0 {
                    found.push((save, si, sj, d, "long jump"));
                }
            }
        }
        found.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let mut shown: Vec<(f64, f64)> = vec![];
        for (save, si, sj, d, kind) in found {
            if shown.iter().any(|(a, b)| (si - a).abs() < 1200.0 && (sj - b).abs() < 1200.0) {
                continue;
            }
            shown.push((si, sj));
            let (a, b) = (rail.at(si).point, rail.at(sj).point);
            eprintln!("SHORTCUT {kind}: s {si:.0} -> {sj:.0} ({d:.0} straight, saves {save:.0}) from ({:.0},{:.0},{:.0}) to ({:.0},{:.0},{:.0})", a[0], a[1], a[2], b[0], b[1], b[2]);
            if shown.len() >= 15 {
                break;
            }
        }
    }

    /// Runs one full-physics Mario straight from route distance `RACE_FROM` to `RACE_TO` (full stick at the target; long jumps
    /// with `RACE_LJ=1`) and prints how long it took and where it ended. `cargo test --release -p jumpman --lib try_race_line --
    /// --ignored --nocapture`.
    #[test]
    #[ignore]
    fn try_race_line() {
        use crate::game::world::{CourseRequest, LoadedCourse};
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let Ok(course) = LoadedCourse::load(&assets, CourseRequest::new("bob")) else { return };
        let rc = race_course(&course.world);
        let rail = RailPath::new(&rc.route);
        let env = |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let (from, to) = (env("RACE_FROM", 9450.0), env("RACE_TO", 12600.0));
        let r = try_line(&course.world, &rail, rail.at(from).point, rail.at(to).point, std::env::var_os("RACE_LJ").is_some(), true);
        eprintln!("LINE {from:.0} -> {to:.0}: {r:?}");
    }

    /// Straight-line run of one crowd Mario from `a` to `b`: (seconds to arrive within 150 units at the target's height, or
    /// None, final position).
    pub(super) fn try_line(world: &std::sync::Arc<CollisionWorld>, rail: &RailPath, a: V3, b: V3, long_jump: bool, trace: bool) -> (Option<f64>, V3) {
        let opts = MarioCrowdOptions { world: world.clone(), route: None, has_leader: false, coin_count: 0, center: [0.0; 3], radius: 6000.0, seed: 1, single_lap: true };
        let mut crowd = MarioCrowd::new(opts, None, None);
        let yaw = atan2s(b[2] - a[2], b[0] - a[0]) as f64;
        crowd.populate_at(&[[a[0], a[1] + 20.0, a[2]]], Personality::Racer, yaw, None);
        let mut crouched = false;
        for tick in 0..30 * 20 {
            let p = crowd.agents[0].body.pos();
            if (p[0] - b[0]).hypot(p[2] - b[2]) < 150.0 && (p[1] - b[1]).abs() < 200.0 {
                return (Some(tick as f64 / 30.0), p);
            }
            let m = &crowd.agents[0].body.runtime;
            let grounded = m.action & jumpman_sim::state::ACT_FLAG_AIR == 0;
            let mut input = DriveInput { yaw: Some(atan2s(b[2] - p[2], b[0] - p[0]) as f64), mag: Some(64.0), ..Default::default() };
            if crouched {
                crouched = false;
                input.jump = true;
                input.hold_a = true;
            } else if long_jump && grounded && m.forward_vel > 26.0 {
                crouched = true;
                input.crouch = true;
            }
            if trace && tick % 15 == 0 {
                let h = rail.nearest(&p, None, 0);
                eprintln!("  t={:.1} pos=({:.0},{:.0},{:.0}) s={:.0} act={:x} fv={:.0}", tick as f64 / 30.0, p[0], p[1], p[2], h.s, m.action, m.forward_vel);
            }
            crowd.agents[0].manual = Some(input);
            crowd.update(None);
        }
        (None, crowd.agents[0].body.pos())
    }

    /// `RACE_PROBE=x,y,z cargo test ... probe_collision -- --ignored --nocapture`: the walls and floors near a spot of the
    /// route (what a rival is stuck on).
    #[test]
    #[ignore]
    fn probe_collision() {
        use crate::game::world::{CourseRequest, LoadedCourse};
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let Ok(course) = LoadedCourse::load(&assets, CourseRequest::new("bob")) else { return };
        let p: Vec<f64> = std::env::var("RACE_PROBE").unwrap_or_default().split(',').filter_map(|v| v.parse().ok()).collect();
        let r: f64 = std::env::var("RACE_PROBE_R").ok().and_then(|v| v.parse().ok()).unwrap_or(250.0);
        for s in course.world.surfaces() {
            let c = [(s.v1[0] + s.v2[0] + s.v3[0]) / 3.0, (s.v1[1] + s.v2[1] + s.v3[1]) / 3.0, (s.v1[2] + s.v2[2] + s.v3[2]) / 3.0];
            let near = [s.v1, s.v2, s.v3, c].iter().any(|v| (v[0] - p[0]).hypot(v[2] - p[2]) < r) && s.upper_y > p[1] - 300.0 && s.lower_y < p[1] + 500.0;
            if near {
                let kind = if s.normal[1] > 0.01 { "floor" } else if s.normal[1] < -0.01 { "ceil" } else { "wall" };
                eprintln!("{kind} n=({:.2},{:.2},{:.2}) type={:#x} obj={:?} v=({:.0},{:.0},{:.0}) ({:.0},{:.0},{:.0}) ({:.0},{:.0},{:.0})", s.normal[0], s.normal[1], s.normal[2], s.kind, s.object, s.v1[0], s.v1[1], s.v1[2], s.v2[0], s.v2[1], s.v2[2], s.v3[0], s.v3[1], s.v3[2]);
            }
        }
        for o in course.objects.objects.iter().filter(|o| (o.pos[0] - p[0]).hypot(o.pos[2] - p[2]) < r * 2.0) {
            eprintln!("object {:?} at {:?}", o.behavior, o.pos.map(|v| v.round()));
        }
    }

    /// Simulates a field of seven rivals of `class` (and no player: the rubber band sees the player at the
    /// leader's progress) on the real bob course. None when the game data is absent.
    pub(super) fn bob_field_times(class: RaceClass, ticks: u32, seed: u64) -> Option<Vec<Option<u32>>> {
        bob_race(class, ticks, seed, None).map(|r| r.finish)
    }

    /// One simulated race (see [`bob_race`]).
    pub(super) struct RaceRun {
        /// Finish tick per racer; with a player, the player is the last entry.
        pub finish: Vec<Option<u32>>,
        /// Times the player's place changed (after the first 3 s).
        pub place_changes: u32,
        /// Share of the player's racing ticks with a rival within 1500 units (15 m).
        pub near: f64,
        /// The player's route progress (s) at each pick-up and lift (diagnostics).
        pub player_pickups: u32,
    }

    /// As `bob_field_times`, optionally with an eighth racer standing in for the player on the back row, driven by the race brain
    /// with `player` habits; the rivals' rubber band then follows that racer, as in the game.
    pub(super) fn bob_race(class: RaceClass, ticks: u32, seed: u64, player: Option<RivalSkill>) -> Option<RaceRun> {
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
        let racers = RIVALS + player.is_some() as usize;
        let pi = player.map(|_| RIVALS);
        let spots: Vec<V3> = (0..racers)
            .map(|i| {
                // The player starts on the back row (grid spot 5), as in the game; the rivals fill the rest.
                let i = match pi {
                    Some(_) if i == RIVALS => 5,
                    Some(_) if i >= 5 => i + 1,
                    _ => i,
                };
                let (row, col) = ((i / 4) as f64, (i % 4) as f64 - 1.5);
                let (tx, tz) = (start.tangent[0], start.tangent[1]);
                let (x, z) = (start.point[0] - tx * row * 220.0 - tz * col * 170.0, start.point[2] - tz * row * 220.0 + tx * col * 170.0);
                [x, course.world.find_floor(x, start.point[1] + 400.0, z).height, z]
            })
            .collect();
        crowd.populate_at(&spots, Personality::Racer, yaw, None);
        let mut brains = vec![RivalBrain::default(); racers];
        let mut finish = vec![None; racers];
        let mut rides: Vec<Option<LakituRide>> = vec![None; racers];
        let mut lifted = vec![0u32; racers];
        let (mut last_place, mut place_changes, mut near, mut racing) = (usize::MAX, 0u32, 0u32, 0u32);
        let mut rng = BrainRng::new(seed);
        let trace = std::env::var_os("RACE_TRACE").is_some();
        if std::env::var("RACE_TRACE").as_deref() == Ok("2") {
            for p in &route {
                let h = rail.nearest(p, None, 0);
                eprintln!("route s={:.0} p=({:.0},{:.0},{:.0})", h.s, p[0], p[1], p[2]);
            }
        }
        for tick in 0..ticks {
            let positions: Vec<V3> = crowd.agents.iter().map(|a| a.body.pos()).collect();
            let lead = match pi {
                Some(p) => brains[p].s,
                None => brains.iter().map(|b| b.s).fold(0.0, f64::max),
            };
            for (i, a) in crowd.agents.iter_mut().enumerate() {
                if finish[i].is_some() {
                    a.manual = Some(DriveInput::default());
                    continue;
                }
                if ride_rival(&mut a.body, &mut a.manual, &mut rides[i]) {
                    continue;
                }
                let m = &a.body.runtime;
                let view = RacerView { pos: m.pos, grounded: m.action & jumpman_sim::state::ACT_FLAG_AIR == 0, forward_vel: m.forward_vel, against_wall: m.wall.is_some(), stunned: m.action & (jumpman_sim::state::ACT_FLAG_INTANGIBLE | jumpman_sim::state::ACT_FLAG_INVULNERABLE) != 0 };
                let others: Vec<V3> = positions.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| *p).collect();
                let skill = if Some(i) == pi { player.unwrap() } else { class.skill(i) };
                let band = if Some(i) == pi { brains[i].s } else { lead };
                let steer = brains[i].steer(&view, &rail, skill, &others, band, &mut rng);
                if let Some((p, y)) = steer.respawn {
                    if std::env::var_os("RACE_PICKS").is_some() {
                        eprintln!("PICKUP rival {i} t={:.1} at s={:.0} pos=({:.0},{:.0},{:.0})", tick as f64 / 30.0, brains[i].pick_at, m.pos[0], m.pos[1], m.pos[2]);
                    }
                    a.body.teleport(p, Some(y));
                }
                a.manual = Some(steer.input);
                if let Some(top) = lift_for(brains[i].best_s, &lifts) {
                    rides[i] = Some(LakituRide::new(a.body.pos(), &rail, top).clear_ground(&course.world));
                    brains[i].lifted(&rail, top);
                    ride_rival(&mut a.body, &mut a.manual, &mut rides[i]);
                    lifted[i] += 1;
                }
            }
            crowd.update(None);
            for (i, a) in crowd.agents.iter().enumerate() {
                let pos = a.body.pos();
                let hit = rail.nearest(&pos, brains[i].hint, 60);
                if finish[i].is_none() && at_finish(rail.total, &hit, &pos) {
                    finish[i] = Some(tick);
                }
                let every = std::env::var("RACE_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
                if trace && Some(i) == std::env::var("RACE_IDX").ok().and_then(|v| v.parse().ok()).or(Some(0)) && tick % every == 0 {
                    eprintln!("t={:.1} s={:.0}/{:.0} best={:.0} pos=({:.0},{:.0},{:.0}) rail=({:.0},{:.0},{:.0}) act={:x} fv={:.0} wall={} pickups={}", tick as f64 / 30.0, hit.s, rail.total, brains[i].best_s, pos[0], pos[1], pos[2], hit.point[0], hit.point[1], hit.point[2], a.body.runtime.action, a.body.runtime.forward_vel, a.body.runtime.wall.is_some(), brains[i].pickups);
                }
            }
            if let Some(p) = pi {
                if finish[p].is_none() {
                    let progress: Vec<f64> = brains.iter().map(|b| b.s).collect();
                    let place = standings(&finish, &progress).iter().position(|&i| i == p).unwrap_or(0);
                    if tick > 90 && place != last_place && last_place != usize::MAX {
                        place_changes += 1;
                    }
                    last_place = place;
                    racing += 1;
                    let me = crowd.agents[p].body.pos();
                    near += crowd.agents.iter().enumerate().any(|(j, a)| j != p && {
                        let q = a.body.pos();
                        (q[0] - me[0]).hypot(q[2] - me[2]) < 1500.0 && (q[1] - me[1]).abs() < 600.0
                    }) as u32;
                }
            }
            if finish.iter().all(|f| f.is_some()) {
                break;
            }
        }
        eprintln!("route {:.0}, pick-ups {:?}, lakitu lifts {:?}", rail.total, brains.iter().map(|b| b.pickups).collect::<Vec<_>>(), lifted);
        Some(RaceRun { finish, place_changes, near: near as f64 / racing.max(1) as f64, player_pickups: pi.map_or(0, |p| brains[p].pickups) })
    }
}
