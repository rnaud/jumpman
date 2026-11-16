//! Gem Clash, the Smash fighters (engine-free, deterministic): a `Kind::Mario` fighter with `smash: Some(_)` fights with
//! its SSB64 moveset.
//!
//! - **Specials** run on the fighter's [`Seat`]: a private copy of Mario's sim (`MarioRuntime` on a flat floor) whose
//!   `jumpman_sim::smash` layer plays the special exactly as on the courses. The seat is placed where the fighter
//!   stands (its floor at the arena's floor under the fighter, re-anchored when that changes), stepped with the
//!   fighter's input while the special runs (`Act::Special`), and its displacement is fed back through Gem Clash's own
//!   collision (`integrate`: walls, floors, the void). Its projectiles keep flying after the special (the seat coasts
//!   them every tick). Hitboxes and projectiles resolve against the other fighters and the monsters ([`GemTargets`]).
//! - **Normal attacks**: the punch chain, the dive, the ground pound and its landing play the fighter's scripted
//!   motions (`Attack11` / `Attack12` / `Attack13`, `AttackAirF`, `AttackAirLw`, `LandingAirLw`) with the motion event
//!   scripts' hitboxes, damage and knockback when the import has them (`Brawl::scripts`); otherwise Mario's blows.
//! - **Knockback**: every Smash hit (special, projectile, scripted attack) launches with SSB64's formula
//!   (`jumpman_sim::smash_knockback`): the victim's percent ([`Fighter::percent`], its lost health), the victim's SSB64
//!   weight, hitstun, hitlag on both sides, the launch velocity decaying 1.7 units a frame (`Fighter::kvel`). A launched
//!   fighter (damage level 3) tumbles over the invisible rim and is knocked out beyond the blast zones
//!   ([`Brawl::blast_zone`]: the rim or the floor's half size plus [`BLAST_MARGIN`], [`BLAST_TOP`] above). Health still
//!   knocks out at 0 as for everyone in Gem Clash.
//! - **Balance**: [`tuning`] per fighter (health, speed, damage, knockback dealt), measured by the round-robin harness
//!   (`brawl_balance_tests.rs`, `native/SSB64.md` "Balance").
use super::*;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
use jumpman_sim::smash::{HitKind, HitOutcome, Incoming, SmashHit, SmashTargets, SpecialMove, Target, K};
use jumpman_sim::smash_hitbox::{to_world, Motion};
use jumpman_sim::smash_knockback::{self as skb, BlastZone, Launch};
use jumpman_sim::state::{ControllerSnapshot, ACT_FREEFALL, ACT_IDLE, ACT_LEDGE_GRAB, A_BUTTON, R_TRIG, Z_TRIG};
use std::sync::{Arc, OnceLock};

/// Blast zones: this far beyond the rim (or the main floor's half size), and this high above the arena's floor.
pub const BLAST_MARGIN: f32 = 520.0;
pub const BLAST_TOP: f32 = 2600.0;
/// Gem Clash health per SSB64 percent of a Smash hit (the bars are 100 points: one percent, one point).
pub const SMASH_DAMAGE: f32 = 1.0;
/// Playback rate of the punch chain's motions (the view's 1.15x).
pub const PUNCH_RATE: f32 = 1.15;

/// Per-fighter balance knobs (`native/SSB64.md` "Balance"): health, run speed, damage dealt and knockback dealt
/// multipliers. 1.0 everywhere is SSB64's own numbers on Mario's body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmashTuning {
    pub hp: f32,
    pub speed: f32,
    pub power: f32,
    pub knock: f32,
}

const fn t(hp: f32, speed: f32, power: f32, knock: f32) -> SmashTuning {
    SmashTuning { hp, speed, power, knock }
}

/// The balance table (round-robin harness, `brawl_balance_tests.rs`).
pub fn tuning(f: SmashFighter) -> SmashTuning {
    match f {
        SmashFighter::Mario => t(100.0, 1.0, 1.08, 1.0),
        SmashFighter::Luigi => t(100.0, 1.0, 1.08, 1.0),
        SmashFighter::Donkey => t(96.0, 1.0, 0.77, 1.0),
        SmashFighter::Link => t(100.0, 1.0, 1.0, 1.0),
        SmashFighter::Samus => t(100.0, 1.0, 1.08, 1.0),
        SmashFighter::Yoshi => t(100.0, 1.0, 1.0, 1.0),
        SmashFighter::Kirby => t(95.0, 1.0, 0.81, 1.0),
        SmashFighter::Fox => t(100.0, 1.0, 1.08, 1.0),
        SmashFighter::Pikachu => t(100.0, 1.0, 1.0, 1.0),
        SmashFighter::Purin => t(100.0, 1.0, 1.0, 1.0),
        SmashFighter::Captain => t(100.0, 1.0, 1.0, 1.0),
        SmashFighter::Ness => t(104.0, 1.0, 1.12, 1.0),
    }
}

