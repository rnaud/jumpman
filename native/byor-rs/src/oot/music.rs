//! OoT music: the binary sequences of the ROM (Audioseq) run through a Rust port of the decomp's sequence interpreter
//! (`src/audio/internal/seqplayer.c`: sequence player, channel and layer scripts), then rendered with the soundfonts
//! (Audiobank) and samples (Audiotable) into mono 16 kHz WAV clips with a loop region (`assets/audio/oot_music/<n>.wav`
//! plus `manifest.json`). The tables the engine keeps in the `code` segment (sequence table, sequence-to-font table,
//! soundfont table, sample bank table) are located by their structure, so no ROM offsets are hard-coded.
//!
//! Not modelled: reverb / filters, vibrato, portamento, per-update channel volume or pan automation after a note
//! started, multi-player sequences (`runseq`: Hyrule Field's `field_logic` hands over to another sequence, recorded in
//! the manifest as `runseq`), stereo.
use super::sfx::small_adpcm;
use super::voices::{audio_seg, vadpcm, wav_bytes};
use super::{i16_, i32_, u16_, u32_, u8_, Ctx};
use crate::pyc::{dumps, re, round, R};
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

pub const OUT_RATE: f64 = 16000.0;
const ENV_TICK: f64 = 1.0 / 180.0;
/// Longest stretch of music that is rendered for one sequence (seconds).
pub const MAX_SECONDS: f64 = 190.0;
const TAIL_SECONDS: f64 = 2.5;

// ---------------------------------------------------------------------------------------------------------------
// Tables in the code segment

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableEntry {
    pub rom_addr: u32,
    pub size: u32,
    pub medium: i8,
    pub cache: i8,
    pub d1: u16,
    pub d2: u16,
    pub d3: u16,
}

fn entries(code: &[u8], p: usize) -> Vec<TableEntry> {
    let n = u16_(code, p) as usize;
    (0..n)
        .map(|i| {
            let q = p + 16 + 16 * i;
            TableEntry { rom_addr: u32_(code, q), size: u32_(code, q + 4), medium: code[q + 8] as i8, cache: code[q + 9] as i8, d1: u16_(code, q + 10), d2: u16_(code, q + 12), d3: u16_(code, q + 14) }
        })
        .collect()
}

fn find_table(code: &[u8], ok: impl Fn(&[u8], usize, &[TableEntry]) -> bool) -> Option<usize> {
    let mut p = 0;
    while p + 32 <= code.len() {
        let n = u16_(code, p) as usize;
        if (2..400).contains(&n) && p + 16 + 16 * n <= code.len() && ok(code, p, &entries(code, p)) {
            return Some(p);
        }
        p += 16;
    }
    None
}

pub struct Tables {
    pub seqs: Vec<TableEntry>,
    pub fonts: Vec<TableEntry>,
    pub banks: Vec<TableEntry>,
    /// Per sequence: the soundfont ids it names (the last one is the default font).
    pub seq_fonts: Vec<Vec<u8>>,
}

pub fn find_tables(code: &[u8], audioseq_len: usize, audiotable_len: usize) -> R<Tables> {
    let seq_at = find_table(code, |_, _, e| {
        e.len() >= 90 && e.len() <= 160 && e[0].rom_addr == 0 && e[0].size > 0 && e[0].medium == 2 && {
            let real: Vec<_> = e.iter().filter(|x| x.size > 0).collect();
            real.windows(2).all(|w| w[0].rom_addr <= w[1].rom_addr) && real.iter().all(|x| (x.rom_addr + x.size) as usize <= audioseq_len) && e.iter().filter(|x| x.size == 0).all(|x| (x.rom_addr as usize) < e.len())
        }
    })
    .ok_or("OoT music: sequence table not found in the code segment")?;
    let seqs = entries(code, seq_at);
    let font_at = find_table(code, |_, _, e| e.len() >= 30 && e[0].rom_addr == 0 && e[0].size == 15008 && e[1].rom_addr == 15008 && e[1].size == 6064).ok_or("OoT music: soundfont table not found")?;
    let fonts = entries(code, font_at);
    let bank_at = find_table(code, |_, _, e| {
        let real: Vec<_> = e.iter().filter(|x| x.size > 0).collect();
        (3..16).contains(&e.len()) && e[0].rom_addr == 0 && real.len() >= 3 && real.iter().all(|x| (x.rom_addr + x.size) as usize <= audiotable_len) && real.windows(2).all(|w| w[0].rom_addr <= w[1].rom_addr) && real.iter().map(|x| x.size as usize).sum::<usize>() > audiotable_len / 2
    })
    .ok_or("OoT music: sample bank table not found")?;
    let banks = entries(code, bank_at);
    // sequence font table: u16 offsets, one per sequence, each chaining to the next (1 + count bytes later)
    let n = seqs.len();
    let mut seq_fonts = Vec::new();
    let mut p = 0;
    while p + 2 * n + 4 <= code.len() {
        if u16_(code, p) as usize == 2 * n {
            let mut good = true;
            for i in 0..n - 1 {
                let (a, b) = (u16_(code, p + 2 * i) as usize, u16_(code, p + 2 * i + 2) as usize);
                if p + a >= code.len() || b != a + 1 + code[p + a] as usize || code[p + a] > 8 {
                    good = false;
                    break;
                }
            }
            if good {
                for i in 0..n {
                    let a = u16_(code, p + 2 * i) as usize;
                    let cnt = code[p + a] as usize;
                    seq_fonts.push(code[p + a + 1..p + a + 1 + cnt].to_vec());
                }
                break;
            }
        }
        p += 2;
    }
    if seq_fonts.is_empty() {
        return Err("OoT music: sequence font table not found".into());
    }
    Ok(Tables { seqs, fonts, banks, seq_fonts })
}

// ---------------------------------------------------------------------------------------------------------------
// Soundfont access

pub struct Sample {
    pub pcm: Vec<i32>,
    /// `(start, end)` of the loop when it repeats.
    pub lp: Option<(usize, usize)>,
}

pub struct Font {
    data: Vec<u8>,
    bank1: Option<usize>,
    bank2: Option<usize>,
    pub n_inst: usize,
    pub n_drums: usize,
    pub n_sfx: usize,
    cache: RefCell<HashMap<usize, Option<Rc<Sample>>>>,
}

#[derive(Clone)]
pub struct Tuned {
    pub sample: Rc<Sample>,
    pub tuning: f64,
}

#[derive(Clone)]
pub struct Instrument {
    pub env: Option<usize>,
    pub release: u8,
    lo: u8,
    hi: u8,
    low: Option<Tuned>,
    normal: Option<Tuned>,
    high: Option<Tuned>,
}

impl Instrument {
    fn tuned(&self, key: u8) -> Option<&Tuned> {
        if key < self.lo {
            self.low.as_ref()
        } else if key <= self.hi {
            self.normal.as_ref()
        } else {
            self.high.as_ref()
        }
    }
}

pub struct Drum {
    pub env: usize,
    pub release: u8,
    pub pan: u8,
    pub tuned: Option<Tuned>,
}

pub struct Banks<'a> {
    pub table: &'a [u8],
    pub bank_base: Vec<usize>,
}

