//! The scene / actor model half of `export-oot-courses.py`: `SceneModel` (embedded-glTF writer), the F3DEX2 display list
//! walker `display_mesh`, texture decoding to PNG, `room_mesh` and `actor_model` (skeleton + joint-index animation to a
//! skinned glTF). Floating point operations are kept in the Python's order so the GLB files come out byte-identical.
use super::link::rgba16;
use super::libm;
use super::*;
use crate::pyc::{dumps, py_float, round};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub const WORLD_SCALE: f64 = 4.4;
pub type Segs<'a> = HashMap<u32, &'a [u8]>;
pub type Transform<'a> = dyn Fn(f64, f64, f64) -> [f64; 3] + 'a;

/// `commands(data)`: scene / room header commands, op -> (count, pointer & MASK), last one wins.
pub fn commands(data: &[u8]) -> R<HashMap<u8, (u8, u32)>> {
    let header = commands_at(data, 0)?;
    // Scenes made for one age only (the Fire / Water temples) keep their real header in the alternate-header table (command
    // 0x18: one pointer per scene layer, 0 for the layers without one): take the first present one.
    if !header.contains_key(&3) {
        if let Some(&(_, table)) = header.get(&0x18) {
            for i in 0..8 {
                let ptr = u32_(data, (table & MASK) as usize + 4 * i);
                if ptr != 0 && ptr >> 24 == 2 {
                    let alt = commands_at(data, (ptr & MASK) as usize)?;
                    if alt.contains_key(&3) {
                        return Ok(alt);
                    }
                }
            }
        }
    }
    Ok(header)
}

pub fn commands_at(data: &[u8], start: usize) -> R<HashMap<u8, (u8, u32)>> {
    let mut header = HashMap::new();
    let mut pos = start;
    while pos + 8 <= data.len() {
        let (op, count, pointer) = (data[pos], data[pos + 1], u32_(data, pos + 4));
        if op == 0x14 {
            return Ok(header);
        }
        header.insert(op, (count, pointer & MASK));
        pos += 8;
    }
    Err("Unterminated scene header".into())
}

pub fn png(width: usize, height: usize, pixels: &[u8]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, tag: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        let mut crc = flate2::Crc::new();
        crc.update(tag);
        crc.update(data);
        out.extend_from_slice(&crc.sum().to_be_bytes());
    }
    let mut rows = Vec::with_capacity(height * (width * 4 + 1));
    for y in 0..height {
        rows.push(0);
        rows.extend_from_slice(&pixels[y * width * 4..(y + 1) * width * 4]);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    std::io::Write::write_all(&mut z, &rows).unwrap();
    let idat = z.finish().unwrap();
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &idat);
    chunk(&mut out, b"IEND", b"");
    out
}

/// `decode_image`: a texture in a segment to an RGBA PNG.
#[allow(clippy::too_many_arguments)]
pub fn decode_image(segments: &Segs, address: u32, palette: u32, fmt: u32, size: u32, bank: u32, width: usize, height: usize) -> R<Vec<u8>> {
    // Textures in segments the scene sets at run time (8 / 9: animated water, lava, doors) are not in the file: flat grey.
    let Some(&src) = segments.get(&(address >> 24)) else {
        if matches!(address >> 24, 3..=13) {
            return Ok(png(width, height, &[128u8, 128, 128, 255].repeat(width * height)));
        }
        return Err(format!("KeyError: {}", address >> 24));
    };
    let start = (address & MASK) as usize;
    let mut pixels = Vec::with_capacity(width * height * 4);
    for i in 0..width * height {
        let value: u32 = match size {
            0 => ((src[start + i / 2] >> (if i % 2 != 0 { 0 } else { 4 })) & 15) as u32,
            1 => src[start + i] as u32,
            _ => u16_(src, start + i * 2) as u32,
        };
        let color: [u8; 4] = if fmt == 0 && size == 2 {
            rgba16(value as u16)
        } else if fmt == 2 {
            let pal = *segments.get(&(palette >> 24)).ok_or("KeyError: palette segment")?;
            let index = value as usize + if size == 0 { 16 * bank as usize } else { 0 };
            rgba16(u16_(pal, (palette & MASK) as usize + 2 * index))
        } else if fmt == 4 {
            let i = (value * if size == 0 { 17 } else { 1 }) as u8;
            [i, i, i, i]
        } else if fmt == 3 && size == 2 {
            [(value >> 8) as u8, (value >> 8) as u8, (value >> 8) as u8, (value & 255) as u8]
        } else if fmt == 3 && (size == 0 || size == 1) {
            let intensity = if size == 0 { (value >> 1) * 255 / 7 } else { (value >> 4) * 17 } as u8;
            let alpha = if size == 0 { (value & 1) * 255 } else { (value & 15) * 17 } as u8;
            [intensity, intensity, intensity, alpha]
        } else {
            return Err(format!("Unsupported scene texture format {fmt}/{size}"));
        };
        pixels.extend_from_slice(&color);
    }
    Ok(png(width, height, &pixels))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DetailKey {
    pub address: u32,
    pub dw: i64,
    pub dh: i64,
    pub base_step: [u64; 2],
    pub detail_step: [u64; 2],
    pub out_w: i64,
    pub out_h: i64,
    pub alpha: i64,
}

/// A batch key: everything `material()` needs, hashable.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub address: u32,
    pub palette: u32,
    pub fmt: u32,
    pub size: u32,
    pub bank: u32,
    pub w: i64,
    pub h: i64,
    pub wrap_s: u32,
    pub wrap_t: u32,
    pub mode: &'static str,
    pub tint: [u8; 4],
    pub decal: bool,
    pub lit: bool,
    pub detail: Option<DetailKey>,
}

#[derive(Clone, Debug)]
pub struct Vert {
    pub pos: [f64; 3],
    pub uv: [f64; 2],
    pub rgba: [u8; 4],
    pub bone: Option<usize>,
    /// Normal and whether Python held it as ints (the `(0, 1, 0)` fallback), which prints without `.0` in min / max.
    pub normal: Option<([f64; 3], bool)>,
}

#[derive(Default)]
pub struct Batches {
    index: HashMap<Key, usize>,
    pub keys: Vec<Key>,
    pub verts: Vec<Vec<Vert>>,
}

impl Batches {
    pub fn entry(&mut self, k: &Key) -> &mut Vec<Vert> {
        let i = match self.index.get(k) {
            Some(i) => *i,
            None => {
                self.keys.push(k.clone());
                self.verts.push(Vec::new());
                self.index.insert(k.clone(), self.keys.len() - 1);
                self.keys.len() - 1
            }
        };
        &mut self.verts[i]
    }
}

