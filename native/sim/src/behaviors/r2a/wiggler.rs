//! Wiggler (THI area 3, "Make Wiggler Squirm"): `bhvWigglerHead` and its three `bhvWigglerBody`
//! parts. Port of `src/game/behaviors/wiggler.inc.c` (`bhv_wiggler_update`,
//! `bhv_wiggler_body_part_update`), `obj_handle_attacks`, `obj_set_knockback_action` and
//! `obj_bounce_off_walls_edges_objects` of `obj_behaviors_2.c`.
//!
//! Left out: sounds, `seq_player_lower_volume`, Mario's look-up pose during the dialogs, the
//! `numCollidedObjs` push-apart of `obj_resolve_object_collisions` (no object collision list).
use super::helpers::*;
use crate::behaviors::g1::anim::{self, AnimSpec};
use crate::behaviors::g1::INTERACT_BOUNCE_TOP;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use ObjectBehaviorKind as K;

pub const ACT_UNINITIALIZED: i32 = 0;
pub const ACT_WALK: i32 = 1;
pub const ACT_KNOCKBACK: i32 = 2;
pub const ACT_JUMPED_ON: i32 = 3;
pub const ACT_SHRINK: i32 = 4;
pub const ACT_FALL_THROUGH_FLOOR: i32 = 5;

/// `wiggler_seg5_anims_0500EC8C` / `wiggler_seg5_anims_0500C874`: one 26 frame loop each.
pub const ANIMS: [AnimSpec; 1] = [anim_spec(false, 0, 0, 26)];
pub const ANIM_NAMES_HEAD: [&str; 1] = ["wiggler_seg5_anim_0500EC74"];
pub const ANIM_NAMES_BODY: [&str; 1] = ["wiggler_seg5_anim_0500C760"];

/// sWigglerBodyPartHitbox.
pub const BODY_HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 0.0, 3.0, 99.0, 0.0, 20.0, 20.0, 20.0, 10.0);
/// sWigglerHitbox.
pub const HEAD_HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 0.0, 3.0, 4.0, 0.0, 60.0, 50.0, 30.0, 40.0);
/// sWigglerSpeeds: target walking speed at health 1..4.
pub const SPEEDS: [f64; 4] = [2.0, 40.0, 30.0, 16.0];
/// The text shown after the first, second and third jump (DIALOG_152, 168, 151).
pub const ATTACK_TEXT: [u32; 3] = [152, 168, 151];

pub fn init_head(o: &mut SimObject) {
    o.visible = false;
    o.scale = 4.0;
    o.hitbox = HEAD_HITBOX;
    o.set_var("hbInit", 1.0);
    o.set_var("realInteract", INTERACT_BOUNCE_TOP as f64);
    phys::set_physics(o, 60.0, -400.0, 0.0, 1000.0, 1000.0, 200.0);
    o.set_var("fallHeight", 5000.0);
    o.set_var("scaleY", 4.0);
    anim::init(o, 0, ANIMS[0]);
}

pub fn init_body(o: &mut SimObject) {
    o.visible = true;
    o.scale = 4.0;
    o.hitbox = BODY_HITBOX;
    o.set_var("hbInit", 1.0);
    o.set_var("realInteract", INTERACT_BOUNCE_TOP as f64);
    phys::set_physics(o, 0.0, -400.0, 0.0, 1000.0, 1000.0, 200.0);
    o.set_var("scaleY", 4.0);
    anim::init(o, 0, ANIMS[0]);
}

fn seg(o: &SimObject, i: usize) -> [f64; 5] {
    let k = |s: &str| o.var(&format!("s{i}{s}"));
    [k("x"), k("y"), k("z"), k("p"), k("w")]
}

fn set_seg(o: &mut SimObject, i: usize, s: [f64; 5]) {
    for (n, v) in ["x", "y", "z", "p", "w"].iter().zip(s) {
        o.set_var(&format!("s{i}{n}"), v);
    }
}

/// `wiggler_init_segments`.
fn init_segments(m: &mut MarioRuntime, id: ObjId) {
    let (pos, pitch, yaw) = (m.objects[id].pos, m.objects[id].var("facePitch"), m.objects[id].face_yaw);
    for i in 0..4 {
        set_seg(&mut m.objects[id], i, [pos[0], pos[1], pos[2], pitch, yaw]);
    }
    m.objects[id].set_var("animFrame", -1.0);
    m.objects[id].set_var("animAssist", -1.0);
    for i in 1..=3 {
        let b = spawn_child(m, id, K::WigglerBody, Some("actors/wiggler_body/wiggler_body_geo.glb"), 0.0);
        let o = &mut m.objects[b];
        o.set_var("part", i as f64);
        o.set_var("animFrame", ((23 * i) % 26) as f64 - 1.0);
        o.set_var("animAssist", ((23 * i) % 26) as f64 - 1.0);
    }
    let o = &mut m.objects[id];
    o.action = ACT_WALK;
    o.visible = true;
    o.hitbox.health = 4.0;
}

