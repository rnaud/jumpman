//! Port of `scripts/export-bk-banjo.py`: Banjo and Kazooie (high-poly model, bone-tagged vertices, texture atlas,
//! move clips) from the ROM's rarezip asset table. Writes `public/models/bk/banjo.json` and `banjo-extra.json`.
use super::Assets;
use crate::pyc::{dumps, R};
use crate::zpng::{round_f, Num};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

const MODEL: usize = 0x34E;

/// Clip name -> (asset enum suffix, seconds per playthrough).
const CLIPS: &[(&str, &str, f64)] = &[
    ("idle", "6F_ANIM_BSSTAND_IDLE", 5.5),
    ("idlePeck", "95_ANIM_BSSTAND_KAZOOIE_PECK", 5.5),
    ("creep", "2_ANIM_BSWALK_CREEP", 0.8),
    ("walk", "3_ANIM_BSWALK", 0.43),
    ("run", "C_ANIM_BSWALK_RUN", 0.66),
    ("turn", "E_ANIM_BSTURN", 0.5),
    ("jump", "8_ANIM_BSJUMP", 1.9),
    ("fall", "B0_ANIM_BSJUMP_FALL", 0.38),
    ("tumble", "68_ANIM_BSJUMP_TUMBLE", 0.35),
    ("flapEnter", "18_ANIM_BSBFLAP_ENTER", 0.3),
    ("flap", "17_ANIM_BSBFLAP", 0.3),
    ("flipEnter", "4B_ANIM_BSBFLIP_ENTER", 2.2),
    ("flipHold", "4C_ANIM_BSBFLIP_HOLD", 0.13),
    ("flipExit", "61_ANIM_BSBFLIP_EXIT", 0.8),
    ("claw", "5_ANIM_BSPUNCH", 1.3),
    ("roll", "4F_ANIM_BSTWIRL", 0.9),
    ("peckEnter", "19_ANIM_BSBPECK_ENTER", 0.35),
    ("peck", "1A_ANIM_BSBPECK", 0.2),
    ("barge", "1C_ANIM_BSBBARGE", 1.0),
    ("bust", "1D_ANIM_BSBBUSTER", 1.02),
    ("crouchEnter", "1_ANIM_BSCROUCH_ENTER", 0.5),
    ("crouch", "10C_ANIM_BSCROUCH_IDLE", 0.5),
    ("trotIdle", "26_ANIM_BSBTROT_IDLE", 1.2),
    ("trot", "15_ANIM_BSBTROT_WALK", 0.45),
    ("climb", "A_ANIM_BSCLIMB_MOVE", 0.9),
    ("climbIdle", "B2_ANIM_BSCLIMB_IDLE_2", 2.64),
    ("swimIdle", "57_ANIM_BSSWIM_IDLE", 1.2),
    ("swim", "39_ANIM_BSSWIM_MOVE", 0.8),
    ("dive", "3F_ANIM_BSSWIM_DIVE_MOVE", 0.8),
    ("slideFront", "5A_ANIM_BSSLIDE_FRONT", 0.6),
    ("slideBack", "59_ANIM_BSSLIDE_BACK", 0.6),
    ("ow", "4D_ANIM_BSOW", 1.7),
    ("rebound", "F_ANIM_BSREBOUND", 1.0),
    ("die", "9_ANIM_BSDIE", 2.0),
    ("jiggy", "2E_ANIM_BSJIG_JIGGY", 3.0),
    ("carry", "73_ANIM_BSCARRY_WALK", 0.8),
    ("carryIdle", "72_ANIM_BSCARRY_IDLE", 1.0),
    ("throw", "11B_ANIM_BSTHROW", 0.6),
];

