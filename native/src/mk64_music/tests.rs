//! Headless checks of the Mario Kart 64 music: every course theme starts its decomp sequence and is audible with
//! musical content (pitches on the equal-tempered scale, the beat of the sequence's tempo), the sequence metadata
//! the ROM exporter parsed (tempo, loop, duration) agrees with what the engine does, and the race cues behave as
//! the decomp's own game-side logic dictates (final lap, star, lightning, finish -> results).
//!
//! The engine is the real sound library, so these tests need `assets/audio/music/mk64/mk64-audio.wasm`
//! (or `$JUMPMAN_TEST_ASSETS/audio/music/mk64/...` to test a BYOR extraction).
use super::courses::*;
use super::engine::{Mk64Module, ENGINE_RATE};
use super::processor::{Cue, Processor};
use serde_json::Value;
use std::f64::consts::PI;
use std::path::PathBuf;

const FPS: usize = 60;

fn assets() -> PathBuf {
    match std::env::var_os("JUMPMAN_TEST_ASSETS").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"),
    }
}

fn processor() -> Processor {
    static MODULE: std::sync::OnceLock<Mk64Module> = std::sync::OnceLock::new();
    let module = MODULE.get_or_init(|| {
        let bytes = std::fs::read(assets().join("audio/music/mk64/mk64-audio.wasm")).expect("mk64-audio.wasm (run scripts/export-mk64-music.py)");
        Mk64Module::load(&bytes).expect("module")
    });
    Processor::new(module.clone())
}

fn sequences() -> Vec<Value> {
    let raw = std::fs::read_to_string(assets().join("audio/music/mk64/sequences.json")).expect("sequences.json");
    serde_json::from_str::<Value>(&raw).unwrap()["sequences"].as_array().unwrap().clone()
}

fn mono(pcm: &[i16]) -> Vec<f64> {
    pcm.chunks(2).map(|c| (c[0] as f64 + c[1] as f64) / 2.0).collect()
}

fn rms(v: &[f64]) -> f64 {
    (v.iter().map(|x| x * x).sum::<f64>() / v.len().max(1) as f64).sqrt()
}

/// In-place radix-2 FFT magnitudes of a Hann-windowed block of `n` samples.
fn spectrum(x: &[f64], n: usize) -> Vec<f64> {
    let mut re: Vec<f64> = (0..n).map(|i| x[i] * (0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos())).collect();
    let mut im = vec![0.0; n];
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
    (0..n / 2).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt()).collect()
}

/// Distance in cents of `hz` from the nearest equal-tempered note (A4 = 440).
fn cents_off(hz: f64) -> f64 {
    let semis = 12.0 * (hz / 440.0).log2();
    (semis - semis.round()).abs() * 100.0
}

/// Strongest bins' frequencies that are a local maximum.
fn peaks(spec: &[f64], n: usize, count: usize) -> Vec<f64> {
    let mut idx: Vec<usize> = (3..spec.len() - 1).filter(|&i| spec[i] > spec[i - 1] && spec[i] >= spec[i + 1]).collect();
    idx.sort_by(|a, b| spec[*b].partial_cmp(&spec[*a]).unwrap());
    idx.into_iter().take(count).map(|i| i as f64 * ENGINE_RATE as f64 / n as f64).collect()
}

/// Normalised autocorrelation of the onset envelope at `lag` seconds.
fn onset_correlation(x: &[f64], lag: f64) -> f64 {
    let hop = 256;
    let env: Vec<f64> = x.chunks(hop).map(|c| rms(c)).collect();
    let d: Vec<f64> = env.windows(2).map(|w| (w[1] - w[0]).max(0.0)).collect();
    let mean = d.iter().sum::<f64>() / d.len() as f64;
    let d: Vec<f64> = d.iter().map(|v| v - mean).collect();
    let k = (lag * ENGINE_RATE as f64 / hop as f64).round() as usize;
    let num: f64 = (0..d.len() - k).map(|i| d[i] * d[i + k]).sum();
    let den: f64 = d.iter().map(|v| v * v).sum();
    num / den.max(1e-9)
}

fn all_dirs() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(assets().join("courses"))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str()?.strip_prefix("mk64_")?.strip_suffix(".json").map(String::from))
        .collect();
    v.sort();
    v
}

