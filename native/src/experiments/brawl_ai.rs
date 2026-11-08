//! Arena Brawl, bots (engine-free): a small goal state machine per bot (wander, seek an item, go
//! for a gem, chase, flee, run for a heart, climb to high ground when the lava rises, use the
//! power form) with three difficulty levels. A bot reads the world through `&Brawl` and returns an
//! [`Input`] exactly like a human's, so it obeys the same rules. No allocation per tick.
use super::brawl_arena::*;
use super::brawl_core::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Diff {
    Easy,
    Normal,
    Hard,
}

impl Diff {
    pub fn name(self) -> &'static str {
        match self {
            Diff::Easy => "Easy",
            Diff::Normal => "Normal",
            Diff::Hard => "Hard",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "easy" | "0" => Some(Diff::Easy),
            "normal" | "1" => Some(Diff::Normal),
            "hard" | "2" => Some(Diff::Hard),
            _ => None,
        }
    }
    fn params(self) -> Params {
        match self {
            // Easy is for someone who has never played: slow to think, slow to react, wanders off, misses a lot,
            // rarely grabs a gem or an item and never dodges.
            Diff::Easy => Params { think: 40, atk_p: 0.02, gem_greed: 0.12, dodge: 0.0, hazards: false, item_use: 0.15, dive_p: 0.0, ranged_p: 0.08, special_delay: 70, keep_up: 0.0, speed: 0.68, react: 18, whiff: 0.5 },
            Diff::Normal => Params { think: 18, atk_p: 0.1, gem_greed: 0.2, dodge: 0.04, hazards: true, item_use: 0.45, dive_p: 0.006, ranged_p: 0.3, special_delay: 30, keep_up: 0.3, speed: 0.88, react: 10, whiff: 0.35 },
            Diff::Hard => Params { think: 4, atk_p: 0.45, gem_greed: 1.0, dodge: 0.3, hazards: true, item_use: 1.0, dive_p: 0.03, ranged_p: 0.95, special_delay: 4, keep_up: 1.0, speed: 1.0, react: 1, whiff: 0.0 },
        }
    }
}

#[derive(Clone, Copy)]
struct Params {
    think: i16,
    atk_p: f32,
    gem_greed: f32,
    dodge: f32,
    hazards: bool,
    item_use: f32,
    dive_p: f32,
    ranged_p: f32,
    special_delay: i16,
    keep_up: f32,
    speed: f32,
    react: i16,
    /// Chance that a button press the bot decided on never happens (a late or fumbled hit).
    whiff: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    Wander,
    Item(u8),
    Gem(u8),
    Chase(u8),
    Flee(u8),
    Heal(u8),
    HighGround,
    /// Go and hit a monster (slot).
    Hunt(u8),
}

#[derive(Clone, Copy, Debug)]
pub struct Brain {
    pub diff: Diff,
    pub boss: bool,
    pub goal: Goal,
    pub timer: i16,
    pub rng: Rng,
    pub atk_cd: i16,
    pub jump_cd: i16,
    pub stuck: i16,
    pub last: V2,
    pub wander: V2,
    pub strafe: f32,
    pub dive_arm: bool,
    pub spec_wait: i16,
    pub contact: i16,
}

impl Brain {
    pub fn new(diff: Diff, boss: bool, seed: u64) -> Self {
        Self { diff, boss, goal: Goal::Wander, timer: 0, rng: Rng::new(seed), atk_cd: 0, jump_cd: 0, stuck: 0, last: [0.0; 2], wander: [0.0; 2], strafe: 1.0, dive_arm: false, spec_wait: 0, contact: 0 }
    }
    pub fn reset(&mut self) {
        self.goal = Goal::Wander;
        self.timer = self.rng.below(12) as i16;
        self.atk_cd = 30;
        self.dive_arm = false;
        self.spec_wait = 0;
        self.contact = 0;
    }
}

