//! The SSL Tweester (`bhvTweester`, `src/game/behaviors/tweester.inc.c`) and the WF owl Hoot
//! (`bhvHoot`, `hoot.inc.c`). The Mario side lives in `mario_r2a.rs` (`ACT_TORNADO_TWIRLING`,
//! `ACT_TWIRLING`, `ACT_RIDING_HOOT`, `interact_tornado`, `interact_hoot`).
//!
//! Left out: the sand particles of the Tweester, the time stop during Hoot's mid-air dialog, sounds.
use super::helpers::*;
use crate::behaviors::g1::anim::{self, AnimSpec};
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::collision::WallData;
use crate::mario_r2a::ACT_TWIRLING;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

pub const TWEESTER_IDLE: i32 = 0;
pub const TWEESTER_CHASE: i32 = 1;
pub const TWEESTER_HIDE: i32 = 2;
/// sTweesterHitbox: `INTERACT_TORNADO`, radius 1500, height 4000.
pub const TWEESTER_HITBOX: ObjectHitbox = hitbox(super::interact::INTERACT_TORNADO, 0.0, 0.0, 0.0, 0.0, 1500.0, 4000.0, 0.0, 0.0);

pub fn init_tweester(o: &mut SimObject) {
    o.visible = true;
    o.hitbox = TWEESTER_HITBOX;
    o.set_var("hbInit", 1.0);
    o.set_var("realInteract", super::interact::INTERACT_TORNADO as f64);
    phys::set_physics(o, 30.0, -400.0, 0.0, 0.0, 0.0, 200.0);
    o.scale = 0.0;
}

/// `tweester_scale_and_move`.
fn scale_and_move(o: &mut SimObject, pre_scale: f64) {
    let scale = pre_scale * 0.4;
    let t = o.var("scaleTimer");
    o.set_var("scaleX", ((coss(t) + 1.0) * 0.5 * 0.3 + 1.0) * scale);
    o.set_var("scaleY", ((-coss(t) + 1.0) * 0.5 * 0.5 + 0.5) * scale);
    o.scale = o.var("scaleX");
    o.set_var("scaleTimer", t + 0x200 as f64);
    o.forward_vel = 14.0;
    o.face_yaw = to_s16(o.face_yaw + 0x2C00 as f64) as f64;
}

/// `bhv_tweester_loop`.
pub fn tweester(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let world = m.world.clone();
        let mario = m.pos;
        let twirling = m.action == ACT_TWIRLING;
        if first_frame(&mut m.objects[id]) {
            drop_to_floor(&mut m.objects[id], &world);
            let o = &mut m.objects[id];
            o.home = o.pos;
        }
        let o = &mut m.objects[id];
        set_hitbox(o, TWEESTER_HITBOX);
        let activation = ((o.param as u32) >> 16 & 0xFF) as f64 * 100.0;
        match o.action {
            TWEESTER_IDLE => {
                if o.action_state == 0.0 {
                    crate::behaviors::g1::become_tangible(o);
                    o.pos = o.home;
                    o.scale = 0.0;
                    o.set_var("scaleY", 0.0);
                    if o.distance_to_mario < 1500.0 {
                        o.action_state += 1.0;
                    }
                    o.timer = 0.0;
                } else {
                    scale_and_move(o, o.timer / 60.0);
                    if o.timer >= 60.0 {
                        o.action = TWEESTER_CHASE;
                    }
                }
            }
            TWEESTER_CHASE => {
                o.set_var("angleToHome", angle_to_home(o));
                let (dx, dz) = (o.home[0] - mario[0], o.home[2] - mario[2]);
                if (dx * dx + dz * dz).sqrt() < activation && o.action_state == 0.0 {
                    o.forward_vel = 20.0;
                    let t = o.angle_to_mario;
                    rotate_yaw_toward(o, t, 0x200 as f64);
                    if twirling {
                        o.action_state += 1.0;
                    }
                } else {
                    o.forward_vel = 20.0;
                    let t = o.var("angleToHome");
                    rotate_yaw_toward(o, t, 0x200 as f64);
                    let (hx, hz) = (o.home[0] - o.pos[0], o.home[2] - o.pos[2]);
                    if (hx * hx + hz * hz).sqrt() < 200.0 {
                        o.action = TWEESTER_HIDE;
                    }
                }
                if o.distance_to_mario > 3000.0 {
                    o.action = TWEESTER_HIDE;
                }
                phys::update_floor_and_walls(o, &world);
                if phys::flags(o) & phys::MOVE_HIT_WALL != 0 {
                    o.move_yaw = o.var("wallAngle");
                }
                phys::move_standard(o, &world, 60.0);
                scale_and_move(o, 1.0);
            }
            _ => {
                let shrink = 60.0 - o.timer;
                if shrink >= 0.0 {
                    scale_and_move(o, shrink / 60.0);
                } else {
                    crate::behaviors::g1::become_intangible(o);
                    let (dx, dz) = (o.home[0] - mario[0], o.home[2] - mario[2]);
                    if (dx * dx + dz * dz).sqrt() > 2500.0 || o.timer > 360.0 {
                        o.action = TWEESTER_IDLE;
                    }
                }
            }
        }
        m.objects[id].interact_status = 0;
    });
}

