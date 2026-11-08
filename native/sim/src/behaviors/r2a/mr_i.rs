//! Mr. I (`bhvMrI`, BBH / LLL / HMC): the staring eyeball. Port of `src/game/behaviors/mr_i.inc.c`
//! (`bhv_mr_i_loop`, `bhv_mr_i_body_loop`, `bhv_mr_i_particle_loop`).
//!
//! It cannot be hurt by attacks; Mario kills it by running circles around it (the turn it makes to
//! follow him accumulates past 65536, then it spins and dies). The Mr. I with `BPARAM2(1)` in the BBH
//! secret room is twice as big, lasts longer and spawns the act-6 star at (1370, 2000, -320).
//!
//! Left out: the iris texture frames (`oAnimState`), the purple marble burst when a particle hits,
//! sounds, `ACTIVE_FLAG_IN_DIFFERENT_ROOM`.
use super::helpers::*;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use ObjectBehaviorKind as K;

/// sMrIHitbox.
pub const HITBOX: ObjectHitbox = hitbox(INTERACT_DAMAGE, 0.0, 2.0, 2.0, 5.0, 80.0, 150.0, 0.0, 0.0);

pub fn init(o: &mut SimObject) {
    o.visible = true;
    o.billboard = true;
    o.set_var("realInteract", INTERACT_DAMAGE as f64);
    o.hitbox = HITBOX;
    o.set_var("hbInit", 1.0);
    o.set_var("spawnedBody", 0.0);
}

/// `(s16)` helper for fields stored as f64.
fn s16(v: f64) -> f64 {
    to_s16(v) as f64
}

/// `obj_turn_toward_object(o, gMarioObject, O_MOVE_ANGLE_YAW_INDEX, amount)`.
fn turn_yaw_toward_mario(o: &mut SimObject, mario: Vec3, amount: f64) {
    let target = atan2s(mario[2] - o.pos[2], mario[0] - o.pos[0]) as f64;
    o.move_yaw = approach_s16_symmetric(o.move_yaw, target, amount);
}

/// `obj_turn_toward_object(o, gMarioObject, O_MOVE_ANGLE_PITCH_INDEX, amount)`.
fn turn_pitch_toward_mario(o: &mut SimObject, mario: Vec3, amount: f64) {
    let a = js_hypot(mario[0] - o.pos[0], mario[2] - o.pos[2]);
    let target = atan2s(a, -mario[1] + o.pos[1]) as f64;
    let pitch = approach_s16_symmetric(o.var("movePitch"), target, amount);
    o.set_var("movePitch", pitch);
}

fn spawn_particle(m: &mut MarioRuntime, id: ObjId) {
    let (scale, yaw) = (m.objects[id].scale, m.objects[id].move_yaw);
    let p = spawn_child(m, id, K::MrIParticle, Some("actors/yellow_sphere_small/yellow_sphere_geo.glb"), 0.0);
    let o = &mut m.objects[p];
    o.pos[1] += 50.0 * scale;
    o.pos[0] += sins(yaw) * 90.0 * scale;
    o.pos[2] += coss(yaw) * 90.0 * scale;
    m.objects.sound(m.objects[p].pos, "OBJ_STOMPED");
}

/// `bhv_mr_i_loop`.
pub fn mr_i(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW | F_MOVE_XZ, |m, id| {
        if m.objects[id].var("spawnedBody") == 0.0 {
            // SPAWN_CHILD(MODEL_MR_I_IRIS, bhvMrIBody)
            m.objects[id].set_var("spawnedBody", 1.0);
            spawn_child(m, id, K::MrIBody, Some("actors/mr_i_iris/mr_i_iris_geo.glb"), 0.0);
        }
        set_hitbox(&mut m.objects[id], HITBOX);
        match m.objects[id].action {
            0 => act0(&mut m.objects[id]),
            1 => act1(m, id),
            2 => act2(m, id),
            _ => act3(m, id),
        }
        let o = &mut m.objects[id];
        if o.action != 3 && o.distance_to_mario > 3000.0 {
            o.action = 0;
        }
        o.interact_status = 0;
    });
}

fn act0(o: &mut SimObject) {
    o.move_yaw = 0.0;
    o.face_yaw = 0.0;
    o.set_var("movePitch", 0.0);
    // cur_obj_scale(oBhvParams2ndByte + 1)
    o.scale = (((o.param as u32) >> 16 & 0xFF) + 1) as f64;
    if o.timer == 0.0 {
        o.pos = o.home;
    }
    if o.distance_to_mario < 1500.0 {
        o.action = 1;
    }
}

