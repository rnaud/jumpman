//! Small Bullies from Lethal Lava Land: charge Mario and knock him back
//! (src/sim/behaviors/bully.ts).
use super::koopa::var_or;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

const BULLY_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 80.0,
    height: 120.0,
    hurtbox_radius: 70.0,
    hurtbox_height: 110.0,
};

pub fn init_bully(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.set_hitbox(BULLY_HITBOX);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn bully_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let object = &mut m.objects[id];
    let t = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", t);
    if object.distance_to_mario < 1600.0 {
        object.move_yaw = atan2s(mpos[2] - object.pos[2], mpos[0] - object.pos[0]) as f64;
        object.face_yaw = object.move_yaw;
        object.forward_vel = 7.0;
    } else {
        object.forward_vel = 0.0;
    }
    object.step_physics(&world, None);
}
