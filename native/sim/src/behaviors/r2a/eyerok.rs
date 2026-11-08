//! Eyerok (SSL pyramid boss room, `ssl.3`): `bhvEyerokBoss` and the two `bhvEyerokHand`s. Port of
//! `src/game/behaviors/eyerok.inc.c` (`bhv_eyerok_boss_loop`, `bhv_eyerok_hand_loop`).
//!
//! Left out: the camera shake / mist / sounds, the `oAnimState` eye texture (the hand model has
//! one texture), Mario's look-up dialog pose (the NPC dialog modal is used as is).
use super::helpers::*;
use crate::behaviors::g1::anim::{self, AnimSpec};
use crate::behaviors::g1::INTERACT_BOUNCE_TOP;
use crate::behaviors::g2::dyn_collision::{load_object_collision_model, run_platform};
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use ObjectBehaviorKind as K;

// Boss actions (object_constants.h).
pub const BOSS_SLEEP: i32 = 0;
pub const BOSS_WAKE_UP: i32 = 1;
pub const BOSS_SHOW_INTRO_TEXT: i32 = 2;
pub const BOSS_FIGHT: i32 = 3;
pub const BOSS_DIE: i32 = 4;
// Hand actions.
pub const HAND_SLEEP: i32 = 0;
pub const HAND_IDLE: i32 = 1;
pub const HAND_OPEN: i32 = 2;
pub const HAND_SHOW_EYE: i32 = 3;
pub const HAND_CLOSE: i32 = 4;
pub const HAND_RETREAT: i32 = 5;
pub const HAND_TARGET_MARIO: i32 = 6;
pub const HAND_SMASH: i32 = 7;
pub const HAND_FIST_PUSH: i32 = 8;
pub const HAND_FIST_SWEEP: i32 = 9;
pub const HAND_BEGIN_DOUBLE_POUND: i32 = 10;
pub const HAND_DOUBLE_POUND: i32 = 11;
pub const HAND_ATTACKED: i32 = 12;
pub const HAND_RECOVER: i32 = 13;
pub const HAND_BECOME_ACTIVE: i32 = 14;
pub const HAND_DIE: i32 = 15;

/// `eyerok_seg5_anims_050116E4` (actors/eyerok/anims): flags bit 0 = no loop, start, loopStart, loopEnd.
pub const ANIMS: [AnimSpec; 8] = [
    anim_spec(true, 0, 0, 15),
    anim_spec(false, 0, 0, 40),
    anim_spec(false, 0, 0, 2),
    anim_spec(false, 0, 0, 25),
    anim_spec(true, 0, 0, 30),
    anim_spec(false, 0, 0, 29),
    anim_spec(false, 0, 0, 80),
    anim_spec(true, 29, 0, 30),
];
pub const ANIM_NAMES: [&str; 8] = [
    "eyerok_seg5_anim_0500D270",
    "eyerok_seg5_anim_0500DF50",
    "eyerok_seg5_anim_0500E1D8",
    "eyerok_seg5_anim_0500E99C",
    "eyerok_seg5_anim_0500F3D8",
    "eyerok_seg5_anim_0500FCCC",
    "eyerok_seg5_anim_050116CC",
    "eyerok_seg5_anim_0500F3F0",
];

/// sEyerokHitbox.
pub const HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 0.0, 0.0, 4.0, 0.0, 150.0, 100.0, 1.0, 1.0);

/// `ssl_seg7_collision_07028274` (closed), `070284B0` / `07028370` (sleeping left / right fist) and
/// `070282F8` (open hand).
const COL_KEYS: [&str; 5] = ["", "g2/ssl_seg7_collision_07028274", "g2/ssl_seg7_collision_070284B0", "g2/ssl_seg7_collision_07028370", "g2/ssl_seg7_collision_070282F8"];

pub fn init_boss(o: &mut SimObject) {
    o.visible = false;
}

