//! The Super Smash Bros. stage and music import (native/SSB64.md "Stages", "Music"). Skipped unless
//!   BYOR_TEST_SSB64_ROM = a Super Smash Bros. (US) ROM in any byte order; for the parity tests also
//!   BYOR_TEST_SSB64_STAGES_REF = the `--out` folder of `python3 scripts/export-ssb64-stages.py` (holds courses/, levels/),
//!   BYOR_TEST_SSB64_MUSIC_REF = the `--out` folder of `python3 scripts/export-ssb64-music.py`.
//! No ROM or extracted data is committed.
use std::path::{Path, PathBuf};

fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rom() -> Option<Vec<u8>> {
    let path = std::env::var_os("BYOR_TEST_SSB64_ROM")?;
    let (game, z64) = byor_rs::rom::identify(Path::new(&path)).unwrap().expect("a known ROM");
    assert_eq!(game, "ssb64");
    Some(z64)
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    out.sort();
    out
}

fn same_tree(ours: &Path, reference: &Path, filter: &dyn Fn(&Path) -> bool) -> usize {
    let mine: Vec<PathBuf> = files_under(ours).into_iter().filter(|p| filter(p)).collect();
    let theirs: Vec<PathBuf> = files_under(reference).into_iter().filter(|p| filter(p)).collect();
    assert_eq!(mine, theirs, "file lists differ");
    let mut differ = vec![];
    for p in &mine {
        if std::fs::read(ours.join(p)).unwrap() != std::fs::read(reference.join(p)).unwrap() {
            differ.push(p.display().to_string());
        }
    }
    assert!(differ.is_empty(), "{} of {} files differ from the Python export: {differ:?}", differ.len(), mine.len());
    mine.len()
}

#[test]
fn ssb64_stages_export_and_match_python() {
    let Some(z64) = rom() else {
        eprintln!("skipped: BYOR_TEST_SSB64_ROM not set");
        return;
    };
    let out = std::env::temp_dir().join(format!("byor-ssb64-stages-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let n = byor_rs::ssb64_stages::export(&source(), &z64, &out, None, &mut |m| eprintln!("{m}")).unwrap();
    assert_eq!(n, 41);
    for (id, _, _, kind) in byor_rs::ssb64_stages::stages() {
        let course: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join(format!("courses/ssb64_{id}.json"))).unwrap()).unwrap();
        let glb = std::fs::read(out.join(format!("levels/ssb64_{id}/stage.glb"))).unwrap();
        assert_eq!(&glb[..4], b"glTF", "{id}");
        assert!(glb.len() > 10_000, "{id}: glb of {} bytes", glb.len());
        let s = &course["ssb64"];
        assert!(course["triangles"].as_array().unwrap().len() >= 10, "{id}: collision");
        assert!(s["lines"]["floor"].as_u64().unwrap() >= 1, "{id}: no floor line");
        let (b, sp) = (&s["blast"], &course["spawn"]["pos"]);
        let x = sp[0].as_f64().unwrap();
        assert!(b["left"].as_f64().unwrap() < x && x < b["right"].as_f64().unwrap(), "{id}: spawn outside the blast zones");
        assert_eq!(course["deathHeight"], b["bottom"], "{id}");
        if kind == "battle" {
            assert_eq!(s["players"].as_array().unwrap().len(), 4, "{id}: four player starts");
            assert!(s["bgm"].as_u64().unwrap() <= 10, "{id}: a stage theme");
        }
    }
    if let Some(reference) = std::env::var_os("BYOR_TEST_SSB64_STAGES_REF") {
        let reference = PathBuf::from(reference);
        let n = same_tree(&out, &reference, &|p| p.to_string_lossy().contains("ssb64_"));
        eprintln!("{n} stage files byte-identical to the Python export");
    }
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn ssb64_music_exports_and_matches_python() {
    let Some(z64) = rom() else {
        eprintln!("skipped: BYOR_TEST_SSB64_ROM not set");
        return;
    };
    let out = std::env::temp_dir().join(format!("byor-ssb64-music-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    assert_eq!(byor_rs::ssb64_music::export(&source(), &z64, &out).unwrap(), 47);
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join("music.json")).unwrap()).unwrap();
    let tracks = doc["tracks"].as_array().unwrap();
    assert_eq!(tracks[0]["name"], "Pupupu");
    assert_eq!(tracks[6]["name"], "Castle");
    for t in tracks {
        assert_eq!(t["division"], 480, "{}", t["name"]);
        assert!(t["channels"].as_u64().unwrap() >= 1, "{}", t["name"]);
    }
    assert_eq!(doc["victory"]["purin"], doc["victory"]["pikachu"]);
    if let Some(reference) = std::env::var_os("BYOR_TEST_SSB64_MUSIC_REF") {
        let n = same_tree(&out, Path::new(&reference), &|_| true);
        eprintln!("{n} music files byte-identical to the Python export");
    }
    let _ = std::fs::remove_dir_all(&out);
}
