//! SSB64's knockback, hitstun, hitlag and damage percent for the Smash fighters (engine-free, deterministic).
//!
//! The formulas are the decomp's (`reference/ssb64`):
//!
//! - **Knockback** (`ftParamGetCommonKnockback`, `src/ft/ftparam.c`), with `p` the target's percent before the hit plus
//!   the damage queued this frame (`percent_damage + damage_queue`, i.e. including this hit), `d` the hit's damage,
//!   `w` the target's `FTAttributes.weight` (a knockback multiplier: Mario 1.0, Donkey Kong 0.83, Jigglypuff 1.3):
//!   `kb = ((((p / 10 + p * d / 20) * w * 1.4) + 18) * growth / 100) + base`; a hit with set ("fixed") knockback
//!   `f != 0` uses `(((1 + 10 * f / 20) * w * 1.4) + 18) * growth / 100 + base` instead. Capped at 2500. The damage
//!   ratio and handicaps (`damage_ratio` 100, handicap 9 against 9) are 1.
//! - **Angle** (`ftCommonDamageGetKnockbackAngle`, `src/ft/ftcommon/ftcommondamage.c`): 361 is the Sakurai angle: 43
//!   degrees in the air; on the ground 0 below 32 knockback and 42.5 degrees above.
//! - **Launch** (`ftCommonDamageInitDamageVars`): the launch speed is `kb` SSB64 units per 60 Hz frame along the angle
//!   (`vel_damage_air`); every frame it loses 1.7 along its own direction (`ftMainUpdateVelDamage`, ftmain.c), on top of
//!   the fighter's own gravity.
//! - **Hitstun** (`ftParamGetHitStun`): `kb / 1.875` frames; the damage level (`ftCommonDamageGetDamageLevel`) is 0..2
//!   below 12 / 24 / 32 frames of hitstun, 3 (launched, tumbling) above.
//! - **Hitlag** (`ftParamGetHitLag`, US): `(d / 3 + 5)` frames (both fighters freeze), x1.5 for electric hits.
//! - **Weights** (`FTAttributes.weight`, `src/relocData/2xx_<Name>Main.c`): see [`weight`].
//! - **Weapons** (`WPAttributes`, `src/relocData/*Special1.c`, `*Main.c`, `251_ITCommonData.c`): the projectiles'
//!   angle / growth / base, see [`weapon`]. Projectiles without a table entry and the specials' approximate hitboxes get
//!   [`approximate`].
//!
//! Units: SSB64 units per frame -> SM64 units per 30 Hz tick is `x * K * 2` (`smash::KV`), an acceleration per frame^2
//! -> per tick^2 `x * K * 4` (`smash::KA`). [`Launch`] holds both.
use crate::smash::{HitKind, ProjKind, SmashFighter, SmashHit, SpecialMove, K};
use crate::smash_hitbox::{Element, Knockback};

/// `FTAttributes.weight` of each fighter (the knockback multiplier; the decomp's `/* weight */` field).
pub fn weight(f: SmashFighter) -> f32 {
    match f {
        SmashFighter::Mario => 1.0,
        SmashFighter::Luigi => 1.0,
        SmashFighter::Donkey => 0.83,
        SmashFighter::Link => 0.96,
        SmashFighter::Samus => 0.92,
        SmashFighter::Yoshi => 0.93,
        SmashFighter::Kirby => 1.19,
        SmashFighter::Fox => 1.0,
        SmashFighter::Pikachu => 1.16,
        SmashFighter::Purin => 1.3,
        SmashFighter::Captain => 0.96,
        SmashFighter::Ness => 1.1,
    }
}

