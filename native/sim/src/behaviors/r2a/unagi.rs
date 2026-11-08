//! Unagi, the eel of Jolly Roger Bay ("Can the Eel Come Out to Play?"): `bhvUnagi` and its eight
//! `bhvUnagiSubobject` body parts. Port of `src/game/behaviors/unagi.inc.c`, `cur_obj_follow_path` and
//! `cur_obj_set_pos_via_transform` (`object_helpers.c`).
//!
//! `BPARAM2` 0 / 2 are the wandering eels (path `jrb_seg7_trajectory_unagi_1`); 1 is the one in the
//! sunken-ship corner of act 2: it waits at its home, lunges out of the hole at Mario, and swims off along
//! `jrb_seg7_trajectory_unagi_2`. Touching its tail tip (part -4) while it carries the star
//! (`oAnimState`) gives `spawn_default_star(6833, -3654, 2230)`.
//!
//! Left out: sounds, the eel's drawing distance.
use super::helpers::*;
use crate::behaviors::g1::anim::{self, AnimSpec};
use crate::behaviors::g2::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use ObjectBehaviorKind as K;

/// `unagi_seg5_anims_05012824`.
pub const ANIMS: [AnimSpec; 7] = [
    anim_spec(false, 0, 0, 140),
    anim_spec(false, 0, 0, 34),
    anim_spec(false, 0, 0, 54),
    anim_spec(true, 0, 0, 76),
    anim_spec(true, 0, 0, 30),
    anim_spec(false, 0, 0, 24),
    anim_spec(false, 0, 0, 70),
];
pub const ANIM_NAMES: [&str; 7] = [
    "unagi_seg5_anim_0500FBF4",
    "unagi_seg5_anim_05010488",
    "unagi_seg5_anim_05010B54",
    "unagi_seg5_anim_05011488",
    "unagi_seg5_anim_05011890",
    "unagi_seg5_anim_05011D40",
    "unagi_seg5_anim_0501280C",
];

/// `jrb_seg7_trajectory_unagi_1` (levels/jrb/areas/1/trajectory.inc.c).
pub const TRAJECTORY_1: [[f64; 3]; 6] = [
    [5300.0, -3800.0, 1200.0],
    [3700.0, -3600.0, 1700.0],
    [3400.0, -3400.0, 3500.0],
    [3900.0, -3600.0, 4400.0],
    [5300.0, -3800.0, 4400.0],
    [6200.0, -4000.0, 2700.0],
];
/// `jrb_seg7_trajectory_unagi_2`.
pub const TRAJECTORY_2: [[f64; 3]; 9] = [
    [5381.0, 0.0, 2758.0],
    [5803.0, -3130.0, 3036.0],
    [4876.0, -3045.0, 2706.0],
    [4313.0, -3246.0, 2736.0],
    [3792.0, -3413.0, 3668.0],
    [4971.0, -3130.0, 3688.0],
    [5392.0, -3130.0, 3326.0],
    [6341.0, -3130.0, 2450.0],
    [9431.0, -3130.0, 1400.0],
];

/// sUnagiHitbox (`INTERACT_CLAM_OR_BUBBA`, damage 3).
pub const INTERACT_CLAM_OR_BUBBA: u32 = super::interact::INTERACT_CLAM_OR_BUBBA;
pub const PART_HITBOX: ObjectHitbox = hitbox(INTERACT_CLAM_OR_BUBBA, 50.0, 3.0, 99.0, 0.0, 150.0, 150.0, 150.0, 150.0);

const WAYPOINT_INITIALIZED: f64 = 0x8000 as f64;
const PATH_NONE: i32 = 0;
const PATH_REACHED_END: i32 = -1;

fn trajectory(o: &SimObject) -> &'static [[f64; 3]] {
    if o.var("pathId") == 2.0 { &TRAJECTORY_2 } else { &TRAJECTORY_1 }
}

pub fn init(o: &mut SimObject) {
    o.visible = true;
    o.scale = 3.0;
    anim::init(o, 6, ANIMS[6]);
    // bhv_unagi_init
    let kind = ((o.param as u32) >> 16 & 0xFF) as i32;
    if kind != 1 {
        o.set_var("pathId", 1.0);
        if kind == 0 {
            o.set_var("facePitch", -7600.0);
        } else {
            o.action = 1;
        }
    } else {
        o.set_var("pathId", 2.0);
        o.action = 3;
        o.set_var("animState", 1.0);
        o.set_var("u1B0", o.move_yaw);
    }
    o.set_var("prevWaypoint", 0.0);
}