fn spawn_hand(m: &mut MarioRuntime, boss: ObjId, side: f64) {
    let model = if side < 0.0 { "actors/eyerok/eyerok_left_hand_geo.glb" } else { "actors/eyerok/eyerok_right_hand_geo.glb" };
    let hand = spawn_child(m, boss, K::EyerokHand, Some(model), 0.0);
    let (bpos, byaw) = (m.objects[boss].pos, m.objects[boss].move_yaw);
    let h = &mut m.objects[hand];
    // eyerok_spawn_hand: spawn_object_relative_with_scale(side, -500 * side, 0, 300, 1.5, ...)
    set_pos_relative(h, bpos, byaw, -500.0 * side, 0.0, 300.0);
    h.home = h.pos;
    h.scale = 1.5;
    h.face_yaw = to_s16(h.move_yaw - 0x4000 as f64 * side) as f64;
    h.set_var("side", side);
    h.set_var("flipX", if side < 0.0 { -1.0 } else { 1.0 });
    h.param = ((side as i32 & 0xFF) << 16) as f64;
}

/// `bhv_eyerok_boss_loop`.
pub fn boss(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        match m.objects[id].action {
            BOSS_SLEEP => {
                if m.objects[id].timer == 0.0 {
                    spawn_hand(m, id, -1.0);
                    spawn_hand(m, id, 1.0);
                } else if m.objects[id].distance_to_mario < 500.0 {
                    m.objects[id].action = BOSS_WAKE_UP;
                }
            }
            BOSS_WAKE_UP => {
                let ready = mario_ready_to_speak(m);
                let o = &mut m.objects[id];
                if o.var("numHands") == 2.0 {
                    if o.timer > 5.0 {
                        if o.action_state == 0.0 {
                            o.action_state += 1.0; // seq_player_lower_volume
                        }
                        let u110 = o.var("unk110");
                        if u110 == 0.0 && ready {
                            o.action = BOSS_SHOW_INTRO_TEXT;
                        } else if o.timer > 150.0 {
                            let mut v = u110;
                            if approach_f32_ptr(&mut v, 0.0, 10.0) {
                                o.timer = 0.0;
                            }
                            o.set_var("unk110", v);
                        } else if o.timer > 90.0 {
                            let mut v = u110;
                            approach_f32_ptr(&mut v, 300.0, 10.0);
                            o.set_var("unk110", v);
                        }
                    }
                } else {
                    o.timer = 0.0;
                }
            }
            BOSS_SHOW_INTRO_TEXT => {
                if update_dialog(m, id, 117) {
                    m.objects[id].action = BOSS_FIGHT;
                }
            }
            BOSS_FIGHT => boss_fight(m, id),
            BOSS_DIE => {
                if m.objects[id].timer == 60.0 {
                    if update_dialog(m, id, 118) {
                        spawn_default_star(m, [0.0, -900.0, -3700.0]);
                    } else {
                        m.objects[id].timer -= 1.0;
                    }
                } else if m.objects[id].timer > 120.0 {
                    mark_for_deletion(&mut m.objects[id]);
                }
            }
            _ => {}
        }
    });
}

/// `eyerok_boss_act_fight`.
fn boss_fight(m: &mut MarioRuntime, id: ObjId) {
    let mario_z = m.pos[2];
    let o = &mut m.objects[id];
    let near = |o: &SimObject, lim: f64| mario_z - o.home[2] < lim;
    if o.var("numHands") == 0.0 {
        o.action = BOSS_DIE;
    } else if o.var("unk1AC") == 0.0 && o.var("activeHand") == 0.0 {
        let u104 = o.var("unk104");
        if u104 != 0.0 {
            let mut u110 = o.var("unk110");
            let reached = approach_f32_ptr(&mut u110, 1.0, 0.02);
            o.set_var("unk110", u110);
            if reached {
                let mut u104 = u104;
                if u104 < 0.0 {
                    if !near(o, 400.0) {
                        u104 += 1.0;
                        if u104 == 0.0 {
                            u104 = 1.0;
                        }
                    }
                } else {
                    u104 -= 1.0;
                }
                o.set_var("unk104", u104);
                if u104 != 0.0 && u104 != 1.0 {
                    let fc = o.var("unkFC") + 1.0;
                    o.set_var("unkFC", fc);
                    let hand = (fc as i32 & 1) as f64;
                    o.set_var("activeHand", if hand == 0.0 { -1.0 } else { hand });
                }
            }
        } else {
            let fc = o.var("unkFC") + 1.0;
            o.set_var("unkFC", fc);
            if near(o, 400.0) {
                o.set_var("unk104", -8.0);
                o.set_var("unk110", 1.0);
                o.set_var("unk108", 0.0);
            } else if o.var("numHands") == 2.0 && (fc as i32) % 6 == 0 {
                o.set_var("unk104", 8.0);
                o.set_var("unk110", 0.0);
                let r = (random_u16() as i32 & 1) as f64;
                o.set_var("unkFC", r);
                o.set_var("unk108", if r != 0.0 { -1.0 } else { 1.0 });
                let z = clamp_f(mario_z, o.pos[2] + 400.0, o.pos[2] + 1600.0);
                o.set_var("unk10C", z);
            } else {
                let hand = (fc as i32 & 1) as f64;
                o.set_var("activeHand", if hand == 0.0 { -1.0 } else { hand });
            }
        }
    }
}

