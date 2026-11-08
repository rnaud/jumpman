//! Shell Lane: run down your lane, swapping between the two rows to dodge spiny rocks and catch speed pads.
use crate::experiments::party::common::*;

const LIMIT: f32 = 20.0;
const TRACK: f32 = 78.0;

#[derive(Clone, Copy)]
struct Obj {
    d: f32,
    row: u8,
    pad: bool,
}

pub struct ShellLane {
    r: Roster,
    t: f32,
    dist: [f32; 4],
    row: [u8; 4],
    stun: [f32; 4],
    boost: [f32; 4],
    prev_y: [f32; 4],
    objs: Vec<Obj>,
    done_at: [f32; 4],
    brng: Rng,
    thought: [f32; 4],
}

impl ShellLane {
    pub fn new(seed: u64, active: [bool; 4]) -> Self {
        let mut rng = Rng::new(seed);
        let mut objs = vec![];
        let mut d = 9.0;
        while d < TRACK - 6.0 {
            let row = rng.below(2) as u8;
            let pad = rng.chance(0.28);
            objs.push(Obj { d, row, pad });
            if rng.chance(0.35) {
                objs.push(Obj { d: d + 0.1, row: 1 - row, pad: !pad && rng.chance(0.5) });
            }
            d += rng.range(4.0, 7.0);
        }
        ShellLane { r: Roster::new(active), t: 0.0, dist: [0.0; 4], row: [0; 4], stun: [0.0; 4], boost: [0.0; 4], prev_y: [0.0; 4], objs, done_at: [LIMIT; 4], brng: Rng::new(seed ^ 0x5e1), thought: [0.0; 4] }
    }
    fn speed(&self, p: usize) -> f32 {
        if self.stun[p] > 0.0 {
            1.5
        } else if self.boost[p] > 0.0 {
            6.4
        } else {
            4.2
        }
    }
}

impl MiniGame for ShellLane {
    fn step(&mut self, dt: f32, inp: &Inputs) {
        self.t += dt;
        for p in self.r.ids().collect::<Vec<_>>() {
            if self.dist[p] >= TRACK {
                continue;
            }
            // Swap rows on stick edge (up -> row 0, down -> row 1) or action.
            let y = inp[p].y;
            if y < -0.5 && self.prev_y[p] >= -0.5 {
                self.row[p] = 0;
            } else if y > 0.5 && self.prev_y[p] <= 0.5 {
                self.row[p] = 1;
            }
            if inp[p].act {
                self.row[p] = 1 - self.row[p];
            }
            self.prev_y[p] = y;
            self.stun[p] = (self.stun[p] - dt).max(0.0);
            self.boost[p] = (self.boost[p] - dt).max(0.0);
            let before = self.dist[p];
            self.dist[p] += self.speed(p) * dt;
            for o in &self.objs {
                if o.row == self.row[p] && o.d > before && o.d <= self.dist[p] {
                    if o.pad {
                        self.boost[p] = 1.4;
                    } else {
                        self.stun[p] = 0.9;
                        self.boost[p] = 0.0;
                    }
                }
            }
            if self.dist[p] >= TRACK {
                self.done_at[p] = self.t;
            }
        }
    }
    fn bot(&mut self, p: usize, lvl: Level) -> PadIn {
        // Look a few metres ahead; the look-ahead and the reaction budget shrink on Easy.
        let ahead = 0.8 + lvl.skill() * 3.2;
        let me = self.dist[p];
        let row = self.row[p];
        let danger = |r: u8, objs: &Vec<Obj>| objs.iter().any(|o| o.row == r && !o.pad && o.d > me && o.d < me + ahead);
        let prize = |r: u8, objs: &Vec<Obj>| objs.iter().any(|o| o.row == r && o.pad && o.d > me && o.d < me + ahead);
        self.thought[p] -= 1.0 / 30.0;
        if self.thought[p] > 0.0 {
            return PadIn::default();
        }
        let want_swap = (danger(row, &self.objs) && !danger(1 - row, &self.objs)) || (!prize(row, &self.objs) && prize(1 - row, &self.objs) && !danger(1 - row, &self.objs));
        if want_swap && self.brng.chance(0.3 + 0.7 * lvl.skill()) {
            self.thought[p] = lvl.react() * 0.8;
            return PadIn { y: if row == 0 { 1.0 } else { -1.0 }, ..Default::default() };
        }
        PadIn::default()
    }
    fn done(&self) -> bool {
        self.t >= LIMIT || self.r.ids().all(|p| self.dist[p] >= TRACK)
    }
    fn time_left(&self) -> f32 {
        (LIMIT - self.t).max(0.0)
    }
    fn scores(&self) -> [f32; 4] {
        let mut s = [0.0; 4];
        for p in 0..4 {
            s[p] = if self.dist[p] >= TRACK { 1000.0 - self.done_at[p] * 10.0 } else { self.dist[p] };
        }
        self.r.mask(s)
    }
    fn scene(&self, out: &mut Stage) {
        // The camera sees a 18-unit window; scale the 78 m track to the lane width.
        let k = 16.0 / TRACK;
        for p in self.r.ids() {
            let z = lane_z(p);
            out.shape(Sh::Box, [0.0, -0.05, z], [17.5, 0.1, 2.0], 0x76757f);
            out.shape(Sh::Box, [0.0, 0.0, z], [17.5, 0.02, 0.06], 0xe8e8e8);
            out.shape(Sh::Box, [8.2, 0.02, z], [0.2, 0.04, 2.0], 0xffffff);
            for o in &self.objs {
                let x = -8.0 + o.d * k;
                let oz = z + if o.row == 0 { -0.5 } else { 0.5 };
                if o.pad {
                    out.shape(Sh::Box, [x, 0.02, oz], [0.5, 0.05, 0.6], 0xffd23a);
                } else {
                    out.shape(Sh::Cone, [x, 0.3, oz], [0.3, 0.6, 0.0], 0x8a3a2a);
                }
            }
            let oz = z + if self.row[p] == 0 { -0.5 } else { 0.5 };
            out.avatar(p, -8.0 + self.dist[p].min(TRACK) * k, oz, 0.0, 1.5708, false);
            out.tags[p] = format!("{}%", (self.dist[p] / TRACK * 100.0).min(100.0) as u32);
        }
    }
}
