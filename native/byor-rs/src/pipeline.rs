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
<<<<<<< HEAD
/// SM64 is complete (every exporter of `do_sm64` is ported and verified byte for byte, see native/ANDROID_IMPORT.md);
/// MK64, OoT and BK are not ported yet.
pub const PORTED: [(&str, &str); 3] = [("sm64", "1"), ("mk64", "1"), ("bk", "1")];
=======
/// SM64 and OoT are complete (every exporter of `do_sm64` / `do_oot` is ported and verified byte for byte, see
/// native/ANDROID_IMPORT.md); MK64 and BK are not ported yet.
pub const PORTED: [(&str, &str); 2] = [("sm64", "1"), ("oot", "1")];
>>>>>>> 32cc3432 (byor-rs: OoT enemies and the pipeline hook: the complete OoT import (524 files) byte-identical to byor/pipeline.py, using only decomp-manifest files)

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

fn rel_files(stage: &Path) -> std::collections::BTreeSet<String> {
    fn walk(dir: &Path, base: &Path, out: &mut std::collections::BTreeSet<String>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, base, out);
            } else {
                out.insert(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    for top in ["assets", "public"] {
        walk(&stage.join(top), stage, &mut out);
    }
    out
}

fn owned_by_others(data: &Path, game: &str) -> std::collections::HashSet<String> {
    let mut keep = std::collections::HashSet::new();
    for e in std::fs::read_dir(data.join("stamps")).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "json") && p.file_stem().is_some_and(|s| s != game) {
            if let Some(v) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
                keep.extend(v["files"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_string)));
            }
        }
    }
    keep
}

/// `install` of pipeline.py: swap the staged files over the install (stage dir inside the data dir: same-volume renames), then
/// drop files the previous import produced that this one no longer does. The stamp goes first, the caller rewrites it last.
fn install(stage: &Path, data: &Path, game: &str, owned: &[String]) -> std::io::Result<()> {
    let stamp = data.join("stamps").join(format!("{game}.json"));
    let old: std::collections::BTreeSet<String> = std::fs::read_to_string(&stamp).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).map(|v| v["files"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let _ = std::fs::remove_file(&stamp);
    for rel in rel_files(stage) {
        let d = data.join(&rel);
        std::fs::create_dir_all(d.parent().unwrap())?;
        std::fs::rename(stage.join(&rel), d)?;
    }
    let keep: std::collections::HashSet<&String> = owned.iter().collect();
    let others = owned_by_others(data, game);
    for rel in old {
        if keep.contains(&rel) || others.contains(&rel) {
            continue;
        }
        let _ = std::fs::remove_file(data.join(&rel));
        let mut parent = data.join(&rel).parent().map(Path::to_path_buf);
        while let Some(p) = parent.filter(|p| p != data && p.is_dir() && p.read_dir().is_ok_and(|mut r| r.next().is_none())) {
            let _ = std::fs::remove_dir(&p);
            parent = p.parent().map(Path::to_path_buf);
        }
    }
    Ok(())
}

/// Imports every given ROM; returns 0 when all succeeded, 1 when any game failed.
pub fn run(source: &Path, data: &Path, roms: &[(String, PathBuf)], out: &mut dyn Write) -> i32 {
    fn say(out: &mut dyn Write, s: String) {
        let _ = writeln!(out, "{s}");
        let _ = out.flush();
    }
    let _ = std::fs::create_dir_all(data.join("stamps"));
    let mut failed = false;
    for (game, path) in roms {
        let label = rom::ROMS.iter().find(|r| r.id == game).map(|r| r.label).unwrap_or("?");
        let z64 = match rom::identify(path) {
            Ok(Some((g, z))) if g == game => z,
            Ok(_) => {
                say(out, format!("@@FAIL {game} {} is not a recognised {label} ROM", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                failed = true;
                continue;
            }
            Err(e) => {
                say(out, format!("@@FAIL {game} cannot read {}: {e}", path.display()));
                failed = true;
                continue;
            }
        };
        let Some(hash) = exporter_hash(game) else {
            say(out, format!("@@FAIL {game} the {label} exporters are not ported to the pure-Rust importer yet (see native/ANDROID_IMPORT.md); import on a Mac and copy the data folder"));
            failed = true;
            continue;
        };
        say(out, format!("@@PROGRESS {game} 0 Preparing"));
        let stage = data.join(".stage");
        let _ = std::fs::remove_dir_all(&stage);
        let result = (|| -> Result<Vec<String>, String> {
            std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
            crate::sm64_import::copy_original(source, &stage)?;
            let before = rel_files(&stage);
            {
                let mut prog = |p: u32, m: &str| {
                    let _ = writeln!(out, "@@PROGRESS {game} {p} {m}");
                    let _ = out.flush();
                };
                match game.as_str() {
                    "sm64" => crate::sm64_import::run(source, &stage, &z64, &mut prog)?,
<<<<<<< HEAD
                    "mk64" => crate::mk64::run(source, &stage, &z64, &mut prog)?,
                    "bk" => crate::bk::run(source, &stage, &z64, &mut prog)?,
=======
                    "oot" => crate::oot::run(source, &stage, &z64, &mut prog)?,
>>>>>>> 32cc3432 (byor-rs: OoT enemies and the pipeline hook: the complete OoT import (524 files) byte-identical to byor/pipeline.py, using only decomp-manifest files)
                    _ => return Err("not ported".into()),
                }
            }
            let owned: Vec<String> = rel_files(&stage).difference(&before).cloned().collect();
            install(&stage, data, game, &owned).map_err(|e| e.to_string())?;
            Ok(owned)
        })();
        let _ = std::fs::remove_dir_all(&stage);
        match result {
            Ok(owned) => {
                remember_rom(data, game, path);
                let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let stamp = serde_json::json!({"version":VERSION,"game":game,"time":time,"hash":hash,"files":owned});
                let tmp = data.join("stamps").join(format!("{game}.json.tmp"));
                let ok = std::fs::write(&tmp, stamp.to_string()).and_then(|()| std::fs::rename(&tmp, data.join("stamps").join(format!("{game}.json")))).is_ok();
                if ok {
                    say(out, format!("@@DONE {game}"));
                } else {
                    say(out, format!("@@FAIL {game} could not write the stamp"));
                    failed = true;
                }
            }
            Err(e) => {
                say(out, format!("@@FAIL {game} {}", e.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect::<String>().replace('\n', " | ")));
                failed = true;
            }
        }
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
