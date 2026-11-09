//! Headless checks of the Banjo-Kazooie music: the sound font parses, every sequence decodes, the tempo / pitch arithmetic of the
//! synth holds on the real bank, every world theme is audible and the course -> track table matches the decomp.
//!
//! They need the importer's output (`assets/audio/music/bk/`, or `$JUMPMAN_TEST_ASSETS/audio/music/bk/`); without it each test
//! says so and passes (a public checkout has no ROM data).
use super::synth::{Bank, Instrument, Player, Seq, RATE};
use super::synth::Event as SeqEvent;
use super::*;

fn dir() -> PathBuf {
    let base = match std::env::var_os("JUMPMAN_TEST_ASSETS").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"),
    };
    base.join(FOLDER)
}

fn library() -> Option<&'static Library> {
    static LIB: OnceLock<Option<Library>> = OnceLock::new();
    LIB.get_or_init(|| match Library::load(&dir()) {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("skipped: no BK music data ({e}); run the importer with --bk");
            None
        }
    })
    .as_ref()
}

fn render(player: &mut Player, seconds: f32) -> Vec<i16> {
    let mut out = Vec::new();
    player.render((seconds * RATE as f32) as usize, &mut out);
    out
}

fn rms(pcm: &[i16]) -> f64 {
    (pcm.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / pcm.len().max(1) as f64).sqrt()
}

#[test]
fn the_sound_font_parses_with_instruments_and_looped_waves() {
    let Some(lib) = library() else { return };
    let b = &lib.bank;
    let used: Vec<&Instrument> = b.instruments.iter().flatten().collect();
    assert!(used.len() > 20, "{} instruments", used.len());
    assert!(used.iter().map(|i| i.sounds.len()).sum::<usize>() > 40);
    assert!(used.iter().flat_map(|i| &i.sounds).any(|s| s.wave.looped.is_some()), "no looped waves");
    assert!(b.rate >= 8000 && b.rate <= 48000, "rate {}", b.rate);
    // the key maps are well formed (a few padding sounds may be empty)
    let all: Vec<_> = used.iter().flat_map(|i| &i.sounds).collect();
    let bad: Vec<_> = all.iter().filter(|s| !(s.keys.key_min <= s.keys.key_max && s.keys.vel_min <= s.keys.vel_max)).map(|s| (s.keys.clone(), s.wave.len)).collect();
    assert!(bad.len() * 10 < all.len(), "{} of {} key maps are inverted: {:?}", bad.len(), all.len(), &bad[..bad.len().min(4)]);
}

#[test]
fn every_sequence_decodes_to_the_end_without_unknown_codes() {
    let Some(lib) = library() else { return };
    let mut notes_total = 0;
    for t in &lib.data.tracks {
        let bytes = std::fs::read(lib.dir.join(&t.file)).unwrap();
        let seq = Seq::parse(&bytes).unwrap_or_else(|e| panic!("track {:#x} {}: {e}", t.id, t.name));
        let events = seq.dump(400_000);
        assert!(!events.is_empty(), "track {:#x} is empty", t.id);
        let notes = events.iter().filter(|e| matches!(e.2, SeqEvent::Midi { status, b2, .. } if status & 0xF0 == 0x90 && b2 != 0)).count();
        notes_total += notes;
        // events come out in time order
        assert!(events.windows(2).all(|w| w[0].0 <= w[1].0), "track {:#x} {}: events out of order", t.id, t.name);
        // either the sequence ends or it loops (a loop end appears)
        let ended = events.last().is_some_and(|e| e.2 == SeqEvent::TrackEnd) || events.iter().any(|e| e.2 == SeqEvent::LoopEnd);
        assert!(ended || events.len() >= 400_000, "track {:#x} {} neither ends nor loops", t.id, t.name);
    }
    assert!(notes_total > 20_000, "{notes_total} notes over all tracks");
}

