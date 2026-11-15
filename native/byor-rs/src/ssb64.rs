//! Port of `scripts/export-ssb64.py`: the Super Smash Bros. (US) fighters as skinned glTF with every motion, read from the
//! user's ROM through the relocData file table, with names from the decomp (`reference/ssb64`, see native/SSB64.md).
//! Writes `public/models/ssb64/<fighter>.glb` and `public/models/ssb64/fighters.json`, byte-identical to the Python exporter.
use crate::pyc::{dumps, re, read, R};
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const SHA1: &str = "e2929e10fccc0aa84e5776227e798abc07cedabf";
const TABLE: usize = 0x1AC870;
const FILE_COUNT: usize = 2132;
const DOBJ_ARRAY_MAX: i32 = 18;
const ATTR_SETUP_PARTS: usize = 0x29C;
const ATTR_COMMONPARTS: usize = 0x2D4;
pub const UNIT: f64 = 0.004;
const FPS: f64 = 60.0;
pub const FIGHTERS: [&str; 13] = ["Mario", "Fox", "Donkey", "Samus", "Luigi", "Link", "Yoshi", "Captain", "Kirby", "Pikachu", "Purin", "Ness", "Boss"];

fn label(name: &str) -> &'static str {
    match name {
        "Mario" => "Mario",
        "Fox" => "Fox",
        "Donkey" => "Donkey Kong",
        "Samus" => "Samus",
        "Luigi" => "Luigi",
        "Link" => "Link",
        "Yoshi" => "Yoshi",
        "Captain" => "Captain Falcon",
        "Kirby" => "Kirby",
        "Pikachu" => "Pikachu",
        "Purin" => "Jigglypuff",
        "Ness" => "Ness",
        _ => "Master Hand",
    }
}

/// Where the SSB64 decomp checkout lives inside a repo checkout or a BYOR bundle (`byor-src`).
pub fn decomp_dir(source: &Path) -> PathBuf {
    for base in ["reference/ssb64", "ssb64"] {
        let p = source.join(base);
        if p.join("src/ft/ftdata.c").is_file() {
            return p;
        }
    }
    source.join("reference/ssb64")
}

// The platform libm, like CPython's `math` module.
pub(crate) mod libm {
    extern "C" {
        fn sin(x: f64) -> f64;
        fn cos(x: f64) -> f64;
    }
    pub fn sin_(x: f64) -> f64 {
        unsafe { sin(x) }
    }
    pub fn cos_(x: f64) -> f64 {
        unsafe { cos(x) }
    }
}

fn be16(d: &[u8], o: usize) -> R<u16> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| format!("read outside the data at {o:#x}"))
}
fn be32(d: &[u8], o: usize) -> R<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("read outside the data at {o:#x}"))
}
fn bef32(d: &[u8], o: usize) -> R<f64> {
    Ok(f32::from_bits(be32(d, o)?) as f64)
}
pub(crate) fn f32r(x: f64) -> f64 {
    x as f32 as f64
}

// ------------------------------------------------------------------------------------------------ relocData files

/// `syDmaDecodeVpk0` (src/sys/dma.c).
pub fn vpk0(src: &[u8]) -> R<Vec<u8>> {
    if src.get(..4) != Some(b"vpk0") {
        return Err("not vpk0 data".into());
    }
    #[derive(Clone)]
    enum Node {
        Leaf(u32),
        Pair(Box<Node>, Box<Node>),
    }
    struct Bits<'a> {
        src: &'a [u8],
        pos: usize,
        buf: u64,
        n: u32,
    }
    impl Bits<'_> {
        fn get(&mut self, n: u32) -> R<u32> {
            while self.n < n {
                let hi = *self.src.get(self.pos).ok_or("vpk0: truncated")? as u64;
                let lo = *self.src.get(self.pos + 1).ok_or("vpk0: truncated")? as u64;
                self.buf = ((self.buf << 16) | (hi << 8) | lo) & 0xFFFF_FFFF_FFFF;
                self.pos += 2;
                self.n += 16;
            }
            self.n -= n;
            Ok(((self.buf >> self.n) & ((1u64 << n) - 1)) as u32)
        }
        fn tree(&mut self) -> R<Node> {
            let mut stack: Vec<Node> = vec![];
            loop {
                if self.get(1)? != 0 {
                    if stack.len() < 2 {
                        break;
                    }
                    let r = stack.pop().unwrap();
                    let l = stack.pop().unwrap();
                    stack.push(Node::Pair(Box::new(l), Box::new(r)));
                } else {
                    stack.push(Node::Leaf(self.get(8)?));
                }
            }
            Ok(stack.into_iter().next().unwrap_or(Node::Leaf(0)))
        }
        fn read(&mut self, t: &Node) -> R<u32> {
            let mut t = t;
            loop {
                match t {
                    Node::Leaf(n) => return self.get(*n),
                    Node::Pair(l, r) => t = if self.get(1)? != 0 { r } else { l },
                }
            }
        }
    }
    let mut b = Bits { src, pos: 0, buf: 0, n: 0 };
    b.get(32)?;
    let size = b.get(32)? as usize;
    let method = b.get(8)?;
    let (offsets, lengths) = (b.tree()?, b.tree()?);
    let mut out: Vec<u8> = Vec::with_capacity(size);
    while out.len() < size {
        if b.get(1)? == 0 {
            out.push(b.get(8)? as u8);
            continue;
        }
        let mut start = if method != 0 {
            let mut extra = 0i64;
            let mut v = b.read(&offsets)? as i64;
            if v <= 2 {
                extra = v + 1;
                v = b.read(&offsets)? as i64;
            }
            out.len() as i64 - v * 4 - extra + 8
        } else {
            out.len() as i64 - b.read(&offsets)? as i64
        };
        for _ in 0..b.read(&lengths)? {
            let c = *out.get(usize::try_from(start).map_err(|_| "vpk0: bad offset")?).ok_or("vpk0: bad offset")?;
            out.push(c);
            start += 1;
        }
    }
    out.truncate(size);
    Ok(out)
}

pub type Ref = (usize, usize);

pub struct File {
    pub data: Vec<u8>,
    pub ptrs: HashMap<usize, Ref>,
}

/// `lbRelocLoadAndRelocFile`: file bytes plus every pointer slot resolved to (file id, byte offset).
pub struct Files<'a> {
    rom: &'a [u8],
    data_start: usize,
    cache: RefCell<HashMap<usize, Rc<File>>>,
}

impl<'a> Files<'a> {
    pub fn new(rom: &'a [u8]) -> Self {
        Files { rom, data_start: TABLE + (FILE_COUNT + 1) * 12, cache: RefCell::new(HashMap::new()) }
    }

    pub fn get(&self, fid: usize) -> R<Rc<File>> {
        if let Some(f) = self.cache.borrow().get(&fid) {
            return Ok(f.clone());
        }
        let e = TABLE + fid * 12;
        let word = be32(self.rom, e)? as usize;
        let (intern, csize, extern_, dsize) = (be16(self.rom, e + 4)?, be16(self.rom, e + 6)? as usize, be16(self.rom, e + 8)?, be16(self.rom, e + 10)? as usize);
        let start = self.data_start + (word & 0x7FFF_FFFF);
        let raw = self.rom.get(start..start + csize * 4).ok_or_else(|| format!("file {fid} outside the ROM"))?;
        let data = if word >> 31 != 0 { vpk0(raw)? } else { raw.get(..dsize * 4).ok_or("short file")?.to_vec() };
        if data.len() != dsize * 4 {
            return Err(format!("file {fid}: {} bytes, expected {}", data.len(), dsize * 4));
        }
        let mut ptrs = HashMap::new();
        let mut r = intern;
        while r != 0xFFFF {
            let w = be32(&data, r as usize * 4)?;
            ptrs.insert(r as usize * 4, (fid, (w & 0xFFFF) as usize * 4));
            r = (w >> 16) as u16;
        }
        let ids = start + csize * 4;
        let (mut r, mut k) = (extern_, 0);
        while r != 0xFFFF {
            let w = be32(&data, r as usize * 4)?;
            ptrs.insert(r as usize * 4, (be16(self.rom, ids + 2 * k)? as usize, (w & 0xFFFF) as usize * 4));
            r = (w >> 16) as u16;
            k += 1;
        }
        let f = Rc::new(File { data, ptrs });
        self.cache.borrow_mut().insert(fid, f.clone());
        Ok(f)
    }

    /// The pointer stored at `r` + `off`, or None.
    pub fn ptr(&self, r: Option<Ref>, off: usize) -> R<Option<Ref>> {
        match r {
            None => Ok(None),
            Some((f, o)) => Ok(self.get(f)?.ptrs.get(&(o + off)).copied()),
        }
    }
}

// ------------------------------------------------------------------------------------------------ decomp tables

/// tools/relocFileDescriptions.us.txt: `-NNN: Name` lines -> {name: id}.
pub fn file_names(decomp: &Path) -> R<HashMap<String, usize>> {
    let mut names = HashMap::new();
    let line_re = re(r"^-(\d+): (\w+)");
    for line in read(&decomp.join("tools/relocFileDescriptions.us.txt"))?.split('\n') {
        if line.starts_with("# FILE CONTENTS") {
            break;
        }
        if let Some(c) = line_re.captures(line) {
            names.insert(c[2].to_string(), c[1].parse().unwrap());
        }
    }
    Ok(names)
}

/// src/ft/ftdef.h enum FTCommonMotion: motion id -> name.
pub fn common_motions(decomp: &Path) -> R<BTreeMap<i64, String>> {
    let text = read(&decomp.join("src/ft/ftdef.h"))?;
    let body = &text[text.find("typedef enum FTCommonMotion").ok_or("FTCommonMotion")?..];
    let body = &body[body.find('{').ok_or("{")? + 1..body.find('}').ok_or("}")?];
    let mut out: BTreeMap<i64, String> = BTreeMap::new();
    let mut value: i64 = -1;
    let item = re(r"(?s)^nFTCommonMotion(\w+)\s*(?:=\s*(\S+))?$");
    let int = re(r"^-?\d+$");
    let comment = re(r"//.*");
    for raw in body.split(',') {
        let line = comment.replace_all(raw, "");
        let line = line.trim();
        let Some(m) = item.captures(line) else { continue };
        match m.get(2) {
            None => value += 1,
            Some(v) if int.is_match(v.as_str()) => value = v.as_str().parse().unwrap(),
            Some(v) => {
                let target = v.as_str().replace("nFTCommonMotion", "");
                if target.ends_with("Start") && out.get(&value) == Some(&target) {
                    out.insert(value, m[1].to_string());
                }
                continue;
            }
        }
        out.entry(value).or_insert_with(|| m[1].to_string());
    }
    Ok(out)
}

pub struct Table {
    pub main: usize,
    pub model: usize,
    pub attr: usize,
    pub motions: Vec<Option<String>>,
    pub specials: Vec<String>,
}