fn act1(m: &mut MarioRuntime, id: ObjId) {
    let mario = m.pos;
    let mario_face = m.face_angle[1];
    let o = &mut m.objects[id];
    let to_mario = atan2s(mario[2] - o.pos[2], mario[0] - o.pos[0]) as f64;
    let d_mario = abs_angle_diff(o.move_yaw, to_mario);
    let d_face = abs_angle_diff(o.move_yaw, mario_face);
    let mut spawn = false;
    if o.timer == 0.0 {
        crate::behaviors::g1::become_tangible(o);
        o.set_var("movePitch", 0.0);
        o.set_var("u104", 30.0);
        let r = (random_float() * 20.0).trunc();
        o.set_var("u108", r);
        o.set_var("avYaw", if (r as i32) & 1 != 0 { -256.0 } else { 256.0 });
    }
    if d_mario < 1024.0 && d_face > 0x4000 as f64 {
        if o.distance_to_mario < 700.0 {
            o.action = 2;
        } else {
            o.set_var("u104", o.var("u104") + 1.0);
        }
    } else {
        o.move_yaw = s16(o.move_yaw + o.var("avYaw"));
        o.set_var("u104", 30.0);
    }
    if o.var("u104") == o.var("u108") + 60.0 {
        o.set_var("u110", 1.0);
    }
    if o.var("u104") > o.var("u108") + 80.0 {
        o.set_var("u104", 0.0);
        o.set_var("u108", (random_float() * 80.0).trunc());
        spawn = true;
    }
    if spawn {
        spawn_particle(m, id);
    }
}

fn act2(m: &mut MarioRuntime, id: ObjId) {
    let mario = m.pos;
    let mut spawn = false;
    {
        let o = &mut m.objects[id];
        let before = s16(o.move_yaw);
        if o.timer == 0.0 {
            o.set_var("uF4", if o.param as u32 >> 16 & 0xFF != 0 { 200.0 } else { 120.0 });
            o.set_var("uFC", 0.0);
            o.set_var("u100", 0.0);
            o.set_var("u104", 0.0);
        }
        turn_yaw_toward_mario(o, mario, 0x800 as f64);
        turn_pitch_toward_mario(o, mario, 0x400 as f64);
        let d = s16(before - s16(o.move_yaw));
        if d == 0.0 {
            o.set_var("uFC", 0.0);
            o.set_var("u100", 0.0);
        } else if d > 0.0 {
            if o.var("u100") > 0.0 {
                o.set_var("uFC", o.var("uFC") + d);
            } else {
                o.set_var("uFC", 0.0);
            }
            o.set_var("u100", 1.0);
        } else {
            if o.var("u100") < 0.0 {
                o.set_var("uFC", o.var("uFC") - d);
            } else {
                o.set_var("uFC", 0.0);
            }
            o.set_var("u100", -1.0);
        }
        if o.var("uFC") == 0.0 {
            o.set_var("uF4", 120.0);
        }
        if o.var("uFC") > 65536.0 {
            o.action = 3;
        }
        o.set_var("uF4", o.var("uF4") - 1.0);
        if o.var("uF4") == 0.0 {
            o.set_var("uF4", 120.0);
            o.set_var("uFC", 0.0);
        }
        if o.var("uFC") < 5000.0 {
            if o.var("u104") == o.var("u108") {
                o.set_var("u110", 1.0);
            }
            if o.var("u104") == o.var("u108") + 20.0 {
                spawn = true;
                o.set_var("u104", 0.0);
                o.set_var("u108", (random_float() * 50.0 + 50.0).trunc());
            }
            o.set_var("u104", o.var("u104") + 1.0);
        } else {
            o.set_var("u104", 0.0);
            o.set_var("u108", (random_float() * 50.0 + 50.0).trunc());
        }
        if o.distance_to_mario > 800.0 {
            o.action = 1;
        }
    }
    if spawn {
        spawn_particle(m, id);
    }
}

/// `cur_obj_shake_y`.
fn shake_y(o: &mut SimObject, amount: f64) {
    if (o.timer as i64) % 2 == 0 {
        o.pos[1] += amount;
    } else {
        o.pos[1] -= amount;
    }
}

