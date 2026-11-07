//! Bob-omb Buddy (src/sim/behaviors/bobombBuddy.ts); also used for `bobombBuddyAdvice`.
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

const BUDDY_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 100.0,
    height: 60.0,
    hurtbox_radius: 100.0,
    hurtbox_height: 60.0,
};

pub fn init_bobomb_buddy(object: &mut SimObject) {
    object.set_hitbox(BUDDY_HITBOX);
    object.gravity = 2.5;
    object.friction = 0.8;
    object.buoyancy = 1.3;
    object.visible = true;
}

pub fn bobomb_buddy_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let object = &mut m.objects[id];
    if object.timer == 1.0 {
        let floor = world.find_floor(object.home[0], object.home[1] + 200.0, object.home[2]);
        if floor.surface.is_some() {
            object.home[1] = floor.height;
            object.pos[1] = object.home[1];
        }
    }
    object.step_physics(&world, None);
    if object.distance_to_mario < 1000.0 { let a = object.angle_to_mario; object.turn_toward_angle(a, 0x140 as f64); }
    object.face_yaw = object.move_yaw;
    object.visible = object.distance_to_mario < 3000.0;
}
