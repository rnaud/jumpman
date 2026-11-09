//! Port of `scripts/export-character-voices.py` (OoT half): Link's and Epona's voice clips from the Audiobank / Audiotable.
//! `vadpcm`, `write_wav`, `stats` and `emit` are shared building blocks (the Banjo-Kazooie half can reuse them).
use super::{f32_, i32_, round_to, u32_, u8_, Ctx};
use crate::pyc::{dumps, round, R};
use serde_json::{json, Map, Value};
use std::path::Path;

pub const OOT_SFX_COUNT: usize = 136;

/// NA_SE_VO_LI_<name> -> Soundfont_0 sound-effect indices.
pub const VO_LI_ARRAYS: [(&str, &[(&str, &[usize])]); 2] = [
    (
        "link",
        &[("AUTO_JUMP", &[26, 27]), ("SWORD_N", &[0, 1, 2, 3]), ("SWORD_L", &[4, 5]), ("CLIMB_END", &[7, 8]), ("DAMAGE_S", &[9, 10, 11]), ("FALL_S", &[17, 18]), ("FALL_L", &[15, 16]), ("LAND_DAMAGE_S", &[24])],
    ),
    (
        "young_link",
        &[("AUTO_JUMP", &[53, 54]), ("SWORD_N", &[28, 29, 30, 31]), ("SWORD_L", &[32, 33]), ("CLIMB_END", &[35, 36]), ("DAMAGE_S", &[37, 38, 39]), ("FALL_S", &[45, 46]), ("FALL_L", &[43, 44]), ("LAND_DAMAGE_S", &[49])],
    ),
];

/// Decode N64 VADPCM; `book` is the AdpcmBook struct (order, npredictors, s16 table).
pub fn vadpcm(book: &[u8], data: &[u8]) -> R<Vec<i32>> {
    let (order, npred) = (i32_(book, 0), i32_(book, 4));
    if order != 2 {
        return Err(format!("unsupported VADPCM order {order}"));
    }
    let n = 8 * order as usize * npred as usize;
    let table: Vec<i64> = (0..n).map(|i| super::i16_(book, 8 + 2 * i) as i64).collect();
    let books: Vec<[[i64; 8]; 2]> = (0..npred as usize)
        .map(|p| {
            let mut b = [[0i64; 8]; 2];
            for (o, row) in b.iter_mut().enumerate() {
                row.copy_from_slice(&table[p * 16 + o * 8..p * 16 + o * 8 + 8]);
            }
            b
        })
        .collect();
    let mut out = Vec::new();
    let (mut h2, mut h1) = (0i64, 0i64);
    let mut off = 0;
    while off + 8 < data.len() {
        let scale = (data[off] >> 4) as u32;
        let pred = (data[off] & 15) as usize;
        let [t0, t1] = books.get(pred).ok_or("VADPCM predictor out of range")?;
        let mut res = [0i64; 16];
        for (k, b) in data[off + 1..off + 9].iter().enumerate() {
            for (j, n) in [(b >> 4) as i64, (b & 15) as i64].into_iter().enumerate() {
                res[2 * k + j] = (if n >= 8 { n - 16 } else { n }) << scale;
            }
        }
        for half in [0usize, 8] {
            let e = &res[half..half + 8];
            let mut y = [0i64; 8];
            for i in 0..8 {
                let mut acc = (e[i] << 11) + h2 * t0[i] + h1 * t1[i];
                for j in 0..i {
                    acc += e[j] * t1[i - 1 - j];
                }
                y[i] = (acc >> 11).clamp(-32768, 32767);
            }
            out.extend(y.iter().map(|&v| v as i32));
            h2 = y[6];
            h1 = y[7];
        }
        off += 9;
    }
    Ok(out)
}

/// Python's `wave` module output for mono 16-bit PCM.
pub fn wav_bytes(pcm: &[i32], rate: u32) -> Vec<u8> {
    let n = pcm.len() * 2;
    let mut w = Vec::with_capacity(44 + n);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + n as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(n as u32).to_le_bytes());
    for &s in pcm {
        w.extend_from_slice(&(s as i16).to_le_bytes());
    }
    w
}

pub fn write_wav(path: &Path, pcm: &[i32], rate: u32) -> R<()> {
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(path, wav_bytes(pcm, rate)).map_err(|e| format!("{}: {e}", path.display()))
}

/// `(peak, rms)` with `rms` rounded to one decimal.
pub fn stats(pcm: &[i32]) -> (i64, f64) {
    let peak = pcm.iter().map(|&x| (x as i64).abs()).max().unwrap_or(0);
    let sum: i64 = pcm.iter().map(|&x| x as i64 * x as i64).sum();
    let rms = (sum as f64 / pcm.len() as f64).powf(0.5);
    (peak, round_to(rms, 1))
}

/// Writes `<out>/<group>/<name>_<n>.wav` for each variant and records it in `manifest` (key `voices/<group>/<name>_<n>.wav`).
pub fn emit(out: &Path, group: &str, name: &str, variants: &[(Vec<i32>, u32, String)], manifest: &mut Map<String, Value>) -> R<()> {
    for (n, (pcm, rate, source)) in variants.iter().enumerate() {
        let rel = format!("voices/{group}/{name}_{n}.wav");
        write_wav(&out.parent().unwrap().join(&rel), pcm, *rate)?;
        let (peak, rms) = stats(pcm);
        manifest.insert(rel, json!({"sampleRate": rate, "frames": pcm.len(), "duration": round_to(pcm.len() as f64 / *rate as f64, 4), "peak": peak, "rms": rms, "source": source}));
    }
    Ok(())
}

