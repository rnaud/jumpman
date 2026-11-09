//! Super Mario 64 (US) asset extraction: the Rust port of the decomp's `extract_assets.py us` plus the
//! `mio0` and `n64graphics` tools (no C compiler, no Python). Driven by the decomp's own `assets.json`.
//!
//! Done: textures (every `*.rgba16/ia16/ia8/ia4/ia1.png`), skybox / cake images (`skyconv --combine`), raw `.m64`
//! sequences and `.bin` blobs.
//! Not done yet (counted in [`Stats`], never silently dropped): the sound samples (`disassemble_sound.py` + `aifc_decode`).
use crate::n64::{self, TexFormat};
use crate::sound;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Stats {
    pub textures: usize,
    pub raw: usize,
    /// skybox / cake images (skyconv --combine)
    pub sky: usize,
    /// `.aiff` samples decoded from the sound bank (disassemble_sound + aifc_decode)
    pub sound: usize,
}

/// One `assets.json` entry applicable to the US ROM.
struct Entry<'a> {
    name: &'a str,
    pos: usize,
    size: usize,
    meta: Vec<u64>,
    /// `Some(offset)` for assets inside a MIO0 block, `None` for plain ROM data; `Err` marks `@sound`.
    mio0: Option<Result<usize, ()>>,
}

fn entries(asset_map: &Value) -> Result<Vec<Entry<'_>>, String> {
    let obj = asset_map.as_object().ok_or("assets.json is not an object")?;
    let mut out = Vec::new();
    for (name, v) in obj {
        if name.starts_with('@') {
            continue;
        }
        let a = v.as_array().filter(|a| a.len() >= 2).ok_or_else(|| format!("{name}: bad entry"))?;
        let (size, positions) = (&a[a.len() - 2], &a[a.len() - 1]);
        let Some(pos) = positions.get("us").and_then(Value::as_array) else { continue };
        let last = pos.last().and_then(Value::as_u64).ok_or_else(|| format!("{name}: bad position"))? as usize;
        let mio0 = if pos.len() == 1 {
            None
        } else if pos[0].as_str() == Some("@sound") {
            Some(Err(()))
        } else {
            Some(Ok(pos[0].as_u64().ok_or_else(|| format!("{name}: bad mio0 offset"))? as usize))
        };
        out.push(Entry { name, pos: last, size: size.as_u64().unwrap_or(0) as usize, meta: a[..a.len() - 2].iter().filter_map(Value::as_u64).collect(), mio0 });
    }
    out.sort_by(|a, b| a.name.cmp(b.name));
    Ok(out)
}

