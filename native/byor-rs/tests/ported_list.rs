//! `native/byor/ported-games.txt` (read by the packaging scripts to decide whether Python is bundled) must list exactly the
//! games the crate says it can import end to end.
#[test]
fn ported_games_file_matches_the_crate() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../byor/ported-games.txt")).unwrap();
    let mut listed: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    listed.sort();
    let mut ported: Vec<&str> = byor_rs::pipeline::PORTED.iter().map(|(g, _)| *g).collect();
    ported.sort();
    assert_eq!(listed, ported, "update native/byor/ported-games.txt together with PORTED in byor-rs/src/pipeline.rs");
    for (g, _) in byor_rs::pipeline::PORTED {
        assert!(byor_rs::rom::ROMS.iter().any(|r| r.id == g), "{g} is not a known game");
        assert!(byor_rs::pipeline::exporter_hash(g).is_some());
    }
}
