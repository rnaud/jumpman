//! Headless checks of the Mario Kart 64 sound effects. The engine is the real sound library (the decomp's
//! `src/audio/*.c` in wasm), so these need `assets/audio/music/mk64/mk64-audio.wasm` built with the `sfx_*`
//! exports. The tests render a few seconds of the stereo output and check it is non-silent, bounded, and has the
//! pitch behaviour the decomp's logic dictates (engine pitch against speed). They cannot say whether it sounds right
//! to a listener; they say the right sound ids reach the mixer and are audible. `survey` and `bank_survey` are
//! ignored explorations that print a table.
use super::engine::ENGINE_RATE;
use super::processor::{Cue, Processor};
use super::sfx::{flags, id, voice, PlayerIn, SfxCmd};
use super::tests::{mono, peaks, processor, rms, spectrum};

pub const FPS: usize = 60;

/// A kart driving at `current_speed` (decomp units; Mario's top speed is about 320) with the throttle down.
pub fn kart(character: u32, human: bool, current_speed: f32, throttle: bool) -> PlayerIn {
    PlayerIn {
        character,
        human,
        kart_props: if throttle { flags::THROTTLE } else { 0 },
        speed: current_speed / 30.0,
        current_speed,
        engine_speed: super::sfx::engine_speed(current_speed as f64, false) as f32,
        ..PlayerIn::default()
    }
}

pub fn session(p: &mut Processor) {
    p.apply(&Cue::Sfx(SfxCmd::Begin { course: 0, humans: 1 }));
}

/// Render `frames` video frames, resending `kart` as player 0 before each.
pub fn drive(p: &mut Processor, k: PlayerIn, frames: usize) -> Vec<i16> {
    let mut out = vec![];
    for _ in 0..frames {
        p.apply(&Cue::Sfx(SfxCmd::Player { slot: 0, kart: k }));
        if let Some(b) = p.render() {
            out.extend_from_slice(b);
        }
    }
    out
}

pub fn peak(pcm: &[i16]) -> i32 {
    pcm.iter().map(|v| (*v as i32).abs()).max().unwrap_or(0)
}

/// Render `frames` frames with nothing new asked.
pub fn run(p: &mut Processor, frames: usize) -> Vec<i16> {
    let mut out = vec![];
    for _ in 0..frames {
        if let Some(b) = p.render() {
            out.extend_from_slice(b);
        }
    }
    out
}

fn parked(p: &mut Processor) {
    // A parked human kart so voices know their character; engines off.
    p.apply(&Cue::Sfx(SfxCmd::Player { slot: 0, kart: kart(0, true, 0.0, false) }));
}

/// Exploration: prints the level of every sound id (run with `-- --ignored --nocapture`).
#[test]
#[ignore = "exploration, prints a table"]
fn survey() {
    let ids: Vec<(&str, u32)> = vec![
        ("HOP", id::HOP), ("BUMP", id::BUMP), ("STOP", id::STOP), ("SHELL_FIRE", id::SHELL_FIRE), ("SPLASH", id::SPLASH),
        ("BANANA_DROP", id::BANANA_DROP), ("DRIFT_READY", id::DRIFT_READY), ("EXPLOSION", id::EXPLOSION),
        ("EXPLOSION_2", id::EXPLOSION_2), ("HIT", id::HIT), ("LAP", id::LAP), ("STAR_BUMP", id::STAR_BUMP),
        ("FINISH", id::FINISH), ("BOOST", id::BOOST), ("RAMP", id::RAMP), ("LAKITU_APPEAR", id::LAKITU_APPEAR),
        ("LAKITU_DROP", id::LAKITU_DROP), ("ITEM_BOX", id::ITEM_BOX), ("ITEM_ROULETTE", id::ITEM_ROULETTE),
        ("ITEM_NONE", id::ITEM_NONE), ("BALLOON", id::BALLOON),
    ];
    for (name, bits) in ids {
        let mut p = processor();
        session(&mut p);
        parked(&mut p);
        run(&mut p, 5);
        p.apply(&Cue::Sfx(SfxCmd::PlayerSound { slot: 0, bits }));
        let pcm = run(&mut p, FPS * 2);
        let has = p.engine_for_test().map_or(false, |e| e.sfx_bank_has(bits));
        let m = mono(&pcm);
        println!("{name:14} {bits:08X} peak {:6} rms {:7.1} in-bank-after-2s {has}", peak(&pcm), rms(&m));
    }
    for (name, bits) in [("COUNTDOWN", id::COUNTDOWN_LIGHT), ("GREEN", id::GREEN_LIGHT), ("REV", id::REV_ENGINE), ("REV2", id::REV_ENGINE_2), ("PING", id::PING),
        ("M_CURSOR", id::MENU_CURSOR), ("M_SELECT", id::MENU_SELECT), ("M_BACK", id::MENU_BACK), ("M_OK", id::MENU_OK), ("M_ERR", id::MENU_ERROR), ("M_ENTER", id::MENU_ENTER), ("SCORE", id::SCORE_COUNT)] {
        let mut p = processor();
        session(&mut p);
        run(&mut p, 5);
        p.apply(&Cue::Sfx(SfxCmd::Global { bits }));
        let pcm = run(&mut p, FPS * 2);
        println!("{name:14} {bits:08X} peak {:6} rms {:7.1}", peak(&pcm), rms(&mono(&pcm)));
    }
    for ch in 0..8u32 {
        for sub in [0u32, 1, 2, 3, 4, 5, 8, 0xC, 0xD] {
            let mut p = processor();
            session(&mut p);
            p.apply(&Cue::Sfx(SfxCmd::Player { slot: 0, kart: kart(ch, true, 0.0, false) }));
            run(&mut p, 5);
            p.apply(&Cue::Sfx(SfxCmd::Voice { slot: 0, sub }));
            let pcm = run(&mut p, FPS * 2);
            print!("{}:{sub:X}={}/{:.0} ", voice::CHARACTERS[ch as usize], peak(&pcm), rms(&mono(&pcm)));
        }
        println!();
    }
}

