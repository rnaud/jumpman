//! Climbable tree (src/sim/behaviors/tree.ts).
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

const TREE_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_POLE,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 80.0,
    height: 500.0,
    hurtbox_radius: 80.0,
    hurtbox_height: 500.0,
};

pub fn init_tree(object: &mut SimObject) {
    object.set_hitbox(TREE_HITBOX);
    object.billboard = true;
    object.visible = true;
}

/// bhvTree is a static pole-like actor; climbing is handled by Mario's actions.
pub fn tree_behavior(_id: ObjId, _m: &mut MarioRuntime) {}