/// `cur_obj_follow_path`.
fn follow_path(o: &mut SimObject) -> i32 {
    let path = trajectory(o);
    if o.var("pathFlags") == 0.0 {
        o.set_var("prevWaypoint", 0.0);
        o.set_var("pathFlags", WAYPOINT_INITIALIZED);
    }
    let last = o.var("prevWaypoint") as usize;
    let target = if last + 1 < path.len() { last + 1 } else { 0 };
    o.set_var("pathFlags", last as f64 + WAYPOINT_INITIALIZED);
    let (l, t) = (path[last], path[target]);
    let prev_to_next = [t[0] - l[0], t[1] - l[1], t[2] - l[2]];
    let obj_to_next = [t[0] - o.pos[0], t[1] - o.pos[1], t[2] - o.pos[2]];
    let xz = (obj_to_next[0] * obj_to_next[0] + obj_to_next[2] * obj_to_next[2]).sqrt();
    o.set_var("targetYaw", atan2s(obj_to_next[2], obj_to_next[0]) as f64);
    o.set_var("targetPitch", atan2s(xz, -obj_to_next[1]) as f64);
    if prev_to_next[0] * obj_to_next[0] + prev_to_next[1] * obj_to_next[1] + prev_to_next[2] * obj_to_next[2] <= 0.0 {
        o.set_var("prevWaypoint", target as f64);
        if target + 1 >= path.len() {
            return PATH_REACHED_END;
        }
        return 1;
    }
    PATH_NONE
}

fn act_0(o: &mut SimObject) {
    if o.distance_to_mario > 4500.0 && o.action_state != 0.0 {
        o.action = 1;
        let p = trajectory(o)[0];
        o.pos = p;
    } else if o.var("u1AC") < 700.0 {
        o.action_state = 1.0;
    }
}

/// `unagi_act_1_4(arg0)`.
fn act_1_4(o: &mut SimObject, arg0: i32) {
    if o.var("animation") == 3.0 {
        if anim::check_frame(o, 30) {
            o.forward_vel = 40.0;
        }
    } else if anim::at_end(o) {
        if o.action != arg0 && (o.var("pathFlags") as u32 & 0xFF) >= 7 {
            init_anim(o, &ANIMS, 3);
        } else {
            init_anim(o, &ANIMS, 2);
        }
    }
    if follow_path(o) == PATH_REACHED_END {
        o.action = arg0;
    }
    let pitch = approach_s16_symmetric(o.var("movePitch"), o.var("targetPitch"), 50.0);
    o.set_var("movePitch", pitch);
    o.set_var("facePitch", pitch);
    let ty = o.var("targetYaw");
    rotate_yaw_toward(o, ty, 120.0);
    // obj_roll_to_match_yaw_turn(targetYaw, 0x2000, 100)
    let target_roll = clamp_f(to_s16(o.move_yaw - ty) as f64, -0x2000 as f64, 0x2000 as f64);
    let roll = approach_s16_symmetric(o.var("faceRoll"), target_roll, 100.0);
    o.set_var("faceRoll", roll);
    forward_vel_approach(o, 10.0, 0.2);
    // cur_obj_set_pos_via_transform: forward along (cos pitch sin yaw, -sin pitch, cos pitch cos yaw)
    let (sx, cx) = (sins(pitch), coss(pitch));
    o.pos[0] += cx * sins(o.move_yaw) * o.forward_vel;
    o.pos[1] += -sx * o.forward_vel;
    o.pos[2] += cx * coss(o.move_yaw) * o.forward_vel;
}

fn act_2(o: &mut SimObject) {
    o.set_var("prevWaypoint", 0.0);
    o.set_var("pathFlags", 0.0);
    o.pos = o.home;
    o.set_var("movePitch", 0.0);
    o.set_var("facePitch", 0.0);
    o.move_yaw = o.var("u1B0");
    o.face_yaw = o.move_yaw;
    o.set_var("faceRoll", 0.0);
    o.forward_vel = 0.0;
    o.vel[0] = 0.0;
    o.vel[2] = 0.0;
    o.set_var("uF8", 0.0);
    o.set_var("uF4", -800.0);
    o.action = 3;
}

