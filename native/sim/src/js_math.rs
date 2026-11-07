//! `Math.sin` / `Math.cos` as V8 computes them (fdlibm: `base::ieee754::sin/cos`), which can differ
//! from the platform libm in the last bit. Arguments beyond 2^19*pi/2 (never produced by the
//! sim's angle math) fall back to the platform functions.
//!
//! The reference Node build fuses `a*b + c` into FMAs (clang's default on arm64), so the
//! polynomial evaluation below uses `mul_add` in the same places to be bit-identical
//! (verified on 18000 samples by `tests/behaviors_golden.rs`). Use these instead of
//! `f64::sin/cos` wherever the TS calls `Math.sin/cos`.

fn hi(x: f64) -> i32 { (x.to_bits() >> 32) as i32 }

const S1: f64 = -1.66666666666666324348e-01;
const S2: f64 = 8.33333333332248946124e-03;
const S3: f64 = -1.98412698298579493134e-04;
const S4: f64 = 2.75573137070700676789e-06;
const S5: f64 = -2.50507602534068634195e-08;
const S6: f64 = 1.58969099521155010221e-10;

const C1: f64 = 4.16666666666666019037e-02;
const C2: f64 = -1.38888888888741095749e-03;
const C3: f64 = 2.48015872894767294178e-05;
const C4: f64 = -2.75573143513906633035e-07;
const C5: f64 = 2.08757232129817482790e-09;
const C6: f64 = -1.13596475577881948265e-11;

fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    let ix = hi(x) & 0x7fffffff;
    if ix < 0x3e400000 && x as i32 == 0 { return x; }
    let z = x * x;
    let v = z * x;
    // The reference build contracts a*b+c into fused multiply-adds (clang -ffp-contract=on on arm64).
    let r = z.mul_add(z.mul_add(z.mul_add(z.mul_add(S6, S5), S4), S3), S2);
    if iy == 0 {
        v.mul_add(z.mul_add(r, S1), x)
    } else {
        let a = 0.5f64.mul_add(y, -(v * r));
        let b = z.mul_add(a, -y);
        x - (-v).mul_add(S1, b)
    }
}

fn kernel_cos(x: f64, y: f64) -> f64 {
    let ix = hi(x) & 0x7fffffff;
    if ix < 0x3e400000 && x as i32 == 0 { return 1.0; }
    let z = x * x;
    let r = z * z.mul_add(z.mul_add(z.mul_add(z.mul_add(z.mul_add(C6, C5), C4), C3), C2), C1);
    if ix < 0x3FD33333 {
        1.0 - (0.5 * z - z.mul_add(r, -(x * y)))
    } else {
        let qx = if ix > 0x3fe90000 { 0.28125 } else { f64::from_bits(((ix - 0x00200000) as u32 as u64) << 32) };
        let hz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (hz - z.mul_add(r, -(x * y)))
    }
}

const NPIO2_HW: [i32; 32] = [
    0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB,
    0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB,
    0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x404147AC, 0x4042106C, 0x4042D97C,
    0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
];
const INVPIO2: f64 = 6.36619772367581382433e-01;
const PIO2_1: f64 = 1.57079632673412561417e+00;
const PIO2_1T: f64 = 6.07710050650619224932e-11;
const PIO2_2: f64 = 6.07710050630396597660e-11;
const PIO2_2T: f64 = 2.02226624879595063154e-21;
const PIO2_3: f64 = 2.02226624871116645580e-21;
const PIO2_3T: f64 = 8.47842766036889956997e-32;

/// `__ieee754_rem_pio2` for |x| <= 2^19*(pi/2). Returns (n, y0, y1).
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    let hx = hi(x);
    let ix = hx & 0x7fffffff;
    if ix <= 0x3fe921fb { return (0, x, 0.0); }
    if ix < 0x4002d97c {
        if hx > 0 {
            let mut z = x - PIO2_1;
            let (y0, y1);
            if ix != 0x3ff921fb {
                y0 = z - PIO2_1T;
                y1 = (z - y0) - PIO2_1T;
            } else {
                z -= PIO2_2;
                y0 = z - PIO2_2T;
                y1 = (z - y0) - PIO2_2T;
            }
            return (1, y0, y1);
        } else {
            let mut z = x + PIO2_1;
            let (y0, y1);
            if ix != 0x3ff921fb {
                y0 = z + PIO2_1T;
                y1 = (z - y0) + PIO2_1T;
            } else {
                z += PIO2_2;
                y0 = z + PIO2_2T;
                y1 = (z - y0) + PIO2_2T;
            }
            return (-1, y0, y1);
        }
    }
    let t = x.abs();
    let n = t.mul_add(INVPIO2, 0.5) as i32;
    let fnn = n as f64;
    let mut r = (-fnn).mul_add(PIO2_1, t);
    let mut w = fnn * PIO2_1T;
    let mut y0;
    if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
        y0 = r - w;
    } else {
        let j = ix >> 20;
        y0 = r - w;
        let mut i = j - ((hi(y0) >> 20) & 0x7ff);
        if i > 16 {
            let t = r;
            w = fnn * PIO2_2;
            r = t - w;
            w = fnn.mul_add(PIO2_2T, -((t - r) - w));
            y0 = r - w;
            i = j - ((hi(y0) >> 20) & 0x7ff);
            if i > 49 {
                let t = r;
                w = fnn * PIO2_3;
                r = t - w;
                w = fnn.mul_add(PIO2_3T, -((t - r) - w));
                y0 = r - w;
            }
        }
    }
    let y1 = (r - y0) - w;
    if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) }
}

fn in_range(ix: i32) -> bool { ix <= 0x413921fb }

/// `Math.sin(x)`.
pub fn js_sin(x: f64) -> f64 {
    let ix = hi(x) & 0x7fffffff;
    if ix <= 0x3fe921fb { return kernel_sin(x, 0.0, 0); }
    if ix >= 0x7ff00000 { return f64::NAN; }
    if !in_range(ix) { return x.sin(); }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1, 1),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, 1),
        _ => -kernel_cos(y0, y1),
    }
}

/// `Math.cos(x)`.
pub fn js_cos(x: f64) -> f64 {
    let ix = hi(x) & 0x7fffffff;
    if ix <= 0x3fe921fb { return kernel_cos(x, 0.0); }
    if ix >= 0x7ff00000 { return f64::NAN; }
    if !in_range(ix) { return x.cos(); }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, 1),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, 1),
    }
}

