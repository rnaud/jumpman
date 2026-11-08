//! Port of the decompilation's `src/game/mario_actions_submerged.c` (all 32 submerged actions),
//! plus the pieces of `mario.c` / `interaction.c` / `mario_actions_airborne.c` that belong to
//! swimming: `set_water_plunge_action`, `transition_submerged_to_walking`, `act_water_jump`,
//! `act_hold_water_jump`, `interact_whirlpool`, the water branch of `determine_knockback_action`
//! and the shock branch of `interact_shock`.
//!
//! This replaces the TS-derived approximation in `submerged.rs` (kept only because the stationary
//! golden test pins it): the runtime now runs these actions through the normal action loop, so a
//! transition executes the new action in the same frame like `execute_mario_action`.
//!
//! Source constants are used as written. The C uses `f32`; this sim uses `f64` with the sine/atan
//! tables of `math.rs`, so truncations that land exactly on an integer can differ in rare frames.
//!
//! Animation-dependent branches (`is_anim_at_end`, the frame returned by `set_mario_animation`)
//! use the header words of the real animations through [`crate::mario_anim::AnimClock`].
use crate::collision::Whirlpool;
use crate::mario_anim::*;
use crate::mario_helpers::{find_floor_slope, mario_set_forward_vel};
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::{stop_and_set_height_to_floor, AIR_CHECK_LEDGE_GRAB, AIR_HIT_LAVA_WALL, AIR_HIT_WALL, AIR_LANDED, GROUND_STEP_HIT_WALL_I, GROUND_STEP_LEFT_GROUND_I};
use crate::math::*;
use crate::object::*;
use crate::state::*;

use std::sync::atomic::{AtomicBool, Ordering};

static FAITHFUL: AtomicBool = AtomicBool::new(false);

/// Whether Mario swims with these decompilation ports. The default is **off**: the TypeScript
/// goldens (`longrun`, `focus`, `game`, `warps` fixtures, `stationary` golden) pin the old
/// approximation in `submerged.rs` / the runtime's metal-water stub, and the generator that made
/// them is gone, so they can only keep passing unchanged while that path stays reachable. The
/// game turns the faithful path on at startup (`jumpman_game::run`); tests of this module call
/// [`set_faithful`]. Course loading also keys off it (the underwater shell and the whirlpool
/// objects are only placed when it is on, because the fixtures count objects).
pub fn faithful() -> bool {
    static ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    // `JUMPMAN_FAITHFUL_WATER=1` runs any binary or test with the port on (to see what the goldens say).
    FAITHFUL.load(Ordering::Relaxed) || *ENV.get_or_init(|| std::env::var_os("JUMPMAN_FAITHFUL_WATER").is_some())
}

pub fn set_faithful(on: bool) { FAITHFUL.store(on, Ordering::Relaxed); }

pub const MIN_SWIM_STRENGTH: f64 = 160.0;
pub const MIN_SWIM_SPEED: f64 = 16.0;
/// `sWaterCurrentSpeeds`.
pub const WATER_CURRENT_SPEEDS: [f64; 4] = [28.0, 12.0, 8.0, 4.0];

pub const WATER_STEP_NONE: i32 = 0;
pub const WATER_STEP_HIT_FLOOR: i32 = 1;
pub const WATER_STEP_HIT_CEILING: i32 = 2;
pub const WATER_STEP_CANCELLED: i32 = 3;
pub const WATER_STEP_HIT_WALL: i32 = 4;

/// `m->waterLevel` when there is no water box (`find_water_level` returns `FLOOR_LOWER_LIMIT`).
pub const NO_WATER_LEVEL: f64 = -11000.0;

/// Everything the submerged code keeps outside `MarioState`: the file-static variables
/// (`sWasAtSurface`, `sBobTimer`, `sBobIncrement`, `sBobHeight`), `m->waterLevel`, the animation
/// clock and the pieces of `marioObj` / `marioBodyState` the actions touch.
#[derive(Clone, Debug)]
pub struct WaterState {
    pub clock: AnimClock,
    /// `m->waterLevel`, refreshed in `update_mario_geometry_inputs`.
    pub level: f64,
    pub was_at_surface: bool,
    pub bob_timer: i32,
    pub bob_increment: i32,
    pub bob_height: f64,
    /// `marioBodyState->headAngle[0]` (visual).
    pub head_angle0: f64,
    /// `marioObj->header.gfx.pos` / `.angle`: the graphics copy `perform_water_step` writes and
    /// `update_water_pitch` / `surface_swim_bob` adjust. Renderers use these for submerged actions.
    pub gfx_pos: Vec3,
    pub gfx_angle: [f64; 3],
    /// `marioObj->oMarioWhirlpoolPosY`.
    pub whirlpool_pos_y: f64,
    /// `marioObj->oInteractStatus & INT_STATUS_MARIO_DROP_OBJECT` (an object that wants to be
    /// let go of, e.g. an exploding Bob-omb). Cleared at the end of every frame.
    pub drop_object: bool,
}

impl Default for WaterState {
    fn default() -> Self {
        Self {
            clock: AnimClock::default(),
            level: NO_WATER_LEVEL,
            was_at_surface: false,
            bob_timer: 0,
            bob_increment: 0,
            bob_height: 0.0,
            head_angle0: 0.0,
            gfx_pos: [0.0; 3],
            gfx_angle: [0.0; 3],
            whirlpool_pos_y: 0.0,
            drop_object: false,
        }
    }
}

/// Actions during which an object may stay in Mario's hands (the `HOLDING_ACTIONS` list of
/// `mario_runtime.rs` for the water set). `set_action` drops the held object otherwise.
pub fn keeps_held_object(action: u32) -> bool {
    matches!(
        action,
        ACT_HOLD_WATER_IDLE | ACT_HOLD_WATER_ACTION_END | ACT_HOLD_BREASTSTROKE | ACT_HOLD_SWIMMING_END | ACT_HOLD_FLUTTER_KICK
            | ACT_WATER_SHELL_SWIMMING | ACT_WATER_THROW | ACT_WATER_PLUNGE | ACT_HOLD_WATER_JUMP
            | ACT_HOLD_METAL_WATER_STANDING | ACT_HOLD_METAL_WATER_WALKING | ACT_HOLD_METAL_WATER_FALLING
            | ACT_HOLD_METAL_WATER_FALL_LAND | ACT_HOLD_METAL_WATER_JUMP | ACT_HOLD_METAL_WATER_JUMP_LAND
    )
}

/// True for the actions whose `check_common_*_cancels` runs `set_water_plunge_action` when Mario is
/// below `waterLevel - 100` (stationary, moving, airborne, automatic, object groups and the few
/// cutscene actions that ask for it).
pub fn plunges_into_water(action: u32) -> bool {
    match action & ACT_GROUP_MASK {
        ACT_GROUP_SUBMERGED => false,
        ACT_GROUP_CUTSCENE => matches!(action, ACT_SPAWN_SPIN_AIRBORNE | ACT_SPAWN_NO_SPIN_AIRBORNE | ACT_TELEPORT_FADE_IN),
        _ => true,
    }
}

fn stop_shell(m: &mut MarioRuntime) {
    if let Some(id) = m.held_obj.take() {
        m.objects[id].interact_status = INT_STATUS_STOP_RIDING;
    }
}

fn drop_requested(m: &MarioRuntime) -> bool { m.water.drop_object || m.held_obj.is_none() }

/// `drop_and_set_mario_action`.
fn drop_and_set(m: &mut MarioRuntime, action: u32, arg: f64) -> bool {
    m.drop_held_object();
    m.set_action(action, arg)
}

fn metal_cap(m: &MarioRuntime) -> bool { m.flags & MARIO_METAL_CAP != 0 }

// -----------------------------------------------------------------------------------------------
// Helpers (mario_actions_submerged.c, top to bottom)
// ---------------------------------------------------------------------------------------------

