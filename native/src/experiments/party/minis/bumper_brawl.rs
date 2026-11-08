//! Bumper Brawl: bumper cars on a shrinking disc. Steer, dash into rivals, stay on.
use crate::experiments::party::common::*;

const LIMIT: f32 = 26.0;
const R: f32 = 0.7;

pub struct BumperBrawl {
    r: Roster,
    t: f32,
    pos: [[f32; 2]; 4],
    vel: [[f32; 2]; 4],
    face: [f32; 4],
    cd: [f32; 4],
    alive: [bool; 4],
    out_at: [f32; 4],
    brng: Rng,
}

impl BumperBrawl {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let a = [[-2.4, -1.5], [2.4, -1.5], [-2.4, 1.5], [2.4, 1.5]];
        BumperBrawl { r: Roster::new(active), t: 0.0, pos: a, vel: [[0.0; 2]; 4], face: [0.0; 4], cd: [0.0; 4], alive: active, out_at: [LIMIT; 4], brng: Rng::new(seed ^ 0xb0a) }
    }
    fn arena(&self) -> f32 {
        (4.4 - (self.t - 5.0).max(0.0) * 0.09).max(2.0)
    }
}

impl MiniGame for BumperBrawl {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        let ids: Vec<usize> = self.r.ids().filter(|&p| self.alive[p]).collect();
        for &p in &ids {
            self.cd[p] = (self.cd[p] - dt).max(0.0);
            let (mut x, mut y) = (inp[p].x, inp[p].y);
            let m = (x * x + y * y).sqrt();
            if m > 1.0 {
                x /= m;
                y /= m;
            }
            self.vel[p][0] += x * 14.0 * dt;
            self.vel[p][1] += y * 14.0 * dt;
            if m > 0.1 {
                self.face[p] = x.atan2(y);
            }
            if inp[p].act && self.cd[p] <= 0.0 {
                let (fx, fz) = (self.face[p].sin(), self.face[p].cos());
                self.vel[p][0] += fx * 5.0;
                self.vel[p][1] += fz * 5.0;
                self.cd[p] = 1.3;
            }
            // Friction.
            let k = (1.0 - 1.6 * dt).max(0.0);
            self.vel[p][0] *= k;
            self.vel[p][1] *= k;
            self.pos[p][0] += self.vel[p][0] * dt;
            self.pos[p][1] += self.vel[p][1] * dt;
        }
        // Elastic-ish collisions.
        for i in 0..ids.len() {
            for j in i + 1..ids.len() {
                let (a, b) = (ids[i], ids[j]);
                let (dx, dz) = (self.pos[b][0] - self.pos[a][0], self.pos[b][1] - self.pos[a][1]);
                let d = (dx * dx + dz * dz).sqrt();
                if d < 2.0 * R && d > 1e-4 {
                    let (nx, nz) = (dx / d, dz / d);
                    let rel = (self.vel[a][0] - self.vel[b][0]) * nx + (self.vel[a][1] - self.vel[b][1]) * nz;
                    let push = (2.0 * R - d) * 0.5;
                    self.pos[a][0] -= nx * push;
                    self.pos[a][1] -= nz * push;
                    self.pos[b][0] += nx * push;
                    self.pos[b][1] += nz * push;
                    if rel > 0.0 {
                        let j = rel * 1.1;
                        self.vel[a][0] -= nx * j;
                        self.vel[a][1] -= nz * j;
                        self.vel[b][0] += nx * j;
                        self.vel[b][1] += nz * j;
                    }
                }
            }
        }
        let ar = self.arena();
        for &p in &ids {
            // The disc is an ellipse so it fits the wide screen.
            let e = (self.pos[p][0] / (ar * 1.6)).powi(2) + (self.pos[p][1] / ar).powi(2);
            if e > 1.0 {
                self.alive[p] = false;
                self.out_at[p] = self.t;
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if !self.alive[p] {
            return PadIn::default();
        }
        let me = self.pos[p];
        let ar = self.arena();
        let edge = (me[0] / (ar * 1.6)).powi(2) + (me[1] / ar).powi(2);
        // Near the rim: steer home. Otherwise chase the nearest rival and dash when lined up.
        if edge > (0.62 + 0.12 * (1.0 - lvl.skill())).powi(2) {
            let (dx, dz) = (-me[0], -me[1]);
            let m = (dx * dx + dz * dz).sqrt().max(1e-3);
            return PadIn { x: dx / m, y: dz / m, ..Default::default() };
        }
        let mut best: Option<(f32, usize)> = None;
        for q in self.r.ids() {
            if q != p && self.alive[q] {
                let d = dist(me[0], me[1], self.pos[q][0], self.pos[q][1]);
                if best.map_or(true, |b| d < b.0) {
                    best = Some((d, q));
                }
            }
        }
        let Some((d, q)) = best else { return PadIn::default() };
        let (dx, dz) = (self.pos[q][0] - me[0], self.pos[q][1] - me[1]);
        let m = d.max(1e-3);
        let aggressive = lvl.skill();
        let dash = d < 2.3 && self.cd[p] <= 0.0 && self.brng.chance(0.04 + 0.1 * aggressive);
        PadIn { x: dx / m * (0.5 + 0.5 * aggressive), y: dz / m * (0.5 + 0.5 * aggressive), act: dash, ..Default::default() }
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
            s[p] = if self.alive[p] { LIMIT + 5.0 } else { self.out_at[p] };
        }
        self.r.mask(s)
    }
    fn scene(&self, out: &mut Stage) {
        let ar = self.arena();
        out.shape(Sh::Disc, [0.0, -0.1, 0.0], [ar * 1.6, 0.2, ar], 0xd8c8a0);
        out.shape(Sh::Disc, [0.0, -0.02, 0.0], [ar * 1.6 + 0.25, 0.02, ar + 0.25], 0xa0522d);
        for p in self.r.ids() {
            out.avatar(p, self.pos[p][0], self.pos[p][1], 0.0, self.face[p], !self.alive[p]);
            if self.alive[p] {
                out.shape(Sh::Disc, [self.pos[p][0], 0.12, self.pos[p][1]], [R, 0.22, 0.0], PLAYER_COLORS[p]);
            }
            out.tags[p] = if self.alive[p] { "in".into() } else { "OUT".into() };
        }
    }
}