impl Font {
    fn sample(&self, banks: &Banks, ptr: usize) -> R<Option<Rc<Sample>>> {
        if let Some(c) = self.cache.borrow().get(&ptr) {
            return Ok(c.clone());
        }
        let font = &self.data;
        let (bits, addr, lp, book) = (u32_(font, ptr), u32_(font, ptr + 4) as usize, u32_(font, ptr + 8) as usize, u32_(font, ptr + 12) as usize);
        let (codec, size, medium) = (bits >> 28, (bits & 0xFF_FFFF) as usize, (bits >> 26) & 3);
        let base = match medium {
            0 => self.bank1,
            1 => self.bank2,
            _ => None,
        };
        let r = match (codec, base) {
            (0 | 3, Some(base)) if size > 0 => {
                let (order, npred) = (i32_(font, book) as usize, i32_(font, book + 4) as usize);
                let b = font.get(book..book + 8 + 16 * order * npred).ok_or("book outside the font")?;
                let d = banks.table.get(base + addr..base + addr + size).ok_or("sample outside the Audiotable")?;
                let pcm = if codec == 0 { vadpcm(b, d)? } else { small_adpcm(b, d)? };
                let (ls, le, lc) = (u32_(font, lp) as usize, u32_(font, lp + 4) as usize, u32_(font, lp + 8));
                let lp = if lc != 0 && le > ls { Some((ls, le.min(pcm.len()))) } else { None };
                Some(Rc::new(Sample { pcm, lp }))
            }
            _ => None,
        };
        self.cache.borrow_mut().insert(ptr, r.clone());
        Ok(r)
    }

    fn tuned(&self, banks: &Banks, at: usize) -> R<Option<Tuned>> {
        let (ptr, tuning) = (u32_(&self.data, at) as usize, super::f32_(&self.data, at + 4) as f64);
        if ptr == 0 {
            return Ok(None);
        }
        Ok(self.sample(banks, ptr)?.map(|sample| Tuned { sample, tuning }))
    }

    pub fn instrument(&self, banks: &Banks, i: usize) -> R<Option<Instrument>> {
        if i >= self.n_inst {
            return Ok(None);
        }
        let inst = u32_(&self.data, 8 + 4 * i) as usize;
        if inst == 0 {
            return Ok(None);
        }
        let d = &self.data;
        Ok(Some(Instrument {
            env: Some(u32_(d, inst + 4) as usize).filter(|e| *e != 0),
            release: u8_(d, inst + 3),
            lo: u8_(d, inst + 1),
            hi: u8_(d, inst + 2),
            low: self.tuned(banks, inst + 8)?,
            normal: self.tuned(banks, inst + 16)?,
            high: self.tuned(banks, inst + 24)?,
        }))
    }

    pub fn drum(&self, banks: &Banks, i: usize) -> R<Option<Drum>> {
        if i >= self.n_drums {
            return Ok(None);
        }
        let list = u32_(&self.data, 0) as usize;
        let drum = u32_(&self.data, list + 4 * i) as usize;
        if list == 0 || drum == 0 {
            return Ok(None);
        }
        let d = &self.data;
        Ok(Some(Drum { release: u8_(d, drum), pan: u8_(d, drum + 1), tuned: self.tuned(banks, drum + 4)?, env: u32_(d, drum + 12) as usize }))
    }

    pub fn sfx(&self, banks: &Banks, i: usize) -> R<Option<Tuned>> {
        if i >= self.n_sfx {
            return Ok(None);
        }
        let list = u32_(&self.data, 4) as usize;
        if list == 0 {
            return Ok(None);
        }
        self.tuned(banks, list + 8 * i)
    }

    fn env_points(&self, at: usize) -> Vec<(i64, i64)> {
        read_env(&self.data, at)
    }
}

fn read_env(d: &[u8], at: usize) -> Vec<(i64, i64)> {
    let mut pts = Vec::new();
    for k in 0..64 {
        let q = at + 4 * k;
        if q + 4 > d.len() {
            break;
        }
        let (delay, arg) = (i16_(d, q) as i64, i16_(d, q + 2) as i64);
        pts.push((delay, arg));
        if delay == 0 || delay == -1 || delay == -3 {
            break;
        }
    }
    pts
}

// ---------------------------------------------------------------------------------------------------------------
// Sequence interpreter

/// `sSeqInstructionArgsTable` for 0xB0..=0xFF: `(argument count, which are 16-bit)`.
fn cmd_args(cmd: u8) -> (usize, [bool; 3]) {
    let (n, w): (usize, [bool; 3]) = match cmd {
        0xB0 | 0xB2 | 0xB7 | 0xBC | 0xC2 | 0xCB | 0xCE | 0xCF | 0xDA | 0xDE | 0xE7 | 0xF5 | 0xF9 | 0xFA | 0xFB | 0xFC => (1, [true, false, false]),
        0xB3 | 0xB8 | 0xB9 | 0xBA | 0xC1 | 0xC6 | 0xC8 | 0xC9 | 0xCA | 0xCC | 0xCD | 0xD0..=0xD9 | 0xDB | 0xDC | 0xDD | 0xDF | 0xE0 | 0xE3 | 0xE5 | 0xE6 | 0xE9 | 0xED | 0xEE | 0xF1 | 0xF2 | 0xF3 | 0xF4 | 0xF8 => (1, [false; 3]),
        0xBB | 0xC7 => (2, [false, true, false]),
        0xBD => (2, [true, true, false]),
        0xE1 | 0xE2 | 0xE8 => (3, [false; 3]),
        0xEB => (2, [false; 3]),
        0xEF => (2, [true, false, false]),
        _ => (0, [false; 3]),
    };
    (n, w)
}

#[derive(Clone, Default)]
struct Script {
    pc: usize,
    stack: [usize; 4],
    loops: [i32; 4],
    depth: usize,
    value: i32,
}

fn rd8(seq: &[u8], s: &mut Script) -> u8 {
    let v = seq.get(s.pc).copied().unwrap_or(0xFF);
    s.pc += 1;
    v
}
fn rd16(seq: &[u8], s: &mut Script) -> i32 {
    let hi = rd8(seq, s) as i32;
    let lo = rd8(seq, s) as i32;
    ((hi << 8) | lo) as i16 as i32
}
fn rd_len(seq: &[u8], s: &mut Script) -> u16 {
    let mut r = rd8(seq, s) as u16;
    if r & 0x80 != 0 {
        r = ((r << 8) & 0x7F00) | rd8(seq, s) as u16;
    }
    r
}

const END: i32 = -1;

