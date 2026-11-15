//! Port of `scripts/export-ssb64-sounds.py`: the Super Smash Bros. (US) fighter voices and special-move sounds, rendered
//! from the user's ROM by running the game's FGM sound-effect interpreters offline (`fgm.ucd` voice scripts, `fgm.tbl`
//! articulations, `fgm.unk` LFOs over the `B1_sounds2` VADPCM samples; `src/libultra/n_audio/n_env.c` in the decomp,
//! see native/SSB64.md "Sounds"). Writes `<out>/<Name>_0.wav` (mono 16-bit, 32000 Hz) and `<out>/manifest-ssb64.json`,
//! byte-identical to the Python exporter.
use crate::oot::round_to;
use crate::oot::voices::{vadpcm, wav_bytes};
use crate::pyc::{dumps, read, R};
use crate::ssb64::libm::sin_;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

const CTL: (usize, usize) = (0xC6B650, 0xC7B1F0);
const TBL: (usize, usize) = (0xC7B1F0, 0xF573D0);
const FGM_UNK: (usize, usize) = (0xF573D0, 0xF57BF0);
const FGM_TBL: (usize, usize) = (0xF57BF0, 0xF5A9C0);
const FGM_UCD: (usize, usize) = (0xF5A9C0, 0xF5F4E0);
pub const RATE: u32 = 32000;
const MAX_FRAMES: usize = 210;
const FADE: usize = 320;
const MAX_RATIO: f64 = 1.99996;

pub const FIGHTER_NAMES: [&str; 12] = ["Mario", "Luigi", "Donkey", "Link", "Samus", "Yoshi", "Kirby", "Fox", "Pikachu", "Purin", "Captain", "Ness"];
/// Every fighter voice (`nSYAudioVoice<Fighter>*`) plus these effects (jumps, special moves, the KO star).
pub const EXPORT_FGM: [&str; 41] = [
    "FGMDeadUpStar", "FGMSamusJump1", "FGMSamusJumpAerial", "FGMKirbyPurinJump", "FGMInflateJump1", "FGMNessJump",
    "FGMMarioSpecialN", "FGMMarioSpecialHiJump", "FGMMarioSpecialHiCoin",
    "FGMDonkeyCharge", "FGMDonkeySpin", "FGMDonkeySlap1",
    "FGMLinkSpecialNShoot", "FGMLinkSpecialNReturn", "FGMLinkSpecialLwGet", "FGMLinkSpecialHi",
    "FGMSamusSpecialNCharge0", "FGMSamusSpecialNShootL", "FGMSamusSpecialLw", "FGMSamusSpecialHi", "FGMExplodeS",
    "FGMYoshiSpecialNTongue", "FGMYoshiEggLayShatter", "FGMYoshiSpecialHiThrow",
    "FGMKirbySpecialNStart", "FGMKirbySpecialNThrow", "FGMKirbySpecialLwStart",
    "FGMFoxBlaster", "FGMFoxSpecialHiStart", "FGMFoxSpecialHiFly", "FGMFoxSpecialLwStart", "FGMFoxSpecialLwHit",
    "FGMPikachuElectric1", "FGMPikachuSpecialHiStart", "FGMPikachuSpecialLwThunder",
    "FGMCaptainSpecialNStart", "FGMCaptainSpecialNPunch", "FGMCaptainSpecialHi",
    "FGMNessPSI", "FGMNessPKThunderLoop", "FGMNessSpecialLwStart",
];

// ---------------------------------------------------------------------------------------------- names

/// gmFGMVoiceID (src/gm/gmsound.h) names in enum order, US branch, without the `nSYAudio` prefix.
pub fn voice_names(decomp: &Path) -> R<Vec<String>> {
    let text = read(&decomp.join("src/gm/gmsound.h"))?;
    let start = text.find("typedef enum gmFGMVoiceID").ok_or("gmsound.h: no gmFGMVoiceID")?;
    let body = &text[start..];
    let body = &body[body.find('{').ok_or("gmsound.h: no enum body")? + 1..body.find("} gmFGMVoiceID").ok_or("gmsound.h: no enum end")?];
    let (mut names, mut keep) = (Vec::new(), vec![true]);
    for line in body.split('\n') {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.starts_with("#if") {
            let k = *keep.last().unwrap() && line == "#if defined(REGION_US)";
            keep.push(k);
            continue;
        }
        if line.starts_with("#else") {
            let n = keep.len();
            keep[n - 1] = keep[n - 2] && !keep[n - 1];
            continue;
        }
        if line.starts_with("#endif") {
            keep.pop();
            continue;
        }
        if !*keep.last().unwrap() {
            continue;
        }
        for part in line.split(',') {
            let part = part.split('=').next().unwrap_or("").trim();
            if let Some(rest) = part.strip_prefix("nSYAudio") {
                if !rest.is_empty() && rest.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    names.push(rest.to_string());
                }
            }
        }
    }
    Ok(names)
}

