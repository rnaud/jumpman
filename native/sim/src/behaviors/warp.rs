//! Warp trigger volumes (src/sim/behaviors/warp.ts).
use crate::mario_runtime::MarioRuntime;
use crate::object::*;
use crate::state::*;

const WARP_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 50.0,
    height: 50.0,
    hurtbox_radius: 0.0,
    hurtbox_height: 0.0,
};

pub fn init_warp(object: &mut SimObject) {
    // `object.param === 0xff ? 10000 : object.param * 10 || 50`
    let radius = if object.param == 255.0 { 10000.0 } else {
        let r = object.param * 10.0;
        if r == 0.0 || r.is_nan() { 50.0 } else { r }
    };
    object.set_hitbox(ObjectHitbox { radius, ..WARP_HITBOX });
    object.gravity = 0.0;
    object.visible = false;
}

pub fn warp_behavior(id: ObjId, m: &mut MarioRuntime) {
    let o = &m.objects[id];
    let Some(dest) = o.warp_to.clone() else { return };
    if dest.is_empty() || m.action & ACT_FLAG_INTANGIBLE != 0 { return; }
    if !mario_overlaps_object(m, o) { return; }
    let (entry, area) = (o.warp_entry.clone(), o.warp_area);
    m.objects.warp(&dest, entry.as_deref(), area);
    if area.unwrap_or(0) == 0 { m.objects[id].deactivate(-1.0); }
}