/// Extract everything that is ported into `out` (the decomp tree root: `textures/...`, `actors/...`, `levels/...`).
/// `progress(done, total)` is called per asset group.
pub fn extract(rom: &[u8], assets_json: &[u8], out: &Path, mut progress: impl FnMut(usize, usize)) -> Result<Stats, String> {
    let map: Value = serde_json::from_slice(assets_json).map_err(|e| format!("assets.json: {e}"))?;
    let list = entries(&map)?;
    let total = list.len();
    let mut stats = Stats::default();
    let mut cache: HashMap<usize, Vec<u8>> = HashMap::new();
    let mut sounds: Vec<(&str, usize)> = Vec::new();
    for (n, e) in list.iter().enumerate() {
        if n % 64 == 0 {
            progress(n, total);
        }
        let is_png = e.name.ends_with(".png");
        let sky = e.name.starts_with("textures/skyboxes/") || e.name.starts_with("levels/ending/cake");
        match e.mio0 {
            Some(Err(())) => {
                sounds.push((e.name, e.pos));
                continue;
            }
            _ => {}
        }
        let image: &[u8] = match e.mio0 {
            Some(Ok(off)) => {
                if !cache.contains_key(&off) {
                    cache.insert(off, n64::mio0_decode(rom, off)?);
                }
                &cache[&off]
            }
            _ => rom,
        };
        let data = image.get(e.pos.min(image.len())..(e.pos + e.size).min(image.len())).ok_or_else(|| format!("{}: outside the ROM / MIO0 block", e.name))?;
        // like the Python slice `image[pos:pos+size]`, a block that runs past the end is truncated
        let dest = out.join(e.name);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|er| er.to_string())?;
        let bytes = if is_png && sky {
            stats.sky += 1;
            if e.name.starts_with("textures/skyboxes/") {
                n64::skybox_png(data)?
            } else if e.name.ends_with("/cake.png") {
                n64::cake_png(data)?
            } else {
                return Err(format!("{}: unsupported cake variant", e.name));
            }
        } else if is_png {
            let (w, h) = match e.meta[..] {
                [w, h] => (w as usize, h as usize),
                _ => return Err(format!("{}: missing width/height", e.name)),
            };
            let fmt = e.name.rsplit('.').nth(1).and_then(TexFormat::parse).ok_or_else(|| format!("{}: unknown texture format", e.name))?;
            stats.textures += 1;
            n64::texture_png(data, w, h, fmt)
        } else {
            stats.raw += 1;
            data.to_vec()
        };
        std::fs::write(&dest, bytes).map_err(|er| format!("{}: {er}", dest.display()))?;
    }
    if !sounds.is_empty() {
        let at = |key: &str| -> Result<(usize, usize), String> {
            let v = map.get(key).and_then(Value::as_array).ok_or_else(|| format!("assets.json has no {key}"))?;
            let off = v[1]["us"][0].as_u64().ok_or_else(|| format!("{key}: no US offset"))? as usize;
            Ok((off, v[0].as_u64().unwrap_or(0) as usize))
        };
        let (ctl, tbl) = (at("@sound ctl us")?, at("@sound tbl us")?);
        let slice = |(o, n): (usize, usize)| rom.get(o..o + n).ok_or("sound bank outside the ROM");
        let samples = sound::sample_entries(slice(ctl)?, slice(tbl)?)?;
        for (n, (name, index)) in sounds.iter().enumerate() {
            progress(n, sounds.len());
            let entry = samples.get(*index).ok_or_else(|| format!("{name}: sample index {index} out of range"))?;
            let dest = out.join(name);
            std::fs::create_dir_all(dest.parent().unwrap()).map_err(|er| er.to_string())?;
            std::fs::write(&dest, sound::decode_to_aiff(entry)?).map_err(|er| er.to_string())?;
            stats.sound += 1;
        }
    }
    progress(total, total);
    Ok(stats)
}

fn collect(dir: &Path, rel: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        rel.push(name.clone());
        if e.path().is_dir() {
            collect(&e.path(), rel, out);
        } else if name.ends_with(".aiff") {
            out.push(rel.clone());
        }
        rel.pop();
    }
}

/// Port of `scripts/export-audio.py`: every `<samples>/**/*.aiff` to `<out>/<same path>.wav` (`#` becomes `sharp`)
/// and `<out>/manifest.json`. Returns the number of samples.
pub fn export_audio(samples: &Path, out: &Path) -> Result<usize, String> {
    let mut files = Vec::new();
    collect(samples, &mut Vec::new(), &mut files);
    files.sort(); // Python sorts Paths part by part
    if files.is_empty() {
        return Err("No extracted sound samples found".into());
    }
    let mut manifest = String::from("{\n");
    for (n, parts) in files.iter().enumerate() {
        let rel = parts.join("/");
        let wav_rel = format!("{}.wav", rel.trim_end_matches(".aiff")).replace('#', "sharp");
        let info = sound::aiff_to_wav(&std::fs::read(samples.join(&rel)).map_err(|e| e.to_string())?).map_err(|e| format!("{rel}: {e}"))?;
        let dest = out.join(&wav_rel);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&dest, &info.wav).map_err(|e| e.to_string())?;
        let loop_ = match info.loop_ {
            Some((a, b)) => format!("[\n      {},\n      {}\n    ]", sound::py_float(a), sound::py_float(b)),
            None => "null".into(),
        };
        manifest += &format!(
            "  {}: {{\n    \"loop\": {loop_},\n    \"frames\": {},\n    \"sampleRate\": {},\n    \"duration\": {},\n    \"instrumentHex\": \"{}\",\n    \"markersHex\": \"{}\"\n  }}{}\n",
            serde_json::to_string(&wav_rel).unwrap(),
            info.frames,
            info.rate,
            sound::py_float(info.frames as f64 / info.rate as f64),
            info.instrument_hex,
            info.markers_hex,
            if n + 1 < files.len() { "," } else { "" }
        );
    }
    manifest += "}\n";
    std::fs::write(out.join("manifest.json"), manifest).map_err(|e| e.to_string())?;
    Ok(files.len())
}