#[test]
fn tempo_places_notes_on_the_beat() {
    let Some(lib) = library() else { return };
    // 120 bpm (500000 us per quarter), 96 ticks per quarter: a note every 0.5 s.
    let inst = lib.bank.instruments.iter().position(|i| i.as_ref().is_some_and(|i| i.sounds.iter().any(|s| s.wave.looped.is_some()))).unwrap();
    let sound = lib.bank.instruments[inst].as_ref().unwrap().sounds.iter().find(|s| s.wave.looped.is_some()).unwrap();
    let key = sound.keys.key_base.clamp(sound.keys.key_min, sound.keys.key_max);
    let vel = sound.keys.vel_max.max(1);
    let mut t = vec![0x00, 0xFF, 0x51, 0x07, 0xA1, 0x20, 0x00, 0xC0, inst as u8];
    for i in 0..4 {
        t.extend_from_slice(&[if i == 0 { 0 } else { 96 }, 0x90, key, vel, 24]);
    }
    t.extend_from_slice(&[96, 0xFF, 0x2F]);
    let mut file = vec![0u8; 0x44];
    file[3] = 0x44;
    file[0x40..].copy_from_slice(&96u32.to_be_bytes());
    file.extend_from_slice(&t);
    let mut p = Player::new(lib.bank.clone());
    p.play(&file, 0.5).unwrap();
    // 10 ms steps; the time a note starts is when the started-notes counter moves
    let mut onsets = Vec::new();
    let mut seen = 0;
    let mut pcm = Vec::new();
    for step in 0..220 {
        p.render((RATE / 100) as usize, &mut pcm);
        if p.notes_started() > seen {
            seen = p.notes_started();
            onsets.push(step as f64 * 0.01);
        }
    }
    assert_eq!(onsets.len(), 4, "onsets {onsets:?}");
    for (k, o) in onsets.iter().enumerate() {
        assert!((o - 0.5 * k as f64).abs() < 0.021, "onset {k} at {o}: {onsets:?}");
    }
    assert!(rms(&pcm) > 5.0, "the notes are silent");
    // a tempo change halves the spacing: 250000 us per quarter
    let mut t2 = vec![0x00, 0xFF, 0x51, 0x03, 0xD0, 0x90, 0x00, 0xC0, inst as u8];
    for i in 0..4 {
        t2.extend_from_slice(&[if i == 0 { 0 } else { 96 }, 0x90, key, vel, 24]);
    }
    t2.extend_from_slice(&[96, 0xFF, 0x2F]);
    let mut file2 = file[..0x44].to_vec();
    file2.extend_from_slice(&t2);
    let mut q = Player::new(lib.bank.clone());
    q.play(&file2, 0.5).unwrap();
    let mut starts = Vec::new();
    let mut seen = 0;
    for step in 0..120 {
        q.render((RATE / 100) as usize, &mut Vec::new());
        if q.notes_started() > seen {
            seen = q.notes_started();
            starts.push(step as f64 * 0.01);
        }
    }
    assert_eq!(starts.len(), 4, "{starts:?}");
    assert!((starts[3] - 0.75).abs() < 0.021, "{starts:?}");
}

#[test]
fn an_octave_up_halves_the_length_of_a_one_shot() {
    let Some(lib) = library() else { return };
    // a sound without a loop, whose key map leaves room for +12 semitones
    let (inst, sound) = lib
        .bank
        .instruments
        .iter()
        .enumerate()
        .find_map(|(i, inst)| inst.as_ref().and_then(|inst| inst.sounds.iter().find(|s| s.wave.looped.is_none() && s.wave.len > 4000 && s.keys.key_base + 12 <= s.keys.key_max && s.keys.key_base >= s.keys.key_min).map(|s| (i, s.clone()))))
        .expect("a one-shot sound");
    let length = |key: u8| {
        let mut t = vec![0x00, 0xC0, inst as u8, 0x00, 0x90, key, sound.keys.vel_max.max(1), 0, 0x00, 0xFF, 0x2F];
        let mut file = vec![0u8; 0x44];
        file[3] = 0x44;
        file[0x40..].copy_from_slice(&96u32.to_be_bytes());
        file.append(&mut t);
        let mut p = Player::new(lib.bank.clone());
        p.play(&file, 1.0).unwrap();
        let mut frames = 0;
        while p.active_voices() > 0 || frames == 0 {
            render(&mut p, 0.05);
            frames += (0.05 * RATE as f32) as usize;
            assert!(frames < RATE as usize * 60);
        }
        frames
    };
    let (low, high) = (length(sound.keys.key_base), length(sound.keys.key_base + 12));
    let ratio = low as f64 / high as f64;
    assert!((ratio - 2.0).abs() < 0.35, "lengths {low} / {high} = {ratio}");
}

