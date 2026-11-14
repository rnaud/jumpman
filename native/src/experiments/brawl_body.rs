//! Gem Clash's solid bodies: fighters, roaming monsters and course set pieces never stand inside each other. A child module
//! of `brawl_core` run at the end of every world step, after everything has moved and struck (so this tick's contacts,
//! bites and lunges have already landed), engine-free and deterministic (fixed order, no randomness; the positions it
//! changes are in [`Brawl::digest`]).
//!
//! * Fighter vs fighter: an overlapping pair is pushed apart, the lighter one taking more of it; nobody is shoved off a
//!   ledge by a body (the other one gives way instead), walls still win.
//! * Fighter vs monster: a monster that walks into a fighter, or a fighter who walks into a monster, ends at body contact.
//!   Light monsters give way (shoved over valid ground only), rooted ones (shooters, Sir Slush, the Deku Baba, the Bigbutt)
//!   do not; the fighter is always put outside. Flyers mid-swoop pass through (their attack dives through by design).
//! * Fighter vs course hazard: set pieces are immovable, the fighter is put outside their body (see `hz_body` for which are
//!   solid: Boos, the Tweester and waves stay intangible by design). A roller pushes sideways out of its lane.
//!
//! Every body is a little smaller than its contact reach ([`SQUEEZE`]), so a fighter at body contact is still touched,
//! still bitten and still lands every melee move. A body more than 70% cleared by a jump does not block (stomping the giant
//! Goomba, hopping a low roller still work).
use super::*;
use crate::experiments::body::{self as bd, SQUEEZE};

/// Fraction of a body's height a jump must clear before it stops blocking (the top 30%: landing on it from above).
const CLEAR: f32 = 0.3;

/// A non-fighter body in this tick's pass.
#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub c: V2,
    pub base: f32,
    pub height: f32,
    pub radius: f32,
    /// Monster slot that gives way when shoved (`None`: immovable).
    pub shove: Option<usize>,
    /// A roller's lane: the fighter leaves sideways.
    pub lane: Option<V2>,
}

impl Brawl {
    /// Every monster and course hazard with a solid body right now (fixed order: monsters, then hazards).
    pub fn solids(&self) -> ([Option<Solid>; MAXM + MAXH], usize) {
        let mut out = [None; MAXM + MAXH];
        let mut n = 0;
        if self.cfg.monsters != Monsters::Off {
            for m in 0..MAXM {
                if let Some((c, base, height, radius, rooted)) = self.mon_body(m) {
                    out[n] = Some(Solid { c, base, height, radius, shove: (!rooted).then_some(m), lane: None });
                    n += 1;
                }
            }
        }
        for h in 0..self.arena.hazards.len().min(MAXH) {
            if let Some((c, base, height, radius, lane)) = self.hz_body(h) {
                out[n] = Some(Solid { c, base, height, radius, shove: None, lane });
                n += 1;
            }
        }
        (out, n)
    }

    /// Does fighter `i` stand at the height of a body `[base, base + height]`?
    fn level_with(&self, i: usize, base: f32, height: f32) -> bool {
        let f = &self.f[i];
        bd::stacked(f.pos[1], f.height(), base, height, CLEAR)
    }

    /// Move fighter `i` by `d` on the ground plane, then out of the walls. A grounded fighter is not moved over a drop unless
    /// `force` (an immovable body leaves no choice), sliding around the `avoid` bodies rather than into them. Returns whether it moved.
    fn body_move(&mut self, i: usize, d: V2, force: bool, avoid: &[Option<Solid>]) -> bool {
        let r = self.f[i].radius();
        let mut p = self.f[i].pos;
        p[0] += d[0];
        p[2] += d[1];
        if !force && self.f[i].grounded {
            let floor = self.arena.support(p[0], p[2], p[1] + STEP).map(|s| s.0);
            if floor.is_none_or(|y| y < p[1] - STEP) {
                return false;
            }
        }
        self.arena.push_out(&mut p, r);
        // Slide around the solid bodies in the way instead of into them.
        if avoid.iter().flatten().any(|s| bd::stacked(p[1], self.f[i].height(), s.base, s.height, CLEAR)) {
            let mut q = [p[0], p[2]];
            for s in avoid.iter().flatten().filter(|s| bd::stacked(p[1], self.f[i].height(), s.base, s.height, CLEAR)) {
                q = bd::push_out(q, s.c, (s.radius + r) * SQUEEZE, bd::away(s.c, pos_of(&self.f[i]), [1.0, 0.0]));
            }
            p[0] = q[0];
            p[2] = q[1];
            self.arena.push_out(&mut p, r);
        }
        self.f[i].pos = p;
        true
    }

