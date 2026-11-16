//! The Smash fighters' specials (`jumpman_sim::smash`) against the minigames' enemies: each mode exposes its foes as
//! [`SmashTargets`] in course coordinates, and turns a [`SmashHit`] (SSB64 percent) into its own damage, stun and kills.
//! The modes call [`resolve`] right after their own tick, so a fireball, a Falcon Punch or PK Fire hurts the same foes
//! Mario's punches and pounds do.
//!
//! - Hold the Gate: the marching foes. 1% = [`GATE_PER_PERCENT`] of a foe's health (Mario's punch is 16, a ~7% hit);
//!   electric and sleep hits stun, Kirby / Yoshi swallow a non-boss foe outright.
//! - Star Swarm: the swarm. 1% = 1/[`SWARM_PERCENT_PER_HIT`] of the run's weapon damage; sleep freezes, swallowing kills
//!   anything but a boss or an elite.
//! - Relic Descent: the dungeon's foes. 1% = 1/[`RELIC_PERCENT_PER_SWING`] of the hero's swing (base damage x level
//!   multiplier: Mario's punch combo is a ~7% hit), knockback from SSB64's formula with the foe's missing health as its
//!   percent (`smash_knockback::minigame_launch`), electric / sleep stun, swallowing kills non-bosses.
//! - Last Star Standing: the other Marios (player's specials only). 1% = 1 hit point (their 100-point bars are SSB64-like),
//!   SSB64 knockback with their lost health as the percent, launched per tick through the mode's own knock.
//! - Star Deck: the stage's foes. A special the hero plays on the stage between cards is an attack of
//!   [`DECK_PER_PERCENT`] x percent with the card modifiers, at most `deck_core::SMASH_HITS_PER_TURN` hits a turn.
//!
//! Normal attacks (`mv: None`) are Mario's own punches and kicks, which every one of these modes already scores; only
//! the specials, projectiles and items come through here.
use super::gate::BoardGround;
use super::gate_core::{ekind_height, Gate};
use super::swarm_core::Swarm;
use jumpman_sim::smash::{HitKind, HitOutcome, SmashHit, SmashTargets, Target};

/// Per-fighter scale on the specials' minigame damage (Star Swarm, Hold the Gate, Relic Descent, Last Star Standing,
/// Star Deck), from the balance harness's swarm score (`brawl_balance_tests.rs` `swarm_score`, `native/SSB64.md`
/// "Balance"): area multi-hitters (Donkey Kong's Hand Slap, Pikachu's Thunder, Yoshi's bomb) are toned down, the
/// single-target fighters lifted, so no fighter's score runs far from the median.
pub fn minigame_scale(f: jumpman_sim::smash::SmashFighter) -> f32 {
    use jumpman_sim::smash::SmashFighter as F;
    match f {
        F::Mario => 1.25,
        F::Luigi => 1.1,
        F::Donkey => 0.5,
        F::Link => 1.3,
        F::Samus => 3.6,
        F::Yoshi => 0.42,
        F::Kirby => 1.05,
        F::Fox => 1.2,
        F::Pikachu => 0.38,
        F::Purin => 0.95,
        F::Captain => 0.8,
        F::Ness => 0.85,
    }
}

/// Hold the Gate damage per SSB64 percent.
pub const GATE_PER_PERCENT: f32 = 2.2;
/// Star Swarm: this many percent make one weapon hit (`Stats::damage`).
pub const SWARM_PERCENT_PER_HIT: f32 = 6.0;

/// Gate's foes, board-local positions lifted onto the course.
pub struct GateTargets<'a> {
    pub core: &'a mut Gate,
    pub ground: &'a BoardGround,
    idx: Vec<usize>,
}

impl<'a> GateTargets<'a> {
    pub fn new(core: &'a mut Gate, ground: &'a BoardGround) -> Self {
        let idx = (0..core.enemies.len()).filter(|&i| core.enemies[i].alive).collect();
        Self { core, ground, idx }
    }
}

