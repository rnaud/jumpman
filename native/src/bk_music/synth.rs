//! A software port of the libultra music path Banjo-Kazooie uses (`core1/n_audio/n_csq.c`, `n_csplayer.c`, `n_seqplayer.c`):
//! * [`Bank`]: the ALBankFile of `soundfont2ctl/tbl` (instruments -> sounds -> envelope / key map / VADPCM wavetable);
//! * [`Seq`]: the compact MIDI ("CSeq") event stream with its loop and back-reference codes;
//! * [`Player`]: channel state, note on / off with durations, tempo, pitch bend, volume / pan controllers and a polyphonic
//!   voice mixer (linear-interpolating resampler, attack / decay / sustain / release envelope).
//! No RSP microcode, reverb or tremolo / vibrato oscillators (BK sets none: `initOsc` is NULL).
//!
//! Everything here is engine-free; `mod.rs` adds the Bevy side.
use byor_rs::bk::voices::vadpcm;
use std::collections::HashMap;
use std::sync::Arc;

/// Output rate of the mixer.
pub const RATE: u32 = 32000;
/// Frames mixed between two passes over the sequence events (2 ms).
const BLOCK: usize = 64;
/// `maxVoices` of `sMusicInstrumentsSeqConfig`.
const MAX_VOICES: usize = 24;

fn be16(d: &[u8], o: usize) -> Result<u16, String> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| format!("bank: read at {o:#x} outside the file"))
}
fn be32(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("bank: read at {o:#x} outside the file"))
}
fn u8_at(d: &[u8], o: usize) -> Result<u8, String> {
    d.get(o).copied().ok_or_else(|| format!("bank: read at {o:#x} outside the file"))
}

#[derive(Clone, Debug)]
pub struct Envelope {
    pub attack_us: i32,
    pub decay_us: i32,
    pub release_us: i32,
    pub attack_vol: u8,
    pub decay_vol: u8,
}

#[derive(Clone, Debug)]
pub struct KeyMap {
    pub vel_min: u8,
    pub vel_max: u8,
    pub key_min: u8,
    pub key_max: u8,
    pub key_base: u8,
    pub detune: i8,
}

#[derive(Clone, Debug)]
pub struct Wave {
    pub base: usize,
    pub len: usize,
    /// 0 = VADPCM, 1 = 16 bit PCM
    pub kind: u8,
    /// (start, end, count) in samples; count 0 = no loop
    pub looped: Option<(u32, u32, u32)>,
    pub book: usize,
}

#[derive(Clone, Debug)]
pub struct Sound {
    pub env: Envelope,
    pub keys: KeyMap,
    pub wave: Wave,
    pub pan: u8,
    pub volume: u8,
}

#[derive(Clone, Debug)]
pub struct Instrument {
    pub volume: u8,
    pub pan: u8,
    pub priority: u8,
    pub bend_range: i16,
    pub sounds: Vec<Sound>,
}

/// An ALBankFile (first bank only, as `alBnkfNew` + `bankArray[0]` do).
pub struct Bank {
    ctl: Vec<u8>,
    tbl: Vec<u8>,
    pub rate: u32,
    pub instruments: Vec<Option<Instrument>>,
    pub percussion: Option<Instrument>,
}

impl Bank {
    pub fn parse(ctl: Vec<u8>, tbl: Vec<u8>) -> Result<Bank, String> {
        if be16(&ctl, 0)? != 0x4231 {
            return Err("not an ALBankFile (revision B1)".into());
        }
        let bank = be32(&ctl, 4)? as usize;
        let count = be16(&ctl, bank)? as usize;
        let rate = be32(&ctl, bank + 4)?;
        let perc = be32(&ctl, bank + 8)? as usize;
        let mut instruments = Vec::with_capacity(count);
        for i in 0..count {
            let p = be32(&ctl, bank + 12 + 4 * i)? as usize;
            instruments.push(if p == 0 { None } else { Some(parse_instrument(&ctl, p)?) });
        }
        let percussion = if perc == 0 { None } else { Some(parse_instrument(&ctl, perc)?) };
        Ok(Bank { ctl, tbl, rate, instruments, percussion })
    }

