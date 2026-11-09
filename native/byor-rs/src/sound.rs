//! SM64 sound-bank sample extraction: the port of the decomp's `tools/disassemble_sound.py --only-samples` and
//! `tools/aifc_decode.c` (VADPCM to PCM AIFF, the "bruteforcing" decoder that round-trips with `vadpcm_enc`), plus
//! `scripts/export-audio.py` (AIFF to WAV + manifest). Every step is deterministic and byte-identical to the originals.
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Entry {
    pub data: Vec<u8>,
    pub book: Vec<i16>, // order * npredictors * 8 values
    pub order: i32,
    pub npredictors: i32,
    /// `(start, end, count, state)`; `count == 0` means no loop
    pub loop_: (u32, u32, i32, Vec<i16>),
    pub tunings: Vec<f64>,
}

struct Bank {
    data: Vec<u8>,
    entries: BTreeMap<u32, Entry>,
}

fn be32(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap())).ok_or_else(|| "sound data truncated".to_string())
}
fn be16s(d: &[u8], o: usize) -> Result<i16, String> {
    d.get(o..o + 2).map(|b| i16::from_be_bytes(b.try_into().unwrap())).ok_or_else(|| "sound data truncated".to_string())
}

/// `parse_seqfile`: `(offset, length)` per entry of a ctl / tbl file.
fn seqfile(d: &[u8], filetype: u16) -> Result<Vec<(usize, usize)>, String> {
    let magic = u16::from_be_bytes(d.get(..2).ok_or("short sound file")?.try_into().unwrap());
    if magic != filetype {
        return Err("bad sound file magic".into());
    }
    let n = u16::from_be_bytes(d[2..4].try_into().unwrap()) as usize;
    (0..n).map(|i| Ok((be32(d, 4 + i * 8)? as usize, be32(d, 8 + i * 8)? as usize))).collect()
}

/// Reads the US (non-Shindou) ctl/tbl pair and returns every sample entry in extraction order (the `index` of
/// `assets.json`'s `["@sound", index]`).
pub fn sample_entries(ctl: &[u8], tbl: &[u8]) -> Result<Vec<Entry>, String> {
    let ctl_entries = seqfile(ctl, 1)?;
    let tbl_entries = seqfile(tbl, 2)?;
    if ctl_entries.len() != tbl_entries.len() {
        return Err("ctl/tbl entry counts differ".into());
    }
    // parse_tbl: one sample bank per distinct tbl offset, in first-seen order
    let mut banks: Vec<Bank> = Vec::new();
    let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
    let mut of_entry = Vec::new();
    for &(off, len) in &tbl_entries {
        let idx = *seen.entry(off).or_insert_with(|| {
            banks.push(Bank { data: tbl.get(off..(off + len).min(tbl.len())).unwrap_or(&[]).to_vec(), entries: BTreeMap::new() });
            banks.len() - 1
        });
        of_entry.push(idx);
    }
    for (i, &(off, len)) in ctl_entries.iter().enumerate() {
        let entry = ctl.get(off..(off + len).min(ctl.len())).ok_or("ctl entry outside the file")?;
        parse_ctl(entry, &mut banks[of_entry[i]])?;
    }
    Ok(banks.into_iter().flat_map(|b| b.entries.into_values()).collect())
}

