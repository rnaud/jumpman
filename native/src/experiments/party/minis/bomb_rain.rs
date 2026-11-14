//! Bob-omb Rain: Bob-ombs fall on the battlefield where red rings grow on the ground. A blast knocks you flying and
//! costs a heart; three and you are out. The rain gets heavier and aims at people more as the clock runs down.
//! Score: seconds survived, plus two per heart left. 35 seconds, or until one Mario is left.
use super::*;

pub const SECS: f32 = 35.0;
pub const HEARTS: u8 = 3;
/// Blast radius.
pub const BLAST: f64 = 230.0;

/// Seconds from the ring appearing to the blast at the start; the fuses get shorter as the clock runs down.
const FUSE: f32 = 1.4;
const POWER: f64 = 36.0;
const SAFE: f32 = 1.4;

#[derive(Clone, Debug)]
pub struct Bomb {
    pub pos: [f64; 3],
    /// Seconds since the ring appeared.
    pub age: f32,
    /// Seconds from the ring to the blast.
    pub fuse: f32,
}

pub struct BombRain {
    r: Roster,
    rng: Rng,
    field: Arc<Field>,
    pub bombs: Vec<Bomb>,
    /// Blasts of the last moment, for the flash (pos, age).
    pub blasts: Vec<([f64; 3], f32)>,
    pub hearts: [u8; 4],
    out_at: [Option<f32>; 4],
    safe: [f32; 4],
    t: f32,
    next: f32,
    flee: [Option<[f64; 3]>; 4],
    think: [f32; 4],
    evs: Vec<MiniEv>,
}

impl BombRain {
    pub fn new(seed: u64, active: [bool; 4], field: Arc<Field>) -> Self {
        BombRain { r: Roster::new(active), rng: Rng::new(seed), field, bombs: vec![], blasts: vec![], hearts: [HEARTS; 4], out_at: [None; 4], safe: [0.0; 4], t: 0.0, next: 1.0, flee: [None; 4], think: [0.0; 4], evs: vec![] }
    }

    fn alive(&self) -> impl Iterator<Item = usize> + '_ {
        self.r.ids().filter(|&p| self.out_at[p].is_none())
    }

    /// Seconds between drops at time `t`: from about one a second down to three a second.
    pub fn interval(t: f32) -> f32 {
        1.0 - 0.72 * (t / SECS).min(1.0)
    }

    /// How dangerous a spot is: the bombs whose blast will cover it, weighted by how soon.
    fn danger(&self, at: [f64; 3], seen: f32) -> f64 {
        self.bombs.iter().filter(|b| b.age >= seen && hdist(b.pos, at) < BLAST + 70.0).map(|b| 1.0 + (b.age / b.fuse) as f64).sum()
    }
}

impl MiniGame for BombRain {
    fn start(&self, p: usize) -> [f64; 3] {
        self.field.start(p, 0.35)
    }

    fn step(&mut self, dt: f32, bodies: &[Body; 4], _drives: &[Drive; 4]) {
        if self.done() {
            return;
        }
        self.t += dt;
        for s in &mut self.safe {
            *s = (*s - dt).max(0.0);
        }
        self.blasts.retain_mut(|b| {
            b.1 += dt;
            b.1 < 0.5
        });
        // New drops.
        self.next -= dt;
        while self.next <= 0.0 {
            self.next += Self::interval(self.t);
            let alive: Vec<usize> = self.alive().collect();
            let aim = 0.5 + 0.35 * (self.t / SECS);
            let pos = if !alive.is_empty() && self.rng.chance(aim) {
                let p = alive[self.rng.below(alive.len())];
                let a = self.rng.f() as f64 * std::f64::consts::TAU;
                let r = self.rng.f() as f64 * 150.0;
                let b = bodies[p].pos;
                self.field.nearest([b[0] + a.cos() * r, b[1], b[2] + a.sin() * r])
            } else {
                self.field.rand_point(&mut self.rng)
            };
            self.bombs.push(Bomb { pos, age: 0.0, fuse: FUSE - 0.45 * (self.t / SECS).min(1.0) });
        }
        // Blasts.
        let mut boom = vec![];
        self.bombs.retain_mut(|b| {
            b.age += dt;
            if b.age >= b.fuse {
                boom.push(b.pos);
                false
            } else {
                true
            }
        });
        for at in boom {
            self.blasts.push((at, 0.0));
            self.evs.push(MiniEv::Cue { kind: "explosion", pos: at });
            for p in self.alive().collect::<Vec<_>>() {
                let b = bodies[p].pos;
                if self.safe[p] > 0.0 || hdist(b, at) > BLAST || (b[1] - at[1]).abs() > 260.0 {
                    continue;
                }
                self.hearts[p] -= 1;
                self.safe[p] = SAFE;
                if self.hearts[p] == 0 {
                    self.out_at[p] = Some(self.t);
                    self.evs.push(MiniEv::Cue { kind: "hurt", pos: b });
                } else {
                    self.evs.push(MiniEv::Knock { p, from: at, power: POWER });
                }
            }
        }
        // Wandered off the field: carried back (no heart lost, but it costs time).
        for p in self.alive().collect::<Vec<_>>() {
            if !self.field.on(bodies[p].pos) && self.field.fallen(bodies[p].pos) {
                self.evs.push(MiniEv::Place { p, pos: self.field.start(p, 0.2) });
            }
        }
    }