const EXTRA_CLIPS: &[(&str, &str, f64)] = &[
    ("eggHead", "2A_ANIM_BSEGGHEAD", 1.0),
    ("eggAss", "2B_ANIM_BSEGGASS", 1.0),
    ("shockCharge", "48_ANIM_BSBSHOCK_CHARGE", 2.8),
    ("shockJump", "49_ANIM_BSBSHOCK_JUMP", 0.8),
    ("wonderEnter", "22_ANIM_BSWHIRL_EXIT", 0.5),
    ("wonderIdle", "23_ANIM_BSWONDERWING_IDLE", 1.0),
    ("wonderWalk", "11_ANIM_BSWHIRL_WALK", 0.53),
    ("wonderJump", "1B_ANIM_BSWHIRL_JUMP", 0.8),
    ("flyEnter", "45_ANIM_BSBFLY_ENTER", 1.4),
    ("fly", "38_ANIM_BSBFLY", 0.62),
    ("bombStart", "43_ANIM_BSBFLY_BEAKBOMB_START", 1.0),
    ("bombCrash", "3E_ANIM_BSBFLY_BEAKBOMB_CRASH", 1.0),
    ("bombEnd", "CC_ANIM_BSFLY_BEAKBOMB_END", 0.38),
    ("trotEnter", "16_ANIM_BSBTROT_ENTER", 0.4),
    ("trotTurbo", "44_ANIM_BSBTROT_TURBO_WALK", 0.4),
];

// ------------------------------------------------------------------------------------------------ big-endian reads

pub fn u8_at(m: &[u8], o: usize) -> R<u8> {
    m.get(o).copied().ok_or_else(|| format!("read outside the data at {o:#x}"))
}
pub fn u16_at(m: &[u8], o: usize) -> R<u16> {
    m.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| format!("read outside the data at {o:#x}"))
}
pub fn i16_at(m: &[u8], o: usize) -> R<i16> {
    u16_at(m, o).map(|v| v as i16)
}
pub fn u32_at(m: &[u8], o: usize) -> R<u32> {
    m.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("read outside the data at {o:#x}"))
}
pub fn i32_at(m: &[u8], o: usize) -> R<i32> {
    u32_at(m, o).map(|v| v as i32)
}
pub fn f32_at(m: &[u8], o: usize) -> R<f32> {
    u32_at(m, o).map(f32::from_bits)
}

/// `[x, y, z, u, v, r, g, b, a, bone]` rows are JSON arrays.
pub fn vertex_row(v: &[Num; 10]) -> Value {
    Value::Array(v.iter().map(|n| n.json()).collect())
}

pub fn rgba16(value: u32) -> [u8; 4] {
    [((value >> 11 & 31) * 255 / 31) as u8, ((value >> 6 & 31) * 255 / 31) as u8, ((value >> 1 & 31) * 255 / 31) as u8, (255 * (value & 1)) as u8]
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn b64(data: &[u8]) -> String {
    base64(data)
}

// ------------------------------------------------------------------------------------------------ model

/// The F3DEX state shared by the Banjo / enemy model builders.
struct State {
    tex_on: bool,
    scale: (f64, f64),
    timg: u32,
    tlut: u32,
    fmt: u32,
    siz: u32,
    wrap: (u32, u32),
    width: u32,
    height: u32,
    texgen: bool,
}

type BatchKey = (String, Option<String>, (u32, u32));

struct Model<'a> {
    m: &'a [u8],
    tex_data: usize,
    vertices: usize,
    commands: usize,
    textures: Vec<(String, Value)>,
    texture_ids: HashMap<String, usize>,
    batches: Vec<(BatchKey, Vec<Value>)>,
    batch_ids: HashMap<BatchKey, usize>,
    cache: Vec<Option<[i64; 10]>>,
    state: State,
    selectors: HashMap<i64, i64>,
}

const PARTS: [(i64, &str); 3] = [(1, "kazooieUpper"), (2, "kazooieFeet"), (8, "kazooieAss")];

