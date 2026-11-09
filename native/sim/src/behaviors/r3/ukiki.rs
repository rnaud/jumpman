//! Ukiki (`bhvUkiki` / `bhvMacroUkiki`) and his cage (`bhvUkikiCage`, `bhvUkikiCageStar`) of Tall, Tall
//! Mountain. Port of `src/game/behaviors/ukiki.inc.c` and `ukiki_cage.inc.c`.
//!
//! * The cap Ukiki (`UKIKI_BP_CAP`, every act) runs at Mario while he wears his cap; grabbing him makes him
//!   steal it (`mario_lose_cap_to_enemy(2)`, DIALOG_100) and jump away; grabbing him again gives it back
//!   (DIALOG_101, `mario_retrieve_cap`).
//! * The cage Ukiki (`UKIKI_BP_CAGE`, act 2) flees; held near his home he asks to be taken to his cage
//!   (DIALOG_079 with a Yes / No choice). "Yes": he runs `sCageUkikiPath` to the cage, waits for Mario
//!   (DIALOG_080), jumps on the cage, spins it and rides it off the cliff; the cage breaks where it lands and
//!   the cage star (`STAR_INDEX_ACT_2`) flies to (2500, -1200, 1300).
//!
//! Vars: `textState` (oUkikiTextState), `hasCap` (oUkikiHasCap), `tauntCounter`, `tauntsToBeDone`,
//! `chaseFleeRange`, `textboxTimer`, `spinTimer`, `animState` (oAnimState: 2 = cap on), `soundState`
//! (oSoundStateID), `pathPrev` (oPathedPrevWaypoint index); on the cage `nextAction`
//! (oUkikiCageNextAction).
use super::*;
use crate::behaviors::g1::anim::{self, spec};
use crate::behaviors::g1::{abs_angle_diff, become_intangible, become_tangible, hitbox};
use crate::behaviors::g2::dyn_collision::{load_object_collision_model, run_platform, unload_object_collision};
use crate::behaviors::g2::obj::{begin, drop_to_floor, finish, first_frame, mark_for_deletion, random_float, rotate_yaw_toward, spawn_child, F_FACE_YAW};
use crate::behaviors::g2::phys::{self, MASK_IN_WATER, MASK_ON_GROUND, MOVE_AT_WATER_SURFACE, MOVE_ENTERED_WATER, MOVE_HIT_EDGE, MOVE_HIT_WALL, MOVE_LANDED, MOVE_UNDERWATER_ON_GROUND};
use crate::behaviors::part_b::npc::{
    set_mario_npc_dialog, update_dialog, update_dialog_with_cutscene, DIALOG_FLAG_TEXT_DEFAULT, DIALOG_FLAG_TIME_STOP_ENABLED, MARIO_DIALOG_LOOK_DOWN,
    MARIO_DIALOG_LOOK_UP, MARIO_DIALOG_STATUS_SPEAK, MARIO_DIALOG_STOP,
};
use crate::cutscene_ext::{CutCue, DIALOG_RESPONSE_NONE, DIALOG_RESPONSE_YES};

// object_constants.h
pub const UKIKI_ACT_IDLE: i32 = 0;
pub const UKIKI_ACT_RUN: i32 = 1;
pub const UKIKI_ACT_TURN_TO_MARIO: i32 = 2;
pub const UKIKI_ACT_JUMP: i32 = 3;
pub const UKIKI_ACT_GO_TO_CAGE: i32 = 4;
pub const UKIKI_ACT_WAIT_TO_RESPAWN: i32 = 5;
pub const UKIKI_ACT_UNUSED_TURN: i32 = 6;
pub const UKIKI_ACT_RETURN_HOME: i32 = 7;

pub const UKIKI_SUB_ACT_TAUNT_NONE: f64 = 0.0;
pub const UKIKI_SUB_ACT_TAUNT_ITCH: f64 = 1.0;
pub const UKIKI_SUB_ACT_TAUNT_SCREECH: f64 = 2.0;
pub const UKIKI_SUB_ACT_TAUNT_JUMP_CLAP: f64 = 3.0;
pub const UKIKI_SUB_ACT_TAUNT_HANDSTAND: f64 = 4.0;

pub const UKIKI_SUB_ACT_CAGE_RUN_TO_CAGE: i32 = 0;
pub const UKIKI_SUB_ACT_CAGE_WAIT_FOR_MARIO: i32 = 1;
pub const UKIKI_SUB_ACT_CAGE_TALK_TO_MARIO: i32 = 2;
pub const UKIKI_SUB_ACT_CAGE_TURN_TO_CAGE: i32 = 3;
pub const UKIKI_SUB_ACT_CAGE_JUMP_TO_CAGE: i32 = 4;
pub const UKIKI_SUB_ACT_CAGE_LAND_ON_CAGE: i32 = 5;
pub const UKIKI_SUB_ACT_CAGE_SPIN_ON_CAGE: i32 = 6;
pub const UKIKI_SUB_ACT_CAGE_DESPAWN: i32 = 7;

