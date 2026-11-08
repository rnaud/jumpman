//! Spindrift (`bhvSpindrift`), the flower-headed Cool, Cool Mountain walker.
//! Port of `src/game/behaviors/spindrift.inc.c`.
use super::anim::{self, spec, AnimSpec};
use super::*;

/// `spindrift_seg5_anims_05002D68`: one 19 frame spin.
pub const ANIMS: [AnimSpec; 1] = [spec(0, 0, 0x13, false)];
pub const ANIM_NAMES: [&str; 1] = ["spindrift_seg5_anim_050006AC"];
/// sSpindriftHitbox (BOUNCE_TOP, damage 2, health 1, 3 coins).
pub const HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 0.0, 2.0, 1.0, 3.0, 90.0, 80.0, 80.0, 70.0);

pub fn init(o: &mut SimObject) {
    o.hitbox = HITBOX;
    o.set_var("realInteract", HITBOX.interact_type as f64);
    o.interact_subtype = INT_SUBTYPE_TWIRL_BOUNCE;
    o.gravity = 4.0;
    o.friction = 0.0;
    o.visible = true;
    o.action = 0;
    o.prev_action = 0;
    anim::init(o, 0, ANIMS[0]);
}

/// bhv_spindrift_loop.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    // cur_obj_set_hitbox_and_die_if_attacked(&sSpindriftHitbox, SOUND_OBJ_DYING_ENEMY1, 0)
    if die_if_attacked(id, m, "OBJ_DYING_ENEMY1", false) { m.objects[id].action = 1; }
    if !m.objects[id].active { return; }
    let o = &mut m.objects[id];
    let dist = o.distance_to_mario;
    match o.action {
        0 => {
            let mut v = o.forward_vel;
            approach_f32_ptr(&mut v, 4.0, 1.0);
            o.forward_vel = v;
            let mut angle = o.angle_to_mario;
            if js_hypot(o.home[0] - mpos[0], o.home[2] - mpos[2]) > 1000.0 {
                angle = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
            }
            // `else if (dist > 300) angle = angle to Mario` is the default above; at <= 300 the
            // previous oAngleToMario (from the engine, still toward Mario) is kept as well.
            let _ = dist;
            rotate_yaw_toward(o, angle, 0x400 as f64);
            o.face_yaw = o.move_yaw;
        }
        _ => {
            // OBJ_FLAG_SET_FACE_YAW_TO_MOVE_YAW cleared: backs away facing Mario.
            o.forward_vel = -10.0;
            if o.action_timer > 20.0 {
                o.action = 0;
                o.interact_status = 0;
                o.face_yaw = o.move_yaw;
            }
        }
    }
    // cur_obj_move_standard(-60)
    move_standard(o, &world, 60.0, true, 30.0);
    end_frame(o);
}
