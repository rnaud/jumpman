//! Skeeter (`bhvSkeeter`, Wet-Dry World): the water strider. Port of `src/game/behaviors/skeeter.inc.c`.
//!
//! It walks on the shore, wanders and turns toward Mario inside 500 units, and on the water lunges
//! across the surface (speed 80) leaving four ripples; a stomp (INTERACT_BOUNCE_TOP) kills it for 3 coins.
//! The `cur_obj_move_standard` framework of the group-2 ports gives it the water surface flags.
//!
//! Left out: the object-to-object pushing of `obj_resolve_object_collisions`, sounds.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::INTERACT_BOUNCE_TOP;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys::{self, MOVE_AT_WATER_SURFACE, MOVE_HIT_EDGE, MOVE_HIT_WALL, MASK_ON_GROUND};
use crate::behaviors::r2a::helpers::{hitbox, obj_check_attacks, random_linear_offset};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;

pub const WAVE_MODEL: &str = "actors/water_wave/idle_water_wave_geo.glb";

/// `skeeter_seg6_anims_06007DE0`: 0 = lunge (20, no loop), 1 = swim (49), 2 = walk (19), 3 = idle (39).
pub const ANIMS: [AnimSpec; 4] = [spec(0, 0, 0x14, true), spec(0, 0, 0x31, false), spec(0, 0, 0x13, false), spec(0, 0, 0x27, false)];
pub const ANIM_NAMES: [&str; 4] = ["skeeter_seg6_anim_06005D44", "skeeter_seg6_anim_06006B70", "skeeter_seg6_anim_060071E0", "skeeter_seg6_anim_06007DC8"];

/// sSkeeterHitbox.
pub const HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 20.0, 2.0, 0.0, 3.0, 180.0, 100.0, 150.0, 90.0);

const IDLE: i32 = 0;
const LUNGE: i32 = 1;
const WALK: i32 = 2;

/// `D_80331C38`: the four ripples' (left, forward) offsets.
const WAVES: [(f64, f64); 4] = [(-130.0, -190.0), (130.0, -190.0), (-180.0, 130.0), (180.0, 130.0)];

pub fn init(o: &mut SimObject) {
    o.visible = true;
    phys::set_physics(o, 180.0, -400.0, -50.0, 1000.0, 1000.0, 1200.0);
    o.set_var("faceFollows", 1.0);
}

/// `treat_far_home_as_mario(1000)`.
fn treat_far_home_as_mario(o: &mut SimObject, mario: Vec3) {
    let d = dist_3d(&o.home, &o.pos);
    if d > 1000.0 {
        o.angle_to_mario = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
        o.distance_to_mario = 25000.0;
    } else if dist_3d(&o.home, &mario) > 1000.0 {
        o.distance_to_mario = 20000.0;
    }
}

/// `obj_smooth_turn(&oSkeeterUnk1AC, &oMoveAngleYaw, target, proportion, accel, minSpeed, maxSpeed)`.
fn smooth_turn(o: &mut SimObject, target: f64, proportion: f64, accel: f64, min: f64, max: f64) -> bool {
    let current = to_s16(o.move_yaw) as f64;
    let vel = approach_s16_symmetric(o.var("turnVel"), (to_s16(target - current) as f64 * proportion).trunc(), accel);
    o.set_var("turnVel", vel);
    let speed = vel.abs().clamp(min, max);
    o.move_yaw = approach_s16_symmetric(o.move_yaw, target, speed);
    to_s16(o.move_yaw) == to_s16(target)
}

/// `obj_random_fixed_turn(delta)`.
fn random_fixed_turn(o: &SimObject, delta: f64) -> f64 {
    let sign = if random_u16() >= 0x7FFF as f64 { 1.0 } else { -1.0 };
    to_s16(o.move_yaw + sign * delta) as f64
}

/// `obj_bounce_off_walls_edges_objects`: the new target yaw when it ran into a wall or an edge.
fn bounce_off_walls_edges(o: &SimObject) -> Option<f64> {
    let f = phys::flags(o);
    if f & MOVE_HIT_WALL != 0 {
        let wall = o.var("wallAngle");
        Some(to_s16(wall - (to_s16(o.move_yaw) as f64 - to_s16(wall) as f64) + 0x8000 as f64) as f64)
    } else if f & MOVE_HIT_EDGE != 0 {
        Some(to_s16(o.move_yaw + 0x8000 as f64) as f64)
    } else {
        None
    }
}

fn spawn_waves(m: &mut MarioRuntime, id: ObjId) {
    let (pos, yaw) = (m.objects[id].pos, m.objects[id].move_yaw);
    for (left, forward) in WAVES {
        let w = spawn_child(m, id, K::SkeeterWave, Some(WAVE_MODEL), 0.0);
        let o = &mut m.objects[w];
        // `spawn_object_relative_with_scale(0, left, 0, forward, 0.8, ...)`: offsets rotated by the parent's yaw.
        o.pos = [pos[0] + left * coss(yaw) + forward * sins(yaw), pos[1], pos[2] - left * sins(yaw) + forward * coss(yaw)];
        o.scale = 0.8;
        o.visible = true;
    }
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let flags = if m.objects[id].var("faceFollows") != 0.0 { F_FACE_YAW } else { 0 };
    run(m, id, flags, |m, id| {
        let world = m.world.clone();
        let mario = m.pos;
        treat_far_home_as_mario(&mut m.objects[id], mario);
        phys::update_floor_and_walls(&mut m.objects[id], &world);
        match m.objects[id].action {
            IDLE => idle(m, id),
            LUNGE => lunge(m, id),
            _ => walk(m, id),
        }
        let o = &mut m.objects[id];
        let action = o.action;
        let attack = obj_check_attacks(o, HITBOX, action, false);
        if attack != 0 && o.hitbox.health <= 0.0 {
            // obj_die_if_health_non_positive: three coins (SOUND_OBJ_SNUFIT_SKEETER_DEATH).
            let pos = o.pos;
            o.deactivate(-1.0);
            for _ in 0..3 {
                m.objects.spawn_coin(pos, true, 1.0);
            }
            return;
        }
        phys::move_standard(&mut m.objects[id], &world, -78.0);
        anim::advance(&mut m.objects[id]);
    });
}

