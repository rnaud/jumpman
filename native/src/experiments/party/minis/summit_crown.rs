//! Summit Crown: king of the hill on the real summit of the mountain. Standing in the crown circle alone scores; in a
//! shared circle the one who has stood in it longest keeps the crown and scores at half rate (so a contest is still worth
//! winning, and pushing in is how you take it). Punches (and dives) knock rivals back, and the summit's edges are real: a
//! Mario knocked off is carried back up after a moment. 35 seconds. Twist (half the games): the Roaming Crown, whose
//! circle moves to another spot of the summit every few seconds.
use super::*;

pub const SECS: f32 = 35.0;
/// Radius of the crown circle.
pub const ZONE: f64 = 300.0;
const REACH: f64 = 160.0;
const POWER: f64 = 34.0;
/// Seconds a knocked Mario cannot be hit again (and does not score).
const DAZE: f32 = 1.0;
/// Seconds before a Mario who fell off is carried back up.
const CARRY: f32 = 1.4;
/// Scoring rate of the crown holder while someone else shares the circle.
pub const CONTESTED_RATE: f32 = 0.5;
/// The Roaming Crown moves this often, to a spot this far (at most) from the summit's middle.
const ROAM_SECS: f32 = 7.0;
const ROAM_R: f64 = 260.0;

pub struct SummitCrown {
    r: Roster,
    rng: Rng,
    field: Arc<Field>,
    score: [f32; 4],
    t: f32,
    daze: [f32; 4],
    fallen: [Option<f32>; 4],
    /// Who held the circle alone last step (for the "crown" cue).
    king: Option<usize>,
    pub falls: [u32; 4],
    /// Seconds each Mario has stood in the circle without a break (the longest in a shared circle holds the crown).
    pub in_for: [f32; 4],
    /// Someone shares the circle with the holder.
    pub contested: bool,
    /// Where the circle is now (the summit's middle unless the crown roams).
    pub zone: [f64; 3],
    pub roam: bool,
    think: [f32; 4],
    aim: [Option<usize>; 4],
    evs: Vec<MiniEv>,
}

impl SummitCrown {
    pub fn new(seed: u64, active: [bool; 4], field: Arc<Field>) -> Self {
        let zone = field.center;
        SummitCrown { r: Roster::new(active), rng: Rng::new(seed), field, score: [0.0; 4], t: 0.0, daze: [0.0; 4], fallen: [None; 4], king: None, falls: [0; 4], in_for: [0.0; 4], contested: false, zone, roam: (seed >> 7) & 1 == 1, think: [0.0; 4], aim: [None; 4], evs: vec![] }
    }

    pub fn in_zone(&self, b: &Body) -> bool {
        hdist(b.pos, self.zone) < ZONE && (b.pos[1] - self.zone[1]).abs() < 220.0
    }

    /// The next spot of the Roaming Crown: a standable point of the summit near its middle, away from where it was.
    fn next_zone(&mut self) -> [f64; 3] {
        let c = self.field.center;
        let pts: Vec<[f64; 3]> = self.field.points().iter().copied().filter(|p| hdist(*p, c) < ROAM_R && hdist(*p, self.zone) > 150.0 && (p[1] - c[1]).abs() < 120.0).collect();
        if pts.is_empty() {
            return c;
        }
        pts[self.rng.below(pts.len())]
    }
}

impl MiniGame for SummitCrown {
    fn start(&self, p: usize) -> [f64; 3] {
        self.field.start(p, 0.62)
    }

    fn step(&mut self, dt: f32, bodies: &[Body; 4], _drives: &[Drive; 4]) {
        if self.done() {
            return;
        }
        let before = self.t;
        self.t += dt;
        if self.roam && (before / ROAM_SECS).floor() != (self.t / ROAM_SECS).floor() {
            self.zone = self.next_zone();
            self.evs.push(MiniEv::Cue { kind: "teleport", pos: self.zone });
        }
        for d in &mut self.daze {
            *d = (*d - dt).max(0.0);
        }
        // Falls: off the summit (below it, or wandered off its floor) -> carried back after a moment.
        for p in self.r.ids() {
            match self.fallen[p] {
                Some(t0) if self.t - t0 > CARRY => {
                    self.fallen[p] = None;
                    let spot = self.field.start((p + self.falls[p] as usize) % 4, 0.62);
                    self.evs.push(MiniEv::Place { p, pos: spot });
                    self.daze[p] = 0.6;
                }
                Some(_) => {}
                None => {
                    let b = bodies[p].pos;
                    if self.field.fallen(b) || (hdist(b, self.field.center) > self.field.radius + 150.0) {
                        self.fallen[p] = Some(self.t);
                        self.falls[p] += 1;
                        self.evs.push(MiniEv::Cue { kind: "bonk", pos: b });
                    }
                }
            }
        }
        // Hits (ties go round the seats).
        for a in seat_order(self.t).into_iter().filter(|&a| self.r.active[a]) {
            if self.fallen[a].is_some() || self.daze[a] > 0.0 {
                continue;
            }
            for v in self.r.ids() {
                if v == a || self.fallen[v].is_some() || self.daze[v] > 0.0 {
                    continue;
                }
                if hits(&bodies[a], &bodies[v], REACH) {
                    let power = if bodies[a].grounded { POWER } else { POWER * 1.25 };
                    self.evs.push(MiniEv::Knock { p: v, from: bodies[a].pos, power });
                    self.evs.push(MiniEv::Cue { kind: "kick", pos: bodies[v].pos });
                    self.daze[v] = DAZE;
                }
            }
        }
        // Scoring: alone in the circle at the full rate; shared, the one who has stood in it longest (ties: who held it
        // last, then round the seats) at half rate.
        let inside: Vec<usize> = self.r.ids().filter(|&p| self.fallen[p].is_none() && self.daze[p] <= 0.0 && self.in_zone(&bodies[p])).collect();
        for p in 0..4 {
            self.in_for[p] = if inside.contains(&p) { self.in_for[p] + dt } else { 0.0 };
        }
        let order = seat_order(self.t);
        let king = inside.iter().copied().max_by(|&a, &b| {
            let key = |p: usize| (self.in_for[p], self.king == Some(p), 4 - order.iter().position(|&q| q == p).unwrap_or(0));
            key(a).partial_cmp(&key(b)).unwrap()
        });
        self.contested = inside.len() > 1;
        if let Some(k) = king {
            self.score[k] += if self.contested { dt * CONTESTED_RATE } else { dt };
            if self.king != Some(k) {
                self.evs.push(MiniEv::Cue { kind: "coin", pos: bodies[k].pos });
            }
        }
        self.king = king;
    }

