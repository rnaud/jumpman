//! sounds.json / courses.json against the checked-in files (original, non-ROM data produced by render-music.py).
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn check(name: &str, got: Result<String, String>) {
    let want = std::fs::read_to_string(root().join("assets/audio/music").join(name)).expect("checked-in file");
    assert!(got.unwrap() == want, "{name} differs");
}

#[test]
fn sounds_json_matches() {
    check("sounds.json", byor_rs::music::sounds_json(&root().join("reference/sm64")));
}

#[test]
fn courses_json_matches() {
    check("courses.json", byor_rs::music::courses_json(&root().join("reference/sm64")));
}
