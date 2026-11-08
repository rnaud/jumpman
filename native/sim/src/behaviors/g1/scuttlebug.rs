//! Scuttlebug (`bhvScuttlebug`) and its spawner (`bhvScuttlebugSpawn`), Big Boo's Haunt and
//! Hazy Maze Cave. Port of `src/game/behaviors/scuttlebug.inc.c`.
use super::anim::{self, spec, AnimSpec};
use super::*;

/// `scuttlebug_seg6_anims_06015064`: one 50 frame walk cycle.
pub const ANIMS: [AnimSpec; 1] = [spec(0, 0, 0x32, false)];
pub const ANIM_NAMES: [&str; 1] = ["scuttlebug_seg6_anim_0601504C"];

/// sScuttlebugHitbox.
pub const HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 0.0, 1.0, 1.0, 3.0, 130.0, 70.0, 90.0, 60.0);
pub const MODEL: &str = "actors/scuttlebug/scuttlebug_geo.glb";

/// bhvScuttlebug: SET_OBJ_PHYSICS(wall radius 80, gravity -400, bounciness -50, ...).
pub fn init_scuttlebug(o: &mut SimObject) {
    o.hitbox = HITBOX;
    o.set_var("realInteract", HITBOX.interact_type as f64);
    o.gravity = 4.0;
    o.friction = 0.0;
    o.visible = true;
    o.action = 0;
    o.prev_action = 0;
    o.action_state = 0.0; // oSubAction
    anim::init(o, 0, ANIMS[0]);
}

/// update_angle_from_move_flags: 1 on a wall, -1 on an edge, 0 otherwise.
fn flags_result(o: &SimObject) -> i32 {
    if o.var("mfWall") != 0.0 { 1 } else if o.var("mfEdge") != 0.0 { -1 } else { 0 }
}

/// bhv_scuttlebug_loop. `oSubAction` is `action_state`; `oScuttlebugUnkF4` (no loot) is the var
/// `noLoot`, `oScuttlebugUnkF8/FC` the vars `charge` / `chargeTimer`.
pub fn scuttlebug_update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let sub = m.objects[id].action_state as i32;
    // cur_obj_set_hitbox_and_die_if_attacked
    if sub != 0 {
        let no_loot = m.objects[id].var("noLoot") != 0.0;
        if !m.objects[id].active { return; }
        let touched = die_if_attacked(id, m, "OBJ_DYING_ENEMY1", no_loot);
        if !m.objects[id].active { return; }
        if touched { m.objects[id].action_state = 3.0; }
    } else {
        m.objects[id].interact_status = 0;
    }
    let pos = m.objects[id].pos;
    let o = &mut m.objects[id];
    if o.action_state != 1.0 { o.set_var("charge", 0.0); }
    let dist = o.distance_to_mario;
    let angle_to_mario_now = o.angle_to_mario;
    let angle_to_home = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
    let lateral_mario_home = js_hypot(o.home[0] - mpos[0], o.home[2] - mpos[2]);
    let mut angle_to_mario = angle_to_mario_now;
    let _ = dist;
    let mut sound: Option<&'static str> = None;
    let mut delete = false;
    match o.action_state as i32 {
        0 => {
            if o.var("mfLanded") != 0.0 { sound = Some("OBJ_GOOMBA_ALERT"); }
            if o.var("mfGround") != 0.0 {
                o.home = o.pos;
                o.action_state += 1.0;
            }
        }
        1 => {
            o.forward_vel = 5.0;
            if lateral_mario_home > 1000.0 {
                angle_to_mario = angle_to_home;
            } else if o.var("charge") == 0.0 {
                o.set_var("chargeTimer", 0.0);
                if abs_angle_diff(angle_to_mario, o.move_yaw) < 0x800 as f64 {
                    o.set_var("charge", 1.0);
                    o.vel[1] = 20.0;
                    sound = Some("OBJ2_SCUTTLEBUG_ALERT");
                }
            } else if o.var("charge") == 1.0 {
                o.forward_vel = 15.0;
                o.set_var("chargeTimer", o.var("chargeTimer") + 1.0);
                if o.var("chargeTimer") > 50.0 { o.set_var("charge", 0.0); }
            }
            if flags_result(o) != 0 { o.action_state = 2.0; }
            rotate_yaw_toward(o, angle_to_mario, 0x200 as f64);
        }
        2 => {
            o.forward_vel = 5.0;
            if to_s16(o.move_yaw) == to_s16(angle_to_mario) { o.action_state = 1.0; }
            if o.pos[1] - o.home[1] < -200.0 { delete = true; }
            rotate_yaw_toward(o, angle_to_mario, 0x400 as f64);
        }
        3 => {
            // OBJ_FLAG_SET_FACE_YAW_TO_MOVE_YAW is cleared: the model keeps facing Mario.
            o.set_var("faceFrozen", 1.0);
            o.forward_vel = -10.0;
            o.vel[1] = 30.0;
            sound = Some("OBJ2_SCUTTLEBUG_ALERT");
            o.action_state += 1.0;
        }
        4 => {
            o.forward_vel = -10.0;
            if o.var("mfLanded") != 0.0 {
                o.action_state += 1.0;
                o.vel[1] = 0.0;
                o.set_var("chargeTimer", 0.0);
                o.set_var("faceFrozen", 0.0);
                o.interact_status = 0;
            }
        }
        5 => {
            o.forward_vel = 2.0;
            o.set_var("chargeTimer", o.var("chargeTimer") + 1.0);
            if o.var("chargeTimer") > 30.0 { o.action_state = 0.0; }
        }
        _ => {}
    }
    let sp18 = if o.forward_vel < 10.0 { 1.0 } else { 3.0 };
    anim::init_accel(o, 0, ANIMS[0], sp18);
    if delete {
        o.deactivate(-1.0);
        return;
    }
    if let Some(s) = sound { m.objects.sound(pos, s); }
    let o = &mut m.objects[id];
    // cur_obj_move_standard(-50): care about edges and steep slopes.
    let f = move_standard(o, &world, 50.0, true, 80.0);
    o.set_var("mfLanded", f.landed as i32 as f64);
    o.set_var("mfGround", f.on_ground as i32 as f64);
    o.set_var("mfWall", f.hit_wall as i32 as f64);
    o.set_var("mfEdge", f.hit_edge as i32 as f64);
    if o.var("faceFrozen") == 0.0 { o.face_yaw = o.move_yaw; }
    end_frame(o);
}

