//! Odd One Out: a grid of look-alike tiles hides one that differs. First to tap it scores; wrong taps cost.
use crate::experiments::party::common::*;

const LIMIT: f32 = 24.0;
const PAIRS: [(&str, &str); 8] = [("6", "9"), ("E", "F"), ("O", "Q"), ("M", "N"), ("b", "d"), ("2", "5"), ("R", "P"), ("C", "G")];

pub struct OddOneOut {
    r: Roster,
    t: f32,
    round: usize,
    odd: usize,
    pair: usize,
    score: [i32; 4],
    lock: [f32; 4],
    rng: Rng,
    brng: Rng,
    round_t: f32,
    plan: [Option<(usize, f32)>; 4],
    flip: bool,
    pause: f32,
}

impl OddOneOut {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut g = OddOneOut { r: Roster::new(active), t: 0.0, round: 0, odd: 0, pair: 0, score: [0; 4], lock: [0.0; 4], rng: Rng::new(seed), brng: Rng::new(seed ^ 0x0dd), round_t: 0.0, plan: [None; 4], flip: false, pause: 0.0 };
        g.next_round();
        g
    }
    fn next_round(&mut self) {
        self.round += 1;
        self.odd = self.rng.below(12);
        self.pair = self.rng.below(PAIRS.len());
        self.flip = self.rng.chance(0.5);
        self.round_t = 0.0;
        self.plan = [None; 4];
        self.pause = 0.0;
    }
}

impl MiniGame for OddOneOut {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        self.round_t += dt;
        for l in &mut self.lock {
            *l = (*l - dt).max(0.0);
        }
        if self.pause > 0.0 {
            self.pause -= dt;
            if self.pause <= 0.0 {
                self.next_round();
            }
            return;
        }
        for p in self.r.ids().collect::<Vec<_>>() {
            if self.lock[p] > 0.0 {
                continue;
            }
            if let Some(k) = inp[p].pick {
                if k == self.odd {
                    self.score[p] += 1;
                    self.pause = 0.45;
                    break;
                }
                self.score[p] -= 1;
                self.lock[p] = 0.7;
            }
        }
        if self.pause <= 0.0 && self.round_t > 5.0 {
            self.next_round();
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if self.pause > 0.0 || self.lock[p] > 0.0 {
            return PadIn::default();
        }
        if self.plan[p].is_none() {
            // Searching takes a while; skill sets both the speed and the chance to spot it.
            let delay = lvl.react() * self.brng.range(2.0, 4.5) + 0.3;
            let right = self.brng.chance(0.4 + 0.58 * lvl.skill());
            let k = if right { self.odd } else { (self.odd + 1 + self.brng.below(11)) % 12 };
            self.plan[p] = Some((k, delay));
        }
        let (k, d) = self.plan[p].unwrap();
        if self.round_t >= d {
            self.plan[p] = Some((0, f32::MAX));
            return PadIn { pick: Some(k), ..Default::default() };
        }
        PadIn::default()
    }
    fn done(&self) -> bool {
        self.t >= LIMIT
    }
    fn time_left(&self) -> f32 {
        (LIMIT - self.t).max(0.0)
    }
    fn scores(&self) -> [f32; 4] {
        let mut s = [0.0; 4];
        for p in 0..4 {
            s[p] = self.score[p] as f32;
        }
        self.r.mask(s)
    }
    fn scene(&self, out: &mut Scene) {
        let (a, b) = PAIRS[self.pair];
        let (common, odd) = if self.flip { (b, a) } else { (a, b) };
        for k in 0..12 {
            let (c, r) = (k % 4, k / 4);
            let glyph = if k == self.odd { odd } else { common };
            let col = if self.pause > 0.0 && k == self.odd { 0x6fd36b } else { 0x5560b8 };
            out.tile(k, [0.22 + c as f32 * 0.14, 0.1 + r as f32 * 0.29, 0.125, 0.26], col, glyph);
        }
        for p in self.r.ids() {
            out.avatar(p, -7.2 + if p % 2 == 0 { 0.0 } else { 14.4 }, -2.0 + 3.6 * (p / 2) as f32, 0.0, if p % 2 == 0 { 1.57 } else { -1.57 }, false);
            out.tags[p] = format!("{}", self.score[p]);
        }
    }
}
