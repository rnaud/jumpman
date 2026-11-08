//! Star Swarm's ported species: six Ocarina of Time / Banjo-Kazooie enemies that join the wave
//! director as the run goes on (a child module of `swarm_core`; engine-free, seeded).
//!
//! * **Tektite** hops at you in bursts (decomp hp 4, one half-heart of touch damage).
//! * **Bat** weaves in fast and frail (hp 1) from the second minute.
//! * **Wolfos** circles at range and lunges (hp 8, the OoT set's toughest plain foe).
//! * **Peahat** drifts, then spins at you in a burst (hp 6, a whole heart of touch damage).
//! * **Sir Slush** hangs back and fires a ring of snowballs.
//! * **Bigbutt** is a rare heavy: it telegraphs, charges, then stands dazed (the decomp's 99 hp
//!   "cannot be hurt by ordinary blows" becomes a 260 hp wall).
//!
//! Hit points come from the decomp tables via [`Species`]: `decomp hp x 7` (min 12), contact
//! damage `hearts x 16` (min 5), then the usual wave multipliers. The tests keep the kiting
//! player's win and the cap intact.
use super::*;
use crate::experiments::foes::{Pose, Species};

pub const PORTED: [EnemyKind; 6] = [EnemyKind::Tektite, EnemyKind::Bat, EnemyKind::Wolfos, EnemyKind::Peahat, EnemyKind::SirSlush, EnemyKind::Bigbutt];

impl EnemyKind {
    pub fn species(self) -> Option<Species> {
        Some(match self {
            EnemyKind::Tektite => Species::Tektite,
            EnemyKind::Bat => Species::Bat,
            EnemyKind::Wolfos => Species::Wolfos,
            EnemyKind::Peahat => Species::Peahat,
            EnemyKind::SirSlush => Species::SirSlush,
            EnemyKind::Bigbutt => Species::Bigbutt,
            _ => return None,
        })
    }

    pub fn from_u8_ported(v: u8) -> Option<Self> {
        PORTED.iter().copied().find(|k| *k as u8 == v)
    }
}

pub fn stats(kind: EnemyKind) -> EnemyStats {
    let Some(sp) = kind.species() else { return EnemyStats { hp: 20.0, speed: 10.0, radius: 60.0, height: 100.0, contact: 5.0, xp: 1 } };
    let hp = (sp.decomp_hp() as f32 * 7.0).max(12.0);
    let contact = (sp.touch_hearts() * 16.0).max(5.0);
    match kind {
        EnemyKind::Tektite => EnemyStats { hp, speed: 12.0, radius: 60.0, height: 100.0, contact: contact.max(8.0), xp: 3 },
        // Flyers cannot be jumped over: their height is out of reach.
        EnemyKind::Bat => EnemyStats { hp: hp.max(14.0), speed: 17.0, radius: 48.0, height: 400.0, contact: contact.max(6.0), xp: 2 },
        EnemyKind::Wolfos => EnemyStats { hp, speed: 13.0, radius: 70.0, height: 120.0, contact: contact.max(11.0), xp: 5 },
        EnemyKind::Peahat => EnemyStats { hp: hp * 1.3, speed: 6.0, radius: 95.0, height: 400.0, contact, xp: 6 },
        EnemyKind::SirSlush => EnemyStats { hp: hp.max(40.0), speed: 7.0, radius: 85.0, height: 180.0, contact: 4.0, xp: 7 },
        // 99 hit points in the decomp: not hurt by ordinary blows. Here a very tall wall.
        EnemyKind::Bigbutt => EnemyStats { hp: 260.0, speed: 8.0, radius: 115.0, height: 200.0, contact: 16.0, xp: 20 },
        _ => unreachable!(),
    }
}

/// Spawn weights of the six ported kinds at `secs` (they phase in one after another).
pub fn weights(secs: f32) -> [f32; 6] {
    let ramp = |from: f32, per: f32, cap: f32| if secs < from { 0.0 } else { ((secs - from) / per).min(cap) };
    [ramp(30.0, 15.0, 5.0), ramp(60.0, 15.0, 5.0), ramp(100.0, 20.0, 4.0), ramp(150.0, 25.0, 3.0), ramp(180.0, 25.0, 3.0), ramp(210.0, 40.0, 1.5)]
}

/// First timer and seed of a new ported enemy.
pub fn init(kind: EnemyKind, rng: &mut Rng) -> (i16, u8) {
    let seed = rng.below(256) as u8;
    let timer = match kind {
        EnemyKind::Tektite => 5 + rng.below(20) as i16,
        EnemyKind::Wolfos => 40 + rng.below(30) as i16,
        EnemyKind::Peahat => 60 + rng.below(40) as i16,
        EnemyKind::SirSlush => 60 + rng.below(50) as i16,
        EnemyKind::Bigbutt => 60 + rng.below(40) as i16,
        _ => 0,
    };
    (timer, seed)
}

