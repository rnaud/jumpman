//! The Smash fighters' specials (`jumpman_sim::smash`) against the minigames' enemies: each mode exposes its foes as
//! [`SmashTargets`] in course coordinates, and turns a [`SmashHit`] (SSB64 percent) into its own damage, stun and kills.
//! The modes call [`resolve`] right after their own tick, so a fireball, a Falcon Punch or PK Fire hurts the same foes
//! Mario's punches and pounds do.
//!
//! - Hold the Gate: the marching foes. 1% = [`GATE_PER_PERCENT`] of a foe's health (Mario's punch is 16, a ~7% hit);
//!   electric and sleep hits stun, Kirby / Yoshi swallow a non-boss foe outright.
//! - Star Swarm: the swarm. 1% = 1/[`SWARM_PERCENT_PER_HIT`] of the run's weapon damage; sleep freezes, swallowing kills
//!   anything but a boss or an elite.
use super::gate::BoardGround;
use super::gate_core::{ekind_height, Gate};
use super::swarm_core::Swarm;
use jumpman_sim::smash::{HitKind, HitOutcome, SmashHit, SmashTargets, Target};

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
        if !e.alive {
            return HitOutcome::Miss;
        }
        let tough = e.kind.is_boss() || e.elite;
        let per_hit = self.core.stats.damage.max(1.0) / SWARM_PERCENT_PER_HIT;
        match hit.kind {
            HitKind::Sleep => {
                self.core.enemies[k].freeze = self.core.enemies[k].freeze.max(if tough { 30 } else { 90 });
                return HitOutcome::Hit;
            }
            HitKind::Capture if !tough => {
                let hp = self.core.enemies[k].hp;
                self.core.damage_enemy(k, hp + 1.0);
                return HitOutcome::Captured(None);
            }
            HitKind::Electric => {
                self.core.enemies[k].freeze = self.core.enemies[k].freeze.max(6);
            }
            _ => {}
        }
        let dmg = hit.damage.max(if hit.kind == HitKind::Capture { 7.0 } else { 0.0 }) * per_hit;
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
        SmashHit { damage, kind, dir: [0.0, 1.0, 0.0], knock: 10.0, pos: [0.0; 3], fighter: SmashFighter::Mario, mv: SpecialMove::Fireball }
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
