//! Dorrie (HMC lake, "Swimming Beast in the Cavern"): `bhvDorrie`. Port of
//! `src/game/behaviors/dorrie.inc.c` (`bhv_dorrie_update`, `dorrie_act_*`).
//!
//! She swims in circles around her home, follows Mario's facing while he stands on her back, lowers her
//! head when he ground-pounds her (making a ramp to the shore) and lifts him up when he stands on the
//! lowered head. The NPC dialog states of `set_mario_npc_dialog` use the sim's `ACT_READING_NPC_DIALOG`
//! (start = the action begins, speak = `action_state == 8`, stop = `action_state = 9`).
//! Left out: sounds.
use super::helpers::*;
use crate::behaviors::g1::anim::{self, AnimSpec};
use crate::behaviors::g2::dyn_collision::{load_object_collision_model, run_platform};
use crate::behaviors::g2::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

pub const ACT_MOVE: i32 = 0;
pub const ACT_LOWER_HEAD: i32 = 1;
pub const ACT_RAISE_HEAD: i32 = 2;

/// `dorrie_seg6_anims_0600F638`.
pub const ANIMS: [AnimSpec; 3] = [anim_spec(false, 0, 0, 60), anim_spec(false, 0, 0, 60), anim_spec(false, 0, 0, 80)];
pub const ANIM_NAMES: [&str; 3] = ["dorrie_seg6_anim_0600E18C", "dorrie_seg6_anim_0600E9BC", "dorrie_seg6_anim_0600F620"];
const COL_RAISED: &str = "g2/dorrie_seg6_collision_0600F644";
const COL_LOWERED: &str = "g2/dorrie_seg6_collision_0600FBB8";

pub fn init(o: &mut SimObject) {
    o.visible = true;
    anim::init(o, 0, ANIMS[0]);
    o.set_var("neck", -0x26F4 as f64);
}

fn raise_head(m: &mut MarioRuntime, id: ObjId) {
    let o = &mut m.objects[id];
    let start = o.var("neck");
    let neck = to_s16(start - (370.0 * sins(o.var("headRaiseSpeed"))).abs().trunc()) as f64;
    o.set_var("neck", neck);
    let xz = 440.0 * (coss(neck) - coss(start));
    let y = 440.0 * (sins(neck) - sins(start));
    let yaw = o.move_yaw;
    // set_mario_pos
    m.state.pos[0] += xz * sins(yaw);
    m.state.pos[1] -= y;
    m.state.pos[2] += xz * coss(yaw);
}

fn act_move(m: &mut MarioRuntime, id: ObjId, riding: bool) {
    let mario_face = m.face_angle[1];
    let o = &mut m.objects[id];
    let start_yaw = o.move_yaw;
    o.set_var("neck", -0x26F4 as f64);
    init_anim(o, &ANIMS, 1);
    if o.var("fwdDist") < 320.0 && o.var("pounded") != 0.0 {
        o.set_var("col", 2.0);
        o.action = ACT_LOWER_HEAD;
        o.forward_vel = 0.0;
        o.set_var("yawVel", 0.0);
    } else {
        let (target_yaw, target_speed);
        if riding {
            target_yaw = mario_face;
            target_speed = 10.0;
        } else {
            let mut turn = 0x4000 as f64 - atan2s(2000.0, o.var("distToHome") - 2000.0) as f64;
            if to_s16(o.move_yaw - o.var("angleToHome")) < 0 {
                turn = -turn;
            }
            target_yaw = o.var("angleToHome") + turn;
            target_speed = 5.0;
        }
        forward_vel_approach(o, target_speed, 0.5);
        let want = (to_s16(target_yaw - o.move_yaw) as f64 / 50.0).trunc();
        let vel = approach_s16_symmetric(o.var("yawVel"), want, 5.0);
        o.set_var("yawVel", vel);
        o.move_yaw = to_s16(o.move_yaw + vel) as f64;
    }
    let d = to_s16(o.move_yaw - start_yaw) as f64;
    o.set_var("avYaw", d);
}

fn begin_head_raise(o: &mut SimObject, lifting: bool) {
    o.set_var("lifting", lifting as i32 as f64);
    o.action = ACT_RAISE_HEAD;
    o.set_var("headRaiseSpeed", 0.0);
}

