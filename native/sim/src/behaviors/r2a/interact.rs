//! Mario <-> object interaction handlers of `interaction.c` that the round-2 actors need
//! (registered per kind with `BehaviorTable::set_interact`; returning `true` consumes the overlap).
use crate::mario_runtime::MarioRuntime;
use crate::object::*;
use crate::state::*;

/// interaction.h `INTERACT_CLAM_OR_BUBBA` (1 << 26), `INTERACT_WATER_RING` (1 << 16), `INTERACT_TORNADO`
/// (1 << 24), `INTERACT_HOOT` (1 << 0).
pub const INTERACT_CLAM_OR_BUBBA: u32 = 1 << 26;
pub const INTERACT_WATER_RING: u32 = 1 << 16;
pub const INTERACT_TORNADO: u32 = 1 << 24;
pub const INTERACT_HOOT: u32 = 1 << 0;
/// `INT_SUBTYPE_EATS_MARIO`.
pub const INT_SUBTYPE_EATS_MARIO: u32 = 0x2000;

/// `interact_clam_or_bubba`: Bubba eats Mario (`ACT_EATEN_BY_BUBBA`); anything else (the Unagi, clams)
/// hurts with `take_damage_and_knock_back`.
pub fn clam_or_bubba(id: ObjId, m: &mut MarioRuntime) -> bool {
    if m.objects[id].interact_subtype & INT_SUBTYPE_EATS_MARIO != 0 {
        if m.action != ACT_EATEN_BY_BUBBA {
            m.objects[id].interact_status = INT_STATUS_INTERACTED;
            m.interact_obj = Some(id);
            m.used_obj = Some(id);
            m.set_action(ACT_EATEN_BY_BUBBA, 0.0);
        }
        return true;
    }
    let invulnerable = m.action & ACT_FLAG_INVULNERABLE != 0 || m.invinc_timer > 0.0 || m.vanished();
    if invulnerable {
        return true;
    }
    let (pos, damage) = {
        let o = &m.objects[id];
        (o.pos, o.hitbox.damage_or_coin_value)
    };
    m.objects[id].interact_status = INT_STATUS_INTERACTED | INT_STATUS_ATTACKED_MARIO;
    m.take_damage(damage, Some(pos));
    true
}

/// `interact_water_ring`: `healCounter += 4 * oDamageOrCoinValue`; the ring sees INT_STATUS_INTERACTED.
pub fn water_ring(id: ObjId, m: &mut MarioRuntime) -> bool {
    let value = m.objects[id].hitbox.damage_or_coin_value;
    m.heal_counter += 4.0 * value;
    m.objects[id].interact_status = INT_STATUS_INTERACTED;
    true
}