/// The fighter's name as Gem Clash prints it (cards, banners).
pub fn name(f: SmashFighter) -> &'static str {
    match f {
        SmashFighter::Mario => "Mario",
        SmashFighter::Luigi => "Luigi",
        SmashFighter::Donkey => "Donkey Kong",
        SmashFighter::Link => "Link",
        SmashFighter::Samus => "Samus",
        SmashFighter::Yoshi => "Yoshi",
        SmashFighter::Kirby => "Kirby",
        SmashFighter::Fox => "Fox",
        SmashFighter::Pikachu => "Pikachu",
        SmashFighter::Purin => "Jigglypuff",
        SmashFighter::Captain => "C. Falcon",
        SmashFighter::Ness => "Ness",
    }
}

/// A Smash fighter's private copy of Mario's sim: it plays the specials (`jumpman_sim::smash`).
pub struct Seat {
    pub rt: MarioRuntime,
    /// Arena height of the seat's floor (`y = 0` in the seat).
    pub base_y: f32,
    /// Stepped by the special this tick (else the projectiles coast).
    driven: bool,
}

fn flat_world() -> Arc<CollisionWorld> {
    static W: OnceLock<Arc<CollisionWorld>> = OnceLock::new();
    W.get_or_init(|| {
        let mut w = CollisionWorld::default();
        let o = SurfaceOptions::default();
        let e = 30000.0;
        w.add_triangle([-e, 0.0, -e], [-e, 0.0, e], [e, 0.0, e], &o);
        w.add_triangle([-e, 0.0, -e], [e, 0.0, e], [e, 0.0, -e], &o);
        Arc::new(w)
    })
    .clone()
}

impl Seat {
    pub fn new(f: SmashFighter, scripts: Option<Arc<jumpman_sim::smash_hitbox::MotionEvents>>) -> Self {
        let mut rt = MarioRuntime::new(flat_world(), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -40000.0);
        rt.smash.fighter = Some(f);
        rt.smash.scripts = scripts;
        Seat { rt, base_y: 0.0, driven: false }
    }

    /// Move the seat's floor to arena height `floor`, keeping what flies where it is.
    fn anchor(&mut self, floor: f32) {
        let d = (self.base_y - floor) as f64;
        if d.abs() < 0.5 {
            return;
        }
        for p in self.rt.smash.projectiles.iter_mut() {
            p.pos[1] += d;
        }
        self.base_y = floor;
    }

    /// The running special (for the view and the bots).
    pub fn special(&self) -> Option<jumpman_sim::smash::Special> {
        self.rt.smash.special
    }
}

/// Gem Clash yaw (radians, `dir_of`) -> SM64 s16 yaw, and back.
pub fn yaw16(yaw: f32) -> f64 {
    let v = (yaw as f64 / std::f64::consts::TAU * 65536.0).rem_euclid(65536.0);
    if v >= 32768.0 { v - 65536.0 } else { v }
}
pub fn yaw_rad(y16: f64) -> f32 {
    (y16 / 65536.0 * std::f64::consts::TAU) as f32
}

/// The SSB64 motions Gem Clash's acts play for a Smash fighter (best first), and their rate.
pub fn normal_motion(act: Act) -> Option<(&'static [&'static str], f32)> {
    Some(match act {
        Act::Punch(0) => (&["Attack11"][..], PUNCH_RATE),
        Act::Punch(1) => (&["Attack12", "Attack11"][..], PUNCH_RATE),
        Act::Punch(_) => (&["Attack13", "AttackS3"][..], PUNCH_RATE),
        Act::Dive => (&["AttackAirF"][..], 1.0),
        Act::PoundWind | Act::Pound => (&["AttackAirLw"][..], 1.2),
        Act::PoundLand => (&["LandingAirLw"][..], 1.0),
        _ => return None,
    })
}

impl Brawl {
    /// Test / harness hook: the motion event scripts the Smash fighters use (also their seats'); the app keeps the
    /// registered table the match was created with.
    #[allow(dead_code)]
    pub fn set_scripts(&mut self, s: Option<Arc<jumpman_sim::smash_hitbox::MotionEvents>>) {
        self.scripts = s.clone();
        for seat in self.seats.iter_mut().flatten() {
            seat.rt.smash.scripts = s.clone();
        }
    }