impl<'a> Model<'a> {
    fn texture(&mut self, image: u32, tlut: u32, fmt: u32, siz: u32, width: u32, height: u32) -> R<String> {
        let key = format!("{image:x}");
        if self.texture_ids.contains_key(&key) {
            return Ok(key);
        }
        let m = self.m;
        let mut pixels = Vec::new();
        for i in 0..(width * height) as usize {
            let value = if fmt == 2 && siz == 0 {
                let byte = u8_at(m, self.tex_data + image as usize + i / 2)?;
                let index = if i % 2 == 0 { byte >> 4 } else { byte & 15 } as usize;
                u16_at(m, self.tex_data + tlut as usize + 2 * index)?
            } else if fmt == 2 && siz == 1 {
                let index = u8_at(m, self.tex_data + image as usize + i)? as usize;
                u16_at(m, self.tex_data + tlut as usize + 2 * index)?
            } else if fmt == 0 && siz == 2 {
                u16_at(m, self.tex_data + image as usize + 2 * i)?
            } else {
                return Err(format!("texture format {fmt}/{siz}"));
            };
            pixels.extend_from_slice(&rgba16(value as u32));
        }
        self.texture_ids.insert(key.clone(), self.textures.len());
        self.textures.push((key.clone(), json!({"width": width, "height": height, "rgba": base64(&pixels)})));
        Ok(key)
    }