pub const UKIKI_TEXT_DEFAULT: f64 = 0.0;
pub const UKIKI_TEXT_CAGE_TEXTBOX: f64 = 1.0;
pub const UKIKI_TEXT_GO_TO_CAGE: f64 = 2.0;
pub const UKIKI_TEXT_STOLE_CAP: f64 = 3.0;
pub const UKIKI_TEXT_HAS_CAP: f64 = 4.0;
pub const UKIKI_TEXT_GAVE_CAP_BACK: f64 = 5.0;
pub const UKIKI_TEXT_DO_NOT_LET_GO: f64 = 6.0;
pub const UKIKI_TEXT_STEAL_CAP: f64 = 7.0;

pub const UKIKI_BP_CAGE: i32 = 0;
pub const UKIKI_BP_CAP: i32 = 1;

pub const UKIKI_ANIM_RUN: usize = 0;
pub const UKIKI_ANIM_SCREECH: usize = 4;
pub const UKIKI_ANIM_JUMP_CLAP: usize = 5;
pub const UKIKI_ANIM_LAND: usize = 7;
pub const UKIKI_ANIM_JUMP: usize = 8;
pub const UKIKI_ANIM_ITCH: usize = 9;
pub const UKIKI_ANIM_HANDSTAND: usize = 10;
pub const UKIKI_ANIM_TURN: usize = 11;
pub const UKIKI_ANIM_HELD: usize = 12;

pub const UKIKI_ANIM_STATE_DEFAULT: f64 = 0.0;
pub const UKIKI_ANIM_STATE_CAP_ON: f64 = 2.0;
pub const UKIKI_CAP_ON: f64 = 1.0;

pub const UKIKI_CAGE_STAR_ACT_IN_CAGE: i32 = 0;
pub const UKIKI_CAGE_STAR_ACT_SPAWN_STAR: i32 = 1;

pub const UKIKI_CAGE_ACT_WAIT_FOR_UKIKI: i32 = 0;
pub const UKIKI_CAGE_ACT_SPIN: i32 = 1;
pub const UKIKI_CAGE_ACT_FALL: i32 = 2;
pub const UKIKI_CAGE_ACT_HIDE: i32 = 3;

pub const DIALOG_079: u32 = 79;
pub const DIALOG_080: u32 = 80;
pub const DIALOG_100: u32 = 100;
pub const DIALOG_101: u32 = 101;

/// Where the cage star flies (`spawn_default_star(2500.0f, -1200.0f, 1300.0f)`, ukiki_cage.inc.c).
pub const CAGE_STAR_POS: Vec3 = [2500.0, -1200.0, 1300.0];
/// The cage's collision model (`LOAD_COLLISION_DATA(ttm_seg7_collision_ukiki_cage)`).
pub const CAGE_COLLISION: &str = "g2/ttm_seg7_collision_ukiki_cage";
pub const STAR_MODEL: &str = "actors/star/star_geo.glb";

/// `ukiki_seg5_anims_05015784` (actors/ukiki/anims/*.inc.c headers: flags, startFrame, loopStart, loopEnd).
pub const ANIMS: [AnimSpec; 13] = [
    spec(0, 0, 0x0C, false),
    spec(0, 0, 0x10, false),
    spec(0, 0, 0x64, false),
    spec(0, 0, 0x26, false),
    spec(0, 0, 0x0A, false),
    spec(0, 0, 0x10, false),
    spec(0, 0, 0x12, true),
    spec(0, 0, 0x08, false),
    spec(0, 0, 0x07, true),
    spec(0, 0, 0x64, false),
    spec(0, 0, 0x55, false),
    spec(0, 0, 0x14, false),
    spec(0, 0, 0x0E, false),
];
pub const ANIM_NAMES: [&str; 13] = [
    "ukiki_seg5_anim_0500D658", "ukiki_seg5_anim_0500DC80", "ukiki_seg5_anim_0500DDEC", "ukiki_seg5_anim_0500EACC",
    "ukiki_seg5_anim_0500EEA8", "ukiki_seg5_anim_0500F530", "ukiki_seg5_anim_0500FC84", "ukiki_seg5_anim_0501006C",
    "ukiki_seg5_anim_050103F4", "ukiki_seg5_anim_05012ABC", "ukiki_seg5_anim_05014BE4", "ukiki_seg5_anim_050153C4",
    "ukiki_seg5_anim_0501576C",
];

/// `sUkikiSoundStates`: (playSound, animFrame1, animFrame2, sound).
const SOUND_STATES: [(bool, i32, i32, &str); 13] = [
    (true, 1, 10, "OBJ_UKIKI_STEP_DEFAULT"),
    (false, 0, 0, ""),
    (false, 0, 0, ""),
    (false, 0, 0, ""),
    (true, 0, -1, "OBJ_UKIKI_CHATTER_SHORT"),
    (true, 0, -1, "OBJ_UKIKI_CHATTER_LONG"),
    (false, 0, 0, ""),
    (false, 0, 0, ""),
    (true, 0, -1, "OBJ_UKIKI_CHATTER_LONG"),
    (true, 0, -1, "OBJ_UKIKI_STEP_LEAVES"),
    (true, 0, -1, "OBJ_UKIKI_CHATTER_IDLE"),
    (false, 0, 0, ""),
    (false, 0, 0, ""),
];