// ---------------------------------------------------------------------------------------------
// Hoot
// ---------------------------------------------------------------------------------------------

/// `hoot_seg5_anims_05005768`.
pub const HOOT_ANIMS: [AnimSpec; 2] = [anim_spec(false, 0, 0, 20), anim_spec(false, 0, 0, 10)];
pub const HOOT_ANIM_NAMES: [&str; 2] = ["hoot_seg5_anim_050053EC", "hoot_seg5_anim_05005750"];
pub const HOOT_ASCENT: i32 = 0;
pub const HOOT_CARRY: i32 = 1;
pub const HOOT_TIRED: i32 = 2;

pub fn init_hoot(o: &mut SimObject) {
    o.visible = false;
    o.hitbox = hitbox(super::interact::INTERACT_HOOT, 0.0, 0.0, 0.0, 0.0, 75.0, 75.0, 75.0, 75.0);
    o.set_var("hbInit", 1.0);
    o.set_var("realInteract", super::interact::INTERACT_HOOT as f64);
    // bhv_hoot_init
    o.home = [o.pos[0] + 800.0, o.pos[1] - 150.0, o.pos[2] + 300.0];
    anim::init(o, 0, HOOT_ANIMS[0]);
    crate::behaviors::g1::become_intangible(o);
}

fn floor_y(m: &MarioRuntime, o: &SimObject, dist: f64) -> f64 {
    let (x, z) = (dist * sins(o.move_yaw) + o.pos[0], dist * coss(o.move_yaw) + o.pos[2]);
    m.world.find_floor(x, 10000.0, z).height
}

fn free_step(m: &mut MarioRuntime, id: ObjId, fast: bool, speed: f64) {
    let (xp, zp) = (m.objects[id].pos[0], m.objects[id].pos[2]);
    let o = &mut m.objects[id];
    let (yaw, pitch, frame) = (o.move_yaw, o.var("movePitch"), o.var("animFrame"));
    o.vel[1] = sins(pitch) * speed;
    let h = coss(pitch) * speed;
    o.vel[0] = sins(yaw) * h;
    o.vel[2] = coss(yaw) * h;
    o.pos[0] += o.vel[0];
    let k = if fast { 6553.6 } else { 3276.8 };
    o.pos[1] -= o.vel[1] + coss((frame * k).trunc()) * 50.0 / 4.0;
    o.pos[2] += o.vel[2];
    let f = m.world.find_floor(m.objects[id].pos[0], m.objects[id].pos[1], m.objects[id].pos[2]);
    if f.surface.is_none() {
        m.objects[id].pos[0] = xp;
        m.objects[id].pos[2] = zp;
    }
}

fn carry_step(o: &mut SimObject, speed: f64) {
    let (yaw, pitch, frame) = (o.move_yaw, o.var("movePitch"), o.var("animFrame"));
    o.vel[1] = sins(pitch) * speed;
    let h = coss(pitch) * speed;
    o.vel[0] = sins(yaw) * h;
    o.vel[2] = coss(yaw) * h;
    o.pos[0] += o.vel[0];
    o.pos[1] -= o.vel[1] + coss((frame * 6553.6).trunc()) * 50.0 / 4.0;
    o.pos[2] += o.vel[2];
}

/// `hoot_surface_collision`.
fn surface_collision(m: &mut MarioRuntime, id: ObjId, xp: f64, zp: f64) {
    let o = &m.objects[id];
    let mut d = WallData { x: o.pos[0], y: o.pos[1], z: o.pos[2], offset_y: 10.0, radius: 50.0, walls: vec![] };
    let hit = m.world.find_wall_collisions(&mut d, false) != 0;
    let o = &mut m.objects[id];
    if hit {
        o.pos = [d.x, d.y, d.z];
        o.set_var("release", 1.0);
    }
    let f = m.world.find_floor(m.objects[id].pos[0], m.objects[id].pos[1], m.objects[id].pos[2]);
    let o = &mut m.objects[id];
    if f.surface.is_none() {
        o.pos[0] = xp;
        o.pos[2] = zp;
        return;
    }
    if o.pos[0].abs() > 8000.0 {
        o.pos[0] = xp;
    }
    if o.pos[2].abs() > 8000.0 {
        o.pos[2] = zp;
    }
    if f.height + 125.0 > o.pos[1] {
        o.pos[1] = f.height + 125.0;
    }
}

