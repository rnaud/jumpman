//! What the Smash fighters' specials (`jumpman_sim::smash`) look and sound like, and what they hit outside the sim.
//!
//! - **Sound hook**: every sound moment of a special is a [`SmashSpecialSfx`] message (fighter, move, slot, phase). The
//!   sound layer only has to read them: `fn play(mut ev: MessageReader<SmashSpecialSfx>, ..) { for e in ev.read() {
//!   smash_sfx(e.fighter, ..) } }`. Nothing here plays audio. The phases are [`SfxPhase`]: `Start` (the special begins:
//!   voice / wind-up), `Charge` / `Full` (Giant Punch, Charge Shot, Fire Fox, Sing's verses), `Release` (the attack comes
//!   out), `Projectile` (a fireball, laser, boomerang, bolt... leaves), `Hit` (one of its hits lands), `Explode` (bombs,
//!   eggs), `Special` (reflect, absorb, swallow, catch), `Land` (Yoshi Bomb, Stone, Final Cutter, Falcon Kick touch down)
//!   and `End` (Rest wakes, Stone reverts, the boomerang is caught). One message per moment, never per frame.
//! - **Visuals**: projectiles and the moves' own effects are particle puffs in the style of Bowser's fire breath (the
//!   ROM's weapon models are not exported yet): a bouncing fireball, Luigi's green one, the Charge Shot growing with its
//!   level, the boomerang, bombs and eggs, lasers, the crawling Thunder Jolt, the bolt of Thunder, PK Fire's pillar, PK
//!   Thunder's ball; flames on Fire Fox and Falcon Punch / Kick, Sing's notes, Rest's bubbles, the Reflector and PSI
//!   Magnet fields. Hits spark, blasts burst.
//! - **Targets**: the SM64 objects are hit inside the sim; here the walking goombas ([`smash_vs_goombas`]); the minigames
//!   resolve against their own enemies (`experiments::smash_targets`).
use super::smash::Fighter;
use super::smash_audio::SmashSfx as SmashSfxId;
use crate::fx::particles::{BurstOptions, Particles};
use crate::game::runner::GameSim;
use bevy::prelude::*;
pub use jumpman_sim::smash::{SfxPhase, SpecialMove, SpecialSlot};
use jumpman_sim::smash::{HitKind, HitOutcome, Phase, ProjKind, SmashEvent, SmashHit, SmashTargets, Target};

/// A sound moment of a Smash special (see the module docs). Stable surface for the sound layer.
#[derive(Message, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SmashSpecialSfx {
    /// Who makes the sound (for Kirby's copied specials: the fighter whose move it is).
    pub fighter: Fighter,
    pub mv: SpecialMove,
    pub slot: SpecialSlot,
    pub phase: SfxPhase,
}

pub struct SmashFxPlugin;

impl Plugin for SmashFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SmashSpecialSfx>()
            .add_systems(FixedUpdate, smash_vs_goombas.in_set(crate::sim_plugin::PostTickSet))
            .add_systems(Update, (drain_events, projectile_fx).chain().before(crate::CameraSet))
            .add_systems(Update, play_special_sfx.after(drain_events).run_if(resource_exists::<Messages<crate::audio::SfxCue>>));
    }
}