fn parse_ctl(entry: &[u8], bank: &mut Bank) -> Result<(), String> {
    let (num_inst, num_drums) = (be32(entry, 0)? as usize, be32(entry, 4)? as usize);
    let data = entry.get(16..).ok_or("short ctl entry")?;
    let drum_base = be32(data, 0)? as usize;
    let drum_addrs: Vec<usize> = (0..num_drums).map(|i| be32(data, drum_base + i * 4).map(|v| v as usize)).collect::<Result<_, _>>()?;
    let mut inst_addrs = Vec::new();
    for i in 0..num_inst {
        let a = be32(data, 4 + i * 4)? as usize;
        if a != 0 {
            inst_addrs.push(a);
        }
    }
    inst_addrs.sort_unstable();
    let sound = |o: usize| -> Result<Option<(u32, f64)>, String> {
        let addr = be32(data, o)?;
        let tuning = f32::from_bits(be32(data, o + 4)?) as f64;
        Ok((addr != 0).then_some((addr, tuning)))
    };
    let mut sounds: Vec<(u32, f64)> = Vec::new();
    for &a in &inst_addrs {
        for k in [8, 16, 24] {
            sounds.extend(sound(a + k)?);
        }
    }
    for &a in &drum_addrs {
        sounds.extend(sound(a + 4)?);
    }
    let mut tunings: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    for (a, t) in sounds {
        tunings.entry(a).or_default().push(t);
    }
    for (addr, tun) in tunings {
        let o = addr as usize;
        let (zero, sample_addr, loop_addr, book_addr, mut size) = (be32(data, o)?, be32(data, o + 4)?, be32(data, o + 8)? as usize, be32(data, o + 12)? as usize, be32(data, o + 16)? as usize);
        if zero != 0 || loop_addr == 0 || book_addr == 0 || size % 2 != 0 {
            return Err("unexpected sample header".into());
        }
        if size % 9 != 0 {
            size -= 1;
        }
        let e = match bank.entries.get_mut(&sample_addr) {
            Some(e) => e,
            None => {
                let start = sample_addr as usize;
                let d = bank.data.get(start..start + size).ok_or("sample outside its bank")?.to_vec();
                let (ls, le, lc) = (be32(data, loop_addr)?, be32(data, loop_addr + 4)?, be32(data, loop_addr + 8)? as i32);
                let state = if lc != 0 { (0..16).map(|i| be16s(data, loop_addr + 16 + i * 2)).collect::<Result<Vec<_>, _>>()? } else { Vec::new() };
                let (order, npred) = (be32(data, book_addr)? as i32, be32(data, book_addr + 4)? as i32);
                if order != 2 || npred != 2 {
                    return Err("unsupported codebook".into());
                }
                let book = (0..32).map(|i| be16s(data, book_addr + 8 + i * 2)).collect::<Result<Vec<_>, _>>()?;
                bank.entries.insert(sample_addr, Entry { data: d, book, order, npredictors: npred, loop_: (ls, le, lc, state), tunings: Vec::new() });
                bank.entries.get_mut(&sample_addr).unwrap()
            }
        };
        e.tunings.extend(tun);
    }
    Ok(())
}

/// `serialize_f80`
pub fn f80(num: f64) -> [u8; 10] {
    let bits = num.to_bits();
    let sign = bits & (1 << 63);
    let mut out = [0u8; 10];
    if num == 0.0 {
        if sign != 0 {
            out[0] = 0x80;
        }
        return out;
    }
    let exp = ((bits ^ sign) >> 52) as i64 - 1023;
    let mant = (1u64 << 63) | ((bits & ((1 << 52) - 1)) << 11);
    let se = ((sign >> 48) as u16) | ((exp + 0x3FFF) as u16);
    out[..2].copy_from_slice(&se.to_be_bytes());
    out[2..].copy_from_slice(&mant.to_be_bytes());
    out
}

pub fn sample_rate(e: &Entry) -> f64 {
    let t = &e.tunings;
    let all_same = t.iter().all(|x| *x == t[0]);
    if all_same {
        return 32000.0 * t[0];
    }
    let (lo, hi) = (t.iter().cloned().fold(f64::INFINITY, f64::min), t.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    if lo <= 0.5 && 0.5 <= hi {
        16000.0
    } else if lo <= 1.0 && 1.0 <= hi {
        32000.0
    } else if lo <= 1.5 && 1.5 <= hi {
        48000.0
    } else if lo <= 2.5 && 2.5 <= hi {
        80000.0
    } else {
        16000.0 * (lo + hi)
    }
}

// ---- aifc_decode.c ----

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(3123692312231);
        self.0 = self.0.wrapping_add(1);
        (self.0 >> 33) as i32
    }
}

