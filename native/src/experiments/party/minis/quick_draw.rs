//! Quick Draw: wait for the flag to drop, then tap. Tap early and you lose the round. Three rounds.
use crate::experiments::party::common::*;

const ROUNDS: usize = 3;

pub struct QuickDraw {
    r: Roster,
    t: f32,
    round: usize,
    /// Time in the round when the flag drops.
    go_at: f32,
    rt: f32,
    /// Reaction time per player this round (None = not yet), `false` start flagged.
    got: [Option<f32>; 4],
    foul: [bool; 4],
    points: [u32; 4],
    total_rt: [f32; 4],
    gap: f32,
    rng: Rng,
    brng: Rng,
    plan: [Option<f32>; 4],
    over: bool,
}

impl QuickDraw {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut g = QuickDraw { r: Roster::new(active), t: 0.0, round: 0, go_at: 0.0, rt: 0.0, got: [None; 4], foul: [false; 4], points: [0; 4], total_rt: [0.0; 4], gap: 0.0, rng: Rng::new(seed), brng: Rng::new(seed ^ 0x9d), plan: [None; 4], over: false };
        g.start_round();
        g
    }
    fn start_round(&mut self) {
        self.round += 1;
        self.rt = 0.0;
        self.go_at = self.rng.range(1.6, 4.0);
        self.got = [None; 4];
        self.foul = [false; 4];
        self.plan = [None; 4];
        self.gap = 0.0;
    }
    fn flag_up(&self) -> bool {
        self.rt >= self.go_at
    }
    fn settle(&mut self) {
        // Award 3/2/1 by speed to those without a foul.
        let mut order: Vec<usize> = self.r.ids().filter(|&p| self.got[p].is_some() && !self.foul[p]).collect();
        order.sort_by(|&a, &b| self.got[a].partial_cmp(&self.got[b]).unwrap());
        for (i, p) in order.into_iter().enumerate() {
            self.points[p] += [3, 2, 1, 0][i.min(3)];
            self.total_rt[p] += self.got[p].unwrap();
        }
    }
}

impl MiniGame for QuickDraw {
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
        for p in self.r.ids().collect::<Vec<_>>() {
            if self.foul[p] || self.got[p].is_some() || !inp[p].act {
                continue;
            }
            if self.flag_up() {
                // Sub-step jitter so same-step taps never resolve by seat order.
                self.got[p] = Some(self.rt - self.go_at + self.rng.f() / 30.0);
            } else {
                self.foul[p] = true;
            }
        }
        let all_in = self.r.ids().all(|p| self.foul[p] || self.got[p].is_some());
        if all_in || self.rt > self.go_at + 1.8 {
            self.settle();
            self.gap = 1.4;
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if self.gap > 0.0 || self.foul[p] || self.got[p].is_some() {
            return PadIn::default();
        }
        if self.plan[p].is_none() {
            // Hard bots occasionally jump the gun; Easy ones are just slow.
            let jump = self.brng.chance(0.03 + 0.07 * lvl.skill());
            let at = if jump { self.go_at - self.brng.range(0.05, 0.4) } else { self.go_at + lvl.react() * self.brng.range(0.7, 1.5) };
            self.plan[p] = Some(at);
        }
        if self.rt >= self.plan[p].unwrap() {
            self.plan[p] = Some(f32::MAX);
            return PadIn { act: true, ..Default::default() };
        }
        PadIn::default()
    }
    fn done(&self) -> bool {
        self.over || self.t > 40.0
    }
    fn time_left(&self) -> f32 {
        (40.0 - self.t).max(0.0)
    }
    fn scores(&self) -> [f32; 4] {
        let mut s = [0.0; 4];
        for p in 0..4 {
            s[p] = self.points[p] as f32 * 10.0 - self.total_rt[p];
        }
        self.r.mask(s)
    }
    fn scene(&self, out: &mut Scene) {
        let up = self.flag_up() && self.gap <= 0.0;
        out.shape(Sh::Disc, [0.0, 1.8, 0.0], [0.07, 3.6, 0.0], 0xd8d0c0);
        out.shape(Sh::Box, [0.6, if up { 3.2 } else { 1.2 }, 0.0], [1.2, 0.8, 0.06], if up { 0x43d65a } else { 0xe5342a });
        out.banner = if self.gap > 0.0 {
            "".into()
        } else if up {
            "TAP NOW!".into()
        } else {
            "Wait for it...".into()
        };
        for p in self.r.ids() {
            out.avatar(p, -6.0 + 4.0 * p as f32, 2.4, 0.0, 3.14, false);
            out.tags[p] = if self.foul[p] {
                "EARLY".into()
            } else if let Some(g) = self.got[p] {
                format!("{}ms", (g * 1000.0) as u32)
            } else {
                format!("{}pt", self.points[p])
            };
        }
    }
}