/// Most knockback a hit can deal (`ftParamGetCommonKnockback`).
pub const KNOCKBACK_MAX: f32 = 2500.0;
/// Launch speed lost per 60 Hz frame (`vel_damage_air` decay).
pub const LAUNCH_DECAY: f32 = 1.7;
/// Sakurai angle constants (`ftcommon.h`).
pub const SAKURAI_KNOCKBACK_LOW: f32 = 32.0;
pub const SAKURAI_ANGLE_GROUND: f32 = 42.5;
pub const SAKURAI_ANGLE_AIR: f32 = 43.0;
/// Hitstun frames that make damage level 1, 2, 3 (`FTCOMMON_DAMAGE_LEVEL_HITSTUN_*`).
pub const HITSTUN_LEVELS: [f32; 3] = [12.0, 24.0, 32.0];

/// SSB64 knockback of a hit: `percent` the target's damage before the hit, `damage` the hit's (this frame's) damage.
pub fn knockback(percent: f32, damage: f32, kb: &Knockback, weight: f32) -> f32 {
    let growth = kb.growth as f32 * 0.01;
    let base = kb.base as f32;
    let k = if kb.fixed != 0 {
        (((1.0 + 10.0 * kb.fixed as f32 * 0.05) * weight * 1.4) + 18.0) * growth + base
    } else {
        let p = percent + damage;
        ((((p * 0.1) + (p * damage * 0.05)) * weight * 1.4) + 18.0) * growth + base
    };
    k.clamp(0.0, KNOCKBACK_MAX)
}

/// Launch angle in degrees (counter-clockwise from "away", 90 = straight up): the hit's, or the Sakurai angle.
pub fn launch_angle(angle: i32, airborne: bool, knockback: f32) -> f32 {
    if angle != 361 {
        angle as f32
    } else if airborne {
        SAKURAI_ANGLE_AIR
    } else if knockback < SAKURAI_KNOCKBACK_LOW {
        0.0
    } else {
        // the decomp's ramp: ((kb - 32) / 0.1) * 42.5 + 1 degrees, clamped at 42.5 (it saturates at once)
        (((knockback - SAKURAI_KNOCKBACK_LOW) / 0.099_998_47) * SAKURAI_ANGLE_GROUND + 1.0).min(SAKURAI_ANGLE_GROUND)
    }
}

/// Hitstun in 60 Hz frames (`ftParamGetHitStun`).
pub fn hitstun_frames(knockback: f32) -> f32 {
    knockback / 1.875
}

/// Damage level 0..3 of a hitstun (3: launched, tumbling).
pub fn damage_level(hitstun: f32) -> u8 {
    HITSTUN_LEVELS.iter().position(|l| hitstun < *l).unwrap_or(3) as u8
}

/// Hitlag in 60 Hz frames (`ftParamGetHitLag`, US): both sides freeze this long.
pub fn hitlag_frames(damage: f32, element: Element) -> u32 {
    let mul = if element == Element::Electric { 1.5 } else { 1.0 };
    (((damage / 3.0 + 5.0).floor()) * mul) as u32
}

/// The SSB64 numbers of a projectile or item without a script (`WPAttributes` of the decomp).
pub fn weapon(mv: SpecialMove, kind: Option<ProjKind>, damage: f32) -> Option<Knockback> {
    use SpecialMove::*;
    // (angle, growth, fixed, base, element)
    let (angle, growth, fixed, base, element) = match (mv, kind) {
        (Fireball, _) => (361, 25, 0, 10, Element::Fire),
        (Blaster, _) => (10, 100, 1, 0, Element::Normal),
        (ChargeShot, _) => (361, 100, 0, 0, Element::Electric),
        (SamusBomb, _) => (361, 65, 0, 10, Element::Normal),
        (LinkBomb, _) => (80, 20, 0, 60, Element::Normal),
        (Boomerang, _) => (70, 30, 0, 55, Element::Normal),
        (FinalCutter, Some(ProjKind::Cutter)) => (361, 50, 0, 70, Element::Normal),
        (PkThunder, _) => (100, 30, 0, 50, Element::Electric),
        (PkFire, _) => (80, 50, 0, 40, Element::Fire),
        (Thunder, _) => (70, 50, 0, 80, Element::Electric),
        (ThunderJolt, _) => (361, 20, 0, 10, Element::Electric),
        (EggThrow, _) => (361, 50, 0, 50, Element::Normal),
        (YoshiBomb, Some(ProjKind::Star)) | (EggLay, Some(ProjKind::Star)) => (361, 100, 30, 0, Element::Normal),
        // Kirby's spat star (no WPAttributes: an item-style throw), the items' throws / shots (ITCommonData's Star Rod)
        (Inhale, Some(ProjKind::Star)) => (361, 100, 0, 10, Element::Normal),
        (Item, _) => (361, 100, 0, 10, Element::Normal),
        _ => return None,
    };
    Some(Knockback { angle, growth, fixed, base, element, damage: damage.max(0.0).round() as u32, shield_damage: 0, hit_ground: true, hit_air: true, can_rebound: false, sfx_kind: 0, sfx_level: 1 })
}

