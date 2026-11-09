//! Ports of `scripts/export-character-voices.py` (the Banjo-Kazooie half) and `scripts/export-bk-sfx.py`: VADPCM decoding of the
//! sound effects in BK's libultra ALBankFile and WAV output.
//! Writes `assets/audio/voices/banjo/*.wav` + `manifest-bk.json`, and `assets/audio/voices/bk_sfx/*.wav` + `manifest-bk-sfx.json`.
use super::banjo::{i16_at, u32_at, u8_at};
use crate::pyc::{dumps, int0, re, read, R};
use crate::zpng::round_f;
use serde_json::{json, Map, Value};
use std::path::Path;

/// output name -> `enum sfx_e` values
const BK_VOICES: &[(&str, &[usize])] = &[
    ("JUMP", &[0x54, 0x55, 0x56]),
    ("OW", &[0x32, 0x34, 0x35, 0x37, 0x38, 0x39, 0x3A, 0x3B]),
    ("AHOO", &[0x33]),
    ("DOH", &[0x36]),
    ("HUIII", &[0x53]),
    ("YAH_OH", &[0x52]),
    ("WAHOO", &[0x4F]),
    ("UWAAAAOOH", &[0x63]),
    ("KAZOOIE_HEUGH", &[0x47]),
    ("KAZOOIE_RUUH", &[0x48]),
    ("KAZOOIE_RUH", &[0x43]),
    ("KAZOOIE_HUGHH", &[0x45]),
    ("KAZOOIE_BRUIII", &[0x4E]),
];

/// Decode N64 VADPCM; `book` is the ALADPCMBook struct (order, npredictors, s16 table).
pub fn vadpcm(book: &[u8], data: &[u8]) -> R<Vec<i16>> {
    let (order, npred) = (u32_at(book, 0)? as i32, u32_at(book, 4)? as i32);
    if order != 2 {
        return Err(format!("unsupported VADPCM order {order}"));
    }
    let n = (8 * order * npred) as usize;
    let mut table = Vec::with_capacity(n);
    for k in 0..n {
        table.push(i16_at(book, 8 + 2 * k)? as i64);
    }
    let mut out: Vec<i16> = Vec::new();
    let (mut h2, mut h1) = (0i64, 0i64);
    let mut off = 0;
    while off + 8 < data.len() {
        let (scale, pred) = (data[off] >> 4, (data[off] & 15) as usize);
        if pred >= npred as usize {
            return Err("VADPCM predictor out of range".into());
        }
        let t0 = &table[pred * 16..pred * 16 + 8];
        let t1 = &table[pred * 16 + 8..pred * 16 + 16];
        let mut res = [0i64; 16];
        for (k, b) in data[off + 1..off + 9].iter().enumerate() {
            for (j, nib) in [b >> 4, b & 15].into_iter().enumerate() {
                let n = nib as i64;
                res[k * 2 + j] = (if n >= 8 { n - 16 } else { n }) << scale;
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
            out.extend(y.iter().map(|&v| v as i16));
            h2 = y[6];
            h1 = y[7];
        }
        off += 9;
    }
    Ok(out)
}

pub fn write_wav(path: &Path, pcm: &[i16], rate: u32) -> R<()> {
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

/// (peak, rms rounded to 1 decimal)
fn stats(pcm: &[i16]) -> (i64, f64) {
    let peak = pcm.iter().map(|&x| (x as i64).abs()).max().unwrap_or(0);
    let sum: i64 = pcm.iter().map(|&x| x as i64 * x as i64).sum();
    let rms = (sum as f64 / pcm.len() as f64).powf(std::hint::black_box(0.5));
    (peak, round_f(rms, 1))
}

struct Bank<'a> {
    ctl: &'a [u8],
    tbl: &'a [u8],
    rate: i32,
    instrument: usize,
    count: i64,
}

fn bank(rom: &[u8]) -> R<Bank<'_>> {
    let ctl = rom.get(0xD846C0..0xD954B0).ok_or("ROM too small")?;
    let tbl = rom.get(0xD954B0..0xEA3EB0).ok_or("ROM too small")?;
    let b = u32_at(ctl, 4)? as usize;
    let rate = u32_at(ctl, b + 4)? as i32;
    let instrument = u32_at(ctl, b + 12)? as usize;
    let count = i16_at(ctl, instrument + 14)? as i64;
    if rate != 22050 || count != 402 {
        return Err("unexpected Banjo-Kazooie sound bank".into());
    }
    Ok(Bank { ctl, tbl, rate, instrument, count })
}