/// Sim cues into sound messages and particle bursts (and the SSB64 hit flash, `smash_models`).
pub(crate) fn drain_events(mut sim: ResMut<GameSim>, mut sfx: MessageWriter<SmashSpecialSfx>, particles: Option<ResMut<Particles>>, mut models: Option<ResMut<super::smash_models::SmashModels>>) {
    if sim.mario.smash.events.is_empty() {
        return;
    }
    let events = std::mem::take(&mut sim.mario.smash.events);
    let mut particles = particles;
    for e in events {
        match e {
            // Items make their own sounds (`smash_models::item_sounds`).
            SmashEvent::Sfx(s) if s.mv == SpecialMove::Item => {}
            SmashEvent::Sfx(s) => {
                sfx.write(SmashSpecialSfx { fighter: Fighter::from_sim(s.fighter), mv: s.mv, slot: s.slot, phase: s.phase });
            }
            SmashEvent::Hit { pos, kind, .. } => {
                if let Some(m) = models.as_mut() {
                    super::smash_models::flash(m, pos, kind);
                }
                if let Some(p) = particles.as_mut() {
                    let color = match kind {
                        HitKind::Fire => 0xff7a20,
                        HitKind::Electric => 0xfff060,
                        HitKind::Explosion => 0xffa040,
                        _ => 0xffffff,
                    };
                    p.burst(pos, BurstOptions { color, count: 8, speed: 300.0, life: 0.3, gravity: 500.0, size: 20.0, upward: 80.0, spread: 1.0 });
                }
            }
            SmashEvent::Explode { pos, radius } => {
                if let Some(p) = particles.as_mut() {
                    p.burst(pos, BurstOptions { color: 0xff9030, count: 16, speed: (radius * 4.0) as f32, life: 0.45, gravity: 120.0, size: 46.0, upward: 120.0, spread: 1.0 });
                    p.burst(pos, BurstOptions { color: 0xffe070, count: 6, speed: (radius * 2.0) as f32, life: 0.3, gravity: 0.0, size: 34.0, upward: 40.0, spread: 1.0 });
                }
            }
            SmashEvent::Reflected { pos } => {
                if let Some(p) = particles.as_mut() {
                    p.burst(pos, BurstOptions { color: 0x70c8ff, count: 10, speed: 260.0, life: 0.3, gravity: 0.0, size: 24.0, upward: 0.0, spread: 1.0 });
                }
            }
            SmashEvent::Absorbed { pos } => {
                if let Some(p) = particles.as_mut() {
                    p.burst(pos, BurstOptions { color: 0x90ffc8, count: 10, speed: 160.0, life: 0.35, gravity: 0.0, size: 24.0, upward: 0.0, spread: 1.0 });
                }
            }
            SmashEvent::Swallowed { .. } => {
                if let Some(p) = particles.as_mut() {
                    let m = sim.mario.pos;
                    p.burst([m[0], m[1] + 80.0, m[2]], BurstOptions { color: 0xffe0f0, count: 8, speed: 200.0, life: 0.35, gravity: 0.0, size: 22.0, upward: 60.0, spread: 1.0 });
                }
            }
        }
    }
}

