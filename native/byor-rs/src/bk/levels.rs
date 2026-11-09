//! Port of `scripts/export-bk-levels.py`: every Banjo-Kazooie world and sub-area as a textured glTF (`assets/levels/<id>/terrain.glb`)
//! and an SM64-style collision soup (`assets/courses/<id>.json`).
use super::banjo::{f32_at, i16_at, i32_at, u16_at, u32_at, u8_at};
use super::maps::{exported_maps, sky_key, Catalogue, WORLD_MAPS};
use super::Assets;
use crate::pyc::{dumps, round, R};
use crate::zpng::{png_rgba_loose, round_f, Num};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

pub const WORLD_SCALE: f64 = 1.25;
const WATER: u32 = 0x1E0000;
const BK_WATER_ID: i64 = 45;
const TWO_SIDED: u32 = 0x10000;

/// (key, top, mid, bottom, fog, fogStart, fogEnd)
type Sky = (&'static str, [f64; 3], [f64; 3], [f64; 3], [f64; 3], i64, i64);
const SKIES: [Sky; 15] = [
    ("bk_mad_monster_mansion", [0.08, 0.08, 0.15], [0.15, 0.15, 0.25], [0.2, 0.2, 0.3], [0.12, 0.12, 0.2], 4000, 20000),
    ("bk_click_clock_wood", [0.35, 0.6, 0.9], [0.6, 0.8, 0.95], [0.85, 0.93, 1.0], [0.7, 0.86, 1.0], 12000, 40000),
    ("bk_click_clock_wood_spring", [0.4, 0.65, 0.95], [0.65, 0.85, 0.98], [0.9, 0.95, 1.0], [0.75, 0.9, 1.0], 12000, 40000),
    ("bk_click_clock_wood_summer", [0.3, 0.58, 0.95], [0.55, 0.8, 0.98], [0.85, 0.94, 1.0], [0.7, 0.88, 1.0], 12000, 40000),
    ("bk_click_clock_wood_autumn", [0.7, 0.55, 0.45], [0.88, 0.72, 0.55], [0.95, 0.85, 0.7], [0.85, 0.7, 0.55], 10000, 36000),
    ("bk_click_clock_wood_winter", [0.65, 0.75, 0.9], [0.82, 0.88, 0.96], [0.94, 0.96, 1.0], [0.88, 0.92, 0.98], 8000, 30000),
    ("bk_spiral_mountain", [0.3, 0.55, 0.9], [0.55, 0.78, 0.97], [0.85, 0.93, 1.0], [0.75, 0.88, 1.0], 14000, 42000),
    ("bk_mumbos_mountain", [0.35, 0.55, 0.55], [0.5, 0.65, 0.5], [0.65, 0.75, 0.55], [0.45, 0.58, 0.42], 7000, 26000),
    ("bk_treasure_trove_cove", [0.25, 0.55, 0.95], [0.55, 0.8, 0.98], [0.85, 0.95, 1.0], [0.7, 0.88, 1.0], 18000, 52000),
    ("bk_clankers_cavern", [0.04, 0.07, 0.05], [0.05, 0.09, 0.06], [0.06, 0.1, 0.07], [0.04, 0.07, 0.05], 3500, 17000),
    ("bk_bubblegloop_swamp", [0.25, 0.3, 0.2], [0.4, 0.5, 0.3], [0.5, 0.55, 0.35], [0.34, 0.42, 0.27], 5000, 22000),
    ("bk_gobis_valley", [0.45, 0.6, 0.85], [0.85, 0.8, 0.65], [0.95, 0.85, 0.6], [0.9, 0.8, 0.6], 12000, 40000),
    ("bk_freezeezy_peak", [0.55, 0.65, 0.85], [0.8, 0.85, 0.95], [0.92, 0.95, 1.0], [0.85, 0.9, 0.97], 9000, 32000),
    ("bk_rusty_bucket_bay", [0.08, 0.1, 0.14], [0.18, 0.2, 0.22], [0.3, 0.3, 0.3], [0.15, 0.17, 0.2], 6000, 26000),
    ("lair", [0.1, 0.08, 0.16], [0.14, 0.12, 0.22], [0.18, 0.15, 0.26], [0.1, 0.09, 0.16], 3000, 16000),
];

fn sky_json(key: &str) -> R<Value> {
    let s = SKIES.iter().find(|s| s.0 == key).ok_or_else(|| format!("no sky {key}"))?;
    Ok(json!({"top": s.1, "mid": s.2, "bottom": s.3, "fog": s.4, "fogStart": s.5, "fogEnd": s.6}))
}

/// id -> (opa asset, xlu asset, spawn hint)
const WORLDS: [(&str, i64, i64, Option<[i64; 3]>); 14] = [
    ("bk_spiral_mountain", 0x14CF, 0x14D0, Some([6400, 520, 0])),
    ("bk_mumbos_mountain", 0x14AA, 0x14AB, None),
    ("bk_treasure_trove_cove", 0x146B, 0x146C, None),
    ("bk_clankers_cavern", 0x14ED, 0x14EE, None),
    ("bk_bubblegloop_swamp", 0x14D1, 0x14D2, None),
    ("bk_gobis_valley", 0x1474, 0x1475, None),
    ("bk_freezeezy_peak", 0x14C8, 0x14C9, None),
    ("bk_rusty_bucket_bay", 0x14B0, 0x14B1, None),
    ("bk_mad_monster_mansion", 0x147F, 0x1480, None),
    ("bk_click_clock_wood", 0x14D8, 0x14E3, None),
    ("bk_click_clock_wood_spring", 0x14D9, 0x14E4, None),
    ("bk_click_clock_wood_summer", 0x14DA, 0x14E5, None),
    ("bk_click_clock_wood_autumn", 0x14DB, 0x14E6, None),
    ("bk_click_clock_wood_winter", 0x14DC, 0x14E7, None),
];

struct World {
    id: String,
    opa: i64,
    xlu: i64,
    spawn: Option<[i64; 3]>,
    sky: &'static str,
    map: i64,
    glb: String,
}

fn worlds(cat: &Catalogue) -> R<Vec<World>> {
    let mut out: Vec<World> = WORLDS.iter().map(|(id, opa, xlu, spawn)| World { id: id.to_string(), opa: *opa, xlu: *xlu, spawn: *spawn, sky: SKIES.iter().find(|s| s.0 == *id).unwrap().0, map: WORLD_MAPS.iter().find(|w| w.0 == *id).unwrap().1, glb: id.to_string() }).collect();
    let mut glb_of: Vec<((i64, i64), String)> = Vec::new();
    for (mid, cid) in exported_maps(cat)? {
        if WORLDS.iter().any(|w| w.0 == cid) {
            continue;
        }
        let e = cat.entry(mid).ok_or("map entry")?;
        let owner = match glb_of.iter().find(|(k, _)| *k == (e.opa, e.xlu)) {
            Some((_, o)) => o.clone(),
            None => {
                glb_of.push(((e.opa, e.xlu), cid.clone()));
                cid.clone()
            }
        };
        out.push(World { id: cid, opa: e.opa, xlu: e.xlu, spawn: None, sky: sky_key(cat, mid)?, map: mid, glb: owner });
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ textures

fn rgba16(v: u32) -> [u8; 4] {
    let r = |x: u32| round((x * 255) as f64 / 31.0) as u8;
    [r(v >> 11 & 31), r(v >> 6 & 31), r(v >> 1 & 31), (255 * (v & 1)) as u8]
}

/// N64 image at `image` (offset in the texture block) -> RGBA bytes.
pub fn decode_texture(m: &[u8], base: usize, image: usize, tlut: usize, fmt: u32, siz: u32, width: usize, height: usize) -> R<Vec<u8>> {
    let n = width * height;
    let mut px = Vec::new();
    if fmt == 0 && siz == 2 {
        for i in 0..n {
            px.extend_from_slice(&rgba16(u16_at(m, base + image + 2 * i)? as u32));
        }
    } else if fmt == 0 && siz == 3 {
        let a = (base + image).min(m.len());
        px.extend_from_slice(&m[a..(base + image + 4 * n).min(m.len())]);
    } else if fmt == 2 && (siz == 0 || siz == 1) {
        let mut pal = Vec::new();
        for k in 0..(if siz == 0 { 16 } else { 256 }) {
            pal.push(rgba16(u16_at(m, base + tlut + 2 * k)? as u32));
        }
        for i in 0..n {
            let idx = if siz == 0 {
                let byte = u8_at(m, base + image + i / 2)?;
                if i % 2 == 0 { byte >> 4 } else { byte & 15 }
            } else {
                u8_at(m, base + image + i)?
            } as usize;
            px.extend_from_slice(pal.get(idx).ok_or("palette index out of range")?);
        }
    } else if fmt == 3 {
        for i in 0..n {
            let (c, a) = if siz == 0 {
                let byte = u8_at(m, base + image + i / 2)?;
                let v = if i % 2 == 0 { byte >> 4 } else { byte & 15 } as u32;
                (((v >> 1) * 255 / 7) as u8, (255 * (v & 1)) as u8)
            } else if siz == 1 {
                let v = u8_at(m, base + image + i)?;
                ((v >> 4) * 17, (v & 15) * 17)
            } else {
                (u8_at(m, base + image + 2 * i)?, u8_at(m, base + image + 2 * i + 1)?)
            };
            px.extend_from_slice(&[c, c, c, a]);
        }
    } else if fmt == 4 {
        for i in 0..n {
            let v = if siz == 0 {
                let byte = u8_at(m, base + image + i / 2)?;
                (if i % 2 == 0 { byte >> 4 } else { byte & 15 }) * 17
            } else {
                u8_at(m, base + image + i)?
            };
            px.extend_from_slice(&[v, v, v, 255]);
        }
    } else {
        return Err(format!("texture format {fmt}/{siz}"));
    }
    Ok(px)
}

// ------------------------------------------------------------------------------------------------ model parsing

type TexKey = (i64, i64, u32, u32, i64, i64);
type Tile = (u32, u32, (u32, u32), i64, i64);
/// vertex as the display list sees it: x y z s t r g b a, index
type Corner = [i64; 10];
type Tri = [(i64, i64, i64, f64, f64, i64, i64, i64, i64); 3];

pub struct Level<'a> {
    m: &'a [u8],
    geo: usize,
    gfx: usize,
    vtx_base: usize,
    cmd_base: usize,
    coll: usize,
    tex_data: usize,
    tex_infos: Vec<(i32, i16, u8, u8)>,
    /// `BKAnimTextureList` of the model header (offset 0x2C): per animated-texture slot (DL segment 0x0F - slot) the byte size of one
    /// frame, the frame count and the frames per second (`AnimTextureListCache_update`).
    anims: [(i64, i64, f64); 4],
}

/// One animated texture of a level: the frames after the first (which the DL names itself), the rate and the tile it is drawn with.
pub struct AnimTexture {
    pub key: TexKey,
    pub rate: f64,
    pub frames: Vec<(i64, i64, Rc<Vec<u8>>)>,
}

struct Geo<'l, 'a> {
    level: &'l Level<'a>,
    out: Vec<(Option<(TexKey, (u32, u32))>, Vec<Tri>)>,
    out_ids: HashMap<Option<(TexKey, (u32, u32))>, usize>,
    textures: HashMap<TexKey, (i64, i64, Rc<Vec<u8>>)>,
    // state
    tex_on: bool,
    tex_tile: i64,
    scale: (f64, f64),
    timg: u32,
    timg_segment: u32,
    tlut: u32,
    anim: HashMap<TexKey, (usize, u32, u32, Tile)>,
    texgen: bool,
    lighting: bool,
    tiles: HashMap<i64, Tile>,
    cache: Vec<Option<Corner>>,
    seen: HashSet<(usize, u32)>,
}

const DEFAULT_TILE: Tile = (0, 2, (0, 0), 32, 32);

impl<'l, 'a> Geo<'l, 'a> {
    fn texture(&mut self, image: u32, tile: Tile) -> R<(TexKey, (i64, i64), (u32, u32))> {
        let m = self.level.m;
        let (mut fmt, mut siz, wrap, tw, th) = tile;
        for &(off, kind, iw, ih) in &self.level.tex_infos {
            let pal: i64 = if kind & 1 != 0 { 32 } else if kind & 2 != 0 { 512 } else { 0 };
            if image as i64 == off as i64 + pal && iw != 0 && ih != 0 {
                (fmt, siz) = if kind & 1 != 0 { (2, 0) } else if kind & 2 != 0 { (2, 1) } else if kind & 4 != 0 { (0, 2) } else { (0, 3) };
                let key: TexKey = (off as i64, pal, fmt, siz, iw as i64, ih as i64);
                if !self.textures.contains_key(&key) {
                    let px = decode_texture(m, self.level.tex_data, image as usize, off as usize, fmt, siz, iw as usize, ih as usize)?;
                    self.textures.insert(key, (iw as i64, ih as i64, Rc::new(px)));
                }
                return Ok((key, (iw as i64, ih as i64), wrap));
            }
        }
        let key: TexKey = (image as i64, self.tlut as i64, fmt, siz, tw, th);
        if !self.textures.contains_key(&key) {
            let px = decode_texture(m, self.level.tex_data, image as usize, self.tlut as usize, fmt, siz, tw as usize, th as usize)?;
            self.textures.insert(key, (tw, th, Rc::new(px)));
        }
        Ok((key, (tw, th), wrap))
    }

    fn run_dl(&mut self, index: i64) -> R<()> {
        let m = self.level.m;
        let mut p = (self.level.cmd_base as i64 + 8 * index) as usize;
        loop {
            let (a, b) = (u32_at(m, p)?, u32_at(m, p + 4)?);
            p += 8;
            let op = a >> 24;
            match op {
                0xB8 => return Ok(()),
                0x04 => {
                    let n = (a >> 10) & 0x3F;
                    let first = ((a >> 16) & 0xFF) / 2;
                    for i in 0..n {
                        let vi = (b & 0xFFFFFF) as usize / 16 + i as usize;
                        let c = self.level.vertex(vi)?;
                        *self.cache.get_mut((first + i) as usize).ok_or("vertex cache overflow")? = Some([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7], c[8], vi as i64]);
                    }
                }
                0xBF | 0xB1 => {
                    let words: Vec<u32> = if op == 0xBF { vec![b] } else { vec![a, b] };
                    let textured = self.tex_on && !self.texgen;
                    let (w, h, mat) = if textured {
                        let tile = self.tiles.get(&self.tex_tile).copied().or_else(|| self.tiles.get(&7).copied()).unwrap_or(DEFAULT_TILE);
                        let (key, (w, h), wrap) = self.texture(self.timg, tile)?;
                        // segments 0x0C..0x0F hold animated textures (`modelRender_draw`: segment 15 - slot)
                        if (0x0C..=0x0F).contains(&self.timg_segment) {
                            let slot = (0x0F - self.timg_segment) as usize;
                            if self.level.anims[slot].1 > 1 && self.level.anims[slot].0 > 0 {
                                self.anim.entry(key).or_insert((slot, self.timg, self.tlut, tile));
                            }
                        }
                        (w, h, Some((key, wrap)))
                    } else {
                        (1, 1, None)
                    };
                    let id = match self.out_ids.get(&mat) {
                        Some(&i) => i,
                        None => {
                            self.out_ids.insert(mat, self.out.len());
                            self.out.push((mat, vec![]));
                            self.out.len() - 1
                        }
                    };
                    for word in words {
                        let mut tri: Vec<(i64, i64, i64, f64, f64, i64, i64, i64, i64)> = Vec::new();
                        let corners: Vec<Corner> = [16, 8, 0].iter().map(|shift| self.cache.get(((word >> shift & 0xFF) / 2) as usize).copied().flatten().ok_or("triangle references an empty vertex slot".to_string())).collect::<R<_>>()?;
                        for c in corners {
                            let (mut u, mut v) = (0.0f64, 0.0f64);
                            if textured {
                                u = (c[3] as f64 * self.scale.0 / 32.0 + 0.5) / w as f64;
                                v = (c[4] as f64 * self.scale.1 / 32.0 + 0.5) / h as f64;
                            }
                            let (mut r, mut g, mut bl) = (c[5], c[6], c[7]);
                            if self.lighting {
                                r = 255;
                                g = 255;
                                bl = 255;
                            }
                            tri.push((c[0], c[1], c[2], u, v, r, g, bl, c[8]));
                        }
                        self.out[id].1.push([tri[0], tri[1], tri[2]]);
                    }
                }
                0xBB => {
                    self.tex_on = a & 0xFF != 0;
                    self.tex_tile = ((a >> 8) & 7) as i64;
                    self.scale = ((b >> 16) as f64 / 65536.0, (b & 0xFFFF) as f64 / 65536.0);
                }
                0xB7 => {
                    if b & 0x40000 != 0 {
                        self.texgen = true;
                    }
                    if b & 0x20000 != 0 {
                        self.lighting = true;
                    }
                }
                0xB6 => {
                    if b & 0x40000 != 0 {
                        self.texgen = false;
                    }
                    if b & 0x20000 != 0 {
                        self.lighting = false;
                    }
                }
                0xFD => {
                    self.timg = b & 0xFFFFFF;
                    self.timg_segment = b >> 24;
                }
                0xF0 => self.tlut = self.timg,
                0xF5 => {
                    let t = ((b >> 24) & 7) as i64;
                    let old = self.tiles.get(&t).copied().unwrap_or(DEFAULT_TILE);
                    self.tiles.insert(t, ((a >> 21) & 7, (a >> 19) & 3, ((b >> 8) & 3, (b >> 18) & 3), old.3, old.4));
                }
                0xF2 => {
                    let t = ((b >> 24) & 7) as i64;
                    let old = self.tiles.get(&t).copied().unwrap_or(DEFAULT_TILE);
                    self.tiles.insert(t, (old.0, old.1, old.2, ((b >> 12) & 0xFFF) as i64 / 4 + 1, (b & 0xFFF) as i64 / 4 + 1));
                }
                _ => {}
            }
        }
    }

    fn geo(&mut self, mut p: usize) -> R<()> {
        let m = self.level.m;
        let at = |p: usize, d: i64| (p as i64 + d) as usize;
        loop {
            let (cmd, nxt) = (u32_at(m, p)?, i32_at(m, p + 4)?);
            if !self.seen.insert((p, cmd)) {
                return Ok(());
            }
            match cmd {
                0x00 => {
                    let br = i16_at(m, p + 8)?;
                    if br != 0 {
                        self.geo(at(p, br as i64))?;
                    }
                }
                0x01 => {
                    let (b1, b2) = (i16_at(m, p + 34)?, i32_at(m, p + 36)?);
                    if b1 != 0 {
                        self.geo(at(p, b1 as i64))?;
                    }
                    if b2 != 0 {
                        self.geo(at(p, b2 as i64))?;
                    }
                }
                0x02 => {
                    let br = u8_at(m, p + 8)?;
                    if br != 0 {
                        self.geo(p + br as usize)?;
                    }
                }
                0x03 => self.run_dl(i16_at(m, p + 8)? as i64)?,
                0x07 => self.run_dl(i16_at(m, p + 10)? as i64)?,
                0x05 => {
                    let mut q = p + 8;
                    let mut first = true;
                    loop {
                        let idx = i16_at(m, q)?;
                        q += 2;
                        if idx == 0 && !first {
                            break;
                        }
                        first = false;
                        self.run_dl(idx as i64)?;
                    }
                }
                0x06 => self.geo(at(p, i32_at(m, p + 8)? as i64))?,
                0x08 => {
                    let lod_min = f32_at(m, p + 12)?;
                    let br = i32_at(m, p + 28)?;
                    if lod_min == 0.0 && br != 0 {
                        self.geo(at(p, br as i64))?;
                    }
                }
                0x0C => {} // selectors = {} : choice is always 0
                0x0D => {
                    let br = i16_at(m, p + 20)?;
                    if br != 0 {
                        self.geo(at(p, br as i64))?;
                    }
                }
                0x0E => {
                    let br = i16_at(m, p + 16)?;
                    if br != 0 {
                        self.geo(at(p, br as i64))?;
                    }
                }
                0x0F => {
                    let br = i16_at(m, p + 8)?;
                    if br != 0 {
                        self.geo(at(p, br as i64))?;
                    }
                }
                0x0A | 0x04 | 0x09 | 0x0B | 0x10 => {}
                _ => return Err(format!("unsupported geo command {cmd:#x} at {p:#x}")),
            }
            if nxt == 0 {
                return Ok(());
            }
            p = at(p, nxt as i64);
        }
    }
}

pub struct Collision {
    /// distinct (v0, v1, v2, flags)
    pub tris: Vec<(i16, i16, i16, u32)>,
}

impl<'a> Level<'a> {
    pub fn new(m: &'a [u8]) -> R<Level<'a>> {
        let (geo, tex_list, gfx, vtx, coll) = (i32_at(m, 4)? as usize, u16_at(m, 8)? as usize, i32_at(m, 12)? as usize, i32_at(m, 16)? as usize, i32_at(m, 28)? as usize);
        let count = i16_at(m, tex_list + 4)? as usize;
        let mut tex_infos = Vec::new();
        for k in 0..count {
            let o = tex_list + 8 + 16 * k;
            tex_infos.push((i32_at(m, o)?, i16_at(m, o + 4)?, u8_at(m, o + 8)?, u8_at(m, o + 9)?));
        }
        tex_infos.sort();
        let anim_list = i32_at(m, 0x2C).unwrap_or(0) as usize;
        let mut anims = [(0i64, 0i64, 0.0f64); 4];
        if anim_list != 0 && anim_list + 32 <= m.len() {
            for (k, a) in anims.iter_mut().enumerate() {
                let o = anim_list + 8 * k;
                *a = (i16_at(m, o)? as i64, i16_at(m, o + 2)? as i64, f32_at(m, o + 4)? as f64);
            }
        }
        Ok(Level { m, geo, gfx, vtx_base: vtx + 0x18, cmd_base: gfx + 8, coll, tex_data: tex_list + 8 + 16 * count, tex_infos, anims })
    }

    /// x y z s t r g b a
    pub fn vertex(&self, i: usize) -> R<[i64; 9]> {
        let m = self.m;
        let o = self.vtx_base + 16 * i;
        Ok([i16_at(m, o)? as i64, i16_at(m, o + 2)? as i64, i16_at(m, o + 4)? as i64, i16_at(m, o + 8)? as i64, i16_at(m, o + 10)? as i64, u8_at(m, o + 12)? as i64, u8_at(m, o + 13)? as i64, u8_at(m, o + 14)? as i64, u8_at(m, o + 15)? as i64])
    }

    #[allow(clippy::type_complexity)]
    fn geometry(&self) -> R<(Vec<(Option<(TexKey, (u32, u32))>, Vec<Tri>)>, HashMap<TexKey, (i64, i64, Rc<Vec<u8>>)>, Vec<AnimTexture>)> {
        let _ = self.gfx;
        let mut g = Geo {
            level: self,
            out: vec![],
            out_ids: HashMap::new(),
            textures: HashMap::new(),
            tex_on: false,
            tex_tile: 0,
            scale: (1.0, 1.0),
            timg: 0,
            timg_segment: 0,
            tlut: 0,
            anim: HashMap::new(),
            texgen: false,
            lighting: false,
            tiles: HashMap::new(),
            cache: vec![None; 64],
            seen: HashSet::new(),
        };
        g.geo(self.geo)?;
        // the frames after the first: the base image (and palette) stepped by `frame_size` bytes per frame
        let mut anims = Vec::new();
        let mut keys: Vec<_> = g.anim.iter().map(|(k, v)| (*k, *v)).collect();
        keys.sort_by_key(|(k, _)| *k);
        for (key, (slot, image, tlut, tile)) in keys {
            let (size, count, rate) = self.anims[slot];
            let siz_fmt = (key.2, key.3);
            let (tw, th) = (key.4 as usize, key.5 as usize);
            let mut frames = Vec::new();
            for f in 1..count {
                let (img, pal) = (image as i64 + f * size, tlut as i64 + f * size);
                if img < 0 || pal < 0 {
                    break;
                }
                match decode_texture(self.m, self.tex_data, img as usize, pal as usize, siz_fmt.0, siz_fmt.1, tw, th) {
                    Ok(px) => frames.push((tw as i64, th as i64, Rc::new(px))),
                    Err(_) => break,
                }
            }
            let _ = tile;
            if frames.len() as i64 == count - 1 {
                anims.push(AnimTexture { key, rate, frames });
            }
        }
        Ok((g.out, g.textures, anims))
    }

    /// Distinct (v0, v1, v2, flags) collision triangles with vertices in model units.
    pub fn collision(&self) -> R<Collision> {
        let m = self.m;
        if self.coll == 0 {
            return Ok(Collision { tris: vec![] });
        }
        let c = self.coll;
        let geo_count = i16_at(m, c + 16)?.max(0) as usize;
        let cells = c + 0x18;
        let tris = cells + 4 * geo_count;
        let mut seen: HashSet<(i16, i16, i16)> = HashSet::new();
        let mut out = Vec::new();
        for g in 0..geo_count {
            let (start, n) = (i16_at(m, cells + 4 * g)?, i16_at(m, cells + 4 * g + 2)?);
            for t in start as i64..start as i64 + n as i64 {
                let o = tris + 12 * t as usize;
                let (a, b, cc, flags) = (i16_at(m, o)?, i16_at(m, o + 2)?, i16_at(m, o + 4)?, u32_at(m, o + 8)?);
                if seen.insert((a, b, cc)) {
                    out.push((a, b, cc, flags));
                }
            }
        }
        Ok(Collision { tris: out })
    }
}

// ------------------------------------------------------------------------------------------------ glTF

struct Glb {
    data: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
    images: Vec<Value>,
    textures: Vec<Value>,
    samplers: Vec<Value>,
    materials: Vec<Value>,
    primitives: Vec<Value>,
    /// `anim.json` rows (material name, rate, wrap, frame files) and the frame PNGs written next to the glb.
    anims: Vec<Value>,
    sidecars: Vec<(String, Vec<u8>)>,
}

impl Glb {
    fn new() -> Self {
        Glb { data: vec![], buffer_views: vec![], accessors: vec![], images: vec![], textures: vec![], samplers: vec![], materials: vec![], primitives: vec![], anims: vec![], sidecars: vec![] }
    }

    fn view(&mut self, data: &[u8], target: Option<u32>) -> usize {
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        let mut v = Map::new();
        v.insert("buffer".into(), json!(0));
        v.insert("byteOffset".into(), json!(self.data.len()));
        v.insert("byteLength".into(), json!(data.len()));
        if let Some(t) = target {
            v.insert("target".into(), json!(t));
        }
        self.buffer_views.push(Value::Object(v));
        self.data.extend_from_slice(data);
        self.buffer_views.len() - 1
    }

    fn accessor(&mut self, flat: Vec<u8>, kind: &str, component: u32, count: usize, bounds: Option<(Vec<Num>, Vec<Num>)>, target: u32) -> usize {
        let mut acc = Map::new();
        acc.insert("bufferView".into(), json!(self.view(&flat, Some(target))));
        acc.insert("componentType".into(), json!(component));
        acc.insert("count".into(), json!(count));
        acc.insert("type".into(), json!(kind));
        if let Some((lo, hi)) = bounds {
            acc.insert("min".into(), Value::Array(lo.iter().map(|n| n.json()).collect()));
            acc.insert("max".into(), Value::Array(hi.iter().map(|n| n.json()).collect()));
        }
        if component == 5121 {
            acc.insert("normalized".into(), json!(true));
        }
        self.accessors.push(Value::Object(acc));
        self.accessors.len() - 1
    }

    fn material(&mut self, png: Option<Vec<u8>>, wrap: (u32, u32), mode: &str, name: Option<&str>) -> usize {
        let mut mat = Map::new();
        if let Some(n) = name {
            mat.insert("name".into(), json!(n));
        }
        mat.insert("doubleSided".into(), json!(true));
        mat.insert("extensions".into(), json!({"KHR_materials_unlit": {}}));
        mat.insert("alphaMode".into(), json!(mode));
        if mode == "MASK" {
            mat.insert("alphaCutoff".into(), json!(0.5));
        }
        match png {
            Some(bytes) => {
                let wraps = [10497, 33648, 33071, 33071];
                let i = self.images.len();
                let bv = self.view(&bytes, None);
                self.images.push(json!({"bufferView": bv, "mimeType": "image/png"}));
                self.samplers.push(json!({"magFilter": 9729, "minFilter": 9987, "wrapS": wraps[wrap.0 as usize & 3], "wrapT": wraps[wrap.1 as usize & 3]}));
                self.textures.push(json!({"source": i, "sampler": i}));
                mat.insert("pbrMetallicRoughness".into(), json!({"baseColorTexture": {"index": i}, "metallicFactor": 0, "roughnessFactor": 1}));
            }
            None => {
                mat.insert("pbrMetallicRoughness".into(), json!({"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0, "roughnessFactor": 1}));
            }
        }
        self.materials.push(Value::Object(mat));
        self.materials.len() - 1
    }

    #[allow(clippy::type_complexity)]
    fn primitive(&mut self, verts: &[([f64; 3], [f64; 2], [u8; 4])], indices: &[u32], material: usize) {
        let mut flat = Vec::new();
        for v in verts {
            for c in v.0 {
                flat.extend_from_slice(&(c as f32).to_le_bytes());
            }
        }
        let bounds = if verts.is_empty() {
            (vec![], vec![])
        } else {
            let mut lo = vec![];
            let mut hi = vec![];
            for k in 0..3 {
                let (mut l, mut h) = (verts[0].0[k], verts[0].0[k]);
                for v in &verts[1..] {
                    if v.0[k] < l {
                        l = v.0[k];
                    }
                    if v.0[k] > h {
                        h = v.0[k];
                    }
                }
                lo.push(Num::F(l));
                hi.push(Num::F(h));
            }
            (lo, hi)
        };
        let pos = self.accessor(flat, "VEC3", 5126, verts.len(), Some(bounds), 34962);
        let mut flat = Vec::new();
        for v in verts {
            for c in v.1 {
                flat.extend_from_slice(&(c as f32).to_le_bytes());
            }
        }
        let uv = self.accessor(flat, "VEC2", 5126, verts.len(), None, 34962);
        let flat: Vec<u8> = verts.iter().flat_map(|v| v.2).collect();
        let col = self.accessor(flat, "VEC4", 5121, verts.len(), None, 34962);
        let flat: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let ib = match (indices.iter().min(), indices.iter().max()) {
            (Some(l), Some(h)) => (vec![Num::I(*l as i64)], vec![Num::I(*h as i64)]),
            _ => (vec![], vec![]),
        };
        let idx = self.accessor(flat, "SCALAR", 5125, indices.len(), Some(ib), 34963);
        self.primitives.push(json!({"attributes": {"POSITION": pos, "TEXCOORD_0": uv, "COLOR_0": col}, "indices": idx, "material": material}));
    }

    fn write(mut self, path: &Path) -> R<()> {
        let doc = json!({
            "asset": {"version": "2.0", "extras": {"sm64UnitsToMeters": 1}},
            "buffers": [{"byteLength": self.data.len()}],
            "bufferViews": self.buffer_views,
            "accessors": self.accessors,
            "images": self.images,
            "textures": self.textures,
            "samplers": self.samplers,
            "materials": self.materials,
            "meshes": [{"primitives": self.primitives}],
            "nodes": [{"mesh": 0}],
            "scenes": [{"nodes": [0]}],
            "scene": 0,
            "extensionsUsed": ["KHR_materials_unlit"],
        });
        let mut meta = dumps(&doc, None).into_bytes();
        while meta.len() % 4 != 0 {
            meta.push(b' ');
        }
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&((28 + meta.len() + self.data.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&meta);
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&self.data);
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        if !self.anims.is_empty() {
            std::fs::create_dir_all(dir.join("anim")).map_err(|e| e.to_string())?;
            for (file, bytes) in &self.sidecars {
                std::fs::write(dir.join(file), bytes).map_err(|e| e.to_string())?;
            }
            std::fs::write(dir.join("anim.json"), dumps(&json!({"textures": self.anims}), Some(1)) + "\n").map_err(|e| e.to_string())?;
        }
        std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn linear_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    for (c, e) in t.iter_mut().enumerate() {
        let x = c as f64 / 255.0;
        *e = round(255.0 * if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(std::hint::black_box(2.4)) }) as u8;
    }
    t
}

/// `str(key)` of the (texture key, wrap) tuple, which the Python code sorts the materials by.
fn key_str(k: &Option<(TexKey, (u32, u32))>) -> String {
    match k {
        None => "(None, (0, 0))".to_string(),
        Some((t, w)) => format!("(({}, {}, {}, {}, {}, {}), ({}, {}))", t.0, t.1, t.2, t.3, t.4, t.5, w.0, w.1),
    }
}

fn add_geometry(glb: &mut Glb, level: &Level, translucent: bool, scale: f64, linear: &[u8; 256]) -> R<()> {
    let (mut batches, textures, animated) = level.geometry()?;
    batches.sort_by_key(|b| key_str(&b.0));
    for (key, tris) in batches {
        let (mut has_alpha, mut png) = (false, None);
        if let Some((tk, _)) = &key {
            let (w, h, rgba) = &textures[tk];
            has_alpha = rgba.iter().skip(3).step_by(4).any(|&a| a < 255);
            png = Some(png_rgba_loose(*w as usize, *h as usize, rgba, 9));
        }
        let mode = if translucent { "BLEND" } else if has_alpha { "MASK" } else { "OPAQUE" };
        let wrap = key.map(|k| k.1).unwrap_or((0, 0));
        // an animated texture names its material `bkanim<n>`; `anim.json` lists the frames (the first is the glb's own image)
        let anim = key.as_ref().and_then(|(tk, _)| animated.iter().find(|a| a.key == *tk));
        let name = anim.map(|a| {
            let id = glb.anims.len();
            let mut files = Vec::new();
            for (f, (w, h, rgba)) in a.frames.iter().enumerate() {
                let file = format!("anim/a{id}_{}.png", f + 1);
                glb.sidecars.push((file.clone(), png_rgba_loose(*w as usize, *h as usize, rgba, 9)));
                files.push(file);
            }
            glb.anims.push(json!({"material": format!("bkanim{id}"), "rate": round_f(a.rate, 4), "wrap": [wrap.0, wrap.1], "frames": files}));
            format!("bkanim{id}")
        });
        let material = glb.material(png, wrap, mode, name.as_deref());
        let mut index: HashMap<(i64, i64, i64, u64, u64, [u8; 4]), u32> = HashMap::new();
        let mut verts: Vec<([f64; 3], [f64; 2], [u8; 4])> = Vec::new();
        let mut indices = Vec::new();
        for tri in &tris {
            for &(x, y, z, u, v, r, g, b, a) in tri {
                let color = [linear[r as usize], linear[g as usize], linear[b as usize], if translucent { a as u8 } else { 255 }];
                let (ru, rv) = (round_f(u, 4), round_f(v, 4));
                // dict keys: -0.0 == 0.0
                let k = (x, y, z, (ru + 0.0).to_bits(), (rv + 0.0).to_bits(), color);
                let id = *index.entry(k).or_insert_with(|| {
                    verts.push(([x as f64 * scale, y as f64 * scale, z as f64 * scale], [ru, rv], color));
                    verts.len() as u32 - 1
                });
                indices.push(id);
            }
        }
        glb.primitive(&verts, &indices, material);
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ collision -> SM64 triangle soup

type V3 = [f64; 3];

struct Built {
    verts: Vec<V3>,
    tris: Vec<[i64; 5]>,
    water: Vec<[V3; 3]>,
}

fn build_collision(levels: &[Level], scale: f64) -> R<Built> {
    let mut b = Built { verts: vec![], tris: vec![], water: vec![] };
    let mut vindex: HashMap<[u64; 3], usize> = HashMap::new();
    for level in levels {
        for (a, bb, c, flags) in level.collision()?.tris {
            let mut pts = [[0i64; 3]; 3];
            for (k, i) in [a, bb, c].into_iter().enumerate() {
                let v = level.vertex(i as usize)?;
                pts[k] = [v[0], v[1], v[2]];
            }
            if flags & WATER != 0 {
                b.water.push(pts.map(|p| p.map(|q| q as f64 * scale)));
                continue;
            }
            let ids: Vec<usize> = pts
                .iter()
                .map(|p| {
                    let key = p.map(|c| round_f(c as f64 * scale, 2));
                    *vindex.entry(key.map(|f| (f + 0.0).to_bits())).or_insert_with(|| {
                        b.verts.push(key);
                        b.verts.len() - 1
                    })
                })
                .collect();
            if ids[0] == ids[1] || ids[1] == ids[2] || ids[0] == ids[2] {
                continue;
            }
            b.tris.push([ids[0] as i64, ids[1] as i64, ids[2] as i64, 0, 0]);
            if flags & TWO_SIDED != 0 {
                b.tris.push([ids[0] as i64, ids[2] as i64, ids[1] as i64, 0, 0]);
            }
        }
    }
    Ok(b)
}

/// [x0, z0, x1, z1, y] in jumpman units; the JSON row is `[BK_WATER_ID, x0, z0, x1, z1, y]`.
struct WaterBox {
    x0: f64,
    z0: f64,
    x1: f64,
    z1: f64,
    y: i64,
}

/// Rasterise flat water-surface triangles into boxes.
fn water_boxes(water: &[[V3; 3]]) -> Vec<WaterBox> {
    let cell = 128.0f64;
    let mut groups: HashMap<i64, HashSet<(i64, i64)>> = HashMap::new();
    for tri in water {
        let y = round((tri[0][1] + tri[1][1] + tri[2][1]) / 3.0);
        let xs = [tri[0][0], tri[1][0], tri[2][0]];
        let zs = [tri[0][2], tri[1][2], tri[2][2]];
        let fmin = |v: &[f64; 3]| v.iter().copied().fold(f64::INFINITY, f64::min);
        let fmax = |v: &[f64; 3]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        for ix in (fmin(&xs) / cell).floor() as i64..=(fmax(&xs) / cell).floor() as i64 {
            for iz in (fmin(&zs) / cell).floor() as i64..=(fmax(&zs) / cell).floor() as i64 {
                let (cx, cz) = ((ix as f64 + 0.5) * cell, (iz as f64 + 0.5) * cell);
                let ((ax, az), (bx, bz), (qx, qz)) = ((tri[0][0], tri[0][2]), (tri[1][0], tri[1][2]), (tri[2][0], tri[2][2]));
                let d = [(bx - ax) * (cz - az) - (bz - az) * (cx - ax), (qx - bx) * (cz - bz) - (qz - bz) * (cx - bx), (ax - qx) * (cz - qz) - (az - qz) * (cx - qx)];
                if d.iter().all(|&v| v >= 0.0) || d.iter().all(|&v| v <= 0.0) {
                    groups.entry(y).or_default().insert((ix, iz));
                }
            }
        }
    }
    let mut ys: Vec<i64> = groups.keys().copied().collect();
    ys.sort();
    let mut boxes = Vec::new();
    for y in ys {
        let mut cells: Vec<(i64, i64)> = groups[&y].iter().copied().collect();
        cells.sort();
        // rows: iz -> ixs (in the sorted order of the cells), dict insertion order irrelevant (iterated sorted)
        let mut rows: std::collections::BTreeMap<i64, Vec<i64>> = std::collections::BTreeMap::new();
        for (ix, iz) in cells {
            rows.entry(iz).or_default().push(ix);
        }
        let mut runs: Vec<((i64, i64), Vec<i64>)> = Vec::new();
        for (iz, xs) in &rows {
            let (mut start, mut prev) = (xs[0], xs[0]);
            let mut it: Vec<Option<i64>> = xs[1..].iter().map(|v| Some(*v)).collect();
            it.push(None);
            for ix in it {
                if ix.is_none() || ix != Some(prev + 1) {
                    match runs.iter_mut().find(|r| r.0 == (start, prev)) {
                        Some(r) => r.1.push(*iz),
                        None => runs.push(((start, prev), vec![*iz])),
                    }
                    if let Some(i) = ix {
                        start = i;
                    }
                }
                if let Some(i) = ix {
                    prev = i;
                }
            }
        }
        for ((x0, x1), mut zs) in runs {
            zs.sort();
            let (mut z0, mut prev_z) = (zs[0], zs[0]);
            let mut it: Vec<Option<i64>> = zs[1..].iter().map(|v| Some(*v)).collect();
            it.push(None);
            for z in it {
                if z.is_none() || z != Some(prev_z + 1) {
                    boxes.push(WaterBox { x0: x0 as f64 * cell, z0: z0 as f64 * cell, x1: (x1 + 1) as f64 * cell, z1: (prev_z + 1) as f64 * cell, y });
                    if let Some(zz) = z {
                        z0 = zz;
                    }
                }
                if let Some(zz) = z {
                    prev_z = zz;
                }
            }
        }
    }
    boxes
}

/// A broad, flat, dry clearing near the world's lowest dry ground.
fn pick_spawn(verts: &[V3], tris: &[[i64; 5]], boxes: &[WaterBox]) -> [i64; 3] {
    let (cell, want) = (100.0f64, 7i64);
    let mut order: Vec<(i64, i64)> = Vec::new();
    let mut floors: HashMap<(i64, i64), Vec<f64>> = HashMap::new();
    for t in tris {
        let (p, q, r) = (verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]);
        let (ux, uy, uz) = (q[0] - p[0], q[1] - p[1], q[2] - p[2]);
        let (vx, vy, vz) = (r[0] - p[0], r[1] - p[1], r[2] - p[2]);
        let (nx, ny, nz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
        let m = (nx * nx + ny * ny + nz * nz).sqrt();
        if m < 1e-6 || ny / m < 0.97 {
            continue;
        }
        let xs = [p[0], q[0], r[0]];
        let zs = [p[2], q[2], r[2]];
        let den = (q[2] - r[2]) * (p[0] - r[0]) + (r[0] - q[0]) * (p[2] - r[2]);
        if den.abs() < 1e-9 {
            continue;
        }
        let fmin = |v: &[f64; 3]| v.iter().copied().fold(f64::INFINITY, f64::min);
        let fmax = |v: &[f64; 3]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        for ix in (fmin(&xs) / cell).floor() as i64..=(fmax(&xs) / cell).floor() as i64 {
            for iz in (fmin(&zs) / cell).floor() as i64..=(fmax(&zs) / cell).floor() as i64 {
                let (x, z) = ((ix as f64 + 0.5) * cell, (iz as f64 + 0.5) * cell);
                let l1 = ((q[2] - r[2]) * (x - r[0]) + (r[0] - q[0]) * (z - r[2])) / den;
                let l2 = ((r[2] - p[2]) * (x - r[0]) + (p[0] - r[0]) * (z - r[2])) / den;
                if l1.min(l2).min(1.0 - l1 - l2) >= 0.0 {
                    let list = floors.entry((ix, iz)).or_insert_with(|| {
                        order.push((ix, iz));
                        vec![]
                    });
                    list.push(l1 * p[1] + l2 * q[1] + (1.0 - l1 - l2) * r[1]);
                }
            }
        }
    }
    let lowest_water = boxes.iter().map(|b| b.y as f64).fold(f64::INFINITY, f64::min);
    let lowest_water = if boxes.is_empty() { -1e9 } else { lowest_water } - 100.0;
    let wet = |x: f64, z: f64, y: f64| y < lowest_water || boxes.iter().any(|b| b.x0 <= x && x <= b.x1 && b.z0 <= z && z <= b.z1 && y < b.y as f64 + 300.0);
    let mut best: Option<(f64, f64, f64)> = None;
    for key in &order {
        let (ix, iz) = *key;
        for &y in &floors[key] {
            if wet((ix as f64 + 0.5) * cell, (iz as f64 + 0.5) * cell, y) {
                continue;
            }
            let mut ok = true;
            'n: for dx in -want..=want {
                for dz in -want..=want {
                    if dx * dx + dz * dz <= want * want && !floors.get(&(ix + dx, iz + dz)).is_some_and(|l| l.iter().any(|y2| (y2 - y).abs() < 30.0)) {
                        ok = false;
                        break 'n;
                    }
                }
            }
            if ok && best.is_none_or(|b| y < b.1) {
                best = Some(((ix as f64 + 0.5) * cell, y, (iz as f64 + 0.5) * cell));
            }
        }
    }
    match best {
        Some(b) => [round(b.0), round(b.1) + 2, round(b.2)],
        None => [0, 0, 0],
    }
}

fn export_world(assets: &Assets, w: &World, out_levels: &Path, out_courses: &Path, linear: &[u8; 256]) -> R<()> {
    let opa = assets.read(w.opa as usize)?;
    let xlu = if w.xlu != 0 { Some(assets.read(w.xlu as usize)?) } else { None };
    let mut models = vec![Level::new(&opa)?];
    if let Some(x) = &xlu {
        models.push(Level::new(x)?);
    }
    let mut glb = Glb::new();
    for (i, level) in models.iter().enumerate() {
        add_geometry(&mut glb, level, i == 1, WORLD_SCALE, linear)?;
    }
    if w.glb == w.id {
        glb.write(&out_levels.join(&w.id).join("terrain.glb"))?;
    }
    let built = build_collision(&models, WORLD_SCALE)?;
    let xs = built.verts.iter().map(|v| v[0].abs()).fold(f64::NEG_INFINITY, f64::max);
    let zs = built.verts.iter().map(|v| v[2].abs()).fold(f64::NEG_INFINITY, f64::max);
    let boundary = (xs.max(zs) + 2000.0).ceil() as i64;
    let boxes = water_boxes(&built.water);
    let spawn = match w.spawn {
        Some(s) => s,
        None => pick_spawn(&built.verts, &built.tris, &boxes),
    };
    let data = json!({
        "model": format!("levels/{}/terrain.glb", w.glb),
        "vertices": built.verts,
        "triangles": built.tris,
        "spawn": {"pos": spawn, "faceYaw": 0},
        "waterBoxes": boxes.iter().map(|b| json!([BK_WATER_ID, b.x0, b.z0, b.x1, b.z1, b.y])).collect::<Vec<_>>(),
        "horizontalBoundary": boundary, "objects": [], "coins": [], "sky": sky_json(w.sky)?, "explorationOnly": true,
        "bkMap": w.map,
    });
    std::fs::write(out_courses.join(format!("{}.json", w.id)), dumps(&data, None)).map_err(|e| e.to_string())
}

/// `export-bk-levels.py` for every world and sub-area.
pub fn export_all(assets: &Assets, cat: &Catalogue, out: &Path, progress: &mut dyn FnMut(u32, &str)) -> R<()> {
    std::fs::create_dir_all(out.join("courses")).map_err(|e| e.to_string())?;
    let linear = linear_table();
    let all = worlds(cat)?;
    let n = all.len();
    for (i, w) in all.iter().enumerate() {
        progress(40 + (i * 50 / n) as u32, "Exporting Banjo-Kazooie worlds and sub-areas");
        export_world(assets, w, &out.join("levels"), &out.join("courses"), &linear)?;
    }
    Ok(())
}