impl SmashTargets for GateTargets<'_> {
    fn targets(&self) -> Vec<Target> {
        self.idx
            .iter()
            .map(|&i| {
                let e = &self.core.enemies[i];
                let lift = if e.air { 150.0 } else { 0.0 };
                Target { key: e.uid as u64, pos: self.ground.world(e.pos, lift), radius: e.radius as f64, height: (ekind_height(e.kind) as f64).max(80.0) + if e.air { 60.0 } else { 0.0 }, fighter: None }
            })
            .collect()
    }

    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        // normal attacks (`mv: None`) are Mario's punches and kicks, which the mode already scores (`HeroFrame`)
        if hit.mv.is_none() {
            return HitOutcome::Miss;
        }
        let k = self.idx[i];
        let boss = self.core.enemies[k].kind.is_boss();
        let (dmg, stun) = match hit.kind {
            HitKind::Sleep => (0.0, 120.0),
            HitKind::Capture if !boss => (1.0e6, 0.0),
            HitKind::Capture => (hit.damage.max(7.0) * GATE_PER_PERCENT, 30.0),
            HitKind::Electric => (hit.damage * GATE_PER_PERCENT, 12.0),
            _ => (hit.damage * GATE_PER_PERCENT, 0.0),
        };
        if dmg <= 0.0 && stun <= 0.0 {
            return HitOutcome::Miss;
        }
        let dmg = if dmg < 1.0e5 { dmg * minigame_scale(hit.fighter) } else { dmg };
        let dead = self.core.smash_hit(k, dmg, stun);
        match (hit.kind, dead) {
            (HitKind::Capture, true) => HitOutcome::Captured(None),
            (_, true) => HitOutcome::Killed,
            _ => HitOutcome::Hit,
        }
    }
}

/// The swarm, positions relative to the level's centre on the ground under them.
pub struct SwarmTargets<'a> {
    pub core: &'a mut Swarm,
    idx: Vec<usize>,
    pos: Vec<[f64; 3]>,
}

impl<'a> SwarmTargets<'a> {
    /// `ground(slot)` is the course height under enemy `slot`; `center` the level's centre (course x, z).
    pub fn new(core: &'a mut Swarm, center: [f64; 2], ground: impl Fn(usize) -> f32) -> Self {
        let mut idx = vec![];
        let mut pos = vec![];
        for (i, e) in core.enemies.iter().enumerate() {
            if !e.alive {
                continue;
            }
            idx.push(i);
            pos.push([center[0] + e.pos[0] as f64, ground(i) as f64, center[1] + e.pos[1] as f64]);
        }
        Self { core, idx, pos }
    }
}

impl SmashTargets for SwarmTargets<'_> {
    fn targets(&self) -> Vec<Target> {
        self.idx.iter().zip(&self.pos).map(|(&i, p)| Target { key: i as u64, pos: *p, radius: self.core.enemies[i].radius as f64, height: 160.0, fighter: None }).collect()
    }

    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        let k = self.idx[i];
        let e = &self.core.enemies[k];
        // normal attacks are Mario's own punches, which the swarm already takes
        if !e.alive || hit.mv.is_none() {
            return HitOutcome::Miss;
        }
        let tough = e.kind.is_boss() || e.elite;
        let per_hit = self.core.stats.damage.max(1.0) / SWARM_PERCENT_PER_HIT;
        match hit.kind {
            HitKind::Sleep => {
                self.core.enemies[k].freeze = self.core.enemies[k].freeze.max(if tough { 30 } else { 90 });
                return HitOutcome::Hit;
            }
            // swallowed whole: anything a few weapon hits would finish (the rest takes a heavy hit)
            HitKind::Capture if !tough && self.core.enemies[k].hp <= 4.0 * self.core.stats.damage.max(1.0) => {
                let hp = self.core.enemies[k].hp;
                self.core.damage_enemy(k, hp + 1.0);
                return HitOutcome::Captured(None);
            }
            HitKind::Electric => {
                self.core.enemies[k].freeze = self.core.enemies[k].freeze.max(6);
            }
            _ => {}
        }
        let dmg = hit.damage.max(if hit.kind == HitKind::Capture { 12.0 } else { 0.0 }) * per_hit * minigame_scale(hit.fighter);
        if dmg <= 0.0 {
            return HitOutcome::Miss;
        }
        self.core.damage_enemy(k, dmg);
        if self.core.enemies[k].alive {
            HitOutcome::Hit
        } else {
            HitOutcome::Killed
        }
    }
}

