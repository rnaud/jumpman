//! Piranha Plant (`bhvPiranhaPlant`), the Whomp's Fortress lullaby plants.
//! Port of `src/game/behaviors/piranha_plant.inc.c` (US build: no BUGFIX_* branches).
use super::anim::{self, spec, AnimSpec};
use super::*;

pub const ACT_IDLE: i32 = 0;
pub const ACT_SLEEPING: i32 = 1;
pub const ACT_BITING: i32 = 2;
pub const ACT_WOKEN_UP: i32 = 3;
pub const ACT_STOPPED_BITING: i32 = 4;
pub const ACT_ATTACKED: i32 = 5;
pub const ACT_SHRINK_AND_DIE: i32 = 6;
pub const ACT_WAIT_TO_RESPAWN: i32 = 7;
pub const ACT_RESPAWN: i32 = 8;

/// `piranha_plant_seg6_anims_0601C31C` (data from assets/animations.json headers): index ->
/// (start, loopStart, loopEnd, noloop). The glb clip order is by address, the table order differs
/// for entries 6 and 7.
pub const ANIMS: [AnimSpec; 10] = [
    spec(0, 0, 0x5A, false), spec(0, 0, 0x04, false), spec(0, 0, 0x46, true), spec(0, 0, 0x13, false),
    spec(0, 0, 0x48, false), spec(0, 0, 0x2A, false), spec(0, 0, 0x1A, false), spec(0, 0, 0x4B, false),
    spec(0, 0, 0x38, false), spec(0, 0, 0x5A, false),
];
/// Names of the table entries, for the renderer (`ANIM_TABLE` order of the decomp).
pub const ANIM_NAMES: [&str; 10] = [
    "piranha_plant_seg6_anim_06017C38", "piranha_plant_seg6_anim_06017D88", "piranha_plant_seg6_anim_060187B0",
    "piranha_plant_seg6_anim_06018BA8", "piranha_plant_seg6_anim_06019854", "piranha_plant_seg6_anim_0601A014",
    "piranha_plant_seg6_anim_0601AF34", "piranha_plant_seg6_anim_0601AAE4", "piranha_plant_seg6_anim_0601B634",
    "piranha_plant_seg6_anim_0601C304",
];

/// bhvPiranhaPlant script: SET_INTERACT_TYPE(DAMAGE), SET_HITBOX(100, 200), SET_HURTBOX(50, 200),
/// oDamageOrCoinValue 3, oNumLootCoins 5.
pub fn init(o: &mut SimObject) {
    o.hitbox = hitbox(INTERACT_DAMAGE, 0.0, 3.0, 0.0, 5.0, 100.0, 200.0, 50.0, 200.0);
    o.set_var("realInteract", INTERACT_DAMAGE as f64);
    o.set_var("pureDamage", 1.0);
    o.set_var("scale", 1.0);
    o.visible = true;
    o.action = ACT_IDLE;
    o.prev_action = ACT_IDLE;
    anim::init(o, 0, ANIMS[0]);
}

fn set_box(o: &mut SimObject, radius: f64, height: f64, hurt_radius: f64, hurt_height: f64) {
    o.hitbox.radius = radius;
    o.hitbox.height = height;
    o.hitbox.hurtbox_radius = hurt_radius;
    o.hitbox.hurtbox_height = hurt_height;
}

/// piranha_plant_check_interactions
fn check_interactions(id: ObjId, m: &mut MarioRuntime) -> bool {
    let o = &mut m.objects[id];
    if o.interact_status & INT_STATUS_INTERACTED == 0 { return false; }
    if o.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        let pos = o.pos;
        o.action = ACT_ATTACKED;
        o.interact_status = 0;
        m.objects.sound(pos, "OBJ2_PIRANHA_PLANT_DYING");
    } else {
        o.action = ACT_WOKEN_UP;
        o.interact_status = 0;
    }
    true
}