/// Audiobank (dmadata entry 3), Audiotable (5), as the voices / sfx scripts read them straight from the ROM.
pub fn audio_seg(ctx: &Ctx, i: usize, size: usize) -> R<Vec<u8>> {
    let p = 0x7960 + 16 * i;
    let (vstart, vend, pstart, pend) = (u32_(&ctx.rom, p), u32_(&ctx.rom, p + 4), u32_(&ctx.rom, p + 8) as usize, u32_(&ctx.rom, p + 12));
    if pend != 0 || (vend - vstart) as usize != size {
        return Err(format!("unexpected dmadata entry {i}"));
    }
    Ok(ctx.rom[pstart..pstart + size].to_vec())
}

fn sorted_manifest(m: Map<String, Value>) -> Value {
    let mut keys: Vec<_> = m.keys().cloned().collect();
    keys.sort();
    let mut out = Map::new();
    for k in keys {
        let v = m[&k].clone();
        out.insert(k, v);
    }
    Value::Object(out)
}

pub fn export_oot(ctx: &Ctx, out: &Path) -> R<()> {
    let bank = audio_seg(ctx, 3, 179648)?;
    let table = audio_seg(ctx, 5, 4526992)?;
    let font = &bank[0..0x3AA0];
    let sfx_list = u32_(font, 4) as usize;
    let book_of = |book: usize| -> R<&[u8]> {
        let (order, npred) = (i32_(font, book), i32_(font, book + 4));
        Ok(&font[book..book + 8 + 16 * order as usize * npred as usize])
    };
    let effect = |i: usize| -> R<(Vec<i32>, u32, String)> {
        if i >= OOT_SFX_COUNT {
            return Err(format!("sfx index {i}"));
        }
        let (sample, tuning) = (u32_(font, sfx_list + 8 * i) as usize, f32_(font, sfx_list + 8 * i + 4));
        let (bits, addr, lp, book) = (u32_(font, sample), u32_(font, sample + 4) as usize, u32_(font, sample + 8) as usize, u32_(font, sample + 12) as usize);
        let (codec, size) = (bits >> 28, (bits & 0xFF_FFFF) as usize);
        if codec != 0 || u32_(font, lp + 8) != 0 {
            return Err(format!("effect {i}: not a one-shot ADPCM sample"));
        }
        let pcm = vadpcm(book_of(book)?, &table[addr..addr + size])?;
        Ok((pcm, round(tuning as f64 * 32000.0) as u32, format!("OoT Soundfont_0 SF0_EFFECT_{i}")))
    };
    let mut manifest = Map::new();
    for (group, names) in VO_LI_ARRAYS {
        for (name, idx) in names {
            let v: R<Vec<_>> = idx.iter().map(|&i| effect(i)).collect();
            emit(out, group, name, &v?, &mut manifest)?;
        }
    }
    let instrument = |i: usize, key: i32, max_seconds: f64| -> R<(Vec<i32>, u32)> {
        let inst = u32_(font, 8 + 4 * i) as usize;
        let (lo, hi) = (u8_(font, inst + 1) as i32, u8_(font, inst + 2) as i32);
        let slot = if key < lo {
            8
        } else if key > hi {
            24
        } else {
            16
        };
        let (sample, tuning) = (u32_(font, inst + slot) as usize, f32_(font, inst + slot + 4));
        let (bits, addr, _lp, book) = (u32_(font, sample), u32_(font, sample + 4) as usize, 0, u32_(font, sample + 12) as usize);
        let (codec, size) = (bits >> 28, (bits & 0xFF_FFFF) as usize);
        if codec != 0 {
            return Err(format!("instrument {i}: not ADPCM"));
        }
        let pcm = vadpcm(book_of(book)?, &table[addr..addr + size])?;
        let rate = round(tuning as f64 * 32000.0 * 2f64.powf((key - 39) as f64 / 12.0)) as u32;
        let n = ((max_seconds * rate as f64) as usize).min(pcm.len());
        Ok((pcm[..n].to_vec(), rate))
    };
    let horse: [(&str, &[(usize, i32, f64)]); 8] = [
        ("WALK", &[(48, 34, 0.6)]),
        ("RUN", &[(48, 39, 0.8)]),
        ("NEIGH", &[(50, 39, 3.0)]),
        ("GROAN", &[(50, 33, 1.2)]),
        ("JUMP", &[(48, 41, 0.4), (48, 43, 0.5)]),
        ("LAND", &[(48, 44, 0.4), (48, 41, 0.5)]),
        ("LAND2", &[(48, 38, 0.6)]),
        ("SLIP", &[(12, 27, 1.0)]),
    ];
    for (name, notes) in horse {
        let mut pcm: Vec<i32> = Vec::new();
        let mut rate: Option<u32> = None;
        for &(inst, key, secs) in notes {
            let (mut part, r) = instrument(inst, key, secs)?;
            match rate {
                None => rate = Some(r),
                Some(base) if r != base => {
                    let n = (part.len() as f64 * base as f64 / r as f64) as usize;
                    let last = part.len() - 1;
                    part = (0..n).map(|k| part[last.min((k as f64 * r as f64 / base as f64) as usize)]).collect();
                }
                _ => {}
            }
            pcm.extend(part);
        }
        let ids: Vec<usize> = notes.iter().map(|n| n.0).collect();
        emit(out, "epona", name, &[(pcm, rate.unwrap_or(0), format!("OoT Soundfont_0 instruments {ids:?}"))], &mut manifest)?;
    }
    std::fs::write(out.join("manifest-oot.json"), dumps(&sorted_manifest(manifest), Some(1)) + "\n").map_err(|e| e.to_string())
}
