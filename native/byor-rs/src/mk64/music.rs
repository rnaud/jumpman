//! Port of `scripts/export-mk64-music.py`: converts the ROM's sequence archive, instrument banks, sample table and bank-set
//! table to the little-endian layout a wasm32 build reads and patches them into the data-less engine template
//! (`assets/audio/music/mk64/mk64-audio.wasm` + `sequences.json`).
use crate::pyc::{dumps, R};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const SEQ_ROM: usize = 0xBC5F60;
const BANKS_ROM: usize = SEQ_ROM - 0x25FD00;
const TABLES_ROM: usize = BANKS_ROM + 0x13840;
const SEQ_COUNT: usize = 30;

fn be16(d: &[u8], o: usize) -> R<u32> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as u32).ok_or_else(|| format!("read outside the buffer at {o:#x}"))
}
fn be32(d: &[u8], o: usize) -> R<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("read outside the buffer at {o:#x}"))
}

/// Byte-swap a big-endian buffer in place, field by field, refusing conflicting passes.
struct Swapper {
    d: Vec<u8>,
    done: HashMap<usize, usize>,
}

impl Swapper {
    fn new(data: &[u8]) -> Self {
        Swapper { d: data.to_vec(), done: HashMap::new() }
    }
    fn u32(&mut self, off: usize) -> R<()> {
        self.swap(off, 4)
    }
    fn u16(&mut self, off: usize) -> R<()> {
        self.swap(off, 2)
    }
    fn swap(&mut self, off: usize, size: usize) -> R<()> {
        if off + size > self.d.len() {
            return Err(format!("field at {off:#x} is outside the buffer"));
        }
        if let Some(&s) = self.done.get(&off) {
            if s != size {
                return Err(format!("conflicting field sizes at {off:#x}"));
            }
            return Ok(());
        }
        self.done.insert(off, size);
        self.d[off..off + size].reverse();
        Ok(())
    }
}

/// Parse an ALSeqFile header (rev u16, count u16, {offset u32, len u32}*count).
fn alseqfile(data: &[u8], base: usize, count_hint: usize) -> R<(u32, Vec<(usize, usize)>)> {
    let rev = be16(data, base)?;
    let count = be16(data, base + 2)? as usize;
    if count != count_hint {
        return Err(format!("expected {count_hint} entries, found {count}"));
    }
    let mut e = Vec::new();
    for i in 0..count {
        e.push((be32(data, base + 4 + i * 8)? as usize, be32(data, base + 8 + i * 8)? as usize));
    }
    Ok((rev, e))
}

fn swap_header(buf: &[u8], count: usize) -> R<Vec<u8>> {
    let mut s = Swapper::new(buf);
    s.u16(0)?;
    s.u16(2)?;
    for i in 0..count {
        s.u32(4 + i * 8)?;
        s.u32(8 + i * 8)?;
    }
    Ok(s.d)
}

struct BankStats {
    samples: usize,
    envelopes: usize,
    extents: Vec<(usize, usize)>,
}

