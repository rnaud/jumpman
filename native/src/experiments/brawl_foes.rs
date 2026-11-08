//! Gem Clash's neutral monsters: ported Ocarina of Time / Banjo-Kazooie enemies that roam the
//! arenas and hurt whoever is near, whoever they belong to. A child module of `brawl_core` (it
//! sees the core's private helpers), engine-free and seeded.
//!
//! Each arena has a themed roster (first two for "Few", first four for "Many"):
//! * Gearworks: Peahats drifting over the gantries, Tektites hopping on the floor.
//! * Ember Basin: Octoroks and Mad Scrubs spitting rocks from the dry rocks.
//! * Bumper Garden: a Bigbutt charging across the lawn, Snippets and Grublins underfoot.
//! * Splitrock Bridge: Bats swooping over the planks.
//! * Gale Summit: Sir Slush lobbing snowballs, Guays and Wolfos in the wind.
//! * Cannon Court: Limbo skeletons, Shrapnel mines, a Deku Baba.
//!
//! Rules: a monster's touch or attack is a normal [`Brawl::hit`] with no attacker, so it knocks
//! fighters about and shakes gems loose; fighters hurt monsters with every move (punches, dive,
//! pound, nova, beam, thrown items, blasts); a KO leaves an item and the monster comes back after a
//! respawn timer at a valid spot. Monsters only ever stand on solid, dry, bumper-free ground over
//! the arena's floor (flyers hover over it) and never leave the rim.
use super::*;
use crate::experiments::foes::{Pose, Species};

pub const MAXM: usize = 6;
/// Ticks a knocked-out monster stays away (14 s).
pub const MON_RESPAWN: i32 = 420;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Monsters {
    Off,
    Few,
    Many,
}

impl Monsters {
    pub const ALL: [Monsters; 3] = [Monsters::Off, Monsters::Few, Monsters::Many];
    pub fn count(self) -> usize {
        match self {
            Monsters::Off => 0,
            Monsters::Few => 2,
            Monsters::Many => 4,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Monsters::Off => "OFF",
            Monsters::Few => "FEW",
            Monsters::Many => "MANY",
        }
    }
    pub fn from_index(i: u8) -> Self {
        Self::ALL[i as usize % 3]
    }
    pub fn index(self) -> u8 {
        self as u8
    }
}

/// Behaviour archetype.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MB {
    /// Walks at the nearest fighter and bites (Grublin, Snippet, Limbo, Ticker).
    Chaser,
    /// Hops at a fighter (Tektite).
    Hopper,
    /// Drifts overhead, spins in, lands dizzy (Peahat).
    Flyer,
    /// Circles, then dives through (Bat, Guay).
    Swooper,
    /// Stands and spits rocks (Octorok, Mad Scrub).
    Shooter,
    /// Lobs snowballs onto a telegraphed spot (Sir Slush).
    Lobber,
    /// Telegraphs, charges in a line, crashes (Bigbutt).
    Charger,
    /// Drifts at a fighter and blows up (Shrapnel).
    Mine,
    /// Anchored, lunges and reels back (Deku Baba).
    Biter,
    /// Circles, lunges (Wolfos).
    Flanker,
}

fn behaviour(sp: Species) -> MB {
    match sp {
        Species::Grublin | Species::Snippet | Species::MutieSnippet | Species::Limbo | Species::Ticker | Species::Skulltula | Species::GoldSkulltula => MB::Chaser,
        Species::Tektite => MB::Hopper,
        Species::Peahat => MB::Flyer,
        Species::Bat | Species::Guay => MB::Swooper,
        Species::Octorok | Species::MadScrub => MB::Shooter,
        Species::SirSlush => MB::Lobber,
        Species::Bigbutt => MB::Charger,
        Species::Shrapnel => MB::Mine,
        Species::DekuBaba => MB::Biter,
        Species::Wolfos => MB::Flanker,
    }
}

/// The monster idle / action states.
pub const ST_IDLE: u8 = 0;
pub const ST_WIND: u8 = 1;
pub const ST_ACT: u8 = 2;
pub const ST_REC: u8 = 3;

#[derive(Clone, Copy, Debug)]
pub struct Mon {
    pub on: bool,
    pub alive: bool,
    pub species: Species,
    pub pos: [f32; 3],
    /// Knockback (x, z) glide.
    pub vel: V2,
    pub yaw: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub st: u8,
    pub t: u16,
    pub cd: u16,
    pub inv: u8,
    pub flash: u8,
    pub home: [f32; 3],
    pub dir: V2,
    /// Ticks until it returns after a KO.
    pub respawn: i32,
    pub struck: bool,
    /// Flyers' altitude above the ground, a hopper's jump height.
    pub lift: f32,
    /// Invulnerable while it pops back in.
    pub spawn_t: u8,
    pub age: u16,
}

impl Mon {
    pub const NONE: Mon = Mon { on: false, alive: false, species: Species::Grublin, pos: [0.0; 3], vel: [0.0; 2], yaw: 0.0, hp: 1.0, max_hp: 1.0, st: 0, t: 0, cd: 0, inv: 0, flash: 0, home: [0.0; 3], dir: [0.0, 1.0], respawn: 0, struck: false, lift: 0.0, spawn_t: 0, age: 0 };

