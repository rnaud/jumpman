//! Big Boo's Haunt furniture that bites back: the Mad Piano, the Haunted Chair and the Flying
//! Bookend (+ its spawner). Ports of `mad_piano.inc.c`, `haunted_chair.inc.c` and the bookend
//! half of `flying_bookend_switch.inc.c` (`bhv_flying_bookend_loop`, `bhv_bookend_spawn_loop`).
//!
//! Not ported from that file: the haunted bookshelf puzzle (`bhvHauntedBookshelfManager`,
//! `bhvBookSwitch`, `bhvHauntedBookshelf`) and the coffins. Sounds follow the originals where the
//! sound table has them; `ACTIVE_FLAG_IN_DIFFERENT_ROOM` (the room culling of `bhv_init_room`) is not
//! modelled, so the furniture is awake in every room.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::{become_intangible, become_tangible, INTERACT_HIT_FROM_BELOW};
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys::{self, MASK_ON_GROUND, MOVE_HIT_WALL};
use crate::behaviors::r2a::helpers::{hitbox, obj_check_attacks};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;

pub const BOOK_MODEL: &str = "actors/book/bookend_geo.glb";
pub const BOOKEND_MODEL: &str = "actors/bookend/bookend_part_geo.glb";

// ---------------------------------------------------------------------------------------------
// Mad Piano
// ---------------------------------------------------------------------------------------------

/// `mad_piano_seg5_anims_05009B14`: 0 = idle (100 frames), 1 = chomping (15).
pub const PIANO_ANIMS: [AnimSpec; 2] = [spec(0, 0, 0x64, false), spec(0, 0, 0x0F, false)];
pub const PIANO_ANIM_NAMES: [&str; 2] = ["mad_piano_seg5_anim_05009A04", "mad_piano_seg5_anim_05009AFC"];

/// sMadPianoHitbox.
pub const PIANO_HITBOX: ObjectHitbox = hitbox(INTERACT_MR_BLIZZARD, 0.0, 3.0, 99.0, 0.0, 200.0, 150.0, 200.0, 150.0);

const PIANO_WAIT: i32 = 0;
const PIANO_ATTACK: i32 = 1;

pub fn init_piano(o: &mut SimObject) {
    o.visible = true;
    // ADD_INT(oMoveAngleYaw, 0x4000)
    o.move_yaw = to_s16(o.move_yaw + 0x4000 as f64) as f64;
    phys::set_physics(o, 40.0, 0.0, -50.0, 1000.0, 1000.0, 200.0);
}

/// The first frame: `DROP_TO_FLOOR`, `SET_HOME`.
fn drop_and_home(m: &mut MarioRuntime, id: ObjId) {
    let world = m.world.clone();
    let o = &mut m.objects[id];
    if first_frame(o) {
        drop_to_floor(o, &world);
        o.home = o.pos;
        anim::init(o, 0, PIANO_ANIMS[0]);
    }
}

pub fn piano(id: ObjId, m: &mut MarioRuntime) {
    drop_and_home(m, id);
    run(m, id, 0, |m, id| {
        let world = m.world.clone();
        let fvel = m.forward_vel;
        {
            let o = &mut m.objects[id];
            o.face_yaw = to_s16(o.move_yaw - 0x4000 as f64) as f64;
        }
        match m.objects[id].action {
            PIANO_WAIT => {
                let o = &mut m.objects[id];
                anim::init(o, 0, PIANO_ANIMS[0]);
                if o.distance_to_mario < 500.0 {
                    if o.timer > 20.0 && fvel > 10.0 {
                        o.action = PIANO_ATTACK;
                        become_tangible(o);
                    }
                } else {
                    o.timer = 0.0;
                }
                push_cylinder(m, id, 280.0, 150.0);
            }
            _ => {
                {
                    let o = &mut m.objects[id];
                    phys::update_floor_and_walls(o, &world);
                    anim::init(o, 1, PIANO_ANIMS[1]);
                    if o.distance_to_mario < 500.0 {
                        o.timer = 0.0;
                    }
                    if o.timer > 80.0 && anim::near_end(o) {
                        o.action = PIANO_WAIT;
                        o.forward_vel = 0.0;
                        become_intangible(o);
                    } else {
                        let dx = o.pos[0] - o.home[0];
                        let dz = o.pos[2] - o.home[2];
                        let dist = (dx * dx + dz * dz).sqrt();
                        if dist > 400.0 {
                            let k = 400.0 / dist;
                            o.pos[0] = o.home[0] + dx * k;
                            o.pos[2] = o.home[2] + dz * k;
                        }
                        let target = o.angle_to_mario;
                        rotate_yaw_toward(o, target, 400.0);
                        o.forward_vel = 5.0;
                    }
                }
                let action = m.objects[id].action;
                obj_check_attacks(&mut m.objects[id], PIANO_HITBOX, action, true);
                phys::move_standard(&mut m.objects[id], &world, 78.0);
            }
        }
        anim::advance(&mut m.objects[id]);
    });
}

