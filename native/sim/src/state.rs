//! Mario state and controller snapshot (src/sim/state.ts).
use crate::collision::Surface;
use crate::math::Vec3;
use std::sync::Arc;

pub use crate::state_consts::*;
pub use crate::water_consts::*;

#[derive(Clone, Debug, Default)]
pub struct ControllerSnapshot {
    /// Facing direction while Link holds focus.
    pub focus_yaw: Option<f64>,
    /// [-64, 64], positive right
    pub stick_x: f64,
    /// [-64, 64], positive up
    pub stick_y: f64,
    /// [0, 64]
    pub stick_mag: f64,
    pub button_down: u32,
    pub button_pressed: u32,
}

#[derive(Clone, Debug)]
pub struct MarioState {
    pub slide_terrain: bool,
    pub action: u32,
    pub prev_action: u32,
    pub action_state: f64,
    pub action_timer: f64,
    pub action_arg: f64,

    pub input: u32,
    pub flags: u32,
    pub particle_flags: u32,

    pub pos: Vec3,
    pub vel: Vec3,
    pub face_angle: [f64; 3],

    pub forward_vel: f64,
    pub gravity_scale: f64,
    /// `m->gettingBlownGravity` (ACT_GETTING_BLOWN).
    pub getting_blown_gravity: f64,
    pub angle_vel: [f64; 3],
    pub slide_vel_x: f64,
    pub slide_vel_z: f64,
    pub slide_yaw: f64,
    pub intended_mag: f64,
    pub intended_yaw: f64,

    pub floor: Option<Arc<Surface>>,
    pub ceil: Option<Arc<Surface>>,
    pub wall: Option<Arc<Surface>>,
    pub floor_height: f64,
    pub ceil_height: f64,
    pub floor_angle: f64,

    pub peak_height: f64,
    pub quicksand_depth: f64,
    pub squish_timer: f64,

    pub double_jump_timer: f64,
    pub wall_kick_timer: f64,
    pub frames_since_a: f64,
    pub frames_since_b: f64,

    pub health: f64,
    pub hurt_counter: f64,
    pub heal_counter: f64,
    /// `oMarioBurnTimer` of Mario's object (burning actions).
    pub burn_timer: f64,

    // Purely visual / bookkeeping state. Not read by the physics.
    pub anim_frame: f64,
    pub anim_length: f64,
    pub jump_combo: f64,
    pub lands: f64,
    pub wall_kicks: f64,
    pub respawns: f64,
}

impl Default for MarioState {
    fn default() -> Self {
        Self {
            slide_terrain: false, action: ACT_IDLE, prev_action: 0, action_state: 0.0, action_timer: 0.0, action_arg: 0.0,
            input: 0, flags: 0, particle_flags: 0, pos: [0.0; 3], vel: [0.0; 3], face_angle: [0.0; 3],
            forward_vel: 0.0, gravity_scale: 1.0, getting_blown_gravity: 0.0, angle_vel: [0.0; 3], slide_vel_x: 0.0, slide_vel_z: 0.0, slide_yaw: 0.0,
            intended_mag: 0.0, intended_yaw: 0.0, floor: None, ceil: None, wall: None, floor_height: 0.0, ceil_height: 0.0,
            floor_angle: 0.0, peak_height: 0.0, quicksand_depth: 0.0, squish_timer: 0.0, double_jump_timer: 0.0,
            wall_kick_timer: 0.0, frames_since_a: 0.0, frames_since_b: 0.0, health: 0x880 as f64, hurt_counter: 0.0,
            heal_counter: 0.0, burn_timer: 0.0, anim_frame: 0.0, anim_length: 0.0, jump_combo: 0.0, lands: 0.0, wall_kicks: 0.0, respawns: 0.0,
        }
    }
}

/// Events emitted by the sim for audio and effects (src/sim/state.ts `MarioEvent`).
#[derive(Clone, Debug, PartialEq)]
pub enum MarioEvent {
    Jump { action: u32, vy: f64 },
    Land { speed: f64, vy: f64 },
    WingFlight { pull_up: bool },
    FlightBoundary,
    FlightBurn,
    Bonk { speed: f64 },
    Wallkick,
    Groundpound,
    CannonMove { phase: u8 },
    CannonFire,
    WaterPlunge,
    FlutterKick,
    BowserGrab,
    BowserSpin,
    BowserThrow { fast: bool },
    SwimStroke { fast: bool },
    GroundpoundDrop,
    GroundpoundLand,
    Death,
    Longjump,
    Triplejump,
    Dive,
    Step { speed: f64 },
    CastleEnding,
    EndingCredits,
    DeathLanding,
    MamaMia,
    Respawn,
    Hurt { damage: f64 },
}
