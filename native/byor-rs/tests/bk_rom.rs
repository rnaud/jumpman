//! The Banjo-Kazooie import against a data dir made by the Python pipeline from the same ROM. Skipped unless
//!   BYOR_TEST_BK_ROM = a Banjo-Kazooie (US 1.0) ROM in any byte order, and
//!   BYOR_TEST_BK_REF = a data dir written by `python3 native/byor/pipeline.py ... --bk ROM` (its assets/ and public/).
//! Every file the Rust import writes must be byte-identical to the Python one and no file may be missing.
//! No ROM or extracted data is committed.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn files(base: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            files(base, &p, out);
        } else {
            out.insert(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
        }
    }
}

#[test]
fn bk_music_export() {
    let Some(rom) = std::env::var_os("BYOR_TEST_BK_ROM") else {
        eprintln!("skipped: BYOR_TEST_BK_ROM not set");
        return;
    };
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (_, z64) = byor_rs::rom::identify(Path::new(&rom)).unwrap().expect("a known ROM");
    let keep = std::env::var_os("BYOR_TEST_BK_MUSIC_OUT");
    let out = match &keep {
        Some(o) => PathBuf::from(o),
        None => std::env::temp_dir().join(format!("byor-bk-music-{}", std::process::id())),
    };
    let decomp = byor_rs::bk::decomp_dir(&source);
    let cat = byor_rs::bk::maps::Catalogue::load(&source).unwrap();
    let assets = byor_rs::bk::Assets::new(&z64);
    byor_rs::bk::music::export(&assets, &cat, &decomp, &z64, &out).unwrap();
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join("audio/music/bk/music.json")).unwrap()).unwrap();
    assert!(json["tracks"].as_array().unwrap().len() > 100, "{} tracks", json["tracks"].as_array().unwrap().len());
    // Spiral Mountain plays COMUSIC_10_SM with the hang-bridge track as the secondary one.
    assert_eq!(json["maps"]["bk_spiral_mountain"]["track"], 0x10);
    assert_eq!(json["maps"]["bk_spiral_mountain"]["secondary"], 0x56);
    assert_eq!(json["maps"]["bk_mumbos_mountain"]["track"], 2);
    assert!(json["maps"].as_object().unwrap().len() > 80);
    let a = std::fs::read(out.join("audio/music/bk/bank.ctl")).unwrap();
    assert_eq!(&a[..4], &[0x42, 0x31, 0, 1], "ALBankFile revision B1");
    if keep.is_none() {
        let _ = std::fs::remove_dir_all(&out);
    }
}

#[test]
fn bk_import_matches_python() {
    let (Some(rom), Some(reference)) = (std::env::var_os("BYOR_TEST_BK_ROM"), std::env::var_os("BYOR_TEST_BK_REF")) else {
        eprintln!("skipped: BYOR_TEST_BK_ROM / BYOR_TEST_BK_REF not set");
        return;
    };
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (game, z64) = byor_rs::rom::identify(Path::new(&rom)).unwrap().expect("a known ROM");
    assert_eq!(game, "bk");
    let stage = std::env::temp_dir().join(format!("byor-bk-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    byor_rs::bk::run(&source, &stage, &z64, &mut |_, _| {}).unwrap();
    let mut got = BTreeSet::new();
    for top in ["assets", "public"] {
        files(&stage, &stage.join(top), &mut got);
    }
    assert!(got.len() > 600, "only {} files written", got.len());
    let reference = PathBuf::from(reference);
    let mut want = BTreeSet::new();
    for top in ["assets", "public"] {
        files(&reference, &reference.join(top), &mut want);
    }
    for f in &got {
        let a = std::fs::read(stage.join(f)).unwrap();
        if f.starts_with("assets/audio/music/bk/") {
            continue; // new in the Rust importer (music), no Python counterpart
        }
        let b = std::fs::read(reference.join(f)).unwrap_or_else(|_| panic!("{f} is not in the Python output"));
        assert!(a == b, "{f} differs from the Python output");
    }
    let missing: Vec<_> = want.difference(&got).filter(|f| !f.starts_with("assets/ui") && !f.starts_with("public/models/quaternius") && !f.starts_with("public/icon") && !f.starts_with("public/favicon") && !f.starts_with("public/manifest") && !f.contains("item-") && !f.contains("kart-voices") && !f.contains("character-sounds")).collect();
    assert!(missing.is_empty(), "files only the Python import writes: {missing:?}");
    let _ = std::fs::remove_dir_all(&stage);
}