fn qsample(x: i32, scale: i32) -> i16 {
    if scale == 0 {
        return x as i16;
    }
    ((x.wrapping_add(1 << (scale - 1)).wrapping_sub((x > 0) as i32)) >> scale) as i16
}

fn clamp16(x: i32) -> i16 {
    x.clamp(-0x8000, 0x7fff) as i16
}

fn inner_product(len: usize, v1: &[i32], v2: &[i32]) -> i32 {
    let mut out = 0i32;
    for i in 0..len {
        out = out.wrapping_add(v1[i].wrapping_mul(v2[i]));
    }
    let dout = out / (1 << 11);
    let fiout = dout.wrapping_mul(1 << 11);
    dout - (out.wrapping_sub(fiout) < 0) as i32
}

type Table = Vec<Vec<Vec<i32>>>; // [predictor][8][order + 8]

fn codebook(book: &[i16], order: usize, npred: usize) -> Table {
    let mut t = vec![vec![vec![0i32; order + 8]; 8]; npred];
    for i in 0..npred {
        for j in 0..order {
            for k in 0..8 {
                t[i][k][j] = book[i * order * 8 + j * 8 + k] as i32;
            }
        }
        for k in 1..8 {
            t[i][k][order] = t[i][k - 1][order - 1];
        }
        t[i][0][order] = 1 << 11;
        for k in 1..8 {
            let mut j = 0;
            while j < k {
                t[i][j][k + order] = 0;
                j += 1;
            }
            while j < 8 {
                t[i][j][k + order] = t[i][j - k][order];
                j += 1;
            }
        }
    }
    t
}

fn decode_frame(frame: &[u8], state: &mut [i32; 16], order: usize, table: &Table) {
    let scale = 1i32 << (frame[0] >> 4);
    let optimalp = (frame[0] & 0xf) as usize;
    let mut ix = [0i32; 16];
    for i in (0..16).step_by(2) {
        let c = frame[1 + i / 2];
        ix[i] = (c >> 4) as i32;
        ix[i + 1] = (c & 0xf) as i32;
    }
    for v in ix.iter_mut() {
        if *v >= 8 {
            *v -= 16;
        }
        *v = v.wrapping_mul(scale);
    }
    for j in 0..2 {
        let mut in_vec = [0i32; 16];
        for i in 0..order {
            in_vec[i] = if j == 0 { state[16 - order + i] } else { state[8 - order + i] };
        }
        for i in 0..8 {
            let ind = j * 8 + i;
            in_vec[order + i] = ix[ind];
            state[ind] = inner_product(order + i, &table[optimalp][i], &in_vec).wrapping_add(ix[ind]);
        }
    }
}

