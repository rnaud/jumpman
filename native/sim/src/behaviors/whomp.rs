//! Whomp enemies, simplified from Whomp's Fortress (src/sim/behaviors/whomp.ts). Small Whomps
//! are one-hit enemies; the Whomp King is a three-hit boss that unlocks the WF star.
use super::koopa::{js_cos_yaw, js_sin_yaw, var_or};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

const SMALL_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE | INTERACT_BREAKABLE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 1.0,
    num_loot_coins: 0.0,
    radius: 90.0,
    height: 150.0,
    hurtbox_radius: 80.0,
    hurtbox_height: 140.0,
};

const KING_HITBOX: ObjectHitbox = ObjectHitbox {
    health: 3.0,
    radius: 180.0,
    height: 300.0,
    hurtbox_radius: 160.0,
    hurtbox_height: 280.0,
    ..SMALL_HITBOX
};

pub fn init_whomp(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.scale = 2.0;
    object.set_hitbox(SMALL_HITBOX);
    object.set_var("health", 1.0);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn init_whomp_king(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.scale = 3.0;
    object.set_hitbox(KING_HITBOX);
    object.set_var("health", 3.0);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn whomp_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let object = &mut m.objects[id];
    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.interact_status = 0;
        let health = var_or(object, "health", 1.0) - 1.0;
        if health <= 0.0 {
            let pos = object.pos;
            object.deactivate(-1.0);
            m.objects.spawn_coin(pos, false, 1.0);
            return;
        }
        object.set_var("health", health);
        object.set_var("flash", 12.0);
        return;
    }
    if var_or(object, "flash", 0.0) > 0.0 { let f = object.var("flash") - 1.0; object.set_var("flash", f); }
    let t = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", t);
    if object.distance_to_mario < 1400.0 {
        object.move_yaw = atan2s(mpos[2] - object.pos[2], mpos[0] - object.pos[0]) as f64;
        object.face_yaw = object.move_yaw;
        object.forward_vel = 3.0;
    } else {
        object.forward_vel = 0.0;
    }
    object.step_physics(&world, None);
}

pub fn whomp_king_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let object = &mut m.objects[id];
    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.interact_status = 0;
        let health = var_or(object, "health", 3.0) - 1.0;
        if health <= 0.0 {
            let pos = object.pos;
            object.deactivate(-1.0);
            m.objects.whomp_king_defeated(pos);
            return;
        }
        object.set_var("health", health);
        object.set_var("flash", 12.0);
        object.pos[0] += js_sin_yaw(object.face_yaw) * 40.0;
        object.pos[2] += js_cos_yaw(object.face_yaw) * 40.0;
        return;
    }
    if var_or(object, "flash", 0.0) > 0.0 { let f = object.var("flash") - 1.0; object.set_var("flash", f); }
    let t = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", t);
    if object.distance_to_mario < 1800.0 {
        object.move_yaw = atan2s(mpos[2] - object.pos[2], mpos[0] - object.pos[0]) as f64;
        object.face_yaw = object.move_yaw;
        object.forward_vel = 2.4;
    } else {
        object.forward_vel = 0.0;
    }
    object.step_physics(&world, None);
}
