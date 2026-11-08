//! How the three playable characters meet the OoT enemies.
//!
//! * Link: sword/stick/hammer swings (`LinkExt::swing`, damage row from `swing_damage`), the
//!   reach of the weapon, spin attacks hit all around. Projectiles, bombs and the hookshot arrive
//!   separately as `Target` hits (`Field::hit` with the `Dmg` of the `HitExtra` event).
//! * Mario and Banjo: the SM64 attack states they share (`determine_interaction`): punches,
//!   kicks, dives (and so Beak Barge/Beak Bomb), slide kicks, ground pounds (and so Beak Buster),
//!   jumping on an enemy's head, Wonderwing, plus Banjo's eggs. Each maps to a `Dmg` row so the
//!   enemy's own `DamageTable` decides the result.
//! * Everyone is hurt through `MarioRuntime::take_damage` (SM64 wedges), Link's hearts follow
//!   through the wedge bridge with the leftover of a half-heart refunded.
use super::*;
use crate::banjo_ext::wonderwing_active;
use crate::characters::link_guarding;
use crate::link::combat::{swing_damage, Swing};
use crate::link::items::HEART;
use crate::mario_runtime::MarioRuntime;
use crate::math::{atan2s, js_hypot, sins, coss, to_s16};
use crate::state_consts::*;

/// One landed blow: enemy index, damage row and where the blow came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub index: usize,
    pub dmg: Dmg,
    pub from: Vec3,
    /// A stomp: the player should bounce.
    pub stomp: bool,
}

fn horizontal_gap(a: &Vec3, b: &Vec3) -> f64 {
    js_hypot(a[0] - b[0], a[2] - b[2])
}

/// Which `Dmg` row a Mario/Banjo attack state counts as (None: not attacking).
pub fn mario_attack(m: &MarioRuntime, to_enemy_yaw: f64) -> Option<Dmg> {
    let action = m.action;
    if action & ACT_FLAG_ATTACKING == 0 {
        return None;
    }
    let d_yaw = to_s16(to_enemy_yaw - m.face_angle[1]) as f64;
    if action == ACT_PUNCHING || action == ACT_MOVE_PUNCHING || action == ACT_JUMP_KICK {
        if m.flags & MARIO_PUNCHING != 0 && d_yaw.abs() <= 0x2aaa as f64 {
            return Some(Dmg::KokiriSword);
        }
        if m.flags & MARIO_KICKING != 0 && d_yaw.abs() <= 0x2aaa as f64 {
            return Some(Dmg::MasterSword);
        }
        if m.flags & MARIO_TRIPPING != 0 && d_yaw.abs() <= 0x4000 as f64 {
            return Some(Dmg::MasterSword);
        }
        return None;
    }
    if action == ACT_GROUND_POUND {
        return (m.vel[1] < 0.0).then_some(Dmg::MasterJump);
    }
    if action == ACT_GROUND_POUND_LAND {
        return Some(Dmg::MasterJump);
    }
    if action == ACT_SLIDE_KICK || action == ACT_SLIDE_KICK_SLIDE {
        return Some(Dmg::MasterSword);
    }
    if m.forward_vel.abs() >= 26.0 {
        return Some(Dmg::MasterSword);
    }
    None
}