/// Star Swarm's SSB64 items (a Smash fighter's item layer, `jumpman_sim::smash::items`): a drop every 8 s, four at most,
/// the weapons and throwables of the classic set with food and the Star rarer; no containers / Poke Ball / Bumper (the
/// swarm would only bury them).
pub fn swarm_item_rates() -> jumpman_sim::smash::items::ItemRates {
    use jumpman_sim::smash::items::ItemKind as I;
    jumpman_sim::smash::items::ItemRates {
        every: 240,
        max: 4,
        pool: vec![(I::BeamSword, 4), (I::HomeRunBat, 2), (I::Fan, 2), (I::StarRod, 3), (I::RayGun, 3), (I::FireFlower, 3), (I::Hammer, 2), (I::BobOmb, 3), (I::GreenShell, 2), (I::RedShell, 2), (I::MotionBomb, 2), (I::MaximTomato, 2), (I::Heart, 1), (I::Star, 1)],
    }
}

/// One Star Swarm tick of the items: its drop rule, food heals the run's hero (Maxim Tomato half, Heart Container all of
/// its health), the Star makes it invulnerable. (Item hits reach the swarm through [`SwarmTargets`].)
pub fn swarm_items(core: &mut Swarm, items: &mut jumpman_sim::smash::items::Items) {
    use jumpman_sim::smash::items::ItemKind as I;
    if items.rates.is_none() {
        items.rates = Some(swarm_item_rates());
    }
    if let Some(k) = items.last_meal.take() {
        let max = core.player.max_hp;
        core.heal(if k == I::Heart { max } else { 0.5 * max });
    }
    if items.star > 0 {
        core.player.iframes = core.player.iframes.max(2);
    }
}

/// Relic Descent: this many percent make one swing's damage.
pub const RELIC_PERCENT_PER_SWING: f32 = 7.0;
/// Star Deck damage per SSB64 percent of a special.
pub const DECK_PER_PERCENT: f32 = 0.5;

/// Relic Descent's foes on the dungeon floor (course coordinates).
pub struct RelicTargets<'a> {
    pub core: &'a mut super::relic_core::Relic,
    idx: Vec<usize>,
}

impl<'a> RelicTargets<'a> {
    pub fn new(core: &'a mut super::relic_core::Relic) -> Self {
        let idx = (0..core.enemies.len()).filter(|&i| core.enemies[i].alive && core.enemies[i].state != super::relic_core::EState::Hidden).collect();
        Self { core, idx }
    }
}

