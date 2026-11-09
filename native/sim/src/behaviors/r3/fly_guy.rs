//! Fly Guy (`macro_fly_guy` / `macro_fire_fly_guy` -> `bhvFlyGuy`): the propeller Shy Guy of SSL, THI, SL, RR and TTM.
//! Port of `src/game/behaviors/fly_guy.inc.c`.
//!
//! It hovers (a 1.5 unit sine bob), turns toward Mario inside 2000 and approaches; when facing him and level or far above, it
//! lunges down and twirls back up, or (fire variant, `oBhvParams2ndByte` 1) swells and spits one flame. A stomp kills it for 2 coins.
//!
//! Left out: `bhv_init_room` room culling, sounds. The `bhvFlyguyFlame` trail is the shared small piranha flame.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::{INTERACT_BOUNCE_TOP, INT_SUBTYPE_TWIRL_BOUNCE};
use crate::behaviors::g2::lll::bparam2;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys::{self, MASK_IN_WATER, MOVE_HIT_WALL};
use crate::behaviors::g2::thi::spit_fire;
use crate::behaviors::r2a::helpers::{abs_angle_diff, face_yaw_approach, forward_vel_approach, hitbox, obj_check_attacks};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

pub const MODEL: &str = "actors/flyguy/flyguy_geo.glb";
/// `flyguy_seg8_anims_08011A64`: one 46 frame flutter.
pub const ANIMS: [AnimSpec; 1] = [spec(0, 0, 0x2E, false)];
pub const ANIM_NAMES: [&str; 1] = ["flyguy_seg8_anim_08011A4C"];

/// `sFlyGuyHitbox`.
const HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 0.0, 2.0, 0.0, 2.0, 70.0, 60.0, 40.0, 50.0);

const IDLE: i32 = 0;
const APPROACH: i32 = 1;
const LUNGE: i32 = 2;
const SHOOT_FIRE: i32 = 3;

pub fn init(o: &mut SimObject) {
    o.visible = true;
    o.scale = 1.5;
    o.hitbox = HITBOX;
    o.interact_subtype = INT_SUBTYPE_TWIRL_BOUNCE;
    phys::set_physics(o, 50.0, 0.0, 0.0, 0.0, 1000.0, 600.0);
    anim::init(o, 0, ANIMS[0]);
}

fn s16(v: f64) -> f64 {
    to_s16(v) as f64
}

fn approach(value: &mut f64, target: f64, inc: f64) -> bool {
    if *value > target {
        *value = (*value - inc).max(target);
    } else {
        *value = (*value + inc).min(target);
    }
    *value == target
}

/// `treat_far_home_as_mario(2000)`.
fn treat_far_home_as_mario(o: &mut SimObject, mario: Vec3, dist: f64) {
    if dist_3d(&o.home, &o.pos) > dist {
        o.angle_to_mario = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
        o.distance_to_mario = 25000.0;
    } else if dist_3d(&o.home, &mario) > dist {
        o.distance_to_mario = 20000.0;
    }
}

/// `obj_face_pitch_approach` / `obj_face_roll_approach` on the drawn angle `var` (`facePitch` / `faceRoll`).
fn face_approach(o: &mut SimObject, var: &str, target: f64, delta: f64) -> bool {
    let v = approach_s16_symmetric(o.var(var), target, delta);
    o.set_var(var, v);
    s16(v) == s16(target)
}

/// `obj_turn_pitch_toward_mario(offset, 0)`: the target pitch only.
fn pitch_toward_mario(o: &SimObject, mario: Vec3, offset_y: f64) -> f64 {
    let lateral = js_hypot(mario[0] - o.pos[0], mario[2] - o.pos[2]);
    atan2s(lateral, -mario[1] + (o.pos[1] - offset_y)) as f64
}

/// `obj_grow_then_shrink` (`obj_behaviors_2.c`): -1 once at `end`, 1 on the frame the flame is shot.
fn grow_then_shrink(o: &mut SimObject, shoot_scale: f64, end: f64) -> i32 {
    let mut vel = o.var("scaleVel");
    let mut status = 0;
    if o.timer < 2.0 {
        o.scale += vel;
        vel -= 0.01;
        if vel > -0.03 {
            o.timer = 0.0;
        }
    } else if o.timer > 10.0 {
        if approach(&mut o.scale, end, 0.05) {
            status = -1;
        } else if vel != 0.0 && o.scale < shoot_scale {
            vel = 0.0;
            status = 1;
        }
    }
    o.set_var("scaleVel", vel);
    status
}

fn act_idle(o: &mut SimObject) {
    o.forward_vel = 0.0;
    if approach(&mut o.scale, 1.5, 0.02) {
        if o.distance_to_mario >= 25000.0 || o.distance_to_mario < 2000.0 {
            let target = o.angle_to_mario;
            face_yaw_approach(o, target, 0x300 as f64);
            if rotate_yaw_toward(o, target, 0x300 as f64) {
                o.action = APPROACH;
            }
        } else {
            let t = o.var("idleTimer");
            if t >= 3.0 || t == (random_u16() as i64 & 1) as f64 + 2.0 {
                o.set_var("idleTimer", 0.0);
                o.action = APPROACH;
            } else {
                o.set_var("idleTimer", t + 1.0);
            }
        }
    }
}

