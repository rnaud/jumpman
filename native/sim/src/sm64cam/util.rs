//! Vector / angle helpers of `src/game/camera.c` and `src/engine/math_util.c`, f64 versions of
//! the decomp's f32 code. s16 values are `i32` kept in range with [`s16`] / [`w`].
use crate::math::{atan2s, coss, sins, to_s16};

pub type V3 = [f64; 3];

/// `(s16)` cast of a float.
pub fn s16(v: f64) -> i32 { to_s16(v) }
/// `(s16)` cast of an integer sum (wraps like C).
pub fn w(v: i32) -> i32 { to_s16(v as f64) }
/// `DEGREES(x)` of camera.h.
pub fn degrees(x: i32) -> i32 { x * 0x10000 / 360 }

pub fn add(a: &mut V3, b: V3) { for i in 0..3 { a[i] += b[i]; } }
pub fn sub(a: &mut V3, b: V3) { for i in 0..3 { a[i] -= b[i]; } }

/// `vec3f_get_dist_and_angle` (math_util.c): (dist, pitch, yaw).
pub fn dist_and_angle(from: V3, to: V3) -> (f64, i32, i32) {
    let (x, y, z) = (to[0] - from[0], to[1] - from[1], to[2] - from[2]);
    ((x * x + y * y + z * z).sqrt(), atan2s((x * x + z * z).sqrt(), y), atan2s(z, x))
}

/// `vec3f_set_dist_and_angle` (math_util.c): the point `dist` from `from` at `pitch`/`yaw`.
pub fn set_dist_and_angle(from: V3, dist: f64, pitch: i32, yaw: i32) -> V3 {
    [
        from[0] + dist * coss(pitch as f64) * sins(yaw as f64),
        from[1] + dist * sins(pitch as f64),
        from[2] + dist * coss(pitch as f64) * coss(yaw as f64),
    ]
}

/// `calculate_pitch` (camera.c).
pub fn calculate_pitch(from: V3, to: V3) -> i32 {
    let (dx, dy, dz) = (to[0] - from[0], to[1] - from[1], to[2] - from[2]);
    atan2s((dx * dx + dz * dz).sqrt(), dy)
}
/// `calculate_yaw` (camera.c).
pub fn calculate_yaw(from: V3, to: V3) -> i32 { atan2s(to[2] - from[2], to[0] - from[0]) }
/// `calculate_angles` (camera.c): (pitch, yaw).
pub fn calculate_angles(from: V3, to: V3) -> (i32, i32) { (calculate_pitch(from, to), calculate_yaw(from, to)) }
/// `calc_abs_dist` (camera.c).
pub fn abs_dist(a: V3, b: V3) -> f64 {
    let (x, y, z) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    (x * x + y * y + z * z).sqrt()
}
/// `calc_hor_dist` (camera.c).
pub fn hor_dist(a: V3, b: V3) -> f64 {
    let (x, z) = (b[0] - a[0], b[2] - a[2]);
    (x * x + z * z).sqrt()
}

/// `rotate_in_xz` (camera.c).
pub fn rotate_in_xz(src: V3, yaw: i32) -> V3 {
    let (s, c) = (sins(yaw as f64), coss(yaw as f64));
    [src[2] * s + src[0] * c, src[1], src[2] * c - src[0] * s]
}
/// `rotate_in_yz` (camera.c).
pub fn rotate_in_yz(src: V3, pitch: i32) -> V3 {
    let (s, c) = (sins(pitch as f64), coss(pitch as f64));
    [src[0], src[2] * s + src[1] * c, -(src[2] * c - src[1] * s)]
}

/// `offset_rotated` (camera.c): `to` is an offset in the frame rotated by `rotation` (pitch, yaw, roll) around `from`.
pub fn offset_rotated(from: V3, to: V3, rotation: [i32; 3]) -> V3 {
    let (sp, cp) = (sins(rotation[0] as f64), coss(rotation[0] as f64));
    let (sy, cy) = (sins(rotation[1] as f64), coss(rotation[1] as f64));
    let pz = -(to[2] * cp - to[1] * sp);
    let py = to[2] * sp + to[1] * cp;
    let px = to[0];
    [from[0] + pz * sy + px * cy, from[1] + py, from[2] + pz * cy - px * sy]
}

/// `scale_along_line` (camera.c).
pub fn scale_along_line(from: V3, to: V3, scale: f64) -> V3 {
    [(to[0] - from[0]) * scale + from[0], (to[1] - from[1]) * scale + from[1], (to[2] - from[2]) * scale + from[2]]
}