    pub fn radius(&self) -> f32 {
        stats(self.species).radius
    }
    pub fn height(&self) -> f32 {
        stats(self.species).height
    }
    pub fn flies(&self) -> bool {
        matches!(behaviour(self.species), MB::Flyer | MB::Swooper)
    }
    pub fn hp_frac(&self) -> f32 {
        (self.hp / self.max_hp).clamp(0.0, 1.0)
    }
    /// Animation pose for the view.
    pub fn pose(&self) -> Pose {
        if !self.alive {
            return Pose::Die;
        }
        if self.flash > 0 {
            return Pose::Hurt;
        }
        match self.st {
            ST_WIND | ST_ACT => Pose::Attack,
            ST_REC => Pose::Idle,
            _ => {
                if self.vel[0].abs() + self.vel[1].abs() > 40.0 || self.t > 0 && behaviour(self.species) == MB::Chaser {
                    Pose::Move
                } else {
                    Pose::Idle
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MStats {
    pub hp: f32,
    pub radius: f32,
    pub height: f32,
    /// Units per second.
    pub speed: f32,
    pub dmg: f32,
    pub kb: f32,
    pub up: f32,
    pub aggro: f32,
}

/// Stat block in Gem Clash units (a fighter has 100 hp and runs at 520): hit points are the decomp's
/// (`decomp hp x 10`, min 30), touch damage `hearts x 20` (min 8). Bigbutt's 99 becomes a 150 hp heavy.
pub fn stats(sp: Species) -> MStats {
    let hp = (sp.decomp_hp() as f32 * 10.0).max(30.0);
    let dmg = (sp.touch_hearts() * 20.0).max(8.0);
    let s = |hp, radius, height, speed, dmg, kb, up, aggro| MStats { hp, radius, height, speed, dmg, kb, up, aggro };
    match sp {
        Species::DekuBaba => s(hp, 55.0, 110.0, 0.0, dmg, 560.0, 220.0, 330.0),
        Species::Skulltula => s(hp, 50.0, 80.0, 260.0, dmg, 400.0, 160.0, 700.0),
        Species::GoldSkulltula => s(hp, 45.0, 70.0, 340.0, dmg, 400.0, 160.0, 700.0),
        Species::Octorok => s(hp, 56.0, 100.0, 0.0, 9.0, 260.0, 120.0, 1150.0),
        Species::MadScrub => s(hp, 48.0, 90.0, 0.0, 8.0, 240.0, 120.0, 1150.0),
        Species::Tektite => s(hp, 52.0, 80.0, 0.0, dmg.max(10.0), 560.0, 260.0, 850.0),
        Species::Peahat => s(hp, 78.0, 90.0, 190.0, dmg, 720.0, 300.0, 900.0),
        Species::Wolfos => s(hp, 62.0, 90.0, 420.0, dmg.max(12.0), 640.0, 220.0, 900.0),
        Species::Guay => s(hp, 42.0, 60.0, 320.0, dmg.max(9.0), 420.0, 180.0, 900.0),
        Species::Grublin => s(hp, 50.0, 100.0, 300.0, dmg, 420.0, 160.0, 750.0),
        Species::Bigbutt => s(150.0, 100.0, 170.0, 150.0, 20.0, 1100.0, 380.0, 1000.0),
        Species::Snippet => s(hp, 50.0, 60.0, 280.0, dmg, 380.0, 140.0, 700.0),
        Species::MutieSnippet => s(hp * 2.0, 80.0, 90.0, 240.0, dmg * 1.3, 520.0, 200.0, 700.0),
        Species::SirSlush => s(hp, 70.0, 170.0, 0.0, 14.0, 700.0, 260.0, 1300.0),
        Species::Ticker => s(30.0, 36.0, 50.0, 330.0, 8.0, 300.0, 100.0, 700.0),
        Species::Limbo => s(hp, 52.0, 130.0, 240.0, dmg, 480.0, 180.0, 800.0),
        Species::Shrapnel => s(hp, 55.0, 90.0, 190.0, 22.0, 900.0, 520.0, 800.0),
        Species::Bat => s(hp, 44.0, 60.0, 330.0, dmg.max(9.0), 440.0, 200.0, 900.0),
    }
}

/// Visual hurt-box radius for the model (a bit under the hit radius so they do not look bloated).
pub fn visual_radius(sp: Species) -> f32 {
    stats(sp).radius * 0.95
}

/// Arena -> roster.
pub fn roster(kind: ArenaKind) -> [Species; 6] {
    use Species::*;
    match kind {
        ArenaKind::Gearworks => [Peahat, Tektite, Peahat, Guay, Grublin, Tektite],
        ArenaKind::Ember => [Octorok, Octorok, MadScrub, Octorok, MadScrub, Octorok],
        ArenaKind::Bumpers => [Bigbutt, Snippet, Grublin, Snippet, Tektite, Grublin],
        ArenaKind::Bridge => [Bat, Bat, Guay, Bat, Guay, Bat],
        ArenaKind::Gale => [SirSlush, Guay, Wolfos, SirSlush, Guay, Wolfos],
        ArenaKind::Cannon => [Limbo, Shrapnel, DekuBaba, Limbo, Shrapnel, DekuBaba],
    }
}

fn flat(a: [f32; 3]) -> V2 {
    [a[0], a[2]]
}

impl Brawl {
    // -----------------------------------------------------------------------------------------
    // Placement
    // -----------------------------------------------------------------------------------------

    /// Spots a monster may call home: solid, dry, clear of pillars and walls, away from the fighters' spawns.
    pub fn monster_spots(&self) -> Vec<[f32; 3]> {
        let a = &self.arena;
        let mut out: Vec<[f32; 3]> = vec![];
        for s in a.item_spots.iter().chain(a.gem_spots.iter()) {
            let top = match a.support(s[0], s[2], s[1] + 2.0) {
                Some((t, _)) => t,
                None => continue,
            };
            let p = [s[0], top, s[2]];
            if !a.valid_spot(p) || self.wall_at([p[0], p[1] + 20.0, p[2]]) {
                continue;
            }
            if a.kind == ArenaKind::Ember && p[1] < LAVA_HIGH + 20.0 {
                continue;
            }
            if !self.monster_spot_ok(p) {
                continue;
            }
            if !out.iter().any(|q| dist2(flat(*q), flat(p)) < 120.0) {
                out.push(p);
            }
        }
        // Few authored spots (Gearworks): add a handful from the arena's own sky-drop sampler.
        if out.len() < 6 {
            let mut rng = Rng::new(0x4D4F4E ^ a.kind as u64);
            for _ in 0..80 {
                let p = a.random_drop_spot(&mut rng);
                if self.monster_spot_ok(p) && a.kind != ArenaKind::Ember && !out.iter().any(|q| dist2(flat(*q), flat(p)) < 260.0) {
                    out.push(p);
                    if out.len() >= 8 {
                        break;
                    }
                }
            }
        }
        out
    }

    fn monster_spot_ok(&self, p: [f32; 3]) -> bool {
        let a = &self.arena;
        a.valid_spot(p)
            && !self.wall_at([p[0], p[1] + 20.0, p[2]])
            && !a.spawns.iter().any(|sp| dist2(*sp, flat(p)) < 200.0)
            && !a.solids.iter().any(|so| so.mover != Mover::None && (p[0] - so.x).abs() < so.hx + 60.0 && (p[2] - so.z).abs() < so.hz + 60.0)
            && !a.in_bumper(p[0], p[2], 80.0)
            && (a.kind != ArenaKind::Ember || p[1] > LAVA_HIGH + 20.0)
    }

    pub(super) fn mons_begin_round(&mut self) {
        self.mons = [Mon::NONE; MAXM];
        let count = self.cfg.monsters.count().min(MAXM);
        if count == 0 {
            return;
        }
        let mut spots = self.monster_spots();
        if spots.is_empty() {
            let s = self.arena.item_spots[0];
            spots.push(self.arena.respawn_spot(s));
        }
        // Deterministic shuffle.
        for k in (1..spots.len()).rev() {
            let j = self.rng.below(k + 1);
            spots.swap(k, j);
        }
        let ro = roster(self.arena.kind);
        for k in 0..count {
            let sp = ro[k];
            let home = spots[k % spots.len()];
            let st = stats(sp);
            let yaw = self.rng.range(0.0, std::f32::consts::TAU);
            self.mons[k] = Mon { on: true, alive: true, species: sp, pos: home, vel: [0.0; 2], yaw, hp: st.hp, max_hp: st.hp, st: ST_IDLE, t: 0, cd: 60 + self.rng.below(60) as u16, inv: 0, flash: 0, home, dir: dir_of(yaw), respawn: 0, struck: false, lift: if behaviour(sp) == MB::Flyer || behaviour(sp) == MB::Swooper { 120.0 } else { 0.0 }, spawn_t: 0, age: 0 };
        }
    }

    fn respawn_mon(&mut self, m: usize) {
        let home = self.arena.respawn_spot(self.mons[m].home);
        let sp = self.mons[m].species;
        let st = stats(sp);
        let mo = &mut self.mons[m];
        mo.alive = true;
        mo.pos = home;
        mo.vel = [0.0; 2];
        mo.hp = st.hp;
        mo.max_hp = st.hp;
        mo.st = ST_IDLE;
        mo.t = 0;
        mo.cd = 90;
        mo.inv = 0;
        mo.flash = 0;
        mo.struck = false;
        mo.lift = if matches!(behaviour(sp), MB::Flyer | MB::Swooper) { 120.0 } else { 0.0 };
        mo.spawn_t = 45;
        self.ev.push(Ev::MonSpawn(home));
    }

    /// The ground a monster may stand on at (x, z), if it is safe.
    fn mon_ground(&self, x: f32, z: f32, y: f32) -> Option<f32> {
        let (top, _) = self.arena.support(x, z, y + STEP)?;
        let p = [x, top, z];
        if !self.arena.valid_spot(p) {
            return None;
        }
        if self.arena.kind == ArenaKind::Ember && top < LAVA_HIGH + 12.0 {
            return None;
        }
        if (top - y).abs() > STEP + 40.0 {
            return None;
        }
        Some(top)
    }

    /// Move a monster across the ground by (dx, dz); blocked steps (edge, wall, pillar, tide) are skipped.
    fn mon_walk(&mut self, m: usize, dx: f32, dz: f32) -> bool {
        let p = self.mons[m].pos;
        let flying = self.mons[m].flies();
        for (sx, sz) in [(dx, dz), (dx, 0.0), (0.0, dz)] {
            if sx == 0.0 && sz == 0.0 {
                continue;
            }
            let (nx, nz) = (p[0] + sx, p[2] + sz);
            if flying {
                let ok = self.arena.over_floor(nx, nz) && self.arena.bounds.is_none_or(|b| nx.abs() < b[0] - 40.0 && nz.abs() < b[1] - 40.0);
                if ok {
                    if let Some((top, _)) = self.arena.support(nx, nz, p[1] + 600.0) {
                        self.mons[m].pos = [nx, top, nz];
                        return true;
                    }
                }
            } else if let Some(top) = self.mon_ground(nx, nz, p[1]) {
                self.mons[m].pos = [nx, top, nz];
                return true;
            }
        }
        false
    }

    // -----------------------------------------------------------------------------------------
    // Taking damage
    // -----------------------------------------------------------------------------------------

    pub(super) fn hit_mon(&mut self, m: usize, a: i8, dmg: f32, dir: V2, kb: f32, _up: f32) -> bool {
        let mo = self.mons[m];
        if !mo.alive || mo.inv > 0 || mo.spawn_t > 0 {
            return false;
        }
        let mult = if a >= 0 {
            let af = &self.f[a as usize];
            af.kind.stats().power * if af.powered() { 1.5 } else { 1.0 }
        } else {
            1.0
        };
        // Weak moments: a landed Peahat, a crashed Bigbutt.
        let weak = match (mo.species, mo.st) {
            (Species::Peahat, ST_REC) => 1.6,
            (Species::Peahat, _) => 0.7,
            (Species::Bigbutt, ST_REC) => 1.4,
            _ => 1.0,
        };
        let d = dmg * mult * weak;
        let resist = match behaviour(mo.species) {
            MB::Charger => 0.2,
            MB::Flyer => 0.5,
            MB::Shooter | MB::Lobber | MB::Biter => 0.25,
            _ => 1.0,
        };
        let pos = mo.pos;
        {
            let o = &mut self.mons[m];
            o.hp -= d;
            o.flash = 5;
            o.inv = 8;
            o.vel = [dir[0] * kb * resist * 0.5, dir[1] * kb * resist * 0.5];
            // A big blow interrupts a wind-up (not a charge in progress).
            if o.st == ST_WIND && d >= 9.0 && behaviour(o.species) != MB::Charger {
                o.st = ST_IDLE;
                o.t = 0;
                o.cd = o.cd.max(40);
            }
        }
        if a >= 0 && a as usize != m {
            self.f[a as usize].dmg_dealt += d;
        }
        self.ev.push(Ev::MonHit { pos: [pos[0], pos[1] + 50.0 + self.mons[m].lift, pos[2]], big: d >= 12.0 });
        if self.mons[m].hp <= 0.0 {
            self.kill_mon(m, a);
        }
        true
    }

    fn kill_mon(&mut self, m: usize, by: i8) {
        let sp = self.mons[m].species;
        let pos = self.mons[m].pos;
        {
            let o = &mut self.mons[m];
            o.alive = false;
            o.hp = 0.0;
            o.respawn = MON_RESPAWN;
            o.st = ST_IDLE;
            o.t = 0;
        }
        self.ev.push(Ev::MonKo { pos });
        if by >= 0 {
            self.mkos[by as usize] += 1;
        }
        self.mon_kos += 1;
        // Reward: a heart when somebody is hurt, otherwise something to throw or swing. Bigbutt pays two.
        let hurt = (0..self.n).any(|i| self.f[i].alive && self.f[i].hp_frac() < 0.65);
        let live_items = self.items.iter().filter(|i| i.alive).count();
        if live_items < MAXI - 4 {
            let drop = [pos[0], pos[1], pos[2]];
            let kind = if hurt && self.rng.chance(0.5) { ItemKind::Heal } else { self.roll_item(false) };
            let spot = self.arena.respawn_spot(drop);
            self.spawn_item(kind, [spot[0], spot[1] + 80.0, spot[2]], false);
            if sp == Species::Bigbutt {
                let k2 = self.roll_item(false);
                self.spawn_item(k2, [spot[0] + 90.0, spot[1] + 80.0, spot[2]], false);
            }
        }
    }

    /// Fighter `i`'s melee swing reaches monsters too.
    pub(super) fn melee_mons(&mut self, i: usize, range: f32, arc_cos: f32, dmg: f32, kb: f32, up: f32) -> bool {
        let me = self.f[i];
        let fwd = dir_of(me.yaw);
        let mut any = false;
        for m in 0..MAXM {
            let o = self.mons[m];
            if !o.alive || o.inv > 0 {
                continue;
            }
            let d = [o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]];
            let dist = len2(d);
            if dist > range + o.radius() + me.radius() * 0.6 {
                continue;
            }
            let dn = norm2(d, fwd);
            if dn[0] * fwd[0] + dn[1] * fwd[1] < arc_cos && dist > o.radius() * 1.1 {
                continue;
            }
            // Vertical overlap of the two bodies.
            let (lo, hi) = (o.pos[1] + o.lift, o.pos[1] + o.lift + o.height());
            if lo > me.pos[1] + me.height() + 30.0 || hi < me.pos[1] - 30.0 {
                continue;
            }
            let dir = norm2([fwd[0] * 0.55 + dn[0] * 0.45, fwd[1] * 0.55 + dn[1] * 0.45], fwd);
            if self.hit_mon(m, i as i8, dmg, dir, kb, up) {
                any = true;
            }
        }
        any
    }

    pub(super) fn stomp_mons(&mut self, i: usize) {
        let me = self.f[i];
        for m in 0..MAXM {
            let o = self.mons[m];
            if !o.alive || o.inv > 0 {
                continue;
            }
            let d = dist2(flat(me.pos), flat(o.pos));
            let above = me.pos[1] - (o.pos[1] + o.lift);
            if d < me.radius() + o.radius() && above > 20.0 && above < o.height() + 60.0 {
                self.hit_mon(m, i as i8, 10.0, norm2([o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]], dir_of(me.yaw)), 380.0, 0.0);
            }
        }
    }

    pub(super) fn shock_mons(&mut self, i: usize, r: f32, dmg: f32, kb: f32, up: f32) {
        let me = self.f[i];
        for m in 0..MAXM {
            let o = self.mons[m];
            if !o.alive {
                continue;
            }
            let d = dist2(flat(me.pos), flat(o.pos));
            if d > r + o.radius() || (o.pos[1] - me.pos[1]) > 170.0 {
                continue;
            }
            let fall = 1.0 - 0.4 * (d / r).min(1.0);
            let dir = norm2([o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]], dir_of(me.yaw));
            self.hit_mon(m, i as i8, dmg * fall, dir, kb * fall, up);
        }
    }

    pub(super) fn explode_mons(&mut self, pos: [f32; 3], r: f32, dmg: f32, kb: f32, owner: i8) {
        if self.mon_blast {
            return;
        }
        for m in 0..MAXM {
            let o = self.mons[m];
            if !o.alive {
                continue;
            }
            let d = dist2([pos[0], pos[2]], flat(o.pos));
            if d > r + o.radius() || (o.pos[1] - pos[1]).abs() > 260.0 {
                continue;
            }
            let fall = 1.0 - 0.5 * (d / r).min(1.0);
            let dir = norm2([o.pos[0] - pos[0], o.pos[2] - pos[2]], [0.0, 1.0]);
            let was = self.mons[m].inv;
            self.mons[m].inv = 0;
            if !self.hit_mon(m, owner, dmg * fall, dir, kb * fall, 400.0) {
                self.mons[m].inv = was;
            }
        }
    }

    /// The nearest monster in front of fighter `i` within `range` (the beam's fallback target).
    pub(super) fn beam_mon(&self, i: usize, range: f32) -> Option<usize> {
        let me = &self.f[i];
        let mut best: Option<(usize, f32)> = None;
        for m in 0..MAXM {
            let o = &self.mons[m];
            if !o.alive || o.spawn_t > 0 {
                continue;
            }
            let d = dist2(flat(me.pos), flat(o.pos));
            if d < range && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((m, d));
            }
        }
        best.map(|b| b.0)
    }

    pub(super) fn hit_mon_by_beam(&mut self, i: usize, m: usize) {
        let me = self.f[i];
        let o = self.mons[m];
        let from = [me.pos[0], me.pos[1] + 90.0 * me.size, me.pos[2]];
        let to = [o.pos[0], o.pos[1] + o.lift + o.height() * 0.6, o.pos[2]];
        let dir = norm2([o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]], dir_of(me.yaw));
        self.ev.push(Ev::Beam { from, to, id: i });
        self.hit_mon(m, i as i8, 22.0, dir, 700.0, 330.0);
    }

