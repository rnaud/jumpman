//! Port of `sm64Animation.ts` (action -> Mario clip selection) and the frame clock of
//! `Sm64AnimationPlayer.ts`. Engine-free: it produces `(clip id, sample frame)` per 30 Hz sim
//! tick; `render::mario_view` turns that into Bevy `AnimationPlayer` seeks.
//!
//! The exported Mario glb has one clip per SM64 animation id; the clip with id `0xC5` is glTF
//! animation index `0xC5` (named `anim_C5`).
use jumpman_sim::state::*;
use serde_json::Value;

// IDs from include/mario_animation_ids.h.
pub mod anim {
    pub const IDLE: u32 = 0xc5;
    pub const IDLE_LEFT: u32 = 0xc3;
    pub const TIPTOE_START: u32 = 0xca;
    pub const TIPTOE: u32 = 0x92;
    pub const WALK: u32 = 0x48;
    pub const RUN: u32 = 0x72;
    pub const JUMP: u32 = 0x4d;
    pub const JUMP_LAND: u32 = 0x4e;
    pub const DOUBLE_RISE: u32 = 0x50;
    pub const DOUBLE_FALL: u32 = 0x4c;
    pub const DOUBLE_LAND: u32 = 0x4b;
    pub const TRIPLE: u32 = 0xc1;
    pub const TRIPLE_LAND: u32 = 0xc0;
    pub const BACKFLIP: u32 = 0x04;
    pub const SIDEFLIP: u32 = 0xbf;
    pub const SIDE_LAND: u32 = 0xbe;
    pub const LONG_JUMP: u32 = 0x13;
    pub const LONG_JUMP_SLOW: u32 = 0x14;
    pub const LONG_LAND: u32 = 0x11;
    pub const LONG_LAND_SLOW: u32 = 0x12;
    pub const FALL: u32 = 0x56;
    pub const FALL_FROM_SLIDE: u32 = 0x90;
    pub const FALL_FROM_SLIDE_KICK: u32 = 0x53;
    pub const LAND: u32 = 0x57;
    pub const CROUCH: u32 = 0x98;
    pub const CROUCH_START: u32 = 0x97;
    pub const CROUCH_STOP: u32 = 0x96;
    pub const CRAWL: u32 = 0x99;
    pub const CRAWL_START: u32 = 0x9b;
    pub const CRAWL_STOP: u32 = 0x9a;
    pub const DIVE: u32 = 0x88;
    pub const DIVE_SLIDE: u32 = 0x89;
    pub const SLIDE: u32 = 0x91;
    pub const SLIDE_STOP: u32 = 0x8f;
    pub const SLIDE_KICK: u32 = 0x8c;
    pub const SLIDE_KICK_STOP: u32 = 0x8d;
    pub const POUND_START: u32 = 0x3c;
    pub const TRIPLE_POUND: u32 = 0x3b;
    pub const POUND: u32 = 0x3d;
    pub const POUND_LAND: u32 = 0x3a;
    pub const WALL_KICK: u32 = 0xcb;
    pub const HIT_WALL: u32 = 0xcc;
    pub const AIR_KICK: u32 = 0x4f;
    pub const PUNCH: u32 = 0x67;
    pub const PUNCH_FAST: u32 = 0x69;
    pub const PUNCH_SECOND: u32 = 0x68;
    pub const PUNCH_SECOND_FAST: u32 = 0x6a;
    pub const KICK: u32 = 0x66;
    pub const BREAKDANCE: u32 = 0x71;
    pub const BRAKE: u32 = 0x0f;
    pub const BRAKE_STOP: u32 = 0x10;
    pub const TURN: u32 = 0xbc;
    pub const TURN_END: u32 = 0xbd;
    pub const DEATH_ON_BACK: u32 = 0x03;
    pub const DEATH_ON_STOMACH: u32 = 0x2e;
    pub const DEATH_FALL_OVER: u32 = 0x32;
    pub const DROWNING1: u32 = 0xa5;
    pub const DROWNING2: u32 = 0xa6;
    pub const WATER_DYING: u32 = 0xa7;
    pub const HARD_BACKWARD_KB: u32 = 0x01;
    pub const HARD_FORWARD_KB: u32 = 0x2c;
    pub const LEDGE: u32 = 0x33;
    pub const LEDGE_CLIMB: u32 = 0x34;
    pub const SLOW_LEDGE: u32 = 0x00;
    pub const HANG_ON_CEILING: u32 = 0x35;
    pub const HANDSTAND_LEFT: u32 = 0xc6;
    pub const HANDSTAND_RIGHT: u32 = 0xc7;
    pub const POLE_CLIMB: u32 = 0x05;
    pub const POLE_GRAB: u32 = 0x06;
    pub const POLE_TOP: u32 = 0x09;
    pub const POLE_TRANSITION: u32 = 0x0b;
    pub const POLE_IDLE: u32 = 0x0d;
    pub const LAVA_LAND: u32 = 0x28;
    pub const LAVA_BURN: u32 = 0x29;
    pub const QUICKSAND_DEATH: u32 = 0x76;
    pub const WIRE_LEFT: u32 = 0x5d;
    pub const WIRE_RIGHT: u32 = 0x5c;
    pub const START_SLEEP_IDLE: u32 = 0x81;
    pub const START_SLEEP_SCRATCH: u32 = 0x82;
    pub const START_SLEEP_YAWN: u32 = 0x83;
    pub const START_SLEEP_SITTING: u32 = 0x84;
    pub const SLEEP_IDLE: u32 = 0x85;
    pub const SLEEP_START_LYING: u32 = 0x86;
    pub const SLEEP_LYING: u32 = 0x87;
    pub const WAKE_FROM_SLEEP: u32 = 0xc8;
    pub const PICK_UP: u32 = 0x6b;
    pub const PLACE_DOWN: u32 = 0x6e;
    pub const GROUND_THROW: u32 = 0x65;
    pub const AIR_THROW: u32 = 0x52;
    pub const HOLD_IDLE: u32 = 0x3f;
    pub const HOLD_JUMP: u32 = 0x41;
    pub const HOLD_FALL: u32 = 0x43;
    pub const HOLD_LAND: u32 = 0x40;
    pub const HOLD_FALL_LAND: u32 = 0x42;
    pub const HOLD_SLOW_WALK: u32 = 0x18;
    pub const HOLD_WALK: u32 = 0x16;
    pub const HOLD_RUN: u32 = 0x17;
}
use anim as an;