    fn run_dl(&mut self, index: i64, bone: i64, part: &str) -> R<()> {
        let mut p = (self.commands as i64 + 8 * index) as usize;
        let m = self.m;
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
                        let o = self.vertices + (b & 0xFFFFFF) as usize + 16 * i as usize;
                        let v = [i16_at(m, o)? as i64, i16_at(m, o + 2)? as i64, i16_at(m, o + 4)? as i64, i16_at(m, o + 8)? as i64, i16_at(m, o + 10)? as i64, u8_at(m, o + 12)? as i64, u8_at(m, o + 13)? as i64, u8_at(m, o + 14)? as i64, u8_at(m, o + 15)? as i64, bone];
                        *self.cache.get_mut((first + i) as usize).ok_or("vertex cache overflow")? = Some(v);
                    }
                }
                0xBF | 0xB1 => {
                    let words: Vec<u32> = if op == 0xBF { vec![b] } else { vec![a, b] };
                    let textured = self.state.tex_on && !self.state.texgen;
                    let tex = if textured { Some(self.texture(self.state.timg, self.state.tlut, self.state.fmt, self.state.siz, self.state.width, self.state.height)?) } else { None };
                    let key: BatchKey = (part.to_string(), tex, self.state.wrap);
                    let id = match self.batch_ids.get(&key) {
                        Some(&i) => i,
                        None => {
                            self.batch_ids.insert(key.clone(), self.batches.len());
                            self.batches.push((key, vec![]));
                            self.batches.len() - 1
                        }
                    };
                    for word in words {
                        for shift in [16, 8, 0] {
                            let c = self.cache.get(((word >> shift & 0xFF) / 2) as usize).copied().flatten().ok_or("triangle references an empty vertex slot")?;
                            let (mut u, mut v) = (Num::I(0), Num::I(0));
                            if textured {
                                u = Num::F(round_f((c[3] as f64 * self.state.scale.0 / 32.0 + 0.5) / self.state.width as f64, 4));
                                v = Num::F(round_f((c[4] as f64 * self.state.scale.1 / 32.0 + 0.5) / self.state.height as f64, 4));
                            }
                            self.batches[id].1.push(vertex_row(&[Num::I(c[0]), Num::I(c[1]), Num::I(c[2]), u, v, Num::I(c[5]), Num::I(c[6]), Num::I(c[7]), Num::I(c[8]), Num::I(c[9])]));
                        }
                    }
                }
                0xBB => {
                    self.state.tex_on = a & 0xFF != 0;
                    self.state.scale = ((b >> 16) as f64 / 65536.0, (b & 0xFFFF) as f64 / 65536.0);
                }
                0xB7 => {
                    if b & 0x40000 != 0 {
                        self.state.texgen = true;
                    }
                }
                0xB6 => {
                    if b & 0x40000 != 0 {
                        self.state.texgen = false;
                    }
                }
                0xFD => self.state.timg = b & 0xFFFFFF,
                0xF0 => self.state.tlut = self.state.timg,
                0xF5 => {
                    if (b >> 24) & 7 == 0 {
                        self.state.fmt = (a >> 21) & 7;
                        self.state.siz = (a >> 19) & 3;
                        self.state.wrap = ((b >> 8) & 3, (b >> 18) & 3);
                    }
                }
                0xF2 => {
                    if (b >> 24) & 7 == 0 {
                        self.state.width = ((b >> 12) & 0xFFF) / 4 + 1;
                        self.state.height = (b & 0xFFF) / 4 + 1;
                    }
                }
                0x06 if b >> 24 != 3 => return Err(format!("nested display list {b:x}")),
                _ => {}
            }
        }
    }

    fn geo(&mut self, mut p: usize, bone: i64, part: &str) -> R<()> {
        let m = self.m;
        loop {
            let (cmd, nxt) = (u32_at(m, p)?, i32_at(m, p + 4)?);
            match cmd {
                0x02 => {
                    let (branch, mtx) = (u8_at(m, p + 8)?, u8_at(m, p + 9)? as i8);
                    if branch != 0 {
                        self.geo(p + branch as usize, mtx as i64, part)?;
                    }
                }
                0x03 => self.run_dl(i16_at(m, p + 8)? as i64, bone, part)?,
                0x07 => self.run_dl(i16_at(m, p + 10)? as i64, bone, part)?,
                0x05 => {
                    let mut q = p + 8;
                    let mut first = true;
                    loop {
                        let index = i16_at(m, q)?;
                        q += 2;
                        if index == 0 && !first {
                            break;
                        }
                        first = false;
                        self.run_dl(index as i64, bone, part)?;
                    }
                }
                0x06 => self.geo((p as i64 + i32_at(m, p + 8)? as i64) as usize, bone, part)?,
                0x08 => {
                    let (_lod_max, lod_min) = (f32_at(m, p + 8)?, f32_at(m, p + 12)?);
                    let branch = i32_at(m, p + 28)?;
                    if lod_min == 0.0 && branch != 0 {
                        self.geo((p as i64 + branch as i64) as usize, bone, part)?;
                    }
                }
                0x0C => {
                    let (count, index) = (i16_at(m, p + 8)? as i64, i16_at(m, p + 10)? as i64);
                    if let Some((_, name)) = PARTS.iter().find(|(i, _)| *i == index) {
                        self.geo((p as i64 + i32_at(m, p + 12)? as i64) as usize, bone, name)?;
                    } else {
                        let choice = self.selectors.get(&index).copied().unwrap_or(0);
                        if 0 < choice && choice <= count {
                            self.geo((p as i64 + i32_at(m, p + 12 + 4 * (choice as usize - 1))? as i64) as usize, bone, part)?;
                        }
                    }
                }
                0x0D => {
                    let branch = i16_at(m, p + 20)?;
                    if branch != 0 {
                        self.geo((p as i64 + branch as i64) as usize, bone, part)?;
                    }
                }
                0x0A | 0x04 | 0x09 | 0x0B | 0x10 => {}
                _ => return Err(format!("unsupported geo command {cmd:#x}")),
            }
            if nxt == 0 {
                return Ok(());
            }
            p = (p as i64 + nxt as i64) as usize;
        }
    }
}

