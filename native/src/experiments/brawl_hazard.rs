//! Gem Clash's course hazards: each real-world region brings one or two set pieces of its own course (a Chain Chomp on its post
//! on Bob-omb's lower slope, the Whomp King slamming his deck, Thwomps over Whomp's fortress yard, iron balls rolling down the
//! Bob-omb path, horses running through the Lon Lon paddock, a Tweester in the Shifting Sand pit, Boos in the castle courtyard,
//! Deku Babas in Kokiri Forest, Redeads in Kakariko's graveyard, a giant Goomba on Tiny-Huge Island, waves on its shore ...).
//! A child module of `brawl_core` (it sees the core's private helpers), engine-free, seeded and deterministic.
//!
//! Every attack is telegraphed for at least two thirds of a second: the [`Hazard::zone`] it is about to strike (a ring, a lane)
//! is what the view draws on the ground and what the bots read, so a human gets exactly the warning a bot gets. A hazard hits
//! like any blow with no attacker (knocks fighters about, shakes gems loose) but keeps the credit of whoever knocked the victim
//! into it a moment before (see [`Brawl::hit`]), so setting a foe up for the Chomp counts as your knock-out.
//!
//! Kinds:
//! * Lunger (Chain Chomp, Deku Baba): anchored; picks a fighter in reach of its post, rears back (ring on the spot), lunges.
//! * Slam (Whomp, Whomp King): walks after the nearest fighter on a leash, rears up (lane ahead of it), falls flat, gets up.
//! * Drop (Thwomp): hovers, tracks a fighter (square shadow), locks, drops, rises.
//! * Roller (iron ball, boulder, snowball, horse, giant Goomba, wave): crosses the whole arena along a lane aimed at someone;
//!   the lane shows first. Low rollers can be jumped; a giant Goomba is stomped flat from above.
//! * Drifter (Tweester, Boo, Redead): always on the field and slow. The Tweester flings whoever it touches into the air; a Boo
//!   only moves while its target's back is turned (and cannot hurt whoever faces it); a Redead screams (ring) and freezes
//!   everyone near it, then bites.
use super::*;

/// Most hazards a region has.
pub const MAXH: usize = 4;
/// Seconds of a round before the first attack (the round starts calm).
pub const HAZARD_GRACE: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Look {
    ChainChomp,
    DekuBaba,
    Whomp,
    WhompKing,
    Thwomp,
    IronBall,
    Boulder,
    Snowball,
    Horse,
    Goomba,
    Wave,
    Tornado,
    Boo,
    Redead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HzKind {
    Lunger,
    Slam,
    Drop,
    Roller,
    Drifter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HzState {
    Idle,
    Warn,
    Act,
    Rest,
}

/// How much faster than first tuned the set pieces move (they were tuned against a 520-unit run; it is 760 now).
pub const HAZARD_PACE: f32 = 1.2;

/// Tuning of one look (ticks at 30 Hz, SM64 units).
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub kind: HzKind,
    /// Lunger: chain length; Slam: lane length; Drop: half side of the footprint; Drifter (Redead): scream radius.
    pub reach: f32,
    /// Head / body radius; Roller: half width of the lane; Slam: half width of the lane.
    pub radius: f32,
    /// Height of the body: a fighter whose feet are above it is not touched (jump the iron ball).
    pub height: f32,
    pub warn: u16,
    pub act: u16,
    pub rest: u16,
    pub dmg: f32,
    pub kb: f32,
    pub up: f32,
    /// Units per second: roller travel, Whomp walk, Thwomp tracking, drifting.
    pub speed: f32,
    /// Ticks between attacks (plus up to a second of jitter).
    pub every: i32,
}

impl Look {
    pub fn spec(self) -> Spec {
        // The table was written for the old 520-unit walk; fighters now run 760, so the set pieces keep pace: they move a fifth
        // faster and come round a sixth sooner. Telegraphs stay as long as they were (they are the human's warning), and a
        // fighter now crosses a strike zone in two thirds of the time it used to.
        let s = |kind, reach, radius, height, warn, act, rest, dmg, kb, up, speed: f32, every: i32| Spec { kind, reach, radius, height, warn, act, rest, dmg, kb, up, speed: speed * HAZARD_PACE, every: every * 5 / 6 };
        use HzKind::*;
        match self {
            Look::ChainChomp => s(Lunger, 560.0, 100.0, 220.0, 26, 7, 26, 15.0, 1000.0, 430.0, 0.0, 70),
            Look::DekuBaba => s(Lunger, 300.0, 62.0, 160.0, 21, 5, 20, 9.0, 650.0, 300.0, 0.0, 55),
            Look::Whomp => s(Slam, 430.0, 120.0, 400.0, 30, 6, 50, 19.0, 650.0, 340.0, 115.0, 45),
            Look::WhompKing => s(Slam, 620.0, 175.0, 600.0, 34, 7, 56, 23.0, 760.0, 380.0, 95.0, 50),
            Look::Thwomp => s(Drop, 150.0, 150.0, 300.0, 40, 5, 36, 21.0, 620.0, 420.0, 430.0, 60),
            Look::IronBall => s(Roller, 0.0, 85.0, 165.0, 36, 0, 0, 14.0, 900.0, 380.0, 950.0, 150),
            Look::Boulder => s(Roller, 0.0, 100.0, 190.0, 36, 0, 0, 15.0, 920.0, 400.0, 820.0, 150),
            Look::Snowball => s(Roller, 0.0, 110.0, 200.0, 36, 0, 0, 12.0, 900.0, 420.0, 760.0, 150),
            Look::Horse => s(Roller, 0.0, 95.0, 260.0, 42, 0, 0, 13.0, 1000.0, 460.0, 1250.0, 165),
            Look::Goomba => s(Roller, 0.0, 135.0, 300.0, 42, 0, 0, 12.0, 850.0, 520.0, 430.0, 170),
            Look::Wave => s(Roller, 0.0, 90.0, 150.0, 54, 0, 0, 5.0, 720.0, 260.0, 620.0, 200),
            Look::Tornado => s(Drifter, 0.0, 105.0, 400.0, 0, 0, 45, 7.0, 380.0, 1500.0, 190.0, 0),
            Look::Boo => s(Drifter, 0.0, 72.0, 220.0, 0, 0, 60, 9.0, 520.0, 260.0, 250.0, 0),
            Look::Redead => s(Drifter, 330.0, 70.0, 180.0, 24, 30, 90, 10.0, 320.0, 160.0, 115.0, 0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Look::ChainChomp => "Chain Chomp",
            Look::DekuBaba => "Deku Baba",
            Look::Whomp => "Whomp",
            Look::WhompKing => "Whomp King",
            Look::Thwomp => "Thwomp",
            Look::IronBall => "Iron ball",
            Look::Boulder => "Boulder",
            Look::Snowball => "Snowball",
            Look::Horse => "Horses",
            Look::Goomba => "Giant Goomba",
            Look::Wave => "Wave",
            Look::Tornado => "Tweester",
            Look::Boo => "Boo",
            Look::Redead => "Redead",
        }
    }

    /// What the HUD's warning line says while this one is about to strike.
    pub fn warning(self) -> &'static str {
        match self {
            Look::ChainChomp => "CHAIN CHOMP!",
            Look::DekuBaba => "DEKU BABA!",
            Look::Whomp | Look::WhompKing => "WHOMP - GET OUT OF THE LANE",
            Look::Thwomp => "THWOMP!",
            Look::IronBall => "IRON BALL - JUMP IT",
            Look::Boulder => "BOULDER - JUMP IT",
            Look::Snowball => "SNOWBALL - JUMP IT",
            Look::Horse => "STAMPEDE!",
            Look::Goomba => "GIANT GOOMBA - STOMP IT",
            Look::Wave => "WAVE INCOMING",
            Look::Tornado => "TWEESTER",
            Look::Boo => "BOO - FACE IT",
            Look::Redead => "REDEAD SCREAM",
        }
    }
}

/// A hazard as a region lists it: arena-local home (x, z), and for rollers a lane direction hint ([0, 0] = any way).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazardDef {
    pub look: Look,
    pub at: V2,
    pub dir: V2,
}