fn encode_frame(out: &mut [u8; 9], inb: &[i16; 16], state: &mut [i32; 16], table: &Table, order: usize, npred: usize) {
    let mut prediction = [0i32; 16];
    let mut in_vec = [0i32; 16];
    let mut e = [0i32; 16];
    let mut optimalp = 0usize;
    let mut min = 1e30f32;
    for k in 0..npred {
        for j in 0..2 {
            for i in 0..order {
                in_vec[i] = if j == 0 { state[16 - order + i] } else { inb[8 - order + i] as i32 };
            }
            for i in 0..8 {
                prediction[j * 8 + i] = inner_product(order + i, &table[k][i], &in_vec);
                e[j * 8 + i] = (inb[j * 8 + i] as i32).wrapping_sub(prediction[j * 8 + i]);
                in_vec[i + order] = e[j * 8 + i];
            }
        }
        let mut se = 0.0f32;
        for v in e {
            se += (v as f32) * (v as f32);
        }
        if se < min {
            min = se;
            optimalp = k;
        }
    }
    for j in 0..2 {
        for i in 0..order {
            in_vec[i] = if j == 0 { state[16 - order + i] } else { inb[8 - order + i] as i32 };
        }
        for i in 0..8 {
            prediction[j * 8 + i] = inner_product(order + i, &table[optimalp][i], &in_vec);
            e[j * 8 + i] = (inb[j * 8 + i] as i32).wrapping_sub(prediction[j * 8 + i]);
            in_vec[i + order] = e[j * 8 + i];
        }
    }
    let mut max = 0i32;
    for v in e {
        let ie = clamp16(v) as i32;
        if ie.abs() > max.abs() {
            max = ie;
        }
    }
    let mut scale = 0i32;
    while scale <= 12 {
        if (-8..=7).contains(&max) {
            break;
        }
        max /= 2;
        scale += 1;
    }
    let save = *state;
    let mut ix = [0i16; 16];
    let (mut n_iter, mut again) = (0, true);
    while n_iter < 2 && again {
        again = false;
        if n_iter == 1 {
            scale += 1;
        }
        if scale > 12 {
            scale = 12;
        }
        for j in 0..2 {
            let base = j * 8;
            for i in 0..order {
                in_vec[i] = if j == 0 { save[16 - order + i] } else { state[8 - order + i] };
            }
            for i in 0..8 {
                prediction[base + i] = inner_product(order + i, &table[optimalp][i], &in_vec);
                let se = (inb[base + i] as i32).wrapping_sub(prediction[base + i]);
                ix[base + i] = qsample(se, scale);
                let cv = clamp16(ix[base + i] as i32) as i32 - ix[base + i] as i32;
                if !(-1..=1).contains(&cv) {
                    again = true;
                }
                ix[base + i] = (ix[base + i] as i32 + cv) as i16;
                in_vec[i + order] = (ix[base + i] as i32).wrapping_mul(1 << scale);
                state[base + i] = prediction[base + i].wrapping_add(in_vec[i + order]);
            }
        }
        n_iter += 1;
    }
    out[0] = ((scale << 4) | (optimalp as i32 & 0xf)) as u8;
    for i in (0..16).step_by(2) {
        out[1 + i / 2] = (((ix[i] & 0xf) << 4) | (ix[i + 1] & 0xf)) as u8;
    }
}

fn permute(out: &mut [i16; 16], inp: &[i32; 16], scale: i32, rng: &mut Rng) {
    for i in 0..16 {
        out[i] = clamp16(inp[i].wrapping_sub(scale / 2).wrapping_add(rng.next() % (scale + 1)));
    }
}