/// decomp: `set_swimming_at_surface_particles`.
fn set_swimming_at_surface_particles(m: &mut MarioRuntime, particle_flag: u32) {
    let at_surface = m.pos[1] >= m.water.level - 130.0;
    if at_surface {
        m.state.particle_flags |= particle_flag;
    }
    m.water.was_at_surface = at_surface;
}

/// decomp: `swimming_near_surface`.
pub fn swimming_near_surface(m: &MarioRuntime) -> bool {
    if metal_cap(m) { return false; }
    (m.water.level - 80.0) - m.pos[1] < 400.0
}

/// decomp: `get_buoyancy`.
pub fn get_buoyancy(m: &MarioRuntime) -> f64 {
    if metal_cap(m) {
        if m.action & ACT_FLAG_INVULNERABLE != 0 { -2.0 } else { -18.0 }
    } else if swimming_near_surface(m) {
        1.25
    } else if m.action & ACT_FLAG_MOVING == 0 {
        -2.0
    } else {
        0.0
    }
}

/// decomp: `perform_water_full_step`.
fn perform_water_full_step(m: &mut MarioRuntime, next: &mut Vec3) -> i32 {
    let world = m.world.clone();
    let wall = world.resolve_and_return_wall_collisions(next, 10.0, 110.0);
    let floor = world.find_floor(next[0], next[1], next[2]);
    let ceil_height = world.vec3f_find_ceil(next, floor.height).height;
    let Some(surface) = floor.surface else { return WATER_STEP_CANCELLED };
    let floor_height = floor.height;

    if next[1] >= floor_height {
        if ceil_height - next[1] >= 160.0 {
            m.state.pos = *next;
            m.state.floor = Some(surface);
            m.state.floor_height = floor_height;
            return if wall.is_some() { WATER_STEP_HIT_WALL } else { WATER_STEP_NONE };
        }
        if ceil_height - floor_height < 160.0 { return WATER_STEP_CANCELLED; }
        // Water ceiling downwarp (a source bug kept on purpose).
        m.state.pos = [next[0], ceil_height - 160.0, next[2]];
        m.state.floor = Some(surface);
        m.state.floor_height = floor_height;
        WATER_STEP_HIT_CEILING
    } else {
        if ceil_height - floor_height < 160.0 { return WATER_STEP_CANCELLED; }
        m.state.pos = [next[0], floor_height, next[2]];
        m.state.floor = Some(surface);
        m.state.floor_height = floor_height;
        WATER_STEP_HIT_FLOOR
    }
}

/// decomp: `apply_water_current` (flowing water floors and the area's whirlpools).
fn apply_water_current(m: &MarioRuntime, step: &mut Vec3) {
    if let Some(floor) = &m.floor {
        if floor.kind == SURFACE_FLOWING_WATER {
            let current_angle = to_s16(((floor.force as i64) << 8) as f64) as f64;
            let index = ((floor.force >> 8).max(0) as usize).min(3);
            let current_speed = WATER_CURRENT_SPEEDS[index];
            step[0] += current_speed * sins(current_angle);
            step[2] += current_speed * coss(current_angle);
        }
    }

    for whirlpool in m.world.whirlpools.iter().take(2) {
        let mut strength = 0.0;
        let dx = whirlpool.pos[0] - m.pos[0];
        let dy = whirlpool.pos[1] - m.pos[1];
        let dz = whirlpool.pos[2] - m.pos[2];
        let lateral_dist = (dx * dx + dz * dz).sqrt();
        let distance = (lateral_dist * lateral_dist + dy * dy).sqrt();
        let pitch_to_whirlpool = atan2s(lateral_dist, dy) as f64;
        let mut yaw_to_whirlpool = atan2s(dz, dx) as f64;
        yaw_to_whirlpool = to_s16(yaw_to_whirlpool - to_s16(0x2000 as f64 * 1000.0 / (distance + 1000.0)) as f64) as f64;

        if whirlpool.strength >= 0.0 {
            if distance >= 26.0 && distance < whirlpool.radius {
                strength = whirlpool.strength * (1.0 - distance / whirlpool.radius);
            }
        } else if distance < 2000.0 {
            strength = whirlpool.strength * (1.0 - distance / 2000.0);
        }

        step[0] += strength * coss(pitch_to_whirlpool) * sins(yaw_to_whirlpool);
        step[1] += strength * sins(pitch_to_whirlpool);
        step[2] += strength * coss(pitch_to_whirlpool) * coss(yaw_to_whirlpool);
    }
}

/// decomp: `perform_water_step`.
pub fn perform_water_step(m: &mut MarioRuntime) -> i32 {
    let mut step = m.vel;
    if m.action & ACT_FLAG_SWIMMING != 0 {
        apply_water_current(m, &mut step);
    }

    let mut next = [m.pos[0] + step[0], m.pos[1] + step[1], m.pos[2] + step[2]];
    if next[1] > m.water.level - 80.0 {
        next[1] = m.water.level - 80.0;
        m.state.vel[1] = 0.0;
    }

    let result = perform_water_full_step(m, &mut next);

    m.water.gfx_pos = m.pos;
    m.water.gfx_angle = [-m.face_angle[0], m.face_angle[1], m.face_angle[2]];
    result
}

/// decomp: `update_water_pitch` (graphics pitch and the dip of the model when diving).
fn update_water_pitch(m: &mut MarioRuntime) {
    let a = m.water.gfx_angle[0];
    if a > 0.0 {
        m.water.gfx_pos[1] += 60.0 * sins(a) * sins(a);
    }
    if m.water.gfx_angle[0] < 0.0 {
        m.water.gfx_angle[0] = (m.water.gfx_angle[0] * 6.0 / 10.0).trunc();
    }
    if m.water.gfx_angle[0] > 0.0 {
        m.water.gfx_angle[0] = (m.water.gfx_angle[0] * 10.0 / 8.0).trunc();
    }
}

/// decomp: `stationary_slow_down`.
pub fn stationary_slow_down(m: &mut MarioRuntime) {
    let buoyancy = get_buoyancy(m);
    m.state.angle_vel[0] = 0.0;
    m.state.angle_vel[1] = 0.0;

    m.state.forward_vel = approach_f32(m.forward_vel, 0.0, 1.0, 1.0);
    m.state.vel[1] = approach_f32(m.vel[1], buoyancy, 2.0, 1.0);

    m.state.face_angle[0] = approach_s32(m.face_angle[0], 0.0, 0x200 as f64, 0x200 as f64);
    m.state.face_angle[2] = approach_s32(m.face_angle[2], 0.0, 0x100 as f64, 0x100 as f64);

    m.state.vel[0] = m.forward_vel * coss(m.face_angle[0]) * sins(m.face_angle[1]);
    m.state.vel[2] = m.forward_vel * coss(m.face_angle[0]) * coss(m.face_angle[1]);
}

/// decomp: `update_swimming_speed`.
fn update_swimming_speed(m: &mut MarioRuntime, decel_threshold: f64) {
    let buoyancy = get_buoyancy(m);
    let max_speed = 28.0;

    if m.action & ACT_FLAG_STATIONARY != 0 {
        m.state.forward_vel -= 2.0;
    }
    if m.forward_vel < 0.0 { m.state.forward_vel = 0.0; }
    if m.forward_vel > max_speed { m.state.forward_vel = max_speed; }
    if m.forward_vel > decel_threshold { m.state.forward_vel -= 0.5; }

    m.state.vel[0] = m.forward_vel * coss(m.face_angle[0]) * sins(m.face_angle[1]);
    m.state.vel[1] = m.forward_vel * sins(m.face_angle[0]) + buoyancy;
    m.state.vel[2] = m.forward_vel * coss(m.face_angle[0]) * coss(m.face_angle[1]);
}

