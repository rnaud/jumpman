//! Super Smash Bros. music: the game's own compact-MIDI BGM sequences (`S1_music.sbk`) played with its music bank
//! (`B1_sounds1.ctl/tbl`) by Banjo-Kazooie's software port of the libultra sequence player (`bk_music::synth`, the same
//! `n_alCSPlay` format), at SSB64's 32 kHz output rate. Data: `assets/audio/music/ssb64/` (`seq_NN.bin`, `bank.ctl`,
//! `bank.tbl`, `music.json`), written by `scripts/export-ssb64-music.py` / `byor-rs/src/ssb64_music.rs`.
//!
//! * Walking a Smash stage (`game::ssb64`) plays the stage's BGM (`ssb64.bgm` of its course json = sequence index,
//!   `MPGroundData.bgm_id`) and steps the SM64 engine's music aside, like `bk_music` / `mk64_music`.
//! * [`cue`] / [`Cue::Track`] play any BGM by id; `music.json` names the menu, character select, results and victory
//!   themes (`menu`, `victory`) for a menu layer that wants them ([`Data::menu`], [`Data::victory`]).
//!
//! Not modelled: the reverb bus, the Mushroom Kingdom hurry theme, the stage-specific BGM switches (Saffron's Pokémon,
//! the hammer / star themes).
//!
//! Env: `JUMPMAN_MUSIC_LOG=1` prints every cue; `JUMPMAN_NO_AUDIO=1` disables the stream.
use crate::audio::{GameAudio, SfxCue};
use crate::bk_music::synth::{Bank, Player, RATE};
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

/// Below `assets/`.
pub const FOLDER: &str = "audio/music/ssb64";
/// Same mix level as the other engines (`audio::sfx::MASTER_VOLUME`).
const MASTER_VOLUME: f32 = 0.24;
/// SSB64's output rate (`dSYAudioPublicSettings`): a sample at pitch 1.0 plays at this many Hz.
pub const OUTPUT_RATE: f64 = 32000.0;
const BLOCK_FRAMES: usize = (RATE / 60) as usize;

#[derive(Deserialize, Clone, Debug)]
pub struct TrackInfo {
    pub id: u32,
    pub file: String,
    /// `gmMusicID` name without `nSYAudioBGM` (`Pupupu`, `WinMario`, `ModeSelect`).
    pub name: String,
    pub volume: u32,
}

#[derive(Deserialize, Debug, Default)]
#[allow(dead_code)] // `menu` / `victory`: the themes a menu music layer can pick (none plays per-game menu music yet)
pub struct Data {
    pub tracks: Vec<TrackInfo>,
    /// `menu`, `select`, `results`, `training`, `data`, `opening`, `credits`, `stage_clear`, `game_over` -> BGM id.
    #[serde(default)]
    pub menu: HashMap<String, u32>,
    /// Fighter stem (`mario`, `purin`) -> victory fanfare BGM id.
    #[serde(default)]
    pub victory: HashMap<String, u32>,
}

#[allow(dead_code)] // `by_name` / `menu` / `victory`: lookups for a menu music layer and the tests
impl Data {
    pub fn load(dir: &Path) -> Result<Data, String> {
        let text = std::fs::read_to_string(dir.join("music.json")).map_err(|e| format!("{}: {e}", dir.join("music.json").display()))?;
        serde_json::from_str(&text).map_err(|e| format!("music.json: {e}"))
    }

    pub fn track(&self, id: u32) -> Option<&TrackInfo> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn by_name(&self, name: &str) -> Option<u32> {
        self.tracks.iter().find(|t| t.name == name).map(|t| t.id)
    }

    /// A menu theme (`menu`, `select`, `results`, ...).
    pub fn menu(&self, which: &str) -> Option<u32> {
        self.menu.get(which).copied()
    }

    /// The victory fanfare of a fighter (`fighters.json` stem).
    pub fn victory(&self, fighter: &str) -> Option<u32> {
        self.victory.get(fighter).copied()
    }
}

/// What the game asks the music for.
#[derive(Clone, Debug, PartialEq)]
pub enum Cue {
    /// `syAudioPlayBGM`: restart the music with this BGM id.
    Track(u32),
    Stop,
}

static SENDER: OnceLock<Mutex<Sender<Cue>>> = OnceLock::new();
static GAIN: AtomicU32 = AtomicU32::new(0);
static DATA: OnceLock<Data> = OnceLock::new();
static STAGE_BGM: Mutex<Option<(String, Option<u32>)>> = Mutex::new(None);