/// `AudioSeq_HandleScriptFlowControl`; `taken` is set for a backward jump (loop detection).
fn flow(seq: &[u8], s: &mut Script, cmd: u8, arg: i32, jump: &mut Option<(usize, usize)>) -> i32 {
    let before = s.pc;
    match cmd {
        0xFF => {
            if s.depth == 0 {
                return END;
            }
            s.depth -= 1;
            s.pc = s.stack[s.depth];
        }
        0xFD => return rd_len(seq, s) as i32,
        0xFE => return 1,
        0xFC => {
            if s.depth < 4 {
                s.stack[s.depth] = s.pc;
                s.depth += 1;
            }
            s.pc = arg as u16 as usize;
        }
        0xF8 => {
            if s.depth < 4 {
                s.loops[s.depth] = arg;
                s.stack[s.depth] = s.pc;
                s.depth += 1;
            }
        }
        0xF7 => {
            if s.depth > 0 {
                s.loops[s.depth - 1] -= 1;
                if s.loops[s.depth - 1] != 0 {
                    s.pc = s.stack[s.depth - 1];
                } else {
                    s.depth -= 1;
                }
            }
        }
        0xF6 => s.depth = s.depth.saturating_sub(1),
        0xF5 | 0xF9 | 0xFA | 0xFB => {
            let skip = (cmd == 0xFA && s.value != 0) || (cmd == 0xF9 && s.value >= 0) || (cmd == 0xF5 && s.value < 0);
            if !skip {
                s.pc = arg as u16 as usize;
                if s.depth == 0 && s.pc < before {
                    *jump = Some((before, s.pc));
                }
            }
        }
        0xF2 | 0xF3 | 0xF4 => {
            let skip = (cmd == 0xF3 && s.value != 0) || (cmd == 0xF2 && s.value >= 0);
            if !skip {
                s.pc = (s.pc as i64 + (arg & 0xFF) as u8 as i8 as i64) as usize;
                if s.depth == 0 && s.pc < before {
                    *jump = Some((before, s.pc));
                }
            }
        }
        _ => {}
    }
    0
}

#[derive(Clone)]
struct Adsr {
    env: Env,
    decay: u8,
}

#[derive(Clone)]
enum Env {
    /// The default envelope (`gDefaultEnvelope`).
    Default,
    /// Offset into the sequence data.
    Seq(usize),
    /// Offset into the soundfont `font`.
    Font(u8, usize),
}

#[derive(Clone, Default)]
struct Layer {
    enabled: bool,
    finished: bool,
    s: Script,
    delay: i32,
    gate_delay: i32,
    gate_time: u8,
    last_delay: u16,
    short_delay: u16,
    vel_sq: f64,
    transposition: u8,
    continuous: bool,
    muted: bool,
    inst_or_wave: u8,
    instrument: Option<usize>,
    adsr: Option<Adsr>,
    pan: u8,
    ignore_drum_pan: bool,
    bend: f64,
    note: Option<usize>,
}

struct Chan {
    enabled: bool,
    stop: bool,
    s: Script,
    delay: i32,
    layers: [Option<Layer>; 4],
    font: u8,
    inst_or_wave: u8,
    instrument: Option<usize>,
    has_instrument: bool,
    large_notes: bool,
    volume: f64,
    vol_scale: f64,
    pan: u8,
    transposition: i8,
    freq_scale: f64,
    adsr: Adsr,
    io: [i8; 8],
}

impl Chan {
    fn new() -> Chan {
        Chan {
            enabled: false,
            stop: false,
            s: Script::default(),
            delay: 0,
            layers: [None, None, None, None],
            font: 0,
            inst_or_wave: 0,
            instrument: None,
            has_instrument: false,
            large_notes: false,
            volume: 1.0,
            vol_scale: 1.0,
            pan: 0x40,
            transposition: 0,
            freq_scale: 1.0,
            adsr: Adsr { env: Env::Default, decay: 0xF0 },
            io: [-1; 8],
        }
    }
}

#[derive(Clone)]
pub struct Note {
    pub start: f64,
    pub end: Option<f64>,
    pub tuned: Tuned,
    /// Envelope points and the release rate index.
    pub env: Vec<(i64, i64)>,
    pub release: u8,
    pub gain: f64,
    /// Playback ratio against the sample's recorded rate (pitch x tuning x bend).
    pub freq: f64,
    pub channel: usize,
}

pub struct Song {
    pub notes: Vec<Note>,
    /// Seconds of music simulated.
    pub length: f64,
    /// `(start, end)` seconds of the loop when the sequence repeats.
    pub looping: Option<(f64, f64)>,
    pub runseq: Option<u8>,
    pub finished: bool,
}

struct Machine<'a> {
    seq: &'a [u8],
    fonts: &'a [Font],
    banks: &'a Banks<'a>,
    default_font: u8,
    font_ids: &'a [u8],
    // player
    s: Script,
    delay: i32,
    tempo: i32,
    tempo_change: i32,
    transposition: i32,
    vel_table: Vec<u8>,
    gate_table: Vec<u8>,
    fade_volume: f64,
    io: [i8; 8],
    chans: Vec<Chan>,
    notes: Vec<Note>,
    now: f64,
    stopped: bool,
    runseq: Option<u8>,
    inst_cache: HashMap<(u8, usize), Option<Instrument>>,
    drum_cache: HashMap<(u8, usize), Option<Rc<Drum>>>,
    /// First time each (script id, pc) was fetched, and the backward jumps seen: `(script, tick time of the jump, target time)`.
    visited: HashMap<(usize, usize), f64>,
    loops: Vec<(usize, f64, f64)>,
}

const PLAYER: usize = 16;

impl<'a> Machine<'a> {
    fn pitch(semitone: u8) -> f64 {
        2f64.powf((semitone as f64 - 39.0) / 12.0)
    }

    fn instrument(&mut self, font: u8, i: usize) -> Option<Instrument> {
        let key = (font, i);
        if !self.inst_cache.contains_key(&key) {
            let v = self.fonts.get(font as usize).and_then(|f| f.instrument(self.banks, i).ok().flatten());
            self.inst_cache.insert(key, v);
        }
        self.inst_cache[&key].clone()
    }

    fn drum(&mut self, font: u8, i: usize) -> Option<Rc<Drum>> {
        let key = (font, i);
        if !self.drum_cache.contains_key(&key) {
            let v = self.fonts.get(font as usize).and_then(|f| f.drum(self.banks, i).ok().flatten()).map(Rc::new);
            self.drum_cache.insert(key, v);
        }
        self.drum_cache[&key].clone()
    }

    fn env_points(&self, env: &Env) -> Vec<(i64, i64)> {
        match env {
            Env::Default => vec![(1, 32000), (1000, 32000), (-1, 0), (0, 0)],
            Env::Seq(at) => read_env(self.seq, *at),
            Env::Font(f, at) => self.fonts.get(*f as usize).map(|f| f.env_points(*at)).unwrap_or_default(),
        }
    }

    fn decay_layer(&mut self, layer: &mut Layer) {
        if let Some(i) = layer.note.take() {
            if self.notes[i].end.is_none() {
                self.notes[i].end = Some(self.now);
            }
        }
    }

    // ---- layer ----------------------------------------------------------------------------------------------

    fn layer_step(&mut self, ci: usize, li: usize) {
        let Some(mut l) = self.chans[ci].layers[li].take() else { return };
        self.layer_run(ci, &mut l);
        // the script of this layer may have been replaced meanwhile (never, layers are only swapped by the channel script)
        self.chans[ci].layers[li] = Some(l);
    }