/// `scripts/render-music-template.py` for the US build: the music engine wasm with the user's sequences, banks and samples
/// patched into the data-less template. `out` is the tree `extract` wrote (needs `sound/sequences/us/*.m64`), `decomp` the
/// decomp checkout (`sound/sound_banks`, `sound/sequences.json`), `sound_player` the precomputed bytes of
/// `00_sound_player.s` (`native/byor/sm64-us-sound-player.bin`).
pub fn music_wasm(rom: &[u8], assets_json: &[u8], out: &Path, decomp: &Path, template: &[u8], layout_json: &str, sound_player: &[u8]) -> Result<Vec<u8>, String> {
    let map: Value = serde_json::from_slice(assets_json).map_err(|e| e.to_string())?;
    let list = entries(&map)?;
    let at = |key: &str| -> Result<(usize, usize), String> {
        let v = map.get(key).and_then(Value::as_array).ok_or_else(|| format!("assets.json has no {key}"))?;
        Ok((v[1]["us"][0].as_u64().ok_or("no US offset")? as usize, v[0].as_u64().unwrap_or(0) as usize))
    };
    let slice = |(o, n): (usize, usize)| rom.get(o..o + n).ok_or("sound bank outside the ROM");
    let entries_v = sound::sample_entries(slice(at("@sound ctl us")?)?, slice(at("@sound tbl us")?)?)?;
    let mut files = Vec::new();
    for e in list.iter().filter(|e| matches!(e.mio0, Some(Err(())))) {
        let parts: Vec<&str> = e.name.split('/').collect();
        let (bank, file) = (parts[parts.len() - 2], parts[parts.len() - 1]);
        files.push(crate::sound_banks::SampleFile { bank: bank.to_string(), name: file.trim_end_matches(".aiff").to_string(), entry: entries_v.get(e.pos).ok_or("sample index")?.clone() });
    }
    let mut seqs: Vec<(String, Vec<u8>)> = vec![("00_sound_player".into(), sound_player.to_vec())];
    let sdir = out.join("sound/sequences/us");
    for f in std::fs::read_dir(&sdir).map_err(|e| format!("{}: {e}", sdir.display()))?.flatten() {
        let n = f.file_name().to_string_lossy().into_owned();
        if let Some(stem) = n.strip_suffix(".m64") {
            seqs.push((stem.to_string(), std::fs::read(f.path()).map_err(|e| e.to_string())?));
        }
    }
    let blobs = crate::sound_banks::build(decomp, &mut files, &seqs)?;
    let layout: Value = serde_json::from_str(layout_json).map_err(|e| e.to_string())?;
    let mut wasm = template.to_vec();
    for (symbol, blob) in [("gMusicData", &blobs.sequences), ("gBankSetsData", &blobs.sets), ("gSoundDataADSR", &blobs.ctl), ("gSoundDataRaw", &blobs.tbl)] {
        let (off, len) = (layout[symbol]["offset"].as_u64().ok_or("layout")? as usize, layout[symbol]["length"].as_u64().ok_or("layout")? as usize);
        if blob.len() != len {
            return Err(format!("{symbol}: built {} bytes, template expects {len}", blob.len()));
        }
        if wasm[off..off + len].iter().any(|b| *b != 0) {
            return Err("template is not data-less".into());
        }
        wasm[off..off + len].copy_from_slice(blob);
    }
    Ok(wasm)
}