/// mario_moving_fast_enough_to_make_piranha_plant_bite
fn mario_fast(m: &MarioRuntime) -> bool { m.vel[1] > 10.0 || m.forward_vel > 10.0 }

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let dist = m.objects[id].distance_to_mario;
    let angle_to_mario = m.objects[id].angle_to_mario;
    let fast = mario_fast(m);
    let mario_y = m.pos[1];
    let metal = m.metal_cap_timer > 0.0;
    match m.objects[id].action {
        // piranha_plant_act_idle
        ACT_IDLE => {
            let o = &mut m.objects[id];
            become_intangible(o);
            anim::init(o, 8, ANIMS[8]);
            o.scale = 1.0;
            if dist < 1200.0 { o.action = ACT_SLEEPING; }
        }
        // piranha_plant_act_sleeping
        ACT_SLEEPING => {
            let o = &mut m.objects[id];
            become_tangible(o);
            set_interact(o, INTERACT_BOUNCE_TOP);
            anim::init(o, 8, ANIMS[8]);
            set_box(o, 250.0, 200.0, 150.0, 100.0);
            if dist < 400.0 && fast { o.action = ACT_WOKEN_UP; }
            check_interactions(id, m);
        }
        // piranha_plant_act_woken_up
        ACT_WOKEN_UP => {
            if !check_interactions(id, m) && m.objects[id].action_timer > 10.0 {
                m.objects[id].action = ACT_BITING;
            }
        }
        // piranha_plant_act_biting
        ACT_BITING => {
            let o = &mut m.objects[id];
            become_tangible(o);
            set_interact(o, INTERACT_DAMAGE);
            anim::init(o, 0, ANIMS[0]);
            set_box(o, 150.0, 100.0, 150.0, 100.0);
            o.move_yaw = approach_s16(o.move_yaw, angle_to_mario, 0x400 as f64);
            if dist > 500.0 && anim::near_end(o) { o.action = ACT_STOPPED_BITING; }
            if o.interact_status & INT_STATUS_INTERACTED != 0 && metal { o.action = ACT_ATTACKED; }
        }
        // piranha_plant_act_stopped_biting
        ACT_STOPPED_BITING => {
            let o = &mut m.objects[id];
            become_intangible(o);
            anim::init(o, 6, ANIMS[6]);
            if anim::near_end(o) { o.action = ACT_SLEEPING; }
            if dist < 400.0 && fast { o.action = ACT_BITING; }
        }
        // piranha_plant_attacked
        ACT_ATTACKED => {
            let o = &mut m.objects[id];
            become_intangible(o);
            anim::init(o, 2, ANIMS[2]);
            o.interact_status = 0;
            if anim::near_end(o) { o.action = ACT_SHRINK_AND_DIE; }
        }
        // piranha_plant_act_shrink_and_die
        ACT_SHRINK_AND_DIE => {
            let pos = m.objects[id].pos;
            let o = &mut m.objects[id];
            if o.action_timer == 0.0 {
                o.set_var("scale", 1.0);
                m.objects.sound(pos, "OBJ_ENEMY_DEFEAT_SHRINK");
            }
            let o = &mut m.objects[id];
            let s = o.var("scale");
            if s > 0.0 {
                o.set_var("scale", s - 0.04);
            } else {
                o.set_var("scale", 0.0);
                o.action = ACT_WAIT_TO_RESPAWN;
                // cur_obj_spawn_loot_blue_coin (oNumLootCoins >= 5)
                m.objects.spawn_coin(pos, true, 5.0);
            }
            let o = &mut m.objects[id];
            o.scale = o.var("scale");
        }
        // piranha_plant_act_wait_to_respawn
        ACT_WAIT_TO_RESPAWN => {
            if dist > 1200.0 { m.objects[id].action = ACT_RESPAWN; }
        }
        // piranha_plant_act_respawn
        ACT_RESPAWN => {
            let o = &mut m.objects[id];
            anim::init(o, 8, ANIMS[8]);
            if o.action_timer == 0.0 { o.set_var("scale", 0.3); }
            let s = o.var("scale");
            if s < 1.0 {
                o.set_var("scale", s + 0.02);
            } else {
                o.set_var("scale", 1.0);
                o.action = ACT_IDLE;
            }
            o.scale = o.var("scale");
        }
        _ => {}
    }
    // bhv_piranha_plant_loop: in WF all plants are hidden once Mario is high up.
    let o = &mut m.objects[id];
    o.visible = !(mario_y > 3400.0);
    o.interact_status = 0;
    o.face_yaw = o.move_yaw;
    end_frame(o);
}
