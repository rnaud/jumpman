//! Audio-thread side: turns [`Cue`]s into the game-side calls of the MK64 sound library and renders its
//! output one video frame (1/60 s) at a time.
use super::courses::*;
use super::engine::{Mk64Engine, Mk64Module, PlayerState};

/// What the games asks the music to do. Names follow the decomp's own events.
#[derive(Clone, Debug, PartialEq)]
pub enum Cue {
    /// A course's theme (`play_music_for_current_track`): the decomp's course directory name
    /// (`mario_raceway`, with or without the `mk64_` prefix). Free roam, walking and the green light.
    Theme(String),
    /// The pre-race fanfare (`SEQ_EVENT_RACE_STARTING`); the course theme is faded out under it.
    Fanfare,
    /// Start of the last lap (`func_800CA49C`): the jingle, then the theme again 25 % faster.
    FinalLap,
    /// Super-star music over the theme (`func_800CA59C` / `func_800CA730`).
    Star(bool),
    /// Lightning: the theme drops in volume and tempo (`func_800CAB4C` / `func_800CAC08`).
    Lightning(bool),
    /// The player crossed the finish line in this place (0 = first, decomp numbering, see
    /// [`decomp_rank`]): finish fanfare, then the results theme.
    Finish(u8),
    /// Fade everything out and reset the library.
    Stop,
}

/// Frames of audio a fade-out takes before the library is reset.
const STOP_FRAMES: u32 = 40;
/// Silent frames (no music player running) after which the idle engine is dropped.
const IDLE_FRAMES: u32 = 150;

pub struct Processor {
    module: Mk64Module,
    engine: Option<Mk64Engine>,
    reset_in: Option<u32>,
    silent: u32,
    frame: u64,
    log: bool,
    /// The last course theme requested (for the log and for the finish logic).
    pub course: Option<String>,
    pcm: Vec<i16>,
}

impl Processor {
    pub fn new(module: Mk64Module) -> Self {
        Self { module, engine: None, reset_in: None, silent: 0, frame: 0, log: std::env::var_os("JUMPMAN_MUSIC_LOG").is_some(), course: None, pcm: vec![] }
    }

    fn engine(&mut self) -> Option<&mut Mk64Engine> {
        if self.engine.is_none() {
            match self.module.instantiate() {
                Ok(e) => self.engine = Some(e),
                Err(e) => {
                    bevy::log::error!("MK64 music engine: {e}");
                    return None;
                }
            }
        }
        self.engine.as_mut()
    }

    #[cfg(test)]
    pub fn engine_for_test(&mut self) -> Option<&mut Mk64Engine> {
        self.engine()
    }

    pub fn is_active(&self) -> bool {
        self.engine.is_some()
    }

    /// Players 0 and 1 of the live engine.
    pub fn status(&mut self) -> Option<[PlayerState; 2]> {
        let e = self.engine.as_mut()?;
        Some([e.player(0), e.player(1)])
    }

    pub fn unknown_ops(&mut self) -> u32 {
        self.engine.as_mut().map_or(0, |e| e.unknown_ops())
    }

    pub fn apply(&mut self, cue: &Cue) {
        if self.log {
            println!("mk64 music: cue {cue:?}");
        }
        // A new request while a Stop is still fading: finish the reset first, so nothing of the old music survives.
        if self.reset_in.take().is_some() {
            self.engine = None;
        }
        match cue {
            Cue::Theme(course) => {
                let Some(seq) = theme_for(course) else {
                    bevy::log::warn!("no MK64 theme for course {course}");
                    return;
                };
                self.course = Some(course.clone());
                if self.log {
                    println!("mk64 music: course {course} -> sequence {seq}");
                }
                if let Some(e) = self.engine() {
                    // The same theme still running (a neighbouring course with the same sequence) carries on.
                    let p = e.player(0);
                    if !(p.enabled && p.sequence == seq && p.volume > 0.3) {
                        e.mode(0, 0, 0);
                        e.track(seq);
                    }
                }
            }
            Cue::Fanfare => {
                if let Some(e) = self.engine() {
                    e.mode(0, 0, 0);
                    // Fade the theme (player 0) and anything on player 1 out, then the start jingle.
                    e.command(0x1001_00FF);
                    e.command(0x1101_00FF);
                    e.event(SEQ_RACE_STARTING);
                }
            }
            Cue::FinalLap => {
                if let Some(e) = self.engine() {
                    e.final_lap();
                }
            }
            Cue::Star(on) => {
                if let Some(e) = self.engine() {
                    e.star(*on);
                }
            }
            Cue::Lightning(on) => {
                if let Some(e) = self.engine() {
                    e.lightning(*on);
                }
            }
            Cue::Finish(rank) => {
                if let Some(e) = self.engine() {
                    e.mode(0, *rank as u32, 0);
                    e.finish();
                }
            }
            Cue::Stop => {
                if let Some(e) = self.engine.as_mut() {
                    // 0x1P(t)...: op 1 = fade the player out over t * 8 frames.
                    e.command(0x1005_00FF);
                    e.command(0x1105_00FF);
                    self.reset_in = Some(STOP_FRAMES);
                }
                self.course = None;
            }
        }
    }

    /// Render the next video frame; `None` while idle (no engine).
    pub fn render(&mut self) -> Option<&[i16]> {
        let engine = self.engine.as_mut()?;
        let frames = engine.render();
        engine.pcm(frames, &mut self.pcm);
        self.frame += 1;
        if let Some(left) = self.reset_in.as_mut() {
            *left = left.saturating_sub(1);
            if *left == 0 {
                self.engine = None;
                self.reset_in = None;
                self.silent = 0;
                return None;
            }
        }
        let engine = self.engine.as_mut()?;
        let running = engine.player(0).enabled || engine.player(1).enabled;
        self.silent = if running { 0 } else { self.silent + 1 };
        if self.log && self.frame % 120 == 1 {
            let [a, b] = [engine.player(0), engine.player(1)];
            println!("mk64 music: frame {} p0 {} seq {} tempo {} vol {:.2} | p1 {} seq {} tempo {}", self.frame, a.enabled, a.sequence, a.tempo, a.volume, b.enabled, b.sequence, b.tempo);
        }
        if self.silent > IDLE_FRAMES {
            self.engine = None;
            self.silent = 0;
        }
        Some(&self.pcm)
    }

    /// Apply `cues`, then render `frames` video frames offline (tests and tooling). Returns the interleaved PCM.
    pub fn render_offline(&mut self, cues: &[Cue], frames: usize) -> Vec<i16> {
        for c in cues {
            self.apply(c);
        }
        let mut out = vec![];
        for _ in 0..frames {
            if let Some(p) = self.render() {
                out.extend_from_slice(p);
            }
        }
        out
    }
}