    fn decode(&self, w: &Wave) -> Result<Sample, String> {
        let end = (w.base + w.len).min(self.tbl.len());
        let data = self.tbl.get(w.base.min(end)..end).ok_or("wave outside the table")?;
        let pcm = if w.kind == 0 {
            let order = be32(&self.ctl, w.book)? as usize;
            let npred = be32(&self.ctl, w.book + 4)? as usize;
            let book_end = (w.book + 8 + 16 * order * npred).min(self.ctl.len());
            vadpcm(&self.ctl[w.book..book_end], data)?
        } else {
            data.chunks_exact(2).map(|c| i16::from_be_bytes([c[0], c[1]])).collect()
        };
        let looped = w.looped.filter(|(s, e, c)| *c != 0 && e > s && (*e as usize) <= pcm.len()).map(|(s, e, _)| (s as usize, e as usize));
        Ok(Sample { pcm, looped })
    }
}

fn parse_instrument(ctl: &[u8], at: usize) -> Result<Instrument, String> {
    let bend_range = be16(ctl, at + 12)? as i16;
    let count = be16(ctl, at + 14)? as usize;
    let mut sounds = Vec::with_capacity(count);
    for i in 0..count {
        let p = be32(ctl, at + 16 + 4 * i)? as usize;
        if p == 0 {
            continue;
        }
        let (env, key, wav) = (be32(ctl, p)? as usize, be32(ctl, p + 4)? as usize, be32(ctl, p + 8)? as usize);
        let looped = match be32(ctl, wav + 12)? as usize {
            0 => None,
            l => Some((be32(ctl, l)?, be32(ctl, l + 4)?, be32(ctl, l + 8)?)),
        };
        sounds.push(Sound {
            env: Envelope { attack_us: be32(ctl, env)? as i32, decay_us: be32(ctl, env + 4)? as i32, release_us: be32(ctl, env + 8)? as i32, attack_vol: u8_at(ctl, env + 12)?, decay_vol: u8_at(ctl, env + 13)? },
            keys: KeyMap { vel_min: u8_at(ctl, key)?, vel_max: u8_at(ctl, key + 1)?, key_min: u8_at(ctl, key + 2)?, key_max: u8_at(ctl, key + 3)?, key_base: u8_at(ctl, key + 4)?, detune: u8_at(ctl, key + 5)? as i8 },
            wave: Wave { base: be32(ctl, wav)? as usize, len: be32(ctl, wav + 4)? as usize, kind: u8_at(ctl, wav + 8)?, looped, book: be32(ctl, wav + 16)? as usize },
            pan: u8_at(ctl, p + 12)?,
            volume: u8_at(ctl, p + 13)?,
        });
    }
    Ok(Instrument { volume: u8_at(ctl, at)?, pan: u8_at(ctl, at + 1)?, priority: u8_at(ctl, at + 2)?, bend_range, sounds })
}

pub struct Sample {
    pub pcm: Vec<i16>,
    pub looped: Option<(usize, usize)>,
}

// ---------------------------------------------------------------------------------------------------------------------
// compact MIDI

const META_TEMPO: u8 = 0x51;
const META_EOT: u8 = 0x2F;
const LOOP_START: u8 = 0x2E;
const LOOP_END: u8 = 0x2D;
const BLOCK_CODE: u8 = 0xFE;

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Tempo(u32),
    TrackEnd,
    LoopStart,
    LoopEnd,
    Midi { status: u8, b1: u8, b2: u8, duration: u32 },
}

#[derive(Clone)]
struct Track {
    at: usize,
    backup: usize,
    backup_left: usize,
    last_status: u8,
    /// absolute tick of the next event
    due: f64,
    live: bool,
}

/// The cursor over a compact MIDI file (`ALCSeq`). The file is copied because the loop counters live in it.
#[derive(Clone)]
pub struct Seq {
    data: Vec<u8>,
    tracks: Vec<Track>,
    pub division: u32,
}

