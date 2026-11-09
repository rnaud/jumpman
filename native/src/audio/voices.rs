//! Character voice clips that do not come from the SM64 sound engine (Link from Ocarina of Time, Banjo from
//! Banjo-Kazooie). `character-sounds.json` names them `voices/<set>/<NAME>` with N variants; the files are
//! `assets/audio/voices/<set>/<NAME>_<i>.wav` (mono 16-bit, produced by `scripts/export-character-voices.py`).
//! A clip is picked at random, decoded once, and mixed into the engine's output stream by [`Mixer`]
//! (linear resampling to the engine rate). A missing file, e.g. a build without that game's ROM assets, is silent.
use super::engine::ENGINE_RATE;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub const PREFIX: &str = "voices/";
/// Clips keep the ROM's own level (like the SM64 clips); this leaves a little headroom under the master gain.
const VOICE_GAIN: f32 = 0.9;
/// Frames between looks at the queue of newly started clips (about 8 ms).
const POLL_FRAMES: u32 = 256;

pub struct Clip {
    pub pcm: Vec<f32>,
    pub rate: u32,
}

static ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);
static CACHE: Mutex<Option<HashMap<PathBuf, Option<Arc<Clip>>>>> = Mutex::new(None);
static PENDING: Mutex<Vec<(Arc<Clip>, f32)>> = Mutex::new(Vec::new());
/// Names of the clips started since the last [`take_log`] (test and `JUMPMAN_VOICE_LOG` hook).
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SEED: AtomicU32 = AtomicU32::new(0);

/// True for layer names that refer to a clip file rather than a sounds.json entry.
pub fn is_clip(name: &str) -> bool {
    name.starts_with(PREFIX)
}

/// Where the clips live (`<root>/audio/voices/...`); set once the asset root is known.
pub fn set_root(root: PathBuf) {
    *ROOT.lock().unwrap() = Some(root);
}

/// `voices/link/AUTO_JUMP` + variant 1 -> `<root>/audio/voices/link/AUTO_JUMP_1.wav`.
pub fn clip_path(root: &Path, name: &str, variant: usize) -> PathBuf {
    root.join("audio").join(format!("{name}_{variant}.wav"))
}

fn random(bound: usize) -> usize {
    let mut x = SEED.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() | 1).unwrap_or(0x9E37_79B9);
    }
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    SEED.store(x, Ordering::Relaxed);
    x as usize % bound.max(1)
}

