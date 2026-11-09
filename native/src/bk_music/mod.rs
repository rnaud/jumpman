//! Banjo-Kazooie music: the game's own compact-MIDI sequences (`comusic_e`, asset `0x1516 + id`) played with its second sound font
//! (`soundfont2ctl/tbl`) by the software port of the libultra sequence player in [`synth`]. The data is written by the Rust
//! importer (`byor-rs/src/bk/music.rs`) to `audio/music/bk/` (`seq_<id>.bin`, `bank.ctl`, `bank.tbl`, `music.json`).
//!
//! * [`Data`] (`music.json`): per track the name and default volume (`musicTrackInfo`), per exported `bk_*` course the track
//!   its map plays (`sCore2_9B650MapMusicList`: primary, secondary track, flags);
//! * the render thread owns the [`synth::Player`]; [`cue`] sends it a [`Cue`], [`follow`] turns "walking a BK course" into
//!   `Cue::Track`, steps the SM64 engine's music aside and follows the global mute (M) like `mk64_music`.
//!
//! Not modelled: the world-position layer mix of `midichannel` (all channels play), the secondary track cross-fade (seagulls,
//! hang bridge), the reverb bus, music stingers (collect jingles, Jiggy fanfare) other than through [`cue`].
//!
//! Env: `JUMPMAN_MUSIC_LOG=1` prints every cue; `JUMPMAN_NO_AUDIO=1` disables the stream.
pub mod synth;
#[cfg(test)]
mod tests;

use crate::audio::{GameAudio, SfxCue};
use crate::game::runner::GameSim;
use crate::render::AssetRoot;
use bevy::audio::{AddAudioSource, Decodable, Source};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use synth::{Bank, Player, RATE};

/// Below `assets/`.
pub const FOLDER: &str = "audio/music/bk";
/// Same mix level as the other engines (`audio::sfx::MASTER_VOLUME`).
const MASTER_VOLUME: f32 = 0.24;
/// Frames rendered per block (one video frame).
const BLOCK_FRAMES: usize = (RATE / 60) as usize;

#[derive(Deserialize, Clone, Debug)]
pub struct TrackInfo {
    pub id: u32,
    pub file: String,
    pub name: String,
    pub volume: u32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct MapMusic {
    pub map: i64,
    pub track: i64,
    pub secondary: i64,
    pub flags: i64,
}

#[derive(Deserialize, Debug, Default)]
pub struct Data {
    pub tracks: Vec<TrackInfo>,
    pub maps: HashMap<String, MapMusic>,
}

impl Data {
    pub fn load(dir: &Path) -> Result<Data, String> {
        let text = std::fs::read_to_string(dir.join("music.json")).map_err(|e| format!("{}: {e}", dir.join("music.json").display()))?;
        serde_json::from_str(&text).map_err(|e| format!("music.json: {e}"))
    }

    pub fn track(&self, id: i64) -> Option<&TrackInfo> {
        self.tracks.iter().find(|t| t.id as i64 == id)
    }

    /// Track a course plays (the primary one of its map).
    pub fn course_track(&self, course: &str) -> Option<i64> {
        self.maps.get(course).map(|m| m.track).filter(|t| *t >= 0)
    }
}

/// What the game asks the music for.
#[derive(Clone, Debug, PartialEq)]
pub enum Cue {
    /// `comusic_playTrack`: restart the music with this track id.
    Track(u32),
    /// `set_channel_mask`: layers audible.
    Mask(u16),
    Stop,
}

static SENDER: OnceLock<Mutex<Sender<Cue>>> = OnceLock::new();
static GAIN: AtomicU32 = AtomicU32::new(0);
static DATA: OnceLock<Data> = OnceLock::new();

pub fn cue(c: Cue) {
    if let Some(tx) = SENDER.get().and_then(|m| m.lock().ok()) {
        let _ = tx.send(c);
    }
}

/// True for the `bk_*` courses whose music is this module's (their map has an entry in the exported table).
pub fn owns_course(id: &str) -> bool {
    DATA.get().is_some_and(|d| d.maps.contains_key(id))
}

pub struct BkMusicPlugin;

impl Plugin for BkMusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<BkAudio>().add_systems(Startup, setup).add_systems(Update, follow);
    }
}

fn setup(mut commands: Commands, mut assets: ResMut<Assets<BkAudio>>, root: Res<AssetRoot>) {
    if std::env::var_os("JUMPMAN_NO_AUDIO").is_some() {
        return;
    }
    let dir = root.0.join(FOLDER);
    let Ok(data) = Data::load(&dir) else { return };
    let _ = DATA.set(data);
    let (tx, rx) = channel();
    let _ = SENDER.set(Mutex::new(tx));
    let blocks = spawn_render_thread(dir, rx);
    commands.spawn(AudioPlayer(assets.add(BkAudio { blocks: Arc::new(Mutex::new(blocks)) })));
}