/// `sCageUkikiPath`: from the top of the mountain to the cage.
pub const CAGE_PATH: [Vec3; 11] = [
    [1011.0, 2306.0, -285.0],
    [1151.0, 2304.0, -510.0],
    [1723.0, 1861.0, -964.0],
    [2082.0, 1775.0, -1128.0],
    [2489.0, 1717.0, -1141.0],
    [2662.0, 1694.0, -1140.0],
    [2902.0, 1536.0, -947.0],
    [2946.0, 1536.0, -467.0],
    [2924.0, 1536.0, 72.0],
    [2908.0, 1536.0, 536.0],
    [2886.0, 1536.0, 783.0],
];

fn bparam2(o: &SimObject) -> i32 {
    ((o.param as u32) >> 16 & 0xFF) as i32
}

/// `cur_obj_init_animation_with_sound(index)`.
fn init_anim(o: &mut SimObject, index: usize) {
    anim::init(o, index, ANIMS[index]);
    o.set_var("soundState", index as f64);
}

/// bhvMacroUkiki's script up to `CALL_NATIVE(bhv_ukiki_init)` (DROP_TO_FLOOR / SET_HOME / the save check run
/// on the first update, which has the world and Mario).
pub fn init(o: &mut SimObject) {
    // SET_INT(oInteractType, INTERACT_GRABBABLE), SET_INT(oInteractionSubtype, INT_SUBTYPE_HOLDABLE_NPC),
    // SET_HITBOX(40, 40)
    o.hitbox = hitbox(INTERACT_GRABBABLE, 0.0, 0.0, 0.0, 0.0, 40.0, 40.0, 0.0, 0.0);
    o.set_var("realInteract", INTERACT_GRABBABLE as f64);
    o.interact_subtype = INT_SUBTYPE_HOLDABLE_NPC;
    // SET_OBJ_PHYSICS(30, -400, -50, 0, 0, 200)
    phys::set_physics(o, 30.0, -400.0, -50.0, 0.0, 0.0, 200.0);
    o.visible = true;
    o.action = UKIKI_ACT_IDLE;
    o.prev_action = UKIKI_ACT_IDLE;
    init_anim(o, 0); // ANIMATE(0)
}

/// `bhv_ukiki_loop`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    begin(m, id);
    if first_frame(&mut m.objects[id]) {
        let o = &mut m.objects[id];
        drop_to_floor(o, &world);
        o.home = o.pos;
        // bhv_ukiki_init
        if bparam2(o) == UKIKI_BP_CAP && m.cut.cap_save_flags & crate::cutscene_ext::SAVE_FLAG_CAP_ON_UKIKI != 0 {
            let o = &mut m.objects[id];
            o.set_var("textState", UKIKI_TEXT_HAS_CAP);
            o.set_var("hasCap", UKIKI_CAP_ON);
        }
    }
    match m.objects[id].held_state {
        HELD_FREE => {
            // @bug (PARTIAL_UPDATE)
            m.objects[id].set_var("textboxTimer", 0.0);
            free_loop(id, m, &world);
        }
        HELD_HELD => {
            // cur_obj_unrender_set_action_and_anim(UKIKI_ANIM_HELD, 0) + obj_copy_pos(o, gMarioObject). The held
            // object stays drawn in front of Mario (the sim has no hand attachment, see object_manager::sync_held).
            let mpos = m.pos;
            let o = &mut m.objects[id];
            become_intangible(o);
            init_anim(o, UKIKI_ANIM_HELD);
            o.action = UKIKI_ACT_IDLE;
            o.pos = mpos;
            if bparam2(o) == UKIKI_BP_CAP {
                cap_ukiki_held_loop(id, m);
            } else {
                cage_ukiki_held_loop(id, m);
            }
        }
        _ => get_dropped(&mut m.objects[id], &world, m.state.pos),
    }
    let o = &mut m.objects[id];
    let cap = o.var("hasCap") as i32 & UKIKI_CAP_ON as i32 != 0;
    o.set_var("animState", if cap { UKIKI_ANIM_STATE_CAP_ON } else { UKIKI_ANIM_STATE_DEFAULT });
    o.interact_status = 0;
    finish(m, id, F_FACE_YAW);
    anim::advance(&mut m.objects[id]);
}

/// object_helpers.c `cur_obj_get_dropped` (+ `cur_obj_move_after_thrown_or_dropped(0, 0)`).
fn get_dropped(o: &mut SimObject, world: &crate::collision::CollisionWorld, mario: Vec3) {
    become_tangible(o);
    o.visible = true;
    o.held_state = HELD_FREE;
    phys::set_flags(o, 0);
    let mut floor = world.find_floor(o.pos[0], o.pos[1] + 160.0, o.pos[2]).height;
    if floor > o.pos[1] {
        o.pos[1] = floor;
    } else if floor < crate::collision::FLOOR_LOWER_LIMIT + 1000.0 {
        // OoB failsafe
        o.pos = mario;
        floor = world.find_floor(o.pos[0], o.pos[1], o.pos[2]).height;
    }
    o.set_var("floorH", floor);
    o.forward_vel = 0.0;
    o.vel[1] = 0.0;
}

fn is_cap_ukiki_and_mario_has_normal_cap_on_head(m: &MarioRuntime, o: &SimObject) -> bool {
    bparam2(o) == UKIKI_BP_CAP && does_mario_have_normal_cap_on_head(m)
}

