//! Port of `src/sim/behaviors/bowserFlame.ts`: Bowser's fire breath projectile.
use crate::mario_runtime::MarioRuntime;
use crate::math::{js_cos, js_sin};
use crate::object::*;
use std::f64::consts::PI;

const FLAME_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 90.0,
    height: 110.0,
    hurtbox_radius: 80.0,
    hurtbox_height: 100.0,
};

pub fn init_bowser_flame(object: &mut SimObject) {
    object.gravity = 0.0;
    object.friction = 0.0;
    object.scale = 1.5;
    object.set_hitbox(FLAME_HITBOX);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn bowser_flame_behavior(id: ObjId, m: &mut MarioRuntime) {
    let object = &mut m.objects[id];
    let t = object.var("timer") + 1.0;
    object.set_var("timer", t);
    let speed = 7.0;
    object.pos[0] += js_sin(object.move_yaw * PI * 2.0 / 65536.0) * speed;
    object.pos[2] += js_cos(object.move_yaw * PI * 2.0 / 65536.0) * speed;
    object.pos[1] = object.home[1] + js_sin(t * 0.1) * 8.0;
    if t > 100.0 { object.deactivate(-1.0); }
}