/// Output rate of BK's audio (`sn_alConfig.outputRate = osAiSetFrequency(22000)` in `core1/code_1D00.c`). libultra ignores the bank's
/// `sampleRate` field: a voice reads `pitch` samples per output sample, so a sample at pitch 1.0 plays at this rate.
pub const OUTPUT_RATE: f64 = 22000.0;

/// The pitch ratio the sound-effect player gives a sound (`func_802444C0` in `core1/code_5650.c`):
/// `alCents2Ratio(keyBase * 100 + detune - 6000)`, i.e. 2^((keyBase - 60) / 12) (+ detune cents), keyBase 60 being unity. The detune is
/// left out when bit 0x20 of the key map's `keyMax` is set.
pub fn sfx_pitch(key_base: u8, key_max: u8, detune: i8) -> f64 {
    let cents = key_base as f64 * 100.0 + if key_max & 0x20 != 0 { 0.0 } else { detune as f64 } - 6000.0;
    2f64.powf(cents / 1200.0)
}

/// Rate a sound's PCM must be played at (real time) to sound like the game: `OUTPUT_RATE` x the pitch ratio.
pub fn sfx_rate(key_base: u8, key_max: u8, detune: i8) -> u32 {
    (OUTPUT_RATE * sfx_pitch(key_base, key_max, detune)).round() as u32
}

/// (keyMax, keyBase, detune) of sound i's key map.
fn key_map(b: &Bank, i: usize) -> R<Option<(u8, u8, i8)>> {
    let p = u32_at(b.ctl, b.instrument + 16 + 4 * i)? as usize;
    if p == 0 {
        return Ok(None);
    }
    let k = u32_at(b.ctl, p + 4)? as usize;
    Ok(Some((u8_at(b.ctl, k + 3)?, u8_at(b.ctl, k + 4)?, u8_at(b.ctl, k + 5)? as i8)))
}

/// Playback rate of sound `i` of the bank in `rom` (for tests and tools).
pub fn rate_of_sound(rom: &[u8], i: usize) -> R<u32> {
    let b = bank(rom)?;
    let (km, kb, d) = key_map(&b, i)?.ok_or_else(|| format!("sound {i}: empty slot"))?;
    Ok(sfx_rate(kb, km, d))
}

/// (base, length, kind, flags, loop, book) of sound i, or None for an empty slot
fn wave(b: &Bank, i: usize) -> R<Option<(i32, i32, u8, u8, u32, usize)>> {
    let p = u32_at(b.ctl, b.instrument + 16 + 4 * i)? as usize;
    if p == 0 {
        return Ok(None);
    }
    let w = u32_at(b.ctl, p + 8)? as usize;
    Ok(Some((u32_at(b.ctl, w)? as i32, u32_at(b.ctl, w + 4)? as i32, u8_at(b.ctl, w + 8)?, u8_at(b.ctl, w + 9)?, u32_at(b.ctl, w + 12)?, u32_at(b.ctl, w + 16)? as usize)))
}

fn decode(b: &Bank, base: i32, length: i32, book: usize) -> R<Vec<i16>> {
    let (order, npred) = (u32_at(b.ctl, book)? as usize, u32_at(b.ctl, book + 4)? as usize);
    let book_end = (book + 8 + 16 * order * npred).min(b.ctl.len());
    let data_end = ((base + length).max(0) as usize).min(b.tbl.len());
    vadpcm(&b.ctl[book..book_end], &b.tbl[(base.max(0) as usize).min(data_end)..data_end])
}

pub fn export_voices(rom: &[u8], out: &Path) -> R<()> {
    let b = bank(rom)?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut manifest: Vec<(String, Value)> = Vec::new();
    for (name, ids) in BK_VOICES {
        for (n, &i) in ids.iter().enumerate() {
            let (base, length, kind, _flags, lp, book) = wave(&b, i)?.ok_or_else(|| format!("sound {i}: empty slot"))?;
            if kind != 0 || lp != 0 {
                return Err(format!("sound {i}: not a one-shot ADPCM sample"));
            }
            let pcm = decode(&b, base, length, book)?;
            let rel = format!("voices/banjo/{name}_{n}.wav");
            let (km, kb, d) = key_map(&b, i)?.ok_or_else(|| format!("sound {i}: no key map"))?;
            let rate = sfx_rate(kb, km, d);
            write_wav(&out.parent().unwrap().join(&rel), &pcm, rate)?;
            let (peak, rms) = stats(&pcm);
            manifest.push((rel, json!({"sampleRate": rate, "frames": pcm.len(), "duration": round_f(pcm.len() as f64 / rate as f64, 4), "peak": peak, "rms": rms, "keyBase": kb, "detune": d, "source": format!("BK sfx_e 0x{i:X}")})));
        }
    }
    manifest.sort_by(|a, b| a.0.cmp(&b.0));
    let mut m = Map::new();
    for (k, v) in manifest {
        m.insert(k, v);
    }
    std::fs::write(out.join("manifest-bk.json"), dumps(&Value::Object(m), Some(1)) + "\n").map_err(|e| e.to_string())
}