pub fn export_list(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = names.iter().filter(|n| n.strip_prefix("Voice").is_some_and(|r| FIGHTER_NAMES.iter().any(|f| r.starts_with(f)))).cloned().collect();
    out.extend(EXPORT_FGM.iter().map(|s| s.to_string()));
    out
}

// ---------------------------------------------------------------------------------------------- data

fn u32b(d: &[u8], o: usize) -> R<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap())).ok_or_else(|| format!("read past the end at {o:#x}"))
}

fn package(data: &[u8]) -> R<Vec<Vec<u8>>> {
    let count = u32b(data, 0)? as i32 as usize;
    let offs: Vec<usize> = (0..count).map(|i| u32b(data, 4 + 4 * i).map(|v| v as usize)).collect::<R<_>>()?;
    Ok((0..count)
        .map(|i| {
            let end = if i + 1 < count { offs[i + 1] } else { data.len() };
            data.get(offs[i]..end.min(data.len())).unwrap_or(&[]).to_vec()
        })
        .collect())
}

#[derive(Clone, Copy)]
struct Lfo {
    shape: u8,
    target: u8,
    post: u8,
    phase0: u8,
    period: f64,
    amp: f64,
    offset: f64,
}

fn lfos(data: &[u8]) -> R<Vec<Lfo>> {
    let count = u32b(data, 0)? as usize;
    (0..count)
        .map(|i| {
            let o = 4 + 16 * i;
            let f = |k: usize| u32b(data, o + 4 + 4 * k).map(|v| f32::from_bits(v) as f64);
            Ok(Lfo { shape: data[o], target: data[o + 1], post: data[o + 2], phase0: data[o + 3], period: f(0)?, amp: f(1)?, offset: f(2)? })
        })
        .collect()
}

type Sound = (Vec<i32>, Option<(usize, usize, u32)>);

struct Bank<'a> {
    ctl: &'a [u8],
    tbl: &'a [u8],
    sounds: Vec<usize>,
    cache: HashMap<usize, std::rc::Rc<Sound>>,
}

impl<'a> Bank<'a> {
    fn new(rom: &'a [u8]) -> R<Self> {
        let (ctl, tbl) = (&rom[CTL.0..CTL.1], &rom[TBL.0..TBL.1]);
        let bank = u32b(ctl, 4)? as usize;
        let inst = u32b(ctl, bank + 12)? as usize;
        let count = i16::from_be_bytes([ctl[inst + 14], ctl[inst + 15]]) as usize;
        let sounds = (0..count).map(|i| u32b(ctl, inst + 16 + 4 * i).map(|v| v as usize)).collect::<R<_>>()?;
        Ok(Bank { ctl, tbl, sounds, cache: HashMap::new() })
    }

    fn sound(&mut self, i: usize) -> R<std::rc::Rc<Sound>> {
        if let Some(s) = self.cache.get(&i) {
            return Ok(s.clone());
        }
        let ctl = self.ctl;
        let wave = u32b(ctl, self.sounds[i] + 8)? as usize;
        let (base, length, kind) = (u32b(ctl, wave)? as i32 as usize, u32b(ctl, wave + 4)? as i32 as usize, ctl[wave + 8]);
        let (loop_ptr, book) = (u32b(ctl, wave + 12)? as usize, u32b(ctl, wave + 16)? as usize);
        if kind != 0 {
            return Err(format!("sound {i}: not ADPCM"));
        }
        let (order, npred) = (u32b(ctl, book)? as usize, u32b(ctl, book + 4)? as usize);
        let pcm = vadpcm(&ctl[book..book + 8 + 16 * order * npred], self.tbl.get(base..base + length).ok_or("sample past the table")?)?;
        let mut lp = None;
        if loop_ptr != 0 {
            let (start, end, count) = (u32b(ctl, loop_ptr)? as usize, u32b(ctl, loop_ptr + 4)? as usize, u32b(ctl, loop_ptr + 8)?);
            if count != 0 && start < end && end <= pcm.len() {
                lp = Some((start, end, count));
            }
        }
        let s = std::rc::Rc::new((pcm, lp));
        self.cache.insert(i, s.clone());
        Ok(s)
    }
}

