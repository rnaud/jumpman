//! Port of `src/sim/behaviors/bobomb.ts` (bobomb.inc.c).
//!
//! The generic Bob-omb patrols, chases Mario, is picked up and thrown, lights its fuse while
//! held, and explodes after landing. The stationary variant never patrols.
use super::moving_yellow_coin::spawn_moving_yellow_coins;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;
use crate::object_manager;

pub const BOBOMB_ACT_PATROL: i32 = 0;
pub const BOBOMB_ACT_LAUNCHED: i32 = 1;
pub const BOBOMB_ACT_CHASE_MARIO: i32 = 2;
pub const BOBOMB_ACT_EXPLODE: i32 = 3;

pub const BOBOMB_BP_STYPE_GENERIC: i32 = 0;
pub const BOBOMB_BP_STYPE_STATIONARY: i32 = 1;

const BOBOMB_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_GRABBABLE,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 65.0,
    height: 113.0,
    hurtbox_radius: 0.0,
    hurtbox_height: 0.0,
};

pub fn init_bobomb(object: &mut SimObject) {
    object.gravity = 2.5;
    object.friction = 0.8;
    object.buoyancy = 1.3;
    object.interact_subtype = INT_SUBTYPE_KICKABLE;
    object.set_hitbox(BOBOMB_HITBOX);
    object.set_action(BOBOMB_ACT_PATROL);
    object.set_var("fuseLit", 0.0);
    object.set_var("fuseTimer", 0.0);
    object.set_var("blinkTimer", 0.0);
}

fn check_interactions(id: ObjId, m: &mut MarioRuntime) {
    let face = m.face_angle[1];
    let object = &mut m.objects[id];
    object.set_hitbox(BOBOMB_HITBOX);

    if object.interact_status & INT_STATUS_MARIO_KNOCKBACK_DMG != 0 {
        object.move_yaw = face;
        object.forward_vel = 25.0;
        object.vel[1] = 30.0;
        object.set_action(BOBOMB_ACT_LAUNCHED);
    }

    if object.interact_status & INT_STATUS_TOUCHED_BOB_OMB != 0 {
        object.set_action(BOBOMB_ACT_EXPLODE);
    }

    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.forward_vel = 20.0;
        object.vel[1] = 20.0;
        object.set_action(BOBOMB_ACT_LAUNCHED);
    }

    object.interact_status = 0;
}

fn explode(id: ObjId, m: &mut MarioRuntime) {
    let object = &mut m.objects[id];
    object.action_timer += 1.0;
    if object.action_timer < 5.0 {
        object.scale = 1.0 + object.action_timer / 5.0;
        return;
    }

    object.scale = 1.0;
    object_manager::explode(m, id, 240.0, 1.0);
    if !m.objects[id].coin_dropped {
        let pos = m.objects[id].pos;
        spawn_moving_yellow_coins(&mut m.objects, pos, 1);
        m.objects[id].coin_dropped = true;
    }
    m.objects[id].wait_to_respawn(3000.0);
}

fn free_loop(id: ObjId, m: &mut MarioRuntime) {
    match m.objects[id].action {
        BOBOMB_ACT_PATROL => {
            let (mflags, mpos) = (m.flags, m.pos);
            let object = &mut m.objects[id];
            object.forward_vel = 5.0;
            object.step_physics(&m.world, None);
            if return_home_if_safe_at(object, mflags, mpos, 400.0)
                && object.facing_toward_angle(object.move_yaw, object.angle_to_mario, 0x2000 as f64)
            {
                object.set_var("fuseLit", 1.0);
                object.set_action(BOBOMB_ACT_CHASE_MARIO);
            }
        }

        BOBOMB_ACT_LAUNCHED => {
            let object = &mut m.objects[id];
            object.step_physics(&m.world, None);
            if object.grounded { object.set_action(BOBOMB_ACT_EXPLODE); }
        }

        BOBOMB_ACT_CHASE_MARIO => {
            let object = &mut m.objects[id];
            object.forward_vel = 20.0;
            object.step_physics(&m.world, None);
            object.turn_toward_angle(object.angle_to_mario, 0x800 as f64);
        }

        BOBOMB_ACT_EXPLODE => explode(id, m),

        _ => m.objects[id].set_action(BOBOMB_ACT_PATROL),
    }

    check_interactions(id, m);
}

fn held_loop(id: ObjId, m: &mut MarioRuntime) {
    let object = &mut m.objects[id];
    object.set_var("fuseLit", 1.0);
    object.visible = true;
    if object.var("fuseTimer") > 150.0 {
        object.set_action(BOBOMB_ACT_EXPLODE);
        m.drop_held_object();
    }
}

fn thrown_loop(object: &mut SimObject) {
    object.held_state = HELD_FREE;
    object.forward_vel = 25.0;
    object.vel[1] = 20.0;
    object.set_action(BOBOMB_ACT_LAUNCHED);
}

fn dropped_loop(object: &mut SimObject) {
    object.held_state = HELD_FREE;
    object.forward_vel = 0.0;
    object.set_action(BOBOMB_ACT_PATROL);
}

pub fn bobomb_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].distance_to_mario < 4000.0 {
        match m.objects[id].held_state {
            HELD_HELD => held_loop(id, m),
            HELD_THROWN => thrown_loop(&mut m.objects[id]),
            HELD_DROPPED => {
                if m.objects[id].action == BOBOMB_ACT_EXPLODE { explode(id, m); } else { dropped_loop(&mut m.objects[id]); }
            }
            _ => {
                let o = &m.objects[id];
                if !o.coin_dropped && (o.param as i64 & 0xff) as i32 == BOBOMB_BP_STYPE_STATIONARY {
                    match o.action {
                        BOBOMB_ACT_EXPLODE => explode(id, m),
                        BOBOMB_ACT_LAUNCHED => {
                            let object = &mut m.objects[id];
                            object.step_physics(&m.world, None);
                            if object.grounded { object.set_action(BOBOMB_ACT_EXPLODE); }
                            check_interactions(id, m);
                        }
                        _ => check_interactions(id, m),
                    }
                } else {
                    free_loop(id, m);
                }
                let object = &mut m.objects[id];
                if object.var("fuseTimer") > 150.0 && object.action != BOBOMB_ACT_EXPLODE { object.set_action(BOBOMB_ACT_EXPLODE); }
            }
        }
        let object = &mut m.objects[id];
        if object.var("fuseLit") != 0.0 {
            let t = object.var("fuseTimer") + 1.0;
            object.set_var("fuseTimer", t);
        }
    }

    let object = &mut m.objects[id];
    object.face_yaw = object.move_yaw;
}
