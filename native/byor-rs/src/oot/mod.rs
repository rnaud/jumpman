//! Ocarina of Time (US 1.2) import in Rust: the port of the OoT branch of `byor/pipeline.py` (`do_oot`) and its exporters
//! (`scripts/export-oot-*.py`, `export-character-voices.py`, `oot-residents.py`, `oot-scenery.py`). Each stage writes the same
//! files as the Python one, byte for byte where the tests can check it (tests/oot_rom.rs).
use crate::pyc::R;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub mod courses;
pub mod enemies;
pub mod link;
pub mod model;
pub mod scenery;
pub mod sfx;
pub mod text;
pub mod voices;
pub mod xml;

/// The platform libm, like CPython's `math` module (Rust's own `f64::sin` / `powf` may differ in the last bit).
pub mod libm {
    extern "C" {
        fn sin(x: f64) -> f64;
        fn cos(x: f64) -> f64;
        fn pow(x: f64, y: f64) -> f64;
        fn atan2(y: f64, x: f64) -> f64;
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
    pub fn atan2_(y: f64, x: f64) -> f64 {
        unsafe { atan2(y, x) }
    }
}

pub const MD5: &str = "57a9719ad547c516342e1a15d5c28c3d";
pub const MASK: u32 = 0xff_ffff;

// ---- big-endian readers (a short read is a corrupt ROM; callers run under `catch_unwind` in `run`) ----
pub fn u8_(d: &[u8], p: usize) -> u8 {
    d[p]
}
pub fn i8_(d: &[u8], p: usize) -> i8 {
    d[p] as i8
}
pub fn u16_(d: &[u8], p: usize) -> u16 {
    u16::from_be_bytes([d[p], d[p + 1]])
}
pub fn i16_(d: &[u8], p: usize) -> i16 {
    u16_(d, p) as i16
}
pub fn u32_(d: &[u8], p: usize) -> u32 {
    u32::from_be_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]])
}
pub fn i32_(d: &[u8], p: usize) -> i32 {
    u32_(d, p) as i32
}
pub fn f32_(d: &[u8], p: usize) -> f32 {
    f32::from_bits(u32_(d, p))
}

/// `round(x, n)`: correctly rounded decimal (half to even on the exact binary value), like CPython.
pub fn round_to(x: f64, digits: usize) -> f64 {
    format!("{x:.digits$}").parse().unwrap_or(x)
}

/// Attributes of one XML element (`Name`, `Offset`, `Format`, `Width` ...).
#[derive(Debug, Clone, Default)]
pub struct Attrs(pub Vec<(String, String)>);