/// decomp: `update_swimming_yaw`.
fn update_swimming_yaw(m: &mut MarioRuntime) {
    let target_yaw_vel = -((10.0 * m.controller.stick_x).trunc());

    if target_yaw_vel > 0.0 {
        if m.angle_vel[1] < 0.0 {
            m.state.angle_vel[1] += 0x40 as f64;
            if m.angle_vel[1] > 0x10 as f64 { m.state.angle_vel[1] = 0x10 as f64; }
        } else {
            m.state.angle_vel[1] = approach_s32(m.angle_vel[1], target_yaw_vel, 0x10 as f64, 0x20 as f64);
        }
    } else if target_yaw_vel < 0.0 {
        if m.angle_vel[1] > 0.0 {
            m.state.angle_vel[1] -= 0x40 as f64;
            if m.angle_vel[1] < -0x10 as f64 { m.state.angle_vel[1] = -0x10 as f64; }
        } else {
            m.state.angle_vel[1] = approach_s32(m.angle_vel[1], target_yaw_vel, 0x20 as f64, 0x10 as f64);
        }
    } else {
        m.state.angle_vel[1] = approach_s32(m.angle_vel[1], 0.0, 0x40 as f64, 0x40 as f64);
    }

    m.state.face_angle[1] = to_s16(m.face_angle[1] + m.angle_vel[1]) as f64;
    m.state.face_angle[2] = to_s16(-m.angle_vel[1] * 8.0) as f64;
}

/// decomp: `update_swimming_pitch`.
fn update_swimming_pitch(m: &mut MarioRuntime) {
    let target_pitch = -((252.0 * m.controller.stick_y).trunc());
    let pitch_vel = if m.face_angle[0] < 0.0 { 0x100 } else { 0x200 } as f64;

    if m.face_angle[0] < target_pitch {
        m.state.face_angle[0] += pitch_vel;
        if m.face_angle[0] > target_pitch { m.state.face_angle[0] = target_pitch; }
    } else if m.face_angle[0] > target_pitch {
        m.state.face_angle[0] -= pitch_vel;
        if m.face_angle[0] < target_pitch { m.state.face_angle[0] = target_pitch; }
    }
}

/// decomp: `common_idle_step`.
fn common_idle_step(m: &mut MarioRuntime, animation: i32, arg: i32) {
    update_swimming_yaw(m);
    update_swimming_pitch(m);
    update_swimming_speed(m, MIN_SWIM_SPEED);
    perform_water_step(m);
    update_water_pitch(m);

    if m.face_angle[0] > 0.0 {
        m.water.head_angle0 = approach_s32(m.water.head_angle0, (m.face_angle[0] / 2.0).trunc(), 0x80 as f64, 0x200 as f64);
    } else {
        m.water.head_angle0 = approach_s32(m.water.head_angle0, 0.0, 0x200 as f64, 0x200 as f64);
    }

    if arg == 0 {
        m.water.clock.set(animation);
    } else {
        m.water.clock.set_with_accel(animation, arg);
    }
    set_swimming_at_surface_particles(m, PARTICLE_IDLE_WATER_WAVE);
}

fn act_water_idle(m: &mut MarioRuntime) -> bool {
    let mut val = 0x10000;
    if metal_cap(m) { return m.set_action(ACT_METAL_WATER_FALLING, 1.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_PUNCH, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_BREASTSTROKE, 0.0); }
    if m.face_angle[0] < -0x1000 as f64 { val = 0x30000; }
    common_idle_step(m, MARIO_ANIM_WATER_IDLE, val);
    false
}

fn act_hold_water_idle(m: &mut MarioRuntime) -> bool {
    if metal_cap(m) { return m.set_action(ACT_HOLD_METAL_WATER_FALLING, 0.0); }
    if drop_requested(m) { return drop_and_set(m, ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_THROW, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_HOLD_BREASTSTROKE, 0.0); }
    common_idle_step(m, MARIO_ANIM_WATER_IDLE_WITH_OBJ, 0);
    false
}

fn act_water_action_end(m: &mut MarioRuntime) -> bool {
    if metal_cap(m) { return m.set_action(ACT_METAL_WATER_FALLING, 1.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_PUNCH, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_BREASTSTROKE, 0.0); }
    common_idle_step(m, MARIO_ANIM_WATER_ACTION_END, 0);
    if m.water.clock.is_at_end() { m.set_action(ACT_WATER_IDLE, 0.0); }
    false
}

fn act_hold_water_action_end(m: &mut MarioRuntime) -> bool {
    if metal_cap(m) { return m.set_action(ACT_HOLD_METAL_WATER_FALLING, 0.0); }
    if drop_requested(m) { return drop_and_set(m, ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_THROW, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_HOLD_BREASTSTROKE, 0.0); }
    let anim = if m.action_arg == 0.0 { MARIO_ANIM_WATER_ACTION_END_WITH_OBJ } else { MARIO_ANIM_STOP_GRAB_OBJ_WATER };
    common_idle_step(m, anim, 0);
    if m.water.clock.is_at_end() { m.set_action(ACT_HOLD_WATER_IDLE, 0.0); }
    false
}

/// decomp: `reset_bob_variables`.
fn reset_bob_variables(m: &mut MarioRuntime) {
    m.water.bob_timer = 0;
    m.water.bob_increment = 0x800;
    m.water.bob_height = (m.face_angle[0] / 256.0).trunc() + 20.0;
}

/// decomp: `surface_swim_bob` (controls the bobbing near the surface; graphics only).
fn surface_swim_bob(m: &mut MarioRuntime) {
    if m.water.bob_increment != 0 && m.pos[1] > m.water.level - 85.0 && m.face_angle[0] >= 0.0 {
        m.water.bob_timer = to_s16((m.water.bob_timer + m.water.bob_increment) as f64);
        if m.water.bob_timer >= 0 {
            m.water.gfx_pos[1] += m.water.bob_height * sins(m.water.bob_timer as f64);
            return;
        }
    }
    m.water.bob_increment = 0;
}

/// decomp: `common_swimming_step`.
fn common_swimming_step(m: &mut MarioRuntime, swim_strength: f64) {
    update_swimming_yaw(m);
    update_swimming_pitch(m);
    update_swimming_speed(m, swim_strength / 10.0);

    match perform_water_step(m) {
        WATER_STEP_HIT_FLOOR => {
            let world = m.world.clone();
            let floor_pitch = -(find_floor_slope(&m.state, &world, -0x8000 as f64) as f64);
            if m.face_angle[0] < floor_pitch { m.state.face_angle[0] = floor_pitch; }
        }
        WATER_STEP_HIT_CEILING => {
            if m.face_angle[0] > -0x3000 as f64 { m.state.face_angle[0] -= 0x100 as f64; }
        }
        WATER_STEP_HIT_WALL => {
            if m.controller.stick_y == 0.0 {
                if m.face_angle[0] > 0.0 {
                    m.state.face_angle[0] += 0x200 as f64;
                    if m.face_angle[0] > 0x3F00 as f64 { m.state.face_angle[0] = 0x3F00 as f64; }
                } else {
                    m.state.face_angle[0] -= 0x200 as f64;
                    if m.face_angle[0] < -0x3F00 as f64 { m.state.face_angle[0] = -0x3F00 as f64; }
                }
            }
        }
        _ => {}
    }

    update_water_pitch(m);
    m.water.head_angle0 = approach_s32(m.water.head_angle0, 0.0, 0x200 as f64, 0x200 as f64);
    surface_swim_bob(m);
    set_swimming_at_surface_particles(m, PARTICLE_WAVE_TRAIL);
}

/// decomp: `play_swimming_noise`.
fn play_swimming_noise(m: &mut MarioRuntime) {
    let frame = m.water.clock.frame;
    if frame == 0 || frame == 12 { m.events.push(MarioEvent::FlutterKick); }
}

/// decomp: `check_water_jump`.
fn check_water_jump(m: &mut MarioRuntime) -> bool {
    let probe = (m.pos[1] + 1.5).trunc();
    if m.input & INPUT_A_PRESSED != 0 && probe >= m.water.level - 80.0 && m.face_angle[0] >= 0.0 && m.controller.stick_y < -60.0 {
        m.state.angle_vel = [0.0; 3];
        m.state.vel[1] = 62.0;
        return if m.held_obj.is_none() { m.set_action(ACT_WATER_JUMP, 0.0) } else { m.set_action(ACT_HOLD_WATER_JUMP, 0.0) };
    }
    false
}

