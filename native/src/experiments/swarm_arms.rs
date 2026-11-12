//! Star Swarm weapons that came with the slot system (see `SWARM_WEAPONS.md`): the sword swing, the
//! flame trail, fireballs, the frost ring, poison flasks, the black hole and homing missiles, plus the
//! status effects they share (burn, poison, freeze) and the ground zones some of them leave behind.
//!
//! A child module of `swarm_core`: it works on the same private slot arrays, never allocates in a tick,
//! and draws every random number from the core's seeded [`Rng`].

use super::*;

/// Ground effects alive at once (flame trail, poison clouds, black holes).
pub const ZONE_SLOTS: usize = 40;
/// A burn or a poison deals damage this often (ticks).
const STATUS_PERIOD: u32 = 6;
/// Frozen foes take this much more damage from everything (the shatter synergy).
pub const SHATTER: f32 = 1.4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ZoneKind {
    Fire,
    Poison,
    Gravity,
}

#[derive(Clone, Copy, Debug)]
pub struct Zone {
    pub alive: bool,
    pub kind: ZoneKind,
    pub pos: [f32; 2],
    pub radius: f32,
    pub age: u16,
    pub life: u16,
    /// Damage per status tick (fire, poison) or per pulse (gravity).
    power: f32,
    /// Gravity: units per tick a foe is dragged; the implosion's damage.
    pull: f32,
    boom: f32,
}

impl Zone {
    pub const EMPTY: Zone = Zone { alive: false, kind: ZoneKind::Fire, pos: [0.0; 2], radius: 0.0, age: 0, life: 1, power: 0.0, pull: 0.0, boom: 0.0 };
    /// 0..1 across the zone's life (for the renderer's fade).
    pub fn fade(&self) -> f32 {
        let left = 1.0 - self.age as f32 / self.life.max(1) as f32;
        left.clamp(0.0, 1.0).min(self.age as f32 / 6.0 + 0.2).min(1.0)
    }
}

/// What a bolt is: the star shot, a fireball, a homing missile or a sword beam.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BoltKind {
    Star = 0,
    Fire = 1,
    Missile = 2,
    Beam = 3,
}

/// Everything a new bolt needs.
#[derive(Clone, Copy, Debug)]
pub struct BoltSpec {
    pub kind: BoltKind,
    pub dmg: f32,
    pub speed: f32,
    pub life: u16,
    pub pierce: u8,
    pub radius: f32,
    pub blast: f32,
    pub crit: bool,
    pub aim: u16,
}

impl Swarm {
    fn lvl(&self, u: Upgrade) -> f32 {
        self.up.level(u) as f32
    }

    /// The live foe nearest to `p` within `max` units.
    fn nearest_foe(&self, p: [f32; 2], max: f32) -> Option<usize> {
        let mut best = (max * max, usize::MAX);
        for &i in &self.alive_idx {
            let e = &self.enemies[i as usize];
            if !e.alive {
                continue;
            }
            let d = dist2(e.pos, p);
            if d < best.0 {
                best = (d, i as usize);
            }
        }
        (best.1 != usize::MAX).then_some(best.1)
    }

    pub(super) fn spawn_bolt(&mut self, from: [f32; 2], angle: f32, s: BoltSpec) {
        if let Some(b) = self.bolts.iter_mut().find(|b| !b.alive) {
            *b = Bolt { alive: true, pos: from, prev: from, vel: [angle.cos() * s.speed, angle.sin() * s.speed], life: s.life, dmg: s.dmg, pierce: s.pierce, radius: s.radius, crit: s.crit, kind: s.kind, blast: s.blast, aim: s.aim, recent: [u16::MAX; 3] };
        }
    }

    fn add_zone(&mut self, kind: ZoneKind, pos: [f32; 2], radius: f32, life: u16, power: f32, pull: f32, boom: f32) {
        // A full table recycles the zone closest to its end.
        let slot = match self.zones.iter().position(|z| !z.alive) {
            Some(i) => i,
            None => {
                let mut best = (f32::MAX, 0);
                for (i, z) in self.zones.iter().enumerate() {
                    let left = z.life as f32 - z.age as f32;
                    if left < best.0 {
                        best = (left, i);
                    }
                }
                best.1
            }
        };
        self.zones[slot] = Zone { alive: true, kind, pos, radius, age: 0, life, power, pull, boom };
    }

