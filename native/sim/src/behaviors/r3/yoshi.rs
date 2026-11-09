//! Yoshi (`bhvYoshi`, castle roof; appears at 120 stars): port of `src/game/behaviors/yoshi.inc.c`.
//!
//! He strolls between four spots on the roof; talk to him (A or B, dialog 161) and he showers Mario with lives (one every
//! four frames up to 100), then walks to the roof edge and jumps off for good. The app publishes the star count as the
//! `stars` var (`progress::castle_npcs`); below 120 he never appears.
//!
//! Left out: the credits pose, the star-spawn cutscene camera, the special triple jump grant, sounds.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::hitbox;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::behaviors::part_b::npc::{check_npc_talk, update_dialog_with_cutscene, MARIO_DIALOG_LOOK_FRONT};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

pub const MODEL: &str = "actors/yoshi/yoshi_geo.glb";
/// `yoshi_seg5_anims_05024100`: 0 = idle (50), 1 = walk (29), 2 = jump (6).
pub const ANIMS: [AnimSpec; 3] = [spec(0, 0, 0x32, false), spec(0, 0, 0x1D, false), spec(0, 0, 0x06, false)];
pub const ANIM_NAMES: [&str; 3] = ["yoshi_seg5_anim_050233A4", "yoshi_seg5_anim_05023E4C", "yoshi_seg5_anim_050240E8"];

const IDLE: i32 = 0;
const WALK: i32 = 1;
const TALK: i32 = 2;
const WALK_JUMP_OFF_ROOF: i32 = 3;
const FINISH_JUMPING: i32 = 4;
const GIVE_PRESENT: i32 = 5;

/// `sYoshiHomeLocations` (x, z).
const HOMES: [(f64, f64); 4] = [(0.0, -5625.0), (-1364.0, -5912.0), (-1403.0, -4609.0), (-1004.0, -5308.0)];

pub fn init(o: &mut SimObject) {
    o.visible = false;
    o.hitbox = hitbox(INTERACT_TEXT, 0.0, 0.0, 0.0, 0.0, 160.0, 150.0, 160.0, 150.0);
    o.set_var("realInteract", INTERACT_TEXT as f64);
    o.interact_subtype = INT_SUBTYPE_NPC;
    phys::set_physics(o, 0.0, -200.0, 0.0, 0.0, 90.0, 130.0);
    anim::init(o, 0, ANIMS[0]);
}

/// The hook while Mario overlaps him: A or B facing him starts the talk.
pub fn interact(id: ObjId, m: &mut MarioRuntime) -> bool {
    if !m.objects[id].visible {
        return false;
    }
    check_npc_talk(m, id)
}

fn close_to(o: &SimObject, x: f64, y: f64, z: f64, d: f64) -> bool {
    dist_3d(&o.pos, &[x, y, z]) < d
}

/// `bhv_yoshi_loop`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    if !m.objects[id].has_var("stars") {
        return;
    }
    if !m.objects[id].has_var("shown") {
        if m.objects[id].var("stars") < 120.0 {
            m.objects[id].deactivate(-1.0);
            return;
        }
        let o = &mut m.objects[id];
        o.set_var("shown", 1.0);
        o.visible = true;
    }
    let world = m.world.clone();
    run(m, id, F_FACE_YAW, |m, id| {
        let mut give_life = false;
        {
            let o = &mut m.objects[id];
            let mut talked = o.interact_status == INT_STATUS_INTERACTED;
            match o.action {
                IDLE => {
                    if o.timer > 90.0 {
                        let chosen = (random_float() * 3.99) as i64;
                        if o.var("home") as i64 != chosen {
                            o.set_var("home", chosen as f64);
                            let (hx, hz) = HOMES[chosen as usize];
                            o.home[0] = hx;
                            o.home[2] = hz;
                            o.set_var("targetYaw", atan2s(hz - o.pos[2], hx - o.pos[0]) as f64);
                            o.action = WALK;
                        }
                    }
                    anim::init(o, 0, ANIMS[0]);
                    if talked {
                        o.action = TALK;
                    }
                }
                WALK => {
                    o.forward_vel = 10.0;
                    let target = o.var("targetYaw");
                    o.move_yaw = approach_s16_symmetric(o.move_yaw, target, 0x500 as f64);
                    phys::update_floor_and_walls(o, &world);
                    phys::move_standard(o, &world, -78.0);
                    if close_to(o, o.home[0], 3174.0, o.home[2], 200.0) {
                        o.action = IDLE;
                    }
                    anim::init(o, 1, ANIMS[1]);
                    if talked {
                        o.action = TALK;
                    }
                    if o.pos[1] < 2100.0 {
                        o.deactivate(-1.0);
                    }
                }
                TALK => {
                    talked = false;
                }
                WALK_JUMP_OFF_ROOF => {
                    o.forward_vel = 10.0;
                    phys::update_floor_and_walls(o, &world);
                    phys::move_standard(o, &world, -78.0);
                    anim::init(o, 1, ANIMS[1]);
                    let target = o.var("targetYaw");
                    o.move_yaw = approach_s16_symmetric(o.move_yaw, target, 0x500 as f64);
                    if close_to(o, o.home[0], 3174.0, o.home[2], 200.0) {
                        anim::init(o, 2, ANIMS[2]);
                        o.forward_vel = 50.0;
                        o.vel[1] = 40.0;
                        o.move_yaw = -0x3FFF as f64;
                        o.action = FINISH_JUMPING;
                    }
                }
                FINISH_JUMPING => {
                    o.pos[0] += sins(o.move_yaw) * o.forward_vel;
                    o.pos[2] += coss(o.move_yaw) * o.forward_vel;
                    o.pos[1] += o.vel[1];
                    o.vel[1] -= 2.0;
                    if o.pos[1] < 2100.0 {
                        o.deactivate(-1.0);
                    }
                }
                _ => {
                    // GIVE_PRESENT: one life every four frames until 100.
                    give_life = true;
                }
            }
            let _ = talked;
            if !o.active {
                return;
            }
            anim::advance(o);
        }
        if m.objects[id].action == TALK
            && update_dialog_with_cutscene(m, id, "talkState", MARIO_DIALOG_LOOK_FRONT, true, 161)
        {
            let o = &mut m.objects[id];
            o.interact_status = 0;
            o.home[0] = HOMES[1].0;
            o.home[2] = HOMES[1].1;
            o.set_var("targetYaw", atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64);
            o.action = GIVE_PRESENT;
        }
        if give_life {
            if m.lives >= 100.0 {
                m.objects[id].action = WALK_JUMP_OFF_ROOF;
            } else if (m.objects[id].timer as i64) % 4 == 0 {
                m.lives += 1.0;
            }
        }
    });
}
