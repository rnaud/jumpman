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
    // Banjo-Kazooie doors also give the cylinder's height: `param = radius / 10 + 1000 * height / 10` (the SM64 warps
    // all have a param below 1000, so they keep the 50-unit height).
    let (param, height_tens) = if object.param >= 1000.0 { (object.param % 1000.0, (object.param / 1000.0).floor()) } else { (object.param, 0.0) };
    let radius = if param == 255.0 { 10000.0 } else {
        let r = param * 10.0;
        if r == 0.0 || r.is_nan() { 50.0 } else { r }
    };
    let height = if height_tens > 0.0 { height_tens * 10.0 } else { WARP_HITBOX.height };
    object.set_hitbox(ObjectHitbox { radius, height, ..WARP_HITBOX });
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