impl Brawl {
    pub(crate) fn bot_input(&self, i: usize, b: &mut Brain) -> Input {
        let p = b.diff.params();
        let me = self.f[i];
        let mut inp = Input::default();
        b.atk_cd = (b.atk_cd - 1).max(0);
        b.jump_cd = (b.jump_cd - 1).max(0);
        b.timer -= 1;
        // Stuck? (not moving though we want to)
        let here = [me.pos[0], me.pos[2]];
        if dist2(here, b.last) < 3.0 && matches!(me.act, Act::Idle | Act::Air) {
            b.stuck += 1;
        } else {
            b.stuck = 0;
        }
        b.last = here;
        if b.timer <= 0 || b.stuck > 25 {
            self.bot_choose(i, b, &p);
            b.timer = p.think + b.rng.below(4) as i16;
            if b.stuck > 25 {
                b.stuck = 0;
                b.goal = Goal::Wander;
                b.wander = self.random_point(&mut b.rng);
                b.strafe = -b.strafe;
            }
        }
        let foe = self.nearest_foe(i);
        // Rocks and charging monsters: hop out of the way (Normal and Hard).
        if p.dodge > 0.0 && me.grounded && b.jump_cd == 0 && !matches!(me.act, Act::Punch(_) | Act::Swing) && self.mon_threat_near([me.pos[0], me.pos[2]]) && b.rng.chance((p.dodge * 1.5).min(0.5)) {
            inp.jump = true;
            b.jump_cd = 16;
        }
        // Defensive reflex: jump out of the way of a swing aimed at me.
        if let Some((j, d)) = foe {
            let o = &self.f[j];
            let swinging = matches!(o.act, Act::Punch(_) | Act::Swing | Act::Dive | Act::Nova | Act::PoundWind);
            if swinging && d < 230.0 && me.grounded && p.dodge > 0.0 && b.jump_cd == 0 && b.rng.chance(p.dodge * 0.35) && !matches!(me.act, Act::Punch(_) | Act::Swing) {
                inp.jump = true;
                b.jump_cd = 14;
            }
        }
        // Power form: blast when close, zap when far.
        if me.powered() {
            if let Some((j, d)) = foe {
                b.spec_wait -= 1;
                if b.spec_wait <= 0 {
                    if d < 360.0 && me.cd_s1 == 0 && me.power > NOVA_COST {
                        inp.s1 = true;
                        b.spec_wait = p.special_delay;
                    } else if d < 1300.0 && d > 300.0 && me.cd_s2 == 0 && me.power > BEAM_COST {
                        inp.s2 = true;
                        b.spec_wait = p.special_delay;
                    }
                }
                let _ = j;
            }
        }
        let goal = b.goal;
        let mut dest: Option<V2> = None;
        let mut want_jump_when_close = false;
        match goal {
            Goal::Wander => {
                if dist2(here, b.wander) < 90.0 || b.wander == [0.0; 2] {
                    b.wander = self.random_point(&mut b.rng);
                }
                dest = Some(b.wander);
            }
            Goal::Item(k) => {
                let it = &self.items[k as usize];
                if !it.alive || it.state != IState::Rest || me.held >= 0 {
                    b.timer = 0;
                } else {
                    let ip = [it.pos[0], it.pos[2]];
                    dest = Some(ip);
                    let d = dist2(here, ip);
                    if d < 100.0 + me.radius() && (it.pos[1] - me.pos[1]).abs() < 150.0 {
                        inp.z = me.grounded;
                        inp.grab = true;
                    }
                    want_jump_when_close = it.pos[1] > me.pos[1] + STEP;
                }
            }
            Goal::Heal(k) => {
                let it = &self.items[k as usize];
                if !it.alive || it.kind != ItemKind::Heal {
                    b.timer = 0;
                } else {
                    dest = Some([it.pos[0], it.pos[2]]);
                    want_jump_when_close = it.pos[1] > me.pos[1] + STEP;
                }
            }
            Goal::Gem(g) => {
                let gem = &self.gems[g as usize];
                if gem.state != GemState::Free {
                    b.timer = 0;
                } else {
                    dest = Some([gem.pos[0], gem.pos[2]]);
                    want_jump_when_close = gem.pos[1] > me.pos[1] + STEP;
                }
            }
            Goal::HighGround => {
                dest = self.arena.nearest_high_ground(here[0], here[1], 100.0);
                want_jump_when_close = true;
            }
            Goal::Flee(j) => {
                let o = &self.f[j as usize];
                let away = norm2([me.pos[0] - o.pos[0], me.pos[2] - o.pos[2]], [1.0, 0.0]);
                let d = [-away[1] * b.strafe * 0.4 + away[0], away[0] * b.strafe * 0.4 + away[1]];
                dest = Some([here[0] + d[0] * 400.0, here[1] + d[1] * 400.0]);
                if me.grounded && b.jump_cd == 0 && b.rng.chance(0.03) {
                    inp.jump = true;
                    b.jump_cd = 20;
                }
            }
            Goal::Chase(j) => {
                self.bot_fight(i, j as usize, b, &p, &mut inp, &mut dest);
            }
            Goal::Hunt(m) => {
                let o = &self.mons[m as usize];
                if !o.alive || o.spawn_t > 0 {
                    b.timer = 0;
                } else {
                    let tp = [o.pos[0], o.pos[2]];
                    let d = dist2(here, tp);
                    let to = norm2([tp[0] - here[0], tp[1] - here[1]], dir_of(me.yaw));
                    let facing = angle_diff(me.yaw, yaw_of(to)).abs();
                    let free = matches!(me.act, Act::Idle | Act::Air);
                    dest = Some(tp);
                    let reach = 110.0 + o.radius() * 0.6 + me.radius() * 0.5;
                    if d < reach + 12.0 && b.atk_cd == 0 && free && me.grounded && me.held < 0 && facing < 0.9 && b.rng.chance((p.atk_p * 3.0).min(1.0)) {
                        inp.attack = true;
                        b.atk_cd = 2;
                    } else if matches!(me.act, Act::Punch(_)) && d < reach + 40.0 && b.rng.chance(0.95 * p.keep_up) {
                        inp.attack = true;
                    }
                    if me.held >= 0 && free && d < 700.0 && d > 150.0 && facing < 0.6 && b.atk_cd == 0 {
                        inp.attack = true;
                        b.atk_cd = 12;
                    }
                }
            }
        }
        // Steering with edge / hazard avoidance.
        if let Some(d) = dest {
            let dir = norm2([d[0] - here[0], d[1] - here[1]], [0.0, 0.0]);
            let slow = p.speed;
            let (mv, need_jump) = self.steer(&me, dir, p.hazards, b);
            inp.mv = [mv[0] * slow, mv[1] * slow];
            if (need_jump || (want_jump_when_close && dist2(here, d) < 220.0)) && me.grounded && b.jump_cd == 0 {
                inp.jump = true;
                b.jump_cd = 18;
            } else if need_jump && !me.grounded && me.jumps == 1 && me.vel[1] < 150.0 && b.jump_cd == 0 {
                inp.jump = true;
                b.jump_cd = 10;
            }
        }
        if inp.attack && b.rng.chance(p.whiff) {
            inp.attack = false;
        }
        inp.jump_held = inp.jump || (!me.grounded && me.vel[1] > 0.0);
        inp
    }