#[test]
fn the_theme_table_covers_exactly_the_exported_courses_like_the_decomp_switch() {
    let mut table: Vec<String> = COURSE_THEMES.iter().map(|(d, _)| d.to_string()).collect();
    table.sort();
    assert_eq!(table, all_dirs(), "every exported mk64 course has a theme and no theme is for a missing course");
    // Spot checks against play_music_for_current_track.
    assert_eq!(theme_for("mk64_wario_stadium"), Some(SEQ_RACEWAY));
    assert_eq!(theme_for("yoshi_valley"), Some(SEQ_FARM));
    assert_eq!(theme_for("block_fort"), Some(SEQ_MOUNTAIN));
    assert_eq!(theme_for("big_donut"), Some(SEQ_BATTLE));
    assert_eq!(theme_for("nowhere"), None);
    assert_eq!((decomp_rank(0, 4), decomp_rank(1, 4), decomp_rank(2, 4), decomp_rank(3, 4)), (0, 2, 4, 6));
}

#[test]
fn every_course_theme_starts_its_sequence_with_the_right_tempo_and_is_audible() {
    let seqs = sequences();
    let mut heard = 0;
    for (dir, seq) in COURSE_THEMES {
        let mut p2 = processor();
        let mut pcm = p2.render_offline(&[Cue::Theme(dir.to_string())], 8);
        let [p0, _] = p2.status().expect("engine running");
        assert!(p0.enabled, "{dir}: player 0 not running");
        assert_eq!(p0.sequence, seq, "{dir}: wrong sequence");
        let expected = seqs[seq as usize]["tempo"].as_u64().unwrap() as u32;
        assert_eq!(p0.tempo, expected, "{dir}: tempo of sequence {seq}");
        pcm.extend(p2.render_offline(&[], 5 * FPS));
        let m = mono(&pcm);
        let peak = pcm.iter().map(|v| (*v as i32).abs()).max().unwrap();
        let level = rms(&m[ENGINE_RATE as usize..]);
        assert!(peak > 3000, "{dir}: silent (peak {peak})");
        assert!(level > 600.0, "{dir}: too quiet (rms {level:.0})");
        let clipped = pcm.iter().filter(|v| v.unsigned_abs() >= 32767).count() as f64 / pcm.len() as f64;
        assert!(clipped < 0.02, "{dir}: {:.1}% of the samples clip", clipped * 100.0);
        println!("{dir:19} seq {seq:2} tempo {} peak {peak:5} rms {level:5.0} clip {:.2}%", p0.tempo, clipped * 100.0);
        assert_eq!(p2.unknown_ops(), 0, "{dir}: the mixer met a microcode command it does not know");
        heard += 1;
    }
    assert_eq!(heard, 20);
}

#[test]
fn themes_have_musical_pitch_content_and_the_beat_of_their_tempo() {
    let seqs = sequences();
    let n = 1 << 14;
    for seq in [SEQ_RACEWAY, SEQ_FARM, SEQ_MOUNTAIN, SEQ_BEACH, SEQ_SNOW, SEQ_DESERT, SEQ_RAINBOW, SEQ_JUNGLE, SEQ_TURNPIKE] {
        let dir = COURSE_THEMES.iter().find(|(_, s)| *s == seq).unwrap().0;
        let mut p = processor();
        let pcm = p.render_offline(&[Cue::Theme(dir.to_string())], 14 * FPS);
        let x = mono(&pcm);
        // Pitch: the loudest spectral peaks of a two-second window sit on the equal-tempered scale (a wrong
        // sample rate or resampler would put them a constant ratio off).
        let start = 4 * ENGINE_RATE as usize;
        let spec = spectrum(&x[start..start + n], n);
        let top = peaks(&spec, n, 8);
        let on_scale = top.iter().filter(|f| **f > 60.0 && **f < 4000.0 && cents_off(**f) < 35.0).count();
        assert!(on_scale >= 4, "{dir} (seq {seq}): loudest peaks {top:?} are not on the musical scale");
        // Most of the energy is musical (below 5 kHz), not noise or a whine.
        let total: f64 = spec.iter().map(|v| v * v).sum();
        let low: f64 = spec.iter().take((5000.0 / (ENGINE_RATE as f64 / n as f64)) as usize).map(|v| v * v).sum();
        assert!(low / total > 0.8, "{dir}: only {:.0}% of the energy is below 5 kHz", 100.0 * low / total);
        // Rhythm: the onset envelope repeats at the beat (or a small multiple of it) of the sequence's tempo.
        let tempo = seqs[seq as usize]["tempo"].as_f64().unwrap();
        let beat = 60.0 / tempo;
        let best = (1..=8).map(|k| onset_correlation(&x, beat * k as f64)).fold(f64::MIN, f64::max);
        let off = onset_correlation(&x, beat * 1.37 + 0.011);
        println!("seq {seq:2} {dir:19} tempo {tempo:3} beat corr {best:.2} (off-beat {off:.2}) low-band {:.0}% peaks {:?}", 100.0 * low / total, top.iter().take(5).map(|f| f.round()).collect::<Vec<_>>());
        assert!(best > 0.12 && best > off + 0.05, "{dir}: no beat at {tempo} bpm (corr {best:.2} vs off-beat {off:.2})");
    }
}

