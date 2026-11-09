//! Port of `scripts/export-sm64.py`: decomp GeoLayouts, Fast3D display lists and rigid animations to GLB.
use crate::pyc::{clean, dumps, int, py_float, re, read, round, R};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const SCALE: f64 = 0.01;

// ---------------------------------------------------------------- numbers

#[derive(Clone, Copy, Debug)]
pub enum N {
    I(i64),
    F(f64),
}

impl N {
    fn f(self) -> f64 {
        match self {
            N::I(i) => i as f64,
            N::F(f) => f,
        }
    }
    fn json(self) -> Value {
        match self {
            N::I(i) => json!(i),
            N::F(f) => json!(f),
        }
    }
    fn repr(self) -> String {
        match self {
            N::I(i) => i.to_string(),
            N::F(f) => py_float(f),
        }
    }
    fn lt(self, o: N) -> bool {
        self.f() < o.f()
    }
}

fn list_repr(v: &[N]) -> String {
    format!("[{}]", v.iter().map(|x| x.repr()).collect::<Vec<_>>().join(", "))
}

// ---------------------------------------------------------------- C expressions (Python `ast` numeric subset)

struct P<'a> {
    t: Vec<&'a str>,
    i: usize,
}

fn lex_num(s: &str) -> Option<Vec<String>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            let st = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            out.push(s[st..i].to_string());
        } else if s[i..].starts_with("<<") {
            out.push("<<".into());
            i += 2;
        } else if "|+-*/()".contains(c) {
            out.push(c.to_string());
            i += 1;
        } else {
            return None;
        }
    }
    Some(out)
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.t.get(self.i).copied()
    }
    fn or(&mut self) -> Option<N> {
        let mut v = self.shift()?;
        while self.peek() == Some("|") {
            self.i += 1;
            let r = self.shift()?;
            v = N::I((v.f() as i64) | (r.f() as i64));
        }
        Some(v)
    }
    fn shift(&mut self) -> Option<N> {
        let mut v = self.add()?;
        while self.peek() == Some("<<") {
            self.i += 1;
            let r = self.add()?;
            v = N::I((v.f() as i64).checked_shl(r.f() as u32)?);
        }
        Some(v)
    }
    fn add(&mut self) -> Option<N> {
        let mut v = self.mul()?;
        while let Some(op) = self.peek().filter(|o| *o == "+" || *o == "-") {
            self.i += 1;
            let r = self.mul()?;
            v = match (v, r) {
                (N::I(a), N::I(b)) => N::I(if op == "+" { a + b } else { a - b }),
                (a, b) => N::F(if op == "+" { a.f() + b.f() } else { a.f() - b.f() }),
            };
        }
        Some(v)
    }
    fn mul(&mut self) -> Option<N> {
        let mut v = self.unary()?;
        while let Some(op) = self.peek().filter(|o| *o == "*" || *o == "/") {
            self.i += 1;
            let r = self.unary()?;
            v = if op == "*" {
                match (v, r) {
                    (N::I(a), N::I(b)) => N::I(a * b),
                    (a, b) => N::F(a.f() * b.f()),
                }
            } else {
                N::F(v.f() / r.f())
            };
        }
        Some(v)
    }
    fn unary(&mut self) -> Option<N> {
        let t = self.peek()?;
        self.i += 1;
        match t {
            "-" => Some(match self.unary()? {
                N::I(i) => N::I(-i),
                N::F(f) => N::F(-f),
            }),
            "+" => self.unary(),
            "(" => {
                let v = self.or()?;
                if self.peek()? != ")" {
                    return None;
                }
                self.i += 1;
                Some(v)
            }
            n => {
                if let Some(h) = n.strip_prefix("0x").or_else(|| n.strip_prefix("0X")) {
                    return i64::from_str_radix(h, 16).ok().map(N::I);
                }
                if n.bytes().all(|c| c.is_ascii_digit()) {
                    if n.len() > 1 && n.starts_with('0') && n.bytes().any(|c| c != b'0') {
                        return None;
                    }
                    return n.parse().ok().map(N::I);
                }
                if n.bytes().next().is_some_and(|c| c.is_ascii_digit() || c == b'.') && n.bytes().all(|c| c.is_ascii_digit() || c == b'.') && n.matches('.').count() == 1 {
                    return n.parse().ok().map(N::F);
                }
                None
            }
        }
    }
}

pub fn num(s: &str) -> R<N> {
    let s = s.trim();
    match s {
        "NULL" | "FALSE" => return Ok(N::I(0)),
        "TRUE" => return Ok(N::I(1)),
        _ => {}
    }
    let toks = lex_num(s).ok_or_else(|| format!("Unsupported numeric expression: {s}"))?;
    let mut p = P { t: toks.iter().map(String::as_str).collect(), i: 0 };
    let v = p.or().filter(|_| p.i == p.t.len());
    v.ok_or_else(|| format!("Unsupported numeric expression: {s}"))
}

fn numf(s: &str) -> R<f64> {
    num(s).map(N::f)
}

fn numi(s: &str) -> R<i64> {
    num(s).map(|n| n.f() as i64)
}

// ---------------------------------------------------------------- preprocessor / tokenising

fn condition(expr: &str) -> R<bool> {
    let known = |m: &str| m == "VERSION_US" || m == "TARGET_N64";
    let e = re(r"defined\s*\(?\s*(\w+)\s*\)?").replace_all(expr, |c: &regex::Captures| if known(&c[1]) { "1" } else { "0" }).into_owned();
    let e = re(r"\b[A-Za-z_]\w*\b").replace_all(&e, |c: &regex::Captures| if known(&c[0]) { "1" } else { "0" }).into_owned();
    // boolean expression: ! && || ( ) and integers
    let toks: Vec<String> = re(r"&&|\|\||!|\(|\)|\w+").find_iter(&e).map(|m| m.as_str().to_string()).collect();
    fn or(t: &[String], i: &mut usize) -> R<i64> {
        let mut v = and(t, i)?;
        while t.get(*i).is_some_and(|x| x == "||") {
            *i += 1;
            let r = and(t, i)?;
            v = if v != 0 { v } else { r };
        }
        Ok(v)
    }
    fn and(t: &[String], i: &mut usize) -> R<i64> {
        let mut v = not(t, i)?;
        while t.get(*i).is_some_and(|x| x == "&&") {
            *i += 1;
            let r = not(t, i)?;
            v = if v == 0 { v } else { r };
        }
        Ok(v)
    }
    fn not(t: &[String], i: &mut usize) -> R<i64> {
        let x = t.get(*i).ok_or("bad preprocessor condition")?.clone();
        *i += 1;
        match x.as_str() {
            "!" => Ok(i64::from(not(t, i)? == 0)),
            "(" => {
                let v = or(t, i)?;
                *i += 1;
                Ok(v)
            }
            n => n.parse::<i64>().map_err(|_| format!("Unsupported preprocessor condition {n}")),
        }
    }
    let mut i = 0;
    Ok(or(&toks, &mut i)? != 0)
}

