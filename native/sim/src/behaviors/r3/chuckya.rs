//! Chuckya (`macro_chuckya` -> `bhvChuckya`): the grabbing purple bomb-thrower of SSL, THI, TTM, WDW, RR and BitS.
//! Port of `src/game/behaviors/chuckya.inc.c` (with its `bhvChuckyaAnchorMario`, folded in: the anchor is (0, -60, 150)).
//!
//! It stands, turns toward Mario inside 2000 of home and runs at him (speed 30), and when Mario walks into its front cone it
//! grabs him (`ACT_GRABBED`, see `MarioRuntime::start_king_grab` / `release_grab`). It spins and then throws him (forward 40, up
//! 40, `ACT_THROWN_FORWARD`); mashing A / flicking the stick more than 10 times frees him early (forward 10, up 10) and the
//! Chuckya waits 100 frames. Thrown or dropped by Mario it flies and dies on the first wall, water or floor with 5 coins.
//!
//! Left out: sounds, the mist puff, the "held" rendering (`cur_obj_unrender_set_action_and_anim`), `print_debug_bottom_up`.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::{abs_angle_diff, hitbox};
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys::{self, MASK_IN_WATER, MASK_ON_GROUND, MOVE_HIT_WALL};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

pub const MODEL: &str = "actors/chuckya/chuckya_geo.glb";
/// `chuckya_seg8_anims_0800C070`.
pub const ANIMS: [AnimSpec; 6] = [spec(0, 0, 0x12, false), spec(0, 0, 0x14, false), spec(0, 0, 0x0F, false), spec(0, 0, 0x1D, false), spec(0, 0, 0x07, false), spec(0, 0, 0x1D, false)];
pub const ANIM_NAMES: [&str; 6] = ["chuckya_seg8_anim_0800AF68", "chuckya_seg8_anim_0800B1A8", "chuckya_seg8_anim_0800B4A8", "chuckya_seg8_anim_0800B9F8", "chuckya_seg8_anim_0800BBEC", "chuckya_seg8_anim_0800C058"];

pub fn init(o: &mut SimObject) {
    o.visible = true;
    o.scale = 2.0;
    o.hitbox = hitbox(INTERACT_GRABBABLE, 0.0, 0.0, 0.0, 0.0, 150.0, 100.0, 150.0, 100.0);
    o.set_var("realInteract", INTERACT_GRABBABLE as f64);
    o.interact_subtype |= INT_SUBTYPE_GRABS_MARIO;
    o.set_var("grabDx", 0.0);
    o.set_var("grabDy", -60.0);
    o.set_var("grabDz", 150.0);
    phys::set_physics(o, 30.0, -400.0, -50.0, 1000.0, 1000.0, 200.0);
    anim::init(o, 4, ANIMS[4]);
}

fn approach(value: &mut f64, target: f64, inc: f64) {
    if *value > target {
        *value = (*value - inc).max(target);
    } else if *value < target {
        *value = (*value + inc).min(target);
    }
}

fn lateral_dist_from_mario_to_home(o: &SimObject, mario: Vec3) -> f64 {
    js_hypot(mario[0] - o.home[0], mario[2] - o.home[2])
}

fn lateral_dist_to_home(o: &SimObject) -> f64 {
    js_hypot(o.home[0] - o.pos[0], o.home[2] - o.pos[2])
}

fn angle_to_home(o: &SimObject) -> f64 {
    atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64
}

/// `player_performed_grab_escape_action`: a stick flick past 40 (re-armed under 30) or an A press.
fn grab_escape(o: &mut SimObject, m_mag: f64, a_pressed: bool) -> bool {
    let mut result = false;
    if m_mag < 30.0 {
        o.set_var("escState", 0.0);
    }
    if o.var("escState") == 0.0 && m_mag > 40.0 {
        o.set_var("escState", 1.0);
        result = true;
    }
    result || a_pressed
}