impl SmashTargets for RelicTargets<'_> {
    fn targets(&self) -> Vec<Target> {
        self.idx
            .iter()
            .map(|&i| {
                let e = &self.core.enemies[i];
                Target { key: i as u64, pos: self.core.dungeon.world(e.pos, 0.0), radius: e.radius as f64, height: (e.radius * 2.2).max(90.0) as f64, fighter: None }
            })
            .collect()
    }

    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        if hit.mv.is_none() {
            return HitOutcome::Miss;
        }
        let k = self.idx[i];
        let e = self.core.enemies[k];
        if !e.alive {
            return HitOutcome::Miss;
        }
        let boss = e.kind == super::relic_core::EnemyKind::King;
        let swing = self.core.stats.base_damage * self.core.stats.dmg_mult;
        let per = swing / RELIC_PERCENT_PER_SWING;
        let from = self.core.dungeon.plane([hit.pos[0] - hit.dir[0] * 50.0, hit.pos[1], hit.pos[2] - hit.dir[2] * 50.0]);
        match hit.kind {
            HitKind::Sleep => {
                self.core.enemies[k].stun = self.core.enemies[k].stun.max(if boss { 30 } else { 120 });
                return HitOutcome::Hit;
            }
            HitKind::Capture if !boss => {
                let hp = e.hp + e.shield + 1.0;
                self.core.damage_enemy(k, hp * 4.0, from, false, 0.0);
                return if self.core.enemies[k].alive { HitOutcome::Hit } else { HitOutcome::Captured(None) };
            }
            _ => {}
        }
        let l = jumpman_sim::smash_knockback::minigame_launch(hit, e.hp / e.max_hp.max(1.0));
        let dmg = hit.damage.max(if hit.kind == HitKind::Capture { 7.0 } else { 0.0 }) * per * minigame_scale(hit.fighter);
        if dmg <= 0.0 {
            return HitOutcome::Miss;
        }
        self.core.damage_enemy(k, dmg, from, false, l.speed.min(60.0));
        if hit.kind == HitKind::Electric {
            self.core.enemies[k].stun = self.core.enemies[k].stun.max(12);
        }
        if self.core.enemies[k].alive { HitOutcome::Hit } else { HitOutcome::Killed }
    }
}

/// Last Star Standing's other Marios (course coordinates, the player is fighter 0 and never a target).
pub struct LssTargets<'a> {
    pub core: &'a mut super::lss_core::Lss,
    idx: Vec<usize>,
}

impl<'a> LssTargets<'a> {
    pub fn new(core: &'a mut super::lss_core::Lss) -> Self {
        let idx = (1..core.fighters.len()).filter(|&i| core.fighters[i].alive).collect();
        Self { core, idx }
    }
}

impl SmashTargets for LssTargets<'_> {
    fn targets(&self) -> Vec<Target> {
        self.idx
            .iter()
            .map(|&i| {
                let f = &self.core.fighters[i];
                Target { key: i as u64, pos: [f.pos[0] as f64, f.pos[1] as f64, f.pos[2] as f64], radius: super::lss_core::MARIO_RADIUS as f64, height: 160.0, fighter: None }
            })
            .collect()
    }

    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        if hit.mv.is_none() || hit.kind == HitKind::Grab {
            return HitOutcome::Miss;
        }
        let k = self.idx[i];
        let f = &self.core.fighters[k];
        if !f.alive {
            return HitOutcome::Miss;
        }
        let hp_frac = f.hp / super::lss_core::MAX_HP;
        let raw = match hit.kind {
            HitKind::Sleep => 0.0,
            HitKind::Capture => hit.damage.max(10.0),
            _ => hit.damage,
        } * minigame_scale(hit.fighter);
        let l = jumpman_sim::smash_knockback::minigame_launch(hit, hp_frac);
        let d = [hit.dir[0] as f32, hit.dir[2] as f32];
        let n = d[0].hypot(d[1]).max(1e-3);
        let dealt = self.core.smash_hit(k, raw, [d[0] / n, d[1] / n], l.horizontal.min(70.0), l.vertical.min(60.0), [hit.pos[0] as f32, hit.pos[1] as f32, hit.pos[2] as f32]);
        if dealt <= 0.0 && raw > 0.0 {
            return HitOutcome::Miss;
        }
        if self.core.fighters[k].hp <= 0.0 { HitOutcome::Killed } else { HitOutcome::Hit }
    }
}

