//! Star Tag: one Power Star on the floating island. Touch it to take it; it scores every second you hold it, but its
//! holder runs a little slower and anyone who punches (or dives into) the holder takes it. Fall off the island and the
//! star drops back in the middle. 35 seconds.
use super::*;

pub const SECS: f32 = 35.0;
const REACH: f64 = 165.0;
const GRAB: f64 = 120.0;
const POWER: f64 = 30.0;
/// The holder can't lose the star again this soon after taking it.
const GUARD: f32 = 1.1;
const CARRY: f32 = 1.4;
/// Speed of the holder.
pub const HOLDER_SPEED: f64 = 0.84;

pub struct StarTag {
    r: Roster,
    rng: Rng,
    field: Arc<Field>,
    score: [f32; 4],
    t: f32,
    pub holder: Option<usize>,
    /// Where the star sits when nobody holds it.
    pub loose: [f64; 3],
    guard: f32,
    fallen: [Option<f32>; 4],
    pub steals: [u32; 4],
    think: [f32; 4],
    flee: [Option<[f64; 3]>; 4],
    evs: Vec<MiniEv>,
}

impl StarTag {
    pub fn new(seed: u64, active: [bool; 4], field: Arc<Field>) -> Self {
        // The star first appears somewhere off the middle, so one Mario is a step closer to it.
        let mut rng = Rng::new(seed);
        let a = rng.f() as f64 * std::f64::consts::TAU;
        let c = field.center;
        let loose = field.nearest([c[0] + a.cos() * field.radius * 0.25, c[1], c[2] + a.sin() * field.radius * 0.25]);
        StarTag { r: Roster::new(active), rng, field, score: [0.0; 4], t: 0.0, holder: None, loose, guard: 0.0, fallen: [None; 4], steals: [0; 4], think: [0.0; 4], flee: [None; 4], evs: vec![] }
    }
}

impl MiniGame for StarTag {
    fn start(&self, p: usize) -> [f64; 3] {
        self.field.start(p, 0.55)
    }

    fn step(&mut self, dt: f32, bodies: &[Body; 4], _drives: &[Drive; 4]) {
        if self.done() {
            return;
        }
        self.t += dt;
        self.guard = (self.guard - dt).max(0.0);
        for p in self.r.ids() {
            match self.fallen[p] {
                Some(t0) if self.t - t0 > CARRY => {
                    self.fallen[p] = None;
                    self.evs.push(MiniEv::Place { p, pos: self.field.start(p, 0.55) });
                }
                Some(_) => {}
                None => {
                    let b = bodies[p].pos;
                    if self.field.fallen(b) || hdist(b, self.field.center) > self.field.radius + 150.0 {
                        self.fallen[p] = Some(self.t);
                        self.evs.push(MiniEv::Cue { kind: "bonk", pos: b });
                        if self.holder == Some(p) {
                            self.holder = None;
                            self.loose = self.field.center;
                        }
                    }
                }
            }
        }
        match self.holder {
            None => {
                let s = self.loose;
                let near = seat_order(self.t).into_iter().filter(|&p| self.r.active[p] && self.fallen[p].is_none() && hdist(bodies[p].pos, s) < GRAB && (bodies[p].pos[1] - s[1]).abs() < 220.0);
                if let Some(p) = near.min_by(|a, b| hdist(bodies[*a].pos, s).partial_cmp(&hdist(bodies[*b].pos, s)).unwrap()) {
                    self.holder = Some(p);
                    self.guard = GUARD;
                    self.evs.push(MiniEv::Cue { kind: "star", pos: s });
                }
            }
            Some(h) => {
                self.score[h] += dt;
                if self.guard <= 0.0 {
                    let thief = seat_order(self.t).into_iter().find(|&a| a != h && self.r.active[a] && self.fallen[a].is_none() && hits(&bodies[a], &bodies[h], REACH));
                    if let Some(a) = thief {
                        self.holder = Some(a);
                        self.guard = GUARD;
                        self.steals[a] += 1;
                        self.evs.push(MiniEv::Knock { p: h, from: bodies[a].pos, power: POWER });
                        self.evs.push(MiniEv::Cue { kind: "star", pos: bodies[h].pos });
                    }
                }
            }
        }
    }

