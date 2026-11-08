//! Shared helpers of the SM64 round-2 boss / big-enemy ports (`object_helpers.c`,
//! `obj_behaviors_2.c`). Conventions are those of `behaviors/g2/obj.rs` (decomp `oTimer` /
//! `oAction`, fields in `vars` under the decomp spelling without the `o`) plus the animation state
//! of `behaviors/g1/anim.rs`.
use crate::behaviors::g1::anim::{self, AnimSpec};
use crate::behaviors::g2::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

pub use crate::behaviors::g1::{abs_angle_diff, approach_f32_ptr, hitbox, set_hitbox};

/// `cur_obj_init_animation_with_sound(index)` (sound omitted) for an actor with `table`.
pub fn init_anim(o: &mut SimObject, table: &[AnimSpec], index: usize) {
    anim::init(o, index, table[index]);
}

/// `cur_obj_init_anim_and_check_if_end` (obj_behaviors_2.c).
pub fn init_anim_check_end(o: &mut SimObject, table: &[AnimSpec], index: usize) -> bool {
    init_anim(o, table, index);
    anim::near_end(o)
}

/// `cur_obj_init_anim_check_frame` (obj_behaviors_2.c).
pub fn init_anim_check_frame(o: &mut SimObject, table: &[AnimSpec], index: usize, frame: i32) -> bool {
    init_anim(o, table, index);
    anim::check_frame(o, frame)
}

/// `cur_obj_reverse_animation`: steps the animation frame back one (the draw pass then advances
/// it, so a reversed animation holds).
pub fn reverse_animation(o: &mut SimObject) {
    if o.var("animFrame") >= 0.0 {
        o.set_var("animFrame", o.var("animFrame") - 1.0);
        o.set_var("animAssist", o.var("animFrame"));
    }
}

/// Draw-pass tail of `cur_obj_update`: the object's animation advances once per tick.
pub fn advance_anim(o: &mut SimObject) {
    anim::advance(o);
}

/// `obj_face_yaw_approach(target, delta)`.
pub fn face_yaw_approach(o: &mut SimObject, target: f64, delta: f64) -> bool {
    o.face_yaw = approach_s16_symmetric(o.face_yaw, target, delta);
    to_s16(o.face_yaw) == to_s16(target)
}

/// `obj_forward_vel_approach`.
pub fn forward_vel_approach(o: &mut SimObject, target: f64, delta: f64) -> bool {
    approach_f32_ptr(&mut o.forward_vel, target, delta)
}

/// `random_linear_offset(base, range)`: `base + (s16)(range * random_float())`.
pub fn random_linear_offset(base: f64, range: f64) -> f64 {
    base + (range * random_float()).trunc()
}

/// `clamp_f32`.
pub fn clamp_f(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

/// `cur_obj_angle_to_home`.
pub fn angle_to_home(o: &SimObject) -> f64 {
    atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64
}

/// `obj_check_attacks` (obj_behaviors_2.c) minus the lava test: sets the hitbox, and when Mario
/// touched the object returns the attack type (0 when none), switching to `attacked_mario_action`
/// when the object hurt Mario. With `die_with_health` the object disappears at health <= 0
/// (`obj_die_if_health_non_positive`).
pub fn obj_check_attacks(o: &mut SimObject, hb: ObjectHitbox, attacked_mario_action: i32, die_with_health: bool) -> i32 {
    set_hitbox(o, hb);
    let mut attack = 0;
    if o.interact_status & INT_STATUS_INTERACTED != 0 {
        if o.interact_status & INT_STATUS_ATTACKED_MARIO != 0 {
            if o.action != attacked_mario_action {
                o.action = attacked_mario_action;
                o.timer = 0.0;
            }
        } else {
            attack = (o.interact_status & 0xFF) as i32;
            if die_with_health && o.hitbox.health <= 0.0 {
                o.deactivate(-1.0);
            }
        }
    }
    o.interact_status = 0;
    attack
}

/// `cur_obj_update_dialog_with_cutscene` over the app's NPC dialog modal (`ObjectEvent::KingDialog`:
/// dialog camera + the text pages of `sm64_dialogs`): true on the frame the dialog has ended. It
/// starts when Mario is in a stationary / moving action (`mario_ready_to_speak`).
pub fn update_dialog(m: &mut MarioRuntime, id: ObjId, dialog_id: u32) -> bool {
    if m.objects[id].var("dialogDone") != 0.0 {
        let o = &mut m.objects[id];
        o.set_var("dialogDone", 0.0);
        o.set_var("dlgStarted", 0.0);
        return true;
    }
    if m.objects[id].var("dlgStarted") == 0.0 && mario_ready_to_speak(m) {
        m.objects.king_dialog(id, dialog_id as f64);
        m.objects[id].set_var("dlgStarted", 1.0);
    }
    false
}

/// `spawn_default_star(x, y, z)`.
pub fn spawn_default_star(m: &mut MarioRuntime, pos: Vec3) {
    m.objects.events.push(crate::object_manager::ObjectEvent::DefaultStar { pos });
}

/// `mario_ready_to_speak`.
pub fn mario_ready_to_speak(m: &MarioRuntime) -> bool {
    crate::behaviors::king_bobomb::ready_for_npc_dialog(m)
}

/// `obj_explode_and_spawn_coins`: the object disappears in a puff (the app's `Break` event) and
/// drops `coins` yellow coins.
pub fn explode_and_spawn_coins(m: &mut MarioRuntime, id: ObjId, coins: u32) {
    let pos = m.objects[id].pos;
    m.objects.events.push(crate::object_manager::ObjectEvent::Break { pos, wall: None });
    for _ in 0..coins {
        m.objects.spawn_coin(pos, true, 1.0);
    }
    mark_for_deletion(&mut m.objects[id]);
}

/// The anim table of an actor as the decomp lists it: `(flags & 1 = no loop, start, loopStart, loopEnd)`.
pub const fn anim_spec(noloop: bool, start: i32, loop_start: i32, loop_end: i32) -> AnimSpec {
    anim::spec(start, loop_start, loop_end, noloop)
}
