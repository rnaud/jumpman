//! Relic Descent's ported cast: eighteen Ocarina of Time / Banjo-Kazooie enemies as dungeon
//! monsters. A child module of `relic_core` (it sees the core's private helpers), engine-free and
//! seeded like the rest of the core.
//!
//! Stat blocks come from the decomp tables behind the course ports ([`Species`]): hit points are
//! `decomp hp x 16` (min 28, then the floor's `enemy_hp_mult`), touch damage is `hearts x 22`
//! (OoT sixteenths / 16, a BK wedge = half a heart; min 7), and the hurt-box radius is shrunk to
//! the dungeon's scale. A few numbers are by hand where the decomp has none (Bigbutt's 99 means
//! "unhurtable by ordinary blows", so it becomes a 380 hp mini-boss).
//!
//! Behaviours (one archetype serves several species):
//! * **Biter** (Deku Baba): anchored, lunges and reels back.
//! * **Ambush** (Skulltula, Gold Skulltula): hangs from the ceiling untouchable, drops with a
//!   red telegraph when you come close (or when it is the last one left), then fights. The gold
//!   one carries a token = a guaranteed good item.
//! * **Shooter** (Octorok) / **Burrower** (Mad Scrub): rocks you can swat back with a swing; the
//!   scrub burrows (untouchable) and pops up elsewhere.
//! * **Hopper** (Tektite): hops at you, hurts only while airborne.
//! * **Spinner** (Peahat): drifts overhead (hardly hurt while flying), spins in, then lands dizzy
//!   (takes extra damage).
//! * **Flanker** (Wolfos): circles, lunges, backflips away.
//! * **Swooper** (Guay, Bat): orbits, then dives through you.
//! * **Swarmer** (Grublin, Ticker, Limbo): bites; Limbo falls apart once and rebuilds.
//! * **Skitter** (Snippet, Mutie Snippet): pinches, then scuttles off.
//! * **Charger** (Bigbutt): telegraphs, charges in a line, stuns itself on a wall (extra damage).
//! * **Lobber** (Sir Slush): lobs snowballs as delayed blast discs onto where you are heading.
//! * **Mine** (Shrapnel): drifts at you and blows up with a fuse.
use super::*;
use crate::experiments::foes::{Pose, Species};

const HP_UNIT: f32 = 16.0;
/// Ticks a biter rears back before it bites (half a second: the research checklist's minimum telegraph).
pub const BITE_WINDUP: u16 = 15;
/// The shortest wind-up of any other attack (a lunge, a dive, a shot).
pub const MIN_WINDUP: u16 = 16;
const HEART_DMG: f32 = 22.0;

/// Map a species to its core kind.
pub fn kind_of_species(sp: Species) -> Option<EnemyKind> {
    EnemyKind::from_u8_raw(sp as u8 + 7)
}

impl EnemyKind {
    /// The raw table (no species round trip).
    fn from_u8_raw(v: u8) -> Option<EnemyKind> {
        Some(match v {
            8 => EnemyKind::DekuBaba,
            9 => EnemyKind::Skulltula,
            10 => EnemyKind::GoldSkulltula,
            11 => EnemyKind::Octorok,
            12 => EnemyKind::MadScrub,
            13 => EnemyKind::Tektite,
            14 => EnemyKind::Peahat,
            15 => EnemyKind::Wolfos,
            16 => EnemyKind::Guay,
            17 => EnemyKind::Grublin,
            18 => EnemyKind::Bigbutt,
            19 => EnemyKind::Snippet,
            20 => EnemyKind::MutieSnippet,
            21 => EnemyKind::SirSlush,
            22 => EnemyKind::Ticker,
            23 => EnemyKind::Limbo,
            24 => EnemyKind::Shrapnel,
            25 => EnemyKind::Bat,
            _ => return None,
        })
    }
}