/// Per-frame puffs: every projectile in flight and the running special's own effects (where no SSB64 model draws them,
/// `smash_models`).
fn projectile_fx(sim: Res<GameSim>, time: Res<Time>, mut frame: Local<u32>, particles: Option<ResMut<Particles>>, models: Option<Res<super::smash_models::SmashModels>>, brawl: Option<Res<crate::experiments::brawl_view::BrawlBridge>>) {
    let Some(mut p) = particles else { return };
    let ext = &sim.mario.smash;
    // Gem Clash: the player's seat's projectiles (puffs; the SSB64 models follow the course sim's only)
    let gem: &[jumpman_sim::smash::Projectile] = brawl.as_ref().filter(|b| b.active).map_or(&[], |b| &b.smash_projectiles[..]);
    if ext.fighter.is_none() || (ext.projectiles.is_empty() && ext.special.is_none() && gem.is_empty()) {
        return;
    }
    *frame = frame.wrapping_add(1);
    let f = *frame;
    let t = time.elapsed_secs();
    for (k, pr) in ext.projectiles.iter().chain(gem.iter()).enumerate() {
        if pr.dead || (k < ext.projectiles.len() && models.as_ref().is_some_and(|m| m.covers(pr))) {
            continue;
        }
        let pos = pr.pos;
        let vel = [pr.vel[0] as f32, pr.vel[1] as f32, pr.vel[2] as f32];
        // Half the sim speed as drift: the puffs trail behind.
        let drift = [vel[0] * 3.0, vel[1] * 3.0, vel[2] * 3.0];
        match pr.kind {
            ProjKind::Fireball => {
                p.puff(pos, drift, 0xff6a18, 46.0, 0.16);
                p.puff(pos, [0.0; 3], 0xffd040, 26.0, 0.12);
            }
            ProjKind::GreenFireball => {
                p.puff(pos, drift, 0x30d040, 44.0, 0.16);
                p.puff(pos, [0.0; 3], 0xc8ff90, 24.0, 0.12);
            }
            ProjKind::ChargeShot => {
                let size = 30.0 + pr.level as f32 * 9.0;
                p.puff(pos, drift, 0xffc840, size, 0.12);
                p.puff(pos, [0.0; 3], 0xfff8c0, size * 0.55, 0.1);
            }
            ProjKind::Boomerang => {
                let a = t * 30.0;
                let off = [a.sin() as f64 * 22.0, 0.0, a.cos() as f64 * 22.0];
                p.puff([pos[0] + off[0], pos[1], pos[2] + off[2]], [0.0; 3], 0xc8964a, 26.0, 0.1);
                p.puff([pos[0] - off[0], pos[1], pos[2] - off[2]], [0.0; 3], 0xa07030, 26.0, 0.1);
            }
            ProjKind::LinkBomb => {
                p.puff(pos, [0.0; 3], 0x283c78, 44.0, 0.06);
                if f.is_multiple_of(3) {
                    p.puff([pos[0], pos[1] + 40.0, pos[2]], [0.0, 60.0, 0.0], 0xffd040, 14.0, 0.2);
                }
            }
            ProjKind::SamusBomb => {
                let blink = (pr.age / 3) % 2 == 0;
                p.puff(pos, [0.0; 3], if blink { 0xffb040 } else { 0xff5020 }, 30.0, 0.06);
            }
            ProjKind::Egg => {
                p.puff(pos, [0.0; 3], 0xf8f4e8, 44.0, 0.06);
                p.puff([pos[0], pos[1] + 8.0, pos[2]], [0.0; 3], 0x50c050, 16.0, 0.06);
            }
            ProjKind::Star => {
                p.puff(pos, drift, 0xfff060, 38.0, 0.14);
            }
            ProjKind::Laser => {
                for k in 0..4 {
                    let s = k as f64 * 0.25;
                    p.puff([pos[0] - pr.vel[0] * s, pos[1] - pr.vel[1] * s, pos[2] - pr.vel[2] * s], [0.0; 3], 0xff3048, 18.0, 0.06);
                }
            }
            ProjKind::Cutter => {
                p.puff(pos, [0.0, 200.0, 0.0], 0xa8e8ff, 40.0, 0.18);
            }
            ProjKind::ThunderJolt => {
                p.puff(pos, [0.0; 3], 0xfff040, 34.0, 0.1);
                if f.is_multiple_of(2) {
                    p.sparks(pos, 2);
                }
            }
            ProjKind::Thunderbolt => {
                for k in 0..5 {
                    let y = pos[1] + k as f64 * 70.0;
                    let jitter = ((f + k) % 3) as f64 * 12.0 - 12.0;
                    p.puff([pos[0] + jitter, y, pos[2] - jitter], [0.0; 3], if k % 2 == 0 { 0xfff8a0 } else { 0xffffff }, 36.0, 0.06);
                }
            }
            ProjKind::PkFireSpark => {
                p.puff(pos, drift, 0xff7020, 24.0, 0.12);
            }
            ProjKind::PkFirePillar => {
                let r = ((f * 37) % 100) as f64 / 100.0 - 0.5;
                p.puff([pos[0] + r * 60.0, pos[1] - 40.0, pos[2] - r * 60.0], [0.0, 420.0, 0.0], if f.is_multiple_of(2) { 0xff5010 } else { 0xffb030 }, 40.0, 0.35);
            }
            ProjKind::PkThunder => {
                p.puff(pos, [0.0; 3], 0x88c8ff, 40.0, 0.25);
                p.puff(pos, [0.0; 3], 0xffffff, 18.0, 0.12);
            }
            ProjKind::Blast | ProjKind::Item => {}
        }
    }
    // The running special's own effects.
    let Some(sp) = ext.special else { return };
    if models.as_ref().is_some_and(|m| m.covers_special(sp.mv, sp.phase, sp.total)) {
        return;
    }
    let m = &sim.mario;
    let yaw = jumpman_sim::math::angle_to_radians(m.face_angle[1]);
    let fwd = [yaw.sin(), yaw.cos()];
    let at = |ahead: f64, up: f64| [m.pos[0] + fwd[0] * ahead, m.pos[1] + up, m.pos[2] + fwd[1] * ahead];
    let ring = |p: &mut Particles, r: f64, up: f64, color: u32| {
        let a = t as f64 * 9.0 + f as f64;
        p.puff([m.pos[0] + a.sin() * r, m.pos[1] + up, m.pos[2] + a.cos() * r], [0.0; 3], color, 26.0, 0.18);
    };
    match (sp.mv, sp.phase) {
        (SpecialMove::FireFox, Phase::Start | Phase::Hold | Phase::Attack) => {
            p.puff(at(0.0, 70.0), [(fwd[0] * -200.0) as f32, -120.0, (fwd[1] * -200.0) as f32], if f.is_multiple_of(2) { 0xff6020 } else { 0xffc040 }, 52.0, 0.22);
        }
        (SpecialMove::FalconPunch, _) if (22..32).contains(&sp.total) => {
            p.flame_jet(at(90.0, 100.0), [fwd[0] as f32, 0.0, fwd[1] as f32]);
        }
        (SpecialMove::FalconKick, Phase::Start | Phase::Attack) if sp.total < 30 => {
            p.puff(at(50.0, 40.0), [0.0, 80.0, 0.0], 0xff7020, 40.0, 0.2);
        }
        (SpecialMove::FalconDive, _) | (SpecialMove::GiantPunch, Phase::Hold) if f.is_multiple_of(3) => {
            p.puff(at(30.0, 110.0), [0.0, 60.0, 0.0], 0xffd890, 24.0, 0.25);
        }
        (SpecialMove::Tornado | SpecialMove::SpinningKong | SpecialMove::ScrewAttack | SpecialMove::SpinAttack, Phase::Start | Phase::Attack) => {
            let color = match sp.mv {
                SpecialMove::ScrewAttack => 0xffe060,
                SpecialMove::SpinAttack => 0xd8f0ff,
                _ => 0xffffff,
            };
            ring(&mut p, 90.0, 70.0, color);
        }
        (SpecialMove::Reflector, Phase::Start | Phase::Hold) => ring(&mut p, 70.0, 70.0, 0x60c0ff),
        (SpecialMove::PsiMagnet, Phase::Start | Phase::Hold) => ring(&mut p, 60.0, 80.0, 0x90ffc8),
        (SpecialMove::Sing, _) if f.is_multiple_of(6) && sp.total > 15 => {
            p.puff(at(0.0, 120.0), [((f % 5) as f32 - 2.0) * 60.0, 140.0, ((f % 3) as f32 - 1.0) * 60.0], 0xff90c8, 22.0, 0.9);
        }
        (SpecialMove::Rest, Phase::Sleep) if f.is_multiple_of(20) => {
            p.puff(at(10.0, 90.0), [20.0, 60.0, 0.0], 0xe0e8ff, 26.0, 1.4);
        }
        (SpecialMove::Thunder, Phase::Attack) => {
            if f.is_multiple_of(2) {
                p.sparks(at(0.0, 70.0), 3);
            }
        }
        (SpecialMove::ChargeShot, Phase::Hold) | (SpecialMove::GiantPunch, Phase::Start) => {
            p.puff(at(50.0, 90.0), [0.0; 3], 0xffe080, 18.0 + sp.charge as f32 * 4.0, 0.08);
        }
        (SpecialMove::QuickAttack | SpecialMove::FireFox, Phase::Travel | Phase::End) => {
            p.puff(at(0.0, 50.0), [0.0; 3], 0xfff8c0, 30.0, 0.15);
        }
        (SpecialMove::PkThunder, Phase::Travel) => {
            p.puff(at(0.0, 70.0), [0.0; 3], 0x88c8ff, 60.0, 0.2);
        }
        _ => {}
    }
}