impl HazardDef {
    pub const fn new(look: Look, at: V2) -> Self {
        Self { look, at, dir: [0.0, 0.0] }
    }
    pub const fn lane(look: Look, at: V2, dir: V2) -> Self {
        Self { look, at, dir }
    }
}

/// The ground a hazard is about to strike (or is striking).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    Circle { c: V2, r: f32 },
    /// From `a` along unit `dir` for `len`, `half` to each side.
    Lane { a: V2, dir: V2, len: f32, half: f32 },
    /// Axis-aligned square.
    Square { c: V2, half: f32 },
}

impl Zone {
    pub fn contains(&self, p: V2, pad: f32) -> bool {
        match *self {
            Zone::Circle { c, r } => dist2(p, c) <= r + pad,
            Zone::Lane { a, dir, len, half } => {
                let q = [p[0] - a[0], p[1] - a[1]];
                let along = q[0] * dir[0] + q[1] * dir[1];
                let side = q[0] * dir[1] - q[1] * dir[0];
                along >= -pad && along <= len + pad && side.abs() <= half + pad
            }
            Zone::Square { c, half } => (p[0] - c[0]).abs() <= half + pad && (p[1] - c[1]).abs() <= half + pad,
        }
    }

    /// The way out of the zone from `p` (unit): sideways out of a lane, away from a ring's centre.
    pub fn escape(&self, p: V2) -> V2 {
        match *self {
            Zone::Circle { c, .. } | Zone::Square { c, .. } => norm2([p[0] - c[0], p[1] - c[1]], [1.0, 0.0]),
            Zone::Lane { a, dir, .. } => {
                let q = [p[0] - a[0], p[1] - a[1]];
                let side = q[0] * dir[1] - q[1] * dir[0];
                let n = [dir[1], -dir[0]];
                if side >= 0.0 { n } else { [-n[0], -n[1]] }
            }
        }
    }