/// `write_aifc` + `aifc_decode`: the PCM AIFF the decomp extracts for one sample.
pub fn decode_to_aiff(entry: &Entry) -> Result<Vec<u8>, String> {
    let mut data = entry.data.clone();
    if data.len() % 9 != 0 {
        return Err("sample length is not a multiple of 9".into());
    }
    if data.len() % 2 == 1 {
        data.push(0);
    }
    let mut n_samples = data.len() * 16 / 9;
    if n_samples % 16 != 0 {
        n_samples -= 1;
    }
    let (order, npred) = (entry.order as usize, entry.npredictors as usize);
    let table = codebook(&entry.book, order, npred);
    let mut rng = Rng(1619236481962341);
    let mut state = [0i32; 16];
    let mut out: Vec<u8> = Vec::with_capacity(n_samples * 2);
    let mut pos = 0usize;
    let mut input_at = 0usize;
    while pos < n_samples {
        let input: [u8; 9] = data.get(input_at..input_at + 9).ok_or("truncated sample")?.try_into().unwrap();
        input_at += 9;
        let last_state = state;
        decode_frame(&input, &mut state, order, &table);
        let decoded = state;
        let mut orig = [0i16; 16];
        for i in 0..16 {
            orig[i] = clamp16(state[i]);
        }
        state = last_state;
        let mut guess = orig;
        let mut encoded = [0u8; 9];
        encode_frame(&mut encoded, &guess, &mut state, &table, order, npred);
        if input != encoded {
            let scale = 1i32 << (input[0] >> 4);
            loop {
                permute(&mut guess, &decoded, scale, &mut rng);
                state = last_state;
                encode_frame(&mut encoded, &guess, &mut state, &table, order, npred);
                if input == encoded {
                    break;
                }
            }
            let mut failures = 0;
            while failures < 50 {
                let ind = (rng.next() % 16) as usize;
                let old = guess[ind];
                if old == orig[ind] {
                    failures += 1;
                    continue;
                }
                guess[ind] = orig[ind];
                if rng.next() % 2 != 0 {
                    guess[ind] = (guess[ind] as i32 + (old as i32 - orig[ind] as i32) / 2) as i16;
                }
                state = last_state;
                encode_frame(&mut encoded, &guess, &mut state, &table, order, npred);
                if input == encoded {
                    failures = -1;
                } else {
                    guess[ind] = old;
                }
                failures += 1;
            }
        }
        state = decoded;
        for g in guess {
            out.extend_from_slice(&g.to_be_bytes());
        }
        pos += 16;
    }
    // ---- AIFF output ----
    let mut f: Vec<u8> = b"FORM\0\0\0\0AIFF".to_vec();
    let chunk = |f: &mut Vec<u8>, id: &[u8; 4], body: &[u8]| {
        f.extend_from_slice(id);
        f.extend_from_slice(&(body.len() as u32).to_be_bytes());
        f.extend_from_slice(body);
    };
    let mut comm = Vec::new();
    comm.extend_from_slice(&1i16.to_be_bytes());
    comm.extend_from_slice(&(n_samples as u32).to_be_bytes());
    comm.extend_from_slice(&16i16.to_be_bytes());
    comm.extend_from_slice(&f80(sample_rate(entry)));
    chunk(&mut f, b"COMM", &comm);
    let (ls, le, lc, _) = &entry.loop_;
    if *lc != 0 {
        let mut mark = vec![0, 2];
        for (id, pos, name) in [(1u16, *ls, &b"start"[..]), (2, *le, &b"end"[..])] {
            mark.extend_from_slice(&id.to_be_bytes());
            mark.extend_from_slice(&(pos >> 16).to_be_bytes()[2..]);
            mark.extend_from_slice(&(pos & 0xffff).to_be_bytes()[2..]);
            mark.push(name.len() as u8);
            mark.extend_from_slice(name);
        }
        chunk(&mut f, b"MARK", &mark);
        let mut inst = vec![0u8; 8];
        for v in [1u16, 1, 2, 0, 0, 0] {
            inst.extend_from_slice(&v.to_be_bytes());
        }
        chunk(&mut f, b"INST", &inst);
    }
    let mut appl = b"stoc\x0bVADPCMCODES".to_vec();
    for v in [1i16, order as i16, npred as i16] {
        appl.extend_from_slice(&v.to_be_bytes());
    }
    for i in 0..npred {
        for j in 0..order {
            for k in 0..8 {
                appl.extend_from_slice(&(table[i][k][j] as i16).to_be_bytes());
            }
        }
    }
    chunk(&mut f, b"APPL", &appl);
    let mut ssnd = vec![0u8; 8];
    ssnd.extend_from_slice(&out);
    chunk(&mut f, b"SSND", &ssnd);
    let size = (f.len() - 8) as u32;
    f[4..8].copy_from_slice(&size.to_be_bytes());
    Ok(f)
}

// ---- export-audio.py ----

pub struct WavInfo {
    pub wav: Vec<u8>,
    pub loop_: Option<(f64, f64)>,
    pub frames: u32,
    pub rate: u32,
    pub instrument_hex: String,
    pub markers_hex: String,
}