/// `ukiki_free_loop`.
fn free_loop(id: ObjId, m: &mut MarioRuntime, world: &crate::collision::CollisionWorld) {
    phys::update_floor_and_walls(&mut m.objects[id], world);
    match m.objects[id].action {
        UKIKI_ACT_IDLE => act_idle(id, m, world),
        UKIKI_ACT_RUN => act_run(id, m),
        UKIKI_ACT_TURN_TO_MARIO => act_turn_to_mario(id, m),
        UKIKI_ACT_JUMP => act_jump(&mut m.objects[id]),
        UKIKI_ACT_GO_TO_CAGE => act_go_to_cage(id, m),
        UKIKI_ACT_WAIT_TO_RESPAWN => {
            // ukiki_act_wait_to_respawn
            idle_ukiki_taunt(&mut m.objects[id]);
            if mario_far_away(m, &m.objects[id]) {
                set_pos_to_home_and_stop(&mut m.objects[id]);
                m.objects[id].action = UKIKI_ACT_IDLE;
            }
        }
        UKIKI_ACT_UNUSED_TURN => {
            let o = &mut m.objects[id];
            idle_ukiki_taunt(o);
            if o.action_state == UKIKI_SUB_ACT_TAUNT_JUMP_CLAP {
                let a = o.angle_to_mario;
                rotate_yaw_toward(o, a, 0x400 as f64);
            }
        }
        _ => {
            // ukiki_act_return_home
            let o = &mut m.objects[id];
            init_anim(o, UKIKI_ANIM_RUN);
            o.move_yaw = crate::behaviors::r2a::helpers::angle_to_home(o);
            o.forward_vel = 10.0;
            if o.pos[1] > -1550.0 {
                o.action = UKIKI_ACT_IDLE;
            }
        }
    }
    let o = &mut m.objects[id];
    let steep = if o.action == UKIKI_ACT_GO_TO_CAGE || o.action == UKIKI_ACT_RETURN_HOME { -88.0 } else { -20.0 };
    phys::move_standard(o, world, steep);
    // handle_cap_ukiki_reset
    if bparam2(&m.objects[id]) == UKIKI_BP_CAP {
        if mario_far_away(m, &m.objects[id]) {
            set_pos_to_home_and_stop(&mut m.objects[id]);
            m.objects[id].action = UKIKI_ACT_IDLE;
        } else if phys::flags(&m.objects[id]) & MASK_IN_WATER != 0 {
            m.objects[id].action = UKIKI_ACT_WAIT_TO_RESPAWN;
        }
    }
    if phys::flags(&m.objects[id]) & MASK_IN_WATER == 0 {
        exec_anim_sound_state(id, m);
    }
}

/// spawn_sound.c `exec_anim_sound_state(sUkikiSoundStates)`.
fn exec_anim_sound_state(id: ObjId, m: &mut MarioRuntime) {
    let o = &m.objects[id];
    let (play, f1, f2, sound) = SOUND_STATES[(o.var("soundState") as usize).min(12)];
    if !play {
        return;
    }
    let pos = o.pos;
    let frame = anim::frame(o);
    if (f1 >= 0 && frame == f1) || (f2 >= 0 && frame == f2) {
        m.objects.sound(pos, sound);
    }
}

/// object_helpers.c `cur_obj_set_pos_to_home_and_stop`.
fn set_pos_to_home_and_stop(o: &mut SimObject) {
    o.pos = o.home;
    o.forward_vel = 0.0;
    o.vel[1] = 0.0;
}

/// `idle_ukiki_taunt`: random taunts, looped a random number of times.
fn idle_ukiki_taunt(o: &mut SimObject) {
    o.forward_vel = 0.0;
    if o.action_state == UKIKI_SUB_ACT_TAUNT_NONE {
        // Subaction is between 1 and 4.
        o.action_state = (random_float() * 4.0 + 1.0).trunc();
        o.set_var("tauntCounter", 0.0);
        o.set_var("tauntsToBeDone", (random_float() * 4.0 + 2.0).trunc());
    }
    let s = o.action_state;
    if s == UKIKI_SUB_ACT_TAUNT_ITCH {
        init_anim(o, UKIKI_ANIM_ITCH);
        if anim::near_end(o) {
            o.action_state = UKIKI_SUB_ACT_TAUNT_NONE;
        }
    } else if s == UKIKI_SUB_ACT_TAUNT_SCREECH {
        init_anim(o, UKIKI_ANIM_SCREECH);
        if anim::near_end(o) {
            o.set_var("tauntCounter", o.var("tauntCounter") + 1.0);
        }
        if o.var("tauntCounter") >= o.var("tauntsToBeDone") * 2.0 {
            o.action_state = UKIKI_SUB_ACT_TAUNT_NONE;
        }
    } else if s == UKIKI_SUB_ACT_TAUNT_JUMP_CLAP {
        init_anim(o, UKIKI_ANIM_JUMP_CLAP);
        if anim::near_end(o) {
            o.set_var("tauntCounter", o.var("tauntCounter") + 1.0);
        }
        if o.var("tauntCounter") >= o.var("tauntsToBeDone") {
            o.action_state = UKIKI_SUB_ACT_TAUNT_NONE;
        }
    } else if s == UKIKI_SUB_ACT_TAUNT_HANDSTAND {
        init_anim(o, UKIKI_ANIM_HANDSTAND);
        if anim::near_end(o) {
            o.action_state = UKIKI_SUB_ACT_TAUNT_NONE;
        }
    }
}

