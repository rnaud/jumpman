//! What the MK64 experiment should sound like, engine-free: kart state and race events in, sound cues and
//! continuous engine/skid parameters out. `src/game/experiments/kartSound.ts` is the browser twin; the synthesis is
//! `kart_audio.rs` / `kartAudio.ts`, both playing `assets/audio/kart-voices.json`.
use super::kart_race::{Event, Item, LAPS};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    Hop,
    DriftStart,
    Charge1,
    Charge2,
    Turbo,
    Bump,
    Box,
    Roulette,
    ItemGet,
    Mushroom,
    BananaDrop,
    BananaHit,
    ShellFire,
    ShellHit,
    Count,
    Go,
    Lap,
    FinalLap,
    Finish,
    Lakitu,
}

impl Cue {
    #[cfg(test)]
    pub const ALL: [Cue; 20] = [
        Cue::Hop,
        Cue::DriftStart,
        Cue::Charge1,
        Cue::Charge2,
        Cue::Turbo,
        Cue::Bump,
        Cue::Box,
        Cue::Roulette,
        Cue::ItemGet,
        Cue::Mushroom,
        Cue::BananaDrop,
        Cue::BananaHit,
        Cue::ShellFire,
        Cue::ShellHit,
        Cue::Count,
        Cue::Go,
        Cue::Lap,
        Cue::FinalLap,
        Cue::Finish,
        Cue::Lakitu,
    ];
    /// Key of the voice in `kart-voices.json`.
    pub fn voice(self) -> &'static str {
        match self {
            Cue::Hop => "hop",
            Cue::DriftStart => "driftStart",
            Cue::Charge1 => "charge1",
            Cue::Charge2 => "charge2",
            Cue::Turbo => "turbo",
            Cue::Bump => "bump",
            Cue::Box => "box",
            Cue::Roulette => "roulette",
            Cue::ItemGet => "itemGet",
            Cue::Mushroom => "mushroom",
            Cue::BananaDrop => "bananaDrop",
            Cue::BananaHit => "bananaHit",
            Cue::ShellFire => "shellFire",
            Cue::ShellHit => "shellHit",
            Cue::Count => "count",
            Cue::Go => "go",
            Cue::Lap => "lap",
            Cue::FinalLap => "finalLap",
            Cue::Finish => "finish",
            Cue::Lakitu => "lakitu",
        }
    }
}

/// The slice of kart and racer state the sounds follow (sampled once per 30 Hz tick).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoundFrame {
    pub hop: bool,
    pub drifting: bool,
    pub drift_state: i32,
    pub mini_turbo: bool,
    pub bumped: bool,
    pub midair: bool,
    /// currentSpeed / top speed, may exceed 1 while boosting.
    pub speed_ratio: f64,
    pub grass: bool,
    pub accel: bool,
    pub reversing: bool,
    /// Roulette ticks left (0 = not spinning) and the sim clock for the tick rhythm.
    pub roulette: u32,
    pub clock: u32,
    pub spinning: bool,
    pub boosting: bool,
    pub driving: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Engine {
    /// Fundamental in Hz.
    pub freq: f64,
    pub gain: f64,
    /// Tyre skid noise level.
    pub skid: f64,
}

/// Engine note: idles low and climbs with speed, with a lift while the throttle is down, like the MK64 engine
/// sample being resampled by speed. Silent when parked on foot.
pub fn engine_sound(f: &SoundFrame) -> Engine {
    if !f.driving {
        return Engine { freq: 50., gain: 0., skid: 0. };
    }
    let r = f.speed_ratio.clamp(0., 1.3);
    let freq = 46. + 128. * r.powf(0.85) + if f.accel { 7. } else { 0. } + if f.boosting { 22. } else { 0. } + if f.reversing { 10. } else { 0. };
    let load = if f.accel { 0.2 } else { 0. };
    let gain = (0.2 + 0.34 * r.min(1.) + load) * if f.midair { 0.55 } else { 1. } * if f.spinning { 0.5 } else { 1. };
    let slide: f64 = if f.drifting && !f.hop { 0.55 } else { 0. };
    let rough: f64 = if f.grass && !f.midair { 0.35 * r.min(1.) } else { 0. };
    Engine { freq, gain, skid: slide.max(rough) * if f.midair { 0. } else { 1. } }
}

/// Edge detector: previous frame in, cues out.
#[derive(Debug, Default)]
pub struct KartSounds {
    prev: Option<SoundFrame>,
}

impl KartSounds {
    pub fn reset(&mut self) {
        self.prev = None;
    }