fn us_source(text: &str) -> R<String> {
    let mut stack: Vec<(bool, bool)> = Vec::new();
    let mut active = true;
    let mut out = Vec::new();
    let rx = re(r"^\s*#(ifdef|ifndef|if|elif|else|endif)\b(.*)");
    for line in text.lines() {
        let Some(m) = rx.captures(line) else {
            if active {
                out.push(line);
            }
            continue;
        };
        let (d, arg) = (&m[1], m[2].trim());
        match d {
            "if" | "ifdef" | "ifndef" => {
                let mut yes = condition(&if d == "if" { arg.to_string() } else { format!("defined({arg})") })?;
                if d == "ifndef" {
                    yes = !yes;
                }
                stack.push((active, yes));
                active = active && yes;
            }
            "else" | "elif" => {
                let (parent, seen) = *stack.last().ok_or("stray #else")?;
                let yes = !seen && (d == "else" || condition(arg)?);
                stack.last_mut().unwrap().1 |= yes;
                active = parent && yes;
            }
            _ => active = stack.pop().ok_or("stray #endif")?.0,
        }
    }
    Ok(out.join("\n"))
}

fn split(s: &str) -> Vec<String> {
    // re.split(r',(?![^()]*\))', s)
    let b = s.as_bytes();
    let mut parts = Vec::new();
    let mut cur = 0;
    for (k, &c) in b.iter().enumerate() {
        if c != b',' {
            continue;
        }
        let first = b[k + 1..].iter().find(|&&x| x == b'(' || x == b')');
        if first == Some(&b')') {
            continue;
        }
        parts.push(&s[cur..k]);
        cur = k + 1;
    }
    parts.push(&s[cur..]);
    parts.into_iter().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect()
}

pub type Cmd = (String, Vec<String>);

pub fn commands(body: &str) -> Vec<Cmd> {
    let b = body.as_bytes();
    let mut out = Vec::new();
    for m in re(r"\b((?:gs|GEO_)\w+)\s*\(").captures_iter(body) {
        let start = m.get(0).unwrap().end();
        let (mut depth, mut end) = (1i32, start);
        while depth != 0 && end < b.len() {
            depth += i32::from(b[end] == b'(') - i32::from(b[end] == b')');
            end += 1;
        }
        out.push((m[1].to_string(), split(&body[start..end - 1])));
    }
    out
}

// The platform libm, like CPython's `math` module: Rust's own f64::sin/cos/powf can differ in the last bit, which would change
// the (printed) accessor min/max of rotation channels.
mod libm {
    extern "C" {
        fn sin(x: f64) -> f64;
        fn cos(x: f64) -> f64;
        fn pow(x: f64, y: f64) -> f64;
    }
    pub fn sin_(x: f64) -> f64 {
        unsafe { sin(x) }
    }
    pub fn cos_(x: f64) -> f64 {
        unsafe { cos(x) }
    }
    pub fn pow_(x: f64, y: f64) -> f64 {
        unsafe { pow(x, y) }
    }
}

fn quat(x: f64, y: f64, z: f64) -> [f64; 4] {
    let (sx, cx) = (libm::sin_(x / 2.0), libm::cos_(x / 2.0));
    let (sy, cy) = (libm::sin_(y / 2.0), libm::cos_(y / 2.0));
    let (sz, cz) = (libm::sin_(z / 2.0), libm::cos_(z / 2.0));
    [sx * cy * cz - cx * sy * sz, cx * sy * cz + sx * cy * sz, cx * cy * sz - sx * sy * cz, cx * cy * cz + sx * sy * sz]
}

// ---------------------------------------------------------------- PNG decal baking

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { 0xEDB88320 ^ (crc >> 1) } else { crc >> 1 };
        }
    }
    !crc
}

fn zlib_decompress(d: &[u8]) -> R<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(d).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn zlib_compress(d: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    e.write_all(d).unwrap();
    e.finish().unwrap()
}

fn be32(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes(d[o..o + 4].try_into().unwrap())
}

fn bake_decal(data: &[u8], color: Option<&[N]>, start_row: usize) -> R<Vec<u8>> {
    let (width, height, depth, kind) = (be32(data, 16) as usize, be32(data, 20) as usize, data[24], data[25]);
    if depth != 8 || ![0, 2, 4, 6].contains(&kind) {
        return Err(format!("Unsupported PNG for decal baking: depth={depth}, kind={kind}"));
    }
    let channels = match kind {
        0 => 1,
        2 => 3,
        4 => 2,
        _ => 4,
    };
    let mut chunks: Vec<u8> = Vec::new();
    let mut offset = 8;
    while offset < data.len() {
        let size = be32(data, offset) as usize;
        if &data[offset + 4..offset + 8] == b"IDAT" {
            chunks.extend_from_slice(&data[offset + 8..offset + 8 + size]);
        }
        offset += size + 12;
    }
    let raw = zlib_decompress(&chunks)?;
    let stride = width * channels;
    let mut previous = vec![0u8; stride];
    let mut out: Vec<u8> = Vec::new();
    let mut pos = 0;
    for y in 0..height {
        let mode = raw[pos];
        pos += 1;
        let mut row = raw[pos..pos + stride].to_vec();
        pos += stride;
        for i in 0..stride {
            let a = if i >= channels { row[i - channels] as i32 } else { 0 };
            let b = previous[i] as i32;
            let c = if i >= channels { previous[i - channels] as i32 } else { 0 };
            let pred = match mode {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                4 => {
                    let p = a + b - c;
                    let dist = [(p - a).abs(), (p - b).abs(), (p - c).abs()];
                    let m = *dist.iter().min().unwrap();
                    [a, b, c][dist.iter().position(|d| *d == m).unwrap()]
                }
                _ => return Err("Unsupported PNG filter".into()),
            };
            row[i] = (row[i] as i32 + pred) as u8;
        }
        if y >= start_row {
            out.push(0);
            for x in 0..width {
                let i = x * channels;
                let alpha: N = if kind == 4 || kind == 6 { N::F(row[i + channels - 1] as f64 / 255.0) } else { N::I(1) };
                let rgb: [u8; 3] = if kind == 0 || kind == 4 { [row[i]; 3] } else { [row[i], row[i + 1], row[i + 2]] };
                for k in 0..3 {
                    out.push(match color {
                        Some(c) => round(rgb[k] as f64 * alpha.f() + c[k].f() * 255.0 * (1.0 - alpha.f())) as u8,
                        None => rgb[k],
                    });
                }
                out.push(if color.is_some() { 255 } else { round(alpha.f() * 255.0) as u8 });
            }
        }
        previous = row;
    }
    let chunk = |tag: &[u8; 4], payload: &[u8]| -> Vec<u8> {
        let mut c = (payload.len() as u32).to_be_bytes().to_vec();
        let mut body = tag.to_vec();
        body.extend_from_slice(payload);
        c.extend_from_slice(&body);
        c.extend_from_slice(&crc32(&body).to_be_bytes());
        c
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&((height - start_row) as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &zlib_compress(&out)));
    png.extend(chunk(b"IEND", b""));
    Ok(png)
}

// ---------------------------------------------------------------- source index