/// `ukiki_act_idle`.
fn act_idle(id: ObjId, m: &mut MarioRuntime, world: &crate::collision::CollisionWorld) {
    let has_cap = is_cap_ukiki_and_mario_has_normal_cap_on_head(m, &m.objects[id]);
    let mario_yaw = m.face_angle[1];
    let o = &mut m.objects[id];
    idle_ukiki_taunt(o);
    let d = o.distance_to_mario;
    if has_cap {
        if d > 700.0 && d < 1000.0 {
            o.action = UKIKI_ACT_RUN;
        } else if d <= 700.0 && d > 200.0 && abs_angle_diff(o.angle_to_mario, o.move_yaw) > 0x1000 as f64 {
            o.action = UKIKI_ACT_TURN_TO_MARIO;
        }
    } else if d < 300.0 {
        o.action = UKIKI_ACT_RUN;
    }
    if o.var("textState") == UKIKI_TEXT_GO_TO_CAGE {
        o.action = UKIKI_ACT_GO_TO_CAGE;
    }
    // Jump away from Mario after stealing his cap.
    if o.var("textState") == UKIKI_TEXT_STOLE_CAP {
        o.move_yaw = to_s16(mario_yaw + 0x8000 as f64) as f64;
        if check_if_moving_over_floor(o, world, 50.0, 150.0) {
            o.action = UKIKI_ACT_JUMP;
        } else {
            o.move_yaw = to_s16(mario_yaw + 0x4000 as f64) as f64;
            if check_if_moving_over_floor(o, world, 50.0, 150.0) {
                o.action = UKIKI_ACT_JUMP;
            } else {
                o.move_yaw = to_s16(mario_yaw - 0x4000 as f64) as f64;
                if check_if_moving_over_floor(o, world, 50.0, 150.0) {
                    o.action = UKIKI_ACT_JUMP;
                }
            }
        }
        o.set_var("textState", UKIKI_TEXT_HAS_CAP);
    }
    if bparam2(o) == UKIKI_BP_CAP && o.pos[1] < -1550.0 {
        o.action = UKIKI_ACT_RETURN_HOME;
    }
}

/// `ukiki_act_turn_to_mario`.
fn act_turn_to_mario(id: ObjId, m: &mut MarioRuntime) {
    let has_cap = is_cap_ukiki_and_mario_has_normal_cap_on_head(m, &m.objects[id]);
    let o = &mut m.objects[id];
    // Initialize the action with a random fVel from 2-5.
    if o.timer == 0.0 {
        o.forward_vel = random_float() * 3.0 + 2.0;
    }
    init_anim(o, UKIKI_ANIM_TURN);
    let a = o.angle_to_mario;
    if rotate_yaw_toward(o, a, 0x800 as f64) {
        o.action = UKIKI_ACT_IDLE;
    }
    if has_cap {
        if o.distance_to_mario > 500.0 {
            o.action = UKIKI_ACT_RUN;
        }
    } else if o.distance_to_mario < 300.0 {
        o.action = UKIKI_ACT_RUN;
    }
}

/// `ukiki_act_run`: away from Mario, or toward him to steal the cap.
fn act_run(id: ObjId, m: &mut MarioRuntime) {
    let has_cap = is_cap_ukiki_and_mario_has_normal_cap_on_head(m, &m.objects[id]);
    let fast = is_mario_moving_fast_or_in_air(m, 10.0);
    let mario_fvel = m.forward_vel;
    let o = &mut m.objects[id];
    let mut flee = true;
    let mut goal = to_s16(o.angle_to_mario + 0x8000 as f64) as f64;
    if has_cap {
        flee = false;
        goal = o.angle_to_mario;
    }
    if o.timer == 0.0 {
        o.set_var("chaseFleeRange", random_float() * 100.0 + 350.0);
    }
    init_anim(o, UKIKI_ANIM_RUN);
    rotate_yaw_toward(o, goal, 0x800 as f64);
    // @bug (Ukikispeedia) cur_obj_set_vel_from_mario_vel(20.0f, 0.9f): 0.9 * Mario's forward speed.
    let base = 20.0 * 0.9;
    o.forward_vel = if mario_fvel < base { base } else { mario_fvel * 0.9 };
    let range = o.var("chaseFleeRange");
    if flee {
        if o.distance_to_mario > range {
            o.action = UKIKI_ACT_TURN_TO_MARIO;
        }
    } else if o.distance_to_mario < range {
        o.action = UKIKI_ACT_TURN_TO_MARIO;
    }
    if flee && o.distance_to_mario < 200.0 {
        let f = phys::flags(o);
        if f & MOVE_HIT_WALL != 0 && fast {
            o.action = UKIKI_ACT_JUMP;
            o.move_yaw = o.var("wallAngle");
        } else if f & MOVE_HIT_EDGE != 0 && fast {
            o.action = UKIKI_ACT_JUMP;
            o.move_yaw = to_s16(o.move_yaw + 0x8000 as f64) as f64;
        }
    }
}