#[test]
fn sequence_metadata_agrees_with_the_engine_tempo_loop_and_duration() {
    let seqs = sequences();
    assert_eq!(seqs.len(), SEQ_COUNT as usize);
    for s in &seqs {
        let id = s["id"].as_u64().unwrap() as u32;
        let tempo = s["tempo"].as_u64().unwrap();
        assert!((40..=200).contains(&tempo), "sequence {id} tempo {tempo}");
        if s["loops"].as_bool().unwrap() && s["ticks"].as_u64().unwrap() < 100000 {
            let (start, len) = (s["loop_start_ticks"].as_u64().unwrap(), s["loop_ticks"].as_u64().unwrap());
            assert!(len > 0 && start + len <= s["ticks"].as_u64().unwrap(), "sequence {id} loop {start}+{len}");
        }
    }
    // Jingles that end: the player stops when the parsed length says it does.
    for id in [SEQ_RACE_STARTING, SEQ_FINAL_LAP, 13, 14, SEQ_FINISH_LOSE, 22] {
        let s = &seqs[id as usize];
        assert!(!s["loops"].as_bool().unwrap(), "sequence {id}");
        // The parsed length is the main script's; the sequence ends when that script and its channels do.
        let secs = s["ticks"].as_f64().unwrap() * 60.0 / (s["tempo"].as_f64().unwrap() * 48.0);
        let mut p = processor();
        p.apply(&Cue::Theme("mario_raceway".into())); // brings the engine up
        p.render_offline(&[], 4);
        // The jingle on player 1.
        if let Some(e) = p.engine_for_test() {
            e.event(id);
        }
        let frames = ((secs + 3.0) * FPS as f64) as usize;
        let mut ended_at = None;
        for f in 0..frames {
            p.render_offline(&[], 1);
            let [_, p1] = p.status().unwrap();
            if f > 10 && !p1.enabled && ended_at.is_none() {
                ended_at = Some(f as f64 / FPS as f64);
            }
        }
        let t = ended_at.unwrap_or_else(|| panic!("sequence {id} never ended"));
        assert!((t - secs).abs() < 1.5, "sequence {id}: ended at {t:.1}s, parsed {secs:.1}s");
    }
    // A short looping sequence (the star theme) is still running after several loop lengths.
    let star = &seqs[SEQ_STAR as usize];
    assert!(star["loops"].as_bool().unwrap());
    let secs = star["loop_ticks"].as_f64().unwrap() * 60.0 / (star["tempo"].as_f64().unwrap() * 48.0);
    let mut p = processor();
    p.render_offline(&[Cue::Theme("mario_raceway".into())], 4);
    p.engine_for_test().unwrap().event(SEQ_STAR);
    p.render_offline(&[], (secs * 3.0 * FPS as f64) as usize);
    assert!(p.status().unwrap()[1].enabled, "the star theme should loop");
}

