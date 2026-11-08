//! Echo Pads: watch the pads light up, then repeat the order. One slip and you are out of the round.
use crate::experiments::party::common::*;

const LIMIT: f32 = 40.0;
const SHOW: f32 = 0.55;
const COLS: [u32; 4] = [0xe8473a, 0x3d7bff, 0x34b34a, 0xf2c230];

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Watch,
    Repeat,
    Gap,
}

pub struct EchoPads {
    r: Roster,
    t: f32,
    seq: Vec<usize>,
    round: usize,
    phase: Phase,
    ph_t: f32,
    pos: [usize; 4],
    alive: [bool; 4],
    best: [usize; 4],
    brng: Rng,
    /// Next time each bot may tap.
    wait: [f32; 4],
    flash: Option<usize>,
}

impl EchoPads {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut rng = Rng::new(seed);
        let seq: Vec<usize> = (0..16).map(|_| rng.below(4)).collect();
        EchoPads { r: Roster::new(active), t: 0.0, seq, round: 2, phase: Phase::Gap, ph_t: 0.0, pos: [0; 4], alive: active, best: [0; 4], brng: Rng::new(seed ^ 0xec40), wait: [0.0; 4], flash: None }
    }
    fn len(&self) -> usize {
        self.round
    }
    fn end_round(&mut self) {
        self.round += 1;
        self.phase = Phase::Gap;
        self.ph_t = 0.0;
        self.pos = [0; 4];
    }
}

impl MiniGame for EchoPads {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        self.ph_t += dt;
        self.flash = None;
        match self.phase {
            Phase::Gap => {
                if self.ph_t > 0.9 {
                    self.phase = Phase::Watch;
                    self.ph_t = 0.0;
                }
            }
            Phase::Watch => {
                let i = (self.ph_t / SHOW) as usize;
                if i >= self.len() {
                    self.phase = Phase::Repeat;
                    self.ph_t = 0.0;
                    for w in &mut self.wait {
                        *w = 0.0;
                    }
                } else if self.ph_t - i as f32 * SHOW < SHOW * 0.7 {
                    self.flash = Some(self.seq[i]);
                }
            }
            Phase::Repeat => {
                for p in self.r.ids().collect::<Vec<_>>() {
                    if !self.alive[p] || self.pos[p] >= self.len() {
                        continue;
                    }
                    if let Some(k) = inp[p].pick {
                        if k == self.seq[self.pos[p]] {
                            self.pos[p] += 1;
                            if self.pos[p] == self.len() {
                                self.best[p] = self.len();
                            }
                        } else {
                            self.alive[p] = false;
                        }
                    }
                }
                let waiting = self.r.ids().any(|p| self.alive[p] && self.pos[p] < self.len());
                if !waiting || self.ph_t > 4.0 + self.len() as f32 * 0.9 {
                    // Anyone still unfinished timed out.
                    for p in self.r.ids().collect::<Vec<_>>() {
                        if self.pos[p] < self.len() {
                            self.alive[p] = false;
                        }
                    }
                    if self.r.ids().any(|p| self.alive[p]) && self.round < 14 {
                        self.end_round();
                    } else {
                        self.t = LIMIT;
                    }
                }
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if self.phase != Phase::Repeat || !self.alive[p] || self.pos[p] >= self.len() {
            return PadIn::default();
        }
        if self.ph_t < self.wait[p] {
            return PadIn::default();
        }
        // Delay between taps; skill is the chance of remembering each pad (worse for longer runs).
        self.wait[p] = self.ph_t + lvl.react() * 1.2 + self.brng.range(0.0, 0.3);
        let forget = (1.0 - lvl.skill()) * 0.28 + self.len() as f32 * (1.0 - lvl.skill()) * 0.03;
        let truth = self.seq[self.pos[p]];
        let k = if self.brng.chance(forget) { (truth + 1 + self.brng.below(3)) % 4 } else { truth };
        PadIn { pick: Some(k), ..Default::default() }
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
            s[p] = self.best[p] as f32 + if self.alive[p] { 0.0 } else { 0.0 } + self.pos[p] as f32 * 0.01;
        }
        self.r.mask(s)
    }
    fn scene(&self, out: &mut Scene) {
        for k in 0..4 {
            let lit = self.flash == Some(k);
            let col = if lit { 0xffffff } else { COLS[k] };
            out.tile(k, [0.07 + 0.23 * k as f32, 0.34, 0.2, 0.42], col, if lit { "*" } else { "" });
        }
        out.banner = match self.phase {
            Phase::Watch => format!("Watch! {}", self.len()),
            Phase::Repeat => "Your turn!".into(),
            Phase::Gap => format!("Round {}", self.round - 1),
        };
        for p in self.r.ids() {
            out.avatar(p, -6.0 + 4.0 * p as f32, -2.6, 0.0, 0.0, !self.alive[p]);
            out.tags[p] = if self.alive[p] { format!("{}", self.pos[p]) } else { "OUT".into() };
        }
    }
}