    /// Fighters against each other: an overlapping pair is pushed apart by weight; a side that cannot give way (a ledge or,
    /// with `avoid`, a solid body behind it) leaves the whole push to the other.
    fn separate_fighters(&mut self, avoid: &[Option<Solid>]) -> bool {
        let mut any = false;
        for a in 0..self.n {
            for b in a + 1..self.n {
                let (fa, fb) = (&self.f[a], &self.f[b]);
                if !fa.alive || !fb.alive || !(self.level_with(a, fb.pos[1], fb.height()) && self.level_with(b, fa.pos[1], fa.height())) {
                    continue;
                }
                let reach = (fa.radius() + fb.radius()) * SQUEEZE;
                let (wa, wb) = (fa.stats().weight.max(0.1), fb.stats().weight.max(0.1));
                let fallback = if a % 2 == 0 { [1.0, 0.0] } else { [0.0, 1.0] };
                let Some((da, db)) = bd::separate(pos_of(fa), pos_of(fb), reach, wb / (wa + wb), fallback) else { continue };
                any = true;
                self.body_move(a, da, false, avoid);
                self.body_move(b, db, false, avoid);
                // Whatever one side could not take (a wall, the rim or a solid behind it), the other takes.
                for (me, other) in [(a, b), (b, a)] {
                    if let Some((d, _)) = bd::separate(pos_of(&self.f[me]), pos_of(&self.f[other]), reach, 1.0, fallback) {
                        self.body_move(me, d, false, avoid);
                    }
                }
            }
        }
        any
    }

    /// Push every overlapping pair of bodies apart (see the module docs). Run at the end of the world step. A few rounds of
    /// relaxation, stopping as soon as nothing overlaps (one round in the usual case): fighters apart (sliding around the
    /// solid bodies), then each fighter out of the monsters (a light one gives way, 60%) and set pieces. A pile-up (four
    /// fighters jammed against the rim, a monster squeezed between two fighters) settles over the rounds.
    pub(super) fn step_bodies(&mut self) {
        for round in 0..12 {
            let (solids, n) = self.solids();
            let mut any = self.separate_fighters(if round == 0 { &[] } else { &solids[..n] });
            for s in solids[..n].iter().flatten() {
                for i in 0..self.n {
                    if !self.f[i].alive || !self.level_with(i, s.base, s.height) {
                        continue;
                    }
                    let reach = (s.radius + self.f[i].radius()) * SQUEEZE;
                    // A monster may have been shoved by a fighter before: its body is where it is now.
                    let mut c = s.shove.map_or(s.c, |m| [self.mons[m].pos[0], self.mons[m].pos[2]]);
                    let p = pos_of(&self.f[i]);
                    if bd::dist(p, c) >= reach - 0.01 {
                        continue;
                    }
                    any = true;
                    if let Some(m) = s.shove {
                        if let Some((_, dm)) = bd::separate(p, c, reach, 0.4, [1.0, 0.0]) {
                            self.mon_shove(m, dm);
                            c = [self.mons[m].pos[0], self.mons[m].pos[2]];
                        }
                    }
                    self.put_outside(i, c, reach, s.lane, &solids[..n]);
                }
            }
            if !any {
                break;
            }
        }
    }

    /// How deep fighter `i` (at its current spot) is inside the deepest of `solids` it stands level with.
    fn worst_in(&self, i: usize, solids: &[Option<Solid>]) -> f32 {
        let (p, r) = (pos_of(&self.f[i]), self.f[i].radius());
        solids
            .iter()
            .flatten()
            .filter(|s| self.level_with(i, s.base, s.height))
            .map(|s| {
                let c = s.shove.map_or(s.c, |m| [self.mons[m].pos[0], self.mons[m].pos[2]]);
                bd::overlap(p, c, (s.radius + r) * SQUEEZE)
            })
            .fold(0.0, f32::max)
    }