    /// Every new weapon's firing, once a tick.
    pub(super) fn attack_arms(&mut self) {
        if self.stats.sword > 0 {
            self.sword();
        }
        if self.up.level(Upgrade::Flame) > 0 {
            self.flame();
        }
        if self.up.level(Upgrade::Fireball) > 0 {
            self.fireball();
        }
        if self.up.level(Upgrade::Frost) > 0 {
            self.frost();
        }
        if self.up.level(Upgrade::Poison) > 0 {
            self.flask();
        }
        if self.up.level(Upgrade::Gravity) > 0 {
            self.black_hole();
        }
        if self.up.level(Upgrade::Missiles) > 0 {
            self.missiles();
        }
    }

    // ---- sword -----------------------------------------------------------------------------

    /// The sword: swings at the nearest foe in a wide arc and cuts everything in it; every third swing is a
    /// full spin. From level 4 the swing also cuts behind you; Master Sword throws a piercing beam on every swing.
    fn sword(&mut self) {
        self.sword_cd = self.sword_cd.saturating_sub(1);
        if self.sword_cd > 0 {
            return;
        }
        let l = self.lvl(Upgrade::Sword);
        let p = self.player.pos;
        let reach = (430.0 + 40.0 * l) * self.stats.area.sqrt();
        let Some(t) = self.nearest_foe(p, reach + 450.0) else {
            self.sword_cd = 3;
            return;
        };
        let tp = self.enemies[t].pos;
        let dir = (tp[1] - p[1]).atan2(tp[0] - p[0]);
        self.sword_n += 1;
        let spin = self.sword_n % 3 == 0;
        self.sword_cd = (((38.0 - 3.0 * (l - 1.0)) * self.stats.cd) as u32).max(9);
        let crit = self.rng.chance(self.stats.crit);
        let mut dmg = self.stats.damage * (2.0 + 0.4 * (l - 1.0)) * if crit { 2.0 } else { 1.0 };
        self.ev.slash = true;
        if crit {
            self.ev.crit = true;
        }
        if spin {
            dmg *= 1.25;
            let r = reach * 1.1;
            self.add_ring(RingKind::Slash, p, r, 8);
            self.arc_hit(p, dir, PI, r, dmg, 60.0, crit);
        } else {
            let half = 1.0 + 0.06 * l;
            self.add_arc(p, reach, dir, half, 9);
            self.arc_hit(p, dir, half, reach, dmg, 55.0, crit);
            if l >= 4.0 {
                self.add_arc(p, reach, dir + PI, half, 9);
                self.arc_hit(p, dir + PI, half, reach, dmg, 55.0, crit);
            }
        }
        if self.up.level(Upgrade::MasterSword) > 0 {
            let spec = BoltSpec { kind: BoltKind::Beam, dmg: 0.9 * self.stats.damage * (1.0 + 0.2 * l), speed: 46.0, life: 36, pierce: 6, radius: 95.0 * self.stats.area.sqrt(), blast: 0.0, crit: false, aim: u16::MAX };
            self.spawn_bolt(p, dir, spec);
        }
    }

    /// Damage every foe inside the arc (`half` of PI or more is the full circle); foes right at your feet are always hit.
    fn arc_hit(&mut self, c: [f32; 2], dir: f32, half: f32, reach: f32, dmg: f32, knock: f32, crit: bool) {
        let (dx, dz) = (dir.cos(), dir.sin());
        let cos_half = half.cos();
        for k in 0..self.alive_idx.len() {
            let i = self.alive_idx[k] as usize;
            let e = &self.enemies[i];
            if !e.alive {
                continue;
            }
            let (tx, tz) = (e.pos[0] - c[0], e.pos[1] - c[1]);
            let d2 = tx * tx + tz * tz;
            let r = reach + e.radius * 0.5;
            if d2 > r * r {
                continue;
            }
            let d = d2.sqrt().max(1.0);
            if half < PI && d > 130.0 && (tx * dx + tz * dz) / d < cos_half {
                continue;
            }
            self.damage_enemy_ex(i, dmg, crit);
            let e = &mut self.enemies[i];
            if knock > 0.0 && e.alive && !e.kind.is_boss() && !e.elite {
                e.pos[0] += tx / d * knock;
                e.pos[1] += tz / d * knock;
            }
        }
    }