pub fn terrain_detail_image(segments: &Segs, base: u32, width: usize, height: usize, d: &DetailKey) -> R<Vec<u8>> {
    let tile = |addr: u32, w: i64, h: i64| -> R<Vec<[u8; 3]>> {
        let seg = *segments.get(&(addr >> 24)).ok_or("KeyError: detail segment")?;
        let from = (addr & MASK) as usize;
        let n = ((w * h * 2) as usize).min(seg.len().saturating_sub(from));
        Ok((0..n / 2).map(|i| {
            let c = rgba16(u16_(seg, from + 2 * i));
            [c[0], c[1], c[2]]
        })
        .collect())
    };
    let sample = |pixels: &[[u8; 3]], w: i64, h: i64, x: f64, y: f64| -> R<[f64; 3]> {
        let (ix, iy) = (x as i64, y as i64);
        let (fx, fy) = (x - ix as f64, y - iy as f64);
        let at = |i: i64, j: i64| -> R<[u8; 3]> { pixels.get(((j % h) * w + i % w) as usize).copied().ok_or_else(|| "IndexError: detail pixel".to_string()) };
        let (a, b, c, dd) = (at(ix, iy)?, at(ix + 1, iy)?, at(ix, iy + 1)?, at(ix + 1, iy + 1)?);
        let mut out = [0.0; 3];
        for k in 0..3 {
            out[k] = (a[k] as f64 * (1.0 - fx) + b[k] as f64 * fx) * (1.0 - fy) + (c[k] as f64 * (1.0 - fx) + dd[k] as f64 * fx) * fy;
        }
        Ok(out)
    };
    let (first, second) = (tile(base, width as i64, height as i64)?, tile(d.address, d.dw, d.dh)?);
    let alpha = d.alpha as f64;
    let mut pixels = Vec::with_capacity((d.out_w * d.out_h * 4) as usize);
    for y in 0..d.out_h {
        for x in 0..d.out_w {
            let a = sample(&first, width as i64, height as i64, x as f64 * f64::from_bits(d.base_step[0]), y as f64 * f64::from_bits(d.base_step[1]))?;
            let b = sample(&second, d.dw, d.dh, x as f64 * f64::from_bits(d.detail_step[0]), y as f64 * f64::from_bits(d.detail_step[1]))?;
            for k in 0..3 {
                pixels.push(round((a[k] * (255.0 - alpha) + b[k] * alpha) / 255.0) as u8);
            }
            pixels.push(255);
        }
    }
    Ok(png(d.out_w as usize, d.out_h as usize, &pixels))
}

/// Embedded glTF: triangle batches plus their textures.
pub struct SceneModel {
    pub data: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
    images: Vec<Value>,
    textures: Vec<Value>,
    samplers: Vec<Value>,
    materials: Vec<Value>,
    primitives: Vec<Value>,
    pub nodes: Vec<Map<String, Value>>,
    pub scene_nodes: Vec<i64>,
    pub extra: Vec<(String, Value)>,
    cache: HashMap<(String, Key), usize>,
}

pub enum Rows<'a> {
    F(&'a [Vec<f64>]),
}

impl SceneModel {
    pub fn new() -> SceneModel {
        let mut node0 = Map::new();
        node0.insert("mesh".into(), 0.into());
        SceneModel { data: Vec::new(), buffer_views: vec![], accessors: vec![], images: vec![], textures: vec![], samplers: vec![], materials: vec![], primitives: vec![], nodes: vec![node0], scene_nodes: vec![0], extra: vec![], cache: HashMap::new() }
    }

    pub fn view(&mut self, bytes: &[u8]) -> usize {
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        self.buffer_views.push(json!({"buffer": 0, "byteOffset": self.data.len(), "byteLength": bytes.len()}));
        self.data.extend_from_slice(bytes);
        self.buffer_views.len() - 1
    }

    /// `attribute(rows, kind, component, fmt)`: `rows` are f64 rows packed as f32 (or u8 / u16 where `component` says so).
    /// `int_flags` marks components Python held as ints (only matters for the min / max of a VEC3).
    pub fn attribute_f32(&mut self, rows: &[Vec<f64>], kind: &str, int_flags: Option<&[[bool; 3]]>) -> usize {
        let mut bytes = Vec::with_capacity(rows.len() * rows[0].len() * 4);
        for r in rows {
            for &v in r {
                bytes.extend_from_slice(&(v as f32).to_le_bytes());
            }
        }
        let view = self.view(&bytes);
        let mut acc = Map::new();
        acc.insert("bufferView".into(), view.into());
        acc.insert("componentType".into(), 5126.into());
        acc.insert("count".into(), rows.len().into());
        acc.insert("type".into(), kind.into());
        if kind == "SCALAR" || kind == "VEC3" {
            let n = rows[0].len();
            let (mut mins, mut maxs) = (Vec::new(), Vec::new());
            for k in 0..n {
                let (mut lo, mut hi) = (0usize, 0usize);
                for (i, r) in rows.iter().enumerate() {
                    if r[k] < rows[lo][k] {
                        lo = i;
                    }
                    if r[k] > rows[hi][k] {
                        hi = i;
                    }
                }
                let val = |i: usize| -> Value {
                    if int_flags.is_some_and(|f| f[i][k]) {
                        Value::from(rows[i][k] as i64)
                    } else {
                        Value::from(rows[i][k])
                    }
                };
                mins.push(val(lo));
                maxs.push(val(hi));
            }
            acc.insert("min".into(), Value::Array(mins));
            acc.insert("max".into(), Value::Array(maxs));
        }
        self.accessors.push(Value::Object(acc));
        self.accessors.len() - 1
    }

    /// Unsigned-integer attribute (`COLOR_0` as normalized bytes, `JOINTS_0` as shorts).
    pub fn attribute_uint(&mut self, bytes: Vec<u8>, count: usize, kind: &str, component: u32) -> usize {
        let view = self.view(&bytes);
        let mut acc = Map::new();
        acc.insert("bufferView".into(), view.into());
        acc.insert("componentType".into(), component.into());
        acc.insert("count".into(), count.into());
        acc.insert("type".into(), kind.into());
        if component == 5121 {
            acc.insert("normalized".into(), true.into());
        }
        self.accessors.push(Value::Object(acc));
        self.accessors.len() - 1
    }