fn act_breaststroke(m: &mut MarioRuntime) -> bool {
    if m.action_arg == 0.0 { m.swim_strength = MIN_SWIM_STRENGTH; }
    if metal_cap(m) { return m.set_action(ACT_METAL_WATER_FALLING, 1.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_PUNCH, 0.0); }

    m.state.action_timer += 1.0;
    if m.action_timer == 14.0 { return m.set_action(ACT_FLUTTER_KICK, 0.0); }
    if check_water_jump(m) { return true; }

    if m.action_timer < 6.0 { m.state.forward_vel += 0.5; }
    if m.action_timer >= 9.0 { m.state.forward_vel += 1.5; }

    if m.action_timer >= 2.0 {
        if m.action_timer < 6.0 && m.input & INPUT_A_PRESSED != 0 { m.state.action_state = 1.0; }
        if m.action_timer == 9.0 && m.action_state == 1.0 {
            m.water.clock.set_to_frame(0);
            m.state.action_state = 0.0;
            m.state.action_timer = 1.0;
            m.swim_strength = MIN_SWIM_STRENGTH;
        }
    }

    if m.action_timer == 1.0 {
        let fast = m.swim_strength != MIN_SWIM_STRENGTH;
        m.events.push(MarioEvent::SwimStroke { fast });
        reset_bob_variables(m);
    }

    m.water.clock.set(MARIO_ANIM_SWIM_PART1);
    let strength = m.swim_strength;
    common_swimming_step(m, strength);
    false
}

fn act_swimming_end(m: &mut MarioRuntime) -> bool {
    if metal_cap(m) { return m.set_action(ACT_METAL_WATER_FALLING, 1.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_PUNCH, 0.0); }
    if m.action_timer >= 15.0 { return m.set_action(ACT_WATER_ACTION_END, 0.0); }
    if check_water_jump(m) { return true; }

    if m.input & INPUT_A_DOWN != 0 && m.action_timer >= 7.0 {
        if m.action_timer == 7.0 && m.swim_strength < 280.0 { m.swim_strength += 10.0; }
        return m.set_action(ACT_BREASTSTROKE, 1.0);
    }
    if m.action_timer >= 7.0 { m.swim_strength = MIN_SWIM_STRENGTH; }

    m.state.action_timer += 1.0;
    m.state.forward_vel -= 0.25;
    m.water.clock.set(MARIO_ANIM_SWIM_PART2);
    let strength = m.swim_strength;
    common_swimming_step(m, strength);
    false
}

fn act_flutter_kick(m: &mut MarioRuntime) -> bool {
    if metal_cap(m) { return m.set_action(ACT_METAL_WATER_FALLING, 1.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_PUNCH, 0.0); }

    if m.input & INPUT_A_DOWN == 0 {
        if m.action_timer == 0.0 && m.swim_strength < 280.0 { m.swim_strength += 10.0; }
        return m.set_action(ACT_SWIMMING_END, 0.0);
    }

    m.state.forward_vel = approach_f32(m.forward_vel, 12.0, 0.1, 0.15);
    m.state.action_timer = 1.0;
    m.swim_strength = MIN_SWIM_STRENGTH;

    if m.forward_vel < 14.0 {
        play_swimming_noise(m);
        m.water.clock.set(MARIO_ANIM_FLUTTERKICK);
    }
    let strength = m.swim_strength;
    common_swimming_step(m, strength);
    false
}

fn act_hold_breaststroke(m: &mut MarioRuntime) -> bool {
    if metal_cap(m) { return m.set_action(ACT_HOLD_METAL_WATER_FALLING, 0.0); }
    if drop_requested(m) { return drop_and_set(m, ACT_WATER_IDLE, 0.0); }

    m.state.action_timer += 1.0;
    if m.action_timer == 17.0 { return m.set_action(ACT_HOLD_FLUTTER_KICK, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_THROW, 0.0); }
    if check_water_jump(m) { return true; }

    if m.action_timer < 6.0 { m.state.forward_vel += 0.5; }
    if m.action_timer >= 9.0 { m.state.forward_vel += 1.5; }

    if m.action_timer >= 2.0 {
        if m.action_timer < 6.0 && m.input & INPUT_A_PRESSED != 0 { m.state.action_state = 1.0; }
        if m.action_timer == 9.0 && m.action_state == 1.0 {
            m.water.clock.set_to_frame(0);
            m.state.action_state = 0.0;
            m.state.action_timer = 1.0;
        }
    }

    if m.action_timer == 1.0 {
        m.events.push(MarioEvent::SwimStroke { fast: false });
        reset_bob_variables(m);
    }

    m.water.clock.set(MARIO_ANIM_SWIM_WITH_OBJ_PART1);
    common_swimming_step(m, 0x00A0 as f64);
    false
}

fn act_hold_swimming_end(m: &mut MarioRuntime) -> bool {
    if metal_cap(m) { return m.set_action(ACT_HOLD_METAL_WATER_FALLING, 0.0); }
    if drop_requested(m) { return drop_and_set(m, ACT_WATER_IDLE, 0.0); }
    if m.action_timer >= 15.0 { return m.set_action(ACT_HOLD_WATER_ACTION_END, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_THROW, 0.0); }
    if check_water_jump(m) { return true; }
    if m.input & INPUT_A_DOWN != 0 && m.action_timer >= 7.0 { return m.set_action(ACT_HOLD_BREASTSTROKE, 0.0); }

    m.state.action_timer += 1.0;
    m.state.forward_vel -= 0.25;
    m.water.clock.set(MARIO_ANIM_SWIM_WITH_OBJ_PART2);
    common_swimming_step(m, 0x00A0 as f64);
    false
}

fn act_hold_flutter_kick(m: &mut MarioRuntime) -> bool {
    if metal_cap(m) { return m.set_action(ACT_HOLD_METAL_WATER_FALLING, 0.0); }
    if drop_requested(m) { return drop_and_set(m, ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_THROW, 0.0); }
    if m.input & INPUT_A_DOWN == 0 { return m.set_action(ACT_HOLD_SWIMMING_END, 0.0); }

    m.state.forward_vel = approach_f32(m.forward_vel, 12.0, 0.1, 0.15);
    if m.forward_vel < 14.0 {
        play_swimming_noise(m);
        m.water.clock.set(MARIO_ANIM_FLUTTERKICK_WITH_OBJ);
    }
    common_swimming_step(m, 0x00A0 as f64);
    false
}

