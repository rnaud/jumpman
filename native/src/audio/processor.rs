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

    pub fn apply(&mut self, cmd: &Cmd) {
        let e = &mut self.engine;
        let _ = match cmd {
            Cmd::Location(l) => e.call("music_location", l),
            Cmd::Sound(s) => e.call("sound_effect", s),
            Cmd::Birds(birds) => {
                for (i, b) in birds.iter().enumerate() {
                    let _ = e.call("music_bird", &[i as f64, b[0], b[1], b[2]]);
                }
                Ok(vec![])
            }
            Cmd::Track { sequence, preset } => {
                let r = e.call("music_start", &[*sequence as f64, *preset as f64]);
                self.frames = 0;
                self.index = 0;
                self.playing = true;
                r
            }
            Cmd::Power(seq, fade) => e.call("music_power", &[*seq, *fade]),
            Cmd::Event(ev) => match ev {
                MusicEvent::ExitKey => e.call("music_exit_reward", &[1.0]),
                MusicEvent::ExitStar => e.call("music_exit_reward", &[0.0]),
                MusicEvent::Clear => e.call("play_course_clear", &[]),
                MusicEvent::Spawn => e.call("play_power_star_jingle", &[1.0]),
                MusicEvent::Puzzle => e.call("play_puzzle_jingle", &[]),
                MusicEvent::KingIntro => e.call("music_king_dialog", &[0.0]),
                MusicEvent::KingFight => e.call("music_king_dialog", &[1.0]),
                MusicEvent::KingDefeat => e.call("music_king_dialog", &[2.0]),
                MusicEvent::KingLeave => e.call("music_king_dialog", &[3.0]),
                MusicEvent::RaceStart => e.call("music_race", &[1.0]),
                MusicEvent::RaceEnd => e.call("music_race", &[0.0]),
            },
            Cmd::Stop => e.call("seq_player_fade_out", &[0.0, 126.0]),
        };
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
    let spawned = std::thread::Builder::new().name("sm64-audio".into()).stack_size(64 << 20).spawn(move || {
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
    type DecoderItem = f32;
    type Decoder = Sm64Source;
    fn decoder(&self) -> Sm64Source {
        Sm64Source { blocks: self.blocks.clone(), gain: self.gain.clone(), block: Vec::new(), at: 0 }
    }
}

pub struct Sm64Source {
    blocks: Arc<Mutex<Blocks>>,
    gain: MasterGain,
    block: Vec<i16>,
    at: usize,
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
            if self.block.is_empty() {
                return Some(0.0);
            }
        }
        let v = self.block[self.at] as f32 / 32768.0;
        self.at += 1;
        Some(v * self.gain.get())
    }
}

impl Source for Sm64Source {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        ENGINE_RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
