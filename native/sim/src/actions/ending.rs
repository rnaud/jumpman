//! Port of `src/sim/actions/ending.ts` (`ACT_JUMBO_STAR_CUTSCENE`, `ACT_END_PEACH_CUTSCENE`).
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::stop_and_set_height_to_floor;
use crate::math::*;
use crate::state::*;
use std::sync::OnceLock;

// sJumboStarKeyframes in mario_actions_cutscene.c. Speeds are in thousandths per tick.
const FLIGHT_KEYS: [[f64; 4]; 27] = [
    [20.0, 0.0, 678.0, -2916.0], [30.0, 0.0, 680.0, -3500.0], [40.0, 1000.0, 700.0, -4000.0],
    [50.0, 2500.0, 750.0, -3500.0], [50.0, 3500.0, 800.0, -2000.0], [50.0, 4000.0, 850.0, 0.0],
    [50.0, 3500.0, 900.0, 2000.0], [50.0, 2000.0, 950.0, 3500.0], [50.0, 0.0, 1000.0, 4000.0],
    [50.0, -2000.0, 1050.0, 3500.0], [50.0, -3500.0, 1100.0, 2000.0], [50.0, -4000.0, 1150.0, 0.0],
    [50.0, -3500.0, 1200.0, -2000.0], [50.0, -2000.0, 1250.0, -3500.0], [50.0, 0.0, 1300.0, -4000.0],
    [50.0, 2000.0, 1350.0, -3500.0], [50.0, 3500.0, 1400.0, -2000.0], [50.0, 4000.0, 1450.0, 0.0],
    [50.0, 3500.0, 1500.0, 2000.0], [50.0, 2000.0, 1600.0, 3500.0], [50.0, 0.0, 1700.0, 4000.0],
    [50.0, -2000.0, 1800.0, 3500.0], [50.0, -3500.0, 1900.0, 2000.0], [30.0, -4000.0, 2000.0, 0.0],
    [0.0, -3500.0, 2100.0, -2000.0], [0.0, -2000.0, 2200.0, -3500.0], [0.0, 0.0, 2300.0, -4000.0],
];
// Root translation of anim_25, divided by four as in update_mario_pos_for_anim.
const X25: [f64; 1] = [0.0];
const Y25: [f64; 100] = [
    42.0, 42.0, 42.0, 42.0, 42.0, 42.0, 42.0, 43.0, 43.0, 43.0, 43.0, 43.0, 43.0, 43.0, 43.0, 44.0, 44.0, 44.0, 44.0, 44.0, 44.0, 44.0, 44.0, 45.0, 45.0, 45.0, 45.0, 45.0, 45.0, 45.0, 45.0, 46.0, 46.0, 46.0, 46.0, 46.0, 46.0, 46.0, 46.0, 46.0, 47.0, 47.0, 47.0, 47.0, 47.0, 47.0, 47.0, 47.0, 46.0, 46.0, 46.0, 45.0, 44.0, 43.0, 42.0, 40.0, 38.0, 36.0, 34.0, 33.0, 31.0, 30.0, 30.0, 27.0, 26.0, 40.0, 65.0, 88.0, 108.0, 127.0, 143.0, 158.0, 171.0, 182.0, 192.0, 200.0, 207.0, 212.0, 217.0, 220.0, 223.0, 224.0, 225.0, 224.0, 222.0, 218.0, 212.0, 205.0, 196.0, 186.0, 175.0, 162.0, 148.0, 132.0, 116.0, 98.0, 79.0, 60.0, 39.0, 29.0,
];
const Z25: [f64; 100] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 7.0, 9.0, 11.0, 9.0, 6.0, 4.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0,
];
// Root translation of anim_26, divided by four as in update_mario_pos_for_anim.
const X26: [f64; 1] = [0.0];
const Y26: [f64; 94] = [
    29.0, 29.0, 29.0, 77.0, 119.0, 154.0, 184.0, 209.0, 229.0, 245.0, 258.0, 267.0, 273.0, 278.0, 280.0, 281.0, 281.0, 281.0, 278.0, 273.0, 266.0, 256.0, 242.0, 225.0, 205.0, 180.0, 150.0, 115.0, 75.0, 30.0, 29.0, 29.0, 30.0, 76.0, 118.0, 155.0, 188.0, 218.0, 243.0, 265.0, 284.0, 299.0, 312.0, 322.0, 330.0, 336.0, 341.0, 343.0, 344.0, 344.0, 338.0, 325.0, 306.0, 281.0, 253.0, 221.0, 188.0, 153.0, 119.0, 86.0, 55.0, 29.0, 29.0, 29.0, 29.0, 29.0, 75.0, 117.0, 156.0, 191.0, 223.0, 252.0, 278.0, 300.0, 320.0, 338.0, 353.0, 365.0, 376.0, 384.0, 391.0, 396.0, 399.0, 401.0, 401.0, 400.0, 399.0, 396.0, 393.0, 389.0, 385.0, 380.0, 376.0, 371.0,
];
const Z26: [f64; 1] = [4.0];