    fn add_arc(&mut self, center: [f32; 2], max: f32, dir: f32, half: f32, life: u16) {
        if let Some(r) = self.rings.iter_mut().find(|r| !r.alive) {
            *r = Ring { alive: true, kind: RingKind::Arc, center, max, age: 0, life, dir, half };
        }
    }

    // ---- fire ------------------------------------------------------------------------------

    /// Flamewalker: every so often (and not standing still) the ground behind you catches fire.
    fn flame(&mut self) {
        self.flame_cd = self.flame_cd.saturating_sub(1);
        let p = self.player.pos;
        if self.flame_cd > 0 || self.player.height > 160.0 || dist2(p, self.flame_last) < 130.0 * 130.0 {
            return;
        }
        self.flame_cd = 3;
        self.flame_last = p;
        let l = self.lvl(Upgrade::Flame);
        let inferno = self.up.level(Upgrade::Inferno) > 0;
        let radius = (150.0 + 12.0 * l) * self.stats.area.sqrt() * if inferno { 1.4 } else { 1.0 };
        let life = ((70.0 + 10.0 * l) * if inferno { 1.5 } else { 1.0 }) as u16;
        self.add_zone(ZoneKind::Fire, p, radius, life, self.stats.damage * (0.25 + 0.08 * l), 0.0, 0.0);
    }

    /// Fire Staff: explosive fireballs at the nearest foes.
    fn fireball(&mut self) {
        self.fire_cd2 = self.fire_cd2.saturating_sub(1);
        if self.fire_cd2 > 0 {
            return;
        }
        let l = self.lvl(Upgrade::Fireball);
        let p = self.player.pos;
        let n = 1 + (l >= 3.0) as usize + (l >= 6.0) as usize + self.up.level(Upgrade::Bolts) as usize / 2;
        let mut best = [(f32::INFINITY, u16::MAX); 6];
        let mut found = 0;
        for &i in &self.alive_idx {
            let e = &self.enemies[i as usize];
            let d = dist2(e.pos, p);
            if !e.alive || d > 1500.0 * 1500.0 || d >= best[best.len() - 1].0 {
                continue;
            }
            let mut k = best.len() - 1;
            while k > 0 && best[k - 1].0 > d {
                best[k] = best[k - 1];
                k -= 1;
            }
            best[k] = (d, i);
            found = (found + 1).min(best.len());
        }
        if found == 0 {
            self.fire_cd2 = 4;
            return;
        }
        self.fire_cd2 = (((62.0 - 4.0 * l) * self.stats.cd) as u32).max(14);
        for k in 0..n.min(6) {
            let t = self.enemies[best[k % found].1 as usize].pos;
            let a = (t[1] - p[1]).atan2(t[0] - p[0]) + if k >= found { 0.15 * (k - found + 1) as f32 } else { 0.0 };
            let crit = self.rng.chance(self.stats.crit);
            let spec = BoltSpec { kind: BoltKind::Fire, dmg: self.stats.damage * (1.5 + 0.25 * l) * if crit { 2.0 } else { 1.0 }, speed: 27.0, life: 56, pierce: 0, radius: 55.0 * self.stats.area.sqrt(), blast: (170.0 + 18.0 * l) * self.stats.area.sqrt(), crit, aim: u16::MAX };
            self.spawn_bolt(p, a, spec);
        }
        self.ev.fired = true;
    }