impl Seq {
    pub fn parse(data: &[u8]) -> Result<Seq, String> {
        if data.len() < 0x44 {
            return Err("sequence too short".into());
        }
        let division = be32(data, 0x40)?;
        let mut s = Seq { data: data.to_vec(), tracks: Vec::new(), division: division.max(1) };
        for i in 0..16 {
            let off = be32(data, 4 * i)? as usize;
            let mut t = Track { at: off, backup: 0, backup_left: 0, last_status: 0, due: 0.0, live: off != 0 };
            if t.live {
                if off >= data.len() {
                    return Err("track offset outside the file".into());
                }
                t.due = s.var_len(&mut t) as f64;
            }
            s.tracks.push(t);
        }
        Ok(s)
    }

    fn byte(&self, t: &mut Track) -> u8 {
        if t.backup_left > 0 {
            let b = self.data.get(t.backup).copied().unwrap_or(0);
            t.backup += 1;
            t.backup_left -= 1;
            return b;
        }
        let b = self.data.get(t.at).copied().unwrap_or(0);
        t.at += 1;
        if b == BLOCK_CODE {
            let next = self.data.get(t.at).copied().unwrap_or(0);
            t.at += 1;
            if next != BLOCK_CODE {
                let lo = self.data.get(t.at).copied().unwrap_or(0) as usize;
                let len = self.data.get(t.at + 1).copied().unwrap_or(0) as usize;
                t.at += 2;
                t.backup = t.at.saturating_sub((next as usize) * 256 + lo + 4);
                t.backup_left = len;
                let r = self.data.get(t.backup).copied().unwrap_or(0);
                t.backup += 1;
                t.backup_left = t.backup_left.saturating_sub(1);
                return r;
            }
        }
        b
    }

    fn var_len(&self, t: &mut Track) -> u32 {
        let mut v = self.byte(t) as u32;
        if v & 0x80 != 0 {
            v &= 0x7F;
            loop {
                let c = self.byte(t) as u32;
                v = (v << 7) + (c & 0x7F);
                if c & 0x80 == 0 {
                    break;
                }
            }
        }
        v
    }

    fn event(&mut self, ti: usize) -> Event {
        let mut t = self.tracks[ti].clone();
        let status = self.byte(&mut t);
        let ev = if status == 0xFF {
            let ty = self.byte(&mut t);
            t.last_status = 0;
            match ty {
                META_TEMPO => {
                    let (a, b, c) = (self.byte(&mut t) as u32, self.byte(&mut t) as u32, self.byte(&mut t) as u32);
                    Event::Tempo((a << 16) | (b << 8) | c)
                }
                LOOP_START => {
                    self.byte(&mut t);
                    self.byte(&mut t);
                    Event::LoopStart
                }
                LOOP_END => {
                    // raw bytes: count, remaining, u32 distance back from the end of the event
                    let p = t.at;
                    let (total, left) = (self.data[p], self.data[p + 1]);
                    if left == 0 {
                        self.data[p + 1] = total;
                        t.at = p + 6;
                    } else {
                        if left != 0xFF {
                            self.data[p + 1] = left - 1;
                        }
                        let end = p + 6;
                        let dist = u32::from_be_bytes([self.data[p + 2], self.data[p + 3], self.data[p + 4], self.data[p + 5]]) as usize;
                        t.at = end.saturating_sub(dist);
                    }
                    t.backup_left = 0;
                    Event::LoopEnd
                }
                // META_EOT and anything unknown end the track
                _ => {
                    t.live = false;
                    Event::TrackEnd
                }
            }
        } else {
            let (st, b1) = if status & 0x80 != 0 {
                let b1 = self.byte(&mut t);
                t.last_status = status;
                (status, b1)
            } else {
                (t.last_status, status)
            };
            let kind = st & 0xF0;
            let b2 = if kind != 0xC0 && kind != 0xD0 { self.byte(&mut t) } else { 0 };
            let duration = if kind == 0x90 { self.var_len(&mut t) } else { 0 };
            Event::Midi { status: st, b1, b2, duration }
        };
        if t.live {
            let d = self.var_len(&mut t);
            t.due += d as f64;
        }
        self.tracks[ti] = t;
        ev
    }

    pub fn finished(&self) -> bool {
        self.tracks.iter().all(|t| !t.live)
    }

