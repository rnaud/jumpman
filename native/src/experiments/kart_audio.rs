//! Synthesised Mario Kart 64 audio for the kart experiment: a looped engine note whose pitch follows the speed,
//! a tyre skid layer and the one-shot cues of `kart_sound`. `src/game/experiments/kartAudio.ts` is the browser twin;
//! both play the layer tables in `assets/audio/kart-voices.json` (the MK64 samples are not in the repository).
//!
//! The synth ([`KartSynth`]) is plain DSP and unit tested. The Bevy side ([`KartAudioPlugin`]) streams it as an
//! audio source while the kart experiment is active; the experiment talks to it through the module functions
//! ([`set_active`], [`set_engine`], [`play`]) so that neither `Host` nor `XpParams` grew a field.
use super::kart_sound::{Cue, Engine};
use bevy::audio::{AddAudioSource, Decodable, Source};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const VOICES_JSON: &str = include_str!("../../../assets/audio/kart-voices.json");
/// Output rate of the synth; rodio resamples to the device.
pub const RATE: u32 = 44100;
/// `Sfx` master (0.24) times the kart bus gain (1.6) of the browser twin.
const OUTPUT_GAIN: f32 = 0.24 * 1.6;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Wave {
    Sine,
    Square,
    Saw,
    Tri,
    Noise,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Layer {
    pub wave: Wave,
    pub f0: f64,
    pub f1: f64,
    pub dur: f64,
    pub gain: f64,
    #[serde(default)]
    pub t0: f64,
    #[serde(default)]
    pub attack: Option<f64>,
    #[serde(default)]
    pub hold: bool,
    #[serde(default)]
    pub vib: Option<[f64; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Voice {
    pub gain: f64,
    pub layers: Vec<Layer>,
}

/// The voice table, keyed like the JSON (`Cue::voice`).
pub fn voices() -> &'static HashMap<String, Voice> {
    static TABLE: OnceLock<HashMap<String, Voice>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let raw: HashMap<String, serde_json::Value> = serde_json::from_str(VOICES_JSON).expect("kart-voices.json");
        raw.into_iter().filter(|(k, _)| !k.starts_with('_')).map(|(k, v)| (k, serde_json::from_value(v).expect("voice"))).collect()
    })
}

struct Playing {
    layer: Layer,
    /// Samples since the voice started (negative until `t0`).
    age: i64,
    phase: f64,
    lp: f64,
    peak: f64,
}

pub struct KartSynth {
    rate: f64,
    playing: Vec<Playing>,
    engine: Engine,
    smooth: Engine,
    phases: [f64; 3],
    lp: [f64; 2],
    skid_lp: [f64; 2],
    noise: u32,
}

impl KartSynth {
    pub fn new(rate: u32) -> Self {
        let quiet = Engine { freq: 50., gain: 0., skid: 0. };
        Self { rate: rate as f64, playing: vec![], engine: quiet, smooth: quiet, phases: [0.; 3], lp: [0.; 2], skid_lp: [0.; 2], noise: 0x1234567 }
    }

    pub fn set_engine(&mut self, e: Engine) {
        self.engine = e;
    }

    pub fn play(&mut self, cue: Cue) {
        let Some(voice) = voices().get(cue.voice()) else { return };
        for layer in &voice.layers {
            self.playing.push(Playing { layer: layer.clone(), age: -((layer.t0 * self.rate) as i64), phase: 0., lp: 0., peak: (layer.gain * voice.gain).max(0.0001) });
        }
        // Keep the mix bounded if cues pile up.
        if self.playing.len() > 48 {
            self.playing.drain(0..self.playing.len() - 48);
        }
    }

    #[cfg(test)]
    pub fn voices_playing(&self) -> usize {
        self.playing.len()
    }

    fn white(&mut self) -> f64 {
        self.noise = self.noise.wrapping_mul(1664525).wrapping_add(1013904223);
        self.noise as f64 / 2147483648. - 1.
    }

    /// One-pole low-pass coefficient for a cutoff.
    fn coef(&self, fc: f64) -> f64 {
        1. - (-TAU * fc.clamp(20., self.rate * 0.45) / self.rate).exp()
    }