    fn layer_run(&mut self, ci: usize, l: &mut Layer) {
        if !l.enabled {
            return;
        }
        if l.delay > 1 {
            l.delay -= 1;
            if !l.muted && l.delay <= l.gate_delay {
                self.decay_layer(l);
                l.muted = true;
            }
            return;
        }
        // step 1
        if !l.continuous {
            self.decay_layer(l);
        }
        // step 2: layer commands up to the next note / rest
        let seq = self.seq;
        let mut cmd;
        loop {
            self.visit(ci * 4 + 100, l.s.pc);
            cmd = rd8(seq, &mut l.s);
            if cmd <= 0xC0 {
                break;
            }
            if cmd >= 0xF2 {
                let (n, w) = cmd_args(cmd);
                let arg = if n == 1 {
                    if w[0] {
                        rd16(seq, &mut l.s)
                    } else {
                        rd8(seq, &mut l.s) as i32
                    }
                } else {
                    0
                };
                let mut j = None;
                if flow(seq, &mut l.s, cmd, arg, &mut j) == 0 {
                    continue;
                }
                self.decay_layer(l);
                l.enabled = false;
                l.finished = true;
                return;
            }
            match cmd {
                0xC1 => l.vel_sq = (rd8(seq, &mut l.s) as f64).powi(2) / 127f64.powi(2),
                0xC2 => l.transposition = rd8(seq, &mut l.s),
                0xC3 => l.short_delay = rd_len(seq, &mut l.s),
                0xC4 | 0xC5 => {
                    l.continuous = cmd == 0xC4;
                    self.decay_layer(l);
                }
                0xC6 => {
                    let c = rd8(seq, &mut l.s);
                    if c >= 0x7E {
                        l.inst_or_wave = if c == 0x7E { 1 } else if c == 0x7F { 0 } else { c };
                        l.instrument = None;
                        if c == 0xFF {
                            l.adsr = None;
                        }
                    } else {
                        let font = self.chans[ci].font;
                        match self.instrument(font, c as usize) {
                            Some(inst) => {
                                l.inst_or_wave = c + 2;
                                l.instrument = Some(c as usize);
                                l.adsr = Some(Adsr { env: inst.env.map(|e| Env::Font(font, e)).unwrap_or(Env::Default), decay: inst.release });
                            }
                            None => {
                                l.inst_or_wave = 0xFF;
                                l.instrument = None;
                            }
                        }
                    }
                }
                0xC7 => {
                    // portamento: mode, target note, then a time (1 byte for the special modes, else compressed)
                    let mode = rd8(seq, &mut l.s);
                    rd8(seq, &mut l.s);
                    if mode & 0x80 != 0 {
                        rd8(seq, &mut l.s);
                    } else {
                        rd_len(seq, &mut l.s);
                    }
                }
                0xC8 => {}
                0xC9 => l.gate_time = rd8(seq, &mut l.s),
                0xCA => l.pan = rd8(seq, &mut l.s),
                0xCB => {
                    let at = rd16(seq, &mut l.s) as u16 as usize;
                    let d = rd8(seq, &mut l.s);
                    l.adsr = Some(Adsr { env: Env::Seq(at), decay: d });
                }
                0xCF => {
                    let d = rd8(seq, &mut l.s);
                    let a = l.adsr.clone().unwrap_or_else(|| self.chans[ci].adsr.clone());
                    l.adsr = Some(Adsr { env: a.env, decay: d });
                }
                0xCC => l.ignore_drum_pan = true,
                0xCD => {
                    rd8(seq, &mut l.s);
                }
                0xCE => {
                    let b = rd8(seq, &mut l.s);
                    l.bend = 2f64.powf((b as i8 as f64) / 128.0 * 2.0 / 12.0);
                }
                _ => match cmd & 0xF0 {
                    0xD0 => l.vel_sq = (self.vel_table.get((cmd & 15) as usize).copied().unwrap_or(127) as f64).powi(2) / 127f64.powi(2),
                    0xE0 => l.gate_time = self.gate_table.get((cmd & 15) as usize).copied().unwrap_or(0),
                    _ => {}
                },
            }
        }
        // step 3
        if cmd == 0xC0 {
            l.delay = rd_len(seq, &mut l.s) as i32;
            l.muted = true;
            return;
        }
        l.muted = false;
        let large = self.chans[ci].large_notes;
        let mut cmd = cmd;
        let delay: u16;
        if large {
            let mut velocity = 0u8;
            match cmd & 0xC0 {
                0x00 => {
                    delay = rd_len(seq, &mut l.s);
                    velocity = rd8(seq, &mut l.s);
                    l.gate_time = rd8(seq, &mut l.s);
                    l.last_delay = delay;
                }
                0x40 => {
                    delay = rd_len(seq, &mut l.s);
                    velocity = rd8(seq, &mut l.s);
                    l.gate_time = 0;
                    l.last_delay = delay;
                }
                _ => {
                    delay = l.last_delay;
                    velocity = rd8(seq, &mut l.s);
                    l.gate_time = rd8(seq, &mut l.s);
                }
            }
            let v = velocity.min(127) as f64;
            l.vel_sq = v * v / (127.0 * 127.0);
            cmd -= cmd & 0xC0;
        } else {
            match cmd & 0xC0 {
                0x00 => {
                    delay = rd_len(seq, &mut l.s);
                    l.last_delay = delay;
                }
                0x40 => delay = l.short_delay,
                _ => delay = l.last_delay,
            }
            cmd -= cmd & 0xC0;
        }
        l.delay = delay as i32;
        l.gate_delay = ((l.gate_time as i32) * delay as i32) >> 8;
        // step 4: which sample plays
        let mut inst_or_wave = l.inst_or_wave;
        if inst_or_wave == 0xFF {
            if !self.chans[ci].has_instrument {
                return;
            }
            inst_or_wave = self.chans[ci].inst_or_wave;
        }
        let font = self.chans[ci].font;
        let chan_trans = self.chans[ci].transposition as u8;
        let mut semitone = cmd;
        let (tuned, freq, adsr): (Tuned, f64, Adsr);
        match inst_or_wave {
            0 => {
                semitone = semitone.wrapping_add(chan_trans).wrapping_add(l.transposition);
                let Some(drum) = self.drum(font, semitone as usize) else {
                    l.muted = true;
                    return;
                };
                let Some(t) = drum.tuned.clone() else {
                    l.muted = true;
                    return;
                };
                if !l.ignore_drum_pan {
                    l.pan = drum.pan;
                }
                freq = t.tuning;
                adsr = Adsr { env: Env::Font(font, drum.env), decay: drum.release };
                tuned = t;
            }
            1 => {
                let id = ((l.transposition as usize) << 6) + semitone as usize;
                let Some(t) = self.fonts.get(font as usize).and_then(|f| f.sfx(self.banks, id).ok().flatten()) else {
                    l.muted = true;
                    return;
                };
                freq = t.tuning;
                adsr = self.chans[ci].adsr.clone();
                tuned = t;
            }
            _ => {
                semitone = semitone.wrapping_add(self.transposition as u8).wrapping_add(chan_trans).wrapping_add(l.transposition);
                if semitone >= 0x80 {
                    l.muted = true;
                    return;
                }
                let idx = if l.inst_or_wave == 0xFF { self.chans[ci].instrument } else { l.instrument };
                let Some(inst) = idx.and_then(|i| self.instrument(font, i)) else {
                    l.muted = true;
                    return;
                };
                let Some(t) = inst.tuned(semitone).cloned() else {
                    l.muted = true;
                    return;
                };
                freq = Self::pitch(semitone) * t.tuning;
                adsr = l.adsr.clone().filter(|a| a.decay != 0).unwrap_or_else(|| self.chans[ci].adsr.clone());
                tuned = t;
            }
        }
        let mut freq = freq * l.bend.max(0.01);
        let mut ticks = l.delay;
        if l.delay == 0 {
            // a note without a length plays its sample once
            let secs = tuned.sample.pcm.len() as f64 / (freq * 32000.0);
            ticks = ((secs * (self.tempo + self.tempo_change) as f64 / 60.0) as i32 + 1).min(0x7FFE);
            l.delay = ticks;
            l.gate_delay = 0;
        }
        let _ = ticks;
        let ch = &self.chans[ci];
        freq *= ch.freq_scale.max(0.01);
        let gain = l.vel_sq * ch.volume * ch.vol_scale * self.fade_volume;
        let release = adsr.decay;
        let env = self.env_points(&adsr.env);
        // step 5: a new note replaces the previous one of this layer
        self.decay_layer(l);
        self.notes.push(Note { start: self.now, end: None, tuned, env, release, gain, freq, channel: ci });
        l.note = Some(self.notes.len() - 1);
        if l.muted {
            self.decay_layer(l);
        }
    }