#[derive(Default)]
struct Owner {
    owns: bool,
    course: Option<String>,
}

fn follow(sim: Option<Res<GameSim>>, audio: Option<Res<GameAudio>>, mut owner: Local<Owner>, mut cues: MessageWriter<SfxCue>) {
    GAIN.store(if audio.is_some_and(|a| a.is_muted()) { 0.0f32 } else { MASTER_VOLUME }.to_bits(), Ordering::Relaxed);
    let Some(sim) = sim else { return };
    let id = sim.course.request.id.as_str();
    let owns = owns_course(id);
    if owns && !owner.owns {
        cues.write(SfxCue::StopMusic);
    } else if !owns && owner.owns {
        cue(Cue::Stop);
        cues.write(SfxCue::ResumeCourseMusic);
        owner.course = None;
    }
    owner.owns = owns;
    if owns && owner.course.as_deref() != Some(id) {
        owner.course = Some(id.to_string());
        match DATA.get().and_then(|d| d.course_track(id)) {
            Some(t) => {
                if std::env::var_os("JUMPMAN_MUSIC_LOG").is_some() {
                    println!("bk music: {id} -> track {t:#x} {}", DATA.get().and_then(|d| d.track(t)).map_or("", |i| i.name.as_str()));
                }
                cue(Cue::Track(t as u32));
            }
            None => cue(Cue::Stop),
        }
    }
}

/// The loaded sound font and sequence files of a data directory.
pub struct Library {
    pub dir: PathBuf,
    pub data: Data,
    pub bank: Arc<Bank>,
}

impl Library {
    pub fn load(dir: &Path) -> Result<Library, String> {
        let data = Data::load(dir)?;
        let ctl = std::fs::read(dir.join("bank.ctl")).map_err(|e| e.to_string())?;
        let tbl = std::fs::read(dir.join("bank.tbl")).map_err(|e| e.to_string())?;
        Ok(Library { dir: dir.to_path_buf(), data, bank: Arc::new(Bank::parse(ctl, tbl)?) })
    }

    /// Start `track` on `player` with its default volume.
    pub fn start(&self, player: &mut Player, track: u32) -> Result<(), String> {
        let info = self.data.track(track as i64).ok_or_else(|| format!("no BK music track {track:#x}"))?;
        let bytes = std::fs::read(self.dir.join(&info.file)).map_err(|e| e.to_string())?;
        player.play(&bytes, info.volume as f32 / 32767.0)
    }
}

pub fn spawn_render_thread(dir: PathBuf, cues: Receiver<Cue>) -> Receiver<Vec<i16>> {
    let (tx, rx) = sync_channel::<Vec<i16>>(3);
    let spawned = std::thread::Builder::new().name("bk-music".into()).stack_size(16 << 20).spawn(move || {
        let lib = match Library::load(&dir) {
            Ok(l) => l,
            Err(e) => {
                bevy::log::error!("BK music unavailable: {e}");
                return;
            }
        };
        let log = std::env::var_os("JUMPMAN_MUSIC_LOG").is_some();
        let mut player = Player::new(lib.bank.clone());
        loop {
            let mut next = if player.playing() { cues.try_recv() } else { cues.recv_timeout(Duration::from_millis(10)).map_err(|e| match e {
                std::sync::mpsc::RecvTimeoutError::Timeout => TryRecvError::Empty,
                std::sync::mpsc::RecvTimeoutError::Disconnected => TryRecvError::Disconnected,
            }) };
            loop {
                match next {
                    Ok(c) => {
                        if log {
                            println!("bk music: cue {c:?}");
                        }
                        match c {
                            Cue::Track(t) => {
                                if let Err(e) = lib.start(&mut player, t) {
                                    bevy::log::error!("BK music: {e}");
                                }
                            }
                            Cue::Mask(m) => player.set_channel_mask(m),
                            Cue::Stop => player.stop(),
                        }
                        next = cues.try_recv();
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            if !player.playing() {
                continue;
            }
            let mut block = Vec::with_capacity(BLOCK_FRAMES * 2);
            player.render(BLOCK_FRAMES, &mut block);
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
        bevy::log::error!("could not start the BK music thread: {e}");
    }
    rx
}

#[derive(Asset, TypePath)]
pub struct BkAudio {
    pub blocks: Arc<Mutex<Receiver<Vec<i16>>>>,
}

impl Decodable for BkAudio {
    type Decoder = BkSource;
    fn decoder(&self) -> BkSource {
        BkSource { blocks: self.blocks.clone(), block: Vec::new(), at: 0 }
    }
}

pub struct BkSource {
    blocks: Arc<Mutex<Receiver<Vec<i16>>>>,
    block: Vec<i16>,
    at: usize,
}

impl Iterator for BkSource {
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

impl Source for BkSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> std::num::NonZero<u16> {
        std::num::NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> std::num::NonZero<u32> {
        std::num::NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