/// Every ported kind.
pub const FOE_KINDS: [EnemyKind; 18] = [
    EnemyKind::DekuBaba,
    EnemyKind::Skulltula,
    EnemyKind::GoldSkulltula,
    EnemyKind::Octorok,
    EnemyKind::MadScrub,
    EnemyKind::Tektite,
    EnemyKind::Peahat,
    EnemyKind::Wolfos,
    EnemyKind::Guay,
    EnemyKind::Grublin,
    EnemyKind::Bigbutt,
    EnemyKind::Snippet,
    EnemyKind::MutieSnippet,
    EnemyKind::SirSlush,
    EnemyKind::Ticker,
    EnemyKind::Limbo,
    EnemyKind::Shrapnel,
    EnemyKind::Bat,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum B {
    Biter,
    Ambush,
    Shooter,
    Burrower,
    Hopper,
    Spinner,
    Flanker,
    Swooper,
    Swarmer,
    Skitter,
    Charger,
    Lobber,
    Mine,
}

fn behaviour(sp: Species) -> B {
    match sp {
        Species::DekuBaba => B::Biter,
        Species::Skulltula | Species::GoldSkulltula => B::Ambush,
        Species::Octorok => B::Shooter,
        Species::MadScrub => B::Burrower,
        Species::Tektite => B::Hopper,
        Species::Peahat => B::Spinner,
        Species::Wolfos => B::Flanker,
        Species::Guay | Species::Bat => B::Swooper,
        Species::Grublin | Species::Ticker | Species::Limbo => B::Swarmer,
        Species::Snippet | Species::MutieSnippet => B::Skitter,
        Species::Bigbutt => B::Charger,
        Species::SirSlush => B::Lobber,
        Species::Shrapnel => B::Mine,
    }
}

/// Stat block at floor 1 (the core scales hp and damage by floor on spawn).
pub fn stats(kind: EnemyKind) -> EnemyStats {
    let Some(sp) = kind.species() else { return EnemyStats { hp: 30.0, speed: 8.0, radius: 60.0, dmg: 8.0, xp: 6 } };
    let hp = (sp.decomp_hp() as f32 * HP_UNIT).max(28.0);
    let dmg = (sp.touch_hearts() * HEART_DMG).max(7.0);
    // (hp, dmg, units per tick, radius, xp)
    let (hp, dmg, speed, radius, xp) = match sp {
        Species::DekuBaba => (hp, dmg, 8.0, 70.0, 8),
        Species::Skulltula => (hp, dmg, 10.0, 60.0, 9),
        Species::GoldSkulltula => (60.0, dmg, 14.0, 50.0, 25),
        // Ranged foes get a floor of 40 hp: they are the ones you have to walk up to.
        Species::Octorok => (hp.max(40.0), dmg, 0.0, 65.0, 10),
        Species::MadScrub => (hp.max(40.0), dmg, 0.0, 55.0, 11),
        Species::Tektite => (hp, dmg.max(12.0), 22.0, 60.0, 10),
        Species::Peahat => (hp, dmg, 7.0, 95.0, 20),
        Species::Wolfos => (hp, dmg.max(12.0), 12.0, 80.0, 22),
        Species::Guay => (hp, dmg, 14.0, 45.0, 6),
        Species::Grublin => (hp, dmg, 10.0, 55.0, 6),
        // 99 hit points in the decomp = not hurt by ordinary blows: a hand-set mini-boss.
        Species::Bigbutt => (380.0, dmg, 8.0, 120.0, 60),
        Species::Snippet => (hp, dmg, 9.0, 55.0, 8),
        Species::MutieSnippet => (hp * 2.5, dmg * 1.3, 8.0, 90.0, 20),
        Species::SirSlush => (hp.max(40.0), dmg.max(14.0), 6.0, 90.0, 14),
        Species::Ticker => (24.0, 6.0, 13.0, 40.0, 4),
        Species::Limbo => (hp, dmg, 9.0, 60.0, 14),
        Species::Shrapnel => (hp, 26.0, 6.0, 60.0, 10),
        Species::Bat => (hp, dmg, 15.0, 45.0, 6),
    };
    EnemyStats { hp, speed, radius, dmg, xp }
}

/// Weighted additions to a floor's packs: each floor has a theme.
/// 1 Overgrown Vaults (plants, spiders, crawlers), 2 Frostbound Halls (wolves, bats, skeletons, snow), 3 Ashen Crypt (mines, flyers, the undead).
pub fn theme(floor: u32) -> &'static [(EnemyKind, f32)] {
    use EnemyKind::*;
    match floor {
        0 | 1 => &[(DekuBaba, 8.0), (Skulltula, 6.0), (Grublin, 14.0), (Ticker, 10.0), (Tektite, 8.0), (Octorok, 7.0), (Snippet, 6.0), (Guay, 3.0)],
        2 => &[(Wolfos, 9.0), (SirSlush, 7.0), (Bat, 9.0), (Guay, 8.0), (Snippet, 6.0), (Limbo, 8.0), (Skulltula, 4.0), (Octorok, 5.0), (MadScrub, 6.0), (Grublin, 8.0), (Tektite, 4.0)],
        _ => &[(Peahat, 8.0), (Shrapnel, 9.0), (Limbo, 10.0), (Wolfos, 8.0), (MadScrub, 7.0), (SirSlush, 7.0), (Bat, 6.0), (MutieSnippet, 6.0), (Tektite, 5.0), (Skulltula, 4.0)],
    }
}

/// Starting state of a new foe.
pub fn init(e: &mut Enemy) {
    if let Some(sp) = e.kind.species() {
        if behaviour(sp) == B::Ambush {
            e.state = EState::Hidden;
        }
    }
}

/// Cannot be hit right now (on the ceiling, underground).
pub fn untouchable(e: &Enemy) -> bool {
    e.state == EState::Hidden
}

/// Damage after the species' weak and strong moments.
pub fn damage_taken(e: &Enemy, dmg: f32) -> f32 {
    match e.kind {
        // Peahat: hardly hurt while it flies, wide open when it lands dizzy.
        EnemyKind::Peahat => match e.state {
            EState::Recover => dmg * 1.6,
            _ => dmg * 0.6,
        },
        // Bigbutt: a wall crash stuns it.
        EnemyKind::Bigbutt if e.state == EState::Recover => dmg * 1.5,
        _ => dmg,
    }
}

