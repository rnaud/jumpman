//! Ocarina of Time background music: the ROM's sequences, rendered at import time by the importer's sequence player
//! (`native/byor-rs/src/oot/music.rs`) into `audio/oot_music/<seq>.wav` + `manifest.json` (loop region per track),
//! played through the voice mixer's music slot ([`super::voices::play_music`]) with a cross-fade.
//!
//! * [`scene_track`] maps a scene's `SoundSettings` sequence id to a rendered track (Hyrule Field's `field_logic`
//!   sequence plays the `field_default_*` segments one after another in random order, like the real one does);
//! * [`next_after`] says what follows a track that does not loop (the next field segment, otherwise silence);
//! * jingles ([`JINGLE_ITEM_GET`], ...) are tracks too; the caller decides what resumes after them.
use super::voices::{self, Clip, MusicCmd};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub const DIR: &str = "audio/oot_music";
pub const FIELD_LOGIC: u8 = 2;
/// `NA_BGM_FIELD_DEFAULT_1..B`.
pub const FIELD_SEGMENTS: [u8; 11] = [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
/// Scene sequence ids that mean "no background music" (the graveyard's 0x7F, an unset 0xFF, the nature ambience).
pub const SILENT: [u8; 3] = [127, 255, 1];

pub const JINGLE_ITEM_GET: u8 = 34;
pub const JINGLE_SMALL_ITEM_GET: u8 = 57;
pub const JINGLE_HEART_GET: u8 = 36;
pub const JINGLE_OPEN_CHEST: u8 = 43;
pub const JINGLE_EVENT_CLEAR: u8 = 59;
pub const JINGLE_BOSS_CLEAR: u8 = 33;
pub const JINGLE_GAME_OVER: u8 = 32;
/// Ocarina songs, by `NA_BGM_OCA_*` sequence id.
pub const OCARINA_SARIA: u8 = 68;
pub const OCARINA_EPONA: u8 = 69;
pub const OCARINA_ZELDA: u8 = 70;
pub const OCARINA_SUNS: u8 = 71;
pub const OCARINA_TIME: u8 = 72;
pub const OCARINA_STORM: u8 = 73;

#[derive(Deserialize, Clone, Debug)]
pub struct Track {
    pub name: String,
    pub file: String,
    pub rate: u32,
    pub frames: usize,
    pub duration: f64,
    #[serde(rename = "loopStart")]
    pub loop_start: Option<f64>,
    #[serde(rename = "loopEnd")]
    pub loop_end: Option<f64>,
}

impl Track {
    pub fn looping(&self) -> Option<(usize, usize)> {
        Some(((self.loop_start? * self.rate as f64) as usize, ((self.loop_end? * self.rate as f64) as usize).min(self.frames)))
    }
}

#[derive(Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub tracks: HashMap<String, Track>,
}

impl Manifest {
    pub fn load(root: &Path) -> Self {
        std::fs::read(root.join(DIR).join("manifest.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn get(&self, seq: u8) -> Option<&Track> {
        self.tracks.get(&seq.to_string())
    }
}

/// The track a scene starts with. `roll` picks among the Hyrule Field segments.
pub fn scene_track(seq: u8, roll: u32) -> Option<u8> {
    if SILENT.contains(&seq) {
        return None;
    }
    if seq == FIELD_LOGIC {
        return Some(FIELD_SEGMENTS[roll as usize % FIELD_SEGMENTS.len()]);
    }
    Some(seq)
}

/// What plays after a track that does not loop ends.
pub fn next_after(seq: u8, roll: u32) -> Option<u8> {
    if FIELD_SEGMENTS.contains(&seq) {
        // a different segment from the one that just ended
        let mut n = FIELD_SEGMENTS[roll as usize % FIELD_SEGMENTS.len()];
        if n == seq {
            n = FIELD_SEGMENTS[(roll as usize + 1) % FIELD_SEGMENTS.len()];
        }
        return Some(n);
    }
    None
}

pub fn load_clip(root: &Path, track: &Track) -> Option<Arc<Clip>> {
    let bytes = std::fs::read(root.join(DIR).join(&track.file)).ok()?;
    voices::parse_wav(&bytes).map(Arc::new)
}

/// Hand a track to the mixer; false when its file is missing (a build without the ROM data stays silent).
pub fn play(root: &Path, track: &Track, gain: f32) -> bool {
    let Some(clip) = load_clip(root, track) else { return false };
    let looping = track.looping();
    voices::play_music(&track.name, MusicCmd { clip, looping, gain });
    true
}

pub fn stop() {
    voices::stop_music();
}

/// Music started since the last call (track names, `<stop>` for a stop): the test hook behind `JUMPMAN_MUSIC_LOG`.
pub fn take_log() -> Vec<String> {
    voices::take_music_log()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn scenes_pick_their_sequence_and_field_logic_picks_a_segment() {
        assert_eq!(scene_track(60, 0), Some(60));
        assert_eq!(scene_track(127, 0), None);
        assert_eq!(scene_track(255, 3), None);
        let picks: std::collections::HashSet<_> = (0..40).filter_map(|r| scene_track(FIELD_LOGIC, r)).collect();
        assert_eq!(picks.len(), FIELD_SEGMENTS.len());
        assert!(picks.iter().all(|p| FIELD_SEGMENTS.contains(p)));
    }

    #[test]
    fn a_field_segment_is_followed_by_another_one_and_a_theme_by_silence() {
        for r in 0..30 {
            for &s in &FIELD_SEGMENTS {
                let n = next_after(s, r).unwrap();
                assert!(FIELD_SEGMENTS.contains(&n) && n != s);
            }
        }
        assert_eq!(next_after(28, 5), None);
    }

    #[test]
    fn track_loop_region_is_in_frames() {
        let t = Track { name: "x".into(), file: "x.wav".into(), rate: 16000, frames: 160000, duration: 10.0, loop_start: Some(2.0), loop_end: Some(10.0) };
        assert_eq!(t.looping(), Some((32000, 160000)));
        let once = Track { loop_start: None, loop_end: None, ..t };
        assert_eq!(once.looping(), None);
    }

    /// With the imported data: every scene's sequence has a rendered, audible, looping track.
    #[test]
    fn the_rendered_scene_music_exists_and_loops() {
        let m = Manifest::load(&assets());
        if m.tracks.is_empty() {
            return eprintln!("no OoT music imported; skipped");
        }
        for seq in [60u8, 28, 31, 47, 48, 62, 80, 85, 95, 39] {
            let t = m.get(seq).unwrap_or_else(|| panic!("track {seq}"));
            assert!(t.looping().is_some(), "{} loops", t.name);
            assert!(t.duration > 15.0, "{} lasts {}", t.name, t.duration);
            let clip = load_clip(&assets(), t).unwrap_or_else(|| panic!("{} wav", t.name));
            let peak = clip.pcm.iter().fold(0.0f32, |a, v| a.max(v.abs()));
            let rms = (clip.pcm.iter().map(|v| v * v).sum::<f32>() / clip.pcm.len() as f32).sqrt();
            assert!(peak > 0.4 && rms > 0.03, "{}: peak {peak} rms {rms}", t.name);
        }
        for f in FIELD_SEGMENTS {
            assert!(m.get(f).is_some(), "field segment {f}");
        }
        assert!(m.get(JINGLE_ITEM_GET).is_some() && m.get(JINGLE_ITEM_GET).unwrap().looping().is_none());
    }
}
