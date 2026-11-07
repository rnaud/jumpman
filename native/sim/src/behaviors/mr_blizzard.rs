//! Mr. Blizzard snowman enemy, simplified from the CCM behavior (src/sim/behaviors/mrBlizzard.ts).
use super::koopa::var_or;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

const BLIZZARD_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE | INTERACT_BREAKABLE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 1.0,
    num_loot_coins: 0.0,
    radius: 100.0,
    height: 170.0,
    hurtbox_radius: 90.0,
    hurtbox_height: 150.0,
};

pub fn init_mr_blizzard(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.scale = 2.0;
    object.set_hitbox(BLIZZARD_HITBOX);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn mr_blizzard_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let object = &mut m.objects[id];
    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.interact_status = 0;
        let pos = object.pos;
        object.deactivate(-1.0);
        m.objects.spawn_coin(pos, false, 1.0);
        return;
    }
    let t = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", t);
    if t % 90.0 == 0.0 && object.distance_to_mario < 900.0 {
        object.vel[1] = 24.0;
        object.forward_vel = 4.0;
        object.move_yaw = object.angle_to_mario;
        object.face_yaw = object.move_yaw;
    }
    object.step_physics(&world, None);
}