/// The slice of Mario state `chooseSm64Clip` reads (`Sm64AnimationState`).
#[derive(Clone, Debug, Default)]
pub struct AnimInput {
    pub action: u32,
    pub action_arg: f64,
    pub action_state: f64,
    pub action_timer: f64,
    pub forward_vel: f64,
    pub intended_mag: f64,
    pub holding: bool,
    pub floor_very_slippery: bool,
    pub long_jump_is_slow: bool,
    pub angle_vel: [f64; 3],
    pub face_angle: [f64; 3],
    pub vel_y: f64,
    /// Submerged actions: the animation the sim's clock (`actions/water.rs`, `set_mario_animation`)
    /// selected this tick as `(animation id, animAccel)`, plus the graphics pitch `update_water_pitch` left.
    pub water_anim: Option<(i32, i32)>,
    pub water_gfx_pitch: f64,
}

impl AnimInput {
    pub fn from_mario(m: &jumpman_sim::mario_runtime::MarioRuntime) -> Self {
        Self {
            action: m.action,
            action_arg: m.action_arg,
            action_state: m.action_state,
            action_timer: m.action_timer,
            forward_vel: m.forward_vel,
            intended_mag: m.intended_mag,
            holding: m.held_obj.is_some(),
            floor_very_slippery: m.floor_class() == jumpman_sim::surfaces::SURFACE_CLASS_VERY_SLIPPERY,
            long_jump_is_slow: m.long_jump_is_slow,
            angle_vel: m.angle_vel,
            face_angle: m.face_angle,
            vel_y: m.vel[1],
            water_anim: (m.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED && m.water.clock.id >= 0).then_some((m.water.clock.id, m.water.clock.accel)),
            water_gfx_pitch: m.water.gfx_angle[0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipChoice {
    pub id: u32,
    pub speed: f64,
    pub looping: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Playback {
    pub clip: Option<u32>,
    pub frame: f64,
    pub cycles: f64,
    pub finished: bool,
}

fn clip(id: u32) -> ClipChoice {
    ClipChoice { id, speed: 1.0, looping: None }
}
fn clip_at(id: u32, speed: f64) -> ClipChoice {
    ClipChoice { id, speed, looping: None }
}
fn clip_loop(id: u32, speed: f64, looping: bool) -> ClipChoice {
    ClipChoice { id, speed, looping: Some(looping) }
}

/// `chooseSm64Clip`. Unmapped actions fall back to idle instead of throwing.
pub fn choose_clip(m: &AnimInput, previous: Option<&Playback>) -> ClipChoice {
    let arg = m.action_arg;
    let state = m.action_state;
    let timer = m.action_timer;
    let prev_is = |id: u32| previous.map_or(false, |p| p.clip == Some(id));
    // Every submerged action plays whatever the decomp's action code asked for (the sim keeps the
    // animation clock for `is_anim_at_end`); `animAccel` is a 16.16 playback rate.
    if let Some((id, accel)) = m.water_anim {
        return clip_at(id as u32, if accel != 0 { accel as f64 / 65536.0 } else { 1.0 });
    }
    match m.action {
        ACT_FALL_AFTER_STAR_GRAB => clip(an::FALL),
        ACT_STAR_DANCE_EXIT | ACT_STAR_DANCE_NO_EXIT => clip(if state == 2.0 { 0xce } else { 0xcd }),
        ACT_STAR_DANCE_WATER => clip(if state == 2.0 { 0xb4 } else { 0xb3 }),
        ACT_READING_SIGN => clip(0xc2),
        ACT_READING_NPC_DIALOG => clip(if m.holding { an::HOLD_IDLE } else { 0xc2 }),
        ACT_JUMBO_STAR_CUTSCENE => {
            if arg == 0.0 {
                clip(if state != 0.0 { an::LAND } else { an::FALL })
            } else if arg == 1.0 {
                clip(if state != 0.0 { 0x26 } else { 0x25 })
            } else {
                clip(0x2a)
            }
        }
        ACT_END_PEACH_CUTSCENE => match arg as i32 {
            0 => clip(an::FALL),
            1 => clip(an::LAND),
            2 => clip(if timer < 96.0 { 0x20 } else { 0x21 }),
            3 => clip(0x21),
            4 => clip(if timer < 200.0 { 0x21 } else { 0x1f }),
            5 => clip_at(an::RUN, 8.0),
            6 => clip(if timer < 27.0 { 0x22 } else { 0x1e }),
            7 | 8 => clip(0x1e),
            9 => clip(0x27),
            10 => clip(0xc2),
            _ => clip(if timer < 100.0 { 0x23 } else { 0x24 }),
        },
        ACT_WATER_IDLE => clip_loop(178, 1.0, true),
        ACT_IN_CANNON => clip(an::DIVE),
        ACT_SHOT_FROM_CANNON => clip(0x15),
        ACT_FLYING_TRIPLE_JUMP => clip(if state != 0.0 { 0x6f } else { 0xd0 }),
        ACT_FLYING => {
            if state != 0.0 {
                clip_loop(0x2a, 1.0, true)
            } else {
                clip(if arg != 0.0 { 0xcf } else { 0x5b })
            }
        }
        ACT_GRABBED => clip_loop(an::HARD_BACKWARD_KB, 1.0, true),
        ACT_TELEPORT_FADE_IN => clip(0xc2),
        ACT_TELEPORT_FADE_OUT => clip(if arg != 0.0 { an::CROUCH } else { 0xc2 }),
        ACT_RIDING_SHELL_GROUND => clip(if arg != 0.0 { 0x47 } else { 0x6d }),
        ACT_RIDING_SHELL_FALL | ACT_RIDING_SHELL_JUMP => clip(0x4a),
        ACT_METAL_WATER_STANDING => clip_loop(an::IDLE, 1.0, true),
        ACT_METAL_WATER_WALKING => clip_loop(an::WALK, 1.0, true),
        ACT_METAL_WATER_FALLING => clip_loop(an::FALL, 1.0, true),
        ACT_METAL_WATER_JUMP => clip(an::JUMP),
        ACT_SOFT_BACKWARD_GROUND_KB => clip(0x74),
        ACT_SOFT_FORWARD_GROUND_KB => clip(0x75),
        ACT_BACKWARD_GROUND_KB => clip(0x7b),
        ACT_FORWARD_GROUND_KB => clip(0x7c),
        ACT_BACKWARD_AIR_KB => clip(0x02),
        ACT_FORWARD_AIR_KB => clip(0x2d),
        ACT_HARD_BACKWARD_AIR_KB | ACT_HARD_BACKWARD_GROUND_KB => clip(an::HARD_BACKWARD_KB),
        ACT_HARD_FORWARD_AIR_KB | ACT_HARD_FORWARD_GROUND_KB => clip(an::HARD_FORWARD_KB),
        ACT_SPAWN_SPIN_AIRBORNE => clip(if state == 0.0 { 0x6f } else { an::FALL }),
        ACT_SPAWN_NO_SPIN_AIRBORNE | ACT_SPECIAL_EXIT_AIRBORNE => clip(an::JUMP),
        ACT_DEATH_EXIT_LAND => clip(0x01),
        ACT_SPECIAL_DEATH_EXIT => clip(0x02),
        ACT_DEATH_EXIT | ACT_FALLING_DEATH_EXIT | ACT_UNUSED_DEATH_EXIT | ACT_EXIT_AIRBORNE | ACT_FALLING_EXIT_AIRBORNE => clip(an::FALL),
        ACT_SPAWN_NO_SPIN_LANDING | ACT_SPAWN_SPIN_LANDING => clip(an::LAND),
        ACT_EXIT_LAND_SAVE_DIALOG => {
            if state == 1.0 {
                clip(0x31)
            } else if state == 3.0 {
                clip(0x37)
            } else {
                clip(if arg != 0.0 { an::JUMP_LAND } else { an::LAND })
            }
        }
        ACT_ENTERING_STAR_DOOR => {
            if timer < 15.0 {
                clip(0xc2)
            } else {
                clip_at(an::WALK, 2.5)
            }
        }
        ACT_UNLOCKING_STAR_DOOR => clip(if state == 3.0 { 0x9d } else { 0x9c }),
        ACT_UNLOCKING_KEY_DOOR => clip(0x61),
        ACT_PULLING_DOOR => clip(0x5f),
        ACT_PUSHING_DOOR => clip(0x60),
        ACT_WATER_JUMP => clip(an::JUMP),
        ACT_WATER_ACTION_END | ACT_WATER_PLUNGE => clip(173),
        ACT_BREASTSTROKE => clip(170),
        ACT_SWIMMING_END => clip(171),
        ACT_FLUTTER_KICK => clip_loop(172, 1.0, true),
        ACT_IDLE => {
            let seq = [0xc3, 0xc4, 0xc5];
            let index = seq.iter().position(|&id| prev_is(id));
            let next = match index {
                None => 0,
                Some(i) => (i + usize::from(previous.map_or(false, |p| p.finished))) % seq.len(),
            };
            clip_loop(seq[next], 1.0, false)
        }
        ACT_WALKING => {
            let speed = m.intended_mag.max(m.forward_vel).max(4.0);
            if speed <= 8.0 && (previous.is_none() || ![an::WALK, an::RUN, an::TIPTOE].iter().any(|&id| prev_is(id))) {
                let p = previous;
                if !prev_is(an::TIPTOE_START) || p.is_none() || p.unwrap().cycles == 0.0 || p.unwrap().frame < 23.0 {
                    return clip_at(an::TIPTOE_START, speed / 4.0);
                }
            }
            if speed < 5.0 || (prev_is(an::TIPTOE) && speed <= 8.0) {
                return clip_at(an::TIPTOE, speed);
            }
            if speed > 22.0 || (prev_is(an::RUN) && speed >= 18.0) {
                return clip_at(an::RUN, speed / 4.0);
            }
            clip_at(an::WALK, speed / 4.0)
        }
        ACT_DECELERATING => {
            if m.floor_very_slippery {
                clip(an::IDLE_LEFT)
            } else {
                clip_at(an::WALK, (m.forward_vel / 4.0).max(1.0 / 16.0))
            }
        }
        ACT_TURNING_AROUND => clip(if m.forward_vel >= 18.0 { an::TURN } else { an::TURN_END }),
        ACT_FINISH_TURNING_AROUND => clip(an::TURN_END),
        ACT_BRAKING => clip(an::BRAKE),
        ACT_BRAKING_STOP => clip(an::BRAKE_STOP),
        ACT_JUMP | ACT_STEEP_JUMP => clip(an::JUMP),
        ACT_DOUBLE_JUMP => clip(if m.vel_y >= 0.0 { an::DOUBLE_RISE } else { an::DOUBLE_FALL }),
        ACT_TRIPLE_JUMP => clip(an::TRIPLE),
        ACT_BACKFLIP => clip(an::BACKFLIP),
        ACT_SIDE_FLIP => clip(an::SIDEFLIP),
        ACT_LONG_JUMP => clip(if m.long_jump_is_slow { an::LONG_JUMP_SLOW } else { an::LONG_JUMP }),
        ACT_WALL_KICK_AIR => clip(an::WALL_KICK),
        ACT_AIR_HIT_WALL => clip(an::HIT_WALL),
        ACT_JUMP_KICK => clip(an::AIR_KICK),
        ACT_JUMP_LAND | ACT_JUMP_LAND_STOP => clip(an::JUMP_LAND),
        ACT_DOUBLE_JUMP_LAND | ACT_DOUBLE_JUMP_LAND_STOP => clip(an::DOUBLE_LAND),
        ACT_BACKFLIP_LAND | ACT_BACKFLIP_LAND_STOP | ACT_TRIPLE_JUMP_LAND | ACT_TRIPLE_JUMP_LAND_STOP => clip(an::TRIPLE_LAND),
        ACT_SIDE_FLIP_LAND | ACT_SIDE_FLIP_LAND_STOP => clip(an::SIDE_LAND),
        ACT_LONG_JUMP_LAND | ACT_LONG_JUMP_LAND_STOP => clip(if m.long_jump_is_slow { an::LONG_LAND_SLOW } else { an::LONG_LAND }),
        ACT_FREEFALL_LAND | ACT_FREEFALL_LAND_STOP => clip(an::LAND),
        ACT_FREEFALL => clip(if arg == 1.0 {
            an::FALL_FROM_SLIDE
        } else if arg == 2.0 {
            an::FALL_FROM_SLIDE_KICK
        } else {
            an::FALL
        }),
        ACT_SOFT_BONK => clip(an::FALL),
        ACT_CROUCHING => clip(an::CROUCH),
        ACT_PANTING => clip(0xba),
        ACT_COUGHING => clip(0x30),
        ACT_SHIVERING => clip([0x19, 0x1b, 0x1a][(state as usize).min(2)]),
        ACT_IN_QUICKSAND => clip(0x77),
        ACT_STANDING_AGAINST_WALL => clip(0x7e),
        ACT_CROUCH_SLIDE | ACT_START_CROUCHING => clip(an::CROUCH_START),
        ACT_STOP_CROUCHING => clip(an::CROUCH_STOP),
        ACT_CRAWLING => clip_at(an::CRAWL, (m.intended_mag * 2.0).max(0.0)),
        ACT_START_CRAWLING => clip(an::CRAWL_START),
        ACT_STOP_CRAWLING => clip(an::CRAWL_STOP),
        ACT_DIVE | ACT_DIVE_SLIDE => clip(an::DIVE),
        ACT_STOMACH_SLIDE => clip(an::DIVE_SLIDE),
        ACT_BUTT_SLIDE | ACT_BUTT_SLIDE_AIR => clip(an::SLIDE),
        ACT_BUTT_SLIDE_STOP => clip(an::SLIDE_STOP),
        ACT_SLIDE_KICK | ACT_SLIDE_KICK_SLIDE => clip(an::SLIDE_KICK),
        ACT_SLIDE_KICK_SLIDE_STOP => clip(an::SLIDE_KICK_STOP),
        ACT_GROUND_POUND => clip(if state == 0.0 {
            if arg == 0.0 {
                an::POUND_START
            } else {
                an::TRIPLE_POUND
            }
        } else {
            an::POUND
        }),
        ACT_GROUND_POUND_LAND => clip(an::POUND_LAND),
        ACT_PUNCHING | ACT_MOVE_PUNCHING => {
            let a = arg as i32;
            clip(match a {
                9 => an::BREAKDANCE,
                6 => an::KICK,
                5 => an::PUNCH_SECOND_FAST,
                3 | 4 => an::PUNCH_SECOND,
                2 => an::PUNCH_FAST,
                _ => an::PUNCH,
            })
        }
        ACT_LEDGE_GRAB => clip(an::LEDGE),
        ACT_LEDGE_CLIMB_SLOW_1 | ACT_LEDGE_CLIMB_SLOW_2 => clip(an::SLOW_LEDGE),
        ACT_LEDGE_CLIMB_FAST => clip(an::LEDGE_CLIMB),
        ACT_START_HANGING => clip(an::HANG_ON_CEILING),
        ACT_GRAB_POLE_SLOW => clip(an::POLE_GRAB),
        ACT_GRAB_POLE_FAST => clip(if state != 0.0 { 0x08 } else { 0x07 }),
        ACT_HOLDING_POLE => clip(an::POLE_IDLE),
        ACT_CLIMBING_POLE => clip(an::POLE_CLIMB),
        ACT_TOP_OF_POLE_TRANSITION => clip(if arg == 0.0 { an::POLE_TRANSITION } else { 0x0c }),
        ACT_TOP_OF_POLE_JUMP => clip(0x0a),
        ACT_TOP_OF_POLE => clip(an::POLE_TOP),
        ACT_LAVA_BOOST => clip(an::LAVA_BURN),
        ACT_BURNING_JUMP => clip(if arg == 0.0 { an::JUMP } else { an::LAVA_BURN }),
        ACT_BURNING_FALL => clip(an::FALL),
        ACT_BURNING_GROUND => clip_at(an::RUN, m.forward_vel / 4.0),
        ACT_LAVA_BOOST_LAND => clip(an::LAVA_LAND),
        ACT_QUICKSAND_DEATH => clip(an::QUICKSAND_DEATH),
        ACT_HANGING => clip(if (arg as i32) & 1 != 0 { an::HANDSTAND_LEFT } else { an::HANDSTAND_RIGHT }),
        ACT_HANG_MOVING => clip(if (arg as i32) & 1 != 0 { an::WIRE_LEFT } else { an::WIRE_RIGHT }),
        ACT_START_SLEEPING => clip([an::START_SLEEP_IDLE, an::START_SLEEP_SCRATCH, an::START_SLEEP_YAWN, an::START_SLEEP_SITTING][(state as usize).min(3)]),
        ACT_SLEEPING => clip([an::SLEEP_IDLE, an::SLEEP_START_LYING, an::SLEEP_LYING][(state as usize).min(2)]),
        ACT_WAKING_UP => clip(an::WAKE_FROM_SLEEP),
        ACT_STANDING_DEATH => clip(an::DEATH_FALL_OVER),
        ACT_DEATH_ON_BACK => clip(an::DEATH_ON_BACK),
        ACT_DEATH_ON_STOMACH => clip(an::DEATH_ON_STOMACH),
        ACT_DROWNING => clip(if state == 0.0 { an::DROWNING1 } else { an::DROWNING2 }),
        ACT_WATER_DEATH => clip(an::WATER_DYING),
        ACT_PICKING_UP_BOWSER => clip(0xb5),
        ACT_HOLDING_BOWSER => clip_loop(if m.angle_vel[1] != 0.0 { 0xb6 } else { 0xb8 }, 1.0, true),
        ACT_RELEASING_BOWSER => clip(0xb7),
        ACT_HOLD_BUTT_SLIDE_STOP => clip(0x46),
        ACT_DIVE_PICKING_UP => clip(0x8b),
        ACT_HOLD_BUTT_SLIDE_AIR | ACT_HOLD_BUTT_SLIDE => clip(0x45),
        ACT_HOLD_STOMACH_SLIDE => clip(0x89),
        ACT_PICKING_UP => clip(an::PICK_UP),
        ACT_PLACING_DOWN => clip(an::PLACE_DOWN),
        ACT_THROWING => clip(an::GROUND_THROW),
        ACT_AIR_THROW | ACT_AIR_THROW_LAND => clip(an::AIR_THROW),
        ACT_HOLD_IDLE => clip_loop(an::HOLD_IDLE, 1.0, true),
        ACT_HOLD_WALKING => {
            let speed = m.intended_mag.max(m.forward_vel).max(4.0);
            if speed > 11.0 {
                clip_at(an::HOLD_RUN, speed / 2.0)
            } else if speed > 3.0 {
                clip_at(an::HOLD_WALK, speed)
            } else {
                clip_at(an::HOLD_SLOW_WALK, speed)
            }
        }
        ACT_HOLD_DECELERATING => clip_at(an::HOLD_SLOW_WALK, (m.forward_vel / 4.0).max(1.0 / 16.0)),
        ACT_HOLD_JUMP => clip(an::HOLD_JUMP),
        ACT_HOLD_FREEFALL => clip(an::HOLD_FALL),
        ACT_HOLD_JUMP_LAND | ACT_HOLD_JUMP_LAND_STOP => clip(an::HOLD_LAND),
        ACT_HOLD_FREEFALL_LAND | ACT_HOLD_FREEFALL_LAND_STOP => clip(an::HOLD_FALL_LAND),
        // TS throws here; keep the model alive with the idle pose instead.
        _ => clip_loop(an::IDLE, 1.0, true),
    }
}

/// `sm64VisualRotation`: graphics-only yaw/pitch/roll offsets (radians).
pub fn visual_rotation(m: &AnimInput) -> (f64, f64, f64) {
    use std::f64::consts::PI;
    let a = |v: f64| v * PI / 32768.0;
    if m.action == ACT_FLYING || m.action == ACT_SHOT_FROM_CANNON || (m.action == ACT_JUMBO_STAR_CUTSCENE && m.action_arg == 2.0) {
        return (0.0, -a(m.face_angle[0]), a(m.face_angle[2]));
    }
    let reverse = matches!(m.action, ACT_SIDE_FLIP | ACT_SIDE_FLIP_LAND | ACT_SIDE_FLIP_LAND_STOP | ACT_FINISH_TURNING_AROUND);
    if m.action == ACT_HOLDING_BOWSER {
        return (0.0, a(m.angle_vel[1].abs()), 0.0);
    }
    if m.water_anim.is_some() {
        // gfx.angle = (-faceAngle[0] as shaped by update_water_pitch, faceAngle[1], faceAngle[2]).
        // Metal-cap walking and jumping go through the ground/air step, which resets gfx.angle to (0, yaw, 0).
        if m.action & ACT_FLAG_SWIMMING == 0 { return (0.0, 0.0, 0.0); }
        return (0.0, a(m.water_gfx_pitch), a(m.face_angle[2]));
    }
    let pitch = if m.action == ACT_DIVE || m.action & ACT_FLAG_SWIMMING != 0 { -a(m.face_angle[0]) } else { 0.0 };
    (if reverse { PI } else { 0.0 }, pitch, 0.0)
}

/// `continuesSm64Clip`.
pub fn continues_clip(previous_action: u32, action: u32) -> bool {
    if previous_action == action {
        return true;
    }
    let from: &[u32] = match action {
        ACT_JUMP_LAND_STOP => &[ACT_JUMP_LAND],
        ACT_DOUBLE_JUMP_LAND_STOP => &[ACT_DOUBLE_JUMP_LAND],
        ACT_TRIPLE_JUMP_LAND_STOP => &[ACT_TRIPLE_JUMP_LAND],
        ACT_BACKFLIP_LAND_STOP => &[ACT_BACKFLIP_LAND],
        ACT_SIDE_FLIP_LAND_STOP => &[ACT_SIDE_FLIP_LAND],
        ACT_LONG_JUMP_LAND_STOP => &[ACT_LONG_JUMP_LAND],
        ACT_FREEFALL_LAND_STOP => &[ACT_FREEFALL_LAND],
        ACT_DIVE_SLIDE => &[ACT_DIVE],
        ACT_SLIDE_KICK_SLIDE => &[ACT_SLIDE_KICK],
        ACT_BUTT_SLIDE => &[ACT_BUTT_SLIDE_AIR],
        ACT_BUTT_SLIDE_AIR => &[ACT_BUTT_SLIDE],
        ACT_FINISH_TURNING_AROUND => &[ACT_TURNING_AROUND],
        ACT_START_CROUCHING => &[ACT_CROUCH_SLIDE],
        ACT_DECELERATING => &[ACT_WALKING],
        ACT_WALKING => &[ACT_DECELERATING],
        _ => &[],
    };
    from.contains(&previous_action)
}

/// Per-clip playback metadata from the glb's animation `extras`.
#[derive(Clone, Copy, Debug)]
pub struct ClipMeta {
    pub fps: f64,
    pub start_frame: f64,
    pub loop_start: f64,
    pub looping: bool,
    pub backward: bool,
    /// Number of source frames (keyframes).
    pub frames: f64,
}

impl Default for ClipMeta {
    fn default() -> Self {
        Self { fps: 30.0, start_frame: 0.0, loop_start: 0.0, looping: false, backward: false, frames: 1.0 }
    }
}

/// Read per-clip metadata (indexed by glTF animation index == SM64 animation id) from a glb.
pub fn read_clip_meta(glb: &[u8]) -> Vec<ClipMeta> {
    let json = match glb_json(glb) {
        Some(j) => j,
        None => return vec![],
    };
    let accessors = json["accessors"].as_array().cloned().unwrap_or_default();
    let mut out = vec![];
    for anim in json["animations"].as_array().cloned().unwrap_or_default() {
        let extras = &anim["extras"];
        let mut meta = ClipMeta::default();
        if let Some(v) = extras["fps"].as_f64() {
            meta.fps = v;
        }
        meta.start_frame = extras["startFrame"].as_f64().unwrap_or(0.0);
        meta.loop_start = extras["loopStart"].as_f64().unwrap_or(0.0);
        meta.looping = extras["loop"].as_bool().unwrap_or(false);
        meta.backward = extras["backward"].as_bool().unwrap_or(false);
        let input = anim["samplers"][0]["input"].as_u64().map(|i| i as usize);
        meta.frames = input.and_then(|i| accessors.get(i)).and_then(|a| a["count"].as_f64()).unwrap_or(1.0);
        out.push(meta);
    }
    out
}

/// The JSON chunk of a binary glTF.
pub fn glb_json(glb: &[u8]) -> Option<Value> {
    if glb.len() < 20 || &glb[0..4] != b"glTF" {
        return None;
    }
    let len = u32::from_le_bytes(glb[12..16].try_into().ok()?) as usize;
    serde_json::from_slice(glb.get(20..20 + len)?).ok()
}

/// Output of one player tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub clip: u32,
    /// Integer source frame to display.
    pub frame: u32,
    /// Seconds into the glTF clip (`frame / fps`).
    pub seconds: f32,
    /// Fractional playback position in seconds (not floored to a source frame); the renderer
    /// samples between keyframes with it (TS `smooth` mode).
    pub sample_seconds: f32,
    /// This tick advanced the same clip from the previous one without a restart or loop wrap, so
    /// the previous pose's `sample_seconds` can be interpolated towards this one.
    pub continuous: bool,
}

impl Pose {
    /// Clip time to display at `t` in [0, 1) between the previous and current 30 Hz tick.
    pub fn seconds_at(prev: &Pose, cur: &Pose, t: f64) -> f32 {
        if cur.continuous && prev.clip == cur.clip {
            prev.sample_seconds + (cur.sample_seconds - prev.sample_seconds) * t as f32
        } else {
            cur.sample_seconds
        }
    }
}

/// `Sm64AnimationPlayer`'s frame clock, advanced once per 30 Hz sim tick.
pub struct Sm64AnimationPlayer {
    meta: Vec<ClipMeta>,
    current: Option<u32>,
    last_action: u32,
    last_arg: f64,
    last_timer: f64,
    position: f64,
    speed: f64,
    frame: f64,
    cycles: f64,
    looping: bool,
}

impl Sm64AnimationPlayer {
    pub fn new(meta: Vec<ClipMeta>) -> Self {
        Self { meta, current: None, last_action: u32::MAX, last_arg: -1.0, last_timer: -1.0, position: 0.0, speed: 1.0, frame: 0.0, cycles: 0.0, looping: false }
    }

    fn playback(&self) -> Playback {
        let finished = self.current.and_then(|c| self.meta.get(c as usize)).map_or(false, |m| self.position >= m.frames);
        Playback { clip: self.current, frame: self.frame, cycles: self.cycles, finished }
    }

    pub fn current_clip(&self) -> Option<u32> {
        self.current
    }

    /// Advance by `dt` seconds (1/30 per sim tick) and return the pose to show.
    pub fn update(&mut self, m: &AnimInput, dt: f64) -> Pose {
        let previous = self.playback();
        let use_prev = !(m.action == ACT_IDLE && self.last_action != ACT_IDLE);
        let mut choice = choose_clip(m, if use_prev { Some(&previous) } else { None });
        if self.meta.get(choice.id as usize).is_none() {
            choice.id = an::IDLE;
        }
        let meta = self.meta.get(choice.id as usize).copied().unwrap_or_default();
        let frames = meta.frames.max(1.0);
        let fps = meta.fps;
        self.looping = choice.looping.unwrap_or(meta.looping);
        let was = self.current;
        let restart = self.current != Some(choice.id)
            || !continues_clip(self.last_action, m.action)
            || m.action_arg != self.last_arg
            || (!self.looping && m.action == self.last_action && m.action_timer < self.last_timer);
        if restart {
            self.current = Some(choice.id);
            self.position = meta.start_frame.clamp(0.0, frames - 1.0);
        } else {
            self.position += dt.max(0.0) * fps * choice.speed * if meta.backward { -1.0 } else { 1.0 };
        }
        let low = meta.loop_start.clamp(0.0, frames - 1.0);
        let span = frames - low;
        self.cycles = 0.0;
        let mut sample = self.position;
        let mut wrapped = false;
        if self.looping && (sample >= frames || (meta.backward && sample < low)) {
            wrapped = true;
            self.cycles = ((sample - low) / span).floor().abs();
            sample = low + ((sample - low) % span + span) % span;
        }
        self.frame = (sample + 1e-6).floor().clamp(0.0, frames - 1.0);
        self.speed = choice.speed;
        self.last_action = m.action;
        self.last_arg = m.action_arg;
        self.last_timer = m.action_timer;
        Pose {
            clip: choice.id,
            frame: self.frame as u32,
            seconds: (self.frame / fps) as f32,
            sample_seconds: (sample.clamp(0.0, frames - 1.0) / fps) as f32,
            continuous: !restart && !wrapped && was == Some(choice.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta_table() -> Vec<ClipMeta> {
        (0..0xd1).map(|i| ClipMeta { frames: 30.0, looping: i == an::WALK || i == an::RUN, ..Default::default() }).collect()
    }

    #[test]
    fn idle_cycles_through_sequence() {
        let mut p = Sm64AnimationPlayer::new(meta_table());
        let m = AnimInput { action: ACT_IDLE, ..Default::default() };
        let first = p.update(&m, 0.0);
        assert_eq!(first.clip, 0xc3);
        let mut clips = vec![first.clip];
        for _ in 0..80 {
            clips.push(p.update(&m, 1.0 / 30.0).clip);
        }
        assert!(clips.contains(&0xc4) && clips.contains(&0xc5));
    }

    #[test]
    fn walking_speed_selects_clip() {
        let mut p = Sm64AnimationPlayer::new(meta_table());
        let mut m = AnimInput { action: ACT_WALKING, forward_vel: 12.0, intended_mag: 12.0, ..Default::default() };
        assert_eq!(p.update(&m, 0.0).clip, an::WALK);
        m.forward_vel = 28.0;
        m.intended_mag = 28.0;
        assert_eq!(p.update(&m, 1.0 / 30.0).clip, an::RUN);
    }

    #[test]
    fn pose_interpolates_between_ticks_within_a_clip() {
        let mut p = Sm64AnimationPlayer::new(meta_table());
        let m = AnimInput { action: ACT_WALKING, forward_vel: 12.0, intended_mag: 12.0, ..Default::default() };
        p.update(&m, 0.0);
        let a = p.update(&m, 1.0 / 30.0);
        let b = p.update(&m, 1.0 / 30.0);
        assert!(b.continuous);
        let mid = Pose::seconds_at(&a, &b, 0.5);
        assert!(mid > a.sample_seconds && mid < b.sample_seconds);
        assert!((mid - (a.sample_seconds + b.sample_seconds) / 2.0).abs() < 1e-6);
        let mut last = Pose::seconds_at(&a, &b, 0.0);
        for i in 1..10 {
            let v = Pose::seconds_at(&a, &b, i as f64 / 10.0);
            assert!(v >= last);
            last = v;
        }
        // A restart does not blend from the old clip.
        let n = AnimInput { action: ACT_JUMP, ..Default::default() };
        let c = p.update(&n, 1.0 / 30.0);
        assert!(!c.continuous);
        assert_eq!(Pose::seconds_at(&b, &c, 0.5), c.sample_seconds);
    }

    #[test]
    fn jump_restarts_on_new_action() {
        let mut p = Sm64AnimationPlayer::new(meta_table());
        let m = AnimInput { action: ACT_JUMP, ..Default::default() };
        p.update(&m, 0.0);
        let a = p.update(&m, 1.0 / 30.0);
        assert_eq!((a.clip, a.frame), (an::JUMP, 1));
        let n = AnimInput { action: ACT_JUMP_LAND, ..Default::default() };
        assert_eq!(p.update(&n, 1.0 / 30.0).frame, 0);
    }
}