/// Knockback multiplier (the heavy ones barely budge).
pub fn knock_taken(kind: EnemyKind) -> f32 {
    match kind {
        EnemyKind::Bigbutt => 0.15,
        EnemyKind::Peahat | EnemyKind::MutieSnippet => 0.5,
        EnemyKind::DekuBaba | EnemyKind::Octorok | EnemyKind::MadScrub => 0.0,
        _ => 1.0,
    }
}

/// Visual lift above the floor (units) and whether the thing is drawn at all.
pub fn lift(e: &Enemy) -> f32 {
    let Some(sp) = e.kind.species() else { return 0.0 };
    match sp {
        Species::Skulltula | Species::GoldSkulltula => match e.state {
            EState::Hidden => 520.0,
            EState::Air => 520.0 * e.timer as f32 / 14.0,
            _ => 0.0,
        },
        Species::Tektite => match e.state {
            EState::Air => (e.timer as f32 / 18.0 * std::f32::consts::PI).sin() * 260.0,
            _ => 0.0,
        },
        Species::Peahat => match e.state {
            EState::Recover => 0.0,
            _ => 150.0,
        },
        Species::Guay | Species::Bat => match e.state {
            EState::Dash => 40.0,
            _ => 160.0,
        },
        _ => 0.0,
    }
}

/// Animation pose for the view.
pub fn pose_of(e: &Enemy, moving: bool) -> Pose {
    if e.kind == EnemyKind::Limbo && e.phase == 1 && e.state == EState::Recover {
        return Pose::Die;
    }
    if e.flash > 0 {
        return Pose::Hurt;
    }
    match e.state {
        EState::Windup | EState::Dash | EState::Fuse => Pose::Attack,
        EState::Air => Pose::Move,
        EState::Hidden => Pose::Idle,
        EState::Recover => {
            if matches!(e.kind, EnemyKind::Snippet | EnemyKind::MutieSnippet | EnemyKind::Wolfos | EnemyKind::Guay | EnemyKind::Bat) {
                Pose::Move
            } else {
                Pose::Idle
            }
        }
        EState::Chase => {
            if moving {
                Pose::Move
            } else {
                Pose::Idle
            }
        }
    }
}

impl Relic {
    /// A swing sends Octorok / Mad Scrub rocks back (they now hurt foes, a lot).
    pub(super) fn reflect_rocks(&mut self, p: [f32; 2], face: f32, half_arc: f32, reach: f32) {
        for i in 0..SHOT_SLOTS {
            let s = self.shots[i];
            if !s.alive || s.from_player || !s.rock {
                continue;
            }
            let d = dist(s.pos, p);
            if d > reach + 80.0 {
                continue;
            }
            let to = (s.pos[0] - p[0]).atan2(s.pos[1] - p[1]);
            if d > 90.0 && angle_diff(to, face).abs() > half_arc + 0.3 {
                continue;
            }
            let (dmg, _) = roll_damage(&self.stats, 1.6, &mut self.rng);
            let sp = (s.vel[0] * s.vel[0] + s.vel[1] * s.vel[1]).sqrt().max(1.0) * 1.4;
            let sh = &mut self.shots[i];
            sh.from_player = true;
            sh.vel = [face.sin() * sp, face.cos() * sp];
            sh.dmg = dmg;
            sh.life = 70;
            sh.radius = 45.0;
            self.ev.reflected = true;
        }
    }

    /// Limbo falls apart the first time it is beaten and rebuilds after about two seconds.
    pub(super) fn foe_revives(&mut self, i: usize) -> bool {
        let e = &mut self.enemies[i];
        if e.kind == EnemyKind::Limbo && e.phase == 0 {
            e.phase = 1;
            e.hp = e.max_hp * 0.55;
            e.state = EState::Recover;
            e.timer = 60;
            e.invuln = 62;
            e.shield = 0.0;
            return true;
        }
        false
    }

    /// Loot on top of the common table, mapped from the course drops: coins -> gold, heart tables -> potions,
    /// a Gold Skulltula token -> a guaranteed good item, Bigbutt's trophy -> a second.
    pub(super) fn foe_loot(&mut self, e: &Enemy) {
        let Some(sp) = e.kind.species() else { return };
        let l = sp.loot();
        let floor = self.floor;
        let level = self.player.level;
        let magpie = if self.stats.magpie { 1.5 } else { 1.0 };
        if l.coins > 0.0 && self.rng.chance(0.5) {
            let amt = (l.coins * (1.0 + 0.6 * floor as f32) * magpie) as u32;
            self.drop_gold(e.pos, amt.max(1));
        }
        if l.hearts > 0.0 && self.rng.chance(l.hearts * 0.5) {
            self.drop_potion(e.pos);
        }
        if l.token {
            self.tokens += 1;
            self.ev.token = true;
            self.drop_item(e.pos, DropSource::Elite, floor, level);
            self.drop_gold(e.pos, (20 + 10 * floor) * if self.stats.magpie { 2 } else { 1 });
        }
        if sp == Species::Bigbutt {
            self.drop_item(e.pos, DropSource::Elite, floor, level);
            self.drop_potion(e.pos);
        }
    }

    fn foe_room_alone(&self, i: usize) -> bool {
        let room = self.enemies[i].room;
        !self.enemies.iter().enumerate().any(|(j, x)| j != i && x.alive && x.room == room)
    }

