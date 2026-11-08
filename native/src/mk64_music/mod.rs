//! Mario Kart 64 music: the game's own sound library (the decomp's `src/audio/*.c`, compiled to WebAssembly
//! by `scripts/render-music-mk64.py`, ROM data patched in by `scripts/export-mk64-music.py`) run in
//! `wasmi` on its own thread, next to the SM64 engine of `audio/`.
//!
//! * [`engine`]: the wasm instance and its exports; [`processor`]: [`Cue`]s -> the library's game-side calls
//!   (course theme, start fanfare, final-lap jingle and 25 % faster theme, star / lightning, finish
//!   fanfare then results theme) and one video frame of audio per render call;
//! * [`courses`]: which of the 30 sequences each of the 20 courses plays;
//! * this file: the Bevy side. [`cue`] is the module-level entry for the kart mode (like `kart_audio::play`);
//!   walking an `mk64_*` course plays its theme; the SM64 music is stopped while this one is in charge;
//!   the stream follows the global mute (M) and gain of `audio::GameAudio`.
//!
//! Env: `JUMPMAN_MUSIC_LOG=1` prints every cue, the sequence each course starts and the players' state every 2 s;
//! `JUMPMAN_NO_AUDIO=1` disables the stream. Assets: `audio/music/mk64/mk64-audio.wasm`.
pub mod courses;
pub mod engine;
pub mod processor;
#[cfg(test)]
mod tests;

pub use processor::Cue;

use crate::audio::{GameAudio, SfxCue};
use crate::game::runner::GameSim;
use crate::render::AssetRoot;
use bevy::audio::{AddAudioSource, Decodable, Source};
use bevy::prelude::*;
use engine::{Mk64Module, ENGINE_RATE};
use processor::Processor;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, TryRecvError, TrySendError};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Linear master volume of the SM64 engine (`audio::sfx::MASTER_VOLUME`); the MK64 mix is at the same level.
const MASTER_VOLUME: f32 = 0.24;
/// Where the engine lives below the assets root.
pub const WASM: &str = "audio/music/mk64/mk64-audio.wasm";

static SENDER: OnceLock<Mutex<Sender<Cue>>> = OnceLock::new();
static GAIN: AtomicU32 = AtomicU32::new(0);
/// The kart experiment is running (it sends its own cues); set by [`kart_session`].
static KART: AtomicBool = AtomicBool::new(false);
static KART_COURSE: Mutex<String> = Mutex::new(String::new());

/// Ask the music for something (no-op before the plugin is built or with `JUMPMAN_NO_AUDIO`).
pub fn cue(c: Cue) {
    if let Some(tx) = SENDER.get().and_then(|m| m.lock().ok()) {
        let _ = tx.send(c);
    }
}

/// The kart experiment starts (`Some(course dir)`) or ends (`None`): the SM64 music steps aside while it runs.
pub fn kart_session(course: Option<&str>) {
    match course {
        Some(dir) => {
            if let Ok(mut c) = KART_COURSE.lock() {
                *c = dir.to_string();
            }
            KART.store(true, Ordering::SeqCst);
        }
        None => {
            KART.store(false, Ordering::SeqCst);
            cue(Cue::Stop);
        }
    }
}

/// The course theme of the kart session's track (what free roam and the green light play).
pub fn kart_course() -> String {
    KART_COURSE.lock().map(|c| c.clone()).unwrap_or_default()
}

/// True for the courses whose music is this module's: the exported `mk64_*` courses.
pub fn owns_course(id: &str) -> bool {
    crate::game::mk64::is_mk64(id)
}

pub struct Mk64MusicPlugin;

impl Plugin for Mk64MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Mk64Audio>().add_systems(Startup, setup).add_systems(Update, follow);
    }
}

fn setup(mut commands: Commands, mut assets: ResMut<Assets<Mk64Audio>>, root: Res<AssetRoot>) {
    if std::env::var_os("JUMPMAN_NO_AUDIO").is_some() {
        return;
    }
    let (tx, rx) = channel();
    let _ = SENDER.set(Mutex::new(tx));
    let blocks = spawn_render_thread(root.0.join(WASM), rx);
    commands.spawn(AudioPlayer(assets.add(Mk64Audio { blocks: std::sync::Arc::new(Mutex::new(blocks)) })));
}

/// Who plays what: the kart mode, an `mk64_*` course, or neither (the SM64 engine's music).
#[derive(Default)]
struct Owner {
    owns: bool,
    theme: Option<String>,
}

