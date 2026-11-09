//! Super Mario 64 (US) asset extraction: the Rust port of the decomp's `extract_assets.py us` plus the
//! `mio0` and `n64graphics` tools (no C compiler, no Python). Driven by the decomp's own `assets.json`.
//!
//! Done: textures (every `*.rgba16/ia16/ia8/ia4/ia1.png`), raw `.m64` sequences and `.bin` blobs.
//! Not done yet (counted in [`Stats`], never silently dropped): the skybox / cake images (`skyconv`) and the
//! sound samples (`disassemble_sound.py` + `aifc_decode`).
use crate::n64::{self, TexFormat};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Stats {
    pub textures: usize,
    pub raw: usize,
    /// skybox / cake images (skyconv not ported)
    pub skipped_sky: usize,
    /// `.aiff` samples inside the sound bank (disassemble_sound not ported)
    pub skipped_sound: usize,
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
    for (n, e) in list.iter().enumerate() {
        if n % 64 == 0 {
            progress(n, total);
        }
        let is_png = e.name.ends_with(".png");
        let sky = e.name.starts_with("textures/skyboxes/") || e.name.starts_with("levels/ending/cake");
        match e.mio0 {
            Some(Err(())) => {
                stats.skipped_sound += 1;
                continue;
            }
            _ if is_png && sky => {
                stats.skipped_sky += 1;
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
        let data = image.get(e.pos..e.pos + e.size).ok_or_else(|| format!("{}: outside the ROM / MIO0 block", e.name))?;
        let dest = out.join(e.name);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|er| er.to_string())?;
        let bytes = if is_png {
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
    progress(total, total);
    Ok(stats)
}