fn idle(m: &mut MarioRuntime, id: ObjId) {
    let on_ground = phys::flags(&m.objects[id]) & MASK_ON_GROUND != 0;
    if on_ground {
        let o = &mut m.objects[id];
        anim::init(o, 3, ANIMS[3]);
        o.forward_vel = 0.0;
        if o.timer > o.var("waitTime") && anim::near_end(o) {
            o.action = WALK;
        }
        return;
    }
    anim::init(&mut m.objects[id], 1, ANIMS[1]);
    if phys::flags(&m.objects[id]) & MOVE_AT_WATER_SURFACE != 0 {
        spawn_waves(m, id);
        let o = &mut m.objects[id];
        let target = o.var("targetAngle");
        if o.timer > 60.0 && smooth_turn(o, target, 0.02, 5.0, 50.0, 200.0) {
            if o.var("waitTime") != 0.0 {
                o.set_var("waitTime", o.var("waitTime") - 1.0);
            } else if anim::near_end(o) {
                o.action = LUNGE;
                o.forward_vel = 80.0;
                o.set_var("turnVel", 0.0);
            }
        }
    }
}

fn lunge(m: &mut MarioRuntime, id: ObjId) {
    if phys::flags(&m.objects[id]) & MOVE_AT_WATER_SURFACE == 0 {
        m.objects[id].action = IDLE;
        return;
    }
    spawn_waves(m, id);
    let o = &mut m.objects[id];
    anim::init(o, 0, ANIMS[0]);
    if phys::flags(o) & MOVE_HIT_WALL != 0 {
        let wall = o.var("wallAngle");
        o.move_yaw = to_s16(wall - (to_s16(o.move_yaw) as f64 - to_s16(wall) as f64) + 0x8000 as f64) as f64;
        o.forward_vel *= 0.3;
        // The model keeps its facing until the lunge ends (`OBJ_FLAG_SET_FACE_YAW_TO_MOVE_YAW` cleared).
        o.set_var("faceFollows", 0.0);
    }
    if approach_f32_ptr_to(&mut o.forward_vel, 0.0, 0.8) && anim::at_end(o) {
        o.move_yaw = o.face_yaw;
        o.set_var("targetAngle", if o.distance_to_mario >= 25000.0 { o.angle_to_mario } else { random_fixed_turn(o, random_u16() % 0x2000 as f64) });
        o.action = IDLE;
        o.set_var("waitTime", random_linear_offset(0.0, 30.0));
        o.set_var("faceFollows", 1.0);
    }
}

fn walk(m: &mut MarioRuntime, id: ObjId) {
    let o = &mut m.objects[id];
    if phys::flags(o) & MASK_ON_GROUND == 0 {
        o.action = IDLE;
        return;
    }
    let goal = o.var("walkSpeed");
    approach_f32_ptr_to(&mut o.forward_vel, goal, 0.4);
    let accel = 0.12 * o.forward_vel;
    anim::init_accel(o, 2, ANIMS[2], accel);
    if o.var("turningAway") != 0.0 {
        let target = o.var("targetAngle");
        let turning = !rotate_yaw_toward(o, target, 0x400 as f64);
        o.set_var("turningAway", if turning { 1.0 } else { 0.0 });
    } else {
        if o.distance_to_mario >= 25000.0 {
            o.set_var("targetAngle", o.angle_to_mario);
            o.set_var("waitTime", random_linear_offset(20.0, 30.0));
        }
        if let Some(t) = bounce_off_walls_edges(o) {
            o.set_var("targetAngle", t);
            o.set_var("turningAway", 1.0);
        } else {
            o.set_var("turningAway", 0.0);
            if o.distance_to_mario < 500.0 {
                o.set_var("targetAngle", o.angle_to_mario);
                o.set_var("walkSpeed", 20.0);
            } else {
                o.set_var("walkSpeed", 10.0);
                if o.var("waitTime") != 0.0 {
                    o.set_var("waitTime", o.var("waitTime") - 1.0);
                } else if anim::near_end(o) {
                    if (random_u16() as i64) & 3 != 0 {
                        let t = random_fixed_turn(o, 0x2000 as f64);
                        o.set_var("targetAngle", t);
                    } else {
                        o.action = IDLE;
                    }
                    o.set_var("waitTime", random_linear_offset(100.0, 100.0));
                }
            }
        }
    }
    let target = o.var("targetAngle");
    rotate_yaw_toward(o, target, 0x400 as f64);
}

/// `obj_forward_vel_approach`-style approach (`approach_f32_ptr`): true once at the target.
fn approach_f32_ptr_to(value: &mut f64, target: f64, delta: f64) -> bool {
    let d = if *value > target { -delta } else { delta };
    *value += d;
    if (*value - target) * d >= 0.0 {
        *value = target;
        return true;
    }
    false
}

pub fn init_wave(o: &mut SimObject) {
    o.visible = true;
}

/// `bhv_skeeter_wave_update`: shrinks by 0.3 per frame and disappears at 0.
pub fn wave(id: ObjId, m: &mut MarioRuntime) {
    let o = &mut m.objects[id];
    let mut s = o.scale;
    let done = approach_f32_ptr_to(&mut s, 0.0, 0.3);
    o.scale = s;
    if done {
        mark_for_deletion(o);
    }
}