/// Star Deck's foes where they stand on the stage (`DeckGame::foes3d` homes, course coordinates).
pub struct DeckTargets<'a> {
    pub combat: &'a mut super::deck_core::Combat,
    pub homes: Vec<[f64; 3]>,
}

impl SmashTargets for DeckTargets<'_> {
    fn targets(&self) -> Vec<Target> {
        self.homes
            .iter()
            .enumerate()
            .filter(|(i, _)| self.combat.foes.get(*i).is_some_and(|f| f.alive))
            .map(|(i, h)| Target { key: i as u64, pos: *h, radius: 70.0, height: 180.0, fighter: None })
            .collect()
    }

    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        if hit.mv.is_none() || matches!(hit.kind, HitKind::Sleep | HitKind::Grab) {
            return HitOutcome::Miss;
        }
        let Some(k) = self.targets().get(i).map(|t| t.key as usize) else { return HitOutcome::Miss };
        let base = (hit.damage.max(if hit.kind == HitKind::Capture { 8.0 } else { 0.0 }) * DECK_PER_PERCENT * minigame_scale(hit.fighter)).round() as i32;
        if !self.combat.smash_strike(k, base) {
            return HitOutcome::Miss;
        }
        if self.combat.foes[k].alive { HitOutcome::Hit } else { HitOutcome::Killed }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jumpman_sim::smash::{SmashFighter, SpecialMove};

    /// Mario's sim on a flat floor playing a Smash fighter.
    pub(crate) fn smash_runtime(f: jumpman_sim::smash::SmashFighter) -> jumpman_sim::mario_runtime::MarioRuntime {
        use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
        use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
        let mut w = CollisionWorld::default();
        let o = SurfaceOptions::default();
        w.add_triangle([-8000.0, 0.0, -8000.0], [-8000.0, 0.0, 8000.0], [8000.0, 0.0, 8000.0], &o);
        w.add_triangle([-8000.0, 0.0, -8000.0], [8000.0, 0.0, 8000.0], [8000.0, 0.0, -8000.0], &o);
        let mut m = MarioRuntime::new(std::sync::Arc::new(w), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1600.0);
        m.smash.fighter = Some(f);
        for _ in 0..3 {
            smash_step(&mut m, 0);
        }
        m
    }

    pub(crate) fn smash_step(m: &mut jumpman_sim::mario_runtime::MarioRuntime, pressed: u32) {
        let c = jumpman_sim::state::ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: 0.0, stick_mag: 0.0, button_down: pressed, button_pressed: pressed };
        m.step(&c, 0x8000 as f64, false, false);
    }

    fn hit(kind: HitKind, damage: f32) -> SmashHit {
        SmashHit { damage, kind, dir: [0.0, 1.0, 0.0], knock: 10.0, pos: [0.0; 3], fighter: SmashFighter::Mario, mv: Some(SpecialMove::Fireball), knockback: None, motion: None }
    }

    #[test]
    fn swarm_foes_take_smash_hits() {
        let mut s = Swarm::new(7);
        assert!(s.spawn_enemy(crate::experiments::swarm_core::EnemyKind::Skitter, [200.0, 0.0], 0.0));
        let mut t = SwarmTargets::new(&mut s, [1000.0, 2000.0], |_| 50.0);
        let list = t.targets();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].pos, [1200.0, 50.0, 2000.0], "course coordinates");
        let hp = t.core.enemies[t.idx[0]].hp;
        assert_ne!(t.hit(0, &hit(HitKind::Fire, 6.0)), HitOutcome::Miss);
        assert!(t.core.enemies[t.idx[0]].hp < hp || !t.core.enemies[t.idx[0]].alive);
        assert_eq!(t.hit(0, &hit(HitKind::Sleep, 0.0)), HitOutcome::Hit);
    }

    /// Relic Descent: a fireball special hurts and pushes a dungeon foe; a Mario punch (a normal attack) is left to the
    /// mode; Sing puts a foe to sleep.
    #[test]
    fn relic_foes_take_smash_hits() {
        use super::super::relic_core::{EnemyKind, Hero, Relic};
        let mut r = Relic::new(11, Hero::Mario, 1, 0);
        let slot = (0..r.enemies.len()).find(|&i| !r.enemies[i].alive).unwrap();
        let p = r.player.pos;
        let k = r.spawn_enemy(EnemyKind::Scamp, [p[0] + 300.0, p[1]], r.enemies[slot].room, 0).unwrap();
        let mut t = RelicTargets::new(&mut r);
        let list = t.targets();
        let i = list.iter().position(|x| x.key == k as u64).unwrap();
        let hp = t.core.enemies[k].hp;
        let mut punch = hit(HitKind::Normal, 7.0);
        punch.mv = None;
        assert_eq!(t.hit(i, &punch), HitOutcome::Miss, "the mode scores Mario's own punches");
        assert_ne!(t.hit(i, &hit(HitKind::Fire, 7.0)), HitOutcome::Miss);
        assert!(t.core.enemies[k].hp < hp || !t.core.enemies[k].alive);
        assert!(!t.core.enemies[k].alive || t.core.enemies[k].vel != [0.0; 2], "knocked back");
        let mut r2 = Relic::new(11, Hero::Mario, 1, 0);
        let k2 = r2.spawn_enemy(EnemyKind::Scamp, [p[0] + 300.0, p[1]], r2.enemies[slot].room, 0).unwrap();
        let mut t2 = RelicTargets::new(&mut r2);
        let i2 = t2.targets().iter().position(|x| x.key == k2 as u64).unwrap();
        assert_eq!(t2.hit(i2, &hit(HitKind::Sleep, 0.0)), HitOutcome::Hit);
        assert!(t2.core.enemies[k2].stun >= 100);
    }

    /// Last Star Standing: a special hurts and launches another Mario; the player is never a target.
    #[test]
    fn lss_marios_take_smash_hits() {
        use super::super::lss_core::{tests::Flat, Arena, Config, Lss};
        let flat = Flat { edge: 6300.0, hazard: None };
        let mut l = Lss::new(Config::new(4, 3, Arena { cx: 0.0, cz: 0.0, r: 6000.0, top_y: 0.0, kill_y: -600.0 }), &flat);
        for f in l.fighters.iter_mut() {
            f.iframes = 0;
            f.mode = super::super::lss_core::Mode::Ground;
        }
        let mut t = LssTargets::new(&mut l);
        let list = t.targets();
        assert!(list.iter().all(|x| x.key != 0), "never the player");
        let k = list[0].key as usize;
        let hp = t.core.fighters[k].hp;
        assert_eq!(t.hit(0, &hit(HitKind::Explosion, 15.0)), HitOutcome::Hit);
        assert!((t.core.fighters[k].hp - (hp - 15.0 * minigame_scale(SmashFighter::Mario))).abs() < 0.01, "1% = 1 hit point (x the fighter's minigame scale)");
        assert!(t.core.fighters[k].knocked && t.core.fighters[k].knock[1] > 0.0, "launched");
    }

    /// Star Deck: a special on the stage is an attack on the foe it reaches, two hits a turn at most.
    #[test]
    fn deck_foes_take_two_smash_hits_a_turn() {
        use super::super::deck_core::{Combat, EnemyKind, Hero, POTION_SLOTS, SMASH_HITS_PER_TURN};
        let mut c = Combat::new(3, Hero::Mario, 60, 60, &Hero::Mario.starter_deck(), &[], [None; POTION_SLOTS], &[EnemyKind::Scuttler]);
        let hp = c.foes[0].hp;
        let mut t = DeckTargets { combat: &mut c, homes: vec![[0.0, 0.0, 300.0]] };
        assert_eq!(t.targets().len(), 1);
        for _ in 0..SMASH_HITS_PER_TURN {
            assert_ne!(t.hit(0, &hit(HitKind::Fire, 8.0)), HitOutcome::Miss);
        }
        assert_eq!(t.hit(0, &hit(HitKind::Fire, 8.0)), HitOutcome::Miss, "the turn's special hits are spent");
        assert!(t.combat.foes[0].hp < hp || !t.combat.foes[0].alive);
    }

    /// Star Swarm items: they drop near the hero, a thrown shell hurts a swarm foe, a tomato heals the run's hero.
    #[test]
    fn swarm_items_drop_hit_and_heal() {
        use jumpman_sim::smash::items::ItemKind as I;
        use jumpman_sim::smash::resolve;
        let mut m = smash_runtime(SmashFighter::Mario);
        m.smash.items.enabled = true;
        let mut s = Swarm::new(7);
        swarm_items(&mut s, &mut m.smash.items);
        assert_eq!(m.smash.items.rates.as_ref().unwrap().every, 240);
        for _ in 0..250 {
            smash_step(&mut m, 0);
        }
        assert!(!m.smash.items.list.is_empty(), "an item dropped near the hero");
        // a green shell thrown at a foe 300 ahead
        m.smash.items.list.clear();
        m.smash.items.enabled = false;
        m.smash.items.spawn(I::GreenShell, [0.0, 10.0, 40.0]);
        for _ in 0..20 {
            smash_step(&mut m, 0);
        }
        assert!(s.spawn_enemy(crate::experiments::swarm_core::EnemyKind::Skitter, [0.0, 320.0], 0.0));
        let slot = s.enemies.iter().position(|e| e.alive).unwrap();
        let hp = s.enemies[slot].hp;
        smash_step(&mut m, jumpman_sim::state::B_BUTTON);
        for _ in 0..12 {
            smash_step(&mut m, 0);
        }
        let c = jumpman_sim::state::ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: 64.0, stick_mag: 64.0, button_down: jumpman_sim::state::B_BUTTON, button_pressed: jumpman_sim::state::B_BUTTON };
        m.step(&c, 0x8000 as f64, false, false);
        for _ in 0..40 {
            smash_step(&mut m, 0);
            let mut t = SwarmTargets::new(&mut s, [0.0, 0.0], |_| 0.0);
            resolve(&mut m.smash, &mut t);
        }
        assert!(!s.enemies[slot].alive || s.enemies[slot].hp < hp, "the shell hit the swarm foe");
        // food
        s.player.hp = 10.0;
        m.smash.items.last_meal = Some(I::MaximTomato);
        swarm_items(&mut s, &mut m.smash.items);
        assert!(s.player.hp > 10.0 + 0.4 * s.player.max_hp);
        m.smash.items.star = 30;
        swarm_items(&mut s, &mut m.smash.items);
        assert!(s.player.iframes > 0);
    }

    /// A special hits a minigame enemy: Mario's fireball, thrown on the sim, burns a Star Swarm foe 500 units ahead.
    #[test]
    fn a_fireball_from_the_sim_hurts_a_swarm_foe() {
        use jumpman_sim::smash::{resolve, SmashFighter};
        let mut m = smash_runtime(SmashFighter::Mario);
        let mut s = Swarm::new(7);
        assert!(s.spawn_enemy(crate::experiments::swarm_core::EnemyKind::Skitter, [0.0, 500.0], 0.0));
        let slot = s.enemies.iter().position(|e| e.alive).unwrap();
        let hp = s.enemies[slot].hp;
        smash_step(&mut m, jumpman_sim::state::R_TRIG);
        for _ in 0..40 {
            smash_step(&mut m, 0);
            let mut t = SwarmTargets::new(&mut s, [0.0, 0.0], |_| 0.0);
            resolve(&mut m.smash, &mut t);
        }
        assert!(!s.enemies[slot].alive || s.enemies[slot].hp < hp, "the fireball reached the foe");
    }
}
