//! Audio-thread side: turns [`Cue`]s into the game-side calls of the MK64 sound library and renders its
//! output one video frame (1/60 s) at a time.
use super::courses::*;
use super::engine::{Mk64Engine, Mk64Module, PlayerState};
use super::sfx::{PlayerIn, SfxCmd};

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
    /// A sound effect request (see [`SfxCmd`]).
    Sfx(SfxCmd),
    /// Fade everything out and reset the library.
    Stop,
}

/// Frames of audio a fade-out takes before the library is reset.
const STOP_FRAMES: u32 = 40;
/// Silent frames (no music player running) after which the idle engine is dropped.
const IDLE_FRAMES: u32 = 150;

/// The kart session of the sound effects: what the host last said about the listener and the karts. The library
/// reads kart positions by pointer every frame, so they are re-sent (moved on by their velocity) before each
/// frame; the host only updates at its own rate.
struct Session {
    course: u32,
    humans: u32,
    camera: ([f32; 3], i32, [f32; 3]),
    camera_age: u32,
    players: [Option<(PlayerIn, u32)>; 8],
    engines: [bool; 8],
}

/// Frames a position is carried forward by its velocity before the host must have updated it.
const EXTRAPOLATE_MAX: u32 = 6;

pub struct Processor {
    module: Mk64Module,
    engine: Option<Mk64Engine>,
    reset_in: Option<u32>,
    silent: u32,
    frame: u64,
    log: bool,
    /// The last course theme requested (for the log and for the finish logic).
    pub course: Option<String>,
    /// A theme asked for while the start fanfare is still playing: it begins when the fanfare ends (the game's
    /// own order; the fanfare is about 7 s long, longer than the kart mode's 3-2-1).
    pending_theme: Option<String>,
    pcm: Vec<i16>,
    sfx: Option<Session>,
    /// Set once a library with sound effects ran a `Begin`; read by the Bevy side to retire the synthesised stand-ins.
    pub sfx_ok: bool,
}

impl Processor {
    pub fn new(module: Mk64Module) -> Self {
        Self { module, engine: None, reset_in: None, silent: 0, frame: 0, log: std::env::var_os("JUMPMAN_MUSIC_LOG").is_some(), course: None, pending_theme: None, pcm: vec![], sfx: None, sfx_ok: false }
    }