fn convert_bank(raw: &[u8]) -> R<(Vec<u8>, BankStats)> {
    let n_inst = be32(raw, 0)? as usize;
    let n_drums = be32(raw, 4)? as usize;
    let body = &raw[16..];
    let mut sw = Swapper::new(raw);
    for w in (0..16).step_by(4) {
        sw.u32(w)?;
    }
    let mut s = Swapper::new(body);
    let ln = body.len();
    let chk = |off: usize, size: usize| -> R<()> {
        if off + size > ln {
            return Err(format!("bank offset {off:#x} out of range ({ln:#x})"));
        }
        Ok(())
    };
    let (mut seen_env, mut seen_sample, mut seen_loop, mut seen_book) = (HashSet::new(), HashSet::new(), HashSet::new(), HashSet::new());
    let mut extents = Vec::new();

    // Envelopes stay big endian: the engine byte-swaps them when it reads them. Only validate the extent here.
    let envelope = |mut off: usize, seen: &mut HashSet<usize>| -> R<()> {
        if !seen.insert(off) {
            return Ok(());
        }
        loop {
            chk(off, 4)?;
            let delay = i16::from_be_bytes([body[off], body[off + 1]]);
            off += 4;
            if delay <= 0 {
                break;
            }
        }
        Ok(())
    };

    macro_rules! sample {
        ($off:expr) => {{
            let off: usize = $off;
            if off != 0 && seen_sample.insert(off) {
                chk(off, 0x14)?;
                extents.push((be32(body, off + 4)? as usize, be32(body, off + 16)? as usize));
                for k in [4, 8, 12, 16] {
                    s.u32(off + k)?;
                }
                let (lp, book) = (be32(body, off + 8)? as usize, be32(body, off + 12)? as usize);
                if lp != 0 && seen_loop.insert(lp) {
                    chk(lp, 16)?;
                    let count = be32(body, lp + 8)?;
                    for k in [0, 4, 8, 12] {
                        s.u32(lp + k)?;
                    }
                    if count != 0 {
                        chk(lp + 16, 32)?;
                        for k in 0..16 {
                            s.u16(lp + 16 + k * 2)?;
                        }
                    }
                }
                if book != 0 && seen_book.insert(book) {
                    chk(book, 8)?;
                    let (order, npred) = (be32(body, book)? as usize, be32(body, book + 4)? as usize);
                    if !(0 < order && order <= 8 && 0 < npred && npred <= 16) {
                        return Err(format!("implausible ADPCM book at {book:#x}: order {order} predictors {npred}"));
                    }
                    s.u32(book)?;
                    s.u32(book + 4)?;
                    chk(book + 8, order * npred * 16)?;
                    for k in 0..order * npred * 8 {
                        s.u16(book + 8 + k * 2)?;
                    }
                }
            }
        }};
    }
    macro_rules! sound {
        ($off:expr) => {{
            let off: usize = $off;
            let smp = be32(body, off)? as usize;
            s.u32(off)?;
            s.u32(off + 4)?;
            sample!(smp);
        }};
    }

    let drums_off = be32(body, 0)? as usize;
    s.u32(0)?;
    let mut inst = Vec::new();
    for i in 0..n_inst {
        let o = 4 + i * 4;
        inst.push(be32(body, o)? as usize);
        s.u32(o)?;
    }
    for off in inst {
        if off == 0 {
            continue;
        }
        chk(off, 0x20)?;
        let env = be32(body, off + 4)? as usize;
        s.u32(off + 4)?;
        sound!(off + 8);
        sound!(off + 16);
        sound!(off + 24);
        envelope(env, &mut seen_env)?;
    }
    if drums_off != 0 && n_drums != 0 {
        chk(drums_off, n_drums * 4)?;
        let mut drums = Vec::new();
        for i in 0..n_drums {
            drums.push(be32(body, drums_off + i * 4)? as usize);
            s.u32(drums_off + i * 4)?;
        }
        for off in drums {
            if off == 0 {
                continue;
            }
            chk(off, 0x10)?;
            let env = be32(body, off + 12)? as usize;
            sound!(off + 4);
            s.u32(off + 12)?;
            envelope(env, &mut seen_env)?;
        }
    }
    let mut out = sw.d[..16].to_vec();
    out.extend_from_slice(&s.d);
    Ok((out, BankStats { samples: seen_sample.len(), envelopes: seen_env.len(), extents }))
}

fn seq_args(cmd: u8) -> &'static str {
    match cmd {
        0xfc | 0xfb | 0xfa | 0xf9 | 0xf5 | 0xd7 | 0xd6 | 0xd2 | 0xd1 => "w",
        0xf8 | 0xf4 | 0xf3 | 0xf2 | 0xf1 | 0xdf | 0xde | 0xdd | 0xdc | 0xdb | 0xd9 | 0xd5 | 0xd3 | 0xd0 | 0xcc | 0xc9 | 0xc8 => "b",
        0xda => "bw",
        _ => "",
    }
}

enum Frame {
    Call(usize),
    Loop(usize, i64),
}