    pub fn material(&mut self, key: &Key, segments: &Segs, room: &str) -> R<usize> {
        let ck = (room.to_string(), key.clone());
        if let Some(i) = self.cache.get(&ck) {
            return Ok(*i);
        }
        let mut m = Map::new();
        m.insert("doubleSided".into(), true.into());
        m.insert("extensions".into(), json!({"KHR_materials_unlit": {}}));
        m.insert("alphaMode".into(), key.mode.into());
        if key.lit {
            m.shift_remove("extensions");
            m.insert("pbrMetallicRoughness".into(), json!({"metallicFactor": 0, "roughnessFactor": 1}));
        }
        if key.decal {
            m.insert("extras".into(), json!({"ootDecal": true}));
        }
        if key.mode == "MASK" {
            m.insert("alphaCutoff".into(), 0.4.into());
        }
        if key.address != 0 {
            let image = match &key.detail {
                Some(d) => terrain_detail_image(segments, key.address, key.w as usize, key.h as usize, d)?,
                None => decode_image(segments, key.address, key.palette, key.fmt, key.size, key.bank, key.w as usize, key.h as usize)?,
            };
            let view = self.view(&image);
            self.images.push(json!({"bufferView": view, "mimeType": "image/png"}));
            let wrap = [10497, 33648, 33071, 33648];
            self.samplers.push(json!({"magFilter": 9729, "minFilter": 9987, "wrapS": wrap[key.wrap_s as usize], "wrapT": wrap[key.wrap_t as usize]}));
            let i = self.textures.len();
            self.textures.push(json!({"source": i, "sampler": i}));
            let linear: Vec<Value> = key.tint[..3]
                .iter()
                .map(|&c| {
                    let c = c as f64;
                    Value::from(if c > 10.0 { libm::pow_((c / 255.0 + 0.055) / 1.055, 2.4) } else { c / 3294.6 })
                })
                .chain(std::iter::once(Value::from(key.tint[3] as f64 / 255.0)))
                .collect();
            let pbr = m.entry("pbrMetallicRoughness").or_insert_with(|| json!({}));
            let pbr = pbr.as_object_mut().unwrap();
            pbr.insert("baseColorTexture".into(), json!({"index": i}));
            pbr.insert("baseColorFactor".into(), Value::Array(linear));
        }
        let index = self.materials.len();
        self.materials.push(Value::Object(m));
        self.cache.insert(ck, index);
        Ok(index)
    }

    pub fn batch(&mut self, vertices: &[Vert], material: usize) {
        let pos: Vec<Vec<f64>> = vertices.iter().map(|v| v.pos.to_vec()).collect();
        let a_pos = self.attribute_f32(&pos, "VEC3", None);
        let uv: Vec<Vec<f64>> = vertices.iter().map(|v| v.uv.to_vec()).collect();
        let a_uv = self.attribute_f32(&uv, "VEC2", None);
        let col: Vec<u8> = vertices.iter().flat_map(|v| v.rgba).collect();
        let a_col = self.attribute_uint(col, vertices.len(), "VEC4", 5121);
        let mut attrs = Map::new();
        attrs.insert("POSITION".into(), a_pos.into());
        attrs.insert("TEXCOORD_0".into(), a_uv.into());
        attrs.insert("COLOR_0".into(), a_col.into());
        if vertices[0].normal.is_some() {
            let rows: Vec<Vec<f64>> = vertices.iter().map(|v| v.normal.unwrap().0.to_vec()).collect();
            let flags: Vec<[bool; 3]> = vertices.iter().map(|v| [v.normal.unwrap().1; 3]).collect();
            attrs.insert("NORMAL".into(), self.attribute_f32(&rows, "VEC3", Some(&flags)).into());
        }
        if vertices[0].bone.is_some() {
            let mut b = Vec::with_capacity(vertices.len() * 8);
            for v in vertices {
                b.extend_from_slice(&(v.bone.unwrap() as u16).to_le_bytes());
                b.extend_from_slice(&[0; 6]);
            }
            attrs.insert("JOINTS_0".into(), self.attribute_uint(b, vertices.len(), "VEC4", 5123).into());
            let w: Vec<Vec<f64>> = vertices.iter().map(|_| vec![1.0, 0.0, 0.0, 0.0]).collect();
            attrs.insert("WEIGHTS_0".into(), self.attribute_f32(&w, "VEC4", None).into());
        }
        self.primitives.push(json!({"attributes": attrs, "material": material}));
    }

    pub fn add_batches(&mut self, batches: &Batches, segments: &Segs, name: &str) -> R<()> {
        for (key, verts) in batches.keys.iter().zip(&batches.verts) {
            let m = self.material(key, segments, name)?;
            self.batch(verts, m);
        }
        Ok(())
    }

    pub fn write(&mut self, path: &Path) -> R<()> {
        let mut doc = Map::new();
        doc.insert("asset".into(), json!({"version": "2.0", "extras": {"sm64UnitsToMeters": 1}}));
        doc.insert("buffers".into(), json!([{"byteLength": self.data.len()}]));
        doc.insert("bufferViews".into(), Value::Array(self.buffer_views.clone()));
        doc.insert("accessors".into(), Value::Array(self.accessors.clone()));
        doc.insert("images".into(), Value::Array(self.images.clone()));
        doc.insert("textures".into(), Value::Array(self.textures.clone()));
        doc.insert("samplers".into(), Value::Array(self.samplers.clone()));
        doc.insert("materials".into(), Value::Array(self.materials.clone()));
        doc.insert("meshes".into(), json!([{"primitives": self.primitives}]));
        doc.insert("nodes".into(), Value::Array(self.nodes.iter().cloned().map(Value::Object).collect()));
        doc.insert("scenes".into(), json!([{"nodes": self.scene_nodes}]));
        doc.insert("scene".into(), 0.into());
        doc.insert("extensionsUsed".into(), json!(["KHR_materials_unlit"]));
        for (k, v) in &self.extra {
            doc.insert(k.clone(), v.clone());
        }
        let mut meta = dumps(&Value::Object(doc), None).into_bytes();
        while meta.len() % 4 != 0 {
            meta.push(b' ');
        }
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        let mut out = Vec::with_capacity(28 + meta.len() + self.data.len());
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&((28 + meta.len() + self.data.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&meta);
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&self.data);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
    }
}

impl Default for SceneModel {
    fn default() -> Self {
        Self::new()
    }
}