    /// A fireball or missile goes off: area damage; fire also ignites (and with Inferno leaves flames).
    pub(super) fn bolt_blast(&mut self, at: [f32; 2], r: f32, dmg: f32, kind: BoltKind) {
        self.add_ring(RingKind::Blast, at, r, 9);
        self.ev.blast = true;
        self.ev.blast_pos = at;
        let burn = kind == BoltKind::Fire;
        let l = self.lvl(Upgrade::Fireball);
        let power = self.stats.damage * (0.25 + 0.08 * l.max(1.0));
        for k in 0..self.alive_idx.len() {
            let i = self.alive_idx[k] as usize;
            let e = &self.enemies[i];
            if !e.alive {
                continue;
            }
            let reach = r + e.radius * 0.5;
            if dist2(e.pos, at) > reach * reach {
                continue;
            }
            if burn {
                let e = &mut self.enemies[i];
                e.burn = 42;
                self.burn_dmg = self.burn_dmg.max(power);
            }
            self.damage_enemy_ex(i, dmg, false);
        }
        if burn && self.up.level(Upgrade::Inferno) > 0 {
            self.add_zone(ZoneKind::Fire, at, r * 0.8, 100, power, 0.0, 0.0);
        }
    }

    // ---- ice and poison --------------------------------------------------------------------

    /// Frost Ring: a freezing wave; frozen foes stand still (bosses and elites are only slowed) and take extra damage.
    fn frost(&mut self) {
        self.frost_cd = self.frost_cd.saturating_sub(1);
        if self.frost_cd > 0 {
            return;
        }
        let l = self.lvl(Upgrade::Frost);
        let p = self.player.pos;
        let radius = (360.0 + 45.0 * l) * self.stats.area.sqrt();
        if self.nearest_foe(p, radius).is_none() {
            self.frost_cd = 5;
            return;
        }
        self.frost_cd = (((150.0 - 12.0 * l) * self.stats.cd) as u32).max(40);
        self.add_ring(RingKind::Frost, p, radius, 12);
        self.ev.nova = true;
        let ticks = 42 + 8 * l as u32;
        let dmg = self.stats.damage * (0.6 + 0.12 * l);
        for k in 0..self.alive_idx.len() {
            let i = self.alive_idx[k] as usize;
            let e = &mut self.enemies[i];
            if !e.alive {
                continue;
            }
            let reach = radius + e.radius * 0.5;
            if dist2(e.pos, p) > reach * reach {
                continue;
            }
            let t = if e.kind.is_boss() || e.elite { ticks / 2 } else { ticks };
            e.freeze = e.freeze.max(t.min(255) as u8);
            self.damage_enemy(i, dmg);
        }
    }

    /// Poison Flask: lobs a flask at a random foe nearby; the cloud poisons and slows whoever stands in it.
    fn flask(&mut self) {
        self.poison_cd = self.poison_cd.saturating_sub(1);
        if self.poison_cd > 0 {
            return;
        }
        let l = self.lvl(Upgrade::Poison);
        let p = self.player.pos;
        let mut spot = None;
        if !self.alive_idx.is_empty() {
            for _ in 0..6 {
                let i = self.alive_idx[self.rng.below(self.alive_idx.len() as u32) as usize] as usize;
                if self.enemies[i].alive && dist2(self.enemies[i].pos, p) < 1000.0 * 1000.0 {
                    spot = Some(self.enemies[i].pos);
                    break;
                }
            }
        }
        let Some(at) = spot else {
            self.poison_cd = 5;
            return;
        };
        self.poison_cd = (((118.0 - 9.0 * l) * self.stats.cd) as u32).max(30);
        let radius = (210.0 + 22.0 * l) * self.stats.area.sqrt();
        self.add_zone(ZoneKind::Poison, at, radius, (120.0 + 20.0 * l) as u16, self.stats.damage * (0.3 + 0.09 * l), 0.0, 0.0);
        if l >= 4.0 {
            let a = self.rng.range(0.0, TAU);
            self.add_zone(ZoneKind::Poison, [at[0] + a.cos() * radius * 1.2, at[1] + a.sin() * radius * 1.2], radius * 0.8, (100.0 + 20.0 * l) as u16, self.stats.damage * (0.3 + 0.09 * l), 0.0, 0.0);
        }
    }

    // ---- black hole ------------------------------------------------------------------------