/// Walk the sequence player's top-level script (conditional branches not taken).
fn summarize_sequence(data: &[u8]) -> Value {
    let (mut pc, mut ticks, mut tempo) = (0usize, 0i64, 120i64);
    let mut stack: Vec<Frame> = Vec::new();
    let mut channels = std::collections::BTreeSet::new();
    let mut at: HashMap<usize, i64> = HashMap::new();
    let mut first_tempo: Option<i64> = None;
    let ft = |first: Option<i64>, tempo: i64| if first.is_some_and(|v| v != 0) { first.unwrap() } else { tempo };
    let mk = |tempo: i64, channels: &std::collections::BTreeSet<u8>, loops: bool, ticks: i64| {
        let mut m = Map::new();
        m.insert("tempo".into(), json!(tempo));
        m.insert("channels".into(), json!(channels.iter().collect::<Vec<_>>()));
        m.insert("loops".into(), json!(loops));
        m.insert("ticks".into(), json!(ticks));
        m
    };
    for _ in 0..200000 {
        if pc >= data.len() {
            break;
        }
        at.insert(pc, ticks);
        let cmd = data[pc];
        pc += 1;
        if cmd == 0xff {
            match stack.pop() {
                None => return Value::Object(mk(ft(first_tempo, tempo), &channels, false, ticks)),
                Some(Frame::Call(r)) => pc = r,
                Some(Frame::Loop(r, _)) => pc = r,
            }
            continue;
        }
        if cmd == 0xfe {
            ticks += 1;
        } else if cmd == 0xfd {
            let Some(&b) = data.get(pc) else { break };
            let mut v = b as i64;
            pc += 1;
            if v & 0x80 != 0 {
                let Some(&b2) = data.get(pc) else { break };
                v = ((v & 0x7f) << 8) | b2 as i64;
                pc += 1;
            }
            ticks += v;
        } else if cmd >= 0xc0 {
            let mut vals: Vec<i64> = Vec::new();
            for a in seq_args(cmd).chars() {
                if a == 'b' {
                    let Some(&b) = data.get(pc) else { return Value::Null };
                    vals.push(b as i64);
                    pc += 1;
                } else {
                    let Some(w) = data.get(pc..pc + 2) else { return Value::Null };
                    vals.push(u16::from_be_bytes([w[0], w[1]]) as i64);
                    pc += 2;
                }
            }
            match cmd {
                0xdd => {
                    tempo = vals[0];
                    if !first_tempo.is_some_and(|v| v != 0) {
                        first_tempo = Some(tempo);
                    }
                }
                0xfc => {
                    stack.push(Frame::Call(pc));
                    pc = vals[0] as usize;
                }
                0xf8 => stack.push(Frame::Loop(pc, if vals[0] != 0 { vals[0] } else { 256 })),
                0xf7 if matches!(stack.last(), Some(Frame::Loop(..))) => {
                    if let Some(Frame::Loop(r, n)) = stack.last_mut() {
                        *n -= 1;
                        if *n != 0 {
                            pc = *r;
                        } else {
                            stack.pop();
                        }
                    }
                }
                0xfb => {
                    let target = vals[0] as usize;
                    if let Some(&t0) = at.get(&target) {
                        let mut m = mk(ft(first_tempo, tempo), &channels, true, ticks);
                        m.insert("loop_start_ticks".into(), json!(t0));
                        m.insert("loop_ticks".into(), json!(ticks - t0));
                        m.insert("loop_target".into(), json!(target));
                        return Value::Object(m);
                    }
                    pc = target;
                }
                _ => {}
            }
        } else if cmd & 0xf0 == 0x90 {
            channels.insert(cmd & 0xf);
            pc += 2;
        }
    }
    let mut m = mk(ft(first_tempo, tempo), &channels, false, ticks);
    m.insert("truncated".into(), json!(true));
    Value::Object(m)
}

pub struct Blobs {
    seq: Vec<u8>,
    ctl: Vec<u8>,
    tbl: Vec<u8>,
    sets: Vec<u8>,
}

