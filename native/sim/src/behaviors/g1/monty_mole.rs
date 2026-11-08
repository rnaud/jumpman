//! Monty Mole (`bhvMontyMole`), its holes (`bhvMontyMoleHole`) and thrown rocks
//! (`bhvMontyMoleRock`), Hazy Maze Cave. Port of `src/game/behaviors/monty_mole.inc.c`.
//! Not ported: the dirt particle bursts and the pebble / hole display lists (no glb exists for
//! them, the rock is drawn as a small sphere and the hole is invisible).
use super::anim::{self, spec, AnimSpec};
use super::*;

pub const ACT_SELECT_HOLE: i32 = 0;
pub const ACT_RISE_FROM_HOLE: i32 = 1;
pub const ACT_SPAWN_ROCK: i32 = 2;
pub const ACT_BEGIN_JUMP_INTO_HOLE: i32 = 3;
pub const ACT_THROW_ROCK: i32 = 4;
pub const ACT_JUMP_INTO_HOLE: i32 = 5;
pub const ACT_HIDE: i32 = 6;
pub const ACT_JUMP_OUT_OF_HOLE: i32 = 7;
pub const BP_NO_ROCK: f64 = 0.0;

/// `monty_mole_seg5_anims_05007248`.
pub const ANIMS: [AnimSpec; 10] = [
    spec(0, 0, 0x0C, false), spec(0, 0, 0x0C, false), spec(0, 0, 0x0A, false), spec(0, 0, 0x50, false), spec(1, 1, 0x0E, false),
    spec(0, 0, 0x0C, false), spec(0, 0, 0x0C, false), spec(0, 0, 0x14, false), spec(0, 0, 0x14, false), spec(0, 0, 0x01, false),
];
pub const ANIM_NAMES: [&str; 10] = [
    "monty_mole_seg5_anim_05004FEC", "monty_mole_seg5_anim_0500527C", "monty_mole_seg5_anim_050054B0", "monty_mole_seg5_anim_050062B0",
    "monty_mole_seg5_anim_050065C0", "monty_mole_seg5_anim_05006880", "monty_mole_seg5_anim_05006B10", "monty_mole_seg5_anim_05006DB8",
    "monty_mole_seg5_anim_05007230", "monty_mole_seg5_anim_050065D8",
];

/// sMontyMoleHitbox.
pub const MOLE_HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 0.0, 2.0, -1.0, 0.0, 70.0, 50.0, 30.0, 40.0);
/// sMontyMoleRockHitbox.
pub const ROCK_HITBOX: ObjectHitbox = hitbox(INTERACT_MR_BLIZZARD, 15.0, 1.0, 99.0, 0.0, 30.0, 15.0, 30.0, 15.0);
pub const ROCK_MODEL: &str = "actors/bowling_ball/bowling_ball_geo.glb";

/// bhvMontyMole: hidden and intangible, SCALE 150, graph y offset -60, wall radius 30.
pub fn init_mole(o: &mut SimObject) {
    o.hitbox = MOLE_HITBOX;
    o.set_var("realInteract", MOLE_HITBOX.interact_type as f64);
    o.hitbox.interact_type = 0; // oIntangibleTimer = -1
    o.scale = 1.5;
    o.graph_y_offset = -60.0;
    o.gravity = 0.0;
    o.friction = 0.0;
    o.visible = false;
    o.action = ACT_SELECT_HOLE;
    o.prev_action = ACT_SELECT_HOLE;
    anim::init(o, 3, ANIMS[3]);
}

/// bhvMontyMoleHole: DROP_TO_FLOOR (done on the first update), SCALE 150, no model.
pub fn init_hole(o: &mut SimObject) { o.visible = false; }

/// bhv_monty_mole_hole_update (cooldown only; the list building is done by scanning the arena).
pub fn hole_update(id: ObjId, m: &mut MarioRuntime) {
    if !m.objects[id].has_var("dropped") {
        let pos = m.objects[id].pos;
        let f = m.world.find_floor(pos[0], pos[1] + 100.0, pos[2]);
        let o = &mut m.objects[id];
        if f.surface.is_some() { o.pos[1] = f.height; o.home[1] = f.height; }
        o.set_var("dropped", 1.0);
    }
    let o = &mut m.objects[id];
    let cd = o.var("cooldown");
    if cd > 0.0 { o.set_var("cooldown", cd - 1.0); }
}

/// monty_mole_select_available_hole.
fn select_available_hole(m: &MarioRuntime, min_dist: f64) -> Option<ObjId> {
    let ok = |o: &SimObject| o.behavior == ObjectBehaviorKind::MontyMoleHole && o.active && o.var("cooldown") == 0.0 && o.distance_to_mario < 1500.0 && o.distance_to_mario > min_dist;
    let holes: Vec<ObjId> = m.objects.ids().filter(|i| ok(&m.objects[*i])).collect();
    if holes.is_empty() { return None; }
    let pick = ((random() * holes.len() as f64) as usize).min(holes.len() - 1);
    Some(holes[pick])
}