/// One tick of a ported enemy: the step to take, and a volley to fire (damage, ring size; 0 = aimed).
pub fn step(e: &mut Enemy, dir: [f32; 2], d: f32, tick: u32) -> ([f32; 2], Option<(f32, usize)>) {
    let mut step = [0.0f32; 2];
    let mut fire = None;
    let v = |k: f32| [dir[0] * e.speed * k, dir[1] * e.speed * k];
    let side = if (e.orbit_cd as u32 + tick / 120) % 2 == 0 { 1.0 } else { -1.0 };
    let perp = [-dir[1] * side, dir[0] * side];
    match e.kind {
        EnemyKind::Tektite => match e.state {
            0 => {
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 1;
                    e.timer = 14;
                    e.dash = dir;
                }
            }
            _ => {
                step = [e.dash[0] * e.speed * 2.3, e.dash[1] * e.speed * 2.3];
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 0;
                    e.timer = 16 + (e.orbit_cd % 12) as i16;
                }
            }
        },
        EnemyKind::Bat => {
            // Weave toward the player.
            let w = ((tick as f32 * 0.17) + e.orbit_cd as f32).sin() * 0.7;
            let (s, c) = w.sin_cos();
            let h = [dir[0] * c - dir[1] * s, dir[0] * s + dir[1] * c];
            step = [h[0] * e.speed, h[1] * e.speed];
        }
        EnemyKind::Wolfos => match e.state {
            0 => {
                let k = if d > 520.0 { v(1.0) } else if d < 300.0 { v(-0.7) } else { [perp[0] * e.speed * 0.9, perp[1] * e.speed * 0.9] };
                step = k;
                e.timer -= 1;
                if e.timer <= 0 && d < 620.0 {
                    e.state = 1;
                    e.timer = 9;
                }
            }
            1 => {
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 2;
                    e.timer = 14;
                    e.dash = dir;
                }
            }
            2 => {
                step = [e.dash[0] * 34.0, e.dash[1] * 34.0];
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 3;
                    e.timer = 22;
                }
            }
            _ => {
                step = v(-0.6);
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 0;
                    e.timer = 55 + (e.orbit_cd % 30) as i16;
                }
            }
        },
        EnemyKind::Peahat => match e.state {
            0 => {
                step = v(1.0);
                e.timer -= 1;
                if e.timer <= 0 && d < 1100.0 {
                    e.state = 1;
                    e.timer = 18;
                }
            }
            _ => {
                step = v(3.0);
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 0;
                    e.timer = 110;
                }
            }
        },
        EnemyKind::SirSlush => {
            step = if d > 1100.0 { v(1.0) } else if d < 700.0 { v(-0.8) } else { [perp[0] * e.speed * 0.4, perp[1] * e.speed * 0.4] };
            e.timer -= 1;
            if e.timer <= 0 {
                e.timer = 140;
                fire = Some((6.0, 6));
            }
        }
        EnemyKind::Bigbutt => match e.state {
            0 => {
                step = v(1.0);
                e.timer -= 1;
                if e.timer <= 0 && d < 900.0 {
                    e.state = 1;
                    e.timer = 24;
                    e.dash = dir;
                }
            }
            1 => {
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 2;
                    e.timer = 26;
                }
            }
            2 => {
                step = [e.dash[0] * 28.0, e.dash[1] * 28.0];
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 3;
                    e.timer = 45;
                }
            }
            _ => {
                e.timer -= 1;
                if e.timer <= 0 {
                    e.state = 0;
                    e.timer = 70;
                }
            }
        },
        _ => {}
    }
    (step, fire)
}

/// Pose for the view.
pub fn pose_of(e: &Enemy) -> Pose {
    match (e.kind, e.state) {
        (EnemyKind::Wolfos, 1 | 2) | (EnemyKind::Bigbutt, 1 | 2) | (EnemyKind::Tektite, 1) | (EnemyKind::Peahat, 1) => Pose::Attack,
        (EnemyKind::Bigbutt, 3) => Pose::Hurt,
        (EnemyKind::Tektite, 0) => Pose::Idle,
        (EnemyKind::SirSlush, _) if e.timer > 130 => Pose::Attack,
        _ => Pose::Move,
    }
}

