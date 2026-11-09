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
        let b = std::fs::read(reference.join(f)).unwrap_or_else(|_| panic!("{f} is not in the Python output"));
        assert!(a == b, "{f} differs from the Python output");
    }
    let missing: Vec<_> = want.difference(&got).filter(|f| !f.starts_with("assets/ui") && !f.starts_with("public/models/quaternius") && !f.starts_with("public/icon") && !f.starts_with("public/favicon") && !f.starts_with("public/manifest") && !f.contains("item-") && !f.contains("kart-voices") && !f.contains("character-sounds")).collect();
    assert!(missing.is_empty(), "files only the Python import writes: {missing:?}");
    let _ = std::fs::remove_dir_all(&stage);
}