    pub fn next_sample(&mut self) -> f32 {
        let rate = self.rate;
        // Engine and skid parameters glide (the browser uses setTargetAtTime with ~50 ms).
        let k = 1. - (-1. / (0.05 * rate)).exp();
        self.smooth.freq += (self.engine.freq - self.smooth.freq) * k;
        self.smooth.gain += (self.engine.gain - self.smooth.gain) * k;
        self.smooth.skid += (self.engine.skid - self.smooth.skid) * k;
        let mut out = 0.;
        if self.smooth.gain > 1e-4 {
            let f = self.smooth.freq;
            let mut mix = 0.;
            for (i, (ratio, level, square)) in [(1., 0.5, false), (1.006, 0.4, false), (0.5, 0.45, true)].into_iter().enumerate() {
                self.phases[i] = (self.phases[i] + f * ratio / rate).fract();
                let p = self.phases[i];
                let v = if square { if p < 0.5 { 1. } else { -1. } } else { 2. * p - 1. };
                mix += v * level;
            }
            // Two cascaded one-pole low-passes that open with the revs (the browser's resonant low-pass).
            let a = self.coef(380. + f * 9.);
            self.lp[0] += (mix - self.lp[0]) * a;
            self.lp[1] += (self.lp[0] - self.lp[1]) * a;
            out += self.lp[1] * self.smooth.gain * 0.5 * 1.8;
        }
        if self.smooth.skid > 1e-3 {
            let n = self.white();
            let centre = 1900. + self.smooth.freq * 6.;
            let (hi, lo) = (self.coef(centre * 0.55), self.coef(centre * 1.6));
            self.skid_lp[0] += (n - self.skid_lp[0]) * lo;
            self.skid_lp[1] += (n - self.skid_lp[1]) * hi;
            out += (self.skid_lp[0] - self.skid_lp[1]) * self.smooth.skid * 0.35 * 2.2;
        }
        let mut i = 0;
        while i < self.playing.len() {
            let noise = self.white();
            let p = &mut self.playing[i];
            if p.age < 0 {
                p.age += 1;
                i += 1;
                continue;
            }
            let l = &p.layer;
            let t = p.age as f64 / rate;
            if t >= l.dur {
                self.playing.swap_remove(i);
                continue;
            }
            let u = t / l.dur;
            let attack = l.attack.unwrap_or(0.003);
            let env = if t < attack {
                0.0001 + (p.peak - 0.0001) * t / attack
            } else if l.hold && u < 0.7 {
                p.peak
            } else {
                let (from, span) = if l.hold { (l.dur * 0.7, l.dur * 0.3) } else { (attack, l.dur - attack) };
                p.peak * (0.0001 / p.peak).powf(((t - from) / span).clamp(0., 1.))
            };
            let freq = l.f0 * (l.f1 / l.f0).powf(u);
            let sample = if l.wave == Wave::Noise {
                let a = 1. - (-TAU * freq.clamp(20., rate * 0.45) / rate).exp();
                p.lp += (noise - p.lp) * a;
                p.lp * 1.6
            } else {
                let vib = l.vib.map_or(1., |[hz, depth]| 1. + depth * (TAU * hz * t).sin() * l.f0.max(l.f1) / freq);
                p.phase = (p.phase + freq * vib / rate).fract();
                match l.wave {
                    Wave::Sine => (TAU * p.phase).sin(),
                    Wave::Square => if p.phase < 0.5 { 1. } else { -1. },
                    Wave::Saw => 2. * p.phase - 1.,
                    Wave::Tri => 4. * (p.phase - 0.5).abs() - 1.,
                    Wave::Noise => 0.,
                }
            };
            out += sample * env;
            p.age += 1;
            i += 1;
        }
        // The browser runs a compressor on this bus; a soft clip keeps the loud cues from distorting.
        (out * 1.6).tanh() as f32
    }

    #[cfg(test)]
    pub fn render(&mut self, out: &mut [f32]) {
        for s in out {
            *s = self.next_sample();
        }
    }
}

/// State shared between the experiment (game thread) and the audio thread.
#[derive(Default)]
struct Shared {
    active: bool,
    engine: Option<Engine>,
    cues: Vec<Cue>,
}

fn shared() -> &'static Mutex<Shared> {
    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
    SHARED.get_or_init(|| Mutex::new(Shared::default()))
}
static MASTER: AtomicU32 = AtomicU32::new(0);
/// Last engine parameters handed to the synth, for the debug hook.
static LAST: Mutex<Option<Engine>> = Mutex::new(None);

/// The experiment is entered (true) or left (false); the stream starts and stops with it.
pub fn set_active(on: bool) {
    if let Ok(mut s) = shared().lock() {
        s.active = on;
        if !on {
            s.cues.clear();
            s.engine = None;
        }
    }
}
pub fn set_engine(e: Engine) {
    if let Ok(mut s) = shared().lock() {
        s.engine = Some(e);
    }
    if let Ok(mut l) = LAST.lock() {
        *l = Some(e);
    }
}
pub fn play(cue: Cue) {
    if let Ok(mut s) = shared().lock() {
        if s.cues.len() < 32 {
            s.cues.push(cue);
        }
    }
}
/// The last engine parameters, e.g. `freq=123.4 gain=0.52` for the status line / debug.
pub fn last_engine() -> Option<Engine> {
    LAST.lock().ok().and_then(|l| *l)
}

/// The Bevy asset: a decoder that renders the synth in the audio thread.
#[derive(Asset, TypePath)]
pub struct KartAudio;

pub struct KartSource {
    synth: KartSynth,
    countdown: u32,
}

impl Decodable for KartAudio {
    type DecoderItem = f32;
    type Decoder = KartSource;
    fn decoder(&self) -> KartSource {
        KartSource { synth: KartSynth::new(RATE), countdown: 0 }
    }
}

