//! Bubba (`bhvBubba`, Tiny-Huge Island's big fish): port of `src/game/behaviors/bubba.inc.c`.
//!
//! Bubba swims around its home underwater; within 2000 of Mario (after 30 frames) it chases him, lining up a
//! lunge, opening its mouth for 30 frames and swallowing him (`ACT_EATEN_BY_BUBBA`, via
//! `interact_clam_or_bubba` with `INT_SUBTYPE_EATS_MARIO`) when he is within 250 of the open mouth.
//!
//! Left out: the water-splash and snow-particle spawns, the closed/open mouth texture (`oAnimState` is kept in the `animState`
//! var), sounds.
use crate::behaviors::g1::abs_angle_diff;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys::{self, MASK_IN_WATER, MOVE_HIT_WALL};
use crate::behaviors::r2a::helpers::{hitbox, obj_check_attacks, random_linear_offset};
use crate::behaviors::r2a::interact::{INTERACT_CLAM_OR_BUBBA, INT_SUBTYPE_EATS_MARIO};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

/// sBubbaHitbox.
pub const HITBOX: ObjectHitbox = hitbox(INTERACT_CLAM_OR_BUBBA, 0.0, 1.0, 99.0, 0.0, 300.0, 200.0, 300.0, 200.0);

pub fn init(o: &mut SimObject) {
    o.visible = true;
    o.scale = 0.5;
    phys::set_physics(o, 200.0, -400.0, -50.0, 1000.0, 1000.0, 0.0);
}

fn s16(v: f64) -> f64 {
    to_s16(v) as f64
}

/// `approach_f32_ptr` (obj_behaviors_2.c).
fn approach(value: &mut f64, target: f64, delta: f64) -> bool {
    let d = if *value > target { -delta } else { delta };
    *value += d;
    if (*value - target) * d >= 0.0 {
        *value = target;
        return true;
    }
    false
}

/// `obj_smooth_turn` on an angle and its velocity.
fn smooth_turn(vel: &mut f64, angle: &mut f64, target: f64, proportion: f64, accel: f64, min: f64, max: f64) -> bool {
    let current = s16(*angle);
    *vel = approach_s16_symmetric(*vel, (s16(target - current) * proportion).trunc(), accel);
    let speed = vel.abs().clamp(min, max);
    *angle = approach_s16_symmetric(*angle, target, speed);
    s16(*angle) == s16(target)
}

/// `treat_far_home_as_mario(threshold)`.
fn treat_far_home_as_mario(o: &mut SimObject, mario: Vec3, threshold: f64) {
    let d = dist_3d(&o.home, &o.pos);
    if d > threshold {
        o.angle_to_mario = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
        o.distance_to_mario = 25000.0;
    } else if dist_3d(&o.home, &mario) > threshold {
        o.distance_to_mario = 20000.0;
    }
}

/// `obj_random_fixed_turn(delta)`.
fn random_fixed_turn(o: &SimObject, delta: f64) -> f64 {
    let sign = if random_u16() >= 0x7FFF as f64 { 1.0 } else { -1.0 };
    s16(o.move_yaw + sign * delta)
}

/// `obj_compute_vel_from_move_pitch(speed)`.
fn vel_from_move_pitch(o: &mut SimObject, speed: f64) {
    let pitch = o.var("movePitch");
    o.forward_vel = speed * coss(pitch);
    o.vel[1] = speed * -sins(pitch);
}

fn act0(o: &mut SimObject, mario: Vec3) {
    let lateral = lateral_dist_to_home(o);
    treat_far_home_as_mario(o, mario, 2000.0);
    o.set_var("animState", 0.0);
    o.set_var("pitchTarget", atan2s(lateral, o.pos[1] - o.home[1]) as f64);
    let mut speed = o.var("speed");
    approach(&mut speed, 5.0, 0.5);
    o.set_var("speed", speed);
    if o.var("turning") != 0.0 {
        if abs_angle_diff(o.move_yaw, o.var("yawTarget")) < 800.0 {
            o.set_var("turning", 0.0);
        }
    } else {
        if o.distance_to_mario >= 25000.0 {
            o.set_var("yawTarget", o.angle_to_mario);
            o.set_var("wait", random_linear_offset(20.0, 30.0));
        }
        let hit = phys::flags(o) & MOVE_HIT_WALL != 0;
        o.set_var("turning", if hit { 1.0 } else { 0.0 });
        if hit {
            // cur_obj_reflect_move_angle_off_wall
            let wall = o.var("wallAngle");
            o.set_var("yawTarget", s16(wall - (s16(o.move_yaw) - s16(wall)) + 0x8000 as f64));
        } else if o.timer > 30.0 && o.distance_to_mario < 2000.0 {
            o.action = 1;
        } else if o.var("wait") != 0.0 {
            o.set_var("wait", o.var("wait") - 1.0);
        } else {
            let t = random_fixed_turn(o, 0x2000 as f64);
            o.set_var("yawTarget", t);
            o.set_var("wait", random_linear_offset(100.0, 100.0));
        }
    }
}

