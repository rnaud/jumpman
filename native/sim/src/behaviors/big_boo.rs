//! Big Boo boss: a larger, punchable Boo used by the BBH ghost-hunt mission
//! (src/sim/behaviors/bigBoo.ts).
use super::koopa::{js_cos_yaw, js_sin_yaw, var_or};
use crate::js_math::{js_cos, js_sin};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

const BIG_BOO_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE | INTERACT_BREAKABLE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 3.0,
    num_loot_coins: 0.0,
    radius: 150.0,
    height: 260.0,
    hurtbox_radius: 120.0,
    hurtbox_height: 240.0,
};

pub fn init_big_boo(object: &mut SimObject) {
    object.gravity = 0.0;
    object.friction = 0.0;
    object.scale = 3.0;
    object.set_hitbox(BIG_BOO_HITBOX);
    object.set_var("health", 3.0);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn big_boo_behavior(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let vanished = m.flags & MARIO_VANISH_CAP != 0;
    let object = &mut m.objects[id];
    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.interact_status = 0;
        let health = var_or(object, "health", 3.0) - 1.0;
        object.set_var("health", health);
        object.set_var("flash", 12.0);
        if health <= 0.0 {
            let pos = object.pos;
            object.deactivate(-1.0);
            m.objects.big_boo_defeated(pos);
            return;
        }
        object.pos[0] += js_sin(object.angle_to_mario) * 30.0;
        object.pos[2] += js_cos(object.angle_to_mario) * 30.0;
        return;
    }

    let timer = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", timer);
    if var_or(object, "flash", 0.0) > 0.0 { let f = object.var("flash") - 1.0; object.set_var("flash", f); }
    let dx = mpos[0] - object.pos[0];
    let dz = mpos[2] - object.pos[2];
    let distance = js_hypot(dx, dz);
    if distance < 2600.0 && !vanished {
        object.move_yaw = atan2s(dz, dx) as f64;
        object.face_yaw = object.move_yaw;
        if distance > 240.0 {
            let speed = 4f64.min(distance - 220.0);
            object.pos[0] += js_sin_yaw(object.move_yaw) * speed;
            object.pos[2] += js_cos_yaw(object.move_yaw) * speed;
        }
    }
    object.pos[1] = object.home[1] + js_sin(timer * 0.035) * 28.0;
    object.scale = 3.0 + js_sin(timer * 0.08) * 0.08;
    object.visible = true;
}
