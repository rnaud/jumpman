//! A mini-game's arena: the standable floor of one real spot of the course, sampled once on a grid ([`Field`]), and
//! [`Kin`], the kinematic bodies that stand in for the physics in tests and in the board's headless mode.
use super::{hdist, Body, Drive, MiniEv};
use crate::experiments::party::common::Rng;
use std::sync::Arc;

/// Height difference between neighbouring cells that still counts as one connected floor.
const STEP_H: f64 = 110.0;

/// A course object no Mario walks through (a tree trunk, a post): a vertical cylinder standing on `pos`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solid {
    pub pos: [f64; 3],
    pub r: f64,
    pub h: f64,
}

/// Mario's own body radius against objects (his hitbox).
pub const BODY_R: f64 = 37.0;

/// Where a body at `pos` must stand to be out of every solid it is inside (SM64 pushes Mario out of an object's cylinder the
/// same way), None when it touches none.
pub fn push_out(solids: &[Solid], pos: [f64; 3]) -> Option<[f64; 3]> {
    let mut q = pos;
    let mut moved = false;
    for s in solids {
        if q[1] < s.pos[1] - 120.0 || q[1] > s.pos[1] + s.h {
            continue;
        }
        let (dx, dz) = (q[0] - s.pos[0], q[2] - s.pos[2]);
        let d = dx.hypot(dz);
        let min = s.r + BODY_R;
        if d < min {
            let (ux, uz) = if d > 0.01 { (dx / d, dz / d) } else { (1.0, 0.0) };
            q = [s.pos[0] + ux * min, q[1], s.pos[2] + uz * min];
            moved = true;
        }
    }
    moved.then_some(q)
}

/// Two Marios never stand inside each other: bodies closer than two body radii within 100 in height (a jump that clears more
/// passes over) are pushed apart along the line between them, half each (a few passes settle a pile). Returns the moves, so
/// the caller can keep each on its floor. `live[p]` false leaves a seat out (fallen, out of the game); a `fixed` body is not
/// moved and the other one takes the whole push (one held at the field's edge or against a trunk).
pub fn separate_bodies(pos: &[[f64; 3]; 4], live: &[bool; 4]) -> [[f64; 2]; 4] {
    separate_bodies_fixed(pos, live, &[false; 4])
}

pub fn separate_bodies_fixed(pos: &[[f64; 3]; 4], live: &[bool; 4], fixed: &[bool; 4]) -> [[f64; 2]; 4] {
    let mut q = *pos;
    let mut moved = [[0.0f64; 2]; 4];
    for _ in 0..3 {
        for a in 0..4 {
            for b in a + 1..4 {
                if !live[a] || !live[b] || fixed[a] && fixed[b] || (q[a][1] - q[b][1]).abs() > 100.0 {
                    continue;
                }
                let (dx, dz) = (q[b][0] - q[a][0], q[b][2] - q[a][2]);
                let d = dx.hypot(dz);
                let min = 2.0 * BODY_R;
                if d >= min {
                    continue;
                }
                let (ux, uz) = if d > 0.01 { (dx / d, dz / d) } else { (if a % 2 == 0 { 1.0 } else { -1.0 }, 0.0) };
                let push = min - d;
                let (ka, kb) = if fixed[a] { (0.0, 1.0) } else if fixed[b] { (1.0, 0.0) } else { (0.5, 0.5) };
                for (k, s) in [(a, -ka), (b, kb)] {
                    q[k][0] += ux * push * s;
                    q[k][2] += uz * push * s;
                    moved[k][0] += ux * push * s;
                    moved[k][1] += uz * push * s;
                }
            }
        }
    }
    moved
}

