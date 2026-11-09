//! Port of the decomp's `tools/assemble_sound.py` for the US build (little-endian, 32-bit words, the layout the wasm music
//! engine reads): sound banks + samples to `native.ctl` / `native.tbl`, and sequences + bank sets to `sequences.bin` /
//! `sets.bin`. The ADPCM sample bytes come straight from the ROM (`aifc_decode` round-trips with `vadpcm_enc`, so
//! re-encoding is not needed); validation is dropped, the layout logic is a line-by-line port.
use crate::sound::{sample_rate, Entry};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

pub struct SampleFile {
    pub bank: String,
    pub name: String,
    pub entry: Entry,
}

fn align(v: usize, a: usize) -> usize {
    (v + a - 1) & !(a - 1)
}

/// `GarbageSerializer`: the output buffer plus the model of the N64 DMA scratch buffer used to fill alignment padding.
struct Garbage {
    out: Vec<u8>,
    bufs: Vec<Vec<(usize, usize, usize)>>, // (garbage pos, start in out, len)
    pos: usize,
}

impl Garbage {
    fn new() -> Self {
        Garbage { out: Vec::new(), bufs: vec![Vec::new()], pos: 0 }
    }
    fn reset(&mut self) {
        self.bufs.push(Vec::new());
        self.pos = 0;
    }
    fn add(&mut self, part: &[u8]) {
        let start = self.out.len();
        self.out.extend_from_slice(part);
        self.bufs.last_mut().unwrap().push((self.pos, start, part.len()));
        self.pos += part.len();
    }
    fn align(&mut self, a: usize) {
        let n = align(self.out.len(), a) - self.out.len();
        self.add(&vec![0u8; n]);
    }
    fn garbage_at(&self, pos: usize) -> u8 {
        let pos = (pos & 0xFFFF) as i64;
        for bufs in self.bufs.iter().rev() {
            for &(bpos, start, len) in bufs.iter().rev() {
                let q = ((bpos as i64 + len as i64 - 1 - pos) & !0xFFFFi64) + pos;
                if q >= bpos as i64 {
                    return self.out[start + (q as usize - bpos)];
                }
            }
        }
        0
    }
    fn align_garbage(&mut self, a: usize) {
        while self.out.len() % a != 0 {
            let b = self.garbage_at(self.pos);
            self.add(&[b]);
        }
    }
}

fn strip_comments(s: &str) -> String {
    // re.sub(r"/\*.*?\*/", "", s, DOTALL) then re.sub(r"//.*?\n", "", s)
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find("/*") {
        match rest[a + 2..].find("*/") {
            Some(b) => {
                out.push_str(&rest[..a]);
                rest = &rest[a + 2 + b + 2..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    let mut res = String::new();
    let mut rest = out.as_str();
    while let Some(a) = rest.find("//") {
        match rest[a..].find('\n') {
            Some(b) => {
                res.push_str(&rest[..a]);
                rest = &rest[a + b + 1..];
            }
            None => break,
        }
    }
    res.push_str(rest);
    res
}

fn apply_ifs(v: Value, defines: &[&str]) -> Value {
    match v {
        Value::Object(m) if m.contains_key("ifdef") && m.contains_key("then") && m.contains_key("else") => {
            let yes = m["ifdef"].as_array().is_some_and(|a| a.iter().any(|d| d.as_str().is_some_and(|d| defines.contains(&d))));
            apply_ifs(if yes { m["then"].clone() } else { m["else"].clone() }, defines)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(|x| apply_ifs(x, defines)).collect()),
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, x)| (k, apply_ifs(x, defines))).collect()),
        other => other,
    }
}

struct Bank {
    name: String,
    sample_bank: usize,
    json: Map<String, Value>,
}

fn sample_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(m) => m.get("sample").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

fn used_samples(json: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for inst in json["instruments"].as_object().unwrap().values() {
        if let Some(list) = inst.as_array() {
            for d in list {
                out.extend(sample_of(&d["sound"]));
            }
        } else {
            for k in ["sound_lo", "sound", "sound_hi"] {
                if let Some(s) = inst.get(k).and_then(sample_of) {
                    out.push(s);
                }
            }
        }
    }
    out
}

fn u16le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&(x as u16).to_le_bytes());
}
fn u32le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