/// src/ft/ftchar/ft<name>/ft<name>.h enum FT<Name>Motion: the names of the motions from nFTCommonMotionSpecialStart on.
pub fn special_motions(decomp: &Path, name: &str) -> R<Vec<String>> {
    let low = name.to_lowercase();
    let text = read(&decomp.join(format!("src/ft/ftchar/ft{low}/ft{low}.h")))?;
    let m = re(&format!(r"nFT{name}Motion\w+\s*=\s*nFTCommonMotionSpecialStart")).find(&text).ok_or_else(|| format!("FT{name}Motion"))?;
    let body = &text[m.start()..];
    let body = &body[..body.find('}').ok_or("enum end")?];
    let item = re(&format!(r"(?s)^nFT{name}Motion(\w+)\s*(?:=\s*(\S+))?$"));
    let comment = re(r"//.*");
    let mut out: Vec<String> = vec![];
    for raw in body.split(',') {
        let line = comment.replace_all(raw, "");
        if let Some(c) = item.captures(line.trim()) {
            if out.is_empty() || c.get(2).is_none() {
                out.push(c[1].to_string());
            }
        }
    }
    Ok(out)
}

/// src/ft/ftdata.c: FTData and the main motion descriptor table per fighter.
pub fn fighter_tables(decomp: &Path, names: &HashMap<String, usize>) -> R<HashMap<String, Table>> {
    let text = read(&decomp.join("src/ft/ftdata.c"))?;
    let mut out = HashMap::new();
    let sym = re(r"^&ll(\w+)FileID");
    let strip = re(r"(?s)//[^\n]*|/\*.*?\*/|[{}]");
    for f in FIGHTERS {
        let m = re(&format!(r"(?s)FTData dFT{f}Data =\s*\{{(.*?)\}};")).captures(&text).ok_or_else(|| format!("FTData for {f}"))?;
        let fields: Vec<String> = m[1].split(',').map(|x| x.trim().to_string()).collect();
        let id = |s: &str| -> R<usize> {
            let c = sym.captures(s).ok_or_else(|| format!("file id {s}"))?;
            names.get(&c[1]).copied().ok_or_else(|| format!("no file {}", &c[1]))
        };
        let (main, model) = (id(&fields[0])?, id(&fields[3])?);
        let attr = usize::from_str_radix(fields[24].trim_start_matches("0x"), 16).map_err(|e| e.to_string())?;
        let tname = regex::escape(fields[25].trim_start_matches('&'));
        let table = re(&format!(r"(?s)FTMotionDesc {tname}\[\]\s*=\s*\{{(.*?)\n\}};")).captures(&text).ok_or("motion table")?[1].to_string();
        let flat = strip.replace_all(&table, " ");
        let items: Vec<&str> = flat.split(',').map(str::trim).filter(|x| !x.is_empty()).collect();
        let mut motions = vec![];
        let mut k = 0;
        while k + 2 < items.len() {
            let special = items[k + 2].contains("ANIMJOINT") || items[k + 2].contains("SHIELDPOSE");
            motions.push(sym.captures(items[k]).filter(|_| !special).map(|c| c[1].to_string()));
            k += 3;
        }
        out.insert(f.to_string(), Table { main, model, attr, motions, specials: special_motions(decomp, f)? });
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ math

pub(crate) type M4 = [[f64; 4]; 4];

pub(crate) fn mat_trs(t: [f64; 3], r: [f64; 3], s: [f64; 3]) -> M4 {
    let (cr, sr) = (libm::cos_(r[0]), libm::sin_(r[0]));
    let (cp, sp) = (libm::cos_(r[1]), libm::sin_(r[1]));
    let (cy, sy) = (libm::cos_(r[2]), libm::sin_(r[2]));
    let m = [
        [cy * cp, cy * sp * sr - sy * cr, cy * sp * cr + sy * sr],
        [sy * cp, sy * sp * sr + cy * cr, sy * sp * cr - cy * sr],
        [-sp, cp * sr, cp * cr],
    ];
    [
        [m[0][0] * s[0], m[0][1] * s[1], m[0][2] * s[2], t[0]],
        [m[1][0] * s[0], m[1][1] * s[1], m[1][2] * s[2], t[1]],
        [m[2][0] * s[0], m[2][1] * s[1], m[2][2] * s[2], t[2]],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub(crate) fn mat_mul(a: &M4, b: &M4) -> M4 {
    let mut o = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            o[i][j] = (0..4).fold(0.0, |acc, k| acc + a[i][k] * b[k][j]);
        }
    }
    o
}

fn mat_inv(m: &M4) -> M4 {
    let a = m;
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    let inv = [
        [(a[1][1] * a[2][2] - a[1][2] * a[2][1]) / det, (a[0][2] * a[2][1] - a[0][1] * a[2][2]) / det, (a[0][1] * a[1][2] - a[0][2] * a[1][1]) / det],
        [(a[1][2] * a[2][0] - a[1][0] * a[2][2]) / det, (a[0][0] * a[2][2] - a[0][2] * a[2][0]) / det, (a[0][2] * a[1][0] - a[0][0] * a[1][2]) / det],
        [(a[1][0] * a[2][1] - a[1][1] * a[2][0]) / det, (a[0][1] * a[2][0] - a[0][0] * a[2][1]) / det, (a[0][0] * a[1][1] - a[0][1] * a[1][0]) / det],
    ];
    let t: Vec<f64> = (0..3).map(|i| -(0..3).fold(0.0, |acc, k| acc + inv[i][k] * m[k][3])).collect();
    [[inv[0][0], inv[0][1], inv[0][2], t[0]], [inv[1][0], inv[1][1], inv[1][2], t[1]], [inv[2][0], inv[2][1], inv[2][2], t[2]], [0.0, 0.0, 0.0, 1.0]]
}

pub(crate) fn apply(m: &M4, v: [f64; 3], w: f64) -> [f64; 3] {
    let f = |i: usize| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2] + m[i][3] * w;
    [f(0), f(1), f(2)]
}

pub(crate) fn quat(r: [f64; 3]) -> [f64; 4] {
    let (cx, sx) = (libm::cos_(r[0] / 2.0), libm::sin_(r[0] / 2.0));
    let (cy, sy) = (libm::cos_(r[1] / 2.0), libm::sin_(r[1] / 2.0));
    let (cz, sz) = (libm::cos_(r[2] / 2.0), libm::sin_(r[2] / 2.0));
    [sx * cy * cz - cx * sy * sz, cx * sy * cz + sx * cy * sz, cx * cy * sz - sx * sy * cz, cx * cy * cz + sx * sy * sz]
}

// ------------------------------------------------------------------------------------------------ textures

fn rgba16(v: u32) -> [u8; 4] {
    [((v >> 11 & 31) * 255 / 31) as u8, ((v >> 6 & 31) * 255 / 31) as u8, ((v >> 1 & 31) * 255 / 31) as u8, if v & 1 != 0 { 255 } else { 0 }]
}

fn ia16(c: u32) -> [u8; 4] {
    [(c >> 8) as u8, (c >> 8) as u8, (c >> 8) as u8, (c & 0xFF) as u8]
}

fn tile_axis(size: i64, mask: i64, mode: i64) -> (Vec<i64>, i64) {
    let (clamp, mirror) = (mode & 2 != 0, mode & 1 != 0);
    if mask != 0 && !clamp {
        return ((0..1i64 << mask).collect(), if mirror { 33648 } else { 10497 });
    }
    let out = (0..size)
        .map(|i| {
            if mask == 0 {
                return i;
            }
            let v = i & ((1 << mask) - 1);
            if mirror && (i >> mask) & 1 != 0 {
                (1 << mask) - 1 - v
            } else {
                v
            }
        })
        .collect();
    (out, if clamp { 33071 } else if mirror { 33648 } else { 10497 })
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Tile {
    fmt: i64,
    siz: i64,
    line: i64,
    tmem: i64,
    palette: i64,
    cms: i64,
    cmt: i64,
    masks: i64,
    maskt: i64,
    shifts: i64,
    shiftt: i64,
    uls: i64,
    ult: i64,
    lrs: i64,
    lrt: i64,
}

fn decode_tile(tmem: &[u8; 4096], t: &Tile, xs: &[i64], ys: &[i64], tlut: i64) -> Vec<u8> {
    let (stride, base) = (t.line * 8, t.tmem * 8);
    let at = |p: i64| tmem[(p & 0xFFF) as usize] as u32;
    let mut out = Vec::with_capacity(xs.len() * ys.len() * 4);
    for &y in ys {
        let row = base + y * stride;
        for &x in xs {
            let px: [u8; 4] = match t.siz {
                0 => {
                    let b = at(row + x / 2);
                    let v = if x % 2 == 0 { b >> 4 } else { b & 15 };
                    match t.fmt {
                        2 => {
                            let p = 0x800 + (t.palette * 16 + v as i64) * 2;
                            let c = at(p) << 8 | at(p + 1);
                            if tlut == 3 { ia16(c) } else { rgba16(c) }
                        }
                        3 => {
                            let i = ((v >> 1) * 255 / 7) as u8;
                            [i, i, i, if v & 1 != 0 { 255 } else { 0 }]
                        }
                        _ => [(v * 17) as u8; 4],
                    }
                }
                1 => {
                    let v = at(row + x);
                    match t.fmt {
                        2 => {
                            let p = 0x800 + v as i64 * 2;
                            let c = at(p) << 8 | at(p + 1);
                            if tlut == 3 { ia16(c) } else { rgba16(c) }
                        }
                        3 => {
                            let i = ((v >> 4) * 17) as u8;
                            [i, i, i, ((v & 15) * 17) as u8]
                        }
                        _ => [v as u8; 4],
                    }
                }
                2 => {
                    let p = row + x * 2;
                    let c = at(p) << 8 | at(p + 1);
                    if t.fmt == 3 { ia16(c) } else { rgba16(c) }
                }
                _ => {
                    let p = base + y * stride * 2 + x * 4;
                    [at(p) as u8, at(p + 1) as u8, at(p + 2) as u8, at(p + 3) as u8]
                }
            };
            out.extend_from_slice(&px);
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ colour combiner

fn cycles(mode: (u32, u32)) -> [[u32; 8]; 2] {
    let (w0, w1) = mode;
    [
        [(w0 >> 20) & 15, (w1 >> 28) & 15, (w0 >> 15) & 31, (w1 >> 15) & 7, (w0 >> 12) & 7, (w1 >> 12) & 7, (w0 >> 9) & 7, (w1 >> 9) & 7],
        [(w0 >> 5) & 15, (w1 >> 24) & 15, w0 & 31, (w1 >> 6) & 7, (w1 >> 21) & 7, (w1 >> 3) & 7, (w1 >> 18) & 7, w1 & 7],
    ]
}

#[allow(clippy::too_many_arguments)]
fn combine(mode: (u32, u32), two: bool, t0: [f64; 4], t1: [f64; 4], prim: [f64; 4], shade: [f64; 4], env: [f64; 4], prim_lod: f64) -> [f64; 4] {
    let cyc = cycles(mode);
    let mut comb = [0.0; 4];
    for c in &cyc[..if two { 2 } else { 1 }] {
        let [a, b, cc, d, aa, ab, ac, ad] = *c;
        let src = |sel: u32| -> Option<[f64; 4]> {
            match sel {
                0 => Some(comb),
                1 => Some(t0),
                2 => Some(t1),
                3 => Some(prim),
                4 => Some(shade),
                5 => Some(env),
                _ => None,
            }
        };
        let col = |sel: u32, kind: u8| -> f64x3 {
            if let Some(s) = src(sel) {
                return [s[0], s[1], s[2]];
            }
            match kind {
                b'a' | b'd' => [if sel == 6 { 1.0 } else { 0.0 }; 3],
                b'b' => [0.0; 3],
                _ => match sel {
                    7 => [comb[3]; 3],
                    8 => [t0[3]; 3],
                    9 => [t1[3]; 3],
                    10 => [prim[3]; 3],
                    11 => [shade[3]; 3],
                    12 => [env[3]; 3],
                    14 => [prim_lod; 3],
                    _ => [0.0; 3],
                },
            }
        };
        let alp = |sel: u32, is_c: bool| -> f64 {
            if is_c {
                if sel == 0 {
                    return 0.0;
                }
                if sel == 6 {
                    return prim_lod;
                }
            } else if sel == 0 {
                return comb[3];
            }
            if (1..=5).contains(&sel) {
                return src(sel).unwrap()[3];
            }
            if sel == 6 && !is_c {
                return 1.0;
            }
            0.0
        };
        let (ca, cb, ccv, cd) = (col(a, b'a'), col(b, b'b'), col(cc, b'c'), col(d, b'd'));
        let rgb: Vec<f64> = (0..3).map(|i| ((ca[i] - cb[i]) * ccv[i] + cd[i]).clamp(0.0, 1.0)).collect();
        let al = ((alp(aa, false) - alp(ab, false)) * alp(ac, true) + alp(ad, false)).clamp(0.0, 1.0);
        comb = [rgb[0], rgb[1], rgb[2], al];
    }
    comb
}
#[allow(non_camel_case_types)]
type f64x3 = [f64; 3];

fn uses(mode: (u32, u32), two: bool, inputs: &[u32]) -> bool {
    for c in &cycles(mode)[..if two { 2 } else { 1 }] {
        let [a, b, cc, d, aa, ab, ac, ad] = *c;
        let mut sel = vec![a, b, d, aa, ab, ad, ac];
        if cc <= 5 {
            sel.push(cc);
        } else if (8..=12).contains(&cc) {
            sel.push(cc - 7);
        }
        if sel.iter().any(|s| inputs.contains(s)) {
            return true;
        }
    }
    false
}

// ------------------------------------------------------------------------------------------------ display lists

const G_LIGHTING: u32 = 0x0002_0000;
pub(crate) const G_CULL_BACK: u32 = 0x0000_0400;
const G_CULL_FRONT: u32 = 0x0000_0200;
const G_TEXTURE_GEN: u32 = 0x0004_0000;

#[derive(Clone)]
struct Cached {
    joint: usize,
    pos: [f64; 3],
    nrm: Option<[f64; 3]>,
    st: [i64; 2],
    col: Option<[f64; 4]>,
}

#[derive(Clone)]
#[allow(dead_code)] // fmt / block_fmt mirror the MObjSub layout; the TMEM model does not need them
pub(crate) struct MObj {
    fmt: u8,
    siz: u8,
    sprites: Option<Ref>,
    unk08: u16,
    unk0a: u16,
    unk0c: u16,
    unk0e: u16,
    unk10: i32,
    trau: f64,
    trav: f64,
    scau: f64,
    scav: f64,
    palettes: Option<Ref>,
    palette: Option<Ref>,
    flags: u16,
    block_fmt: u8,
    block_siz: u8,
    block_dxt: u16,
    unk36: u16,
    prim: [f64; 4],
    lfrac: f64,
    env: [f64; 4],
    light1: [f64; 3],
    light2: [f64; 3],
    texture_id: i64,
    palette_id: Option<i64>,
}

struct Timg {
    addr: Option<Ref>,
    siz: i64,
    width: i64,
}

type Uv = (i64, i64, i64, i64, f64, f64, (f64, f64));

pub(crate) struct Material {
    uv: Option<Uv>,
    pub(crate) vcol: bool,
    pub(crate) double: bool,
    pub(crate) image: Option<(usize, usize, Vec<u8>)>,
    pub(crate) wrap: (i64, i64),
    pub(crate) alpha: &'static str,
    pub(crate) color: [f64; 4],
}

pub(crate) struct Vert {
    pub(crate) joint: usize,
    pub(crate) pos: [f64; 3],
    pub(crate) nrm: Option<[f64; 3]>,
    pub(crate) uv: [f64; 2],
    pub(crate) col: Option<[f64; 4]>,
}

pub(crate) struct Mesh<'f, 'a> {
    files: &'f Files<'a>,
    cache: Vec<Option<Cached>>,
    pub(crate) verts: Vec<Vert>,
    pub(crate) tris: Vec<Vec<usize>>,
    keys: Vec<Vec<u8>>,
    pub(crate) materials: Vec<Material>,
    tmem: [u8; 4096],
    tiles: [Tile; 8],
    timg: Option<Timg>,
    pub(crate) geom: u32,
    combine: (u32, u32),
    prim: [f64; 4],
    prim_lod: f64,
    env: [f64; 4],
    pub(crate) light1: [f64; 3],
    light2: [f64; 3],
    tex_on: bool,
    tex_scale: (f64, f64),
    tex_tile: usize,
    other_h: u32,
    pub(crate) mobjs: Vec<MObj>,
    pub(crate) unknown: std::collections::BTreeSet<String>,
    gen: u64,
    last: Option<(u64, usize)>,
}

fn color4(w1: u32) -> [f64; 4] {
    [(w1 >> 24) as f64 / 255.0, (w1 >> 16 & 255) as f64 / 255.0, (w1 >> 8 & 255) as f64 / 255.0, (w1 & 255) as f64 / 255.0]
}

fn push_f(k: &mut Vec<u8>, xs: &[f64]) {
    for x in xs {
        k.extend_from_slice(&x.to_bits().to_le_bytes());
    }
}

impl<'f, 'a> Mesh<'f, 'a> {
    pub(crate) fn new(files: &'f Files<'a>) -> Self {
        Mesh {
            files,
            cache: vec![None; 64],
            verts: vec![],
            tris: vec![],
            keys: vec![],
            materials: vec![],
            tmem: [0; 4096],
            tiles: [Tile::default(); 8],
            timg: None,
            geom: 0x1 | 0x4 | G_CULL_BACK | G_LIGHTING | 0x0020_0000,
            combine: (0, 0),
            prim: [1.0; 4],
            prim_lod: 0.0,
            env: [1.0; 4],
            light1: [1.0; 3],
            light2: [0.3; 3],
            tex_on: false,
            tex_scale: (1.0, 1.0),
            tex_tile: 0,
            other_h: 1 << 20,
            mobjs: vec![],
            unknown: Default::default(),
            gen: 0,
            last: None,
        }
    }

    fn mobj_dl(&mut self, index: usize) -> R<()> {
        let Some(m) = self.mobjs.get(index).cloned() else { return Ok(()) };
        let flags = if m.flags != 0 { m.flags as u32 } else { 0x80 | 0x20 | 0x01 };
        let (mut scau, scav, mut trau, trav) = (m.scau, m.scav, m.trau, m.trav);
        if m.unk10 == 1 && flags & (0x80 | 0x40 | 0x20) != 0 {
            scau *= 0.5;
            trau = ((trau - m.trau) + 1.0 - (m.scau * 0.5)) * 0.5;
        }
        if flags & 0x04 != 0 && m.palette.is_some() {
            self.timg = Some(Timg { addr: m.palette, siz: 2, width: 1 });
            if flags & 0x03 != 0 {
                let t = &mut self.tiles[5];
                (t.fmt, t.siz, t.line, t.tmem, t.palette) = (0, 0, 0, 0x100, 0);
                self.load_tlut(5, if m.siz == 1 { 0xFF } else { 0xF })?;
            }
        }
        if flags & 0x1000 != 0 {
            self.light1 = m.light1;
        }
        if flags & 0x2000 != 0 {
            self.light2 = m.light2;
        }
        if flags & (0x200 | 0x10 | 0x8) != 0 {
            self.prim = m.prim;
            self.prim_lod = m.lfrac;
        }
        if flags & 0x400 != 0 {
            self.env = m.env;
        }
        if flags & (0x10 | 0x02) != 0 && m.sprites.is_some() {
            self.timg = Some(Timg { addr: self.files.ptr(m.sprites, 0)?, siz: if m.block_siz == 3 { 3 } else { 2 }, width: 1 });
            if flags & (0x10 | 0x01) != 0 {
                let (dxt, rows) = (m.block_dxt as i64, m.unk36 as i64);
                let n = match m.block_siz {
                    0 => ((dxt * rows + 3) >> 2) - 1,
                    1 => ((dxt * rows + 1) >> 1) - 1,
                    _ => dxt * rows - 1,
                };
                self.tiles[6].tmem = 0;
                self.load_block(6, n)?;
            }
        }
        if flags & (0x10 | 0x01) != 0 && m.sprites.is_some() {
            self.timg = Some(Timg { addr: self.files.ptr(m.sprites, 4 * m.texture_id as usize)?, siz: m.siz as i64, width: 1 });
        }
        let tiny = 1.0 / 65535.0;
        if flags & 0x20 != 0 {
            let (c, e, a) = (m.unk0c as f64, m.unk0e as f64, m.unk0a as f64);
            let (uls, ult) = if m.unk10 == 2 {
                let uls = if scau.abs() > tiny { ((c * trau / scau) * 4.0) as i64 } else { 0 };
                let ult = if scav.abs() > tiny { ((e * trav / scav) * 4.0) as i64 } else { 0 };
                (uls.max(0), ult.max(0))
            } else {
                let uls = if scau.abs() > tiny { (((c * trau + a) / scau) * 4.0) as i64 } else { 0 };
                let ult = if scav.abs() > tiny { (((((1.0 - scav) - trav) * e + a) / scav) * 4.0) as i64 } else { 0 };
                (uls, ult)
            };
            let t = &mut self.tiles[0];
            (t.uls, t.ult, t.lrs, t.lrt) = (uls, ult, ((m.unk0c as i64 - 1) << 2) + uls, ((m.unk0e as i64 - 1) << 2) + ult);
        }
        if flags & 0x80 != 0 {
            let (s, t) = if m.unk10 == 2 {
                (
                    if scau.abs() > tiny { ((m.unk0c as f64 * 64.0) / scau) as i64 } else { 0 },
                    if scav.abs() > tiny { ((m.unk0e as f64 * 64.0) / scav) as i64 } else { 0 },
                )
            } else {
                let u8_ = m.unk08 as f64;
                (
                    if scau.abs() > tiny && m.unk08 != 0 { ((2097152.0 / u8_) / scau) as i64 } else { 0 },
                    if scav.abs() > tiny && m.unk08 != 0 { ((2097152.0 / u8_) / scav) as i64 } else { 0 },
                )
            };
            self.tex_scale = (s.min(0xFFFF) as f64 / 65536.0, t.min(0xFFFF) as f64 / 65536.0);
            self.tex_on = true;
        }
        Ok(())
    }

    fn src_bytes(&self, n: usize) -> R<Vec<u8>> {
        let (f, o) = self.timg.as_ref().and_then(|t| t.addr).ok_or("no texture image")?;
        let d = self.files.get(f)?;
        let mut v: Vec<u8> = d.data.get(o.min(d.data.len())..(o + n).min(d.data.len())).unwrap_or(&[]).to_vec();
        v.resize(n, 0);
        Ok(v)
    }

    fn load_block(&mut self, tile: usize, lrs: i64) -> R<()> {
        let siz = self.timg.as_ref().map_or(0, |t| t.siz);
        let n = if siz != 0 { ((lrs + 1) << siz) >> 1 } else { (lrs + 1) / 2 };
        let base = self.tiles[tile].tmem * 8;
        for (i, b) in self.src_bytes(n.max(0) as usize)?.into_iter().enumerate() {
            self.tmem[((base + i as i64) & 0xFFF) as usize] = b;
        }
        Ok(())
    }

    fn load_tile(&mut self, tile: usize, uls: i64, ult: i64, lrs: i64, lrt: i64) -> R<()> {
        let t = self.tiles[tile];
        let timg = self.timg.as_ref().unwrap();
        let (siz, width) = (timg.siz, timg.width);
        let (x0, y0, x1, y1) = (uls >> 2, ult >> 2, lrs >> 2, lrt >> 2);
        let bpp = if siz != 0 { ((4 << siz) / 8) as f64 } else { 0.5 };
        let (f, o) = timg.addr.unwrap();
        let file = self.files.get(f)?;
        let d = &file.data;
        let rowbytes = ((x1 - x0 + 1) as f64 * bpp) as i64;
        for y in y0..=y1 {
            let src = o as i64 + ((y * width + x0) as f64 * bpp) as i64;
            let dst = t.tmem * 8 + (y - y0) * t.line * 8;
            for i in 0..rowbytes {
                self.tmem[((dst + i) & 0xFFF) as usize] = d.get((src + i) as usize).copied().unwrap_or(0);
            }
        }
        Ok(())
    }

    fn load_tlut(&mut self, tile: usize, count: i64) -> R<()> {
        let base = self.tiles[tile].tmem * 8;
        for (i, b) in self.src_bytes(((count + 1) * 2) as usize)?.into_iter().enumerate() {
            self.tmem[((base + i as i64) & 0xFFF) as usize] = b;
        }
        Ok(())
    }

    pub(crate) fn run(&mut self, at: Ref, joint: usize, world: &M4) -> R<()> {
        let file = self.files.get(at.0)?;
        let mut o = at.1;
        loop {
            let (w0, w1) = (be32(&file.data, o)?, be32(&file.data, o + 4)?);
            let op = w0 >> 24;
            let target = file.ptrs.get(&(o + 4)).copied();
            if ![0x01, 0x02, 0x05, 0x06, 0x07, 0xDF, 0xE7, 0xE8, 0xE6, 0xE9, 0x00].contains(&op) {
                self.gen += 1;
            }
            match op {
                0x01 if target.is_none() => {
                    self.unknown.insert("vtx-segment".into());
                }
                0x01 => {
                    let n = ((w0 >> 12) & 0xFF) as usize;
                    let v0 = ((w0 >> 1) & 0x7F) as usize - n;
                    let (tf, to) = target.unwrap();
                    let vf = self.files.get(tf)?;
                    for i in 0..n {
                        let p = to + 16 * i;
                        let d = &vf.data;
                        let s16 = |q: usize| -> R<f64> { Ok(be16(d, q)? as i16 as f64) };
                        let (x, y, z) = (s16(p)?, s16(p + 2)?, s16(p + 4)?);
                        let (s, t) = (be16(d, p + 8)? as i16 as i64, be16(d, p + 10)? as i16 as i64);
                        let c = d.get(p + 12..p + 16).ok_or("vertex outside the file")?;
                        let (nrm, col) = if self.geom & G_LIGHTING != 0 {
                            let nv = [c[0] as i8 as f64 / 127.0, c[1] as i8 as f64 / 127.0, c[2] as i8 as f64 / 127.0];
                            (Some(apply(world, nv, 0.0)), None)
                        } else {
                            (None, Some([c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0, c[3] as f64 / 255.0]))
                        };
                        self.cache[v0 + i] = Some(Cached { joint, pos: apply(world, [x, y, z], 1.0), nrm, st: [s, t], col });
                    }
                }
                0x02 => {
                    let (where_, vi) = ((w0 >> 16) & 0xFF, (w0 & 0xFFFF) as usize / 2);
                    if where_ == 0x14 {
                        if let Some(c) = self.cache[vi].as_mut() {
                            c.st = [(w1 >> 16) as u16 as i16 as i64, (w1 & 0xFFFF) as u16 as i16 as i64];
                        }
                    }
                }
                0x05..=0x07 => {
                    let mut tris = vec![[(w0 >> 16) & 0xFF, (w0 >> 8) & 0xFF, w0 & 0xFF]];
                    if op != 0x05 {
                        tris.push([(w1 >> 16) & 0xFF, (w1 >> 8) & 0xFF, w1 & 0xFF]);
                    }
                    for t in tris {
                        self.triangle([t[0] as usize / 2, t[1] as usize / 2, t[2] as usize / 2])?;
                    }
                }
                0xD7 => {
                    self.tex_on = (w0 >> 1) & 0x7F != 0;
                    self.tex_tile = ((w0 >> 8) & 7) as usize;
                    let sc = |v: u32| if v == 0xFFFF { 1.0 } else { v as f64 / 65536.0 };
                    self.tex_scale = (sc(w1 >> 16), sc(w1 & 0xFFFF));
                }
                0xD9 => self.geom = (self.geom & (w0 & 0xFF_FFFF)) | w1,
                0xFC => self.combine = (w0 & 0xFF_FFFF, w1),
                0xFA => {
                    self.prim = color4(w1);
                    self.prim_lod = (w0 & 0xFF) as f64 / 255.0;
                }
                0xFB => self.env = color4(w1),
                0xFD => self.timg = Some(Timg { addr: target, siz: ((w0 >> 19) & 3) as i64, width: (w0 & 0xFFF) as i64 + 1 }),
                0xF5 => {
                    let t = &mut self.tiles[((w1 >> 24) & 7) as usize];
                    let (a, b) = (w0 as i64, w1 as i64);
                    (t.fmt, t.siz, t.line, t.tmem) = ((a >> 21) & 7, (a >> 19) & 3, (a >> 9) & 0x1FF, a & 0x1FF);
                    (t.palette, t.cmt, t.maskt, t.shiftt) = ((b >> 20) & 15, (b >> 18) & 3, (b >> 14) & 15, (b >> 10) & 15);
                    (t.cms, t.masks, t.shifts) = ((b >> 8) & 3, (b >> 4) & 15, b & 15);
                }
                0xF2 => {
                    let t = &mut self.tiles[((w1 >> 24) & 7) as usize];
                    let (a, b) = (w0 as i64, w1 as i64);
                    (t.uls, t.ult, t.lrs, t.lrt) = ((a >> 12) & 0xFFF, a & 0xFFF, (b >> 12) & 0xFFF, b & 0xFFF);
                }
                0xF3 | 0xF4 | 0xF0 if self.timg.as_ref().is_some_and(|t| t.addr.is_some()) => {
                    let tile = ((w1 >> 24) & 7) as usize;
                    let (a, b) = (w0 as i64, w1 as i64);
                    match op {
                        0xF3 => self.load_block(tile, (b >> 12) & 0xFFF)?,
                        0xF4 => self.load_tile(tile, (a >> 12) & 0xFFF, a & 0xFFF, (b >> 12) & 0xFFF, b & 0xFFF)?,
                        _ => self.load_tlut(tile, (b >> 14) & 0x3FF)?,
                    }
                }
                0xF3 | 0xF4 | 0xF0 => {}
                0xE3 if (w0 & 0xFF) + 1 + ((w0 >> 8) & 0xFF) <= 32 => {
                    let length = (w0 & 0xFF) + 1;
                    let shift = 32 - ((w0 >> 8) & 0xFF) - length;
                    let mask = (((1u64 << length) - 1) << shift) as u32;
                    self.other_h = (self.other_h & !mask) | (w1 & mask);
                }
                0xDB => {
                    if (w0 >> 16) & 0xFF == 0x0A {
                        let c = color4(w1);
                        match w0 & 0xFFFF {
                            0x00 => self.light1 = [c[0], c[1], c[2]],
                            0x18 => self.light2 = [c[0], c[1], c[2]],
                            _ => {}
                        }
                    }
                }
                0xDE => {
                    if w1 >> 24 == 0x0E {
                        self.mobj_dl(((w1 & 0xFF_FFFF) / 8) as usize)?;
                    } else if let Some(t) = target {
                        self.run(t, joint, world)?;
                    }
                    if (w0 >> 16) & 0xFF == 1 {
                        return Ok(());
                    }
                }
                0xDF => return Ok(()),
                0xE3 | 0xE7 | 0xE8 | 0xE6 | 0xE9 | 0xE2 | 0xE1 | 0xF7 | 0xF8 | 0xF9 | 0xD8 | 0x00 => {}
                _ => {
                    self.unknown.insert(format!("{op:#x}"));
                }
            }
            o += 8;
        }
    }

    fn triangle(&mut self, idx: [usize; 3]) -> R<()> {
        let vs: Vec<Cached> = match idx.iter().map(|&i| self.cache.get(i).cloned().flatten()).collect::<Option<Vec<_>>>() {
            Some(v) => v,
            None => return Ok(()),
        };
        let mi = self.material_key()?;
        let mat = &self.materials[mi];
        let mut list = vec![];
        for v in vs {
            let uv = match mat.uv {
                Some((tw, th, uls, ult, ss, ts, sh)) => [
                    (v.st[0] as f64 / 32.0 * ss * sh.0 - uls as f64 / 4.0) / tw as f64,
                    (v.st[1] as f64 / 32.0 * ts * sh.1 - ult as f64 / 4.0) / th as f64,
                ],
                None => [0.0, 0.0],
            };
            let col = if mat.vcol { v.col } else { None };
            self.verts.push(Vert { joint: v.joint, pos: v.pos, nrm: v.nrm, uv, col });
            list.push(self.verts.len() - 1);
        }
        self.tris[mi].extend(list);
        Ok(())
    }

    fn material_key(&mut self) -> R<usize> {
        if let Some((g, k)) = self.last {
            if g == self.gen {
                return Ok(k);
            }
        }
        let k = self.new_material()?;
        self.last = Some((self.gen, k));
        Ok(k)
    }

    fn new_material(&mut self) -> R<usize> {
        let two = (self.other_h >> 20) & 3 == 1;
        let lit = self.geom & G_LIGHTING != 0;
        let mut uv: Option<Uv> = None;
        let textured = self.tex_on && uses(self.combine, two, &[1, 2]);
        let mut tiles: Vec<(usize, usize, Vec<u8>, (i64, i64))> = vec![];
        let mut flat: Option<Vec<[f64; 4]>> = None;
        if textured {
            for ti in [self.tex_tile, self.tex_tile + 1] {
                let t = self.tiles[ti & 7];
                let (xs, ws) = tile_axis((((t.lrs - t.uls) >> 2) + 1).clamp(1, 1024), t.masks, t.cms);
                let (ys, wt) = tile_axis((((t.lrt - t.ult) >> 2) + 1).clamp(1, 1024), t.maskt, t.cmt);
                let tlut = ((self.other_h >> 14) & 3) as i64;
                let px = decode_tile(&self.tmem, &t, &xs, &ys, tlut);
                tiles.push((xs.len(), ys.len(), px, (ws, wt)));
            }
            let t = self.tiles[self.tex_tile & 7];
            let shift = |s: i64| if 0 < s && s <= 10 { 1.0 / (1i64 << s) as f64 } else if s > 10 { (1i64 << (16 - s)) as f64 } else { 1.0 };
            uv = Some((tiles[0].0 as i64, tiles[0].1 as i64, t.uls, t.ult, self.tex_scale.0, self.tex_scale.1, (shift(t.shifts), shift(t.shiftt))));
            if self.geom & G_TEXTURE_GEN != 0 {
                flat = Some(
                    tiles
                        .iter()
                        .map(|(w, h, px, _)| {
                            let mut c = [0.0; 4];
                            for (k, v) in c.iter_mut().enumerate() {
                                let sum: u64 = px.iter().skip(k).step_by(4).map(|&b| b as u64).sum();
                                *v = sum as f64 / (255.0 * (*w * *h) as f64);
                            }
                            c
                        })
                        .collect(),
                );
                tiles.clear();
                uv = None;
            }
        }
        let shade = if lit { [self.light1[0], self.light1[1], self.light1[2], 1.0] } else { [1.0; 4] };
        let vcol = !lit && uses(self.combine, two, &[4]);
        let mut key = vec![];
        key.extend_from_slice(&self.combine.0.to_le_bytes());
        key.extend_from_slice(&self.combine.1.to_le_bytes());
        key.push(two as u8);
        push_f(&mut key, &self.prim);
        push_f(&mut key, &[self.prim_lod]);
        push_f(&mut key, &self.env);
        push_f(&mut key, &shade);
        key.extend_from_slice(&[vcol as u8, (self.geom & G_CULL_BACK != 0) as u8, (self.geom & G_CULL_FRONT != 0) as u8]);
        if let Some((w, h, uls, ult, ss, ts, sh)) = uv {
            key.push(1);
            for i in [w, h, uls, ult] {
                key.extend_from_slice(&i.to_le_bytes());
            }
            push_f(&mut key, &[ss, ts, sh.0, sh.1]);
        }
        key.push(0xFE);
        for c in flat.iter().flatten() {
            push_f(&mut key, c);
        }
        key.push(0xFD);
        for (w, h, px, wrap) in &tiles {
            key.extend_from_slice(&(*w as u64).to_le_bytes());
            key.extend_from_slice(&(*h as u64).to_le_bytes());
            key.extend_from_slice(&(px.len() as u64).to_le_bytes());
            key.extend_from_slice(px);
            key.extend_from_slice(&wrap.0.to_le_bytes());
            key.extend_from_slice(&wrap.1.to_le_bytes());
        }
        if let Some(i) = self.keys.iter().position(|k| *k == key) {
            return Ok(i);
        }
        let mut mat = Material { uv, vcol, double: self.geom & (G_CULL_BACK | G_CULL_FRONT) == 0, image: None, wrap: (10497, 10497), alpha: "OPAQUE", color: [1.0; 4] };
        if !tiles.is_empty() {
            let (w, h, px, wrap) = &tiles[0];
            let (w1, h1, px1, _) = &tiles[1];
            let mut out = Vec::with_capacity(w * h * 4);
            for i in 0..w * h {
                let t0 = [px[i * 4] as f64 / 255.0, px[i * 4 + 1] as f64 / 255.0, px[i * 4 + 2] as f64 / 255.0, px[i * 4 + 3] as f64 / 255.0];
                let j = ((i / w) % h1) * w1 + (i % w) % w1;
                let t1 = [px1[j * 4] as f64 / 255.0, px1[j * 4 + 1] as f64 / 255.0, px1[j * 4 + 2] as f64 / 255.0, px1[j * 4 + 3] as f64 / 255.0];
                let c = combine(self.combine, two, t0, t1, self.prim, shade, self.env, self.prim_lod);
                out.extend(c.iter().map(|x| crate::pyc::round(x * 255.0) as u8));
            }
            let alphas: Vec<u8> = out.iter().skip(3).step_by(4).copied().collect();
            mat.alpha = if *alphas.iter().min().unwrap_or(&255) > 250 {
                "OPAQUE"
            } else if alphas.iter().all(|&a| !(6..=250).contains(&a)) {
                "MASK"
            } else {
                "BLEND"
            };
            mat.image = Some((*w, *h, out));
            mat.wrap = *wrap;
        } else {
            let (t0, t1) = match &flat {
                Some(f) => (f[0], f[1]),
                None => ([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]),
            };
            let c = combine(self.combine, two, t0, t1, self.prim, shade, self.env, self.prim_lod);
            mat.color = c;
            mat.alpha = if c[3] > 0.98 { "OPAQUE" } else { "BLEND" };
        }
        self.keys.push(key);
        self.materials.push(mat);
        self.tris.push(vec![]);
        Ok(self.materials.len() - 1)
    }
}

// ------------------------------------------------------------------------------------------------ fighters

pub(crate) fn read_mobjs(files: &Files, list: Option<Ref>) -> R<Vec<MObj>> {
    let mut out = vec![];
    let mut k = 0;
    while list.is_some() {
        let Some(sub) = files.ptr(list, 4 * k)? else { break };
        let f = files.get(sub.0)?;
        let (d, o) = (&f.data, sub.1);
        let col = |off: usize| -> R<[f64; 4]> {
            let c = d.get(o + off..o + off + 4).ok_or("MObjSub outside the file")?;
            Ok([c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0, c[3] as f64 / 255.0])
        };
        let palettes = files.ptr(Some(sub), 0x2C)?;
        let (l1, l2) = (col(0x60)?, col(0x64)?);
        out.push(MObj {
            fmt: d[o + 2],
            siz: d[o + 3],
            sprites: files.ptr(Some(sub), 4)?,
            unk08: be16(d, o + 8)?,
            unk0a: be16(d, o + 0xA)?,
            unk0c: be16(d, o + 0xC)?,
            unk0e: be16(d, o + 0xE)?,
            unk10: be32(d, o + 0x10)? as i32,
            trau: bef32(d, o + 0x14)?,
            trav: bef32(d, o + 0x18)?,
            scau: bef32(d, o + 0x1C)?,
            scav: bef32(d, o + 0x20)?,
            palettes,
            palette: if palettes.is_some() { files.ptr(palettes, 0)? } else { None },
            flags: be16(d, o + 0x30)?,
            block_fmt: d[o + 0x32],
            block_siz: d[o + 0x33],
            block_dxt: be16(d, o + 0x34)?,
            unk36: be16(d, o + 0x36)?,
            prim: col(0x50)?,
            lfrac: d[o + 0x54] as f64 / 255.0,
            env: col(0x58)?,
            light1: [l1[0], l1[1], l1[2]],
            light2: [l2[0], l2[1], l2[2]],
            texture_id: 0,
            palette_id: None,
        });
        k += 1;
    }
    Ok(out)
}

/// Costume 0 of a material script (AObjEvent32 at frame 0): the values the first block sets.
pub(crate) fn costume(files: &Files, m: &mut MObj, script: Option<Ref>) -> R<()> {
    let Some((f, mut o)) = script else { return Ok(()) };
    let file = files.get(f)?;
    let d = &file.data;
    for _ in 0..64 {
        let w = be32(d, o)?;
        let (op, flags, payload) = (w >> 25, (w >> 15) & 0x3FF, w & 0x7FFF);
        o += 4;
        if payload != 0 && [3, 4, 5, 6, 8, 9, 10, 11, 18, 19, 20, 21].contains(&op) {
            break;
        }
        if [3, 4, 5, 6, 8, 9, 10, 11].contains(&op) {
            let rate = op == 5 || op == 6;
            for i in 0..10 {
                if flags >> i & 1 != 0 {
                    let v = bef32(d, o)?;
                    o += 4;
                    if rate {
                        o += 4;
                    }
                    if i == 9 {
                        m.palette_id = Some(v as i64);
                    } else if i == 0 {
                        m.texture_id = v as i64;
                    }
                }
            }
        } else if (18..=21).contains(&op) {
            for i in 0..5 {
                if flags >> i & 1 != 0 {
                    let c = d.get(o..o + 4).ok_or("costume script")?;
                    let c = [c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0, c[3] as f64 / 255.0];
                    o += 4;
                    match i {
                        0 => m.prim = c,
                        1 => m.env = c,
                        2 => {}
                        3 => m.light1 = [c[0], c[1], c[2]],
                        _ => m.light2 = [c[0], c[1], c[2]],
                    }
                }
            }
        } else if op == 7 {
            o += 4 * flags.count_ones() as usize;
        } else if op == 15 {
        } else {
            break;
        }
        if op == 2 || op == 0 {
            break;
        }
    }
    if let (Some(_), Some(id)) = (m.palettes, m.palette_id) {
        m.palette = files.ptr(m.palettes, 4 * id as usize)?;
    }
    Ok(())
}

pub(crate) struct Joint {
    pub(crate) index: usize,
    pub(crate) parent: Option<usize>,
    dl: Option<Ref>,
    pub(crate) t: [f64; 3],
    pub(crate) r: [f64; 3],
    pub(crate) s: [f64; 3],
    world: M4,
}

pub(crate) struct Fighter<'f, 'a> {
    files: &'f Files<'a>,
    pub(crate) size: f64,
    mobj_table: Option<Ref>,
    costume_table: Option<Ref>,
    split: bool,
    pub(crate) joints: Vec<Joint>,
}

impl<'f, 'a> Fighter<'f, 'a> {
    pub(crate) fn new(files: &'f Files<'a>, table: &Table) -> R<Self> {
        let attr = Some((table.main, table.attr));
        let size = bef32(&files.get(table.main)?.data, table.attr)?;
        let cpc = files.ptr(attr, ATTR_COMMONPARTS)?.ok_or("no commonparts")?;
        let setup = files.ptr(attr, ATTR_SETUP_PARTS)?;
        let flags = match setup {
            Some((f, o)) => {
                let d = &files.get(f)?.data;
                (be32(d, o)?, be32(d, o + 4)?)
            }
            None => (0xFFFF_FFFF, 0xFFFF_FFFF),
        };
        let desc = files.ptr(Some(cpc), 0)?.ok_or("no joint tree")?;
        let mobj_table = files.ptr(Some(cpc), 4)?;
        let costume_table = files.ptr(Some(cpc), 8)?;
        let split = files.get(cpc.0)?.data[cpc.1 + 12] & 0xF == 1;
        let dd = files.get(desc.0)?;
        let mut joints: Vec<Joint> = vec![];
        let mut stack: HashMap<i64, usize> = HashMap::new();
        let mut i = 0;
        loop {
            let o = desc.1 + 44 * i;
            let jid = be32(&dd.data, o)? as i32;
            if jid == DOBJ_ARRAY_MAX {
                break;
            }
            let bit = if i < 32 { flags.0 >> (31 - i) & 1 } else { flags.1 >> (63 - i) & 1 };
            if bit != 0 {
                let depth = (jid & 0xFFF) as i64;
                let parent = if depth != 0 { stack.get(&(depth - 1)).copied() } else { None };
                let v: Vec<f64> = (0..9).map(|k| bef32(&dd.data, o + 8 + 4 * k)).collect::<R<_>>()?;
                let (t, r, s) = ([v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]);
                let local = mat_trs(t, r, s);
                let world = match parent {
                    Some(p) => mat_mul(&joints[p].world, &local),
                    None => local,
                };
                stack.insert(depth, joints.len());
                joints.push(Joint { index: i, parent, dl: files.ptr(Some(desc), 44 * i + 4)?, t, r, s, world });
            }
            i += 1;
        }
        Ok(Fighter { files, size, mobj_table, costume_table, split, joints })
    }

    fn mesh(&self) -> R<Mesh<'f, 'a>> {
        let files = self.files;
        let mut m = Mesh::new(files);
        for (ji, j) in self.joints.iter().enumerate() {
            let mlist = if self.mobj_table.is_some() { files.ptr(self.mobj_table, 4 * j.index)? } else { None };
            let clist = if self.costume_table.is_some() { files.ptr(self.costume_table, 4 * j.index)? } else { None };
            let load = |m: &mut Mesh| -> R<()> {
                m.mobjs = read_mobjs(files, mlist)?;
                for k in 0..m.mobjs.len() {
                    let script = if clist.is_some() { files.ptr(clist, 4 * k)? } else { None };
                    costume(files, &mut m.mobjs[k], script)?;
                }
                Ok(())
            };
            if self.split {
                let Some(dl) = j.dl else { continue };
                let (pre, own) = (files.ptr(Some(dl), 0)?, files.ptr(Some(dl), 4)?);
                if let Some(pre) = pre {
                    let p = j.parent.unwrap_or(ji);
                    m.run(pre, p, &self.joints[p].world)?;
                }
                load(&mut m)?;
                if let Some(own) = own {
                    m.run(own, ji, &j.world)?;
                }
                continue;
            }
            load(&mut m)?;
            if let Some(dl) = j.dl {
                m.run(dl, ji, &j.world)?;
            }
        }
        Ok(m)
    }

    /// Per joint: the sampled (t, r, s) of every frame, and the clip length in frames.
    pub(crate) fn animation(&self, fid: usize) -> R<(Vec<Option<Vec<Pose>>>, usize)> {
        let f = self.files.get(fid)?;
        let mut tracks = vec![];
        let mut length = 0;
        for (ji, j) in self.joints.iter().enumerate() {
            let tr = match f.ptrs.get(&(4 * ji)) {
                Some(&(sf, so)) if sf == fid => Some(simulate(&f.data, so, j)?),
                _ => None,
            };
            if let Some(t) = &tr {
                length = length.max(t.len());
            }
            tracks.push(tr);
        }
        Ok((tracks, length.max(1)))
    }
}

pub(crate) type Pose = ([f64; 3], [f64; 3], [f64; 3]);

#[derive(Clone, Copy, Default)]
struct AObj {
    kind: u8,
    vb: f64,
    vt: f64,
    rb: f64,
    rt: f64,
    length: f64,
    inv: f64,
}

const FRACS: [f64; 8] = [1.0 / 512.0, 1.0 / 4.0, 1.0 / 4096.0, 1.0 / 16384.0, 1.0 / 512.0, 1.0 / 32.0, 1.0 / 8192.0, 1.0 / 16384.0];

fn value(raw: i16, track: usize, step: bool) -> f64 {
    let kind = if track < 3 { 0 } else if (4..=6).contains(&track) { 1 } else if track > 6 { 2 } else { 3 };
    raw as f64 * FRACS[kind + if step { 4 } else { 0 }]
}

/// Frame-by-frame run of one joint's AObjEvent16 script (ftAnimParseDObjFigatree + gcPlayDObjAnimJoint).
fn simulate(data: &[u8], mut pos: usize, joint: &Joint) -> R<Vec<Pose>> {
    let (mut t, mut r, mut s) = (joint.t, joint.r, joint.s);
    let mut aobjs: BTreeMap<usize, AObj> = BTreeMap::new();
    let fresh = AObj { inv: 1.0, ..Default::default() };
    let mut wait = 0.0f64;
    let mut frames = vec![];
    let h = |p: usize| be16(data, p);
    let mut ended = false;
    for frame in 0..2000 {
        if frame > 0 {
            if ended {
                break;
            }
            wait -= 1.0;
        }
        while wait <= 0.0 && !ended {
            let cmd = h(pos)?;
            let (op, mut flags, toggle) = (cmd >> 11, (cmd >> 1) & 0x3FF, cmd & 1);
            pos += 2;
            let mut payload = 0.0;
            if [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 14].contains(&op) && toggle != 0 {
                payload = h(pos)? as f64;
                pos += 2;
            }
            match op {
                2 | 3 | 4 | 5 | 7 | 8 | 9 | 10 => {
                    for i in 0..10 {
                        if flags == 0 {
                            break;
                        }
                        if flags & 1 != 0 {
                            let a = aobjs.entry(i).or_insert(fresh);
                            a.vb = a.vt;
                            a.vt = value(h(pos)? as i16, i, false);
                            pos += 2;
                            if op == 4 || op == 5 {
                                a.rb = a.rt;
                                a.rt = value(h(pos)? as i16, i, true);
                                pos += 2;
                                a.kind = 3;
                                if payload != 0.0 {
                                    a.inv = 1.0 / payload;
                                }
                            } else if op == 7 || op == 8 {
                                a.rb = a.rt;
                                a.rt = 0.0;
                                a.kind = 3;
                                if payload != 0.0 {
                                    a.inv = 1.0 / payload;
                                }
                            } else if op == 2 || op == 3 {
                                a.kind = 2;
                                if payload != 0.0 {
                                    a.rb = (a.vt - a.vb) / payload;
                                }
                                a.rt = 0.0;
                            } else {
                                a.kind = 1;
                                a.inv = payload;
                                a.rt = 0.0;
                            }
                            a.length = -wait - 1.0;
                        }
                        flags >>= 1;
                    }
                    if [2, 4, 7, 9].contains(&op) {
                        wait += payload;
                    }
                }
                6 => {
                    for i in 0..10 {
                        if flags == 0 {
                            break;
                        }
                        if flags & 1 != 0 {
                            let a = aobjs.entry(i).or_insert(fresh);
                            a.rt = value(h(pos)? as i16, i, true);
                            pos += 2;
                        }
                        flags >>= 1;
                    }
                }
                1 => {
                    if toggle != 0 {
                        wait += h(pos)? as f64;
                        pos += 2;
                    }
                }
                11 => {
                    for i in 0..10 {
                        if flags == 0 {
                            break;
                        }
                        if flags & 1 != 0 {
                            if let Some(a) = aobjs.get_mut(&i) {
                                a.length += payload;
                            }
                        }
                        flags >>= 1;
                    }
                }
                12 => pos += 2,
                13 => {
                    ended = true;
                    break;
                }
                0 => {
                    ended = true;
                    for a in aobjs.values_mut() {
                        if a.kind != 0 {
                            a.length += 1.0 + wait;
                        }
                    }
                    break;
                }
                14 => {}
                _ => {
                    ended = true;
                    break;
                }
            }
        }
        for (&i, a) in aobjs.iter_mut() {
            if a.kind == 0 {
                continue;
            }
            if !ended {
                a.length += 1.0;
            }
            let l = a.length;
            let v = match a.kind {
                2 => a.vb + l * a.rb,
                3 => {
                    let inv = a.inv;
                    let (f16, f12) = (inv * inv, l * l);
                    let f18 = inv * f12;
                    let f14 = l * f12 * f16;
                    let f20 = 2.0 * f14 * inv;
                    let f22 = 3.0 * f12 * f16;
                    let f24 = f14 - f18;
                    a.vb * ((f20 - f22) + 1.0) + a.vt * (f22 - f20) + a.rb * ((f24 - f18) + l) + a.rt * f24
                }
                _ => {
                    if a.inv <= l {
                        a.vt
                    } else {
                        a.vb
                    }
                }
            };
            match i {
                0..=2 => r[i] = v,
                4..=6 => t[i - 4] = v,
                7.. => s[i - 7] = v,
                _ => {}
            }
        }
        frames.push((t, r, s));
    }
    Ok(frames)
}

// ------------------------------------------------------------------------------------------------ glTF

pub(crate) struct Gltf {
    bin: Vec<u8>,
    pub(crate) j: Map<String, Value>,
}

pub(crate) enum Rows<'a> {
    F(&'a [Vec<f64>]),
    U8(&'a [[u8; 4]]),
    Idx(&'a [usize], bool),
}

impl Gltf {
    pub(crate) fn new() -> Self {
        let mut j = Map::new();
        j.insert("asset".into(), json!({"version": "2.0", "generator": "jumpman export-ssb64.py"}));
        j.insert("scene".into(), json!(0));
        j.insert("scenes".into(), json!([{"nodes": []}]));
        for k in ["nodes", "meshes", "buffers", "bufferViews", "accessors", "materials", "textures", "images", "samplers", "skins", "animations"] {
            j.insert(k.into(), json!([]));
        }
        Gltf { bin: vec![], j }
    }

    pub(crate) fn arr(&mut self, k: &str) -> &mut Vec<Value> {
        self.j.get_mut(k).unwrap().as_array_mut().unwrap()
    }

    pub(crate) fn view(&mut self, data: &[u8], target: Option<u32>) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let mut v = Map::new();
        v.insert("buffer".into(), json!(0));
        v.insert("byteOffset".into(), json!(self.bin.len()));
        v.insert("byteLength".into(), json!(data.len()));
        if let Some(t) = target {
            v.insert("target".into(), json!(t));
        }
        self.bin.extend_from_slice(data);
        let a = self.arr("bufferViews");
        a.push(Value::Object(v));
        a.len() - 1
    }

    pub(crate) fn accessor(&mut self, rows: Rows, kind: &str, target: Option<u32>, minmax: bool) -> usize {
        let mut data = vec![];
        let (ctype, count) = match &rows {
            Rows::F(r) => {
                for row in r.iter() {
                    for x in row {
                        data.extend_from_slice(&(*x as f32).to_le_bytes());
                    }
                }
                (5126, r.len())
            }
            Rows::U8(r) => {
                for row in r.iter() {
                    data.extend_from_slice(row);
                }
                (5121, r.len())
            }
            Rows::Idx(r, wide) => {
                for &x in r.iter() {
                    if *wide {
                        data.extend_from_slice(&(x as u32).to_le_bytes());
                    } else {
                        data.extend_from_slice(&(x as u16).to_le_bytes());
                    }
                }
                (if *wide { 5125 } else { 5123 }, r.len())
            }
        };
        let view = self.view(&data, target);
        let mut a = Map::new();
        a.insert("bufferView".into(), json!(view));
        a.insert("componentType".into(), json!(ctype));
        a.insert("count".into(), json!(count));
        a.insert("type".into(), json!(kind));
        if minmax {
            if let Rows::F(r) = rows {
                let n = r[0].len();
                // Python's min()/max(): the first of equal values wins (0.0 vs -0.0 prints differently)
                let pick = |k: usize, lower: bool| {
                    let mut best = r[0][k];
                    for x in r.iter().skip(1) {
                        if (lower && x[k] < best) || (!lower && x[k] > best) {
                            best = x[k];
                        }
                    }
                    json!(best)
                };
                let lo: Vec<Value> = (0..n).map(|k| pick(k, true)).collect();
                let hi: Vec<Value> = (0..n).map(|k| pick(k, false)).collect();
                a.insert("min".into(), Value::Array(lo));
                a.insert("max".into(), Value::Array(hi));
            }
        }
        let acc = self.arr("accessors");
        acc.push(Value::Object(a));
        acc.len() - 1
    }

    pub(crate) fn glb(self) -> Vec<u8> {
        let mut j: Map<String, Value> = self.j.into_iter().filter(|(_, v)| v.as_array().is_none_or(|a| !a.is_empty())).collect();
        j.insert("buffers".into(), json!([{"byteLength": self.bin.len()}]));
        let mut js = dumps(&Value::Object(j), None).into_bytes();
        while js.len() % 4 != 0 {
            js.push(b' ');
        }
        let mut b = self.bin;
        while b.len() % 4 != 0 {
            b.push(0);
        }
        let mut out = vec![];
        for w in [0x4654_6C67u32, 2, (12 + 8 + js.len() + 8 + b.len()) as u32, js.len() as u32, 0x4E4F_534A] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(&js);
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        out.extend_from_slice(&b);
        out
    }
}

fn round4(x: f64) -> f64 {
    format!("{x:.4}").parse().unwrap()
}

/// Sorted keys at every level (`json.dumps(..., sort_keys=True)`).
pub(crate) fn sort_keys(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let sorted: BTreeMap<String, Value> = m.into_iter().map(|(k, v)| (k, sort_keys(v))).collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sort_keys).collect()),
        x => x,
    }
}

fn export_fighter(files: &Files, names: &HashMap<String, usize>, cmotions: &BTreeMap<i64, String>, name: &str, table: &Table, out_dir: &Path, log: &mut dyn FnMut(&str)) -> R<Value> {
    let fighter = Fighter::new(files, table)?;
    let mesh = fighter.mesh()?;
    let mut g = Gltf::new();
    let jn = &fighter.joints;
    let scale = UNIT * fighter.size;
    let root_m: M4 = [[scale, 0.0, 0.0, 0.0], [0.0, scale, 0.0, 0.0], [0.0, 0.0, scale, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let mut nodes: Vec<Map<String, Value>> = vec![];
    nodes.push(json!({"name": format!("{name}_mesh"), "mesh": 0, "skin": 0}).as_object().unwrap().clone());
    nodes.push(json!({"name": name, "scale": [scale, scale, scale], "children": []}).as_object().unwrap().clone());
    g.j.insert("scene".into(), json!(0));
    g.j.insert("scenes".into(), json!([{"nodes": [0, 1]}]));
    for (ji, j) in jn.iter().enumerate() {
        let mut node = Map::new();
        node.insert("name".into(), json!(format!("joint_{:02}", j.index)));
        node.insert("translation".into(), json!(j.t.map(f32r)));
        node.insert("rotation".into(), json!(quat(j.r).map(f32r)));
        if j.s != [1.0, 1.0, 1.0] {
            node.insert("scale".into(), json!(j.s.map(f32r)));
        }
        nodes.push(node);
        let parent = j.parent.map_or(1, |p| 2 + p);
        let children = nodes[parent].entry("children").or_insert(json!([]));
        children.as_array_mut().unwrap().push(json!(2 + ji));
    }
    *g.arr("nodes") = nodes.into_iter().map(Value::Object).collect();
    let mut prims = vec![];
    let mut image_index: Vec<((Vec<u8>, (i64, i64)), usize)> = vec![];
    for (mi, idx) in mesh.tris.iter().enumerate() {
        let mat = &mesh.materials[mi];
        let mut vids: Vec<usize> = idx.clone();
        vids.sort_unstable();
        vids.dedup();
        let remap: HashMap<usize, usize> = vids.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        let (mut pos, mut nrm, mut uv, mut jt, mut wt, mut col) = (vec![], vec![], vec![], vec![], vec![], vec![]);
        for &v in &vids {
            let vert = &mesh.verts[v];
            pos.push(vert.pos.iter().map(|x| f32r(x * scale)).collect::<Vec<_>>());
            let n = vert.nrm.unwrap_or([0.0, 1.0, 0.0]);
            let ln = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let ln = if ln == 0.0 { 1.0 } else { ln };
            nrm.push(n.iter().map(|x| f32r(x / ln)).collect::<Vec<_>>());
            uv.push(vert.uv.iter().map(|&x| f32r(x)).collect::<Vec<_>>());
            jt.push([vert.joint as u8, 0, 0, 0]);
            wt.push(vec![1.0, 0.0, 0.0, 0.0]);
            col.push(vert.col.unwrap_or([1.0; 4]).iter().map(|&x| f32r(x)).collect::<Vec<_>>());
        }
        let mut attrs = Map::new();
        attrs.insert("POSITION".into(), json!(g.accessor(Rows::F(&pos), "VEC3", Some(34962), true)));
        attrs.insert("NORMAL".into(), json!(g.accessor(Rows::F(&nrm), "VEC3", Some(34962), false)));
        attrs.insert("JOINTS_0".into(), json!(g.accessor(Rows::U8(&jt), "VEC4", Some(34962), false)));
        attrs.insert("WEIGHTS_0".into(), json!(g.accessor(Rows::F(&wt), "VEC4", Some(34962), false)));
        if mat.image.is_some() {
            attrs.insert("TEXCOORD_0".into(), json!(g.accessor(Rows::F(&uv), "VEC2", Some(34962), false)));
        }
        if mat.vcol {
            attrs.insert("COLOR_0".into(), json!(g.accessor(Rows::F(&col), "VEC4", Some(34962), false)));
        }
        let ind: Vec<usize> = idx.iter().map(|i| remap[i]).collect();
        let indices = g.accessor(Rows::Idx(&ind, vids.len() >= 65536), "SCALAR", Some(34963), false);
        let mut pbr = Map::new();
        pbr.insert("baseColorFactor".into(), json!(mat.color.map(f32r)));
        pbr.insert("metallicFactor".into(), json!(0.0));
        pbr.insert("roughnessFactor".into(), json!(1.0));
        let mut m = Map::new();
        if mat.double {
            m.insert("doubleSided".into(), json!(true));
        }
        if mat.alpha != "OPAQUE" {
            m.insert("alphaMode".into(), json!(mat.alpha));
            if mat.alpha == "MASK" {
                m.insert("alphaCutoff".into(), json!(0.5));
            }
        }
        if let Some((w, h, px)) = &mat.image {
            let data = crate::zpng::png_rgba(*w, *h, px, 9)?;
            let ik = (data.clone(), mat.wrap);
            let tex = match image_index.iter().find(|(k, _)| *k == ik) {
                Some((_, t)) => *t,
                None => {
                    let view = g.view(&data, None);
                    g.arr("images").push(json!({"bufferView": view, "mimeType": "image/png"}));
                    let images = g.arr("images").len();
                    let sampler = json!({"magFilter": 9729, "minFilter": 9729, "wrapS": mat.wrap.0, "wrapT": mat.wrap.1});
                    let si = match g.arr("samplers").iter().position(|s| *s == sampler) {
                        Some(i) => i,
                        None => {
                            g.arr("samplers").push(sampler);
                            g.arr("samplers").len() - 1
                        }
                    };
                    g.arr("textures").push(json!({"source": images - 1, "sampler": si}));
                    let t = g.arr("textures").len() - 1;
                    image_index.push((ik, t));
                    t
                }
            };
            pbr.insert("baseColorTexture".into(), json!({"index": tex}));
        }
        // the Python dict: name, pbrMetallicRoughness, then the optional keys
        let mut ordered = Map::new();
        ordered.insert("name".into(), json!(format!("mat{mi:03}")));
        ordered.insert("pbrMetallicRoughness".into(), Value::Object(pbr));
        ordered.extend(m);
        g.arr("materials").push(Value::Object(ordered));
        let material = g.arr("materials").len() - 1;
        prims.push(json!({"attributes": attrs, "indices": indices, "material": material}));
    }
    g.arr("meshes").push(json!({"name": name, "primitives": prims}));
    let ibm: Vec<Vec<f64>> = jn
        .iter()
        .map(|j| {
            let m = mat_inv(&mat_mul(&root_m, &j.world));
            (0..16).map(|k| f32r(m[k % 4][k / 4])).collect()
        })
        .collect();
    let ibm_acc = g.accessor(Rows::F(&ibm), "MAT4", None, false);
    g.arr("skins").push(json!({"joints": (0..jn.len()).map(|i| 2 + i).collect::<Vec<_>>(), "skeleton": 1, "inverseBindMatrices": ibm_acc}));
    // motions
    let special = *cmotions.iter().find(|(_, v)| *v == "SpecialStart").ok_or("SpecialStart")?.0;
    let mut users: Vec<(String, Vec<i64>)> = vec![];
    for (mid, anim) in table.motions.iter().enumerate() {
        if let Some(a) = anim.as_ref().filter(|a| names.contains_key(*a)) {
            match users.iter_mut().find(|(k, _)| k == a) {
                Some((_, v)) => v.push(mid as i64),
                None => users.push((a.clone(), vec![mid as i64])),
            }
        }
    }
    // sorted by the first motion that plays them (equal to insertion order here)
    users.sort_by_key(|(_, m)| m[0]);
    let mut clip_of: Vec<(String, String)> = vec![];
    for (anim, mids) in &users {
        let stem = if anim.starts_with("FT") && anim.contains("Anim") { anim.split_once("Anim").unwrap().1.to_string() } else { anim.clone() };
        let common: Vec<i64> = mids.iter().copied().filter(|m| (4..special).contains(m)).collect();
        let mut clip = if !common.is_empty() && name != "Boss" { cmotions[common.iter().min().unwrap()].clone() } else { stem.clone() };
        if clip_of.iter().any(|(_, c)| *c == clip) {
            clip = format!("{clip}_{stem}");
        }
        clip_of.push((anim.clone(), clip));
    }
    let mut clips = Map::new();
    let mut motion_map: BTreeMap<i64, String> = BTreeMap::new();
    let mut time_cache: HashMap<(bool, usize), usize> = HashMap::new();
    for (anim, mids) in &users {
        let clip = clip_of.iter().find(|(a, _)| a == anim).unwrap().1.clone();
        for &mid in mids {
            motion_map.insert(mid, clip.clone());
        }
        let (tracks, length) = fighter.animation(names[anim])?;
        let n = length.max(2);
        let times = match time_cache.get(&(false, n)) {
            Some(&t) => t,
            None => {
                let rows: Vec<Vec<f64>> = (0..n).map(|i| vec![f32r(i as f64 / FPS)]).collect();
                let t = g.accessor(Rows::F(&rows), "SCALAR", None, true);
                time_cache.insert((false, n), t);
                t
            }
        };
        let (mut channels, mut samplers) = (vec![], vec![]);
        for (ji, j) in jn.iter().enumerate() {
            let mut fr: Vec<Pose> = tracks[ji].clone().unwrap_or_else(|| vec![(j.t, j.r, j.s)]);
            while fr.len() < n {
                fr.push(*fr.last().unwrap());
            }
            let paths: [(&str, &str, Vec<Vec<f64>>); 3] = [
                ("translation", "VEC3", fr.iter().map(|f| f.0.iter().map(|&x| f32r(x)).collect()).collect()),
                ("rotation", "VEC4", fr.iter().map(|f| quat(f.1).iter().map(|&x| f32r(x)).collect()).collect()),
                ("scale", "VEC3", fr.iter().map(|f| f.2.iter().map(|&x| f32r(x)).collect()).collect()),
            ];
            for (path, kind, rows) in paths {
                let (inp, rows) = if rows.iter().all(|r| *r == rows[0]) {
                    let t = match time_cache.get(&(true, n)) {
                        Some(&t) => t,
                        None => {
                            let t = g.accessor(Rows::F(&[vec![0.0], vec![f32r((n - 1) as f64 / FPS)]]), "SCALAR", None, true);
                            time_cache.insert((true, n), t);
                            t
                        }
                    };
                    (t, vec![rows[0].clone(), rows[0].clone()])
                } else {
                    (times, rows)
                };
                let out = g.accessor(Rows::F(&rows), kind, None, false);
                samplers.push(json!({"input": inp, "output": out, "interpolation": "LINEAR"}));
                channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": 2 + ji, "path": path}}));
            }
        }
        g.arr("animations").push(json!({"name": clip, "channels": channels, "samplers": samplers}));
        clips.insert(clip.clone(), json!({"frames": length, "seconds": round4(length as f64 / FPS), "file": names[anim], "source": anim}));
    }
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let file = format!("{}.glb", name.to_lowercase());
    std::fs::write(out_dir.join(&file), g.glb()).map_err(|e| e.to_string())?;
    if !mesh.unknown.is_empty() {
        log(&format!("{name}: unhandled display list opcodes {:?}", mesh.unknown));
    }
    let mut named = Map::new();
    for (mid, clip) in &motion_map {
        let motion = if *mid < special {
            cmotions[mid].clone()
        } else {
            table.specials.get((mid - special) as usize).cloned().unwrap_or_else(|| format!("Special{}", mid - special))
        };
        named.insert(mid.to_string(), json!({"clip": clip, "motion": motion}));
    }
    let tris: usize = mesh.tris.iter().map(Vec::len).sum::<usize>() / 3;
    let main = files.get(table.main)?;
    let mut attrs = Map::new();
    for (o, k) in ATTRIBUTES {
        let v = if k == "jumps_max" { json!(be32(&main.data, table.attr + o)? as i32) } else { json!(bef32(&main.data, table.attr + o)?) };
        attrs.insert(k.to_string(), v);
    }
    Ok(json!({"label": label(name), "file": file, "joints": jn.len(), "vertices": mesh.verts.len(), "triangles": tris,
        "materials": mesh.materials.len(), "size": fighter.size, "unit": UNIT, "clips": clips, "motions": named, "attributes": attrs}))
}

/// FTAttributes fields (src/ft/fttypes.h) a controller can tune a fighter with: byte offset, name (f32 unless noted).
const ATTRIBUTES: [(usize, &str); 17] = [
    (0x08, "walkmiddle_anim_length"),
    (0x20, "walk_speed_mul"),
    (0x24, "traction"),
    (0x28, "dash_speed"),
    (0x30, "run_speed"),
    (0x34, "kneebend_anim_length"),
    (0x38, "jump_vel_x"),
    (0x3C, "jump_height_mul"),
    (0x40, "jump_height_base"),
    (0x48, "jumpaerial_height"),
    (0x4C, "air_accel"),
    (0x50, "air_speed_max_x"),
    (0x58, "gravity"),
    (0x5C, "tvel_base"),
    (0x60, "tvel_fast"),
    (0x64, "jumps_max"),
    (0x68, "weight"),
];

/// `main()` of export-ssb64.py: every fighter into `<public>/models/ssb64`.
pub fn export(source: &Path, z64: &[u8], public: &Path, progress: &mut dyn FnMut(u32, &str)) -> R<()> {
    if crate::hash::hex(&crate::hash::sha1(z64)) != SHA1 {
        return Err("not Super Smash Bros. (US)".into());
    }
    let decomp = decomp_dir(source);
    let files = Files::new(z64);
    let names = file_names(&decomp)?;
    let cmotions = common_motions(&decomp)?;
    let tables = fighter_tables(&decomp, &names)?;
    let out = public.join("models/ssb64");
    let mut fighters = Map::new();
    for (i, name) in FIGHTERS.iter().enumerate() {
        progress(5 + (90 * i / FIGHTERS.len()) as u32, &format!("Exporting {}", label(name)));
        let info = export_fighter(&files, &names, &cmotions, name, &tables[*name], &out, &mut |m| eprintln!("{m}"))?;
        fighters.insert(name.to_lowercase(), info);
    }
    let index = sort_keys(json!({"game": "ssb64", "fps": FPS, "fighters": fighters}));
    std::fs::write(out.join("fighters.json"), dumps(&index, Some(1)) + "\n").map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run(source: &Path, stage: &Path, z64: &[u8], progress: &mut dyn FnMut(u32, &str)) -> R<()> {
    export(source, z64, &stage.join("public"), progress)?;
    crate::ssb64_motions::export(source, z64, &stage.join("public/models/ssb64"))?;
    progress(97, "Exporting Super Smash Bros. voices");
    crate::ssb64_sounds::export(source, z64, &stage.join("assets/audio/voices/ssb64"))?;
    progress(98, "Exporting Super Smash Bros. stages");
    crate::ssb64_stages::export(source, z64, &stage.join("assets"), None, &mut |m| eprintln!("{m}"))?;
    progress(99, "Exporting Super Smash Bros. music");
    crate::ssb64_music::export(source, z64, &stage.join("assets/audio/music/ssb64"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-made vpk0 stream (one-sample mode): literal 'a', literal 'b', then a 4-byte copy from 2 back.
    #[test]
    fn vpk0_literals_and_back_references() {
        // bits: magic(32) size(32)=6 method(8)=0, offsets tree: leaf 2 then end, lengths tree: leaf 3 then end,
        // 0 'a', 0 'b', 1 off(2 bits)=2 len(3 bits)=4
        let mut bits = String::new();
        bits += &format!("{:032b}", u32::from_be_bytes(*b"vpk0"));
        bits += &format!("{:032b}", 6);
        bits += &format!("{:08b}", 0);
        bits += &format!("0{:08b}1", 2);
        bits += &format!("0{:08b}1", 3);
        bits += &format!("0{:08b}", b'a');
        bits += &format!("0{:08b}", b'b');
        bits += &format!("1{:02b}{:03b}", 2, 4);
        while bits.len() % 16 != 0 {
            bits.push('0');
        }
        let bytes: Vec<u8> = bits.as_bytes().chunks(8).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 2).unwrap()).collect();
        assert_eq!(vpk0(&bytes).unwrap(), b"ababab");
    }

    /// One joint whose script sets RotX linearly from 0 to 1 rad over 4 frames, then ends.
    #[test]
    fn figatree_linear_track() {
        let j = Joint { index: 0, parent: None, dl: None, t: [0.0; 3], r: [0.0; 3], s: [1.0; 3], world: mat_trs([0.0; 3], [0.0; 3], [1.0; 3]) };
        // SetValBlock (op 2) on RotX (flag bit 0) with payload 4, value 512 (= 1.0 rad at 1/512), then End.
        let cmd = |op: u16, flags: u16, toggle: u16| (op << 11) | (flags << 1) | toggle;
        let mut data = vec![];
        for w in [cmd(2, 0, 1), 4, cmd(2, 1, 1), 4, 512, cmd(0, 0, 0)] {
            data.extend_from_slice(&w.to_be_bytes());
        }
        let frames = simulate(&data, 0, &j).unwrap();
        let rx: Vec<f64> = frames.iter().map(|f| f.1[0]).collect();
        assert_eq!(rx.len(), 9);
        assert!((rx[4] - 0.0).abs() < 1e-9, "{rx:?}");
        assert!((rx[6] - 0.5).abs() < 1e-9, "{rx:?}");
        assert!((rx[8] - 1.0).abs() < 1e-9, "{rx:?}");
    }

    #[test]
    fn combiner_shade_times_prim() {
        // (PRIM - 0) * SHADE + 0 in colour, PRIM alpha: FC127E03 FFFFF3F8 style; built here from the selectors
        let (a, b, c, d) = (3u32, 8u32, 4u32, 7u32);
        let w0 = (a << 20) | (c << 15) | (7 << 12) | (7 << 9) | (a << 5) | c;
        let w1 = (b << 28) | (b << 24) | (7 << 21) | (7 << 18) | (d << 15) | (7 << 12) | (3 << 9) | (d << 6) | (7 << 3) | 3;
        let out = combine((w0, w1), false, [0.0; 4], [0.0; 4], [1.0, 0.5, 0.0, 1.0], [0.5, 0.5, 0.5, 1.0], [0.0; 4], 0.0);
        assert_eq!(out, [0.5, 0.25, 0.0, 1.0]);
        assert!(uses((w0, w1), false, &[4]));
        assert!(!uses((w0, w1), false, &[1, 2]));
    }

    #[test]
    fn tile_axes_mirror_and_clamp() {
        assert_eq!(tile_axis(8, 2, 3).0, vec![0, 1, 2, 3, 3, 2, 1, 0]);
        assert_eq!(tile_axis(8, 2, 0), (vec![0, 1, 2, 3], 10497));
        assert_eq!(tile_axis(4, 0, 2), (vec![0, 1, 2, 3], 33071));
    }
}