fn follow(
    sim: Option<Res<GameSim>>,
    audio: Option<Res<GameAudio>>,
    mut owner: Local<Owner>,
    mut cues: MessageWriter<SfxCue>,
) {
    GAIN.store(if audio.is_some_and(|a| a.is_muted()) { 0.0f32 } else { MASTER_VOLUME }.to_bits(), Ordering::Relaxed);
    let Some(sim) = sim else { return };
    let kart = KART.load(Ordering::SeqCst);
    let id = sim.course.request.id.as_str();
    let walking = !kart && owns_course(id);
    let owns = kart || walking;
    if owns && !owner.owns {
        // Course music of the SM64 engine out (it may already have started for this course).
        cues.write(SfxCue::StopMusic);
    } else if !owns && owner.owns {
        cue(Cue::Stop);
        cues.write(SfxCue::ResumeCourseMusic);
        owner.theme = None;
    }
    owner.owns = owns;
    if kart {
        owner.theme = None;
    } else if walking && owner.theme.as_deref() != Some(id) {
        owner.theme = Some(id.to_string());
        cue(Cue::Theme(id.to_string()));
    }
}

/// Run the library on its own thread (the interpreter needs a deep stack and must not run inside the audio
/// callback). Cues are applied as they arrive; while music is playing one block per video frame is rendered
/// and handed over a short bounded queue, so cue latency stays around three frames. The module is compiled
/// on the first cue and the library is dropped again when it falls silent.
pub fn spawn_render_thread(wasm: std::path::PathBuf, cues: Receiver<Cue>) -> Receiver<Vec<i16>> {
    let (tx, rx) = sync_channel::<Vec<i16>>(3);
    let spawned = std::thread::Builder::new().name("mk64-audio".into()).stack_size(256 << 20).spawn(move || {
        let mut processor: Option<Processor> = None;
        let mut failed = false;
        let mut dump = std::env::var("JUMPMAN_MUSIC_DUMP").ok().and_then(|f| std::fs::File::create(f).ok());
        'main: loop {
            // Apply what is waiting; block briefly when idle.
            let first = match processor.as_ref().is_some_and(|p| p.is_active()) {
                true => cues.try_recv(),
                false => cues.recv_timeout(Duration::from_millis(5)).map_err(|e| match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => TryRecvError::Empty,
                    std::sync::mpsc::RecvTimeoutError::Disconnected => TryRecvError::Disconnected,
                }),
            };
            let mut next = Some(first);
            while let Some(r) = next.take() {
                match r {
                    Ok(c) => {
                        if processor.is_none() && !failed {
                            match std::fs::read(&wasm).map_err(|e| e.to_string()).and_then(|b| Mk64Module::load(&b)) {
                                Ok(m) => processor = Some(Processor::new(m)),
                                Err(e) => {
                                    failed = true;
                                    bevy::log::error!("MK64 music unavailable ({}): {e}", wasm.display());
                                }
                            }
                        }
                        if let Some(p) = processor.as_mut() {
                            p.apply(&c);
                        }
                        next = Some(cues.try_recv());
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break 'main,
                }
            }
            let Some(p) = processor.as_mut() else { continue };
            let Some(block) = p.render().map(|b| b.to_vec()) else { continue };
            if block.is_empty() {
                continue;
            }
            if let Some(f) = dump.as_mut() {
                use std::io::Write;
                let bytes: Vec<u8> = block.iter().flat_map(|v| v.to_le_bytes()).collect();
                let _ = f.write_all(&bytes);
            }
            let mut pending = Some(block);
            while let Some(b) = pending.take() {
                match tx.try_send(b) {
                    Ok(()) => {}
                    Err(TrySendError::Full(b)) => {
                        pending = Some(b);
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(TrySendError::Disconnected(_)) => return,
                }
            }
        }
    });
    if let Err(e) = spawned {
        bevy::log::error!("could not start the MK64 audio thread: {e}");
    }
    rx
}

/// The Bevy audio asset that streams the library.
#[derive(Asset, TypePath)]
pub struct Mk64Audio {
    pub blocks: std::sync::Arc<Mutex<Receiver<Vec<i16>>>>,
}

impl Decodable for Mk64Audio {
    type Decoder = Mk64Source;
    fn decoder(&self) -> Mk64Source {
        Mk64Source { blocks: self.blocks.clone(), block: Vec::new(), at: 0 }
    }
}

pub struct Mk64Source {
    blocks: std::sync::Arc<Mutex<Receiver<Vec<i16>>>>,
    block: Vec<i16>,
    at: usize,
}

impl Iterator for Mk64Source {
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
        if self.block.is_empty() {
            return Some(0.0);
        }
        let v = self.block[self.at] as f32 / 32768.0;
        self.at += 1;
        Some(v * f32::from_bits(GAIN.load(Ordering::Relaxed)))
    }
}

impl Source for Mk64Source {
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
