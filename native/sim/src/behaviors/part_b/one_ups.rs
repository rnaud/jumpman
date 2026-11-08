//! The 1-up mushroom variants of `src/game/behaviors/mushroom_1up.inc.c`: `bhv1UpWalking` (box
//! contents), `bhv1UpRunningAway`, `bhv1UpSliding`, `bhvHidden1Up` and `bhvHidden1UpTrigger`
//! (`bhv_1up_*_loop`). The plain `bhv1Up` is `behaviors/one_up.rs`. Not ported: the two keyed 1-ups
//! (`ONE_UP_BP_BEAT_BITDW_BOWSER` / `_BITFS_BOWSER`, they need the save file) and the in-pole variant
//! (`bhvHidden1UpInPole*`).
use super::*;
use crate::state::*;

const HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0,
    radius: 30.0, height: 30.0, hurtbox_radius: 30.0, hurtbox_height: 30.0,
};

/// `bhv_1up_common_init` + SET_HITBOX_WITH_OFFSET(30, 30, 0) + oGraphYOffset 30.
pub fn init(o: &mut SimObject) {
    o.set_var("pitch", -0x4000 as f64);
    o.gravity = 3.0;
    o.friction = 1.0;
    o.buoyancy = 1.0;
    o.hitbox = HITBOX;
    o.graph_y_offset = 30.0;
    o.billboard = true;
    o.visible = true;
}

/// `bhv_1up_interact`.
fn interact(id: ObjId, m: &mut MarioRuntime) {
    if m.action & ACT_FLAG_INTANGIBLE == 0 && mario_overlaps_object(m, &m.objects[id]) {
        m.lives += 1.0;
        let pos = m.objects[id].pos;
        m.objects.one_up_collected(pos);
        m.objects.sound(pos, "GENERAL_COLLECT_1UP");
        m.objects[id].deactivate(-1.0);
    }
}

/// `one_up_loop_in_air`: the pop-out arc of the first 37 frames.
fn loop_in_air(o: &mut SimObject) {
    if o.action_timer < 5.0 {
        o.vel[1] = 40.0;
    } else {
        let pitch = to_s16(o.var("pitch") - 0x1000 as f64) as f64;
        o.set_var("pitch", pitch);
        o.vel[1] = coss(pitch) * 30.0 + 2.0;
        o.forward_vel = -sins(pitch) * 30.0;
    }
}

fn appear(m: &mut MarioRuntime, id: ObjId) {
    let o = &m.objects[id];
    if o.action_timer == 0.0 {
        let pos = o.pos;
        m.objects.sound(pos, "GENERAL2_1UP_APPEAR");
    }
}

/// `one_up_move_away_from_mario`.
fn move_away(id: ObjId, m: &mut MarioRuntime, flags: u32) {
    let o = &mut m.objects[id];
    o.forward_vel = 8.0;
    o.move_yaw = to_s16(o.angle_to_mario + 0x8000 as f64) as f64;
    interact(id, m);
    if !m.objects[id].active {
        return;
    }
    if flags & OBJ_COL_FLAG_HIT_WALL != 0 {
        m.objects[id].action = 2;
    }
    if !near_mario(m, &m.objects[id].pos, 3000.0) {
        m.objects[id].action = 2;
    }
}

/// bhv_1up_walking_loop.
pub fn walking(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    m.objects[id].step_physics(&world, None);
    match m.objects[id].action {
        0 => {
            appear(m, id);
            let o = &mut m.objects[id];
            loop_in_air(o);
            if o.action_timer == 37.0 {
                o.action = 1;
                o.forward_vel = 2.0;
            }
        }
        1 => {
            if m.objects[id].action_timer > 300.0 {
                m.objects[id].action = 2;
            }
            interact(id, m);
        }
        _ => {
            if flicker_and_disappear(&mut m.objects[id], 30.0) {
                return;
            }
            interact(id, m);
        }
    }
    end(&mut m.objects[id]);
}