    pub(super) fn smash_begin_round(&mut self) {
        for i in 0..MAXF {
            self.seats[i] = match (i < self.n, self.f[i].smash) {
                (true, Some(sf)) => Some(Box::new(Seat::new(sf, self.scripts.clone()))),
                _ => None,
            };
        }
    }

    /// Blast zones of the arena (see the module doc).
    pub fn blast_zone(&self) -> BlastZone {
        let half = self.arena.bounds.unwrap_or(self.arena.half);
        let top = self.arena.spawns.iter().filter_map(|s| self.arena.support(s[0], s[1], 5000.0).map(|t| t.0)).fold(0.0f32, f32::max);
        BlastZone::around([0.0; 3], half, BLAST_MARGIN, self.arena.void_y, top + BLAST_TOP)
    }

    /// Kirby's Stone (and any armoured phase): nothing hurts the fighter.
    pub(super) fn smash_armored(&self, v: usize) -> bool {
        self.f[v].act == Act::Special && self.seats[v].as_ref().is_some_and(|s| s.rt.smash.armor())
    }

    fn floor_under(&self, i: usize) -> f32 {
        let p = self.f[i].pos;
        self.arena.support(p[0], p[2], p[1] + STEP).map_or(self.arena.void_y - 50.0, |t| t.0)
    }

    /// Start a special (`Input::special`). Returns true when the fighter is now in it.
    pub(super) fn smash_start(&mut self, i: usize, inp: Input) -> bool {
        let floor = self.floor_under(i);
        let f = self.f[i];
        let Some(seat) = self.seats[i].as_mut() else { return false };
        if seat.rt.smash.special.is_some() {
            seat.rt.smash.special = None;
        }
        seat.anchor(floor);
        // Neutral and up specials turn to the stick first (as on the courses).
        let mut yaw = f.yaw;
        if len2(inp.mv) > 0.3 && inp.slot != 2 {
            yaw = yaw_of(inp.mv);
        }
        let rt = &mut seat.rt;
        rt.pos = [f.pos[0] as f64, (f.pos[1] - seat.base_y) as f64, f.pos[2] as f64];
        if f.grounded {
            rt.set_action(ACT_IDLE, 0.0);
            rt.vel = [0.0; 3];
            rt.forward_vel = 0.0;
        } else {
            rt.set_action(ACT_FREEFALL, 0.0);
            rt.vel = [f.vel[0] as f64 / 30.0, f.vel[1] as f64 / 30.0, f.vel[2] as f64 / 30.0];
        }
        rt.face_angle[1] = yaw16(yaw);
        rt.intended_yaw = rt.face_angle[1];
        self.f[i].yaw = yaw;
        self.start_act(i, Act::Special);
        self.smash_drive(i, inp, true);
        if self.f[i].act == Act::Special || !self.f[i].alive {
            true
        } else {
            // the move cannot start here (Hand Slap in the air): back to normal this same tick
            false
        }
    }

    /// The seat's controller for this tick (camera yaw 0: world direction (x, z) = stick (x, -y)).
    fn seat_controller(inp: Input, start: bool) -> ControllerSnapshot {
        let mut down = 0u32;
        let mut pressed = 0u32;
        if inp.special_held || (start && inp.special) {
            down |= R_TRIG;
        }
        if inp.special {
            pressed |= R_TRIG;
        }
        let (sx, sy) = if start { (0.0, 0.0) } else { (inp.mv[0] as f64 * 64.0, -inp.mv[1] as f64 * 64.0) };
        if start {
            match inp.slot {
                1 => down |= A_BUTTON,
                2 => down |= Z_TRIG,
                _ => {}
            }
        } else {
            if inp.jump {
                down |= A_BUTTON;
                pressed |= A_BUTTON;
            }
            if inp.z {
                down |= Z_TRIG;
                pressed |= Z_TRIG;
            }
        }
        let mag = sx.hypot(sy).min(64.0);
        ControllerSnapshot { focus_yaw: None, stick_x: sx, stick_y: sy, stick_mag: mag, button_down: down, button_pressed: pressed }
    }