#[test]
fn race_cues_follow_the_decomp_logic_final_lap_star_lightning_and_finish() {
    let mut p = processor();
    let secs = |s: f64| (s * FPS as f64) as usize;
    // Pre-race fanfare, then the theme on GO.
    p.render_offline(&[Cue::Fanfare], secs(1.0));
    let [a, b] = p.status().unwrap();
    assert!(b.enabled && b.sequence == SEQ_RACE_STARTING && !a.enabled, "fanfare on player 1: {a:?} {b:?}");
    p.render_offline(&[Cue::Theme("luigi_raceway".into())], secs(6.0));
    let [a, _] = p.status().unwrap();
    assert!(a.enabled && a.sequence == SEQ_RACEWAY);
    let tempo = a.tempo;
    // Final lap: jingle on player 1 with the theme muted under it, then the theme again, faster.
    p.render_offline(&[Cue::FinalLap], secs(1.0));
    let [a, b] = p.status().unwrap();
    assert!(b.enabled && b.sequence == SEQ_FINAL_LAP && a.volume < 0.2, "final lap jingle: {a:?} {b:?}");
    p.render_offline(&[], secs(8.0));
    let [a, b] = p.status().unwrap();
    assert!(a.enabled && a.sequence == SEQ_RACEWAY && a.volume > 0.5 && !b.enabled, "theme back after the jingle: {a:?} {b:?}");
    let ratio = a.tempo as f64 / tempo as f64;
    assert!((ratio - 1.25).abs() < 0.02, "final-lap tempo {} vs {tempo}", a.tempo);
    // Star: its theme on player 1, the course theme silent, then back.
    p.render_offline(&[Cue::Star(true)], secs(2.0));
    let [a, b] = p.status().unwrap();
    assert!(b.enabled && b.sequence == SEQ_STAR && a.volume < 0.2, "star: {a:?} {b:?}");
    p.render_offline(&[Cue::Star(false)], secs(3.0));
    let [a, b] = p.status().unwrap();
    assert!(a.enabled && a.volume > 0.5 && !b.enabled, "after the star: {a:?} {b:?}");
    // Lightning: the theme keeps playing, quieter.
    p.render_offline(&[Cue::Lightning(true)], secs(2.0));
    assert!(p.status().unwrap()[0].enabled);
    p.render_offline(&[Cue::Lightning(false)], secs(3.0));
    // Finish first: the winner's fanfare, then the results theme.
    p.render_offline(&[Cue::Finish(0)], secs(1.0));
    let [a, b] = p.status().unwrap();
    assert!(b.enabled && b.sequence == 13 && !a.enabled, "first place fanfare: {a:?} {b:?}");
    p.render_offline(&[], secs(12.0));
    let [_, b] = p.status().unwrap();
    assert!(b.enabled && b.sequence == SEQ_RESULTS_WIN, "results theme: {b:?}");
    // Finish last: the losing fanfare and the sad results theme.
    let mut q = processor();
    q.render_offline(&[Cue::Theme("luigi_raceway".into())], secs(2.0));
    q.render_offline(&[Cue::Finish(6)], secs(1.0));
    let [_, b] = q.status().unwrap();
    assert_eq!(b.sequence, SEQ_FINISH_LOSE);
    q.render_offline(&[], secs(14.0));
    assert_eq!(q.status().unwrap()[1].sequence, SEQ_RESULTS_LOSE);
}

#[test]
fn stop_fades_out_resets_and_the_music_can_start_again() {
    let mut p = processor();
    p.render_offline(&[Cue::Theme("rainbow_road".into())], 2 * FPS);
    assert!(p.is_active());
    let pcm = p.render_offline(&[Cue::Stop], 2 * FPS);
    assert!(!p.is_active(), "the library is dropped after the fade");
    let tail = mono(&pcm[pcm.len().saturating_sub(2 * 2000)..]);
    assert!(rms(&tail) < 300.0, "faded out");
    let pcm = p.render_offline(&[Cue::Theme("rainbow_road".into())], 3 * FPS);
    assert!(p.status().unwrap()[0].sequence == SEQ_RAINBOW && rms(&mono(&pcm)) > 500.0);
    // Starting something new during a fade drops the old music at once.
    p.render_offline(&[Cue::Stop], 10);
    p.render_offline(&[Cue::Theme("bowsers_castle".into())], 10);
    assert_eq!(p.status().unwrap()[0].sequence, SEQ_CASTLE);
    assert!(p.status().unwrap()[0].volume > 0.4, "the new theme is not fading");
}

#[test]
fn the_same_theme_on_a_neighbour_course_carries_on_without_restarting() {
    let mut p = processor();
    p.render_offline(&[Cue::Theme("mario_raceway".into())], 5 * FPS);
    let before = p.status().unwrap()[0];
    p.render_offline(&[Cue::Theme("luigi_raceway".into())], 1);
    let after = p.status().unwrap()[0];
    assert!(after.enabled && after.sequence == SEQ_RACEWAY && (after.volume - before.volume).abs() < 0.05, "{before:?} {after:?}");
    // A different theme does replace it.
    p.render_offline(&[Cue::Theme("moo_moo_farm".into())], 10);
    assert_eq!(p.status().unwrap()[0].sequence, SEQ_FARM);
}
