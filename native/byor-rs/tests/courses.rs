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

/// The whole SM64 course export (`Courses::export` + every post-processor: g1 actors, solids, bosses, castle) must run to the
/// end on the decomp. Needs a data dir whose `assets/` holds the exported GLBs (`BYOR_TEST_DATA_REF`, e.g. a real
/// `byor-rs --sm64` data dir) and optionally `BYOR_TEST_SM64_REF` (the decomp, default `reference/sm64`); no Python reference
/// is compared. Regression test for symbolic `BPARAM2(...)` arguments in the object tables.
#[test]
fn the_whole_sm64_course_export_runs_end_to_end() {
    let Some(data) = std::env::var_os("BYOR_TEST_DATA_REF") else {
        eprintln!("skipped: BYOR_TEST_DATA_REF not set");
        return;
    };
    let data = PathBuf::from(data);
    let decomp = std::env::var_os("BYOR_TEST_SM64_REF").map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../reference/sm64"), PathBuf::from);
    let courses = byor_rs::courses::Courses::new(&decomp, &data.join("assets")).unwrap();
    let out = std::env::temp_dir().join(format!("byor-rs-courses-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let mut log = courses.export(&out).unwrap();
    log.extend(byor_rs::courses_post::run_all(&courses, &out).unwrap());
    assert!(log.len() >= 40);
    let thi: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join("thi.json")).unwrap()).unwrap();
    assert!(!thi["mountainBallSpawners"].as_array().unwrap().is_empty(), "the THI bowling ball spawner is exported");
    let count = |stem: &str, key: &str, value: &str| -> usize {
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join(format!("{stem}.json"))).unwrap()).unwrap();
        v["objects"].as_array().unwrap().iter().filter(|o| o[key] == value).count()
    };
    assert_eq!(count("ccm", "collision", "signpost"), 6, "wooden signposts are placed outside BoB");
    assert!(count("wdw", "behavior", "fireSpitter") >= 8, "macro fire spitters are placed");
    let _ = std::fs::remove_dir_all(&out);
}