impl Attrs {
    pub fn get(&self, k: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
    pub fn has(&self, k: &str) -> bool {
        self.get(k).is_some()
    }
    /// `int(item['Offset'], 16)`
    pub fn offset(&self) -> usize {
        hex(self.get("Offset").unwrap_or("0"))
    }
    pub fn int(&self, k: &str) -> usize {
        self.get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
    }
    pub fn name(&self) -> &str {
        self.get("Name").unwrap_or("")
    }
    /// `v.get('Offset', '').startswith('0x')`
    pub fn has_hex_offset(&self) -> bool {
        self.get("Offset").is_some_and(|o| o.starts_with("0x"))
    }
}

pub fn hex(s: &str) -> usize {
    let t = s.trim();
    usize::from_str_radix(t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")).unwrap_or(t), 16).unwrap_or(0)
}

/// `definitions()` of export-oot-link.py: the named children of the first `<File>` of a ZAPD xml, in document order
/// (a repeated name keeps its first position and takes the last value, like a Python dict).
#[derive(Debug, Default)]
pub struct Defs {
    order: Vec<String>,
    map: HashMap<String, Attrs>,
}

impl Defs {
    pub fn parse(text: &str) -> R<Defs> {
        let root = xml::parse(text)?;
        let mut d = Defs::default();
        if let Some(file) = root.children.iter().find(|c| c.tag == "File") {
            for el in &file.children {
                if let Some(n) = el.attr("Name").filter(|n| !n.is_empty()) {
                    if !d.map.contains_key(n) {
                        d.order.push(n.to_string());
                    }
                    d.map.insert(n.to_string(), Attrs(el.attrs.clone()));
                }
            }
        }
        Ok(d)
    }
    pub fn get(&self, name: &str) -> R<&Attrs> {
        self.map.get(name).ok_or_else(|| format!("KeyError: {name}"))
    }
    pub fn try_get(&self, name: &str) -> Option<&Attrs> {
        self.map.get(name)
    }
    pub fn offset(&self, name: &str) -> R<usize> {
        Ok(self.get(name)?.offset())
    }
    /// Values in dict order.
    pub fn values(&self) -> impl Iterator<Item = &Attrs> {
        self.order.iter().map(|n| &self.map[n])
    }
    /// `{v['Name']: v for v in values}`: same thing keyed by the `Name` attribute (here identical to the key).
    pub fn by_name(&self, name: &str) -> R<&Attrs> {
        self.get(name)
    }
}

pub type Seg = Rc<Vec<u8>>;

/// ROM + decomp checkout, with caches for decompressed segments and parsed xml.
pub struct Ctx {
    pub rom: Vec<u8>,
    pub decomp: PathBuf,
    names: Vec<String>,
    segs: RefCell<HashMap<String, Seg>>,
    defs: RefCell<HashMap<String, Rc<Defs>>>,
}

/// `decompress()` of export-oot-link.py (Yaz0). Like the Python it does not truncate a final over-long copy.
pub fn yaz0(data: &[u8]) -> R<Vec<u8>> {
    if data.get(..4) != Some(b"Yaz0") {
        return Err("Expected a Yaz0 segment".into());
    }
    let length = u32_(data, 4) as usize;
    let mut out: Vec<u8> = Vec::with_capacity(length + 32);
    let mut c = 16;
    let g = |c: usize| data.get(c).copied().ok_or_else(|| "truncated Yaz0".to_string());
    while out.len() < length {
        let flags = g(c)?;
        c += 1;
        for bit in (0..8).rev() {
            if flags & (1 << bit) != 0 {
                out.push(g(c)?);
                c += 1;
            } else {
                let (a, b) = (g(c)?, g(c + 1)?);
                c += 2;
                let distance = (((a & 15) as usize) << 8) + b as usize + 1;
                let mut count = (a >> 4) as usize + 2;
                if count == 2 {
                    count = g(c)? as usize + 18;
                    c += 1;
                }
                if distance > out.len() {
                    return Err("bad Yaz0 back reference".into());
                }
                for _ in 0..count {
                    out.push(out[out.len() - distance]);
                }
            }
            if out.len() >= length {
                break;
            }
        }
    }
    Ok(out)
}

impl Ctx {
    pub fn decomp_dir(source: &Path) -> PathBuf {
        for base in ["reference/oot", "oot"] {
            let p = source.join(base);
            if p.join("baseroms/ntsc-1.2/segments.csv").is_file() {
                return p;
            }
        }
        source.join("reference/oot")
    }

    pub fn new(source: &Path, rom: &[u8]) -> R<Ctx> {
        if crate::hash::hex(&crate::hash::md5(rom)) != MD5 {
            return Err("Expected a big-endian OoT US 1.2 ROM".into());
        }
        let decomp = Self::decomp_dir(source);
        let csv = crate::pyc::read(&decomp.join("baseroms/ntsc-1.2/segments.csv")).map_err(|e| format!("decomp sources missing: {e}"))?;
        let names = csv.lines().skip(1).filter(|l| !l.trim().is_empty()).map(|l| l.split(',').next().unwrap_or("").to_string()).collect();
        Ok(Ctx { rom: rom.to_vec(), decomp, names, segs: Default::default(), defs: Default::default() })
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// `segment(rom, names, name)`: a file of the ROM by its dmadata entry, Yaz0-decompressed when needed.
    pub fn segment(&self, name: &str) -> R<Seg> {
        if let Some(s) = self.segs.borrow().get(name) {
            return Ok(s.clone());
        }
        let idx = self.names.iter().position(|n| n == name).ok_or_else(|| format!("{name} is not in segments.csv"))?;
        let p = 0x7960 + 16 * idx;
        let (start, end, physical, cend) = (u32_(&self.rom, p) as usize, u32_(&self.rom, p + 4) as usize, u32_(&self.rom, p + 8) as usize, u32_(&self.rom, p + 12) as usize);
        let data = if cend != 0 { yaz0(&self.rom[physical..cend])? } else { self.rom[physical..physical + end - start].to_vec() };
        let s = Rc::new(data);
        self.segs.borrow_mut().insert(name.to_string(), s.clone());
        Ok(s)
    }

    /// Parsed `assets/xml/<rel>` of the decomp, e.g. `objects/object_link_boy.xml`.
    pub fn defs(&self, rel: &str) -> R<Rc<Defs>> {
        if let Some(d) = self.defs.borrow().get(rel) {
            return Ok(d.clone());
        }
        let text = crate::pyc::read(&self.decomp.join("assets/xml").join(rel))?;
        let d = Rc::new(Defs::parse(&text).map_err(|e| format!("{rel}: {e}"))?);
        self.defs.borrow_mut().insert(rel.to_string(), d.clone());
        Ok(d)
    }

    pub fn object_defs(&self, bank: &str) -> R<Rc<Defs>> {
        self.defs(&format!("objects/{bank}.xml"))
    }

    pub fn read_decomp(&self, rel: &str) -> R<String> {
        crate::pyc::read(&self.decomp.join(rel))
    }
}

/// Progress callback type shared by the stages.
pub type Progress<'a> = dyn FnMut(u32, &str) + 'a;

/// Which stages run (dev / test hook: `BYOR_OOT_STAGES=voices,sfx`); everything when unset.
fn stage_enabled(name: &str) -> bool {
    match std::env::var("BYOR_OOT_STAGES") {
        Ok(v) if !v.is_empty() => v.split(',').any(|s| s == name),
        _ => true,
    }
}

/// `do_oot` of pipeline.py: writes `<stage>/assets` and `<stage>/public`.
pub fn run(source: &Path, stage: &Path, z64: &[u8], progress: &mut Progress) -> R<()> {
    let ctx = Ctx::new(source, z64)?;
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_stages(&ctx, stage, progress)));
    match res {
        Ok(r) => r,
        Err(e) => Err(format!(
            "internal error: {}",
            e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
        )),
    }
}

fn run_stages(ctx: &Ctx, stage: &Path, progress: &mut Progress) -> R<()> {
    let public = stage.join("public");
    let assets = stage.join("assets");
    if stage_enabled("link") {
        progress(5, "Exporting Link");
        link::export_link(ctx, &public.join("models/oot"))?;
        progress(26, "Exporting the item models");
        link::export_items(ctx, &public.join("models/oot/items"))?;
        progress(26, "Exporting Epona");
        link::export_epona(ctx, &public.join("models/oot/epona.json"))?;
    }
    std::fs::create_dir_all(assets.join("oot-dialogue")).map_err(|e| e.to_string())?;
    if stage_enabled("courses") {
        progress(30, "Exporting Kokiri Forest, Deku Tree and Lon Lon Ranch");
        courses::export_all(ctx, stage, progress)?;
    }
    if stage_enabled("enemies") {
        progress(96, "Exporting the OoT enemies");
        enemies::export_all(ctx, stage)?;
    }
    if stage_enabled("voices") {
        progress(99, "Exporting Link voices");
        voices::export_oot(ctx, &assets.join("audio/voices"))?;
    }
    if stage_enabled("sfx") {
        progress(99, "Exporting item and combat sounds");
        sfx::export(ctx, &assets.join("audio/voices"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_matches_cpython() {
        assert_eq!(round_to(0.125, 2), 0.12);
        assert_eq!(round_to(0.375, 2), 0.38);
        assert_eq!(round_to(2.675, 2), 2.67);
        assert_eq!(round_to(-0.005, 2), -0.01);
        assert_eq!(round_to(1.00005, 4), 1.0001);
    }
}
