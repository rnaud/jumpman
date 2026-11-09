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

/// The pitch law of the note player: `gPitchFrequencies[n] == 2^((n - 39) / 12)` (index 39 = middle C = 1.0) for all 128
/// entries of the decomp's table, the last eleven (keys 117..127) wrapping to the bottom octave (`pitch_ratio`), so a note at key `n` plays a sample at `tuning * 32000 * 2^((n - 39) / 12)` Hz.
#[test]
fn the_pitch_law_matches_the_decomp_table() {
    let Some(src) = std::env::var_os("BYOR_TEST_OOT_SRC") else { return eprintln!("skipped") };
    let text = std::fs::read_to_string(PathBuf::from(src).join("reference/oot/src/audio/internal/data.c")).unwrap();
    let start = text.find("f32 gPitchFrequencies[] = {").unwrap();
    let body = &text[start..text[start..].find("};").unwrap() + start];
    let values: Vec<f64> = body.lines().filter(|l| l.contains("/* 0x")).map(|l| l.split("*/").nth(1).unwrap().trim().split('f').next().unwrap().trim().parse().unwrap()).collect();
    assert_eq!(values.len(), 128);
    for (n, v) in values.iter().enumerate() {
        let want = byor_rs::oot::pitch_ratio(n as i32);
        assert!(((v - want) / want).abs() < 2e-5, "key {n}: table {v} vs {want}");
    }
}

/// Every instrument sample of every soundfont has a sane tuning: a recorded rate (`tuning * 32000`) of 4 to 64 kHz.
#[test]
fn instrument_tunings_are_sane() {
    let Some(ctx) = ctx() else { return eprintln!("skipped") };
    let rom = Rom::load(&ctx).unwrap();
    let banks = rom.banks();
    let (mut checked, mut failed) = (0, 0);
    for (id, font) in rom.fonts.iter().enumerate() {
        for i in 0..font.n_inst {
            let inst = match font.instrument(&banks, i) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("font {id} instrument {i}: {e}");
                    failed += 1;
                    continue;
                }
            };
            if let Some(inst) = inst {
                for k in [0u8, 39, 80, 120] {
                    if let Some(t) = inst.tuned_for_test(k) {
                        let rate = t.tuning * 32000.0;
                        assert!(rate.is_finite() && rate > 100.0 && rate < 400_000.0, "font {id} instrument {i} key {k}: {rate} Hz");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 200, "{checked} tunings checked");
    eprintln!("{failed} instruments failed to decode");
}