    /// One tick of a running special: step the seat with the fighter's input, move the fighter by the seat's motion
    /// through the arena's collision, resolve the hits.
    pub(super) fn smash_drive(&mut self, i: usize, inp: Input, start: bool) {
        let floor = self.floor_under(i);
        let Some(mut seat) = self.seats[i].take() else {
            self.end_act(i);
            return;
        };
        seat.anchor(floor);
        seat.driven = true;
        let old = self.f[i].pos;
        seat.rt.pos = [old[0] as f64, (old[1] - seat.base_y) as f64, old[2] as f64];
        let c = Self::seat_controller(inp, start);
        seat.rt.step(&c, 0.0, false, false);
        let rt = &seat.rt;
        let np = [rt.pos[0] as f32, rt.pos[1] as f32 + seat.base_y, rt.pos[2] as f32];
        let rv = [rt.vel[0] as f32 * 30.0, rt.vel[1] as f32 * 30.0, rt.vel[2] as f32 * 30.0];
        let ledge = rt.action == ACT_LEDGE_GRAB;
        {
            let f = &mut self.f[i];
            f.yaw = yaw_rad(rt.face_angle[1]);
            f.kvel = [0.0; 3];
            f.vel = [(np[0] - old[0]) / DT, (np[1] - old[1]) / DT, (np[2] - old[2]) / DT];
        }
        self.integrate(i);
        if self.f[i].alive {
            self.f[i].vel = if ledge { [0.0; 3] } else { rv };
        }
        self.smash_resolve(i, &mut seat);
        let active = seat.rt.smash.special.is_some();
        self.seats[i] = Some(seat);
        if !active && self.f[i].act == Act::Special {
            self.end_act(i);
        }
    }

    /// After the fighters stepped: the projectiles of every seat whose special did not run this tick fly on.
    pub(super) fn smash_coast(&mut self) {
        for i in 0..self.n {
            let Some(seat) = self.seats[i].as_mut() else { continue };
            if std::mem::take(&mut seat.driven) {
                continue;
            }
            if self.f[i].act != Act::Special && seat.rt.smash.special.is_some() {
                // knocked out of the special
                seat.rt.smash.special = None;
            }
            if seat.rt.smash.projectiles.is_empty() {
                continue;
            }
            let floor = self.floor_under(i);
            let mut seat = self.seats[i].take().unwrap();
            seat.anchor(floor);
            let p = self.f[i].pos;
            seat.rt.pos = [p[0] as f64, (p[1] - seat.base_y) as f64, p[2] as f64];
            seat.rt.smash.begin_tick();
            jumpman_sim::smash::after_step(&mut seat.rt);
            self.smash_resolve(i, &mut seat);
            self.seats[i] = Some(seat);
        }
    }

    fn smash_resolve(&mut self, i: usize, seat: &mut Seat) {
        if seat.rt.smash.hitboxes.is_empty() && seat.rt.smash.projectiles.is_empty() && !seat.rt.smash.reflecting() && !seat.rt.smash.absorbing() {
            return;
        }
        let mut t = GemTargets { b: self, me: i, base: seat.base_y, keys: vec![] };
        jumpman_sim::smash::resolve(&mut seat.rt.smash, &mut t);
        seat.rt.smash.projectiles.retain(|p| !p.dead);
    }

    /// An SSB64 hit on fighter `v` from fighter `a`: damage through [`Brawl::hit`] (credit, combos, gems, KOs), then
    /// SSB64's launch replaces the blow's knockback.
    pub fn smash_hit(&mut self, v: usize, a: usize, hit: &SmashHit) -> HitOutcome {
        let victim = self.f[v];
        if !victim.alive || victim.inv > 0 || (a != v && self.f[a].team == victim.team) || self.smash_armored(v) {
            return HitOutcome::Miss;
        }
        let me = self.f[a];
        let dir = norm2([hit.dir[0] as f32, hit.dir[2] as f32], norm2([victim.pos[0] - me.pos[0], victim.pos[2] - me.pos[2]], dir_of(me.yaw)));
        match hit.kind {
            HitKind::Sleep => {
                // Sing: asleep for longer at a higher percent
                let f = &mut self.f[v];
                f.act = Act::Hurt;
                f.act_t = 0;
                f.stun = (45.0 + f.percent() * 0.6).min(110.0) as u16;
                f.vel = [0.0, f.vel[1].min(0.0), 0.0];
                f.inv = 2;
                return HitOutcome::Hit;
            }
            HitKind::Grab => return HitOutcome::Hit,
            _ => {}
        }
        let knock = me.smash.map_or(1.0, |s| tuning(s).knock);
        // the attacker's damage multiplier (`tuning().power` for a Smash fighter, the hero's power otherwise) is applied
        // by `hit` through `Fighter::stats`
        let damage = if hit.kind == HitKind::Capture { hit.damage.max(8.0) } else { hit.damage } * SMASH_DAMAGE;
        let before = victim.percent();
        let hp0 = victim.hp;
        if !self.hit(v, a as i8, damage, dir, 0.0, 0.0) {
            return HitOutcome::Miss;
        }
        if !self.f[v].alive {
            return HitOutcome::Killed;
        }
        let dealt = (hp0 - self.f[v].hp).max(0.0) * 100.0 / victim.max_hp.max(1.0);
        let kb = skb::descriptor(hit);
        let weight = match victim.smash {
            Some(s) => skb::weight(s),
            None => (1.0 / victim.stats().weight).clamp(0.3, 1.5),
        } * if victim.powered() { 0.75 } else { 1.0 };
        let l = Launch::of(&kb, dealt, before, weight, !victim.grounded);
        self.apply_launch(v, a, dir, &l, knock);
        if hit.kind == HitKind::Capture && hit.mv == Some(SpecialMove::Inhale) {
            return HitOutcome::Captured(victim.smash);
        }
        HitOutcome::Hit
    }