    fn bot(&mut self, p: usize, lvl: Level, bodies: &[Body; 4]) -> Drive {
        if self.fallen[p].is_some() || self.daze[p] > 0.0 {
            return Drive::default();
        }
        let me = bodies[p];
        let skill = lvl.skill() as f64;
        self.think[p] -= DT;
        if self.think[p] <= 0.0 {
            self.think[p] = lvl.react() + self.rng.f() * 0.3;
            // Go for the nearest rival in or near the circle (Easy bots often just walk in).
            let rivals = self.r.ids().filter(|&q| q != p && self.fallen[q].is_none() && hdist(bodies[q].pos, self.zone) < ZONE + 160.0);
            let target = rivals.min_by(|a, b| hdist(me.pos, bodies[*a].pos).partial_cmp(&hdist(me.pos, bodies[*b].pos)).unwrap());
            self.aim[p] = target.filter(|_| self.rng.chance(0.35 + 0.6 * skill as f32));
        }
        let home = {
            // Hold a spot inside the circle, a little off the middle (not all four on one point).
            let a = p as f64 * 1.7;
            [self.zone[0] + a.cos() * 60.0, self.zone[1], self.zone[2] + a.sin() * 60.0]
        };
        match self.aim[p] {
            Some(q) if self.fallen[q].is_none() => {
                let them = bodies[q].pos;
                let d = hdist(me.pos, them);
                // Shove toward the edge: come at them from the middle's side.
                let mut drive = run_to(me.pos, them, 0.0);
                drive.mag = 1.0;
                if d < REACH * (0.75 + 0.2 * skill) && facing(&me, them, 0.5) && self.rng.chance(0.25 + 0.5 * skill as f32) {
                    drive.punch = true;
                }
                // Out of reach but closing fast: a diving lunge (Hard bots).
                if d > 200.0 && d < 330.0 && me.grounded && self.rng.chance(0.03 * skill as f32) {
                    drive.jump = true;
                }
                drive
            }
            _ => {
                let mut d = run_to(me.pos, home, 40.0);
                d.mag = d.mag.max(0.5);
                d
            }
        }
    }

    fn events(&mut self) -> Vec<MiniEv> {
        std::mem::take(&mut self.evs)
    }

    fn done(&self) -> bool {
        self.t >= SECS
    }

    fn time_left(&self) -> f32 {
        (SECS - self.t).max(0.0)
    }

    fn scores(&self) -> [f32; 4] {
        self.r.mask(self.score)
    }

    fn twist(&self) -> Option<&'static str> {
        self.roam.then_some("Roaming Crown: the circle moves to a new spot every 7 seconds.")
    }

    fn scene(&self, out: &mut Stage) {
        let c = self.zone;
        let col = match self.king {
            // A contested crown blinks between the holder's colour and white.
            Some(k) if !self.contested || (self.t * 6.0) as i32 % 2 == 0 => PLAYER_COLORS[k],
            Some(_) => 0xffffff,
            None => 0xfff2a0,
        };
        // The circle: a ring of studs on the summit floor, in the holder's colour.
        for i in 0..20 {
            let a = i as f64 / 20.0 * std::f64::consts::TAU + self.t as f64 * 0.4;
            let (x, z) = (c[0] + a.cos() * ZONE, c[2] + a.sin() * ZONE);
            let y = self.field.height(x, z).unwrap_or(c[1]);
            out.shape_w(Sh::Disc, [x, y + 3.0, z], [0.36, 0.05, 0.36], 0xffffff);
            out.shape_w(Sh::Disc, [x, y + 6.0, z], [0.27, 0.05, 0.27], col);
        }
        out.shape_w(Sh::Disc, [c[0], c[1] + 2.0, c[2]], [0.62, 0.04, 0.62], 0xffffff);
        out.shape_w(Sh::Disc, [c[0], c[1] + 5.0, c[2]], [0.5, 0.04, 0.5], col);
        // A crown over the holder.
        out.crown = self.king;
        for p in self.r.ids() {
            out.tags[p] = if self.fallen[p].is_some() { "FELL".into() } else { format!("{:.1}", self.score[p]) };
        }
        if let Some(k) = self.king.filter(|_| self.contested) {
            out.tags[k] = format!("{:.1} (1/2)", self.score[k]);
        }
        if self.t < 1.2 {
            out.banner = if self.roam { "THE CROWN ROAMS!".into() } else { "HOLD THE CROWN!".into() };
        } else if self.time_left() < 5.0 {
            out.banner = format!("{}", self.time_left().ceil() as i32);
        }
    }
}