    fn bot(&mut self, p: usize, lvl: Level, bodies: &[Body; 4]) -> Drive {
        if self.out_at[p].is_some() {
            return Drive::default();
        }
        let me = bodies[p].pos;
        let skill = lvl.skill() as f64;
        // A bot sees a ring only once it is `react` old; Easy bots sometimes miss it altogether.
        let seen = lvl.react() * 0.9;
        self.think[p] -= DT;
        if self.think[p] <= 0.0 {
            self.think[p] = 0.12 + (1.0 - lvl.skill()) * 0.3;
            if self.danger(me, seen) > 0.0 && self.rng.chance(0.55 + 0.45 * skill as f32) {
                // Try spots around, keep the safest (and then the nearest).
                let mut best = (f64::MAX, me);
                for k in 0..14 {
                    let a = k as f64 / 14.0 * std::f64::consts::TAU;
                    let r = if k % 2 == 0 { 300.0 } else { 480.0 };
                    let Some(h) = self.field.height(me[0] + a.cos() * r, me[2] + a.sin() * r) else { continue };
                    let spot = [me[0] + a.cos() * r, h, me[2] + a.sin() * r];
                    let edge = (hdist(spot, self.field.center) / self.field.radius - 0.75).max(0.0) * 4.0;
                    let v = self.danger(spot, seen) * 10.0 + edge + r / 1000.0;
                    if v < best.0 {
                        best = (v, spot);
                    }
                }
                self.flee[p] = Some(best.1);
            } else if self.danger(me, seen) == 0.0 {
                self.flee[p] = None;
            }
        }
        match self.flee[p] {
            Some(to) => {
                let mut d = run_to(me, to, 30.0);
                d.mag = d.mag.max(0.7) * (0.85 + 0.15 * skill);
                d
            }
            // Idle: drift toward the middle of the field.
            None if hdist(me, self.field.center) > self.field.radius * 0.5 => {
                let mut d = run_to(me, self.field.center, 0.0);
                d.mag = 0.5;
                d
            }
            None => Drive::default(),
        }
    }

    fn events(&mut self) -> Vec<MiniEv> {
        std::mem::take(&mut self.evs)
    }

    fn done(&self) -> bool {
        let alive = self.alive().count();
        self.t >= SECS || (self.r.count() > 1 && alive <= 1) || alive == 0
    }

    fn time_left(&self) -> f32 {
        (SECS - self.t).max(0.0)
    }

    fn scores(&self) -> [f32; 4] {
        // The last one standing keeps the clock to the end.
        let end = if self.done() { SECS } else { self.t };
        self.r.mask(std::array::from_fn(|p| self.out_at[p].unwrap_or(end) + 2.0 * self.hearts[p] as f32))
    }

    fn out(&self, p: usize) -> bool {
        self.out_at[p].is_some()
    }

    fn scene(&self, out: &mut Stage) {
        for b in &self.bombs {
            let k = (b.age / b.fuse).min(1.0);
            let y = self.field.height(b.pos[0], b.pos[2]).unwrap_or(b.pos[1]);
            // The ring grows to the blast size; the Bob-omb drops from the sky onto it.
            out.shape_w(Sh::Disc, [b.pos[0], y + 3.0, b.pos[2]], [(BLAST / 100.0) as f32 * (0.35 + 0.65 * k), 0.05, 0.0], if k > 0.75 { 0xff2a1a } else { 0xc0402a });
            out.bombs.push([b.pos[0], y + 1400.0 * (1.0 - k as f64).powi(2) + 60.0, b.pos[2]]);
        }
        for (at, age) in &self.blasts {
            let s = (BLAST / 100.0) as f32 * (0.6 + age * 1.2);
            out.shape_w(Sh::Ball, [at[0], at[1] + 80.0, at[2]], [s * (1.0 - age * 1.6).max(0.05), 0.0, 0.0], 0xffc040);
        }
        for p in self.r.ids() {
            out.tags[p] = if self.out_at[p].is_some() { "OUT".into() } else { "♥".repeat(self.hearts[p] as usize).replace('♥', "*") };
        }
        if self.t < 1.2 {
            out.banner = "DODGE THE BOB-OMBS!".into();
        } else if self.time_left() < 5.0 {
            out.banner = format!("{}", self.time_left().ceil() as i32);
        }
    }
}