fn act_walk(o: &mut SimObject, mario: Vec3) {
    if o.timer == 0.0 {
        o.set_var("fc", 0.0);
    }
    let sub = o.action_state as i32;
    match sub {
        0 => {
            o.forward_vel = 0.0;
            if lateral_dist_from_mario_to_home(o, mario) < 2000.0 {
                let target = o.angle_to_mario;
                rotate_yaw_toward(o, target, 0x400 as f64);
                if o.var("fc") > 40.0 || abs_angle_diff(o.move_yaw, o.angle_to_mario) < 0x1000 as f64 {
                    o.action_state = 1.0;
                }
            } else {
                o.action_state = 3.0;
            }
        }
        1 => {
            approach(&mut o.forward_vel, 30.0, 4.0);
            if abs_angle_diff(o.move_yaw, o.angle_to_mario) > 0x4000 as f64 {
                o.action_state = 2.0;
            }
            if lateral_dist_from_mario_to_home(o, mario) > 2000.0 {
                o.action_state = 3.0;
            }
        }
        2 => {
            approach(&mut o.forward_vel, 0.0, 4.0);
            if o.var("fc") > 48.0 {
                o.action_state = 0.0;
            }
        }
        _ => {
            if lateral_dist_to_home(o) < 500.0 {
                o.forward_vel = 0.0;
            } else {
                approach(&mut o.forward_vel, 10.0, 4.0);
                o.angle_to_mario = angle_to_home(o);
                let target = o.angle_to_mario;
                rotate_yaw_toward(o, target, 0x800 as f64);
            }
            if lateral_dist_from_mario_to_home(o, mario) < 1900.0 {
                o.action_state = 0.0;
            }
        }
    }
    if o.action_state as i32 != sub {
        o.set_var("fc", 0.0);
    } else {
        o.set_var("fc", o.var("fc") + 1.0);
    }
    anim::init(o, 4, ANIMS[4]);
}

/// 0 nothing, 2 = throw, 3 = escaped.
fn act_grabbed(o: &mut SimObject, world: &crate::collision::CollisionWorld, mag: f64, a_pressed: bool) -> i32 {
    let mut release = 0;
    if o.action_state == 0.0 {
        anim::init(o, 0, ANIMS[0]);
        if anim::near_end(o) {
            o.action_state += 1.0;
        }
        o.set_var("fc", (random_float() * 30.0 + 10.0).trunc());
        o.set_var("escapes", 0.0);
        o.forward_vel = 0.0;
    } else if o.action_state == 1.0 {
        let n = o.var("escapes") + grab_escape(o, mag, a_pressed) as i32 as f64;
        o.set_var("escapes", n);
        if n > 10.0 {
            release = 3;
            o.action = 3;
        } else {
            anim::init(o, 1, ANIMS[1]);
            o.move_yaw += 0x800 as f64;
            let fc = o.var("fc");
            o.set_var("fc", fc - 1.0);
            if fc < 0.0 && (super::check_if_moving_over_floor(o, world, 50.0, 150.0) || fc - 1.0 < -16.0) {
                o.action_state += 1.0;
            }
        }
    } else {
        anim::init(o, 3, ANIMS[3]);
        if anim::check_frame(o, 18) {
            release = 2;
            o.action = 3;
        }
    }
    release
}

/// `bhv_chuckya_loop`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, F_FACE_YAW, |m, id| {
        let (mario, mag, a_pressed) = (m.pos, m.controller.stick_mag, m.input & INPUT_A_PRESSED != 0);
        let mut release = 0;
        {
            let o = &mut m.objects[id];
            o.interact_subtype |= INT_SUBTYPE_GRABS_MARIO;
            match o.held_state {
                HELD_HELD => {
                    o.action = 2;
                    o.action_state = 0.0;
                    anim::init(o, 0, ANIMS[0]);
                    o.timer = 0.0;
                    return;
                }
                HELD_THROWN | HELD_DROPPED => {
                    // cur_obj_get_thrown_or_placed(20, 50, 2)
                    if o.held_state == HELD_THROWN {
                        o.forward_vel = 20.0;
                        o.vel[1] = 50.0;
                    } else {
                        o.forward_vel = 0.0;
                    }
                    o.held_state = HELD_FREE;
                    o.action = 2;
                }
                _ => {}
            }
        }
        {
            let o = &mut m.objects[id];
            phys::update_floor_and_walls(o, &world);
            match o.action {
                0 => act_walk(o, mario),
                1 => release = act_grabbed(o, &world, mag, a_pressed),
                3 => {
                    o.forward_vel = 0.0;
                    o.vel[1] = 0.0;
                    anim::init(o, 4, ANIMS[4]);
                    if o.timer > 100.0 {
                        o.action = 0;
                    }
                }
                _ => {
                    let f = phys::flags(o);
                    if f & (MOVE_HIT_WALL | MASK_IN_WATER | MASK_ON_GROUND) != 0 {
                        let pos = o.pos;
                        o.deactivate(-1.0);
                        for _ in 0..5 {
                            m.objects.spawn_coin(pos, true, 1.0);
                        }
                        return;
                    }
                }
            }
            phys::move_standard(o, &world, -30.0);
        }
        // `if (oInteractStatus & INT_STATUS_GRABBED_MARIO) { oAction = 1; ... }`
        if m.objects[id].interact_status & INT_STATUS_GRABBED_MARIO != 0 {
            let o = &mut m.objects[id];
            o.action = 1;
            o.action_state = 0.0;
        }
        match release {
            2 => m.release_grab(id, 40.0, 40.0, false),
            3 => m.release_grab(id, 10.0, 10.0, false),
            _ => {}
        }
        let o = &mut m.objects[id];
        o.interact_status = 0;
        anim::advance(o);
    });
}
