//! The kart experiment's side of the Mario Kart 64 music (`crate::mk64_music`): which cue goes with which
//! moment of a race. The sequences and their follow-ups (the tempo-up after the final-lap jingle, the
//! results theme after the finish fanfare, the theme coming back after a star) are the sound library's
//! own logic; this only says when to call it.
//!
//! * free roam (entering, after a race, back to the line): the track's theme;
//! * T (race): the starting fanfare during the 3-2-1, the theme on GO (the original starts it in `start_race`);
//! * the lap before the last: the final-lap jingle, then the theme 25 % faster;
//! * the player crossing the line: the finish fanfare for the place (win / other / lose), then the results theme;
//! * leaving the mode: everything fades out.
//!
//! Not triggered here: the star and lightning themes (`Cue::Star` / `Cue::Lightning` exist and are tested,
//! but the kart mode has no such items yet; the three it has - mushroom, banana, green shell - change no music in
//! the original either).
use super::kart_race::{Event, KartRace, LAPS, RACERS};
use crate::mk64_music::{self, courses::decomp_rank, Cue};

/// The kart mode opened on this track (decomp directory name).
pub fn session_start(dir: &str) {
    mk64_music::kart_session(Some(dir));
}

/// The kart mode closed: fade the music out and hand the SM64 course music back.
pub fn session_end() {
    mk64_music::kart_session(None);
}

/// Free roam (no race running): the course theme.
pub fn free_roam() {
    let dir = mk64_music::kart_course();
    if !dir.is_empty() {
        mk64_music::cue(Cue::Theme(dir));
    }
}

/// A race was started or restarted: the starting fanfare.
pub fn race_start() {
    super::kart_sfx::race_start(super::kart_race::CPU_COUNT);
    mk64_music::cue(Cue::Fanfare);
}

/// What the race did this tick.
pub fn on_events(events: &[Event], race: &KartRace) {
    super::kart_sfx::on_events(events);
    for e in events {
        match *e {
            Event::Go => free_roam(),
            Event::Lap(0, n) if n + 1 == LAPS => mk64_music::cue(Cue::FinalLap),
            Event::Finish(0) => mk64_music::cue(Cue::Finish(decomp_rank(race.rank(0), RACERS))),
            _ => {}
        }
    }
}