/// `enum sfx_e` -> {value: name without the `SFX_<hex>_` prefix}
fn sfx_names(enums_h: &Path) -> R<std::collections::HashMap<i64, String>> {
    let text = read(enums_h)?;
    let body = text.split_once("enum sfx_e").ok_or("enum sfx_e")?.1.split_once("};").ok_or("enum sfx_e end")?.0.to_string();
    let body = re(r"//[^\n]*").replace_all(&body, "").into_owned();
    let body = re(r"(?s)/\*.*?\*/").replace_all(&body, "").into_owned();
    let mut names = std::collections::HashMap::new();
    let mut value: i64 = 0;
    for entry in body.split_once('{').ok_or("enum body")?.1.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some(m) = re(r"^(\w+)(?:\s*=\s*(0x[0-9A-Fa-f]+|\d+))?").captures(entry) else { continue };
        if let Some(v) = m.get(2) {
            value = int0(v.as_str())?;
        }
        names.insert(value, re(r"^SFX_[0-9A-Fa-f]+_").replace(&m[1], "").into_owned());
        value += 1;
    }
    Ok(names)
}

fn sort_keys(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut e: Vec<(String, Value)> = m.into_iter().collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(e.into_iter().map(|(k, v)| (k, sort_keys(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

/// Every sound effect of the bank. `out` is `assets/audio`.
pub fn export_sfx(rom: &[u8], decomp: &Path, out: &Path) -> R<()> {
    let enums = decomp.join("include/enums.h");
    let names = if enums.is_file() { sfx_names(&enums)? } else { Default::default() };
    let b = bank(rom)?;
    let mut manifest = Map::new();
    let mut skipped: Vec<(usize, String)> = Vec::new();
    for i in 0..b.count as usize {
        let Some((base, length, kind, _flags, lp, book)) = wave(&b, i)? else {
            skipped.push((i, "empty slot".into()));
            continue;
        };
        if kind != 0 || length <= 0 {
            skipped.push((i, format!("sample type {kind}, length {length}")));
            continue;
        }
        let pcm = decode(&b, base, length, book)?;
        if pcm.is_empty() {
            skipped.push((i, "empty sample".into()));
            continue;
        }
        let name = format!("{i:X}_{}", names.get(&(i as i64)).map(String::as_str).unwrap_or("SOUND"));
        let rel = format!("voices/bk_sfx/{name}_0.wav");
        let (km, kb, d) = key_map(&b, i)?.unwrap_or((0, 60, 0));
        let rate = sfx_rate(kb, km, d);
        write_wav(&out.join(&rel), &pcm, rate)?;
        let (peak, rms) = stats(&pcm);
        manifest.insert(rel, json!({"id": i, "sampleRate": rate, "frames": pcm.len(), "duration": round_f(pcm.len() as f64 / rate as f64, 4), "peak": peak, "rms": rms, "keyBase": kb, "detune": d, "looped": lp != 0}));
    }
    let mut sk = Map::new();
    for (k, v) in skipped {
        sk.insert(format!("{k:X}"), Value::String(v));
    }
    let doc = sort_keys(json!({"clips": Value::Object(manifest), "skipped": Value::Object(sk)}));
    std::fs::write(out.join("voices/manifest-bk-sfx.json"), dumps(&doc, Some(1))).map_err(|e| e.to_string())
}

#[cfg(test)]
mod rate_tests {
    use super::*;

    #[test]
    fn the_pitch_formula_is_the_decomps() {
        // keyBase 60 = unity, 48 = an octave down, 42 = 1.5 octaves down; detune in cents unless flagged off
        assert_eq!(sfx_rate(60, 0, 0), 22000);
        assert_eq!(sfx_rate(48, 0, 0), 11000);
        assert_eq!(sfx_rate(72, 0, 0), 44000);
        assert_eq!(sfx_rate(42, 0, 0), 7778);
        assert_eq!(sfx_rate(60, 0, 100), 23308);
        assert_eq!(sfx_rate(60, 0x20, 100), 22000, "keyMax bit 0x20 drops the detune");
    }
}
