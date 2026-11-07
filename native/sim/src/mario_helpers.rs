//! src/sim/marioHelpers.ts
use crate::collision::CollisionWorld;
use crate::math::*;
use crate::state::*;
use crate::surfaces::*;

pub fn mario_set_forward_vel(m: &mut MarioState, forward_vel: f64) {
    m.forward_vel = forward_vel;
    m.slide_vel_x = sins(m.face_angle[1]) * m.forward_vel;
    m.slide_vel_z = coss(m.face_angle[1]) * m.forward_vel;
    m.vel[0] = m.slide_vel_x;
    m.vel[2] = m.slide_vel_z;
}

pub fn mario_get_floor_class(m: &MarioState) -> i32 {
    let mut class = if m.slide_terrain { SURFACE_CLASS_VERY_SLIPPERY } else { SURFACE_CLASS_DEFAULT };
    if let Some(f) = &m.floor {
        class = match f.kind {
            SURFACE_NOT_SLIPPERY | SURFACE_HARD_NOT_SLIPPERY | SURFACE_SWITCH => SURFACE_CLASS_NOT_SLIPPERY,
            SURFACE_SLIPPERY | SURFACE_NOISE_SLIPPERY | SURFACE_HARD_SLIPPERY | SURFACE_NO_CAM_COL_SLIPPERY => SURFACE_CLASS_SLIPPERY,
            SURFACE_VERY_SLIPPERY | SURFACE_ICE | SURFACE_HARD_VERY_SLIPPERY | SURFACE_NOISE_VERY_SLIPPERY_73
            | SURFACE_NOISE_VERY_SLIPPERY_74 | SURFACE_NOISE_VERY_SLIPPERY | SURFACE_NO_CAM_COL_VERY_SLIPPERY => SURFACE_CLASS_VERY_SLIPPERY,
            _ => class,
        };
    }
    if m.action == ACT_CRAWLING && class == SURFACE_CLASS_DEFAULT && m.floor.as_ref().is_some_and(|f| f.normal[1] > 0.5) {
        class = SURFACE_CLASS_NOT_SLIPPERY;
    }
    class
}

pub fn mario_facing_downhill(m: &MarioState, turn_yaw: bool) -> bool {
    let mut yaw = m.face_angle[1];
    if turn_yaw && m.forward_vel < 0.0 {
        yaw = to_s16(yaw + 32768.0) as f64;
    }
    let yaw = to_s16(m.floor_angle - yaw);
    -0x4000 < yaw && yaw < 0x4000
}

pub fn mario_floor_is_slippery(m: &MarioState) -> bool {
    let Some(f) = &m.floor else { return false };
    if m.slide_terrain && f.normal[1] < 0.9998477 { return true; }
    let norm_y = match mario_get_floor_class(m) {
        SURFACE_CLASS_VERY_SLIPPERY => 0.9848077,
        SURFACE_CLASS_SLIPPERY => 0.9396926,
        SURFACE_CLASS_NOT_SLIPPERY => 0.0,
        _ => 0.7880108,
    };
    f.normal[1] <= norm_y
}

pub fn mario_floor_is_slope(m: &MarioState) -> bool {
    let Some(f) = &m.floor else { return false };
    if m.slide_terrain && f.normal[1] < 0.9998477 { return true; }
    let norm_y = match mario_get_floor_class(m) {
        SURFACE_CLASS_VERY_SLIPPERY => 0.9961947,
        SURFACE_CLASS_SLIPPERY => 0.9848077,
        SURFACE_CLASS_NOT_SLIPPERY => 0.9396926,
        _ => 0.9659258,
    };
    f.normal[1] <= norm_y
}

pub fn mario_floor_is_steep(m: &MarioState) -> bool {
    let Some(f) = &m.floor else { return false };
    if mario_facing_downhill(m, false) { return false; }
    let norm_y = match mario_get_floor_class(m) {
        SURFACE_CLASS_VERY_SLIPPERY => 0.9659258,
        SURFACE_CLASS_SLIPPERY => 0.9396926,
        _ => 0.8660254,
    };
    f.normal[1] <= norm_y
}

pub fn find_floor_height_relative_polar(m: &MarioState, world: &CollisionWorld, angle_from_mario: f64, dist: f64) -> f64 {
    let a = to_s16(m.face_angle[1] + angle_from_mario) as f64;
    let y = sins(a) * dist;
    let x = coss(a) * dist;
    world.find_floor(m.pos[0] + y, m.pos[1] + 100.0, m.pos[2] + x).height
}

pub fn find_floor_slope(m: &MarioState, world: &CollisionWorld, yaw_offset: f64) -> i32 {
    let a = to_s16(m.face_angle[1] + yaw_offset) as f64;
    let (x, z) = (sins(a) * 5.0, coss(a) * 5.0);
    let forward = world.find_floor(m.pos[0] + x, m.pos[1] + 100.0, m.pos[2] + z).height;
    let backward = world.find_floor(m.pos[0] - x, m.pos[1] + 100.0, m.pos[2] - z).height;
    let fd = forward - m.pos[1];
    let bd = m.pos[1] - backward;
    if fd * fd < bd * bd { atan2s(5.0, fd) } else { atan2s(5.0, bd) }
}
