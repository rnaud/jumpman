//! Static red flame hazard used in the haunted mansion (src/sim/behaviors/flame.ts).
use super::koopa::var_or;
use crate::math::js_sin;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

const FLAME_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 55.0,
    height: 110.0,
    hurtbox_radius: 45.0,
    hurtbox_height: 100.0,
};

pub fn init_flame(object: &mut SimObject) {
    object.gravity = 0.0;
    object.friction = 0.0;
    object.set_hitbox(FLAME_HITBOX);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn flame_behavior(id: ObjId, m: &mut MarioRuntime) {
    let object = &mut m.objects[id];
    let t = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", t);
    object.pos[1] = object.home[1] + js_sin(t * 0.12) * 6.0;
    object.scale = 1.0 + js_sin(t * 0.2) * 0.08;
}
