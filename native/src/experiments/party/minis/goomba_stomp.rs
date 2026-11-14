//! Goomba Stomp: Goombas keep wandering onto the meadow and chase whoever is near. Land on one for a point, and bounce
//! straight onto another for two; walk into one and it knocks you back and costs a point. 30 seconds.
use super::*;

pub const SECS: f32 = 30.0;
const ON_FIELD: usize = 7;
const WALK: f64 = 7.0;
const CHASE: f64 = 520.0;
const POWER: f64 = 24.0;

#[derive(Clone, Debug)]
pub struct Goomba {
    pub pos: [f64; 3],
    pub dir: [f64; 2],
    /// Seconds since it appeared.
    pub age: f32,
    /// Squashed / sent flying: removed after a moment.
    pub dead: Option<f32>,
    goal: [f64; 3],
}

pub struct GoombaStomp {
    r: Roster,
    rng: Rng,
    field: Arc<Field>,
    pub goombas: Vec<Goomba>,
    score: [f32; 4],
    pub stomps: [u32; 4],
    t: f32,
    safe: [f32; 4],
    aim: [Option<usize>; 4],
    chain: [u32; 4],
    hop: [f64; 4],
    think: [f32; 4],
    evs: Vec<MiniEv>,
}

impl GoombaStomp {
    pub fn new(seed: u64, active: [bool; 4], field: Arc<Field>) -> Self {
        let mut g = GoombaStomp { r: Roster::new(active), rng: Rng::new(seed), field, goombas: vec![], score: [0.0; 4], stomps: [0; 4], t: 0.0, safe: [0.0; 4], aim: [None; 4], chain: [0; 4], hop: [0.0; 4], think: [0.0; 4], evs: vec![] };
        for _ in 0..ON_FIELD {
            let starts: Vec<[f64; 3]> = (0..4).map(|p| g.field.start(p, 0.2)).collect();
            let m = g.fresh(&starts);
            g.goombas.push(m);
        }
        g
    }

    fn fresh(&mut self, away: &[[f64; 3]]) -> Goomba {
        let mut pos = self.field.rand_point(&mut self.rng);
        for _ in 0..8 {
            if away.iter().all(|a| hdist(*a, pos) > 380.0) {
                break;
            }
            pos = self.field.rand_point(&mut self.rng);
        }
        let goal = self.field.rand_point(&mut self.rng);
        Goomba { pos, dir: toward(pos, goal).unwrap_or([1.0, 0.0]), age: 0.0, dead: None, goal }
    }

    /// A falling Mario over a Goomba's head.
    pub fn stomping(b: &Body, g: &Goomba) -> bool {
        !b.grounded && b.vy < -4.0 && hdist(b.pos, g.pos) < 105.0 && b.pos[1] > g.pos[1] + 15.0 && b.pos[1] < g.pos[1] + 190.0
    }
}

impl MiniGame for GoombaStomp {
    fn start(&self, p: usize) -> [f64; 3] {
        self.field.start(p, 0.2)
    }

    fn step(&mut self, dt: f32, bodies: &[Body; 4], _drives: &[Drive; 4]) {
        if self.done() {
            return;
        }
        self.t += dt;
        for s in &mut self.safe {
            *s = (*s - dt).max(0.0);
        }
        // Walk: chase the nearest Mario in range, else wander to a goal.
        let ids: Vec<usize> = self.r.ids().collect();
        for g in &mut self.goombas {
            g.age += dt;
            if let Some(d) = &mut g.dead {
                *d += dt;
                continue;
            }
            let near = ids.iter().map(|&p| bodies[p].pos).filter(|b| hdist(*b, g.pos) < CHASE).min_by(|a, b| hdist(*a, g.pos).partial_cmp(&hdist(*b, g.pos)).unwrap());
            let to = near.unwrap_or(g.goal);
            if near.is_none() && hdist(g.pos, g.goal) < 80.0 {
                g.goal = self.field.rand_point(&mut self.rng);
            }
            if let Some(d) = toward(g.pos, to) {
                // Turn gradually.
                g.dir = [g.dir[0] * 0.85 + d[0] * 0.15, g.dir[1] * 0.85 + d[1] * 0.15];
                let l = g.dir[0].hypot(g.dir[1]).max(1e-6);
                g.dir = [g.dir[0] / l, g.dir[1] / l];
            }
            let speed = if near.is_some() { WALK * 1.3 } else { WALK };
            let (nx, nz) = (g.pos[0] + g.dir[0] * speed, g.pos[2] + g.dir[1] * speed);
            match self.field.height(nx, nz) {
                Some(h) if (h - g.pos[1]).abs() < 90.0 && (nx - self.field.center[0]).hypot(nz - self.field.center[2]) < self.field.radius * 0.92 => g.pos = [nx, h, nz],
                _ => {
                    g.goal = self.field.rand_point(&mut self.rng);
                    g.dir = [-g.dir[0], -g.dir[1]];
                }
            }
        }
        // Stomps, punches, bumps (ties go round the seats).
        for p in seat_order(self.t).into_iter().filter(|&p| self.r.active[p]) {
            let b = bodies[p];
            if b.grounded {
                self.chain[p] = 0;
            }
            for i in 0..self.goombas.len() {
                let g = self.goombas[i].clone();
                let g = &g;
                if g.dead.is_some() || g.age < 0.5 {
                    continue;
                }
                if Self::stomping(&b, g) {
                    // A stomp chained before landing (bounce to the next one) is worth double.
                    let chain = self.chain[p] > 0;
                    self.score[p] += if chain { 2.0 } else { 1.0 };
                    self.chain[p] += 1;
                    self.stomps[p] += 1;
                    self.goombas[i].dead = Some(0.0);
                    self.evs.push(MiniEv::Bounce { p });
                    self.evs.push(MiniEv::Cue { kind: "stomp", pos: g.pos });
                } else if b.grounded && self.safe[p] <= 0.0 && hdist(b.pos, g.pos) < 85.0 && (b.pos[1] - g.pos[1]).abs() < 100.0 {
                    self.score[p] = (self.score[p] - 1.0).max(0.0);
                    self.safe[p] = 1.2;
                    self.evs.push(MiniEv::Knock { p, from: g.pos, power: POWER });
                    self.evs.push(MiniEv::Cue { kind: "hurt", pos: b.pos });
                }
            }
        }
        // Replace the fallen.
        let away: Vec<[f64; 3]> = ids.iter().map(|&p| bodies[p].pos).collect();
        for i in 0..self.goombas.len() {
            if self.goombas[i].dead.is_some_and(|d| d > 0.6) {
                self.goombas[i] = self.fresh(&away);
            }
        }
    }

