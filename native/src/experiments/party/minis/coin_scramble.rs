//! Coin Scramble: coins keep appearing all over the hills; run, jump onto the ledges and grab the most. A gold coin is
//! worth three. 30 seconds.
use super::*;

pub const SECS: f32 = 30.0;
/// Coins on the field at any time.
const ON_FIELD: usize = 9;
/// Pick-up reach (horizontal) and height window.
const REACH: f64 = 105.0;

#[derive(Clone, Debug)]
pub struct Coin {
    pub pos: [f64; 3],
    pub gold: bool,
    /// Seconds since it appeared (it pops up for a moment before it can be taken).
    pub age: f32,
}

pub struct CoinScramble {
    r: Roster,
    rng: Rng,
    field: Arc<Field>,
    pub coins: Vec<Coin>,
    score: [f32; 4],
    t: f32,
    think: [f32; 4],
    goal: [Option<usize>; 4],
    evs: Vec<MiniEv>,
}

impl CoinScramble {
    pub fn new(seed: u64, active: [bool; 4], field: Arc<Field>) -> Self {
        let mut g = CoinScramble { r: Roster::new(active), rng: Rng::new(seed), field, coins: vec![], score: [0.0; 4], t: 0.0, think: [0.0; 4], goal: [None; 4], evs: vec![] };
        for _ in 0..ON_FIELD {
            let c = g.fresh_coin();
            g.coins.push(Coin { age: 1.0, ..c });
        }
        g
    }

    /// A new coin on a random point of the arena, away from the other coins.
    fn fresh_coin(&mut self) -> Coin {
        let mut best = self.field.rand_point(&mut self.rng);
        for _ in 0..6 {
            if self.coins.iter().all(|c| hdist(c.pos, best) > 260.0) {
                break;
            }
            best = self.field.rand_point(&mut self.rng);
        }
        Coin { pos: best, gold: self.rng.chance(0.2), age: 0.0 }
    }

    fn value(c: &Coin) -> f32 {
        if c.gold {
            3.0
        } else {
            1.0
        }
    }
}

impl MiniGame for CoinScramble {
    fn start(&self, p: usize) -> [f64; 3] {
        self.field.start(p, 0.18)
    }

    fn step(&mut self, dt: f32, bodies: &[Body; 4], _drives: &[Drive; 4]) {
        if self.done() {
            return;
        }
        self.t += dt;
        for c in &mut self.coins {
            c.age += dt;
        }
        // Players take a coin they touch (ties go round the seats, see `seat_order`).
        for p in seat_order(self.t) {
            if !self.r.active[p] {
                continue;
            }
            let b = bodies[p].pos;
            if let Some(i) = self.coins.iter().position(|c| c.age > 0.35 && hdist(c.pos, b) < REACH && b[1] > c.pos[1] - 120.0 && b[1] < c.pos[1] + 190.0) {
                let c = self.coins[i].clone();
                self.score[p] += Self::value(&c);
                self.evs.push(MiniEv::Cue { kind: if c.gold { "red_coin" } else { "coin" }, pos: c.pos });
                self.coins[i] = self.fresh_coin();
            }
        }
    }

    fn bot(&mut self, p: usize, lvl: Level, bodies: &[Body; 4]) -> Drive {
        let me = bodies[p].pos;
        self.think[p] -= DT;
        let gone = self.goal[p].is_none_or(|i| i >= self.coins.len());
        if self.think[p] <= 0.0 || gone {
            self.think[p] = lvl.react() + self.rng.f() * 0.25;
            let skill = lvl.skill() as f64;
            let mut best = (f64::MIN, None);
            for (i, c) in self.coins.iter().enumerate() {
                let d = hdist(me, c.pos) + (c.pos[1] - me[1]).max(0.0) * 1.5;
                // Better bots leave a coin to a rival who is clearly closer.
                let rival_closer = (0..4).any(|q| q != p && self.r.active[q] && hdist(bodies[q].pos, c.pos) + 150.0 < hdist(me, c.pos));
                let mut v = Self::value(c) as f64 / (d + 260.0);
                if rival_closer {
                    v *= 1.0 - skill * 0.7;
                }
                v *= 1.0 + (self.rng.f() as f64 - 0.5) * (1.0 - skill) * 1.6;
                if v > best.0 {
                    best = (v, Some(i));
                }
            }
            self.goal[p] = best.1;
        }
        let Some(c) = self.goal[p].and_then(|i| self.coins.get(i)) else { return Drive::default() };
        let mut d = run_to(me, c.pos, 0.0);
        d.mag = d.mag.max(0.6) * (0.82 + 0.18 * lvl.skill() as f64);
        // A coin on a ledge: jump for it.
        if c.pos[1] > me[1] + 90.0 && hdist(me, c.pos) < 330.0 && bodies[p].grounded {
            d.jump = true;
            d.hold = true;
        }
        d
    }

    fn events(&mut self) -> Vec<MiniEv> {
        std::mem::take(&mut self.evs)
    }

    fn done(&self) -> bool {
        self.t >= SECS
    }

    fn time_left(&self) -> f32 {
        (SECS - self.t).max(0.0)
    }

    fn scores(&self) -> [f32; 4] {
        self.r.mask(self.score)
    }

    fn scene(&self, out: &mut Stage) {
        for c in &self.coins {
            let pop = (c.age / 0.35).min(1.0);
            let y = c.pos[1] + 70.0 + 10.0 * (self.t * 4.0 + c.pos[0] as f32 * 0.01).sin() as f64;
            let spin = self.t * 5.0 + c.pos[2] as f32 * 0.01;
            if c.gold {
                out.coin([c.pos[0], y, c.pos[2]], 0.5 * pop, spin, 0xff9a1f);
            } else {
                out.coin([c.pos[0], y, c.pos[2]], 0.36 * pop, spin, 0xffd21f);
            }
        }
        for p in self.r.ids() {
            out.tags[p] = format!("{}", self.score[p] as i32);
        }
        if self.t < 1.0 {
            out.banner = "GRAB THE COINS!".into();
        } else if self.time_left() < 5.0 {
            out.banner = format!("{}", self.time_left().ceil() as i32);
        }
    }
}
