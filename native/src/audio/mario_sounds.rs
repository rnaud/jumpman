//! Mario's action sounds that the sim has no event for, keyed on the action Mario is in.
//!
//! The sim reports jumps, landings, swimming strokes, bonks and so on as `MarioEvent`s ([`super::dispatch_sim_event`]).
//! Everything else the decomp plays from `mario_actions_*.c` (punch and throw grunts, ledge and pole voices,
//! slide scrapes, lava and wind loops, hanging steps...) is listed here by the action that plays it, with the real
//! `SOUND_*` name. [`entry`] gives the sounds played once when an action starts, [`each_tick`] the ones the action
//! requests every tick (or every `period` ticks for animation-frame sounds). `MARIO_*` names are voices: they are
//! only played for Mario's voice set (the other characters have their own sets in `character-sounds.json`).
use jumpman_sim::state::*;

/// Sounds played when `action` starts (`play_sound_if_no_flag(... MARIO_MARIO_SOUND_PLAYED)` at entry).
pub fn entry(action: u32, arg: f64) -> &'static [&'static str] {
    match action {
        ACT_PUNCHING | ACT_MOVE_PUNCHING => match arg as i32 {
            0 => &["MARIO_PUNCH_YAH"],
            1 => &["MARIO_PUNCH_WAH"],
            _ => &["MARIO_PUNCH_HOO"],
        },
        ACT_SLIDE_KICK => &["MARIO_PUNCH_HOO"],
        ACT_PICKING_UP | ACT_DIVE_PICKING_UP => &["MARIO_HRMM"],
        ACT_THROWING | ACT_AIR_THROW => &["MARIO_WAH2", "ACTION_THROW"],
        ACT_GRAB_POLE_SLOW | ACT_GRAB_POLE_FAST | ACT_LEDGE_GRAB => &["MARIO_WHOA"],
        ACT_LEDGE_CLIMB_SLOW_1 => &["MARIO_EEUH"],
        ACT_LEDGE_CLIMB_FAST => &["MARIO_UH2", "ACTION_TERRAIN_LANDING"],
        ACT_START_HANGING => &["ACTION_HANGING_STEP"],
        ACT_LAVA_BOOST => &["MARIO_ON_FIRE", "MOVING_LAVA_BURN"],
        ACT_START_SLEEPING => &["MARIO_YAWNING"],
        ACT_DROWNING => &["MARIO_DROWNING"],
        ACT_METAL_WATER_JUMP => &["ACTION_METAL_JUMP_WATER"],
        _ => &[],
    }
}

/// Sounds played when Mario left `from` for `to` (landings from a metal-water fall).
pub fn transition(from: u32, to: u32) -> &'static [&'static str] {
    if from == ACT_METAL_WATER_FALLING && matches!(to, ACT_METAL_WATER_STANDING | ACT_METAL_WATER_WALKING) {
        return &["ACTION_METAL_LAND_WATER"];
    }
    &[]
}

/// A sound an action requests while it lasts: every `period` ticks (1 = every tick).
pub struct Looping {
    pub name: &'static str,
    pub period: u32,
}

/// Sounds requested every tick / every `period` ticks while in `action` (`play_sound(...)` in the action's step).
pub fn each_tick(action: u32) -> &'static [Looping] {
    match action {
        // `play_sound(SOUND_MOVING_TERRAIN_SLIDE + m->terrainSoundAddend)` in every slide.
        ACT_BUTT_SLIDE | ACT_STOMACH_SLIDE | ACT_HOLD_BUTT_SLIDE | ACT_HOLD_STOMACH_SLIDE | ACT_DIVE_SLIDE | ACT_CROUCH_SLIDE
        | ACT_SLIDE_KICK_SLIDE | ACT_BEGIN_SLIDING | ACT_HOLD_BEGIN_SLIDING => &[Looping { name: "MOVING_TERRAIN_SLIDE", period: 3 }],
        ACT_LAVA_BOOST_LAND => &[Looping { name: "MOVING_LAVA_BURN", period: 10 }],
        ACT_GETTING_BLOWN => &[Looping { name: "ENV_WIND2", period: 8 }],
        ACT_CLIMBING_POLE => &[Looping { name: "ACTION_CLIMB_UP_POLE", period: 12 }],
        ACT_HANG_MOVING => &[Looping { name: "ACTION_HANGING_STEP", period: 14 }],
        ACT_METAL_WATER_WALKING => &[Looping { name: "ACTION_METAL_STEP_WATER", period: 16 }],
        _ => &[],
    }
}

/// Every name this module can request (for the "ids exist in the table" test).
pub const NAMES: &[&str] = &[
    "MARIO_PUNCH_YAH", "MARIO_PUNCH_WAH", "MARIO_PUNCH_HOO", "MARIO_HRMM", "MARIO_WAH2", "ACTION_THROW", "MARIO_WHOA", "MARIO_EEUH",
    "MARIO_UH2", "ACTION_TERRAIN_LANDING", "ACTION_HANGING_STEP", "MARIO_ON_FIRE", "MOVING_LAVA_BURN", "MARIO_YAWNING", "MARIO_DROWNING",
    "ACTION_METAL_JUMP_WATER", "ACTION_METAL_LAND_WATER", "MOVING_TERRAIN_SLIDE", "ENV_WIND2", "ACTION_CLIMB_UP_POLE",
    "ACTION_METAL_STEP_WATER",
];
