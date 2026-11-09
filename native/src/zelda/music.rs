//! Background music of the OoT courses: the scene's `SoundSettings` sequence (read into the placement file's
//! `environment.music`) rendered from the ROM by the importer, started when the course changes and followed by the
//! next Hyrule Field segment / silence when it does not loop. Jingles (item get, heart, chest, boss clear) interrupt the
//! scene music and hand back to it when they end.
//!
//! Env: `JUMPMAN_MUSIC_LOG=1` prints every track started; `JUMPMAN_MUSIC_SEQ=n` forces the scene music to sequence `n`.
use crate::audio::oot_music::{self as tracks, Manifest};
use crate::render::AssetRoot;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct OotMusic {
    manifest: Option<Manifest>,
    /// The course the scene music was chosen for.
    course: String,
    /// The scene's own sequence id (`SoundSettings`).
    scene_seq: u8,
    /// The sequence playing now, and when (seconds of app time) a track that does not loop ends.
    pub current: Option<u8>,
    ends_at: Option<f64>,
    /// A jingle is playing: the scene music resumes after it.
    jingle: bool,
    roll: u32,
    /// Seconds since the first frame (fixed and render systems share it).
    pub now: f64,
}

impl OotMusic {
    fn manifest(&mut self, root: &std::path::Path) -> &Manifest {
        self.manifest.get_or_insert_with(|| Manifest::load(root))
    }

    fn next_roll(&mut self) -> u32 {
        self.roll = self.roll.wrapping_mul(1664525).wrapping_add(1013904223);
        self.roll >> 8
    }

    /// Start sequence `seq` now (cross-fading from what plays). Returns false when it has no rendered track.
    pub fn start(&mut self, root: &std::path::Path, now: f64, seq: u8) -> bool {
        let Some(track) = self.manifest(root).get(seq).cloned() else {
            self.current = None;
            self.ends_at = None;
            return false;
        };
        let ok = tracks::play(root, &track, 1.0);
        self.current = Some(seq);
        self.ends_at = track.looping().is_none().then_some(now + track.duration);
        ok
    }

    fn start_scene(&mut self, root: &std::path::Path, now: f64) {
        self.jingle = false;
        let roll = self.next_roll();
        match tracks::scene_track(self.scene_seq, roll) {
            Some(seq) => {
                self.start(root, now, seq);
            }
            None => {
                tracks::stop();
                self.current = None;
                self.ends_at = None;
            }
        }
    }

    /// Play a jingle (`JINGLE_*`) over the scene music.
    pub fn jingle(&mut self, root: &std::path::Path, now: f64, seq: u8) {
        if self.start(root, now, seq) {
            self.jingle = true;
        }
    }

    /// One tick of the clock: what follows a track that has ended.
    pub fn tick(&mut self, root: &std::path::Path, now: f64) {
        let Some(end) = self.ends_at else { return };
        if now < end {
            return;
        }
        self.ends_at = None;
        if self.jingle {
            self.start_scene(root, now);
        } else if let Some(cur) = self.current {
            let roll = self.next_roll();
            match tracks::next_after(cur, roll) {
                Some(seq) => {
                    self.start(root, now, seq);
                }
                None => self.current = None,
            }
        }
    }
}

pub fn drive(time: Res<super::kankyo::OotTime>, real: Res<Time>, root: Res<AssetRoot>, mut music: ResMut<OotMusic>) {
    music.now += real.delta_secs_f64();
    let now = music.now;
    if !time.course.is_empty() && music.course != time.course {
        music.course = time.course.clone();
        music.scene_seq = std::env::var("JUMPMAN_MUSIC_SEQ").ok().and_then(|v| v.parse().ok()).unwrap_or(time.env.music.seq);
        if music.roll == 0 {
            music.roll = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1;
        }
        music.start_scene(&root.0, now);
    }
    music.tick(&root.0, now);
    if std::env::var_os("JUMPMAN_MUSIC_LOG").is_some() {
        for line in tracks::take_log() {
            eprintln!("oot music: {line} (course {})", music.course);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn with_data() -> Option<OotMusic> {
        let mut m = OotMusic::default();
        m.manifest(&root());
        (!m.manifest.as_ref().unwrap().tracks.is_empty()).then_some(m)
    }

    #[test]
    fn a_scene_starts_its_theme_and_a_jingle_hands_back() {
        let Some(mut m) = with_data() else { return eprintln!("no OoT music imported; skipped") };
        let root = root();
        tracks::take_log();
        m.scene_seq = 60;
        m.start_scene(&root, 0.0);
        assert_eq!(m.current, Some(60));
        assert!(m.ends_at.is_none(), "the Kokiri theme loops");
        m.jingle(&root, 10.0, tracks::JINGLE_ITEM_GET);
        assert_eq!(m.current, Some(tracks::JINGLE_ITEM_GET));
        let end = m.ends_at.expect("a jingle ends");
        m.tick(&root, end - 0.5);
        assert_eq!(m.current, Some(tracks::JINGLE_ITEM_GET));
        m.tick(&root, end + 0.1);
        assert_eq!(m.current, Some(60), "the scene theme resumes");
        // the log is shared with the other tests: look for the three starts in order
        let log = tracks::take_log();
        let mut want = ["NA_BGM_KOKIRI", "NA_BGM_ITEM_GET", "NA_BGM_KOKIRI"].into_iter();
        let mut next = want.next();
        for l in &log {
            if Some(l.as_str()) == next {
                next = want.next();
            }
        }
        assert!(next.is_none(), "{log:?}");
    }

    #[test]
    fn hyrule_field_chains_its_segments_and_the_graveyard_is_silent() {
        let Some(mut m) = with_data() else { return eprintln!("no OoT music imported; skipped") };
        let root = root();
        m.scene_seq = tracks::FIELD_LOGIC;
        m.roll = 7;
        m.start_scene(&root, 0.0);
        let first = m.current.unwrap();
        assert!(tracks::FIELD_SEGMENTS.contains(&first));
        let end = m.ends_at.unwrap();
        m.tick(&root, end + 0.1);
        let second = m.current.unwrap();
        assert!(tracks::FIELD_SEGMENTS.contains(&second) && second != first);
        m.scene_seq = 127;
        m.start_scene(&root, 100.0);
        assert_eq!(m.current, None);
    }

    #[test]
    fn a_missing_track_is_silent_not_a_panic() {
        let mut m = OotMusic::default();
        assert!(!m.start(std::path::Path::new("/nonexistent"), 0.0, 60));
        assert_eq!(m.current, None);
    }
}
