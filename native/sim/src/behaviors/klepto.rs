//! Simplified Klepto: circles above the desert and dives at Mario when close
//! (src/sim/behaviors/klepto.ts).
use super::koopa::{js_cos_yaw, js_random, js_sin_yaw, var_or};
use crate::js_math::{js_cos, js_sin};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

const KLEPTO_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 90.0,
    height: 120.0,
    hurtbox_radius: 80.0,
    hurtbox_height: 110.0,
};

pub fn init_klepto(object: &mut SimObject) {
    object.gravity = 0.0;
    object.friction = 0.0;
    object.scale = 2.0;
    object.set_hitbox(KLEPTO_HITBOX);
    object.set_var("timer", js_random() * 100.0);
    object.visible = true;
}

pub fn klepto_behavior(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let object = &mut m.objects[id];
    let t = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", t);
    if object.distance_to_mario < 900.0 {
        object.move_yaw = atan2s(mpos[2] - object.pos[2], mpos[0] - object.pos[0]) as f64;
        object.face_yaw = object.move_yaw;
        object.pos[0] += js_sin_yaw(object.move_yaw) * 4.0;
        object.pos[2] += js_cos_yaw(object.move_yaw) * 4.0;
    } else {
        let angle = t * 0.012;
        object.pos[0] = object.home[0] + js_sin(angle) * 500.0;
        object.pos[2] = object.home[2] + js_cos(angle) * 500.0;
    }
    object.pos[1] = object.home[1] + js_sin(t * 0.04) * 120.0;
    object.face_yaw = object.move_yaw;
}