fn act_lower_head(m: &mut MarioRuntime, id: ObjId, riding: bool) {
    let ready = mario_ready_to_speak(m);
    let o = &mut m.objects[id];
    if init_anim_check_frame(o, &ANIMS, 2, 35) {
        reverse_animation(o);
        let mut start_lift = false;
        if riding {
            if o.var("offsetY") == -17.0 && o.var("fwdDist") > 780.0 && ready {
                start_lift = true;
            } else if o.var("fwdDist") > 320.0 {
                o.timer = 0.0;
            }
        } else if o.timer > 150.0 {
            begin_head_raise(o, false);
        }
        if start_lift {
            // set_mario_npc_dialog(MARIO_DIALOG_LOOK_UP) == START
            begin_head_raise(o, true);
            m.used_obj = Some(id);
            m.set_action(ACT_READING_NPC_DIALOG, 2.0);
        }
    } else {
        o.set_var("neck", o.var("neck") + 0x115 as f64);
    }
}

fn act_raise_head(m: &mut MarioRuntime, id: ObjId) {
    let speaking = m.action == ACT_READING_NPC_DIALOG && m.used_obj == Some(id) && m.action_state == 8.0;
    let o = &mut m.objects[id];
    o.set_var("col", 1.0);
    if anim::near_end(o) {
        o.action = ACT_MOVE;
    } else if o.var("lifting") != 0.0 && o.var("animFrame") < 74.0 {
        if speaking {
            o.set_var("headRaiseSpeed", o.var("headRaiseSpeed") + 0x1CC as f64);
            let at73 = anim::check_frame(o, 73);
            if at73 {
                m.action_state = 9.0; // set_mario_npc_dialog(MARIO_DIALOG_STOP)
            }
            raise_head(m, id);
        } else {
            reverse_animation(o);
        }
    }
}

/// `bhv_dorrie_update`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    if first_frame(&mut m.objects[id]) {
        // ADD_FLOAT(oPosX, 2000) after SET_HOME
        m.objects[id].pos[0] += 2000.0;
        m.objects[id].set_var("col", 1.0);
        m.objects[id].set_var("grav", 0.0);
    }
    run_platform(m, id, F_FACE_YAW, |m, id, riding| {
        let (d, a) = (m.objects[id].distance_to_mario, m.objects[id].angle_to_mario);
        {
            let o = &mut m.objects[id];
            o.set_var("fwdDist", d * coss(a - o.move_yaw));
            cur_obj_move_using_fvel_and_gravity(o, 0.0);
            o.set_var("angleToHome", angle_to_home(o));
            o.set_var("distToHome", lateral_dist_to_home(o));
            let shift = 440.0 * coss(o.var("neck")) * coss(o.move_yaw - o.var("angleToHome"));
            let dist = o.var("distToHome");
            let clamped = clamp_f(dist, 1650.0 + shift, 2300.0 + shift);
            if clamped != dist {
                o.set_var("distToHome", clamped);
                let ath = o.var("angleToHome");
                o.pos[0] = o.home[0] - clamped * sins(ath);
                o.pos[2] = o.home[2] - clamped * coss(ath);
            }
        }
        let pounded = riding && m.action == ACT_GROUND_POUND_LAND;
        let o = &mut m.objects[id];
        o.set_var("pounded", pounded as i32 as f64);
        let max_offset;
        if riding {
            max_offset = -17.0;
            if o.var("offsetY") >= 0.0 {
                o.set_var("velY", if pounded { -15.0 } else { -6.0 });
            }
        } else {
            max_offset = 0.0;
        }
        o.set_var("offsetY", o.var("offsetY") + o.var("velY"));
        let mut vy = o.var("velY");
        approach_f32_ptr(&mut vy, 3.0, 1.0);
        o.set_var("velY", vy);
        if vy > 0.0 && o.var("offsetY") > max_offset {
            o.set_var("offsetY", max_offset);
        }
        o.pos[1] = o.home[1] + o.var("offsetY");
        match o.action {
            ACT_MOVE => act_move(m, id, riding),
            ACT_LOWER_HEAD => act_lower_head(m, id, riding),
            ACT_RAISE_HEAD => act_raise_head(m, id),
            _ => {}
        }
        let col = if m.objects[id].var("col") == 2.0 { COL_LOWERED } else { COL_RAISED };
        load_object_collision_model(m, id, col, 30000.0);
        advance_anim(&mut m.objects[id]);
    });
}

fn lateral_dist_to_home(o: &SimObject) -> f64 {
    let (dx, dz) = (o.home[0] - o.pos[0], o.home[2] - o.pos[2]);
    (dx * dx + dz * dz).sqrt()
}