/// `wiggler_update_segments`.
fn update_segments(o: &mut SimObject) {
    let length = 35.0 * o.scale;
    for i in 1..=3 {
        let prev = seg(o, i - 1);
        let mut cur = seg(o, i);
        let dx = cur[0] - prev[0];
        let dy = cur[1] - prev[1];
        let dz = cur[2] - prev[2];
        let dyaw = clamp_f(to_s16(atan2s(-dz, -dx) as f64 - prev[4]) as f64, -0x2000 as f64, 0x2000 as f64);
        cur[4] = prev[4] + dyaw;
        let dxz = (dx * dx + dz * dz).sqrt();
        let dpitch = clamp_f(to_s16(atan2s(dxz, dy) as f64 - prev[3]) as f64, -0x2000 as f64, 0x2000 as f64);
        cur[3] = prev[3] + dpitch;
        cur[1] = length * sins(cur[3]) + prev[1];
        let dxz = length * coss(cur[3]);
        cur[0] = prev[0] - dxz * sins(cur[4]);
        cur[2] = prev[2] - dxz * coss(cur[4]);
        set_seg(o, i, cur);
    }
}

/// `obj_set_knockback_action` for the Wiggler (the handler's action is overwritten by `ACT_KNOCKBACK`).
fn set_knockback(m: &mut MarioRuntime, id: ObjId, attack: i32) {
    let mpos = m.pos;
    let o = &mut m.objects[id];
    if matches!(attack, ATTACK_KICK_OR_TRIP | ATTACK_FAST_ATTACK) {
        o.forward_vel = 20.0;
        o.vel[1] = 50.0;
    } else {
        o.forward_vel = 50.0;
        o.vel[1] = 30.0;
    }
    o.move_yaw = atan2s(o.pos[2] - mpos[2], o.pos[0] - mpos[0]) as f64;
}

/// `obj_bounce_off_walls_edges_objects`: the new target yaw, if the wiggler bounced.
fn bounce_off_walls_edges(o: &SimObject) -> Option<f64> {
    let f = phys::flags(o);
    if f & phys::MOVE_HIT_WALL != 0 {
        let wall = o.var("wallAngle");
        Some(to_s16(wall - (to_s16(o.move_yaw) as f64 - to_s16(wall) as f64) + 0x8000 as f64) as f64)
    } else if f & phys::MOVE_HIT_EDGE != 0 {
        Some(to_s16(o.move_yaw + 0x8000 as f64) as f64)
    } else {
        None
    }
}

fn walk(m: &mut MarioRuntime, id: ObjId) {
    let mario_y = m.pos[1];
    let o = &mut m.objects[id];
    o.set_var("walkAnimSpeed", 0.06 * o.forward_vel);
    if o.var("textStatus") < 2.0 {
        o.set_var("textStatus", 1.0);
        if mario_y < o.pos[1] || update_dialog(m, id, 150) {
            m.objects[id].set_var("textStatus", 2.0);
        }
        return;
    }
    let health = o.hitbox.health.max(1.0) as usize;
    forward_vel_approach(o, SPEEDS[(health - 1).min(3)], 1.0);
    if o.var("awayTimer") != 0.0 {
        o.set_var("awayTimer", o.var("awayTimer") - 1.0);
    } else {
        if o.distance_to_mario >= 25000.0 {
            o.set_var("targetYaw", o.angle_to_mario);
        }
        if let Some(t) = bounce_off_walls_edges(o) {
            o.set_var("targetYaw", t);
            o.set_var("awayTimer", random_linear_offset(30.0, 30.0));
        } else if o.hitbox.health < 4.0 {
            o.set_var("targetYaw", o.angle_to_mario);
        } else if o.var("turnTimer") != 0.0 {
            o.set_var("turnTimer", o.var("turnTimer") - 1.0);
        } else {
            let sign = if random_float() < 0.5 { 1.0 } else { -1.0 };
            o.set_var("targetYaw", to_s16(o.move_yaw + 0x4000 as f64 * sign) as f64);
            o.set_var("turnTimer", random_linear_offset(30.0, 50.0));
        }
    }
    let turn = to_s16((30.0 * o.forward_vel).trunc()) as f64;
    let target = o.var("targetYaw");
    rotate_yaw_toward(o, target, turn);
    let my = o.move_yaw;
    face_yaw_approach(o, my, 2.0 * turn);
    let pitch = approach_s16_symmetric(o.var("facePitch"), 0.0, 0x320 as f64);
    o.set_var("facePitch", pitch);

    // For the first two seconds of walking, stay invulnerable.
    let action = o.action;
    if o.timer < 60.0 {
        obj_check_attacks(o, HEAD_HITBOX, action, false);
        return;
    }
    set_hitbox(o, HEAD_HITBOX);
    let status = o.interact_status;
    let mut attack = 0;
    if status & INT_STATUS_INTERACTED != 0 {
        if status & INT_STATUS_ATTACKED_MARIO != 0 {
            if o.action != action {
                o.action = action;
                o.timer = 0.0;
            }
        } else {
            attack = (status & 0xFF) as i32;
        }
    }
    o.interact_status = 0;
    if attack != 0 {
        match attack {
            ATTACK_FROM_ABOVE | ATTACK_GROUND_POUND_OR_TWIRL => {
                // wiggler_jumped_on_attack_handler
                o.action = ACT_JUMPED_ON;
                o.forward_vel = 0.0;
                o.vel[1] = 0.0;
                o.set_var("squishSpeed", 0.4);
            }
            _ => {
                set_knockback(m, id, attack);
                m.objects[id].action = ACT_KNOCKBACK;
            }
        }
        let o = &mut m.objects[id];
        o.set_var("awayTimer", 0.0);
        o.set_var("walkAnimSpeed", 0.0);
    }
}