/// Uniform clamped cubic, with the decomp's special first and last two segments.
pub fn spline_weights(segment: i64, t: f64) -> [f64; 4] {
    let n = FLIGHT_KEYS.len() as i64 - 4;
    if segment >= n - 1 {
        let mut w = spline_weights(n - segment, 1.0 - t);
        w.reverse();
        return w;
    }
    let a = t * t;
    let b = a * t;
    let c = (1.0 - t).powf(3.0);
    if segment == 0 { return [c, b * 1.75 - a * 4.5 + t * 3.0, -b * 11.0 / 12.0 + a * 1.5, b / 6.0]; }
    if segment == 1 { return [c / 4.0, b * 7.0 / 12.0 - a * 1.25 + t / 4.0 + 7.0 / 12.0, -b / 2.0 + a / 2.0 + t / 2.0 + 1.0 / 6.0, b / 6.0]; }
    [c / 6.0, b / 2.0 - a + 2.0 / 3.0, -b / 2.0 + a / 2.0 + t / 2.0 + 1.0 / 6.0, b / 6.0]
}

fn fround(x: f64) -> f64 { x as f32 as f64 }

/// The path and its speed are fixed. Sample it once at the source's 30 Hz clock.
pub fn flight_path() -> Vec<Vec3> {
    let mut path: Vec<Vec3> = vec![];
    let mut t = 0.0f64;
    for segment in 0..(FLIGHT_KEYS.len() - 3) {
        while t < 1.0 {
            let weights = spline_weights(segment as i64, t);
            let mut pos: Vec3 = [0.0; 3];
            for i in 0..4 {
                for axis in 0..3 { pos[axis] += weights[i] * FLIGHT_KEYS[segment + i][axis + 1]; }
            }
            path.push(pos);
            t = fround(t + fround(FLIGHT_KEYS[segment][0] / 1000.0));
        }
        t = fround(t - 1.0);
    }
    path
}

fn flight() -> &'static Vec<Vec3> {
    static FLIGHT: OnceLock<Vec<Vec3>> = OnceLock::new();
    FLIGHT.get_or_init(flight_path)
}

pub fn advance(m: &mut MarioRuntime) {
    m.action_arg += 1.0;
    m.action_timer = 0.0;
    m.action_state = 0.0;
}

pub fn wing_landing(m: &mut MarioRuntime, step_arg: u32) -> bool {
    m.input |= INPUT_A_DOWN;
    m.flags |= MARIO_WING_CAP | MARIO_CAP_ON_HEAD;
    mario_set_forward_vel(m, 0.0);
    m.step_air(step_arg) == AIR_STEP_LANDED as i32
}

fn at(frames: &[f64], t: f64) -> f64 {
    frames[(t.min(frames.len() as f64 - 1.0)) as usize]
}

