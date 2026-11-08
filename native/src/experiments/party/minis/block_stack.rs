//! Block Stack: a block slides back and forth; tap to drop it. Only the overlap stays. Build the tallest tower.
use crate::experiments::party::common::*;

const LIMIT: f32 = 20.0;
const BASE_W: f32 = 2.4;

pub struct BlockStack {
    r: Roster,
    t: f32,
    /// Per player: centre and width of the top block, the sliding block's phase, height count, alive.
    top: [(f32, f32); 4],
    phase: [f32; 4],
    h: [u32; 4],
    perfect: [u32; 4],
    alive: [bool; 4],
    brng: Rng,
    aim: [Option<f32>; 4],
    drop_fx: [f32; 4],
}

impl BlockStack {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut rng = Rng::new(seed);
        let mut phase = [0.0; 4];
        for p in phase.iter_mut() {
            *p = rng.range(0.0, 6.28);
        }
        BlockStack { r: Roster::new(active), t: 0.0, top: [(0.0, BASE_W); 4], phase, h: [0; 4], perfect: [0; 4], alive: active, brng: Rng::new(seed ^ 0xb5), aim: [None; 4], drop_fx: [0.0; 4] }
    }
    fn speed(&self, p: usize) -> f32 {
        1.6 + 0.09 * self.h[p] as f32
    }
    fn slider(&self, p: usize) -> f32 {
        (self.phase[p]).sin() * 3.0
    }
}

impl MiniGame for BlockStack {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        for p in self.r.ids().collect::<Vec<_>>() {
            self.drop_fx[p] = (self.drop_fx[p] - dt).max(0.0);
            if !self.alive[p] {
                continue;
            }
            self.phase[p] += self.speed(p) * dt;
            if inp[p].act {
                let x = self.slider(p);
                let (tx, tw) = self.top[p];
                let (lo, hi) = ((x - tw / 2.0).max(tx - tw / 2.0), (x + tw / 2.0).min(tx + tw / 2.0));
                let ov = hi - lo;
                if ov <= 0.12 {
                    self.alive[p] = false;
                } else if (x - tx).abs() < 0.1 {
                    // Perfect: keep the full width and add a little.
                    self.top[p] = (tx, (tw + 0.15).min(BASE_W));
                    self.h[p] += 1;
                    self.perfect[p] += 1;
                } else {
                    self.top[p] = ((lo + hi) / 2.0, ov);
                    self.h[p] += 1;
                }
                self.drop_fx[p] = 0.25;
                self.aim[p] = None;
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if !self.alive[p] {
            return PadIn::default();
        }
        // Tap when the slider is close to the tower centre; sloppiness is the tolerance and timing jitter.
        if self.aim[p].is_none() {
            self.aim[p] = Some((self.brng.f() - 0.5) * (1.0 - lvl.skill()) * 2.6 * self.top[p].1);
        }
        let want = self.top[p].0 + self.aim[p].unwrap();
        let x = self.slider(p);
        let vel = self.phase[p].cos() * self.speed(p) * 3.0;
        let lead = vel * 0.033 * (1.0 + (1.0 - lvl.skill()) * 2.0);
        if (x + lead - want).abs() < (vel.abs() / 30.0).max(0.08) {
            return PadIn { act: true, ..Default::default() };
        }
        PadIn::default()
    }
    fn done(&self) -> bool {
        self.t >= LIMIT || self.r.ids().all(|p| !self.alive[p])
    }
    fn time_left(&self) -> f32 {
        (LIMIT - self.t).max(0.0)
    }
    fn scores(&self) -> [f32; 4] {
        let mut s = [0.0; 4];
        for p in 0..4 {
            s[p] = self.h[p] as f32 + self.perfect[p] as f32 * 0.1 + self.top[p].1 * 0.01;
        }
        self.r.mask(s)
    }
    fn scene(&self, out: &mut Stage) {
        for p in self.r.ids() {
            let cx = -6.5 + 4.3 * p as f32;
            out.shape(Sh::Box, [cx, -0.1, 0.0], [BASE_W + 0.6, 0.2, 1.6], 0x555566);
            // The tower is drawn as a stack of slabs up to 10 high (the rest compress).
            let n = self.h[p].min(10);
            for i in 0..n {
                out.shape(Sh::Box, [cx + (if i + 1 == n { self.top[p].0 * 0.6 } else { 0.0 }), 0.2 + 0.32 * i as f32, 0.0], [if i + 1 == n { self.top[p].1 * 0.6 } else { BASE_W * 0.6 }, 0.3, 1.2], PLAYER_COLORS[p]);
            }
            if self.alive[p] {
                let y = 0.2 + 0.32 * n as f32 + 0.8;
                out.shape(Sh::Box, [cx + self.slider(p) * 0.6, y, 0.0], [self.top[p].1 * 0.6, 0.3, 1.2], 0xffffff);
            }
            out.avatar(p, cx - 1.2, 2.6, 0.0, 0.0, false);
            out.tags[p] = if self.alive[p] { format!("{}", self.h[p]) } else { format!("{} (fell)", self.h[p]) };
        }
    }
}
