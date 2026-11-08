//! Blue coins and the recovery heart.
//!
//! * `bhvBlueCoinSwitch`, `bhvHiddenBlueCoin`: `src/game/behaviors/blue_coin_switch.inc.c`
//!   (`bhv_blue_coin_switch_loop`, `bhv_hidden_blue_coin_loop`). The switch has the collision model
//!   `blue_coin_switch_seg8_collision_08000E98` (registered under `g2/...` by the course loader and
//!   loaded each frame like `load_object_collision_model`).
//! * `bhvMovingBlueCoin`: `moving_coin.inc.c` (`bhv_moving_blue_coin_init` / `_loop`).
//! * `bhvRecoveryHeart`: `recovery_heart.inc.c` (`bhv_recovery_heart_loop`).
use super::*;
use crate::behaviors::g2::dyn_collision::{load_object_collision_model, unload_object_collision};
use crate::behaviors::g2::obj::mario_on_platform;
use crate::state::*;

pub const SWITCH_COLLISION: &str = "g2/blue_coin_switch_seg8_collision_08000E98";

pub const BLUE_COIN_SWITCH_ACT_IDLE: i32 = 0;
pub const BLUE_COIN_SWITCH_ACT_RECEDING: i32 = 1;
pub const BLUE_COIN_SWITCH_ACT_TICKING: i32 = 2;

pub const HIDDEN_BLUE_COIN_ACT_INACTIVE: i32 = 0;
pub const HIDDEN_BLUE_COIN_ACT_WAITING: i32 = 1;
pub const HIDDEN_BLUE_COIN_ACT_ACTIVE: i32 = 2;

/// `sMovingBlueCoinHitbox` (INTERACT_COIN, damageOrCoinValue 5, radius 100, height 64).
const COIN_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0, down_offset: 0.0, damage_or_coin_value: 5.0, health: 0.0, num_loot_coins: 0.0,
    radius: 100.0, height: 64.0, hurtbox_radius: 0.0, hurtbox_height: 0.0,
};

pub fn init_switch(o: &mut SimObject) {
    // cur_obj_scale(3.0): the switch's model is 1/3 size.
    o.scale = 3.0;
    o.visible = true;
    o.action = BLUE_COIN_SWITCH_ACT_IDLE;
    o.prev_action = o.action;
}

/// bhv_blue_coin_switch_loop.
pub fn switch_update(id: ObjId, m: &mut MarioRuntime) {
    match m.objects[id].action {
        BLUE_COIN_SWITCH_ACT_IDLE => {
            // Mario on the switch and just ground-pounded: recede and get ready to tick.
            if mario_on_platform(m, id) && m.action == ACT_GROUND_POUND_LAND {
                let o = &mut m.objects[id];
                o.action = BLUE_COIN_SWITCH_ACT_RECEDING;
                o.vel[1] = -20.0;
                o.gravity = 0.0;
                let pos = o.pos;
                m.objects.sound(pos, "GENERAL_SWITCH_DOOR_OPEN");
            }
            load_object_collision_model(m, id, SWITCH_COLLISION, 1000.0);
        }
        BLUE_COIN_SWITCH_ACT_RECEDING => {
            // Recede for 6 frames before going invisible and ticking (the original's off-by-one).
            if m.objects[id].action_timer > 5.0 {
                unload_object_collision(m, id);
                let mario_y = m.pos[1];
                let o = &mut m.objects[id];
                o.visible = false;
                o.action = BLUE_COIN_SWITCH_ACT_TICKING;
                o.pos[1] = mario_y - 40.0;
            } else {
                load_object_collision_model(m, id, SWITCH_COLLISION, 1000.0);
                let o = &mut m.objects[id];
                o.pos[1] += o.vel[1];
                o.vel[1] -= o.gravity;
            }
        }
        _ => {
            // Tick faster while the coins have not started blinking; delete the switch after the
            // last coin or when the coins unload after 240 frames.
            let t = m.objects[id].action_timer;
            let pos = m.objects[id].pos;
            m.objects.sound(pos, if t < 200.0 { "GENERAL2_SWITCH_TICK_FAST" } else { "GENERAL2_SWITCH_TICK_SLOW" });
            let any_coin = m.objects.objects.iter().any(|c| c.active && c.behavior == K::HiddenBlueCoin);
            if !any_coin || t > 240.0 {
                m.objects[id].deactivate(-1.0);
            }
        }
    }
    end(&mut m.objects[id]);
}

pub fn init_hidden(o: &mut SimObject) {
    o.hitbox = COIN_HITBOX;
    o.billboard = true;
    o.visible = false;
    o.action = HIDDEN_BLUE_COIN_ACT_INACTIVE;
    o.prev_action = o.action;
}

