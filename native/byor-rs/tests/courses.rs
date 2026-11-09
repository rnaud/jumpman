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
    let log = courses.export(&out).unwrap();
    assert!(log.len() >= 40);
    let want_dir = std::env::var_os("BYOR_TEST_COURSES_REF").map_or_else(|| data.join("assets/courses"), PathBuf::from);
    let mut bad = Vec::new();
    let mut n = 0;
    for e in std::fs::read_dir(&want_dir).unwrap().flatten() {
        let name = e.file_name();
        n += 1;
        match std::fs::read(out.join(&name)) {
            Ok(got) if got == std::fs::read(e.path()).unwrap() => {}
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
