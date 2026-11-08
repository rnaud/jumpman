//! Mips the rabbit (`bhvMips`), the castle basement NPC. Port of `src/game/behaviors/mips.inc.c`.
//! Round 2 part B added the tail: the star-count gating of `bhv_mips_init` (the app publishes the star
//! count and the two Mips flags as the vars `stars` / `collected1` / `collected2`, see
//! `progress::castle_npcs`; the first update applies the original's choice of MIPS_BP_15_STARS or
//! MIPS_BP_50_STARS or hides Mips), the held dialog (`bhv_mips_held`: DIALOG_084 / DIALOG_162, then
//! `INT_SUBTYPE_DROP_IMMEDIATELY`) and the star spawned from `bhv_mips_act_idle`
//! (`STAR_INDEX_ACT_4 + param`, castle star acts 4 and 5).
//! The ten escape paths come from `inside_castle_seg7_trajectory_mips`, concatenated in the
//! object's `ball_route` with [`PATH_BREAK`] between paths.
use super::anim::{self, spec, AnimSpec};
use super::*;

pub const ACT_WAIT_FOR_NEARBY_MARIO: i32 = 0;
pub const ACT_FOLLOW_PATH: i32 = 1;
pub const ACT_WAIT_FOR_ANIMATION_DONE: i32 = 2;
pub const ACT_FALL_DOWN: i32 = 3;
pub const ACT_IDLE: i32 = 4;

/// Separator point between two trajectories in the flattened path list.
pub const PATH_BREAK: Vec3 = [0.0, -1.0e6, 0.0];

/// `mips_seg6_anims_06015634`.
pub const ANIMS: [AnimSpec; 5] = [spec(0, 0, 0x55, false), spec(0, 0, 0x0C, false), spec(0, 0, 0x50, false), spec(0, 0, 0x6E, false), spec(0, 0, 0x1E, false)];
pub const ANIM_NAMES: [&str; 5] = [
    "mips_seg6_anim_06014B94", "mips_seg6_anim_060139F8", "mips_seg6_anim_06013248", "mips_seg6_anim_0601561C", "mips_seg6_anim_0601369C",
];

/// bhvMips: SET_INT(oInteractType, GRABBABLE), SET_HITBOX(50, 75); bhv_mips_init (US values).
pub fn init(o: &mut SimObject) {
    o.hitbox = hitbox(INTERACT_GRABBABLE, 0.0, 0.0, 0.0, 0.0, 50.0, 75.0, 50.0, 75.0);
    o.set_var("realInteract", INTERACT_GRABBABLE as f64);
    o.interact_subtype = INT_SUBTYPE_HOLDABLE_NPC;
    o.gravity = 15.0;
    o.friction = 0.89;
    o.buoyancy = 1.2;
    o.set_var("forwardVelocity", 40.0); // MIPS_BP_15_STARS (bhv_mips_init picks 40 or 45 in `gate`)
    o.visible = true;
    o.action = ACT_WAIT_FOR_NEARBY_MARIO;
    o.prev_action = ACT_WAIT_FOR_NEARBY_MARIO;
    anim::init(o, 0, ANIMS[0]);
}

/// The trajectories packed in `ball_route`.
fn paths(o: &SimObject) -> Vec<Vec<Vec3>> {
    let mut out = vec![vec![]];
    if let Some(r) = &o.ball_route {
        for p in &r.waypoints {
            if p[1] <= -1.0e5 { out.push(vec![]); } else { out.last_mut().unwrap().push(*p); }
        }
    }
    out.retain(|p| !p.is_empty());
    out
}

/// bhv_mips_find_furthest_waypoint_to_mario: the path whose first waypoint within 800 of Mips is
/// furthest (horizontally) from Mario.
fn find_furthest_path(o: &SimObject, mario: &Vec3) -> Option<usize> {
    let mut best = None;
    let mut best_dist = -10000.0;
    for (i, path) in paths(o).iter().enumerate() {
        let w = path[0];
        if dist_3d(&o.pos, &w) < 800.0 {
            let d = (w[0] - mario[0]).powi(2) + (w[2] - mario[2]).powi(2);
            if best_dist < d {
                best_dist = d;
                best = Some(i);
            }
        }
    }
    best
}

