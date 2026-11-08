//! Audio-thread side: port of `music-worklet.js`. [`Cmd`]s posted by [`super::sfx::Sfx`] are applied
//! to the wasm engine, whose 32 kHz stereo output is streamed as a rodio source (rodio resamples
//! linearly to the device rate, like the worklet's phase accumulator).
use super::engine::{Sm64Engine, ENGINE_RATE};
use bevy::audio::{Decodable, Source};
use bevy::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One-shot musical cues (`Sfx.musicEvent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicEvent {
    Clear,
    ExitKey,
    ExitStar,
    Spawn,
    Puzzle,
    KingIntro,
    KingFight,
    KingDefeat,
    KingLeave,
    RaceStart,
    RaceEnd,
}

/// Messages of the worklet port.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// `music_location(level, area, room, x, y, z, stars)`
    Location([f64; 7]),
    /// `sound_effect(sound, variants, source, x, y, z)`
    Sound([f64; 6]),
    /// `music_bird(i, x, y, z)` for each bird.
    Birds(Vec<[f64; 3]>),
    /// `music_start(sequence, preset)`
    Track { sequence: u32, preset: u32 },
    /// `music_power(sequence, fade)`
    Power(f64, f64),
    Event(MusicEvent),
    /// `seq_player_fade_out(0, 126)`
    Stop,
}

/// Linear master volume (`0.24` unmuted in Sfx.ts), stored as f32 bits.
#[derive(Clone)]
pub struct MasterGain(Arc<AtomicU32>);

impl MasterGain {
    pub fn new(v: f32) -> Self {
        Self(Arc::new(AtomicU32::new(v.to_bits())))
    }
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// Engine plus command stream; owned by whichever thread renders (the audio thread, or a test).
pub struct Processor {
    pub engine: Sm64Engine,
    frames: usize,
    index: usize,
    pub pcm: Vec<i16>,
    pub playing: bool,
}

impl Processor {
    pub fn new(engine: Sm64Engine) -> Self {
        Self { engine, frames: 0, index: 0, pcm: Vec::new(), playing: false }
    }

    /// The engine calls a worklet message turns into, in the worklet's order.
    pub fn calls(cmd: &Cmd) -> Vec<(&'static str, Vec<f64>)> {
        match cmd {
            Cmd::Location(l) => vec![("music_location", l.to_vec())],
            Cmd::Sound(s) => vec![("sound_effect", s.to_vec())],
            Cmd::Birds(birds) => birds.iter().enumerate().map(|(i, b)| ("music_bird", vec![i as f64, b[0], b[1], b[2]])).collect(),
            Cmd::Track { sequence, preset } => vec![("music_start", vec![*sequence as f64, *preset as f64])],
            Cmd::Power(seq, fade) => vec![("music_power", vec![*seq, *fade])],
            Cmd::Event(ev) => vec![match ev {
                MusicEvent::ExitKey => ("music_exit_reward", vec![1.0]),
                MusicEvent::ExitStar => ("music_exit_reward", vec![0.0]),
                MusicEvent::Clear => ("play_course_clear", vec![]),
                MusicEvent::Spawn => ("play_power_star_jingle", vec![1.0]),
                MusicEvent::Puzzle => ("play_puzzle_jingle", vec![]),
                MusicEvent::KingIntro => ("music_king_dialog", vec![0.0]),
                MusicEvent::KingFight => ("music_king_dialog", vec![1.0]),
                MusicEvent::KingDefeat => ("music_king_dialog", vec![2.0]),
                MusicEvent::KingLeave => ("music_king_dialog", vec![3.0]),
                MusicEvent::RaceStart => ("music_race", vec![1.0]),
                MusicEvent::RaceEnd => ("music_race", vec![0.0]),
            }],
            Cmd::Stop => vec![("seq_player_fade_out", vec![0.0, 126.0])],
        }
    }

    pub fn apply(&mut self, cmd: &Cmd) {
        for (name, args) in Self::calls(cmd) {
            let _ = self.engine.call(name, &args);
        }
        if matches!(cmd, Cmd::Track { .. }) {
            self.frames = 0;
            self.index = 0;
            self.playing = true;
        }
    }

    /// Next stereo frame at 32 kHz (silence until a track has started), `None` when a new block is
    /// needed first (callers apply pending commands, then call again).
    pub fn next_frame(&mut self) -> Option<(f32, f32)> {
        if !self.playing {
            return Some((0.0, 0.0));
        }
        if self.index >= self.frames {
            return None;
        }
        let l = self.pcm[self.index * 2] as f32 / 32768.0;
        let r = self.pcm[self.index * 2 + 1] as f32 / 32768.0;
        self.index += 1;
        Some((l, r))
    }

    pub fn refill(&mut self) {
        self.frames = self.engine.render();
        self.engine.pcm(self.frames, &mut self.pcm);
        self.index = 0;
    }

    /// Render `frames` stereo frames, applying `cmds` first (offline, for tests and tooling).
    pub fn render_offline(&mut self, cmds: &[Cmd], frames: usize) -> Vec<(f32, f32)> {
        for c in cmds {
            self.apply(c);
        }
        let mut out = Vec::with_capacity(frames);
        while out.len() < frames {
            match self.next_frame() {
                Some(f) => out.push(f),
                None => self.refill(),
            }
        }
        out
    }
}

/// Interleaved stereo blocks (at most [`super::engine::PCM_FRAMES`] frames) from the render thread.
pub type Blocks = Receiver<Vec<i16>>;

/// Run the engine on its own thread (the interpreter needs a deep stack and must not run inside the
/// audio callback): it applies [`Cmd`]s, renders blocks while a track plays and hands them over a
/// short bounded queue, so command latency stays around three blocks.
pub fn spawn_render_thread(wasm: std::path::PathBuf, commands: Receiver<Cmd>) -> Blocks {
    let (tx, rx) = sync_channel::<Vec<i16>>(3);
    let spawned = std::thread::Builder::new().name("sm64-audio".into()).stack_size(256 << 20).spawn(move || {
        let engine = match std::fs::read(&wasm).map_err(|e| e.to_string()).and_then(|b| Sm64Engine::new(&b)) {
            Ok(e) => e,
            Err(e) => {
                bevy::log::error!("audio engine unavailable ({}): {e}", wasm.display());
                return;
            }
        };
        let mut p = Processor::new(engine);
        // JUMPMAN_AUDIO_DUMP=<file.raw> records the engine output (s16le stereo, 32 kHz) for checks.
        let mut dump = std::env::var("JUMPMAN_AUDIO_DUMP").ok().and_then(|f| std::fs::File::create(f).ok());
        loop {
            loop {
                match commands.try_recv() {
                    Ok(c) => p.apply(&c),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            if !p.playing {
                std::thread::sleep(Duration::from_millis(4));
                continue;
            }
            p.refill();
            let block = std::mem::take(&mut p.pcm);
            if let Some(f) = dump.as_mut() {
                use std::io::Write;
                let bytes: Vec<u8> = block.iter().flat_map(|v| v.to_le_bytes()).collect();
                let _ = f.write_all(&bytes);
            }
            if tx.send(block).is_err() {
                return;
            }
        }
    });
    if let Err(e) = spawned {
        bevy::log::error!("could not start the audio thread: {e}");
    }
    rx
}

/// The Bevy audio asset that streams the engine.
#[derive(Asset, TypePath)]
pub struct Sm64Audio {
    pub blocks: Arc<Mutex<Blocks>>,
    pub gain: MasterGain,
}

impl Decodable for Sm64Audio {
    type Decoder = Sm64Source;
    fn decoder(&self) -> Sm64Source {
        Sm64Source { blocks: self.blocks.clone(), gain: self.gain.clone(), block: Vec::new(), at: 0, voices: super::voices::Mixer::default(), voice: 0.0, right: true }
    }
}

pub struct Sm64Source {
    blocks: Arc<Mutex<Blocks>>,
    gain: MasterGain,
    block: Vec<i16>,
    at: usize,
    /// Character voice clips (Link, Banjo) mixed on top of the engine; mono, so both channels share `voice`.
    voices: super::voices::Mixer,
    voice: f32,
    right: bool,
}

impl Iterator for Sm64Source {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.at >= self.block.len() {
            self.at = 0;
            self.block.clear();
            if let Ok(rx) = self.blocks.try_lock() {
                if let Ok(b) = rx.try_recv() {
                    self.block = b;
                }
            }
        }
        let engine = if self.block.is_empty() { 0.0 } else { self.block[self.at] as f32 / 32768.0 };
        if !self.block.is_empty() {
            self.at += 1;
        }
        // Interleaved stereo: the voice mixer advances one frame per left sample.
        self.right = !self.right;
        if !self.right {
            self.voice = self.voices.next_frame();
        }
        Some((engine + self.voice) * self.gain.get())
    }
}

impl Source for Sm64Source {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> std::num::NonZero<u16> {
        std::num::NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> std::num::NonZero<u32> {
        std::num::NonZero::new(ENGINE_RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
