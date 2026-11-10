//! Simplified Boo: drifts toward Mario and damages on contact (src/sim/behaviors/boo.ts); Link's blade defeats it in one blow and it drops a coin.
use super::koopa::{js_cos_yaw, js_sin_yaw, var_or};
use crate::math::js_sin;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

const BOO_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 80.0,
    height: 130.0,
    hurtbox_radius: 70.0,
    hurtbox_height: 120.0,
};

pub fn init_boo(object: &mut SimObject) {
    object.gravity = 0.0;
    object.friction = 0.0;
    object.set_hitbox(BOO_HITBOX);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn boo_behavior(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let invisible = m.flags & MARIO_VANISH_CAP != 0;
    let link = m.link.enabled;
    let object = &mut m.objects[id];
    // One blow defeats it (boo.inc.c: any attack starts the death action and it leaves a coin). The TS
    // version has no way to beat a Boo at all, which left Link's sword passing through it; Mario keeps
    // that behaviour so the TypeScript goldens still match.
    if link && object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.interact_status = 0;
        let pos = object.pos;
        object.deactivate(-1.0);
        m.objects.sound(pos, "OBJ_BOO_LAUGH_LONG");
        m.objects.spawn_coin(pos, false, 1.0);
        return;
    }
    let timer = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", timer);
    let dx = mpos[0] - object.pos[0];
    let dz = mpos[2] - object.pos[2];
    let distance = js_hypot(dx, dz);
    if distance < 2600.0 && !invisible {
        let yaw = atan2s(dz, dx) as f64;
        object.move_yaw = yaw;
        object.face_yaw = yaw;
        object.forward_vel = if distance > 110.0 { 6.0 } else { 0.0 };
        if distance > 110.0 {
            let speed = 6f64.min(distance - 100.0);
            object.pos[0] += js_sin_yaw(yaw) * speed;
            object.pos[2] += js_cos_yaw(yaw) * speed;
        }
    } else {
        object.forward_vel = 0.0;
    }
    object.pos[1] = object.home[1] + js_sin(timer * 0.045) * 22.0;
    object.visible = true;
}
