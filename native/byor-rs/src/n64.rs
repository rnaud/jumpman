//! Shared low-level N64 pieces: MIO0 decompression and the texture decoders of the decomp's `n64graphics`
//! (RGBA16/32, IA16/8/4/1, I8/4 raw texels to 8-bit PNG).
use crate::png;

/// libmio0 `mio0_decode` on the block that starts at `data[offset]`.
pub fn mio0_decode(data: &[u8], offset: usize) -> Result<Vec<u8>, String> {
    let b = data.get(offset..).ok_or("mio0 offset out of range")?;
    if b.len() < 16 || &b[..4] != b"MIO0" {
        return Err(format!("no MIO0 header at 0x{offset:x}"));
    }
    let be = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let (dest, comp, uncomp) = (be(4), be(8), be(12));
    let mut out: Vec<u8> = Vec::with_capacity(dest);
    let (mut bit, mut ci, mut ui) = (0usize, 0usize, 0usize);
    let bad = || "truncated MIO0 block".to_string();
    while out.len() < dest {
        let flag = b.get(16 + bit / 8).ok_or_else(bad)? & (1 << (7 - bit % 8));
        if flag != 0 {
            out.push(*b.get(uncomp + ui).ok_or_else(bad)?);
            ui += 1;
        } else {
            let (v0, v1) = (*b.get(comp + ci).ok_or_else(bad)?, *b.get(comp + ci + 1).ok_or_else(bad)?);
            ci += 2;
            let len = ((v0 & 0xF0) >> 4) as usize + 3;
            let idx = (((v0 & 0x0F) as usize) << 8) + v1 as usize + 1;
            for _ in 0..len {
                let c = *out.get(out.len().wrapping_sub(idx)).ok_or_else(bad)?;
                out.push(c);
            }
        }
        bit += 1;
    }
    Ok(out)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TexFormat {
    Rgba16,
    Rgba32,
    Ia16,
    Ia8,
    Ia4,
    Ia1,
    I8,
    I4,
}

impl TexFormat {
    /// The format suffix of a decomp texture name (`foo.rgba16.png`).
    pub fn parse(s: &str) -> Option<TexFormat> {
        Some(match s {
            "rgba16" => Self::Rgba16,
            "rgba32" => Self::Rgba32,
            "ia16" => Self::Ia16,
            "ia8" => Self::Ia8,
            "ia4" => Self::Ia4,
            "ia1" => Self::Ia1,
            "i8" => Self::I8,
            "i4" => Self::I4,
            _ => return None,
        })
    }
    pub fn depth(self) -> usize {
        match self {
            Self::Rgba16 | Self::Ia16 => 16,
            Self::Rgba32 => 32,
            Self::Ia8 | Self::I8 => 8,
            Self::Ia4 | Self::I4 => 4,
            Self::Ia1 => 1,
        }
    }
}

const fn scale_5_8(v: u32) -> u8 {
    ((v * 0xFF) / 0x1F) as u8
}

/// Raw texels to the pixel buffer `stbi_write_png` gets (4 components for RGBA, 2 for IA/I), then PNG-encoded.
/// `raw` shorter than the image needs is zero padded (n64graphics reads into an uninitialised buffer; never happens
/// with the sizes in assets.json).
pub fn texture_png(raw: &[u8], w: usize, h: usize, fmt: TexFormat) -> Vec<u8> {
    let n = w * h;
    let need = (n * fmt.depth()).div_ceil(8);
    let mut padded;
    let raw = if raw.len() < need {
        padded = raw.to_vec();
        padded.resize(need, 0);
        &padded[..]
    } else {
        raw
    };
    match fmt {
        TexFormat::Rgba16 | TexFormat::Rgba32 => {
            let mut px = vec![0u8; n * 4];
            for i in 0..n {
                let o = &mut px[i * 4..i * 4 + 4];
                if fmt == TexFormat::Rgba16 {
                    let (a, b) = (raw[i * 2] as u32, raw[i * 2 + 1] as u32);
                    o[0] = scale_5_8((a & 0xF8) >> 3);
                    o[1] = scale_5_8(((a & 0x07) << 2) | ((b & 0xC0) >> 6));
                    o[2] = scale_5_8((b & 0x3E) >> 1);
                    o[3] = if b & 1 != 0 { 0xFF } else { 0 };
                } else {
                    o.copy_from_slice(&raw[i * 4..i * 4 + 4]);
                }
            }
            png::write_png(&px, w, h, 4)
        }
        _ => {
            let mut px = vec![0u8; n * 2];
            for i in 0..n {
                let nib = |raw: &[u8]| if i % 2 == 1 { raw[i / 2] & 0xF } else { raw[i / 2] >> 4 };
                let (inten, alpha) = match fmt {
                    TexFormat::Ia16 => (raw[i * 2], raw[i * 2 + 1]),
                    TexFormat::Ia8 => (((raw[i] & 0xF0) >> 4) * 0x11, (raw[i] & 0x0F) * 0x11),
                    TexFormat::Ia4 => {
                        let bits = nib(raw);
                        (((bits >> 1) & 7) * 0x24, if bits & 1 != 0 { 0xFF } else { 0 })
                    }
                    TexFormat::Ia1 => {
                        let v = if raw[i / 8] & (1 << (7 - i % 8)) != 0 { 0xFF } else { 0 };
                        (v, v)
                    }
                    TexFormat::I8 => (raw[i], 0xFF),
                    _ => {
                        let v = nib(raw) * 0x11; // I4: alpha copies intensity
                        (v, v)
                    }
                };
                px[i * 2] = inten;
                px[i * 2 + 1] = alpha;
            }
            png::write_png(&px, w, h, 2)
        }
    }
}