/// Mono 16-bit PCM WAV; anything else is rejected.
pub fn parse_wav(bytes: &[u8]) -> Option<Clip> {
    if bytes.get(0..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let (mut at, mut rate, mut data) = (12, 0u32, None);
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?) as usize;
        let body = bytes.get(at + 8..(at + 8 + size).min(bytes.len()))?;
        match &bytes[at..at + 4] {
            b"fmt " => {
                let (format, channels, bits) = (u16::from_le_bytes(body.get(0..2)?.try_into().ok()?), u16::from_le_bytes(body.get(2..4)?.try_into().ok()?), u16::from_le_bytes(body.get(14..16)?.try_into().ok()?));
                if format != 1 || channels != 1 || bits != 16 {
                    return None;
                }
                rate = u32::from_le_bytes(body.get(4..8)?.try_into().ok()?);
            }
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    let pcm = data?.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    (rate > 0).then_some(Clip { pcm, rate })
}

fn load(path: &Path) -> Option<Arc<Clip>> {
    let mut cache = CACHE.lock().unwrap();
    cache.get_or_insert_with(HashMap::new).entry(path.to_path_buf()).or_insert_with(|| std::fs::read(path).ok().and_then(|b| parse_wav(&b)).map(Arc::new)).clone()
}

/// Start a random one of `variants` clips of `name`. Returns the file that was chosen (relative to the audio
/// directory) and whether it could be loaded; nothing plays when it could not.
pub fn play(name: &str, variants: f64) -> (String, bool) {
    play_gain(name, variants, 1.0)
}

/// Clips played since the previous call (newest last), kept to the last 64.
pub fn take_log() -> Vec<String> {
    std::mem::take(&mut *LOG.lock().unwrap())
}

/// [`play`] with a linear gain.
pub fn play_gain(name: &str, variants: f64, gain: f32) -> (String, bool) {
    let variant = random(variants.max(1.0) as usize);
    let file = format!("{name}_{variant}.wav");
    let root = ROOT.lock().unwrap().clone();
    {
        let mut log = LOG.lock().unwrap();
        log.push(format!("{name}_{variant}"));
        let excess = log.len().saturating_sub(64);
        log.drain(..excess);
    }
    let Some(clip) = root.and_then(|r| load(&clip_path(&r, name, variant))) else {
        return (file, false);
    };
    let mut pending = PENDING.lock().unwrap();
    pending.push((clip, gain));
    let excess = pending.len().saturating_sub(16);
    pending.drain(..excess);
    (file, true)
}

/// Queue synthesized mono PCM (the ocarina's notes) for the mixer.
pub fn play_pcm(pcm: Vec<f32>, rate: u32) {
    let mut pending = PENDING.lock().unwrap();
    pending.push((Arc::new(Clip { pcm, rate }), 1.0));
    let excess = pending.len().saturating_sub(16);
    pending.drain(..excess);
}

/// A background music clip handed to the mixer: loops over `looping` (frame range) when set, otherwise plays once.
pub struct MusicCmd {
    pub clip: Arc<Clip>,
    pub looping: Option<(usize, usize)>,
    pub gain: f32,
}

enum MusicMsg {
    Play(MusicCmd),
    Stop,
}

static MUSIC: Mutex<Vec<MusicMsg>> = Mutex::new(Vec::new());
/// Names of the music clips started (test hook, like [`take_log`]).
static MUSIC_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Seconds a music change takes to cross-fade.
const MUSIC_FADE: f32 = 1.2;

/// Start looping / one-shot background music (the previous music fades out). `name` is only logged.
pub fn play_music(name: &str, cmd: MusicCmd) {
    MUSIC_LOG.lock().unwrap().push(name.to_string());
    MUSIC.lock().unwrap().push(MusicMsg::Play(cmd));
}

/// Fade the background music out.
pub fn stop_music() {
    MUSIC_LOG.lock().unwrap().push("<stop>".to_string());
    MUSIC.lock().unwrap().push(MusicMsg::Stop);
}

pub fn take_music_log() -> Vec<String> {
    std::mem::take(&mut *MUSIC_LOG.lock().unwrap())
}

struct MusicVoice {
    clip: Arc<Clip>,
    pos: f64,
    step: f64,
    looping: Option<(usize, usize)>,
    gain: f32,
    level: f32,
    target: f32,
}

struct Active {
    clip: Arc<Clip>,
    pos: f64,
    step: f64,
    gain: f32,
}

/// Mixes the clips started by [`play`]; lives in the audio source and yields one mono sample per output frame.
#[derive(Default)]
pub struct Mixer {
    active: Vec<Active>,
    music: Vec<MusicVoice>,
    countdown: u32,
}

impl Mixer {
    fn poll(&mut self) {
        if self.countdown > 0 {
            self.countdown -= 1;
            return;
        }
        self.countdown = POLL_FRAMES;
        if let Ok(mut m) = MUSIC.try_lock() {
            for msg in m.drain(..) {
                for v in &mut self.music {
                    v.target = 0.0;
                }
                if let MusicMsg::Play(c) = msg {
                    let step = c.clip.rate as f64 / ENGINE_RATE as f64;
                    self.music.push(MusicVoice { clip: c.clip, pos: 0.0, step, looping: c.looping, gain: c.gain, level: 0.0, target: 1.0 });
                }
            }
        }
        if let Ok(mut p) = PENDING.try_lock() {
            for (clip, gain) in p.drain(..) {
                let step = clip.rate as f64 / ENGINE_RATE as f64;
                self.active.push(Active { clip, pos: 0.0, step, gain });
            }
        }
    }

    fn next_music(&mut self) -> f32 {
        if self.music.is_empty() {
            return 0.0;
        }
        let rate = 1.0 / (MUSIC_FADE * ENGINE_RATE as f32);
        let mut sum = 0.0;
        self.music.retain_mut(|v| {
            if v.level < v.target {
                v.level = (v.level + rate).min(v.target);
            } else if v.level > v.target {
                v.level = (v.level - rate).max(v.target);
            }
            if v.target == 0.0 && v.level <= 0.0 {
                return false;
            }
            if let Some((a, b)) = v.looping {
                if v.pos as usize >= b {
                    v.pos -= (b - a).max(1) as f64;
                }
            }
            let i = v.pos as usize;
            let Some(&x0) = v.clip.pcm.get(i) else {
                return false;
            };
            let x1 = v.clip.pcm.get(i + 1).copied().unwrap_or(x0);
            sum += (x0 + (x1 - x0) * (v.pos - i as f64) as f32) * v.gain * v.level;
            v.pos += v.step;
            true
        });
        sum * VOICE_GAIN
    }

    /// Next output frame (mono, before the master gain).
    pub fn next_frame(&mut self) -> f32 {
        self.poll();
        let music = self.next_music();
        if self.active.is_empty() {
            return music;
        }
        let mut sum = music / VOICE_GAIN;
        self.active.retain_mut(|a| {
            let i = a.pos as usize;
            let (Some(&x0), frac) = (a.clip.pcm.get(i), (a.pos - i as f64) as f32) else {
                return false;
            };
            let x1 = a.clip.pcm.get(i + 1).copied().unwrap_or(0.0);
            sum += (x0 + (x1 - x0) * frac) * a.gain;
            a.pos += a.step;
            true
        });
        sum * VOICE_GAIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, samples: &[i16]) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend((36 + samples.len() as u32 * 2).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend([1, 0, 1, 0]);
        v.extend(rate.to_le_bytes());
        v.extend((rate * 2).to_le_bytes());
        v.extend([2, 0, 16, 0]);
        v.extend(b"data");
        v.extend((samples.len() as u32 * 2).to_le_bytes());
        samples.iter().for_each(|s| v.extend(s.to_le_bytes()));
        v
    }

    #[test]
    fn music_loops_between_its_points_and_crossfades() {
        // a 1000-frame ramp at the engine rate looping over frames 400..800
        let pcm: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let clip = Arc::new(Clip { pcm, rate: ENGINE_RATE });
        let mut m = Mixer::default();
        m.music.push(MusicVoice { clip: clip.clone(), pos: 0.0, step: 1.0, looping: Some((400, 800)), gain: 1.0, level: 1.0, target: 1.0 });
        let out: Vec<f32> = (0..3000).map(|_| m.next_music()).collect();
        assert!(out.iter().all(|v| *v < 0.81 * VOICE_GAIN), "never passes the loop end");
        assert!(out[2999] > 0.39 * VOICE_GAIN, "keeps sounding after the clip's end");
        // a second track fades the first one out: the first voice is dropped once silent
        m.music.push(MusicVoice { clip, pos: 0.0, step: 1.0, looping: None, gain: 1.0, level: 0.0, target: 1.0 });
        m.music[0].target = 0.0;
        for _ in 0..(ENGINE_RATE as f32 * MUSIC_FADE) as usize + 10 {
            m.next_music();
        }
        assert!(m.music.len() <= 1, "{} voices", m.music.len());
    }

    #[test]
    fn wav_roundtrip_and_resampling_length() {
        let clip = parse_wav(&wav(16000, &[0, 16384, -16384, 0])).unwrap();
        assert_eq!((clip.rate, clip.pcm.len()), (16000, 4));
        assert_eq!(clip.pcm[1], 0.5);
        let mut m = Mixer::default();
        m.active.push(Active { clip: Arc::new(parse_wav(&wav(16000, &vec![1000; 160])).unwrap()), pos: 0.0, step: 0.5, gain: 1.0 });
        let frames = (0..400).filter(|_| m.next_frame() > 0.0).count();
        assert!((318..=322).contains(&frames), "{frames}");
        assert!(parse_wav(b"nonsense").is_none());
    }
}