/// The walking goombas as targets (any damaging hit defeats one, like Mario's punch; Kirby / Yoshi swallow them).
struct GoombaTargets<'a> {
    goombas: &'a mut [crate::game::goomba::WalkingGoomba],
    idx: Vec<usize>,
}

impl SmashTargets for GoombaTargets<'_> {
    fn targets(&self) -> Vec<Target> {
        self.idx
            .iter()
            .map(|&i| {
                let g = &self.goombas[i];
                Target { key: (2u64 << 40) | i as u64, pos: [g.pos.x, g.pos.y, g.pos.z], radius: crate::game::goomba::RADIUS * 0.6, height: (g.top() - g.pos.y).max(60.0), fighter: None }
            })
            .collect()
    }

    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        let g = &mut self.goombas[self.idx[i]];
        if !g.tangible() || hit.kind == HitKind::Sleep || (hit.damage <= 0.0 && hit.kind != HitKind::Capture) {
            return HitOutcome::Miss;
        }
        g.defeat(false);
        if hit.kind == HitKind::Capture {
            HitOutcome::Captured(None)
        } else {
            HitOutcome::Killed
        }
    }
}

/// After each tick: the specials against the course's walking goombas.
pub fn smash_vs_goombas(mut sim: ResMut<GameSim>, goombas: Option<ResMut<crate::objects::goombas::Goombas>>) {
    let Some(mut goombas) = goombas else { return };
    let ext = &sim.mario.smash;
    if ext.fighter.is_none() || (ext.hitboxes.is_empty() && ext.projectiles.is_empty()) {
        return;
    }
    let idx: Vec<usize> = goombas.0.iter().enumerate().filter(|(_, g)| g.tangible()).map(|(i, _)| i).collect();
    if idx.is_empty() {
        return;
    }
    let mut t = GoombaTargets { goombas: &mut goombas.0, idx };
    jumpman_sim::smash::resolve(&mut sim.mario.smash, &mut t);
}