    fn engine(&mut self) -> Option<&mut Mk64Engine> {
        if self.engine.is_none() {
            match self.module.instantiate() {
                Ok(e) => {
                    self.engine = Some(e);
                    // A session outlives the instance (a Stop resets the library): start it again.
                    if let (Some(s), Some(e)) = (self.sfx.as_ref(), self.engine.as_mut()) {
                        e.sfx(&SfxCmd::Begin { course: s.course, humans: s.humans });
                    }
                }
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
                self.pending_theme = None;
                if let Some(e) = self.engine.as_mut() {
                    let fanfare = e.player(1);
                    if fanfare.enabled && fanfare.sequence == SEQ_RACE_STARTING {
                        if self.log {
                            println!("mk64 music: course {course} -> sequence {seq} after the fanfare");
                        }
                        self.pending_theme = Some(course.clone());
                        return;
                    }
                }
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
                self.pending_theme = None;
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
            Cue::Sfx(cmd) => self.apply_sfx(cmd),
            Cue::Stop => {
                self.sfx = None;
                if let Some(e) = self.engine.as_mut() {
                    // 0x1P(t)...: op 1 = fade the player out over t * 8 frames.
                    e.command(0x1005_00FF);
                    e.command(0x1105_00FF);
                    self.reset_in = Some(STOP_FRAMES);
                }
                self.course = None;
                self.pending_theme = None;
            }
        }
    }

    /// A sound-effect request. Without a session only the global sounds (menus, lights) are served.
    fn apply_sfx(&mut self, cmd: &SfxCmd) {
        match cmd {
            SfxCmd::Begin { course, humans } => {
                let Some(e) = self.engine() else { return };
                if !e.has_sfx() {
                    bevy::log::warn!("mk64-audio.wasm has no sound effects (rebuild it with scripts/export-mk64-music.py)");
                    return;
                }
                e.sfx(cmd);
                self.sfx_ok = true;
                self.sfx = Some(Session { course: *course, humans: *humans, camera: ([0.; 3], 0, [0.; 3]), camera_age: 0, players: [None; 8], engines: [false; 8] });
            }
            SfxCmd::End => {
                if let Some(e) = self.engine.as_mut() {
                    e.sfx(cmd);
                }
                self.sfx = None;
            }
            SfxCmd::Camera { pos, yaw } => {
                if let Some(s) = self.sfx.as_mut() {
                    // The listener's velocity is the player's kart's (set with its state); keep the old one.
                    s.camera = (*pos, *yaw, s.camera.2);
                    s.camera_age = 0;
                }
            }
            SfxCmd::Player { slot, kart } => {
                if let (Some(s), true) = (self.sfx.as_mut(), *slot < 8) {
                    s.players[*slot] = Some((*kart, 0));
                    if *slot == 0 {
                        s.camera.2 = kart.vel;
                    }
                }
            }
            SfxCmd::Engine { slot, on } => {
                if *slot >= 8 || self.sfx.is_none() {
                    return;
                }
                self.push_player(*slot);
                if let Some(s) = self.sfx.as_mut() {
                    if s.engines[*slot] == *on {
                        return;
                    }
                    s.engines[*slot] = *on;
                }
                if let Some(e) = self.engine.as_mut() {
                    e.sfx(cmd);
                }
            }
            SfxCmd::Voice { slot, .. } | SfxCmd::VoiceRandom { slot } | SfxCmd::PlayerSound { slot, .. } | SfxCmd::PlayerStop { slot, .. } => {
                if self.sfx.is_none() || *slot >= 8 {
                    return;
                }
                self.push_player(*slot);
                if let Some(e) = self.engine.as_mut() {
                    e.sfx(cmd);
                }
            }
            SfxCmd::Global { .. } => {
                if let Some(e) = self.engine().filter(|e| e.has_sfx()) {
                    e.sfx(cmd);
                }
            }
            SfxCmd::Place { .. } | SfxCmd::PlaceSound { .. } | SfxCmd::PlaceLoop { .. } | SfxCmd::PlaceStop { .. } | SfxCmd::Pause(_) => {
                if self.sfx.is_some() {
                    if let Some(e) = self.engine.as_mut() {
                        e.sfx(cmd);
                    }
                }
            }
        }
    }

    /// Hand one kart's last state (advanced by its velocity for the frames since the host sent it) to the library.
    fn push_player(&mut self, slot: usize) {
        let (Some(s), Some(e)) = (self.sfx.as_ref(), self.engine.as_mut()) else { return };
        if let Some((k, age)) = s.players[slot] {
            let mut k = k;
            let t = age.min(EXTRAPOLATE_MAX) as f32;
            for i in 0..3 {
                k.pos[i] += k.vel[i] * t;
            }
            e.sfx(&SfxCmd::Player { slot, kart: k });
        }
    }

    /// Before every audio frame of a session: listener and karts, carried forward in time.
    fn push_session(&mut self) {
        let Some(s) = self.sfx.as_mut() else { return };
        let t = s.camera_age.min(EXTRAPOLATE_MAX) as f32;
        let (pos, yaw, vel) = s.camera;
        let pos = [pos[0] + vel[0] * t, pos[1] + vel[1] * t, pos[2] + vel[2] * t];
        s.camera_age += 1;
        for p in s.players.iter_mut().flatten() {
            p.1 += 1;
        }
        let used: Vec<usize> = (0..8).filter(|i| s.players[*i].is_some()).collect();
        if let Some(e) = self.engine.as_mut() {
            e.sfx(&SfxCmd::Camera { pos, yaw });
        }
        for i in used {
            self.push_player(i);
        }
    }

    /// Render the next video frame; `None` while idle (no engine).
    pub fn render(&mut self) -> Option<&[i16]> {
        self.push_session();
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
        if self.pending_theme.is_some() && !self.engine.as_mut()?.player(1).enabled {
            if let Some(course) = self.pending_theme.take() {
                self.apply(&Cue::Theme(course));
            }
        }
        let engine = self.engine.as_mut()?;
        let mut running = engine.player(0).enabled || engine.player(1).enabled;
        if !running && engine.has_sfx() {
            // Sound effects still sounding (a menu click, an engine) keep the library alive too.
            running = self.sfx.is_some() || engine.sfx_busy() > 0;
        }
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