fn act_water_shell_swimming(m: &mut MarioRuntime) -> bool {
    if drop_requested(m) { return drop_and_set(m, ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_B_PRESSED != 0 { return m.set_action(ACT_WATER_THROW, 0.0); }

    let timer = m.action_timer;
    m.state.action_timer += 1.0;
    if timer == 240.0 {
        stop_shell(m);
        m.set_action(ACT_FLUTTER_KICK, 0.0);
    }

    m.state.forward_vel = approach_f32(m.forward_vel, 30.0, 2.0, 1.0);
    play_swimming_noise(m);
    m.water.clock.set(MARIO_ANIM_FLUTTERKICK_WITH_OBJ);
    common_swimming_step(m, 0x012C as f64);
    false
}

/// decomp: `check_water_grab`. `mario_get_collided_object(m, INTERACT_GRABBABLE)` is the first
/// object whose hitbox touches Mario's this frame; like the original this does not look at
/// `INPUT_INTERACT_OBJ_GRABBABLE` (so any grabbable object in front of Mario is picked up).
fn check_water_grab(m: &mut MarioRuntime) -> bool {
    let found = (0..m.objects.objects.len()).map(|i| ObjId(i as u32)).find(|id| {
        let o = &m.objects[*id];
        o.active && o.held_state != HELD_HELD && o.hitbox.interact_type & INTERACT_GRABBABLE != 0 && mario_overlaps_object(m, o)
    });
    let Some(id) = found else { return false };
    let o = &m.objects[id];
    let d_angle = to_s16(atan2s(o.pos[2] - m.pos[2], o.pos[0] - m.pos[0]) as f64 - m.face_angle[1]);
    if (-0x2AAA..=0x2AAA).contains(&d_angle) {
        m.used_obj = Some(id);
        m.grab_used_object();
        return true;
    }
    false
}

fn act_water_throw(m: &mut MarioRuntime) -> bool {
    update_swimming_yaw(m);
    update_swimming_pitch(m);
    update_swimming_speed(m, MIN_SWIM_SPEED);
    perform_water_step(m);
    update_water_pitch(m);

    m.water.clock.set(MARIO_ANIM_WATER_THROW_OBJ);
    m.state.flags |= MARIO_ACTION_SOUND_PLAYED;
    m.water.head_angle0 = approach_s32(m.water.head_angle0, 0.0, 0x200 as f64, 0x200 as f64);

    let timer = m.action_timer;
    m.state.action_timer += 1.0;
    if timer == 5.0 { m.throw_held_object(); }
    if m.water.clock.is_at_end() { m.set_action(ACT_WATER_IDLE, 0.0); }
    false
}

fn act_water_punch(m: &mut MarioRuntime) -> bool {
    if m.forward_vel < 7.0 { m.state.forward_vel += 1.0; }

    update_swimming_yaw(m);
    update_swimming_pitch(m);
    update_swimming_speed(m, MIN_SWIM_SPEED);
    perform_water_step(m);
    update_water_pitch(m);

    m.water.head_angle0 = approach_s32(m.water.head_angle0, 0.0, 0x200 as f64, 0x200 as f64);
    m.state.flags |= MARIO_ACTION_SOUND_PLAYED;

    match m.action_state as i32 {
        0 => {
            m.water.clock.set(MARIO_ANIM_WATER_GRAB_OBJ_PART1);
            if m.water.clock.is_at_end() {
                m.state.action_state = check_water_grab(m) as i32 as f64 + 1.0;
            }
        }
        1 => {
            m.water.clock.set(MARIO_ANIM_WATER_GRAB_OBJ_PART2);
            if m.water.clock.is_at_end() { m.set_action(ACT_WATER_ACTION_END, 0.0); }
        }
        2 => {
            m.water.clock.set(MARIO_ANIM_WATER_PICK_UP_OBJ);
            if m.water.clock.is_at_end() {
                let shell = m.held_obj.is_some_and(|id| m.objects[id].behavior == ObjectBehaviorKind::KoopaShellUnderwater);
                if shell {
                    m.set_action(ACT_WATER_SHELL_SWIMMING, 0.0);
                } else {
                    m.set_action(ACT_HOLD_WATER_ACTION_END, 1.0);
                }
            }
        }
        _ => {}
    }
    false
}

/// decomp: `common_water_knockback_step`.
fn common_water_knockback_step(m: &mut MarioRuntime, animation: i32, end_action: u32, arg3: f64) {
    stationary_slow_down(m);
    perform_water_step(m);
    m.water.clock.set(animation);
    m.water.head_angle0 = 0.0;

    if m.water.clock.is_at_end() {
        if arg3 > 0.0 { m.invinc_timer = 30.0; }
        let next = if m.health >= 0x100 as f64 { end_action } else { ACT_WATER_DEATH };
        m.set_action(next, 0.0);
    }
}

fn act_backward_water_kb(m: &mut MarioRuntime) -> bool {
    let arg = m.action_arg;
    common_water_knockback_step(m, MARIO_ANIM_BACKWARDS_WATER_KB, ACT_WATER_IDLE, arg);
    false
}

fn act_forward_water_kb(m: &mut MarioRuntime) -> bool {
    let arg = m.action_arg;
    common_water_knockback_step(m, MARIO_ANIM_WATER_FORWARD_KB, ACT_WATER_IDLE, arg);
    false
}

fn act_water_shocked(m: &mut MarioRuntime) -> bool {
    if m.water.clock.set(MARIO_ANIM_SHOCKED) == 0 {
        m.state.action_timer += 1.0;
        m.state.flags |= MARIO_METAL_SHOCK;
    }
    if m.action_timer >= 6.0 {
        m.invinc_timer = 30.0;
        let next = if m.health < 0x100 as f64 { ACT_WATER_DEATH } else { ACT_WATER_IDLE };
        m.set_action(next, 0.0);
    }
    stationary_slow_down(m);
    perform_water_step(m);
    m.water.head_angle0 = 0.0;
    false
}

fn act_drowning(m: &mut MarioRuntime) -> bool {
    match m.action_state as i32 {
        0 => {
            m.water.clock.set(MARIO_ANIM_DROWNING_PART1);
            if m.water.clock.is_at_end() { m.state.action_state = 1.0; }
        }
        1 => {
            m.water.clock.set(MARIO_ANIM_DROWNING_PART2);
            if m.water.clock.frame == 30 { m.respawn(false); }
        }
        _ => {}
    }
    stationary_slow_down(m);
    perform_water_step(m);
    false
}

fn act_water_death(m: &mut MarioRuntime) -> bool {
    stationary_slow_down(m);
    perform_water_step(m);
    // The source sets the animation twice and warps when the second call returns 35.
    m.water.clock.set(MARIO_ANIM_WATER_DYING);
    if m.water.clock.set(MARIO_ANIM_WATER_DYING) == 35 { m.respawn(false); }
    false
}

fn act_water_plunge(m: &mut MarioRuntime) -> bool {
    let mut state_flags = m.held_obj.is_some() as i32;
    let end_v_speed = if swimming_near_surface(m) { 0.0 } else { -5.0 };

    if metal_cap(m) {
        state_flags |= 4;
    } else if m.prev_action & ACT_FLAG_DIVING != 0 || m.input & INPUT_A_DOWN != 0 {
        state_flags |= 2;
    }

    m.state.action_timer += 1.0;
    stationary_slow_down(m);
    let step_result = perform_water_step(m);

    if m.action_state == 0.0 {
        m.events.push(MarioEvent::WaterPlunge);
        m.state.particle_flags |= PARTICLE_WATER_SPLASH;
        m.state.action_state = 1.0;
    }

    if step_result == WATER_STEP_HIT_FLOOR || m.vel[1] >= end_v_speed || m.action_timer > 20.0 {
        let next = match state_flags {
            0 => ACT_WATER_ACTION_END,
            1 => ACT_HOLD_WATER_ACTION_END,
            2 => ACT_FLUTTER_KICK,
            3 => ACT_HOLD_FLUTTER_KICK,
            4 => ACT_METAL_WATER_FALLING,
            _ => ACT_HOLD_METAL_WATER_FALLING,
        };
        m.set_action(next, 0.0);
        m.water.bob_increment = 0;
    }

    m.water.clock.set(match state_flags {
        0 => MARIO_ANIM_WATER_ACTION_END,
        1 => MARIO_ANIM_WATER_ACTION_END_WITH_OBJ,
        2 => MARIO_ANIM_FLUTTERKICK,
        3 => MARIO_ANIM_FLUTTERKICK_WITH_OBJ,
        4 => MARIO_ANIM_GENERAL_FALL,
        _ => MARIO_ANIM_FALL_WITH_LIGHT_OBJ,
    });
    m.state.particle_flags |= PARTICLE_PLUNGE_BUBBLE;
    false
}

fn act_caught_in_whirlpool(m: &mut MarioRuntime) -> bool {
    let Some(whirlpool) = m.used_obj else { return m.set_action(ACT_WATER_IDLE, 0.0) };
    let wpos = m.objects[whirlpool].pos;

    let mut dx = m.pos[0] - wpos[0];
    let mut dz = m.pos[2] - wpos[2];
    let distance = (dx * dx + dz * dz).sqrt();

    m.water.whirlpool_pos_y += m.vel[1];
    if m.water.whirlpool_pos_y < 0.0 {
        m.water.whirlpool_pos_y = 0.0;
        if distance < 16.1 {
            let timer = m.action_timer;
            m.state.action_timer += 1.0;
            if timer == 16.0 { m.respawn(false); }
        }
    }

    let (new_distance, angle_change);
    if distance <= 28.0 {
        new_distance = 16.0;
        angle_change = 0x1800 as f64;
    } else if distance < 256.0 {
        new_distance = distance - (12.0 - distance / 32.0);
        angle_change = to_s16(0x1C00 as f64 - distance * 20.0) as f64;
    } else {
        new_distance = distance - 4.0;
        angle_change = 0x800 as f64;
    }

    m.state.vel[1] = -640.0 / (new_distance + 16.0);

    let sin_angle_change = sins(angle_change);
    let cos_angle_change = coss(angle_change);

    if distance < 1.0 {
        dx = new_distance * sins(m.face_angle[1]);
        dz = new_distance * coss(m.face_angle[1]);
    } else {
        dx *= new_distance / distance;
        dz *= new_distance / distance;
    }

    m.state.pos[0] = wpos[0] + dx * cos_angle_change + dz * sin_angle_change;
    m.state.pos[2] = wpos[2] - dx * sin_angle_change + dz * cos_angle_change;
    m.state.pos[1] = wpos[1] + m.water.whirlpool_pos_y;

    m.state.face_angle[1] = to_s16(atan2s(dz, dx) as f64 + 0x8000 as f64) as f64;

    m.water.clock.set(MARIO_ANIM_GENERAL_FALL);
    m.water.gfx_pos = m.pos;
    m.water.gfx_angle = [0.0, m.face_angle[1], 0.0];
    false
}

// --- metal cap ----------------------------------------------------------------------------------

/// decomp: `play_metal_water_jumping_sound` (mist circle particles once).
fn play_metal_water_jumping_sound(m: &mut MarioRuntime) {
    if m.flags & MARIO_ACTION_SOUND_PLAYED == 0 { m.state.particle_flags |= PARTICLE_MIST_CIRCLE; }
    m.state.flags |= MARIO_ACTION_SOUND_PLAYED;
}

/// decomp: `play_metal_water_walking_sound`.
fn play_metal_water_walking_sound(m: &mut MarioRuntime) {
    if m.water.clock.is_past_frame(10) || m.water.clock.is_past_frame(49) {
        m.state.particle_flags |= PARTICLE_DUST;
    }
}

/// decomp: `update_metal_water_walking_speed`.
fn update_metal_water_walking_speed(m: &mut MarioRuntime) {
    let val = m.intended_mag / 1.5;

    if m.forward_vel <= 0.0 {
        m.state.forward_vel += 1.1;
    } else if m.forward_vel <= val {
        m.state.forward_vel += 1.1 - m.forward_vel / 43.0;
    } else if m.floor.as_ref().is_some_and(|f| f.normal[1] >= 0.95) {
        m.state.forward_vel -= 1.0;
    }
    if m.forward_vel > 32.0 { m.state.forward_vel = 32.0; }

    let d = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
    m.state.face_angle[1] = to_s16(m.intended_yaw - approach_s32(d, 0.0, 0x800 as f64, 0x800 as f64)) as f64;

    m.state.slide_vel_x = m.forward_vel * sins(m.face_angle[1]);
    m.state.slide_vel_z = m.forward_vel * coss(m.face_angle[1]);
    m.state.vel[0] = m.slide_vel_x;
    m.state.vel[1] = 0.0;
    m.state.vel[2] = m.slide_vel_z;
}

/// decomp: `update_metal_water_jump_speed`. True once Mario rises above the surface.
fn update_metal_water_jump_speed(m: &mut MarioRuntime) -> bool {
    let water_surface = m.water.level - 100.0;
    if m.vel[1] > 0.0 && m.pos[1] > water_surface { return true; }

    if m.input & INPUT_NONZERO_ANALOG != 0 {
        let intended_d_yaw = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
        m.state.forward_vel += 0.8 * coss(intended_d_yaw);
        m.state.face_angle[1] = to_s16(m.face_angle[1] + 0x200 as f64 * sins(intended_d_yaw)) as f64;
    } else {
        m.state.forward_vel = approach_f32(m.forward_vel, 0.0, 0.25, 0.25);
    }
    if m.forward_vel > 16.0 { m.state.forward_vel -= 1.0; }
    if m.forward_vel < 0.0 { m.state.forward_vel += 2.0; }

    m.state.slide_vel_x = m.forward_vel * sins(m.face_angle[1]);
    m.state.slide_vel_z = m.forward_vel * coss(m.face_angle[1]);
    m.state.vel[0] = m.slide_vel_x;
    m.state.vel[2] = m.slide_vel_z;
    false
}

fn act_metal_water_standing(m: &mut MarioRuntime) -> bool {
    if !metal_cap(m) { return m.set_action(ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_METAL_WATER_JUMP, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_METAL_WATER_WALKING, 0.0); }

    match m.action_state as i32 {
        0 => { m.water.clock.set(MARIO_ANIM_IDLE_HEAD_LEFT); }
        1 => { m.water.clock.set(MARIO_ANIM_IDLE_HEAD_RIGHT); }
        2 => { m.water.clock.set(MARIO_ANIM_IDLE_HEAD_CENTER); }
        _ => {}
    }
    if m.water.clock.is_at_end() {
        m.state.action_state += 1.0;
        if m.action_state == 3.0 { m.state.action_state = 0.0; }
    }

    stop_and_set_height_to_floor(&mut m.state);
    if m.pos[1] >= m.water.level - 150.0 { m.state.particle_flags |= PARTICLE_IDLE_WATER_WAVE; }
    false
}

fn act_hold_metal_water_standing(m: &mut MarioRuntime) -> bool {
    if drop_requested(m) { return drop_and_set(m, ACT_METAL_WATER_STANDING, 0.0); }
    if !metal_cap(m) { return m.set_action(ACT_HOLD_WATER_IDLE, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_HOLD_METAL_WATER_JUMP, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_HOLD_METAL_WATER_WALKING, 0.0); }

    stop_and_set_height_to_floor(&mut m.state);
    m.water.clock.set(MARIO_ANIM_IDLE_WITH_LIGHT_OBJ);
    false
}

fn act_metal_water_walking(m: &mut MarioRuntime) -> bool {
    if !metal_cap(m) { return m.set_action(ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_FIRST_PERSON != 0 { return m.set_action(ACT_METAL_WATER_STANDING, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_METAL_WATER_JUMP, 0.0); }
    if m.input & INPUT_UNKNOWN_5 != 0 { return m.set_action(ACT_METAL_WATER_STANDING, 0.0); }

    let mut val04 = (m.forward_vel / 4.0 * 0x10000 as f64).trunc() as i32;
    if val04 < 0x1000 { val04 = 0x1000; }

    m.water.clock.set_with_accel(MARIO_ANIM_WALKING, val04);
    play_metal_water_walking_sound(m);
    update_metal_water_walking_speed(m);

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => { m.set_action(ACT_METAL_WATER_FALLING, 1.0); }
        GROUND_STEP_HIT_WALL_I => m.state.forward_vel = 0.0,
        _ => {}
    }
    false
}

fn act_hold_metal_water_walking(m: &mut MarioRuntime) -> bool {
    if drop_requested(m) { return drop_and_set(m, ACT_METAL_WATER_WALKING, 0.0); }
    if !metal_cap(m) { return m.set_action(ACT_HOLD_WATER_IDLE, 0.0); }
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_HOLD_METAL_WATER_JUMP, 0.0); }
    if m.input & INPUT_UNKNOWN_5 != 0 { return m.set_action(ACT_HOLD_METAL_WATER_STANDING, 0.0); }

    m.state.intended_mag *= 0.4;

    let mut val04 = (m.forward_vel / 2.0 * 0x10000 as f64).trunc() as i32;
    if val04 < 0x1000 { val04 = 0x1000; }

    m.water.clock.set_with_accel(MARIO_ANIM_RUN_WITH_LIGHT_OBJ, val04);
    play_metal_water_walking_sound(m);
    update_metal_water_walking_speed(m);

    match m.step_ground() {
        GROUND_STEP_LEFT_GROUND_I => { m.set_action(ACT_HOLD_METAL_WATER_FALLING, 1.0); }
        GROUND_STEP_HIT_WALL_I => m.state.forward_vel = 0.0,
        _ => {}
    }
    false
}

fn act_metal_water_jump(m: &mut MarioRuntime) -> bool {
    if !metal_cap(m) { return m.set_action(ACT_WATER_IDLE, 0.0); }
    if update_metal_water_jump_speed(m) { return m.set_action(ACT_WATER_JUMP, 1.0); }

    play_metal_water_jumping_sound(m);
    m.water.clock.set(MARIO_ANIM_SINGLE_JUMP);

    match m.step_air(0) {
        AIR_LANDED => { m.set_action(ACT_METAL_WATER_JUMP_LAND, 0.0); }
        AIR_HIT_WALL => m.state.forward_vel = 0.0,
        _ => {}
    }
    false
}

fn act_hold_metal_water_jump(m: &mut MarioRuntime) -> bool {
    if drop_requested(m) { return drop_and_set(m, ACT_METAL_WATER_FALLING, 0.0); }
    if !metal_cap(m) { return m.set_action(ACT_HOLD_WATER_IDLE, 0.0); }
    if update_metal_water_jump_speed(m) { return m.set_action(ACT_HOLD_WATER_JUMP, 1.0); }

    play_metal_water_jumping_sound(m);
    m.water.clock.set(MARIO_ANIM_JUMP_WITH_LIGHT_OBJ);

    match m.step_air(0) {
        AIR_LANDED => { m.set_action(ACT_HOLD_METAL_WATER_JUMP_LAND, 0.0); }
        AIR_HIT_WALL => m.state.forward_vel = 0.0,
        _ => {}
    }
    false
}

fn act_metal_water_falling(m: &mut MarioRuntime) -> bool {
    if !metal_cap(m) { return m.set_action(ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 {
        m.state.face_angle[1] = to_s16(m.face_angle[1] + 0x400 as f64 * sins(to_s16(m.intended_yaw - m.face_angle[1]) as f64)) as f64;
    }
    m.water.clock.set(if m.action_arg == 0.0 { MARIO_ANIM_GENERAL_FALL } else { MARIO_ANIM_FALL_FROM_WATER });
    stationary_slow_down(m);
    if perform_water_step(m) & WATER_STEP_HIT_FLOOR != 0 {
        // hit floor or cancelled
        m.set_action(ACT_METAL_WATER_FALL_LAND, 0.0);
    }
    false
}

fn act_hold_metal_water_falling(m: &mut MarioRuntime) -> bool {
    if drop_requested(m) { return drop_and_set(m, ACT_METAL_WATER_FALLING, 0.0); }
    if !metal_cap(m) { return m.set_action(ACT_HOLD_WATER_IDLE, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 {
        m.state.face_angle[1] = to_s16(m.face_angle[1] + 0x400 as f64 * sins(to_s16(m.intended_yaw - m.face_angle[1]) as f64)) as f64;
    }
    m.water.clock.set(MARIO_ANIM_FALL_WITH_LIGHT_OBJ);
    stationary_slow_down(m);
    if perform_water_step(m) & WATER_STEP_HIT_FLOOR != 0 {
        m.set_action(ACT_HOLD_METAL_WATER_FALL_LAND, 0.0);
    }
    false
}

fn act_metal_water_jump_land(m: &mut MarioRuntime) -> bool {
    play_metal_water_jumping_sound(m);
    if !metal_cap(m) { return m.set_action(ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_METAL_WATER_WALKING, 0.0); }

    stop_and_set_height_to_floor(&mut m.state);
    m.water.clock.set(MARIO_ANIM_LAND_FROM_SINGLE_JUMP);
    if m.water.clock.is_at_end() { return m.set_action(ACT_METAL_WATER_STANDING, 0.0); }
    false
}

fn act_hold_metal_water_jump_land(m: &mut MarioRuntime) -> bool {
    play_metal_water_jumping_sound(m);
    if drop_requested(m) { return drop_and_set(m, ACT_METAL_WATER_STANDING, 0.0); }
    if !metal_cap(m) { return m.set_action(ACT_HOLD_WATER_IDLE, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_HOLD_METAL_WATER_WALKING, 0.0); }

    stop_and_set_height_to_floor(&mut m.state);
    m.water.clock.set(MARIO_ANIM_JUMP_LAND_WITH_LIGHT_OBJ);
    if m.water.clock.is_at_end() { return m.set_action(ACT_HOLD_METAL_WATER_STANDING, 0.0); }
    false
}

fn act_metal_water_fall_land(m: &mut MarioRuntime) -> bool {
    play_metal_water_jumping_sound(m);
    if !metal_cap(m) { return m.set_action(ACT_WATER_IDLE, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_METAL_WATER_WALKING, 0.0); }

    stop_and_set_height_to_floor(&mut m.state);
    m.water.clock.set(MARIO_ANIM_GENERAL_LAND);
    if m.water.clock.is_at_end() { return m.set_action(ACT_METAL_WATER_STANDING, 0.0); }
    false
}

fn act_hold_metal_water_fall_land(m: &mut MarioRuntime) -> bool {
    play_metal_water_jumping_sound(m);
    if drop_requested(m) { return drop_and_set(m, ACT_METAL_WATER_STANDING, 0.0); }
    if !metal_cap(m) { return m.set_action(ACT_HOLD_WATER_IDLE, 0.0); }
    if m.input & INPUT_NONZERO_ANALOG != 0 { return m.set_action(ACT_HOLD_METAL_WATER_WALKING, 0.0); }

    stop_and_set_height_to_floor(&mut m.state);
    m.water.clock.set(MARIO_ANIM_FALL_LAND_WITH_LIGHT_OBJ);
    if m.water.clock.is_at_end() { return m.set_action(ACT_HOLD_METAL_WATER_STANDING, 0.0); }
    false
}

// --- dispatcher --------------------------------------------------------------------------------

/// decomp: `transition_submerged_to_walking` (mario.c).
pub fn transition_submerged_to_walking(m: &mut MarioRuntime) -> bool {
    m.state.angle_vel = [0.0; 3];
    if m.held_obj.is_none() { m.set_action(ACT_WALKING, 0.0) } else { m.set_action(ACT_HOLD_WALKING, 0.0) }
}

/// decomp: `check_common_submerged_cancels`.
fn check_common_submerged_cancels(m: &mut MarioRuntime) -> bool {
    if m.pos[1] > m.water.level - 80.0 {
        if m.water.level - 80.0 > m.floor_height {
            m.state.pos[1] = m.water.level - 80.0;
        } else {
            // If you press B to throw the shell there is a ~5 frame window where the held
            // object is the shell but Mario is not in the water shell swimming action: it can be
            // held on land (used for cloning in DDD).
            if m.action == ACT_WATER_SHELL_SWIMMING && m.held_obj.is_some() { stop_shell(m); }
            return transition_submerged_to_walking(m);
        }
    }

    if m.health < 0x100 as f64 && m.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE) == 0 {
        m.events.push(MarioEvent::Death);
        m.set_action(ACT_DROWNING, 0.0);
    }
    false
}

/// decomp: `mario_execute_submerged_action`. `None` = not a submerged action.
pub fn submerged_action(m: &mut MarioRuntime) -> Option<bool> {
    if m.action & ACT_GROUP_MASK != ACT_GROUP_SUBMERGED { return None; }
    if check_common_submerged_cancels(m) { return Some(true); }

    m.state.quicksand_depth = 0.0;

    let cancel = match m.action {
        ACT_WATER_IDLE => act_water_idle(m),
        ACT_HOLD_WATER_IDLE => act_hold_water_idle(m),
        ACT_WATER_ACTION_END => act_water_action_end(m),
        ACT_HOLD_WATER_ACTION_END => act_hold_water_action_end(m),
        ACT_DROWNING => act_drowning(m),
        ACT_BACKWARD_WATER_KB => act_backward_water_kb(m),
        ACT_FORWARD_WATER_KB => act_forward_water_kb(m),
        ACT_WATER_DEATH => act_water_death(m),
        ACT_WATER_SHOCKED => act_water_shocked(m),
        ACT_BREASTSTROKE => act_breaststroke(m),
        ACT_SWIMMING_END => act_swimming_end(m),
        ACT_FLUTTER_KICK => act_flutter_kick(m),
        ACT_HOLD_BREASTSTROKE => act_hold_breaststroke(m),
        ACT_HOLD_SWIMMING_END => act_hold_swimming_end(m),
        ACT_HOLD_FLUTTER_KICK => act_hold_flutter_kick(m),
        ACT_WATER_SHELL_SWIMMING => act_water_shell_swimming(m),
        ACT_WATER_THROW => act_water_throw(m),
        ACT_WATER_PUNCH => act_water_punch(m),
        ACT_WATER_PLUNGE => act_water_plunge(m),
        ACT_CAUGHT_IN_WHIRLPOOL => act_caught_in_whirlpool(m),
        ACT_METAL_WATER_STANDING => act_metal_water_standing(m),
        ACT_METAL_WATER_WALKING => act_metal_water_walking(m),
        ACT_METAL_WATER_FALLING => act_metal_water_falling(m),
        ACT_METAL_WATER_FALL_LAND => act_metal_water_fall_land(m),
        ACT_METAL_WATER_JUMP => act_metal_water_jump(m),
        ACT_METAL_WATER_JUMP_LAND => act_metal_water_jump_land(m),
        ACT_HOLD_METAL_WATER_STANDING => act_hold_metal_water_standing(m),
        ACT_HOLD_METAL_WATER_WALKING => act_hold_metal_water_walking(m),
        ACT_HOLD_METAL_WATER_FALLING => act_hold_metal_water_falling(m),
        ACT_HOLD_METAL_WATER_FALL_LAND => act_hold_metal_water_fall_land(m),
        ACT_HOLD_METAL_WATER_JUMP => act_hold_metal_water_jump(m),
        ACT_HOLD_METAL_WATER_JUMP_LAND => act_hold_metal_water_jump_land(m),
        _ => return None,
    };
    Some(cancel)
}

// --- airborne water jumps (mario_actions_airborne.c) --------------------------------------------

/// decomp: `lava_boost_on_wall`.
fn lava_boost_on_wall(m: &mut MarioRuntime) -> bool {
    if let Some(w) = m.wall.clone() { m.state.face_angle[1] = atan2s(w.normal[2], w.normal[0]) as f64; }
    if m.forward_vel < 24.0 { m.state.forward_vel = 24.0; }
    if m.flags & MARIO_METAL_CAP == 0 {
        m.state.hurt_counter += if m.flags & MARIO_CAP_ON_HEAD != 0 { 12.0 } else { 18.0 };
    }
    drop_and_set(m, ACT_LAVA_BOOST, 1.0)
}

/// decomp: `act_water_jump` (mario_actions_airborne.c).
fn act_water_jump(m: &mut MarioRuntime) -> bool {
    if m.forward_vel < 15.0 { mario_set_forward_vel(&mut m.state, 15.0); }
    match m.step_air(AIR_CHECK_LEDGE_GRAB) {
        AIR_LANDED => { m.set_action(ACT_JUMP_LAND, 0.0); }
        AIR_HIT_WALL => mario_set_forward_vel(&mut m.state, 15.0),
        crate::mario_step::AIR_GRABBED_LEDGE => { m.set_action(ACT_LEDGE_GRAB, 0.0); }
        AIR_HIT_LAVA_WALL => { lava_boost_on_wall(m); }
        _ => {}
    }
    false
}

/// decomp: `act_hold_water_jump`.
fn act_hold_water_jump(m: &mut MarioRuntime) -> bool {
    if drop_requested(m) { return drop_and_set(m, ACT_FREEFALL, 0.0); }
    if m.forward_vel < 15.0 { mario_set_forward_vel(&mut m.state, 15.0); }
    match m.step_air(0) {
        AIR_LANDED => { m.set_action(ACT_HOLD_JUMP_LAND, 0.0); }
        AIR_HIT_WALL => mario_set_forward_vel(&mut m.state, 15.0),
        AIR_HIT_LAVA_WALL => { lava_boost_on_wall(m); }
        _ => {}
    }
    false
}

/// The two airborne water actions, for the airborne group chain.
pub fn airborne_water_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        ACT_WATER_JUMP => Some(act_water_jump(m)),
        ACT_HOLD_WATER_JUMP => Some(act_hold_water_jump(m)),
        _ => None,
    }
}

// -----------------------------------------------------------------------------------------------
// Transitions and interactions that live in mario.c / interaction.c
// -----------------------------------------------------------------------------------------------

/// decomp: `set_water_plunge_action` (mario.c).
pub fn set_water_plunge_action(m: &mut MarioRuntime) -> bool {
    m.state.forward_vel /= 4.0;
    m.state.vel[1] /= 2.0;
    m.state.pos[1] = m.water.level - 100.0;
    m.state.face_angle[2] = 0.0;
    m.state.angle_vel = [0.0; 3];
    if m.action & ACT_FLAG_DIVING == 0 { m.state.face_angle[0] = 0.0; }
    m.set_action(ACT_WATER_PLUNGE, 0.0)
}

/// decomp: `determine_knockback_action`, water branch (`terrainIndex == 2`): the action Mario
/// is knocked into and the speeds it sets. `object_y` is the interacting object's height.
pub fn water_knockback(m: &mut MarioRuntime, angle_to_object: f64, object_y: f64) -> u32 {
    let facing_d_yaw = to_s16(angle_to_object - m.face_angle[1]);
    m.state.face_angle[1] = angle_to_object;

    if m.forward_vel < 28.0 { mario_set_forward_vel(&mut m.state, 28.0); }
    if m.pos[1] >= object_y {
        if m.vel[1] < 20.0 { m.state.vel[1] = 20.0; }
    } else if m.vel[1] > 0.0 {
        m.state.vel[1] = 0.0;
    }

    if (-0x4000..=0x4000).contains(&facing_d_yaw) {
        m.state.forward_vel *= -1.0;
        ACT_BACKWARD_WATER_KB
    } else {
        m.state.face_angle[1] = to_s16(m.face_angle[1] + 0x8000 as f64) as f64;
        ACT_FORWARD_WATER_KB
    }
}

/// True while Mario is in a swimming or metal-cap submerged action (`ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER`).
pub fn is_in_water_action(action: u32) -> bool { action & (ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER) != 0 }

/// decomp: the water branch of `interact_shock`. Returns true when Mario was shocked (so the
/// caller should not run the dry `ACT_SHOCKED` path).
pub fn shock_in_water(m: &mut MarioRuntime) -> bool {
    if !is_in_water_action(m.action) { return false; }
    drop_and_set(m, ACT_WATER_SHOCKED, 0.0)
}

/// decomp: `interact_whirlpool` (interaction.c). `whirlpool` is the object that touched Mario.
pub fn interact_whirlpool(m: &mut MarioRuntime, whirlpool: ObjId) -> bool {
    if m.action == ACT_CAUGHT_IN_WHIRLPOOL { return false; }
    // mario_stop_riding_and_holding
    m.drop_held_object();
    m.objects[whirlpool].interact_status = INT_STATUS_INTERACTED;
    m.interact_obj = Some(whirlpool);
    m.used_obj = Some(whirlpool);
    m.state.forward_vel = 0.0;
    m.water.whirlpool_pos_y = m.pos[1] - m.objects[whirlpool].pos[1];
    m.set_action(ACT_CAUGHT_IN_WHIRLPOOL, 0.0)
}

/// Whirlpool pull radius for a course (`apply_water_current`: 4000 in DDD area 2, else 2000).
pub fn whirlpool_radius(level: &str, area: i32) -> f64 { if level == "ddd" && area == 2 { 4000.0 } else { 2000.0 } }

/// Convenience constructor for the loader.
pub fn make_whirlpool(pos: Vec3, strength: f64, radius: f64) -> Whirlpool { Whirlpool { pos, strength, radius } }
