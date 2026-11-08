//! Swoop (`bhvSwoop`), the Hazy Maze Cave bat. Port of `src/game/behaviors/swoop.inc.c`.
//! Hangs upside down until Mario is within 1500, turns to him, dives and chases with a swerving
//! flight path; gives up and flies home when Mario is more than 2000 away.
use super::anim::{self, spec, AnimSpec};
use super::*;

pub const ACT_IDLE: i32 = 0;
pub const ACT_MOVE: i32 = 1;
/// `swoop_seg6_anims_060070D0`: 0 = flap (8 frames), 1 = hanging idle (50 frames).
pub const ANIMS: [AnimSpec; 2] = [spec(0, 0, 0x08, false), spec(0, 0, 0x32, false)];
pub const ANIM_NAMES: [&str; 2] = ["swoop_seg6_anim_06006E88", "swoop_seg6_anim_060070B8"];

/// sSwoopHitbox.
pub const HITBOX: ObjectHitbox = hitbox(INTERACT_HIT_FROM_BELOW, 0.0, 1.0, 0.0, 1.0, 100.0, 80.0, 70.0, 70.0);

pub fn init(o: &mut SimObject) {
    o.hitbox = HITBOX;
    o.set_var("realInteract", HITBOX.interact_type as f64);
    o.scale = 0.0;
    o.gravity = 0.0;
    o.friction = 0.0;
    o.visible = true;
    o.action = ACT_IDLE;
    o.prev_action = ACT_IDLE;
    anim::init(o, 1, ANIMS[1]);
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let o = &mut m.objects[id];
    let dist = o.distance_to_mario;
    let angle_to_mario = o.angle_to_mario;
    let tick = o.timer;
    match o.action {
        // swoop_act_idle
        ACT_IDLE => {
            anim::init(o, 1, ANIMS[1]);
            let mut scale = o.scale;
            let grown = approach_f32_ptr(&mut scale, 1.0, 0.05);
            o.scale = scale;
            if grown && dist < 1500.0 && rotate_yaw_toward(o, angle_to_mario, 800.0) {
                let pos = o.pos;
                o.action = ACT_MOVE;
                o.vel[1] = -12.0;
                m.objects.sound(pos, "OBJ2_SWOOP");
            }
            m.objects[id].set_var("faceRoll", 0x8000 as f64);
        }
        // swoop_act_move
        _ => {
            anim::init_accel(o, 0, ANIMS[0], 2.0);
            let far = dist > 2000.0 && dist_3d(&o.home, &mpos) > 2000.0;
            if o.forward_vel == 0.0 {
                // obj_face_roll_approach(0, 2500)
                let roll = approach_s16(o.var("faceRoll"), 0.0, 2500.0);
                o.set_var("faceRoll", roll);
                if to_s16(roll) == 0 {
                    o.forward_vel = 10.0;
                    o.vel[1] = -10.0;
                }
            } else if far {
                o.action = ACT_IDLE;
                o.pos = o.home;
                o.scale = 0.0;
                o.forward_vel = 0.0;
                o.vel[1] = 0.0;
                o.set_var("faceRoll", 0.0);
            } else {
                if o.var("bonk") != 0.0 {
                    o.set_var("bonk", o.var("bonk") - 1.0);
                } else if o.vel[1] != 0.0 {
                    o.set_var("targetYaw", angle_to_mario);
                    if o.pos[1] < mpos[1] + 200.0 {
                        if approach_f32_ptr(&mut o.vel[1], 0.0, 0.5) { o.forward_vel *= 2.0; }
                    } else {
                        approach_f32_ptr(&mut o.vel[1], -10.0, 0.5);
                    }
                } else if o.hit_wall {
                    // cur_obj_reflect_move_angle_off_wall: the wall angle is not tracked, bounce straight back.
                    o.set_var("targetYaw", to_s16(o.move_yaw + 0x8000 as f64) as f64);
                    o.set_var("bonk", 30.0);
                }
                // obj_get_pitch_from_vel
                let mut target_pitch = -(atan2s(o.forward_vel, o.vel[1]) as f64);
                if to_s16(target_pitch) == 0 { target_pitch += o.forward_vel * 500.0; }
                let pitch = approach_s16(o.var("movePitch"), target_pitch, 140.0);
                o.set_var("movePitch", pitch);
                let target_yaw = o.var("targetYaw");
                rotate_yaw_toward(o, target_yaw + (3000.0 * coss(4000.0 * tick)).trunc(), 1200.0);
                // obj_roll_to_match_yaw_turn(targetYaw, 0x3000, 500)
                let target_roll = to_s16(o.move_yaw - target_yaw).clamp(-0x3000, 0x3000) as f64;
                let roll = approach_s16(o.var("faceRoll"), target_roll, 500.0);
                o.set_var("faceRoll", roll + (1000.0 * coss(20000.0 * tick)).trunc());
            }
        }
    }
    // cur_obj_move_standard(78) with gravity 0 and a 50 unit wall hitbox.
    let o = &mut m.objects[id];
    o.step_physics(&world, Some(50.0));
    o.face_yaw = o.move_yaw;
    // The pitch is not applied to the model (OBJ_FLAG_SET_FACE_YAW_TO_MOVE_YAW only copies the yaw).
    let action = o.action;
    check_attacks(id, m, action, "OBJ_SWOOP_DEATH");
    end_frame(&mut m.objects[id]);
}
