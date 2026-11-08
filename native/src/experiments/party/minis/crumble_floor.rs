//! Crumble Floor: tiles flash and drop away. Keep moving onto solid floor; last one standing wins.
use crate::experiments::party::common::*;

const LIMIT: f32 = 28.0;
const COLS: usize = 8;
const ROWS: usize = 4;
const CELL: f32 = 2.2;
const SPEED: f32 = 4.6;
const WARN: f32 = 1.0;

#[derive(Clone, Copy, PartialEq)]
enum Tile {
    Solid,
    Warn(f32),
    Gone,
}

pub struct CrumbleFloor {
    r: Roster,
    t: f32,
    tiles: [[Tile; COLS]; ROWS],
    pos: [[f32; 2]; 4],
    face: [f32; 4],
    alive: [bool; 4],
    out_at: [f32; 4],
    next: f32,
    rng: Rng,
    brng: Rng,
}

fn cell_of(x: f32, z: f32) -> Option<(usize, usize)> {
    let cx = ((x + CELL * COLS as f32 / 2.0) / CELL).floor();
    let cz = ((z + CELL * ROWS as f32 / 2.0) / CELL).floor();
    if cx < 0.0 || cz < 0.0 || cx >= COLS as f32 || cz >= ROWS as f32 {
        None
    } else {
        Some((cz as usize, cx as usize))
    }
}

fn centre(r: usize, c: usize) -> (f32, f32) {
    (-CELL * COLS as f32 / 2.0 + CELL * (c as f32 + 0.5), -CELL * ROWS as f32 / 2.0 + CELL * (r as f32 + 0.5))
}

impl CrumbleFloor {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        CrumbleFloor { r: Roster::new(active), t: 0.0, tiles: [[Tile::Solid; COLS]; ROWS], pos: [[-6.0, -2.0], [-2.0, 2.0], [2.0, -2.0], [6.0, 2.0]], face: [0.0; 4], alive: active, out_at: [LIMIT; 4], next: 1.5, rng: Rng::new(seed), brng: Rng::new(seed ^ 0xc7b) }
    }
    fn solid_at(&self, x: f32, z: f32) -> bool {
        cell_of(x, z).is_some_and(|(r, c)| self.tiles[r][c] != Tile::Gone)
    }
}

impl MiniGame for CrumbleFloor {
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
            self.pos[p][0] += x * SPEED * dt;
            self.pos[p][1] += y * SPEED * dt;
            if !self.solid_at(self.pos[p][0], self.pos[p][1]) {
                self.alive[p] = false;
                self.out_at[p] = self.t;
            }
        }
        for row in &mut self.tiles {
            for t in row.iter_mut() {
                if let Tile::Warn(w) = *t {
                    *t = if w - dt <= 0.0 { Tile::Gone } else { Tile::Warn(w - dt) };
                }
            }
        }
        self.next -= dt;
        if self.next <= 0.0 {
            let n = 1 + (self.t / 7.0) as usize;
            for _ in 0..n {
                // Prefer tiles with a player on them, so camping does not work.
                let mut c = (self.rng.below(ROWS), self.rng.below(COLS));
                let me: Vec<usize> = self.r.ids().filter(|&p| self.alive[p]).collect();
                if !me.is_empty() && self.rng.chance(0.5) {
                    let p = me[self.rng.below(me.len())];
                    if let Some(cc) = cell_of(self.pos[p][0], self.pos[p][1]) {
                        c = cc;
                    }
                }
                if self.tiles[c.0][c.1] == Tile::Solid {
                    self.tiles[c.0][c.1] = Tile::Warn(WARN);
                }
            }
            self.next = (1.1 - self.t * 0.025).max(0.35);
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        if !self.alive[p] {
            return PadIn::default();
        }
        let me = self.pos[p];
        let here = cell_of(me[0], me[1]);
        // A bot only notices a warning after its reaction time has passed.
        let here_bad = here.map_or(true, |(r, c)| match self.tiles[r][c] {
            Tile::Solid => false,
            Tile::Warn(w) => WARN - w >= lvl.react() * 1.1,
            Tile::Gone => true,
        });
        if !here_bad && self.brng.chance(0.9 + 0.1 * lvl.skill()) {
            // Safe: stay (a little drift to the middle).
            return PadIn { x: clamp(-me[0] * 0.05, -0.2, 0.2), y: clamp(-me[1] * 0.05, -0.2, 0.2), ..Default::default() };
        }
        // Head for the nearest solid tile, preferring ones with solid neighbours.
        let mut best: Option<(f32, f32, f32)> = None;
        for r in 0..ROWS {
            for c in 0..COLS {
                if self.tiles[r][c] != Tile::Solid {
                    continue;
                }
                let (cx, cz) = centre(r, c);
                let mut score = dist(me[0], me[1], cx, cz);
                let nb = [(r.wrapping_sub(1), c), (r + 1, c), (r, c.wrapping_sub(1)), (r, c + 1)].iter().filter(|&&(rr, cc)| rr < ROWS && cc < COLS && self.tiles[rr][cc] == Tile::Solid).count();
                score += (4 - nb) as f32 * 0.6 * lvl.skill();
                if best.map_or(true, |b| score < b.0) {
                    best = Some((score, cx, cz));
                }
            }
        }
        let Some((_, tx, tz)) = best else { return PadIn::default() };
        let (dx, dz) = (tx - me[0], tz - me[1]);
        let m = (dx * dx + dz * dz).sqrt().max(1e-3);
        PadIn { x: dx / m, y: dz / m, ..Default::default() }
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
        for r in 0..ROWS {
            for c in 0..COLS {
                let (x, z) = centre(r, c);
                match self.tiles[r][c] {
                    Tile::Gone => {}
                    Tile::Solid => out.shape(Sh::Box, [x, -0.1, z], [CELL - 0.08, 0.2, CELL - 0.08], if (r + c) % 2 == 0 { 0x5fa0d8 } else { 0x4a8cc4 }),
                    Tile::Warn(w) => {
                        let shake = (self.t * 40.0).sin() * 0.04 * (1.0 - w / WARN);
                        out.shape(Sh::Box, [x + shake, -0.12, z], [CELL - 0.12, 0.2, CELL - 0.12], if (self.t * 10.0) as i32 % 2 == 0 { 0xff7a3a } else { 0xffd23a });
                    }
                }
            }
        }
        for p in self.r.ids() {
            out.avatar(p, self.pos[p][0], self.pos[p][1], 0.0, self.face[p], !self.alive[p]);
            out.tags[p] = if self.alive[p] { "alive".into() } else { "OUT".into() };
        }
    }
}