impl Iterator for KartSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.countdown == 0 {
            self.countdown = 128;
            if let Ok(mut s) = shared().try_lock() {
                if let Some(e) = s.engine {
                    self.synth.set_engine(e);
                } else {
                    self.synth.set_engine(Engine { freq: 50., gain: 0., skid: 0. });
                }
                for cue in s.cues.drain(..) {
                    self.synth.play(cue);
                }
            }
        }
        self.countdown -= 1;
        Some(self.synth.next_sample() * f32::from_bits(MASTER.load(Ordering::Relaxed)))
    }
}

impl Source for KartSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[derive(Component)]
struct KartAudioPlayer;

pub struct KartAudioPlugin;

impl Plugin for KartAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<KartAudio>().add_systems(Update, gate);
    }
}

/// Start the stream while the experiment is active, stop it afterwards; follow the global mute.
fn gate(
    mut commands: Commands,
    mut assets: ResMut<Assets<KartAudio>>,
    audio: Option<Res<crate::audio::GameAudio>>,
    players: Query<Entity, With<KartAudioPlayer>>,
) {
    let active = shared().lock().map(|s| s.active).unwrap_or(false);
    let muted = audio.is_some_and(|a| a.is_muted());
    MASTER.store(if muted { 0. } else { OUTPUT_GAIN }.to_bits(), Ordering::Relaxed);
    if std::env::var_os("JUMPMAN_NO_AUDIO").is_some() {
        return;
    }
    if active && players.is_empty() {
        commands.spawn((KartAudioPlayer, AudioPlayer(assets.add(KartAudio))));
    } else if !active {
        for e in &players {
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(s: &mut KartSynth, n: usize) -> Vec<f32> {
        let mut out = vec![0.; n];
        s.render(&mut out);
        out
    }
    fn energy(v: &[f32]) -> f32 {
        v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32
    }
    /// Zero crossings per second.
    fn pitch(v: &[f32]) -> f64 {
        let crossings = v.windows(2).filter(|w| w[0] <= 0. && w[1] > 0.).count();
        crossings as f64 * RATE as f64 / v.len() as f64
    }

    #[test]
    fn every_cue_has_a_voice_that_is_audible_bounded_and_ends() {
        for cue in Cue::ALL {
            assert!(voices().contains_key(cue.voice()), "{cue:?}");
            let mut s = KartSynth::new(RATE);
            s.play(cue);
            assert!(s.voices_playing() > 0);
            let out = render(&mut s, RATE as usize * 3);
            assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{cue:?} out of range");
            let head = energy(&out[..RATE as usize / 4]);
            assert!(head > 1e-4, "{cue:?} is silent ({head})");
            assert!(energy(&out[RATE as usize * 5 / 2..]) < 1e-9, "{cue:?} never ends");
            assert_eq!(s.voices_playing(), 0);
        }
        assert_eq!(voices().len(), Cue::ALL.len());
    }

    #[test]
    fn engine_pitch_follows_the_speed_and_silence_when_parked_on_foot() {
        let at = |freq: f64| {
            let mut s = KartSynth::new(RATE);
            s.set_engine(Engine { freq, gain: 0.5, skid: 0. });
            render(&mut s, RATE as usize / 2);
            pitch(&render(&mut s, RATE as usize))
        };
        let (low, high) = (at(60.), at(180.));
        assert!(low > 25. && high > low * 2., "low {low} high {high}");
        let mut quiet = KartSynth::new(RATE);
        quiet.set_engine(Engine { freq: 50., gain: 0., skid: 0. });
        assert!(energy(&render(&mut quiet, RATE as usize / 4)) < 1e-12);
    }

    #[test]
    fn skid_adds_noise_and_cues_layer_over_the_engine() {
        let mut s = KartSynth::new(RATE);
        s.set_engine(Engine { freq: 120., gain: 0., skid: 0. });
        render(&mut s, 8000);
        assert!(energy(&render(&mut s, 8000)) < 1e-12);
        s.set_engine(Engine { freq: 120., gain: 0., skid: 0.55 });
        render(&mut s, 8000);
        assert!(energy(&render(&mut s, 8000)) > 1e-5, "skid is audible on its own");
        s.set_engine(Engine { freq: 120., gain: 0.4, skid: 0.55 });
        s.play(Cue::ShellHit);
        s.play(Cue::Turbo);
        assert!(s.voices_playing() >= 5);
        let out = render(&mut s, 4000);
        assert!(out.iter().all(|x| x.abs() <= 1.0));
    }

    #[test]
    fn module_api_queues_cues_for_the_audio_thread_and_clears_on_exit() {
        // The queue is process-wide and other kart tests enter/leave the mode in parallel, so one
        // attempt can see their cues; a clean sequence must be observed within a few tries.
        let attempt = || {
            set_active(true);
            play(Cue::Hop);
            set_engine(Engine { freq: 99., gain: 0.3, skid: 0. });
            let queued = {
                let s = shared().lock().unwrap();
                s.active && s.cues == [Cue::Hop] && s.engine.is_some()
            };
            let freq = last_engine().map(|e| e.freq);
            set_active(false);
            let s = shared().lock().unwrap();
            let cleared = !s.active && s.cues.is_empty() && s.engine.is_none();
            queued && cleared && freq == Some(99.)
        };
        assert!((0..200).any(|_| attempt()), "cues never queued cleanly");
    }
}