    /// Put fighter `i` outside a body at `c` (solid reach `reach`). The way out is straight away from the centre (sideways
    /// out of a roller's `lane`). If a wall, the rim or another of the `solids` is in the way, the other ways round are
    /// tried in turn (the far side of the lane, then around the body, then a step further out), so a fighter pinned against
    /// the rim or boxed in by set pieces slips out where there is room; with no room anywhere, the least-overlapping spot.
    fn put_outside(&mut self, i: usize, c: V2, reach: f32, lane: Option<V2>, solids: &[Option<Solid>]) {
        let p = pos_of(&self.f[i]);
        if bd::dist(p, c) >= reach {
            return;
        }
        let q = [p[0] - c[0], p[1] - c[1]];
        let first = bd::away(c, p, bd::away(c, [0.0, 0.0], [1.0, 0.0]));
        let mut ways = [[0.0f32; 2]; 20];
        let mut k = 0;
        if let Some(dir) = lane {
            let along = q[0] * dir[0] + q[1] * dir[1];
            let side = q[0] * dir[1] - q[1] * dir[0];
            let n = if side >= 0.0 { [dir[1], -dir[0]] } else { [-dir[1], dir[0]] };
            let off = (reach * reach - along * along).max(0.0).sqrt();
            ways[0] = [dir[0] * along + n[0] * off, dir[1] * along + n[1] * off];
            ways[1] = [dir[0] * along - n[0] * off, dir[1] * along - n[1] * off];
            k = 2;
        }
        for far in [1.0f32, 1.6] {
            for a in [0.0f32, 1.0, -1.0, 2.0, -2.0, 3.0] {
                let (sn, cs) = (a * std::f32::consts::FRAC_PI_3).sin_cos();
                ways[k] = [(first[0] * cs - first[1] * sn) * reach * far, (first[0] * sn + first[1] * cs) * reach * far];
                k += 1;
            }
        }
        let save = self.f[i].pos;
        let mut best = (f32::INFINITY, save);
        for w in &ways[..k] {
            let to = [c[0] + w[0] * 1.002, c[1] + w[1] * 1.002];
            self.f[i].pos = save;
            self.body_move(i, [to[0] - p[0], to[1] - p[1]], true, &[]);
            let here = bd::overlap(pos_of(&self.f[i]), c, reach).max(self.worst_in(i, solids));
            if here < 0.5 {
                return;
            }
            if here < best.0 {
                best = (here, self.f[i].pos);
            }
        }
        self.f[i].pos = best.1;
    }

    /// Test / audit hook: the deepest any two solid bodies (fighter-fighter, fighter-monster, fighter-hazard) overlap right
    /// now beyond their solid reach, in units, with what overlapped.
    pub fn body_overlap(&self) -> (f32, String) {
        let mut worst = (0.0f32, String::new());
        for a in 0..self.n {
            for b in a + 1..self.n {
                let (fa, fb) = (&self.f[a], &self.f[b]);
                if !fa.alive || !fb.alive || !(self.level_with(a, fb.pos[1], fb.height()) && self.level_with(b, fa.pos[1], fa.height())) {
                    continue;
                }
                let o = bd::overlap(pos_of(fa), pos_of(fb), (fa.radius() + fb.radius()) * SQUEEZE);
                if o > worst.0 {
                    worst = (o, format!("fighters {a} and {b}"));
                }
            }
        }
        let (solids, n) = self.solids();
        for (k, s) in solids[..n].iter().flatten().enumerate() {
            for i in 0..self.n {
                if !self.f[i].alive || !self.level_with(i, s.base, s.height) {
                    continue;
                }
                let o = bd::overlap(pos_of(&self.f[i]), s.c, (s.radius + self.f[i].radius()) * SQUEEZE);
                if o > worst.0 {
                    worst = (o, format!("fighter {i} and solid {k} ({s:?})"));
                }
            }
        }
        worst
    }
}
