//! Pound Grid: one pad of the 3x3 floor lights up at a time. Be the first to pound it; a wrong pad costs a point.
use crate::experiments::party::common::*;

const LIMIT: f32 = 16.0;

pub struct PoundGrid {
    r: Roster,
    t: f32,
    lit: Option<usize>,
    lit_t: f32,
    gap: f32,
    score: [i32; 4],
    lock: [f32; 4],
    rng: Rng,
    brng: Rng,
    plan: [Option<(usize, f32)>; 4],
    flash: Option<(usize, usize, f32)>,
}

impl PoundGrid {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        PoundGrid { r: Roster::new(active), t: 0.0, lit: None, lit_t: 0.0, gap: 0.8, score: [0; 4], lock: [0.0; 4], rng: Rng::new(seed), brng: Rng::new(seed ^ 0x90d), plan: [None; 4], flash: None }
    }
    fn window(&self) -> f32 {
        (1.5 - self.t * 0.04).max(0.8)
    }
}

impl MiniGame for PoundGrid {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        if let Some(f) = &mut self.flash {
            f.2 -= dt;
            if f.2 <= 0.0 {
                self.flash = None;
            }
        }
        for l in &mut self.lock {
            *l = (*l - dt).max(0.0);
        }
        match self.lit {
            None => {
                self.gap -= dt;
                if self.gap <= 0.0 {
                    self.lit = Some(self.rng.below(9));
                    self.lit_t = 0.0;
                    self.plan = [None; 4];
                }
            }
            Some(cell) => {
                self.lit_t += dt;
                let mut winner = None;
                for p in self.r.ids().collect::<Vec<_>>() {
                    if self.lock[p] > 0.0 {
                        continue;
                    }
                    if let Some(k) = inp[p].pick {
                        if k == cell {
                            winner = Some(p);
                            break;
                        } else {
                            self.score[p] -= 1;
                            self.lock[p] = 0.6;
                        }
                    }
                }
                if let Some(p) = winner {
                    self.score[p] += 1;
                    self.flash = Some((cell, p, 0.4));
                    self.lit = None;
                    self.gap = self.rng.range(0.25, 0.7);
                } else if self.lit_t > self.window() {
                    self.lit = None;
                    self.gap = 0.4;
                }
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        let Some(cell) = self.lit else { return PadIn::default() };
        if self.lock[p] > 0.0 {
            return PadIn::default();
        }
        if self.plan[p].is_none() {
            let delay = lvl.react() * self.brng.range(1.0, 2.2) + 0.15;
            let right = self.brng.chance(0.55 + 0.45 * lvl.skill());
            let k = if right { cell } else { (cell + 1 + self.brng.below(8)) % 9 };
            self.plan[p] = Some((k, delay));
        }
        let (k, delay) = self.plan[p].unwrap();
        if self.lit_t >= delay {
            self.plan[p] = Some((usize::MAX, f32::MAX));
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
        for k in 0..9 {
            let (c, r) = (k % 3, k / 3);
            let lit = self.lit == Some(k);
            let col = if lit {
                0xffd23a
            } else if let Some((fc, fp, _)) = self.flash {
                if fc == k {
                    PLAYER_COLORS[fp]
                } else {
                    0x4b5fa8
                }
            } else {
                0x4b5fa8
            };
            out.tile(k, [0.31 + c as f32 * 0.125, 0.12 + r as f32 * 0.27, 0.115, 0.25], col, if lit { "!" } else { "" });
        }
        out.banner = String::new();
        for p in self.r.ids() {
            out.avatar(p, -7.2 + if p % 2 == 0 { 0.0 } else { 14.4 }, -2.0 + 3.6 * (p / 2) as f32, 0.0, if p % 2 == 0 { 1.57 } else { -1.57 }, false);
            out.tags[p] = format!("{}", self.score[p]);
        }
    }
}
