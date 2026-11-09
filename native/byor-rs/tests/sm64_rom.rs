//! Real-ROM check of the SM64 extraction against the decomp's own Python/C extraction. Skipped (passes) unless
//!   BYOR_TEST_SM64_ROM = a Super Mario 64 (US) ROM in any byte order, and
//!   BYOR_TEST_SM64_REF  = a decomp tree where `python3 extract_assets.py us` has run (the byte-for-byte reference)
//! are set. ROMs and extracted data are never committed.
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
fn sm64_extraction_is_byte_identical_to_extract_assets_py() {
    let (Some(rom), Some(reference)) = (std::env::var_os("BYOR_TEST_SM64_ROM"), std::env::var_os("BYOR_TEST_SM64_REF")) else {
        eprintln!("skipped: BYOR_TEST_SM64_ROM / BYOR_TEST_SM64_REF not set");
        return;
    };
    let reference = PathBuf::from(reference);
    let (game, z64) = byor_rs::rom::identify(Path::new(&rom)).unwrap().expect("not a recognised ROM");
    assert_eq!(game, "sm64");
    let json = std::fs::read(reference.join("assets.json")).unwrap();
    let out = std::env::temp_dir().join(format!("byor-rs-sm64-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let stats = byor_rs::sm64::extract(&z64, &json, &out, |_, _| {}).unwrap();
    assert_eq!(stats.textures, 1267);
    assert_eq!(stats.raw, 42);
    assert_eq!(stats.sky, 11);
    assert_eq!(stats.sound, 219);
    let mut files = Vec::new();
    walk(&out, &mut files);
    assert_eq!(files.len(), stats.textures + stats.raw + stats.sky + stats.sound);
    for f in &files {
        let rel = f.strip_prefix(&out).unwrap();
        let want = std::fs::read(reference.join(rel)).unwrap_or_else(|_| panic!("{} missing in the reference", rel.display()));
        assert!(std::fs::read(f).unwrap() == want, "{} differs from the Python/C output", rel.display());
    }
    std::fs::remove_dir_all(&out).unwrap();
}

/// `export_audio` against `scripts/export-audio.py` output: BYOR_TEST_SM64_REF as above and BYOR_TEST_AUDIO_REF = the
/// Python `assets/audio` dir (wav files + manifest.json) produced from the same extraction.
#[test]
fn audio_export_matches_export_audio_py() {
    let (Some(reference), Some(audio)) = (std::env::var_os("BYOR_TEST_SM64_REF"), std::env::var_os("BYOR_TEST_AUDIO_REF")) else {
        eprintln!("skipped: BYOR_TEST_SM64_REF / BYOR_TEST_AUDIO_REF not set");
        return;
    };
    let out = std::env::temp_dir().join(format!("byor-rs-audio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let n = byor_rs::sm64::export_audio(&PathBuf::from(reference).join("sound/samples"), &out).unwrap();
    let mut files = Vec::new();
    walk(&out, &mut files);
    assert_eq!(files.len(), n + 1);
    for f in &files {
        let rel = f.strip_prefix(&out).unwrap();
        let want = std::fs::read(PathBuf::from(&audio).join(rel)).unwrap_or_else(|_| panic!("{} missing", rel.display()));
        assert!(std::fs::read(f).unwrap() == want, "{} differs", rel.display());
    }
    std::fs::remove_dir_all(&out).unwrap();
}

/// The music engine against the Python pipeline's `sm64-us.wasm`: BYOR_TEST_SM64_ROM, BYOR_TEST_SM64_REF (extracted decomp
/// tree), BYOR_TEST_DATA_REF (a data dir made by pipeline.py from the same ROM).
#[test]
fn music_wasm_matches_render_music_template() {
    let (Some(rom), Some(reference), Some(data)) = (std::env::var_os("BYOR_TEST_SM64_ROM"), std::env::var_os("BYOR_TEST_SM64_REF"), std::env::var_os("BYOR_TEST_DATA_REF")) else {
        eprintln!("skipped");
        return;
    };
    let reference = PathBuf::from(reference);
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (_, z64) = byor_rs::rom::identify(Path::new(&rom)).unwrap().unwrap();
    let json = std::fs::read(reference.join("assets.json")).unwrap();
    let wasm = byor_rs::sm64::music_wasm(
        &z64,
        &json,
        &reference,
        &reference,
        &std::fs::read(repo.join("scripts/music/sm64-us-template.wasm")).unwrap(),
        &std::fs::read_to_string(repo.join("scripts/music/sm64-us-template.json")).unwrap(),
        &std::fs::read(repo.join("native/byor/sm64-us-sound-player.bin")).unwrap(),
    )
    .unwrap();
    let want = std::fs::read(PathBuf::from(data).join("assets/audio/music/sm64-us.wasm")).unwrap();
    assert_eq!(wasm.len(), want.len());
    assert!(wasm == want, "sm64-us.wasm differs");
}