pub struct Source {
    pub refd: PathBuf,
    ext: PathBuf,
    pub arrays: HashMap<String, (String, PathBuf)>,
    /// symbol -> (body, source file); insertion order = file order (sorted), later definitions replace earlier ones in place
    pub geos: Vec<(String, (String, PathBuf))>,
    geo_idx: HashMap<String, usize>,
    lights: HashMap<String, Vec<N>>,
    pub animations: Vec<(String, (Vec<String>, PathBuf))>,
    anim_idx: HashMap<String, usize>,
    textures: HashMap<String, PathBuf>,
    verts: std::cell::RefCell<HashMap<String, Vec<Vec<f64>>>>,
}

fn walk_c(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk_c(&p, out);
            } else if p.extension().is_some_and(|x| x == "c") {
                out.push(p);
            }
        }
    }
}

impl Source {
    /// `refd` = decomp checkout, `ext` = extraction output tree (the PNG textures; `refd` itself may hold them too).
    pub fn new(refd: &Path, ext: &Path) -> R<Source> {
        let mut s = Source { refd: refd.into(), ext: ext.into(), arrays: HashMap::new(), geos: Vec::new(), geo_idx: HashMap::new(), lights: HashMap::new(), animations: Vec::new(), anim_idx: HashMap::new(), textures: HashMap::new(), verts: Default::default() };
        for folder in ["actors", "levels", "textures", "bin", "assets/anims"] {
            let mut files = Vec::new();
            walk_c(&refd.join(folder), &mut files);
            let key = |p: &PathBuf| p.strip_prefix(refd).unwrap().components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>();
            files.sort_by_key(key);
            for path in files {
                let text = us_source(&clean(&read(&path)?))?;
                for m in re(r"(?s)\b(?:const\s+)?(?:GeoLayout|Gfx|Vtx|Texture|u8|u16|s16|u64)\s+(\w+)\s*\[[^\]]*\]\s*=\s*\{(.*?)\};").captures_iter(&text) {
                    let (name, body) = (m[1].to_string(), m[2].to_string());
                    s.arrays.insert(name.clone(), (body.clone(), path.clone()));
                    let head = m[0].split('{').next().unwrap();
                    if head.contains("GeoLayout") {
                        match s.geo_idx.get(&name) {
                            Some(&i) => s.geos[i].1 = (body.clone(), path.clone()),
                            None => {
                                s.geo_idx.insert(name.clone(), s.geos.len());
                                s.geos.push((name.clone(), (body.clone(), path.clone())));
                            }
                        }
                    }
                    let mut offset = 0usize;
                    for inc in re(r#"#include\s+"([^"]+)\.inc\.c""#).captures_iter(&body) {
                        let mut image = refd.join(format!("{}.png", &inc[1]));
                        if !image.exists() {
                            image = ext.join(format!("{}.png", &inc[1]));
                        }
                        if !image.exists() {
                            continue;
                        }
                        s.textures.insert(if offset == 0 { name.clone() } else { format!("{name}+{offset}") }, image.clone());
                        let d = std::fs::read(&image).map_err(|e| e.to_string())?;
                        let (w, h) = (be32(&d, 16) as usize, be32(&d, 20) as usize);
                        let bits = re(r"(?:rgba|ia|i|ci)(4|8|16|32)$").captures(&inc[1]).map_or(16, |b| b[1].parse::<usize>().unwrap());
                        offset += w * h * bits / 8;
                    }
                }
                for m in re(r"(?s)Lights1\s+(\w+)\s*=\s*gdSPDefLights1\((.*?)\)").captures_iter(&text) {
                    let parts = split(&m[2]);
                    let v: Vec<N> = parts[3..6].iter().map(|x| num(x).map(|n| N::F(n.f() / 255.0))).collect::<R<_>>()?;
                    s.lights.insert(m[1].to_string(), v);
                }
                for m in re(r"(?s)struct Animation\s+(\w+)(?:\[\])?\s*=\s*\{(.*?)\};").captures_iter(&text) {
                    let v = (split(&m[2]), path.clone());
                    match s.anim_idx.get(&m[1]) {
                        Some(&i) => s.animations[i].1 = v,
                        None => {
                            s.anim_idx.insert(m[1].to_string(), s.animations.len());
                            s.animations.push((m[1].to_string(), v));
                        }
                    }
                }
            }
        }
        let mut add_geo = |name: &str, body: &str, path: PathBuf| {
            let v = (body.to_string(), path);
            match s.geo_idx.get(name) {
                Some(&i) => s.geos[i].1 = v,
                None => {
                    s.geo_idx.insert(name.to_string(), s.geos.len());
                    s.geos.push((name.to_string(), v));
                }
            }
        };
        add_geo("castle_lobby_light", "GEO_DISPLAY_LIST(LAYER_TRANSPARENT,dl_castle_lobby_wing_cap_light), GEO_END()", refd.join("levels/castle_inside/areas/1/geo.inc.c"));
        add_geo("cap_switch_base", "GEO_DISPLAY_LIST(LAYER_OPAQUE,cap_switch_base_seg5_dl_05003120), GEO_END()", refd.join("actors/capswitch/geo.inc.c"));
        Ok(s)
    }

    pub fn geo(&self, name: &str) -> Option<&(String, PathBuf)> {
        self.geo_idx.get(name).map(|&i| &self.geos[i].1)
    }

    fn vertices(&self, name: &str) -> R<std::cell::Ref<'_, Vec<Vec<f64>>>> {
        if !self.verts.borrow().contains_key(name) {
            let body = &self.arrays.get(name).ok_or_else(|| format!("no array {name}"))?.0;
            let mut rows = Vec::new();
            for m in re(r"(?s)\{\{(.*?)\}\}").captures_iter(body) {
                let vals: Vec<f64> = re(r"-?0x[\da-fA-F]+|-?\d+").find_iter(&m[1]).map(|x| numf(x.as_str())).collect::<R<_>>()?;
                if vals.len() != 10 {
                    return Err(format!("Unexpected vertex in {name}: {vals:?}"));
                }
                rows.push(vals);
            }
            self.verts.borrow_mut().insert(name.to_string(), rows);
        }
        Ok(std::cell::Ref::map(self.verts.borrow(), |m| &m[name]))
    }

    pub fn ints(&self, name: &str) -> R<Vec<i64>> {
        split(&self.arrays.get(name).ok_or_else(|| format!("no array {name}"))?.0).iter().map(|x| numi(x)).collect()
    }
}

// ---------------------------------------------------------------- exporter

#[derive(Clone)]
struct State {
    texture: Option<String>,
    enabled: bool,
    light: Vec<N>,
    lighting: bool,
    cull_back: bool,
    cull_front: bool,
    size: (i64, i64),
    uvscale: (f64, f64),
    wrap: (i64, i64),
    cache: HashMap<i64, Vec<f64>>,
    combine: String,
}

pub struct Export<'a> {
    s: &'a Source,
    symbol: String,
    bin: Vec<u8>,
    texcache: HashMap<(String, (i64, i64), Option<String>), (usize, i64, i64)>,
    matcache: HashMap<String, usize>,
    choices: BTreeMap<String, i64>,
    doc: Map<String, Value>,
    issues: BTreeMap<String, i64>,
    joints: Vec<usize>,
    state: State,
}