    fn apply_launch(&mut self, v: usize, a: usize, dir: V2, l: &Launch, knock: f32) {
        let (h, up) = (l.horizontal * 30.0 * knock, l.vertical * 30.0 * knock);
        let boss = self.f[v].kind == Kind::KingBomb;
        let k = if boss { 0.45 } else { 1.0 };
        let f = &mut self.f[v];
        f.vel = [0.0, 0.0, 0.0];
        f.kvel = [dir[0] * h * k, up * k, dir[1] * h * k];
        if f.kvel[1] > 0.0 {
            f.grounded = false;
        }
        f.stun = (l.hitstun as u16).clamp(3, 90);
        f.act_t = 0;
        f.hit_mask = 0;
        f.splat = 0;
        f.inv = 2;
        if l.level >= 3 && !boss {
            f.act = Act::Down;
            f.launched = (l.hitstun as u16).max(10);
        } else {
            f.act = Act::Hurt;
            f.launched = 0;
        }
        let lag = l.hitlag.clamp(1, 10) as u8;
        f.hitstop = lag;
        if a != v {
            self.f[a].hitstop = self.f[a].hitstop.max(lag);
        }
    }

    /// The SSB64 launch's own motion: the knockback velocity decays (1.7 a frame along itself), the tumble counts down.
    pub(super) fn smash_launch_tick(&mut self, i: usize) {
        let f = &mut self.f[i];
        f.launched = f.launched.saturating_sub(1);
        if f.kvel == [0.0; 3] {
            return;
        }
        if f.grounded && f.kvel[1] < 0.0 {
            f.kvel[1] = 0.0;
        }
        let decay = skb::LAUNCH_DECAY * K as f32 * 4.0 * 30.0;
        let l = (f.kvel[0] * f.kvel[0] + f.kvel[1] * f.kvel[1] + f.kvel[2] * f.kvel[2]).sqrt();
        if l <= decay {
            f.kvel = [0.0; 3];
        } else {
            let s = (l - decay) / l;
            f.kvel = [f.kvel[0] * s, f.kvel[1] * s, f.kvel[2] * s];
        }
    }

    /// A launched fighter (or one in a special) beyond the blast zones is knocked out.
    pub(super) fn smash_blast_check(&mut self, i: usize) {
        let f = self.f[i];
        if !f.alive || (f.launched == 0 && f.act != Act::Special) {
            return;
        }
        if !self.blast_zone().contains(f.pos) {
            let by = if f.act == Act::Special && f.launched == 0 { -1 } else { f.last_hit_by };
            self.ko(i, by);
        }
    }

    fn scripted_motion(&self, i: usize, names: &[&str]) -> Option<&Motion> {
        let sf = self.f[i].smash?;
        self.scripts.as_ref()?.fighter(sf.decomp_name())?.first(names).filter(|m| m.has_hitboxes())
    }

    /// Whether the fighter's current act plays a scripted motion with hitboxes (its blow then stays silent).
    pub(super) fn smash_scripted(&self, i: usize) -> bool {
        normal_motion(self.f[i].act).is_some_and(|(names, _)| self.scripted_motion(i, names).is_some())
    }

    pub(super) fn smash_scripted_motion(&self, i: usize, names: &[&str]) -> bool {
        self.scripted_motion(i, names).is_some()
    }

    /// Length of a scripted punch (ticks): the motion's script end at the punch rate.
    pub(super) fn smash_blow_total(&self, i: usize, step: u8) -> Option<u16> {
        let (names, rate) = normal_motion(Act::Punch(step))?;
        let end = self.scripted_motion(i, names)?.end?;
        Some(((end as f32 / (2.0 * rate)).ceil() as u16).clamp(6, 22))
    }

