//! OoT movement numbers for Link on any collision world (SM64 courses included).
//!
//! Until this module Link ran on Mario's physics with hand-tuned constants: a 32 u/tick run (Mario's
//! own cap), Mario's gravity (4 per tick, terminal -75) and jumps three times as high as the real ones.
//! Here the numbers of `z_player.c` are converted with the same factors as the rest of the Link port
//! (`combat::speed / accel / len`: 1 OoT frame = 1.5 ticks, 1 OoT unit = 4.4 world units) so that
//! Link covers the same distance relative to his own body in the same time as in an OoT scene.
//!
//! Every function is pure; `characters::step_link` calls them when `LinkExt::enabled` (the golden
//! tests of the legacy controller never enable it).
use super::combat::{accel, len, speed, ticks};
use super::items::Boots;
use crate::math::*;
use crate::mario_runtime::MarioRuntime;

/// `R_RUN_SPEED_LIMIT` of `sBootData` (`z_player_lib.c`): Kokiri / Hover / Iron boots.
pub fn run_cap(boots: Boots) -> f64 {
    speed(match boots {
        Boots::Iron => 3.0,
        Boots::Hover => 5.5,
        _ => 6.0,
    })
}

/// Largest control stick magnitude OoT sees (`sControlStickMagnitude` is clamped to 60).
pub const STICK_MAX: f64 = 60.0;

/// `Player_CalcSpeedAndYawFromControlStick`, curved mode: the walking / running speed target for a
/// stick magnitude, a floor pitch (sine of the angle the floor rises along the heading, negative
/// downhill) and the boot's speed cap. `0.14` and the pitch term are the decomp's constants.
pub fn walk_target(stick_mag: f64, uphill_sin: f64, cap: f64) -> f64 {
    let m = stick_mag.min(STICK_MAX) - 20.0;
    if m <= 0.0 {
        return 0.0;
    }
    let t = 1.0 - coss(m * 450.0);
    let base = t * t * 30.0 + 7.0;
    let infl = uphill_sin.clamp(0.0, 0.6);
    let oot = base * 0.14 - 8.0 * infl * infl;
    (oot * speed(1.0)).clamp(0.0, cap)
}

/// `func_8083DF68` (`REG(19)` = 200 up, 1.5 down per frame).
pub const GROUND_ACCEL: f64 = accel(2.0);
pub const GROUND_DECEL: f64 = accel(1.5);
/// `REG(27)` = 2000 angle units per frame while running.
pub const TURN_PER_TICK: f64 = 2000.0 / 1.5;
/// `func_8083C484`: a stick more than 0x6000 from the heading stops him first.
pub const REVERSE_YAW: f64 = 0x6000 as f64;

/// `REG(68)` = -100, and -1.2 while Z-targeting an enemy (`Player_Action_8084411C`).
pub const GRAVITY: f64 = accel(1.0);
pub const GRAVITY_LOCKED: f64 = accel(1.2);
/// `minVelocityY` = -20.
pub const TERMINAL: f64 = -speed(20.0);

/// The air gravity for the current Z-target state.
pub fn gravity(locked: bool) -> f64 {
    if locked { GRAVITY_LOCKED } else { GRAVITY }
}

/// `func_8083BCD0`: backflip (stick back) speed 6.0 / vy 5.8, side hops speed 8.5 / vy 3.5.
pub fn backflip() -> (f64, f64) {
    (speed(6.0), speed(5.8))
}
pub fn side_hop() -> (f64, f64) {
    (speed(8.5), speed(3.5))
}
/// `Player_ActionHandler_10`: A while Z-targeting with the stick forward (jump slash start `5.0, 5.0`).
pub fn front_jump() -> (f64, f64) {
    (speed(5.0), speed(5.0))
}

/// The take-off of the Z-targeted forward jump for this Link (the old numbers behind the capture switch).
pub fn front_jump_for(m: &MarioRuntime) -> (f64, f64) {
    if m.link.phys.legacy { (27.0, 30.0) } else { front_jump() }
}

/// `func_8083A4A8`: the automatic jump when he runs off an edge. `IREG(66..69)` of the Kokiri boots:
/// above 5.9 units per frame vy is 7.5, otherwise 1.25 + 0.2 x speed. Returns the take-off vy.
pub fn ledge_jump_vy(forward_vel: f64) -> f64 {
    let v = forward_vel / speed(1.0);
    speed(if v > 5.9 { 7.5 } else { 1.25 + v * 200.0 / 1000.0 })
}

/// He only jumps off an edge when `speedXZ > 3.0` (and the drop is more than 20 units: the ground step
/// already reports `LEFT_GROUND` only for drops beyond 100 world units, 22.7 OoT units).
pub fn ledge_jump_speed() -> f64 {
    speed(3.0)
}

/// The roll's target speed (`Player_Action_Roll`): 1.5 x the walking target, at least 3.0 units/frame.
pub fn roll_target(walk: f64) -> f64 {
    (walk * 1.5).max(speed(3.0))
}

/// Rolling into a wall at 7.0 units per frame or more bonks him.
pub fn bonk_speed() -> f64 {
    speed(7.0)
}

