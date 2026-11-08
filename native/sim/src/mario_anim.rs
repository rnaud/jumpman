//! Mario's animation frame clock, as far as gameplay reads it.
//!
//! The decomp's actions end by asking the animation player (`is_anim_at_end`,
//! `is_anim_past_frame`, the return value of `set_mario_animation`), so the timing of water
//! punches, knockbacks, metal-cap landings ... is the animation length. The sim has no skeleton;
//! this keeps only the `AnimInfo` counters of `src/engine/graph_node.c`:
//! `animID`, `animFrame`, `animFrameAccelAssist`, `animAccel`, plus the header words from
//! `mario_anim_data.rs`.
//!
//! Call [`AnimClock::advance`] once per frame after the action ran (the original advances the
//! frame while the object is drawn, `geo_update_animation_frame`).
use crate::mario_anim_data::MARIO_ANIM_HEADERS;

pub const ANIM_FLAG_NOLOOP: u8 = 1 << 0;
pub const ANIM_FLAG_BACKWARD: u8 = 1 << 1;
pub const ANIM_FLAG_2: u8 = 1 << 2;

// Animation ids from include/mario_animation_ids.h that the submerged actions use.
pub const MARIO_ANIM_WALKING: i32 = 0x48;
pub const MARIO_ANIM_RUN_WITH_LIGHT_OBJ: i32 = 0x17;
pub const MARIO_ANIM_IDLE_WITH_LIGHT_OBJ: i32 = 0x3f;
pub const MARIO_ANIM_JUMP_LAND_WITH_LIGHT_OBJ: i32 = 0x40;
pub const MARIO_ANIM_JUMP_WITH_LIGHT_OBJ: i32 = 0x41;
pub const MARIO_ANIM_FALL_LAND_WITH_LIGHT_OBJ: i32 = 0x42;
pub const MARIO_ANIM_FALL_WITH_LIGHT_OBJ: i32 = 0x43;
pub const MARIO_ANIM_SINGLE_JUMP: i32 = 0x4d;
pub const MARIO_ANIM_LAND_FROM_SINGLE_JUMP: i32 = 0x4e;
pub const MARIO_ANIM_GENERAL_FALL: i32 = 0x56;
pub const MARIO_ANIM_GENERAL_LAND: i32 = 0x57;
pub const MARIO_ANIM_SHOCKED: i32 = 0x7a;
pub const MARIO_ANIM_BACKWARDS_WATER_KB: i32 = 0x9e;
pub const MARIO_ANIM_SWIM_WITH_OBJ_PART1: i32 = 0x9f;
pub const MARIO_ANIM_SWIM_WITH_OBJ_PART2: i32 = 0xa0;
pub const MARIO_ANIM_FLUTTERKICK_WITH_OBJ: i32 = 0xa1;
pub const MARIO_ANIM_WATER_ACTION_END_WITH_OBJ: i32 = 0xa2;
pub const MARIO_ANIM_STOP_GRAB_OBJ_WATER: i32 = 0xa3;
pub const MARIO_ANIM_WATER_IDLE_WITH_OBJ: i32 = 0xa4;
pub const MARIO_ANIM_DROWNING_PART1: i32 = 0xa5;
pub const MARIO_ANIM_DROWNING_PART2: i32 = 0xa6;
pub const MARIO_ANIM_WATER_DYING: i32 = 0xa7;
pub const MARIO_ANIM_WATER_FORWARD_KB: i32 = 0xa8;
pub const MARIO_ANIM_FALL_FROM_WATER: i32 = 0xa9;
pub const MARIO_ANIM_SWIM_PART1: i32 = 0xaa;
pub const MARIO_ANIM_SWIM_PART2: i32 = 0xab;
pub const MARIO_ANIM_FLUTTERKICK: i32 = 0xac;
pub const MARIO_ANIM_WATER_ACTION_END: i32 = 0xad;
pub const MARIO_ANIM_WATER_PICK_UP_OBJ: i32 = 0xae;
pub const MARIO_ANIM_WATER_GRAB_OBJ_PART2: i32 = 0xaf;
pub const MARIO_ANIM_WATER_GRAB_OBJ_PART1: i32 = 0xb0;
pub const MARIO_ANIM_WATER_THROW_OBJ: i32 = 0xb1;
pub const MARIO_ANIM_WATER_IDLE: i32 = 0xb2;
pub const MARIO_ANIM_IDLE_HEAD_LEFT: i32 = 0xc3;
pub const MARIO_ANIM_IDLE_HEAD_RIGHT: i32 = 0xc4;
pub const MARIO_ANIM_IDLE_HEAD_CENTER: i32 = 0xc5;

/// `struct Animation` header fields the clock reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimHeader {
    pub flags: u8,
    pub start_frame: i32,
    pub loop_start: i32,
    pub loop_end: i32,
}

pub fn anim_header(id: i32) -> Option<AnimHeader> {
    MARIO_ANIM_HEADERS.iter().find(|h| h.0 as i32 == id).map(|h| AnimHeader {
        flags: h.1,
        start_frame: h.2 as i32,
        loop_start: h.3 as i32,
        loop_end: h.4 as i32,
    })
}

/// `AnimInfo` (`animID`, `animFrame`, `animFrameAccelAssist`, `animAccel`).
#[derive(Clone, Debug)]
pub struct AnimClock {
    pub id: i32,
    pub frame: i32,
    pub accel_assist: i32,
    pub accel: i32,
    header: Option<AnimHeader>,
}

impl Default for AnimClock {
    fn default() -> Self { Self { id: -1, frame: 0, accel_assist: 0, accel: 0, header: None } }
}

impl AnimClock {
    /// Forget the current animation (the next `set` restarts it). Used when a non-submerged
    /// action runs, whose animation this sim does not track.
    pub fn invalidate(&mut self) { self.id = -1; self.header = None; }