pub fn cue(c: Cue) {
    if let Some(tx) = SENDER.get().and_then(|m| m.lock().ok()) {
        let _ = tx.send(c);
    }
}

/// The exported music table, once the engine started.
#[allow(dead_code)] // API for a menu music layer
pub fn data() -> Option<&'static Data> {
    DATA.get()
}

/// True for the Smash stages while the music data is there (this module plays their BGM).
pub fn owns_course(id: &str) -> bool {
    DATA.get().is_some() && crate::game::ssb64::is_stage(id)
}

pub struct Ssb64MusicPlugin;

impl Plugin for Ssb64MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Ssb64Audio>().add_systems(Startup, setup).add_systems(Update, follow);
    }
}

/// Super Smash Bros. was imported while the game runs: start its music engine now.
pub fn reload(world: &mut World) {
    let _ = world.run_system_cached(setup);
}

fn setup(mut commands: Commands, mut assets: ResMut<Assets<Ssb64Audio>>, root: Res<AssetRoot>) {
    if std::env::var_os("JUMPMAN_NO_AUDIO").is_some() || DATA.get().is_some() {
        return;
    }
    let dir = root.0.join(FOLDER);
    let Ok(data) = Data::load(&dir) else { return };
    let _ = DATA.set(data);
    let (tx, rx) = channel();
    let _ = SENDER.set(Mutex::new(tx));
    let blocks = spawn_render_thread(dir, rx);
    commands.spawn(AudioPlayer(assets.add(Ssb64Audio { blocks: Arc::new(Mutex::new(blocks)) })));
}

/// The BGM of a stage course (read once per course from its json).
fn stage_bgm(root: &Path, id: &str) -> Option<u32> {
    let mut cache = STAGE_BGM.lock().ok()?;
    if cache.as_ref().is_none_or(|(c, _)| c != id) {
        *cache = Some((id.to_string(), crate::game::ssb64::stage(root, id).map(|s| s.bgm)));
    }
    cache.as_ref().and_then(|(_, b)| *b)
}

#[derive(Default)]
struct Owner {
    owns: bool,
    course: Option<String>,
}

fn follow(sim: Option<Res<GameSim>>, audio: Option<Res<GameAudio>>, root: Option<Res<AssetRoot>>, mut owner: Local<Owner>, mut cues: MessageWriter<SfxCue>) {
    GAIN.store(if audio.is_some_and(|a| a.is_muted()) { 0.0f32 } else { MASTER_VOLUME }.to_bits(), Ordering::Relaxed);
    let (Some(sim), Some(root)) = (sim, root) else { return };
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
        match stage_bgm(&root.0, id) {
            Some(t) => {
                if std::env::var_os("JUMPMAN_MUSIC_LOG").is_some() {
                    println!("ssb64 music: {id} -> bgm {t} {}", DATA.get().and_then(|d| d.track(t)).map_or("", |i| i.name.as_str()));
                }
                cue(Cue::Track(t));
            }
            None => cue(Cue::Stop),
        }
    }
}

/// The loaded bank and sequence files of a data directory.
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

    /// A player for this bank at SSB64's output rate.
    pub fn player(&self) -> Player {
        let mut p = Player::new(self.bank.clone());
        p.output_rate = OUTPUT_RATE;
        p
    }

    /// Start BGM `id` on `player` at the game's BGM volume.
    pub fn start(&self, player: &mut Player, id: u32) -> Result<(), String> {
        let info = self.data.track(id).ok_or_else(|| format!("no SSB64 BGM {id}"))?;
        let bytes = std::fs::read(self.dir.join(&info.file)).map_err(|e| e.to_string())?;
        // half the BGM level: SSB64 mixes it under the effects, and the synth soft-clips near full scale
        player.play(&bytes, info.volume as f32 / 32767.0 * 0.5)
    }
}