/// A run direction bent round the solids just ahead of `pos` (a person steers round a tree instead of into it).
pub fn steer_around(solids: &[Solid], pos: [f64; 3], dir: [f64; 2]) -> [f64; 2] {
    let mut out = dir;
    for s in solids {
        if pos[1] < s.pos[1] - 120.0 || pos[1] > s.pos[1] + s.h {
            continue;
        }
        let to = [s.pos[0] - pos[0], s.pos[2] - pos[2]];
        let ahead = to[0] * dir[0] + to[1] * dir[1];
        if ahead <= 0.0 || ahead > 340.0 {
            continue;
        }
        // Positive: the solid is on the left of the run (counter-clockwise of it).
        let lat = dir[0] * to[1] - dir[1] * to[0];
        let clear = s.r + BODY_R + 60.0;
        if lat.abs() >= clear {
            continue;
        }
        let left = [-dir[1], dir[0]];
        let away = if lat > 0.0 { -1.0 } else { 1.0 };
        let k = (clear - lat.abs()) / clear * (1.0 - ahead / 420.0) * 1.8;
        out = [out[0] + left[0] * away * k, out[1] + left[1] * away * k];
    }
    let l = out[0].hypot(out[1]);
    if l < 1e-6 {
        dir
    } else {
        [out[0] / l, out[1] / l]
    }
}

#[derive(Clone, Debug)]
pub struct Field {
    /// Centre of the arena: course x, floor height, z.
    pub center: [f64; 3],
    pub radius: f64,
    pub step: f64,
    n: usize,
    /// Floor height per cell (NaN: no floor of this arena there).
    h: Vec<f32>,
    /// A body below this has fallen off the arena.
    pub low: f64,
    pts: Vec<[f64; 3]>,
    /// The course objects standing in the arena (trees): bodies are pushed out of them and the bots run round them.
    pub solids: Vec<Solid>,
}

impl Field {
    /// A level disc of floor (tests, and a stand-in without game data).
    pub fn flat(center: [f64; 3], radius: f64) -> Self {
        Self::from_heights([center[0], center[2]], radius, 100.0, |_, _| Some(center[1]))
    }

