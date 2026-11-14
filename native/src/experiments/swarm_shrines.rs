//! Star Swarm shrines and pots: the level's interactables besides the chests (in the spirit of Megabonk's shrines; our own
//! rules and numbers). The sites come from the chest planner (`swarm_world::plan_world`, same seeded, reachability-tested
//! flood fill); this module is their engine-free behaviour on the [`Swarm`] core:
//!
//! * **Charge shrine**: stand inside its circle while it charges (about 9 s; it drains slowly when you step out). The swarm
//!   is drawn to it the whole time. Charged: a permanent stat boost (damage, health or XP), a card pick and some healing.
//! * **Challenge shrine**: touch it to call a hard wave (an elite and a ring of foes, reinforced every 1.5 s). Stay within
//!   reach for 25 s: two card picks and gold. Running off fails it (it can be tried again after a short cool-down).
//! * **Greed shrine** (cursed): gold, a card and +15 % XP now; every foe from then on has 12 % more health and the swarm
//!   is 12 % thicker, for the rest of the run (it stacks).
//! * **Boss altar**: summons a mini-boss on the spot (a King or a Chain Chomp, as tough as the next scheduled one); it drops
//!   a chest like any boss. The altar itself pays a little gold.
//! * **Duplicator**: pay gold for a free level of something you already carry (a weapon or tome, never a new one); up to
//!   three times, dearer each time.
//! * **Pots**: break when walked into: a few gold, now and then a heart or a magnet.
//!
//! Every shrine is revealed (and stays on the compass) once the player has been within [`REVEAL_R`] of it.
use super::swarm_core::*;
pub use super::swarm_world::{PotSite, ShrineKind, ShrineSite};

/// Touch distance of a shrine and of a pot.
pub const SHRINE_R: f32 = 170.0;
pub const POT_R: f32 = 110.0;
/// A charge shrine's circle.
pub const CHARGE_R: f32 = 360.0;
pub const CHARGE_TICKS: u32 = 9 * TICKS_PER_SEC;
/// Charge lost per tick outside the circle (a full bar drains in 30 s).
const CHARGE_DRAIN: f32 = 1.0 / (30 * TICKS_PER_SEC) as f32;
pub const SHRINE_FIGHT_TICKS: u32 = 25 * TICKS_PER_SEC;
/// Leave a challenge this far behind and it is lost.
const FIGHT_LEASH: f32 = 1500.0;
const RETRY_TICKS: u32 = 8 * TICKS_PER_SEC;
pub const DUP_COST: u32 = 30;
pub const DUP_USES: u8 = 3;

/// Health and spawn multipliers after `n` greed shrines.
pub fn curse_hp(n: u32) -> f32 {
    1.0 + 0.12 * n as f32
}
pub fn curse_spawn(n: u32) -> f32 {
    1.0 + 0.12 * n as f32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShrineState {
    Idle,
    /// Charging (charge shrine) or fighting (challenge shrine).
    Active,
    Done,
}

/// A charge shrine's permanent boost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boost {
    Damage,
    Health,
    Xp,
}