#[test]
fn the_course_table_matches_the_decomp() {
    let Some(lib) = library() else { return };
    let m = &lib.data.maps;
    // sCore2_9B650MapMusicList
    assert_eq!((m["bk_spiral_mountain"].track, m["bk_spiral_mountain"].secondary), (0x10, 0x56));
    assert_eq!(m["bk_mumbos_mountain"].track, 2);
    assert_eq!((m["bk_treasure_trove_cove"].track, m["bk_treasure_trove_cove"].secondary), (5, 0x11));
    assert_eq!(m["bk_bubblegloop_swamp"].track, 6);
    assert_eq!(m["bk_click_clock_wood_summer"].track, 0x29);
    assert_eq!(m["bk_rusty_bucket_bay"].track, 0x33);
    assert!(m.len() >= 90, "{} maps", m.len());
    // every referenced track has a sequence file
    for (course, e) in m {
        for t in [e.track, e.secondary].into_iter().filter(|t| *t > 0) {
            assert!(lib.data.track(t).is_some(), "{course}: track {t:#x} missing");
        }
    }
    assert_eq!(lib.data.course_track("bk_mumbos_mountain"), Some(2));
    assert_eq!(lib.data.track(0x10).unwrap().name, "Training");
}

#[test]
fn every_world_theme_is_audible_and_uses_known_instruments() {
    let Some(lib) = library() else { return };
    for (course, expect_loud) in [("bk_spiral_mountain", true), ("bk_mumbos_mountain", true), ("bk_treasure_trove_cove", true), ("bk_clankers_cavern", true), ("bk_bubblegloop_swamp", true), ("bk_gobis_valley", true), ("bk_freezeezy_peak", true), ("bk_rusty_bucket_bay", true), ("bk_mad_monster_mansion", true), ("bk_click_clock_wood_autumn", true)] {
        let track = lib.data.course_track(course).unwrap_or_else(|| panic!("{course} has no music"));
        let mut p = Player::new(lib.bank.clone());
        lib.start(&mut p, track as u32).unwrap();
        let pcm = render(&mut p, 6.0);
        let level = rms(&pcm);
        eprintln!("{course}: track {track:#x} rms {level:.0}");
        assert!(!expect_loud || level > 300.0, "{course} (track {track:#x}) is nearly silent: rms {level}");
        assert_eq!(p.missing_sounds, 0, "{course}: {} notes had no sound in their instrument", p.missing_sounds);
        assert!(p.active_voices() > 0 || !p.sequence_ended());
        // the mix is not clipping into a square wave
        let clipped = pcm.iter().filter(|v| v.unsigned_abs() > 29000).count();
        assert!(clipped * 50 < pcm.len(), "{course}: {clipped} of {} samples clip", pcm.len());
    }
}

#[test]
fn loops_repeat_forever_and_channel_masks_silence_layers() {
    let Some(lib) = library() else { return };
    // the Mumbo's Mountain theme loops
    let mut p = Player::new(lib.bank.clone());
    lib.start(&mut p, 2).unwrap();
    let full = rms(&render(&mut p, 4.0));
    p.set_channel_mask(0);
    // voices already sounding are not cut, new notes are dropped
    let _ = render(&mut p, 3.0);
    let muted = rms(&render(&mut p, 3.0));
    assert!(muted < full * 0.2, "mask 0 should be near silent: {muted} vs {full}");
    let mut q = Player::new(lib.bank.clone());
    lib.start(&mut q, 2).unwrap();
    let mut loops = 0;
    for _ in 0..200 {
        render(&mut q, 2.0);
        loops = q.loops();
        if loops > 0 {
            break;
        }
    }
    assert!(loops > 0, "no loop in 400 s");
    assert!(q.playing());
}