    fn tick_down(&mut self, i: usize) -> u16 {
        let en = &mut self.enemies[i];
        en.timer = en.timer.saturating_sub(1);
        en.timer
    }

    fn goto(&mut self, i: usize, st: EState, timer: u16) {
        let en = &mut self.enemies[i];
        en.state = st;
        en.timer = timer;
    }

    fn fire_rock(&mut self, i: usize, dir: [f32; 2], speed: f32) {
        let e = self.enemies[i];
        let slot = Self::free_slot(&self.shots, |s| s.alive);
        self.spawn_shot(false, e.pos, [dir[0] * speed, dir[1] * speed], e.dmg, 30.0, 100, false, false);
        if let Some(k) = slot {
            self.shots[k].rock = true;
        }
    }

    fn explode(&mut self, i: usize, radius: f32) {
        let e = self.enemies[i];
        self.enemies[i].alive = false;
        self.alive_enemies = self.alive_enemies.saturating_sub(1);
        self.add_hazard(e.pos, radius, 0, e.dmg);
        let room = e.room as usize;
        if room < self.rooms.len() && self.rooms[room].active && !self.rooms[room].cleared && !self.enemies.iter().any(|x| x.alive && x.room as usize == room) {
            self.rooms[room].cleared = true;
            self.ev.room_cleared = true;
        }
    }

    /// Pursue with the Scamp's bite: in reach it rears back for [`BITE_WINDUP`] ticks (the shake and attack pose are the
    /// warning), then bites if the hero is still close; stepping or diving away makes it miss.
    pub(super) fn bite(&mut self, i: usize, dir: [f32; 2], d: f32, cd: u16) {
        let e = self.enemies[i];
        let reach = e.radius + PLAYER_RADIUS + 25.0;
        if e.state == EState::Windup {
            if self.tick_down(i) == 0 {
                if d < reach * 1.2 {
                    self.melee_hit(i);
                }
                self.goto(i, EState::Chase, 0);
                self.enemies[i].cd = cd;
            }
            return;
        }
        if d > reach * 0.8 {
            self.step_enemy(i, dir, e.speed);
        }
        if d < reach && e.cd == 0 {
            self.goto(i, EState::Windup, BITE_WINDUP);
            self.enemies[i].dir = dir;
        }
    }

