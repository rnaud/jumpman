//! Mario <-> object interactions the group-2 actors need beyond the generic dispatcher
//! (src/game/interaction.c). Handlers are registered per behavior kind with
//! `BehaviorTable::set_interact`; returning `true` consumes the overlap.
use crate::mario_runtime::MarioRuntime;
use crate::object::*;
use crate::state::*;

/// interaction.h `INTERACT_FLAME`.
pub const INTERACT_FLAME: u32 = 1 << 18;
/// interaction.h `INTERACT_BOUNCE_TOP`.
pub const INTERACT_BOUNCE_TOP: u32 = 1 << 15;
/// interaction.h `INTERACT_SHOCK`.
pub const INTERACT_SHOCK: u32 = 1 << 29;

/// interaction.c `interact_flame`: burning. The sim has no burning actions yet, so Mario takes
/// the burn's damage (`hurtCounter += cap ? 12 : 18`) with a jump (`vel[1] = 31.5`, like
/// `ACT_BURNING_JUMP`) through the regular knockback.
pub fn interact_flame(id: ObjId, m: &mut MarioRuntime) -> bool {
    let invulnerable = m.action & ACT_FLAG_INVULNERABLE != 0 || m.invinc_timer > 0.0;
    if invulnerable || m.flags & (MARIO_METAL_CAP | MARIO_VANISH_CAP) != 0 {
        return true;
    }
    let pos = m.objects[id].pos;
    let damage = if m.flags & MARIO_CAP_ON_HEAD != 0 { 3.0 } else { 4.5 };
    if m.take_damage(damage, Some(pos)) {
        m.objects[id].interact_status = INT_STATUS_INTERACTED;
        m.state.vel[1] = 31.5;
    }
    true
}

/// interaction.c `interact_bounce_top`: an attack from any side but below hurts the object
/// (`attack_object`, which the behavior reads as `INT_STATUS_WAS_ATTACKED`) and a stomp bounces
/// Mario (`bounce_off_object(m, o, 30)`); anything else costs health.
pub fn bounce_top(id: ObjId, m: &mut MarioRuntime) -> bool {
    let attack = if m.flags & MARIO_METAL_CAP != 0 { ATTACK_FAST_ATTACK } else { detect_interaction(m, &m.objects[id]) };
    if attack != 0 && attack != ATTACK_FROM_BELOW {
        attack_object(&mut m.objects[id], attack);
        if matches!(attack, ATTACK_PUNCH | ATTACK_KICK_OR_TRIP) {
            // bounce_back_from_attack
            m.state.forward_vel = if m.action & ACT_FLAG_AIR != 0 { -16.0 } else { -48.0 };
        }
        if attack == ATTACK_FROM_ABOVE {
            let o = &m.objects[id];
            let y = o.pos[1] + o.hitbox.height;
            m.state.pos[1] = y;
            m.state.vel[1] = 30.0;
        }
    } else {
        let (pos, damage) = {
            let o = &m.objects[id];
            (o.pos, o.hitbox.damage_or_coin_value)
        };
        if m.take_damage(damage.max(1.0), Some(pos)) {
            m.objects[id].interact_status |= INT_STATUS_INTERACTED;
        }
    }
    true
}

/// interaction.c `interact_shock`: a shock costs `4 * damage` hurt-counter units. The sim has no
/// electrocution action yet, so Mario takes the regular knockback.
pub fn shock(id: ObjId, m: &mut MarioRuntime) -> bool {
    let invulnerable = m.action & ACT_FLAG_INVULNERABLE != 0 || m.invinc_timer > 0.0;
    if invulnerable || m.flags & MARIO_VANISH_CAP != 0 {
        return true;
    }
    let (pos, damage) = {
        let o = &m.objects[id];
        (o.pos, o.hitbox.damage_or_coin_value)
    };
    if m.take_damage(damage, Some(pos)) {
        m.objects[id].interact_status |= INT_STATUS_INTERACTED;
    }
    true
}

/// Flames use `INTERACT_FLAME`, stompable actors `INTERACT_BOUNCE_TOP`; everything else falls
/// through to the generic dispatcher.
pub fn interact(id: ObjId, m: &mut MarioRuntime) -> bool {
    let kind = m.objects[id].hitbox.interact_type;
    if kind & INTERACT_FLAME != 0 {
        return interact_flame(id, m);
    }
    if kind & INTERACT_SHOCK != 0 {
        return shock(id, m);
    }
    if kind & INTERACT_BOUNCE_TOP != 0 {
        return bounce_top(id, m);
    }
    false
}