fn build(rom: &[u8]) -> R<(Blobs, Value)> {
    let (rev, seqs) = alseqfile(rom, SEQ_ROM, SEQ_COUNT)?;
    if rev != 3 {
        return Err("unexpected sequence archive revision".into());
    }
    let seq_end = seqs.iter().map(|(o, l)| o + l).max().unwrap();
    let mut seq = swap_header(rom.get(SEQ_ROM..SEQ_ROM + seq_end).ok_or("sequences outside the ROM")?, SEQ_COUNT)?;
    seq.resize(seq.len().div_ceil(16) * 16, 0);
    let pattern: Vec<u8> = [SEQ_COUNT * 2, SEQ_COUNT * 2 + 2, SEQ_COUNT * 2 + 4].iter().flat_map(|v| (*v as u16).to_be_bytes()).collect();
    let lo = SEQ_ROM + seq_end;
    let window = rom.get(lo..lo + 0x100).ok_or("instrument set table outside the ROM")?;
    let found = window.windows(pattern.len()).position(|w| w == pattern.as_slice()).ok_or("instrument set table not found behind the sequences (ROM layout changed?)")?;
    let sets_rom = lo + found;
    let mut sets = rom.get(sets_rom..sets_rom + 0x100).ok_or("bank sets outside the ROM")?.to_vec();
    if be16(&sets, 0)? as usize != SEQ_COUNT * 2 {
        return Err("instrument set table starts at the wrong offset (ROM layout changed?)".into());
    }
    for i in 0..SEQ_COUNT {
        sets.swap(i * 2, i * 2 + 1);
    }
    let (_, banks) = alseqfile(rom, BANKS_ROM, 21)?;
    let (_, tables) = alseqfile(rom, TABLES_ROM, 21)?;
    let mut ctl = rom[BANKS_ROM..TABLES_ROM].to_vec();
    ctl = swap_header(&ctl, 21)?;
    let (mut n_samples, mut n_envs) = (0, 0);
    for &(off, ln) in &banks {
        let (conv, st) = convert_bank(&rom[BANKS_ROM + off..BANKS_ROM + off + ln])?;
        for (at, size) in &st.extents {
            if (*at + *size) as i64 > (SEQ_ROM - TABLES_ROM) as i64 - tables[0].0 as i64 + 16 {
                return Err(format!("sample at {at:#x}+{size:#x} lies outside the sample table"));
            }
        }
        ctl[off..off + ln].copy_from_slice(&conv);
        n_samples += st.samples;
        n_envs += st.envelopes;
    }
    let tbl = swap_header(&rom[TABLES_ROM..SEQ_ROM], 21)?;
    for i in 0..SEQ_COUNT {
        let o = u16::from_le_bytes([sets[i * 2], sets[i * 2 + 1]]) as usize;
        let n = sets[o] as usize;
        if !(0 < n && n < 8 && sets[o + 1..o + 1 + n].iter().all(|&b| b < 21)) {
            return Err(format!("bank set {i}"));
        }
    }
    let mut seq_info = Vec::new();
    for (i, &(o, l)) in seqs.iter().enumerate() {
        let summary = summarize_sequence(&rom[SEQ_ROM + o..SEQ_ROM + o + l]);
        let mut m = Map::new();
        m.insert("id".into(), json!(i));
        m.insert("offset".into(), json!(o));
        m.insert("length".into(), json!(l));
        if let Value::Object(s) = summary {
            m.extend(s);
        }
        seq_info.push(Value::Object(m));
    }
    let info = json!({"sequences": seq_info, "banks": banks.len(), "samples": n_samples, "envelopes": n_envs,
        "bytes": {"seq": seq.len(), "ctl": ctl.len(), "tbl": tbl.len(), "sets": sets.len()}});
    Ok((Blobs { seq, ctl, tbl, sets }, info))
}

fn patch_template(blobs: &Blobs, template: &[u8], layout: &Value) -> R<Vec<u8>> {
    let mut wasm = template.to_vec();
    for (symbol, blob) in [("gMk64Sequences", &blobs.seq), ("gMk64Banks", &blobs.ctl), ("gMk64Tables", &blobs.tbl), ("gMk64BankSets", &blobs.sets)] {
        let at = layout[symbol]["offset"].as_u64().ok_or_else(|| format!("layout: {symbol}"))? as usize;
        let length = layout[symbol]["length"].as_u64().ok_or_else(|| format!("layout: {symbol}"))? as usize;
        if blob.len() != length {
            return Err(format!("{symbol}: built {} bytes, the template expects {length} (is this the US 1.0 ROM?)", blob.len()));
        }
        if wasm[at..at + length].iter().any(|&b| b != 0) {
            return Err("template is not data-less".into());
        }
        wasm[at..at + length].copy_from_slice(blob);
    }
    Ok(wasm)
}

/// `assets/audio/music/mk64/{mk64-audio.wasm,sequences.json}` into `out`.
pub fn export(rom: &[u8], template: &[u8], layout: &Value, out: &Path) -> R<()> {
    let (blobs, info) = build(rom)?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    std::fs::write(out.join("mk64-audio.wasm"), patch_template(&blobs, template, layout)?).map_err(|e| e.to_string())?;
    std::fs::write(out.join("sequences.json"), dumps(&info, None) + "\n").map_err(|e| e.to_string())?;
    Ok(())
}