// ---------------------------------------------------------------------------------------------- the FGM engine, offline

/// alCents2Ratio (src/libultra/audio/cents2ratio.c), in doubles.
pub fn cents_ratio(cents: i64) -> f64 {
    let mut x = if cents >= 0 { 1.00057779 } else { 0.9994225441 };
    let mut c = cents.abs();
    let mut ratio = 1.0;
    while c != 0 {
        if c & 1 != 0 {
            ratio *= x;
        }
        x *= x;
        c >>= 1;
    }
    ratio
}

fn byte(code: &[u8], p: usize) -> R<u8> {
    code.get(p).copied().ok_or_else(|| "FGM program runs past its entry".to_string())
}

fn varint(code: &[u8], p: &mut usize) -> R<usize> {
    let mut v = byte(code, *p)? as usize;
    *p += 1;
    if v & 0x80 != 0 {
        v = ((v & 0x7F) << 8) + byte(code, *p)? as usize;
        *p += 1;
    }
    Ok(v)
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        ((self.0 >> 16) & 0xFFFF) as f64 / 65536.0
    }
}

struct Mod {
    id: u8,
    shape: u8,
    target: u8,
    post: u8,
    period: f64,
    amp: f64,
    offset: f64,
    phase: f64,
    t18: f64,
    prev: f64,
    cur: f64,
}

struct Voice {
    code: usize,
    pc: usize,
    loop_pc: usize,
    timer: u32,
    state: u8,
    vol: i64,
    pan: i64,
    fx: i64,
    pitch: i64,
    note: i64,
    chan_vol: i64,
    wave: Option<usize>,
    mods: Vec<Mod>,
    started: bool,
    pos: f64,
    ratio: f64,
    gain: f64,
    done: bool,
}

#[derive(Clone)]
struct Script {
    code: usize,
    pc: usize,
    loop_pc: usize,
    timer: u32,
    artic: usize,
    dur: [u32; 6],
    flag20: u8,
    flag21: u8,
    prio: u8,
    group: u8,
    vol: i64,
    voice: Option<usize>,
    ended: bool,
}

impl Script {
    fn new(code: usize) -> Self {
        Script { code, pc: 0, loop_pc: 0, timer: 1, artic: 0, dur: [0; 6], flag20: 0, flag21: 0, prio: 0, group: 0, vol: 255, voice: None, ended: false }
    }
    fn fork(&self, code: usize) -> Self {
        Script { code, pc: 0, loop_pc: 0, timer: 1, voice: None, ended: false, ..self.clone() }
    }
}

pub struct Engine<'a> {
    ucd: Vec<Vec<u8>>,
    tbl: Vec<Vec<u8>>,
    unk: Vec<Lfo>,
    bank: Bank<'a>,
    rng1: Rng,
    rng2: Rng,
    scripts: Vec<Script>,
    voices: Vec<Voice>,
}

impl<'a> Engine<'a> {
    pub fn new(rom: &'a [u8]) -> R<Self> {
        Ok(Engine {
            ucd: package(&rom[FGM_UCD.0..FGM_UCD.1])?,
            tbl: package(&rom[FGM_TBL.0..FGM_TBL.1])?,
            unk: lfos(&rom[FGM_UNK.0..FGM_UNK.1])?,
            bank: Bank::new(rom)?,
            rng1: Rng(1),
            rng2: Rng(1),
            scripts: vec![],
            voices: vec![],
        })
    }