fn act_3(o: &mut SimObject) {
    if o.var("uF4") < 0.0 {
        init_anim(o, &ANIMS, 6);
        let v = o.var("uF4") + 10.0;
        o.set_var("uF4", if v > 0.0 { 0.0 } else { v });
    } else {
        if o.var("uF4") == 0.0 {
            init_anim(o, &ANIMS, 6);
            if o.timer > 60.0 && o.var("u1AC") < 1000.0 {
                o.set_var("uF8", 30.0);
                o.set_var("u110", 30.0);
            } else {
                o.set_var("u110", 0.0);
            }
        } else if o.var("u110") > 0.0 {
            if init_anim_check_end(o, &ANIMS, 5) {
                o.set_var("u110", 0.0);
            }
        } else if o.var("u110") == 0.0 {
            init_anim(o, &ANIMS, 0);
            if anim::at_end(o) {
                if o.var("u1AC") < 1000.0 {
                    o.action = 4;
                    o.forward_vel = o.var("uF8");
                    init_anim(o, &ANIMS, 1);
                } else {
                    o.set_var("u110", -50.0);
                    init_anim(o, &ANIMS, 4);
                }
            }
        }
        let mut f8 = o.var("uF8");
        approach_f32_ptr(&mut f8, o.var("u110"), 4.0);
        o.set_var("uF8", f8);
        let f4 = o.var("uF4") + f8;
        o.set_var("uF4", f4);
        if f4 < 0.0 {
            o.set_var("uF4", 0.0);
            o.set_var("uF8", 0.0);
            o.timer = 0.0;
        }
    }
    o.pos[0] = o.home[0] + o.var("uF4") * sins(o.move_yaw);
    o.pos[2] = o.home[2] + o.var("uF4") * coss(o.move_yaw);
}

/// `bhv_unagi_loop`.
pub fn unagi(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        // spawn / drop the body parts
        let (flag, dist) = (m.objects[id].var("partsOut"), m.objects[id].distance_to_mario);
        if flag == 0.0 {
            m.objects[id].set_var("u1AC", 99999.0);
            if dist < 3000.0 {
                for i in -4..4 {
                    let p = spawn_child(m, id, K::UnagiPart, None, 0.0);
                    m.objects[p].set_var("part", i as f64);
                }
                m.objects[id].set_var("partsOut", 1.0);
            }
        } else if dist > 4000.0 {
            m.objects[id].set_var("partsOut", 0.0);
        }
        let o = &mut m.objects[id];
        match o.action {
            0 => act_0(o),
            1 => act_1_4(o, 1),
            2 => {
                act_2(o);
                act_3(o);
            }
            3 => act_3(o),
            4 => act_1_4(o, 2),
            _ => {}
        }
        advance_anim(o);
    });
}

pub fn init_part(o: &mut SimObject) {
    o.visible = false;
    o.hitbox = PART_HITBOX;
    o.set_var("hbInit", 1.0);
    o.set_var("realInteract", INTERACT_CLAM_OR_BUBBA as f64);
}

/// `bhv_unagi_subobject_loop`.
pub fn part(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let Some(parent) = m.objects[id].link else { return };
        if m.objects[parent].var("partsOut") == 0.0 || !m.objects[parent].active {
            mark_for_deletion(&mut m.objects[id]);
            return;
        }
        let (ppos, pyaw, ppitch) = {
            let p = &m.objects[parent];
            (p.pos, p.face_yaw, p.var("facePitch"))
        };
        let index = m.objects[id].var("part");
        let mut len = 300.0 * index;
        let o = &mut m.objects[id];
        o.pos[1] = ppos[1] - len * sins(ppitch) * 1.13;
        len *= coss((ppitch / 2.0).trunc());
        o.pos[0] = ppos[0] + len * sins(pyaw);
        o.pos[2] = ppos[2] + len * coss(pyaw);
        if index == -4.0 {
            let carrying = m.objects[parent].var("animState") != 0.0;
            if carrying && m.objects[id].distance_to_mario < 150.0 {
                m.objects[parent].set_var("animState", 0.0);
                let act_param = m.objects[parent].param;
                m.objects[id].param = act_param;
                spawn_default_star(m, id, [6833.0, -3654.0, 2230.0]);
            }
        } else {
            let a = m.objects[id].action;
            obj_check_attacks(&mut m.objects[id], PART_HITBOX, a, false);
            if index == 3.0 {
                let d = m.objects[id].distance_to_mario;
                m.objects[parent].set_var("u1AC", d);
            }
        }
    });
}