fn player_set_yaw(m: &mut MarioRuntime, id: ObjId) {
    let mut stick = m.stick_x;
    if (-9.0..10.0).contains(&stick) {
        stick = 0.0;
    }
    let o = &mut m.objects[id];
    o.move_yaw = to_s16(o.move_yaw - 5.0 * stick.trunc()) as f64;
}

fn action_loop(m: &mut MarioRuntime, id: ObjId) {
    let (xp, zp) = (m.objects[id].pos[0], m.objects[id].pos[2]);
    match m.objects[id].action {
        HOOT_ASCENT => {
            let o = &mut m.objects[id];
            let to_origin = atan2s(-o.pos[2], -o.pos[0]) as f64;
            o.move_yaw = approach_s16_symmetric(o.move_yaw, to_origin, 0x500 as f64);
            o.set_var("movePitch", to_s16(0xCE38 as f64) as f64);
            if o.timer >= 29.0 {
                o.set_var("animFrame", 1.0);
                o.set_var("animAssist", 1.0);
            }
            if o.pos[1] > 6500.0 {
                o.action = HOOT_CARRY;
            }
            carry_step(o, 60.0);
        }
        HOOT_CARRY => {
            player_set_yaw(m, id);
            m.objects[id].set_var("movePitch", 0x71C as f64);
            let low = m.objects[id].pos[1] < 2700.0;
            if low && update_dialog(m, id, 45) {
                m.objects[id].action = HOOT_TIRED;
            }
            carry_step(&mut m.objects[id], 20.0);
        }
        _ => {
            player_set_yaw(m, id);
            let o = &mut m.objects[id];
            o.set_var("movePitch", 0.0);
            carry_step(o, 20.0);
            if o.timer > 60.0 {
                o.set_var("release", 1.0);
            }
        }
    }
    surface_collision(m, id, xp, zp);
}

fn awake_loop(m: &mut MarioRuntime, id: ObjId) {
    if m.objects[id].interact_status == 1 {
        action_loop(m, id);
        init_anim(&mut m.objects[id], &HOOT_ANIMS, 1);
    } else {
        init_anim(&mut m.objects[id], &HOOT_ANIMS, 0);
        // hoot_turn_to_home
        let o = &mut m.objects[id];
        let (hx, hy, hz) = (o.home[0] - o.pos[0], o.home[1] - o.pos[1], o.home[2] - o.pos[2]);
        let h_angle = atan2s(hz, hx) as f64;
        let v_angle = atan2s((hx * hx + hz * hz).sqrt(), -hy) as f64;
        o.move_yaw = approach_s16_symmetric(o.move_yaw, h_angle, 0x140 as f64);
        let pitch = approach_s16_symmetric(o.var("movePitch"), v_angle, 0x140 as f64);
        o.set_var("movePitch", pitch);
        // hoot_floor_bounce
        let mut pitch = pitch;
        let snapshot = o.clone();
        let y375 = floor_y(m, &snapshot, 375.0);
        if y375 + 75.0 > snapshot.pos[1] {
            pitch -= 3640.8888;
        }
        let y200 = floor_y(m, &snapshot, 200.0);
        if y200 + 125.0 > snapshot.pos[1] {
            pitch -= 7281.7776;
        }
        let y0 = floor_y(m, &snapshot, 0.0);
        let o = &mut m.objects[id];
        if y0 + 125.0 > o.pos[1] {
            o.pos[1] = y0 + 125.0;
        }
        if pitch < -21845.3328 {
            pitch = -21845.0;
        }
        o.set_var("movePitch", pitch);
        free_step(m, id, false, 10.0);
        let o = &mut m.objects[id];
        o.action = HOOT_ASCENT;
        o.timer = 0.0;
        o.prev_action = HOOT_ASCENT;
    }
}

/// `bhv_hoot_loop`.
pub fn hoot(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        let age = m.objects[id].var("age") + 1.0;
        m.objects[id].set_var("age", age);
        match m.objects[id].var("avail") as i32 {
            0 => {
                let p = m.objects[id].pos;
                if point_within_radius_of_mario(m, p, 50.0) {
                    m.objects[id].visible = true;
                    m.objects[id].set_var("avail", 1.0);
                }
            }
            _ => {
                awake_loop(m, id);
                if m.objects[id].var("avail") == 1.0 && update_dialog(m, id, 44) {
                    let o = &mut m.objects[id];
                    crate::behaviors::g1::become_tangible(o);
                    o.set_var("avail", 2.0);
                }
            }
        }
        advance_anim(&mut m.objects[id]);
    });
}