    /// The sound `id` (a gmFGMVoiceID) as 32000 Hz samples, and whether it was cut at [`MAX_FRAMES`].
    pub fn render(&mut self, id: usize) -> R<(Vec<i32>, bool)> {
        self.rng1 = Rng(1);
        self.rng2 = Rng(1);
        self.scripts = vec![Script::new(id)];
        self.voices = vec![];
        let (mut out, mut frame, mut cut) = (Vec::<f64>::new(), 0usize, false);
        loop {
            for i in 0..self.scripts.len() {
                if self.scripts[i].voice.is_some_and(|v| self.voices[v].state == 0) {
                    self.scripts[i].voice = None;
                }
            }
            let mut i = 0;
            while i < self.scripts.len() {
                self.run_script(i)?;
                i += 1;
            }
            self.scripts.retain(|s| !s.ended);
            for v in 0..self.voices.len() {
                if self.voices[v].state != 0 {
                    self.run_voice(v)?;
                }
            }
            let (start, end) = (frame * RATE as usize / 60, (frame + 1) * RATE as usize / 60);
            let mut buf = vec![0.0f64; end - start];
            for v in 0..self.voices.len() {
                self.mix(v, &mut buf)?;
            }
            out.extend_from_slice(&buf);
            frame += 1;
            let live = self.voices.iter().any(|v| v.state != 0);
            if self.scripts.is_empty() && !live {
                break;
            }
            if frame >= MAX_FRAMES {
                cut = self.voices.iter().any(|v| v.state != 0 && v.started && !v.done && v.gain > 0.0);
                break;
            }
        }
        while out.last() == Some(&0.0) {
            out.pop();
        }
        if cut {
            let (n, len) = (FADE.min(out.len()), out.len());
            for k in 0..n {
                out[len - n + k] *= (n - k) as f64 / n as f64;
            }
        }
        Ok((out.iter().map(|x| ((x + 0.5).floor() as i64).clamp(-32768, 32767) as i32).collect(), cut))
    }

    fn release(&mut self, s: usize) {
        if let Some(v) = self.scripts[s].voice.take() {
            self.voices[v].timer = 0;
            self.voices[v].state = 2;
        }
    }