    /// Every event with its absolute tick, up to `limit` events (a looping file would never end): for tests and tooling.
    pub fn dump(&self, limit: usize) -> Vec<(f64, usize, Event)> {
        let mut s = self.clone();
        let mut out = Vec::new();
        while out.len() < limit {
            let Some((ti, due)) = s.tracks.iter().enumerate().filter(|(_, t)| t.live).map(|(i, t)| (i, t.due)).min_by(|a, b| a.1.total_cmp(&b.1)) else { break };
            let ev = s.event(ti);
            out.push((due, ti, ev));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// player

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
}

struct Voice {
    sample: Arc<Sample>,
    pos: f64,
    step: f64,
    chan: usize,
    key: u8,
    vel: u8,
    pan_sound: u8,
    volume_sound: u8,
    level: f32,
    stage: Stage,
    env: Envelope,
    off_at: Option<f64>,
    /// pitch of the key itself (cents applied); channel bend is multiplied per block
    base_ratio: f64,
    age: u64,
    done: bool,
}

#[derive(Clone)]
struct Chan {
    inst: Option<usize>,
    perc: bool,
    pan: u8,
    vol: u8,
    expr: u8,
    bend_range: i16,
    bend: f64,
    sustain: u8,
}

pub struct Player {
    bank: Arc<Bank>,
    cache: HashMap<usize, Arc<Sample>>,
    seq: Option<Seq>,
    chans: Vec<Chan>,
    voices: Vec<Voice>,
    /// ticks per second = 1e6 / tempo_us * division
    tempo_us: f64,
    now: f64,
    mask: u16,
    /// `musicTrackInfo[track].volume / 0x7FFF` times the fade
    pub volume: f32,
    pub paused: bool,
    age: u64,
    loops: u32,
    /// Per-channel volume the game's `func_8025F510` sets (127 = on), `chan_mix`
    pub missing_sounds: u32,
}

fn cents_to_ratio(c: f64) -> f64 {
    2f64.powf(c / 1200.0)
}

impl Player {
    pub fn new(bank: Arc<Bank>) -> Player {
        let mut p = Player { bank, cache: HashMap::new(), seq: None, chans: Vec::new(), voices: Vec::new(), tempo_us: 500_000.0, now: 0.0, mask: 0xFFFF, volume: 0.5, paused: false, age: 0, loops: 0, missing_sounds: 0 };
        p.reset_channels();
        p
    }

    fn reset_channels(&mut self) {
        let first = self.bank.instruments.iter().position(|i| i.is_some());
        self.chans = (0..16).map(|_| Chan { inst: first, perc: false, pan: 64, vol: 127, expr: 127, bend_range: 200, bend: 1.0, sustain: 0 }).collect();
        for c in 0..16 {
            self.set_program(c, first);
        }
        if self.bank.percussion.is_some() {
            self.chans[9].perc = true;
            let p = self.bank.percussion.clone().unwrap();
            self.chans[9].pan = p.pan;
            self.chans[9].vol = p.volume;
            self.chans[9].bend_range = p.bend_range;
        }
    }

    fn set_program(&mut self, chan: usize, program: Option<usize>) {
        let Some(i) = program else { return };
        if let Some(Some(inst)) = self.bank.instruments.get(i) {
            let c = &mut self.chans[chan];
            c.inst = Some(i);
            c.perc = false;
            c.pan = inst.pan;
            c.vol = inst.volume;
            c.bend_range = inst.bend_range;
        }
    }

    pub fn play(&mut self, data: &[u8], volume: f32) -> Result<(), String> {
        let seq = Seq::parse(data)?;
        self.seq = Some(seq);
        self.voices.clear();
        self.reset_channels();
        self.tempo_us = 500_000.0;
        self.now = 0.0;
        self.mask = 0xFFFF;
        self.volume = volume;
        self.loops = 0;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.seq = None;
        self.voices.clear();
    }

    /// True while a sequence still has events (a looping sequence never finishes).
    pub fn playing(&self) -> bool {
        self.seq.as_ref().is_some_and(|s| !s.finished()) || self.voices.iter().any(|v| !v.done)
    }

    pub fn sequence_ended(&self) -> bool {
        self.seq.as_ref().is_none_or(|s| s.finished())
    }

    pub fn loops(&self) -> u32 {
        self.loops
    }

    /// Notes started since the sequence began.
    pub fn notes_started(&self) -> u64 {
        self.age
    }

    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| !v.done).count()
    }

