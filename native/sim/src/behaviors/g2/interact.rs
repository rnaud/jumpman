//! Mario <-> object interactions the group-2 actors need beyond the generic dispatcher
//! (src/game/interaction.c). Handlers are registered per behavior kind with
//! `BehaviorTable::set_interact`; returning `true` consumes the overlap.
use crate::mario_runtime::MarioRuntime;
use crate::object::*;
use crate::state::*;

/// interaction.h `INTERACT_FLAME`.
pub const INTERACT_FLAME: u32 = 1 << 18;
/// interaction.h `INTERACT_BOUNCE_TOP`.
pub const INTERACT_BOUNCE_TOP: u32 = 1 << 16;

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

/// Flames use `INTERACT_FLAME`; everything else falls through to the generic dispatcher.
pub fn interact(id: ObjId, m: &mut MarioRuntime) -> bool {
    if m.objects[id].hitbox.interact_type & INTERACT_FLAME != 0 {
        return interact_flame(id, m);
    }
    false
}