    /// Black Hole: opens where the crowd is thickest, drags foes into a pile and collapses with a blast.
    fn black_hole(&mut self) {
        self.grav_cd = self.grav_cd.saturating_sub(1);
        if self.grav_cd > 0 {
            return;
        }
        let l = self.lvl(Upgrade::Gravity);
        let sing = self.up.level(Upgrade::Singularity) > 0;
        let p = self.player.pos;
        // Five random candidates; the one with the most company wins.
        let mut best = (0usize, [0.0f32; 2]);
        if !self.alive_idx.is_empty() {
            let mut near = std::mem::take(&mut self.near);
            for _ in 0..5 {
                let i = self.alive_idx[self.rng.below(self.alive_idx.len() as u32) as usize] as usize;
                let e = &self.enemies[i];
                if !e.alive || dist2(e.pos, p) > 1100.0 * 1100.0 {
                    continue;
                }
                let at = e.pos;
                self.grid.near(at, 350.0, &mut near);
                let n = near.iter().filter(|&&j| self.enemies[j as usize].alive && dist2(self.enemies[j as usize].pos, at) < 350.0 * 350.0).count();
                if n > best.0 {
                    best = (n, at);
                }
            }
            self.near = near;
        }
        if best.0 == 0 {
            self.grav_cd = 6;
            return;
        }
        self.grav_cd = (((260.0 - 18.0 * l) * self.stats.cd) as u32).max(80);
        let radius = (300.0 + 30.0 * l) * self.stats.area.sqrt() * if sing { 1.4 } else { 1.0 };
        let life = ((70.0 + 8.0 * l) * if sing { 1.3 } else { 1.0 }) as u16;
        let boom = self.stats.damage * (3.0 + 0.6 * l) * if sing { 2.0 } else { 1.0 };
        self.add_zone(ZoneKind::Gravity, best.1, radius, life, self.stats.damage * (0.35 + 0.07 * l), (9.0 + l) * if sing { 1.8 } else { 1.0 }, boom);
    }

    // ---- missiles --------------------------------------------------------------------------

    /// Homing Missiles: a salvo that picks distinct targets and chases them.
    fn missiles(&mut self) {
        self.missile_cd = self.missile_cd.saturating_sub(1);
        if self.missile_cd > 0 {
            return;
        }
        let l = self.lvl(Upgrade::Missiles);
        let p = self.player.pos;
        if self.nearest_foe(p, 1300.0).is_none() {
            self.missile_cd = 5;
            return;
        }
        self.missile_cd = (((70.0 - 4.0 * l) * self.stats.cd) as u32).max(18);
        let n = 1 + (l as usize) / 2 + self.up.level(Upgrade::Bolts) as usize / 2;
        for k in 0..n.min(8) {
            let a = self.rng.range(0.0, TAU);
            let crit = self.rng.chance(self.stats.crit);
            let spec = BoltSpec { kind: BoltKind::Missile, dmg: self.stats.damage * (1.4 + 0.2 * l) * if crit { 2.0 } else { 1.0 }, speed: 15.0 + k as f32, life: 90, pierce: self.stats.pierce, radius: 48.0 * self.stats.area.sqrt(), blast: (140.0 + 12.0 * l) * self.stats.area.sqrt(), crit, aim: u16::MAX };
            self.spawn_bolt(p, a, spec);
        }
    }

    /// Steer a missile toward its target (re-picking the nearest foe when the target is gone) and speed it up.
    pub(super) fn steer_missile(&mut self, bi: usize) {
        let b = self.bolts[bi];
        let target = if (b.aim as usize) < ENEMY_SLOTS && self.enemies[b.aim as usize].alive { Some(b.aim as usize) } else { self.nearest_foe(b.pos, 1100.0) };
        let mut sp = (b.vel[0] * b.vel[0] + b.vel[1] * b.vel[1]).sqrt();
        sp = (sp + 1.4).min(34.0);
        let mut a = b.vel[1].atan2(b.vel[0]);
        if let Some(t) = target {
            let tp = self.enemies[t].pos;
            let want = (tp[1] - b.pos[1]).atan2(tp[0] - b.pos[0]);
            let mut d = want - a;
            while d > PI {
                d -= TAU;
            }
            while d < -PI {
                d += TAU;
            }
            a += d.clamp(-0.2, 0.2);
            self.bolts[bi].aim = t as u16;
        }
        self.bolts[bi].vel = [a.cos() * sp, a.sin() * sp];
    }

