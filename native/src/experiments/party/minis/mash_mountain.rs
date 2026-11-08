//! Mash Mountain: tap as fast as you can to climb. Tapping too fast without a rhythm stumbles you.
use crate::experiments::party::common::*;

const LIMIT: f32 = 9.0;

pub struct MashMountain {
    r: Roster,
    t: f32,
    height: [f32; 4],
    /// Stamina 0..1: every tap costs a little, rest refills it; empty = slow climb.
    stamina: [f32; 4],
    acc: [f32; 4],
    brng: Rng,
    rate: [f32; 4],
}

impl MashMountain {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        MashMountain { r: Roster::new(active), t: 0.0, height: [0.0; 4], stamina: [1.0; 4], acc: [0.0; 4], brng: Rng::new(seed ^ 0x3a5), rate: [0.0; 4] }
    }
}

impl MiniGame for MashMountain {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        for p in self.r.ids().collect::<Vec<_>>() {
            self.stamina[p] = (self.stamina[p] + dt * 0.35).min(1.0);
            if inp[p].act {
                let gain = if self.stamina[p] > 0.05 { 1.0 } else { 0.35 };
                self.height[p] += gain;
                self.stamina[p] = (self.stamina[p] - 0.075).max(0.0);
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        // Taps per second by level, wobbling a little.
        let base = match lvl {
            Level::Easy => 5.0,
            Level::Normal => 6.6,
            Level::Hard => 8.0,
        };
        if self.rate[p] == 0.0 || self.brng.chance(0.02) {
            self.rate[p] = base * self.brng.range(0.85, 1.12);
        }
        self.acc[p] += self.rate[p] / 30.0;
        if self.acc[p] >= 1.0 {
            self.acc[p] -= 1.0;
            PadIn { act: true, ..Default::default() }
        } else {
            PadIn::default()
        }
    }
    fn done(&self) -> bool {
        self.t >= LIMIT
    }
    fn time_left(&self) -> f32 {
        (LIMIT - self.t).max(0.0)
    }
    fn scores(&self) -> [f32; 4] {
        self.r.mask(self.height)
    }
    fn scene(&self, out: &mut Scene) {
        // Four poles with a gold flag at the top; climbers rise along them.
        let top = 3.2;
        for p in self.r.ids() {
            let x = -6.0 + 4.0 * p as f32;
            out.shape(Sh::Disc, [x, 1.9, 0.0], [0.12, 3.8, 0.12], 0xd8d0c0);
            out.shape(Sh::Box, [x + 0.45, 3.65, 0.0], [0.9, 0.5, 0.05], PLAYER_COLORS[p]);
            out.shape(Sh::Disc, [x, -0.05, 0.0], [1.0, 0.1, 1.0], 0x6b8f3a);
            let y = (self.height[p] / 60.0).min(1.0) * top;
            out.avatar(p, x, 0.6, y, 0.0, false);
            out.tags[p] = format!("{}", self.height[p] as u32);
            // stamina bar
            out.shape(Sh::Box, [x, 0.12, 1.5], [1.4 * self.stamina[p].max(0.02), 0.12, 0.2], if self.stamina[p] > 0.2 { 0x7bd33a } else { 0xd33a3a });
        }
    }
}