    pub(super) fn ai_foe(&mut self, i: usize, me: [f32; 2], dir: [f32; 2], d: f32) {
        let e = self.enemies[i];
        let Some(sp) = e.kind.species() else { return };
        let reach = e.radius + PLAYER_RADIUS + 25.0;
        let away = [-dir[0], -dir[1]];
        let side = if (i + (self.tick as usize / 100)) % 2 == 0 { 1.0 } else { -1.0 };
        let perp = [-dir[1] * side, dir[0] * side];
        match behaviour(sp) {
            B::Biter => match e.state {
                EState::Windup => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Dash, 8);
                        self.enemies[i].struck = false;
                    }
                }
                EState::Dash => {
                    let dd = e.dir;
                    self.step_enemy(i, dd, 38.0);
                    if !e.struck && dist(self.enemies[i].pos, me) < reach {
                        self.melee_hit(i);
                        self.enemies[i].struck = true;
                    }
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Recover, 30);
                    }
                }
                EState::Recover => {
                    let h = norm([e.home[0] - e.pos[0], e.home[1] - e.pos[1]]);
                    if dist(e.pos, e.home) > 24.0 {
                        self.step_enemy(i, h, 24.0);
                    }
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 40;
                    }
                }
                _ => {
                    if d < 340.0 && e.cd == 0 {
                        self.goto(i, EState::Windup, MIN_WINDUP);
                        self.enemies[i].dir = dir;
                    }
                }
            },
            B::Ambush => match e.state {
                EState::Hidden => {
                    if d < 480.0 || self.foe_room_alone(i) {
                        // Drop on the player's side of the room; the shadow is the warning.
                        let from_me = norm([e.pos[0] - me[0], e.pos[1] - me[1]]);
                        let land = [me[0] + from_me[0] * 130.0, me[1] + from_me[1] * 130.0];
                        let land = if self.dungeon.circle_free(land, 60.0) { land } else { me };
                        self.enemies[i].pos = land;
                        self.goto(i, EState::Air, MIN_WINDUP);
                        self.add_hazard(land, 200.0, MIN_WINDUP, e.dmg);
                    }
                }
                EState::Air => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 24;
                    }
                }
                _ => self.bite(i, dir, d, 38),
            },
            B::Shooter | B::Burrower => {
                let burrower = behaviour(sp) == B::Burrower;
                match e.state {
                    EState::Windup => {
                        if self.tick_down(i) == 0 {
                            self.fire_rock(i, dir, 13.0);
                            self.goto(i, EState::Chase, 0);
                            self.enemies[i].cd = 70 + (self.tick % 20) as u16;
                            self.enemies[i].phase = e.phase.wrapping_add(1);
                        }
                    }
                    EState::Hidden => {
                        // Burrowed: pop up somewhere else nearby.
                        if self.tick_down(i) == 0 {
                            let mut np = e.pos;
                            for _ in 0..6 {
                                let a = self.rng.range(0.0, std::f32::consts::TAU);
                                let r = self.rng.range(150.0, 360.0);
                                let q = [e.pos[0] + a.sin() * r, e.pos[1] + a.cos() * r];
                                if self.dungeon.circle_free(q, 80.0) && dist(q, me) > 320.0 {
                                    np = q;
                                    break;
                                }
                            }
                            self.enemies[i].pos = np;
                            self.enemies[i].home = np;
                            self.goto(i, EState::Recover, 12);
                        }
                    }
                    EState::Recover => {
                        if self.tick_down(i) == 0 {
                            self.goto(i, EState::Chase, 0);
                            self.enemies[i].cd = 30;
                        }
                    }
                    _ => {
                        if burrower && ((e.phase >= 2) || (d < 160.0 && e.cd == 0)) {
                            self.enemies[i].phase = 0;
                            self.goto(i, EState::Hidden, 40);
                        } else if e.cd == 0 && d < 1300.0 && d > 120.0 {
                            self.goto(i, EState::Windup, MIN_WINDUP);
                        } else if !burrower && d < reach && e.cd == 0 {
                            // Cornered: it spits point blank (the same wind-up as a shot).
                            self.goto(i, EState::Windup, MIN_WINDUP);
                        }
                    }
                }
            }
            B::Hopper => match e.state {
                EState::Air => {
                    let dd = e.dir;
                    self.step_enemy(i, dd, e.speed);
                    if !e.struck && dist(self.enemies[i].pos, me) < reach {
                        self.melee_hit(i);
                        self.enemies[i].struck = true;
                    }
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Recover, 14);
                    }
                }
                EState::Recover => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 20 + (self.tick % 24) as u16;
                    }
                }
                EState::Windup => {
                    // Crouched: the hop goes where the hero stood when it crouched.
                    if self.tick_down(i) == 0 {
                        let v = (d / 16.0).clamp(12.0, 30.0);
                        let en = &mut self.enemies[i];
                        en.state = EState::Air;
                        en.timer = 18;
                        en.struck = false;
                        en.speed = v;
                    }
                }
                _ => {
                    if e.cd == 0 && d < 950.0 {
                        self.goto(i, EState::Windup, MIN_WINDUP);
                        self.enemies[i].dir = dir;
                    }
                }
            },
            B::Spinner => match e.state {
                EState::Windup => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Dash, 16);
                        let en = &mut self.enemies[i];
                        en.dir = dir;
                        en.struck = false;
                    }
                }
                EState::Dash => {
                    let dd = e.dir;
                    self.step_enemy(i, dd, 20.0);
                    if !e.struck && dist(self.enemies[i].pos, me) < reach {
                        self.melee_hit(i);
                        self.enemies[i].struck = true;
                    }
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Recover, 45);
                    }
                }
                EState::Recover => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 30;
                    }
                }
                _ => {
                    self.step_enemy(i, dir, e.speed);
                    if d < 450.0 && e.cd == 0 {
                        self.goto(i, EState::Windup, 22);
                    }
                }
            },
            B::Flanker => match e.state {
                EState::Windup => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Dash, 9);
                        self.enemies[i].struck = false;
                    }
                }
                EState::Dash => {
                    let dd = e.dir;
                    self.step_enemy(i, dd, 34.0);
                    if !e.struck && dist(self.enemies[i].pos, me) < reach {
                        self.melee_hit(i);
                        self.enemies[i].struck = true;
                    }
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Recover, 26);
                    }
                }
                EState::Recover => {
                    self.step_enemy(i, away, 14.0);
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 40 + (self.tick % 30) as u16;
                    }
                }
                _ => {
                    if d > 560.0 {
                        self.step_enemy(i, dir, e.speed);
                    } else if d < 340.0 {
                        self.step_enemy(i, away, e.speed * 0.7);
                    } else {
                        self.step_enemy(i, perp, e.speed * 0.9);
                    }
                    if e.cd == 0 && d < 650.0 {
                        self.goto(i, EState::Windup, MIN_WINDUP);
                        self.enemies[i].dir = dir;
                    }
                }
            },
            B::Swooper => match e.state {
                EState::Windup => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Dash, 18);
                        let en = &mut self.enemies[i];
                        en.dir = dir;
                        en.struck = false;
                    }
                }
                EState::Dash => {
                    let dd = e.dir;
                    self.step_enemy(i, dd, 30.0);
                    if !e.struck && dist(self.enemies[i].pos, me) < reach {
                        self.melee_hit(i);
                        self.enemies[i].struck = true;
                    }
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Recover, 20);
                    }
                }
                EState::Recover => {
                    let dd = e.dir;
                    self.step_enemy(i, dd, 12.0);
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 60 + (self.tick % 40) as u16;
                    }
                }
                _ => {
                    if d > 800.0 {
                        self.step_enemy(i, dir, e.speed);
                    } else if d < 500.0 {
                        self.step_enemy(i, away, e.speed);
                    } else {
                        self.step_enemy(i, perp, e.speed * 0.8);
                    }
                    if e.cd == 0 && d < 900.0 {
                        self.goto(i, EState::Windup, MIN_WINDUP);
                    }
                }
            },
            B::Swarmer => {
                if e.state == EState::Recover {
                    // Limbo rebuilding itself.
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 20;
                    }
                } else {
                    self.bite(i, dir, d, 36);
                }
            }
            B::Skitter => match e.state {
                EState::Recover => {
                    self.step_enemy(i, away, e.speed * 1.2);
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 14;
                    }
                }
                EState::Windup => {
                    if self.tick_down(i) == 0 {
                        if d < reach * 1.2 {
                            self.melee_hit(i);
                        }
                        self.goto(i, EState::Recover, 28);
                    }
                }
                _ => {
                    if d > reach * 0.8 {
                        self.step_enemy(i, dir, e.speed);
                    }
                    if d < reach && e.cd == 0 {
                        self.goto(i, EState::Windup, BITE_WINDUP);
                    }
                }
            },
            B::Charger => match e.state {
                EState::Windup => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Dash, 42);
                        self.enemies[i].struck = false;
                    }
                }
                EState::Dash => {
                    let dd = e.dir;
                    let before = e.pos;
                    self.step_enemy(i, dd, 31.0);
                    let after = self.enemies[i].pos;
                    if !e.struck && dist(after, me) < reach {
                        self.melee_hit(i);
                        self.enemies[i].struck = true;
                    }
                    if dist(before, after) < 6.0 {
                        // Hit a wall: crash and stun.
                        self.push_ring(RingKind::Slam, after, 0.0, 0.0, 260.0, 12);
                        self.ev.slam = true;
                        self.goto(i, EState::Recover, 75);
                    } else if self.tick_down(i) == 0 {
                        self.goto(i, EState::Recover, 40);
                    }
                }
                EState::Recover => {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 60;
                    }
                }
                _ => {
                    self.step_enemy(i, dir, e.speed);
                    if d < 950.0 && e.cd == 0 {
                        self.goto(i, EState::Windup, 30);
                        self.enemies[i].dir = dir;
                    }
                }
            },
            B::Lobber => {
                if e.state == EState::Windup {
                    if self.tick_down(i) == 0 {
                        self.goto(i, EState::Chase, 0);
                        self.enemies[i].cd = 110;
                    }
                } else {
                    if d < 800.0 {
                        self.step_enemy(i, away, e.speed);
                    } else if d > 1250.0 {
                        self.step_enemy(i, dir, e.speed);
                    } else {
                        self.step_enemy(i, perp, e.speed * 0.5);
                    }
                    if e.cd == 0 && d < 1400.0 {
                        // Lead the target a little: the snowball lands where the hero is heading.
                        let pv = [me[0] - self.player.prev[0], me[1] - self.player.prev[1]];
                        let target = [me[0] + pv[0] * 14.0, me[1] + pv[1] * 14.0];
                        let target = if self.dungeon.walkable(target) { target } else { me };
                        self.add_hazard(target, 230.0, 26, e.dmg);
                        self.goto(i, EState::Windup, 20);
                    }
                }
            }
            B::Mine => match e.state {
                EState::Fuse => {
                    if self.tick_down(i) == 0 {
                        self.explode(i, 330.0);
                    }
                }
                _ => {
                    self.step_enemy(i, dir, e.speed);
                    if d < 300.0 {
                        self.goto(i, EState::Fuse, 26);
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at(r: &Relic) -> PlayerFrame {
        PlayerFrame { pos: r.player.pos, grounded: true, face: r.player.face, ..Default::default() }
    }

    fn duel(kind: EnemyKind, floor: u32) -> (Relic, usize) {
        let mut r = Relic::new(7, Hero::Mario, floor, 0);
        let c = r.start_pos();
        r.player.pos = c;
        let i = r.spawn_enemy(kind, [c[0], c[1] + 170.0], 0, 0).unwrap();
        (r, i)
    }

    #[test]
    fn stat_blocks_follow_the_decomp_tables() {
        for k in FOE_KINDS {
            let sp = k.species().unwrap();
            assert_eq!(kind_of_species(sp), Some(k));
            assert_eq!(EnemyKind::from_u8(k as u8), Some(k));
            let s = enemy_stats(k);
            assert!(s.hp >= 24.0 && s.dmg >= 5.0 && s.radius >= 35.0 && s.xp >= 4, "{k:?} {s:?}");
            // Never silly next to the classic cast.
            assert!(s.hp <= 400.0 && s.dmg <= 30.0, "{k:?} {s:?}");
        }
        // Ordering follows the decomp: Wolfos (8) and Peahat (6) toughest of the plain OoT set, Bigbutt the mini-boss.
        assert!(enemy_stats(EnemyKind::Wolfos).hp > enemy_stats(EnemyKind::Peahat).hp);
        assert!(enemy_stats(EnemyKind::Peahat).hp > enemy_stats(EnemyKind::Tektite).hp);
        assert!(enemy_stats(EnemyKind::Tektite).hp > enemy_stats(EnemyKind::Guay).hp);
        assert!(enemy_stats(EnemyKind::Bigbutt).hp > 300.0);
        // Peahat touch (1 heart) hits harder than a Deku Baba (half a heart).
        assert!(enemy_stats(EnemyKind::Peahat).dmg > enemy_stats(EnemyKind::DekuBaba).dmg);
    }

    #[test]
    fn every_ported_kind_spawns_acts_and_hurts() {
        for kind in FOE_KINDS {
            let (mut r, _) = duel(kind, 2);
            r.player.hp = r.stats.max_hp * 10.0;
            r.stats.max_hp *= 10.0;
            let hp0 = r.player.hp;
            let mut acted = false;
            for _ in 0..600 {
                let f = frame_at(&r);
                r.tick(&f, &Intent::default());
                acted |= r.player.hp < hp0 || r.shots.iter().any(|s| s.alive && !s.from_player) || r.hazards.iter().any(|h| h.alive);
                r.player.iframes = 0;
            }
            assert!(acted, "{kind:?} never attacked a standing target");
        }
    }

    #[test]
    fn every_ported_kind_can_be_killed_and_pays_out() {
        for kind in FOE_KINDS {
            let mut gold = 0u32;
            let mut drops = 0;
            for seed in 0..30u64 {
                let mut r = Relic::new(seed + 1, Hero::Mario, 2, 0);
                r.filter_commons = false;
                let c = r.start_pos();
                let i = r.spawn_enemy(kind, [c[0] + 300.0, c[1]], 0, 0).unwrap();
                r.enemies[i].state = EState::Chase;
                let k0 = r.kills;
                r.damage_enemy(i, 5000.0, c, false, 0.0);
                if kind == EnemyKind::Limbo {
                    // Limbo falls apart once and rebuilds, then stays dead.
                    assert!(r.enemies[i].alive && r.enemies[i].invuln > 0, "Limbo should rebuild once");
                    assert_eq!(r.kills, k0);
                    r.enemies[i].invuln = 0;
                    r.damage_enemy(i, 5000.0, c, false, 0.0);
                }
                assert!(!r.enemies[i].alive, "{kind:?}");
                assert_eq!(r.kills, k0 + 1, "{kind:?}");
                assert!(r.player.xp > 0 || r.player.level > 1);
                for d in r.drops.iter().filter(|d| d.alive) {
                    drops += 1;
                    if matches!(d.kind, DropKind::Gold) {
                        gold += d.amount;
                    }
                }
                gold += r.player.gold;
            }
            assert!(drops > 3 && gold > 0, "{kind:?} left nothing ({drops} drops, {gold} gold over 30 kills)");
        }
    }

    #[test]
    fn gold_skulltula_pays_a_token_and_a_real_item() {
        let mut r = Relic::new(3, Hero::Mario, 1, 0);
        r.filter_commons = true;
        let c = r.start_pos();
        let i = r.spawn_enemy(EnemyKind::GoldSkulltula, [c[0] + 300.0, c[1]], 0, 0).unwrap();
        r.enemies[i].state = EState::Chase;
        r.damage_enemy(i, 5000.0, c, false, 0.0);
        assert_eq!(r.tokens, 1);
        assert!(r.ev.token);
        // The token's item is an Elite roll: never a plain common turned to gold.
        let items = r.drops.iter().filter(|d| d.alive && matches!(d.kind, DropKind::Item)).count();
        assert!(items >= 1, "an item drops with the token");
    }

    #[test]
    fn skulltula_hangs_untouchable_then_drops_with_a_warning() {
        let (mut r, i) = duel(EnemyKind::Skulltula, 1);
        r.enemies[i].pos = [r.start_pos()[0] + 900.0, r.start_pos()[1]];
        // A second foe keeps it from dropping just because it is alone.
        r.spawn_enemy(EnemyKind::Scamp, [r.start_pos()[0] - 600.0, r.start_pos()[1]], 0, 0);
        assert_eq!(r.enemies[i].state, EState::Hidden);
        let hp = r.enemies[i].hp;
        r.damage_enemy(i, 100.0, r.start_pos(), false, 0.0);
        assert_eq!(r.enemies[i].hp, hp, "hanging foes cannot be hit");
        // Walk under it.
        r.player.pos = [r.enemies[i].pos[0] - 300.0, r.enemies[i].pos[1]];
        let f = frame_at(&r);
        r.tick(&f, &Intent::default());
        assert_eq!(r.enemies[i].state, EState::Air);
        assert!(r.hazards.iter().any(|h| h.alive), "the shadow telegraphs the landing");
        for _ in 0..16 {
            let f = frame_at(&r);
            r.tick(&f, &Intent::default());
        }
        assert_eq!(r.enemies[i].state, EState::Chase);
        r.damage_enemy(i, 5.0, r.start_pos(), false, 0.0);
        assert!(r.enemies[i].hp < hp);
    }

    #[test]
    fn a_swing_sends_octorok_rocks_back_at_the_shooter() {
        let (mut r, i) = duel(EnemyKind::Octorok, 1);
        r.stats.max_hp *= 10.0;
        r.player.hp = r.stats.max_hp;
        let hp0 = r.enemies[i].hp;
        let mut reflected = false;
        for t in 0..400 {
            r.player.face = 0.0; // +z faces the octorok
            let f = frame_at(&r);
            // Swing once the rock is close.
            let near = r.shots.iter().any(|s| s.alive && s.rock && !s.from_player && dist(s.pos, r.player.pos) < 260.0);
            let intent = Intent { attack: near && t % 3 == 0, ..Default::default() };
            r.tick(&f, &intent);
            reflected |= r.ev.reflected;
            r.player.iframes = 0;
            if !r.enemies[i].alive || r.enemies[i].hp < hp0 {
                break;
            }
        }
        assert!(reflected, "a rock was swatted back");
        assert!(!r.enemies[i].alive || r.enemies[i].hp < hp0, "and it hurt the octorok");
    }

    #[test]
    fn peahat_and_bigbutt_have_weak_moments() {
        let (mut r, i) = duel(EnemyKind::Peahat, 1);
        let hp = r.enemies[i].hp;
        r.damage_enemy(i, 10.0, r.start_pos(), false, 0.0);
        let flying = hp - r.enemies[i].hp;
        r.enemies[i].state = EState::Recover;
        let hp = r.enemies[i].hp;
        r.damage_enemy(i, 10.0, r.start_pos(), false, 0.0);
        let landed = hp - r.enemies[i].hp;
        assert!(landed > 2.0 * flying, "landed {landed} vs flying {flying}");
        let (mut r, i) = duel(EnemyKind::Bigbutt, 2);
        r.enemies[i].invuln = 0;
        let hp = r.enemies[i].hp;
        r.damage_enemy(i, 10.0, r.start_pos(), false, 30.0);
        assert!(hp - r.enemies[i].hp > 9.9);
        assert!(r.enemies[i].vel[0].abs() + r.enemies[i].vel[1].abs() < 6.0, "heavy foes barely budge");
    }

    #[test]
    fn bigbutt_charges_in_a_line_and_stuns_on_walls() {
        let mut r = Relic::new(5, Hero::Mario, 2, 0);
        let c = r.start_pos();
        r.player.pos = c;
        r.stats.max_hp *= 10.0;
        r.player.hp = r.stats.max_hp;
        let i = r.spawn_enemy(EnemyKind::Bigbutt, [c[0], c[1] + 600.0], 0, 0).unwrap();
        let mut states = std::collections::HashSet::new();
        for _ in 0..400 {
            let f = frame_at(&r);
            r.tick(&f, &Intent::default());
            r.player.iframes = 0;
            states.insert(r.enemies[i].state as u8);
        }
        assert!(states.contains(&(EState::Windup as u8)) && states.contains(&(EState::Dash as u8)) && states.contains(&(EState::Recover as u8)), "{states:?}");
    }

    #[test]
    fn themes_add_ported_foes_to_every_floor_and_they_differ() {
        let mut seen: [std::collections::HashSet<u8>; 3] = Default::default();
        let mut brutes = 0;
        let mut golds = 0;
        for floor in 1..=3u32 {
            for s in 0..400u64 {
                let mut pack = [PackSlot { kind: EnemyKind::Scamp, mods: 0 }; PACK_MAX];
                let kind = if s % 5 == 0 { RoomKind::Treasure } else { RoomKind::Combat };
                let n = plan_pack(&mut Rng::new(s), floor, kind, 80, &mut pack);
                let elites = pack[..n].iter().filter(|p| p.mods != 0).count();
                assert!(elites <= 1, "one elite per pack");
                for p in &pack[..n] {
                    assert!(p.kind != EnemyKind::King);
                    if p.kind.species().is_some() {
                        seen[floor as usize - 1].insert(p.kind as u8);
                    }
                    if p.kind == EnemyKind::Bigbutt {
                        brutes += 1;
                        assert!(p.mods != 0, "Bigbutt is always an elite");
                        assert!(floor >= 2, "no Bigbutt on floor 1");
                    }
                    golds += (p.kind == EnemyKind::GoldSkulltula) as u32;
                }
            }
        }
        assert!(seen[0].len() >= 7 && seen[1].len() >= 9 && seen[2].len() >= 8, "{:?}", seen.iter().map(|s| s.len()).collect::<Vec<_>>());
        assert!(seen[0].contains(&(EnemyKind::DekuBaba as u8)) && !seen[0].contains(&(EnemyKind::Peahat as u8)));
        assert!(seen[2].contains(&(EnemyKind::Peahat as u8)) && seen[2].contains(&(EnemyKind::Shrapnel as u8)));
        assert!(seen[1].contains(&(EnemyKind::Wolfos as u8)));
        assert!(brutes > 20 && golds > 20, "{brutes} {golds}");
    }

    #[test]
    fn shrapnel_blows_up_with_a_telegraph() {
        let (mut r, i) = duel(EnemyKind::Shrapnel, 1);
        let mut fused = false;
        for _ in 0..200 {
            let f = frame_at(&r);
            r.tick(&f, &Intent::default());
            fused |= r.enemies[i].state == EState::Fuse;
            r.player.iframes = 0;
            if !r.enemies[i].alive {
                break;
            }
        }
        assert!(fused && !r.enemies[i].alive);
        assert!(r.ev.blast || r.rings.iter().any(|x| x.alive));
    }
}