    /// The scripted normal attack's hitboxes this tick (see the module doc).
    pub(super) fn smash_normal(&mut self, i: usize) {
        let act = self.f[i].act;
        let Some((names, rate)) = normal_motion(act) else { return };
        let Some(motion) = self.scripted_motion(i, names) else { return };
        let me = self.f[i];
        let t = me.act_t.saturating_sub(1) as f32;
        let (from, to) = ((t * 2.0 * rate).floor() as u32, (((t + 1.0) * 2.0 * rate).floor() as u32).max((t * 2.0 * rate).floor() as u32 + 1));
        let id = motion.id;
        let hits: Vec<_> = motion
            .active_in_frames(from, to)
            .iter()
            .map(|h| (h.window.knockback, to_world(h.pos, [me.pos[0] as f64, me.pos[1] as f64, me.pos[2] as f64], yaw16(me.yaw), K), h.window.radius() * K, h.window.from >= from && h.window.from < to))
            .collect();
        if hits.is_empty() {
            return;
        }
        if hits.iter().any(|h| h.3) && matches!(act, Act::Punch(_)) {
            if let Act::Punch(step) = act {
                self.ev.push(Ev::Punch { id: i, step });
            }
        }
        let sf = me.smash.unwrap_or(SmashFighter::Mario);
        for (kb, c, r, first) in hits {
            for j in 0..self.n {
                let o = self.f[j];
                if j == i || !o.alive || o.team == me.team || self.f[i].hit_mask & (1 << j) != 0 {
                    continue;
                }
                let (dx, dz) = (c[0] as f32 - o.pos[0], c[2] as f32 - o.pos[2]);
                let rr = r as f32 + o.radius();
                if dx * dx + dz * dz > rr * rr || (c[1] as f32) + (r as f32) < o.pos[1] || (c[1] as f32) - (r as f32) > o.pos[1] + o.height() {
                    continue;
                }
                self.f[i].hit_mask |= 1 << j;
                let d = norm2([o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]], dir_of(me.yaw));
                let hit = SmashHit {
                    damage: kb.damage as f32,
                    kind: jumpman_sim::smash::hit_kind(kb.element),
                    dir: [d[0] as f64 * 0.8, 0.6, d[1] as f64 * 0.8],
                    knock: 10.0 + kb.damage as f64,
                    pos: c,
                    fighter: sf,
                    mv: None,
                    knockback: Some(kb),
                    motion: Some(id),
                };
                self.smash_hit(j, i, &hit);
            }
            if first {
                // the monsters take it as a blow of that reach
                let reach = ((c[0] as f32 - me.pos[0]).hypot(c[2] as f32 - me.pos[2]) + r as f32).max(60.0);
                let l = Launch::of(&kb, kb.damage as f32, 0.0, 1.0, false);
                self.melee_mons(i, reach, -1.0, kb.damage as f32 * SMASH_DAMAGE, (l.horizontal * 30.0).min(1400.0), (l.vertical * 30.0).min(900.0));
            }
        }
    }
}

/// The fighters and monsters of a Gem Clash match as targets of one seat's specials (seat coordinates: the arena's,
/// minus the seat's floor height).
pub struct GemTargets<'a> {
    pub b: &'a mut Brawl,
    pub me: usize,
    pub base: f32,
    keys: Vec<u64>,
}

const MON_KEY: u64 = 1000;