/// The scene's terrain lighting: ambient, key colour, fill colour, key direction, fill direction.
pub type TerrainLight = ([i64; 3], [i64; 3], [i64; 3], [f64; 3], [f64; 3]);

/// CPython 3.9 `math.hypot(*coords)` for floats (the `vector_norm` algorithm).
pub fn hypot3(v: [f64; 3]) -> f64 {
    let max = v.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    if max == 0.0 {
        return max;
    }
    let (mut csum, mut frac) = (1.0f64, 0.0f64);
    for x in v {
        let x = x / max;
        let x = x * x;
        let old = csum;
        csum += x;
        frac += (old - csum) + x;
    }
    max * (csum - 1.0 + frac).sqrt()
}

pub struct Mesh<'a> {
    pub name: &'a str,
    pub segments: &'a Segs<'a>,
    pub transform: &'a Transform<'a>,
    pub layer: u32,
    pub joint: Option<usize>,
    pub matrices: &'a [usize],
    pub terrain_light: Option<&'a TerrainLight>,
    pub texture_defs: Option<&'a HashMap<u32, Attrs>>,
    pub skin_bones: Option<&'a [usize]>,
}

fn tile_scale(shift: u32) -> f64 {
    if shift <= 10 {
        libm::pow_(2.0, -(shift as f64))
    } else {
        libm::pow_(2.0, (16 - shift as i64) as f64)
    }
}

type Cached = ([f64; 3], f64, f64, [u8; 4], Option<usize>, ([f64; 3], bool));

