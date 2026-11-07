//! Audio differential against `tests/rustFixtures.audio.gen.test.ts`: the scenes of `tests/audioScenes.json` are played
//! through the Rust `Sfx` + `Processor` (wasmi) and compared with what the web build's `Sfx.ts` + worklet logic (V8
//! wasm) produced: the engine-call stream (name, arguments, render-frame stamp) must be identical, and the rendered
//! 32 kHz PCM is compared sample for sample (`native/target/audio_ts/<scene>.s16`, written by the generator) with RMS,
//! Pearson correlation and a spectral cosine similarity reported per scene.
use super::engine::Sm64Engine;
use super::processor::{Cmd, MasterGain, MusicEvent, Processor};
use super::sfx::{course_music, PlayerState, Sfx};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::mpsc::channel;

const RATE: f64 = 32000.0;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn pos(v: &Value) -> [f64; 3] {
    let a = v.as_array().unwrap();
    [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
}

/// Play the ops with the Rust Sfx; returns (time, cmd) in op order.
fn script(ops: &[Value]) -> Vec<(f64, Cmd)> {
    let (tx, rx) = channel();
    let mut sfx = Sfx::new(tx, MasterGain::new(0.0), false);
    let mut player = Value::Null;
    let mut out = vec![];
    for op in ops {
        let t = op["t"].as_f64().unwrap();
        let name = op["op"].as_str().unwrap();
        match name {
            "player" => player = op.clone(),
            "music" => sfx.start_music(course_music(op["course"].as_str().unwrap(), op["area"].as_i64().unwrap() as i32)),
            "location" => sfx.set_music_location(op["area"].as_i64().unwrap() as i32, op["room"].as_i64().unwrap() as i32, pos(&op["pos"]), op["stars"].as_u64().unwrap() as u32),
            "power" => sfx.set_power_music(op["seq"].as_f64().unwrap(), op["fade"].as_bool().unwrap()),
            "stop" => sfx.stop_music(),
            "event" => sfx.music_event(match op["name"].as_str().unwrap() {
                "kingIntro" => MusicEvent::KingIntro,
                "kingFight" => MusicEvent::KingFight,
                "kingDefeat" => MusicEvent::KingDefeat,
                "kingLeave" => MusicEvent::KingLeave,
                "raceStart" => MusicEvent::RaceStart,
                "raceEnd" => MusicEvent::RaceEnd,
                "clear" => MusicEvent::Clear,
                other => panic!("event {other}"),
            }),
            _ => {
                sfx.set_player(&PlayerState {
                    camera_space_pos: pos(&player["cam"]),
                    metal: player["metal"].as_bool().unwrap_or(false),
                    floor_type: player["floorType"].as_i64().map(|v| v as i32),
                    underwater_floor: player["underwater"].as_bool().unwrap_or(false),
                });
                match name {
                    "jump" => sfx.jump(),
                    "land" => sfx.land(),
                    "coin" => sfx.coin(op["underwater"].as_bool().unwrap()),
                    "doubleJump" => sfx.double_jump(),
                    "tripleJump" => sfx.triple_jump(),
                    "footstep" => sfx.footstep(op["tiptoe"].as_bool().unwrap()),
                    "star" => sfx.star(),
                    "redCoin" => sfx.red_coin(op["count"].as_u64().unwrap() as u32),
                    "secret" => sfx.secret(op["count"].as_u64().unwrap() as u32),
                    "oneUp" => sfx.one_up(),
                    "bonk" => sfx.bonk(),
                    "groundPound" => sfx.ground_pound(),
                    "explosion" => sfx.explosion(pos(&op["pos"]), op["mine"].as_bool().unwrap()),
                    "splash" => sfx.splash(),
                    "swim" => sfx.swim(op["fast"].as_bool().unwrap()),
                    "capSwitch" => sfx.cap_switch(),
                    other => panic!("op {other}"),
                }
            }
        }
        while let Ok(c) = rx.try_recv() {
            out.push((t, c));
        }
    }
    out
}

struct Rendered {
    calls: Vec<(usize, String, Vec<f64>)>,
    pcm: Vec<i16>,
}

/// Apply the commands between render blocks (the worklet applies a message before the next `music_render`).
fn render(cmds: &[(f64, Cmd)], seconds: f64) -> Rendered {
    let wasm = std::fs::read(root().join("../assets/audio/music/sm64-us.wasm")).unwrap();
    let mut p = Processor::new(Sm64Engine::new(&wasm).unwrap());
    let total = (seconds * RATE) as usize;
    let mut pcm = vec![0i16; total * 2];
    let mut calls = vec![];
    let (mut rendered, mut next) = (0usize, 0usize);
    let mut guard = 0;
    while rendered < total && guard < 100_000 {
        guard += 1;
        while next < cmds.len() && cmds[next].0 * RATE <= rendered as f64 {
            for (name, args) in Processor::calls(&cmds[next].1) {
                calls.push((rendered, name.to_string(), args));
            }
            p.apply(&cmds[next].1);
            next += 1;
        }
        if !p.playing {
            rendered += 544;
            continue;
        }
        p.refill();
        let frames = p.pcm.len() / 2;
        let n = frames.min(total - rendered);
        pcm[rendered * 2..(rendered + n) * 2].copy_from_slice(&p.pcm[..n * 2]);
        rendered += frames;
    }
    Rendered { calls, pcm }
}

fn mono(pcm: &[i16]) -> Vec<f64> {
    pcm.chunks(2).map(|c| (c[0] as f64 + c[1] as f64) / 65536.0).collect()
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len()) as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma).powi(2);
        sbb += (y - mb).powi(2);
    }
    if saa == 0.0 || sbb == 0.0 {
        return if saa == sbb { 1.0 } else { 0.0 };
    }
    sab / (saa * sbb).sqrt()
}