/// `ukiki_act_jump`: a jump over Mario after reaching a wall or edge.
fn act_jump(o: &mut SimObject) {
    o.forward_vel = 10.0;
    become_intangible(o);
    if o.action_state == 0.0 {
        if o.timer == 0.0 {
            // cur_obj_set_y_vel_and_animation
            o.vel[1] = random_float() * 10.0 + 45.0;
            init_anim(o, UKIKI_ANIM_JUMP);
        } else if phys::flags(o) & (MASK_ON_GROUND | MOVE_AT_WATER_SURFACE | MOVE_UNDERWATER_ON_GROUND) != 0 {
            o.action_state += 1.0;
            o.vel[1] = 0.0;
        }
    } else {
        o.forward_vel = 0.0;
        init_anim(o, UKIKI_ANIM_LAND);
        become_tangible(o);
        if anim::near_end(o) {
            o.action = UKIKI_ACT_RUN;
        }
    }
}

/// object_helpers.c `cur_obj_follow_path` on [`CAGE_PATH`] (`pathPrev` = oPathedPrevWaypoint index):
/// returns (reached the end, `oPathedTargetYaw`).
fn follow_cage_path(o: &mut SimObject) -> (bool, f64) {
    let last = (o.var("pathPrev") as usize).min(CAGE_PATH.len() - 1);
    let target = if last + 1 < CAGE_PATH.len() { last + 1 } else { 0 };
    let (a, b) = (CAGE_PATH[last], CAGE_PATH[target]);
    let prev_to_next = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let obj_to_next = [b[0] - o.pos[0], b[1] - o.pos[1], b[2] - o.pos[2]];
    let yaw = atan2s(obj_to_next[2], obj_to_next[0]) as f64;
    let dot = prev_to_next[0] * obj_to_next[0] + prev_to_next[1] * obj_to_next[1] + prev_to_next[2] * obj_to_next[2];
    if dot <= 0.0 {
        o.set_var("pathPrev", target as f64);
        return (target + 1 == CAGE_PATH.len(), yaw);
    }
    (false, yaw)
}