impl SmashTargets for GemTargets<'_> {
    fn targets(&self) -> Vec<Target> {
        let b = &*self.b;
        let me = &b.f[self.me];
        let mut out = vec![];
        for j in 0..b.n {
            let o = &b.f[j];
            if j == self.me || !o.alive || o.team == me.team {
                continue;
            }
            out.push(Target { key: j as u64, pos: [o.pos[0] as f64, (o.pos[1] - self.base) as f64, o.pos[2] as f64], radius: o.radius() as f64, height: o.height() as f64, fighter: o.smash });
        }
        for (m, mo) in b.mons.iter().enumerate() {
            if !mo.alive || mo.spawn_t > 0 {
                continue;
            }
            out.push(Target { key: MON_KEY + m as u64, pos: [mo.pos[0] as f64, (mo.pos[1] - self.base) as f64, mo.pos[2] as f64], radius: mo.radius() as f64, height: mo.height() as f64, fighter: None });
        }
        out
    }

    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        if self.keys.is_empty() {
            self.keys = self.targets().iter().map(|t| t.key).collect();
        }
        let Some(&key) = self.keys.get(i) else { return HitOutcome::Miss };
        let out = if key < MON_KEY {
            self.b.smash_hit(key as usize, self.me, hit)
        } else {
            let m = (key - MON_KEY) as usize;
            let mo = self.b.mons[m];
            let dir = norm2([hit.dir[0] as f32, hit.dir[2] as f32], [1.0, 0.0]);
            let l = skb::minigame_launch(hit, mo.hp / mo.max_hp.max(1.0));
            let dmg = if hit.kind == HitKind::Sleep { 0.0 } else { hit.damage.max(if hit.kind == HitKind::Capture { 8.0 } else { 0.0 }) * SMASH_DAMAGE };
            if dmg <= 0.0 {
                return HitOutcome::Miss;
            }
            if self.b.hit_mon(m, self.me as i8, dmg, dir, (l.horizontal * 30.0).min(1400.0), (l.vertical * 30.0).min(900.0)) {
                if self.b.mons[m].alive { HitOutcome::Hit } else { HitOutcome::Killed }
            } else {
                HitOutcome::Miss
            }
        };
        // a new resolve pass rebuilds the list
        if i + 1 >= self.keys.len() {
            self.keys.clear();
        }
        out
    }

    fn incoming(&self) -> Vec<Incoming> {
        let b = &*self.b;
        let team = b.f[self.me].team;
        b.proj
            .iter()
            .enumerate()
            .filter(|(_, p)| p.alive && p.owner != self.me as i8 && (p.owner < 0 || b.f[p.owner as usize].team != team))
            .map(|(k, p)| Incoming { id: k as u64, pos: [p.pos[0] as f64, (p.pos[1] - self.base) as f64, p.pos[2] as f64], vel: [p.vel[0] as f64 / 30.0, p.vel[1] as f64 / 30.0, p.vel[2] as f64 / 30.0], radius: 20.0, energy: p.kind == PKind::Bullet })
            .collect()
    }

    fn reflect(&mut self, id: u64, vel: jumpman_sim::math::Vec3) {
        if let Some(p) = self.b.proj.get_mut(id as usize) {
            p.owner = self.me as i8;
            p.vel = [vel[0] as f32 * 30.0, vel[1] as f32 * 30.0, vel[2] as f32 * 30.0];
            p.t = 0;
        }
    }

    fn absorb(&mut self, id: u64) {
        if let Some(p) = self.b.proj.get_mut(id as usize) {
            p.alive = false;
        }
        let me = &mut self.b.f[self.me];
        me.hp = (me.hp + 6.0).min(me.max_hp);
    }
}

// ---------------------------------------------------------------------------------------------
// Bots
// ---------------------------------------------------------------------------------------------

/// How a bot uses a special: (`min`, `max`) distance to the foe, the chance per decision, and whether it must start on
/// the ground (rising / rushing moves that would carry it off a ledge from the air).
fn ai_use(mv: SpecialMove) -> (f32, f32, f32, bool) {
    use SpecialMove::*;
    match mv {
        Fireball | Blaster | ThunderJolt | PkFire => (260.0, 1000.0, 0.10, false),
        ChargeShot => (300.0, 1100.0, 0.08, false),
        Boomerang => (250.0, 900.0, 0.10, false),
        EggThrow => (250.0, 800.0, 0.08, false),
        GiantPunch => (0.0, 260.0, 0.10, true),
        FalconPunch => (0.0, 230.0, 0.06, true),
        Pound => (0.0, 230.0, 0.10, false),
        Inhale | EggLay => (0.0, 200.0, 0.08, true),
        SuperJumpPunch | SpinningKong | SpinAttack | ScrewAttack | FinalCutter | FalconDive => (0.0, 170.0, 0.08, true),
        FireFox | QuickAttack => (150.0, 500.0, 0.05, true),
        PkThunder => (300.0, 900.0, 0.04, true),
        Sing => (0.0, 200.0, 0.05, true),
        Tornado | HandSlap | Stone | YoshiBomb | Thunder | FalconKick => (0.0, 190.0, 0.08, true),
        LinkBomb | SamusBomb => (120.0, 600.0, 0.06, false),
        Rest => (0.0, 90.0, 0.10, true),
        Reflector | PsiMagnet => (0.0, 0.0, 0.0, true),
        Item => (0.0, 0.0, 0.0, true),
    }
}

/// A rising special that brings a fighter back from below the floor.
fn recovers(mv: SpecialMove) -> bool {
    use SpecialMove::*;
    matches!(mv, SuperJumpPunch | SpinningKong | SpinAttack | ScrewAttack | FinalCutter | FireFox | QuickAttack | FalconDive | EggThrow | PkThunder)
}