/// monty_mole_hide_in_hole.
fn hide_in_hole(id: ObjId, m: &mut MarioRuntime) {
    if let Some(h) = m.objects[id].link { m.objects[h].set_var("cooldown", 30.0); }
    let o = &mut m.objects[id];
    o.action = ACT_SELECT_HOLE;
    o.vel[1] = 0.0;
    become_intangible(o);
}

pub fn mole_update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let forward = m.forward_vel;
    let (dist, angle_to_mario) = { let o = &m.objects[id]; (o.distance_to_mario, o.angle_to_mario) };
    let floor = world.find_floor(m.objects[id].pos[0], m.objects[id].pos[1], m.objects[id].pos[2]);
    let floor_y = if floor.surface.is_some() { floor.height } else { m.objects[id].pos[1] };
    let height_rel = m.objects[id].pos[1] - floor_y;
    let norock = m.objects[id].param == BP_NO_ROCK;
    let on_ground = m.objects[id].var("mfGround") != 0.0;
    match m.objects[id].action {
        // monty_mole_act_select_hole
        ACT_SELECT_HOLE => {
            let min_dist = if !norock { 200.0 } else if forward < 8.0 { 100.0 } else { 500.0 };
            if let Some(h) = select_available_hole(m, min_dist) {
                let (hpos, hangle, hdist) = { let ho = &m.objects[h]; (ho.pos, ho.angle_to_mario, ho.distance_to_mario) };
                let _ = hdist;
                m.objects.sound(hpos, "OBJ2_MONTY_MOLE_APPEAR");
                m.objects[h].set_var("cooldown", -1.0);
                let o = &mut m.objects[id];
                o.link = Some(h);
                o.pos = hpos;
                o.set_var("facePitch", 0.0);
                o.move_yaw = hangle;
                if dist > 500.0 || min_dist > 100.0 || random() < 0.5 {
                    o.action = ACT_RISE_FROM_HOLE;
                    o.vel[1] = 3.0;
                    o.gravity = 0.0;
                } else {
                    o.action = ACT_JUMP_OUT_OF_HOLE;
                    o.vel[1] = 50.0;
                    o.gravity = 4.0;
                }
                o.visible = true;
                become_tangible(o);
            }
        }
        // monty_mole_act_rise_from_hole
        ACT_RISE_FROM_HOLE => {
            let o = &mut m.objects[id];
            anim::init(o, 1, ANIMS[1]);
            if height_rel >= 49.0 {
                o.pos[1] = floor_y + 50.0;
                o.vel[1] = 0.0;
                if anim::near_end(o) { o.action = ACT_SPAWN_ROCK; }
            }
        }
        // monty_mole_act_spawn_rock
        ACT_SPAWN_ROCK => {
            let o = &mut m.objects[id];
            anim::init(o, 2, ANIMS[2]);
            if anim::near_end(o) {
                if o.param != BP_NO_ROCK && abs_angle_diff(angle_to_mario, o.move_yaw) < 0x4000 as f64 {
                    let (pos, yaw) = (o.pos, o.move_yaw);
                    let mut s = ObjectSpec::new(ObjectBehaviorKind::MontyMoleRock, pos, yaw);
                    s.model = Some(ROCK_MODEL.to_string());
                    let rock = m.objects.spawn(&s);
                    m.objects[rock].link = Some(id);
                    let o = &mut m.objects[id];
                    o.links = Some(vec![rock]);
                    o.action = ACT_THROW_ROCK;
                } else {
                    o.action = ACT_BEGIN_JUMP_INTO_HOLE;
                }
            }
        }
        // monty_mole_act_begin_jump_into_hole
        ACT_BEGIN_JUMP_INTO_HOLE => {
            let o = &mut m.objects[id];
            anim::init(o, 3, ANIMS[3]);
            let facing = dist < 1000.0 && abs_angle_diff(o.move_yaw, angle_to_mario) < 0x4000 as f64;
            if anim::near_end(o) || facing {
                o.action = ACT_JUMP_INTO_HOLE;
                o.vel[1] = 40.0;
                o.gravity = 6.0;
            }
        }
        // monty_mole_act_throw_rock
        ACT_THROW_ROCK => {
            anim::init(&mut m.objects[id], 8, ANIMS[8]);
            if anim::check_frame(&m.objects[id], 10) {
                let pos = m.objects[id].pos;
                m.objects.sound(pos, "OBJ_MONTY_MOLE_ATTACK");
                m.objects[id].links = None; // o->prevObj = NULL releases the rock
            }
            let o = &mut m.objects[id];
            if anim::near_end(o) { o.action = ACT_BEGIN_JUMP_INTO_HOLE; }
        }
        // monty_mole_act_jump_into_hole
        ACT_JUMP_INTO_HOLE => {
            let o = &mut m.objects[id];
            anim::init(o, 0, ANIMS[0]);
            anim::extend_if_at_end(o);
            o.set_var("facePitch", -(atan2s(o.vel[1], -4.0) as f64));
            if o.vel[1] < 0.0 && height_rel < 120.0 {
                o.action = ACT_HIDE;
                o.gravity = 0.0;
            }
        }
        // monty_mole_act_hide
        ACT_HIDE => {
            let o = &mut m.objects[id];
            anim::init(o, 1, ANIMS[1]);
            if on_ground {
                o.visible = false;
                hide_in_hole(id, m);
            } else {
                let mut v = o.vel[1];
                approach_f32_ptr(&mut v, -4.0, 0.5);
                o.vel[1] = v;
            }
        }
        // monty_mole_act_jump_out_of_hole
        _ => {
            let o = &mut m.objects[id];
            if o.vel[1] > 0.0 {
                anim::init(o, 9, ANIMS[9]);
            } else {
                anim::init(o, 4, ANIMS[4]);
                anim::extend_if_at_end(o);
                if height_rel < 50.0 {
                    o.pos[1] = floor_y + 50.0;
                    o.action = ACT_BEGIN_JUMP_INTO_HOLE;
                    o.vel[1] = 0.0;
                    o.gravity = 0.0;
                }
            }
        }
    }
    // obj_check_attacks(&sMontyMoleHitbox, o->oAction)
    let action = m.objects[id].action;
    let pos = m.objects[id].pos;
    {
        let attack = check_attacks(id, m, action, "OBJ_DYING_ENEMY1");
        if attack != 0 {
            kill_streak(id, m, pos);
            if m.objects[id].active { hide_in_hole(id, m); }
            m.objects[id].links = None;
        }
    }
    let o = &mut m.objects[id];
    if o.active {
        let f = move_standard(o, &world, 78.0, false, 30.0);
        o.set_var("mfGround", f.on_ground as i32 as f64);
        o.face_yaw = o.move_yaw;
    }
    end_frame(o);
}