/// The cage (`cur_obj_nearest_object_with_behavior(bhvUkikiCageChild)->parentObj`).
fn find_cage(m: &MarioRuntime, o: &SimObject) -> Option<ObjId> {
    let mut best: Option<(f64, usize)> = None;
    for (i, c) in m.objects.objects.iter().enumerate() {
        if c.active && c.behavior == K::UkikiCage {
            let d = dist_3d(&c.pos, &o.pos);
            if best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
    }
    best.map(|(_, i)| ObjId(i as u32))
}

/// `ukiki_act_go_to_cage`: travel to the cage, wait for Mario, jump on it and ride it down.
fn act_go_to_cage(id: ObjId, m: &mut MarioRuntime) {
    let cage = find_cage(m, &m.objects[id]);
    let (lat_dist, yaw_to_cage) = match cage {
        Some(c) => {
            let (o, c) = (&m.objects[id], &m.objects[c]);
            let lat = ((c.pos[0] - o.pos[0]).powi(2) + (c.pos[2] - o.pos[2]).powi(2)).sqrt();
            (lat, atan2s(c.pos[2] - o.pos[2], c.pos[0] - o.pos[0]) as f64)
        }
        None => (0.0, 0.0),
    };
    become_intangible(&mut m.objects[id]);
    let mut sub = m.objects[id].action_state as i32;
    if sub == UKIKI_SUB_ACT_CAGE_RUN_TO_CAGE {
        let o = &mut m.objects[id];
        init_anim(o, UKIKI_ANIM_RUN);
        let (end, target_yaw) = follow_cage_path(o);
        if !end {
            o.forward_vel = 10.0;
            rotate_yaw_toward(o, target_yaw, 0x400 as f64);
            o.pos[1] = phys::floor_height(o);
        } else {
            o.forward_vel = 0.0;
            o.action_state += 1.0;
        }
        return;
    }
    if sub == UKIKI_SUB_ACT_CAGE_WAIT_FOR_MARIO {
        let o = &mut m.objects[id];
        init_anim(o, UKIKI_ANIM_JUMP_CLAP);
        let a = o.angle_to_mario;
        rotate_yaw_toward(o, a, 0x400 as f64);
        if can_mario_activate_textbox(m, &m.objects[id], 200.0, 30.0) {
            m.objects[id].action_state += 1.0; // fallthrough
            sub = UKIKI_SUB_ACT_CAGE_TALK_TO_MARIO;
        } else {
            return;
        }
    }
    match sub {
        UKIKI_SUB_ACT_CAGE_TALK_TO_MARIO => {
            init_anim(&mut m.objects[id], UKIKI_ANIM_HANDSTAND);
            if update_dialog_with_cutscene(m, id, "dialogState", MARIO_DIALOG_LOOK_DOWN, true, DIALOG_080) {
                m.objects[id].action_state += 1.0;
            }
        }
        UKIKI_SUB_ACT_CAGE_TURN_TO_CAGE => {
            let o = &mut m.objects[id];
            init_anim(o, UKIKI_ANIM_RUN);
            if rotate_yaw_toward(o, yaw_to_cage, 0x400 as f64) {
                o.forward_vel = 10.0;
                o.action_state += 1.0;
            }
        }
        UKIKI_SUB_ACT_CAGE_JUMP_TO_CAGE => {
            let o = &mut m.objects[id];
            o.vel[1] = 55.0;
            init_anim(o, UKIKI_ANIM_JUMP);
            o.forward_vel = 36.0;
            o.action_state += 1.0;
        }
        UKIKI_SUB_ACT_CAGE_LAND_ON_CAGE => {
            let o = &mut m.objects[id];
            if lat_dist < 50.0 {
                o.forward_vel = 0.0;
            }
            if phys::flags(o) & MOVE_LANDED != 0 {
                let pos = o.pos;
                init_anim(o, UKIKI_ANIM_JUMP_CLAP);
                o.action_state += 1.0;
                o.set_var("spinTimer", 32.0);
                o.forward_vel = 0.0;
                // play_puzzle_jingle
                m.objects.sound(pos, "PUZZLE_JINGLE");
                if let Some(c) = cage {
                    m.objects[c].set_var("nextAction", UKIKI_CAGE_ACT_SPIN as f64);
                }
            }
        }
        UKIKI_SUB_ACT_CAGE_SPIN_ON_CAGE => {
            let o = &mut m.objects[id];
            o.move_yaw = to_s16(o.move_yaw + 0x800 as f64) as f64;
            let t = o.var("spinTimer") - 1.0;
            o.set_var("spinTimer", t);
            if t < 0.0 {
                o.action_state += 1.0;
                if let Some(c) = cage {
                    m.objects[c].set_var("nextAction", UKIKI_CAGE_ACT_FALL as f64);
                }
            }
        }
        UKIKI_SUB_ACT_CAGE_DESPAWN => {
            if m.objects[id].pos[1] < -1300.0 {
                mark_for_deletion(&mut m.objects[id]);
            }
        }
        _ => {}
    }
}

/// `cage_ukiki_held_loop`.
fn cage_ukiki_held_loop(id: ObjId, m: &mut MarioRuntime) {
    let o = &m.objects[id];
    if o.pos[1] - o.home[1] > -100.0 {
        let state = o.var("textState");
        if state == UKIKI_TEXT_DEFAULT {
            if set_mario_npc_dialog(m, id, MARIO_DIALOG_LOOK_UP) == MARIO_DIALOG_STATUS_SPEAK {
                // create_dialog_box_with_response(DIALOG_079)
                m.cut.dialog_response = DIALOG_RESPONSE_NONE;
                m.cut.dialog_open = true;
                m.cut.cue(CutCue::DialogWithResponse(DIALOG_079));
                m.objects[id].set_var("textState", UKIKI_TEXT_CAGE_TEXTBOX);
            }
        } else if state == UKIKI_TEXT_CAGE_TEXTBOX {
            if m.cut.dialog_response != DIALOG_RESPONSE_NONE {
                set_mario_npc_dialog(m, id, MARIO_DIALOG_STOP);
                // gDialogResponse goes back to NONE once the box finished closing.
                let yes = m.cut.dialog_response == DIALOG_RESPONSE_YES;
                m.cut.dialog_response = DIALOG_RESPONSE_NONE;
                let o = &mut m.objects[id];
                if yes {
                    o.interact_subtype |= INT_SUBTYPE_DROP_IMMEDIATELY;
                    o.set_var("textState", UKIKI_TEXT_GO_TO_CAGE);
                } else {
                    o.set_var("textState", UKIKI_TEXT_DO_NOT_LET_GO);
                    o.set_var("textboxTimer", 60.0);
                }
            }
        } else if state == UKIKI_TEXT_DO_NOT_LET_GO {
            // Pester Mario with textboxes to discourage walking far.
            let o = &mut m.objects[id];
            let t = o.var("textboxTimer");
            o.set_var("textboxTimer", t - 1.0);
            if t < 0.0 {
                o.set_var("textState", UKIKI_TEXT_DEFAULT);
            }
        }
    } else {
        // If ukiki is far below his home, stop him from trying to walk to the cage and getting stuck.
        let o = &mut m.objects[id];
        o.set_var("textState", UKIKI_TEXT_DEFAULT);
        o.timer = 0.0;
        o.action = UKIKI_ACT_WAIT_TO_RESPAWN;
    }
}

/// `cap_ukiki_held_loop`.
fn cap_ukiki_held_loop(id: ObjId, m: &mut MarioRuntime) {
    let state = m.objects[id].var("textState");
    if state == UKIKI_TEXT_DEFAULT {
        if mario_lose_cap_to_enemy(m, 2) {
            let o = &mut m.objects[id];
            o.set_var("textState", UKIKI_TEXT_STEAL_CAP);
            o.set_var("hasCap", (o.var("hasCap") as i32 | UKIKI_CAP_ON as i32) as f64);
        }
    } else if state == UKIKI_TEXT_STEAL_CAP {
        if update_dialog(m, id, MARIO_DIALOG_LOOK_UP, DIALOG_FLAG_TEXT_DEFAULT, DIALOG_100) != 0 {
            let o = &mut m.objects[id];
            o.interact_subtype |= INT_SUBTYPE_DROP_IMMEDIATELY;
            o.set_var("textState", UKIKI_TEXT_STOLE_CAP);
        }
    } else if state == UKIKI_TEXT_HAS_CAP {
        if update_dialog(m, id, MARIO_DIALOG_LOOK_UP, DIALOG_FLAG_TEXT_DEFAULT | DIALOG_FLAG_TIME_STOP_ENABLED, DIALOG_101) != 0 {
            mario_retrieve_cap(m);
            set_mario_npc_dialog(m, id, MARIO_DIALOG_STOP);
            let o = &mut m.objects[id];
            o.set_var("hasCap", (o.var("hasCap") as i32 & !(UKIKI_CAP_ON as i32)) as f64);
            o.set_var("textState", UKIKI_TEXT_GAVE_CAP_BACK);
        }
    } else if state == UKIKI_TEXT_GAVE_CAP_BACK {
        let o = &mut m.objects[id];
        o.set_var("textState", UKIKI_TEXT_DEFAULT);
        o.action = UKIKI_ACT_IDLE;
    }
}

// ---------------------------------------------------------------------------------------------
// ukiki_cage.inc.c
// ---------------------------------------------------------------------------------------------

/// bhvUkikiCage: SET_HOME, SET_FLOAT(oCollisionDistance, 20000), SET_OBJ_PHYSICS(30, -400, -50, 1000, 1000, 200).
/// The two SPAWN_CHILDs run on the first update (the cage star; the `bhvUkikiCageChild` marker is not needed,
/// Ukiki finds the cage itself).
pub fn init_cage(o: &mut SimObject) {
    phys::set_physics(o, 30.0, -400.0, -50.0, 1000.0, 1000.0, 200.0);
    o.visible = true;
    o.action = UKIKI_CAGE_ACT_WAIT_FOR_UKIKI;
    o.prev_action = UKIKI_CAGE_ACT_WAIT_FOR_UKIKI;
}

/// `bhv_ukiki_cage_loop`.
pub fn cage(id: ObjId, m: &mut MarioRuntime) {
    if !m.objects[id].has_var("starChild") {
        m.objects[id].set_var("starChild", 1.0);
        let param = m.objects[id].param;
        spawn_child(m, id, K::UkikiCageStar, Some(STAR_MODEL), param);
    }
    let world = m.world.clone();
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        let next = m.objects[id].var("nextAction") as i32;
        match m.objects[id].action {
            UKIKI_CAGE_ACT_WAIT_FOR_UKIKI => {
                // ukiki_cage_act_wait_for_ukiki
                if next != UKIKI_CAGE_ACT_WAIT_FOR_UKIKI {
                    m.objects[id].action = UKIKI_CAGE_ACT_SPIN;
                }
                load_object_collision_model(m, id, CAGE_COLLISION, 20000.0);
            }
            UKIKI_CAGE_ACT_SPIN => {
                // ukiki_cage_act_spin
                let o = &mut m.objects[id];
                if next != UKIKI_CAGE_ACT_SPIN {
                    o.action = UKIKI_CAGE_ACT_FALL;
                }
                o.move_yaw = to_s16(o.move_yaw + 0x800 as f64) as f64;
                load_object_collision_model(m, id, CAGE_COLLISION, 20000.0);
            }
            UKIKI_CAGE_ACT_FALL => {
                // ukiki_cage_act_fall: no collision is loaded any more (the dynamic surfaces are rebuilt every frame).
                unload_object_collision(m, id);
                let o = &mut m.objects[id];
                phys::update_floor_and_walls(o, &world);
                phys::move_standard(o, &world, 78.0);
                if phys::flags(o) & (MOVE_LANDED | MOVE_ENTERED_WATER) != 0 {
                    o.action = UKIKI_CAGE_ACT_HIDE;
                }
            }
            _ => {
                // ukiki_cage_act_hide
                unload_object_collision(m, id);
                m.objects[id].visible = false;
            }
        }
    });
}