pub struct Blobs {
    pub ctl: Vec<u8>,
    pub tbl: Vec<u8>,
    pub sequences: Vec<u8>,
    pub sets: Vec<u8>,
}

#[allow(clippy::too_many_lines)]
pub fn build(decomp: &Path, samples: &mut [SampleFile], sequences: &[(String, Vec<u8>)]) -> Result<Blobs, String> {
    let defines = ["VERSION_US"];
    // sample banks (directories) in sorted order; entries sorted by file name
    samples.sort_by(|a, b| (a.bank.as_str(), a.name.as_str()).cmp(&(b.bank.as_str(), b.name.as_str())));
    let mut sb_names: Vec<String> = Vec::new();
    for s in samples.iter() {
        if sb_names.last() != Some(&s.bank) {
            sb_names.push(s.bank.clone());
        }
    }
    let sb_index = |n: &str| sb_names.iter().position(|x| x == n);
    let mut used: Vec<bool> = vec![false; samples.len()];
    let mut offsets: Vec<usize> = vec![0; samples.len()];
    let find = |sb: usize, name: &str| samples.iter().position(|s| s.bank == sb_names[sb] && s.name == name);

    // sound banks
    let dir = decomp.join("sound/sound_banks");
    let mut files: Vec<String> = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    let bank_names: Vec<String> = {
        let mut v: Vec<String> = files.iter().map(|f| f.rsplit_once('.').map_or(f.clone(), |x| x.0.to_string())).collect();
        v.sort();
        v
    };
    let mut banks: Vec<Bank> = Vec::new();
    for f in files.iter().filter(|f| f.ends_with(".json")) {
        let text = strip_comments(&std::fs::read_to_string(dir.join(f)).map_err(|e| e.to_string())?);
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{f}: {e}"))?;
        let Value::Object(mut json) = apply_ifs(v, &defines) else { return Err(format!("{f}: not an object")) };
        // apply_version_diffs: drop instruments whose ifdef list names no active define
        let removed: Vec<String> = json["instruments"].as_object().unwrap().iter().filter(|(_, i)| i.get("ifdef").and_then(Value::as_array).is_some_and(|l| l.iter().all(|d| !defines.contains(&d.as_str().unwrap_or(""))))).map(|(k, _)| k.clone()).collect();
        for k in &removed {
            json["instruments"].as_object_mut().unwrap().shift_remove(k);
            let list = json.get_mut("instrument_list").and_then(Value::as_array_mut).unwrap();
            if let Some(p) = list.iter().position(|x| x.as_str() == Some(k)) {
                list.remove(p);
            }
        }
        let sbn = json["sample_bank"].as_str().ok_or("sample_bank")?.to_string();
        let sb = sb_index(&sbn).ok_or_else(|| format!("sample bank {sbn} not found"))?;
        let name = f.trim_end_matches(".json").to_string();
        for s in used_samples(&json) {
            let i = find(sb, &s).ok_or_else(|| format!("{f}: sample {s} not in {sbn}"))?;
            used[i] = true;
        }
        banks.push(Bank { name, sample_bank: sb, json });
    }
    // sample banks that are used, ordered by their first user's name
    let mut uses: HashMap<usize, Vec<usize>> = HashMap::new();
    for (bi, b) in banks.iter().enumerate() {
        uses.entry(b.sample_bank).or_default().push(bi);
    }
    let mut used_sbs: Vec<usize> = uses.keys().copied().collect();
    used_sbs.sort_by(|a, b| banks[uses[a][0]].name.cmp(&banks[uses[b][0]].name));
    let final_index: HashMap<usize, usize> = used_sbs.iter().enumerate().map(|(i, s)| (*s, i)).collect();

    // ---- tbl ----
    let mut tbl = Garbage::new();
    let mut tbl_entry = vec![(0usize, 0usize); used_sbs.len()];
    for (k, &sb) in used_sbs.iter().enumerate() {
        let start = tbl.out.len();
        tbl.reset();
        for (i, s) in samples.iter().enumerate() {
            if s.bank != sb_names[sb] || !used[i] {
                continue;
            }
            tbl.align(16);
            offsets[i] = tbl.out.len() - start;
            let d = s.entry.data.clone();
            tbl.add(&d);
        }
        tbl.align(2);
        tbl.align_garbage(16);
        tbl_entry[k] = (start, tbl.out.len() - start);
    }
    let entry_list: Vec<usize> = banks.iter().map(|b| final_index[&b.sample_bank]).collect();
    let tbl_file = seqfile(&tbl.out, &tbl_entry, &entry_list, 2, true);

    // ---- ctl ----
    let mut ctl = Garbage::new();
    let mut ctl_entry = Vec::new();
    for b in &banks {
        let start = ctl.out.len();
        serialize_ctl(b, &banks, samples, &offsets, &mut ctl, &sb_names)?;
        ctl_entry.push((start, ctl.out.len() - start));
    }
    let all: Vec<usize> = (0..banks.len()).collect();
    let ctl_file = seqfile(&ctl.out, &ctl_entry, &all, 1, true);

    // ---- sequences ----
    let sj = strip_comments(&std::fs::read_to_string(decomp.join("sound/sequences.json")).map_err(|e| e.to_string())?);
    let Value::Object(mut seqs): Value = serde_json::from_str(&sj).map_err(|e| format!("sequences.json: {e}"))? else { return Err("sequences.json".into()) };
    seqs.shift_remove("comment");
    let mut banks_of: Vec<(String, Option<Vec<String>>)> = Vec::new();
    for (k, v) in &seqs {
        let list = match v {
            Value::Object(o) => {
                if o["ifdef"].as_array().unwrap().iter().all(|d| !defines.contains(&d.as_str().unwrap_or(""))) {
                    None
                } else {
                    Some(o["banks"].as_array().unwrap().iter().map(|b| b.as_str().unwrap().to_string()).collect())
                }
            }
            Value::Array(a) => Some(a.iter().map(|b| b.as_str().unwrap().to_string()).collect()),
            _ => None,
        };
        banks_of.push((k.clone(), list));
    }
    let mut ind_to_name: Vec<Option<usize>> = Vec::new();
    for (i, (k, _)) in banks_of.iter().enumerate() {
        let ind = usize::from_str_radix(k.split('_').next().unwrap(), 16).map_err(|e| e.to_string())?;
        while ind_to_name.len() <= ind {
            ind_to_name.push(None);
        }
        if ind_to_name[ind].is_some() {
            return Err(format!("sequence index clash at {k}"));
        }
        ind_to_name[ind] = Some(i);
    }
    while ind_to_name.last().is_some_and(|x| x.is_none_or(|i| banks_of[i].1.is_none())) {
        ind_to_name.pop();
    }
    let mut seq = Garbage::new();
    let mut seq_entry = Vec::new();
    for slot in &ind_to_name {
        let start = seq.out.len();
        if let Some(i) = slot.filter(|i| banks_of[*i].1.is_some()) {
            seq.reset();
            let want = &banks_of[i].0;
            let (_, bytes) = sequences.iter().find(|(n, _)| n == want).ok_or_else(|| format!("no sequence file for {want}"))?;
            seq.add(bytes);
            seq.align_garbage(16);
        }
        seq_entry.push((start, seq.out.len() - start));
    }
    let all: Vec<usize> = (0..ind_to_name.len()).collect();
    let sequences_bin = seqfile(&seq.out, &seq_entry, &all, 3, false);
    // bank sets
    let mut sets = vec![0u8; ind_to_name.len() * 2];
    for (n, slot) in ind_to_name.iter().enumerate() {
        let set: Vec<String> = slot.and_then(|i| banks_of[i].1.clone()).unwrap_or_default();
        let at = sets.len() as u16;
        sets[n * 2..n * 2 + 2].copy_from_slice(&at.to_le_bytes());
        sets.push(set.len() as u8);
        for b in set.iter().rev() {
            sets.push(bank_names.iter().position(|x| x == b).ok_or_else(|| format!("unknown bank {b}"))? as u8);
        }
    }
    sets.resize(align(sets.len(), 16), 0);
    Ok(Blobs { ctl: ctl_file, tbl: tbl_file, sequences: sequences_bin, sets })
}