/// Exploration: prints which ids of each bank are silent (run with `-- --ignored --nocapture`).
#[test]
#[ignore = "exploration, prints a table"]
fn bank_survey() {
    for (bank, pri) in [(0x49u32, 0x8000u32), (0x29, 0x8000), (0x19, 0x8000), (0x31, 0x8000), (0x51, 0x8000), (0x01, 0x8000)] {
        let mut line = String::new();
        for sub in 0..0x50u32 {
            let mut p = processor();
            session(&mut p);
            p.apply(&Cue::Sfx(SfxCmd::Player { slot: 0, kart: kart(0, true, 0.0, false) }));
            run(&mut p, 3);
            let bits = (bank << 24) | pri | sub;
            if bank == 0x49 {
                p.apply(&Cue::Sfx(SfxCmd::Global { bits }));
            } else if bank == 0x29 || bank == 0x19 || bank == 0x01 {
                p.apply(&Cue::Sfx(SfxCmd::PlayerSound { slot: 0, bits }));
            } else {
                p.apply(&Cue::Sfx(SfxCmd::Place { slot: 1, pos: [0.; 3], vel: [0.; 3] }));
                p.apply(&Cue::Sfx(SfxCmd::PlaceSound { slot: 1, bits }));
            }
            let pcm = run(&mut p, FPS);
            if peak(&pcm) == 0 {
                line += &format!("{sub:X} ");
            }
        }
        println!("bank {bank:X} silent ids: {line}");
    }
}

/// The sound library of the real ROM data is there; without it (no `assets/audio/music/mk64`) the tests below skip.
fn real() -> Option<Processor> {
    let path = super::tests::assets().join("audio/music/mk64/mk64-audio.wasm");
    if !path.exists() {
        return None;
    }
    let mut p = processor();
    session(&mut p);
    // Starting a session instantiates the library; it reports back whether it has the `sfx_*` exports.
    assert!(p.sfx_ok, "mk64-audio.wasm has no sound effects: rebuild it with scripts/export-mk64-music.py from the checked-in template");
    Some(p)
}

#[test]
fn global_sounds_are_audible_bounded_and_end() {
    let Some(mut p) = real() else { return };
    session(&mut p);
    run(&mut p, 5);
    for (name, bits) in [("green light", id::GREEN_LIGHT), ("menu cursor", id::MENU_CURSOR), ("countdown", id::COUNTDOWN_LIGHT)] {
        p.apply(&Cue::Sfx(SfxCmd::Global { bits }));
        let pcm = run(&mut p, FPS);
        assert!(peak(&pcm) > 200, "{name} is silent ({})", peak(&pcm));
        assert!(peak(&pcm) < 32767, "{name} clips");
        let tail = run(&mut p, FPS * 3);
        assert!(peak(&tail[tail.len() - 4000..]) < peak(&pcm) / 4 + 50, "{name} does not die away");
    }
}

#[test]
fn a_kart_sound_and_the_characters_voices_are_audible() {
    let Some(mut p) = real() else { return };
    session(&mut p);
    parked(&mut p);
    run(&mut p, 5);
    p.apply(&Cue::Sfx(SfxCmd::PlayerSound { slot: 0, bits: id::HOP }));
    assert!(peak(&run(&mut p, FPS)) > 200, "the hop sound is silent");
    // Every racer has its own voice for the spin-out and the boost.
    let mut peaks_by_racer = vec![];
    for ch in 0..8u32 {
        let mut q = real().expect("library");
        session(&mut q);
        q.apply(&Cue::Sfx(SfxCmd::Player { slot: 0, kart: kart(ch, true, 0.0, false) }));
        run(&mut q, 5);
        q.apply(&Cue::Sfx(SfxCmd::Voice { slot: 0, sub: voice::SPINOUT }));
        let pcm = run(&mut q, FPS * 2);
        assert!(peak(&pcm) > 200, "{} has no spin-out voice", voice::CHARACTERS[ch as usize]);
        peaks_by_racer.push(rms(&mono(&pcm)));
    }
    peaks_by_racer.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert!(peaks_by_racer.len() > 4, "the racers' voices differ: {peaks_by_racer:?}");
}

#[test]
fn the_engine_note_rises_with_the_speed() {
    let Some(mut p) = real() else { return };
    session(&mut p);
    p.apply(&Cue::Sfx(SfxCmd::Player { slot: 0, kart: kart(0, true, 0.0, false) }));
    p.apply(&Cue::Sfx(SfxCmd::Engine { slot: 0, on: true }));
    let mut last = 0.0f32;
    for speed in [20.0f32, 120.0, 220.0, 320.0] {
        let pcm = drive(&mut p, kart(0, true, speed, true), FPS * 2);
        let pitch = p.engine_for_test().map_or(0.0, |e| e.sfx_engine_pitch(0));
        assert!(peak(&pcm) > 100, "engine silent at {speed}");
        assert!(pitch > last, "pitch {pitch} at {speed} is not above {last}");
        last = pitch;
    }
}