pub fn init_hand(o: &mut SimObject) {
    o.visible = true;
    o.scale = 1.5;
    o.set_var("side", 1.0);
    o.set_var("flipX", 1.0);
    phys::set_physics(o, 150.0, 0.0, 0.0, 0.0, 1000.0, 200.0);
    o.set_var("animState", 3.0);
    anim::init(o, 6, ANIMS[6]);
}

fn pound_ground(m: &mut MarioRuntime, id: ObjId) {
    let pos = m.objects[id].pos;
    m.objects.sound(pos, "OBJ_STOMPED");
}

/// `eyerok_hand_check_attacked`.
fn check_attacked(m: &mut MarioRuntime, id: ObjId, parent: ObjId) -> bool {
    let o = &mut m.objects[id];
    if o.var("receivedAttack") != 0.0 && abs_angle_diff(o.angle_to_mario, o.face_yaw) < 0x3000 as f64 {
        o.hitbox.health -= 1.0;
        let died = o.hitbox.health < 2.0;
        if !died {
            o.action = HAND_ATTACKED;
            o.vel[1] = 30.0;
        } else {
            o.action = HAND_DIE;
            o.vel[1] = 50.0;
        }
        o.forward_vel *= 0.2;
        o.move_yaw = to_s16(o.face_yaw + 0x8000 as f64) as f64;
        phys::set_flags(o, 0);
        o.set_var("grav", -4.0);
        o.set_var("animState", 3.0);
        if died {
            let n = m.objects[parent].var("numHands") - 1.0;
            m.objects[parent].set_var("numHands", n);
        }
        return true;
    }
    false
}

/// `bhv_eyerok_hand_loop`.
pub fn hand(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _riding| {
        let parent = m.objects[id].link.unwrap_or(id);
        let world = m.world.clone();
        let side = m.objects[id].var("side");
        m.objects[id].scale = 1.5;
        if m.objects[id].action == HAND_SLEEP {
            hand_sleep(m, id, parent, side);
        } else {
            phys::update_floor_and_walls(&mut m.objects[id], &world);
            hand_action(m, id, parent, side);
            let a = m.objects[id].action;
            let o = &mut m.objects[id];
            let attack = obj_check_attacks(o, HITBOX, a, false);
            o.set_var("receivedAttack", attack as f64);
            phys::move_standard(o, &world, -78.0);
        }
        let col = m.objects[id].var("col") as usize;
        if col > 0 {
            load_object_collision_model(m, id, COL_KEYS[col], 1000.0);
        }
        advance_anim(&mut m.objects[id]);
    });
}

fn hand_sleep(m: &mut MarioRuntime, id: ObjId, parent: ObjId, side: f64) {
    let parent_action = m.objects[parent].action;
    let mut woke = false;
    {
        let o = &mut m.objects[id];
        let mut awake = false;
        if parent_action != BOSS_SLEEP {
            let t = o.var("wake") + 1.0;
            o.set_var("wake", t);
            awake = t > -3.0 * side;
        }
        if awake {
            if anim::near_end(o) {
                o.action = HAND_IDLE;
                o.set_var("col", 1.0);
                woke = true;
            } else {
                let mut x = o.pos[0];
                approach_f32_ptr(&mut x, o.home[0], 15.0);
                o.pos[0] = x;
                let ang = to_s16(((o.pos[0] - o.home[0]).abs() / 724.0 * 0x8000 as f64).trunc()) as f64;
                o.pos[1] = o.home[1] + (200.0 * side + 400.0) * sins(ang);
                let yaw = o.move_yaw;
                face_yaw_approach(o, yaw, 400.0);
            }
        } else {
            o.set_var("col", if side < 0.0 { 2.0 } else { 3.0 });
            reverse_animation(o);
            o.pos[0] = o.home[0] + 724.0 * side;
        }
    }
    if woke {
        let n = m.objects[parent].var("numHands") + 1.0;
        m.objects[parent].set_var("numHands", n);
    }
}

