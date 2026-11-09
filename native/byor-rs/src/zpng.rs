//! Shared helpers of the Python exporters' own PNG / numeric conventions that the MK64 and BK ports use:
//! `zlib.compress`-based RGBA PNG writer (byte-identical with flate2's zlib feature at the same level) and a
//! Python-style numeric type that remembers whether a value is an `int` or a `float` (JSON prints them differently).
use serde_json::{json, Value};
use std::io::Write;

fn crc_table() -> &'static [u32; 256] {
    static T: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (n, e) in t.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *e = c;
        }
        t
    })
}

/// `zlib.crc32`.
pub fn crc32(data: &[u8]) -> u32 {
    let t = crc_table();
    !data.iter().fold(!0u32, |c, &b| t[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8))
}

/// `zlib.compress(data, level)`.
pub fn zlib(data: &[u8], level: u32) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

/// The exporters' `png(path, w, h, rgba)` / `png_bytes`: filter 0 on every row, one IDAT, `zlib.compress(rows[, level])`.
pub fn png_rgba(width: usize, height: usize, pixels: &[u8], level: u32) -> Result<Vec<u8>, String> {
    if pixels.len() != width * height * 4 {
        return Err(format!("png: {} bytes for {width}x{height}", pixels.len()));
    }
    let chunk = |kind: &[u8; 4], value: &[u8]| {
        let mut c = (value.len() as u32).to_be_bytes().to_vec();
        let mut body = kind.to_vec();
        body.extend_from_slice(value);
        c.extend_from_slice(&body);
        c.extend_from_slice(&crc32(&body).to_be_bytes());
        c
    };
    let mut rows = Vec::with_capacity(height * (width * 4 + 1));
    for y in 0..height {
        rows.push(0);
        rows.extend_from_slice(&pixels[y * width * 4..(y + 1) * width * 4]);
    }
    let mut ihdr = (width as u32).to_be_bytes().to_vec();
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    out.extend(chunk(b"IHDR", &ihdr));
    out.extend(chunk(b"IDAT", &zlib(&rows, level)));
    out.extend(chunk(b"IEND", b""));
    Ok(out)
}

/// A Python number: `int` or `float`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    I(i64),
    F(f64),
}

impl Num {
    pub fn f(self) -> f64 {
        match self {
            Num::I(i) => i as f64,
            Num::F(f) => f,
        }
    }
    pub fn json(self) -> Value {
        match self {
            Num::I(i) => json!(i),
            Num::F(f) => json!(f),
        }
    }
    /// `round(x, n)`: an int stays an int, a float is rounded correctly (half to even on the exact binary value).
    pub fn round_to(self, n: usize) -> Num {
        match self {
            Num::I(_) => self,
            Num::F(f) => Num::F(round_f(f, n)),
        }
    }
}

/// `round(x, n)` for a float.
pub fn round_f(x: f64, n: usize) -> f64 {
    format!("{x:.n$}").parse::<f64>().unwrap_or(x)
}
