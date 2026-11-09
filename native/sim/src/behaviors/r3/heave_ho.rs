//! Heave-Ho (`macro_heave_ho` -> `bhvHeaveHo`, the wind-up toy of BoB-less WDW and TTC): port of
//! `src/game/behaviors/heave_ho.inc.c` (its `bhvHeaveHoThrowMario` anchor is folded in: (200, -50, 0) from the Heave-Ho).
//!
//! Hidden at home until the water is below it and Mario within 4000. It then winds up (150 frames of speed 10 that fade down to
//! 0.1), grabs a Mario in front of it (`ACT_GRABBED`) and tosses him straight back (forward -45, up 95, `ACT_THROWN_BACKWARD`).
//! It cannot be picked up (`INT_SUBTYPE_NOT_GRABBABLE`) and gives up in water.
//!
//! Left out: sounds, the wind-down jitter of the visual graph offset.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::hitbox;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys::{self, MASK_IN_WATER};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

pub const MODEL: &str = "actors/heave_ho/heave_ho_geo.glb";
/// `heave_ho_seg5_anims_0501534C`: 0 = walk (38), 1 = throw (23), 2 = wind-up (24).
pub const ANIMS: [AnimSpec; 3] = [spec(0, 0, 0x26, false), spec(0, 0, 0x17, false), spec(0, 0, 0x18, false)];
pub const ANIM_NAMES: [&str; 3] = ["heave_ho_seg5_anim_05015118", "heave_ho_seg5_anim_05015334", "heave_ho_seg5_anim_05014F28"];

/// `D_8032F460`: (until timer, acceleration of the wind-up clip).
const WIND_UP: [(i32, f64); 9] = [(30, 0.0), (42, 1.0), (52, 0.0), (64, 1.0), (74, 0.0), (86, 1.0), (96, 0.0), (108, 1.0), (118, 0.0)];

pub fn init(o: &mut SimObject) {
    o.visible = true;
    o.scale = 2.0;
    o.hitbox = hitbox(INTERACT_GRABBABLE, 0.0, 0.0, 0.0, 0.0, 120.0, 100.0, 120.0, 100.0);
    o.set_var("realInteract", INTERACT_GRABBABLE as f64);
    o.interact_subtype = INT_SUBTYPE_NOT_GRABBABLE | INT_SUBTYPE_GRABS_MARIO;
    o.set_var("grabDx", 200.0);
    o.set_var("grabDy", -50.0);
    o.set_var("grabDz", 0.0);
    phys::set_physics(o, 200.0, -400.0, -50.0, 1000.0, 1000.0, 600.0);
    anim::init(o, 0, ANIMS[0]);
}

fn anim_accel(o: &mut SimObject, index: usize, accel: f64) {
    anim::init(o, index, ANIMS[index]);
    o.set_var("animAccel", accel);
}

/// `bhv_heave_ho_loop`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, F_FACE_YAW, |m, id| {
        let mut throw = false;
        let mario = m.pos;
        {
            let o = &mut m.objects[id];
            let water = world.water_level(o.pos[0], o.pos[2]);
            phys::update_floor_and_walls(o, &world);
            match o.action {
                0 => {
                    o.pos = o.home;
                    if water < o.pos[1] && o.distance_to_mario < 4000.0 {
                        crate::behaviors::g1::become_tangible(o);
                        o.visible = true;
                        o.action = 1;
                    } else {
                        crate::behaviors::g1::become_intangible(o);
                        o.visible = false;
                    }
                }
                1 => {
                    o.forward_vel = 0.0;
                    crate::behaviors::r2a::helpers::reverse_animation(o);
                    match WIND_UP.iter().find(|(t, _)| o.timer < *t as f64) {
                        Some((_, accel)) => anim_accel(o, 2, *accel),
                        None => o.action = 2,
                    }
                }
                2 => {
                    if js_hypot(mario[0] - o.home[0], mario[2] - o.home[2]) > 1000.0 {
                        o.angle_to_mario = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
                    }
                    let mut scale = 1.0;
                    if o.timer > 150.0 {
                        scale = (302.0 - o.timer) / 152.0;
                        if scale < 0.1 {
                            scale = 0.1;
                            o.action = 1;
                        }
                    }
                    anim_accel(o, 0, scale);
                    o.forward_vel = scale * 10.0;
                    let angle_vel = (scale * 0x400 as f64).trunc();
                    let target = o.angle_to_mario;
                    o.move_yaw = approach_s16_symmetric(o.move_yaw, target, angle_vel);
                }
                _ => {
                    o.forward_vel = 0.0;
                    if o.timer == 0.0 {
                        throw = true;
                    }
                    if o.timer == 1.0 {
                        anim_accel(o, 1, 1.0);
                    }
                    if anim::near_end(o) {
                        o.action = 1;
                    }
                }
            }
            phys::move_standard(o, &world, -78.0);
            if phys::flags(o) & MASK_IN_WATER != 0 && o.action != 0 {
                o.action = 0;
            }
            if o.interact_status & INT_STATUS_GRABBED_MARIO != 0 {
                o.action = 3;
            }
            o.interact_status = 0;
            anim::advance(o);
        }
        if throw {
            // bhv_heave_ho_throw_mario_loop: oHeaveHoUnk88 == 2
            m.release_grab(id, -45.0, 95.0, true);
        }
    });
}