    // -- func_80026B90_27790
    fn run_script(&mut self, si: usize) -> R<()> {
        {
            let s = &mut self.scripts[si];
            if s.ended || s.timer == 0 {
                return Ok(());
            }
            s.timer -= 1;
            if s.timer != 0 {
                return Ok(());
            }
            if s.flag21 != 0 {
                s.flag21 = 0;
                self.release(si);
                self.scripts[si].timer = 1;
                return Ok(());
            }
        }
        let code = self.ucd[self.scripts[si].code].clone();
        let code = &code[..];
        let (mut p, mut t5) = (self.scripts[si].pc, 0i64);
        let mut steps = 0;
        let timer = loop {
            steps += 1;
            if steps > 4096 {
                return Err("fgm.ucd script does not yield".into());
            }
            let op = byte(code, p)?;
            p += 1;
            let mut timer = 0u32;
            if op & 0xF8 >= 0xD0 {
                match op {
                    0xD0 => {
                        self.release(si);
                        self.scripts[si].ended = true;
                        self.scripts[si].pc = p;
                        return Ok(());
                    }
                    0xD1 => self.scripts[si].artic = varint(code, &mut p)?,
                    0xD2 => {
                        self.scripts[si].flag20 = u8::from(byte(code, p)? & 0x80 != 0);
                        p += 1;
                    }
                    0xD3 => {
                        self.scripts[si].prio = byte(code, p)?;
                        p += 1;
                    }
                    0xD4 => {
                        for k in 0..6 {
                            self.scripts[si].dur[k] = varint(code, &mut p)? as u32;
                        }
                    }
                    0xD5 => {
                        self.scripts[si].vol = byte(code, p)? as i64;
                        p += 1;
                    }
                    0xD6 => {
                        let d = byte(code, p)? as i8 as i64;
                        let s = &mut self.scripts[si];
                        s.vol = (s.vol + d).clamp(0, 255);
                        p += 1;
                    }
                    0xD7 | 0xDC | 0xD8 | 0xDD => p += 1,
                    0xD9 => {
                        let idx = varint(code, &mut p)?;
                        if idx < self.ucd.len() {
                            let f = self.scripts[si].fork(idx);
                            self.scripts.push(f);
                        }
                    }
                    0xDA => self.scripts[si].loop_pc = p,
                    0xDB => p = self.scripts[si].loop_pc,
                    0xDE => {
                        let g = byte(code, p)?;
                        p += 1;
                        self.scripts[si].group = g;
                        if g != 0 {
                            for o in 0..self.scripts.len() {
                                if o == si || self.scripts[o].ended || self.scripts[o].group != g {
                                    continue;
                                }
                                if (self.scripts[si].prio & 0x7F) >= (self.scripts[o].prio & 0x7F) {
                                    self.scripts[o].ended = true;
                                    self.release(o);
                                } else {
                                    self.release(si);
                                    self.scripts[si].ended = true;
                                    self.scripts[si].pc = p;
                                    return Ok(());
                                }
                            }
                        }
                    }
                    0xDF => t5 = -2400,
                    0xE0 => t5 = -4800,
                    _ => return Err(format!("fgm.ucd opcode {op:#x}")),
                }
            } else {
                timer = (op & 7) as u32;
                if (1..=6).contains(&timer) {
                    timer = self.scripts[si].dur[timer as usize - 1];
                } else if timer == 7 {
                    timer = varint(code, &mut p)? as u32;
                }
                if op & 0xF8 == 0 {
                    self.release(si);
                } else {
                    let cents = (op >> 3) as i64 * 100 - 1300 + t5;
                    t5 = 0;
                    let artic = self.scripts[si].artic;
                    if let Some(v) = self.scripts[si].voice {
                        self.voices[v].note = cents;
                    } else if artic < self.tbl.len() {
                        let chan_vol = self.scripts[si].vol * 127 >> 7;
                        self.voices.push(Voice {
                            code: artic,
                            pc: 0,
                            loop_pc: 0,
                            timer: 1,
                            state: 3,
                            vol: 127,
                            pan: 64,
                            fx: 0,
                            pitch: 0,
                            note: cents,
                            chan_vol,
                            wave: None,
                            mods: vec![],
                            started: false,
                            pos: 0.0,
                            ratio: 1.0,
                            gain: 0.0,
                            done: false,
                        });
                        self.scripts[si].voice = Some(self.voices.len() - 1);
                    }
                    let s = &mut self.scripts[si];
                    if timer > 1 && s.flag20 != 0 {
                        timer -= 1;
                        s.flag21 = 1;
                    }
                }
            }
            if timer != 0 {
                break timer;
            }
        };
        let s = &mut self.scripts[si];
        s.pc = p;
        s.timer = timer;
        Ok(())
    }