impl Boost {
    pub fn label(self) -> &'static str {
        match self {
            Boost::Damage => "+10% DAMAGE",
            Boost::Health => "+15 MAX HEALTH",
            Boost::Xp => "+12% XP",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Shrine {
    pub site: ShrineSite,
    pub revealed: bool,
    pub state: ShrineState,
    /// Charge (0..1) of a charge shrine; for a challenge, the fight's progress.
    pub charge: f32,
    pub ticks: u32,
    /// Cool-down before a failed challenge can be tried again; duplicator uses.
    pub cooldown: u32,
    pub uses: u8,
    /// The player stood in reach last tick (touches fire once per approach).
    inside: bool,
}

impl Shrine {
    pub fn new(site: ShrineSite) -> Self {
        Self { site, revealed: false, state: ShrineState::Idle, charge: 0.0, ticks: 0, cooldown: 0, uses: 0, inside: false }
    }
    /// Still worth a visit (drawn lit, kept on the compass).
    pub fn live(&self) -> bool {
        self.state != ShrineState::Done
    }
    pub fn dup_cost(&self) -> u32 {
        DUP_COST * (self.uses as u32 + 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pot {
    pub site: PotSite,
    pub broken: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShrineWhat {
    Started,
    Done,
    Failed,
    /// A duplicator touched without the gold (the cost).
    NeedGold(u32),
    /// A duplicator with nothing to copy.
    Nothing,
}

/// What a shrine did this tick (for toasts and sounds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShrineEvent {
    pub kind: ShrineKind,
    pub what: ShrineWhat,
    pub boost: Option<Boost>,
    /// The upgrade a duplicator copied.
    pub copied: Option<Upgrade>,
    pub at: [i32; 2],
}

impl Swarm {
    /// Load a level's planned shrines and pots.
    pub fn set_world_extras(&mut self, shrines: &[ShrineSite], pots: &[PotSite]) {
        self.shrines = shrines.iter().map(|s| Shrine::new(*s)).collect();
        self.pots = pots.iter().map(|p| Pot { site: *p, broken: false }).collect();
    }

    fn shrine_ev(&mut self, i: usize, what: ShrineWhat) -> &mut ShrineEvent {
        let s = self.shrines[i].site;
        self.ev.shrine = Some(ShrineEvent { kind: s.kind, what, boost: None, copied: None, at: [s.pos[0] as i32, s.pos[1] as i32] });
        self.ev.shrine.as_mut().unwrap()
    }

    /// `n` foes on a ring of radius `r` round `at` (walkable spots only, under the cap).
    fn ring_spawn(&mut self, at: [f32; 2], r: f32, n: usize) {
        let secs = self.secs();
        let a0 = self.rng.range(0.0, std::f32::consts::TAU);
        for k in 0..n {
            if self.alive_enemies >= self.cap() + OVERCAP {
                break;
            }
            let a = a0 + k as f32 * std::f32::consts::TAU / n as f32;
            let q = [at[0] + a.cos() * r, at[1] + a.sin() * r];
            if self.walkable(q) {
                let kind = self.pick_kind(secs);
                self.spawn_enemy(kind, q, secs);
            }
        }
    }

    pub(super) fn update_shrines(&mut self) {
        let p = self.player.pos;
        let fy = self.player_floor_y;
        for i in 0..self.shrines.len() {
            let s = self.shrines[i];
            let d2 = dist2(s.site.pos, p);
            let level = (fy - s.site.y).abs() < CHEST_REACH_Y;
            if d2 < REVEAL_R * REVEAL_R {
                self.shrines[i].revealed = true;
            }
            self.shrines[i].cooldown = s.cooldown.saturating_sub(1);
            let touch = d2 < SHRINE_R * SHRINE_R && level;
            let entered = touch && !s.inside;
            self.shrines[i].inside = touch;
            if s.state == ShrineState::Done {
                continue;
            }
            match s.site.kind {
                ShrineKind::Charge => {
                    let inside = d2 < CHARGE_R * CHARGE_R && level;
                    let sh = &mut self.shrines[i];
                    if inside {
                        if sh.state == ShrineState::Idle {
                            sh.state = ShrineState::Active;
                            self.shrine_ev(i, ShrineWhat::Started);
                        }
                        let sh = &mut self.shrines[i];
                        sh.charge += 1.0 / CHARGE_TICKS as f32;
                        sh.ticks += 1;
                        let t = sh.ticks;
                        // The swarm comes for whoever holds the circle.
                        if t % 40 == 0 {
                            self.ring_spawn(s.site.pos, 950.0, 3);
                        }
                    } else if sh.state == ShrineState::Active {
                        sh.charge -= CHARGE_DRAIN;
                        if sh.charge <= 0.0 {
                            sh.charge = 0.0;
                            sh.state = ShrineState::Idle;
                        }
                    }
                    if self.shrines[i].charge >= 1.0 {
                        self.shrines[i].charge = 1.0;
                        self.shrines[i].state = ShrineState::Done;
                        let boost = [Boost::Damage, Boost::Health, Boost::Xp][self.rng.below(3) as usize];
                        match boost {
                            Boost::Damage => self.perks.damage *= 1.1,
                            Boost::Health => self.perks.max_hp += 15.0,
                            Boost::Xp => self.perks.xp *= 1.12,
                        }
                        self.refresh_stats();
                        self.heal(0.25 * self.player.max_hp);
                        self.pending_chests += 1;
                        self.shrine_ev(i, ShrineWhat::Done).boost = Some(boost);
                        self.offer();
                    }
                }
                ShrineKind::Challenge => {
                    if s.state == ShrineState::Active {
                        let sh = &mut self.shrines[i];
                        sh.ticks += 1;
                        sh.charge = sh.ticks as f32 / SHRINE_FIGHT_TICKS as f32;
                        let t = sh.ticks;
                        if d2 > FIGHT_LEASH * FIGHT_LEASH {
                            let sh = &mut self.shrines[i];
                            sh.state = ShrineState::Idle;
                            sh.charge = 0.0;
                            sh.cooldown = RETRY_TICKS;
                            self.shrine_ev(i, ShrineWhat::Failed);
                        } else if t >= SHRINE_FIGHT_TICKS {
                            self.shrines[i].state = ShrineState::Done;
                            self.pending_chests += 2;
                            self.gold += 60;
                            self.heal(0.2 * self.player.max_hp);
                            self.shrine_ev(i, ShrineWhat::Done);
                            self.offer();
                        } else if t % 45 == 0 {
                            self.ring_spawn(s.site.pos, 900.0, 4 + (t / (8 * TICKS_PER_SEC)) as usize);
                        }
                    } else if entered && s.cooldown == 0 {
                        let sh = &mut self.shrines[i];
                        sh.state = ShrineState::Active;
                        sh.ticks = 0;
                        self.shrine_ev(i, ShrineWhat::Started);
                        // The wave: an elite and a ring of foes, all at once.
                        let secs = self.secs();
                        let kinds = [EnemyKind::Brute, EnemyKind::Dasher, EnemyKind::Splitter, EnemyKind::Bomber];
                        let k = kinds[self.rng.below(kinds.len() as u32) as usize];
                        let a = self.rng.range(0.0, std::f32::consts::TAU);
                        let q = [s.site.pos[0] + a.cos() * 800.0, s.site.pos[1] + a.sin() * 800.0];
                        self.spawn_elite(k, if self.walkable(q) { q } else { s.site.pos }, secs);
                        self.ring_spawn(s.site.pos, 1000.0, 12);
                    }
                }
                ShrineKind::Greed if entered => {
                    self.shrines[i].state = ShrineState::Done;
                    self.curses += 1;
                    self.gold += 50;
                    self.perks.xp *= 1.15;
                    self.refresh_stats();
                    self.pending_chests += 1;
                    self.shrine_ev(i, ShrineWhat::Done);
                    self.offer();
                }
                ShrineKind::Altar if entered => {
                    self.shrines[i].state = ShrineState::Done;
                    let secs = self.secs();
                    let kind = if self.bosses % 2 == 0 { EnemyKind::King } else { EnemyKind::Chomp };
                    let a = self.rng.range(0.0, std::f32::consts::TAU);
                    let q = [s.site.pos[0] + a.cos() * 650.0, s.site.pos[1] + a.sin() * 650.0];
                    let q = if self.walkable(q) { q } else { s.site.pos };
                    // As tough as the next scheduled boss (the boss counter scales them), without moving the schedule.
                    self.bosses += 1;
                    let ok = self.spawn_enemy(kind, q, secs);
                    self.bosses -= 1;
                    self.gold += 25;
                    self.shrine_ev(i, if ok { ShrineWhat::Done } else { ShrineWhat::Nothing });
                }
                ShrineKind::Duplicator if entered => {
                    let cost = s.dup_cost();
                    let owned: Vec<Upgrade> = ALL_UPGRADES.iter().copied().filter(|u| self.up.level(*u) > 0 && !self.up.maxed(*u) && !u.is_evolution()).collect();
                    if owned.is_empty() {
                        self.shrine_ev(i, ShrineWhat::Nothing);
                    } else if self.gold < cost {
                        self.shrine_ev(i, ShrineWhat::NeedGold(cost));
                    } else {
                        self.gold -= cost;
                        let u = owned[self.rng.below(owned.len() as u32) as usize];
                        self.up.take(u);
                        let old_max = self.player.max_hp;
                        self.refresh_stats();
                        if self.stats.max_hp > old_max {
                            self.player.hp += self.stats.max_hp - old_max;
                        }
                        let sh = &mut self.shrines[i];
                        sh.uses += 1;
                        if sh.uses >= DUP_USES {
                            sh.state = ShrineState::Done;
                        }
                        self.shrine_ev(i, ShrineWhat::Done).copied = Some(u);
                    }
                }
                _ => {}
            }
        }
        // Pots break when walked into.
        for i in 0..self.pots.len() {
            let pot = self.pots[i];
            if pot.broken || dist2(pot.site.pos, p) > POT_R * POT_R || (fy - pot.site.y).abs() > CHEST_REACH_Y {
                continue;
            }
            self.pots[i].broken = true;
            self.gold += 3 + self.rng.below(6);
            self.ev.pots_broken += 1;
            self.ev.pot_pos = pot.site.pos;
            let roll = self.rng.f32();
            if roll < 0.12 {
                self.drop_special(GemKind::Heart, [pot.site.pos[0] + 40.0, pot.site.pos[1]]);
            } else if roll < 0.16 {
                self.drop_special(GemKind::Magnet, [pot.site.pos[0] + 40.0, pot.site.pos[1]]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with(kind: ShrineKind) -> Swarm {
        let mut s = Swarm::with_cap(5, 120);
        s.explore = true;
        s.half = 1.0e6;
        s.storm = false;
        s.set_world_extras(&[ShrineSite { pos: [600.0, 0.0], y: 0.0, kind }], &[PotSite { pos: [-500.0, 0.0], y: 0.0 }]);
        s
    }

    fn stand(s: &mut Swarm, at: [f32; 2], ticks: u32) {
        for _ in 0..ticks {
            s.player.hp = s.player.max_hp;
            s.tick(&PlayerFrame { pos: at, ..Default::default() });
            while s.choices.is_some() {
                s.pick(0);
            }
        }
    }

    #[test]
    fn a_charge_shrine_charges_while_held_and_drains_outside() {
        let mut s = run_with(ShrineKind::Charge);
        stand(&mut s, [0.0, 0.0], 5);
        assert!(s.shrines[0].revealed && s.shrines[0].state == ShrineState::Idle);
        stand(&mut s, [600.0, 200.0], CHARGE_TICKS / 2);
        let half = s.shrines[0].charge;
        assert!(s.shrines[0].state == ShrineState::Active && (0.4..0.6).contains(&half), "{half}");
        stand(&mut s, [0.0, -1200.0], 90);
        assert!(s.shrines[0].charge < half, "drains outside");
        let (dmg, hp, xp) = (s.perks.damage, s.perks.max_hp, s.perks.xp);
        let cards = s.chests_opened;
        stand(&mut s, [600.0, 200.0], CHARGE_TICKS);
        assert_eq!(s.shrines[0].state, ShrineState::Done);
        assert!(s.perks.damage > dmg || s.perks.max_hp > hp || s.perks.xp > xp, "a boost");
        let _ = cards;
    }

    #[test]
    fn a_challenge_needs_the_player_to_stay() {
        let mut s = run_with(ShrineKind::Challenge);
        let before = s.alive_enemies;
        stand(&mut s, [600.0, 0.0], 2);
        assert_eq!(s.shrines[0].state, ShrineState::Active);
        assert!(s.alive_enemies > before + 5, "the wave comes");
        // Running off loses it.
        stand(&mut s, [600.0, -2000.0], 2);
        assert_eq!(s.shrines[0].state, ShrineState::Idle);
        // After the cool-down, holding on wins two picks and gold.
        stand(&mut s, [600.0, -600.0], RETRY_TICKS + 2);
        let gold = s.gold;
        stand(&mut s, [600.0, 0.0], SHRINE_FIGHT_TICKS + 5);
        assert_eq!(s.shrines[0].state, ShrineState::Done);
        assert!(s.gold >= gold + 60);
    }

    #[test]
    fn greed_pays_now_and_curses_the_rest_of_the_run() {
        let mut s = run_with(ShrineKind::Greed);
        let hp0 = {
            let mut t = run_with(ShrineKind::Greed);
            t.spawn_enemy(EnemyKind::Brute, [2000.0, 0.0], 0.0);
            t.enemies.iter().find(|e| e.alive).unwrap().max_hp
        };
        stand(&mut s, [600.0, 0.0], 1);
        assert_eq!(s.curses, 1);
        assert!(s.gold >= 50 && s.perks.xp > 1.1);
        let n = s.enemies.iter().filter(|e| e.alive).count();
        s.spawn_enemy(EnemyKind::Brute, [2000.0, 0.0], 0.0);
        let e = s.enemies.iter().filter(|e| e.alive && e.kind == EnemyKind::Brute).last().unwrap();
        assert!(e.max_hp > hp0 * 1.1, "{} vs {hp0} ({n})", e.max_hp);
        assert!(target_live_at(100.0, 300, curse_spawn(1)) > target_live_at(100.0, 300, 1.0));
    }

    #[test]
    fn the_altar_summons_a_boss_and_the_duplicator_copies_for_gold() {
        let mut s = run_with(ShrineKind::Altar);
        stand(&mut s, [600.0, 0.0], 1);
        assert!(s.enemies.iter().any(|e| e.alive && e.kind.is_boss()));
        assert_eq!(s.bosses, 0, "the schedule is untouched");

        let mut d = run_with(ShrineKind::Duplicator);
        stand(&mut d, [600.0, 0.0], 1);
        assert!(matches!(d.ev.shrine.map(|e| e.what), Some(ShrineWhat::NeedGold(30))));
        d.gold = 200;
        stand(&mut d, [0.0, 0.0], 1);
        let stars = d.up.level(Upgrade::Stars);
        stand(&mut d, [600.0, 0.0], 1);
        assert_eq!(d.up.level(Upgrade::Stars), stars + 1, "the only thing carried got a level");
        assert_eq!(d.gold, 170);
        assert_eq!(d.shrines[0].uses, 1);
        assert_eq!(d.shrines[0].dup_cost(), 60);
    }

    #[test]
    fn pots_break_once_and_pay_gold() {
        let mut s = run_with(ShrineKind::Greed);
        stand(&mut s, [-500.0, 30.0], 1);
        assert!(s.pots[0].broken && s.gold >= 3);
        let g = s.gold;
        stand(&mut s, [-500.0, 30.0], 3);
        assert_eq!(s.gold, g);
    }
}