    /// `set_mario_animation`: restarts only when the id changes; returns `animFrame`.
    pub fn set(&mut self, id: i32) -> i32 {
        if self.id != id {
            let h = anim_header(id).expect("unknown Mario animation id");
            self.id = id;
            self.header = Some(h);
            self.accel = 0;
            self.frame = if h.flags & ANIM_FLAG_2 != 0 {
                h.start_frame
            } else if h.flags & ANIM_FLAG_BACKWARD != 0 {
                h.start_frame + 1
            } else {
                h.start_frame - 1
            };
        }
        self.frame
    }

    /// `set_mario_anim_with_accel`.
    pub fn set_with_accel(&mut self, id: i32, accel: i32) -> i32 {
        if self.id != id {
            let h = anim_header(id).expect("unknown Mario animation id");
            self.id = id;
            self.header = Some(h);
            self.accel_assist = if h.flags & ANIM_FLAG_2 != 0 {
                h.start_frame << 16
            } else if h.flags & ANIM_FLAG_BACKWARD != 0 {
                (h.start_frame << 16) + accel
            } else {
                (h.start_frame << 16) - accel
            };
            self.frame = self.accel_assist >> 16;
        }
        self.accel = accel;
        self.frame
    }

    /// `set_anim_to_frame`.
    pub fn set_to_frame(&mut self, frame: i32) {
        let backward = self.header.is_some_and(|h| h.flags & ANIM_FLAG_BACKWARD != 0);
        if self.accel != 0 {
            self.accel_assist = if backward { (frame << 16) + self.accel } else { (frame << 16) - self.accel };
        } else {
            self.frame = if backward { frame + 1 } else { frame - 1 };
        }
    }

    /// `is_anim_at_end`.
    pub fn is_at_end(&self) -> bool { self.header.is_some_and(|h| self.frame + 1 == h.loop_end) }

    /// `is_anim_past_end`.
    pub fn is_past_end(&self) -> bool { self.header.is_some_and(|h| self.frame >= h.loop_end - 2) }

    /// `is_anim_past_frame`.
    pub fn is_past_frame(&self, frame: i32) -> bool {
        let Some(h) = self.header else { return false };
        let accelerated = frame << 16;
        let backward = h.flags & ANIM_FLAG_BACKWARD != 0;
        if self.accel != 0 {
            if backward {
                self.accel_assist > accelerated && accelerated >= self.accel_assist - self.accel
            } else {
                self.accel_assist < accelerated && accelerated <= self.accel_assist + self.accel
            }
        } else if backward {
            self.frame == frame + 1
        } else {
            self.frame + 1 == frame
        }
    }

    /// `geo_update_animation_frame` for an animation with a non-zero or zero `animAccel`.
    pub fn advance(&mut self) {
        let Some(h) = self.header else { return };
        if h.flags & ANIM_FLAG_2 != 0 { return; }
        // The original keeps the accel assist in 16.16; `SET_HIGH_S16_OF_32` only replaces the high half.
        let high = |v: i32| v >> 16;
        let set_high = |v: i32, hi: i32| (v & 0xffff) | (hi << 16);
        let mut result;
        if h.flags & ANIM_FLAG_BACKWARD != 0 {
            result = if self.accel != 0 { self.accel_assist - self.accel } else { (self.frame - 1) << 16 };
            if high(result) < h.loop_start {
                result = if h.flags & ANIM_FLAG_NOLOOP != 0 { set_high(result, h.loop_start) } else { set_high(result, h.loop_end - 1) };
            }
        } else {
            result = if self.accel != 0 { self.accel_assist + self.accel } else { (self.frame + 1) << 16 };
            if high(result) >= h.loop_end {
                result = if h.flags & ANIM_FLAG_NOLOOP != 0 { set_high(result, h.loop_end - 1) } else { set_high(result, h.loop_start) };
            }
        }
        self.accel_assist = result;
        self.frame = high(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_match_the_decomp() {
        // anim_AA (swim part 1): NOLOOP, loopEnd 13. anim_AC (flutterkick) loops from start 8.
        assert_eq!(anim_header(MARIO_ANIM_SWIM_PART1), Some(AnimHeader { flags: 1, start_frame: 0, loop_start: 0, loop_end: 13 }));
        assert_eq!(anim_header(MARIO_ANIM_FLUTTERKICK), Some(AnimHeader { flags: 0, start_frame: 8, loop_start: 0, loop_end: 24 }));
        assert_eq!(anim_header(MARIO_ANIM_WATER_THROW_OBJ).unwrap().loop_end, 28);
        assert_eq!(anim_header(MARIO_ANIM_WATER_IDLE).unwrap().flags, 0);
    }

    #[test]
    fn noloop_animation_reaches_its_end_once() {
        let mut c = AnimClock::default();
        // Frame -1 on the frame the animation is set; the first advance shows frame 0.
        assert_eq!(c.set(MARIO_ANIM_WATER_THROW_OBJ), -1);
        let mut at_end = vec![];
        for tick in 0..40 {
            c.set(MARIO_ANIM_WATER_THROW_OBJ);
            if c.is_at_end() { at_end.push(tick); }
            c.advance();
        }
        // loopEnd 28: frame 27 ("at end") is shown on the 29th update after the (-1) set frame; NOLOOP then holds.
        assert_eq!(at_end.first(), Some(&28));
        assert_eq!(c.frame, 27);
    }

    #[test]
    fn looping_animation_wraps_to_loop_start() {
        let mut c = AnimClock::default();
        c.set(MARIO_ANIM_WATER_IDLE); // loopEnd 30, looping
        for _ in 0..31 { c.advance(); }
        assert_eq!(c.frame, 0);
    }
}