/// `serialize_seqfile` (non-Shindou): header + table + data (+ padding), 64-byte aligned.
fn seqfile(data: &[u8], entries: &[(usize, usize)], entry_list: &[usize], magic: u32, extra_padding: bool) -> Vec<u8> {
    let mut out = Vec::new();
    u16le(&mut out, magic);
    u16le(&mut out, entry_list.len() as u32);
    let table_at = out.len();
    out.resize(out.len() + entry_list.len() * 8, 0);
    out.resize(align(out.len(), 16), 0);
    let data_start = out.len();
    out.extend_from_slice(data);
    if extra_padding {
        out.push(0);
    }
    out.resize(align(out.len(), 64), 0);
    for (n, &i) in entry_list.iter().enumerate() {
        out[table_at + n * 8..table_at + n * 8 + 4].copy_from_slice(&((entries[i].0 + data_start) as u32).to_le_bytes());
        out[table_at + n * 8 + 4..table_at + n * 8 + 8].copy_from_slice(&(entries[i].1 as u32).to_le_bytes());
    }
    out
}

fn serialize_ctl(bank: &Bank, banks: &[Bank], samples: &[SampleFile], offsets: &[usize], base: &mut Garbage, sb_names: &[String]) -> Result<(), String> {
    let json = &bank.json;
    let insts = json["instruments"].as_object().unwrap();
    let drums: Vec<&Value> = insts.values().filter_map(Value::as_array).next().map(|l| l.iter().collect()).unwrap_or_default();
    let list = json["instrument_list"].as_array().unwrap();
    let date: u32 = json.get("date").and_then(Value::as_str).map_or(0, |d| {
        let p: Vec<u32> = d.split('-').map(|x| x.parse().unwrap_or(0)).collect();
        p[0] * 10000 + p[1] * 100 + p[2]
    });
    let bcd = {
        let (mut n, mut shift, mut r) = (date, 0, 0u32);
        while n != 0 {
            r |= (n % 10) << shift;
            shift += 4;
            n /= 10;
        }
        r
    };
    let shared = banks.iter().filter(|b| b.sample_bank == bank.sample_bank).count() > 1;
    let mut h = Vec::new();
    for v in [list.len() as u32, drums.len() as u32, u32::from(shared), bcd] {
        u32le(&mut h, v);
    }
    base.add(&h);

    let mut ser: Vec<u8> = Vec::new();
    let drum_pos = if drums.is_empty() {
        ser.extend_from_slice(&[0; 4]);
        None
    } else {
        ser.extend_from_slice(&[0; 4]);
        Some(0usize)
    };
    let inst_pos_at = ser.len();
    ser.resize(ser.len() + 4 * list.len(), 0);
    ser.resize(align(ser.len(), 16), 0);

    let find = |name: &str| -> Result<usize, String> { samples.iter().position(|s| s.bank == sb_names[bank.sample_bank] && s.name == name).ok_or_else(|| format!("sample {name}")) };
    let mut sample_addr: HashMap<String, usize> = HashMap::new();
    for name in used_samples(json) {
        if sample_addr.contains_key(&name) {
            continue;
        }
        sample_addr.insert(name.clone(), ser.len());
        let si = find(&name)?;
        let e = &samples[si].entry;
        let len = e.data.len();
        u32le(&mut ser, 0);
        u32le(&mut ser, offsets[si] as u32);
        let loop_at = ser.len();
        u32le(&mut ser, 0);
        let book_at = ser.len();
        u32le(&mut ser, 0);
        u32le(&mut ser, align(len, 2) as u32);
        ser.resize(align(ser.len(), 16), 0);
        // book
        let pos = ser.len() as u32;
        ser[book_at..book_at + 4].copy_from_slice(&pos.to_le_bytes());
        u32le(&mut ser, e.order as u32);
        u32le(&mut ser, e.npredictors as u32);
        for x in &e.book {
            ser.extend_from_slice(&x.to_le_bytes());
        }
        ser.resize(align(ser.len(), 16), 0);
        // loop
        let pos = ser.len() as u32;
        ser[loop_at..loop_at + 4].copy_from_slice(&pos.to_le_bytes());
        let (ls, le, lc, state) = &e.loop_;
        if *lc == 0 {
            let end = (len / 9 * 16 + (len % 2) + (len % 9)) as u32;
            for v in [0, end, 0, 0] {
                u32le(&mut ser, v);
            }
        } else {
            for v in [*ls, *le, *lc as u32, 0] {
                u32le(&mut ser, v);
            }
            for x in state {
                ser.extend_from_slice(&x.to_le_bytes());
            }
        }
        ser.resize(align(ser.len(), 16), 0);
    }
    let mut env_addr: HashMap<String, usize> = HashMap::new();
    for (name, env) in json["envelopes"].as_object().unwrap() {
        env_addr.insert(name.clone(), ser.len());
        for entry in env.as_array().unwrap() {
            let (a, b): (u32, u32) = match entry {
                Value::String(s) if s == "stop" => (0, 0),
                Value::String(s) if s == "hang" => (65535, 0),
                Value::String(s) if s == "restart" => (65533, 0),
                Value::Array(p) if p[0].as_str() == Some("goto") => (65534, p[1].as_u64().unwrap() as u32),
                Value::Array(p) => (p[0].as_u64().unwrap() as u32, p[1].as_u64().unwrap() as u32),
                _ => return Err("bad envelope entry".into()),
            };
            ser.extend_from_slice(&(a as u16).to_be_bytes());
            ser.extend_from_slice(&(b as u16).to_be_bytes());
        }
        ser.resize(align(ser.len(), 16), 0);
    }
    let ser_sound = |ser: &mut Vec<u8>, sound: Option<&Value>| -> Result<(), String> {
        let sample = sound.and_then(sample_of);
        let addr = match &sample {
            None => 0,
            Some(s) => *sample_addr.get(s).ok_or("sample address")?,
        };
        let tuning: f64 = match sound.and_then(|s| s.get("tuning")).and_then(Value::as_f64) {
            Some(t) => t,
            None => match &sample {
                Some(s) => sample_rate(&samples[find(s)?].entry) / 32000.0,
                None => 0.0,
            },
        };
        u32le(ser, addr as u32);
        ser.extend_from_slice(&(tuning as f32).to_le_bytes());
        Ok(())
    };
    let mut inst_pos: HashMap<String, usize> = HashMap::new();
    for (name, inst) in insts {
        if inst.is_array() {
            continue;
        }
        inst_pos.insert(name.clone(), ser.len());
        let g = |k: &str, d: u64| inst.get(k).and_then(Value::as_u64).unwrap_or(d) as u8;
        ser.extend_from_slice(&[0, g("normal_range_lo", 0), g("normal_range_hi", 127), g("release_rate", 0)]);
        u32le(&mut ser, env_addr[inst["envelope"].as_str().unwrap()] as u32);
        ser_sound(&mut ser, inst.get("sound_lo"))?;
        ser_sound(&mut ser, inst.get("sound"))?;
        ser_sound(&mut ser, inst.get("sound_hi"))?;
    }
    ser.resize(align(ser.len(), 16), 0);
    for (n, name) in list.iter().enumerate() {
        let p = name.as_str().map_or(0, |s| inst_pos[s]) as u32;
        ser[inst_pos_at + n * 4..inst_pos_at + n * 4 + 4].copy_from_slice(&p.to_le_bytes());
    }
    if let Some(at) = drum_pos {
        let mut poses = Vec::new();
        for d in &drums {
            poses.push(ser.len());
            ser.extend_from_slice(&[d["release_rate"].as_u64().unwrap() as u8, d["pan"].as_u64().unwrap() as u8, 0, 0]);
            ser_sound(&mut ser, d.get("sound"))?;
            u32le(&mut ser, env_addr[d["envelope"].as_str().unwrap()] as u32);
        }
        ser.resize(align(ser.len(), 16), 0);
        let pos = ser.len() as u32;
        ser[at..at + 4].copy_from_slice(&pos.to_le_bytes());
        for p in poses {
            u32le(&mut ser, p as u32);
        }
        ser.resize(align(ser.len(), 16), 0);
    }
    base.add(&ser);
    Ok(())
}
