//! Big Bully boss from Lethal Lava Land: three hits, strong contact damage
//! (src/sim/behaviors/bigBully.ts).
use super::koopa::{js_cos_yaw, js_sin_yaw, var_or};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

const BIG_BULLY_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE | INTERACT_BREAKABLE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 3.0,
    num_loot_coins: 0.0,
    radius: 170.0,
    height: 280.0,
    hurtbox_radius: 150.0,
    hurtbox_height: 260.0,
};

pub fn init_big_bully(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.scale = 3.0;
    object.set_hitbox(BIG_BULLY_HITBOX);
    object.set_var("health", 3.0);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn big_bully_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let object = &mut m.objects[id];
    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.interact_status = 0;
        let health = var_or(object, "health", 3.0) - 1.0;
        if health <= 0.0 {
            let pos = object.pos;
            object.deactivate(-1.0);
            m.objects.big_bully_defeated(pos);
            return;
        }
        object.set_var("health", health);
        object.set_var("flash", 12.0);
        object.pos[0] += js_sin_yaw(object.face_yaw) * 50.0;
        object.pos[2] += js_cos_yaw(object.face_yaw) * 50.0;
        return;
    }
    if var_or(object, "flash", 0.0) > 0.0 { let f = object.var("flash") - 1.0; object.set_var("flash", f); }
    let t = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", t);
    if object.distance_to_mario < 2200.0 {
        object.move_yaw = atan2s(mpos[2] - object.pos[2], mpos[0] - object.pos[0]) as f64;
        object.face_yaw = object.move_yaw;
        object.forward_vel = 5.0;
    } else {
        object.forward_vel = 0.0;
    }
    object.step_physics(&world, None);
}