/// The fighter sounds (`smash_audio::SmashSfx`) one special sound moment plays. The SSB64 table has, per slot, an
/// optional wind-up (`*Start`), a charge loop (neutral only), the move's own sound and an optional follow-up (`*End`).
/// Moves that never send `Release` / `Projectile` (the holds, sleeps and spins) play their own sound on `Start`.
pub fn special_cues(mv: SpecialMove, slot: SpecialSlot, phase: SfxPhase) -> Vec<SmashSfxId> {
    use SfxPhase as P;
    use SmashSfxId as S;
    let (start, charge, main, end) = match slot {
        SpecialSlot::Neutral => (Some(S::NeutralSpecialStart), Some(S::NeutralSpecialCharge), S::NeutralSpecial, Some(S::NeutralSpecialEnd)),
        SpecialSlot::Up => (Some(S::UpSpecialStart), None, S::UpSpecial, None),
        SpecialSlot::Down => (None, None, S::DownSpecial, Some(S::DownSpecialEnd)),
    };
    let no_release = matches!(
        mv,
        SpecialMove::Tornado
            | SpecialMove::SpinningKong
            | SpecialMove::HandSlap
            | SpecialMove::EggLay
            | SpecialMove::Reflector
            | SpecialMove::Pound
            | SpecialMove::Sing
            | SpecialMove::Rest
            | SpecialMove::PsiMagnet
            | SpecialMove::YoshiBomb
    );
    let out: Vec<Option<S>> = match (mv, phase) {
        // Kirby: the inhale sound when he opens his mouth, the throw when he spits the star; swallowing is silent.
        (SpecialMove::Inhale, P::Start) => vec![Some(main)],
        (SpecialMove::Inhale, P::Projectile) => vec![end],
        (SpecialMove::Inhale, _) => vec![],
        // Pikachu's Thunder: the call, then the bolt when it strikes him.
        (SpecialMove::Thunder, P::Start) => vec![Some(main)],
        (SpecialMove::Thunder, P::Hit) => vec![end],
        (SpecialMove::Thunder, _) => vec![],
        (SpecialMove::HandSlap, P::Hit) => vec![Some(main)],
        (_, P::Start) if no_release => vec![start, Some(main)],
        (_, P::Start) => vec![start],
        (_, P::Charge) => vec![charge],
        (_, P::Release | P::Projectile) if !no_release => vec![Some(main)],
        (_, P::Explode | P::Special | P::Land | P::End) => vec![end],
        _ => vec![],
    };
    out.into_iter().flatten().collect()
}

