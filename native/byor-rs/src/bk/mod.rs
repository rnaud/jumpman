//! The complete Banjo-Kazooie import in Rust: `do_bk` of `byor/pipeline.py` with every exporter ported
//! (`export-bk-banjo.py`, `export-bk-levels.py` + `bk-maps.py`, `export-bk-enemies.py`, `export-bk-setup.py`,
//! `export-character-voices.py` (the BK half) and `export-bk-sfx.py`).
//! Writes `<stage>/assets` and `<stage>/public/models/bk`; `pipeline::run` installs it.
pub mod banjo;
pub mod enemies;
pub mod levels;
pub mod maps;
pub mod music;
pub mod setup;
pub mod voices;

use crate::pyc::{re, read, R};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const ASSET_TABLE: usize = 0x5E90;

/// Where the BK decomp checkout lives inside a repo checkout or a BYOR bundle (`byor-src`).
pub fn decomp_dir(source: &Path) -> PathBuf {
    for base in ["reference/banjo-kazooie", "banjo-kazooie"] {
        let p = source.join(base);
        if p.join("include/enums.h").is_file() {
            return p;
        }
    }
    source.join("reference/banjo-kazooie")
}

fn be32(d: &[u8], o: usize) -> R<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("read outside the buffer at {o:#x}"))
}

/// The rarezip asset table reader of export-bk-banjo.py `assets(rom)`.
pub struct Assets<'a> {
    rom: &'a [u8],
}

impl<'a> Assets<'a> {
    pub fn new(rom: &'a [u8]) -> Self {
        Assets { rom }
    }

    pub fn read(&self, index: usize) -> R<Vec<u8>> {
        let rom = self.rom;
        let count = be32(rom, ASSET_TABLE)? as usize;
        let table = ASSET_TABLE + 8;
        let data = table + 8 * count;
        let offset = be32(rom, table + 8 * index)? as usize;
        let compressed = u16::from_be_bytes([*rom.get(table + 8 * index + 4).ok_or("asset table")?, *rom.get(table + 8 * index + 5).ok_or("asset table")?]);
        let end = be32(rom, table + 8 * (index + 1))? as usize;
        let raw = rom.get(data + offset..data + end).ok_or_else(|| format!("asset {index:#x} outside the ROM"))?;
        if compressed == 0 {
            return Ok(raw.to_vec());
        }
        if raw.get(..2) != Some(&[0x11, 0x72]) {
            return Err(format!("asset {index:#x} is not rarezip data"));
        }
        let size = be32(raw, 2)? as usize;
        let mut out = Vec::with_capacity(size);
        let _ = flate2::read::DeflateDecoder::new(&raw[6..]).read_to_end(&mut out);
        if out.len() != size {
            return Err(format!("asset {index:#x}: {} != {size}", out.len()));
        }
        Ok(out)
    }
}

/// `enum_ids(source)`: asset enum name -> value from `include/enums.h`.
pub fn enum_ids(decomp: &Path) -> R<HashMap<String, i64>> {
    let text = read(&decomp.join("include/enums.h"))?;
    let start = text.find("enum asset_e").ok_or("enum asset_e")?;
    let body = &text[start..];
    let body = &body[..body.find("};").ok_or("enum asset_e end")?];
    let mut ids = HashMap::new();
    let mut value: i64 = -1;
    for c in re(r"(ASSET_\w+)\s*(?:=\s*(0x[0-9A-Fa-f]+|\d+))?\s*,").captures_iter(body) {
        value = match c.get(2) {
            Some(e) => crate::pyc::int0(e.as_str())?,
            None => value + 1,
        };
        ids.insert(c[1].to_string(), value);
    }
    Ok(ids)
}

pub fn run(source: &Path, stage: &Path, z64: &[u8], progress: &mut dyn FnMut(u32, &str)) -> R<()> {
    let decomp = decomp_dir(source);
    let assets = Assets::new(z64);
    let (out_assets, public) = (stage.join("assets"), stage.join("public"));
    let cat = maps::Catalogue::load(source)?;
    let ids = enum_ids(&decomp)?;
    progress(5, "Exporting Banjo and Kazooie");
    banjo::export(&assets, &ids, &public.join("models/bk"), false)?;
    progress(35, "Exporting Banjo move clips");
    banjo::export(&assets, &ids, &public.join("models/bk"), true)?;
    progress(40, "Exporting Banjo-Kazooie worlds and sub-areas");
    levels::export_all(&assets, &cat, &out_assets, progress)?;
    progress(90, "Exporting Banjo-Kazooie enemies");
    enemies::export(&assets, &ids, &decomp, &public.join("models/bk/enemies"))?;
    progress(96, "Placing enemies, collectibles and doors");
    setup::export(&assets, &cat, &out_assets.join("courses"))?;
    progress(99, "Exporting Banjo voices");
    voices::export_voices(z64, &out_assets.join("audio/voices"))?;
    progress(99, "Exporting Banjo-Kazooie sound effects");
    voices::export_sfx(z64, &decomp, &out_assets.join("audio"))?;
    progress(99, "Exporting Banjo-Kazooie music");
    music::export(&assets, &cat, &decomp, z64, &out_assets)?;
    Ok(())
}