/// bhv_hidden_blue_coin_loop.
pub fn hidden_update(id: ObjId, m: &mut MarioRuntime) {
    match m.objects[id].action {
        HIDDEN_BLUE_COIN_ACT_INACTIVE => {
            // Invisible and intangible until the blue coin switch is found.
            m.objects[id].visible = false;
            if let Some(sw) = nearest_of(m, id, K::BlueCoinSwitch) {
                m.objects[id].set_var("switch", sw.0 as f64);
                m.objects[id].action = HIDDEN_BLUE_COIN_ACT_WAITING;
            }
        }
        HIDDEN_BLUE_COIN_ACT_WAITING => {
            let sw = ObjId(m.objects[id].var("switch") as u32);
            if m.objects[sw].action == BLUE_COIN_SWITCH_ACT_TICKING {
                m.objects[id].action = HIDDEN_BLUE_COIN_ACT_ACTIVE;
            }
        }
        _ => {
            m.objects[id].visible = true;
            if m.action & ACT_FLAG_INTANGIBLE == 0 && mario_overlaps_object(m, &m.objects[id]) {
                // INTERACT_COIN: the coin's value (5) and a heal of 4 per coin (CoinCollected).
                let pos = m.objects[id].pos;
                m.objects.value_coin_collected(pos, 5);
                m.objects[id].deactivate(-1.0);
                return;
            }
            // cur_obj_wait_then_blink(200, 20): blink every other frame, then delete.
            let o = &mut m.objects[id];
            let t = o.action_timer;
            if t >= 200.0 {
                let blinking = t - 200.0;
                if blinking % 2.0 != 0.0 {
                    o.visible = false;
                    if blinking / 2.0 > 20.0 {
                        o.deactivate(-1.0);
                        return;
                    }
                } else {
                    o.visible = true;
                }
            }
        }
    }
    end(&mut m.objects[id]);
}

pub fn init_moving(o: &mut SimObject) {
    // bhv_moving_blue_coin_init
    o.gravity = 5.0;
    o.friction = 1.0;
    o.buoyancy = 1.5;
    o.hitbox = COIN_HITBOX;
    o.billboard = true;
    o.visible = true;
    o.action = 0;
    o.prev_action = 0;
}

/// bhv_moving_blue_coin_loop (MOV_BCOIN_ACT_STILL / MOVING).
pub fn moving_update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    if m.objects[id].action == 0 {
        if near_mario(m, &m.objects[id].pos, 1500.0) {
            m.objects[id].action = 1;
        }
    } else {
        let flags = m.objects[id].step_physics(&world, None);
        let o = &mut m.objects[id];
        if flags & OBJ_COL_FLAG_GROUNDED != 0 {
            o.forward_vel += 25.0;
            if flags & OBJ_COL_FLAG_NO_Y_VEL == 0 {
                let pos = o.pos;
                m.objects.coin_bounce(pos);
            }
        } else {
            o.forward_vel *= 0.98;
        }
        let o = &mut m.objects[id];
        if o.forward_vel > 75.0 {
            o.forward_vel = 75.0;
        }
        if flicker_and_disappear(o, 600.0) {
            return;
        }
    }
    if m.action & ACT_FLAG_INTANGIBLE == 0 && mario_overlaps_object(m, &m.objects[id]) {
        let pos = m.objects[id].pos;
        m.objects.value_coin_collected(pos, 5);
        m.objects[id].deactivate(-1.0);
        return;
    }
    end(&mut m.objects[id]);
}

const HEART_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0,
    radius: 50.0, height: 50.0, hurtbox_radius: 50.0, hurtbox_height: 50.0,
};

pub fn init_heart(o: &mut SimObject) {
    o.hitbox = HEART_HITBOX;
    o.visible = true;
}

/// bhv_recovery_heart_loop: spins faster while Mario touches it and heals 4 per full turn.
pub fn heart_update(id: ObjId, m: &mut MarioRuntime) {
    let touching = mario_overlaps_object(m, &m.objects[id]);
    let forward_vel = m.forward_vel;
    let o = &mut m.objects[id];
    let mut vel = o.var("angleVelYaw");
    if touching {
        if o.var("playedSound") == 0.0 {
            o.set_var("playedSound", 1.0);
            let pos = o.pos;
            m.objects.sound(pos, "GENERAL_HEART_SPIN");
        }
        let o = &mut m.objects[id];
        vel = (200.0 * forward_vel).trunc() + 1000.0;
        o.set_var("angleVelYaw", vel);
    } else {
        o.set_var("playedSound", 0.0);
        vel -= 50.0;
        if vel < 400.0 {
            vel = 400.0;
            o.set_var("totalSpin", 0.0);
        }
        o.set_var("angleVelYaw", vel);
    }
    let o = &mut m.objects[id];
    let total = o.var("totalSpin") + vel;
    if total >= 65536.0 {
        o.set_var("totalSpin", total - 65536.0);
        m.heal_counter += 4.0;
    } else {
        o.set_var("totalSpin", total);
    }
    let o = &mut m.objects[id];
    o.face_yaw = to_s16(o.face_yaw + vel) as f64;
}
