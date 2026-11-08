//! Plank Balance: keep your balance on a wobbling plank with the stick. Last on the plank wins.
use crate::experiments::party::common::*;

const LIMIT: f32 = 24.0;

pub struct PlankBalance {
    r: Roster,
    t: f32,
    ang: [f32; 4],
    vel: [f32; 4],
    alive: [bool; 4],
    out_at: [f32; 4],
    rng: Rng,
    brng: Rng,
    /// Wobble impulses are the same for everyone so luck is shared: (time, impulse).
    kicks: Vec<(f32, f32)>,
    seen: [Vec<f32>; 4],
}

impl PlankBalance {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut rng = Rng::new(seed);
        let mut kicks = vec![];
        let mut t = 0.8;
        while t < LIMIT {
            kicks.push((t, rng.range(-1.0, 1.0) * (1.0 + t * 0.1)));
            t += rng.range(0.5, 1.1);
        }
        PlankBalance { r: Roster::new(active), t: 0.0, ang: [0.0; 4], vel: [0.0; 4], alive: active, out_at: [LIMIT; 4], rng, brng: Rng::new(seed ^ 0xba1), kicks, seen: Default::default() }
    }
}

impl MiniGame for PlankBalance {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        let t0 = self.t;
        self.t += dt;
        let _ = &mut self.rng;
        let kick: f32 = self.kicks.iter().filter(|k| k.0 > t0 && k.0 <= self.t).map(|k| k.1).sum();
        for p in self.r.ids().collect::<Vec<_>>() {
            if !self.alive[p] {
                continue;
            }
            // Gravity-like instability: tilt accelerates away from level; the stick leans against it.
            let acc = 1.35 * self.ang[p] + 3.4 * clamp(inp[p].x, -1.0, 1.0) - 0.9 * self.vel[p];
            self.vel[p] += (acc * dt) + kick * 0.5;
            self.ang[p] += self.vel[p] * dt;
            if self.ang[p].abs() > 1.0 {
                self.alive[p] = false;
                self.out_at[p] = self.t;
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        // A delayed PD controller: skill is the gain accuracy, level sets the lag.
        let lag = ((lvl.react() * 30.0) as usize / 3).max(1);
        self.seen[p].push(self.ang[p]);
        let n = self.seen[p].len();
        let a = self.seen[p][n.saturating_sub(lag + 1)];
        let v = if n > 1 { (self.ang[p] - self.seen[p][n - 2]) * 30.0 } else { 0.0 };
        let noise = (self.brng.f() - 0.5) * (1.0 - lvl.skill()) * 1.4;
        let u = -(a * 0.55 + v * 0.22) * 1.3 + noise;
        PadIn { x: clamp(u, -1.0, 1.0), ..Default::default() }
    }
    fn done(&self) -> bool {
        self.t >= LIMIT || (self.r.count() > 1 && self.r.ids().filter(|&p| self.alive[p]).count() <= 1)
    }
    fn time_left(&self) -> f32 {
        (LIMIT - self.t).max(0.0)
    }
    fn scores(&self) -> [f32; 4] {
        let mut s = [0.0; 4];
        for p in 0..4 {
            s[p] = if self.alive[p] { LIMIT + 5.0 - self.ang[p].abs() } else { self.out_at[p] };
        }
        self.r.mask(s)
    }
    fn scene(&self, out: &mut Scene) {
        for p in self.r.ids() {
            let z = lane_z(p);
            out.shape(Sh::Box, [0.0, -0.4, z], [4.0, 0.2, 0.3], 0x404050);
            let a = self.ang[p];
            // The plank tilts in the xy plane; approximate with a row of boards at tilted heights.
            for i in 0..8 {
                let x = -3.5 + i as f32;
                out.shape(Sh::Box, [x, -a * (x * 0.4), z], [0.95, 0.18, 1.1], if i % 2 == 0 { 0xc89a5a } else { 0xb98a4a });
            }
            let ax = clamp(a * 3.2, -3.6, 3.6);
            out.avatar(p, ax, z, -a * ax * 0.4 + 0.1, 0.0, !self.alive[p]);
            out.tags[p] = if self.alive[p] { "ok".into() } else { "OUT".into() };
        }
    }
}