/// `cur_obj_push_mario_away_from_cylinder(radius, extentY)`.
fn push_cylinder(m: &mut MarioRuntime, id: ObjId, radius: f64, extent_y: f64) {
    if (m.pos[1] - m.objects[id].pos[1]).abs() < extent_y {
        push_mario_away(m, id, radius);
    }
}

// ---------------------------------------------------------------------------------------------
// Haunted Chair
// ---------------------------------------------------------------------------------------------

/// sHauntedChairHitbox.
pub const CHAIR_HITBOX: ObjectHitbox = hitbox(INTERACT_MR_BLIZZARD, 0.0, 2.0, 0.0, 0.0, 50.0, 50.0, 50.0, 50.0);

pub fn init_chair(o: &mut SimObject) {
    o.visible = true;
    phys::set_physics(o, 40.0, 0.0, -50.0, 1000.0, 1000.0, 200.0);
}

/// `oscillate_toward` (obj_behaviors_2.c) on an angle and its velocity, both stored as vars.
fn oscillate_toward(o: &mut SimObject, value: &str, vel: &str, target: f64, vel_close_to_zero: f64, accel: f64, slowdown: f64) -> bool {
    let start = o.var(value);
    let v = o.var(vel);
    let mut cur = start + v.trunc();
    let mut accel = accel;
    if cur == target || ((cur - target) * (start - target) < 0.0 && v > -vel_close_to_zero && v < vel_close_to_zero) {
        o.set_var(value, target);
        o.set_var(vel, 0.0);
        return true;
    }
    if cur >= target {
        accel = -accel;
    }
    if v * accel < 0.0 {
        accel *= slowdown;
    }
    o.set_var(vel, v + accel);
    cur = cur.max(-1.0e9);
    o.set_var(value, cur);
    false
}

/// `obj_turn_pitch_toward_mario(targetOffsetY, turnAmount)`: the target pitch (`turnAmount` 0 leaves
/// `oMoveAnglePitch` alone, bigger values approach it).
fn turn_pitch_toward_mario(o: &mut SimObject, mario: Vec3, offset_y: f64, turn: f64) -> f64 {
    let lateral = js_hypot(mario[0] - o.pos[0], mario[2] - o.pos[2]);
    let target = atan2s(lateral, -mario[1] + (o.pos[1] - offset_y)) as f64;
    let pitch = approach_s16_symmetric(o.var("movePitch"), target, turn);
    o.set_var("movePitch", pitch);
    target
}

/// `obj_compute_vel_from_move_pitch(speed)`.
fn vel_from_move_pitch(o: &mut SimObject, speed: f64) {
    let pitch = o.var("movePitch");
    o.forward_vel = speed * coss(pitch);
    o.vel[1] = speed * -sins(pitch);
}

/// `cur_obj_spin_all_dimensions(arg0, arg1)`: a chair that lies on its side rocks around its home.
fn spin_all_dimensions(o: &mut SimObject, arg0: f64, arg1: f64) {
    if o.forward_vel != 0.0 {
        return;
    }
    let pitch = o.var("facePitch");
    let roll = o.var("faceRoll");
    let (mut v24, mut v20, mut v1c) = (0.0, 0.0, 0.0);
    if phys::flags(o) & phys::MOVE_IN_AIR != 0 {
        v20 = 50.0;
    } else {
        if pitch < 0.0 {
            v1c = -arg0;
        } else if pitch > 0.0 {
            v1c = arg0;
        }
        if roll < 0.0 {
            v24 = -arg1;
        } else if roll > 0.0 {
            v24 = arg1;
        }
    }
    let (c, s) = (coss(pitch), sins(pitch));
    let mut v08 = v1c * c + v20 * s;
    let mut v0c = v20 * c - v1c * s;
    let (c, s) = (coss(roll), sins(roll));
    let v04 = v24 * c + v0c * s;
    v0c = v0c * c - v24 * s;
    let (c, s) = (coss(o.face_yaw), sins(o.face_yaw));
    let v10 = v04 * c - v08 * s;
    v08 = v08 * c + v04 * s;
    let v04 = v24 * c - v1c * s;
    let v00 = v1c * c + v24 * s;
    o.pos[0] = o.home[0] - v04 + v10;
    o.graph_y_offset = v20 - v0c;
    o.pos[2] = o.home[2] + v00 - v08;
}