impl Brawl {
    /// A Smash bot's special: offence by range, a recovery from below the floor, charges released at the foe.
    pub(crate) fn bot_smash(&self, i: usize, rng: &mut Rng, cd: &mut i16, eager: f32, inp: &mut Input) {
        let me = self.f[i];
        let Some(sf) = me.smash else { return };
        *cd = (*cd - 1).max(0);
        let foe = self.nearest_foe(i);
        let seat = self.seats[i].as_ref();
        if me.act == Act::Special {
            let Some(sp) = seat.and_then(|s| s.special()) else { return };
            // steer recoveries back over the floor, everything else at the foe
            let here = [me.pos[0], me.pos[2]];
            let over = self.arena.support(here[0], here[1], me.pos[1] + STEP).is_some();
            let to = if !over { norm2([-here[0], -here[1]], [0.0, 0.0]) } else { foe.map_or([0.0; 2], |(j, _)| norm2([self.f[j].pos[0] - here[0], self.f[j].pos[2] - here[1]], [0.0, 0.0])) };
            inp.mv = to;
            use jumpman_sim::smash::Phase as P;
            let d = foe.map_or(9999.0, |f| f.1);
            match (sp.mv, sp.phase) {
                (SpecialMove::GiantPunch | SpecialMove::ChargeShot, P::Hold) => {
                    if d < 320.0 || (sp.mv == SpecialMove::ChargeShot && d < 1000.0 && sp.t > 60) || sp.t > 120 {
                        inp.special = true;
                    }
                }
                (SpecialMove::Reflector | SpecialMove::PsiMagnet, _) => inp.special_held = sp.t < 20,
                _ => {}
            }
            return;
        }
        if !matches!(me.act, Act::Idle | Act::Air) || me.held >= 0 {
            return;
        }
        // Recovery: off the floor and falling with the jumps spent.
        let here = [me.pos[0], me.pos[2]];
        let below = self.arena.support(here[0], here[1], me.pos[1] + STEP);
        if !me.grounded && below.is_none_or(|(top, _)| top < me.pos[1] - 700.0) && me.vel[1] < 0.0 && me.jumps >= 2 {
            let up = sf.special(SpecialSlot::Up);
            if recovers(up) {
                inp.special = true;
                inp.slot = 1;
                inp.mv = norm2([-here[0], -here[1]], [0.0, 1.0]);
                return;
            }
        }
        if *cd > 0 {
            return;
        }
        let Some((j, d)) = foe else { return };
        let o = self.f[j];
        if (o.pos[1] - me.pos[1]).abs() > 220.0 {
            return;
        }
        // Reflect / absorb a shot on its way.
        let shot = self.proj.iter().any(|p| p.alive && p.owner != i as i8 && dist2([p.pos[0], p.pos[2]], here) < 260.0);
        let down = sf.special(SpecialSlot::Down);
        if shot && matches!(down, SpecialMove::Reflector | SpecialMove::PsiMagnet) && me.grounded {
            inp.special = true;
            inp.special_held = true;
            inp.slot = 2;
            *cd = 20;
            return;
        }
        let slots = [SpecialSlot::Neutral, SpecialSlot::Up, SpecialSlot::Down];
        let pick = rng.below(3);
        for k in 0..3 {
            let slot = slots[(pick + k) % 3];
            let mv = if slot == SpecialSlot::Neutral && sf == SmashFighter::Kirby { self.seats[i].as_ref().and_then(|s| s.rt.smash.copy).map_or(SpecialMove::Inhale, |c| c.special(SpecialSlot::Neutral)) } else { sf.special(slot) };
            let (lo, hi, p, ground) = ai_use(mv);
            if d < lo || d > hi || (ground && !me.grounded) || p <= 0.0 {
                continue;
            }
            // Rest only against a stunned / close foe
            if mv == SpecialMove::Rest && !matches!(o.act, Act::Hurt | Act::Down) {
                continue;
            }
            if rng.chance((p * eager).min(1.0)) {
                inp.special = true;
                inp.slot = match slot {
                    SpecialSlot::Neutral => 0,
                    SpecialSlot::Up => 1,
                    SpecialSlot::Down => 2,
                };
                inp.mv = norm2([o.pos[0] - here[0], o.pos[2] - here[1]], dir_of(me.yaw));
                inp.attack = false;
                *cd = 24;
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "brawl_smash_tests.rs"]
pub(crate) mod tests;