fn build_model(m: &[u8]) -> R<Value> {
    let (geo_off, tex_list, gfx, vtx, anim_list) = (i32_at(m, 4)?, u16_at(m, 8)? as usize, i32_at(m, 12)?, i32_at(m, 16)?, i32_at(m, 24)? as usize);
    let trans_scale = f32_at(m, anim_list)? as f64;
    let bone_count = i16_at(m, anim_list + 4)?;
    let mut bones = Vec::new();
    for i in 0..bone_count.max(0) as usize {
        let o = anim_list + 8 + 16 * i;
        bones.push(json!({"pivot": [round_f(f32_at(m, o)? as f64, 3), round_f(f32_at(m, o + 4)? as f64, 3), round_f(f32_at(m, o + 8)? as f64, 3)], "id": i16_at(m, o + 12)?, "parent": i16_at(m, o + 14)?}));
    }
    let tex_count = i16_at(m, tex_list + 4)? as usize;
    let mut model = Model {
        m,
        tex_data: tex_list + 8 + 16 * tex_count,
        vertices: vtx as usize + 0x18,
        commands: gfx as usize + 8,
        textures: vec![],
        texture_ids: HashMap::new(),
        batches: vec![],
        batch_ids: HashMap::new(),
        cache: vec![None; 64],
        state: State { tex_on: false, scale: (1.0, 1.0), timg: 0, tlut: 0, fmt: 0, siz: 0, wrap: (0, 0), width: 32, height: 32, texgen: false },
        selectors: [(0x1A, 1), (0x1B, 1), (0x12, 1), (0x13, 1), (0x22, 1), (0x23, 1)].into_iter().collect(),
    };
    model.geo(geo_off as usize, -1, "body")?;
    let mut textures = Map::new();
    for (k, v) in model.textures {
        textures.insert(k, v);
    }
    let batches: Vec<Value> = model.batches.into_iter().map(|((part, tex, wrap), verts)| json!({"part": part, "texture": tex, "wrap": [wrap.0, wrap.1], "vertices": verts})).collect();
    Ok(json!({"transScale": trans_scale, "bones": bones, "textures": Value::Object(textures), "batches": batches}))
}

pub fn build_clip(a: &[u8], duration: f64) -> R<Value> {
    let (start, end, count) = (i16_at(a, 0)?, i16_at(a, 2)?, i16_at(a, 4)?);
    let mut p = 8;
    let mut elements = Vec::new();
    for _ in 0..count.max(0) {
        let (head, keys) = (u16_at(a, p)?, i16_at(a, p + 2)?);
        p += 4;
        let mut data: Vec<Value> = Vec::new();
        for _ in 0..keys.max(0) {
            let (flags, value) = (u16_at(a, p)?, i16_at(a, p + 2)?);
            p += 4;
            data.push(json!(flags >> 14));
            data.push(json!(flags & 0x3FFF));
            data.push(json!(value));
        }
        elements.push(json!([head >> 4, head & 15, data]));
    }
    Ok(json!({"start": start, "end": end, "duration": duration, "elements": elements}))
}

/// `out` is `public/models/bk`; `extras` writes only `banjo-extra.json` (the `--extras` run).
pub fn export(assets: &Assets, ids: &HashMap<String, i64>, out: &Path, extras: bool) -> R<()> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let id_of = |suffix: &str| ids.get(&format!("ASSET_{suffix}")).copied().ok_or_else(|| format!("ASSET_{suffix} not in enums.h"));
    if extras {
        let mut clips = Map::new();
        for (name, suffix, d) in EXTRA_CLIPS.iter().copied() {
            clips.insert(name.into(), build_clip(&assets.read(id_of(suffix)? as usize)?, d)?);
        }
        std::fs::write(out.join("banjo-extra.json"), dumps(&json!({"clips": Value::Object(clips)}), None)).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut asset = build_model(&assets.read(MODEL)?)?;
    let mut clips = Map::new();
    for (name, suffix, d) in CLIPS.iter().copied() {
        clips.insert(name.into(), build_clip(&assets.read(id_of(suffix)? as usize)?, d)?);
    }
    asset.as_object_mut().unwrap().insert("clips".into(), Value::Object(clips));
    std::fs::write(out.join("banjo.json"), dumps(&asset, None)).map_err(|e| e.to_string())
}
