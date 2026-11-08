//! Mario <-> object interaction types the original `sInteractionHandlers` has and the sim's
//! object manager lacked: `interact_bounce_top`, `interact_hit_from_below` and
//! `interact_snufit_bullet` (interaction.c). Called from `detect_mario_interactions`.
use super::*;
use crate::state::*;

/// `take_damage_and_knock_back`: marks the object INTERACTED | ATTACKED_MARIO and hurts Mario.
/// Returns false when Mario was invulnerable (nothing happens, as `sInvulnerable`).
fn take_damage_and_knock_back(m: &mut MarioRuntime, id: ObjId) -> bool {
    let (pos, damage) = { let o = &m.objects[id]; (o.pos, o.hitbox.damage_or_coin_value) };
    if m.action & ACT_FLAG_INVULNERABLE != 0 || m.invinc_timer > 0.0 || m.vanished() { return false; }
    m.objects[id].interact_status = INT_STATUS_INTERACTED | INT_STATUS_ATTACKED_MARIO;
    if damage > 0.0 { m.take_damage(damage, Some(pos)); }
    true
}

/// `bounce_back_from_attack`.
fn bounce_back_from_attack(m: &mut MarioRuntime, attack: i32) {
    if matches!(attack, ATTACK_PUNCH | ATTACK_KICK_OR_TRIP) {
        if m.action == ACT_PUNCHING { m.action = ACT_MOVE_PUNCHING; }
        m.forward_vel = if m.action & ACT_FLAG_AIR != 0 { -16.0 } else { -48.0 };
    }
}

/// `bounce_off_object`.
fn bounce_off_object(m: &mut MarioRuntime, id: ObjId, vel_y: f64) {
    let o = &m.objects[id];
    m.pos[1] = o.pos[1] + o.hitbox.height;
    m.vel[1] = vel_y;
    if m.action == ACT_GROUND_POUND || m.action == ACT_GROUND_POUND_LAND { m.set_action(ACT_FREEFALL, 0.0); }
}

/// Returns true when the object's interact type was one of ours and has been handled.
pub fn dispatch(m: &mut MarioRuntime, id: ObjId, attack: i32) -> bool {
    let t = m.objects[id].hitbox.interact_type;
    // interact_damage: only hurts (Piranha Plant, Bullet Bill); attacks do not defeat it.
    if t & INTERACT_DAMAGE != 0 && m.objects[id].var("pureDamage") != 0.0 {
        take_damage_and_knock_back(m, id);
        return true;
    }
    if t & INTERACT_BOUNCE_TOP != 0 {
        // interact_bounce_top
        let attack = if m.metal_cap_timer > 0.0 { ATTACK_FAST_ATTACK } else { attack };
        if attack != 0 && attack != ATTACK_FROM_BELOW {
            attack_object(&mut m.objects[id], attack);
            bounce_back_from_attack(m, attack);
            if attack == ATTACK_FROM_ABOVE {
                // INT_SUBTYPE_TWIRL_BOUNCE would start ACT_TWIRLING with velY 80; the sim has no
                // twirl action, so the 30 bounce is used for every object.
                bounce_off_object(m, id, 30.0);
                m.objects.sound(m.pos, "ACTION_BOUNCE_OFF_OBJECT");
            }
        } else {
            take_damage_and_knock_back(m, id);
        }
        return true;
    }
    if t & INTERACT_HIT_FROM_BELOW != 0 {
        // interact_hit_from_below
        let attack = if m.metal_cap_timer > 0.0 { ATTACK_FAST_ATTACK } else { attack };
        if attack != 0 {
            attack_object(&mut m.objects[id], attack);
            bounce_back_from_attack(m, attack);
            if attack == ATTACK_FROM_BELOW { m.vel[1] = 0.0; }
            if attack == ATTACK_FROM_ABOVE { bounce_off_object(m, id, 30.0); }
        } else {
            take_damage_and_knock_back(m, id);
        }
        return true;
    }
    if t & INTERACT_SNUFIT_BULLET != 0 {
        // interact_snufit_bullet: metal cap attacks the bullet, otherwise it hurts.
        if m.metal_cap_timer > 0.0 {
            attack_object(&mut m.objects[id], ATTACK_FAST_ATTACK);
        } else {
            take_damage_and_knock_back(m, id);
        }
        return true;
    }
    false
}