impl Mesh<'_> {
    /// `display_mesh(...)` appending to `batches`.
    pub fn run(&self, pointer: u32, batches: &mut Batches) -> R<()> {
        let list_segment = pointer >> 24;
        let room = *self.segments.get(&list_segment).ok_or_else(|| format!("KeyError: {list_segment}"))?;
        let name = self.name;
        let mut joint = self.joint;
        let mut stack: Vec<usize> = Vec::new();
        let mut cache: HashMap<i64, Cached> = HashMap::new();
        let (mut image, mut palette) = (0u32, 0u32);
        let (mut fmt, mut size, mut bank, mut wrap_s, mut wrap_t) = (0u32, 0u32, 0u32, 0u32, 0u32);
        let (mut w, mut h) = (1i64, 1i64);
        let (mut u0, mut v0) = (0.0f64, 0.0f64);
        let mut lighting = joint.is_some() && name != "gameplay_keep";
        let mut decal = false;
        let mut primitive = [255u8; 4];
        let mut blend_item_color = false;
        let mut env_tunic: Option<[u8; 3]> = None;
        let (mut tint_rgb, mut tint_alpha, mut vertex_alpha, mut texture_alpha) = (false, false, false, false);
        let mut loads: HashMap<u32, u32> = HashMap::new();
        let (mut tmem, mut load_tmem) = (0u32, 0u32);
        let terrain = self.terrain_light.is_some();
        let mut env_alpha = 128i64;
        let mut terrain_blend = false;
        let mut detail_tile: Option<(u32, f64, f64, u32, u32)> = None;
        let mut detail_size: Option<(i64, i64)> = None;
        let mut uv_scale = (1.0f64, 1.0f64);
        let mut p = (pointer & MASK) as usize;
        loop {
            let (a, b) = (u32_(room, p), u32_(room, p + 4));
            p += 8;
            let op = a >> 24;
            if op == 0xdf {
                if let Some(q) = stack.pop() {
                    p = q;
                    continue;
                }
                break;
            }
            if op == 0xde {
                if name == "object_kw1" && (b >> 24 == 8 || b >> 24 == 9) {
                    env_tunic = Some(if b >> 24 == 8 { [70, 190, 60] } else { [100, 30, 0] });
                }
                if name == "object_km1" && (b >> 24 == 8 || b >> 24 == 9) {
                    env_tunic = Some(if b >> 24 == 8 { [0, 130, 70] } else { [110, 170, 20] });
                }
                if b >> 24 == list_segment {
                    if (a >> 16) & 255 == 0 {
                        stack.push(p);
                    }
                    p = (b & MASK) as usize;
                }
            } else if op == 0xda && joint.is_some() && b >> 24 == 13 {
                joint = Some(*self.matrices.get(((b & MASK) / 64) as usize).ok_or("IndexError: matrix")?);
            } else if op == 0xd9 {
                lighting = (lighting && a & 0x20000 != 0) || b & 0x20000 != 0;
            } else if a == 0xe200_001c {
                decal = b & 0xc00 == 0xc00;
            } else if op == 0xfd {
                image = b;
            } else if op == 0xf3 || op == 0xf4 {
                loads.insert(load_tmem, image);
            } else if op == 0xf0 {
                palette = image;
            } else if op == 0xf5 && b >> 24 == 7 {
                load_tmem = a & 511;
            } else if op == 0xf5 && b >> 24 == 0 {
                if terrain {
                    uv_scale = (tile_scale(b & 15), tile_scale((b >> 10) & 15));
                }
                tmem = a & 511;
                fmt = (a >> 21) & 7;
                size = (a >> 19) & 3;
                bank = (b >> 20) & 15;
                wrap_s = (b >> 8) & 3;
                wrap_t = (b >> 18) & 3;
            } else if terrain && op == 0xf5 && b >> 24 == 1 {
                detail_tile = Some((a & 511, tile_scale(b & 15), tile_scale((b >> 10) & 15), (a >> 21) & 7, (a >> 19) & 3));
            } else if terrain && op == 0xf2 && b >> 24 == 1 {
                detail_size = Some(((((b >> 12 & 4095) as i64) - ((a >> 12 & 4095) as i64)).div_euclid(4) + 1, (((b & 4095) as i64) - ((a & 4095) as i64)).div_euclid(4) + 1));
            } else if terrain && op == 0xfb {
                env_alpha = (b & 255) as i64;
            } else if op == 0xf2 && b >> 24 == 0 {
                u0 = (a >> 12 & 4095) as f64 / 4.0;
                v0 = (a & 4095) as f64 / 4.0;
                w = ((b >> 12 & 4095) as f64 / 4.0 - u0 + 1.0) as i64;
                h = ((b & 4095) as f64 / 4.0 - v0 + 1.0) as i64;
            } else if op == 0xfa {
                primitive = b.to_be_bytes();
            } else if op == 0xfc {
                terrain_blend = terrain && a == 0xfc26_7e04 && b == 0x1ffc_fdf8;
                tint_rgb = a & 31 == 3;
                blend_item_color = name.starts_with("object_gi_") && ((a >> 20 & 15) == 3 || (b >> 28) == 3);
                let alpha_inputs = [a >> 12 & 7, b >> 12 & 7, a >> 9 & 7, b >> 9 & 7, b >> 21 & 7, b >> 3 & 7, b >> 18 & 7, b & 7];
                tint_alpha = alpha_inputs.contains(&3);
                vertex_alpha = alpha_inputs.contains(&4);
                texture_alpha = alpha_inputs.contains(&1) || alpha_inputs.contains(&2);
            } else if op == 1 {
                let n = ((a >> 12) & 255) as i64;
                let first = ((a & 255) / 2) as i64 - n;
                let vseg = *self.segments.get(&(b >> 24)).ok_or_else(|| format!("KeyError: {}", b >> 24))?;
                for i in 0..n {
                    let q = (b & MASK) as usize + i as usize * 16;
                    let (x, y, z, s, t) = (i16_(vseg, q), i16_(vseg, q + 2), i16_(vseg, q + 4), i16_(vseg, q + 8), i16_(vseg, q + 10));
                    let (r, g, blue, alpha) = (vseg[q + 12], vseg[q + 13], vseg[q + 14], vseg[q + 15]);
                    let mut color = if lighting { [255, 255, 255, alpha] } else { [r, g, blue, alpha] };
                    let nrm = [r as i8 as f64, g as i8 as f64, blue as i8 as f64];
                    let length = hypot3(nrm);
                    let normal = if length != 0.0 { ([nrm[0] / length, nrm[1] / length, nrm[2] / length], false) } else { ([0.0, 1.0, 0.0], true) };
                    if terrain && lighting {
                        let (ambient, key, fill, sun_dir, fill_dir) = self.terrain_light.unwrap();
                        let dot = |d: &[f64; 3]| normal.0[0] * d[0] + normal.0[1] * d[1] + normal.0[2] * d[2];
                        let sun = dot(sun_dir).max(0.0);
                        let moon = dot(fill_dir).max(0.0);
                        for k in 0..3 {
                            let shade = ((ambient[k] as f64 + key[k] as f64 * sun + fill[k] as f64 * moon) / 255.0).min(1.0);
                            color[k] = round(255.0 * libm::pow_((shade + 0.055) / 1.055, 2.4)) as u8;
                        }
                        color[3] = alpha;
                    }
                    let bone = match self.skin_bones {
                        Some(sb) if !sb.is_empty() && b >> 24 == 8 => Some(*sb.get(((b & MASK) / 16) as usize + i as usize).ok_or("IndexError: skin bone")?),
                        _ => joint,
                    };
                    cache.insert(first + i, ((self.transform)(x as f64, y as f64, z as f64), s as f64 / 32.0, t as f64 / 32.0, color, bone, normal));
                }
            } else if op == 5 || op == 6 {
                let texture = loads.get(&tmem).copied().unwrap_or(0);
                let rgb = env_tunic.unwrap_or(if tint_rgb || blend_item_color { [primitive[0], primitive[1], primitive[2]] } else { [255, 255, 255] });
                let tint = [rgb[0], rgb[1], rgb[2], if tint_alpha { primitive[3] } else { 255 }];
                let mode = if self.layer != 0 {
                    "BLEND"
                } else if texture_alpha {
                    "MASK"
                } else {
                    "OPAQUE"
                };
                let lit = joint.is_some() && lighting && name != "gameplay_keep";
                if let Some(defs) = self.texture_defs.filter(|d| !d.is_empty()) {
                    if let Some(spec) = defs.get(&texture) {
                        w = spec.int("Width") as i64;
                        h = spec.int("Height") as i64;
                        let f = spec.get("Format").unwrap_or("");
                        let split = f.find(|c: char| c.is_ascii_digit()).ok_or_else(|| format!("bad texture format {f}"))?;
                        fmt = match &f[..split] {
                            "rgba" => 0,
                            "ci" => 2,
                            "ia" => 3,
                            "i" => 4,
                            o => return Err(format!("KeyError: {o}")),
                        };
                        size = ["4", "8", "16", "32"].iter().position(|s| *s == &f[split..]).ok_or_else(|| format!("bad texture size {f}"))? as u32;
                        if fmt == 2 && spec.has("TlutOffset") {
                            palette = (6 << 24) + hex(spec.get("TlutOffset").unwrap()) as u32;
                        }
                    }
                }
                let mut detail = None;
                let (mut us, mut vs) = uv_scale;
                let (mut uw, mut vh) = (w, h);
                if let (true, Some(dt), Some(ds)) = (terrain_blend, detail_tile, detail_size) {
                    if dt.0 != tmem && loads.contains_key(&dt.0) && (fmt, size) == (dt.3, dt.4) && (fmt, size) == (0, 2) {
                        let (slot, dsx, dty) = (dt.0, dt.1, dt.2);
                        let (dw, dh) = ds;
                        let (pu, pv) = ((w as f64 / us).max(dw as f64 / dsx), (h as f64 / vs).max(dh as f64 / dty));
                        let (su, sv) = (us.max(dsx), vs.max(dty));
                        uw = round(pu * su);
                        vh = round(pv * sv);
                        detail = Some(DetailKey { address: loads[&slot], dw, dh, base_step: [(us / su).to_bits(), (vs / sv).to_bits()], detail_step: [(dsx / su).to_bits(), (dty / sv).to_bits()], out_w: uw, out_h: vh, alpha: env_alpha });
                        us = su;
                        vs = sv;
                    }
                }
                let key = Key { address: texture, palette: if fmt == 2 { palette } else { 0 }, fmt, size, bank, w, h, wrap_s, wrap_t, mode, tint, decal, lit, detail };
                let words: &[u32] = if op == 6 { &[a, b] } else { &[a] };
                let batch = batches.entry(&key);
                for &word in words {
                    for shift in [16, 8, 0] {
                        let idx = ((word >> shift & 255) / 2) as i64;
                        let (pos, s, t, color, bone, normal) = cache.get(&idx).ok_or_else(|| format!("KeyError: {idx}"))?;
                        let rgba = if vertex_alpha { *color } else { [color[0], color[1], color[2], 255] };
                        batch.push(Vert { pos: *pos, uv: [(s * us - u0 + 0.5) / uw as f64, (t * vs - v0 + 0.5) / vh as f64], rgba, bone: *bone, normal: if lit { Some(*normal) } else { None } });
                    }
                }
            }
        }
        Ok(())
    }

    /// The standalone form: its own batches, turned into primitives of `model`.
    pub fn into_model(&self, model: &mut SceneModel, pointer: u32) -> R<()> {
        let mut batches = Batches::default();
        self.run(pointer, &mut batches)?;
        model.add_batches(&batches, self.segments, self.name)
    }
}

