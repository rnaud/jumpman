//! SM64 math conventions: s16 angles (65536 = one turn), yaw 0 faces +Z.
use std::sync::OnceLock;

pub type Vec3 = [f64; 3];

pub const TAU: f64 = std::f64::consts::PI * 2.0;
pub const ANGLE_TO_RAD: f64 = std::f64::consts::PI / 32768.0;
pub const RAD_TO_ANGLE: f64 = 32768.0 / std::f64::consts::PI;

/// Wrap into the signed 16-bit range like a C `s16` cast (JS `& 0xffff` semantics).
pub fn to_s16(value: f64) -> i32 {
    let v = (value.trunc() as i64 as i32) & 0xffff;
    if v >= 0x8000 { v - 0x10000 } else { v }
}

pub fn to_u16(value: f64) -> i32 {
    (value.trunc() as i64 as i32) & 0xffff
}

struct Tables {
    sine: Vec<f32>,
    arctan: Vec<u16>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| Tables {
        sine: (0..4096).map(|i| if i == 2048 { 0.0 } else { (i as f64 * TAU / 4096.0).sin() as f32 }).collect(),
        arctan: (0..1025).map(|i| ((i as f64 / 1024.0).atan() * RAD_TO_ANGLE).round() as u16).collect(),
    })
}

pub fn sins(angle: f64) -> f64 {
    tables().sine[(to_u16(angle) >> 4) as usize] as f64
}

pub fn coss(angle: f64) -> f64 {
    tables().sine[(((to_u16(angle) >> 4) + 1024) & 4095) as usize] as f64
}

fn fround(x: f64) -> f64 {
    x as f32 as f64
}

/// Octant lookup from math_util.c (`atan2s(z, x)`; yaw 0 faces +Z).
pub fn atan2s(z: f64, x: f64) -> i32 {
    let z = fround(z);
    let x = fround(x);
    let (az, ax) = (z.abs(), x.abs());
    let longer = az.max(ax);
    if longer == 0.0 {
        return 0;
    }
    let ratio = fround(az.min(ax) / longer);
    let index = fround(fround(ratio * 1024.0) + 0.5).trunc() as usize;
    let mut angle = tables().arctan[index] as i32;
    if ax > az {
        angle = 0x4000 - angle;
    }
    if z < 0.0 {
        angle = 0x8000 - angle;
    }
    to_s16(if x < 0.0 { -angle } else { angle } as f64)
}

/// `Math.sin` / `Math.cos` as V8 computes them (fdlibm). Rust's `f64::sin` calls the platform
/// libm, which can differ in the last bit; use these wherever the TS calls `Math.sin/cos`.
pub fn js_sin(x: f64) -> f64 { libm::sin(x) }
pub fn js_cos(x: f64) -> f64 { libm::cos(x) }

pub fn angle_to_radians(angle: f64) -> f64 { to_s16(angle) as f64 * ANGLE_TO_RAD }
pub fn radians_to_angle(radians: f64) -> i32 { to_s16(js_round(radians * RAD_TO_ANGLE)) }

/// JS `Math.round`: ties toward +Infinity.
pub fn js_round(x: f64) -> f64 { (x + 0.5).floor() }

pub fn approach_s32(mut current: f64, target: f64, inc: f64, dec: f64) -> f64 {
    if current < target {
        current += inc;
        if current > target { current = target; }
    } else {
        current -= dec;
        if current < target { current = target; }
    }
    current
}

pub fn approach_f32(current: f64, target: f64, inc: f64, dec: f64) -> f64 {
    approach_s32(current, target, inc, dec)
}

pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min { min } else if value > max { max } else { value }
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 { a + (b - a) * t }

pub fn dist_2d(x1: f64, z1: f64, x2: f64, z2: f64) -> f64 {
    let (dx, dz) = (x2 - x1, z2 - z1);
    (dx * dx + dz * dz).sqrt()
}

pub fn dist_3d(a: &Vec3, b: &Vec3) -> f64 {
    let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

pub fn deg_to_angle(deg: f64) -> i32 { to_s16(js_round(deg / 360.0 * 65536.0)) }
pub fn angle_delta(a: f64, b: f64) -> i32 { to_s16(a - b) }
pub fn angle_lerp(a: f64, b: f64, t: f64) -> i32 { to_s16(a + angle_delta(b, a) as f64 * t) }
pub fn wrap01(t: f64) -> f64 { t - t.floor() }

/// `Math.hypot(a, b, c)` as V8 computes it (same scaled Kahan summation as [`js_hypot`]).
pub fn js_hypot3(a: f64, b: f64, c: f64) -> f64 {
    let (a, b, c) = (a.abs(), b.abs(), c.abs());
    if a.is_infinite() || b.is_infinite() || c.is_infinite() { return f64::INFINITY; }
    if a.is_nan() || b.is_nan() || c.is_nan() { return f64::NAN; }
    let max = a.max(b).max(c);
    if max == 0.0 { return 0.0; }
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for v in [a, b, c] {
        let n = v / max;
        let summand = n * n - comp;
        let prelim = sum + summand;
        comp = (prelim - sum) - summand;
        sum = prelim;
    }
    sum.sqrt() * max
}

thread_local! { static RANDOM_STATE: std::cell::Cell<u32> = const { std::cell::Cell::new(1) }; }

/// Seed the [`random`] generator (the fixture generators use the same LCG in TypeScript).
pub fn seed_random(seed: u32) { RANDOM_STATE.with(|s| s.set(seed)); }

/// `Math.random()` stand-in: the 32-bit LCG used by the fixture generators
/// (`s = imul(s, 1664525) + 1013904223`, thread-local). The game itself uses the engine RNG,
/// so behaviours that call it are only comparable against fixtures generated with a seeded
/// `Math.random`.
pub fn random() -> f64 {
    RANDOM_STATE.with(|s| {
        let x = s.get().wrapping_mul(1664525).wrapping_add(1013904223);
        s.set(x);
        x as f64 / 4294967296.0
    })
}

/// `Math.hypot(a, b)` as V8 computes it (scale by the max, Kahan-summed squares), which can
/// differ from libm's `hypot` in the last bit.
pub fn js_hypot(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a.is_infinite() || b.is_infinite() { return f64::INFINITY; }
    if a.is_nan() || b.is_nan() { return f64::NAN; }
    let max = a.max(b);
    if max == 0.0 { return 0.0; }
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for v in [a, b] {
        let n = v / max;
        let summand = n * n - comp;
        let prelim = sum + summand;
        comp = (prelim - sum) - summand;
        sum = prelim;
    }
    sum.sqrt() * max
}