    /// Mute / unmute a layer (`musicSlot_stepToChannelMask`): bit set = channel audible.
    pub fn set_channel_mask(&mut self, mask: u16) {
        self.mask = mask;
    }

    fn sample(&mut self, w: &Wave) -> Option<Arc<Sample>> {
        if let Some(s) = self.cache.get(&w.base) {
            return Some(s.clone());
        }
        let s = Arc::new(self.bank.decode(w).ok()?);
        self.cache.insert(w.base, s.clone());
        Some(s)
    }

    fn lookup(&self, chan: usize, key: u8, vel: u8) -> Option<Sound> {
        let c = &self.chans[chan];
        let inst = if c.perc { self.bank.percussion.as_ref()? } else { self.bank.instruments.get(c.inst?)?.as_ref()? };
        inst.sounds.iter().find(|s| key >= s.keys.key_min && key <= s.keys.key_max && vel >= s.keys.vel_min && vel <= s.keys.vel_max).cloned()
    }

    fn note_on(&mut self, chan: usize, key: u8, vel: u8, duration: u32) {
        if self.mask & (1 << chan) == 0 {
            return;
        }
        let Some(sound) = self.lookup(chan, key, vel) else {
            self.missing_sounds += 1;
            return;
        };
        let Some(sample) = self.sample(&sound.wave) else { return };
        // A key already sounding on the channel is cut (`__n_mapVoice` reuses it).
        for v in self.voices.iter_mut().filter(|v| v.chan == chan && v.key == key && v.stage != Stage::Release) {
            v.stage = Stage::Release;
        }
        self.voices.retain(|v| !v.done);
        if self.voices.len() >= MAX_VOICES {
            if let Some(i) = self.voices.iter().enumerate().min_by_key(|(_, v)| (v.stage != Stage::Release, v.age)).map(|(i, _)| i) {
                self.voices.remove(i);
            }
        }
        let cents = (key as f64 - sound.keys.key_base as f64) * 100.0 + sound.keys.detune as f64;
        let base_ratio = cents_to_ratio(cents) * self.bank.rate as f64 / RATE as f64;
        self.age += 1;
        let off_at = (duration != 0).then(|| self.now + duration as f64);
        self.voices.push(Voice { sample, pos: 0.0, step: base_ratio, chan, key, vel, pan_sound: sound.pan, volume_sound: sound.volume, level: 0.0, stage: Stage::Attack, env: sound.env, off_at, base_ratio, age: self.age, done: false });
    }

    fn release(&mut self, chan: usize, key: Option<u8>) {
        for v in self.voices.iter_mut().filter(|v| v.chan == chan && key.is_none_or(|k| k == v.key) && v.stage != Stage::Release) {
            v.stage = Stage::Release;
            v.off_at = None;
        }
    }

    fn handle(&mut self, ev: Event) {
        match ev {
            Event::Tempo(us) => self.tempo_us = us.max(1) as f64,
            Event::LoopEnd => self.loops += 1,
            Event::LoopStart | Event::TrackEnd => {}
            Event::Midi { status, b1, b2, duration } => {
                let chan = (status & 0x0F) as usize;
                match status & 0xF0 {
                    0x90 if b2 != 0 => self.note_on(chan, b1, b2, duration),
                    0x90 | 0x80 => self.release(chan, Some(b1)),
                    0xB0 => match b1 {
                        0x0A => self.chans[chan].pan = b2,
                        0x07 => self.chans[chan].vol = b2,
                        0x7D => self.chans[chan].expr = b2,
                        0x40 => self.chans[chan].sustain = b2,
                        0x7E => {
                            self.mask &= !(1u16 << (b2 & 15));
                            self.release(b2 as usize & 15, None);
                        }
                        0x7F => self.mask |= 1u16 << (b2 & 15),
                        _ => {}
                    },
                    0xC0 => self.set_program(chan, Some(b1 as usize)),
                    0xE0 => {
                        let v = ((b2 as i32) << 7) + b1 as i32 - 8192;
                        let cents = self.chans[chan].bend_range as i32 * v / 8192;
                        self.chans[chan].bend = cents_to_ratio(cents as f64);
                    }
                    _ => {}
                }
            }
        }
    }