pub fn chair(id: ObjId, m: &mut MarioRuntime) {
    drop_and_home_chair(m, id);
    run(m, id, 0, |m, id| {
        let world = m.world.clone();
        let mario = m.pos;
        match m.objects[id].action {
            0 => chair_act0(m, id),
            _ => {
                {
                    let o = &mut m.objects[id];
                    phys::update_floor_and_walls(o, &world);
                    if o.timer < 70.0 {
                        o.vel[1] = if o.timer < 50.0 { 6.0 } else { 0.0 };
                        oscillate_toward(o, "facePitch", "u8", -4000.0, 200.0, 20.0, 2.0);
                        oscillate_toward(o, "faceRoll", "uc", 0.0, 0.0, 20.0, 1.0);
                    } else if o.var("f4") != 0.0 {
                        let f4 = o.var("f4") - 1.0;
                        o.set_var("f4", f4);
                        if f4 == 0.0 {
                            let t = turn_pitch_toward_mario(o, mario, 120.0, 0.0);
                            o.set_var("movePitch", t);
                            o.move_yaw = o.angle_to_mario;
                            vel_from_move_pitch(o, 50.0);
                            let pos = o.pos;
                            m.objects.sound(pos, "GENERAL_HAUNTED_CHAIR");
                        } else if f4 > 20.0 {
                            let o = &mut m.objects[id];
                            o.face_yaw = to_s16(o.face_yaw + 10000.0) as f64;
                        }
                    } else if phys::flags(o) & (MASK_ON_GROUND | MOVE_HIT_WALL) != 0 && o.hitbox.health <= 0.0 {
                        o.deactivate(-1.0);
                    }
                }
                let action = m.objects[id].action;
                obj_check_attacks(&mut m.objects[id], CHAIR_HITBOX, action, true);
                phys::move_standard(&mut m.objects[id], &world, 78.0);
            }
        }
        if m.objects[id].active {
            spin_all_dimensions(&mut m.objects[id], 30.0, 30.0);
        }
    });
}

/// `DROP_TO_FLOOR`, `SET_HOME`, `bhv_haunted_chair_init`: the nearest Mad Piano within 300 becomes
/// the chair's `parentObj` (the chair then tips away from it); without one the chair is a
/// stand-alone trap that waits for Mario.
fn drop_and_home_chair(m: &mut MarioRuntime, id: ObjId) {
    let world = m.world.clone();
    if !first_frame(&mut m.objects[id]) {
        return;
    }
    let (pos, mut best) = {
        let o = &mut m.objects[id];
        drop_to_floor(o, &world);
        o.home = o.pos;
        (o.pos, None::<(ObjId, f64)>)
    };
    for pid in m.objects.ids() {
        let p = &m.objects[pid];
        if p.behavior == K::MadPiano && p.active {
            let d = dist_3d(&pos, &p.pos);
            if best.map_or(true, |(_, bd)| d < bd) {
                best = Some((pid, d));
            }
        }
    }
    let o = &mut m.objects[id];
    match best {
        Some((pid, d)) if d < 300.0 => o.link = Some(pid),
        _ => o.set_var("f4", 1.0),
    }
}