/// cur_obj_follow_path for a flattened path: updates `pathPrev`, returns (reached_end, target_yaw).
fn follow_path(o: &mut SimObject, path: &[Vec3]) -> (bool, f64) {
    let prev = (o.var("pathPrev") as usize).min(path.len() - 1);
    let last = path[prev];
    let (target, _wraps) = if prev + 1 < path.len() { (path[prev + 1], false) } else { (path[0], true) };
    let prev_to_next = [target[0] - last[0], target[1] - last[1], target[2] - last[2]];
    let obj_to_next = [target[0] - o.pos[0], target[1] - o.pos[1], target[2] - o.pos[2]];
    let yaw = atan2s(obj_to_next[2], obj_to_next[0]) as f64;
    let dot = prev_to_next[0] * obj_to_next[0] + prev_to_next[1] * obj_to_next[1] + prev_to_next[2] * obj_to_next[2];
    if dot <= 0.0 {
        let next = if prev + 1 < path.len() { prev + 1 } else { 0 };
        o.set_var("pathPrev", next as f64);
        // PATH_REACHED_END when the new previous waypoint is the last one.
        return (next + 1 >= path.len(), yaw);
    }
    (false, yaw)
}

/// `bhv_mips_init`'s star-count check; false when Mips has nothing to give (he is hidden then).
fn gate(o: &mut SimObject) -> bool {
    if o.has_var("gated") {
        return !o.has_var("hidden");
    }
    if !o.has_var("stars") {
        // No save data published (headless sims, the TS goldens): Mips is always there, as before.
        return true;
    }
    o.set_var("gated", 1.0);
    let stars = o.var("stars");
    if stars >= 15.0 && o.var("collected1") == 0.0 {
        o.set_var("starKind", 0.0); // MIPS_BP_15_STARS
        o.set_var("forwardVelocity", 40.0);
    } else if stars >= 50.0 && o.var("collected2") == 0.0 {
        o.set_var("starKind", 1.0); // MIPS_BP_50_STARS
        o.set_var("forwardVelocity", 45.0);
    } else {
        o.set_var("hidden", 1.0);
        o.deactivate(-1.0);
        return false;
    }
    true
}

pub const DIALOG_MIPS_15_STARS: u32 = 84;
pub const DIALOG_MIPS_50_STARS: u32 = 162;

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    if !gate(&mut m.objects[id]) {
        return;
    }
    let world = m.world.clone();
    let mpos = m.pos;
    let held = m.objects[id].held_state;
    match held {
        HELD_FREE => free(id, m, &world, &mpos),
        HELD_HELD => {
            // bhv_mips_held: invisible in the hand, held animation, intangible.
            let o = &mut m.objects[id];
            anim::init(o, 3, ANIMS[3]);
            become_intangible(o);
            o.visible = false;
            if m.objects[id].var("starStatus") == 0.0 {
                held_dialog(id, m);
            }
        }
        HELD_THROWN => {
            // bhv_mips_thrown
            let o = &mut m.objects[id];
            o.visible = true;
            o.held_state = HELD_FREE;
            anim::init(o, 2, ANIMS[2]);
            become_tangible(o);
            o.forward_vel = 25.0;
            o.vel[1] = 20.0;
            o.action = ACT_FALL_DOWN;
        }
        _ => {
            // bhv_mips_dropped
            let o = &mut m.objects[id];
            o.visible = true;
            anim::init(o, 0, ANIMS[0]);
            o.held_state = HELD_FREE;
            become_tangible(o);
            o.forward_vel = 3.0;
            o.action = ACT_IDLE;
        }
    }
    let o = &mut m.objects[id];
    if o.held_state == HELD_FREE && o.action != ACT_FALL_DOWN { o.face_yaw = o.move_yaw; }
    end_frame(o);
}

