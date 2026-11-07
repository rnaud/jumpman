//! Poison gas area hazard, simplified from the source HMC/BBH gas behavior
//! (src/sim/behaviors/poisonGas.ts).
use super::koopa::var_or;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

const GAS_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 120.0,
    height: 220.0,
    hurtbox_radius: 0.0,
    hurtbox_height: 0.0,
};

pub fn init_poison_gas(object: &mut SimObject) {
    object.gravity = 0.0;
    object.set_hitbox(GAS_HITBOX);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn poison_gas_behavior(id: ObjId, m: &mut MarioRuntime) {
    let (pos, timer) = {
        let object = &mut m.objects[id];
        let t = var_or(object, "timer", 0.0) + 1.0;
        object.set_var("timer", t);
        (object.pos, t)
    };
    let dx = m.pos[0] - pos[0];
    let dz = m.pos[2] - pos[2];
    let dy = m.pos[1] - pos[1];
    if dx * dx + dz * dz > 140.0 * 140.0 || dy < -40.0 || dy > 220.0 { return; }
    if timer % 45.0 != 0.0 { return; }
    m.take_damage(1.0, Some(pos));
}