/// `is_pos_in_bounds` (camera.c).
pub fn is_pos_in_bounds(pos: V3, center: V3, bounds: V3, bounds_yaw: i32) -> bool {
    let rel = rotate_in_xz([center[0] - pos[0], center[1] - pos[1], center[2] - pos[2]], bounds_yaw);
    -bounds[0] < rel[0] && rel[0] < bounds[0] && -bounds[1] < rel[1] && rel[1] < bounds[1] && -bounds[2] < rel[2] && rel[2] < bounds[2]
}

/// `approach_f32_asymptotic_bool`: true while not yet at the target.
pub fn approach_f32_asymptotic_bool(cur: &mut f64, target: f64, mut mul: f64) -> bool {
    if mul > 1.0 { mul = 1.0; }
    *cur += (target - *cur) * mul;
    *cur != target
}
/// `approach_f32_asymptotic`.
pub fn approach_f32_asymptotic(cur: f64, target: f64, mul: f64) -> f64 { cur + (target - cur) * mul }

/// `approach_s16_asymptotic_bool`.
pub fn approach_s16_asymptotic_bool(cur: &mut i32, target: i32, divisor: i32) -> bool {
    if divisor == 0 {
        *cur = target;
    } else {
        let mut t = w(*cur - target);
        t = w(t - t / divisor);
        t = w(t + target);
        *cur = t;
    }
    *cur != target
}
/// `approach_s16_asymptotic`.
pub fn approach_s16_asymptotic(cur: i32, target: i32, divisor: i32) -> i32 {
    let mut c = cur;
    approach_s16_asymptotic_bool(&mut c, target, divisor);
    c
}

/// `camera_approach_s16_symmetric_bool`.
pub fn approach_s16_symmetric_bool(cur: &mut i32, target: i32, inc: i32) -> bool {
    let mut dist = w(target - *cur);
    let inc = inc.abs();
    if dist > 0 {
        dist = w(dist - inc);
        *cur = if dist >= 0 { w(target - dist) } else { target };
    } else {
        dist = w(dist + inc);
        *cur = if dist <= 0 { w(target - dist) } else { target };
    }
    *cur != target
}
/// `camera_approach_s16_symmetric`.
pub fn approach_s16_symmetric(cur: i32, target: i32, inc: i32) -> i32 {
    let mut c = cur;
    approach_s16_symmetric_bool(&mut c, target, inc);
    c
}
/// `camera_approach_f32_symmetric_bool`.
pub fn approach_f32_symmetric_bool(cur: &mut f64, target: f64, inc: f64) -> bool {
    let mut dist = target - *cur;
    let inc = inc.abs();
    if dist > 0.0 {
        dist -= inc;
        *cur = if dist > 0.0 { target - dist } else { target };
    } else {
        dist += inc;
        *cur = if dist < 0.0 { target - dist } else { target };
    }
    *cur != target
}
/// `camera_approach_f32_symmetric`.
pub fn approach_f32_symmetric(cur: f64, target: f64, inc: f64) -> f64 {
    let mut c = cur;
    approach_f32_symmetric_bool(&mut c, target, inc);
    c
}
/// `approach_f32` of math_util.c (separate up / down speeds).
pub fn approach_f32(cur: f64, target: f64, inc: f64, dec: f64) -> f64 {
    if cur < target { (cur + inc).min(target) } else { (cur - dec).max(target) }
}

/// `calc_avoid_yaw` (camera.c).
pub fn calc_avoid_yaw(yaw_from_mario: i32, wall_yaw: i32) -> i32 {
    let diff = w(wall_yaw - yaw_from_mario + degrees(90));
    if diff < 0 { wall_yaw } else { w(wall_yaw + degrees(180)) }
}

/// `evaluate_cubic_spline` (camera.c): uniform cubic B-spline point at `u` through four controls.
pub fn evaluate_cubic_spline(u: f64, a0: V3, a1: V3, a2: V3, a3: V3) -> V3 {
    let u = if u > 1.0 { 1.0 } else { u };
    let b = [
        (1.0 - u) * (1.0 - u) * (1.0 - u) / 6.0,
        u * u * u / 2.0 - u * u + 0.6666667,
        -u * u * u / 2.0 + u * u / 2.0 + u / 2.0 + 0.16666667,
        u * u * u / 6.0,
    ];
    let mut q = [0.0; 3];
    for i in 0..3 {
        q[i] = b[0] * a0[i] + b[1] * a1[i] + b[2] * a2[i] + b[3] * a3[i];
    }
    q
}