    /// Sample the floor on a grid of `step` around `centre` (x, z) and keep the cells connected to the centre's floor
    /// (a ledge far below or a cliff above is not part of the arena).
    pub fn from_heights(centre: [f64; 2], radius: f64, step: f64, sample: impl Fn(f64, f64) -> Option<f64>) -> Self {
        let n = (radius * 2.0 / step).ceil() as usize + 1;
        let x0 = centre[0] - radius;
        let z0 = centre[1] - radius;
        let raw: Vec<Option<f64>> = (0..n * n)
            .map(|k| {
                let (i, j) = (k % n, k / n);
                let (x, z) = (x0 + i as f64 * step, z0 + j as f64 * step);
                if (x - centre[0]).hypot(z - centre[1]) > radius {
                    return None;
                }
                sample(x, z)
            })
            .collect();
        // Flood fill from the cell nearest the centre that has a floor.
        let mid = (n / 2) * n + n / 2;
        let seed = (0..n * n).filter(|k| raw[*k].is_some()).min_by_key(|k| {
            let (i, j) = ((k % n) as i64 - (mid % n) as i64, (k / n) as i64 - (mid / n) as i64);
            i * i + j * j
        });
        let mut h = vec![f32::NAN; n * n];
        if let Some(s) = seed {
            let mut stack = vec![s];
            h[s] = raw[s].unwrap() as f32;
            while let Some(k) = stack.pop() {
                let (i, j) = ((k % n) as i64, (k / n) as i64);
                for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (a, b) = (i + di, j + dj);
                    if a < 0 || b < 0 || a >= n as i64 || b >= n as i64 {
                        continue;
                    }
                    let q = b as usize * n + a as usize;
                    if !h[q].is_nan() {
                        continue;
                    }
                    if let Some(y) = raw[q] {
                        if (y - h[k] as f64).abs() <= STEP_H {
                            h[q] = y as f32;
                            stack.push(q);
                        }
                    }
                }
            }
        }
        let cy = seed.map_or(0.0, |s| h[s] as f64);
        let lo = h.iter().filter(|v| !v.is_nan()).fold(f64::MAX, |a, v| a.min(*v as f64));
        let mut pts = vec![];
        for k in 0..n * n {
            if !h[k].is_nan() {
                let (x, z) = (x0 + (k % n) as f64 * step, z0 + (k / n) as f64 * step);
                if (x - centre[0]).hypot(z - centre[1]) <= radius * 0.92 {
                    pts.push([x, h[k] as f64, z]);
                }
            }
        }
        Field { center: [centre[0], cy, centre[1]], radius, step, n, h, low: if lo == f64::MAX { cy - 400.0 } else { lo - 400.0 }, pts, solids: vec![] }
    }

    /// The same arena with the solids of `all` that stand in or near it; standable points inside one are dropped (no coin
    /// or start inside a trunk).
    pub fn with_solids(mut self, all: &[Solid]) -> Self {
        self.solids = all.iter().filter(|s| hdist(s.pos, self.center) < self.radius + 300.0 && (s.pos[1] - self.center[1]).abs() < 1500.0).copied().collect();
        let solids = self.solids.clone();
        self.pts.retain(|p| solids.iter().all(|s| hdist(*p, s.pos) > s.r + BODY_R + 30.0));
        self
    }

    /// Floor height of the arena at (x, z), None off the arena.
    pub fn height(&self, x: f64, z: f64) -> Option<f64> {
        let fi = ((x - (self.center[0] - self.radius)) / self.step).round();
        let fj = ((z - (self.center[2] - self.radius)) / self.step).round();
        if fi < 0.0 || fj < 0.0 || fi >= self.n as f64 || fj >= self.n as f64 {
            return None;
        }
        let v = self.h[fj as usize * self.n + fi as usize];
        (!v.is_nan()).then_some(v as f64)
    }

    /// Standable points of the arena (cell centres, not right at the rim).
    pub fn points(&self) -> &[[f64; 3]] {
        &self.pts
    }

    /// The standable point nearest to `p`.
    pub fn nearest(&self, p: [f64; 3]) -> [f64; 3] {
        self.pts.iter().copied().min_by(|a, b| hdist(*a, p).partial_cmp(&hdist(*b, p)).unwrap()).unwrap_or(self.center)
    }

    pub fn rand_point(&self, rng: &mut Rng) -> [f64; 3] {
        if self.pts.is_empty() {
            return self.center;
        }
        self.pts[rng.below(self.pts.len())]
    }

    /// Where seat `p` starts: four spots on a ring around the centre, `frac` of the radius out.
    pub fn start(&self, p: usize, frac: f64) -> [f64; 3] {
        let a = (p as f64 + 0.5) * std::f64::consts::FRAC_PI_2;
        self.nearest([self.center[0] + a.cos() * self.radius * frac, self.center[1], self.center[2] + a.sin() * self.radius * frac])
    }

    /// On the arena's floor (not fallen off, not wandered out).
    pub fn on(&self, pos: [f64; 3]) -> bool {
        self.height(pos[0], pos[2]).is_some_and(|h| pos[1] > h - 250.0)
    }

    pub fn fallen(&self, pos: [f64; 3]) -> bool {
        pos[1] < self.low
    }

    /// Fraction of the disc that is standable floor.
    pub fn coverage(&self) -> f64 {
        let cells = std::f64::consts::PI * (self.radius / self.step).powi(2);
        self.pts.len() as f64 / cells.max(1.0)
    }
}

// -- the kinematic stand-in ----------------------------------------------------------------------

const RUN: f64 = 25.0;
const JUMP_VY: f64 = 42.0;
const GRAVITY: f64 = 4.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct KinBody {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f64,
    pub grounded: bool,
    pub attack: u8,
    pub knock: u8,
}

/// Four point bodies that run, jump, punch and get knocked about on a [`Field`] (no walls, no slopes): enough to play
/// every game's rules and bots headlessly and deterministically.
pub struct Kin {
    pub field: Arc<Field>,
    pub b: [KinBody; 4],
}

impl Kin {
    pub fn new(field: Arc<Field>, starts: [[f64; 3]; 4]) -> Self {
        let b = std::array::from_fn(|p| KinBody { pos: starts[p], grounded: true, ..Default::default() });
        Kin { field, b }
    }

