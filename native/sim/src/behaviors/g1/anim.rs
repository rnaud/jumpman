//! Object animation state (`struct AnimInfo`, graph_node.c) kept in an object's vars so behaviors
//! can ask `cur_obj_check_if_near_animation_end` and friends, and the renderer can seek the glb
//! clip to `animFrame`.
//!
//! Vars: `animation` (index in the actor's animation table), `animFrame`, `animLoopEnd`,
//! `animLoopStart`, `animNoloop`, `animAccel` (frames per tick, 0 = 1), `animAssist` (the 16.16
//! accumulator as a float frame count).
use crate::object::SimObject;

/// The header fields of one `struct Animation` that matter to behaviors (`assets/animations.json`
/// `header`: flags, yDivisor, startFrame, loopStart, loopEnd).
#[derive(Clone, Copy, Debug)]
pub struct AnimSpec {
    pub start: i32,
    pub loop_start: i32,
    pub loop_end: i32,
    pub noloop: bool,
}

pub const fn spec(start: i32, loop_start: i32, loop_end: i32, noloop: bool) -> AnimSpec {
    AnimSpec { start, loop_start, loop_end, noloop }
}

/// geo_obj_init_animation: restarts only when the animation changes.
pub fn init(o: &mut SimObject, index: usize, a: AnimSpec) { init_accel(o, index, a, 0.0) }

/// geo_obj_init_animation_accel (`accel` is in frames per tick).
pub fn init_accel(o: &mut SimObject, index: usize, a: AnimSpec, accel: f64) {
    if o.has_var("animation") && o.var("animation") == index as f64 && o.has_var("animFrame") {
        // The same table entry keeps its frame; the accel only matters on a fresh start.
        return;
    }
    o.set_var("animation", index as f64);
    o.set_var("animLoopEnd", a.loop_end as f64);
    o.set_var("animLoopStart", a.loop_start as f64);
    o.set_var("animNoloop", if a.noloop { 1.0 } else { 0.0 });
    o.set_var("animAccel", accel);
    if accel != 0.0 {
        o.set_var("animAssist", a.start as f64 - accel);
        o.set_var("animFrame", (a.start as f64 - accel).floor());
    } else {
        o.set_var("animFrame", a.start as f64 - 1.0);
        o.set_var("animAssist", a.start as f64 - 1.0);
    }
}

pub fn frame(o: &SimObject) -> i32 { o.var("animFrame") as i32 }

/// cur_obj_check_if_near_animation_end.
pub fn near_end(o: &SimObject) -> bool {
    let frame = o.var("animFrame") as i32;
    let near = o.var("animLoopEnd") as i32 - 2;
    (o.var("animNoloop") != 0.0 && near + 1 == frame) || frame == near
}

/// cur_obj_check_if_at_animation_end.
pub fn at_end(o: &SimObject) -> bool { o.var("animFrame") as i32 == o.var("animLoopEnd") as i32 - 1 }

/// cur_obj_check_anim_frame.
pub fn check_frame(o: &SimObject, frame: i32) -> bool { o.var("animFrame") as i32 == frame }

/// cur_obj_extend_animation_if_at_end: holds the animation one frame short of "near the end".
pub fn extend_if_at_end(o: &mut SimObject) {
    if o.var("animFrame") as i32 == o.var("animLoopEnd") as i32 - 2 {
        o.set_var("animFrame", o.var("animFrame") - 1.0);
        o.set_var("animAssist", o.var("animFrame"));
    }
}

/// geo_update_animation_frame, run once per tick after the behavior (the draw pass).
pub fn advance(o: &mut SimObject) {
    if !o.has_var("animation") { return; }
    let accel = o.var("animAccel");
    let end = o.var("animLoopEnd");
    let loop_start = o.var("animLoopStart");
    let noloop = o.var("animNoloop") != 0.0;
    let mut result = if accel != 0.0 { o.var("animAssist") + accel } else { o.var("animFrame") + 1.0 };
    if result.floor() >= end {
        result = if noloop { end - 1.0 } else { loop_start };
    }
    o.set_var("animAssist", result);
    o.set_var("animFrame", result.floor());
}