/// Plays the specials' sound moments through `SfxCue::Smash` (only where the audio plugin runs).
fn play_special_sfx(mut ev: MessageReader<SmashSpecialSfx>, mut out: MessageWriter<crate::audio::SfxCue>) {
    for e in ev.read() {
        for sfx in special_cues(e.mv, e.slot, e.phase) {
            out.write(crate::audio::SfxCue::Smash { fighter: e.fighter, sfx });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every special makes at least one sound with the fighter's SSB64 table, and the two-part ones keep their order.
    #[test]
    fn every_special_has_a_sound() {
        use super::super::smash_audio::smash_sfx;
        use jumpman_sim::smash::SmashFighter as SF;
        let all = [SF::Mario, SF::Luigi, SF::Donkey, SF::Link, SF::Samus, SF::Yoshi, SF::Kirby, SF::Fox, SF::Pikachu, SF::Purin, SF::Captain, SF::Ness];
        let phases = [
            SfxPhase::Start,
            SfxPhase::Charge,
            SfxPhase::Full,
            SfxPhase::Release,
            SfxPhase::Projectile,
            SfxPhase::Hit,
            SfxPhase::Explode,
            SfxPhase::Special,
            SfxPhase::Land,
            SfxPhase::End,
        ];
        for sf in all {
            let f = Fighter::from_sim(sf);
            for slot in [SpecialSlot::Neutral, SpecialSlot::Up, SpecialSlot::Down] {
                let mv = sf.special(slot);
                let heard: usize =
                    phases.iter().flat_map(|&p| special_cues(mv, slot, p)).map(|s| smash_sfx(f, s).len()).sum();
                assert!(heard > 0, "{sf:?} {slot:?} {mv:?} is silent");
            }
        }
        let falcon = |p| special_cues(SpecialMove::FalconPunch, SpecialSlot::Neutral, p);
        assert_eq!(falcon(SfxPhase::Start), vec![SmashSfxId::NeutralSpecialStart]);
        assert_eq!(falcon(SfxPhase::Release), vec![SmashSfxId::NeutralSpecial]);
    }
    use crate::experiments::smash_targets::tests::{smash_runtime, smash_step};
    use jumpman_sim::smash::SmashFighter;

    /// Fox's laser defeats a walking goomba 600 units ahead; Kirby's inhale swallows one.
    #[test]
    fn specials_defeat_walking_goombas() {
        let mut m = smash_runtime(SmashFighter::Fox);
        let mut goombas = vec![crate::game::goomba::WalkingGoomba::new([0.0, 0.0, 600.0], None)];
        assert!(goombas[0].tangible());
        smash_step(&mut m, jumpman_sim::state::R_TRIG);
        for _ in 0..20 {
            smash_step(&mut m, 0);
            let idx = (0..goombas.len()).filter(|i| goombas[*i].tangible()).collect();
            jumpman_sim::smash::resolve(&mut m.smash, &mut GoombaTargets { goombas: &mut goombas, idx });
        }
        assert!(!goombas[0].tangible(), "defeated by the laser");
        let mut m = smash_runtime(SmashFighter::Kirby);
        let mut goombas = vec![crate::game::goomba::WalkingGoomba::new([0.0, 0.0, 180.0], None)];
        smash_step(&mut m, jumpman_sim::state::R_TRIG);
        for _ in 0..30 {
            let c = jumpman_sim::state::ControllerSnapshot { button_down: jumpman_sim::state::R_TRIG, ..Default::default() };
            m.step(&c, 0x8000 as f64, false, false);
            let idx = (0..goombas.len()).filter(|i| goombas[*i].tangible()).collect();
            jumpman_sim::smash::resolve(&mut m.smash, &mut GoombaTargets { goombas: &mut goombas, idx });
        }
        assert!(!goombas[0].tangible() && m.smash.mouth == Some(None), "swallowed");
    }
}
