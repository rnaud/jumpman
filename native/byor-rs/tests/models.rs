//! export-sm64.py port against a pipeline.py data dir. Needs BYOR_TEST_SM64_REF (decomp tree with the extraction output) and
//! BYOR_TEST_DATA_REF; writes into BYOR_TEST_MODELS_OUT (default: a temp dir).
use std::path::{Path, PathBuf};

fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, out);
        } else {
            out.push(p.strip_prefix(base).unwrap().to_path_buf());
        }
    }
}

#[test]
fn models_match_export_sm64_py() {
    let (Some(refd), Some(data)) = (std::env::var_os("BYOR_TEST_SM64_REF"), std::env::var_os("BYOR_TEST_DATA_REF")) else {
        eprintln!("skipped");
        return;
    };
    let (refd, data) = (PathBuf::from(refd), PathBuf::from(data));
    let out = std::env::var_os("BYOR_TEST_MODELS_OUT").map_or_else(|| std::env::temp_dir().join(format!("byor-rs-models-{}", std::process::id())), PathBuf::from);
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(out.join("assets/courses")).unwrap();
    for e in std::fs::read_dir(data.join("assets/courses")).unwrap().flatten() {
        std::fs::copy(e.path(), out.join("assets/courses").join(e.file_name())).unwrap();
    }
    let json = std::fs::read(refd.join("assets.json")).unwrap();
    byor_rs::models::export_all(&refd, &refd, &out, &json, &mut |l| {
        if l.starts_with("FAILED") {
            eprintln!("{l}");
        }
    })
    .unwrap();
    let mut files = Vec::new();
    walk(&out.join("assets"), &out.join("assets"), &mut files);
    let mut bad = Vec::new();
    let mut checked = 0;
    for rel in &files {
        if rel.starts_with("courses") {
            continue;
        }
        checked += 1;
        match std::fs::read(data.join("assets").join(rel)) {
            Ok(w) if w == std::fs::read(out.join("assets").join(rel)).unwrap() => {}
            Ok(_) => bad.push(format!("DIFF {}", rel.display())),
            Err(_) => bad.push(format!("EXTRA {}", rel.display())),
        }
    }
    let mut want = Vec::new();
    for top in ["actors", "levels", "extracted"] {
        walk(&data.join("assets").join(top), &data.join("assets"), &mut want);
    }
    let missing = want.iter().filter(|r| !out.join("assets").join(r).exists()).count();
    assert!(bad.is_empty() && missing == 0, "{} of {checked} differ, {missing} missing: {:?}", bad.len(), &bad[..bad.len().min(15)]);
}