/// bhvUkikiCageStar: SET_HOME.
pub fn init_cage_star(o: &mut SimObject) {
    o.visible = true;
    o.action = UKIKI_CAGE_STAR_ACT_IN_CAGE;
}

/// `bhv_ukiki_cage_star_loop`: sits in the cage, and when the cage hides itself spawns the star.
pub fn cage_star(id: ObjId, m: &mut MarioRuntime) {
    begin(m, id);
    let parent = m.objects[id].link;
    match m.objects[id].action {
        UKIKI_CAGE_STAR_ACT_IN_CAGE => {
            if let Some(p) = parent {
                let (pos, param, hidden) = { let c = &m.objects[p]; (c.pos, c.param, c.action == UKIKI_CAGE_ACT_HIDE) };
                let o = &mut m.objects[id];
                o.pos = pos; // obj_copy_pos
                o.param = param; // obj_copy_behavior_params
                if hidden {
                    o.action = UKIKI_CAGE_STAR_ACT_SPAWN_STAR;
                }
            }
        }
        _ => {
            // obj_mark_for_deletion; spawn_mist_particles; spawn_triangle_break_particles; spawn_default_star
            let pos = m.objects[id].pos;
            m.objects.events.push(crate::object_manager::ObjectEvent::Break { pos, wall: None });
            crate::behaviors::r2a::helpers::spawn_default_star(m, id, CAGE_STAR_POS);
            mark_for_deletion(&mut m.objects[id]);
            return;
        }
    }
    // Spin to look like a star.
    let o = &mut m.objects[id];
    o.face_yaw = to_s16(o.face_yaw + 0x400 as f64) as f64;
    finish(m, id, 0);
}