    fn bot(&mut self, p: usize, lvl: Level, bodies: &[Body; 4]) -> Drive {
        if self.fallen[p].is_some() {
            return Drive::default();
        }
        let me = bodies[p];
        let skill = lvl.skill() as f64;
        match self.holder {
            None => {
                let mut d = run_to(me.pos, self.loose, 0.0);
                d.mag = d.mag.max(0.7) * (0.8 + 0.2 * skill);
                d
            }
            Some(h) if h == p => {
                // Run for the open spot farthest from the chasers, staying away from the edge.
                self.think[p] -= DT;
                if self.think[p] <= 0.0 || self.flee[p].is_none_or(|f| hdist(f, me.pos) < 120.0) {
                    self.think[p] = lvl.react() * 0.8;
                    let mut best = (f64::MIN, None);
                    for k in 0..16 {
                        let a = k as f64 / 16.0 * std::f64::consts::TAU;
                        let r = 420.0;
                        let (x, z) = (me.pos[0] + a.cos() * r, me.pos[2] + a.sin() * r);
                        let Some(y) = self.field.height(x, z) else { continue };
                        let spot = [x, y, z];
                        let near = self.r.ids().filter(|&q| q != p && self.fallen[q].is_none()).map(|q| hdist(bodies[q].pos, spot)).fold(f64::MAX, f64::min);
                        let edge = (hdist(spot, self.field.center) / self.field.radius - 0.7).max(0.0) * 1800.0;
                        let v = near - edge + self.rng.f() as f64 * (1.0 - skill) * 400.0;
                        if v > best.0 {
                            best = (v, Some(spot));
                        }
                    }
                    self.flee[p] = best.1;
                }
                let to = self.flee[p].unwrap_or(self.field.center);
                let mut d = run_to(me.pos, to, 0.0);
                // A sharper holder keeps running flat out; a sloppy one dawdles (Easy 0.85, Hard 0.98 of a full stick).
                d.mag = 0.7 + 0.3 * skill;
                d
            }
            Some(h) => {
                let them = bodies[h];
                // Hard bots lead the holder a little.
                let lead = 6.0 * skill;
                let f = if them.grounded { yaw_dir(them.yaw) } else { [0.0, 0.0] };
                let aim = [them.pos[0] + f[0] * lead * 20.0, them.pos[1], them.pos[2] + f[1] * lead * 20.0];
                let mut d = run_to(me.pos, aim, 0.0);
                d.mag = 1.0;
                let dist = hdist(me.pos, them.pos);
                if dist < REACH * (0.7 + 0.25 * skill) && facing(&me, them.pos, 0.45) && self.guard <= 0.0 && self.rng.chance(0.2 + 0.5 * skill as f32) {
                    d.punch = true;
                }
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

    fn speed(&self, p: usize) -> f64 {
        if self.holder == Some(p) {
            HOLDER_SPEED
        } else {
            1.0
        }
    }

    fn scene(&self, out: &mut Stage) {
        match self.holder {
            Some(h) => out.star_on = Some(h),
            None => out.star_at = Some([self.loose[0], self.loose[1] + 90.0 + 20.0 * (self.t as f64 * 3.0).sin(), self.loose[2]]),
        }
        for p in self.r.ids() {
            out.tags[p] = if self.fallen[p].is_some() { "FELL".into() } else { format!("{:.1}", self.score[p]) };
        }
        if self.t < 1.2 {
            out.banner = "GRAB THE STAR!".into();
        } else if self.time_left() < 5.0 {
            out.banner = format!("{}", self.time_left().ceil() as i32);
        }
    }
}
