//! Port of `scripts/export-ssb64-stages.py`: the Super Smash Bros. (US) stages from the user's ROM as walkable courses.
//! A stage is a `GR<Name>Map` relocData file holding an `MPGroundData` (`src/mp/mptypes.h`): four geometry layers
//! (DObjDesc trees with AObjEvent32 animations and MObjSub materials, drawn through the fighter exporter's F3DEX2
//! interpreter), the 2D map collision (`MPGeometryData`: yakumono groups of floor / ceiling / wall polylines), the camera
//! and blast-zone bounds, the BGM id and the map objects (player starts, items, respawn point). The collision lines are
//! extruded over a depth band closed by two invisible walls (see native/SSB64.md "Stages").
//! Writes `<out>/courses/ssb64_<id>.json`, `<out>/levels/ssb64_<id>/stage.glb` and `<out>/courses/ssb64_stages.json`,
//! byte-identical to the Python exporter.
use crate::oot::round_to;
use crate::pyc::{dumps, R};
use crate::ssb64::{costume, f32r, mat_mul, mat_trs, read_mobjs, Files, Gltf, Mesh, Ref, Rows, G_CULL_BACK, M4};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

pub const K: f64 = 0.348;
const GLB: f64 = K * 0.01;
pub const DEPTH: f64 = 360.0;
const DOBJ_END: i32 = 18;
const SURF_DEFAULT: i64 = 0;
const SURF_NO_CAM: i64 = 0x76;
const LINE_KINDS: [&str; 4] = ["floor", "ceil", "rwall", "lwall"];
const ANIM_FRAMES: usize = 7200;
const FPS: f64 = 60.0;

const BONUS_FIGHTERS: [(&str, &str, &str); 12] = [
    ("mario", "Mario", "Mario"),
    ("fox", "Fox", "Fox"),
    ("donkey", "Donkey Kong", "Donkey"),
    ("samus", "Samus", "Samus"),
    ("luigi", "Luigi", "Luigi"),
    ("link", "Link", "Link"),
    ("yoshi", "Yoshi", "Yoshi"),
    ("captain", "Captain Falcon", "Captain"),
    ("kirby", "Kirby", "Kirby"),
    ("pikachu", "Pikachu", "Pikachu"),
    ("purin", "Jigglypuff", "Purin"),
    ("ness", "Ness", "Ness"),
];

/// (id, map file, label, kind) in dMPCollisionGroundFileInfos order (src/gr/grdef.h GRKind).
pub fn stages() -> Vec<(String, String, String, &'static str)> {
    let mut out: Vec<(String, String, String, &'static str)> = [
        ("castle", "GRCastleMap", "Peach's Castle", "battle"),
        ("sector", "GRSectorMap", "Sector Z", "battle"),
        ("jungle", "GRJungleMap", "Congo Jungle", "battle"),
        ("zebes", "GRZebesMap", "Planet Zebes", "battle"),
        ("hyrule", "GRHyruleMap", "Hyrule Castle", "battle"),
        ("yoster", "GRYosterMap", "Yoshi's Island", "battle"),
        ("pupupu", "GRPupupuMap", "Dream Land", "battle"),
        ("yamabuki", "GRYamabukiMap", "Saffron City", "battle"),
        ("inishie", "GRInishieMap", "Mushroom Kingdom", "battle"),
        ("pupupu_small", "GRPupupuSmallMap", "Dream Land (beta)", "extra"),
        ("pupupu_test", "GRPupupuTestMap", "Test Stage", "extra"),
        ("explain", "GRExplainMap", "How to Play", "extra"),
        ("yoster_small", "GRYosterSmallMap", "Mini Yoshi's Island", "extra"),
        ("metal", "GRMetalMap", "Meta Crystal", "extra"),
        ("zako", "GRZakoMap", "Duel Zone", "extra"),
        ("bonus3", "GRBonus3Map", "Race to the Finish", "extra"),
        ("last", "GRLastMap", "Final Destination", "extra"),
    ]
    .iter()
    .map(|(a, b, c, d)| (a.to_string(), b.to_string(), c.to_string(), *d))
    .collect();
    for (f, n, stem) in BONUS_FIGHTERS {
        out.push((format!("bonus1_{f}"), format!("GRBonus1{stem}Map"), format!("Break the Targets: {n}"), "bonus1"));
    }
    for (f, n, stem) in BONUS_FIGHTERS {
        out.push((format!("bonus2_{f}"), format!("GRBonus2{stem}Map"), format!("Board the Platforms: {n}"), "bonus2"));
    }
    out
}

fn be16(d: &[u8], o: usize) -> R<u16> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| format!("read past the end at {o:#x}"))
}
fn be32(d: &[u8], o: usize) -> R<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap())).ok_or_else(|| format!("read past the end at {o:#x}"))
}
fn bef32(d: &[u8], o: usize) -> R<f64> {
    be32(d, o).map(|v| f32::from_bits(v) as f64)
}
fn bes16(d: &[u8], o: usize) -> R<i64> {
    be16(d, o).map(|v| v as i16 as i64)
}

