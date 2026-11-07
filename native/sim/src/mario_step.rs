//! Ground and air movement integration (src/game/mario_step.c via src/sim/marioStep.ts).
//! Mario moves in four quarter-steps per 30 Hz frame; walls resolve on the
//! destination before floor height is sampled.
use crate::collision::{CollisionWorld, Surface};
use crate::mario_helpers::mario_set_forward_vel;
use crate::math::*;
use crate::state::*;
use crate::surfaces::SURFACE_HANGABLE;

pub const GROUND_STEP_LEFT_GROUND_I: i32 = 0;
pub const GROUND_STEP_NONE_I: i32 = 1;
pub const GROUND_STEP_HIT_WALL_I: i32 = 2;
pub const GROUND_STEP_HIT_WALL_STOP_QSTEPS_I: i32 = 2;
pub const GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS_I: i32 = 3;

pub const AIR_NONE: i32 = 0;
pub const AIR_LANDED: i32 = 1;
pub const AIR_HIT_WALL: i32 = 2;
pub const AIR_GRABBED_LEDGE: i32 = 3;
pub const AIR_GRABBED_CEILING: i32 = 4;
pub const AIR_HIT_LAVA_WALL: i32 = 6;
pub const AIR_CHECK_LEDGE_GRAB: u32 = 1;
pub const AIR_CHECK_HANG: u32 = 2;

pub fn stop_and_set_height_to_floor(m: &mut MarioState) {
    mario_set_forward_vel(m, 0.0);
    m.vel[1] = 0.0;
    m.pos[1] = m.floor_height;
}

pub fn stationary_ground_step(m: &mut MarioState, _world: &CollisionWorld) -> i32 {
    mario_set_forward_vel(m, 0.0);
    m.pos[1] = m.floor_height;
    GROUND_STEP_NONE_I
}

fn ground_quarter_step(m: &mut MarioState, world: &CollisionWorld, next: &mut Vec3) -> i32 {
    world.resolve_and_return_wall_collisions(next, 30.0, 24.0);
    let upper = world.resolve_and_return_wall_collisions(next, 60.0, 50.0);

    let floor_hit = find_movement_floor(world, next, m.action);
    let floor_height = floor_hit.height;
    let ceil_height = world.vec3f_find_ceil(next, floor_height).height;
    m.wall = upper.clone();

    let Some(floor) = floor_hit.surface else { return GROUND_STEP_HIT_WALL_STOP_QSTEPS_I };

    if next[1] > floor_height + 100.0 {
        if next[1] + 160.0 >= ceil_height { return GROUND_STEP_HIT_WALL_STOP_QSTEPS_I; }
        m.pos = *next;
        m.floor = Some(floor);
        m.floor_height = floor_height;
        return GROUND_STEP_LEFT_GROUND_I;
    }
    if floor_height + 160.0 >= ceil_height { return GROUND_STEP_HIT_WALL_STOP_QSTEPS_I; }

    m.pos = [next[0], floor_height, next[2]];
    m.floor = Some(floor);
    m.floor_height = floor_height;

    if let Some(w) = upper {
        let d = to_s16(atan2s(w.normal[2], w.normal[0]) as f64 - m.face_angle[1]);
        if (0x2aaa..=0x5555).contains(&d) || (-0x5555..=-0x2aaa).contains(&d) { return GROUND_STEP_NONE_I; }
        return GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS_I;
    }
    GROUND_STEP_NONE_I
}

/// `CollisionWorld::findMovementFloor`: riding a shell skims the water surface.
pub fn find_movement_floor(world: &CollisionWorld, pos: &Vec3, action: u32) -> crate::collision::Hit {
    let hit = world.find_floor(pos[0], pos[1], pos[2]);
    if action & ACT_FLAG_RIDING_SHELL != 0 {
        let height = world.water_level(pos[0], pos[2]);
        if hit.surface.is_some() && height > hit.height {
            return crate::collision::Hit { height, surface: Some(world.shell_water_floor(height)) };
        }
    }
    hit
}