    fn random_point(&self, rng: &mut Rng) -> V2 {
        let s = self.arena.item_spots[rng.below(self.arena.item_spots.len())];
        [s[0] + rng.range(-120.0, 120.0), s[2] + rng.range(-120.0, 120.0)]
    }

    /// Pick a goal.
    fn bot_choose(&self, i: usize, b: &mut Brain, p: &Params) {
        let me = &self.f[i];
        let here = [me.pos[0], me.pos[2]];
        let foe = self.nearest_foe(i);
        // Lava on its way up: get to the rocks.
        if p.hazards && self.arena.kind == ArenaKind::Ember && !matches!(self.arena.lava_phase, LavaPhase::Low) && me.pos[1] < 100.0 {
            b.goal = Goal::HighGround;
            return;
        }
        // The cannon is aimed at the healthiest fighter; nothing to do but keep moving.
        // Power form: hunt.
        if me.powered() {
            if let Some((j, _)) = foe {
                b.goal = Goal::Chase(j as u8);
                return;
            }
        }
        // Hurt: run for a heart, or away.
        if me.hp_frac() < 0.5 {
            let mut best: Option<(usize, f32)> = None;
            for (k, it) in self.items.iter().enumerate() {
                if it.alive && it.kind == ItemKind::Heal && it.state == IState::Rest {
                    let d = dist2(here, [it.pos[0], it.pos[2]]);
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((k, d));
                    }
                }
            }
            if let Some((k, d)) = best {
                if d < 1100.0 {
                    b.goal = Goal::Heal(k as u8);
                    return;
                }
            }
        }
        if me.hp_frac() < 0.25 && !b.boss && p.keep_up > 0.0 {
            if let Some((j, d)) = foe {
                if d < 380.0 && b.rng.chance(0.5 * p.keep_up) {
                    b.goal = Goal::Flee(j as u8);
                    return;
                }
            }
        }
        // Gems: the more I hold, the more I want the last one.
        if !b.boss {
            let held = me.gems.count_ones() as f32;
            let want = p.gem_greed * (0.8 + 0.9 * held);
            let mut best: Option<(usize, f32)> = None;
            for (g, gem) in self.gems.iter().enumerate() {
                if gem.state == GemState::Free && !(gem.immune == i as i8 && gem.immune_t > 0) {
                    let d = dist2(here, [gem.pos[0], gem.pos[2]]);
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((g, d));
                    }
                }
            }
            if let Some((g, d)) = best {
                let foe_d = foe.map_or(9999.0, |f| f.1);
                if b.rng.chance(want.min(1.0)) && (d < 1000.0 || held > 0.5 || d < foe_d) {
                    b.goal = Goal::Gem(g as u8);
                    return;
                }
            }
        }
        // Someone is carrying gems: take them down.
        let mut carrier: Option<(usize, u32)> = None;
        for j in 0..self.n {
            let o = &self.f[j];
            if j != i && o.alive && o.team != me.team && o.gems != 0 && carrier.is_none_or(|(_, c)| o.gem_count() > c) {
                carrier = Some((j, o.gem_count()));
            }
        }
        if let Some((j, _)) = carrier {
            if b.rng.chance(0.4 + 0.5 * p.gem_greed) {
                b.goal = Goal::Chase(j as u8);
                return;
            }
        }
        // Grab something to hit with.
        if me.held < 0 && b.rng.chance(p.item_use) {
            let foe_d = foe.map_or(9999.0, |f| f.1);
            let mut best: Option<(usize, f32)> = None;
            for (k, it) in self.items.iter().enumerate() {
                if !it.alive || it.state != IState::Rest || it.kind == ItemKind::Heal || it.pos[1] < self.arena.lava_floor() {
                    continue;
                }
                let d = dist2(here, [it.pos[0], it.pos[2]]) - if it.kind.weapon() { 220.0 } else { 0.0 };
                if d < 650.0 && d < foe_d * 1.5 && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((k, d));
                }
            }
            if let Some((k, _)) = best {
                b.goal = Goal::Item(k as u8);
                return;
            }
        }
        // A monster close by and no fighter in the way: knock it out for the drop (or just to stop it biting).
        if let Some((m, d)) = self.nearest_huntable_mon(here) {
            let foe_d = foe.map_or(9999.0, |f| f.1);
            if d < 700.0 && foe_d > 480.0 && me.hp_frac() > 0.4 && b.rng.chance((p.item_use + 0.15).min(0.9)) {
                b.goal = Goal::Hunt(m as u8);
                return;
            }
        }
        match foe {
            Some((j, _)) => b.goal = Goal::Chase(j as u8),
            None => b.goal = Goal::Wander,
        }
    }

    /// Chase / fight logic: moves, attacks and uses what is in hand.
    fn bot_fight(&self, i: usize, j: usize, b: &mut Brain, p: &Params, inp: &mut Input, dest: &mut Option<V2>) {
        let me = &self.f[i];
        let o = &self.f[j];
        if !o.alive {
            b.timer = 0;
            return;
        }
        let here = [me.pos[0], me.pos[2]];
        let lead = if b.diff == Diff::Hard { 0.18 } else { 0.0 };
        let tpos = [o.pos[0] + o.vel[0] * lead, o.pos[2] + o.vel[2] * lead];
        let d = dist2(here, tpos);
        let to = norm2([tpos[0] - here[0], tpos[1] - here[1]], dir_of(me.yaw));
        let facing = angle_diff(me.yaw, yaw_of(to)).abs();
        let high = (o.pos[1] - me.pos[1]).abs() < 130.0;
        *dest = Some(tpos);
        let held = if me.held >= 0 { Some(self.items[me.held as usize].kind) } else { None };
        let free = matches!(me.act, Act::Idle | Act::Air);
        match held {
            Some(ItemKind::Hammer) => {
                if d < 250.0 + o.radius() && high && b.atk_cd == 0 && free {
                    inp.attack = true;
                    b.atk_cd = 20;
                }
            }
            Some(ItemKind::Blaster) => {
                // Hold the range and shoot.
                if d < 300.0 {
                    let away = [-to[0], -to[1]];
                    *dest = Some([here[0] + away[0] * 300.0 + away[1] * 150.0 * b.strafe, here[1] + away[1] * 300.0 - away[0] * 150.0 * b.strafe]);
                }
                if d < 1200.0 && facing < 0.5 && b.atk_cd == 0 && free && b.rng.chance(p.ranged_p) {
                    inp.attack = true;
                    b.atk_cd = 8 + (if b.diff == Diff::Easy { 14 } else { 0 });
                }
            }
            Some(ItemKind::Boomerang) => {
                if d > 220.0 && d < 950.0 && facing < 0.5 && b.atk_cd == 0 && free && b.rng.chance(p.ranged_p) {
                    inp.attack = true;
                    b.atk_cd = 30;
                }
            }
            Some(_) => {
                // A throwable: close in a little, then throw.
                let kind = held.unwrap_or(ItemKind::Crate);
                let (lo, hi) = if kind == ItemKind::Bomb { (200.0, 650.0) } else { (150.0, 750.0) };
                if d > lo && d < hi && facing < 0.6 && b.atk_cd == 0 && free && b.rng.chance(p.ranged_p) {
                    inp.attack = true;
                    b.atk_cd = 12;
                    b.goal = Goal::Wander;
                    b.timer = 0;
                }
                // A lit bomb is about to go: get rid of it whatever happens.
                if kind == ItemKind::Bomb && me.held >= 0 && self.items[me.held as usize].fuse < 24 && free {
                    inp.attack = true;
                }
            }
            None => {}
        }
        if held.is_none() || held == Some(ItemKind::Hammer) {
            let reach = 110.0 + o.radius() * 0.6 + me.radius() * 0.5;
            // Punch when close and roughly facing.
            if d < reach + 12.0 && high {
                b.contact += 1;
            } else {
                b.contact = 0;
            }
            if d < reach + 12.0 && high && b.contact >= p.react && b.atk_cd == 0 && free && me.grounded && held.is_none() && b.rng.chance((p.atk_p * 3.0).min(1.0)) {
                inp.attack = true;
                b.atk_cd = 2;
            } else if matches!(me.act, Act::Punch(_)) && d < reach + 40.0 && b.rng.chance(0.95 * p.keep_up) {
                inp.attack = true;
            }
            // Dive at a fighter a little way off (Normal / Hard).
            if free && me.grounded && d > 200.0 && d < 380.0 && held.is_none() && b.rng.chance(p.dive_p) && b.jump_cd == 0 {
                inp.jump = true;
                b.jump_cd = 14;
                b.dive_arm = true;
            }
            if b.dive_arm && !me.grounded && matches!(me.act, Act::Air) && d < 400.0 && d > 90.0 && facing < 0.6 {
                inp.attack = true;
                b.dive_arm = false;
            }
            if me.grounded {
                b.dive_arm = false;
            }
            // Ground pound on someone beneath, and the boss' favourite.
            if !me.grounded && matches!(me.act, Act::Air) && d < 130.0 && me.pos[1] - o.pos[1] > 60.0 && b.rng.chance(0.25) {
                inp.z = true;
            }
            if b.boss && free && me.grounded && d < 300.0 && b.jump_cd == 0 && b.rng.chance(0.015) {
                inp.jump = true;
                b.jump_cd = 40;
                b.dive_arm = false;
            }
            if b.boss && !me.grounded && matches!(me.act, Act::Air) && me.vel[1] < 100.0 && d < 330.0 {
                inp.z = true;
            }
        }
        // Keep a little spacing sideways so two bots do not just circle.
        if d < 160.0 && b.diff != Diff::Easy && held != Some(ItemKind::Hammer) {
            let side = [-to[1] * b.strafe, to[0] * b.strafe];
            *dest = Some([tpos[0] + side[0] * 90.0, tpos[1] + side[1] * 90.0]);
        }
    }

    /// Choose a safe heading near `dir`: avoids drops, lava, bumpers; flags walls that need a jump.
    fn steer(&self, me: &Fighter, dir: V2, hazards: bool, b: &mut Brain) -> (V2, bool) {
        if len2(dir) < 0.01 {
            return ([0.0; 2], false);
        }
        let here = [me.pos[0], me.pos[2]];
        let look = 95.0 + me.speed2() * 0.12;
        let mut need_jump = false;
        for k in 0..5 {
            let rot = match k {
                0 => 0.0,
                1 => 0.7,
                2 => -0.7,
                3 => 1.4,
                _ => -1.4,
            } * b.strafe.signum();
            let (s, c) = rot.sin_cos();
            let d = [dir[0] * c - dir[1] * s, dir[0] * s + dir[1] * c];
            let ahead = [here[0] + d[0] * look, here[1] + d[1] * look];
            let sup = self.arena.support(ahead[0], ahead[1], me.pos[1] + 400.0);
            let ok_ground = match sup {
                None => false,
                Some((top, _)) => top > me.pos[1] - 150.0 || self.arena.bounds.is_some(),
            };
            let ok_bounds = self.arena.bounds.is_none_or(|bd| ahead[0].abs() < bd[0] - 20.0 && ahead[1].abs() < bd[1] - 20.0);
            let ok_lava = !hazards || self.arena.kind != ArenaKind::Ember || sup.is_none_or(|(top, _)| top >= self.arena.lava + 20.0 || self.arena.lava_phase == LavaPhase::Low && top >= self.arena.lava);
            let ok_bump = !hazards || !self.arena.in_bumper(ahead[0], ahead[1], 55.0);
            let ok_plank = !hazards || sup.is_none_or(|(_, idx)| !matches!(self.arena.solids[idx].plank, Plank::Shaking(_)));
            let ok_mon = !hazards || !self.mon_danger_at(ahead);
            let ok_shell = !hazards || !self.proj.iter().any(|p| p.alive && p.kind == PKind::Shell && dist2([p.aim[0], p.aim[2]], ahead) < 280.0 && dist2([p.aim[0], p.aim[2]], here) > 200.0);
            if ok_ground && ok_bounds && ok_lava && ok_bump && ok_plank && ok_shell && ok_mon {
                if let Some((top, _)) = sup {
                    if top > me.pos[1] + STEP {
                        if top < me.pos[1] + 330.0 {
                            need_jump = true;
                        } else if k < 2 {
                            continue;
                        }
                    }
                }
                return (d, need_jump);
            }
        }
        ([0.0; 2], false)
    }
}

impl Arena {
    /// Lowest height worth walking to (items under the tide are lost).
    pub fn lava_floor(&self) -> f32 {
        if self.kind == ArenaKind::Ember && self.lava_phase != LavaPhase::Low { self.lava } else { f32::NEG_INFINITY }
    }
}