    fn visit(&mut self, script: usize, pc: usize) {
        self.visited.entry((script, pc)).or_insert(self.now);
    }

    // ---- channel --------------------------------------------------------------------------------------------

    fn channel_step(&mut self, ci: usize) {
        if !self.chans[ci].enabled {
            return;
        }
        if !self.chans[ci].stop {
            if self.chans[ci].delay >= 2 {
                self.chans[ci].delay -= 1;
            } else {
                self.channel_script(ci);
            }
        }
        for li in 0..4 {
            self.layer_step(ci, li);
        }
    }

    fn set_layer(&mut self, ci: usize, li: usize, pc: usize) {
        if let Some(mut old) = self.chans[ci].layers[li].take() {
            self.decay_layer(&mut old);
        }
        let ch = &self.chans[ci];
        let l = Layer { enabled: true, gate_time: 0x80, pan: 0x40, bend: 1.0, inst_or_wave: 0xFF, s: Script { pc, ..Default::default() }, adsr: None, vel_sq: 0.0, ..Default::default() };
        let _ = ch;
        self.chans[ci].layers[li] = Some(l);
    }

    fn free_layer(&mut self, ci: usize, li: usize) {
        if let Some(mut l) = self.chans[ci].layers[li].take() {
            self.decay_layer(&mut l);
        }
    }

    fn enable_channel(&mut self, i: usize, pc: usize) {
        for li in 0..4 {
            self.free_layer(i, li);
        }
        let c = &mut self.chans[i];
        c.enabled = true;
        c.s = Script { pc, ..Default::default() };
        c.delay = 0;
    }

    fn disable_channel(&mut self, i: usize) {
        for li in 0..4 {
            self.free_layer(i, li);
        }
        self.chans[i].enabled = false;
    }

    fn set_font(&mut self, ci: usize, arg: u8) {
        let list = self.font_ids;
        if !list.is_empty() {
            let n = list.len();
            let idx = n.saturating_sub(arg as usize + 1).min(n - 1);
            // table[index + numFonts - cmd] with index at the count byte: argument 0 is the last font
            self.chans[ci].font = list[idx.min(n - 1)];
        }
    }

    fn set_instrument(&mut self, ci: usize, id: u8) {
        if id >= 0x80 {
            self.chans[ci].inst_or_wave = id;
            self.chans[ci].instrument = None;
            self.chans[ci].has_instrument = true;
        } else if id == 0x7F {
            self.chans[ci].inst_or_wave = 0;
            self.chans[ci].has_instrument = true;
        } else if id == 0x7E {
            self.chans[ci].inst_or_wave = 1;
            self.chans[ci].has_instrument = true;
        } else {
            let font = self.chans[ci].font;
            match self.instrument(font, id as usize) {
                Some(inst) => {
                    let c = &mut self.chans[ci];
                    c.inst_or_wave = id + 2;
                    c.instrument = Some(id as usize);
                    c.adsr = Adsr { env: inst.env.map(|e| Env::Font(font, e)).unwrap_or(Env::Default), decay: inst.release };
                    c.has_instrument = true;
                }
                None => self.chans[ci].has_instrument = false,
            }
        }
    }

    fn channel_script(&mut self, ci: usize) {
        let seq = self.seq;
        loop {
            let mut s = std::mem::take(&mut self.chans[ci].s);
            self.visit(ci, s.pc);
            let cmd = rd8(seq, &mut s);
            let mut args = [0i32; 3];
            if cmd >= 0xB0 {
                let (n, w) = cmd_args(cmd);
                for i in 0..n {
                    args[i] = if w[i] { rd16(seq, &mut s) } else { rd8(seq, &mut s) as i32 };
                }
                if cmd >= 0xF2 {
                    let mut j = None;
                    let d = flow(seq, &mut s, cmd, args[0], &mut j);
                    if let Some((_, target)) = j {
                        let at = *self.visited.get(&(ci, target)).unwrap_or(&0.0);
                        self.loops.push((ci, self.now, at));
                    }
                    self.chans[ci].s = s;
                    if d != 0 {
                        if d == END {
                            self.disable_channel(ci);
                        } else {
                            self.chans[ci].delay = d;
                        }
                        return;
                    }
                    continue;
                }
                let a0 = args[0];
                match cmd {
                    0xEA => {
                        self.chans[ci].s = s;
                        self.chans[ci].stop = true;
                        return;
                    }
                    0xEB => {
                        self.set_font(ci, a0 as u8);
                        self.set_instrument(ci, args[1] as u8);
                    }
                    0xC1 => self.set_instrument(ci, a0 as u8),
                    0xC3 => self.chans[ci].large_notes = false,
                    0xC4 => self.chans[ci].large_notes = true,
                    0xDF => {
                        self.chans[ci].volume = (a0 as u8) as f64 / 127.0;
                    }
                    0xE0 => self.chans[ci].vol_scale = (a0 as u8) as f64 / 128.0,
                    0xDE => self.chans[ci].freq_scale = (a0 as u16) as f64 / 32768.0,
                    0xD3 | 0xEE => {
                        let b = (a0 as u8).wrapping_add(0x80) as f64 - 128.0;
                        let range = if cmd == 0xD3 { 12.0 } else { 2.0 };
                        self.chans[ci].freq_scale = 2f64.powf(b / 128.0 * range / 12.0);
                    }
                    0xDD => self.chans[ci].pan = a0 as u8,
                    0xDB => self.chans[ci].transposition = a0 as i8,
                    0xDA => self.chans[ci].adsr.env = Env::Seq(a0 as u16 as usize),
                    0xD9 => self.chans[ci].adsr.decay = a0 as u8,
                    0xC6 => self.set_font(ci, a0 as u8),
                    0xC7 => {
                        let at = args[1] as u16 as usize;
                        let v = (s.value as u8).wrapping_add(a0 as u8);
                        self.write_seq(at, &[v]);
                    }
                    0xC8 => s.value -= a0 as i8 as i32,
                    0xCC => s.value = a0 as i8 as i32,
                    0xC9 => s.value &= a0 as i8 as i32,
                    0xCD => {
                        let t = a0 as usize & 15;
                        self.disable_channel(t);
                    }
                    0xCB => s.value = self.seq.get((a0 as u16 as usize).wrapping_add(s.value as usize)).copied().unwrap_or(0) as i32,
                    0xE8 => {
                        // muteBhv, noteAlloc, priority, then transposition, pan, panWeight, reverb, reverbIdx
                        self.chans[ci].transposition = rd8(seq, &mut s) as i8;
                        self.chans[ci].pan = rd8(seq, &mut s);
                        for _ in 0..3 {
                            rd8(seq, &mut s);
                        }
                    }
                    0xE7 => {
                        let at = a0 as u16 as usize;
                        if at + 8 <= seq.len() {
                            self.chans[ci].transposition = seq[at + 3] as i8;
                            self.chans[ci].pan = seq[at + 4];
                        }
                    }
                    0xBD => {}
                    _ => {}
                }
                self.chans[ci].s = s;
                continue;
            }
            if cmd >= 0x70 {
                let mut low = (cmd & 7) as usize;
                if (cmd & 0xF8) != 0x70 && low >= 4 {
                    low = 0;
                }
                match cmd & 0xF8 {
                    0x80 => {
                        s.value = match &self.chans[ci].layers[low] {
                            Some(l) => l.finished as i32,
                            None => -1,
                        }
                    }
                    0x88 => {
                        let at = rd16(seq, &mut s) as u16 as usize;
                        self.set_layer(ci, low, at);
                    }
                    0x90 => self.free_layer(ci, low),
                    0x98 => {}
                    0x70 => self.chans[ci].io[low] = s.value as i8,
                    0x78 => {
                        let off = rd16(seq, &mut s);
                        let at = (s.pc as i64 + off as i64) as usize;
                        self.set_layer(ci, low, at);
                    }
                    _ => {}
                }
                self.chans[ci].s = s;
                continue;
            }
            let low = (cmd & 15) as usize;
            match cmd & 0xF0 {
                0x00 => {
                    self.chans[ci].s = s;
                    self.chans[ci].delay = low as i32;
                    return;
                }
                0x10 => {}
                0x40 => {
                    let c = rd8(seq, &mut s) as usize & 7;
                    s.value = self.chans[low].io[c] as i32;
                }
                0x50 => s.value -= self.chans[ci].io[low & 7] as i32,
                0x60 => {
                    s.value = self.chans[ci].io[low & 7] as i32;
                    if low < 2 {
                        self.chans[ci].io[low] = -1;
                    }
                }
                0x20 => {
                    let at = rd16(seq, &mut s) as u16 as usize;
                    self.chans[ci].s = s;
                    self.enable_channel(low, at);
                    s = std::mem::take(&mut self.chans[ci].s);
                }
                0x30 => {
                    let c = rd8(seq, &mut s) as usize & 7;
                    self.chans[low].io[c] = s.value as i8;
                }
                _ => {}
            }
            self.chans[ci].s = s;
        }
    }