pub fn perform_ground_step(m: &mut MarioState, world: &CollisionWorld) -> i32 {
    let mut result = GROUND_STEP_NONE_I;
    for _ in 0..4 {
        let ny = m.floor.as_ref().map_or(1.0, |f| f.normal[1]);
        let mut intended = [m.pos[0] + ny * (m.vel[0] / 4.0), m.pos[1], m.pos[2] + ny * (m.vel[2] / 4.0)];
        result = ground_quarter_step(m, world, &mut intended);
        if result == GROUND_STEP_LEFT_GROUND_I || result == GROUND_STEP_HIT_WALL_STOP_QSTEPS_I { break; }
    }
    if result == GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS_I { result = GROUND_STEP_HIT_WALL_I; }
    result
}

pub fn check_ledge_grab(m: &mut MarioState, world: &CollisionWorld, wall: &Surface, intended: &Vec3, next: &Vec3) -> bool {
    if m.vel[1] > 0.0 { return false; }
    let dx = next[0] - intended[0];
    let dz = next[2] - intended[2];
    if dx * m.vel[0] + dz * m.vel[2] > 0.0 { return false; }
    let lx = next[0] - wall.normal[0] * 60.0;
    let lz = next[2] - wall.normal[2] * 60.0;
    let hit = world.find_floor(lx, next[1] + 160.0, lz);
    let ly = hit.height;
    if ly - next[1] <= 100.0 { return false; }
    let Some(surface) = hit.surface else { return false };
    m.pos = [lx, ly, lz];
    m.floor_angle = atan2s(surface.normal[2], surface.normal[0]) as f64;
    m.floor = Some(surface);
    m.floor_height = ly;
    m.face_angle[0] = 0.0;
    m.face_angle[1] = to_s16(atan2s(wall.normal[2], wall.normal[0]) as f64 + 32768.0) as f64;
    true
}

fn air_quarter_step(m: &mut MarioState, world: &CollisionWorld, intended: &Vec3, step_arg: u32) -> i32 {
    let mut next = *intended;
    let upper = world.resolve_and_return_wall_collisions(&mut next, 150.0, 50.0);
    let lower = world.resolve_and_return_wall_collisions(&mut next, 30.0, 50.0);

    let floor_hit = find_movement_floor(world, &next, m.action);
    let floor_height = floor_hit.height;
    let ceil_height = world.vec3f_find_ceil(&next, floor_height).height;
    m.wall = None;

    let Some(floor) = floor_hit.surface else {
        if next[1] <= m.floor_height {
            m.pos[1] = m.floor_height;
            return AIR_LANDED;
        }
        m.pos[1] = next[1];
        return AIR_HIT_WALL;
    };

    if next[1] <= floor_height {
        if ceil_height - floor_height > 160.0 {
            m.pos[0] = next[0];
            m.pos[2] = next[2];
            m.floor = Some(floor);
            m.floor_height = floor_height;
        }
        m.pos[1] = floor_height;
        return AIR_LANDED;
    }

    if next[1] + 160.0 > ceil_height {
        if m.vel[1] >= 0.0 {
            m.vel[1] = 0.0;
            if step_arg & AIR_CHECK_HANG != 0 && m.ceil.as_ref().is_some_and(|c| c.kind == SURFACE_HANGABLE) {
                return AIR_GRABBED_CEILING;
            }
            return AIR_NONE;
        }
        if next[1] <= m.floor_height {
            m.pos[1] = m.floor_height;
            return AIR_LANDED;
        }
        m.pos[1] = next[1];
        return AIR_HIT_WALL;
    }

    if step_arg & AIR_CHECK_LEDGE_GRAB != 0 && upper.is_none() {
        if let Some(lw) = &lower {
            if check_ledge_grab(m, world, lw, intended, &next) { return AIR_GRABBED_LEDGE; }
            m.pos = next;
            m.floor = Some(floor);
            m.floor_height = floor_height;
            return AIR_NONE;
        }
    }

    m.pos = next;
    m.floor = Some(floor);
    m.floor_height = floor_height;

    if let Some(wall) = upper.or(lower) {
        let d = to_s16(atan2s(wall.normal[2], wall.normal[0]) as f64 - m.face_angle[1]);
        let lava = wall.kind == 0x0001;
        m.wall = Some(wall);
        if lava { return AIR_HIT_LAVA_WALL; }
        if !(-0x6000..=0x6000).contains(&d) { return AIR_HIT_WALL; }
    }
    AIR_NONE
}