fn hand_action(m: &mut MarioRuntime, id: ObjId, parent: ObjId, side: f64) {
    let mario_z = m.pos[2];
    let mario_x = m.pos[0];
    let (p_act, p_pos, p_n) = {
        let p = &m.objects[parent];
        (p.action, p.pos, p.var("numHands"))
    };
    let p_active = m.objects[parent].var("activeHand");
    let p_u104 = m.objects[parent].var("unk104");
    let p_u1ac = m.objects[parent].var("unk1AC");
    let p_u108 = m.objects[parent].var("unk108");
    let p_u10c = m.objects[parent].var("unk10C");
    let p_u110 = m.objects[parent].var("unk110");
    let rel_z = |o: &SimObject, lim: f64| mario_z - o.home[2] < lim;
    let mut parent_set: Vec<(&'static str, f64)> = vec![];
    let mut pound = false;
    {
        let o = &mut m.objects[id];
        match o.action {
            HAND_IDLE => {
                init_anim(o, &ANIMS, 2);
                if p_act == BOSS_FIGHT {
                    if p_u104 != 0.0 {
                        if p_u104 != 1.0 {
                            o.action = HAND_BEGIN_DOUBLE_POUND;
                            o.set_var("grav", 0.0);
                        }
                    } else if p_u1ac == 0.0 && p_active != 0.0 {
                        if p_active == side {
                            if rel_z(o, 400.0) || (random_u16() as i32) % 2 != 0 {
                                o.action = HAND_TARGET_MARIO;
                                o.move_yaw = o.angle_to_mario;
                                o.set_var("grav", 0.0);
                            } else {
                                o.action = HAND_FIST_PUSH;
                                o.move_yaw = if p_pos[0] - mario_x < 0.0 { -0x800 as f64 } else { 0x800 as f64 };
                                o.move_yaw = to_s16(o.move_yaw + o.angle_to_mario) as f64;
                                o.set_var("grav", -4.0);
                            }
                        } else {
                            o.action = HAND_OPEN;
                        }
                    }
                } else {
                    o.pos[1] = o.home[1] + p_u110;
                }
            }
            HAND_OPEN => {
                parent_set.push(("unk1AC", side));
                if init_anim_check_end(o, &ANIMS, 4) {
                    o.action = HAND_SHOW_EYE;
                    o.set_var("fc", 2.0);
                    o.set_var("x100", 60.0);
                    o.set_var("col", 4.0);
                    if p_n != 2.0 {
                        let a = clamp_f(to_s16(o.angle_to_mario) as f64, -0x3000 as f64, 0x3000 as f64);
                        o.move_yaw = a;
                        o.forward_vel = 50.0;
                    } else {
                        o.move_yaw = 0.0;
                    }
                }
            }
            HAND_SHOW_EYE => {
                init_anim(o, &ANIMS, 5);
                // check_attacked needs the parent; the call is made below with the borrow released.
                o.set_var("showEye", 1.0);
            }
            HAND_CLOSE => {
                if init_anim_check_frame(o, &ANIMS, 7, 1) {
                    o.set_var("col", 1.0);
                    if p_n != 2.0 {
                        o.action = HAND_RETREAT;
                        parent_set.push(("activeHand", side));
                    } else if p_active == 0.0 {
                        o.action = HAND_IDLE;
                        parent_set.push(("unk1AC", 0.0));
                    }
                }
            }
            HAND_RETREAT => {
                let mut dist = lateral_dist_to_home(o);
                let angle = angle_to_home(o);
                dist -= 40.0;
                if dist < 0.0 {
                    dist = 0.0;
                }
                o.pos[0] = o.home[0] - dist * sins(angle);
                o.pos[2] = o.home[2] - dist * coss(angle);
                face_yaw_approach(o, 0.0, 400.0);
                let mut y = o.pos[1];
                let reached = approach_f32_ptr(&mut y, o.home[1], 20.0);
                o.pos[1] = y;
                if reached && dist == 0.0 && to_s16(o.face_yaw) == 0 {
                    o.action = HAND_IDLE;
                    parent_set.push(("activeHand", p_active - side));
                    if p_u1ac == side {
                        parent_set.push(("unk1AC", 0.0));
                    }
                }
            }
            HAND_TARGET_MARIO => {
                let hit_wall = phys::flags(o) & phys::MOVE_HIT_WALL != 0;
                if rel_z(o, 400.0) || o.pos[2] - mario_z > 0.0 || o.pos[2] - p_pos[2] > 1700.0 || (o.pos[0] - p_pos[0]).abs() > 900.0 || hit_wall {
                    o.forward_vel = 0.0;
                    let mut y = o.pos[1];
                    if approach_f32_ptr(&mut y, o.home[1] + 300.0, 20.0) {
                        o.action = HAND_SMASH;
                    }
                    o.pos[1] = y;
                } else {
                    forward_vel_approach(o, 50.0, 5.0);
                    let mut y = o.pos[1];
                    approach_f32_ptr(&mut y, o.home[1] + 300.0, 20.0);
                    o.pos[1] = y;
                    let t = o.angle_to_mario;
                    rotate_yaw_toward(o, t, 4000.0);
                }
            }
            HAND_SMASH => {
                if o.timer > 20.0 {
                    if phys::flags(o) & phys::MASK_ON_GROUND != 0 {
                        if o.var("grav") < -4.0 {
                            pound = true;
                            o.set_var("grav", -4.0);
                        } else {
                            let d = abs_angle_diff(o.face_yaw, o.angle_to_mario);
                            if o.distance_to_mario < 300.0 && d > 0x2000 as f64 && d < 0x6000 as f64 {
                                o.action = HAND_FIST_SWEEP;
                                o.move_yaw = if to_s16(o.face_yaw - o.angle_to_mario) < 0 { 0x4000 as f64 } else { -0x4000 as f64 };
                            } else {
                                o.action = HAND_RETREAT;
                            }
                        }
                    } else {
                        o.set_var("grav", -20.0);
                    }
                }
            }
            HAND_FIST_PUSH => {
                let hit_edge = phys::flags(o) & phys::MOVE_HIT_EDGE != 0;
                if o.timer > 5.0 && (o.pos[2] - mario_z > 0.0 || hit_edge) {
                    o.action = HAND_FIST_SWEEP;
                    o.forward_vel = 0.0;
                    o.move_yaw = if o.pos[0] - mario_x < 0.0 { 0x4000 as f64 } else { -0x4000 as f64 };
                } else {
                    o.forward_vel = 50.0;
                }
            }
            HAND_FIST_SWEEP => {
                let hit_edge = phys::flags(o) & phys::MOVE_HIT_EDGE != 0;
                if o.pos[2] - p_pos[2] < 1000.0 || hit_edge {
                    o.action = HAND_RETREAT;
                    o.forward_vel = 0.0;
                } else {
                    forward_vel_approach(o, 5.0, 0.02);
                    o.forward_vel *= 1.08;
                    o.timer = 0.0;
                }
            }
            HAND_BEGIN_DOUBLE_POUND => {
                if p_u104 < 0.0 || p_active == side {
                    o.action = HAND_DOUBLE_POUND;
                    o.move_yaw = to_s16(o.face_yaw - 0x4000 as f64 * p_u108) as f64;
                } else {
                    let sp4 = p_pos[0] + 400.0 * p_u108 - 180.0 * side;
                    o.pos[0] = o.home[0] + (sp4 - o.home[0]) * p_u110;
                    o.pos[1] = o.home[1] + 300.0 * p_u110;
                    o.pos[2] = o.home[2] + (p_u10c - o.home[2]) * p_u110;
                }
            }
            HAND_DOUBLE_POUND => {
                if p_n != 2.0 {
                    parent_set.push(("activeHand", side));
                }
                if p_u104 == 1.0 {
                    o.action = HAND_RETREAT;
                    parent_set.push(("unk1AC", side));
                } else if p_active == side {
                    if phys::flags(o) & phys::MASK_ON_GROUND != 0 {
                        if o.var("grav") < -15.0 {
                            parent_set.push(("activeHand", 0.0));
                            pound = true;
                            o.forward_vel = 0.0;
                            o.set_var("grav", -15.0);
                        } else {
                            o.forward_vel = 30.0 * p_u108.abs();
                            o.vel[1] = 100.0;
                            phys::set_flags(o, 0);
                        }
                    } else if o.vel[1] <= 0.0 {
                        o.set_var("grav", -20.0);
                    }
                }
            }
            HAND_ATTACKED => {
                if init_anim_check_end(o, &ANIMS, 3) {
                    o.action = HAND_RECOVER;
                    o.set_var("col", 1.0);
                }
                if phys::flags(o) & phys::MASK_ON_GROUND != 0 {
                    o.forward_vel = 0.0;
                }
            }
            HAND_RECOVER => {
                if init_anim_check_end(o, &ANIMS, 0) {
                    o.action = HAND_BECOME_ACTIVE;
                }
            }
            HAND_BECOME_ACTIVE => {
                if p_active == 0.0 || p_n != 2.0 {
                    o.action = HAND_RETREAT;
                    parent_set.push(("activeHand", side));
                }
            }
            HAND_DIE => {
                if init_anim_check_end(o, &ANIMS, 1) {
                    parent_set.push(("unk1AC", 0.0));
                    o.set_var("explode", 1.0);
                }
                if phys::flags(o) & phys::MASK_ON_GROUND != 0 {
                    pound = true;
                    o.forward_vel = 0.0;
                }
            }
            _ => {}
        }
    }
    if m.objects[id].action == HAND_SHOW_EYE {
        show_eye(m, id, parent, side);
    }
    for (k, v) in parent_set {
        m.objects[parent].set_var(k, v);
    }
    if pound {
        pound_ground(m, id);
    }
    if m.objects[id].var("explode") != 0.0 {
        explode_and_spawn_coins(m, id, 0);
    }
}

fn lateral_dist_to_home(o: &SimObject) -> f64 {
    let (dx, dz) = (o.home[0] - o.pos[0], o.home[2] - o.pos[2]);
    (dx * dx + dz * dz).sqrt()
}

/// `eyerok_hand_act_show_eye`.
fn show_eye(m: &mut MarioRuntime, id: ObjId, parent: ObjId, side: f64) {
    let mario_z = m.pos[2];
    let p_active = m.objects[parent].var("activeHand");
    let p_n = m.objects[parent].var("numHands");
    if check_attacked(m, id, parent) {
        return;
    }
    let o = &mut m.objects[id];
    if p_active == 0.0 {
        if o.var("animState") < 3.0 {
            o.set_var("animState", o.var("animState") + 1.0);
        } else if anim::near_end(o) {
            o.action = HAND_CLOSE;
        }
    } else {
        let t = o.var("x100");
        o.set_var("x100", t - 1.0);
        if t != 0.0 {
            let fc = o.var("fc");
            if fc != 0.0 {
                o.set_var("fc", fc - 1.0);
            }
            const D80331BA4: [f64; 6] = [0.0, 1.0, 3.0, 2.0, 1.0, 0.0];
            o.set_var("animState", D80331BA4[(o.var("fc") as usize).min(5)]);
        } else {
            o.set_var("fc", 5.0);
            o.set_var("x100", random_linear_offset(20.0, 50.0));
        }
        if p_n != 2.0 {
            let yaw = o.move_yaw;
            face_yaw_approach(o, yaw, 0x800 as f64);
            if o.timer > 10.0 && (o.pos[2] - mario_z > 0.0 || phys::flags(o) & phys::MOVE_HIT_EDGE != 0) {
                m.objects[parent].set_var("activeHand", 0.0);
                m.objects[id].forward_vel = 0.0;
            }
        }
    }
    let _ = side;
}