/// `haunted_chair_act_0`.
fn chair_act0(m: &mut MarioRuntime, id: ObjId) {
    let parent = m.objects[id].link;
    if let Some(pid) = parent {
        let ppos = m.objects[pid].pos;
        let o = &mut m.objects[id];
        if o.var("u104") == 0.0 {
            let lateral = js_hypot(o.pos[0] - ppos[0], o.pos[2] - ppos[2]);
            if lateral < 250.0 {
                let angle_to_parent = atan2s(ppos[2] - o.pos[2], ppos[0] - o.pos[0]) as f64;
                let v = to_s16(angle_to_parent - o.face_yaw + 0x2000 as f64) as f64;
                if (v as i32) & 0x4000 != 0 {
                    o.set_var("ptrRoll", 1.0);
                    o.set_var("u104", if v > 0.0 { 0x4000 as f64 } else { -0x4000 as f64 });
                } else {
                    o.set_var("ptrRoll", 0.0);
                    o.set_var("u104", if v < 0.0 { 0x5000 as f64 } else { -0x4000 as f64 });
                }
                o.set_var("u8", if o.var("u104") < 0.0 { -1500.0 } else { 1500.0 });
            }
        } else {
            let target = o.var("u104");
            let value = if o.var("ptrRoll") != 0.0 { "faceRoll" } else { "facePitch" };
            oscillate_toward(o, value, "u8", target, 4000.0, 20.0, 2.0);
        }
    } else if m.objects[id].var("f4") != 0.0 {
        let o = &mut m.objects[id];
        if o.distance_to_mario < 500.0 {
            o.set_var("f4", o.var("f4") - 1.0);
        }
        o.timer = 0.0;
    } else {
        let (shaking, pos) = {
            let o = &mut m.objects[id];
            if (o.timer as i32) & 8 != 0 {
                let v = if o.var("facePitch") < 0.0 { 4.0 } else { -4.0 };
                o.home[0] -= v;
                o.home[2] -= v;
                let a = (50.0 * v).trunc();
                o.set_var("facePitch", a);
                o.set_var("faceRoll", a);
                (v > 0.0, o.pos)
            } else {
                o.set_var("facePitch", 0.0);
                o.set_var("faceRoll", 0.0);
                (false, o.pos)
            }
        };
        if shaking {
            m.objects.sound(pos, "GENERAL_HAUNTED_CHAIR_MOVE");
        }
        let o = &mut m.objects[id];
        if o.timer > 30.0 {
            o.action = 1;
            o.set_var("u8", 0.0);
            o.set_var("uc", 200.0);
            o.set_var("f4", 40.0);
        }
    }
    push_cylinder(m, id, 80.0, 120.0);
}

// ---------------------------------------------------------------------------------------------
// Flying Bookend
// ---------------------------------------------------------------------------------------------

/// `bookend_seg5_anims_05002540`: 0 = rest (1 frame), 1 = flap (6), 2 = open (30).
pub const BOOKEND_ANIMS: [AnimSpec; 3] = [spec(0, 0, 0x01, false), spec(0, 0, 0x06, false), spec(0, 0, 0x1E, false)];
pub const BOOKEND_ANIM_NAMES: [&str; 3] = ["bookend_seg5_anim_05002528", "bookend_seg5_anim_050023F4", "bookend_seg5_anim_05002510"];

/// sFlyingBookendHitbox.
pub const BOOKEND_HITBOX: ObjectHitbox = hitbox(INTERACT_HIT_FROM_BELOW, 0.0, 2.0, 0.0, -1.0, 60.0, 30.0, 40.0, 30.0);

pub fn init_bookend(o: &mut SimObject) {
    o.visible = true;
    o.scale = 0.7;
    phys::set_physics(o, 60.0, 0.0, -50.0, 1000.0, 1000.0, 200.0);
    phys::set_flags(o, 0);
    anim::init(o, 0, BOOKEND_ANIMS[0]);
}