pub fn spawn_render_thread(dir: PathBuf, cues: Receiver<Cue>) -> Receiver<Vec<i16>> {
    let (tx, rx) = sync_channel::<Vec<i16>>(3);
    let spawned = std::thread::Builder::new().name("ssb64-music".into()).stack_size(16 << 20).spawn(move || {
        let lib = match Library::load(&dir) {
            Ok(l) => l,
            Err(e) => {
                bevy::log::error!("SSB64 music unavailable: {e}");
                return;
            }
        };
        let log = std::env::var_os("JUMPMAN_MUSIC_LOG").is_some();
        let mut player = lib.player();
        loop {
            let mut next = if player.playing() {
                cues.try_recv()
            } else {
                cues.recv_timeout(Duration::from_millis(10)).map_err(|e| match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => TryRecvError::Empty,
                    std::sync::mpsc::RecvTimeoutError::Disconnected => TryRecvError::Disconnected,
                })
            };
            loop {
                match next {
                    Ok(c) => {
                        if log {
                            println!("ssb64 music: cue {c:?}");
                        }
                        match c {
                            Cue::Track(t) => {
                                if let Err(e) = lib.start(&mut player, t) {
                                    bevy::log::error!("SSB64 music: {e}");
                                }
                            }
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
        bevy::log::error!("could not start the SSB64 music thread: {e}");
    }
    rx
}

#[derive(Asset, TypePath)]
pub struct Ssb64Audio {
    pub blocks: Arc<Mutex<Receiver<Vec<i16>>>>,
}

impl Decodable for Ssb64Audio {
    type Decoder = Ssb64Source;
    fn decoder(&self) -> Ssb64Source {
        Ssb64Source { blocks: self.blocks.clone(), block: Vec::new(), at: 0 }
    }
}

pub struct Ssb64Source {
    blocks: Arc<Mutex<Receiver<Vec<i16>>>>,
    block: Vec<i16>,
    at: usize,
}

impl Iterator for Ssb64Source {
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

impl Source for Ssb64Source {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The import's music folder: `JUMPMAN_SMASH_TEST_ASSETS/audio/music/ssb64` or the repo's (git-ignored) assets.
    fn dir() -> PathBuf {
        std::env::var_os("JUMPMAN_SMASH_TEST_ASSETS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")).join(FOLDER)
    }

    /// Every stage BGM and the menu themes render audibly with the SSB64 bank (skipped without the import).
    /// `JUMPMAN_SSB64_MUSIC_WAV=<dir>` also writes 20 s of each stage theme as a WAV there.
    #[test]
    fn stage_themes_render() {
        let Ok(lib) = Library::load(&dir()) else {
            eprintln!("no SSB64 music import: skipped");
            return;
        };
        assert_eq!(lib.data.tracks.len(), 47);
        assert!(lib.bank.instruments.iter().flatten().count() >= 40, "music bank instruments");
        let wav_dir = std::env::var_os("JUMPMAN_SSB64_MUSIC_WAV").map(PathBuf::from);
        let names = ["Pupupu", "Zebes", "Inishie", "Sector", "Jungle", "Castle", "Yamabuki", "Yoster", "Hyrule", "Last", "Zako", "Metal", "ModeSelect", "BattleSelect", "WinMario"];
        let mut report = String::new();
        for name in names {
            let id = lib.data.by_name(name).unwrap_or_else(|| panic!("no BGM {name}"));
            let mut p = lib.player();
            lib.start(&mut p, id).unwrap();
            let seconds = if wav_dir.is_some() { 20 } else { 6 };
            let mut out = Vec::new();
            p.render(RATE as usize * seconds, &mut out);
            let peak = out.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
            let rms = (out.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / out.len().max(1) as f64).sqrt();
            report.push_str(&format!("{name:<14} id {id:>2} notes {:>5} missing {:>3} peak {peak:>5} rms {rms:>6.0}\n", p.notes_started(), p.missing_sounds));
            assert!(p.notes_started() > 10, "{name}: only {} notes", p.notes_started());
            assert!(peak > 1500 && rms > 150.0, "{name}: silent (peak {peak}, rms {rms})");
            assert!(p.missing_sounds * 10 <= p.notes_started() as u32, "{name}: {} notes without a sound", p.missing_sounds);
            if let Some(d) = &wav_dir {
                let _ = std::fs::create_dir_all(d);
                write_wav(&d.join(format!("ssb64_{id:02}_{name}.wav")), &out);
            }
        }
        eprintln!("{report}");
    }

    fn write_wav(path: &Path, pcm: &[i16]) {
        let mut b = Vec::with_capacity(44 + pcm.len() * 2);
        let data_len = (pcm.len() * 2) as u32;
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&RATE.to_le_bytes());
        b.extend_from_slice(&(RATE * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for v in pcm {
            b.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }
}