pub struct Report {
    pub path: String,
    pub meshes: usize,
    pub joints: usize,
    pub animations: usize,
    pub issues: BTreeMap<String, i64>,
}

fn wrap_of(s: &str) -> i64 {
    if s.contains("CLAMP") {
        33071
    } else if s.contains("MIRROR") && !s.contains("NOMIRROR") {
        33648
    } else {
        10497
    }
}

impl<'a> Export<'a> {
    pub fn new(s: &'a Source, symbol: &str, choices: BTreeMap<String, i64>) -> Export<'a> {
        let mut doc = Map::new();
        doc.insert("asset".into(), json!({"version":"2.0","generator":"Jumpman decomp exporter","extras":{"sm64UnitsToMeters":SCALE,"symbol":symbol}}));
        doc.insert("scene".into(), json!(0));
        doc.insert("scenes".into(), json!([{"nodes":[0]}]));
        for k in ["nodes", "meshes", "materials", "textures", "images", "samplers", "bufferViews", "accessors", "animations"] {
            doc.insert(k.into(), json!([]));
        }
        doc.insert("extensionsUsed".into(), json!(["KHR_materials_unlit"]));
        Export {
            s,
            symbol: symbol.into(),
            bin: Vec::new(),
            texcache: HashMap::new(),
            matcache: HashMap::new(),
            choices,
            doc,
            issues: BTreeMap::new(),
            joints: Vec::new(),
            state: State { texture: None, enabled: false, light: vec![N::I(1); 3], lighting: true, cull_back: true, cull_front: false, size: (32, 32), uvscale: (1.0, 1.0), wrap: (10497, 10497), cache: HashMap::new(), combine: String::new() },
        }
    }

    fn arr(&mut self, k: &str) -> &mut Vec<Value> {
        self.doc.get_mut(k).unwrap().as_array_mut().unwrap()
    }

    fn view(&mut self, data: &[u8]) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(data);
        let v = self.arr("bufferViews");
        v.push(json!({"buffer":0,"byteOffset":offset,"byteLength":data.len()}));
        v.len() - 1
    }

    fn accessor(&mut self, rows: &[Vec<N>], kind: &str, integer: bool) -> usize {
        let size = match kind {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            _ => 4,
        };
        let flat: Vec<N> = rows.iter().flatten().copied().collect();
        let mut bytes = Vec::with_capacity(flat.len() * 4);
        for v in &flat {
            if integer {
                bytes.extend_from_slice(&(v.f() as u32).to_le_bytes());
            } else {
                bytes.extend_from_slice(&(v.f() as f32).to_le_bytes());
            }
        }
        let view = self.view(&bytes);
        let (mut mins, mut maxs) = (Vec::new(), Vec::new());
        for c in 0..size {
            let mut it = flat.iter().skip(c).step_by(size).copied();
            let first = it.next().unwrap();
            let (mut lo, mut hi) = (first, first);
            for v in it {
                if v.lt(lo) {
                    lo = v;
                }
                if hi.lt(v) {
                    hi = v;
                }
            }
            mins.push(lo.json());
            maxs.push(hi.json());
        }
        let d = json!({"bufferView":view,"componentType":if integer { 5125 } else { 5126 },"count":flat.len() / size,"type":kind,"min":mins,"max":maxs});
        let a = self.arr("accessors");
        a.push(d);
        a.len() - 1
    }

    fn material(&mut self, layer: &str) -> R<usize> {
        let st = self.state.clone();
        let tex: Option<String> = if st.enabled { st.texture.clone() } else { None };
        let blend = st.combine.contains("BLENDRGB");
        let light_key = st.light.iter().map(|c| c.f().to_bits().to_string()).collect::<Vec<_>>().join(",");
        let tk = (tex.clone().unwrap_or_default(), st.wrap, if blend { Some(light_key.clone()) } else { None });
        let key = format!("{:?}|{light_key}|{:?}|{layer}|{}|{}|{}|{}", tex, st.wrap, st.lighting, st.combine, st.cull_back, st.cull_front);
        if let Some(&m) = self.matcache.get(&key) {
            if tex.is_some() {
                if let Some(&(_, w, h)) = self.texcache.get(&tk) {
                    self.state.size = (w, h);
                }
            }
            return Ok(m);
        }
        let mut color: Vec<N> = if st.lighting { st.light.iter().map(|c| N::F(if c.f() <= 0.04045 { c.f() / 12.92 } else { libm::pow_((c.f() + 0.055) / 1.055, 2.4) })).collect() } else { vec![N::I(1); 3] };
        if tex.is_some() && (blend || st.combine.contains("DECALRGB")) {
            color = vec![N::I(1); 3];
        }
        let mut bc: Vec<Value> = color.iter().map(|c| c.json()).collect();
        bc.push(json!(1));
        let name = match &tex {
            Some(t) if !t.is_empty() => t.clone(),
            _ => list_repr(&color),
        };
        let mut pbr = json!({"baseColorFactor":bc,"metallicFactor":0,"roughnessFactor":1});
        let mut mat = json!({"name":name,"pbrMetallicRoughness":pbr.clone(),"doubleSided":!(st.cull_back || st.cull_front)});
        if !st.lighting {
            mat["extensions"] = json!({"KHR_materials_unlit":{}});
        }
        if let Some(t) = tex.as_ref().filter(|t| !t.is_empty()) {
            let mut path = self.s.textures.get(t).cloned();
            let mut byte_offset = 0usize;
            if path.is_none() && t.contains('+') {
                let (base, off) = t.rsplit_once('+').unwrap();
                path = self.s.textures.get(base).cloned();
                byte_offset = off.parse().map_err(|_| "texture offset")?;
            }
            if let Some(path) = path {
                if !self.texcache.contains_key(&tk) {
                    let mut data = std::fs::read(&path).map_err(|e| e.to_string())?;
                    let (mut w, mut h) = (be32(&data, 16) as i64, be32(&data, 20) as i64);
                    let pname = path.file_name().unwrap().to_string_lossy().into_owned();
                    if byte_offset != 0 {
                        let bits = re(r"(?:rgba|ia|i|ci)(4|8|16|32)\.png$").captures(&pname).map_or(16, |b| b[1].parse::<usize>().unwrap());
                        let rowbytes = w as usize * bits / 8;
                        if byte_offset % rowbytes != 0 {
                            return Err("Texture offset is not row aligned".into());
                        }
                        data = bake_decal(&data, None, byte_offset / rowbytes)?;
                        w = be32(&data, 16) as i64;
                        h = be32(&data, 20) as i64;
                    }
                    if blend {
                        data = bake_decal(&data, Some(&st.light), 0)?;
                    }
                    let bv = self.view(&data);
                    self.arr("images").push(json!({"bufferView":bv,"mimeType":"image/png","name":pname}));
                    let image = self.arr("images").len() - 1;
                    self.arr("samplers").push(json!({"magFilter":9728,"minFilter":9728,"wrapS":st.wrap.0,"wrapT":st.wrap.1}));
                    let sampler = self.arr("samplers").len() - 1;
                    self.arr("textures").push(json!({"source":image,"sampler":sampler}));
                    let index = self.arr("textures").len() - 1;
                    self.texcache.insert(tk.clone(), (index, w, h));
                }
                let (index, w, h) = self.texcache[&tk];
                self.state.size = (w, h);
                pbr["baseColorTexture"] = json!({"index":index});
                mat["pbrMetallicRoughness"] = pbr;
            } else {
                *self.issues.entry(format!("missing texture {t}")).or_insert(0) += 1;
            }
        }
        if layer.contains("ALPHA") {
            mat["alphaMode"] = json!("MASK");
            mat["alphaCutoff"] = json!(0.5);
        } else if layer.contains("TRANSPARENT") {
            mat["alphaMode"] = json!("BLEND");
        }
        let m = self.arr("materials");
        m.push(mat);
        let idx = m.len() - 1;
        self.matcache.insert(key, idx);
        Ok(idx)
    }