pub fn jumbo_star_cutscene(m: &mut MarioRuntime) -> bool {
    m.flags |= MARIO_WING_CAP | MARIO_CAP_ON_HEAD;
    if m.action_arg == 0.0 {
        m.face_angle[1] = -0x8000 as f64;
        m.pos[0] = 0.0;
        m.pos[2] = 0.0;
        if m.action_state == 0.0 {
            if wing_landing(m, 1) { m.action_state = 1.0; }
        } else {
            m.action_timer += 1.0;
            if m.action_timer >= 15.0 { advance(m); }
        }
    } else if m.action_arg == 1.0 {
        let t = m.action_timer;
        m.action_timer += 1.0;
        let takeoff = m.action_state == 1.0;
        let tr = if takeoff {
            [at(&X26, t), at(&Y26, t), at(&Z26, t)]
        } else {
            [at(&X25, t), at(&Y25, t), at(&Z25, t)]
        };
        m.pos = [-tr[0], 307.0 + tr[1], -tr[2] - if takeoff { (t - 2.0).max(0.0) * 32.0 } else { 0.0 }];
        if m.action_timer == if takeoff { 94.0 } else { 100.0 } {
            if takeoff { advance(m); } else { m.action_state = 1.0; m.action_timer = 0.0; }
        }
    } else {
        let t = m.action_timer;
        m.action_timer += 1.0;
        if t >= 0.0 {
            if let Some(next) = flight().get(t as usize) {
                let (dx, dy, dz) = (next[0] - m.pos[0], next[1] - m.pos[1], next[2] - m.pos[2]);
                let yaw = atan2s(dz, dx) as f64;
                m.face_angle = [
                    -(atan2s(js_hypot(dx, dz), dy) as f64),
                    yaw,
                    to_s16(to_s16(m.face_angle[1] - yaw) as f64 * 20.0) as f64,
                ];
                m.pos = *next;
            }
        }
        if t == 530.0 { m.events.push(MarioEvent::CastleEnding); }
    }
    false
}

pub fn end_peach_cutscene(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    match m.action_arg as i64 {
        0 => { if wing_landing(m, 0) { advance(m); } }
        1 => {
            m.flags |= MARIO_WING_CAP | MARIO_CAP_ON_HEAD;
            stop_and_set_height_to_floor(m);
            if t == 14.0 { m.wing_cap_timer = 60.0; advance(m); }
        }
        2 => { if t == 255.0 { advance(m); } }
        3 => { if t == 355.0 { advance(m); } }
        4 => { if t == 584.0 { advance(m); } }
        5 => {
            m.pos[2] -= 20.0;
            if m.pos[2] <= -1181.0 { m.pos[2] = -1180.0; advance(m); }
            m.pos[1] = m.world.find_floor(m.pos[0], m.pos[1], m.pos[2]).height;
        }
        6 => {
            if t == 8.0 { m.flags = (m.flags & !MARIO_CAP_ON_HEAD) | MARIO_CAP_IN_HAND; }
            if t == 480.0 { advance(m); }
        }
        7 => { if t == 200.0 { advance(m); } }
        8 => { if t == 140.0 { advance(m); } }
        9 => {
            if t == 77.0 { m.flags = (m.flags & !MARIO_CAP_IN_HAND) | MARIO_CAP_ON_HEAD; }
            if t == 142.0 { advance(m); }
        }
        10 => { if t == 350.0 { advance(m); } }
        11 => { if t == 389.0 { advance(m); } }
        12 => { if t == 0.0 { m.events.push(MarioEvent::EndingCredits); } }
        _ => {}
    }
    false
}

/// Action table entries (`ACT_JUMBO_STAR_CUTSCENE`, `ACT_END_PEACH_CUTSCENE`).
pub fn ending_action(m: &mut MarioRuntime) -> Option<bool> {
    Some(match m.action {
        ACT_END_PEACH_CUTSCENE => end_peach_cutscene(m),
        ACT_JUMBO_STAR_CUTSCENE => jumbo_star_cutscene(m),
        _ => return None,
    })
}