/// A plausible SSB64 descriptor for a hit that has none (the specials' approximate hitboxes): Sakurai angle, growth
/// 100, base from the damage, explosions launch up.
pub fn approximate(damage: f32, kind: HitKind) -> Knockback {
    let (angle, element) = match kind {
        HitKind::Explosion => (70, Element::Normal),
        HitKind::Fire => (361, Element::Fire),
        HitKind::Electric => (361, Element::Electric),
        HitKind::Sleep => (0, Element::Sleep),
        _ => (361, Element::Normal),
    };
    let base = if matches!(kind, HitKind::Sleep | HitKind::Capture | HitKind::Grab) { 0 } else { (10.0 + damage * 1.5).min(60.0) as u32 };
    Knockback { angle, growth: 100, fixed: 0, base, element, damage: damage.max(0.0).round() as u32, shield_damage: 0, hit_ground: true, hit_air: true, can_rebound: false, sfx_kind: 0, sfx_level: 1 }
}

/// The descriptor a hit lands with: its own (scripted / weapon), else [`approximate`].
pub fn descriptor(hit: &SmashHit) -> Knockback {
    hit.knockback.unwrap_or_else(|| approximate(hit.damage, hit.kind))
}

/// One launch in SM64 units at 30 Hz.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Launch {
    /// SSB64 knockback.
    pub knockback: f32,
    /// Degrees above the horizontal, away from the attacker.
    pub angle: f32,
    /// Launch velocity: horizontal (along the hit's direction) and vertical, SM64 units per tick.
    pub speed: f32,
    pub horizontal: f32,
    pub vertical: f32,
    /// Speed lost per tick along the launch direction (SM64 units per tick, per tick).
    pub decay: f32,
    /// Hitstun and hitlag in sim ticks.
    pub hitstun: u32,
    pub hitlag: u32,
    /// 0..3; 3 = launched (tumble).
    pub level: u8,
}

impl Launch {
    /// The launch of a hit of `kb` on a target with `percent` (before the hit) and `weight`, `airborne` or not.
    pub fn of(kb: &Knockback, damage: f32, percent: f32, weight: f32, airborne: bool) -> Launch {
        let k = knockback(percent, damage, kb, weight);
        let angle = launch_angle(kb.angle, airborne, k);
        let speed = k * (K as f32) * 2.0;
        let (s, c) = angle.to_radians().sin_cos();
        let stun = hitstun_frames(k);
        Launch {
            knockback: k,
            angle,
            speed,
            horizontal: c * speed,
            vertical: s * speed,
            decay: LAUNCH_DECAY * (K as f32) * 4.0,
            hitstun: (stun / 2.0).ceil().max(1.0) as u32,
            hitlag: (hitlag_frames(damage, kb.element) as f32 / 2.0).round() as u32,
            level: damage_level(stun),
        }
    }

