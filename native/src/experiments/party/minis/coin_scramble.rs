//! Coin Scramble: run around the arena and grab the coins. Gold coins are worth three.
use crate::experiments::party::common::*;

const LIMIT: f32 = 20.0;
const SPEED: f32 = 5.6;

#[derive(Clone, Copy)]
struct Coin {
    x: f32,
    z: f32,
    gold: bool,
    life: f32,
}

pub struct CoinScramble {
    r: Roster,
    t: f32,
    pos: [[f32; 2]; 4],
    face: [f32; 4],
    coins: Vec<Coin>,
    score: [f32; 4],
    spawn: f32,
    rng: Rng,
    brng: Rng,
    target: [Option<usize>; 4],
}

impl CoinScramble {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let starts = [[-6.0, -2.5], [6.0, -2.5], [-6.0, 2.5], [6.0, 2.5]];
        let mut g = CoinScramble { r: Roster::new(active), t: 0.0, pos: starts, face: [0.0; 4], coins: vec![], score: [0.0; 4], spawn: 0.0, rng: Rng::new(seed), brng: Rng::new(seed ^ 0xc01), target: [None; 4] };
        for _ in 0..7 {
            g.add_coin();
        }
        g
    }
    fn add_coin(&mut self) {
        let gold = self.rng.chance(0.14);
        let c = Coin { x: self.rng.range(-8.0, 8.0), z: self.rng.range(-3.7, 3.7), gold, life: if gold { 4.0 } else { 7.0 } };
        self.coins.push(c);
    }
}

impl MiniGame for CoinScramble {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        for p in self.r.ids().collect::<Vec<_>>() {
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
        // Pickups: first player in range takes it.
        let mut i = 0;
        while i < self.coins.len() {
            let c = self.coins[i];
            let hit = self.r.ids().filter(|&p| dist(self.pos[p][0], self.pos[p][1], c.x, c.z) < 0.7).min_by(|&a, &b| dist(self.pos[a][0], self.pos[a][1], c.x, c.z).partial_cmp(&dist(self.pos[b][0], self.pos[b][1], c.x, c.z)).unwrap());
            if let Some(p) = hit {
                self.score[p] += if c.gold { 3.0 } else { 1.0 };
                self.coins.swap_remove(i);
                continue;
            }
            self.coins[i].life -= dt;
            if self.coins[i].life <= 0.0 {
                self.coins.swap_remove(i);
                continue;
            }
            i += 1;
        }
        self.spawn -= dt;
        if self.spawn <= 0.0 && self.coins.len() < 12 {
            self.add_coin();
            self.spawn = 0.35;
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        // Re-pick a target now and then: skilled bots take the best coin by value / distance.
        let retarget = self.target[p].map_or(true, |i| i >= self.coins.len()) || self.brng.chance(0.03);
        if retarget && !self.coins.is_empty() {
            let me = self.pos[p];
            let mut best = 0;
            let mut bs = f32::MIN;
            for (i, c) in self.coins.iter().enumerate() {
                let d = dist(me[0], me[1], c.x, c.z);
                let v = if c.gold { 3.0 } else { 1.0 };
                let s = v / (d + 0.6) * if self.brng.chance(lvl.skill()) { 1.0 } else { self.brng.range(0.2, 1.0) };
                if s > bs {
                    bs = s;
                    best = i;
                }
            }
            self.target[p] = Some(best);
        }
        let Some(i) = self.target[p].filter(|&i| i < self.coins.len()) else { return PadIn::default() };
        let c = self.coins[i];
        let (dx, dz) = (c.x - self.pos[p][0], c.z - self.pos[p][1]);
        let m = (dx * dx + dz * dz).sqrt().max(1e-3);
        let speed = match lvl {
            Level::Easy => 0.7,
            Level::Normal => 0.88,
            Level::Hard => 1.0,
        };
        PadIn { x: dx / m * speed, y: dz / m * speed, ..Default::default() }
    }
    fn done(&self) -> bool {
        self.t >= LIMIT
    }
    fn time_left(&self) -> f32 {
        (LIMIT - self.t).max(0.0)
    }
    fn scores(&self) -> [f32; 4] {
        self.r.mask(self.score)
    }
    fn scene(&self, out: &mut Stage) {
        out.shape(Sh::Box, [0.0, -0.06, 0.0], [18.6, 0.12, 9.4], 0x3f7f4a);
        for c in &self.coins {
            let y = 0.45 + (self.t * 4.0 + c.x).sin() * 0.08;
            let fade = if c.life < 1.2 && (self.t * 10.0) as i32 % 2 == 0 { 0.0 } else { 1.0 };
            if fade > 0.0 {
                out.shape_yaw(Sh::Disc, [c.x, y, c.z], [if c.gold { 0.5 } else { 0.34 }, 0.08, 0.0], 0.0, if c.gold { 0xffe14a } else { 0xf5b81c });
            }
        }
        for p in self.r.ids() {
            out.avatar(p, self.pos[p][0], self.pos[p][1], 0.0, self.face[p], false);
            out.tags[p] = format!("{}", self.score[p] as u32);
        }
    }
}