    #[allow(clippy::too_many_lines)]
    fn display(&mut self, name: &str, layer: &str) -> R<Option<usize>> {
        let mut prim_order: Vec<usize> = Vec::new();
        let mut prims: HashMap<usize, Vec<([f64; 3], [f64; 3], [f64; 2], Vec<N>)>> = HashMap::new();
        self.run(name, layer, 0, &mut prim_order, &mut prims)?;
        let mut parts = Vec::new();
        for mat in prim_order {
            let verts = &prims[&mat];
            if verts.is_empty() {
                continue;
            }
            let pos: Vec<Vec<N>> = verts.iter().map(|v| v.0.iter().map(|x| N::F(*x)).collect()).collect();
            let norm: Vec<Vec<N>> = verts.iter().map(|v| v.1.iter().map(|x| N::F(*x)).collect()).collect();
            let uv: Vec<Vec<N>> = verts.iter().map(|v| v.2.iter().map(|x| N::F(*x)).collect()).collect();
            let color: Vec<Vec<N>> = verts.iter().map(|v| v.3.clone()).collect();
            let idx: Vec<Vec<N>> = (0..verts.len()).map(|i| vec![N::I(i as i64)]).collect();
            let (p, n, u, c, i) = (self.accessor(&pos, "VEC3", false), self.accessor(&norm, "VEC3", false), self.accessor(&uv, "VEC2", false), self.accessor(&color, "VEC4", false), self.accessor(&idx, "SCALAR", true));
            parts.push(json!({"attributes":{"POSITION":p,"NORMAL":n,"TEXCOORD_0":u,"COLOR_0":c},"indices":i,"material":mat}));
        }
        if parts.is_empty() {
            return Ok(None);
        }
        let m = self.arr("meshes");
        m.push(json!({"name":name,"primitives":parts}));
        Ok(Some(m.len() - 1))
    }

