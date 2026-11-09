//! MK64 texture decoders and ROM helpers shared by the course, kart, HUD and music exporters
//! (`mio0`, `rgba16`, `decode` of `scripts/export-mk64.py`).
use crate::n64;
use crate::pyc::R;
use std::collections::HashMap;
use std::rc::Rc;

/// `mio0(data)` of the Python exporter: returns the data unchanged when it has no MIO0 header.
pub fn mio0(rom: &[u8], start: usize) -> R<Vec<u8>> {
    let tail = rom.get(start..).ok_or_else(|| format!("offset {start:#x} outside the ROM"))?;
    if tail.get(..4) != Some(b"MIO0") {
        return Ok(tail.to_vec());
    }
    n64::mio0_decode(rom, start)
}

/// `block(start)` caches of decompressed MIO0 blocks.
pub struct Blocks<'a> {
    pub rom: &'a [u8],
    map: HashMap<usize, Rc<Vec<u8>>>,
    /// bytes held by `map`
    held: usize,
}

/// Decompressed MIO0 blocks are cached (palettes and textures are asked for again and again), but only up to this many bytes:
/// the kart frames alone touch thousands of distinct blocks and keeping them all peaked at 15 GB (iOS kills an app near 1-2 GB).
const BLOCK_CACHE_LIMIT: usize = 48 << 20;

impl<'a> Blocks<'a> {
    pub fn new(rom: &'a [u8]) -> Self {
        Blocks { rom, map: HashMap::new(), held: 0 }
    }
    pub fn block(&mut self, start: usize) -> R<Rc<Vec<u8>>> {
        if let Some(b) = self.map.get(&start) {
            return Ok(b.clone());
        }
        let b = Rc::new(mio0(self.rom, start)?);
        if self.held + b.len() > BLOCK_CACHE_LIMIT {
            self.map.clear();
            self.held = 0;
        }
        self.held += b.len();
        self.map.insert(start, b.clone());
        Ok(b)
    }
}

/// Python slice `data[a:b]` (clamped).
pub fn sl(data: &[u8], a: usize, b: usize) -> &[u8] {
    let a = a.min(data.len());
    &data[a..b.min(data.len()).max(a)]
}

pub fn rgba16(data: &[u8]) -> R<Vec<u8>> {
    if data.len() % 2 != 0 {
        return Err("rgba16: odd length".into());
    }
    let mut out = Vec::with_capacity(data.len() * 2);
    for c in data.chunks_exact(2) {
        let p = u16::from_be_bytes([c[0], c[1]]) as u32;
        out.extend_from_slice(&[((p >> 11) * 255 / 31) as u8, (((p >> 6) & 31) * 255 / 31) as u8, (((p >> 1) & 31) * 255 / 31) as u8, ((p & 1) * 255) as u8]);
    }
    Ok(out)
}

/// `decode(kind, data, w, h, palette)`: N64 texture formats to RGBA bytes (I4/I8/IA4 become white with the intensity as alpha).
pub fn decode(kind: &str, data: &[u8], w: usize, h: usize, palette: Option<&[u8]>) -> R<Vec<u8>> {
    let mut out = Vec::new();
    match kind {
        "rgba16" => return rgba16(sl(data, 0, w * h * 2)),
        "ci8" => {
            let pal = palette.ok_or("ci8 texture without a palette")?;
            for &i in sl(data, 0, w * h) {
                out.extend_from_slice(sl(pal, i as usize * 4, i as usize * 4 + 4));
            }
        }
        "i8" => {
            for &v in sl(data, 0, w * h) {
                out.extend_from_slice(&[255, 255, 255, v]);
            }
        }
        "i4" => {
            for &b in sl(data, 0, w * h / 2) {
                for v in [b >> 4, b & 15] {
                    out.extend_from_slice(&[255, 255, 255, v * 17]);
                }
            }
        }
        "ia4" => {
            for &b in sl(data, 0, w * h / 2) {
                for v in [b >> 4, b & 15] {
                    out.extend_from_slice(&[255, 255, 255, if v & 1 != 0 { 255 } else { 0 }]);
                }
            }
        }
        "ia16" => {
            let mut i = 0;
            while i < w * h * 2 && i + 1 < data.len() {
                out.extend_from_slice(&[data[i], data[i], data[i], data[i + 1]]);
                i += 2;
            }
        }
        other => return Err(format!("unsupported texture format {other}")),
    }
    Ok(out)
}