fn jumped_on(m: &mut MarioRuntime, id: ObjId) {
    let mut finished_dialog = None;
    {
        let o = &mut m.objects[id];
        let mut squish = o.var("squishSpeed");
        let mut sy = o.var("scaleY");
        if approach_f32_ptr(&mut squish, 0.0, 0.05) {
            approach_f32_ptr(&mut sy, 4.0, 0.2);
        } else {
            sy -= squish;
        }
        o.set_var("squishSpeed", squish);
        o.set_var("scaleY", sy);
        if sy >= 4.0 {
            if o.timer > 30.0 {
                finished_dialog = Some(ATTACK_TEXT[((o.hitbox.health as i32) - 2).clamp(0, 2) as usize]);
            }
        } else {
            o.timer = 0.0;
        }
    }
    if let Some(dialog) = finished_dialog {
        if update_dialog(m, id, dialog) {
            let o = &mut m.objects[id];
            // Because we don't want the wiggler to disappear after being defeated, its health stays at 1.
            o.hitbox.health -= 1.0;
            if o.hitbox.health == 1.0 {
                o.action = ACT_SHRINK;
                become_intangible_r2a(o);
            } else {
                o.action = ACT_WALK;
                o.move_yaw = o.face_yaw;
                if o.hitbox.health == 2.0 {
                    o.forward_vel = 10.0;
                    o.vel[1] = 70.0;
                }
            }
        }
    }
    let o = &mut m.objects[id];
    let a = o.action;
    obj_check_attacks(o, HEAD_HITBOX, a, false);
}

fn become_intangible_r2a(o: &mut SimObject) {
    crate::behaviors::g1::become_intangible(o);
}

fn knockback(o: &mut SimObject) {
    if o.vel[1] > 0.0 {
        let p = o.var("facePitch") - o.vel[1] * 30.0;
        o.set_var("facePitch", to_s16(p) as f64);
    } else {
        let p = approach_s16_symmetric(o.var("facePitch"), 0.0, 0x190 as f64);
        o.set_var("facePitch", p);
    }
    if forward_vel_approach(o, 0.0, 1.0) && to_s16(o.var("facePitch")) == 0 {
        o.action = ACT_WALK;
        o.move_yaw = o.face_yaw;
    }
    let a = o.action;
    obj_check_attacks(o, HEAD_HITBOX, a, false);
}

fn shrink(m: &mut MarioRuntime, id: ObjId) {
    let o = &mut m.objects[id];
    if o.timer >= 20.0 {
        let mut s = o.scale;
        let done = approach_f32_ptr(&mut s, 1.0, 0.1);
        o.scale = s;
        o.set_var("scaleY", s);
        if done {
            o.action = ACT_FALL_THROUGH_FLOOR;
            spawn_default_star(m, id, [0.0, 2048.0, 0.0]);
        }
    }
}

fn fall_through_floor(o: &mut SimObject) {
    if o.timer == 60.0 {
        o.set_var("fallHeight", 1700.0);
    } else if o.timer > 60.0 {
        if o.pos[1] < o.var("fallHeight") {
            o.action = ACT_WALK;
        } else {
            o.set_var("facePitch", -(atan2s(o.forward_vel, o.vel[1]) as f64));
        }
        cur_obj_move_using_fvel_and_gravity(o, o.var("grav"));
    }
}