    fn write_seq(&mut self, _at: usize, _v: &[u8]) {
        // Sequence data patches (stseq) are not applied: the data is shared read-only.
    }

    // ---- sequence player ------------------------------------------------------------------------------------

    fn player_step(&mut self) {
        let seq = self.seq;
        if self.delay > 1 {
            self.delay -= 1;
        } else {
            loop {
                let mut s = std::mem::take(&mut self.s);
                self.visit(PLAYER, s.pc);
                let cmd = rd8(seq, &mut s);
                if cmd >= 0xF2 {
                    let (n, w) = cmd_args(cmd);
                    let arg = if n == 1 {
                        if w[0] {
                            rd16(seq, &mut s)
                        } else {
                            rd8(seq, &mut s) as i32
                        }
                    } else {
                        0
                    };
                    let mut j = None;
                    let d = flow(seq, &mut s, cmd, arg, &mut j);
                    if let Some((_, target)) = j {
                        let at = *self.visited.get(&(PLAYER, target)).unwrap_or(&0.0);
                        self.loops.push((PLAYER, self.now, at));
                    }
                    self.s = s;
                    if d != 0 {
                        if d == END {
                            self.stopped = true;
                        } else {
                            self.delay = d;
                        }
                        break;
                    }
                    continue;
                }
                if cmd >= 0xC0 {
                    match cmd {
                        0xF1 => {
                            rd8(seq, &mut s);
                        }
                        0xDF => {
                            self.transposition = 0;
                            self.transposition += rd8(seq, &mut s) as i8 as i32;
                        }
                        0xDE => self.transposition += rd8(seq, &mut s) as i8 as i32,
                        0xDD => self.tempo = (rd8(seq, &mut s) as i32 * 48).clamp(1, 48 * 255),
                        0xDC => self.tempo_change = rd8(seq, &mut s) as i8 as i32 * 48,
                        0xDA => {
                            rd8(seq, &mut s);
                            rd16(seq, &mut s);
                        }
                        0xDB => self.fade_volume = rd8(seq, &mut s) as f64 / 127.0,
                        0xD9 => {
                            rd8(seq, &mut s);
                        }
                        0xD7 => {
                            let bits = rd16(seq, &mut s) as u16;
                            for i in 0..16 {
                                if bits & (1 << i) != 0 {
                                    self.chans[i].font = self.default_font;
                                }
                            }
                        }
                        0xD6 => {
                            rd16(seq, &mut s);
                        }
                        0xD5 | 0xD3 | 0xD0 => {
                            rd8(seq, &mut s);
                        }
                        0xD4 => {}
                        0xD1 | 0xD2 => {
                            let at = rd16(seq, &mut s) as u16 as usize;
                            if let Some(t) = seq.get(at..at + 16) {
                                if cmd == 0xD2 {
                                    self.vel_table = t.to_vec();
                                } else {
                                    self.gate_table = t.to_vec();
                                }
                            }
                        }
                        0xCE => {
                            rd8(seq, &mut s);
                        }
                        0xCD => {
                            rd16(seq, &mut s);
                        }
                        0xCC => s.value = rd8(seq, &mut s) as i32,
                        0xC9 => s.value &= rd8(seq, &mut s) as i32,
                        0xC8 => s.value -= rd8(seq, &mut s) as i32,
                        0xC7 => {
                            rd8(seq, &mut s);
                            rd16(seq, &mut s);
                        }
                        0xC6 => {
                            self.s = s;
                            self.stopped = true;
                            return;
                        }
                        0xC5 => {
                            rd16(seq, &mut s);
                        }
                        0xC4 => {
                            let target = rd8(seq, &mut s);
                            let id = rd8(seq, &mut s);
                            self.s = s;
                            if target == 0xFF || target == 0 {
                                self.runseq = Some(id);
                                self.stopped = true;
                                return;
                            }
                            continue;
                        }
                        0xEF => {
                            rd16(seq, &mut s);
                            rd8(seq, &mut s);
                        }
                        _ => {}
                    }
                    self.s = s;
                    continue;
                }
                let low = (cmd & 15) as usize;
                match cmd & 0xF0 {
                    0x00 => s.value = if self.chans[low].enabled { 0 } else { 1 },
                    0x40 => self.disable_channel(low),
                    0x50 => s.value -= self.io[low & 7] as i32,
                    0x60 => {
                        rd8(seq, &mut s);
                        rd8(seq, &mut s);
                    }
                    0x70 => self.io[low & 7] = s.value as i8,
                    0x80 => {
                        s.value = self.io[low & 7] as i32;
                        if low < 2 {
                            self.io[low] = -1;
                        }
                    }
                    0x90 => {
                        let at = rd16(seq, &mut s) as u16 as usize;
                        self.s = s;
                        self.enable_channel(low, at);
                        s = std::mem::take(&mut self.s);
                    }
                    0xA0 => {
                        let off = rd16(seq, &mut s);
                        let at = (s.pc as i64 + off as i64) as usize;
                        self.s = s;
                        self.enable_channel(low, at);
                        s = std::mem::take(&mut self.s);
                    }
                    0xB0 => {
                        rd8(seq, &mut s);
                        rd16(seq, &mut s);
                    }
                    _ => {}
                }
                self.s = s;
            }
        }
        if self.stopped {
            return;
        }
        for ci in 0..16 {
            self.channel_step(ci);
        }
    }
}