    /// The launch velocity after one tick of decay (`v` = (horizontal, vertical) per tick).
    pub fn decay_step(v: [f32; 2], decay: f32) -> [f32; 2] {
        let l = v[0].hypot(v[1]);
        if l <= decay {
            [0.0, 0.0]
        } else {
            [v[0] * (l - decay) / l, v[1] * (l - decay) / l]
        }
    }
}

/// A fighter's damage percent (SSB64's `percent_damage`): grows with every hit, up to 999.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Percent(pub f32);

impl Percent {
    /// Take a hit: returns the launch (computed on the percent before the hit, as the game does) and adds the damage.
    pub fn take(&mut self, kb: &Knockback, damage: f32, weight: f32, airborne: bool) -> Launch {
        let l = Launch::of(kb, damage, self.0, weight, airborne);
        self.0 = (self.0 + damage.max(0.0)).min(999.0);
        l
    }
}

/// Blast zones (SM64 units, world axes): leaving the box is a KO. Built from a mode's arena or from an SSB64 stage's
/// `blast` block (`game::ssb64::Stage`, whose x / y lines are the stage plane's).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BlastZone {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl BlastZone {
    /// A box `margin` outside a floor of half size `half` (x, z) around `centre`, from `bottom` to `top`.
    pub fn around(centre: [f32; 3], half: [f32; 2], margin: f32, bottom: f32, top: f32) -> Self {
        BlastZone { min: [centre[0] - half[0] - margin, bottom, centre[2] - half[1] - margin], max: [centre[0] + half[0] + margin, top, centre[2] + half[1] + margin] }
    }
    pub fn contains(&self, p: [f32; 3]) -> bool {
        (0..3).all(|k| p[k] >= self.min[k] && p[k] <= self.max[k])
    }
}

