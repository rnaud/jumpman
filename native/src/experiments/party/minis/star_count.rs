//! Star Count: stars flit across the sky for a moment. How many were there? Tap the right number, fast.
use crate::experiments::party::common::*;

const ROUNDS: usize = 5;
const SHOW: f32 = 1.8;
const ANSWER: f32 = 4.0;

pub struct StarCount {
    r: Roster,
    t: f32,
    round: usize,
    rt: f32,
    count: usize,
    stars: Vec<(f32, f32, f32, f32)>,
    options: [usize; 4],
    answered: [Option<(usize, f32)>; 4],
    points: [f32; 4],
    gap: f32,
    rng: Rng,
    brng: Rng,
    plan: [Option<(usize, f32)>; 4],
    over: bool,
}

impl StarCount {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut g = StarCount { r: Roster::new(active), t: 0.0, round: 0, rt: 0.0, count: 0, stars: vec![], options: [0; 4], answered: [None; 4], points: [0.0; 4], gap: 0.0, rng: Rng::new(seed), brng: Rng::new(seed ^ 0x57a), plan: [None; 4], over: false };
        g.start_round();
        g
    }
    fn start_round(&mut self) {
        self.round += 1;
        self.rt = 0.0;
        self.count = 3 + self.rng.below(5) + self.round / 2;
        self.stars = (0..self.count).map(|_| (self.rng.range(-7.5, 7.5), self.rng.range(0.6, 3.6), self.rng.range(-0.9, 0.9), self.rng.range(0.0, 6.28))).collect();
        let mut opts = [self.count - 1, self.count, self.count + 1, self.count + 2];
        self.rng.shuffle(&mut opts);
        self.options = opts;
        self.answered = [None; 4];
        self.plan = [None; 4];
        self.gap = 0.0;
    }
    fn answering(&self) -> bool {
        self.rt >= SHOW && self.gap <= 0.0
    }
}

impl MiniGame for StarCount {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        if self.over {
            return;
        }
        if self.gap > 0.0 {
            self.gap -= dt;
            if self.gap <= 0.0 {
                if self.round >= ROUNDS {
                    self.over = true;
                } else {
                    self.start_round();
                }
            }
            return;
        }
        self.rt += dt;
        if self.answering() {
            for p in self.r.ids().collect::<Vec<_>>() {
                if self.answered[p].is_some() {
                    continue;
                }
                if let Some(k) = inp[p].pick.filter(|&k| k < 4) {
                    let n = self.options[k];
                    self.answered[p] = Some((n, self.rt - SHOW));
                    if n == self.count {
                        // 2 points, +1 for answering within 1.5 s.
                        self.points[p] += 2.0 + if self.rt - SHOW < 1.5 { 1.0 } else { 0.0 } + (ANSWER - (self.rt - SHOW)).max(0.0) * 0.01;
                    }
                }
            }
            let all = self.r.ids().all(|p| self.answered[p].is_some());
            if all || self.rt > SHOW + ANSWER {
                self.gap = 1.6;
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if !self.answering() || self.answered[p].is_some() {
            return PadIn::default();
        }
        if self.plan[p].is_none() {
            let right = self.brng.chance(0.3 + 0.65 * lvl.skill());
            let n = if right { self.count } else { [self.count - 1, self.count + 1, self.count + 2][self.brng.below(3)] };
            let k = self.options.iter().position(|&o| o == n).unwrap_or(0);
            self.plan[p] = Some((k, lvl.react() * self.brng.range(1.5, 4.0)));
        }
        let (k, d) = self.plan[p].unwrap();
        if self.rt - SHOW >= d {
            self.plan[p] = Some((0, f32::MAX));
            return PadIn { pick: Some(k), ..Default::default() };
        }
        PadIn::default()
    }
    fn done(&self) -> bool {
        self.over || self.t > 60.0
    }
    fn time_left(&self) -> f32 {
        (ROUNDS as f32 * (SHOW + ANSWER + 1.6) - self.t).max(0.0)
    }
    fn scores(&self) -> [f32; 4] {
        self.r.mask(self.points)
    }
    fn scene(&self, out: &mut Stage) {
        if self.rt < SHOW && self.gap <= 0.0 {
            for &(x, y, z, ph) in &self.stars {
                let u = self.rt / SHOW;
                // Each star drifts in a loop and blinks.
                let bx = x + (ph + u * 4.0).sin() * 1.1;
                let by = y + (ph * 2.0 + u * 5.0).cos() * 0.5;
                out.shape(Sh::Ball, [bx, by, z], [0.3, 0.3, 0.3], 0xffe14a);
            }
            out.banner = "Count the stars!".into();
        } else if self.gap > 0.0 {
            out.banner = format!("It was {}!", self.count);
        } else {
            out.banner = "How many?".into();
            for (k, n) in self.options.iter().enumerate() {
                out.tile(k, [0.1 + 0.2 * k as f32, 0.45, 0.17, 0.32], 0x4560c8, format!("{n}"));
            }
        }
        for p in self.r.ids() {
            out.avatar(p, -6.0 + 4.0 * p as f32, 3.0, 0.0, 3.14, false);
            out.tags[p] = format!("{:.0}", self.points[p].floor());
        }
    }
}
