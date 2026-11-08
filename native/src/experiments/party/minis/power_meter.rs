//! Power Meter: a marker sweeps the gauge. Tap to stop it as close to the gold zone as you can. Three tries.
use crate::experiments::party::common::*;

const TRIES: usize = 3;

pub struct PowerMeter {
    r: Roster,
    t: f32,
    round: usize,
    rt: f32,
    zone: f32,
    half: f32,
    speed: f32,
    stopped: [Option<f32>; 4],
    points: [f32; 4],
    gap: f32,
    rng: Rng,
    brng: Rng,
    aim: [Option<f32>; 4],
    over: bool,
    phase: [f32; 4],
}

impl PowerMeter {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut g = PowerMeter { r: Roster::new(active), t: 0.0, round: 0, rt: 0.0, zone: 0.5, half: 0.1, speed: 1.0, stopped: [None; 4], points: [0.0; 4], gap: 0.0, rng: Rng::new(seed), brng: Rng::new(seed ^ 0x9e7), aim: [None; 4], over: false, phase: [0.0; 4] };
        g.start_round();
        g
    }
    fn start_round(&mut self) {
        self.round += 1;
        self.rt = 0.0;
        self.zone = self.rng.range(0.2, 0.8);
        self.half = (0.09 - 0.02 * self.round as f32).max(0.04);
        self.speed = 0.9 + 0.35 * self.round as f32;
        for p in 0..4 {
            self.phase[p] = self.rng.range(0.0, 2.0);
        }
        self.stopped = [None; 4];
        self.aim = [None; 4];
        self.gap = 0.0;
    }
    /// Marker position in 0..1 (triangle wave), each player has their own phase so the sweep differs.
    fn marker(&self, p: usize) -> f32 {
        let u = (self.rt * self.speed + self.phase[p]) % 2.0;
        if u < 1.0 {
            u
        } else {
            2.0 - u
        }
    }
}

impl MiniGame for PowerMeter {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        if self.over {
            return;
        }
        if self.gap > 0.0 {
            self.gap -= dt;
            if self.gap <= 0.0 {
                if self.round >= TRIES {
                    self.over = true;
                } else {
                    self.start_round();
                }
            }
            return;
        }
        self.rt += dt;
        for p in self.r.ids().collect::<Vec<_>>() {
            if self.stopped[p].is_none() && (inp[p].act || self.rt > 7.0) {
                let m = self.marker(p);
                self.stopped[p] = Some(m);
                let miss = (m - self.zone).abs();
                // 100 at the centre of the gold, falling to 0 at 0.35 away.
                self.points[p] += (100.0 * (1.0 - ((miss - 0.0) / 0.35).min(1.0))).round() + if miss <= self.half { 25.0 } else { 0.0 };
            }
        }
        if self.r.ids().all(|p| self.stopped[p].is_some()) {
            self.gap = 1.4;
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if self.gap > 0.0 || self.stopped[p].is_some() {
            return PadIn::default();
        }
        if self.aim[p].is_none() {
            self.aim[p] = Some(self.zone + (self.brng.f() - 0.5) * 2.0 * (1.0 - lvl.skill()) * 0.45);
        }
        let want = self.aim[p].unwrap();
        // Anticipate: the marker moves `speed` per second; react a bit late.
        let lead = self.speed * lvl.react() * 0.4;
        let m = self.marker(p);
        let u = (self.rt * self.speed + self.phase[p]) % 2.0;
        let dir = if u < 1.0 { 1.0 } else { -1.0 };
        if (m + dir * lead - want).abs() < self.speed / 30.0 * 0.6 + 0.004 {
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
        self.r.mask(self.points)
    }
    fn scene(&self, out: &mut Scene) {
        for p in self.r.ids() {
            let z = lane_z(p);
            out.shape(Sh::Box, [0.0, 0.0, z], [12.0, 0.1, 0.8], 0x3b3f58);
            out.shape(Sh::Box, [-6.0 + 12.0 * self.zone, 0.08, z], [24.0 * self.half, 0.12, 0.8], 0xffc72a);
            let m = self.stopped[p].unwrap_or_else(|| self.marker(p));
            out.shape(Sh::Box, [-6.0 + 12.0 * m, 0.4, z], [0.18, 0.8, 0.9], if self.stopped[p].is_some() { PLAYER_COLORS[p] } else { 0xffffff });
            out.avatar(p, -7.4, z, 0.0, 1.5708, false);
            out.tags[p] = format!("{}", self.points[p] as u32);
        }
        out.banner = format!("Try {} of {}", self.round, TRIES);
    }
}