    #[allow(clippy::too_many_lines)]
    fn run(&mut self, symbol: &str, layer: &str, depth: usize, order: &mut Vec<usize>, prims: &mut HashMap<usize, Vec<([f64; 3], [f64; 3], [f64; 2], Vec<N>)>>) -> R<()> {
        if depth > 60 {
            return Err(format!("Recursive display list {symbol}"));
        }
        let body = match self.s.arrays.get(symbol) {
            Some((b, _)) => b.clone(),
            None => {
                if symbol != "NULL" {
                    *self.issues.entry(format!("missing display {symbol}")).or_insert(0) += 1;
                }
                return Ok(());
            }
        };
        for (cmd, a) in commands(&body) {
            match cmd.as_str() {
                "gsSPDisplayList" | "gsSPBranchList" => {
                    self.run(&a[0], layer, depth + 1, order, prims)?;
                    if cmd == "gsSPBranchList" {
                        break;
                    }
                }
                "gsSPVertex" => {
                    let m = re(r"^&?(\w+)(?:\s*\+\s*(\d+)|\[(\d+)\])?").captures(&a[0]).ok_or("vertex ref")?;
                    let vn = m[1].to_string();
                    let offset: usize = m.get(2).or(m.get(3)).map_or(0, |x| x.as_str().parse().unwrap());
                    let count = numi(&a[1])?;
                    let base = numi(&a[2])?;
                    let verts = self.s.vertices(&vn)?.clone();
                    for k in 0..count {
                        if offset + k as usize >= verts.len() {
                            continue;
                        }
                        self.state.cache.insert(base + k, verts[offset + k as usize].clone());
                    }
                }
                "gsSP1Triangle" | "gsSP2Triangles" => {
                    let which: &[usize] = if cmd == "gsSP1Triangle" { &[0, 1, 2] } else { &[0, 1, 2, 4, 5, 6] };
                    let mut indices: Vec<i64> = which.iter().map(|i| numi(&a[*i])).collect::<R<_>>()?;
                    if self.state.cull_back && self.state.cull_front {
                        continue;
                    }
                    if self.state.cull_front {
                        for i in (0..indices.len()).step_by(3) {
                            indices.swap(i + 1, i + 2);
                        }
                    }
                    let mat = self.material(layer)?;
                    let (w, h) = self.state.size;
                    if !prims.contains_key(&mat) {
                        order.push(mat);
                    }
                    for i in indices {
                        let v = self.state.cache.get(&i).ok_or_else(|| format!("KeyError: {i}"))?.clone();
                        let nrm: Vec<f64> = v[6..9].iter().map(|n| (if *n < 128.0 { *n } else { *n - 256.0 }) / 127.0).collect();
                        let color: Vec<N> = if !self.state.lighting { v[6..10].iter().map(|c| N::F(c / 255.0)).collect() } else { vec![N::I(1); 4] };
                        prims.entry(mat).or_default().push(([v[0] * SCALE, v[1] * SCALE, v[2] * SCALE], [nrm[0], nrm[1], nrm[2]], [v[4] * self.state.uvscale.0 / (32.0 * w as f64), v[5] * self.state.uvscale.1 / (32.0 * h as f64)], color));
                    }
                }
                "gsDPSetTextureImage" | "gsDPLoadTextureBlock" | "gsDPLoadTextureBlock_4b" => {
                    let t = if cmd == "gsDPSetTextureImage" { a[a.len() - 1].clone() } else { a[0].clone() };
                    let mut expression = re(r"\([^)]*\*\)").replace_all(&t, "").replace(' ', "");
                    if let Some((base, off)) = expression.split_once('+') {
                        expression = format!("{base}+{}", numi(off)?);
                    }
                    self.state.texture = Some(expression);
                    if cmd != "gsDPSetTextureImage" {
                        let start = if cmd == "gsDPLoadTextureBlock" { 6 } else { 5 };
                        self.state.wrap = (wrap_of(&a[start]), wrap_of(&a[start + 1]));
                    }
                }
                "gsSPTexture" => {
                    self.state.enabled = !matches!(a[a.len() - 1].as_str(), "G_OFF" | "0");
                    self.state.uvscale = (numf(&a[0])? / 65536.0, numf(&a[1])? / 65536.0);
                }
                "gsDPSetCombineMode" => self.state.combine = a[0].clone(),
                "gsSPLight" if a[1] == "1" => {
                    let ln = re(r"^&|\.[la]$").replace_all(&a[0], "").into_owned();
                    self.state.light = self.s.lights.get(&ln).cloned().unwrap_or_else(|| vec![N::I(1); 3]);
                }
                "gsSPSetGeometryMode" | "gsSPClearGeometryMode" | "gsSPGeometryMode" | "gsSPGeometryModeSetFirst" => {
                    let modes: Vec<(String, bool)> = match cmd.as_str() {
                        "gsSPGeometryMode" => vec![(a[0].clone(), false), (a[1].clone(), true)],
                        "gsSPGeometryModeSetFirst" => vec![(a[0].clone(), true), (a[1].clone(), false)],
                        _ => vec![(a[0].clone(), cmd == "gsSPSetGeometryMode")],
                    };
                    for (flags, enabled) in modes {
                        if flags.contains("G_LIGHTING") {
                            self.state.lighting = enabled;
                        }
                        if flags.contains("G_CULL_BACK") || flags.contains("G_CULL_BOTH") {
                            self.state.cull_back = enabled;
                        }
                        if flags.contains("G_CULL_FRONT") || flags.contains("G_CULL_BOTH") {
                            self.state.cull_front = enabled;
                        }
                    }
                }
                "gsDPSetTile" if a.len() > 10 && a[4] == "G_TX_RENDERTILE" => {
                    self.state.wrap = (wrap_of(&a[9]), wrap_of(&a[6]));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn node(&mut self, cmd: &str, args: &[String], parent: Option<usize>) -> R<usize> {
        let idx = self.arr("nodes").len();
        let mut n = Map::new();
        n.insert("name".into(), json!(format!("{cmd}_{idx}")));
        n.insert("extras".into(), json!({"sm64Command":cmd,"sm64Args":args}));
        let layer = args.first().cloned().unwrap_or_else(|| "LAYER_OPAQUE".into());
        let mut dl: Option<String> = None;
        let tr = |a: &[String]| -> R<Vec<f64>> { a.iter().map(|x| numf(x).map(|v| v * SCALE)).collect() };
        if cmd == "GEO_SCALE" {
            let sc = numf(&args[1])? / 65536.0;
            n.insert("scale".into(), json!([sc, sc, sc]));
        } else if cmd == "GEO_ANIMATED_PART" {
            n.insert("translation".into(), json!(tr(&args[1..4])?));
            dl = Some(args[4].clone());
            self.joints.push(idx);
        } else if cmd == "GEO_DISPLAY_LIST" {
            dl = Some(args[1].clone());
        } else if cmd.starts_with("GEO_TRANSLATE_ROTATE") {
            n.insert("translation".into(), json!(tr(&args[1..4])?));
            let r: Vec<f64> = args[4..7].iter().map(|x| numf(x).map(f64::to_radians)).collect::<R<_>>()?;
            n.insert("rotation".into(), json!(quat(r[0], r[1], r[2])));
            if cmd.ends_with("_WITH_DL") {
                dl = Some(args[7].clone());
            }
        } else if cmd.starts_with("GEO_TRANSLATE") || cmd.starts_with("GEO_BILLBOARD_WITH_PARAMS") {
            n.insert("translation".into(), json!(tr(&args[1..4])?));
            if cmd.ends_with("_WITH_DL") {
                dl = Some(args[4].clone());
            }
        } else if cmd.starts_with("GEO_ROTATION_NODE") {
            let r: Vec<f64> = args[1..4].iter().map(|x| numf(x).map(f64::to_radians)).collect::<R<_>>()?;
            n.insert("rotation".into(), json!(quat(r[0], r[1], r[2])));
            if cmd.ends_with("_WITH_DL") {
                dl = Some(args[4].clone());
            }
        }
        // the node must exist before its mesh is built (`display` allocates accessors but never nodes)
        self.arr("nodes").push(Value::Object(n));
        if let Some(p) = parent {
            let pn = self.arr("nodes")[p].as_object_mut().unwrap();
            pn.entry("children").or_insert_with(|| json!([])).as_array_mut().unwrap().push(json!(idx));
        }
        if let Some(d) = dl.filter(|d| d != "NULL") {
            if let Some(mesh) = self.display(&d, &layer)? {
                self.arr("nodes")[idx].as_object_mut().unwrap().insert("mesh".into(), json!(mesh));
            }
        }
        Ok(idx)
    }

    fn node_cmd(&self, i: usize) -> String {
        self.doc["nodes"][i]["extras"]["sm64Command"].as_str().unwrap().to_string()
    }

    #[allow(clippy::too_many_lines)]
    fn geo(&mut self, symbol: &str, parent: Option<usize>, depth: usize) -> R<()> {
        if symbol == "NULL" {
            return Ok(());
        }
        if depth > 40 {
            return Err(format!("Recursive geo {symbol}"));
        }
        let Some((body, _)) = self.s.geo(symbol).cloned() else {
            *self.issues.entry(format!("missing geo {symbol}")).or_insert(0) += 1;
            return Ok(());
        };
        let mut stack: Vec<Option<usize>> = vec![parent];
        let mut last = parent;
        let mut switch_counts: HashMap<usize, i64> = HashMap::new();
        let mut switch_states: HashMap<usize, State> = HashMap::new();
        let mut skip_depth = 0;
        for (cmd, a) in commands(&body) {
            if skip_depth != 0 {
                if cmd == "GEO_OPEN_NODE" {
                    skip_depth += 1;
                } else if cmd == "GEO_CLOSE_NODE" {
                    skip_depth -= 1;
                }
                continue;
            }
            if cmd == "GEO_OPEN_NODE" {
                stack.push(last);
                continue;
            }
            if cmd == "GEO_CLOSE_NODE" {
                if stack.len() > 1 {
                    last = stack.pop().unwrap();
                }
                continue;
            }
            if cmd == "GEO_END" || cmd == "GEO_RETURN" {
                break;
            }
            let p = *stack.last().unwrap();
            if let Some(pi) = p {
                if self.node_cmd(pi) == "GEO_SWITCH_CASE" {
                    switch_states.entry(pi).or_insert_with(|| self.state.clone());
                    self.state = switch_states[&pi].clone();
                    let count = *switch_counts.get(&pi).unwrap_or(&0);
                    switch_counts.insert(pi, count + 1);
                    let callback = self.doc["nodes"][pi]["extras"]["sm64Args"].as_array().unwrap().last().unwrap().as_str().unwrap().to_string();
                    let selected = *self.choices.get(&callback).unwrap_or(&0);
                    if count != selected && !(callback == "geo_switch_mario_eyes" && !self.choices.contains_key(&callback)) {
                        let l = self.node("GEO_NODE_START", &[], p)?;
                        let nd = self.arr("nodes")[l].as_object_mut().unwrap();
                        nd.insert("scale".into(), json!([0, 0, 0]));
                        nd["extras"].as_object_mut().unwrap().insert("inactive".into(), json!(true));
                        last = Some(l);
                        continue;
                    }
                }
                if self.doc["nodes"][pi]["extras"].get("inactive").is_some_and(|v| v == true) {
                    let l = self.node("GEO_NODE_START", &[], p)?;
                    self.arr("nodes")[l]["extras"].as_object_mut().unwrap().insert("inactive".into(), json!(true));
                    last = Some(l);
                    continue;
                }
            }
            if cmd == "GEO_BRANCH" || cmd == "GEO_BRANCH_AND_LINK" {
                let l = self.node(&cmd, &a, p)?;
                last = Some(l);
                self.geo(&a[a.len() - 1], Some(l), depth + 1)?;
            } else if cmd == "GEO_RENDER_RANGE" && (if let Some(m) = self.choices.get("_lod_min") { numf(&a[0])? != *m as f64 } else { numf(&a[0])? > 0.0 }) {
                let l = self.node("GEO_NODE_START", &[], p)?;
                self.arr("nodes")[l]["extras"].as_object_mut().unwrap().insert("inactive".into(), json!(true));
                last = Some(l);
            } else {
                last = Some(self.node(&cmd, &a, p)?);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn animate(&mut self) -> R<()> {
        if self.joints.is_empty() {
            return Ok(());
        }
        let srcpath = self.s.geo(&self.symbol).ok_or("geo vanished")?.1.clone();
        let rel = srcpath.strip_prefix(&self.s.refd).unwrap().to_path_buf();
        let actor: Vec<String> = rel.components().take(2).map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        let root = self.joints[0];
        let rootbase: Vec<f64> = self.doc["nodes"][root].get("translation").and_then(Value::as_array).map_or(vec![0.0; 3], |t| t.iter().map(|x| x.as_f64().unwrap()).collect());
        let mut firstpose: Option<Vec<f64>> = None;
        let anims = self.s.animations.clone();
        for (name, (a, path)) in &anims {
            let is_mario = actor == ["actors", "mario"] && path.starts_with(self.s.refd.join("assets/anims"));
            let actor_dir = self.s.refd.join(&actor[0]).join(&actor[1]).join("anims");
            if !is_mario && !path.starts_with(&actor_dir) {
                continue;
            }
            let hdr: Vec<i64> = a[..5].iter().map(|x| numi(x)).collect::<R<_>>()?;
            let (flags, divisor, start, loopstart, frames) = (hdr[0], hdr[1], hdr[2], hdr[3], hdr[4]);
            let values: Vec<i64> = self.s.ints(&a[6])?.into_iter().map(|v| if v < 32768 { v } else { v - 65536 }).collect();
            let indices = self.s.ints(&a[7])?;
            let count = (indices.len() / 6) as i64 - 1;
            if is_mario && self.joints.len() != 20 {
                continue;
            }
            if count < self.joints.len() as i64 {
                *self.issues.entry(format!("animation joint count {name}: {count} != {}", self.joints.len())).or_insert(0) += 1;
            }
            let times: Vec<Vec<N>> = (0..frames).map(|f| vec![N::F(f as f64 / 30.0)]).collect();
            let times = self.accessor(&times, "SCALAR", false);
            let mut animation = json!({"name":name,"samplers":[],"channels":[],"extras":{"fps":30,"startFrame":start,"loopStart":loopstart,"loop":flags & 1 == 0,"backward":flags & 2 != 0,"flags":flags}});
            let sample = |component: usize, frame: i64| -> i64 {
                let (n, offset) = (indices[component * 2], indices[component * 2 + 1]);
                values[(offset + frame.min(n - 1)) as usize]
            };
            let multiplier = if divisor != 0 && is_mario { 189.0 / divisor as f64 } else { 1.0 };
            let mut translation: Vec<Vec<N>> = Vec::new();
            for f in 0..frames {
                let mut tr: Vec<f64> = (0..3).map(|i| sample(i, f) as f64 * SCALE * multiplier).collect();
                if flags & 8 != 0 {
                    tr[0] = 0.0;
                    tr[2] = 0.0;
                } else if flags & 16 != 0 {
                    tr[1] = 0.0;
                } else if flags & 64 != 0 {
                    tr = vec![0.0; 3];
                }
                translation.push((0..3).map(|i| N::F(rootbase[i] + tr[i])).collect());
            }
            let mut channels: Vec<(usize, &str, Vec<Vec<N>>, &str)> = vec![(root, "translation", translation.clone(), "VEC3")];
            if firstpose.is_none() {
                firstpose = Some(translation[0].iter().map(|n| n.f()).collect());
            }
            let tau = std::f64::consts::TAU;
            for (j, node) in self.joints.clone().iter().take(count.max(0) as usize).enumerate() {
                let rotations: Vec<Vec<N>> = (0..frames)
                    .map(|f| {
                        let r: Vec<f64> = (0..3).map(|i| sample(3 + j * 3 + i, f) as f64 * tau / 65536.0).collect();
                        quat(r[0], r[1], r[2]).iter().map(|x| N::F(*x)).collect()
                    })
                    .collect();
                channels.push((*node, "rotation", rotations, "VEC4"));
            }
            for (k, (node, path_name, data, kind)) in channels.into_iter().enumerate() {
                let out = self.accessor(&data, kind, false);
                animation["samplers"].as_array_mut().unwrap().push(json!({"input":times,"output":out,"interpolation":"STEP"}));
                animation["channels"].as_array_mut().unwrap().push(json!({"sampler":k,"target":{"node":node,"path":path_name}}));
            }
            self.arr("animations").push(animation);
        }
        if actor == ["actors", "door"] {
            if let Some(fp) = firstpose {
                self.arr("nodes")[root].as_object_mut().unwrap().insert("translation".into(), json!(fp));
            }
        }
        Ok(())
    }

    pub fn save(mut self, path: &Path, root: &Path) -> R<Report> {
        let symbol = self.symbol.clone();
        self.geo(&symbol, None, 0)?;
        self.animate()?;
        let geometryless = self.doc["meshes"].as_array().unwrap().is_empty();
        self.doc["asset"]["extras"]["geometryless"] = json!(geometryless);
        if self.bin.is_empty() {
            self.bin.extend_from_slice(&[0; 4]);
        }
        self.doc.insert("buffers".into(), json!([{"byteLength":self.bin.len()}]));
        let keys: Vec<String> = self.doc.keys().cloned().collect();
        for k in keys {
            if self.doc[&k].as_array().is_some_and(Vec::is_empty) {
                self.doc.shift_remove(&k);
            }
        }
        let mut raw = dumps(&Value::Object(self.doc.clone()), None).into_bytes();
        while raw.len() % 4 != 0 {
            raw.push(b' ');
        }
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let mut data = Vec::new();
        data.extend_from_slice(&0x46546c67u32.to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&((28 + raw.len() + self.bin.len()) as u32).to_le_bytes());
        data.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        data.extend_from_slice(&0x4e4f534au32.to_le_bytes());
        data.extend_from_slice(&raw);
        data.extend_from_slice(&(self.bin.len() as u32).to_le_bytes());
        data.extend_from_slice(&0x004e4942u32.to_le_bytes());
        data.extend_from_slice(&self.bin);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Report {
            path: path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned(),
            meshes: self.doc.get("meshes").and_then(Value::as_array).map_or(0, Vec::len),
            joints: self.joints.len(),
            animations: self.doc.get("animations").and_then(Value::as_array).map_or(0, Vec::len),
            issues: self.issues.clone(),
        })
    }
}

// ---------------------------------------------------------------- driver

/// `main()` of export-sm64.py (the `--all` default): writes `<out>/assets/{actors,levels}/**/*.glb`, `animations.json`,
/// `layouts.json`, `export-report.json` and copies the extracted resources to `<out>/assets/extracted`.
pub fn export_all(refd: &Path, ext: &Path, out: &Path, assets_json: &[u8], log: &mut dyn FnMut(&str)) -> R<()> {
    let source = Source::new(refd, ext)?;
    let assets = out.join("assets");
    let mut required: HashSet<String> = ["actors/mario/mario_geo.glb", "actors/coin/yellow_coin_no_shadow_geo.glb", "actors/star/star_geo.glb", "actors/goomba/goomba_geo.glb", "actors/tree/bubbly_tree_geo.glb"].iter().map(|s| s.to_string()).collect();
    if let Ok(rd) = std::fs::read_dir(assets.join("courses")) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "json") {
                if let Ok(Value::Object(d)) = serde_json::from_str::<Value>(&read(&e.path())?) {
                    if let Some(m) = d.get("model").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                        required.insert(m.to_string());
                    }
                    for o in d.get("objects").and_then(Value::as_array).map_or(&[][..], Vec::as_slice) {
                        if let Some(m) = o.get("model").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                            required.insert(m.to_string());
                        }
                    }
                }
            }
        }
    }
    let rel_parts = |p: &Path| -> Vec<String> { p.strip_prefix(refd).unwrap().components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect() };
    for (symbol, (_, path)) in &source.geos {
        let parts = rel_parts(path);
        required.insert(format!("{}/{}/{symbol}.glb", parts[0], parts[1]));
    }
    required.retain(|r| !r.contains("/variants/"));
    let mut sorted: Vec<String> = required.into_iter().collect();
    sorted.sort();
    let mut report: Vec<Value> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();
    let rep_json = |r: &Report, sel: Option<Value>| {
        let mut v = json!({"path":r.path,"meshes":r.meshes,"joints":r.joints,"animations":r.animations,"issues":r.issues});
        if let Some(s) = sel {
            v["selection"] = s;
        }
        v
    };
    for rel in &sorted {
        let symbol = Path::new(rel).file_stem().unwrap().to_string_lossy().into_owned();
        match Export::new(&source, &symbol, BTreeMap::new()).save(&assets.join(rel), out) {
            Ok(r) => {
                log(&format!("{rel}: {} meshes, {} animations, {} issue types", r.meshes, r.animations, r.issues.len()));
                report.push(rep_json(&r, None));
            }
            Err(e) => {
                log(&format!("FAILED {rel}: {e}"));
                failed.push(json!({"path":rel,"error":e}));
            }
        }
    }
    for (symbol, (body, path)) in &source.geos {
        let cmds = commands(body);
        let mut cases: Vec<(String, usize)> = Vec::new();
        for (i, (cmd, a)) in cmds.iter().enumerate() {
            if cmd != "GEO_SWITCH_CASE" {
                continue;
            }
            let (mut depth, mut count) = (0i32, 0usize);
            for (child, _) in &cmds[i + 1..] {
                if child == "GEO_OPEN_NODE" {
                    depth += 1;
                } else if child == "GEO_CLOSE_NODE" {
                    depth -= 1;
                    if depth <= 0 {
                        break;
                    }
                } else if depth == 1 {
                    count += 1;
                }
            }
            let k = a[a.len() - 1].clone();
            match cases.iter_mut().find(|(n, _)| *n == k) {
                Some(e) => e.1 = e.1.max(count),
                None => cases.push((k, count)),
            }
        }
        let parts = rel_parts(path);
        let base = format!("{}/{}", parts[0], parts[1]);
        for (callback, count) in cases {
            for case in 1..count {
                let rel = format!("{base}/variants/{symbol}/{callback}_{case}.glb");
                let choices = BTreeMap::from([(callback.clone(), case as i64)]);
                match Export::new(&source, symbol, choices).save(&assets.join(&rel), out) {
                    Ok(r) => report.push(rep_json(&r, Some(json!({callback.clone():case})))),
                    Err(e) => failed.push(json!({"path":rel,"error":e})),
                }
            }
        }
        let mut mins: Vec<i64> = Vec::new();
        for (cmd, a) in &cmds {
            if cmd == "GEO_RENDER_RANGE" {
                if let Ok(v) = num(&a[0]) {
                    if v.f() > 0.0 {
                        mins.push(v.f() as i64);
                    }
                }
            }
        }
        mins.sort_unstable();
        mins.dedup();
        for minimum in mins {
            let rel = format!("{base}/variants/{symbol}/lod_{minimum}.glb");
            match Export::new(&source, symbol, BTreeMap::from([("_lod_min".to_string(), minimum)])).save(&assets.join(&rel), out) {
                Ok(r) => report.push(rep_json(&r, Some(json!({"lodMinimum":minimum})))),
                Err(e) => failed.push(json!({"path":rel,"error":e})),
            }
        }
    }
    // extracted resources
    let map: Value = serde_json::from_slice(assets_json).map_err(|e| e.to_string())?;
    let mut extracted = 0;
    for (rel, info) in map.as_object().ok_or("assets.json")? {
        let last = info.as_array().and_then(|a| a.last());
        if rel.starts_with('@') || !last.is_some_and(|l| l.get("us").is_some()) {
            continue;
        }
        let dest = assets.join("extracted").join(rel);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        let src = [refd.join(rel), ext.join(rel)].into_iter().find(|p| p.is_file());
        match src {
            Some(s) => {
                std::fs::copy(s, &dest).map_err(|e| e.to_string())?;
            }
            None if dest.is_file() => {}
            None => return Err(format!("FileNotFoundError: {rel}")),
        }
        extracted += 1;
    }
    let mut anims = Map::new();
    for (name, (a, path)) in &source.animations {
        anims.insert(name.clone(), json!({"source":path.strip_prefix(refd).unwrap().to_string_lossy(),"header":a[..6],"values":source.ints(&a[6])?,"indices":source.ints(&a[7])?}));
    }
    std::fs::write(assets.join("animations.json"), dumps(&Value::Object(anims.clone()), None)).map_err(|e| e.to_string())?;
    let mut layouts = Map::new();
    for (name, (body, path)) in &source.geos {
        let cmds: Vec<Value> = commands(body).into_iter().map(|(c, a)| json!([c, a])).collect();
        layouts.insert(name.clone(), json!({"source":path.strip_prefix(refd).unwrap().to_string_lossy(),"commands":cmds}));
    }
    std::fs::write(assets.join("layouts.json"), dumps(&Value::Object(layouts.clone()), None)).map_err(|e| e.to_string())?;
    let rep = json!({"exported":report,"failed":failed,"sourceLayouts":layouts.len(),"sourceAnimations":anims.len(),"extractedResources":extracted});
    std::fs::write(assets.join("export-report.json"), dumps(&rep, Some(2))).map_err(|e| e.to_string())?;
    log(&format!("Exported {}; failed {}", report.len(), failed.len()));
    if !failed.is_empty() {
        return Err(format!("{} models failed", failed.len()));
    }
    let _ = int;
    Ok(())
}