    /// A point of the zone (the middle), for the bots that push foes into it.
    pub fn centre(&self) -> V2 {
        match *self {
            Zone::Circle { c, .. } | Zone::Square { c, .. } => c,
            Zone::Lane { a, dir, len, .. } => [a[0] + dir[0] * len * 0.5, a[1] + dir[1] * len * 0.5],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Hazard {
    pub look: Look,
    /// Post / home spot (arena-local, on the floor).
    pub home: [f32; 3],
    /// The body (Chomp: its head).
    pub pos: [f32; 3],
    pub yaw: f32,
    /// Lane / lunge direction (unit).
    pub dir: V2,
    /// The region's lane hint.
    pub hint: V2,
    /// Lunger: the spot it lunges at; Slam / Drop: the footprint centre; Roller: where the lane starts.
    pub aim: [f32; 3],
    pub st: HzState,
    pub t: u16,
    pub cd: i32,
    pub target: i8,
    /// Fighters already struck by this attack.
    pub mask: u8,
    /// Roller: distance travelled; Drifter: ticks to the next retarget.
    pub run: f32,
    /// Roller: total travel of this crossing.
    pub span: f32,
}

impl Hazard {
    pub fn new(def: &HazardDef, home: [f32; 3], k: usize) -> Self {
        let sp = def.look.spec();
        let hover = if sp.kind == HzKind::Drop { 420.0 } else { 0.0 };
        Self {
            look: def.look,
            home,
            pos: [home[0], home[1] + hover, home[2]],
            yaw: 0.0,
            dir: norm2(def.dir, [1.0, 0.0]),
            hint: def.dir,
            aim: home,
            st: HzState::Idle,
            t: 0,
            cd: sp.every / 2 + k as i32 * 45,
            target: -1,
            mask: 0,
            run: 0.0,
            span: 0.0,
        }
    }

    pub fn spec(&self) -> Spec {
        self.look.spec()
    }

    /// Is the body on the field (a roller is only there while it crosses)?
    pub fn shown(&self) -> bool {
        self.spec().kind != HzKind::Roller || self.st == HzState::Act
    }

    /// The ground about to be struck (Warn) or being struck (Act); drifters: the ground they would touch now.
    pub fn zone(&self) -> Option<Zone> {
        let sp = self.spec();
        let flat = |p: [f32; 3]| [p[0], p[2]];
        match (sp.kind, self.st) {
            (HzKind::Lunger, HzState::Warn | HzState::Act) => Some(Zone::Circle { c: flat(self.aim), r: sp.radius + 25.0 }),
            (HzKind::Slam, HzState::Warn | HzState::Act) => Some(Zone::Lane { a: flat(self.pos), dir: self.dir, len: sp.reach, half: sp.radius }),
            (HzKind::Drop, HzState::Warn | HzState::Act) => Some(Zone::Square { c: flat(self.aim), half: sp.reach }),
            (HzKind::Roller, HzState::Warn) => Some(Zone::Lane { a: flat(self.aim), dir: self.dir, len: self.span, half: sp.radius }),
            (HzKind::Roller, HzState::Act) => {
                // Only the stretch still ahead of the body.
                let p = flat(self.pos);
                let back = sp.radius;
                Some(Zone::Lane { a: [p[0] - self.dir[0] * back, p[1] - self.dir[1] * back], dir: self.dir, len: (self.span - self.run).max(0.0) + back, half: sp.radius })
            }
            (HzKind::Drifter, HzState::Warn) if self.look == Look::Redead => Some(Zone::Circle { c: flat(self.pos), r: sp.reach }),
            (HzKind::Drifter, HzState::Idle | HzState::Warn) => Some(Zone::Circle { c: flat(self.pos), r: sp.radius + 40.0 }),
            _ => None,
        }
    }

    /// 0..1 through the telegraph (the view pulses the warning faster as it runs out).
    pub fn warn_k(&self) -> f32 {
        let sp = self.spec();
        if self.st == HzState::Warn && sp.warn > 0 { self.t as f32 / sp.warn as f32 } else { 0.0 }
    }
}

impl Brawl {
    /// Advance every course hazard one tick (Fight phase only).
    pub(super) fn step_course_hazards(&mut self) {
        let calm = self.round_t > ROUND_SECS - HAZARD_GRACE;
        for h in 0..self.arena.hazards.len() {
            let kind = self.arena.hazards[h].spec().kind;
            match kind {
                HzKind::Lunger => self.hz_lunger(h, calm),
                HzKind::Slam => self.hz_slam(h, calm),
                HzKind::Drop => self.hz_drop(h, calm),
                HzKind::Roller => self.hz_roller(h, calm),
                HzKind::Drifter => self.hz_drifter(h, calm),
            }
        }
    }

    fn hz_floor(&self, x: f32, z: f32, y: f32) -> Option<f32> {
        self.arena.support(x, z, y + 160.0).map(|s| s.0)
    }

    /// Set the state and clear the tick count / hit mask.
    fn hz_go(&mut self, h: usize, st: HzState) {
        let hz = &mut self.arena.hazards[h];
        hz.st = st;
        hz.t = 0;
        hz.mask = 0;
        if st == HzState::Warn {
            let (pos, look) = (hz.aim, hz.look);
            self.ev.push(Ev::Hazard { look, pos, strike: false });
        }
    }

    fn hz_cooldown(&mut self, h: usize) {
        let j = self.rng.below(30) as i32;
        let sp = self.arena.hazards[h].spec();
        let base = if self.sudden { sp.every / 2 } else { sp.every };
        self.arena.hazards[h].cd = base + j;
    }

    /// Nearest live fighter to `p` within `r` (and a little above / below), as (slot, distance).
    fn hz_nearest(&self, p: [f32; 3], r: f32) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for i in 0..self.n {
            let f = &self.f[i];
            if !f.alive || (f.pos[1] - p[1]).abs() > 400.0 {
                continue;
            }
            let d = dist2(pos_of(f), [p[0], p[2]]);
            if d <= r && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best
    }

    /// A hazard's blow on fighter `i` (once per attack): a normal hit with no attacker.
    fn hz_strike(&mut self, h: usize, i: usize, dir: V2, scale: f32) -> bool {
        let hz = self.arena.hazards[h];
        if hz.mask & (1 << i) != 0 || !self.f[i].alive || self.f[i].inv > 0 {
            return false;
        }
        self.arena.hazards[h].mask |= 1 << i;
        let sp = hz.spec();
        self.hit(i, -1, sp.dmg * scale, dir, sp.kb * scale, sp.up)
    }

    /// Strike every fighter inside `zone` whose feet are under `top` (local height); knock along `dir` or out of the zone.
    fn hz_strike_zone(&mut self, h: usize, zone: Zone, top: f32, dir: Option<V2>) -> u32 {
        let mut n = 0;
        for i in 0..self.n {
            let f = self.f[i];
            if !f.alive || f.pos[1] > top || f.pos[1] < top - 900.0 {
                continue;
            }
            let p = pos_of(&f);
            if !zone.contains(p, f.radius()) {
                continue;
            }
            let d = dir.unwrap_or_else(|| zone.escape(p));
            if self.hz_strike(h, i, d, 1.0) {
                n += 1;
            }
        }
        n
    }

    fn hz_lunger(&mut self, h: usize, calm: bool) {
        let hz = self.arena.hazards[h];
        let sp = hz.spec();
        let home = hz.home;
        let hz = &mut self.arena.hazards[h];
        hz.t = hz.t.saturating_add(1);
        match hz.st {
            HzState::Idle => {
                // Bob about the post, facing whoever is closest.
                let w = self.tick as f32 * 0.09 + h as f32;
                let rest = [home[0] + w.cos() * 60.0, home[1], home[2] + w.sin() * 60.0];
                hz.pos = rest;
                hz.cd -= 1;
                if hz.cd > 0 || calm {
                    return;
                }
                let Some((j, _)) = self.hz_nearest(home, sp.reach + 80.0) else {
                    self.arena.hazards[h].cd = 10;
                    return;
                };
                let f = self.f[j];
                let lead = [f.pos[0] + f.vel[0] * 0.25, f.pos[2] + f.vel[2] * 0.25];
                let off = [lead[0] - home[0], lead[1] - home[2]];
                let d = len2(off).max(1.0);
                let reach = d.min(sp.reach);
                let dir = norm2(off, [1.0, 0.0]);
                let at = [home[0] + dir[0] * reach, home[2] + dir[1] * reach];
                let y = self.hz_floor(at[0], at[1], home[1] + 100.0).unwrap_or(home[1]);
                let hz = &mut self.arena.hazards[h];
                hz.aim = [at[0], y, at[1]];
                hz.dir = dir;
                hz.yaw = yaw_of(dir);
                hz.target = j as i8;
                self.hz_go(h, HzState::Warn);
            }
            HzState::Warn => {
                // Rear back from the post.
                let k = hz.t as f32 / sp.warn as f32;
                hz.pos = [home[0] - hz.dir[0] * 70.0 * k, home[1] + 30.0 * k, home[2] - hz.dir[1] * 70.0 * k];
                if hz.t >= sp.warn {
                    self.hz_go(h, HzState::Act);
                    let p = self.arena.hazards[h].aim;
                    self.ev.push(Ev::Hazard { look: hz_look(self, h), pos: p, strike: true });
                }
            }
            HzState::Act => {
                let k = (hz.t as f32 / sp.act as f32).min(1.0);
                let from = [home[0] - hz.dir[0] * 70.0, home[2] - hz.dir[1] * 70.0];
                let p = [from[0] + (hz.aim[0] - from[0]) * k, hz.aim[1] + 40.0 * (1.0 - k), from[1] + (hz.aim[2] - from[1]) * k];
                hz.pos = p;
                let (t, dir) = (hz.t, hz.dir);
                let zone = Zone::Circle { c: [p[0], p[2]], r: sp.radius };
                self.hz_strike_zone(h, zone, p[1] + sp.height, Some(dir));
                if t >= sp.act + 3 {
                    self.hz_go(h, HzState::Rest);
                }
            }
            HzState::Rest => {
                let k = (hz.t as f32 / sp.rest as f32).min(1.0);
                hz.pos = [hz.aim[0] + (home[0] - hz.aim[0]) * k, home[1], hz.aim[2] + (home[2] - hz.aim[2]) * k];
                if hz.t >= sp.rest {
                    self.hz_go(h, HzState::Idle);
                    self.hz_cooldown(h);
                }
            }
        }
    }

    fn hz_slam(&mut self, h: usize, calm: bool) {
        let hz = self.arena.hazards[h];
        let sp = hz.spec();
        let leash = 420.0;
        self.arena.hazards[h].t = hz.t.saturating_add(1);
        match hz.st {
            HzState::Idle => {
                // Plod after the nearest fighter, never further than the leash from home and only over solid ground.
                let near = self.hz_nearest(hz.pos, 2400.0);
                if let Some((j, d)) = near {
                    let to = norm2([self.f[j].pos[0] - hz.pos[0], self.f[j].pos[2] - hz.pos[2]], hz.dir);
                    let turn = angle_diff(hz.yaw, yaw_of(to)).clamp(-0.06, 0.06);
                    let yaw = hz.yaw + turn;
                    let fwd = dir_of(yaw);
                    let mut np = hz.pos;
                    if d > sp.reach * 0.55 {
                        np[0] += fwd[0] * sp.speed * DT;
                        np[2] += fwd[1] * sp.speed * DT;
                    }
                    let in_leash = dist2([np[0], np[2]], [hz.home[0], hz.home[2]]) <= leash;
                    let ground = self.hz_floor(np[0], np[2], hz.pos[1]).filter(|y| (y - hz.pos[1]).abs() < 60.0 && self.arena.over_floor(np[0], np[2]));
                    let hzm = &mut self.arena.hazards[h];
                    hzm.yaw = yaw;
                    hzm.dir = fwd;
                    if let (true, Some(y)) = (in_leash, ground) {
                        hzm.pos = [np[0], y, np[2]];
                    }
                    hzm.cd -= 1;
                    // Rear up when someone is in the lane ahead.
                    let lane = Zone::Lane { a: [hzm.pos[0], hzm.pos[2]], dir: fwd, len: sp.reach, half: sp.radius };
                    let pos = hzm.pos;
                    if hzm.cd <= 0 && !calm && (0..self.n).any(|i| self.f[i].alive && (self.f[i].pos[1] - pos[1]).abs() < 300.0 && lane.contains(pos_of(&self.f[i]), 0.0)) {
                        let hzm = &mut self.arena.hazards[h];
                        hzm.aim = [pos[0] + fwd[0] * sp.reach * 0.5, pos[1], pos[2] + fwd[1] * sp.reach * 0.5];
                        self.hz_go(h, HzState::Warn);
                    }
                }
            }
            HzState::Warn => {
                if hz.t >= sp.warn {
                    self.hz_go(h, HzState::Act);
                }
            }
            HzState::Act => {
                if hz.t + 1 == sp.act {
                    let zone = Zone::Lane { a: [hz.pos[0], hz.pos[2]], dir: hz.dir, len: sp.reach, half: sp.radius };
                    self.hz_strike_zone(h, zone, hz.pos[1] + sp.height, None);
                    self.ev.push(Ev::Hazard { look: hz.look, pos: hz.aim, strike: true });
                }
                if hz.t >= sp.act {
                    self.hz_go(h, HzState::Rest);
                }
            }
            HzState::Rest => {
                if hz.t >= sp.rest {
                    self.hz_go(h, HzState::Idle);
                    self.hz_cooldown(h);
                }
            }
        }
    }

    fn hz_drop(&mut self, h: usize, calm: bool) {
        let hz = self.arena.hazards[h];
        let sp = hz.spec();
        let hover = 420.0;
        self.arena.hazards[h].t = hz.t.saturating_add(1);
        match hz.st {
            HzState::Idle => {
                // Drift back over home, then pick someone within range of it.
                let hzm = &mut self.arena.hazards[h];
                let to = [hz.home[0] - hz.pos[0], hz.home[2] - hz.pos[2]];
                let step = (sp.speed * 0.5 * DT).min(len2(to));
                let d = norm2(to, [0.0, 0.0]);
                hzm.pos = [hz.pos[0] + d[0] * step, hz.home[1] + hover, hz.pos[2] + d[1] * step];
                hzm.cd -= 1;
                if hzm.cd > 0 || calm {
                    return;
                }
                match self.hz_nearest(hz.home, 900.0) {
                    Some((j, _)) => {
                        let hzm = &mut self.arena.hazards[h];
                        hzm.target = j as i8;
                        hzm.aim = [hzm.pos[0], hz.home[1], hzm.pos[2]];
                        self.hz_go(h, HzState::Warn);
                    }
                    None => self.arena.hazards[h].cd = 15,
                }
            }
            HzState::Warn => {
                // Track the target for the first part, then lock (the last 12 ticks are a fixed shadow to step out of).
                let lock = hz.t + 12 >= sp.warn;
                if !lock && hz.target >= 0 {
                    let f = self.f[hz.target as usize];
                    let to = [f.pos[0] - hz.pos[0], f.pos[2] - hz.pos[2]];
                    let step = (sp.speed * DT).min(len2(to));
                    let d = norm2(to, [0.0, 0.0]);
                    let np = [hz.pos[0] + d[0] * step, hz.pos[2] + d[1] * step];
                    // Never leave the floor of the arena or stray far from home.
                    if self.arena.over_floor(np[0], np[1]) && dist2(np, [hz.home[0], hz.home[2]]) < 900.0 {
                        let y = self.hz_floor(np[0], np[1], hz.home[1] + 200.0).unwrap_or(hz.aim[1]);
                        let hzm = &mut self.arena.hazards[h];
                        hzm.pos = [np[0], y + hover, np[1]];
                        hzm.aim = [np[0], y, np[1]];
                    }
                }
                if hz.t >= sp.warn {
                    self.hz_go(h, HzState::Act);
                }
            }
            HzState::Act => {
                let k = (hz.t as f32 / sp.act as f32).min(1.0);
                self.arena.hazards[h].pos[1] = hz.aim[1] + hover * (1.0 - k * k);
                if hz.t == sp.act {
                    let zone = Zone::Square { c: [hz.aim[0], hz.aim[2]], half: sp.reach };
                    self.hz_strike_zone(h, zone, hz.aim[1] + sp.height, None);
                    self.ev.push(Ev::Hazard { look: hz.look, pos: hz.aim, strike: true });
                    self.hz_go(h, HzState::Rest);
                }
            }
            HzState::Rest => {
                // Sit, then rise for the second half.
                let half = sp.rest / 2;
                if hz.t > half {
                    let k = (hz.t - half) as f32 / half.max(1) as f32;
                    self.arena.hazards[h].pos[1] = hz.aim[1] + hover * k.min(1.0);
                }
                if hz.t >= sp.rest {
                    self.hz_go(h, HzState::Idle);
                    self.hz_cooldown(h);
                }
            }
        }
    }

    fn hz_roller(&mut self, h: usize, calm: bool) {
        let hz = self.arena.hazards[h];
        let sp = hz.spec();
        self.arena.hazards[h].t = hz.t.saturating_add(1);
        match hz.st {
            HzState::Idle | HzState::Rest => {
                let hzm = &mut self.arena.hazards[h];
                hzm.cd -= 1;
                if hzm.cd > 0 || calm || self.alive_count() == 0 {
                    return;
                }
                // Aim a lane through a fighter: the region's direction (either way) or any direction.
                let alive: Vec<usize> = (0..self.n).filter(|&i| self.f[i].alive).collect();
                let j = alive[self.rng.below(alive.len())];
                let tp = pos_of(&self.f[j]);
                let dir = if hz.hint != [0.0, 0.0] {
                    let d = norm2(hz.hint, [1.0, 0.0]);
                    // Waves always come in from the sea; the rest may run either way along the hint.
                    if hz.look != Look::Wave && self.rng.chance(0.5) { [-d[0], -d[1]] } else { d }
                } else {
                    let a = self.rng.range(0.0, std::f32::consts::TAU);
                    [a.cos(), a.sin()]
                };
                // Waves sweep the whole width: the lane starts at the sea edge, offset across to pass the target.
                let half = self.arena.half;
                let reach = (half[0] * dir[0].abs() + half[1] * dir[1].abs()) * 1.05 + 150.0;
                let (start, span) = if hz.look == Look::Wave {
                    let back = tp[0] * dir[0] + tp[1] * dir[1] + reach;
                    ([tp[0] - dir[0] * back, tp[1] - dir[1] * back], 2.0 * reach)
                } else {
                    let back = reach;
                    ([tp[0] - dir[0] * back, tp[1] - dir[1] * back], back + reach)
                };
                // On the ground under the target (a fighter in the air does not lift the lane).
                let fy = self.f[j].pos[1];
                let y = self.hz_floor(tp[0], tp[1], fy + 40.0).unwrap_or(fy);
                let hzm = &mut self.arena.hazards[h];
                hzm.dir = dir;
                hzm.yaw = yaw_of(dir);
                hzm.aim = [start[0], y, start[1]];
                hzm.span = span;
                hzm.run = 0.0;
                hzm.target = j as i8;
                hzm.pos = [start[0], y, start[1]];
                self.hz_go(h, HzState::Warn);
            }
            HzState::Warn => {
                if hz.t >= sp.warn {
                    self.hz_go(h, HzState::Act);
                    self.ev.push(Ev::Hazard { look: hz.look, pos: hz.aim, strike: true });
                }
            }
            HzState::Act => {
                let run = hz.run + sp.speed * DT;
                let p = [hz.aim[0] + hz.dir[0] * run, hz.aim[2] + hz.dir[1] * run];
                // Follow the ground (a body off the edge keeps its height and rolls on through the air).
                let y = self.hz_floor(p[0], p[1], hz.pos[1] + 60.0).filter(|y| *y > hz.pos[1] - 220.0).unwrap_or(hz.pos[1]);
                let hzm = &mut self.arena.hazards[h];
                hzm.run = run;
                hzm.pos = [p[0], y, p[1]];
                // A giant Goomba is flattened by whoever lands on it from above.
                if hz.look == Look::Goomba {
                    for i in 0..self.n {
                        let f = self.f[i];
                        if f.alive && !f.grounded && f.vel[1] < 0.0 && f.pos[1] > y + sp.height * 0.7 && f.pos[1] < y + sp.height + 60.0 && dist2(pos_of(&f), p) < sp.radius + f.radius() {
                            self.f[i].vel[1] = 950.0;
                            self.ev.push(Ev::Hazard { look: Look::Goomba, pos: [p[0], y + sp.height, p[1]], strike: false });
                            self.ev.push(Ev::MonKo { pos: [p[0], y + 60.0, p[1]] });
                            self.arena.hazards[h].run = self.arena.hazards[h].span;
                        }
                    }
                }
                let zone = if hz.look == Look::Wave {
                    // The wave is a wall across the whole arena.
                    let w = self.arena.half[0].max(self.arena.half[1]) * 2.0;
                    Zone::Lane { a: [p[0] - hz.dir[0] * sp.radius, p[1] - hz.dir[1] * sp.radius], dir: hz.dir, len: sp.radius * 2.0, half: w }
                } else {
                    Zone::Circle { c: p, r: sp.radius }
                };
                let dir = hz.dir;
                // A little sideways too, away from the line, so nobody is carried along.
                for i in 0..self.n {
                    let f = self.f[i];
                    if !f.alive || f.pos[1] > y + sp.height || f.pos[1] < y - 200.0 || !zone.contains(pos_of(&f), f.radius()) {
                        continue;
                    }
                    let q = [f.pos[0] - p[0], f.pos[2] - p[1]];
                    let side = q[0] * dir[1] - q[1] * dir[0];
                    let n = if side >= 0.0 { [dir[1], -dir[0]] } else { [-dir[1], dir[0]] };
                    let k = if hz.look == Look::Wave { 0.0 } else { 0.45 };
                    let d = norm2([dir[0] + n[0] * k, dir[1] + n[1] * k], dir);
                    self.hz_strike(h, i, d, 1.0);
                }
                if self.arena.hazards[h].run >= self.arena.hazards[h].span {
                    self.hz_go(h, HzState::Idle);
                    self.hz_cooldown(h);
                }
            }
        }
    }

    fn hz_drifter(&mut self, h: usize, calm: bool) {
        let hz = self.arena.hazards[h];
        let sp = hz.spec();
        self.arena.hazards[h].t = hz.t.saturating_add(1);
        // (Re)pick a target now and then: the nearest live fighter.
        let mut target = hz.target;
        if hz.run <= 0.0 || target < 0 || !self.f[target as usize].alive {
            target = self.hz_nearest(hz.pos, 1e6).map_or(-1, |t| t.0 as i8);
            let hzm = &mut self.arena.hazards[h];
            hzm.target = target;
            hzm.run = 100.0;
        } else {
            self.arena.hazards[h].run -= 1.0;
        }
        let float = match hz.look {
            Look::Boo => 70.0 + 18.0 * (self.tick as f32 * 0.12 + h as f32).sin(),
            _ => 0.0,
        };
        match hz.st {
            HzState::Idle | HzState::Warn => {
                if target < 0 {
                    return;
                }
                let f = self.f[target as usize];
                let to = [f.pos[0] - hz.pos[0], f.pos[2] - hz.pos[2]];
                let d = len2(to);
                let dir = norm2(to, hz.dir);
                // A Boo only creeps up while its target looks away (and hides, harmless, while watched).
                let watched = hz.look == Look::Boo && {
                    let facing = dir_of(f.yaw);
                    facing[0] * -dir[0] + facing[1] * -dir[1] > 0.2
                };
                let mut speed = if watched { 0.0 } else { sp.speed };
                if calm {
                    speed *= 0.5;
                }
                // A Redead screams once it is close: everyone in the ring freezes.
                if hz.look == Look::Redead {
                    if hz.st == HzState::Idle && d < sp.reach * 0.7 && !calm {
                        let pos = hz.pos;
                        self.arena.hazards[h].aim = pos;
                        self.hz_go(h, HzState::Warn);
                        return;
                    }
                    if hz.st == HzState::Warn {
                        speed = 0.0;
                        if hz.t >= sp.warn {
                            self.ev.push(Ev::Hazard { look: hz.look, pos: hz.pos, strike: true });
                            for i in 0..self.n {
                                let g = self.f[i];
                                if g.alive && g.inv == 0 && dist2(pos_of(&g), [hz.pos[0], hz.pos[2]]) < sp.reach {
                                    let fi = &mut self.f[i];
                                    fi.hp -= 3.0;
                                    fi.act = Act::Hurt;
                                    fi.act_t = 0;
                                    fi.stun = sp.act;
                                    fi.vel[0] = 0.0;
                                    fi.vel[2] = 0.0;
                                    fi.inv = 4;
                                    self.ev.push(Ev::Hit { pos: [g.pos[0], g.pos[1] + 70.0 * g.size, g.pos[2]], big: false, victim: i });
                                    if self.f[i].hp <= 0.0 {
                                        let by = self.f[i].last_hit_by;
                                        self.ko(i, by);
                                    }
                                }
                            }
                            // A long breather before it can scream again (it shambles on, but bites only after it).
                            self.hz_go(h, HzState::Rest);
                            return;
                        }
                    }
                }
                // Move, staying inside the rim and over the arena's floor.
                // It shambles up to body contact (its body is solid, `brawl_body.rs`) and no further; its touch still reaches.
                let stop = if hz.look == Look::Redead { (sp.radius + f.radius()) * crate::experiments::body::SQUEEZE } else { sp.radius * 0.5 };
                let step = (speed * DT).min((d - stop).max(0.0));
                let np = [hz.pos[0] + dir[0] * step, hz.pos[2] + dir[1] * step];
                let lim = self.arena.bounds.unwrap_or(self.arena.half);
                let inside = np[0].abs() < lim[0] * 0.95 && np[1].abs() < lim[1] * 0.95;
                let mut ground = self.hz_floor(np[0], np[1], hz.pos[1] - float + 80.0).filter(|y| (y - (hz.pos[1] - float)).abs() < 90.0);
                // A Boo flies: over a pond or a gap it keeps its height.
                if ground.is_none() && hz.look == Look::Boo && self.arena.support(np[0], np[1], hz.pos[1] + 80.0).is_none_or(|s| s.0 < hz.pos[1] - float - 90.0) {
                    ground = Some(hz.pos[1] - float);
                }
                let hzm = &mut self.arena.hazards[h];
                hzm.dir = dir;
                hzm.yaw = yaw_of(dir);
                if let (true, Some(y)) = (inside, ground) {
                    hzm.pos = [np[0], y + float, np[1]];
                }
                if watched || calm {
                    return;
                }
                // Touch.
                let p = [hzm.pos[0], hzm.pos[2]];
                let base = hzm.pos[1] - float;
                for i in 0..self.n {
                    let g = self.f[i];
                    if !g.alive || g.inv > 0 || g.pos[1] > base + sp.height || g.pos[1] < base - 150.0 || dist2(pos_of(&g), p) > sp.radius + g.radius() {
                        continue;
                    }
                    let away = norm2([g.pos[0] - p[0], g.pos[2] - p[1]], dir);
                    self.arena.hazards[h].mask = 0;
                    if self.hz_strike(h, i, away, 1.0) {
                        if hz.look == Look::Tornado {
                            // Flung high into the air, spinning.
                            self.f[i].vel[1] = sp.up;
                            self.f[i].grounded = false;
                        }
                        self.ev.push(Ev::Hazard { look: hz.look, pos: g.pos, strike: true });
                        self.hz_go(h, HzState::Rest);
                        break;
                    }
                }
            }
            HzState::Act => self.hz_go(h, HzState::Rest),
            HzState::Rest => {
                // Back off for a moment (a Boo retreats, the Tweester settles).
                if target >= 0 && hz.look == Look::Boo {
                    let f = self.f[target as usize];
                    let away = norm2([hz.pos[0] - f.pos[0], hz.pos[2] - f.pos[2]], [1.0, 0.0]);
                    let np = [hz.pos[0] + away[0] * sp.speed * DT, hz.pos[2] + away[1] * sp.speed * DT];
                    let lim = self.arena.bounds.unwrap_or(self.arena.half);
                    if np[0].abs() < lim[0] * 0.95 && np[1].abs() < lim[1] * 0.95 {
                        if let Some(y) = self.hz_floor(np[0], np[1], hz.pos[1]).filter(|y| (y - (hz.pos[1] - float)).abs() < 90.0) {
                            self.arena.hazards[h].pos = [np[0], y + float, np[1]];
                        }
                    }
                }
                if hz.t >= sp.rest {
                    self.hz_go(h, HzState::Idle);
                }
            }
        }
    }

    /// Is `p` (arena-local x, z) inside ground a hazard is about to strike (or a drifter's touch), padded by `pad`?
    pub fn hazard_danger_at(&self, p: V2, pad: f32) -> bool {
        self.arena.hazards.iter().any(|h| h.zone().is_some_and(|z| z.contains(p, pad)))
    }

    /// The first hazard zone containing `p` (padded), for an escape direction.
    pub fn hazard_zone_at(&self, p: V2, pad: f32) -> Option<Zone> {
        self.arena.hazards.iter().filter_map(|h| h.zone()).find(|z| z.contains(p, pad))
    }

    /// A hazard ready to bite near `p` (within `r`): an anchored biter's reach or a drifter, or a lane / footprint being
    /// telegraphed. Returns a point of it, so a bot can knock a foe toward it.
    pub fn hazard_near(&self, p: V2, r: f32) -> Option<V2> {
        for h in &self.arena.hazards {
            let sp = h.spec();
            let c = match (sp.kind, h.zone()) {
                (_, Some(z)) if z.contains(p, r) => z.centre(),
                (HzKind::Lunger, _) if h.st == HzState::Idle && h.cd < 30 && dist2(p, [h.home[0], h.home[2]]) < sp.reach + r => [h.home[0], h.home[2]],
                _ => continue,
            };
            return Some(c);
        }
        None
    }

    /// Course hazard `h`'s solid body for the body pass (`brawl_body.rs`): `(centre, base height, height, radius, lane)`, or
    /// `None` while it has no body a fighter could stand in. Set pieces are immovable: a fighter is pushed out of them. Solid:
    /// the Chain Chomp's head and the Deku Baba (always), a Whomp while it stands (flat on its face it is a slab you walk
    /// over), the Thwomp (hovering 420 up it is over everyone's head; down, it is a block), the rollers while they cross
    /// (`lane` = their direction: a fighter is pushed sideways out of the lane, never carried along it) and the Redead.
    /// Intangible by design: the Boo (a ghost), the Tweester (wind: it flings whoever it touches) and the wave (water).
    pub(super) fn hz_body(&self, h: usize) -> Option<(V2, f32, f32, f32, Option<V2>)> {
        let hz = &self.arena.hazards[h];
        let sp = hz.spec();
        let c = [hz.pos[0], hz.pos[2]];
        match (hz.look, sp.kind) {
            (Look::Boo | Look::Tornado | Look::Wave, _) => None,
            (_, HzKind::Lunger) | (_, HzKind::Drop) | (_, HzKind::Drifter) => Some((c, hz.pos[1], sp.height, sp.radius, None)),
            (_, HzKind::Slam) => matches!(hz.st, HzState::Idle | HzState::Warn).then_some((c, hz.pos[1], sp.height, sp.radius, None)),
            (_, HzKind::Roller) => (hz.st == HzState::Act).then_some((c, hz.pos[1], sp.height, sp.radius, Some(hz.dir))),
        }
    }

    pub(super) fn hazards_digest(&self, mix: &mut impl FnMut(f32)) {
        for h in &self.arena.hazards {
            for v in h.pos {
                mix(v);
            }
            mix(h.st as u8 as f32 + h.t as f32 * 4.0);
        }
    }
}

fn hz_look(b: &Brawl, h: usize) -> Look {
    b.arena.hazards[h].look
}

/// The runtime hazards of a region's list, homes lifted onto `ground` (arena-local).
pub fn build_hazards(defs: &[HazardDef], floor: impl Fn(f32, f32) -> f32) -> Vec<Hazard> {
    defs.iter().take(MAXH).enumerate().map(|(k, d)| Hazard::new(d, [d.at[0], floor(d.at[0], d.at[1]), d.at[1]], k)).collect()
}