    fn bot(&mut self, p: usize, lvl: Level, bodies: &[Body; 4]) -> Drive {
        let me = bodies[p];
        let skill = lvl.skill() as f64;
        self.think[p] -= DT;
        let gone = self.aim[p].is_none_or(|i| self.goombas.get(i).is_none_or(|g| g.dead.is_some()));
        // A target gone: Hard bots pick the next one at once (and chain stomps), others need a moment.
        if gone && skill < 0.9 && self.think[p] > lvl.react() * 0.5 {
            self.think[p] = self.think[p].min(lvl.react());
            self.aim[p] = None;
        }
        if self.think[p] <= 0.0 || (gone && skill >= 0.9) {
            self.think[p] = lvl.react() + self.rng.f() * 0.2;
            // How well the next hop is timed: Hard bots are within a few steps, Easy ones often jump far too soon or late.
            self.hop[p] = (self.rng.f() as f64 - 0.5) * 2.0 * (1.0 - skill) * 300.0;
            self.aim[p] = self.goombas.iter().enumerate().filter(|(_, g)| g.dead.is_none() && g.age > 0.3).min_by(|a, b| {
                let da = hdist(a.1.pos, me.pos) * (1.0 + self.rng.f() as f64 * (1.0 - skill));
                let db = hdist(b.1.pos, me.pos) * (1.0 + self.rng.f() as f64 * (1.0 - skill));
                da.partial_cmp(&db).unwrap()
            }).map(|(i, _)| i);
        }
        let Some(g) = self.aim[p].and_then(|i| self.goombas.get(i)) else { return Drive::default() };
        let d = hdist(me.pos, g.pos);
        if d > 320.0 || !me.grounded {
            // In the air: keep drifting over it (a chained stomp).
            let mut r = run_to(me.pos, g.pos, 0.0);
            r.mag *= 0.7 + 0.3 * skill;
            return r;
        }
        // Close in slowly, or wait if it is coming (it walks in under the fall), and hop at the right distance.
        let coming = toward(g.pos, me.pos).is_some_and(|to_me| to_me[0] * g.dir[0] + to_me[1] * g.dir[1] > 0.7);
        let mut drive = Drive { dir: toward(me.pos, g.pos), mag: if coming { 0.0 } else { 0.4 }, ..Default::default() };
        let hop_at = if coming { 120.0 } else { 175.0 } + self.hop[p];
        if d < hop_at.max(40.0) {
            drive.jump = true;
            drive.mag = if coming { 0.15 } else { 0.4 };
        }
        drive
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

    fn scene(&self, out: &mut Stage) {
        for g in &self.goombas {
            let squash = g.dead.map_or(1.0, |d| (1.0 - d * 1.6).max(0.15));
            let pop = (g.age / 0.5).min(1.0);
            out.goombas.push(([g.pos[0], g.pos[1], g.pos[2]], g.dir[0].atan2(g.dir[1]) as f32, squash * pop));
        }
        for p in self.r.ids() {
            out.tags[p] = format!("{}", self.score[p] as i32);
        }
        if self.t < 1.2 {
            out.banner = "STOMP THE GOOMBAS!".into();
        } else if self.time_left() < 5.0 {
            out.banner = format!("{}", self.time_left().ceil() as i32);
        }
    }
}
