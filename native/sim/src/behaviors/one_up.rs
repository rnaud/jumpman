//! 1-Up mushroom pickup (src/sim/behaviors/oneUp.ts).
use super::koopa::var_or;
use crate::js_math::js_sin;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

const ONE_UP_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 60.0,
    height: 80.0,
    hurtbox_radius: 50.0,
    hurtbox_height: 70.0,
};

pub fn init_one_up(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.set_hitbox(ONE_UP_HITBOX);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn one_up_behavior(id: ObjId, m: &mut MarioRuntime) {
    {
        let object = &mut m.objects[id];
        let t = var_or(object, "timer", 0.0) + 1.0;
        object.set_var("timer", t);
        object.pos[1] = object.home[1] + js_sin(t * 0.06) * 8.0;
    }
    if !mario_overlaps_object(m, &m.objects[id]) { return; }
    m.lives = 100f64.min(m.lives + 1.0);
    let pos = m.objects[id].pos;
    m.objects.one_up_collected(pos);
    m.objects[id].deactivate(-1.0);
}
