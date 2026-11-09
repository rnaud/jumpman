//! Orchestration: ROM identification, stamps and the per-game import, mirroring `byor/pipeline.py` (same `@@` protocol,
//! same `stamps/<game>.json` and `roms.json` formats so a data dir is interchangeable between the two implementations).
use crate::rom;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Must equal `VERSION` in byor/pipeline.py and `PIPELINE_VERSION` in src/byor.rs.
pub const VERSION: u64 = 3;

/// Games whose complete exporter chain exists in Rust. Bump the revision when a ported exporter's output changes
/// (it is the stamp hash, the Rust counterpart of the Python per-game exporter hash).
///
/// Empty today: SM64 has the ROM identification and the decomp asset extraction (textures, sequences) ported, but not
/// the course / model / animation / audio exporters, so no game can be imported end to end in Rust yet.
pub const PORTED: [(&str, &str); 0] = [];

pub fn exporter_hash(game: &str) -> Option<String> {
    PORTED.iter().find(|(g, _)| *g == game).map(|(g, rev)| format!("rust-{g}-{rev}"))
}

pub fn sm64_assets_json(source: &Path) -> PathBuf {
    for base in ["reference/sm64", "native/../reference/sm64", "sm64"] {
        let p = source.join(base).join("assets.json");
        if p.is_file() {
            return p;
        }
    }
    source.join("reference/sm64/assets.json")
}

pub fn print_hashes() -> String {
    let games: serde_json::Map<String, serde_json::Value> = PORTED.iter().filter_map(|(g, _)| Some((g.to_string(), exporter_hash(g)?.into()))).collect();
    serde_json::json!({ "version": VERSION, "games": games }).to_string()
}

/// `missing` | `stale` | `current`, the same rule as `stamp_state` in pipeline.py / src/byor.rs.
pub fn stamp_state(data: &Path, game: &str, expected: Option<&str>) -> &'static str {
    let Ok(text) = std::fs::read_to_string(data.join("stamps").join(format!("{game}.json"))) else {
        return "missing";
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return "stale";
    };
    if v["version"].as_u64() != Some(VERSION) || expected.is_some_and(|e| v["hash"].as_str() != Some(e)) {
        "stale"
    } else {
        "current"
    }
}

/// The ROM path of the last successful import of `game`, if the file is still there and still identifies as it.
pub fn remembered_rom(data: &Path, game: &str) -> Option<PathBuf> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.join("roms.json")).ok()?).ok()?;
    let p = PathBuf::from(v[game]["path"].as_str()?);
    (p.is_file() && rom::identify(&p).ok()??.0 == game).then_some(p)
}

pub fn remember_rom(data: &Path, game: &str, path: &Path) {
    let f = data.join("roms.json");
    let mut v: serde_json::Value = std::fs::read_to_string(&f).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::json!({}));
    v[game] = serde_json::json!({ "path": path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).to_string_lossy() });
    let _ = std::fs::write(f, serde_json::to_string_pretty(&v).unwrap_or_default());
}

pub fn status_lines(data: &Path) -> Vec<String> {
    rom::ROMS
        .iter()
        .map(|r| {
            let state = stamp_state(data, r.id, exporter_hash(r.id).as_deref());
            let remembered = if state == "stale" { remembered_rom(data, r.id) } else { None };
            format!("@@STATUS {} {} {}", r.id, state, remembered.map(|p| p.display().to_string()).unwrap_or_default()).trim_end().to_string()
        })
        .collect()
}

/// Imports every given ROM; returns 0 when all succeeded, 1 when any game failed.
pub fn run(_source: &Path, data: &Path, roms: &[(String, PathBuf)], out: &mut dyn Write) -> i32 {
    let mut say = |s: String| {
        let _ = writeln!(out, "{s}");
        let _ = out.flush();
    };
    let _ = std::fs::create_dir_all(data.join("stamps"));
    let mut failed = false;
    for (game, path) in roms {
        let label = rom::ROMS.iter().find(|r| r.id == game).map(|r| r.label).unwrap_or("?");
        match rom::identify(path) {
            Ok(Some((g, _))) if g == game => {}
            Ok(_) => {
                say(format!("@@FAIL {game} {} is not a recognised {label} ROM", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                failed = true;
                continue;
            }
            Err(e) => {
                say(format!("@@FAIL {game} cannot read {}: {e}", path.display()));
                failed = true;
                continue;
            }
        }
        if exporter_hash(game).is_none() {
            say(format!(
                "@@FAIL {game} the {label} exporters are not ported to the pure-Rust importer yet (see native/ANDROID_IMPORT.md); import on a Mac and copy the data folder"
            ));
            failed = true;
        }
        // Ported games are staged, installed atomically and stamped here (the Python `install` logic) once any exist.
    }
    i32::from(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_rules_match_the_python_pipeline() {
        let d = std::env::temp_dir().join(format!("byor-rs-stamps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("stamps")).unwrap();
        assert_eq!(stamp_state(&d, "sm64", None), "missing");
        std::fs::write(d.join("stamps/sm64.json"), format!("{{\"version\":{VERSION},\"hash\":\"h\"}}")).unwrap();
        assert_eq!(stamp_state(&d, "sm64", Some("h")), "current");
        assert_eq!(stamp_state(&d, "sm64", Some("x")), "stale");
        assert_eq!(stamp_state(&d, "sm64", None), "current");
        std::fs::write(d.join("stamps/sm64.json"), "{\"version\":2}").unwrap();
        assert_eq!(stamp_state(&d, "sm64", None), "stale");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
