//! Barrel Hop: rolling barrels charge down every lane; tap to jump them. Cleared barrels score, hits cost.
use crate::experiments::party::common::*;

const LIMIT: f32 = 22.0;
const AIR: f32 = 0.55;
const PX: f32 = -5.5;
const N: usize = 30;

pub struct BarrelHop {
    r: Roster,
    t: f32,
    /// (spawn time, speed) of each barrel; every lane gets the same schedule.
    logs: Vec<(f32, f32)>,
    jump: [Option<f32>; 4],
    stun: [f32; 4],
    state: [[u8; N]; 4],
    cleared: [u32; 4],
    hits: [u32; 4],
    off: [Option<(usize, f32)>; 4],
    brng: Rng,
}

impl BarrelHop {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut rng = Rng::new(seed);
        let mut t = 1.2;
        let mut logs = vec![];
        for i in 0..N {
            logs.push((t, (5.0 + 0.17 * i as f32).min(9.0)));
            t += rng.range(0.95, 1.5) - (i as f32 * 0.012).min(0.3);
        }
        BarrelHop { r: Roster::new(active), t: 0.0, logs, jump: [None; 4], stun: [0.0; 4], state: [[0; N]; 4], cleared: [0; 4], hits: [0; 4], off: [None; 4], brng: Rng::new(seed ^ 0xb0b) }
    }
    fn x_of(&self, i: usize) -> f32 {
        let (t0, v) = self.logs[i];
        9.5 - v * (self.t - t0)
    }
    fn height(&self, p: usize) -> f32 {
        match self.jump[p] {
            Some(j) => {
                let u = (j / AIR).min(1.0);
                1.3 * 4.0 * u * (1.0 - u)
            }
            None => 0.0,
        }
    }
}

impl MiniGame for BarrelHop {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        for p in self.r.ids().collect::<Vec<_>>() {
            self.stun[p] = (self.stun[p] - dt).max(0.0);
            if let Some(j) = self.jump[p] {
                self.jump[p] = if j + dt >= AIR { None } else { Some(j + dt) };
            } else if inp[p].act && self.stun[p] <= 0.0 {
                self.jump[p] = Some(0.0);
            }
            for i in 0..N {
                if self.state[p][i] != 0 || self.t < self.logs[i].0 {
                    continue;
                }
                let x = self.x_of(i);
                if x < PX + 0.75 && x > PX - 0.75 && self.height(p) < 0.45 {
                    self.state[p][i] = 2;
                    self.hits[p] += 1;
                    self.stun[p] = 0.7;
                    self.jump[p] = None;
                } else if x <= PX - 0.75 {
                    self.state[p][i] = 1;
                    self.cleared[p] += 1;
                }
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        let next = (0..N).find(|&i| self.state[p][i] == 0 && self.t >= self.logs[i].0 && self.x_of(i) > PX - 0.75);
        let Some(i) = next else { return PadIn::default() };
        if self.off[p].map(|o| o.0) != Some(i) {
            let sloppy = (1.0 - lvl.skill()) * 2.2;
            let o = (self.brng.f() - 0.5) * sloppy;
            self.off[p] = Some((i, o));
        }
        let off = self.off[p].unwrap().1;
        let v = self.logs[i].1;
        let want = PX + v * AIR * 0.5 + off;
        PadIn { act: self.x_of(i) <= want && self.jump[p].is_none(), ..Default::default() }
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
            s[p] = self.cleared[p] as f32 - 1.5 * self.hits[p] as f32;
        }
        self.r.mask(s)
    }
    fn scene(&self, out: &mut Scene) {
        for p in self.r.ids() {
            let z = lane_z(p);
            out.shape(Sh::Box, [0.0, -0.05, z], [19.0, 0.1, 2.0], if p % 2 == 0 { 0x8c6a3c } else { 0x7a5a30 });
            out.avatar(p, PX, z, self.height(p), 1.5708, false);
            if self.stun[p] > 0.0 {
                out.shape(Sh::Ball, [PX, 1.9, z], [0.3, 0.3, 0.3], 0xffffff);
            }
            for i in 0..N {
                if self.t < self.logs[i].0 || self.state[p][i] == 1 {
                    continue;
                }
                let x = self.x_of(i);
                if x < -9.2 {
                    continue;
                }
                out.shape_yaw(Sh::Box, [x, 0.45, z], [0.9, 0.9, 1.4], self.t * 6.0, 0x5a2a00);
            }
            out.tags[p] = format!("{}", self.cleared[p]);
        }
    }
}