/// Knockback for a minigame enemy that has hit points but no percent: its missing health stands for the percent
/// (`100 * (1 - hp / max_hp)`), the weight is 1. Returns the launch speed in SM64 units per tick (the mode scales it to
/// its own push).
pub fn minigame_launch(hit: &SmashHit, hp_frac: f32) -> Launch {
    let kb = descriptor(hit);
    Launch::of(&kb, hit.damage, 100.0 * (1.0 - hp_frac.clamp(0.0, 1.0)), 1.0, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(angle: i32, growth: u32, fixed: u32, base: u32, damage: u32) -> Knockback {
        Knockback { angle, growth, fixed, base, element: Element::Normal, damage, shield_damage: 0, hit_ground: true, hit_air: true, can_rebound: true, sfx_kind: 0, sfx_level: 0 }
    }

    /// Worked by hand from `ftParamGetCommonKnockback`: Falcon Punch (25%, growth 120... the sample's numbers 24 / 361 /
    /// 120 / 30) on Mario at 0%: p = 24, (2.4 + 28.8) * 1.4 = 43.68, + 18 = 61.68, * 1.2 = 74.016, + 30 = 104.016.
    #[test]
    fn falcon_punch_on_mario_at_zero_matches_the_decomp_formula() {
        let k = knockback(0.0, 24.0, &kb(361, 120, 0, 30, 24), 1.0);
        assert!((k - 104.016).abs() < 1e-3, "{k}");
        // at 100%: p = 124, (12.4 + 148.8) * 1.4 = 225.68 + 18 = 243.68 * 1.2 = 292.416 + 30 = 322.416
        let k = knockback(100.0, 24.0, &kb(361, 120, 0, 30, 24), 1.0);
        assert!((k - 322.416).abs() < 1e-3, "{k}");
        // heavier targets fly less: Donkey Kong (0.83) vs Jigglypuff (1.3)
        let dk = knockback(100.0, 24.0, &kb(361, 120, 0, 30, 24), weight(SmashFighter::Donkey));
        let puff = knockback(100.0, 24.0, &kb(361, 120, 0, 30, 24), weight(SmashFighter::Purin));
        assert!(dk < k && k < puff);
    }

    /// Set knockback ignores the percent: Mario Tornado's last hit style (fixed 1, growth 0, base 70) is 70 at any percent;
    /// fixed 30 / growth 100 (Yoshi's stars): ((1 + 15) * 1.4 + 18) * 1 + 0 = 40.4.
    #[test]
    fn set_knockback_ignores_the_percent() {
        assert_eq!(knockback(0.0, 1.0, &kb(180, 0, 1, 70, 1), 1.0), 70.0);
        assert_eq!(knockback(150.0, 1.0, &kb(180, 0, 1, 70, 1), 1.0), 70.0);
        let k = knockback(80.0, 4.0, &kb(361, 100, 30, 0, 4), 1.0);
        assert!((k - 40.4).abs() < 1e-4, "{k}");
        assert_eq!(knockback(999.0, 99.0, &kb(361, 1000, 0, 1000, 99), 2.0), KNOCKBACK_MAX);
    }

    #[test]
    fn sakurai_angle_depends_on_air_and_strength() {
        assert_eq!(launch_angle(361, true, 10.0), 43.0);
        assert_eq!(launch_angle(361, false, 31.0), 0.0);
        assert_eq!(launch_angle(361, false, 80.0), 42.5);
        assert_eq!(launch_angle(70, false, 80.0), 70.0);
    }

    #[test]
    fn hitstun_hitlag_and_levels() {
        assert_eq!(hitstun_frames(75.0), 40.0);
        assert_eq!(damage_level(11.0), 0);
        assert_eq!(damage_level(20.0), 1);
        assert_eq!(damage_level(30.0), 2);
        assert_eq!(damage_level(40.0), 3);
        // 24% -> 24 / 3 + 5 = 13 frames; electric x1.5 (floor first): 19
        assert_eq!(hitlag_frames(24.0, Element::Normal), 13);
        assert_eq!(hitlag_frames(24.0, Element::Electric), 19);
        let l = Launch::of(&kb(361, 120, 0, 30, 24), 24.0, 0.0, 1.0, false);
        assert_eq!(l.angle, 42.5);
        assert_eq!(l.level, 3);
        assert_eq!(l.hitstun, (104.016f32 / 1.875 / 2.0).ceil() as u32);
        assert!((l.speed - 104.016 * K as f32 * 2.0).abs() < 1e-3);
        assert!((l.horizontal.hypot(l.vertical) - l.speed).abs() < 1e-3);
    }

    #[test]
    fn launch_decays_to_rest_and_percent_accumulates() {
        let mut v = [30.0, 40.0];
        let mut n = 0;
        while v != [0.0, 0.0] {
            v = Launch::decay_step(v, 2.0);
            n += 1;
            assert!(n < 100);
        }
        assert_eq!(n, 25);
        let mut p = Percent::default();
        let k = kb(361, 100, 0, 10, 10);
        let a = p.take(&k, 10.0, 1.0, false);
        let b = p.take(&k, 10.0, 1.0, false);
        assert_eq!(p.0, 20.0);
        assert!(b.knockback > a.knockback, "the same hit sends further at a higher percent");
    }

    #[test]
    fn blast_zone_and_weapon_tables() {
        let z = BlastZone::around([0.0; 3], [800.0, 800.0], 500.0, -1000.0, 2500.0);
        assert!(z.contains([1200.0, 0.0, 0.0]));
        assert!(!z.contains([1400.0, 0.0, 0.0]));
        assert!(!z.contains([0.0, 2600.0, 0.0]));
        let f = weapon(SpecialMove::Fireball, Some(ProjKind::Fireball), 7.0).unwrap();
        assert_eq!((f.angle, f.growth, f.base, f.element), (361, 25, 10, Element::Fire));
        assert!(weapon(SpecialMove::Item, Some(ProjKind::Item), 12.0).is_some());
        assert!(weapon(SpecialMove::FalconPunch, None, 25.0).is_none());
    }
}