/// `room_mesh(model, room_name, segments, transform, terrain_light)`.
/// Returns the axis-aligned bounds of the room's vertices (the current-room tracker's input).
pub fn room_mesh(model: &mut SceneModel, room_name: &str, segments: &Segs, transform: &Transform, terrain_light: &TerrainLight) -> R<Option<([f64; 3], [f64; 3])>> {
    let room = *segments.get(&3).ok_or("KeyError: 3")?;
    let (_, shape) = *commands(room)?.get(&0x0a).ok_or("KeyError: 10")?;
    let shape = shape as usize;
    let (kind, count, entries) = (room[shape], room[shape + 1], (u32_(room, shape + 4) & MASK) as usize);
    if kind > 2 {
        return Err(format!("Unsupported room shape {kind}"));
    }
    // Pre-rendered rooms (shape 1) have one display list entry whatever their background count (`amountType` 1 single, 2 multi).
    let count = if kind == 1 { 1 } else { count };
    let stride = if kind == 2 { 16 } else { 8 };
    let mut batches = Batches::default();
    let mesh = |layer: u32| Mesh { name: room_name, segments, transform, layer, joint: None, matrices: &[], terrain_light: Some(terrain_light), texture_defs: None, skin_bones: None };
    for entry in 0..count as usize {
        let at = entries + (entry + 1) * stride - 8;
        for (layer, pointer) in [u32_(room, at), u32_(room, at + 4)].into_iter().enumerate() {
            if pointer != 0 {
                mesh(layer as u32).run(pointer, &mut batches)?;
            }
        }
    }
    let mut bounds: Option<([f64; 3], [f64; 3])> = None;
    for v in batches.verts.iter().flatten() {
        let b = bounds.get_or_insert((v.pos, v.pos));
        for k in 0..3 {
            b.0[k] = b.0[k].min(v.pos[k]);
            b.1[k] = b.1[k].max(v.pos[k]);
        }
    }
    model.add_batches(&batches, segments, room_name)?;
    Ok(bounds)
}

// -------------------------------------------------------------------------------------------------- actors

pub struct Resident {
    pub scale: f64,
    pub textures: Vec<String>,
}

#[derive(Default)]
pub struct Actor<'a> {
    pub asset: &'a str,
    pub body: &'a str,
    pub skeleton: Sym<'a>,
    pub animation: Sym<'a>,
    pub animation_bank: Option<&'a str>,
    pub head: Option<&'a str>,
    pub pose: Option<i64>,
    pub resident: Option<&'a Resident>,
    pub model_scale: Option<f64>,
    pub output_folder: Option<&'a str>,
    pub extra_clips: Option<Vec<(&'a str, &'a str)>>,
}

/// A symbol name of the object's xml or a raw offset.
#[derive(Clone, Copy, Default)]
pub enum Sym<'a> {
    #[default]
    None,
    Name(&'a str),
    Off(usize),
}

impl<'a> From<&'a str> for Sym<'a> {
    fn from(s: &'a str) -> Self {
        Sym::Name(s)
    }
}
impl From<usize> for Sym<'_> {
    fn from(s: usize) -> Self {
        Sym::Off(s)
    }
}

fn default_clips(asset: &str) -> Vec<(&'static str, &'static str, Option<i64>)> {
    match asset {
        "gohma" => vec![("Stunned", "gGohmaStunnedAnim", None), ("Attack", "gGohmaAttackAnim", None), ("Death", "gGohmaDeathAnim", None)],
        "sword_chest" => vec![("Open", "gTreasureChestAnim_000128", None)],
        "mido" => vec![("Walk", "gMidoWalkAnim", None), ("Talk", "gMidoHaltAnim", None), ("Curious", "gMidoCuriousAnim", None), ("Annoyed", "gMidoAnnoyedAnim", None)],
        "saria" => vec![("Talk", "gSariaRightArmExtendedWaitAnim", None), ("Surprise", "gSariaHandsOnFaceAnim", None)],
        "kokiri_0" => vec![("Talk", "gKokiriWipingForeheadAnim", None)],
        "kokiri_2" | "kokiri_4" => vec![("Talk", "gKokiriBlockingAnim", Some(1))],
        _ => vec![],
    }
}

/// The pointers inside `object_kingdodongo` use a layout with 0x1400 bytes more than the file between its animations and its
/// limbs (the xml names are in that layout too). Find where the gap goes by checking that the 48 limbs of the skeleton at 0x1B310
/// then all read as limbs (child / sibling indices, display lists that start with a display-list command) and insert it.
fn king_dodongo_layout(file: &[u8]) -> R<Vec<u8>> {
    const GAP: usize = 0x1400;
    const SKEL: usize = 0x1B310;
    let build = |at: usize| -> Vec<u8> {
        let mut v = file[..at].to_vec();
        v.extend(std::iter::repeat_n(0u8, GAP));
        v.extend_from_slice(&file[at..]);
        v
    };
    let ok = |b: &[u8]| -> bool {
        if SKEL + 8 > b.len() || b[SKEL + 4] != 48 {
            return false;
        }
        let t = u32_(b, SKEL);
        if t >> 24 != 6 || (t & MASK) as usize + 192 > b.len() {
            return false;
        }
        (0..48).all(|k| {
            let l = (u32_(b, (t & MASK) as usize + 4 * k) & MASK) as usize;
            if u32_(b, (t & MASK) as usize + 4 * k) >> 24 != 6 || l + 12 > b.len() {
                return false;
            }
            let (child, sibling, dl) = (b[l + 6], b[l + 7], u32_(b, l + 8));
            let dl_ok = dl == 0 || (dl >> 24 == 6 && (dl & MASK) as usize + 8 < b.len() && matches!(b[(dl & MASK) as usize], 0xE7 | 0xFD | 0xFC | 0xDE | 0xD7 | 0x01 | 0xDF | 0xE3 | 0xE2 | 0xFA | 0xF5 | 0xF2 | 0xBB | 0xFB));
            (child == 255 || child < 48) && (sibling == 255 || sibling < 48) && dl_ok
        })
    };
    (0x8000..file.len().min(SKEL)).step_by(16).map(build).find(|b| ok(b)).ok_or_else(|| "king dodongo layout not found".to_string())
}