    pub fn update(&mut self, f: &SoundFrame, events: &[Event]) -> Vec<Cue> {
        let mut cues = vec![];
        let p = self.prev.replace(*f);
        if let Some(p) = p.filter(|p| p.driving && f.driving) {
            if f.hop && !p.hop {
                cues.push(Cue::Hop);
            }
            if f.drifting && !p.drifting && !f.hop {
                cues.push(Cue::DriftStart);
            }
            if f.drifting && f.drift_state > p.drift_state {
                cues.push(if f.drift_state >= 2 { Cue::Charge2 } else { Cue::Charge1 });
            }
            if f.mini_turbo && !p.mini_turbo && !f.boosting {
                cues.push(Cue::Turbo);
            }
            if f.bumped && !p.bumped && f.speed_ratio > 0.1 {
                cues.push(Cue::Bump);
            }
            if f.roulette > 0 && f.clock % 3 == 0 {
                cues.push(Cue::Roulette);
            }
            if p.roulette > 0 && f.roulette == 0 {
                cues.push(Cue::ItemGet);
            }
        }
        for e in events {
            match *e {
                Event::Count(_) => cues.push(Cue::Count),
                Event::Go => cues.push(Cue::Go),
                Event::Pickup(0) => cues.push(Cue::Box),
                Event::Use(0, item) => cues.push(match item {
                    Item::Mushroom => Cue::Mushroom,
                    Item::Banana => Cue::BananaDrop,
                    Item::Shell => Cue::ShellFire,
                }),
                Event::Smash(0, item) => cues.push(if item == Item::Banana { Cue::BananaHit } else { Cue::ShellHit }),
                Event::Lap(0, n) if n < LAPS => cues.push(if n == LAPS - 1 { Cue::FinalLap } else { Cue::Lap }),
                Event::Finish(0) => cues.push(Cue::Finish),
                Event::Rescue(0) => cues.push(Cue::Lakitu),
                _ => {}
            }
        }
        cues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> SoundFrame {
        SoundFrame { speed_ratio: 0.5, accel: true, clock: 1, driving: true, ..Default::default() }
    }

    #[test]
    fn engine_climbs_with_speed_and_is_silent_on_foot() {
        let at = |r: f64, accel: bool| engine_sound(&SoundFrame { speed_ratio: r, accel, ..base() });
        let (idle, half, full) = (at(0., false), at(0.5, true), at(1., true));
        let boost = engine_sound(&SoundFrame { speed_ratio: 1.15, boosting: true, ..base() });
        assert!(idle.freq < half.freq && half.freq < full.freq && full.freq < boost.freq);
        assert!(idle.gain > 0.1 && full.gain > idle.gain);
        assert_eq!(engine_sound(&SoundFrame { driving: false, ..base() }).gain, 0.);
        assert!(engine_sound(&SoundFrame { midair: true, ..base() }).gain < half.gain);
    }

    #[test]
    fn skids_while_drifting_or_on_grass_not_in_the_air() {
        assert!(engine_sound(&SoundFrame { drifting: true, ..base() }).skid > 0.3);
        assert!(engine_sound(&SoundFrame { grass: true, ..base() }).skid > 0.);
        assert_eq!(engine_sound(&base()).skid, 0.);
        assert_eq!(engine_sound(&SoundFrame { drifting: true, midair: true, ..base() }).skid, 0.);
    }

    #[test]
    fn cues_fire_on_edges() {
        let mut s = KartSounds::default();
        assert!(s.update(&base(), &[]).is_empty());
        assert_eq!(s.update(&SoundFrame { hop: true, ..base() }, &[]), [Cue::Hop]);
        assert!(s.update(&SoundFrame { hop: true, ..base() }, &[]).is_empty());
        assert_eq!(s.update(&SoundFrame { drifting: true, ..base() }, &[]), [Cue::DriftStart]);
        assert_eq!(s.update(&SoundFrame { drifting: true, drift_state: 1, ..base() }, &[]), [Cue::Charge1]);
        assert_eq!(s.update(&SoundFrame { drifting: true, drift_state: 2, ..base() }, &[]), [Cue::Charge2]);
        assert_eq!(s.update(&SoundFrame { mini_turbo: true, ..base() }, &[]), [Cue::Turbo]);
        assert_eq!(s.update(&SoundFrame { mini_turbo: true, bumped: true, ..base() }, &[]), [Cue::Bump]);
        assert_eq!(s.update(&SoundFrame { roulette: 30, clock: 3, ..base() }, &[]), [Cue::Roulette]);
        assert!(s.update(&SoundFrame { roulette: 29, clock: 4, ..base() }, &[]).is_empty());
        assert_eq!(s.update(&SoundFrame { roulette: 0, clock: 5, ..base() }, &[]), [Cue::ItemGet]);
    }

    #[test]
    fn race_events_map_to_cues_for_the_player_only() {
        let mut s = KartSounds::default();
        s.update(&base(), &[]);
        assert_eq!(s.update(&base(), &[Event::Count(3), Event::Go]), [Cue::Count, Cue::Go]);
        let events = [
            Event::Pickup(0),
            Event::Pickup(2),
            Event::Use(0, Item::Mushroom),
            Event::Use(0, Item::Banana),
            Event::Use(1, Item::Shell),
            Event::Use(0, Item::Shell),
            Event::Smash(0, Item::Banana),
            Event::Smash(0, Item::Shell),
        ];
        assert_eq!(s.update(&base(), &events), [Cue::Box, Cue::Mushroom, Cue::BananaDrop, Cue::ShellFire, Cue::BananaHit, Cue::ShellHit]);
        let laps = [Event::Lap(0, 1), Event::Lap(0, 2), Event::Lap(0, 3), Event::Lap(1, 1), Event::Finish(0), Event::Rescue(0)];
        assert_eq!(s.update(&base(), &laps), [Cue::Lap, Cue::FinalLap, Cue::Finish, Cue::Lakitu]);
    }
}