/// Python `round()` (half to even) for the sample rate.
fn py_round(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        r - x.signum()
    } else {
        r
    }
}

pub fn aiff_to_wav(src: &[u8]) -> Result<WavInfo, String> {
    if src.get(..4) != Some(b"FORM") || src.get(8..12) != Some(b"AIFF") {
        return Err("not a PCM AIFF".into());
    }
    let mut chunks: BTreeMap<[u8; 4], &[u8]> = BTreeMap::new();
    let mut o = 12;
    while o + 8 <= src.len() {
        let tag: [u8; 4] = src[o..o + 4].try_into().unwrap();
        let size = u32::from_be_bytes(src[o + 4..o + 8].try_into().unwrap()) as usize;
        chunks.insert(tag, &src[o + 8..(o + 8 + size).min(src.len())]);
        o += 8 + size + (size & 1);
    }
    let comm = chunks.get(b"COMM").ok_or("no COMM")?;
    let (ch, frames, bits) = (i16::from_be_bytes(comm[0..2].try_into().unwrap()), u32::from_be_bytes(comm[2..6].try_into().unwrap()), i16::from_be_bytes(comm[6..8].try_into().unwrap()));
    let exponent = u16::from_be_bytes(comm[8..10].try_into().unwrap()) as i32;
    let mantissa = u64::from_be_bytes(comm[10..18].try_into().unwrap());
    let rate = py_round((mantissa as f64) * 2f64.powi((exponent & 0x7fff) - 16383 - 63));
    if ch != 1 || bits != 16 || !(1000.0..=192000.0).contains(&rate) {
        return Err("unexpected audio format".into());
    }
    let rate = rate as u32;
    let ssnd = chunks.get(b"SSND").ok_or("no SSND")?;
    let skip = u32::from_be_bytes(ssnd[0..4].try_into().unwrap()) as usize;
    let pcm = ssnd.get(8 + skip..8 + skip + frames as usize * 2).ok_or("truncated sample")?;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    for c in pcm.chunks_exact(2) {
        wav.extend_from_slice(&[c[1], c[0]]);
    }
    let mut loop_ = None;
    if let (Some(mark), Some(inst)) = (chunks.get(b"MARK"), chunks.get(b"INST")) {
        let mut marks = BTreeMap::new();
        let mut pos = 2usize;
        for _ in 0..u16::from_be_bytes(mark[0..2].try_into().unwrap()) {
            let id = u16::from_be_bytes(mark[pos..pos + 2].try_into().unwrap());
            let frame = u32::from_be_bytes(mark[pos + 2..pos + 6].try_into().unwrap());
            let len = mark[pos + 6] as usize;
            marks.insert(id as i32, frame);
            pos += 6 + ((len + 2) & !1);
        }
        let (mode, begin, end) = (i16::from_be_bytes(inst[8..10].try_into().unwrap()), i16::from_be_bytes(inst[10..12].try_into().unwrap()), i16::from_be_bytes(inst[12..14].try_into().unwrap()));
        if mode == 1 {
            if let (Some(b), Some(e)) = (marks.get(&(begin as i32)), marks.get(&(end as i32))) {
                loop_ = Some((*b as f64 / rate as f64, *e as f64 / rate as f64));
            }
        }
    }
    let hex = |b: Option<&&[u8]>| b.map(|b| crate::hash::hex(b)).unwrap_or_default();
    Ok(WavInfo { wav, loop_, frames, rate, instrument_hex: hex(chunks.get(b"INST")), markers_hex: hex(chunks.get(b"MARK")) })
}

/// Python `repr(float)` for the values that appear in the manifest.
pub fn py_float(x: f64) -> String {
    let s = format!("{x:?}"); // shortest round-trip, like repr(); exponent forms only differ outside 1e-4..1e16
    s
}