    fn frames_per_tick(&self, seq: &Seq) -> f64 {
        self.tempo_us * 1e-6 * RATE as f64 / seq.division as f64
    }

    /// Mix `frames` stereo frames into `out` (interleaved i16, appended).
    pub fn render(&mut self, frames: usize, out: &mut Vec<i16>) {
        let mut left = frames;
        let mut mix = [0f32; BLOCK * 2];
        while left > 0 {
            let n = left.min(BLOCK);
            left -= n;
            if !self.paused {
                self.run_events(n);
            }
            mix[..n * 2].fill(0.0);
            self.mix(&mut mix[..n * 2]);
            for v in &mix[..n * 2] {
                out.push((v.tanh() * 30000.0) as i16);
            }
        }
    }

    fn run_events(&mut self, frames: usize) {
        let Some(mut seq) = self.seq.take() else { return };
        self.now += frames as f64 / self.frames_per_tick(&seq);
        let mut guard = 0;
        loop {
            let Some((ti, due)) = seq.tracks.iter().enumerate().filter(|(_, t)| t.live).map(|(i, t)| (i, t.due)).min_by(|a, b| a.1.total_cmp(&b.1)) else { break };
            if due > self.now || guard > 20000 {
                break;
            }
            guard += 1;
            let ev = seq.event(ti);
            self.handle(ev);
        }
        // notes whose duration ran out
        for v in self.voices.iter_mut() {
            if v.off_at.is_some_and(|t| t <= self.now) {
                v.stage = Stage::Release;
                v.off_at = None;
            }
        }
        self.seq = Some(seq);
    }