/// `actor_model(...)`: standard OoT FlexSkeleton and joint-index animation to a skinned glTF, written to `out`.
pub fn actor_model(ctx: &Ctx, o: &Actor, out: &Path) -> R<()> {
    let body_seg = ctx.segment(o.body)?;
    let padded;
    let body: &[u8] = if o.body == "object_kingdodongo" {
        padded = king_dodongo_layout(&body_seg)?;
        &padded
    } else {
        &body_seg
    };
    let addr = |bank: &str, s: Sym| -> R<usize> {
        match s {
            Sym::Off(v) => Ok(v),
            Sym::Name(n) => ctx.object_defs(bank)?.offset(n),
            Sym::None => Err("missing symbol".into()),
        }
    };
    let skel = addr(o.body, o.skeleton)?;
    let (table, count) = (u32_(body, skel), body[skel + 4] as usize);
    let horse = o.resident.is_some() && (o.body == "object_horse_normal" || o.body == "object_horse_link_child");
    let mut limb_data: HashMap<usize, (Vec<u8>, Vec<usize>)> = HashMap::new();
    let texture_defs: Option<HashMap<u32, Attrs>> = if o.resident.is_some() {
        Some(ctx.object_defs(o.body)?.values().filter(|v| v.has("Format") && v.has("Offset")).map(|v| (0x0600_0000 + v.offset() as u32, v.clone())).collect())
    } else {
        None
    };
    struct L {
        x: i16,
        y: i16,
        z: i16,
        child: u8,
        sibling: u8,
        dl: u32,
    }
    let mut limbs: Vec<L> = Vec::new();
    for j in 0..count {
        let limb = (u32_(body, (table & MASK) as usize + 4 * j) & MASK) as usize;
        let (x, y, z, child, sibling, mut dl) = (i16_(body, limb), i16_(body, limb + 2), i16_(body, limb + 4), body[limb + 6], body[limb + 7], u32_(body, limb + 8));
        if horse {
            let kind = dl;
            dl = u32_(body, limb + 12);
            if kind == 4 {
                let d = (dl & MASK) as usize;
                let (nv, nm, mods) = (u16_(body, d) as usize, u16_(body, d + 2) as usize, u32_(body, d + 4));
                dl = u32_(body, d + 8);
                let mut buf = vec![0u8; nv * 16];
                let mut bones = vec![0usize; nv];
                for m in 0..nm {
                    let q = (mods & MASK) as usize + 16 * m;
                    let (n, nt, vs, ts) = (u16_(body, q) as usize, u16_(body, q + 2) as usize, (u32_(body, q + 8) & MASK) as usize, (u32_(body, q + 12) & MASK) as usize);
                    // max by weight, first one wins a tie
                    let mut best: Option<(u8, i16, i16, i16, u8)> = None;
                    for t in 0..nt {
                        let r = ts + 10 * t;
                        let tr = (body[r], i16_(body, r + 2), i16_(body, r + 4), i16_(body, r + 6), body[r + 8]);
                        if best.is_none_or(|b| tr.4 > b.4) {
                            best = Some(tr);
                        }
                    }
                    let (bone, px, py, pz, _) = best.ok_or("ValueError: max() of an empty sequence")?;
                    for v in 0..n {
                        let r = vs + 10 * v;
                        let index = u16_(body, r) as usize;
                        let at = index * 16;
                        buf[at..at + 2].copy_from_slice(&px.to_be_bytes());
                        buf[at + 2..at + 4].copy_from_slice(&py.to_be_bytes());
                        buf[at + 4..at + 6].copy_from_slice(&pz.to_be_bytes());
                        buf[at + 8..at + 16].copy_from_slice(&body[r + 2..r + 10]);
                        bones[index] = bone as usize;
                    }
                }
                limb_data.insert(j, (buf, bones));
            }
        }
        limbs.push(L { x, y, z, child, sibling, dl });
    }
    let scale = match o.resident {
        Some(r) => r.scale,
        None => {
            if o.asset == "fairy" {
                0.008
            } else {
                o.model_scale.unwrap_or(0.01)
            }
        }
    };
    let units = move |x: f64, y: f64, z: f64| -> [f64; 3] { [x * scale * WORLD_SCALE, y * scale * WORLD_SCALE, z * scale * WORLD_SCALE] };
    let mut model = SceneModel::new();
    let matrices: Vec<usize> = limbs.iter().enumerate().filter(|(j, l)| l.dl != 0 || (o.head.is_some() && *j == count - 1)).map(|(j, _)| j).collect();
    for j in 0..count {
        let l = &limbs[j];
        let mut children = Vec::new();
        let mut child = l.child;
        while child != 255 {
            children.push(Value::from(child as i64 + 1));
            child = limbs[child as usize].sibling;
        }
        let mut node = Map::new();
        node.insert("name".into(), format!("joint_{j}").into());
        node.insert("translation".into(), json!(units(l.x as f64, l.y as f64, l.z as f64)));
        if !children.is_empty() {
            node.insert("children".into(), Value::Array(children));
        }
        model.nodes.push(node);
        let mut bank = o.body.to_string();
        let head_seg;
        let seg_no = if o.body == "gameplay_keep" { 4 } else { 6 };
        let mut segments: Segs = HashMap::from([(seg_no, body)]);
        let mut skin_bones: Option<&[usize]> = None;
        if let Some((buf, bones)) = limb_data.get(&j) {
            segments.insert(8, buf.as_slice());
            skin_bones = Some(bones.as_slice());
        }
        let tail = |sym: &str, bank: &str, data: &'_ [u8]| -> R<usize> {
            let off = ctx.object_defs(bank)?.offset(sym)?;
            if off > data.len() {
                return Err("tail offset outside the object".into());
            }
            Ok(off)
        };
        if horse && o.body == "object_horse_link_child" && !limb_data.contains_key(&j) {
            segments.insert(8, &body[tail("gChildEponaEyeOpenTex", &bank, body)?..]);
        }
        if let Some(r) = o.resident {
            for (slot, symbol) in r.textures.iter().enumerate() {
                segments.insert(8 + slot as u32, &body[tail(symbol, &bank, body)?..]);
            }
        }
        if o.body == "object_md" {
            segments.insert(8, &body[tail("gMidoEyeOpenTex", &bank, body)?..]);
        }
        if o.body == "object_sa" {
            for (s, eye) in [(8, "gSariaEyeOpenTex"), (9, "gSariaEyeOpenTex"), (10, "gSariaMouthClosedTex")] {
                segments.insert(s, &body[tail(eye, &bank, body)?..]);
            }
        }
        let mut dl = l.dl;
        if let (Some(head), true) = (o.head, j == count - 1) {
            bank = head.to_string();
            head_seg = ctx.segment(head)?;
            let data: &[u8] = &head_seg;
            let (symbol, eye) = match head {
                "object_masterkokirihead" => ("gKokiriShopkeeperHeadDL", Some("gKokiriShopkeeperEyeDefaultTex")),
                "object_km1" => ("gKm1DL", None),
                "object_kw1" => ("object_kw1_DL_002C10", Some("gKw1EyeOpenTex")),
                "object_fa" => ("gFaDL", Some("gFaEyeOpenTex")),
                other => return Err(format!("KeyError: {other}")),
            };
            segments = HashMap::from([(6, data)]);
            if let Some(eye) = eye {
                segments.insert(10, &data[tail(eye, &bank, data)?..]);
            }
            dl = 0x0600_0000 + addr(&bank, Sym::Name(symbol))? as u32;
        }
        if o.body == "object_box" {
            dl = match j {
                0 => 0x0600_06F0,
                2 => 0x0600_10C0,
                _ => 0,
            };
        }
        let layer = u32::from(o.asset == "fairy");
        let mesh = |layer: u32, tex: bool| Mesh { name: &bank, segments: &segments, transform: &units, layer, joint: Some(j), matrices: &matrices, terrain_light: None, texture_defs: if tex { texture_defs.as_ref() } else { None }, skin_bones: if tex { skin_bones } else { None } };
        if o.body == "object_daiku" && j == 14 {
            let hair = ["object_daiku_DL_005BD0", "object_daiku_DL_005AC0", "object_daiku_DL_005990", "object_daiku_DL_005880"];
            let which = o.asset.chars().last().and_then(|c| c.to_digit(10)).ok_or("bad daiku asset")? as usize;
            let p = 0x0600_0000 + addr(&bank, Sym::Name(hair[which]))? as u32;
            mesh(0, false).into_model(&mut model, p)?;
        }
        if dl != 0 {
            mesh(layer, true).into_model(&mut model, dl)?;
        }
    }
    // skin
    let identity: Vec<f64> = vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let inverses = model.attribute_f32(&vec![identity; count], "MAT4", None);
    model.extra.push(("skins".into(), json!([{"joints": (1..=count).collect::<Vec<_>>(), "skeleton": 1, "inverseBindMatrices": inverses}])));
    model.nodes[0].insert("skin".into(), 0.into());
    model.scene_nodes.push(1);
    // animations
    let abank = o.animation_bank.unwrap_or(o.body);
    let data_seg = ctx.segment(abank)?;
    let data: &[u8] = &data_seg;
    let mut animations: Vec<Value> = Vec::new();
    let mut add_clip = |model: &mut SceneModel, label: &str, animation: Sym, pose: Option<i64>| -> R<()> {
        let at = addr(abank, animation)?;
        let (frames, values, indices, stat) = (u16_(data, at) as usize, (u32_(data, at + 4) & MASK) as usize, (u32_(data, at + 8) & MASK) as usize, u16_(data, at + 12) as usize);
        let times: Vec<Vec<f64>> = (0..=frames).map(|i| vec![i as f64 / 30.0]).collect();
        let times = model.attribute_f32(&times, "SCALAR", None);
        let (mut samplers, mut channels) = (Vec::new(), Vec::new());
        for j in 0..=count {
            let offsets = [u16_(data, indices + 6 * j) as usize, u16_(data, indices + 6 * j + 2) as usize, u16_(data, indices + 6 * j + 4) as usize];
            let mut samples: Vec<Vec<f64>> = Vec::new();
            for frame in 0..=frames {
                let sample = if label == "Open" { frame.min(frames - 1) } else { frame % frames };
                let xyz: Vec<i64> = offsets.iter().map(|&o| i16_(data, values + 2 * (o + if o >= stat { pose.map_or(sample, |p| p as usize) } else { 0 })) as i64).collect();
                if j == 0 {
                    samples.push(units(xyz[0] as f64, xyz[1] as f64, xyz[2] as f64).to_vec());
                } else {
                    let ang: Vec<f64> = xyz.iter().map(|&v| v as f64 * std::f64::consts::PI / 65536.0).collect();
                    let (a, b, c) = (libm::cos_(ang[0]), libm::cos_(ang[1]), libm::cos_(ang[2]));
                    let (x, y, z) = (libm::sin_(ang[0]), libm::sin_(ang[1]), libm::sin_(ang[2]));
                    samples.push(vec![x * b * c - a * y * z, a * y * c + x * b * z, a * b * z - x * y * c, a * b * c + x * y * z]);
                }
            }
            let path = if j != 0 { "rotation" } else { "translation" };
            if label == "Idle" {
                model.nodes[j.max(1)].insert(path.into(), json!(samples[0]));
            }
            let output = model.attribute_f32(&samples, if j != 0 { "VEC4" } else { "VEC3" }, None);
            samplers.push(json!({"input": times, "output": output}));
            channels.push(json!({"sampler": j, "target": {"node": j.max(1), "path": path}}));
        }
        animations.push(json!({"name": label, "samplers": samplers, "channels": channels}));
        Ok(())
    };
    // `model.doc['animations'] = []` happens before the first clip
    add_clip(&mut model, "Idle", o.animation, o.pose)?;
    let extras: Vec<(String, String, Option<i64>)> = match &o.extra_clips {
        None => default_clips(o.asset).into_iter().map(|(l, s, f)| (l.to_string(), s.to_string(), f)).collect(),
        Some(v) => v.iter().map(|(l, s)| (l.to_string(), s.to_string(), None)).collect(),
    };
    for (label, symbol, frame) in &extras {
        add_clip(&mut model, label, Sym::Name(symbol), *frame)?;
    }
    model.extra.push(("animations".into(), Value::Array(animations)));
    let folder = o.output_folder.unwrap_or(if o.resident.is_some() { "oot_residents" } else { "kokiri_forest" });
    model.write(&out.join("levels").join(folder).join(format!("{}.glb", o.asset)))
}

/// Python's `repr(float)` of a `Vec3` the way tests print it (kept for debugging).
#[allow(dead_code)]
pub fn show(v: &[f64]) -> String {
    v.iter().map(|x| py_float(*x)).collect::<Vec<_>>().join(",")
}