/// Run sequence `seq` (the bytes of its Audioseq entry) with `fonts`; `font_ids` is its row of the sequence font table.
pub fn run_sequence(seq: &[u8], font_ids: &[u8], fonts: &[Font], banks: &Banks, max_seconds: f64) -> Song {
    let default_font = font_ids.last().copied().unwrap_or(0);
    let mut m = Machine {
        seq,
        fonts,
        banks,
        default_font,
        font_ids,
        s: Script::default(),
        delay: 0,
        tempo: 120 * 48,
        tempo_change: 0,
        transposition: 0,
        vel_table: vec![12, 25, 38, 51, 57, 64, 71, 76, 83, 89, 96, 102, 109, 115, 121, 127],
        gate_table: vec![229, 203, 177, 151, 139, 126, 113, 100, 87, 74, 61, 48, 36, 23, 10, 0],
        fade_volume: 1.0,
        io: [-1; 8],
        chans: (0..16).map(|_| Chan::new()).collect(),
        notes: Vec::new(),
        now: 0.0,
        stopped: false,
        runseq: None,
        inst_cache: HashMap::new(),
        drum_cache: HashMap::new(),
        visited: HashMap::new(),
        loops: Vec::new(),
    };
    for c in m.chans.iter_mut() {
        c.font = default_font;
    }
    let mut ticks = 0u64;
    let mut stop_at: Option<f64> = None;
    let mut looping = None;
    while m.now < max_seconds && !m.stopped && ticks < 4_000_000 {
        m.player_step();
        ticks += 1;
        let t = (m.tempo + m.tempo_change).max(1) as f64;
        m.now += 60.0 / t;
        // a loop is complete when the longest first backward jump has been taken
        if stop_at.is_none() && !m.loops.is_empty() {
            let first: HashMap<usize, (f64, f64)> = m.loops.iter().fold(HashMap::new(), |mut a, &(s, t, at)| {
                a.entry(s).or_insert((t, at));
                a
            });
            // wait until every running script has either looped or gone quiet for a while
            let (end, start) = first.values().fold((0.0f64, 0.0f64), |a, &(t, at)| if t > a.0 { (t, at) } else { a });
            let running = m.chans.iter().enumerate().filter(|(i, c)| c.enabled && !first.contains_key(i)).count();
            if running == 0 || m.now > end + 1.0 {
                stop_at = Some(end);
                looping = Some((start, end));
            }
        }
        if let Some(e) = stop_at {
            if m.now >= e {
                break;
            }
        }
    }
    let finished = m.stopped && looping.is_none();
    let length = looping.map(|l| l.1).unwrap_or(m.now);
    // notes still sounding at the end are released there
    let end = length;
    for n in m.notes.iter_mut() {
        if n.end.is_none() || n.end.unwrap() > end {
            n.end = Some(end.max(n.start));
        }
    }
    m.notes.retain(|n| n.start <= end);
    Song { notes: m.notes, length, looping, runseq: m.runseq, finished }
}

// ---------------------------------------------------------------------------------------------------------------
// Rendering

fn release_seconds(i: u8) -> f64 {
    0.02 + (((255 - i as i32) as f64) / 255.0).powf(1.5) * 1.5
}

/// Amplitude (0..1) envelope as piecewise-linear knots; `None` at the end of a one-shot envelope.
fn knots(points: &[(i64, i64)]) -> (Vec<(f64, f64)>, bool) {
    let mut k: Vec<(f64, f64)> = vec![(0.0, 0.0)];
    let (mut t, mut level, mut i, mut hops) = (0.0f64, 0.0f64, 0usize, 0);
    let mut hang = false;
    while i < points.len() && hops < 6 && t < 40.0 {
        let (delay, arg) = points[i];
        match delay {
            0 => {
                k.push((t, 0.0));
                return (k, false);
            }
            -1 => {
                hang = true;
                break;
            }
            -2 => {
                i = arg.max(0) as usize;
                hops += 1;
                continue;
            }
            -3 => {
                i = 0;
                hops += 1;
                continue;
            }
            d => {
                t += d as f64 * ENV_TICK;
                level = (arg as f64 / 32767.0).clamp(0.0, 1.0);
                k.push((t, level));
                i += 1;
            }
        }
    }
    let _ = level;
    (k, hang || i >= points.len())
}

fn level_at(k: &[(f64, f64)], hold: bool, x: f64) -> f64 {
    let last = k[k.len() - 1];
    if x >= last.0 {
        return if hold { last.1 } else { 0.0 };
    }
    for w in k.windows(2) {
        let ((t0, l0), (t1, l1)) = (w[0], w[1]);
        if x <= t1 {
            return l0 + (l1 - l0) * if t1 > t0 { (x - t0) / (t1 - t0) } else { 1.0 };
        }
    }
    0.0
}

/// Mix one note into `mix` (mono, `OUT_RATE`).
fn mix_note(n: &Note, mix: &mut Vec<f64>) {
    if n.gain < 1e-4 {
        return;
    }
    let (k, hold) = knots(&n.env);
    let end = n.end.unwrap_or(n.start) - n.start;
    let release = release_seconds(n.release);
    let g_off = level_at(&k, hold, end);
    let total = end + release;
    let step = n.freq * 32000.0 / OUT_RATE;
    let pcm = &n.tuned.sample.pcm;
    let lp = n.tuned.sample.lp;
    let base = (n.start * OUT_RATE) as usize;
    let count = ((total * OUT_RATE) as usize).min((60.0 * OUT_RATE) as usize);
    if mix.len() < base + count + 1 {
        mix.resize(base + count + 1, 0.0);
    }
    let mut pos = 0.0f64;
    for j in 0..count {
        let mut i = pos as usize;
        if let Some((l0, l1)) = lp {
            if i >= l1 {
                i = l0 + (i - l0) % (l1 - l0).max(1);
                pos = i as f64 + (pos - pos.floor());
            }
        }
        if i + 1 >= pcm.len() {
            break;
        }
        let t = j as f64 / OUT_RATE;
        let g = if t <= end { level_at(&k, hold, t) } else { g_off * (1.0 - (t - end) / release).max(0.0) };
        if g > 0.0 {
            let frac = pos - i as f64;
            mix[base + j] += (pcm[i] as f64 + (pcm[i + 1] - pcm[i]) as f64 * frac) * g * n.gain;
        }
        pos += step;
    }
}