/// The sMontyMoleKillStreak bookkeeping; the statics live on the first hole.
fn kill_streak(id: ObjId, m: &mut MarioRuntime, pos: Vec3) {
    let Some(h) = m.objects.ids().find(|i| m.objects[*i].behavior == ObjectBehaviorKind::MontyMoleHole) else { return };
    let (mut streak, last) = { let o = &m.objects[h]; (o.var("killStreak"), [o.var("lastX"), o.var("lastY"), o.var("lastZ")]) };
    if streak != 0.0 {
        if dist_3d(&pos, &last) < 1500.0 {
            if streak == 7.0 {
                // play_puzzle_jingle + spawn_object(bhv1UpWalking)
                let mut s = ObjectSpec::new(ObjectBehaviorKind::OneUp, pos, 0.0);
                s.model = Some("actors/mushroom_1up/mushroom_1up_geo.glb".to_string());
                m.objects.spawn(&s);
            }
        } else {
            streak = 0.0;
        }
    }
    let _ = id;
    let o = &mut m.objects[h];
    o.set_var("killStreak", streak + 1.0);
    o.set_var("lastX", pos[0]);
    o.set_var("lastY", pos[1]);
    o.set_var("lastZ", pos[2]);
}

/// bhvMontyMoleRock: SET_OBJ_PHYSICS(wall radius 30, gravity -400, ...), SCALE 200, billboard.
pub fn init_rock(o: &mut SimObject) {
    o.hitbox = ROCK_HITBOX;
    o.set_var("realInteract", ROCK_HITBOX.interact_type as f64);
    o.gravity = 4.0;
    o.friction = 0.0;
    o.scale = 0.2;
    o.billboard = true;
    o.graph_y_offset = 10.0;
    o.visible = true;
    o.action = 0; // HELD
    o.prev_action = 0;
}

/// bhv_monty_mole_rock_update.
pub fn rock_update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let action = m.objects[id].action;
    check_attacks(id, m, action, "OBJ_DYING_ENEMY1");
    let parent = m.objects[id].link;
    match m.objects[id].action {
        // monty_mole_rock_act_held: (80, -50, 0) relative to the mole's yaw
        0 => {
            let Some(p) = parent else { m.objects[id].deactivate(-1.0); return };
            let (ppos, pyaw, released) = { let po = &m.objects[p]; (po.pos, po.move_yaw, po.links.is_none() || !po.active) };
            let dist_to_mario = m.objects[id].distance_to_mario;
            let o = &mut m.objects[id];
            o.pos = [ppos[0] + 80.0 * coss(pyaw), ppos[1] - 50.0, ppos[2] - 80.0 * sins(pyaw)];
            if released {
                let d = dist_to_mario.min(600.0);
                o.action = 1;
                o.move_yaw = to_s16(pyaw + 500.0 - d * 0.1) as f64;
                o.forward_vel = 40.0;
                o.vel[1] = d * 0.08 + 8.0;
            }
        }
        // monty_mole_rock_act_move
        _ => {
            let o = &mut m.objects[id];
            let f = move_standard(o, &world, 78.0, false, 30.0);
            if f.on_ground { o.deactivate(-1.0); } // particles + obj_mark_for_deletion
        }
    }
    end_frame(&mut m.objects[id]);
}