    // ---- zones and statuses ----------------------------------------------------------------

    /// Ground zones: fire burns, poison eats and slows, gravity drags (and collapses at the end).
    pub(super) fn update_zones(&mut self) {
        let mut near = std::mem::take(&mut self.near);
        for zi in 0..ZONE_SLOTS {
            if !self.zones[zi].alive {
                continue;
            }
            self.zones[zi].age += 1;
            let z = self.zones[zi];
            let pulse = (self.tick + zi as u32) % STATUS_PERIOD == 0;
            if z.kind == ZoneKind::Gravity || pulse {
                self.grid.near(z.pos, z.radius + self.grid.max_r + 60.0, &mut near);
                for &ii in near.iter() {
                    let i = ii as usize;
                    let e = &self.enemies[i];
                    if !e.alive {
                        continue;
                    }
                    let reach = z.radius + e.radius * 0.4;
                    let d2 = dist2(e.pos, z.pos);
                    if d2 > reach * reach {
                        continue;
                    }
                    match z.kind {
                        ZoneKind::Fire => {
                            self.enemies[i].burn = self.enemies[i].burn.max(42);
                            self.burn_dmg = self.burn_dmg.max(z.power);
                        }
                        ZoneKind::Poison => {
                            self.enemies[i].poison = self.enemies[i].poison.max(54);
                            self.poison_dmg = self.poison_dmg.max(z.power);
                        }
                        ZoneKind::Gravity => {
                            let big = e.kind.is_boss() || e.elite;
                            let d = d2.sqrt().max(1.0);
                            let step = (z.pull * if big { 0.2 } else { 1.0 }).min(d);
                            let q = [e.pos[0] - (e.pos[0] - z.pos[0]) / d * step, e.pos[1] - (e.pos[1] - z.pos[1]) / d * step];
                            if self.walkable(q) {
                                self.enemies[i].pos = q;
                            }
                            if pulse {
                                self.damage_quiet(i, z.power);
                            }
                        }
                    }
                }
            }
            if self.zones[zi].age >= z.life {
                self.zones[zi].alive = false;
                if z.kind == ZoneKind::Gravity {
                    self.add_ring(RingKind::Blast, z.pos, z.radius * 0.8, 10);
                    self.ev.blast = true;
                    self.ev.blast_pos = z.pos;
                    self.area_damage(z.pos, z.radius * 0.75, z.boom, 0.0);
                }
            }
        }
        self.near = near;
    }

    /// Burn and poison deal their damage on a shared beat; a freshly lit foe lights its neighbours.
    pub(super) fn update_statuses(&mut self) {
        for e in self.enemies.iter_mut().filter(|e| e.alive) {
            e.burn = e.burn.saturating_sub(1);
            e.poison = e.poison.saturating_sub(1);
        }
        if self.tick % STATUS_PERIOD != 0 {
            return;
        }
        let (burn, poison) = (self.burn_dmg, self.poison_dmg);
        let mut near = std::mem::take(&mut self.near);
        let mut any = false;
        for k in 0..self.alive_idx.len() {
            let i = self.alive_idx[k] as usize;
            let e = self.enemies[i];
            if !e.alive || (e.burn == 0 && e.poison == 0) {
                continue;
            }
            any = true;
            if e.burn >= 30 {
                // Fire spreads to what touches it.
                self.grid.near(e.pos, 150.0, &mut near);
                for &jj in near.iter() {
                    let j = jj as usize;
                    if j != i && self.enemies[j].alive && self.enemies[j].burn < 24 && dist2(self.enemies[j].pos, e.pos) < 150.0 * 150.0 {
                        self.enemies[j].burn = 24;
                    }
                }
            }
            if e.burn > 0 {
                self.damage_quiet(i, burn);
            }
            if e.poison > 0 && self.enemies[i].alive {
                self.damage_quiet(i, poison);
            }
        }
        self.near = near;
        if !any {
            self.burn_dmg = 0.0;
            self.poison_dmg = 0.0;
        }
    }
}
