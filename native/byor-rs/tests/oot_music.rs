//! OoT music against the real ROM. Needs BYOR_TEST_OOT_ROM (US 1.2) and BYOR_TEST_OOT_SRC (a source root with reference/oot);
//! skipped when unset.
use byor_rs::oot::music::{self, Rom};
use byor_rs::oot::Ctx;
use std::path::{Path, PathBuf};

fn ctx() -> Option<Ctx> {
    let (rom, src) = (std::env::var_os("BYOR_TEST_OOT_ROM")?, std::env::var_os("BYOR_TEST_OOT_SRC")?);
    let (_, z64) = byor_rs::rom::identify(Path::new(&rom)).unwrap().expect("not a recognised ROM");
    Some(Ctx::new(&PathBuf::from(src), &z64).unwrap())
}

#[test]
fn tables_are_found_and_sizes_match_the_decomp() {
    let Some(ctx) = ctx() else { return eprintln!("skipped") };
    let rom = Rom::load(&ctx).unwrap();
    assert_eq!(rom.tables.seqs.len(), 110);
    assert_eq!(rom.names.len(), 110);
    assert_eq!(rom.tables.fonts.len(), 38);
    assert_eq!(rom.tables.seq_fonts.len(), 110);
    assert!(rom.tables.banks.len() >= 6, "{} sample banks", rom.tables.banks.len());
}

#[test]
fn a_dungeon_theme_renders_audible_notes_and_a_loop() {
    let Some(ctx) = ctx() else { return eprintln!("skipped") };
    let rom = Rom::load(&ctx).unwrap();
    let id = rom.names.iter().position(|n| n == "NA_BGM_INSIDE_DEKU_TREE").unwrap();
    let song = rom.song(id, music::MAX_SECONDS).unwrap();
    eprintln!("deku tree: {} notes, {:.1}s, loop {:?}, runseq {:?}", song.notes.len(), song.length, song.looping, song.runseq);
    assert!(song.notes.len() > 50);
    let pcm = music::render(&song);
    let peak = pcm.iter().map(|x| x.abs()).max().unwrap();
    assert!(peak > 5000, "peak {peak}");
}
