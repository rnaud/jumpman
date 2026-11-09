//! The complete Mario Kart 64 import in Rust: `do_mk64` of `byor/pipeline.py` with every exporter ported
//! (`export-mk64.py` + `mk64_courses.py` + `mk64_props.py`, `export-mk64-cpu.py`, `export-mk64-modes.py`, `export-mk64-music.py`).
//! Writes `<stage>/assets` and `<stage>/public/mk64`; `pipeline::run` installs it.
pub mod common;
pub mod courses;
pub mod cpu;
pub mod export;
pub mod modes;
pub mod music;

use crate::pyc::R;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where the mk64 decomp checkout lives inside a repo checkout or a BYOR bundle (`byor-src`).
pub fn decomp_dir(source: &Path) -> PathBuf {
    for base in ["reference/mk64", "mk64"] {
        let p = source.join(base);
        if p.join("assets.json").is_file() {
            return p;
        }
    }
    source.join("reference/mk64")
}

/// A file of the `scripts/` tree, plain or gzipped (the iOS bundle ships the audio template gzipped).
pub fn script_bytes(source: &Path, rel: &str) -> R<Vec<u8>> {
    let plain = source.join("scripts").join(rel);
    if plain.is_file() {
        return std::fs::read(&plain).map_err(|e| format!("{}: {e}", plain.display()));
    }
    let gz = source.join("scripts").join(format!("{rel}.gz"));
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&gz).map_err(|e| format!("{}: {e}", plain.display()))?).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn run(source: &Path, stage: &Path, z64: &[u8], progress: &mut dyn FnMut(u32, &str)) -> R<()> {
    let decomp = decomp_dir(source);
    let (assets, public_mk64) = (stage.join("assets"), stage.join("public/mk64"));
    progress(5, "Exporting the 20 courses (kart + walkable) and karts");
    export::export(z64, &decomp, &public_mk64, &assets, progress)?;
    progress(94, "Exporting the CPU driver data");
    cpu::export(z64, &decomp, &public_mk64)?;
    progress(95, "Exporting cups, points and the battle arenas");
    modes::export(&decomp, &public_mk64, &assets)?;
    progress(96, "Building the Mario Kart 64 music engine");
    let template = script_bytes(source, "music/mk64-template.wasm")?;
    let layout: serde_json::Value = serde_json::from_slice(&script_bytes(source, "music/mk64-template.json")?).map_err(|e| e.to_string())?;
    music::export(z64, &template, &layout, &assets.join("audio/music/mk64"))?;
    Ok(())
}
