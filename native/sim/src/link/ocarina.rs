//! The Ocarina: the five playable buttons, the twelve songs and the note recorder.
//!
//! Sequences are `gOcarinaSongButtons` in `src/audio/game/general.c`; the pitch of each button
//! (`sButtonToPitchMap`: A = D4, C-down = F4, C-right = A4, C-left = B4, C-up = D5) is what
//! `src/audio/game/general.c`'s `sOcarinaSongNotes` plays back.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Note {
    A,
    CDown,
    CRight,
    CLeft,
    CUp,
}

impl Note {
    pub const ALL: [Note; 5] = [Note::A, Note::CDown, Note::CRight, Note::CLeft, Note::CUp];

    /// `sButtonToPitchMap`, semitones above D4 (D4 = 0, F4 = 3, A4 = 7, B4 = 9, D5 = 12).
    pub fn semitone(self) -> i32 {
        match self {
            Note::A => 0,
            Note::CDown => 3,
            Note::CRight => 7,
            Note::CLeft => 9,
            Note::CUp => 12,
        }
    }

    /// Frequency in Hz (A4 = 440 is the C-right button).
    pub fn frequency(self) -> f64 {
        440.0 * 2f64.powf((self.semitone() - 7) as f64 / 12.0)
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Note::A => "A",
            Note::CDown => "v",
            Note::CRight => ">",
            Note::CLeft => "<",
            Note::CUp => "^",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Song {
    Minuet,
    Bolero,
    Serenade,
    Requiem,
    Nocturne,
    Prelude,
    Sarias,
    Eponas,
    Lullaby,
    Suns,
    Time,
    Storms,
}

use Note::{CDown as D, CLeft as L, CRight as R, CUp as U, A};

impl Song {
    pub const ALL: [Song; 12] = [
        Song::Minuet, Song::Bolero, Song::Serenade, Song::Requiem, Song::Nocturne, Song::Prelude,
        Song::Sarias, Song::Eponas, Song::Lullaby, Song::Suns, Song::Time, Song::Storms,
    ];

    pub fn notes(self) -> &'static [Note] {
        match self {
            Song::Minuet => &[A, U, L, R, L, R],
            Song::Bolero => &[D, A, D, A, R, D, R, D],
            Song::Serenade => &[A, D, R, R, L],
            Song::Requiem => &[A, D, A, R, D, A],
            Song::Nocturne => &[L, R, R, A, L, R, D],
            Song::Prelude => &[U, R, U, R, L, U],
            Song::Sarias => &[D, R, L, D, R, L],
            Song::Eponas => &[U, L, R, U, L, R],
            Song::Lullaby => &[L, U, R, L, U, R],
            Song::Suns => &[R, D, U, R, D, U],
            Song::Time => &[R, A, D, R, A, D],
            Song::Storms => &[A, D, U, A, D, U],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Song::Minuet => "MINUET OF FOREST",
            Song::Bolero => "BOLERO OF FIRE",
            Song::Serenade => "SERENADE OF WATER",
            Song::Requiem => "REQUIEM OF SPIRIT",
            Song::Nocturne => "NOCTURNE OF SHADOW",
            Song::Prelude => "PRELUDE OF LIGHT",
            Song::Sarias => "SARIA'S SONG",
            Song::Eponas => "EPONA'S SONG",
            Song::Lullaby => "ZELDA'S LULLABY",
            Song::Suns => "SUN'S SONG",
            Song::Time => "SONG OF TIME",
            Song::Storms => "SONG OF STORMS",
        }
    }

    /// The five warp songs teleport; the rest are world effects.
    pub fn effect(self) -> SongEffect {
        match self {
            Song::Minuet => SongEffect::Warp("sacred_forest_meadow"),
            Song::Bolero => SongEffect::Warp("death_mountain_crater"),
            Song::Serenade => SongEffect::Warp("lake_hylia"),
            Song::Requiem => SongEffect::Warp("desert_colossus"),
            Song::Nocturne => SongEffect::Warp("graveyard"),
            // The Temple of Time is not an exported course; Prelude is handled by the app (nearest
            // available destination) rather than hard-coding one here.
            Song::Prelude => SongEffect::WarpTempleOfTime,
            Song::Sarias => SongEffect::SariasCall,
            Song::Eponas => SongEffect::CallEpona,
            Song::Lullaby => SongEffect::RoyalCrest,
            Song::Suns => SongEffect::SunsSong,
            Song::Time => SongEffect::TimeBlocks,
            Song::Storms => SongEffect::Storm,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SongEffect {
    Warp(&'static str),
    WarpTempleOfTime,
    /// Sun's Song: day <-> night.
    SunsSong,
    /// Song of Storms: rain and thunder (and drains/fills water where scripted).
    Storm,
    /// Song of Time: opens/closes time blocks and the Door of Time.
    TimeBlocks,
    /// Zelda's Lullaby: triggers royal-family crests / switches.
    RoyalCrest,
    /// Epona's Song: calls the horse.
    CallEpona,
    /// Saria's Song: calls Saria (a hint line) and calms enemies.
    SariasCall,
}

/// Record of the notes just played (`sOcarinaNoteBuffer`, at most 8). A song is recognised when the
/// buffer ends with its full sequence.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Staff {
    pub notes: Vec<Note>,
}

pub const STAFF_SIZE: usize = 8;

impl Staff {
    pub fn clear(&mut self) {
        self.notes.clear();
    }

    /// Add a played note; returns the song it completes, if any (the staff clears on a match).
    pub fn play(&mut self, n: Note) -> Option<Song> {
        if self.notes.len() == STAFF_SIZE {
            self.notes.remove(0);
        }
        self.notes.push(n);
        let found = Song::ALL.iter().copied().filter(|s| self.notes.ends_with(s.notes())).max_by_key(|s| s.notes().len());
        if found.is_some() {
            self.notes.clear();
        }
        found
    }
}

/// Songs the player has learned; the sandbox default knows all twelve.
#[derive(Clone, Debug, PartialEq)]
pub struct Learned(pub [bool; 12]);

impl Default for Learned {
    fn default() -> Self {
        Self([true; 12])
    }
}

impl Learned {
    pub fn knows(&self, s: Song) -> bool {
        self.0[s as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_song_is_recognised_from_its_notes() {
        for s in Song::ALL {
            let mut staff = Staff::default();
            let mut got = None;
            for n in s.notes() {
                got = staff.play(*n);
            }
            assert_eq!(got, Some(s), "{}", s.label());
            assert!(staff.notes.is_empty());
        }
    }

    #[test]
    fn songs_match_the_decomp_button_lists() {
        assert_eq!(Song::Minuet.notes().len(), 6);
        assert_eq!(Song::Bolero.notes().len(), 8);
        assert_eq!(Song::Serenade.notes().len(), 5);
        assert_eq!(Song::Nocturne.notes().len(), 7);
        assert_eq!(Song::Time.notes(), &[R, A, D, R, A, D]);
        assert_eq!(Song::Storms.notes(), &[A, D, U, A, D, U]);
        assert_eq!(Song::Suns.notes(), &[R, D, U, R, D, U]);
        assert_eq!(Song::Lullaby.notes(), &[L, U, R, L, U, R]);
    }

    #[test]
    fn no_song_is_a_suffix_of_another() {
        for a in Song::ALL {
            for b in Song::ALL {
                if a != b {
                    assert!(!b.notes().ends_with(a.notes()), "{} hides inside {}", a.label(), b.label());
                }
            }
        }
    }

    #[test]
    fn leading_junk_does_not_prevent_a_match_and_the_buffer_caps_at_eight() {
        let mut staff = Staff::default();
        for n in [Note::CUp, Note::CUp, Note::CUp] {
            assert_eq!(staff.play(n), None);
        }
        let mut got = None;
        for n in Song::Time.notes() {
            got = staff.play(*n);
        }
        assert_eq!(got, Some(Song::Time));
        let mut staff = Staff::default();
        for _ in 0..20 {
            staff.play(Note::CUp);
        }
        assert_eq!(staff.notes.len(), STAFF_SIZE);
    }

    #[test]
    fn pitches_follow_the_button_map() {
        assert!((Note::CRight.frequency() - 440.0).abs() < 1e-9);
        assert!(Note::A.frequency() < Note::CDown.frequency());
        assert!(Note::CUp.frequency() > Note::CLeft.frequency());
        assert_eq!(Note::CUp.semitone() - Note::A.semitone(), 12, "D5 is an octave over D4");
    }

    #[test]
    fn warp_songs_have_destinations() {
        assert_eq!(Song::Minuet.effect(), SongEffect::Warp("sacred_forest_meadow"));
        assert_eq!(Song::Suns.effect(), SongEffect::SunsSong);
    }
}
