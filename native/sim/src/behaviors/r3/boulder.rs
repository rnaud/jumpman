//! Hazy Maze Cave's rolling rocks: `bhvBigBoulderGenerator` and `bhvBigBoulder`. Port of
//! `src/game/behaviors/boulder.inc.c` (+ `set_rolling_sphere_hitbox` / `adjust_rolling_face_pitch` of
//! snowman.inc.c and `current_mario_room_check` of obj_behaviors.c).
//!
//! The generator sits at the top of the rock slope of room 4 (hmc/script.c:55). While Mario is in that room
//! and more than 1500 units away it rolls a boulder every 64 frames (within 6000) or every 128 frames
//! (farther); each boulder starts at 40 units/frame on a random yaw in [0, 4096), rolls with
//! `object_step_without_floor_orient`, is capped at 70 units/frame and disappears below y = -1000.
use super::*;
use crate::behaviors::g2::obj::{begin, finish, random_float, F_FACE_YAW};

/// `MODEL_HMC_ROLLING_ROCK` (`hmc_geo_000548`).
pub const ROLLING_ROCK_MODEL: &str = "levels/hmc/hmc_geo_000548.glb";
/// The room `bhv_big_boulder_generator_loop` checks.
pub const BOULDER_ROOM: i32 = 4;

/// snowman.inc.c `sRollingSphereHitbox`.
pub const ROLLING_SPHERE_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE,
    down_offset: 0.0,
    damage_or_coin_value: 3.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 210.0,
    height: 350.0,
    hurtbox_radius: 0.0,
    hurtbox_height: 0.0,
};

pub fn init_generator(o: &mut SimObject) {
    o.visible = false; // MODEL_NONE
}

/// obj_behaviors.c `current_mario_room_check(room)`: the room of Mario's floor; on an object surface (room
/// 0 / none) the room Mario was last seen in (`sPrevCheckMarioRoom`). Returns (result, new previous room).
pub fn current_mario_room_check(m: &MarioRuntime, prev_room: i32, room: i32) -> (bool, i32) {
    let current = m.floor_surface().and_then(|s| if s.object.is_some() { None } else { s.room }).unwrap_or(0);
    if current == 0 {
        (prev_room == room, prev_room)
    } else {
        (current == room, current)
    }
}

/// `bhv_big_boulder_generator_loop`.
pub fn generator(id: ObjId, m: &mut MarioRuntime) {
    begin(m, id);
    let mpos = m.pos;
    let (in_room, prev) = current_mario_room_check(m, m.objects[id].var("prevRoom") as i32, BOULDER_ROOM);
    let o = &mut m.objects[id];
    o.set_var("prevRoom", prev as f64);
    if o.timer >= 256.0 {
        o.timer = 0.0;
    }
    // `!current_mario_room_check(4) || is_point_within_radius_of_mario(.., 1500)` returns early.
    let dist = dist_3d(&o.pos, &mpos);
    let t = o.timer as i64;
    let spawn = in_room && dist >= 1500.0 && if dist < 6000.0 { t & 0x3F == 0 } else { t & 0x7F == 0 };
    if spawn {
        // spawn_object(o, MODEL_HMC_ROLLING_ROCK, bhvBigBoulder); oMoveAngleYaw = random_float() * 4096.0f
        let pos = m.objects[id].pos;
        let mut spec = ObjectSpec::new(K::BigBoulder, pos, 0.0);
        spec.model = Some(ROLLING_ROCK_MODEL.to_string());
        let slot = m.objects.objects.iter().position(|o| !o.active && o.behavior == K::BigBoulder);
        let b = match slot {
            Some(i) => {
                let mut fresh = SimObject::new(&spec);
                init(&mut fresh);
                m.objects.objects[i] = fresh;
                ObjId(i as u32)
            }
            None => m.objects.spawn(&spec),
        };
        let yaw = (random_float() * 4096.0).trunc();
        let o = &mut m.objects[b];
        o.move_yaw = yaw;
        o.face_yaw = yaw;
    }
    finish(m, id, 0);
}

/// `bhv_big_boulder_init` (+ bhvBigBoulder's SET_FLOAT(oGraphYOffset, 180)).
pub fn init(o: &mut SimObject) {
    o.home = o.pos;
    o.gravity = 8.0;
    o.friction = 0.999;
    o.buoyancy = 2.0;
    o.graph_y_offset = 180.0;
    o.visible = true;
    o.action = 0;
    o.prev_action = 0;
    o.hitbox = ROLLING_SPHERE_HITBOX;
}

/// `bhv_big_boulder_loop`.
pub fn boulder(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    begin(m, id);
    let o = &mut m.objects[id];
    o.scale = 1.5; // cur_obj_scale(1.5f)
    o.graph_y_offset = 270.0;
    let mut deactivate = false;
    match o.action {
        0 => {
            o.forward_vel = 40.0;
            o.action = 1;
        }
        _ => {
            // boulder_act_1
            let flags = o.step_physics(&world, None);
            if flags & (OBJ_COL_FLAG_GROUNDED | OBJ_COL_FLAG_NO_Y_VEL) == OBJ_COL_FLAG_GROUNDED && o.vel[1] > 10.0 {
                let pos = o.pos;
                m.objects.sound(pos, "GENERAL_GRINDEL_ROLL");
            }
            let o = &mut m.objects[id];
            if o.forward_vel > 70.0 {
                o.forward_vel = 70.0;
            }
            if o.pos[1] < -1000.0 {
                deactivate = true;
            }
            // adjust_rolling_face_pitch(1.5f)
            let pitch = to_s16(o.var("facePitch") + (o.forward_vel * (100.0 / 1.5)).trunc()) as f64;
            o.set_var("facePitch", pitch);
            let (pos, key) = (o.pos, o.id);
            m.objects.sound_loop(pos, "ENV_UNKNOWN2", key);
        }
    }
    // set_rolling_sphere_hitbox: obj_set_hitbox scales the cylinder by the object scale.
    let o = &mut m.objects[id];
    let s = o.scale;
    o.hitbox = ObjectHitbox { radius: s * ROLLING_SPHERE_HITBOX.radius, height: s * ROLLING_SPHERE_HITBOX.height, ..ROLLING_SPHERE_HITBOX };
    if o.interact_status & INT_STATUS_INTERACTED != 0 {
        o.interact_status = 0;
    }
    if deactivate {
        o.active = false; // ACTIVE_FLAG_DEACTIVATED
        o.visible = false;
        o.respawn_timer = -1.0;
        o.respawn_distance = -1.0;
    }
    finish(m, id, F_FACE_YAW);
}