/// bhv_1up_running_away_loop.
pub fn running_away(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let flags = m.objects[id].step_physics(&world, None);
    match m.objects[id].action {
        0 => {
            appear(m, id);
            let o = &mut m.objects[id];
            loop_in_air(o);
            if o.action_timer == 37.0 {
                o.action = 1;
                o.forward_vel = 8.0;
            }
        }
        1 => move_away(id, m, flags),
        _ => {
            if flicker_and_disappear(&mut m.objects[id], 30.0) {
                return;
            }
            interact(id, m);
        }
    }
    if m.objects[id].active {
        end(&mut m.objects[id]);
    }
}

/// bhv_1up_sliding_loop (+ sliding_1up_move): wakes within 1000, slides down the slope.
pub fn sliding(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    match m.objects[id].action {
        0 => {
            if near_mario(m, &m.objects[id].pos, 1000.0) {
                m.objects[id].action = 1;
            }
        }
        1 => {
            let flags = m.objects[id].step_physics(&world, None);
            let o = &mut m.objects[id];
            if flags & OBJ_COL_FLAG_GROUNDED != 0 {
                o.forward_vel += 25.0;
                o.vel[1] = 0.0;
            } else {
                o.forward_vel *= 0.98;
            }
            if o.forward_vel > 40.0 {
                o.forward_vel = 40.0;
            }
            if !near_mario(m, &m.objects[id].pos, 5000.0) {
                m.objects[id].action = 2;
            }
        }
        _ => {
            if flicker_and_disappear(&mut m.objects[id], 30.0) {
                return;
            }
            interact(id, m);
        }
    }
    if m.objects[id].active {
        interact(id, m);
    }
    if m.objects[id].active {
        end(&mut m.objects[id]);
    }
}

pub fn init_hidden(o: &mut SimObject) {
    init(o);
    o.visible = false;
}

/// bhv_1up_hidden_loop: invisible until `param` triggers were touched, then it pops out and runs
/// away like the running-away variant.
pub fn hidden(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    match m.objects[id].action {
        0 => {
            let o = &mut m.objects[id];
            o.visible = false;
            if o.var("triggers") == o.param {
                o.vel[1] = 40.0;
                o.action = 3;
                o.visible = true;
                let pos = o.pos;
                m.objects.sound(pos, "GENERAL2_1UP_APPEAR");
            }
        }
        1 => {
            let flags = m.objects[id].step_physics(&world, None);
            move_away(id, m, flags);
        }
        2 => {
            m.objects[id].step_physics(&world, None);
            if flicker_and_disappear(&mut m.objects[id], 30.0) {
                return;
            }
            interact(id, m);
        }
        _ => {
            m.objects[id].step_physics(&world, None);
            let o = &mut m.objects[id];
            loop_in_air(o);
            if o.action_timer == 37.0 {
                o.action = 1;
                o.forward_vel = 8.0;
            }
        }
    }
    if m.objects[id].active {
        end(&mut m.objects[id]);
    }
}

pub fn init_trigger(o: &mut SimObject) {
    o.hitbox = hitbox_100();
    o.visible = false;
}

fn hitbox_100() -> ObjectHitbox {
    ObjectHitbox {
        interact_type: 0, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0,
        radius: 100.0, height: 100.0, hurtbox_radius: 100.0, hurtbox_height: 100.0,
    }
}

/// bhv_1up_hidden_trigger_loop: touching it counts for the nearest hidden 1-up, then it is gone.
pub fn trigger(id: ObjId, m: &mut MarioRuntime) {
    if mario_overlaps_object(m, &m.objects[id]) {
        if let Some(one_up) = nearest_of(m, id, K::Hidden1Up) {
            let n = m.objects[one_up].var("triggers") + 1.0;
            m.objects[one_up].set_var("triggers", n);
        }
        m.objects[id].deactivate(-1.0);
    }
}