/// Python's `rnd`: round to 3 decimals, never -0.0.
fn rnd(x: f64) -> f64 {
    let v = round_to(x, 3);
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

// ---------------------------------------------------------------------------------------------- map header

#[derive(Clone, Copy, Default)]
struct Layer {
    dobjdesc: Option<Ref>,
    anim: Option<Ref>,
    mobjs: Option<Ref>,
    matanim: Option<Ref>,
}

#[derive(Clone, Copy)]
struct Bounds {
    top: i64,
    bottom: i64,
    right: i64,
    left: i64,
}

struct Header {
    layers: [Layer; 4],
    geometry: Option<Ref>,
    layer_mask: u8,
    fog: [u8; 3],
    camera: Bounds,
    blast: Bounds,
    bgm: u32,
    alt_warning: i64,
}

fn header_offset(files: &Files, fid: usize) -> R<usize> {
    let f = files.get(fid)?;
    if f.data.len() >= 0xA8 {
        let mut o = 0;
        while o <= f.data.len() - 0xA8 {
            if f.ptrs.contains_key(&(o + 0x40)) && f.ptrs.contains_key(&(o + 0x84)) {
                return Ok(o);
            }
            o += 4;
        }
    }
    if f.ptrs.contains_key(&0x40) {
        return Ok(0);
    }
    Err(format!("file {fid}: no MPGroundData"))
}

fn read_header(files: &Files, fid: usize) -> R<Header> {
    let o = header_offset(files, fid)?;
    let base = Some((fid, o));
    let f = files.get(fid)?;
    let d = &f.data;
    let mut layers = [Layer::default(); 4];
    for (i, l) in layers.iter_mut().enumerate() {
        *l = Layer { dobjdesc: files.ptr(base, 16 * i)?, anim: files.ptr(base, 16 * i + 4)?, mobjs: files.ptr(base, 16 * i + 8)?, matanim: files.ptr(base, 16 * i + 12)? };
    }
    let h = |off: usize| bes16(d, o + off);
    let b = |at: usize| -> R<Bounds> { Ok(Bounds { top: h(at)?, bottom: h(at + 2)?, right: h(at + 4)?, left: h(at + 6)? }) };
    Ok(Header {
        layers,
        geometry: files.ptr(base, 0x40)?,
        layer_mask: d[o + 0x44],
        fog: [d[o + 0x4C], d[o + 0x4D], d[o + 0x4E]],
        camera: b(0x6C)?,
        blast: b(0x74)?,
        bgm: be32(d, o + 0x7C)?,
        alt_warning: h(0x88)?,
    })
}

// ---------------------------------------------------------------------------------------------- AObjEvent32

type Pose = ([f64; 3], [f64; 3], [f64; 3]);

#[derive(Clone, Copy)]
struct AObj {
    kind: u8,
    vb: f64,
    vt: f64,
    rb: f64,
    rt: f64,
    length: f64,
    inv: f64,
}

const AOBJ0: AObj = AObj { kind: 0, vb: 0.0, vt: 0.0, rb: 0.0, rt: 0.0, length: 0.0, inv: 1.0 };

/// gcParseDObjAnimJoint + gcPlayDObjAnimJoint on one DObj's AObjEvent32 script, frame by frame.
fn simulate32(files: &Files, script: Ref, pose: Pose, frames: usize) -> R<Vec<Pose>> {
    let (mut t, mut r, mut s) = pose;
    let mut aobjs: [Option<AObj>; 10] = [None; 10];
    let mut wait = 0.0f64;
    let mut out = vec![];
    let file = files.get(script.0)?;
    let d = &file.data;
    let mut at = script.1;
    let mut ended = false;
    for frame in 0..frames {
        if frame > 0 {
            if ended {
                break;
            }
            wait -= 1.0;
        }
        while wait <= 0.0 && !ended {
            let w = be32(d, at)?;
            let (op, mut flags, payload) = (w >> 25, (w >> 15) & 0x3FF, (w & 0x7FFF) as f64);
            let nxt = at + 4;
            match op {
                3..=11 => {
                    at = nxt;
                    for i in 0..10 {
                        if flags == 0 {
                            break;
                        }
                        if flags & 1 != 0 {
                            let a = aobjs[i].get_or_insert(AOBJ0);
                            let v = bef32(d, at)?;
                            at += 4;
                            if op == 7 {
                                a.rt = v;
                            } else {
                                a.vb = a.vt;
                                a.vt = v;
                                if op == 5 || op == 6 {
                                    a.rb = a.rt;
                                    a.rt = bef32(d, at)?;
                                    at += 4;
                                    a.kind = 3;
                                    if payload != 0.0 {
                                        a.inv = 1.0 / payload;
                                    }
                                } else if op == 8 || op == 9 {
                                    a.rb = a.rt;
                                    a.rt = 0.0;
                                    a.kind = 3;
                                    if payload != 0.0 {
                                        a.inv = 1.0 / payload;
                                    }
                                } else if op == 3 || op == 4 {
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
                        }
                        flags >>= 1;
                    }
                    if matches!(op, 3 | 5 | 8 | 10) {
                        wait += payload;
                    }
                }
                2 => {
                    wait += payload;
                    at = nxt;
                }
                15 | 16 => {
                    wait += payload;
                    at = nxt;
                }
                17 => {
                    wait += payload;
                    at += 4 + 4 * flags.count_ones() as usize;
                }
                13 => at += 8,
                12 => {
                    at = nxt;
                    for a in aobjs.iter_mut() {
                        if flags == 0 {
                            break;
                        }
                        if flags & 1 != 0 {
                            if let Some(a) = a {
                                a.length += payload;
                            }
                        }
                        flags >>= 1;
                    }
                }
                _ => {
                    // 0 End, 1 Jump / 14 SetAnim (a loop back ends the period), anything unknown
                    ended = true;
                    for a in aobjs.iter_mut().flatten() {
                        if a.kind != 0 {
                            a.length += 1.0 + wait;
                        }
                    }
                    break;
                }
            }
        }
        for (i, slot) in aobjs.iter_mut().enumerate() {
            let Some(a) = slot else { continue };
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
            if i < 3 {
                r[i] = v;
            } else if (4..=6).contains(&i) {
                t[i - 4] = v;
            } else if i >= 7 {
                s[i - 7] = v;
            }
        }
        out.push((t, r, s));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------- geometry

struct Joint {
    parent: Option<usize>,
    dl: Option<Ref>,
    t: [f64; 3],
    r: [f64; 3],
    s: [f64; 3],
    anim: Option<Ref>,
    frames: Option<Vec<Pose>>,
    moving: bool,
    local: M4,
    world: M4,
}

/// gcSetupCustomDObjs: the layer's DObjs posed at frame 0; an animated one keeps one period of its animation.
fn layer_joints(files: &Files, layer: &Layer) -> R<Vec<Joint>> {
    let Some(desc) = layer.dobjdesc else { return Ok(vec![]) };
    let f = files.get(desc.0)?;
    let dd = &f.data;
    let mut joints: Vec<Joint> = vec![];
    let mut stack: HashMap<i32, usize> = HashMap::new();
    let mut i = 0;
    loop {
        let o = desc.1 + 44 * i;
        let jid = be32(dd, o)? as i32;
        if jid == DOBJ_END {
            break;
        }
        let depth = jid & 0xFFF;
        let v: Vec<f64> = (0..9).map(|k| bef32(dd, o + 8 + 4 * k)).collect::<R<_>>()?;
        let (mut t, mut r, mut s) = ([v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]);
        let parent = if depth != 0 { stack.get(&(depth - 1)).copied() } else { None };
        let anim = if layer.anim.is_some() { files.ptr(layer.anim, 4 * i)? } else { None };
        let (mut frames, mut moving) = (None, false);
        if let Some(a) = anim {
            let fr = simulate32(files, a, (t, r, s), ANIM_FRAMES)?;
            if let Some(p) = fr.first().copied() {
                (t, r, s) = p;
                moving = fr.iter().any(|f| *f != p);
                frames = Some(fr);
            }
        }
        let local = mat_trs(t, r, s);
        let world = match parent {
            Some(p) => mat_mul(&joints[p].world, &local),
            None => local,
        };
        stack.insert(depth, joints.len());
        joints.push(Joint { parent, dl: files.ptr(Some(desc), 44 * i + 4)?, t, r, s, anim, frames, moving, local, world });
        i += 1;
    }
    Ok(joints)
}

const IDENTITY: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

#[derive(Clone, Copy, PartialEq)]
enum PartKind {
    Solid,
    Anim,
}

struct Part {
    layer: usize,
    joint: usize,
    kind: PartKind,
    parent: Option<usize>,
}

/// Every layer's display lists into one Mesh (frame 0); vertices tagged with their part number (0 = static).
fn stage_mesh<'f, 'a>(files: &'f Files<'a>, header: &Header, layers: &[Vec<Joint>], solid_ids: &[u16]) -> R<(Mesh<'f, 'a>, Vec<Part>)> {
    let mut m = Mesh::new(files);
    m.geom = 0x1 | 0x4 | G_CULL_BACK | 0x0020_0000;
    m.light1 = [1.0, 1.0, 1.0];
    let mut parts: Vec<Part> = vec![];
    for (li, layer) in header.layers.iter().enumerate() {
        let joints = &layers[li];
        let links = (header.layer_mask >> li) & 1 != 0;
        let mut roots: Vec<Option<usize>> = vec![None; joints.len()];
        let mut part_of: HashMap<usize, usize> = HashMap::new();
        let mut rel: HashMap<usize, M4> = HashMap::new();
        for (ji, j) in joints.iter().enumerate() {
            let p = j.parent;
            if let Some(pr) = p.and_then(|p| roots[p]) {
                roots[ji] = Some(pr);
            } else if j.moving {
                roots[ji] = Some(ji);
                let solid = li == 1 && solid_ids.contains(&(ji as u16));
                parts.push(Part { layer: li, joint: ji, kind: if solid { PartKind::Solid } else { PartKind::Anim }, parent: p });
                part_of.insert(ji, parts.len());
            }
            let (part, world) = match roots[ji] {
                None => (0, j.world),
                Some(root) => {
                    let part = part_of[&root];
                    let world = if parts[part - 1].kind == PartKind::Solid {
                        j.world
                    } else if root == ji {
                        IDENTITY
                    } else {
                        mat_mul(&rel[&p.unwrap()], &j.local)
                    };
                    rel.insert(ji, world);
                    (part, world)
                }
            };
            let mlist = if layer.mobjs.is_some() { files.ptr(layer.mobjs, 4 * ji)? } else { None };
            let clist = if layer.matanim.is_some() { files.ptr(layer.matanim, 4 * ji)? } else { None };
            m.mobjs = read_mobjs(files, mlist)?;
            for k in 0..m.mobjs.len() {
                let script = if clist.is_some() { files.ptr(clist, 4 * k)? } else { None };
                costume(files, &mut m.mobjs[k], script)?;
            }
            let Some(dl) = j.dl else { continue };
            if links {
                let f = files.get(dl.0)?;
                for k in 0..8 {
                    let lid = be32(&f.data, dl.1 + 8 * k)? as i32;
                    if lid == 4 {
                        break;
                    }
                    if let Some(sub) = files.ptr(Some(dl), 8 * k + 4)? {
                        m.run(sub, part, &world)?;
                    }
                }
            } else {
                m.run(dl, part, &world)?;
            }
        }
    }
    Ok((m, parts))
}

#[derive(Default)]
struct Cache {
    materials: HashMap<usize, usize>,
    images: Vec<((Vec<u8>, (i64, i64)), usize)>,
}

/// One part's triangles (by their first vertex) as a glTF mesh; positions (p - shift) * scale.
fn add_mesh(g: &mut Gltf, mesh: &Mesh, part: usize, name: &str, shift: [f64; 3], scale: f64, cache: &mut Cache) -> R<Option<usize>> {
    let mut prims = vec![];
    for (mi, all_idx) in mesh.tris.iter().enumerate() {
        let idx: Vec<usize> = all_idx.chunks(3).filter(|t| mesh.verts[t[0]].joint == part).flatten().copied().collect();
        if idx.is_empty() {
            continue;
        }
        let mat = &mesh.materials[mi];
        let mut vids: Vec<usize> = idx.clone();
        vids.sort_unstable();
        vids.dedup();
        let remap: HashMap<usize, usize> = vids.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        let (mut pos, mut nrm, mut uv, mut col) = (vec![], vec![], vec![], vec![]);
        for &v in &vids {
            let vert = &mesh.verts[v];
            pos.push((0..3).map(|k| f32r((vert.pos[k] - shift[k]) * scale)).collect::<Vec<_>>());
            let n = vert.nrm.unwrap_or([0.0, 1.0, 0.0]);
            let ln = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let ln = if ln == 0.0 { 1.0 } else { ln };
            nrm.push(n.iter().map(|x| f32r(x / ln)).collect::<Vec<_>>());
            uv.push(vert.uv.iter().map(|&x| f32r(x)).collect::<Vec<_>>());
            col.push(vert.col.unwrap_or([1.0; 4]).iter().map(|&x| f32r(x)).collect::<Vec<_>>());
        }
        let mut attrs = Map::new();
        attrs.insert("POSITION".into(), json!(g.accessor(Rows::F(&pos), "VEC3", Some(34962), true)));
        attrs.insert("NORMAL".into(), json!(g.accessor(Rows::F(&nrm), "VEC3", Some(34962), false)));
        if mat.image.is_some() {
            attrs.insert("TEXCOORD_0".into(), json!(g.accessor(Rows::F(&uv), "VEC2", Some(34962), false)));
        }
        if mat.vcol {
            attrs.insert("COLOR_0".into(), json!(g.accessor(Rows::F(&col), "VEC4", Some(34962), false)));
        }
        let ind: Vec<usize> = idx.iter().map(|i| remap[i]).collect();
        let indices = g.accessor(Rows::Idx(&ind, vids.len() >= 65536), "SCALAR", Some(34963), false);
        if !cache.materials.contains_key(&mi) {
            let mut pbr = Map::new();
            pbr.insert("baseColorFactor".into(), json!(mat.color.map(f32r)));
            pbr.insert("metallicFactor".into(), json!(0.0));
            pbr.insert("roughnessFactor".into(), json!(1.0));
            let mut extra = Map::new();
            if mat.double {
                extra.insert("doubleSided".into(), json!(true));
            }
            if mat.alpha != "OPAQUE" {
                extra.insert("alphaMode".into(), json!(mat.alpha));
                if mat.alpha == "MASK" {
                    extra.insert("alphaCutoff".into(), json!(0.5));
                }
            }
            if let Some((w, h, px)) = &mat.image {
                let data = crate::zpng::png_rgba(*w, *h, px, 9)?;
                let ik = (data.clone(), mat.wrap);
                let tex = match cache.images.iter().find(|(k, _)| *k == ik) {
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
                        cache.images.push((ik, t));
                        t
                    }
                };
                pbr.insert("baseColorTexture".into(), json!({"index": tex}));
            }
            let mut ordered = Map::new();
            ordered.insert("name".into(), json!(format!("mat{mi:03}")));
            ordered.insert("pbrMetallicRoughness".into(), Value::Object(pbr));
            ordered.extend(extra);
            g.arr("materials").push(Value::Object(ordered));
            let n = g.arr("materials").len() - 1;
            cache.materials.insert(mi, n);
        }
        prims.push(json!({"attributes": attrs, "indices": indices, "material": cache.materials[&mi]}));
    }
    if prims.is_empty() {
        return Ok(None);
    }
    g.arr("meshes").push(json!({"name": name, "primitives": prims}));
    Ok(Some(g.arr("meshes").len() - 1))
}

fn new_gltf() -> Gltf {
    let mut g = Gltf::new();
    g.j.insert("asset".into(), json!({"version": "2.0", "generator": "jumpman export-ssb64-stages.py"}));
    g
}

fn write_file(path: &Path, bytes: Vec<u8>) -> R<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn named(name: &str, mesh: Option<usize>) -> Value {
    match mesh {
        Some(m) => json!({"name": name, "mesh": m}),
        None => json!({"name": name}),
    }
}

/// stage.glb: node 0 the static stage, node 1 `parts` holding each animated part (parent matrix over the posed and
/// animated root), one looping clip per part.
fn write_stage_glb(mesh: &Mesh, parts: &[Part], layers: &[Vec<Joint>], path: &Path) -> R<()> {
    let mut g = new_gltf();
    let mut cache = Cache::default();
    let m0 = add_mesh(&mut g, mesh, 0, "stage", [0.0; 3], GLB, &mut cache)?;
    g.arr("nodes").push(named("stage", m0));
    g.arr("nodes").push(json!({"name": "parts", "children": []}));
    g.j.insert("scenes".into(), json!([{"nodes": [0, 1]}]));
    let mut times: HashMap<(bool, usize), usize> = HashMap::new();
    for (pi, part) in parts.iter().enumerate() {
        if part.kind != PartKind::Anim {
            continue;
        }
        let joints = &layers[part.layer];
        let root = &joints[part.joint];
        let Some(mi) = add_mesh(&mut g, mesh, pi + 1, &format!("part{}", pi + 1), [0.0; 3], 1.0, &mut cache)? else { continue };
        let pw = part.parent.map_or(IDENTITY, |p| joints[p].world);
        let mat: M4 = std::array::from_fn(|r| if r < 3 { std::array::from_fn(|c| pw[r][c] * GLB) } else { pw[r] });
        let n_nodes = g.arr("nodes").len();
        let matrix: Vec<f64> = (0..16).map(|k| f32r(mat[k % 4][k / 4])).collect();
        let holder = json!({"name": format!("part{}_parent", pi + 1), "matrix": matrix, "children": [n_nodes + 1]});
        let node = json!({"name": format!("part{}", pi + 1), "mesh": mi, "translation": root.t.map(f32r), "rotation": crate::ssb64::quat(root.r).map(f32r), "scale": root.s.map(f32r)});
        g.arr("nodes")[1]["children"].as_array_mut().unwrap().push(json!(n_nodes));
        g.arr("nodes").push(holder);
        g.arr("nodes").push(node);
        let target = n_nodes + 1;
        let fr = root.frames.as_ref().ok_or("an animated part without frames")?;
        let n = fr.len();
        let (mut channels, mut samplers) = (vec![], vec![]);
        let tr: Vec<Vec<f64>> = fr.iter().map(|f| f.0.iter().map(|&x| f32r(x)).collect()).collect();
        let ro: Vec<Vec<f64>> = fr.iter().map(|f| crate::ssb64::quat(f.1).iter().map(|&x| f32r(x)).collect()).collect();
        let sc: Vec<Vec<f64>> = fr.iter().map(|f| f.2.iter().map(|&x| f32r(x)).collect()).collect();
        for (path_name, kind, rows) in [("translation", "VEC3", tr), ("rotation", "VEC4", ro), ("scale", "VEC3", sc)] {
            let held = rows.iter().all(|r| *r == rows[0]);
            let rows = if held { vec![rows[0].clone(), rows[0].clone()] } else { rows };
            let input = match times.get(&(held, n)) {
                Some(&a) => a,
                None => {
                    let t: Vec<Vec<f64>> = if held { vec![vec![0.0], vec![f32r((n - 1) as f64 / FPS)]] } else { (0..n).map(|i| vec![f32r(i as f64 / FPS)]).collect() };
                    let a = g.accessor(Rows::F(&t), "SCALAR", None, true);
                    times.insert((held, n), a);
                    a
                }
            };
            let output = g.accessor(Rows::F(&rows), kind, None, false);
            samplers.push(json!({"input": input, "output": output, "interpolation": "LINEAR"}));
            channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": target, "path": path_name}}));
        }
        g.arr("animations").push(json!({"name": format!("part{}", pi + 1), "channels": channels, "samplers": samplers}));
    }
    write_file(path, g.glb())
}

/// A moving yakumono's model relative to its DObj's frame-0 translation.
fn write_part_glb(mesh: &Mesh, part_no: usize, shift: [f64; 3], path: &Path) -> R<()> {
    let mut g = new_gltf();
    let mut cache = Cache::default();
    let name = format!("part{part_no}");
    let mi = add_mesh(&mut g, mesh, part_no, &name, shift, GLB, &mut cache)?;
    g.arr("nodes").push(named(&name, mi));
    g.j.insert("scenes".into(), json!([{"nodes": [0]}]));
    write_file(path, g.glb())
}

// ---------------------------------------------------------------------------------------------- collision

/// (x, y, flags) of one collision vertex.
type Pt = (i64, i64, u16);

#[derive(Clone)]
struct Group {
    yakumono: u16,
    lines: [Vec<Vec<Pt>>; 4],
    offset: [f64; 2],
    animated: bool,
    moving: bool,
}

fn read_geometry(files: &Files, header: &Header, layer1: &[Joint]) -> R<(Vec<Group>, Vec<(u16, i64, i64)>)> {
    let geo = header.geometry.ok_or("no map geometry")?;
    let gf = files.get(geo.0)?;
    let count = be16(&gf.data, geo.1)? as usize;
    let p = |off: usize| -> R<Ref> { files.ptr(Some(geo), off)?.ok_or_else(|| format!("map geometry: no pointer at +{off:#x}")) };
    let (vdata, vids, vlinks, linfo) = (p(4)?, p(8)?, p(12)?, p(16)?);
    let mcount = be16(&gf.data, geo.1 + 20)? as usize;
    let mobjs = files.ptr(Some(geo), 24)?;
    let (vd, idd, ld, nd) = (files.get(vdata.0)?, files.get(vids.0)?, files.get(vlinks.0)?, files.get(linfo.0)?);
    let vertex = |k: usize| -> R<Pt> {
        let vid = be16(&idd.data, vids.1 + 2 * k)? as usize;
        let o = vdata.1 + 6 * vid;
        Ok((bes16(&vd.data, o)?, bes16(&vd.data, o + 2)?, be16(&vd.data, o + 4)?))
    };
    let mut groups = vec![];
    for g in 0..count {
        let o = linfo.1 + 18 * g;
        let yid = be16(&nd.data, o)?;
        let mut sets: [Vec<Vec<Pt>>; 4] = Default::default();
        for (k, set) in sets.iter_mut().enumerate() {
            let (first, n) = (be16(&nd.data, o + 2 + 4 * k)? as usize, be16(&nd.data, o + 4 + 4 * k)? as usize);
            for line in first..first + n {
                let (v1, vn) = (be16(&ld.data, vlinks.1 + 4 * line)? as usize, be16(&ld.data, vlinks.1 + 4 * line + 2)? as usize);
                set.push((v1..v1 + vn).map(vertex).collect::<R<_>>()?);
            }
        }
        let (mut offset, mut animated, mut moving) = ([0.0, 0.0], false, false);
        if let Some(j) = layer1.get(yid as usize) {
            animated = j.anim.is_some();
            moving = j.moving;
            if animated {
                offset = [j.t[0], j.t[1]];
            }
        }
        groups.push(Group { yakumono: yid, lines: sets, offset, animated, moving });
    }
    let mut mapobjs = vec![];
    if let Some(m) = mobjs {
        let mf = files.get(m.0)?;
        for i in 0..mcount {
            let o = m.1 + 6 * i;
            mapobjs.push((be16(&mf.data, o)?, bes16(&mf.data, o + 2)?, bes16(&mf.data, o + 4)?));
        }
    }
    Ok((groups, mapobjs))
}

struct Collision {
    verts: Vec<[f64; 3]>,
    tris: Vec<[i64; 5]>,
    passing: Vec<usize>,
    ledges: Vec<usize>,
    counts: [usize; 4],
}

fn collision(groups: &[&Group], blast: Bounds, depth: f64, walls: bool) -> Collision {
    let mut c = Collision { verts: vec![], tris: vec![], passing: vec![], ledges: vec![], counts: [0; 4] };
    let mut vindex: HashMap<[u64; 3], usize> = HashMap::new();
    let mut vtx = |c: &mut Collision, p: [f64; 3]| -> usize {
        let key = p.map(|x| if x == 0.0 { 0u64 } else { x.to_bits() });
        *vindex.entry(key).or_insert_with(|| {
            c.verts.push(p);
            c.verts.len() - 1
        })
    };
    // (normal sign y, normal sign x) per line kind
    let side = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];
    for g in groups {
        let [ox, oy] = g.offset;
        for (k, lines) in g.lines.iter().enumerate() {
            for line in lines {
                c.counts[k] += 1;
                for i in 0..line.len().saturating_sub(1) {
                    let ((x1, y1, f1), (x2, y2, _)) = (line[i], line[i + 1]);
                    let a = (rnd((x1 as f64 + ox) * K), rnd((y1 as f64 + oy) * K));
                    let b = (rnd((x2 as f64 + ox) * K), rnd((y2 as f64 + oy) * K));
                    if a == b {
                        continue;
                    }
                    let (nx, ny) = (b.1 - a.1, -(b.0 - a.0));
                    let want = nx * side[k].1 + ny * side[k].0;
                    let mut p = [vtx(&mut c, [a.0, a.1, -depth]), vtx(&mut c, [b.0, b.1, -depth]), vtx(&mut c, [b.0, b.1, depth]), vtx(&mut c, [a.0, a.1, depth])];
                    if want < 0.0 {
                        p = [p[1], p[0], p[3], p[2]];
                    }
                    let start = c.tris.len();
                    c.tris.push([p[0] as i64, p[1] as i64, p[2] as i64, SURF_DEFAULT, 0]);
                    c.tris.push([p[0] as i64, p[2] as i64, p[3] as i64, SURF_DEFAULT, 0]);
                    if f1 & 0x4000 != 0 {
                        c.passing.extend([start, start + 1]);
                    }
                    if f1 & 0x8000 != 0 {
                        c.ledges.extend([start, start + 1]);
                    }
                }
            }
        }
    }
    if walls {
        let (l, r, bo, t) = (rnd(blast.left as f64 * K), rnd(blast.right as f64 * K), rnd(blast.bottom as f64 * K), rnd(blast.top as f64 * K));
        for (z, sign) in [(-depth, 1), (depth, -1)] {
            let mut q = [vtx(&mut c, [l, bo, z]), vtx(&mut c, [r, bo, z]), vtx(&mut c, [r, t, z]), vtx(&mut c, [l, t, z])];
            if sign < 0 {
                q = [q[1], q[0], q[3], q[2]];
            }
            c.tris.push([q[0] as i64, q[1] as i64, q[2] as i64, SURF_NO_CAM, 0]);
            c.tris.push([q[0] as i64, q[2] as i64, q[3] as i64, SURF_NO_CAM, 0]);
        }
    }
    c
}

fn verts_json(v: &[[f64; 3]]) -> Vec<Value> {
    v.iter().map(|p| json!([p[0], p[1], p[2]])).collect()
}

// ---------------------------------------------------------------------------------------------- export

const MAPOBJ_PLAYERS: [u16; 4] = [0, 1, 2, 3];
const MAPOBJ_ITEM: u16 = 4;
const MAPOBJ_REBIRTH: u16 = 0x20;

/// Highest floor line point at x (SSB64 units) at or below y + 1.
fn floor_below(groups: &[Group], x: f64, y: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for g in groups {
        let [ox, oy] = g.offset;
        for line in &g.lines[0] {
            for i in 0..line.len().saturating_sub(1) {
                let ((x1, y1, _), (x2, y2, _)) = (line[i], line[i + 1]);
                let (x1, x2, y1, y2) = (x1 as f64 + ox, x2 as f64 + ox, y1 as f64 + oy, y2 as f64 + oy);
                let (lo, hi) = (x1.min(x2), x1.max(x2));
                if lo <= x && x <= hi && hi > lo {
                    let fy = y1 + (y2 - y1) * (x - x1) / (x2 - x1);
                    if fy <= y + 1.0 && best.is_none_or(|b| fy > b) {
                        best = Some(fy);
                    }
                }
            }
        }
    }
    best
}

fn bounds_json(b: Bounds) -> Value {
    json!({"top": rnd(b.top as f64 * K), "bottom": rnd(b.bottom as f64 * K), "left": rnd(b.left as f64 * K), "right": rnd(b.right as f64 * K)})
}

fn export_stage(files: &Files, names: &HashMap<String, usize>, stage: &(String, String, String, &'static str), out: &Path) -> R<Value> {
    let (sid, map, label, kind) = stage;
    let fid = *names.get(map).ok_or_else(|| format!("{map}: not in relocFileDescriptions"))?;
    let header = read_header(files, fid)?;
    let layers: Vec<Vec<Joint>> = header.layers.iter().map(|l| layer_joints(files, l)).collect::<R<_>>()?;
    let (groups, mapobjs) = read_geometry(files, &header, &layers[1])?;
    let moving: Vec<&Group> = groups.iter().filter(|g| g.moving).collect();
    let solid_ids: Vec<u16> = moving.iter().map(|g| g.yakumono).collect();
    let (mesh, parts) = stage_mesh(files, &header, &layers, &solid_ids)?;
    let course = format!("ssb64_{sid}");
    write_stage_glb(&mesh, &parts, &layers, &out.join("levels").join(&course).join("stage.glb"))?;
    let blast = header.blast;
    let col = collision(&groups.iter().filter(|g| !g.moving).collect::<Vec<_>>(), blast, DEPTH, true);
    let mut extra = Map::new();
    let mut solids = vec![];
    for g in &moving {
        let j = &layers[1][g.yakumono as usize];
        let part_no = parts.iter().position(|p| p.layer == 1 && p.joint == g.yakumono as usize).map(|i| i + 1);
        let key = format!("ssb64_yakumono_{}", g.yakumono);
        let mut rel = (*g).clone();
        rel.offset = [0.0, 0.0];
        let gc = collision(&[&rel], blast, DEPTH, false);
        extra.insert(key.clone(), json!({"vertices": verts_json(&gc.verts), "triangles": gc.tris}));
        let mut model = String::new();
        if let Some(pn) = part_no {
            model = format!("levels/{course}/part_{}.glb", g.yakumono);
            write_part_glb(&mesh, pn, [j.t[0], j.t[1], 0.0], &out.join(&model))?;
        }
        let frames = j.frames.as_ref().ok_or("a moving yakumono without frames")?;
        let track: Vec<[f64; 4]> = frames.iter().step_by(2).enumerate().map(|(i, f)| [i as f64, rnd(f.0[0] * K), rnd(f.0[1] * K), 0.0]).collect();
        solids.push(json!({"behavior": "ssb64Yakumono", "model": model, "collision": key, "pos": [track[0][1], track[0][2], track[0][3]], "track": track}));
    }
    let pt = |x: i64, y: i64| json!([rnd(x as f64 * K), rnd(y as f64 * K), 0.0]);
    let players: Vec<Value> = mapobjs.iter().filter(|m| MAPOBJ_PLAYERS.contains(&m.0)).map(|m| pt(m.1, m.2)).collect();
    let items: Vec<Value> = mapobjs.iter().filter(|m| m.0 == MAPOBJ_ITEM).map(|m| pt(m.1, m.2)).collect();
    let rebirth: Vec<Value> = mapobjs.iter().filter(|m| m.0 == MAPOBJ_REBIRTH).map(|m| pt(m.1, m.2)).collect();
    let mut sorted = mapobjs.clone();
    sorted.sort_by_key(|m| m.0);
    let first: (f64, f64) = match sorted.iter().find(|m| MAPOBJ_PLAYERS.contains(&m.0)) {
        Some(m) => (m.1 as f64, m.2 as f64),
        None => {
            let ys: Vec<f64> = groups.iter().flat_map(|g| g.lines[0].iter().flat_map(move |l| l.iter().map(move |p| p.1 as f64 + g.offset[1]))).collect();
            (0.0, ys.iter().copied().fold(None, |a: Option<f64>, y| Some(a.map_or(y, |a| if y > a { y } else { a }))).unwrap_or(0.0))
        }
    };
    let fy = floor_below(&groups, first.0, first.1);
    let spawn = json!([rnd(first.0 * K), rnd(fy.unwrap_or(first.1) * K), 0.0]);
    let mut lines = Map::new();
    for (k, n) in LINE_KINDS.iter().zip(col.counts) {
        lines.insert(k.to_string(), json!(n));
    }
    let yak: Vec<Value> = groups.iter().map(|g| json!({"id": g.yakumono, "animated": g.animated, "moving": g.moving, "offset": [rnd(g.offset[0] * K), rnd(g.offset[1] * K)]})).collect();
    let part_list: Vec<Value> = parts
        .iter()
        .map(|p| json!({"layer": p.layer, "joint": p.joint, "kind": if p.kind == PartKind::Solid { "solid" } else { "anim" }, "frames": layers[p.layer][p.joint].frames.as_ref().map_or(0, |f| f.len())}))
        .collect();
    let doc = json!({
        "model": format!("levels/{course}/stage.glb"),
        "vertices": verts_json(&col.verts),
        "triangles": col.tris,
        "spawn": {"pos": spawn, "faceYaw": 32768},
        "horizontalBoundary": rnd((blast.left.abs().min(blast.right.abs())) as f64 * K),
        "deathHeight": rnd(blast.bottom as f64 * K),
        "explorationOnly": true,
        "extraCollision": extra,
        "solids": solids,
        "ssb64": {
            "stage": sid, "name": label, "kind": kind, "file": fid, "unit": K, "depth": DEPTH, "bgm": header.bgm,
            "camera": bounds_json(header.camera), "blast": bounds_json(blast), "warning": rnd(header.alt_warning as f64 * K),
            "fog": header.fog, "players": players, "items": items, "rebirth": rebirth,
            "passTriangles": col.passing, "ledgeTriangles": col.ledges, "lines": lines, "layers": layers.iter().map(|l| l.len()).collect::<Vec<_>>(),
            "parts": part_list,
            "yakumono": yak,
        },
    });
    let doc = crate::ssb64::sort_keys(doc);
    let dir = out.join("courses");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(dir.join(format!("{course}.json")), dumps(&doc, None) + "\n").map_err(|e| e.to_string())?;
    let mut unknown: Vec<String> = mesh.unknown.iter().cloned().collect();
    unknown.sort();
    Ok(json!({"id": course, "name": label, "kind": kind, "bgm": header.bgm, "triangles": mesh.tris.iter().map(|t| t.len()).sum::<usize>() / 3,
        "materials": mesh.materials.len(), "collision": col.tris.len(), "parts": parts.len(), "solids": solids.len(), "unknown": unknown}))
}

/// Every stage into `<out>/courses` and `<out>/levels` (`out` = the data dir's `assets`).
pub fn export(source: &Path, z64: &[u8], out: &Path, only: Option<&[&str]>, log: &mut dyn FnMut(&str)) -> R<usize> {
    if crate::hash::hex(&crate::hash::sha1(z64)) != crate::ssb64::SHA1 {
        return Err("not Super Smash Bros. (US)".into());
    }
    let decomp = crate::ssb64::decomp_dir(source);
    let files = Files::new(z64);
    let names = crate::ssb64::file_names(&decomp)?;
    let mut index = vec![];
    for stage in stages() {
        if only.is_some_and(|o| !o.contains(&stage.0.as_str())) {
            continue;
        }
        let info = export_stage(&files, &names, &stage, out).map_err(|e| format!("{}: {e}", stage.0))?;
        log(&format!("{}: {} triangles, {} materials, {} collision, {} moving parts ({} solid), bgm {}", info["id"].as_str().unwrap_or(""), info["triangles"], info["materials"], info["collision"], info["parts"], info["solids"], info["bgm"]));
        index.push(json!({"id": info["id"], "name": info["name"], "kind": info["kind"], "bgm": info["bgm"]}));
    }
    let n = index.len();
    let dir = out.join("courses");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let doc = crate::ssb64::sort_keys(json!({"game": "ssb64", "stages": index}));
    std::fs::write(dir.join("ssb64_stages.json"), dumps(&doc, Some(1)) + "\n").map_err(|e| e.to_string())?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_table_is_complete() {
        let s = stages();
        assert_eq!(s.len(), 41);
        assert_eq!(s[0].1, "GRCastleMap");
        assert_eq!(s[19].1, "GRBonus1DonkeyMap");
        assert_eq!(s[39].1, "GRBonus2PurinMap");
    }

    /// A floor segment faces up, a ceiling down, a right wall +x and a left wall -x; the depth walls face inward.
    #[test]
    fn extruded_lines_face_their_side() {
        let pts = |a: (i64, i64), b: (i64, i64)| vec![(a.0, a.1, 0u16), (b.0, b.1, 0x4000u16)];
        let g = Group {
            yakumono: 0,
            lines: [vec![pts((-1000, 0), (1000, 0))], vec![pts((1000, -100), (-1000, -100))], vec![pts((1000, 0), (1000, -100))], vec![pts((-1000, -100), (-1000, 0))]],
            offset: [0.0, 0.0],
            animated: false,
            moving: false,
        };
        let c = collision(&[&g], Bounds { top: 5000, bottom: -5000, right: 5000, left: -5000 }, DEPTH, true);
        let normal = |t: &[i64; 5]| {
            let (a, b, cc) = (c.verts[t[0] as usize], c.verts[t[1] as usize], c.verts[t[2] as usize]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [cc[0] - b[0], cc[1] - b[1], cc[2] - b[2]]);
            [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        };
        assert_eq!(c.tris.len(), 12);
        assert!(normal(&c.tris[0])[1] > 0.0, "floor up");
        assert!(normal(&c.tris[2])[1] < 0.0, "ceiling down");
        assert!(normal(&c.tris[4])[0] > 0.0, "right wall +x");
        assert!(normal(&c.tris[6])[0] < 0.0, "left wall -x");
        assert!(normal(&c.tris[8])[2] > 0.0 && normal(&c.tris[10])[2] < 0.0, "depth walls inward");
        assert_eq!(c.tris[8][3], SURF_NO_CAM);
        assert_eq!(c.counts, [1, 1, 1, 1]);
        assert!(c.passing.is_empty(), "the pass flag is the segment's first vertex");
    }
}