pub fn bookend(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        let world = m.world.clone();
        let mario = m.pos;
        {
            let o = &mut m.objects[id];
            if o.action == 0 && o.distance_to_mario < 400.0 && crate::behaviors::g1::abs_angle_diff(o.move_yaw, o.angle_to_mario) < 0x3000 as f64 {
                // flying_bookend_act_0: wakes up, swaps to the biting model
                o.action = 1;
                o.set_var("u4", to_s16(o.var("facePitch") + 0x7FFF as f64) as f64);
                o.set_var("u8", to_s16(o.var("faceRoll") - 0x7FFF as f64) as f64);
                o.set_var("partModel", 1.0);
                let pos = o.pos;
                m.objects.sound(pos, "OBJ_DEFAULT_DEATH");
            }
        }
        match m.objects[id].action {
            1 => {
                let o = &mut m.objects[id];
                if approach_f32_ptr_sym(&mut o.forward_vel, 3.0, 1.0) {
                    anim::init(o, 2, BOOKEND_ANIMS[2]);
                    if anim::near_end(o) {
                        o.action = 2;
                        o.forward_vel = 0.0;
                    } else {
                        o.forward_vel = 3.0;
                        if o.timer > 5.0 {
                            let target = o.var("u4");
                            let p = approach_s16_symmetric(o.var("facePitch"), target, 2000.0);
                            o.set_var("facePitch", p);
                            if o.timer >= 10.0 {
                                let target = o.var("u8");
                                let r = approach_s16_symmetric(o.var("faceRoll"), target, 2000.0);
                                o.set_var("faceRoll", r);
                                if o.timer >= 20.0 {
                                    let mut s = o.scale;
                                    crate::behaviors::g1::approach_f32_ptr(&mut s, 3.0, 0.2);
                                    o.scale = s;
                                }
                            }
                        }
                    }
                }
                cur_obj_move_using_fvel_and_gravity(o, 0.0);
            }
            2 => {
                let o = &mut m.objects[id];
                anim::init(o, 1, BOOKEND_ANIMS[1]);
                phys::update_floor_and_walls(o, &world);
                if o.forward_vel == 0.0 {
                    turn_pitch_toward_mario(o, mario, 120.0, 1000.0);
                    o.set_var("facePitch", to_s16(o.var("movePitch") + 0x7FFF as f64) as f64);
                    let target = o.angle_to_mario;
                    rotate_yaw_toward(o, target, 1000.0);
                    if o.timer > 30.0 {
                        vel_from_move_pitch(o, 50.0);
                    }
                }
                phys::move_standard(o, &world, 78.0);
            }
            3 => {
                let o = &mut m.objects[id];
                o.hitbox.damage_or_coin_value = 1.0;
                o.hitbox.num_loot_coins = 0.0;
                if o.timer > 3.0 {
                    o.action = 2;
                    o.forward_vel = 50.0;
                }
                approach_f32_ptr_sym(&mut o.forward_vel, 50.0, 2.0);
                cur_obj_move_using_fvel_and_gravity(o, 0.0);
            }
            _ => {}
        }
        // obj_check_attacks(&sFlyingBookendHitbox, -1): a bookend that hurt Mario gets action -1.
        let o = &mut m.objects[id];
        let attack = obj_check_attacks(o, BOOKEND_HITBOX, -1, false);
        if attack != 0 && o.hitbox.health <= 0.0 {
            // obj_die_if_health_non_positive with oNumLootCoins -1: one blue coin.
            let pos = o.pos;
            o.deactivate(-1.0);
            m.objects.spawn_coin(pos, true, 5.0);
            return;
        }
        let o = &mut m.objects[id];
        if o.action == -1 || phys::flags(o) & (MASK_ON_GROUND | MOVE_HIT_WALL) != 0 {
            // It flew into something (or into Mario): no loot.
            o.hitbox.num_loot_coins = 0.0;
            o.deactivate(-1.0);
            return;
        }
        let o = &mut m.objects[id];
        o.graph_y_offset = 30.0 * o.scale;
        anim::advance(o);
    });
}

/// `approach_f32_ptr` (obj_behaviors_2.c): true once `*value` reached `target`.
fn approach_f32_ptr_sym(value: &mut f64, target: f64, delta: f64) -> bool {
    let d = if *value > target { -delta } else { delta };
    *value += d;
    if (*value - target) * d >= 0.0 {
        *value = target;
        return true;
    }
    false
}

// ---------------------------------------------------------------------------------------------
// Bookend spawner
// ---------------------------------------------------------------------------------------------

pub fn init_spawner(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_bookend_spawn_loop`: every 40 frames, when Mario is within 600 in front of the spawner, a
/// flying bookend is shot (already in action 3).
pub fn spawner(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let o = &m.objects[id];
        let near = o.timer > 40.0 && o.distance_to_mario < 600.0 && crate::behaviors::g1::abs_angle_diff(o.move_yaw, o.angle_to_mario) < 0x2000 as f64;
        if near {
            let b = spawn_child(m, id, K::FlyingBookend, Some(BOOK_MODEL), 0.0);
            m.objects[b].action = 3;
            let pos = m.objects[b].pos;
            m.objects.sound(pos, "OBJ_DEFAULT_DEATH");
            m.objects[id].timer = 0.0;
        }
    });
}