fn free(id: ObjId, m: &mut MarioRuntime, world: &crate::collision::CollisionWorld, mpos: &Vec3) {
    let o = &mut m.objects[id];
    match o.action {
        // bhv_mips_act_wait_for_nearby_mario
        ACT_WAIT_FOR_NEARBY_MARIO => {
            o.forward_vel = 0.0;
            o.step_physics(world, None);
            if dist_3d(&o.pos, mpos) < 500.0 {
                match find_furthest_path(o, mpos) {
                    None => o.action = ACT_WAIT_FOR_ANIMATION_DONE,
                    Some(i) => {
                        o.set_var("path", i as f64);
                        o.set_var("pathPrev", 0.0);
                        anim::init(o, 1, ANIMS[1]);
                        o.action = ACT_FOLLOW_PATH;
                    }
                }
            }
        }
        // bhv_mips_act_follow_path
        ACT_FOLLOW_PATH => {
            let path = paths(o).get(o.var("path") as usize).cloned().unwrap_or_default();
            let (end, yaw) = if path.is_empty() { (true, o.move_yaw) } else { follow_path(o, &path) };
            o.forward_vel = o.var("forwardVelocity");
            o.move_yaw = yaw;
            let flags = o.step_physics(world, None);
            if end {
                anim::init(o, 0, ANIMS[0]);
                o.action = ACT_WAIT_FOR_NEARBY_MARIO;
            }
            let pos = o.pos;
            if anim::near_end(o) {
                let water = flags & OBJ_COL_FLAG_UNDERWATER != 0;
                m.objects.sound(pos, if water { "OBJ_MIPS_RABBIT_WATER" } else { "OBJ_MIPS_RABBIT" }); // SOUND_OBJ_MIPS_RABBIT(_WATER)
            }
        }
        // bhv_mips_act_wait_for_animation_done
        ACT_WAIT_FOR_ANIMATION_DONE => {
            if anim::near_end(o) {
                anim::init(o, 0, ANIMS[0]);
                o.action = ACT_IDLE;
            }
        }
        // bhv_mips_act_fall_down
        ACT_FALL_DOWN => {
            let flags = o.step_physics(world, None);
            o.set_var("animFrame", 0.0);
            o.set_var("animAssist", 0.0);
            if flags & OBJ_COL_FLAG_GROUNDED != 0 {
                o.action = ACT_WAIT_FOR_ANIMATION_DONE;
                o.move_yaw = o.face_yaw;
            }
        }
        // bhv_mips_act_idle: the star is spawned when he was just picked up for the first time.
        _ => {
            o.forward_vel = 0.0;
            o.step_physics(world, None);
            if o.var("starStatus") == 1.0 {
                // bhv_spawn_star_no_level_exit(STAR_INDEX_ACT_4 + oBhvParams2ndByte)
                let (pos, act) = (o.pos, 4 + o.var("starKind") as i32);
                o.set_var("starStatus", 2.0); // MIPS_STAR_STATUS_ALREADY_SPAWNED_STAR
                m.objects.castle_star(pos, act);
            }
        }
    }
}

/// The `oMipsStarStatus == MIPS_STAR_STATUS_HAVENT_SPAWNED_STAR` part of `bhv_mips_held`.
fn held_dialog(id: ObjId, m: &mut MarioRuntime) {
    use super::super::part_b::npc::*;
    if set_mario_npc_dialog(m, id, MARIO_DIALOG_LOOK_FRONT) != MARIO_DIALOG_STATUS_SPEAK {
        return;
    }
    // cutscene_object_with_dialog(CUTSCENE_DIALOG, o, dialogID): open the box, wait until it closes.
    let dialog_id = if m.objects[id].var("starKind") == 0.0 { DIALOG_MIPS_15_STARS } else { DIALOG_MIPS_50_STARS };
    if m.objects[id].var("dialogOpened") == 0.0 {
        m.objects[id].set_var("dialogOpened", 1.0);
        m.cut.dialog_open = true;
        m.cut.cue(crate::cutscene_ext::CutCue::Dialog(dialog_id));
    } else if !m.cut.dialog_open {
        let o = &mut m.objects[id];
        o.interact_subtype |= INT_SUBTYPE_DROP_IMMEDIATELY;
        o.set_var("starStatus", 1.0); // MIPS_STAR_STATUS_SHOULD_SPAWN_STAR
        set_mario_npc_dialog(m, id, MARIO_DIALOG_STOP);
    }
}

/// bhvBooInCastle is not ported (see the module docs of `behaviors/g1`): it only exists to lure
/// Mario into the castle courtyard, which is not part of the exported levels.
pub fn init_boo_in_castle(o: &mut SimObject) { o.visible = false; }
pub fn boo_in_castle_update(_id: ObjId, _m: &mut MarioRuntime) {}