fn act_approach(o: &mut SimObject, mario: Vec3) {
    if o.distance_to_mario >= 25000.0 || o.distance_to_mario < 2000.0 {
        forward_vel_approach(o, 10.0, 0.5);
        let target = o.angle_to_mario;
        face_yaw_approach(o, target, 0x400 as f64);
        rotate_yaw_toward(o, target, 0x200 as f64);
        if abs_angle_diff(o.angle_to_mario, o.face_yaw) < 0x2000 as f64 && (o.pos[1] - mario[1] > 400.0 || o.distance_to_mario < 400.0) {
            if bparam2(o) != 0 && (random_u16() as i64) % 2 != 0 {
                o.action = SHOOT_FIRE;
                o.set_var("scaleVel", 0.06);
            } else {
                o.action = LUNGE;
                let pitch = pitch_toward_mario(o, mario, -200.0);
                o.set_var("lungePitch", pitch);
                o.forward_vel = 25.0 * coss(pitch);
                o.vel[1] = 25.0 * -sins(pitch);
                o.set_var("lungeYDecel", -o.vel[1] / 30.0);
            }
        }
    } else if forward_vel_approach(o, 0.0, 0.2) {
        o.action = IDLE;
    }
}

fn act_lunge(o: &mut SimObject, mario: Vec3) {
    if o.vel[1] < 0.0 {
        o.vel[1] += o.var("lungeYDecel");
        let yaw = o.face_yaw;
        rotate_yaw_toward(o, yaw, 0x800 as f64);
        let lunge = o.var("lungePitch");
        face_approach(o, "facePitch", lunge, 0x400 as f64);
        o.set_var("targetRoll", (0x1000 * (random_float() * 3.0) as i64 - 0x1000) as f64);
        o.timer = 0.0;
    } else {
        face_approach(o, "facePitch", 0.0, 0x100 as f64);
        let roll = o.var("targetRoll");
        face_approach(o, "faceRoll", roll, 300.0);
        o.move_yaw -= (s16(o.var("faceRoll")) / 4.0).trunc();
        let yaw = o.move_yaw;
        face_yaw_approach(o, yaw, 0x800 as f64);
        if o.pos[1] < mario[1] + 200.0 {
            approach(&mut o.vel[1], 20.0, 0.5);
        } else if approach(&mut o.vel[1], 0.0, 0.5) {
            if s16(o.var("faceRoll")) == 0.0 {
                o.action = APPROACH;
            }
            o.set_var("targetRoll", 0.0);
        }
    }
}

/// Returns true on the frame a flame is spat (the caller spawns it).
fn act_shoot_fire(o: &mut SimObject, mario: Vec3) -> Option<f64> {
    o.forward_vel = 0.0;
    let target = o.angle_to_mario;
    if face_yaw_approach(o, target, 0x800 as f64) {
        o.move_yaw = o.face_yaw;
        match grow_then_shrink(o, 1.2, 1.1) {
            0 => {}
            s if s < 0 => o.action = IDLE,
            _ => return Some(pitch_toward_mario(o, mario, 0.0).clamp(0x800 as f64, 0x3000 as f64)),
        }
    } else {
        o.timer = 0.0;
    }
    None
}

/// `bhv_fly_guy_update`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, 0, |m, id| {
        let mario = m.pos;
        let mut spit = None;
        {
            let o = &mut m.objects[id];
            treat_far_home_as_mario(o, mario, 2000.0);
            phys::update_floor_and_walls(o, &world);
            let flags = phys::flags(o);
            if flags & MOVE_HIT_WALL != 0 {
                // cur_obj_reflect_move_angle_off_wall
                let wall = o.var("wallAngle");
                o.move_yaw = s16(wall - (s16(o.move_yaw) - s16(wall)) + 0x8000 as f64);
            } else if flags & MASK_IN_WATER != 0 {
                o.vel[1] = 6.0;
            }
            let osc = o.var("oscTimer") + 1.0;
            o.set_var("oscTimer", osc);
            o.pos[1] += coss(0x400 as f64 * osc) * 1.5;
            match o.action {
                IDLE => act_idle(o),
                APPROACH => act_approach(o, mario),
                LUNGE => act_lunge(o, mario),
                _ => spit = act_shoot_fire(o, mario),
            }
            phys::move_standard(o, &world, -78.0);
            let action = o.action;
            let attack = obj_check_attacks(o, HITBOX, action, false);
            if attack != 0 && o.hitbox.health <= 0.0 {
                let pos = o.pos;
                o.deactivate(-1.0);
                for _ in 0..2 {
                    m.objects.spawn_coin(pos, true, 1.0);
                }
            }
        }
        if let Some(pitch) = spit {
            spit_fire(m, id, 38.0, 20.0, 2.5, 25.0, 20.0, pitch);
        }
        anim::advance(&mut m.objects[id]);
    });
}
