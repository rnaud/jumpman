//! `Math.sin` / `Math.cos` exactly as V8 computes them (its `ieee754::sin/cos`, a port of fdlibm).
//!
//! Closest available match to node's `Math.sin/cos` (V8 13.x agrees with this fdlibm port for
//! ~99.2% of s16 angles; std/libm agree for 96-98%; the remaining last-bit differences come from
//! V8's newer trig implementation, which is not ported). Only `|x| <= 2^19 * pi/2` is ported
//! (angles here are within +-pi); larger arguments use `libm`.

fn d(hi: u32, lo: u32) -> f64 { f64::from_bits(((hi as u64) << 32) | lo as u64) }
fn high(x: f64) -> u32 { (x.to_bits() >> 32) as u32 }

fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    let s1 = d(0xBFC55555, 0x55555549);
    let s2 = d(0x3F811111, 0x1110F8A6);
    let s3 = d(0xBF2A01A0, 0x19C161D5);
    let s4 = d(0x3EC71DE3, 0x57B1FE7D);
    let s5 = d(0xBE5AE5E6, 0x8A2B9CEB);
    let s6 = d(0x3DE5D93A, 0x5ACFD57C);
    let ix = high(x) & 0x7fffffff;
    if ix < 0x3e400000 && (x as i32) == 0 { return x; }
    let z = x * x;
    let v = z * x;
    let r = s2 + z * (s3 + z * (s4 + z * (s5 + z * s6)));
    if iy == 0 { x + v * (s1 + z * r) } else { x - ((z * (0.5 * y - v * r) - y) - v * s1) }
}

fn kernel_cos(x: f64, y: f64) -> f64 {
    let c1 = d(0x3FA55555, 0x5555554C);
    let c2 = d(0xBF56C16C, 0x16C15177);
    let c3 = d(0x3EFA01A0, 0x19CB1590);
    let c4 = d(0xBE927E4F, 0x809C52AD);
    let c5 = d(0x3E21EE9E, 0xBDB4B1C4);
    let c6 = d(0xBDA8FAE9, 0xBE8838D4);
    let ix = high(x) & 0x7fffffff;
    if ix < 0x3e400000 && (x as i32) == 0 { return 1.0; }
    let z = x * x;
    let r = z * (c1 + z * (c2 + z * (c3 + z * (c4 + z * (c5 + z * c6)))));
    if ix < 0x3FD33333 {
        1.0 - (0.5 * z - (z * r - x * y))
    } else {
        let qx = if ix > 0x3fe90000 { 0.28125 } else { f64::from_bits(((ix - 0x00200000) as u64) << 32) };
        let hz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (hz - (z * r - x * y))
    }
}

/// `__ieee754_rem_pio2` for |x| <= 2^19 * pi/2. Returns (n, y0, y1).
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    let invpio2 = d(0x3FE45F30, 0x6DC9C883);
    let pio2_1 = d(0x3FF921FB, 0x54400000);
    let pio2_1t = d(0x3DD0B461, 0x1A626331);
    let pio2_2 = d(0x3DD0B461, 0x1A600000);
    let pio2_2t = d(0x3BA3198A, 0x2E037073);
    let pio2_3 = d(0x3BA3198A, 0x2E000000);
    let pio2_3t = d(0x397B839A, 0x252049C1);
    const NPIO2_HW: [u32; 32] = [
        0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB,
        0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB,
        0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x404147AC, 0x4042106C, 0x4042D97C,
        0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
    ];
    let hx = high(x);
    let ix = hx & 0x7fffffff;
    let neg = (hx as i32) < 0;
    if ix <= 0x3fe921fb { return (0, x, 0.0); }
    if ix < 0x4002d97c {
        // |x| < 3pi/4, special case with n = +-1
        if !neg {
            let mut z = x - pio2_1;
            let (y0, y1);
            if ix != 0x3ff921fb {
                y0 = z - pio2_1t;
                y1 = (z - y0) - pio2_1t;
            } else {
                z -= pio2_2;
                y0 = z - pio2_2t;
                y1 = (z - y0) - pio2_2t;
            }
            return (1, y0, y1);
        } else {
            let mut z = x + pio2_1;
            let (y0, y1);
            if ix != 0x3ff921fb {
                y0 = z + pio2_1t;
                y1 = (z - y0) + pio2_1t;
            } else {
                z += pio2_2;
                y0 = z + pio2_2t;
                y1 = (z - y0) + pio2_2t;
            }
            return (-1, y0, y1);
        }
    }
    // medium size
    let t = x.abs();
    let n = (t * invpio2 + 0.5) as i32;
    let fnn = n as f64;
    let mut r = t - fnn * pio2_1;
    let mut w = fnn * pio2_1t;
    let mut y0;
    if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
        y0 = r - w;
    } else {
        let j = (ix >> 20) as i32;
        y0 = r - w;
        let mut i = j - ((high(y0) >> 20) & 0x7ff) as i32;
        if i > 16 {
            let t2 = r;
            w = fnn * pio2_2;
            r = t2 - w;
            w = fnn * pio2_2t - ((t2 - r) - w);
            y0 = r - w;
            i = j - ((high(y0) >> 20) & 0x7ff) as i32;
            if i > 49 {
                let t3 = r;
                w = fnn * pio2_3;
                r = t3 - w;
                w = fnn * pio2_3t - ((t3 - r) - w);
                y0 = r - w;
            }
        }
    }
    let y1 = (r - y0) - w;
    if neg { (-n, -y0, -y1) } else { (n, y0, y1) }
}

fn in_range(x: f64) -> bool { (high(x) & 0x7fffffff) <= 0x413921fb }

pub fn sin(x: f64) -> f64 {
    let ix = high(x) & 0x7fffffff;
    if ix <= 0x3fe921fb { return kernel_sin(x, 0.0, 0); }
    if !in_range(x) { return libm::sin(x); }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1, 1),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, 1),
        _ => -kernel_cos(y0, y1),
    }
}

pub fn cos(x: f64) -> f64 {
    let ix = high(x) & 0x7fffffff;
    if ix <= 0x3fe921fb { return kernel_cos(x, 0.0); }
    if !in_range(x) { return libm::cos(x); }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, 1),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_v8_samples() {
        // Values from node: Math.sin/cos(-22801 / 32768 * Math.PI) and a +-3pi/4 neighbour.
        let a = -22801.0f64 / 32768.0 * std::f64::consts::PI;
        assert_eq!(sin(a), -0.8166452440184702);
        assert_eq!(cos(a), -0.5771399703902107);
        let b = 24576.0f64 / 32768.0 * std::f64::consts::PI;
        assert_eq!(sin(b), 0.7071067811865476);
        assert_eq!(cos(b), -0.7071067811865475);
    }

    /// Current V8 (13.x) is not plain fdlibm: it differs in the last bit for under 1% of angles
    /// (the platform libm would be 2-4%). Keep the rate from regressing.
    #[test]
    fn mostly_matches_node() {
        let raw = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/trig_v8.json")).unwrap();
        let v: Vec<Vec<f64>> = serde_json::from_slice(&raw).unwrap();
        let mut bad = 0;
        for r in &v {
            let x = r[0] / 32768.0 * std::f64::consts::PI;
            if sin(x) != r[1] { bad += 1; }
            if cos(x) != r[2] { bad += 1; }
        }
        assert!(bad * 100 < v.len() * 2, "{bad} of {} differ", v.len() * 2);
    }
}
