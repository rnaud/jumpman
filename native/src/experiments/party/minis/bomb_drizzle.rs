//! Bomb Drizzle: bombs rain from the sky with a warning shadow. Dodge them; last one standing wins.
use crate::experiments::party::common::*;

const LIMIT: f32 = 26.0;
const SPEED: f32 = 5.2;
const WARN: f32 = 0.9;
const BLAST: f32 = 1.05;

#[derive(Clone, Copy)]
struct Bomb {
    x: f32,
    z: f32,
    t: f32,
}

pub struct BombDrizzle {
    r: Roster,
    t: f32,
    pos: [[f32; 2]; 4],
    face: [f32; 4],
    alive: [bool; 4],
    out_at: [f32; 4],
    bombs: Vec<Bomb>,
    next: f32,
    rng: Rng,
    brng: Rng,
    boom: Vec<(f32, f32, f32)>,
}

impl BombDrizzle {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        BombDrizzle {
            r: Roster::new(active),
            t: 0.0,
            pos: [[-6.0, -2.0], [-2.0, 2.0], [2.0, -2.0], [6.0, 2.0]],
            face: [0.0; 4],
            alive: active,
            out_at: [LIMIT; 4],
            bombs: vec![],
            next: 1.0,
            rng: Rng::new(seed),
            brng: Rng::new(seed ^ 0xb0e),
            boom: vec![],
        }
    }
}

impl MiniGame for BombDrizzle {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        for p in self.r.ids().collect::<Vec<_>>() {
            if !self.alive[p] {
                continue;
            }
            let (mut x, mut y) = (inp[p].x, inp[p].y);
            let m = (x * x + y * y).sqrt();
            if m > 1.0 {
                x /= m;
                y /= m;
            }
            if m > 0.08 {
                self.face[p] = x.atan2(y);
            }
            self.pos[p][0] = clamp(self.pos[p][0] + x * SPEED * dt, -HALF_X + 0.4, HALF_X - 0.4);
            self.pos[p][1] = clamp(self.pos[p][1] + y * SPEED * dt, -HALF_Z + 0.4, HALF_Z - 0.4);
        }
        self.next -= dt;
        if self.next <= 0.0 {
            // Half the bombs fall near a living player (so camping is punished), half anywhere.
            let alive: Vec<usize> = self.r.ids().filter(|&p| self.alive[p]).collect();
            let (x, z) = if !alive.is_empty() && self.rng.chance(0.5) {
                let p = alive[self.rng.below(alive.len())];
                (clamp(self.pos[p][0] + self.rng.range(-1.2, 1.2), -8.6, 8.6), clamp(self.pos[p][1] + self.rng.range(-1.2, 1.2), -4.1, 4.1))
            } else {
                (self.rng.range(-8.6, 8.6), self.rng.range(-4.1, 4.1))
            };
            self.bombs.push(Bomb { x, z, t: 0.0 });
            self.next = (0.6 - self.t * 0.02).max(0.12) * self.rng.range(0.7, 1.2);
        }
        self.boom.retain_mut(|b| {
            b.2 -= dt;
            b.2 > 0.0
        });
        let mut i = 0;
        while i < self.bombs.len() {
            self.bombs[i].t += dt;
            if self.bombs[i].t >= WARN {
                let b = self.bombs.swap_remove(i);
                self.boom.push((b.x, b.z, 0.3));
                for p in self.r.ids().collect::<Vec<_>>() {
                    if self.alive[p] && dist(self.pos[p][0], self.pos[p][1], b.x, b.z) < BLAST {
                        self.alive[p] = false;
                        self.out_at[p] = self.t;
                    }
                }
                continue;
            }
            i += 1;
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if !self.alive[p] {
            return PadIn::default();
        }
        // Flee from bombs about to land on me (the smarter, the earlier); otherwise drift to the centre.
        let me = self.pos[p];
        let (mut fx, mut fz) = (0.0f32, 0.0f32);
        let radius = BLAST + 0.3 + lvl.skill() * 0.4;
        for b in &self.bombs {
            let d = dist(me[0], me[1], b.x, b.z);
            if d < radius && b.t >= lvl.react() * 0.9 {
                let k = (radius - d) / radius + 0.3;
                let (dx, dz) = if d > 0.01 { ((me[0] - b.x) / d, (me[1] - b.z) / d) } else { (1.0, 0.0) };
                fx += dx * k;
                fz += dz * k;
            }
        }
        if fx == 0.0 && fz == 0.0 {
            // Idle: wander toward the middle so walls never trap us.
            let (cx, cz) = (-me[0] * 0.12, -me[1] * 0.2);
            return PadIn { x: clamp(cx, -0.5, 0.5), y: clamp(cz, -0.5, 0.5), ..Default::default() };
        }
        // Walls: if the escape points into one, slide along it.
        if (me[0] > HALF_X - 0.8 && fx > 0.0) || (me[0] < -HALF_X + 0.8 && fx < 0.0) {
            fz += if me[1] > 0.0 { -1.0 } else { 1.0 };
        }
        if (me[1] > HALF_Z - 0.8 && fz > 0.0) || (me[1] < -HALF_Z + 0.8 && fz < 0.0) {
            fx += if me[0] > 0.0 { -1.0 } else { 1.0 };
        }
        let m = (fx * fx + fz * fz).sqrt().max(1e-3);
        let _ = self.brng.f();
        let speed = match lvl {
            Level::Easy => 0.72,
            Level::Normal => 0.88,
            Level::Hard => 1.0,
        };
        PadIn { x: fx / m * speed, y: fz / m * speed, ..Default::default() }
    }
    fn done(&self) -> bool {
        self.t >= LIMIT || (self.r.count() > 1 && self.r.ids().filter(|&p| self.alive[p]).count() <= 1 && self.t > 1.0)
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
        out.shape(Sh::Box, [0.0, -0.06, 0.0], [18.6, 0.12, 9.4], 0x6a5a86);
        for b in &self.bombs {
            let u = b.t / WARN;
            out.shape(Sh::Disc, [b.x, 0.02, b.z], [BLAST * (0.4 + 0.6 * u), 0.04, 0.0], if (self.t * 12.0) as i32 % 2 == 0 { 0xff5a3a } else { 0xffb04a });
            let y = 7.0 * (1.0 - u) + 0.35;
            out.shape(Sh::Ball, [b.x, y, b.z], [0.35, 0.35, 0.35], 0x202028);
        }
        for &(x, z, l) in &self.boom {
            out.shape(Sh::Ball, [x, 0.5, z], [BLAST * (1.3 - l), 0.0, 0.0], 0xffd24a);
        }
        for p in self.r.ids() {
            out.avatar(p, self.pos[p][0], self.pos[p][1], 0.0, self.face[p], !self.alive[p]);
            out.tags[p] = if self.alive[p] { "alive".into() } else { "OUT".into() };
        }
    }
}