pub fn should_strengthen_gravity_for_jump_ascent(m: &MarioState) -> bool {
    if m.flags & MARIO_UNKNOWN_08 == 0 { return false; }
    if m.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE) != 0 { return false; }
    if m.input & INPUT_A_DOWN == 0 && m.vel[1] > 20.0 {
        return m.action & ACT_FLAG_CONTROL_JUMP_HEIGHT != 0;
    }
    false
}

pub fn apply_gravity(m: &mut MarioState) {
    let scale = m.gravity_scale * if m.flags & MARIO_METAL_CAP != 0 { 1.25 } else { 1.0 };
    if m.action == ACT_LONG_JUMP || m.action == ACT_SLIDE_KICK {
        m.vel[1] -= 2.0 * scale;
        if m.vel[1] < -75.0 { m.vel[1] = -75.0; }
    } else if m.action == ACT_SHOT_FROM_CANNON {
        m.vel[1] = (-75f64).max(m.vel[1] - 1.0);
    } else if m.action == ACT_LAVA_BOOST {
        m.vel[1] -= 3.2 * scale;
        if m.vel[1] < -65.0 { m.vel[1] = -65.0; }
    } else if m.action == ACT_GETTING_BLOWN {
        m.vel[1] -= 3.2 * scale;
        if m.vel[1] < -75.0 { m.vel[1] = -75.0; }
    } else if should_strengthen_gravity_for_jump_ascent(m) {
        m.vel[1] /= 4.0;
    } else if m.flags & MARIO_WING_CAP != 0 && m.vel[1] < 0.0 && m.input & INPUT_A_DOWN != 0 {
        m.vel[1] -= 2.0;
        if m.vel[1] < -37.5 { m.vel[1] = (-37.5f64).min(m.vel[1] + 4.0); }
    } else {
        m.vel[1] -= 4.0 * scale;
        if m.vel[1] < -75.0 { m.vel[1] = -75.0; }
    }
}

pub fn perform_air_step(m: &mut MarioState, world: &CollisionWorld, step_arg: u32) -> i32 {
    let mut result = AIR_NONE;
    m.wall = None;
    for _ in 0..4 {
        let intended = [m.pos[0] + m.vel[0] / 4.0, m.pos[1] + m.vel[1] / 4.0, m.pos[2] + m.vel[2] / 4.0];
        let q = air_quarter_step(m, world, &intended, step_arg);
        if q != AIR_NONE { result = q; }
        if matches!(q, AIR_LANDED | AIR_GRABBED_LEDGE | AIR_GRABBED_CEILING | AIR_HIT_LAVA_WALL) { break; }
    }
    if m.vel[1] >= 0.0 { m.peak_height = m.pos[1]; }
    if m.action != ACT_FLYING { apply_gravity(m); }
    result
}

/// mario_bonk_reflection, minus the sound.
pub fn bonk_reflection(m: &mut MarioState, negate_speed: bool) {
    if let Some(w) = &m.wall {
        let wa = atan2s(w.normal[2], w.normal[0]) as f64;
        m.face_angle[1] = to_s16(wa - to_s16(m.face_angle[1] - wa) as f64) as f64;
    }
    if negate_speed {
        let v = -m.forward_vel;
        mario_set_forward_vel(m, v);
    } else {
        m.face_angle[1] = to_s16(m.face_angle[1] + 32768.0) as f64;
    }
}