/// bhvScuttlebugSpawn: invisible, SPAWNER list.
pub fn init_spawner(o: &mut SimObject) {
    o.visible = false;
    o.hitbox = ObjectHitbox { interact_type: 0, ..Default::default() };
}

/// bhv_scuttlebug_spawn_loop. The spawned Scuttlebug is reused (`link`) instead of allocating a
/// new object every time; `oScuttlebugSpawnerUnkF4` is the var `spawned` (no loot after the first).
pub fn spawner_update(id: ObjId, m: &mut MarioRuntime) {
    let o = &m.objects[id];
    let (dist, pos, yaw, link, action) = (o.distance_to_mario, o.pos, o.move_yaw, o.link, o.action);
    let timer = o.action_timer;
    if action == 0 {
        if timer > 30.0 && 500.0 < dist && dist < 1500.0 {
            m.objects.sound(pos, "OBJ2_SCUTTLEBUG_ALERT");
            let no_loot = m.objects[id].var("spawned");
            let child = match link {
                Some(c) => {
                    m.objects[c].reactivate();
                    init_scuttlebug(&mut m.objects[c]);
                    c
                }
                None => {
                    let mut s = ObjectSpec::new(ObjectBehaviorKind::Scuttlebug, pos, yaw);
                    s.model = Some(MODEL.to_string());
                    m.objects.spawn(&s)
                }
            };
            let c = &mut m.objects[child];
            c.pos = pos;
            c.home = pos;
            c.move_yaw = yaw;
            c.face_yaw = yaw;
            c.forward_vel = 30.0;
            c.vel[1] = 80.0;
            c.set_var("noLoot", no_loot);
            c.model = Some(MODEL.to_string());
            let o = &mut m.objects[id];
            o.link = Some(child);
            o.action = 1;
            o.set_var("spawned", 1.0);
        }
    } else if let Some(c) = link {
        // `o->parentObj->oScuttlebugSpawnerUnk88 = 1` when the child is deactivated.
        if !m.objects[c].active { m.objects[id].action = 0; }
    }
    end_frame(&mut m.objects[id]);
}