/// All blows the active character lands this tick. Call once per tick after the player's step
/// (the hit windows of the sword and the SM64 attack flags are set during the step).
pub fn strikes(m: &mut MarioRuntime, field: &Field) -> Vec<Strike> {
    let mut out = vec![];
    let link = m.link.enabled;
    let swing = if link && m.link.striking(m) { m.link.swing } else { None };
    // The hammer's swings are not a `Weapon` of `weapon()`: they take the hammer rows.
    let held = |s: Swing| if matches!(s, Swing::HammerForward | Swing::HammerSide) { crate::link::combat::Weapon::Hammer } else { m.link.weapon() };
    let sword_dmg = swing.map(|s: Swing| swing_damage(held(s), s));
    let reach = swing.map_or(0.0, |s| held(s).reach());
    let wonder = !link && wonderwing_active(m);
    for (i, e) in field.enemies.iter().enumerate() {
        if e.asleep || e.grace > 0.0 || !e.targetable() {
            continue;
        }
        let (c, r, h) = e.body();
        let gap = horizontal_gap(&m.pos, &c);
        let to_enemy = atan2s(c[2] - m.pos[2], c[0] - m.pos[0]) as f64;
        let vertical = m.pos[1] <= c[1] + h && m.pos[1] + 160.0 >= c[1];
        if !vertical && swing.map_or(true, |s| !s.is_jump()) {
            // Sword blows still reach a little above and below.
            if !(m.pos[1] <= c[1] + h + 60.0 && m.pos[1] + 220.0 >= c[1] - 40.0) || swing.is_none() {
                continue;
            }
        }
        // Link's weapons.
        if let (Some(dmg), Some(s)) = (sword_dmg, swing) {
            if gap < r + 37.0 + reach {
                let facing = to_s16(to_enemy - m.face_angle[1]).abs();
                if s.is_spin() || facing <= 0x3000 {
                    out.push(Strike { index: i, dmg, from: m.pos, stomp: false });
                }
            }
            continue;
        }
        if link {
            continue;
        }
        // Mario / Banjo.
        let reach_b = r + 37.0;
        if wonder && gap < reach_b + 30.0 {
            out.push(Strike { index: i, dmg: Dmg::MasterSword, from: m.pos, stomp: false });
            continue;
        }
        if gap < reach_b + 20.0 {
            if let Some(dmg) = mario_attack(m, to_enemy) {
                out.push(Strike { index: i, dmg, from: m.pos, stomp: false });
                continue;
            }
            if e.spec().stompable && m.action & ACT_FLAG_AIR != 0 && m.vel[1] < 0.0 && m.pos[1] > c[1] + h * 0.5 && gap < reach_b {
                out.push(Strike { index: i, dmg: Dmg::KokiriJump, from: m.pos, stomp: true });
            }
        }
    }
    // Banjo's blue eggs.
    if m.banjo.eggs_in_flight.is_empty() {
        return out;
    }
    for egg in m.banjo.eggs_in_flight.iter_mut() {
        if egg.age > 9000.0 {
            continue;
        }
        for (i, e) in field.enemies.iter().enumerate() {
            if e.asleep || !e.targetable() {
                continue;
            }
            let (c, r, h) = e.body();
            if horizontal_gap(&egg.pos, &c) < r + 20.0 && egg.pos[1] > c[1] - 20.0 && egg.pos[1] < c[1] + h + 20.0 {
                out.push(Strike { index: i, dmg: Dmg::Slingshot, from: egg.pos, stomp: false });
                egg.age = 1.0e6;
                break;
            }
        }
    }
    out
}

/// Apply strikes to the field, honouring the once-per-swing grace of melee blows. Returns the
/// results (for the app's sounds and the stomp bounce).
pub fn apply_strikes(m: &mut MarioRuntime, field: &mut Field, strikes: &[Strike]) -> Vec<(Strike, HitResult)> {
    let mut out = vec![];
    for s in strikes {
        let r = field.hit(s.index, s.dmg, s.from);
        let e = &mut field.enemies[s.index];
        // A blocked or landed blow starts the grace so one swing counts once.
        if r.applied || r.clank {
            e.grace = frames(if s.stomp { 12.0 } else { 14.0 });
        }
        if s.stomp && (r.applied || r.clank) {
            m.vel[1] = 30.0;
            let top = e.body().0[1] + e.body().2;
            m.pos[1] = m.pos[1].max(top);
        }
        out.push((*s, r));
    }
    out
}

/// Hurt the active character for `units` (1/16 hearts). Link's hearts lose exactly `units`.
pub fn hurt_player(m: &mut MarioRuntime, units: i32, from: Vec3) -> bool {
    let wedges = ((units as f64) / HEART as f64).ceil().max(1.0);
    let hit = m.take_damage(wedges, Some(from));
    if hit && m.link.enabled {
        // The wedge bridge removes `wedges` full hearts; hand back what the damage did not cost.
        let refund = wedges as i32 * HEART - units;
        m.link.inv.health += refund.max(0);
    }
    hit
}

/// A raised shield turns shots that would hit the front of Link around.
pub fn reflect_with_shield(m: &MarioRuntime, shots: &mut [Shot]) -> usize {
    if !(m.link.enabled && link_guarding(m)) {
        return 0;
    }
    let mut n = 0;
    for s in shots.iter_mut().filter(|s| !s.reflected) {
        if horizontal_gap(&s.pos, &m.pos) < 37.0 + 90.0 && s.pos[1] > m.pos[1] && s.pos[1] < m.pos[1] + 170.0 {
            let to_shot = atan2s(s.pos[2] - m.pos[2], s.pos[0] - m.pos[0]) as f64;
            if to_s16(to_shot - m.face_angle[1]).abs() < 0x4000 {
                s.vel = [-s.vel[0] * 1.2, s.vel[1], -s.vel[2] * 1.2];
                s.reflected = true;
                n += 1;
            }
        }
    }
    let _ = (sins(0.0), coss(0.0));
    n
}