/// Cosine similarity of the averaged magnitude spectra (2048-point Hann windows, 64 log bands).
fn spectral_similarity(a: &[f64], b: &[f64]) -> f64 {
    const N: usize = 2048;
    let bands = |x: &[f64]| -> Vec<f64> {
        let mut acc = vec![0.0; 64];
        let mut at = 0;
        while at + N <= x.len() {
            let w: Vec<f64> = (0..N).map(|i| x[at + i] * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / N as f64).cos())).collect();
            for k in 1..N / 2 {
                let (mut re, mut im) = (0.0, 0.0);
                // Sparse DFT over a log-spaced subset of bins keeps the test fast.
                if !(k < 32 || k % 8 == 0) {
                    continue;
                }
                for (i, v) in w.iter().enumerate() {
                    let ang = 2.0 * std::f64::consts::PI * (k * i % N) as f64 / N as f64;
                    re += v * ang.cos();
                    im -= v * ang.sin();
                }
                let band = (((k as f64).ln() / ((N / 2) as f64).ln()) * 63.0) as usize;
                acc[band.min(63)] += (re * re + im * im).sqrt();
            }
            at += N * 8;
        }
        acc
    };
    let (x, y) = (bands(a), bands(b));
    let dot: f64 = x.iter().zip(&y).map(|(p, q)| p * q).sum();
    let (nx, ny) = (x.iter().map(|v| v * v).sum::<f64>().sqrt(), y.iter().map(|v| v * v).sum::<f64>().sqrt());
    if nx == 0.0 || ny == 0.0 {
        1.0
    } else {
        dot / (nx * ny)
    }
}

#[test]
fn audio_matches_the_web_build() {
    // The wasm interpreter needs a deep stack (as in `spawn_render_thread`).
    std::thread::Builder::new().stack_size(128 << 20).spawn(audio_matches_inner).unwrap().join().unwrap();
}

fn audio_matches_inner() {
    let scenes: Value = serde_json::from_slice(&std::fs::read(root().join("../tests/audioScenes.json")).unwrap()).unwrap();
    let golden: Value = serde_json::from_slice(&std::fs::read(root().join("sim/tests/fixtures/audio.json")).unwrap()).unwrap();
    let seconds = scenes["seconds"].as_f64().unwrap();
    let mut failures = vec![];
    let mut report = String::new();
    for scene in scenes["scenes"].as_array().unwrap() {
        let name = scene["name"].as_str().unwrap();
        let want = golden["scenes"].as_array().unwrap().iter().find(|s| s["name"] == name).unwrap();
        let cmds = script(scene["ops"].as_array().unwrap());
        let got = render(&cmds, seconds);
        // 1. The engine-call stream.
        let want_calls: Vec<(usize, String, Vec<f64>)> = want["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c[0].as_u64().unwrap() as usize, c[1].as_str().unwrap().to_string(), c[2].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()))
            .collect();
        if got.calls != want_calls {
            let first = got.calls.iter().zip(&want_calls).position(|(a, b)| a != b).unwrap_or(got.calls.len().min(want_calls.len()));
            failures.push(format!("{name}: engine calls differ at #{first}: rust {:?} vs web {:?} ({} vs {} calls)", got.calls.get(first), want_calls.get(first), got.calls.len(), want_calls.len()));
        }
        // 2. The PCM.
        let path = root().join(format!("target/audio_ts/{name}.s16"));
        match std::fs::read(&path) {
            Ok(bytes) => {
                let theirs: Vec<i16> = bytes.chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
                let n = theirs.len().min(got.pcm.len());
                let same = (0..n).filter(|&i| theirs[i] == got.pcm[i]).count();
                let (a, b) = (mono(&got.pcm[..n]), mono(&theirs[..n]));
                let (ra, rb) = (rms(&a), rms(&b));
                let corr = correlation(&a, &b);
                let spec = spectral_similarity(&a, &b);
                report += &format!("{name:<22} samples identical {:>6.2}%  rms rust {ra:.5} web {rb:.5}  corr {corr:.6}  spectral cos {spec:.6}\n", 100.0 * same as f64 / n as f64);
                if same != n {
                    failures.push(format!("{name}: PCM differs in {} of {n} samples (corr {corr:.5}, spectral {spec:.5})", n - same));
                }
            }
            Err(_) => report += &format!("{name}: no TS PCM at {} (run the generator); rms rust {:.5}\n", path.display(), rms(&mono(&got.pcm))),
        }
    }
    println!("{report}");
    assert!(failures.is_empty(), "\n{}\n{report}", failures.join("\n"));
}