    // -- func_80027460_28060
    fn run_voice(&mut self, vi: usize) -> R<()> {
        if self.voices[vi].timer != 0 {
            self.voices[vi].timer -= 1;
            if self.voices[vi].timer == 0 {
                let code = self.tbl[self.voices[vi].code].clone();
                let code = &code[..];
                let mut p = self.voices[vi].pc;
                let mut steps = 0;
                let timer = loop {
                    steps += 1;
                    if steps > 4096 {
                        return Err("fgm.tbl program does not yield".into());
                    }
                    let op = byte(code, p)?;
                    p += 1;
                    let mut timer = (op & 0xF) as u32;
                    if timer & 8 != 0 {
                        let b = byte(code, p)? as u32;
                        p += 1;
                        timer = ((timer & 7) << 7) + (b & 0x7F);
                        if b & 0x80 != 0 {
                            timer = (timer << 8) + byte(code, p)? as u32;
                            p += 1;
                        }
                    }
                    let kind = op & 0xF0;
                    let v = &mut self.voices[vi];
                    match kind {
                        0x00 | 0x10 | 0x30 => {
                            let a = byte(code, p)? as i64;
                            p += 1;
                            let cur = match kind {
                                0x00 => v.vol,
                                0x10 => v.pan,
                                _ => v.fx,
                            };
                            let val = if a <= 127 { a } else { (a - 192 + cur).clamp(0, 127) };
                            match kind {
                                0x00 => v.vol = val,
                                0x10 => v.pan = val,
                                _ => v.fx = val,
                            }
                        }
                        0x20 => {
                            let a = i16::from_be_bytes([byte(code, p)?, byte(code, p + 1)?]) as i64;
                            p += 2;
                            v.pitch = if a <= 1200 { a.max(-1200) } else { (a - 2400 + v.pitch).clamp(-1200, 1200) };
                        }
                        0x40 => {
                            let mid = byte(code, p)?;
                            p += 1;
                            let idx = varint(code, &mut p)?;
                            if idx < self.unk.len() {
                                let e = self.unk[idx];
                                self.spawn_mod(vi, mid, e);
                            }
                        }
                        0x50 => {
                            let mid = byte(code, p)?;
                            p += 1;
                            v.mods.retain(|m| m.id != mid);
                        }
                        0x60 => {
                            let idx = varint(code, &mut p)?;
                            if idx < self.bank.sounds.len() {
                                v.wave = Some(idx);
                                v.state = 3;
                            }
                        }
                        0x70 => {
                            v.state = 2;
                            timer = 10000;
                        }
                        0x80 => v.loop_pc = p,
                        0x90 => p = v.loop_pc,
                        _ => return Err(format!("fgm.tbl opcode {op:#x}")),
                    }
                    if timer != 0 {
                        break timer;
                    }
                };
                self.voices[vi].pc = p;
                self.voices[vi].timer = timer;
            }
        }
        self.run_mods(vi);
        let v = &mut self.voices[vi];
        match v.state {
            1 => {
                v.ratio = MAX_RATIO.min(cents_ratio(v.pitch + v.note));
                v.gain = gain(v);
            }
            2 => {
                v.gain = 0.0;
                v.state = 0;
                v.started = false;
            }
            3 => {
                if v.wave.is_none() {
                    v.state = 0;
                } else {
                    v.started = true;
                    v.done = false;
                    v.pos = 0.0;
                    v.ratio = MAX_RATIO.min(cents_ratio(v.pitch + v.note));
                    v.gain = gain(v);
                    v.state = 1;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn spawn_mod(&mut self, vi: usize, mid: u8, e: Lfo) {
        let v = &mut self.voices[vi];
        let k = match v.mods.iter().position(|m| m.id == mid) {
            Some(k) => k,
            None => {
                v.mods.push(Mod { id: mid, shape: 0, target: 0, post: 0, period: 0.0, amp: 0.0, offset: 0.0, phase: 0.0, t18: 0.0, prev: 0.0, cur: 0.0 });
                v.mods.sort_by_key(|m| m.id);
                v.mods.iter().position(|m| m.id == mid).unwrap()
            }
        };
        let m = &mut v.mods[k];
        (m.id, m.shape, m.target, m.post, m.period, m.amp, m.offset) = (mid, e.shape, e.target, e.post, e.period, e.amp, e.offset);
        m.phase = e.period * e.phase0 as f64 / 256.0;
        if e.shape == 4 || e.shape == 8 {
            let rng = if e.shape == 4 { &mut self.rng1 } else { &mut self.rng2 };
            m.cur = rng.next() * e.amp + e.offset;
            m.t18 = rng.next() * e.period;
            m.phase = m.t18 * e.phase0 as f64 / 256.0;
        } else if e.shape == 5 {
            m.prev = 0.0;
            m.cur = self.rng1.next() * e.amp + e.offset;
            m.t18 = self.rng1.next() * e.period + 0.5;
            m.phase = m.t18 * e.phase0 as f64 / 256.0;
        }
    }

    fn run_mods(&mut self, vi: usize) {
        let mut regs = [0.0f64; 10];
        let v = &mut self.voices[vi];
        for k in 0..v.mods.len() {
            let m = &mut v.mods[k];
            if m.shape < 4 {
                m.phase += 1.0;
                if m.period < m.phase {
                    m.phase -= m.period;
                }
            }
            let mut y = match m.shape {
                0 => {
                    let i = if m.period != 0.0 { ((m.phase / m.period * 4096.0) as i64) & 0xFFF } else { 0 };
                    let s = sin_(std::f64::consts::PI * (i & 0x7FF) as f64 / 2048.0);
                    (if i & 0x800 != 0 { -s } else { s }) * m.amp + m.offset
                }
                1 => {
                    if m.period / 2.0 < m.phase {
                        m.amp
                    } else {
                        m.offset
                    }
                }
                2 => {
                    if m.period != 0.0 {
                        m.amp * m.phase / m.period + m.offset
                    } else {
                        m.offset
                    }
                }
                3 => {
                    if m.period != 0.0 {
                        m.amp * (m.period - m.phase) / m.period + m.offset
                    } else {
                        m.offset
                    }
                }
                4 | 8 => {
                    m.phase += 1.0;
                    if m.t18 < m.phase {
                        let rng = if m.shape == 4 { &mut self.rng1 } else { &mut self.rng2 };
                        m.cur = rng.next() * m.amp + m.offset;
                        m.t18 = rng.next() * m.period;
                        m.phase = 0.0;
                    }
                    m.cur
                }
                5 => {
                    m.phase += 1.0;
                    if m.t18 < m.phase {
                        m.prev = m.cur;
                        m.cur = self.rng1.next() * m.amp + m.offset;
                        m.t18 = self.rng1.next() * m.period + 0.5;
                        m.phase = 0.0;
                    }
                    if m.t18 != 0.0 {
                        (m.cur - m.prev) * m.phase / m.t18 + m.prev
                    } else {
                        m.prev
                    }
                }
                6 | 7 => {
                    m.phase += 1.0;
                    if m.period < m.phase {
                        m.phase = m.period;
                    }
                    if m.period == 0.0 {
                        m.offset
                    } else if m.shape == 6 {
                        m.amp * m.phase / m.period + m.offset
                    } else {
                        m.amp * (m.period - m.phase) / m.period + m.offset
                    }
                }
                _ => 0.0,
            };
            let r = (m.post & 0xF) as usize;
            match m.post >> 4 {
                1 => y += if r < 10 { regs[r] } else { 0.0 },
                2 => y *= if r < 10 { regs[r] } else { 0.0 },
                _ => {}
            }
            let t = m.target;
            match t {
                10 | 11 => v.vol = (y + if t == 11 { v.vol as f64 } else { 0.0 }).clamp(0.0, 127.0) as i64,
                12 | 13 => v.pitch = (y + if t == 13 { v.pitch as f64 } else { 0.0 }).clamp(-1200.0, 1200.0) as i64,
                14 | 15 => v.pan = (y + if t == 15 { v.pan as f64 } else { 0.0 }).clamp(0.0, 127.0) as i64,
                0..=9 => regs[t as usize] = y,
                _ => {
                    let (dst, field) = if t < 24 {
                        (Some(k), t)
                    } else {
                        let want = (t - 24) / 8;
                        (v.mods.iter().position(|o| o.id == want), (t - 24) % 8 + 16)
                    };
                    let Some(d) = dst else { continue };
                    let o = &mut v.mods[d];
                    let slot = match field {
                        16 | 17 => &mut o.period,
                        18 | 19 => &mut o.amp,
                        20 | 21 => &mut o.offset,
                        22 | 23 => &mut o.phase,
                        _ => continue,
                    };
                    *slot = y + if field & 1 != 0 { *slot } else { 0.0 };
                }
            }
        }
    }

    fn mix(&mut self, vi: usize, buf: &mut [f64]) -> R<()> {
        if !self.voices[vi].started || self.voices[vi].done {
            return Ok(());
        }
        let sound = self.bank.sound(self.voices[vi].wave.unwrap())?;
        let (pcm, lp) = (&sound.0, sound.1);
        let n = pcm.len();
        let v = &mut self.voices[vi];
        for out in buf.iter_mut() {
            let i = v.pos as usize;
            if i >= n {
                v.done = true;
                return Ok(());
            }
            let a = pcm[i] as f64;
            let b = if i + 1 < n { pcm[i + 1] } else { 0 };
            *out += (a + (b - pcm[i]) as f64 * (v.pos - i as f64)) * v.gain;
            v.pos += v.ratio;
            if let Some((start, end, _)) = lp {
                if v.pos >= end as f64 {
                    v.pos -= (end - start) as f64;
                }
            }
        }
        Ok(())
    }
}

fn gain(v: &Voice) -> f64 {
    (v.vol * v.chan_vol * 127 >> 7) as f64 / 32768.0
}

// ---------------------------------------------------------------------------------------------- output

/// `export()` of export-ssb64-sounds.py: every sound of [`export_list`] into `out`. Returns the number written.
pub fn export(source: &Path, z64: &[u8], out: &Path) -> R<usize> {
    if crate::hash::hex(&crate::hash::sha1(z64)) != crate::ssb64::SHA1 {
        return Err("not Super Smash Bros. (US)".into());
    }
    let names = voice_names(&crate::ssb64::decomp_dir(source))?;
    let ids: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let mut engine = Engine::new(z64)?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut clips = Map::new();
    for name in export_list(&names) {
        let id = *ids.get(name.as_str()).ok_or_else(|| format!("{name}: not in gmFGMVoiceID"))?;
        let (pcm, cut) = engine.render(id).map_err(|e| format!("{name}: {e}"))?;
        if pcm.is_empty() {
            continue;
        }
        let path = out.join(format!("{name}_0.wav"));
        std::fs::write(&path, wav_bytes(&pcm, RATE)).map_err(|e| format!("{}: {e}", path.display()))?;
        let peak = pcm.iter().map(|x| x.abs()).max().unwrap_or(0);
        clips.insert(name, json!({"id": id, "frames": pcm.len(), "duration": round_to(pcm.len() as f64 / RATE as f64, 4), "peak": peak, "cut": cut}));
    }
    let n = clips.len();
    let doc = crate::ssb64::sort_keys(json!({"sampleRate": RATE, "clips": Value::Object(clips)}));
    std::fs::write(out.join("manifest-ssb64.json"), dumps(&doc, Some(1)) + "\n").map_err(|e| e.to_string())?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_ratio_matches_libultra() {
        assert_eq!(cents_ratio(0), 1.0);
        assert!((cents_ratio(1200) - 2.0).abs() < 1e-3);
        assert!((cents_ratio(-1200) - 0.5).abs() < 1e-3);
        assert!((cents_ratio(-1299) - 2f64.powf(-1299.0 / 1200.0)).abs() < 1e-3);
    }

    #[test]
    fn varint_and_package() {
        let (mut p, code) = (0, [0x05u8, 0x81, 0x02]);
        assert_eq!(varint(&code, &mut p).unwrap(), 5);
        assert_eq!(varint(&code, &mut p).unwrap(), 0x102);
        assert_eq!(p, 3);
        // count 2, offsets 12 and 14, payloads [1, 2] and [3]
        let data = [0, 0, 0, 2, 0, 0, 0, 12, 0, 0, 0, 14, 1, 2, 3];
        assert_eq!(package(&data).unwrap(), vec![vec![1, 2], vec![3]]);
    }

    #[test]
    fn enum_names_follow_the_us_branch() {
        let dir = std::env::temp_dir().join(format!("ssb64-sounds-enum-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/gm")).unwrap();
        std::fs::write(
            dir.join("src/gm/gmsound.h"),
            "typedef enum gmFGMVoiceID\n{\n\tnSYAudioFGMA, // a\n#if defined(REGION_US)\n\tnSYAudioFGMUs,\n#else\n\tnSYAudioFGMJp,\n#endif\n\tnSYAudioVoiceMarioJump,\n\tnSYAudioFGMVoiceEnd = 0x2B7,\n} gmFGMVoiceID;\n",
        )
        .unwrap();
        let names = voice_names(&dir).unwrap();
        assert_eq!(names, ["FGMA", "FGMUs", "VoiceMarioJump", "FGMVoiceEnd"]);
        let list = export_list(&names);
        assert_eq!(list[0], "VoiceMarioJump");
        assert_eq!(list.len(), 1 + EXPORT_FGM.len());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The export list here and in the Python exporter are the same.
    #[test]
    fn export_lists_agree_with_python() {
        let py = include_str!("../../../scripts/export-ssb64-sounds.py");
        let block = &py[py.find("EXPORT_FGM = [").unwrap()..];
        let block = &block[..block.find(']').unwrap()];
        let names: Vec<&str> = block.split('\'').skip(1).step_by(2).collect();
        assert_eq!(names, EXPORT_FGM);
        let fighters = &py[py.find("FIGHTER_NAMES = [").unwrap()..];
        let fighters: Vec<&str> = fighters[..fighters.find(']').unwrap()].split('\'').skip(1).step_by(2).collect();
        assert_eq!(fighters, FIGHTER_NAMES);
        for (k, v) in [("RATE", RATE as usize), ("MAX_FRAMES", MAX_FRAMES), ("FADE", FADE)] {
            assert!(py.contains(&format!("\n{k} = {v} ")), "{k}");
        }
    }
}