/// Roll invulnerability: from animation frame 8 for 10 frames (`FRAMERATE_CONST(-10, -8)`).
pub const ROLL_INVINC_TICKS: f64 = 15.0;

/// `func_80843E64` / `Player_Action_8084411C`: fall distances in OoT units.
pub const SHORT_LANDING: f64 = 80.0;
pub const FALL_DAMAGE_LOW: f64 = 400.0;
pub const FALL_DAMAGE_HIGH: f64 = 800.0;

/// What a landing does after falling `distance` world units (from the take-off height) while the
/// stick is (`rolls`) or is not pushed forward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Landing {
    /// `fallDistance <= 80`: a short landing.
    Soft,
    /// 80 to 800 with the stick forward: he rolls out (and takes no damage).
    Roll,
    /// 80 to 400 without the stick: the normal landing crouch.
    Crouch,
    /// Damage in OoT health units (8 = half a heart, 16 = a heart) and the stun in 20 Hz frames.
    Hurt { damage: i32, stun: f64 },
}

pub fn landing(distance: f64, stick_forward: bool) -> Landing {
    let d = distance / len(1.0);
    if d <= SHORT_LANDING {
        Landing::Soft
    } else if d < FALL_DAMAGE_HIGH && stick_forward {
        Landing::Roll
    } else if d >= FALL_DAMAGE_HIGH {
        Landing::Hurt { damage: 16, stun: 20.0 }
    } else if d >= FALL_DAMAGE_LOW {
        Landing::Hurt { damage: 8, stun: 10.0 }
    } else {
        Landing::Crouch
    }
}

/// Stun ticks for a hard landing, and the 40 frames of intangibility after it.
pub fn stun_ticks(stun_frames: f64) -> f64 {
    ticks(stun_frames)
}
pub const FALL_INVINC_TICKS: f64 = 60.0;

/// Sine of the angle the floor under him rises along `yaw` (negative downhill).
pub fn uphill_sin(m: &MarioRuntime, yaw: f64) -> f64 {
    let Some(f) = m.floor.as_ref() else { return 0.0 };
    let n = f.normal;
    if n[1] <= 0.0001 {
        return 0.0;
    }
    let rise = (-(n[0] * sins(yaw)) - n[2] * coss(yaw)) / n[1];
    rise / (1.0 + rise * rise).sqrt()
}

/// The slope slide of `Player_Action_SlideOnSlope` for an SM64 steep floor: the speed target
/// `(1 - ny) * 40` capped at 10 units per frame, the speed gain `max(target^2 * 0.015, 1)` per frame.
/// Returns (target speed, speed gain per tick) in world units.
pub fn slide(normal_y: f64) -> (f64, f64) {
    let t = ((1.0 - normal_y) * 40.0).clamp(0.0, 10.0);
    (speed(t), accel((t * t * 0.015).max(1.0)))
}

/// While climbing (or crossing) a steep floor `Player_HandleSlopes` pushes him back by `(1 - ny) * 40`.
pub fn slope_push(normal_y: f64) -> f64 {
    speed((1.0 - normal_y) * 40.0)
}

/// Per-Link bookkeeping of the movement layer (`LinkExt::phys`).
#[derive(Clone, Debug)]
pub struct Phys {
    /// `JUMPMAN_LINK_LEGACY_MOVE=1`: the pre-parity numbers (Mario's speed, gravity and jumps), for before / after captures.
    pub legacy: bool,
    /// Height of the last tick on the ground (`fallStartHeight`).
    pub fall_start: f64,
    /// Ticks left of the stagger after a hard landing (`fallDamageStunTimer`).
    pub stun: f64,
    /// A roll is carried by a landing (`Player_SetupRoll` from the fall).
    pub landing_roll: bool,
}

impl Default for Phys {
    fn default() -> Self {
        Self { legacy: std::env::var_os("JUMPMAN_LINK_LEGACY_MOVE").is_some(), fall_start: 0.0, stun: 0.0, landing_roll: false }
    }
}

/// `action_arg` of `ACT_IDLE` while he staggers after a hard landing.
pub const ARG_STUN: f64 = 0x79 as f64;

/// `action_arg` of `ACT_WALKING` while he slides down a steep floor.
pub const ARG_SLIDE: f64 = 0x7a as f64;

/// One airborne step with OoT gravity: the velocity drops by `gravity` (floored at `minVelocityY`) before
/// the move, as `Actor_UpdateVelocityXZGravity` does, and SM64's own gravity after the step is undone.
pub fn air_step(m: &mut MarioRuntime, locked: bool) -> i32 {
    use crate::state::*;
    if m.link.phys.legacy {
        return m.step_air(AIR_STEP_CHECK_LEDGE_GRAB);
    }
    m.vel[1] = (m.vel[1] - gravity(locked)).max(TERMINAL);
    let vy = m.vel[1];
    let hit = m.step_air(AIR_STEP_CHECK_LEDGE_GRAB);
    if hit != AIR_STEP_LANDED as i32 {
        m.vel[1] = vy;
    }
    hit
}