/// Mix a song into mono PCM; the tail past a loop end wraps onto the loop start.
pub fn render(song: &Song) -> Vec<i32> {
    let mut mix: Vec<f64> = Vec::new();
    for n in &song.notes {
        mix_note(n, &mut mix);
    }
    let len = (song.length * OUT_RATE) as usize;
    if let Some((s, _)) = song.looping {
        let start = (s * OUT_RATE) as usize;
        if mix.len() > len {
            let tail: Vec<f64> = mix[len..].to_vec();
            for (k, v) in tail.iter().enumerate() {
                if start + k < len {
                    mix[start + k] += v;
                }
            }
        }
    }
    mix.resize(len.max(1), 0.0);
    mix.truncate(len.max(1));
    // loudness: scale so the 99.9th percentile reaches ~0.6, soft clip the rest
    let mut abs: Vec<f64> = mix.iter().map(|x| x.abs()).collect();
    abs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = abs.get((abs.len() as f64 * 0.999) as usize).copied().unwrap_or(1.0).max(1.0);
    let scale = 0.6 / p;
    mix.iter().map(|&x| round((x * scale).tanh() * 28000.0) as i32).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Export

pub struct Rom {
    pub audioseq: Vec<u8>,
    pub fonts: Vec<Font>,
    pub tables: Tables,
    table: Vec<u8>,
    bank_base: Vec<usize>,
    pub names: Vec<String>,
}

impl Rom {
    pub fn load(ctx: &Ctx) -> R<Rom> {
        let code = ctx.segment("code")?;
        let (seq_len, table_len, bank_len) = (ctx.dma_size(4)?, ctx.dma_size(5)?, ctx.dma_size(3)?);
        let audioseq = audio_seg(ctx, 4, seq_len)?;
        let table = audio_seg(ctx, 5, table_len)?;
        let bank = audio_seg(ctx, 3, bank_len)?;
        let tables = find_tables(&code, seq_len, table_len)?;
        let bank_base: Vec<usize> = tables.banks.iter().map(|b| b.rom_addr as usize).collect();
        let mut fonts = Vec::new();
        for e in &tables.fonts {
            let data = bank[e.rom_addr as usize..(e.rom_addr + e.size) as usize].to_vec();
            let (b1, b2) = ((e.d1 >> 8) as usize, (e.d1 & 0xFF) as usize);
            fonts.push(Font {
                data,
                bank1: bank_base.get(b1).copied().filter(|_| b1 != 0xFF),
                bank2: bank_base.get(b2).copied().filter(|_| b2 != 0xFF),
                n_inst: (e.d2 >> 8) as usize,
                n_drums: (e.d2 & 0xFF) as usize,
                n_sfx: e.d3 as usize,
                cache: RefCell::new(HashMap::new()),
            });
        }
        let text = ctx.read_decomp("include/tables/sequence_table.h")?;
        let names: Vec<String> = re(r"DEFINE_SEQUENCE(?:_PTR)?\s*\(\s*\w+,\s*(NA_BGM_\w+)").captures_iter(&text).map(|c| c[1].to_string()).collect();
        Ok(Rom { audioseq, fonts, tables, table, bank_base, names })
    }

    pub fn banks(&self) -> Banks<'_> {
        Banks { table: &self.table, bank_base: self.bank_base.clone() }
    }

    /// The bytes of sequence `id` (aliases resolved).
    pub fn sequence(&self, id: usize) -> Option<&[u8]> {
        let mut e = self.tables.seqs.get(id)?;
        if e.size == 0 {
            e = self.tables.seqs.get(e.rom_addr as usize)?;
        }
        self.audioseq.get(e.rom_addr as usize..(e.rom_addr + e.size) as usize)
    }

    pub fn font_ids(&self, id: usize) -> &[u8] {
        self.tables.seq_fonts.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn song(&self, id: usize, max_seconds: f64) -> Option<Song> {
        let seq = self.sequence(id)?;
        let banks = self.banks();
        Some(run_sequence(seq, self.font_ids(id), &self.fonts, &banks, max_seconds))
    }
}

/// Sequences worth shipping: the music scenes play, the Hyrule Field segments, the ocarina songs and the jingles
/// (not the sound-effect, ambience, staff roll and cutscene-only sequences, which would only add size).
pub fn wanted(id: usize, name: &str) -> bool {
    matches!(id, 4..=14 | 16..=19 | 24..=31 | 33..=34 | 36..=37 | 38..=48 | 50..=51 | 52..=62 | 63..=67 | 68..=73 | 76 | 78..=80 | 81..=85 | 88..=97 | 99..=101 | 107..=108) && name.starts_with("NA_BGM_") && id != 46
}

pub fn export(ctx: &Ctx, out: &Path) -> R<()> {
    let rom = Rom::load(ctx)?;
    let dir = out.join("oot_music");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let only: Option<Vec<usize>> = std::env::var("BYOR_OOT_MUSIC").ok().filter(|v| !v.is_empty()).map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect());
    let mut tracks = Map::new();
    let mut skipped = Map::new();
    for id in 0..rom.tables.seqs.len() {
        let name = rom.names.get(id).cloned().unwrap_or_default();
        if only.as_ref().is_some_and(|o| !o.contains(&id)) || (only.is_none() && !wanted(id, &name)) {
            continue;
        }
        let Some(song) = rom.song(id, MAX_SECONDS) else { continue };
        if song.notes.is_empty() {
            skipped.insert(id.to_string(), json!({"name": name, "why": song.runseq.map(|r| format!("runseq {r}")).unwrap_or_else(|| "no notes".to_string())}));
            continue;
        }
        let pcm = render(&song);
        let file = format!("{id}.wav");
        std::fs::write(dir.join(&file), wav_bytes(&pcm, OUT_RATE as u32)).map_err(|e| e.to_string())?;
        let l = song.looping;
        tracks.insert(
            id.to_string(),
            json!({
                "name": name, "file": file, "rate": OUT_RATE as u32, "frames": pcm.len(), "duration": (song.length * 100.0).round() / 100.0,
                "loopStart": l.map(|l| (l.0 * 1000.0).round() / 1000.0), "loopEnd": l.map(|l| (l.1 * 1000.0).round() / 1000.0),
                "notes": song.notes.len(), "runseq": song.runseq,
            }),
        );
    }
    let doc = json!({"tracks": tracks, "skipped": skipped});
    std::fs::write(dir.join("manifest.json"), dumps(&doc, Some(1)) + "\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_args_follow_the_decomp_table() {
        assert_eq!(cmd_args(0xFB), (1, [true, false, false]));
        assert_eq!(cmd_args(0xF8), (1, [false; 3]));
        assert_eq!(cmd_args(0xE8), (3, [false; 3]));
        assert_eq!(cmd_args(0xC7), (2, [false, true, false]));
        assert_eq!(cmd_args(0xFF), (0, [false; 3]));
    }

    #[test]
    fn compressed_lengths() {
        let mut s = Script::default();
        assert_eq!(rd_len(&[0x30], &mut s), 0x30);
        let mut s = Script::default();
        assert_eq!(rd_len(&[0x81, 0x10], &mut s), 0x110);
    }

    #[test]
    fn flow_loop_counts() {
        // loop 3 { delay1 } : F8 03 | FE | F7
        let seq = [0xF8u8, 3, 0xFE, 0xF7];
        let mut s = Script { pc: 2, ..Default::default() };
        let mut j = None;
        assert_eq!(flow(&seq, &mut s, 0xF8, 3, &mut j), 0);
        assert_eq!((s.depth, s.loops[0]), (1, 3));
        s.pc = 3;
        flow(&seq, &mut s, 0xF7, 0, &mut j);
        assert_eq!((s.pc, s.loops[0]), (2, 2));
    }
}
