//! Port of `src/sim/behaviors/breakableBox.ts` (breakable_box.inc.c / breakable_box_small.inc.c).
use super::moving_yellow_coin::spawn_moving_yellow_coins;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

pub const BREAKABLE_BOX_BP_NO_COINS: i32 = 0;
pub const BREAKABLE_BOX_BP_3_COINS: i32 = 1;
pub const BREAKABLE_BOX_BP_5_COINS: i32 = 2;
pub const BREAKABLE_BOX_BP_GIANT: i32 = 3;

const BREAKABLE_BOX_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_BREAKABLE,
    down_offset: 20.0,
    damage_or_coin_value: 0.0,
    health: 1.0,
    num_loot_coins: 0.0,
    radius: 150.0,
    height: 200.0,
    hurtbox_radius: 150.0,
    hurtbox_height: 200.0,
};

const BREAKABLE_BOX_SMALL_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_GRABBABLE,
    down_offset: 20.0,
    damage_or_coin_value: 0.0,
    health: 1.0,
    num_loot_coins: 0.0,
    radius: 150.0,
    height: 250.0,
    hurtbox_radius: 150.0,
    hurtbox_height: 250.0,
};

/// `param & 0xff` (JS bitwise semantics on the f64 param).
fn low_param(object: &SimObject) -> i32 { (object.param as i64 & 0xff) as i32 }

fn loot_coins(param: i32) -> f64 {
    if param == BREAKABLE_BOX_BP_3_COINS { return 3.0; }
    if param == BREAKABLE_BOX_BP_5_COINS { return 5.0; }
    0.0
}

pub fn init_breakable_box(object: &mut SimObject) {
    let coins = loot_coins(low_param(object));
    object.set_hitbox(ObjectHitbox { num_loot_coins: coins, ..BREAKABLE_BOX_HITBOX });
    object.scale = if low_param(object) == BREAKABLE_BOX_BP_GIANT { 1.5 } else { 1.0 };
    object.gravity = 0.0;
    object.friction = 1.0;
    object.visible = true;
}

pub fn breakable_box_behavior(id: ObjId, m: &mut MarioRuntime) {
    let object = &mut m.objects[id];
    object.timer += 1.0;
    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.interact_status = 0;
        let (pos, coins) = (object.pos, object.hitbox.num_loot_coins);
        m.objects.break_object(id);
        let mut i = 0.0;
        while i < coins {
            m.objects.spawn_coin(pos, true, 1.0);
            i += 1.0;
        }
        m.objects[id].deactivate(-1.0);
    }
}

pub fn init_breakable_box_small(object: &mut SimObject) {
    object.gravity = 2.5;
    object.friction = 0.99;
    object.buoyancy = 1.4;
    object.scale = 0.4;
    // obj_set_hitbox multiplies all cylinder dimensions by the corkbox's 0.4 scale.
    let h = BREAKABLE_BOX_SMALL_HITBOX;
    object.set_hitbox(ObjectHitbox {
        radius: h.radius * object.scale,
        height: h.height * object.scale,
        down_offset: h.down_offset * object.scale,
        hurtbox_radius: h.hurtbox_radius * object.scale,
        hurtbox_height: h.hurtbox_height * object.scale,
        ..h
    });
    object.set_action(0);
    object.set_var("released", 0.0);
    object.visible = true;
}

fn break_small_box(id: ObjId, m: &mut MarioRuntime) {
    m.objects.break_object(id);
    let pos = m.objects[id].pos;
    spawn_moving_yellow_coins(&mut m.objects, pos, 3);
    m.objects[id].deactivate(-1.0);
}

pub fn breakable_box_small_behavior(id: ObjId, m: &mut MarioRuntime) {
    let object = &mut m.objects[id];
    object.timer += 1.0;

    match object.held_state {
        HELD_HELD => return,
        HELD_THROWN => {
            object.held_state = HELD_FREE;
            object.forward_vel = 40.0;
            object.vel[1] = 20.0;
            object.set_action(0);
            object.set_var("released", 1.0);
            object.set_var("releaseFrames", 0.0);
            object.visible = true;
            return;
        }
        HELD_DROPPED => {
            object.held_state = HELD_FREE;
            object.forward_vel = 0.0;
            object.set_action(0);
            object.set_var("released", 1.0);
            object.set_var("releaseFrames", 0.0);
            object.visible = true;
            return;
        }
        _ => {}
    }

    let flags = object.step_physics(&m.world, None);
    object.face_yaw = object.move_yaw;

    if flags & OBJ_COL_FLAG_HIT_WALL != 0 {
        break_small_box(id, m);
    } else if object.var("released") != 0.0 {
        let frames = object.var("releaseFrames") + 1.0;
        object.set_var("releaseFrames", frames);
        if frames > 900.0 {
            object.wait_to_respawn(3000.0);
        } else {
            object.visible = frames <= 810.0 || frames % 2.0 == 0.0;
        }
    }
}