    pub fn bodies(&self) -> [Body; 4] {
        std::array::from_fn(|p| {
            let k = &self.b[p];
            Body { pos: k.pos, yaw: k.yaw, grounded: k.grounded, vy: k.vel[1], attack: k.attack > 0 }
        })
    }

    pub fn step(&mut self, drives: &[Drive; 4]) {
        for (k, d) in self.b.iter_mut().zip(drives) {
            k.attack = k.attack.saturating_sub(1);
            if k.knock > 0 {
                k.knock -= 1;
                k.vel[0] *= 0.9;
                k.vel[2] *= 0.9;
            } else {
                match d.dir {
                    Some(dir) => {
                        let v = RUN * d.mag.clamp(0.0, 1.0);
                        k.vel[0] = dir[0] * v;
                        k.vel[2] = dir[1] * v;
                        k.yaw = dir[0].atan2(dir[1]) / std::f64::consts::PI * 32768.0;
                    }
                    None => {
                        k.vel[0] = 0.0;
                        k.vel[2] = 0.0;
                    }
                }
                if d.jump && k.grounded {
                    k.vel[1] = JUMP_VY;
                    k.grounded = false;
                }
                if d.punch && k.attack == 0 {
                    k.attack = 9;
                }
            }
            k.pos[0] += k.vel[0];
            k.pos[2] += k.vel[2];
            if let Some(q) = push_out(&self.field.solids, k.pos) {
                k.pos = q;
            }
        }
        for p in 0..4 {
            let k = &mut self.b[p];
            let floor = self.field.height(k.pos[0], k.pos[2]);
            if k.grounded {
                match floor {
                    Some(h) if (h - k.pos[1]).abs() < 120.0 => k.pos[1] = h,
                    _ => {
                        k.grounded = false;
                        k.vel[1] = 0.0;
                    }
                }
            } else {
                k.vel[1] -= GRAVITY;
                k.pos[1] += k.vel[1];
                if let Some(h) = floor {
                    if k.vel[1] <= 0.0 && k.pos[1] <= h && k.pos[1] > h - 150.0 {
                        k.pos[1] = h;
                        k.vel[1] = 0.0;
                        k.grounded = true;
                    }
                }
                // Far below the arena the body just waits to be picked up.
                k.pos[1] = k.pos[1].max(self.field.low - 3000.0);
            }
        }
        // Bodies do not pass through each other (onto standable floor only, never into a trunk).
        // A body whose push would leave the floor or enter a trunk stays, and the other takes the whole push.
        let pos: [[f64; 3]; 4] = std::array::from_fn(|p| self.b[p].pos);
        let ok = |p: usize, m: [f64; 2]| {
            let q = [pos[p][0] + m[0], pos[p][1], pos[p][2] + m[1]];
            self.field.height(q[0], q[2]).is_some() && push_out(&self.field.solids, q).is_none()
        };
        let first = separate_bodies(&pos, &[true; 4]);
        let fixed: [bool; 4] = std::array::from_fn(|p| first[p] != [0.0, 0.0] && !ok(p, first[p]));
        let moved = if fixed.iter().any(|f| *f) { separate_bodies_fixed(&pos, &[true; 4], &fixed) } else { first };
        for p in 0..4 {
            if moved[p] != [0.0, 0.0] && ok(p, moved[p]) {
                self.b[p].pos = [pos[p][0] + moved[p][0], pos[p][1], pos[p][2] + moved[p][1]];
            }
        }
    }

    pub fn apply(&mut self, ev: &MiniEv) {
        match *ev {
            MiniEv::Knock { p, from, power } => {
                let k = &mut self.b[p];
                let d = super::toward(from, k.pos).unwrap_or([1.0, 0.0]);
                k.vel = [d[0] * power, 26.0, d[1] * power];
                k.grounded = false;
                k.knock = 14;
            }
            MiniEv::Place { p, pos } => {
                self.b[p] = KinBody { pos, grounded: true, yaw: self.b[p].yaw, ..Default::default() };
            }
            MiniEv::Bounce { p } => {
                let k = &mut self.b[p];
                k.vel[1] = 38.0;
                k.grounded = false;
            }
            MiniEv::Cue { .. } => {}
        }
    }
}