    fn mix(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        let master = self.volume;
        let chans = self.chans.clone();
        let step_ms = frames as f32 / RATE as f32;
        for v in self.voices.iter_mut() {
            if v.done {
                continue;
            }
            let c = &chans[v.chan];
            v.step = v.base_ratio * c.bend;
            let gain = (v.vel as f32 / 127.0) * (v.volume_sound as f32 / 127.0) * (c.vol as f32 / 127.0) * (c.expr as f32 / 127.0) * master;
            let pan = (c.pan as i32 - 64 + v.pan_sound as i32).clamp(0, 127) as f32 / 127.0;
            let (gl, gr) = ((1.0 - pan).sqrt() * gain, pan.sqrt() * gain);
            // envelope target over this block
            let (target, time) = match v.stage {
                Stage::Attack => (v.env.attack_vol as f32 / 127.0, v.env.attack_us),
                Stage::Decay => (v.env.decay_vol as f32 / 127.0, v.env.decay_us),
                Stage::Sustain => (v.level, 0),
                Stage::Release => (0.0, v.env.release_us),
            };
            let secs = if time <= 0 { 0.0 } else { time as f32 / 1e6 };
            let rate = if secs <= 0.0 { f32::INFINITY } else { (target - v.level).abs() / (secs / step_ms).max(1.0) };
            let level0 = v.level;
            let level1 = if v.level < target { (v.level + rate).min(target) } else { (v.level - rate).max(target) };
            if v.level == target || level1 == target {
                match v.stage {
                    Stage::Attack => v.stage = Stage::Decay,
                    Stage::Decay => v.stage = Stage::Sustain,
                    Stage::Release => {}
                    Stage::Sustain => {}
                }
            }
            let s = &v.sample;
            let len = s.pcm.len();
            for f in 0..frames {
                let lv = level0 + (level1 - level0) * (f as f32 / frames as f32);
                let i = v.pos as usize;
                if i + 1 >= len {
                    v.done = true;
                    break;
                }
                let frac = (v.pos - i as f64) as f32;
                let x = (s.pcm[i] as f32 * (1.0 - frac) + s.pcm[i + 1] as f32 * frac) / 32768.0;
                out[f * 2] += x * lv * gl;
                out[f * 2 + 1] += x * lv * gr;
                v.pos += v.step;
                if let Some((ls, le)) = s.looped {
                    if v.pos >= le as f64 {
                        v.pos -= (le - ls) as f64;
                    }
                }
            }
            v.level = level1;
            if v.stage == Stage::Release && v.level <= 0.0005 {
                v.done = true;
            }
        }
        self.voices.retain(|v| !v.done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-track compact MIDI file: `events` are raw track bytes (delta first).
    pub(crate) fn seq_bytes(events: &[u8], division: u32) -> Vec<u8> {
        let mut d = vec![0u8; 0x44];
        d[3] = 0x44;
        d[0x40..0x44].copy_from_slice(&division.to_be_bytes());
        d.extend_from_slice(events);
        d
    }

    #[test]
    fn reads_notes_tempo_and_end() {
        // delta 0: tempo 0x07A120 (500000); delta 0: note-on ch0 key 60 vel 100 dur 48; delta 48: program change ch0 -> 5 ; delta 0: end
        let t = [0x00, 0xFF, 0x51, 0x07, 0xA1, 0x20, 0x00, 0x90, 60, 100, 48, 48, 0xC0, 5, 0x00, 0xFF, 0x2F];
        let seq = Seq::parse(&seq_bytes(&t, 96)).unwrap();
        let evs = seq.dump(10);
        assert_eq!(evs[0].2, Event::Tempo(500_000));
        assert_eq!(evs[1].2, Event::Midi { status: 0x90, b1: 60, b2: 100, duration: 48 });
        assert_eq!(evs[2], (48.0, 0, Event::Midi { status: 0xC0, b1: 5, b2: 0, duration: 0 }));
        assert_eq!(evs[3].2, Event::TrackEnd);
        assert_eq!(evs.len(), 4);
    }

    #[test]
    fn running_status_variable_length_deltas_and_loops() {
        // 0: delta, 1..4 loop start (ends at 5); note at 6..9; delta 0x81 0x00 (=128) + running-status note; loop end x2
        let mut t = vec![0x00, 0xFF, LOOP_START, 0, 0];
        t.extend_from_slice(&[0x00, 0x90, 60, 100, 10]);
        t.extend_from_slice(&[0x81, 0x00, 62, 90, 10]);
        t.extend_from_slice(&[0x00, 0xFF, LOOP_END, 2, 1]);
        // the distance counts back from the end of the event (6 bytes after the type) to the byte after the loop start
        let end = t.len() + 4;
        t.extend_from_slice(&((end - 5) as u32).to_be_bytes());
        t.extend_from_slice(&[0x00, 0xFF, 0x2F]);
        let seq = Seq::parse(&seq_bytes(&t, 96)).unwrap();
        let evs = seq.dump(40);
        let notes = evs.iter().filter(|e| matches!(e.2, Event::Midi { .. })).count();
        // pass 1 (2 notes), loop back once, pass 2 (2 notes), then the loop is done and the track ends
        assert_eq!(notes, 4, "{evs:?}");
        assert_eq!(evs.iter().filter(|e| e.2 == Event::LoopEnd).count(), 2);
        assert_eq!(evs.last().unwrap().2, Event::TrackEnd);
        assert_eq!(evs[2].2, Event::Midi { status: 0x90, b1: 62, b2: 90, duration: 10 });
        assert_eq!(evs[2].0, 128.0);
    }

    #[test]
    fn back_references_repeat_earlier_bytes() {
        // bytes: note(0) ; then 0xFE hi lo len = copy `len` bytes from `backup` before; the copy reproduces `00 90 60 100 10`
        let mut t = vec![0x00, 0x90, 60, 100, 10]; // 5 bytes at track offset 0x44..0x49
        t.extend_from_slice(&[BLOCK_CODE, 0x00, 5, 5]); // copy 5 bytes from (end of the code - (5 + 4)) = the start of the track
        t.extend_from_slice(&[0x00, 0xFF, 0x2F]);
        let seq = Seq::parse(&seq_bytes(&t, 96)).unwrap();
        let evs = seq.dump(10);
        assert_eq!(evs[0].2, Event::Midi { status: 0x90, b1: 60, b2: 100, duration: 10 });
        // the delta after the first note comes from the copy (0x00), then the same bytes again
        assert!(matches!(evs[1].2, Event::Midi { b1: 60, .. }), "{evs:?}");
    }
}
