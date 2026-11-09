//! export-courses / sm64_world / g2 ports against a data dir made by `pipeline.py` (BYOR_TEST_DATA_REF). The GLB library of
//! that data dir stands in for the second export pass (meshless decor is dropped only once the models exist).
use std::path::PathBuf;

#[test]
fn courses_match_the_python_export() {
    let Some(data) = std::env::var_os("BYOR_TEST_DATA_REF") else {
        eprintln!("skipped: BYOR_TEST_DATA_REF not set");
        return;
    };
    let data = PathBuf::from(data);
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let courses = byor_rs::courses::Courses::new(&repo.join("reference/sm64"), &data.join("assets")).unwrap();
    let out = std::env::var_os("BYOR_TEST_OUT").map_or_else(|| std::env::temp_dir().join(format!("byor-rs-courses-{}", std::process::id())), PathBuf::from);
    let _ = std::fs::remove_dir_all(&out);
    let mut log = courses.export(&out).unwrap();
    if std::env::var_os("BYOR_TEST_POST").is_some() {
        log.extend(byor_rs::courses_post::run_all(&courses, &out).unwrap());
    }
    assert!(log.len() >= 40);
    let want_dir = std::env::var_os("BYOR_TEST_COURSES_REF").map_or_else(|| data.join("assets/courses"), PathBuf::from);
    let mut bad = Vec::new();
    let mut n = 0;
    for e in std::fs::read_dir(&want_dir).unwrap().flatten() {
        let name = e.file_name();
        n += 1;
        match std::fs::read(out.join(&name)) {
            Ok(got) if got == std::fs::read(e.path()).unwrap() => {}
            Ok(got) if only_rust_additions(&got, &std::fs::read(e.path()).unwrap()) => {}
            Ok(_) => bad.push(format!("DIFF {}", name.to_string_lossy())),
            Err(_) => bad.push(format!("MISSING {}", name.to_string_lossy())),
        }
    }
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), n, "file count differs");
    assert!(bad.is_empty(), "{} of {n} differ: {:?}", bad.len(), &bad[..bad.len().min(12)]);
    if std::env::var_os("BYOR_TEST_OUT").is_none() {
        let _ = std::fs::remove_dir_all(&out);
    }
}

/// Behaviors only the Rust exporter places (new ports; parity with the frozen Python exporter is no longer required).
const RUST_ONLY: [&str; 4] = ["madPiano", "hauntedChair", "flyingBookend", "bookendSpawn"];

/// True when `got` equals `want` once the Rust-only objects are removed from it.
fn only_rust_additions(got: &[u8], want: &[u8]) -> bool {
    let (Ok(mut got), Ok(want)) = (serde_json::from_slice::<serde_json::Value>(got), serde_json::from_slice::<serde_json::Value>(want)) else { return false };
    if let Some(objects) = got.get_mut("objects").and_then(|o| o.as_array_mut()) {
        objects.retain(|o| !RUST_ONLY.iter().any(|k| o["behavior"] == *k));
    }
    got == want
}