impl Enemy {
    /// Wind-up telegraph of the ported kinds (the renderer shakes them).
    pub fn winding_up_ported(&self) -> bool {
        matches!((self.kind, self.state), (EnemyKind::Wolfos, 1) | (EnemyKind::Bigbutt, 1) | (EnemyKind::Peahat, 1))
    }
    /// Height of a Tektite's hop arc (0 on the ground).
    pub fn hop_height(&self) -> f32 {
        if self.kind == EnemyKind::Tektite && self.state == 1 {
            (self.timer.max(0) as f32 / 14.0 * std::f32::consts::PI).sin() * 240.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> PlayerFrame {
        PlayerFrame { pos: [0.0; 2], height: 0.0, pound_land: false }
    }

    #[test]
    fn stat_blocks_follow_the_decomp_and_stay_in_the_swarm_ballpark() {
        for k in PORTED {
            let s = enemy_stats(k);
            assert!(s.hp >= 12.0 && s.radius >= 40.0 && s.contact >= 4.0, "{k:?} {s:?}");
            assert_eq!(EnemyKind::from_u8(k as u8), Some(k));
        }
        assert!(enemy_stats(EnemyKind::Wolfos).hp > enemy_stats(EnemyKind::Peahat).hp * 0.5);
        assert!(enemy_stats(EnemyKind::Peahat).hp > enemy_stats(EnemyKind::Tektite).hp);
        assert!(enemy_stats(EnemyKind::Tektite).hp > enemy_stats(EnemyKind::Bat).hp);
        assert!(enemy_stats(EnemyKind::Bigbutt).hp > enemy_stats(EnemyKind::Brute).hp * 3.0);
        assert!(enemy_stats(EnemyKind::Peahat).contact > enemy_stats(EnemyKind::Tektite).contact);
    }

    #[test]
    fn species_phase_in_one_after_another_and_all_appear_in_a_run() {
        assert!(weights(10.0).iter().all(|w| *w == 0.0));
        assert!(weights(40.0)[0] > 0.0 && weights(40.0)[1] == 0.0);
        assert!(weights(400.0).iter().all(|w| *w > 0.0));
        let mut seen = std::collections::HashSet::new();
        let mut s = Swarm::with_cap(5, 160);
        s.player.hp = 1e5;
        s.player.max_hp = 1e5;
        for _ in 0..30 * 330 {
            s.player.hp = 1e5;
            s.player.max_hp = 1e5;
            s.tick(&frame());
            while s.choices.is_some() {
                s.pick(0);
            }
            for e in s.enemies.iter().filter(|e| e.alive) {
                seen.insert(e.kind as u8);
            }
        }
        for k in PORTED {
            assert!(seen.contains(&(k as u8)), "{k:?} never spawned in 5.5 minutes: {seen:?} w {:?} secs {}", weights(300.0), s.secs());
        }
    }

    #[test]
    fn every_ported_species_attacks_kills_and_pays_xp() {
        for k in PORTED {
            let mut s = Swarm::with_cap(9, 40);
            s.player.hp = 1e5;
            s.player.max_hp = 1e5;
            assert!(s.spawn_enemy(k, [900.0, 0.0], 100.0));
            let mut hurt = false;
            let mut orbs = false;
            for _ in 0..600 {
                s.player.hp = 1e5;
                let hp = s.player.hp;
                s.tick(&frame());
                s.player.iframes = 0;
                hurt |= s.player.hp < hp;
                orbs |= s.orbs.iter().any(|o| o.alive);
                s.freeze = false;
                if s.choices.is_some() {
                    s.pick(0);
                }
            }
            // (The auto-stars may kill it first; it must have acted or died trying.)
            let alive = s.enemies.iter().any(|e| e.alive && e.kind == k);
            assert!(hurt || orbs || !alive, "{k:?} neither hurt nor shot nor died");
            // Kill one by hand: XP gem and a kill count.
            let mut s = Swarm::with_cap(9, 40);
            s.spawn_enemy(k, [500.0, 0.0], 0.0);
            let i = s.enemies.iter().position(|e| e.alive).unwrap();
            let k0 = s.kills;
            s.damage_enemy(i, 1e6);
            assert!(!s.enemies[i].alive && s.kills == k0 + 1);
            assert!(s.gems.iter().any(|g| g.alive && g.kind == GemKind::Xp && g.value as u32 >= enemy_stats(k).xp as u32), "{k:?} pays its xp");
        }
    }

    #[test]
    fn ported_enemies_stay_in_the_arena_and_the_cap_holds() {
        let mut s = Swarm::with_cap(21, 80);
        s.tick = 30 * 300;
        s.bosses = 2;
        s.player.hp = 1e5;
        s.player.max_hp = 1e5;
        for t in 0..30 * 150 {
            let a = t as f32 * 0.02;
            s.tick(&PlayerFrame { pos: [a.cos() * 2500.0, a.sin() * 2500.0], height: 0.0, pound_land: false });
            while s.choices.is_some() {
                s.pick(0);
            }
            assert!(s.alive_enemies <= s.cap() + 4);
            for e in s.enemies.iter().filter(|e| e.alive) {
                assert!(e.pos[0].abs() <= ARENA_HALF && e.pos[1].abs() <= ARENA_HALF && e.pos[0].is_finite(), "{:?} {:?}", e.kind, e.pos);
            }
        }
    }
}
