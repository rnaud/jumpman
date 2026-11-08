//! Princess's Secret Slide timer: `pss_begin_slide` / `pss_end_slide` (`src/game/interaction.c`,
//! called from `mario_handle_special_floors` on `SURFACE_TIMER_START` (0x33) / `SURFACE_TIMER_END`
//! (0x34)). The HUD timer starts on the first start-line contact; crossing the end line in under 630
//! frames (21 s) spawns the act 2 star at (-6358, -4300, 4700) (`spawn_default_star`). Like the
//! original, the "slide started" flag survives a death, and the HUD timer stays shown once started, so
//! a second run in the same visit does not restart it. The object is a hidden marker the exporter
//! places once in PSS.
use super::*;
use crate::state::*;

pub const SURFACE_TIMER_START: i32 = 0x33;
pub const SURFACE_TIMER_END: i32 = 0x34;
/// `slideTime < 630` in `pss_end_slide`.
pub const STAR_TIME_LIMIT: f64 = 630.0;
/// `spawn_default_star(-6358.0f, -4300.0f, 4700.0f)`.
pub const STAR_POS: Vec3 = [-6358.0, -4300.0, 4700.0];
/// `STAR_INDEX_ACT_2`.
pub const STAR_ACT: i32 = 2;

pub fn init(o: &mut SimObject) {
    o.visible = false;
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    // `gHudDisplay.timer` runs once per frame after the slide started (level_update).
    if m.objects[id].var("running") != 0.0 {
        let t = m.objects[id].var("elapsed") + 1.0;
        m.objects[id].set_var("elapsed", t);
    }
    if m.action & ACT_GROUP_MASK == ACT_GROUP_CUTSCENE {
        return;
    }
    let Some(kind) = m.floor.as_ref().map(|f| f.kind) else { return };
    if kind == SURFACE_TIMER_START {
        // pss_begin_slide: only when the HUD timer is not already displayed.
        if m.objects[id].var("shown") == 0.0 {
            let o = &mut m.objects[id];
            o.set_var("shown", 1.0);
            o.set_var("running", 1.0);
            o.set_var("elapsed", 0.0);
            o.set_var("slideStarted", 1.0);
        }
    } else if kind == SURFACE_TIMER_END && m.objects[id].var("slideStarted") != 0.0 {
        // pss_end_slide: level_control_timer(STOP) returns the time.
        let o = &mut m.objects[id];
        let time = o.var("elapsed");
        o.set_var("running", 0.0);
        o.set_var("slideStarted", 0.0);
        o.set_var("finalTime", time);
        if time < STAR_TIME_LIMIT {
            m.objects.castle_star(STAR_POS, STAR_ACT);
        }
    }
}