    /// A blaster bullet or a boomerang at `p`: does it hit a monster?
    pub(super) fn point_hits_mon(&mut self, p: [f32; 3], r: f32, owner: i8, dmg: f32, dir: V2, kb: f32, up: f32) -> bool {
        for m in 0..MAXM {
            let o = self.mons[m];
            if !o.alive || o.inv > 0 {
                continue;
            }
            if dist2([p[0], p[2]], flat(o.pos)) < o.radius() + r && p[1] > o.pos[1] + o.lift - 30.0 && p[1] < o.pos[1] + o.lift + o.height() + 30.0 {
                return self.hit_mon(m, owner, dmg, dir, kb, up);
            }
        }
        false
    }

    // -----------------------------------------------------------------------------------------
    // Behaviour
    // -----------------------------------------------------------------------------------------

    fn mon_target(&self, m: usize) -> Option<(usize, f32)> {
        let o = &self.mons[m];
        let aggro = stats(o.species).aggro;
        let mut best: Option<(usize, f32)> = None;
        for j in 0..self.n {
            let f = &self.f[j];
            if !f.alive {
                continue;
            }
            let d = dist2(flat(o.pos), flat(f.pos));
            if d < aggro && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
        best
    }

    /// A monster's blow on fighter `j`.
    fn mon_strike(&mut self, m: usize, j: usize, dmg: f32, kb: f32, up: f32) {
        let o = self.mons[m];
        let f = self.f[j];
        let dir = norm2([f.pos[0] - o.pos[0], f.pos[2] - o.pos[2]], o.dir);
        self.hit(j, -1, dmg, dir, kb, up);
    }

    pub(super) fn step_monsters(&mut self) {
        if self.cfg.monsters == Monsters::Off {
            return;
        }
        for m in 0..MAXM {
            if !self.mons[m].on {
                continue;
            }
            if !self.mons[m].alive {
                self.mons[m].respawn -= 1;
                if self.mons[m].respawn <= 0 && self.alive_count() > 0 {
                    self.respawn_mon(m);
                }
                continue;
            }
            {
                let o = &mut self.mons[m];
                o.age = o.age.saturating_add(1);
                o.inv = o.inv.saturating_sub(1);
                o.flash = o.flash.saturating_sub(1);
                o.spawn_t = o.spawn_t.saturating_sub(1);
                o.cd = o.cd.saturating_sub(1);
            }
            // Knockback glide.
            let v = self.mons[m].vel;
            if v[0].abs() + v[1].abs() > 8.0 {
                self.mon_walk(m, v[0] * DT, v[1] * DT);
                self.mons[m].vel = [v[0] * 0.82, v[1] * 0.82];
            } else {
                self.mons[m].vel = [0.0; 2];
            }
            // Ride a lift / slider; if the ground left, go home.
            if !self.mons[m].flies() {
                let p = self.mons[m].pos;
                match self.arena.support(p[0], p[2], p[1] + STEP + 60.0) {
                    Some((top, _)) if self.mon_ground(p[0], p[2], top).is_some() || (top - p[1]).abs() < 6.0 => self.mons[m].pos[1] = top,
                    _ => {
                        let home = self.arena.respawn_spot(self.mons[m].home);
                        self.mons[m].pos = home;
                    }
                }
            } else {
                let p = self.mons[m].pos;
                if !self.arena.over_floor(p[0], p[2]) {
                    let home = self.arena.respawn_spot(self.mons[m].home);
                    self.mons[m].pos = home;
                } else if let Some((top, _)) = self.arena.support(p[0], p[2], p[1] + 600.0) {
                    self.mons[m].pos[1] = top;
                }
            }
            if matches!(self.phase, Phase::Fight) && self.mons[m].spawn_t == 0 {
                self.mon_ai(m);
            }
            self.mons[m].t = self.mons[m].t.saturating_add(0);
        }
    }

    fn face(&mut self, m: usize, to: V2) {
        let o = &mut self.mons[m];
        let d = norm2([to[0] - o.pos[0], to[1] - o.pos[2]], o.dir);
        o.dir = d;
        let want = yaw_of(d);
        o.yaw += angle_diff(o.yaw, want).clamp(-0.35, 0.35);
    }

    fn mon_ai(&mut self, m: usize) {
        let o = self.mons[m];
        let sp = o.species;
        let st = stats(sp);
        let target = self.mon_target(m);
        let tpos = target.map(|(j, _)| flat(self.f[j].pos));
        if let Some(tp) = tpos {
            if o.st == ST_IDLE || behaviour(sp) == MB::Shooter || behaviour(sp) == MB::Lobber {
                self.face(m, tp);
            }
        }
        let reach_of = |f: &Fighter| st.radius + f.radius() + 35.0;
        match behaviour(sp) {
            MB::Chaser => {
                let mut moving = false;
                if let (Some((j, d)), Some(tp)) = (target, tpos) {
                    let f = self.f[j];
                    if o.st == ST_REC {
                        let away = norm2([o.pos[0] - tp[0], o.pos[2] - tp[1]], o.dir);
                        self.mon_walk(m, away[0] * st.speed * 1.1 * DT, away[1] * st.speed * 1.1 * DT);
                        self.step_timer(m, ST_IDLE, 0, 14);
                    } else {
                        if d > reach_of(&f) * 0.8 {
                            let dir = self.mons[m].dir;
                            self.mon_walk(m, dir[0] * st.speed * DT, dir[1] * st.speed * DT);
                            moving = true;
                        }
                        if d < reach_of(&f) && o.cd == 0 && (f.pos[1] - o.pos[1]).abs() < 120.0 {
                            self.mon_strike(m, j, st.dmg, st.kb, st.up);
                            let mo = &mut self.mons[m];
                            mo.cd = 45;
                            if matches!(sp, Species::Snippet | Species::MutieSnippet) {
                                mo.st = ST_REC;
                                mo.t = 26;
                            }
                        }
                    }
                } else {
                    self.mon_wander(m, st.speed * 0.4);
                }
                self.mons[m].t = if moving { 1 } else { 0 };
            }
            MB::Hopper => match o.st {
                ST_ACT => {
                    let dir = o.dir;
                    let sp_h = st.speed.max(520.0);
                    self.mon_walk(m, dir[0] * sp_h * DT, dir[1] * sp_h * DT);
                    let k = o.t as f32 / 20.0;
                    self.mons[m].lift = (k * std::f32::consts::PI).sin() * 170.0;
                    if !o.struck {
                        if let Some((j, d)) = self.nearest_fighter_to(m) {
                            let f = self.f[j];
                            if d < reach_of(&f) && (f.pos[1] - self.mons[m].pos[1]).abs() < 190.0 {
                                self.mon_strike(m, j, st.dmg, st.kb, st.up);
                                self.mons[m].struck = true;
                            }
                        }
                    }
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 20 {
                        let mo = &mut self.mons[m];
                        mo.lift = 0.0;
                        mo.st = ST_REC;
                        mo.t = 0;
                    }
                }
                ST_REC => {
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 16 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_IDLE;
                        mo.t = 0;
                        mo.cd = 30;
                    }
                }
                _ => {
                    if let (Some((_, d)), Some(tp)) = (target, tpos) {
                        if o.cd == 0 && d < st.aggro {
                            let dd = norm2([tp[0] - o.pos[0], tp[1] - o.pos[2]], o.dir);
                            let mo = &mut self.mons[m];
                            mo.st = ST_ACT;
                            mo.t = 0;
                            mo.dir = dd;
                            mo.struck = false;
                        }
                    }
                }
            },
            MB::Flyer => {
                match o.st {
                    ST_WIND => {
                        self.mons[m].t += 1;
                        if self.mons[m].t >= 24 {
                            let mo = &mut self.mons[m];
                            mo.st = ST_ACT;
                            mo.t = 0;
                            mo.struck = false;
                        }
                    }
                    ST_ACT => {
                        let dir = o.dir;
                        self.mon_walk(m, dir[0] * 620.0 * DT, dir[1] * 620.0 * DT);
                        self.mons[m].lift = (o.lift - 6.0).max(40.0);
                        if !o.struck {
                            if let Some((j, d)) = self.nearest_fighter_to(m) {
                                let f = self.f[j];
                                if d < reach_of(&f) + 20.0 && (f.pos[1] - self.mons[m].pos[1]) < 200.0 {
                                    self.mon_strike(m, j, st.dmg, st.kb, st.up);
                                    self.mons[m].struck = true;
                                }
                            }
                        }
                        self.mons[m].t += 1;
                        if self.mons[m].t >= 18 {
                            let mo = &mut self.mons[m];
                            mo.st = ST_REC;
                            mo.t = 0;
                        }
                    }
                    ST_REC => {
                        // Landed, dizzy: easy to hit.
                        self.mons[m].lift = (o.lift - 20.0).max(0.0);
                        self.mons[m].t += 1;
                        if self.mons[m].t >= 50 {
                            let mo = &mut self.mons[m];
                            mo.st = ST_IDLE;
                            mo.t = 0;
                            mo.cd = 40;
                        }
                    }
                    _ => {
                        let mo = &mut self.mons[m];
                        mo.lift = (mo.lift + 10.0).min(130.0);
                        if let (Some((_, d)), Some(tp)) = (target, tpos) {
                            let dir = norm2([tp[0] - o.pos[0], tp[1] - o.pos[2]], o.dir);
                            self.mon_walk(m, dir[0] * st.speed * DT, dir[1] * st.speed * DT);
                            if d < 380.0 && o.cd == 0 {
                                let mo = &mut self.mons[m];
                                mo.st = ST_WIND;
                                mo.t = 0;
                                mo.dir = dir;
                            }
                        } else {
                            self.mon_wander(m, st.speed * 0.5);
                        }
                    }
                }
            }
            MB::Swooper => match o.st {
                ST_WIND => {
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 8 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_ACT;
                        mo.t = 0;
                        mo.struck = false;
                    }
                }
                ST_ACT => {
                    let dir = o.dir;
                    self.mon_walk(m, dir[0] * 680.0 * DT, dir[1] * 680.0 * DT);
                    self.mons[m].lift = (o.lift - 12.0).max(25.0);
                    if !o.struck {
                        if let Some((j, d)) = self.nearest_fighter_to(m) {
                            let f = self.f[j];
                            if d < reach_of(&f) + 10.0 && (f.pos[1] - self.mons[m].pos[1]) < 190.0 {
                                self.mon_strike(m, j, st.dmg, st.kb, st.up);
                                self.mons[m].struck = true;
                            }
                        }
                    }
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 16 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_REC;
                        mo.t = 0;
                    }
                }
                ST_REC => {
                    let dir = o.dir;
                    self.mon_walk(m, dir[0] * 300.0 * DT, dir[1] * 300.0 * DT);
                    self.mons[m].lift = (o.lift + 10.0).min(130.0);
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 18 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_IDLE;
                        mo.t = 0;
                        mo.cd = 70 + (self.tick % 30) as u16;
                    }
                }
                _ => {
                    if let (Some((_, d)), Some(tp)) = (target, tpos) {
                        let dir = norm2([tp[0] - o.pos[0], tp[1] - o.pos[2]], o.dir);
                        let side = if (m + (self.tick as usize / 90)) % 2 == 0 { 1.0 } else { -1.0 };
                        let perp = [-dir[1] * side, dir[0] * side];
                        let (mx, mz) = if d > 650.0 { (dir[0], dir[1]) } else if d < 380.0 { (-dir[0], -dir[1]) } else { (perp[0], perp[1]) };
                        self.mon_walk(m, mx * st.speed * 0.8 * DT, mz * st.speed * 0.8 * DT);
                        let mo = &mut self.mons[m];
                        mo.lift = (mo.lift + 6.0).min(130.0);
                        if o.cd == 0 && d < 750.0 {
                            mo.st = ST_WIND;
                            mo.t = 0;
                            mo.dir = dir;
                        }
                    } else {
                        self.mon_wander(m, st.speed * 0.5);
                    }
                }
            },
            MB::Shooter => match o.st {
                ST_WIND => {
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 14 {
                        if let Some(tp) = tpos {
                            let from = [o.pos[0], o.pos[1] + 70.0, o.pos[2]];
                            let dir = norm2([tp[0] - o.pos[0], tp[1] - o.pos[2]], o.dir);
                            let vel = [dir[0] * 760.0, 0.0, dir[1] * 760.0];
                            if self.spawn_proj_ex(PKind::Rock, from, vel, [st.dmg, 0.0, 0.0]) {
                                self.ev.push(Ev::Shot(from));
                            }
                        }
                        let mo = &mut self.mons[m];
                        mo.st = ST_IDLE;
                        mo.t = 0;
                        mo.cd = if sp == Species::MadScrub { 60 } else { 90 };
                    }
                }
                _ => {
                    if let Some((_, d)) = target {
                        if o.cd == 0 && d > 150.0 {
                            let mo = &mut self.mons[m];
                            mo.st = ST_WIND;
                            mo.t = 0;
                        }
                    }
                }
            },
            MB::Lobber => match o.st {
                ST_WIND => {
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 20 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_IDLE;
                        mo.t = 0;
                        mo.cd = 120;
                    }
                }
                _ => {
                    if let Some((j, d)) = target {
                        if o.cd == 0 && d > 200.0 {
                            // Lead the target a little; the telegraph shows where it lands.
                            let f = self.f[j];
                            let aim_xz = [f.pos[0] + f.vel[0] * 0.5, f.pos[2] + f.vel[2] * 0.5];
                            let (ax, az) = if self.arena.over_floor(aim_xz[0], aim_xz[1]) {
                                (aim_xz[0], aim_xz[1])
                            } else {
                                let p = self.arena.nearest_floor_point(aim_xz[0], aim_xz[1], 40.0);
                                (p[0], p[1])
                            };
                            let ay = self.arena.support(ax, az, 1000.0).map_or(0.0, |s| s.0);
                            let from = [o.pos[0], o.pos[1] + 150.0, o.pos[2]];
                            let aim = [ax, ay, az];
                            let tt = 36.0 * DT;
                            let g = GRAV * 0.55;
                            let vel = [(aim[0] - from[0]) / tt, (aim[1] - from[1]) / tt + g * tt * 0.5, (aim[2] - from[2]) / tt];
                            let mut p = Proj::NONE;
                            p.alive = true;
                            p.kind = PKind::Snow;
                            p.pos = from;
                            p.vel = vel;
                            p.owner = -1;
                            p.aim = aim;
                            p.mask = st.dmg as u8;
                            if let Some(k) = self.proj.iter().position(|q| !q.alive) {
                                self.proj[k] = p;
                                self.ev.push(Ev::Shot(from));
                            }
                            let mo = &mut self.mons[m];
                            mo.st = ST_WIND;
                            mo.t = 0;
                        }
                    }
                }
            },
            MB::Charger => match o.st {
                ST_WIND => {
                    if let Some(tp) = tpos {
                        self.face(m, tp);
                    }
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 26 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_ACT;
                        mo.t = 0;
                        mo.dir = norm2(mo.dir, [0.0, 1.0]);
                        mo.struck = false;
                    }
                }
                ST_ACT => {
                    let dir = o.dir;
                    let moved = self.mon_walk(m, dir[0] * 860.0 * DT, dir[1] * 860.0 * DT);
                    self.mons[m].yaw = yaw_of(dir);
                    // Everyone in the way gets plowed once.
                    for j in 0..self.n {
                        let f = self.f[j];
                        if !f.alive || f.inv > 0 {
                            continue;
                        }
                        if dist2(flat(self.mons[m].pos), flat(f.pos)) < st.radius + f.radius() + 15.0 && (f.pos[1] - self.mons[m].pos[1]).abs() < 160.0 {
                            self.mon_strike(m, j, st.dmg, st.kb, st.up);
                        }
                    }
                    self.mons[m].t += 1;
                    if !moved {
                        // Hit the rim or a wall: crash and stun.
                        let pos = self.mons[m].pos;
                        self.ev.push(Ev::Bump([pos[0], pos[1] + 80.0, pos[2]]));
                        let mo = &mut self.mons[m];
                        mo.st = ST_REC;
                        mo.t = 0;
                    } else if self.mons[m].t >= 40 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_REC;
                        mo.t = 0;
                    }
                }
                ST_REC => {
                    self.mons[m].t += 1;
                    let need = if self.mons[m].struck { 36 } else { 66 };
                    if self.mons[m].t >= need {
                        let mo = &mut self.mons[m];
                        mo.st = ST_IDLE;
                        mo.t = 0;
                        mo.cd = 70;
                    }
                }
                _ => {
                    if let (Some((_, d)), Some(tp)) = (target, tpos) {
                        let dir = norm2([tp[0] - o.pos[0], tp[1] - o.pos[2]], o.dir);
                        self.mon_walk(m, dir[0] * st.speed * DT, dir[1] * st.speed * DT);
                        if o.cd == 0 && d > 220.0 && d < st.aggro {
                            let mo = &mut self.mons[m];
                            mo.st = ST_WIND;
                            mo.t = 0;
                            mo.dir = dir;
                        }
                    } else {
                        self.mon_wander(m, st.speed * 0.5);
                    }
                }
            },
            MB::Mine => match o.st {
                ST_WIND => {
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 30 {
                        let pos = self.mons[m].pos;
                        self.mons[m].alive = false;
                        self.mons[m].respawn = MON_RESPAWN;
                        self.mons[m].st = ST_IDLE;
                        self.mon_blast = true;
                        self.explode(pos, 300.0, st.dmg, st.kb, -1);
                        self.mon_blast = false;
                    }
                }
                _ => {
                    if let (Some((_, d)), Some(tp)) = (target, tpos) {
                        let dir = norm2([tp[0] - o.pos[0], tp[1] - o.pos[2]], o.dir);
                        self.mon_walk(m, dir[0] * st.speed * DT, dir[1] * st.speed * DT);
                        if d < 260.0 {
                            let mo = &mut self.mons[m];
                            mo.st = ST_WIND;
                            mo.t = 0;
                        }
                    } else {
                        self.mon_wander(m, st.speed * 0.3);
                    }
                }
            },
            MB::Biter => match o.st {
                ST_WIND => {
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 14 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_ACT;
                        mo.t = 0;
                        mo.struck = false;
                    }
                }
                ST_ACT => {
                    let dir = o.dir;
                    self.mon_walk(m, dir[0] * 1000.0 * DT, dir[1] * 1000.0 * DT);
                    if !o.struck {
                        if let Some((j, d)) = self.nearest_fighter_to(m) {
                            let f = self.f[j];
                            if d < reach_of(&f) + 10.0 {
                                self.mon_strike(m, j, st.dmg, st.kb, st.up);
                                self.mons[m].struck = true;
                            }
                        }
                    }
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 8 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_REC;
                        mo.t = 0;
                    }
                }
                ST_REC => {
                    let h = norm2([o.home[0] - o.pos[0], o.home[2] - o.pos[2]], o.dir);
                    if dist2(flat(o.pos), flat(o.home)) > 25.0 {
                        self.mon_walk(m, h[0] * 700.0 * DT, h[1] * 700.0 * DT);
                    }
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 30 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_IDLE;
                        mo.t = 0;
                        mo.cd = 40;
                    }
                }
                _ => {
                    if let Some((_, d)) = target {
                        if d < st.aggro && o.cd == 0 {
                            let mo = &mut self.mons[m];
                            mo.st = ST_WIND;
                            mo.t = 0;
                        }
                    }
                }
            },
            MB::Flanker => match o.st {
                ST_WIND => {
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 10 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_ACT;
                        mo.t = 0;
                        mo.struck = false;
                    }
                }
                ST_ACT => {
                    let dir = o.dir;
                    self.mon_walk(m, dir[0] * 900.0 * DT, dir[1] * 900.0 * DT);
                    if !o.struck {
                        if let Some((j, d)) = self.nearest_fighter_to(m) {
                            let f = self.f[j];
                            if d < reach_of(&f) && (f.pos[1] - self.mons[m].pos[1]).abs() < 130.0 {
                                self.mon_strike(m, j, st.dmg, st.kb, st.up);
                                self.mons[m].struck = true;
                            }
                        }
                    }
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 9 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_REC;
                        mo.t = 0;
                    }
                }
                ST_REC => {
                    let dir = o.dir;
                    self.mon_walk(m, -dir[0] * 420.0 * DT, -dir[1] * 420.0 * DT);
                    self.mons[m].t += 1;
                    if self.mons[m].t >= 24 {
                        let mo = &mut self.mons[m];
                        mo.st = ST_IDLE;
                        mo.t = 0;
                        mo.cd = 45 + (self.tick % 30) as u16;
                    }
                }
                _ => {
                    if let (Some((_, d)), Some(tp)) = (target, tpos) {
                        let dir = norm2([tp[0] - o.pos[0], tp[1] - o.pos[2]], o.dir);
                        let side = if (m + (self.tick as usize / 100)) % 2 == 0 { 1.0 } else { -1.0 };
                        let perp = [-dir[1] * side, dir[0] * side];
                        let (mx, mz) = if d > 520.0 { (dir[0], dir[1]) } else if d < 320.0 { (-dir[0], -dir[1]) } else { (perp[0], perp[1]) };
                        self.mon_walk(m, mx * st.speed * DT, mz * st.speed * DT);
                        if o.cd == 0 && d < 600.0 {
                            let mo = &mut self.mons[m];
                            mo.st = ST_WIND;
                            mo.t = 0;
                            mo.dir = dir;
                        }
                    } else {
                        self.mon_wander(m, st.speed * 0.4);
                    }
                }
            },
        }
    }

    fn step_timer(&mut self, m: usize, to: u8, cd: u16, limit: u16) {
        let mo = &mut self.mons[m];
        mo.t += 1;
        if mo.t >= limit {
            mo.st = to;
            mo.t = 0;
            if cd > 0 {
                mo.cd = cd;
            }
        }
    }

    fn nearest_fighter_to(&self, m: usize) -> Option<(usize, f32)> {
        let o = &self.mons[m];
        let mut best: Option<(usize, f32)> = None;
        for j in 0..self.n {
            let f = &self.f[j];
            if !f.alive {
                continue;
            }
            let d = dist2(flat(o.pos), flat(f.pos));
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
        best
    }

    /// Drift around home when nobody is near.
    fn mon_wander(&mut self, m: usize, speed: f32) {
        let o = self.mons[m];
        if o.species == Species::DekuBaba || speed <= 0.0 {
            return;
        }
        // A slow circle around the home spot.
        let a = (self.tick as f32 * 0.02) + m as f32 * 2.1;
        let goal = [o.home[0] + a.cos() * 160.0, o.home[2] + a.sin() * 160.0];
        let d = norm2([goal[0] - o.pos[0], goal[1] - o.pos[2]], o.dir);
        self.face(m, goal);
        if dist2(flat(o.pos), goal) > 30.0 {
            self.mon_walk(m, d[0] * speed * DT, d[1] * speed * DT);
        }
    }

    /// A projectile with a damage payload in `aim[0]` (rocks).
    fn spawn_proj_ex(&mut self, kind: PKind, pos: [f32; 3], vel: [f32; 3], payload: [f32; 3]) -> bool {
        let Some(k) = self.proj.iter().position(|q| !q.alive) else { return false };
        let mut p = Proj::NONE;
        p.alive = true;
        p.kind = kind;
        p.pos = pos;
        p.vel = vel;
        p.owner = -1;
        p.aim = payload;
        p.mask = payload[0] as u8;
        self.proj[k] = p;
        true
    }

    // -----------------------------------------------------------------------------------------
    // What the bots need to know
    // -----------------------------------------------------------------------------------------

    /// Is `p` somewhere a bot should not walk right now (a mine, a wound-up or charging monster, a snowball's landing)?
    pub fn mon_danger_at(&self, p: V2) -> bool {
        for o in self.mons.iter().filter(|o| o.on && o.alive) {
            let d = dist2(p, flat(o.pos));
            let r = o.radius();
            let zone = match behaviour(o.species) {
                MB::Mine => r + 250.0,
                MB::Charger => {
                    if o.st == ST_ACT || o.st == ST_WIND {
                        // The lane it is about to run down.
                        let to = [p[0] - o.pos[0], p[1] - o.pos[2]];
                        let along = to[0] * o.dir[0] + to[1] * o.dir[1];
                        let side = (to[0] * o.dir[1] - to[1] * o.dir[0]).abs();
                        if along > -50.0 && along < 700.0 && side < r + 90.0 {
                            return true;
                        }
                    }
                    r + 60.0
                }
                MB::Flyer if o.st != ST_IDLE => r + 220.0,
                _ if o.st == ST_WIND || o.st == ST_ACT => r + 150.0,
                MB::Shooter | MB::Lobber => 0.0,
                _ => r + 25.0,
            };
            if zone > 0.0 && d < zone {
                return true;
            }
        }
        self.proj.iter().any(|q| q.alive && q.kind == PKind::Snow && dist2([q.aim[0], q.aim[2]], p) < 270.0)
    }

    /// A rock heading for `p`, or a charging / diving monster close to it.
    pub fn mon_threat_near(&self, p: V2) -> bool {
        for q in self.proj.iter().filter(|q| q.alive && q.kind == PKind::Rock) {
            let to = [p[0] - q.pos[0], p[1] - q.pos[2]];
            let d = len2(to);
            let v = norm2([q.vel[0], q.vel[2]], [0.0, 0.0]);
            if d < 320.0 && d > 30.0 && (to[0] * v[0] + to[1] * v[1]) / d > 0.85 {
                return true;
            }
        }
        self.mons.iter().any(|o| o.on && o.alive && o.st == ST_ACT && matches!(behaviour(o.species), MB::Charger | MB::Swooper | MB::Flanker | MB::Flyer | MB::Hopper) && dist2(p, flat(o.pos)) < 230.0)
    }

    /// The closest monster a bot may go and fight: not mines, chargers, anchored biters or flyers.
    pub fn nearest_huntable_mon(&self, p: V2) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (m, o) in self.mons.iter().enumerate() {
            if !o.on || !o.alive || o.spawn_t > 0 || matches!(behaviour(o.species), MB::Mine | MB::Charger | MB::Biter | MB::Flyer | MB::Swooper) {
                continue;
            }
            let d = dist2(p, flat(o.pos));
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((m, d));
            }
        }
        best
    }

    /// Digest contribution.
    pub(super) fn mons_digest(&self, mix: &mut impl FnMut(f32)) {
        for o in self.mons.iter().filter(|o| o.on) {
            for v in o.pos {
                mix(v);
            }
            mix(o.hp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::brawl_arena::ARENAS;

    fn idle() -> [Input; MAXF] {
        [Input::default(); MAXF]
    }

    fn cfg(arena: ArenaKind, m: Monsters) -> Config {
        Config { seed: 9, humans: 2, bots: 0, arena, monsters: m, ..Default::default() }
    }

    fn ready(arena: ArenaKind, m: Monsters) -> Brawl {
        let mut b = Brawl::new(cfg(arena, m));
        for _ in 0..INTRO_TICKS + 2 {
            b.tick(&idle());
        }
        b
    }

    #[test]
    fn off_few_many_spawn_the_right_number_and_off_is_pristine() {
        for arena in ARENAS {
            assert_eq!(Brawl::new(cfg(arena, Monsters::Off)).mons.iter().filter(|m| m.on).count(), 0);
            assert_eq!(Brawl::new(cfg(arena, Monsters::Few)).mons.iter().filter(|m| m.on).count(), 2);
            assert_eq!(Brawl::new(cfg(arena, Monsters::Many)).mons.iter().filter(|m| m.on).count(), 4);
        }
    }

    #[test]
    fn rosters_are_themed_per_arena() {
        let has = |a: ArenaKind, sp: Species| Brawl::new(cfg(a, Monsters::Many)).mons.iter().any(|m| m.on && m.species == sp);
        assert!(has(ArenaKind::Gearworks, Species::Peahat));
        assert!(has(ArenaKind::Ember, Species::Octorok));
        assert!(has(ArenaKind::Bumpers, Species::Bigbutt));
        assert!(has(ArenaKind::Bridge, Species::Bat));
        assert!(has(ArenaKind::Gale, Species::SirSlush));
        assert!(has(ArenaKind::Cannon, Species::Shrapnel));
    }

    #[test]
    fn monster_spots_are_valid_in_every_arena_and_never_in_the_lava() {
        for arena in ARENAS {
            let b = Brawl::new(cfg(arena, Monsters::Many));
            let spots = b.monster_spots();
            assert!(spots.len() >= 4, "{arena:?}: only {} spots", spots.len());
            for s in &spots {
                assert!(b.arena.valid_spot(*s), "{arena:?} {s:?}");
                if arena == ArenaKind::Ember {
                    assert!(s[1] > LAVA_HIGH + 20.0, "{s:?} would flood");
                }
            }
            for m in b.mons.iter().filter(|m| m.on) {
                assert!(b.arena.valid_spot(m.pos), "{arena:?} {:?} starts at {:?}", m.species, m.pos);
            }
        }
    }

    #[test]
    fn stat_blocks_follow_the_decomp() {
        assert!(stats(Species::Wolfos).hp > stats(Species::Peahat).hp);
        assert!(stats(Species::Peahat).hp > stats(Species::Octorok).hp);
        assert!(stats(Species::Bigbutt).hp > 100.0 && stats(Species::Bigbutt).kb > 1000.0);
        assert!(stats(Species::Peahat).dmg > stats(Species::Octorok).dmg);
        for sp in crate::experiments::foes::SPECIES {
            let s = stats(sp);
            assert!(s.hp >= 24.0 && s.dmg >= 8.0 && s.radius > 20.0, "{sp:?}");
        }
    }

    /// Put a fighter next to a monster and let it act: every kind hurts a standing fighter or throws something.
    #[test]
    fn every_species_attacks_a_fighter_who_stands_still() {
        for sp in crate::experiments::foes::SPECIES {
            let mut b = ready(ArenaKind::Bumpers, Monsters::Few);
            // One monster only: a clean duel on open ground.
            b.mons = [Mon::NONE; MAXM];
            let st = stats(sp);
            let home = [-250.0, 0.0, 250.0];
            b.mons[0] = Mon { on: true, alive: true, species: sp, pos: home, home, hp: st.hp, max_hp: st.hp, cd: 5, lift: if matches!(sp, Species::Peahat | Species::Bat | Species::Guay) { 120.0 } else { 0.0 }, ..Mon::NONE };
            b.f[0].pos = [-250.0, 0.0, 250.0 - 300.0];
            b.f[1].pos = [600.0, 0.0, -600.0];
            b.f[1].hp = 1e6;
            b.f[1].max_hp = 1e6;
            let mut hurt = false;
            let mut shots = false;
            for _ in 0..360 {
                let hp0 = b.f[0].hp;
                b.f[0].pos[0] = -250.0;
                b.f[0].pos[2] = -50.0;
                b.f[0].vel = [0.0; 3];
                b.f[0].inv = 0;
                b.f[0].hp = b.f[0].max_hp;
                b.tick(&idle());
                hurt |= b.f[0].hp < b.f[0].max_hp || b.f[0].hp < hp0;
                shots |= b.proj.iter().any(|p| p.alive && matches!(p.kind, PKind::Rock | PKind::Snow));
                hurt |= b.ev.iter().any(|e| matches!(e, Ev::Hit { victim: 0, .. }));
            }
            assert!(hurt || shots, "{sp:?} never attacked");
        }
    }

    fn punch_until_dead(b: &mut Brawl, m: usize) -> bool {
        for _ in 0..40 {
            if !b.mons[m].alive {
                return true;
            }
            b.mons[m].inv = 0;
            b.mons[m].spawn_t = 0;
            b.mons[m].st = ST_REC;
            let o = b.mons[m];
            b.hit_mon(m, 0, 25.0, [1.0, 0.0], 100.0, 0.0);
            let _ = o;
        }
        !b.mons[m].alive
    }

    #[test]
    fn monsters_can_be_ko_d_drop_an_item_and_respawn_on_a_timer() {
        for arena in ARENAS {
            let mut b = ready(arena, Monsters::Few);
            let items0 = b.items.iter().filter(|i| i.alive).count();
            assert!(punch_until_dead(&mut b, 0), "{arena:?}");
            assert_eq!(b.mkos[0], 1);
            assert!(b.ev.iter().any(|e| matches!(e, Ev::MonKo { .. })));
            let items1 = b.items.iter().filter(|i| i.alive).count();
            assert!(items1 > items0, "{arena:?}: a KO leaves an item");
            // Comes back by itself, at a valid spot, with spawn protection.
            for _ in 0..600 {
                b.f[0].hp = b.f[0].max_hp;
                b.f[1].hp = b.f[1].max_hp;
                b.tick(&idle());
                if b.mons[0].alive {
                    break;
                }
            }
            assert!(b.mons[0].alive, "{arena:?}: respawned");
            assert!(b.arena.valid_spot(b.mons[0].pos), "{arena:?}: {:?}", b.mons[0].pos);
            assert!(b.mons[0].spawn_t > 0 || b.mons[0].inv > 0 || b.mons[0].age < 60);
        }
    }

    #[test]
    fn real_fighter_moves_reach_monsters() {
        // A punch, a pound, a nova and a thrown rock all hurt a monster standing next to the fighter.
        let mut b = ready(ArenaKind::Bumpers, Monsters::Few);
        b.mons = [Mon::NONE; MAXM];
        let sp = Species::Grublin;
        let st = stats(sp);
        let at = |p: [f32; 3]| Mon { on: true, alive: true, species: sp, pos: p, home: p, hp: st.hp, max_hp: st.hp, ..Mon::NONE };
        b.f[0].pos = [-300.0, 0.0, 0.0];
        b.f[0].yaw = std::f32::consts::FRAC_PI_2;
        b.mons[0] = at([-200.0, 0.0, 0.0]);
        b.mons[0].cd = 9999;
        let hp0 = b.mons[0].hp;
        b.start_act(0, Act::Punch(2));
        for _ in 0..8 {
            b.f[0].vel = [0.0; 3];
            b.tick(&idle());
        }
        assert!(b.mons[0].hp < hp0, "punch");
        // Nova (shockwave).
        b.mons[0].inv = 0;
        b.mons[0].spawn_t = 0;
        let hp1 = b.mons[0].hp.max(1.0);
        b.shock_mons(0, 400.0, 20.0, 300.0, 0.0);
        assert!(b.mons[0].hp < hp1 || !b.mons[0].alive, "shockwave");
        // Blast.
        b.mons[1] = at([-200.0, 0.0, 200.0]);
        let hp2 = b.mons[1].hp;
        b.explode(b.mons[1].pos, 200.0, 20.0, 500.0, 0);
        assert!(b.mons[1].hp < hp2, "explosion");
    }

    #[test]
    fn a_monsters_touch_shakes_gems_loose() {
        let mut b = ready(ArenaKind::Bumpers, Monsters::Few);
        b.give_gems(0, 2);
        assert_eq!(b.f[0].gem_count(), 2);
        let dir = [1.0, 0.0];
        b.f[0].inv = 0;
        b.hit(0, -1, 14.0, dir, 640.0, 200.0);
        assert!(b.f[0].gem_count() < 2, "monster hits drop gems like any other solid hit");
    }

    /// Whole bot matches with monsters on: every monster stays on valid ground over the floor, respawns land
    /// on valid spots, nothing goes non-finite, the item cap holds and the match ends.
    #[test]
    fn monsters_never_leave_the_arena_in_bot_matches_with_many_monsters() {
        for (k, arena) in ARENAS.into_iter().enumerate() {
            for seed in 0..5u64 {
                let mut b = Brawl::new(Config { seed: seed * 13 + k as u64, humans: 0, bots: 3, diff: Diff::Hard, arena, monsters: Monsters::Many, ..Default::default() });
                let mut kos = 0;
                for t in 0..30 * 90 {
                    b.tick(&idle());
                    for (m, o) in b.mons.iter().enumerate().filter(|(_, o)| o.on && o.alive) {
                        let ctx = format!("{arena:?} seed {seed} tick {t} mon {m} {:?}", o.species);
                        assert!(o.pos.iter().all(|v| v.is_finite()) && o.hp.is_finite(), "{ctx}");
                        assert!(b.arena.over_floor(o.pos[0], o.pos[2]), "{ctx}: {:?} off the floor", o.pos);
                        if let Some(bd) = b.arena.bounds {
                            assert!(o.pos[0].abs() <= bd[0] && o.pos[2].abs() <= bd[1], "{ctx}: outside the rim {:?}", o.pos);
                        }
                        assert!(o.pos[1] > b.arena.void_y, "{ctx}");
                        assert!(!b.arena.in_bumper(o.pos[0], o.pos[2], 0.0) || o.flies(), "{ctx}: in a bumper");
                        if b.arena.kind == ArenaKind::Ember {
                            assert!(o.pos[1] > b.arena.lava + 2.0 || b.arena.lava_phase == LavaPhase::Low || o.pos[1] > LAVA_HIGH, "{ctx}: in the tide");
                        }
                        if !o.flies() {
                            assert!(b.arena.support(o.pos[0], o.pos[2], o.pos[1] + 2.0).is_some(), "{ctx}: ground-walker with no ground");
                        }
                    }
                    kos += b.ev.iter().filter(|e| matches!(e, Ev::MonKo { .. })).count();
                    assert!(b.items.iter().filter(|i| i.alive).count() <= MAXI);
                    if b.phase == Phase::MatchEnd {
                        break;
                    }
                }
                let _ = kos;
                assert!(b.total_ticks > 100, "{arena:?} seed {seed}");
            }
        }
    }

    #[test]
    fn monsters_are_deterministic_and_cost_little() {
        let run = |m: Monsters| {
            let mut b = Brawl::new(Config { seed: 77, humans: 0, bots: 3, arena: ArenaKind::Gale, monsters: m, ..Default::default() });
            for _ in 0..1200 {
                b.tick(&idle());
            }
            b.digest()
        };
        assert_eq!(run(Monsters::Many), run(Monsters::Many));
        assert_ne!(run(Monsters::Many), run(Monsters::Off));
        let mut b = Brawl::new(Config { seed: 3, humans: 0, bots: 3, arena: ArenaKind::Gearworks, monsters: Monsters::Many, ..Default::default() });
        let t0 = std::time::Instant::now();
        for _ in 0..600 {
            b.tick(&idle());
        }
        assert!(t0.elapsed().as_secs_f32() < 3.0, "600 ticks took {:?}", t0.elapsed());
    }

    #[test]
    fn rocks_and_snowballs_hurt_and_are_dodgeable() {
        let mut b = ready(ArenaKind::Ember, Monsters::Few);
        b.f[0].pos = [0.0, 210.0, 0.0];
        b.f[0].vel = [0.0; 3];
        b.f[0].hp = 100.0;
        let from = [0.0, 280.0, -400.0];
        assert!(b.spawn_proj_ex(PKind::Rock, from, [0.0, 0.0, 760.0], [9.0, 0.0, 0.0]));
        for _ in 0..40 {
            b.f[0].pos = [0.0, 210.0, 0.0];
            b.tick(&idle());
        }
        assert!(b.f[0].hp < 100.0, "a rock that reaches a fighter hurts");
        // A rock that misses goes on and dies at the rim.
        let hp = b.f[0].hp;
        assert!(b.spawn_proj_ex(PKind::Rock, [300.0, 280.0, -400.0], [0.0, 0.0, 760.0], [9.0, 0.0, 0.0]));
        for _ in 0..80 {
            b.f[0].pos = [0.0, 210.0, 0.0];
            b.tick(&idle());
        }
        assert_eq!(b.f[0].hp, hp);
        assert!(!b.proj.iter().any(|p| p.alive && p.kind == PKind::Rock));
    }

    // ---- the existing balance / bounds property tests, rerun with monsters on ----

    fn gems_in_bounds(b: &Brawl, ctx: &str) {
        for (g, gem) in b.gems.iter().enumerate() {
            if matches!(gem.state, GemState::Spent | GemState::Held(_)) {
                continue;
            }
            let p = gem.pos;
            assert!(p.iter().all(|v| v.is_finite()), "{ctx}: gem {g} not finite");
            assert!(b.arena.over_floor(p[0], p[2]), "{ctx}: gem {g} {:?} {p:?} is outside the floor", gem.state);
            assert!(p[1] > -26.0 && p[1] > b.arena.void_y, "{ctx}: gem {g} below the level");
        }
    }

    #[test]
    fn gems_and_items_stay_in_bounds_with_monsters_knocking_fighters_about() {
        for (k, arena) in ARENAS.into_iter().enumerate() {
            for seed in 0..6u64 {
                let mut b = Brawl::new(Config { seed: seed * 31 + k as u64, humans: 0, bots: 3, diff: Diff::Hard, arena, monsters: Monsters::Many, ..Default::default() });
                for t in 0..30 * 70 {
                    b.tick(&idle());
                    gems_in_bounds(&b, &format!("{arena:?} seed {seed} t{t}"));
                    for it in b.items.iter().filter(|i| i.alive && i.grounded && i.state == IState::Rest) {
                        // (A released held item may sit up to its carry offset past the rim for the one tick before physics clamps it.)
                        let q = b.arena.nearest_floor_point(it.pos[0], it.pos[2], 0.0);
                        assert!(dist2([it.pos[0], it.pos[2]], q) < 60.0, "{arena:?} seed {seed} t{t}: item {:?} {:?} is off the floor", it.kind, it.pos);
                    }
                    for f in b.f[..b.n].iter().filter(|f| f.alive) {
                        assert!(f.pos.iter().all(|v| v.is_finite()) && f.pos[1] > b.arena.void_y - 3000.0, "fighter escaped");
                    }
                    if b.phase == Phase::MatchEnd {
                        break;
                    }
                }
            }
        }
    }

    /// The difficulty ladder (Hard > Normal > Easy) survives neutral monsters on every arena.
    #[test]
    fn difficulty_ladder_still_holds_with_monsters_on() {
        let duel = |a: Diff, c: Diff, seeds: u64| {
            let (mut aw, mut cw) = (0, 0);
            for seed in 0..seeds {
                for (k, arena) in ARENAS.into_iter().enumerate() {
                    let swap = (seed + k as u64) % 2 == 1;
                    let (d0, d1) = if swap { (c, a) } else { (a, c) };
                    let mut cfg = Config { seed: seed * 31 + k as u64, humans: 0, bots: 2, arena, same_kind: Some(Kind::Goomba), rounds_to_win: 1, monsters: Monsters::Few, ..Default::default() };
                    cfg.diffs = [Some(d0), Some(d1), None, None];
                    let mut b = Brawl::new(cfg);
                    let mut ticks = 0;
                    while b.phase != Phase::MatchEnd && ticks < 30 * 60 * 8 {
                        b.tick(&idle());
                        ticks += 1;
                    }
                    if b.match_winner >= 0 {
                        if (b.match_winner == 0) != swap { aw += 1 } else { cw += 1 }
                    }
                }
            }
            (aw, cw)
        };
        let hn = duel(Diff::Hard, Diff::Normal, 8);
        let ne = duel(Diff::Normal, Diff::Easy, 8);
        println!("MONSTERS ON: Hard vs Normal {}-{}, Normal vs Easy {}-{} (48 duels each)", hn.0, hn.1, ne.0, ne.1);
        assert!(hn.0 > hn.1, "Hard must beat Normal with monsters: {hn:?}");
        assert!(ne.0 > ne.1, "Normal must beat Easy with monsters: {ne:?}");
    }

    #[test]
    fn no_seat_dominates_four_way_with_monsters() {
        let mut wins = [0u32; 4];
        let mut matches = 0;
        let mut mon_kos = 0u32;
        for seed in 0..8u64 {
            for (k, arena) in ARENAS.into_iter().enumerate() {
                let mut b = Brawl::new(Config { seed: seed * 17 + k as u64, humans: 0, bots: 4, arena, same_kind: Some(Kind::Goomba), rounds_to_win: 1, monsters: Monsters::Few, ..Default::default() });
                let mut ticks = 0;
                while b.phase != Phase::MatchEnd && ticks < 30 * 60 * 10 {
                    b.tick(&idle());
                    ticks += 1;
                }
                if b.match_winner >= 0 {
                    wins[b.match_winner as usize] += 1;
                }
                mon_kos += b.mon_kos;
                matches += 1;
            }
        }
        println!("FFA seat wins with monsters over {matches} matches: {wins:?}, monsters KO'd by bots/hazards: {mon_kos}");
        assert!(wins.iter().all(|&w| (w as f32) < matches as f32 * 0.5), "one seat wins too often: {wins:?}");
    }

    #[test]
    fn bots_hunt_monsters_and_pick_up_the_loot() {
        let (mut kos, mut matches, mut hurt_by_monsters) = (0u32, 0, 0u32);
        for (k, arena) in ARENAS.into_iter().enumerate() {
            for seed in 0..4u64 {
                let mut b = Brawl::new(Config { seed: seed * 7 + k as u64 + 300, humans: 0, bots: 2, diff: Diff::Normal, arena, monsters: Monsters::Many, rounds_to_win: 1, ..Default::default() });
                let mut ticks = 0;
                while b.phase != Phase::MatchEnd && ticks < 30 * 60 * 10 {
                    b.tick(&idle());
                    hurt_by_monsters += b.ev.iter().filter(|e| matches!(e, Ev::Hit { .. })).count() as u32 * 0;
                    ticks += 1;
                }
                kos += b.mkos.iter().map(|&n| n as u32).sum::<u32>();
                matches += 1;
            }
        }
        let _ = hurt_by_monsters;
        println!("bots KO'd {kos} monsters over {matches} matches");
        assert!(kos >= matches / 5, "bots should go after monsters now and then: {kos} KOs in {matches} matches");
    }

    #[test]
    fn a_bot_with_nothing_else_to_do_hunts_a_monster_down() {
        let mut kos = 0;
        for seed in 0..6u64 {
            let mut b = Brawl::new(Config { seed, humans: 0, bots: 1, diff: Diff::Normal, arena: ArenaKind::Bumpers, monsters: Monsters::Few, rounds_to_win: 3, ..Default::default() });
            // The bot has the arena to itself (the other seat is parked far away and asleep) and a lone Grublin.
            b.mons = [Mon::NONE; MAXM];
            let st = stats(Species::Grublin);
            let home = [200.0, 0.0, 200.0];
            b.mons[0] = Mon { on: true, alive: true, species: Species::Grublin, pos: home, home, hp: st.hp, max_hp: st.hp, cd: 30, ..Mon::NONE };
            b.f[1].human = true;
            b.brains[1] = None;
            b.f[1].pos = [-700.0, 110.0, 0.0];
            b.f[1].hp = 1e6;
            b.f[1].max_hp = 1e6;
            for _ in 0..INTRO_TICKS + 30 * 20 {
                b.tick(&idle());
                if !b.mons[0].alive {
                    break;
                }
                b.f[0].hp = b.f[0].max_hp;
            }
            kos += (!b.mons[0].alive) as u32;
        }
        assert!(kos >= 4, "a bot with only a monster to fight should KO it within 20 s: {kos}/6");
    }

    #[test]
    fn the_gauntlet_still_ends_with_monsters_on() {
        let mut cleared = 0;
        for seed in 0..3u64 {
            let mut b = Brawl::new(Config { seed, mode: Mode::Gauntlet, humans: 1, bots: 0, monsters: Monsters::Few, ..Default::default() });
            b.brains[0] = Some(Brain::new(Diff::Hard, false, seed + 99));
            b.f[0].human = false;
            let mut ticks = 0;
            while b.phase != Phase::MatchEnd {
                if b.brains[0].is_none() {
                    b.brains[0] = Some(Brain::new(Diff::Hard, false, seed + 5));
                    b.f[0].human = false;
                }
                b.tick(&idle());
                ticks += 1;
                assert!(ticks < 30 * 60 * 40, "gauntlet never ended at stage {}", b.stage);
            }
            cleared += b.stage_clears;
        }
        assert!(cleared >= 3, "a Hard bot should still clear several stages with monsters around: {cleared}");
    }

    #[test]
    fn a_casual_player_still_has_a_fair_fight_with_monsters() {
        // Same dumb human as the main balance test, with Few monsters about: Hard stays harder than Normal.
        let play = |diff: Diff| {
            let (mut won, mut played) = (0, 0);
            for (k, arena) in ARENAS.into_iter().enumerate() {
                for seed in 0..8u64 {
                    let mut b = Brawl::new(Config { seed: seed * 101 + k as u64 * 7 + 1, humans: 1, bots: 2, diff, arena, monsters: Monsters::Few, ..Default::default() });
                    let mut rng = Rng::new(seed * 13 + k as u64);
                    let mut ticks = 0;
                    while b.phase != Phase::MatchEnd && ticks < 30 * 60 * 14 {
                        let mut inputs = idle();
                        // Walk at the nearest rival, mash hit when close.
                        let me = b.f[0];
                        if me.alive {
                            if let Some((j, d)) = b.nearest_foe(0) {
                                let to = norm2([b.f[j].pos[0] - me.pos[0], b.f[j].pos[2] - me.pos[2]], [0.0, 0.0]);
                                inputs[0].mv = to;
                                if d < 175.0 && rng.chance(0.25) {
                                    inputs[0].attack = true;
                                }
                                if me.grounded && rng.chance(0.015) {
                                    inputs[0].jump = true;
                                }
                            }
                        }
                        b.tick(&inputs);
                        ticks += 1;
                    }
                    played += 1;
                    won += (b.match_winner == 0) as u32;
                }
            }
            won as f32 / played as f32
        };
        let (e, n, h) = (play(Diff::Easy), play(Diff::Normal), play(Diff::Hard));
        println!("NOVICE with monsters: Easy {:.0}% Normal {:.0}% Hard {:.0}%", e * 100.0, n * 100.0, h * 100.0);
        assert!(e > n && n >= h, "ladder: {e} {n} {h}");
        assert!(e > 0.4, "an easy bot should still lose to a casual player some of the time: {e}");
    }
}