/// `treat_far_home_as_mario(threshold)`.
fn treat_far_home_as_mario(o: &mut SimObject, mario: Vec3, threshold: f64) {
    let (dx, dy, dz) = (o.home[0] - o.pos[0], o.home[1] - o.pos[1], o.home[2] - o.pos[2]);
    if (dx * dx + dy * dy + dz * dz).sqrt() > threshold {
        o.angle_to_mario = atan2s(dz, dx) as f64;
        o.distance_to_mario = 25000.0;
    } else {
        let (dx, dy, dz) = (o.home[0] - mario[0], o.home[1] - mario[1], o.home[2] - mario[2]);
        if (dx * dx + dy * dy + dz * dz).sqrt() > threshold {
            o.distance_to_mario = 20000.0;
        }
    }
}

/// `bhv_wiggler_update`.
pub fn head(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let world = m.world.clone();
        if first_frame(&mut m.objects[id]) {
            // DROP_TO_FLOOR, SET_HOME
            drop_to_floor(&mut m.objects[id], &world);
            let o = &mut m.objects[id];
            o.home = o.pos;
        }
        if m.objects[id].action == ACT_UNINITIALIZED {
            init_segments(m, id);
            return;
        }
        if m.objects[id].action == ACT_FALL_THROUGH_FLOOR {
            fall_through_floor(&mut m.objects[id]);
        } else {
            let mpos = m.pos;
            treat_far_home_as_mario(&mut m.objects[id], mpos, 1200.0);
            // Walking animation
            let speed = m.objects[id].var("walkAnimSpeed");
            let o = &mut m.objects[id];
            anim::init_accel(o, 0, ANIMS[0], speed);
            o.set_var("animAccel", speed);
            if speed == 0.0 {
                reverse_animation(o);
            }
            phys::update_floor_and_walls(o, &world);
            match m.objects[id].action {
                ACT_WALK => walk(m, id),
                ACT_KNOCKBACK => knockback(&mut m.objects[id]),
                ACT_JUMPED_ON => jumped_on(m, id),
                ACT_SHRINK => shrink(m, id),
                ACT_FALL_THROUGH_FLOOR => fall_through_floor(&mut m.objects[id]),
                _ => {}
            }
            phys::move_standard(&mut m.objects[id], &world, -78.0);
        }
        let o = &mut m.objects[id];
        let pitch = o.var("facePitch");
        let s0 = [o.pos[0], o.pos[1], o.pos[2], pitch, o.face_yaw];
        set_seg(o, 0, s0);
        update_segments(o);
        advance_anim(o);
    });
}

/// `bhv_wiggler_body_part_update`.
pub fn body(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let Some(parent) = m.objects[id].link else { return };
        let world = m.world.clone();
        let (pscale, pshrinking, pfall, pspeed, pseg) = {
            let p = &m.objects[parent];
            let part = m.objects[id].var("part") as usize;
            (p.scale, p.action == ACT_SHRINK, p.var("fallHeight"), p.var("walkAnimSpeed"), seg(p, part))
        };
        let o = &mut m.objects[id];
        o.scale = pscale;
        o.set_var("scaleY", pscale);
        o.set_var("facePitch", pseg[3]);
        o.face_yaw = pseg[4];
        // TODO in the decomp: posOffset
        let off = -37.5 * pscale;
        let dy = off * coss(pseg[3]) - off;
        let dxz = off * sins(pseg[3]);
        let dx = dxz * sins(pseg[4]);
        let dz = dxz * coss(pseg[4]);
        o.pos = [pseg[0] + dx, pseg[1] + dy, pseg[2] + dz];
        if o.pos[1] < pfall {
            o.pos[1] += -30.0;
            phys::update_floor_height(o, &world);
            if phys::floor_height(o) > o.pos[1] {
                o.pos[1] = phys::floor_height(o);
            }
        }
        let part = o.var("part") as usize;
        let y = o.pos[1];
        // segment->posY = o->oPosY
        m.objects[parent].set_var(&format!("s{part}y"), y);
        let o = &mut m.objects[id];
        // Inherit the walking animation speed from the wiggler.
        anim::init_accel(o, 0, ANIMS[0], pspeed);
        o.set_var("animAccel", pspeed);
        if pspeed == 0.0 {
            reverse_animation(o);
        }
        if pshrinking {
            crate::behaviors::g1::become_intangible(o);
        } else {
            crate::behaviors::g1::become_tangible(o);
            let a = o.action;
            obj_check_attacks(o, BODY_HITBOX, a, false);
        }
        advance_anim(&mut m.objects[id]);
    });
}