fn act1(o: &mut SimObject, mario: Vec3, mario_yaw: f64, water: f64) {
    treat_far_home_as_mario(o, mario, 2500.0);
    if o.distance_to_mario > 2500.0 {
        o.action = 0;
        return;
    }
    let to_mario_pitch = o.var("pitchToMario");
    if o.var("chomp") != 0.0 {
        let t = o.var("chomp") - 1.0;
        o.set_var("chomp", t);
        if t == 0.0 {
            o.action = 0;
        } else if t < 15.0 {
            o.set_var("animState", 1.0);
        } else if t == 20.0 {
            let val = 10000.0 - s16(20.0 * (water - o.pos[1])).trunc();
            let p = o.var("pitchTarget") - val;
            o.set_var("pitchTarget", p);
            o.set_var("movePitch", p);
            o.set_var("speed", 40.0);
            vel_from_move_pitch(o, 40.0);
            o.set_var("animState", 0.0);
        } else {
            o.set_var("yawTarget", o.angle_to_mario);
            o.set_var("pitchTarget", to_mario_pitch);
            let target = o.var("yawTarget");
            rotate_yaw_toward(o, target, 400.0);
            let p = approach_s16_symmetric(o.var("movePitch"), to_mario_pitch, 400.0);
            o.set_var("movePitch", p);
        }
        return;
    }
    if abs_angle_diff(mario_yaw, o.angle_to_mario) < 0x3000 as f64 {
        let mut val = 0x4000 as f64 - atan2s(800.0, o.distance_to_mario - 800.0) as f64;
        if s16(o.move_yaw - o.angle_to_mario) < 0.0 {
            val = -val;
        }
        o.set_var("yawTarget", s16(o.angle_to_mario + val));
    } else {
        o.set_var("yawTarget", o.angle_to_mario);
    }
    o.set_var("pitchTarget", to_mario_pitch);
    let near_facing = o.distance_to_mario < 500.0 && abs_angle_diff(o.move_yaw, o.angle_to_mario) < 3000.0;
    if near_facing && abs_angle_diff(to_mario_pitch, o.var("movePitch")) < 3000.0 {
        o.set_var("chomp", 30.0);
        o.set_var("speed", 0.0);
        o.set_var("animState", 1.0);
    } else {
        let mut speed = o.var("speed");
        approach(&mut speed, 20.0, 0.5);
        o.set_var("speed", speed);
    }
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        let world = m.world.clone();
        let (mario, mario_yaw) = (m.pos, m.face_angle[1]);
        let water = world.water_level(m.objects[id].pos[0], m.objects[id].pos[2]);
        {
            let o = &mut m.objects[id];
            o.interact_subtype &= !INT_SUBTYPE_EATS_MARIO;
            // obj_turn_pitch_toward_mario(120, 0): the target only.
            let lateral = js_hypot(mario[0] - o.pos[0], mario[2] - o.pos[2]);
            let to_mario = atan2s(lateral, -mario[1] + (o.pos[1] - 120.0)) as f64;
            o.set_var("pitchToMario", to_mario);
            if abs_angle_diff(o.angle_to_mario, o.move_yaw) < 0x1000 as f64 && abs_angle_diff(to_mario + 0x800 as f64, o.var("movePitch")) < 0x2000 as f64 {
                if o.var("animState") != 0.0 && o.distance_to_mario < 250.0 {
                    o.interact_subtype |= INT_SUBTYPE_EATS_MARIO;
                }
            }
            phys::update_floor_and_walls(o, &world);
            match o.action {
                0 => act0(o, mario),
                _ => act1(o, mario, mario_yaw, water),
            }
            if phys::flags(o) & MASK_IN_WATER != 0 {
                let target_pitch = o.var("pitchTarget");
                let target_yaw = o.var("yawTarget");
                let (mut vp, mut vy) = (o.var("turnVelPitch"), o.var("turnVelYaw"));
                let mut pitch = o.var("movePitch");
                let mut yaw = o.move_yaw;
                smooth_turn(&mut vp, &mut pitch, target_pitch, 0.05, 10.0, 50.0, 2000.0);
                smooth_turn(&mut vy, &mut yaw, target_yaw, 0.05, 10.0, 50.0, 2000.0);
                o.set_var("turnVelPitch", vp);
                o.set_var("turnVelYaw", vy);
                o.set_var("movePitch", pitch);
                o.move_yaw = yaw;
                let speed = o.var("speed");
                vel_from_move_pitch(o, speed);
            } else {
                // Out of the water it falls: the speed and pitch follow the velocity.
                let speed = (o.forward_vel * o.forward_vel + o.vel[1] * o.vel[1]).sqrt();
                o.set_var("speed", speed);
                let mut pitch = -(atan2s(o.forward_vel, o.vel[1]) as f64);
                if s16(pitch) == 0.0 {
                    pitch += o.forward_vel * 500.0;
                }
                o.set_var("movePitch", pitch);
                o.set_var("turnVelPitch", 0.0);
            }
            let pitch = approach_s16_symmetric(o.var("facePitch"), o.var("movePitch"), 400.0);
            o.set_var("facePitch", pitch);
        }
        let action = m.objects[id].action;
        obj_check_attacks(&mut m.objects[id], HITBOX, action, false);
        phys::move_standard(&mut m.objects[id], &world, 78.0);
        let o = &mut m.objects[id];
        let floor = o.var("floorH") + 150.0;
        if o.pos[1] < floor {
            o.pos[1] = floor;
        }
    });
}