fn act3(m: &mut MarioRuntime, id: ObjId) {
    let big = (m.objects[id].param as u32 >> 16 & 0xFF) != 0;
    let scale_factor = if big { 2.0 } else { 1.0 };
    let (mut star, mut blue) = (false, None);
    {
        let o = &mut m.objects[id];
        let sp34 = if o.var("u100") < 0.0 { 0x1000 as f64 } else { -0x1000 as f64 };
        let sp2c = (o.timer + 1.0) / 96.0;
        if o.timer < 64.0 {
            let prev = s16(o.move_yaw);
            o.move_yaw = s16((o.move_yaw + sp34 * coss(0x4000 as f64 * sp2c)).trunc());
            let _ = prev;
            o.set_var("movePitch", ((1.0 - coss(0x4000 as f64 * sp2c)) * -0x4000 as f64).trunc());
            shake_y(o, 4.0);
        } else if o.timer < 96.0 {
            let sp30 = (o.timer - 63.0) / 32.0;
            o.move_yaw = s16((o.move_yaw + sp34 * coss(0x4000 as f64 * sp2c)).trunc());
            o.set_var("movePitch", ((1.0 - coss(0x4000 as f64 * sp2c)) * -0x4000 as f64).trunc());
            shake_y(o, ((1.0 - sp30) * 4.0).trunc());
            o.scale = (coss(0x4000 as f64 * sp30) * 0.4 + 0.6) * scale_factor;
        } else if o.timer < 104.0 {
            // do nothing
        } else if o.timer < 168.0 {
            if o.timer == 104.0 {
                crate::behaviors::g1::become_intangible(o);
                o.set_var("scaleLeft", scale_factor * 0.6);
                if big {
                    o.pos[1] += 100.0;
                    star = true;
                } else if o.hitbox.num_loot_coins >= 5.0 {
                    o.hitbox.num_loot_coins -= 5.0;
                    blue = Some(o.pos);
                }
            }
            let mut s = o.var("scaleLeft") - 0.2 * scale_factor;
            if s < 0.0 {
                s = 0.0;
            }
            o.set_var("scaleLeft", s);
            o.scale = s;
        } else {
            mark_for_deletion(o);
        }
    }
    if star {
        spawn_default_star(m, id, [1370.0, 2000.0, -320.0]);
        mark_for_deletion(&mut m.objects[id]);
    }
    if let Some(pos) = blue {
        m.objects.spawn_coin(pos, true, 5.0);
    }
}

pub fn init_body(o: &mut SimObject) {
    o.visible = true;
}

/// `bhv_mr_i_body_loop`: the iris hovers `100 * scale` in front of the eye along its (pitch, yaw).
pub fn body(id: ObjId, m: &mut MarioRuntime) {
    let Some(parent) = m.objects[id].link else { return };
    if !m.objects[parent].active {
        mark_for_deletion(&mut m.objects[id]);
        return;
    }
    let (ppos, pyaw, ppitch, pscale, pvisible) = {
        let p = &m.objects[parent];
        (p.pos, p.move_yaw, p.var("movePitch"), p.scale, p.visible)
    };
    let o = &mut m.objects[id];
    o.visible = pvisible;
    o.scale = pscale;
    let dist = pscale * 100.0;
    // obj_build_transform_from_pos_and_angle + obj_translate_local: the third row of the zxy matrix is the
    // local +Z axis, (cos(pitch) sin(yaw), -sin(pitch), cos(pitch) cos(yaw)).
    let (sx, cx) = (sins(ppitch), coss(ppitch));
    o.pos = [ppos[0] + dist * cx * sins(pyaw), ppos[1] - dist * sx, ppos[2] + dist * cx * coss(pyaw)];
    o.move_yaw = pyaw;
    o.face_yaw = pyaw;
    o.set_var("facePitch", ppitch);
    o.graph_y_offset = pscale * 100.0;
    let _ = phys::MOVE_LANDED;
}

pub fn init_particle(o: &mut SimObject) {
    o.visible = true;
    o.billboard = true;
    o.scale = 3.0;
    o.hitbox = hitbox(INTERACT_DAMAGE, 0.0, 1.0, 0.0, 0.0, 50.0, 50.0, 50.0, 50.0);
    o.set_var("realInteract", INTERACT_DAMAGE as f64);
    o.set_var("hbInit", 1.0);
    phys::set_physics(o, 30.0, 0.0, 0.0, 0.0, 0.0, 0.0);
}

/// `bhv_mr_i_particle_loop` (`mr_i_piranha_particle_act_0` / `_act_1`).
pub fn particle(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_MOVE_XZ, |m, id| {
        let world = m.world.clone();
        let o = &mut m.objects[id];
        if o.action == 0 {
            o.scale = 3.0;
            o.forward_vel = 20.0;
            phys::update_floor_and_walls(o, &world);
            if o.interact_status & INT_STATUS_INTERACTED != 0 {
                o.action = 1;
            } else if o.timer > 100.0 || phys::flags(o) & phys::MOVE_HIT_WALL != 0 {
                mark_for_deletion(o);
            }
        } else {
            mark_for_deletion(o);
        }
    });
}
