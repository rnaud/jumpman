//! Port of `scripts/export-ssb64-weapons.py`: the Super Smash Bros. (US) weapon, effect and item models (static glTF,
//! one mesh in the bind pose) and the item sounds, byte-identical to the Python exporter. A child module of `ssb64`
//! (declared there), so it draws with the fighters' `Mesh` (F3DEX2 display lists, MObj materials, the combiner) and
//! writes with their `Gltf`. See native/SSB64.md "Weapons, effects and items".
//!
//! Writes `<public>/models/ssb64/objects/<name>.glb`, `<public>/models/ssb64/objects.json`, and
//! `<sounds>/<Name>_0.wav` + `<sounds>/manifest-ssb64-items.json`.
use super::*;

const DOBJ_END: i32 = 18;

/// Where a model's data is: a WPAttributes / ITAttributes struct (file, offset, DObjDesc tree?) or an EFDesc (file, root
/// offset, tree?, MObjSub *** offset, MatAnimJoint offset; 0 = none).
#[derive(Clone, Copy)]
pub enum Spec {
    Attr(&'static str, usize, bool),
    Ef(&'static str, usize, bool, usize, usize),
}

#[derive(Clone, Copy, Default)]
pub struct Opts {
    pub billboard: bool,
    pub palette: Option<usize>,
    pub env0: bool,
    pub solid: bool,
}

const fn o(billboard: bool) -> Opts {
    Opts { billboard, palette: None, env0: false, solid: false }
}

use Spec::{Attr, Ef};

/// `OBJECTS` of the Python exporter (same order, same options).
pub const OBJECTS: [(&str, &str, Spec, Opts); 48] = [
    ("fireball", "weapon", Attr("MarioSpecial1", 0x00, false), o(true)),
    ("fireball_luigi", "weapon", Attr("LuigiSpecial1", 0x00, false), Opts { billboard: true, palette: Some(1), env0: false, solid: false }),
    ("blaster", "weapon", Attr("FoxSpecial1", 0x00, false), o(false)),
    ("charge_shot", "weapon", Attr("SamusSpecial1", 0x00, false), o(true)),
    ("samus_bomb", "weapon", Attr("SamusMain", 0x0C, false), o(false)),
    ("boomerang", "weapon", Attr("LinkSpecial1", 0x00, true), o(false)),
    ("spin_attack", "weapon", Attr("LinkMain", 0x0C, true), Opts { billboard: false, palette: None, env0: false, solid: true }),
    ("final_cutter", "weapon", Attr("KirbyMain", 0x08, true), o(false)),
    ("pk_fire", "weapon", Attr("NessSpecial1", 0x00, false), o(true)),
    ("pk_thunder", "weapon", Attr("NessMain", 0x0C, true), o(true)),
    ("pk_thunder_trail", "weapon", Attr("NessMain", 0x40, false), o(true)),
    ("thunder_head", "weapon", Attr("PikachuMain", 0x0C, false), o(true)),
    ("thunder_jolt_air", "weapon", Attr("PikachuSpecial1", 0x00, false), o(true)),
    ("thunder_jolt", "weapon", Attr("PikachuSpecial1", 0x34, true), o(false)),
    ("yoshi_egg", "weapon", Attr("YoshiMain", 0x0C, false), Opts { billboard: false, palette: None, env0: true, solid: false }),
    ("yoshi_star", "weapon", Attr("YoshiMain", 0x40, false), o(false)),
    ("ray_gun_shot", "weapon", Attr("ITCommonData", 0x2B0, false), o(false)),
    ("capsule", "item", Attr("ITCommonData", 0x050, true), o(false)),
    ("maxim_tomato", "item", Attr("ITCommonData", 0x0B8, true), o(false)),
    ("heart", "item", Attr("ITCommonData", 0x100, true), o(false)),
    ("star", "item", Attr("ITCommonData", 0x148, true), o(false)),
    ("beam_sword", "item", Attr("ITCommonData", 0x190, true), o(false)),
    ("bat", "item", Attr("ITCommonData", 0x1D8, true), o(false)),
    ("fan", "item", Attr("ITCommonData", 0x220, true), o(false)),
    ("ray_gun", "item", Attr("ITCommonData", 0x268, true), o(false)),
    ("fire_flower", "item", Attr("ITCommonData", 0x2E4, true), o(false)),
    ("hammer", "item", Attr("ITCommonData", 0x374, true), Opts { billboard: false, palette: None, env0: true, solid: false }),
    ("motion_bomb", "item", Attr("ITCommonData", 0x3BC, true), o(false)),
    ("bob_omb", "item", Attr("ITCommonData", 0x424, true), Opts { billboard: false, palette: None, env0: true, solid: false }),
    ("star_rod", "item", Attr("ITCommonData", 0x48C, true), o(false)),
    ("green_shell", "item", Attr("ITCommonData", 0x53C, true), Opts { billboard: false, palette: Some(1), env0: false, solid: false }),
    ("red_shell", "item", Attr("ITCommonData", 0x584, true), Opts { billboard: false, palette: Some(0), env0: false, solid: false }),
    ("crate", "item", Attr("ITCommonData", 0x5CC, true), o(false)),
    ("barrel", "item", Attr("ITCommonData", 0x634, true), o(false)),
    ("bumper", "item", Attr("ITCommonData", 0x69C, true), o(false)),
    ("poke_ball", "item", Attr("ITCommonData", 0x6E4, true), o(false)),
    ("egg", "item", Attr("ITCommonData", 0xACC, true), o(false)),
    ("link_bomb", "item", Attr("LinkMain", 0x40, true), Opts { billboard: false, palette: None, env0: true, solid: false }),
    ("pk_fire_pillar", "item", Attr("NessSpecial1", 0x34, true), o(false)),
    ("falcon_punch", "effect", Ef("CaptainSpecial3", 0x760, false, 0x690, 0x830), o(false)),
    ("falcon_kick", "effect", Ef("CaptainSpecial2", 0xB08, true, 0x960, 0xC00), o(false)),
    ("reflector", "effect", Ef("FoxSpecial2", 0x2B0, true, 0, 0), o(true)),
    ("psi_magnet", "effect", Ef("NessSpecial2", 0x9A8, true, 0x810, 0xAD0), o(true)),
    ("sing", "effect", Ef("PurinSpecial2", 0x2130, true, 0x1C20, 0x2D70), Opts { billboard: true, palette: None, env0: false, solid: true }),
    ("spin_attack_trail", "effect", Ef("LinkSpecial2", 0x11C0, true, 0x1038, 0x12F0), Opts { billboard: false, palette: None, env0: false, solid: true }),
    ("thunder_shock", "effect", Ef("PikachuSpecial2", 0x1640, true, 0x13A0, 0x1A80), o(true)),
    ("egg_lay", "effect", Ef("YoshiSpecial3", 0x960, true, 0, 0), o(false)),
    ("hit_slash", "effect", Ef("EFCommonEffects1", 0x7750, true, 0x73E0, 0x7860), o(true)),
];

/// `ITEM_FGM` of the Python exporter.
pub const ITEM_FGM: [&str; 34] = [
    "FGMItemGet", "FGMItemThrow", "FGMItemMapCollide", "FGMItemSpawn1", "FGMContainerSmash", "FGMLightSwingS",
    "FGMLightSwingM", "FGMLightSwingL", "FGMHeavySwing1", "FGMSwordSwing1", "FGMSwordSwing3", "FGMSwordSwing4",
    "FGMBatHit", "FGMHarisenHit", "FGMHammerSwing", "FGMLGunShoot", "FGMLGunEmpty", "FGMFireFlowerShoot", "FGMBurnE",
    "FGMStarRodSwing1", "FGMStarRodSwing3", "FGMStarRodSwing4", "FGMStarRodEmpty", "FGMStarGet", "FGMStarMapCollide",
    "FGMBombHeiFuse", "FGMBombHeiWalkStart", "FGMExplodeL", "FGMShellHit", "FGMMSBombAttach", "FGMBumperHit",
    "FGMMBallOpen", "FGMPlayerHeal", "FGMPunchL",
];

/// A DObj's `dl`: one display list, or a DObjDLLink array (`{s32 list_id; Gfx *dl}` up to list id 4).
fn dl_parts(files: &Files, dl: Option<Ref>) -> R<Vec<Ref>> {
    let Some(dl) = dl else { return Ok(vec![]) };
    let f = files.get(dl.0)?;
    let first = be32(&f.data, dl.1)?;
    if first < 4 && files.ptr(Some(dl), 4)?.is_some() {
        let mut out = vec![];
        let mut k = 0;
        while be32(&f.data, dl.1 + 8 * k)? < 4 {
            if let Some(p) = files.ptr(Some(dl), 8 * k + 4)? {
                out.push(p);
            }
            k += 1;
        }
        return Ok(out);
    }
    Ok(vec![dl])
}

/// gcSetupCustomDObjs: (dl, world) per DObj.
fn tree(files: &Files, root: Ref, is_tree: bool) -> R<Vec<(Option<Ref>, M4)>> {
    if !is_tree {
        return Ok(vec![(Some(root), mat_trs([0.0; 3], [0.0; 3], [1.0; 3]))]);
    }
    let f = files.get(root.0)?;
    let d = &f.data;
    let mut joints: Vec<(Option<Ref>, M4)> = vec![];
    let mut stack: HashMap<i32, usize> = HashMap::new();
    let mut i = 0;
    loop {
        let off = root.1 + 44 * i;
        let jid = be32(d, off)? as i32;
        if jid == DOBJ_END {
            break;
        }
        let depth = jid & 0xFFF;
        let v: Vec<f64> = (0..9).map(|k| bef32(d, off + 8 + 4 * k)).collect::<R<_>>()?;
        let local = mat_trs([v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]);
        let parent = if depth != 0 { stack.get(&(depth - 1)).copied() } else { None };
        let world = match parent {
            Some(p) => mat_mul(&joints[p].1, &local),
            None => local,
        };
        stack.insert(depth, joints.len());
        joints.push((files.ptr(Some(root), 44 * i + 4)?, world));
        i += 1;
    }
    Ok(joints)
}

fn model<'f, 'a>(files: &'f Files<'a>, names: &HashMap<String, usize>, spec: Spec, opts: Opts) -> R<(Mesh<'f, 'a>, usize)> {
    let fid = |n: &str| names.get(n).copied().ok_or_else(|| format!("{n}: not in relocFileDescriptions"));
    let (root, mobs, mats, is_tree) = match spec {
        Attr(fname, off, t) => {
            let r = Some((fid(fname)?, off));
            (files.ptr(r, 0)?, files.ptr(r, 4)?, files.ptr(r, 12)?, t)
        }
        Ef(fname, root_off, t, mob_off, mat_off) => {
            let f = fid(fname)?;
            (Some((f, root_off)), if mob_off != 0 { Some((f, mob_off)) } else { None }, if mat_off != 0 { Some((f, mat_off)) } else { None }, t)
        }
    };
    let joints = match root {
        Some(r) => tree(files, r, is_tree)?,
        None => vec![],
    };
    let mut m = Mesh::new(files);
    if opts.env0 {
        m.env = [0.0; 4];
    }
    let mut palette = opts.palette;
    for (ji, (dl, world)) in joints.iter().enumerate() {
        let mlist = if mobs.is_some() { files.ptr(mobs, 4 * ji)? } else { None };
        let clist = if mats.is_some() { files.ptr(mats, 4 * ji)? } else { None };
        m.mobjs = read_mobjs(files, mlist)?;
        for k in 0..m.mobjs.len() {
            let script = if clist.is_some() { files.ptr(clist, 4 * k)? } else { None };
            let mo = &mut m.mobjs[k];
            costume(files, mo, script)?;
            if opts.solid {
                mo.prim[3] = 1.0;
                mo.env[3] = 1.0;
            }
            if let (Some(p), Some(_)) = (palette, mo.palettes) {
                mo.palette_id = Some(p as i64);
                mo.palette = files.ptr(mo.palettes, 4 * p)?;
                palette = None;
            }
        }
        for part in dl_parts(files, *dl)? {
            m.run(part, ji, world)?;
        }
    }
    let n = joints.len();
    Ok((m, n))
}

/// Whether the combiner's alpha reads the shade (vertex) alpha (`shade_alpha` in the Python exporter).
fn shade_alpha(mode: (u32, u32), two: bool) -> bool {
    let (w0, w1) = mode;
    let cycles = [[(w0 >> 12) & 7, (w1 >> 12) & 7, (w0 >> 9) & 7, (w1 >> 9) & 7], [(w1 >> 21) & 7, (w1 >> 3) & 7, (w1 >> 18) & 7, w1 & 7]];
    cycles[..if two { 2 } else { 1 }].iter().any(|c| c.contains(&4))
}

fn glb(mesh: &Mesh, name: &str) -> R<(Vec<u8>, [f64; 3], [f64; 3])> {
    let mut g = Gltf::new();
    g.j.insert("asset".into(), json!({"version": "2.0", "generator": "jumpman export-ssb64-weapons.py"}));
    g.arr("nodes").push(json!({"name": name, "mesh": 0}));
    g.j.insert("scenes".into(), json!([{"nodes": [0]}]));
    let mut prims = vec![];
    let mut image_index: Vec<((Vec<u8>, (i64, i64)), usize)> = vec![];
    let (mut lo, mut hi) = ([1e30f64; 3], [-1e30f64; 3]);
    let mut any = false;
    for (mi, idx) in mesh.tris.iter().enumerate() {
        if idx.is_empty() {
            continue;
        }
        any = true;
        let mat = &mesh.materials[mi];
        let key = &mesh.keys[mi];
        let mode = (u32::from_le_bytes(key[0..4].try_into().unwrap()), u32::from_le_bytes(key[4..8].try_into().unwrap()));
        let keep_alpha = shade_alpha(mode, key[8] != 0);
        let mut vids: Vec<usize> = idx.clone();
        vids.sort_unstable();
        vids.dedup();
        let remap: HashMap<usize, usize> = vids.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        let (mut pos, mut nrm, mut uv, mut col) = (vec![], vec![], vec![], vec![]);
        for &v in &vids {
            let vert = &mesh.verts[v];
            let pp: Vec<f64> = vert.pos.iter().map(|x| f32r(x * UNIT)).collect();
            for k in 0..3 {
                if pp[k] < lo[k] {
                    lo[k] = pp[k];
                }
                if pp[k] > hi[k] {
                    hi[k] = pp[k];
                }
            }
            pos.push(pp);
            let n = vert.nrm.unwrap_or([0.0, 1.0, 0.0]);
            let ln = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let ln = if ln == 0.0 { 1.0 } else { ln };
            nrm.push(n.iter().map(|x| f32r(x / ln)).collect::<Vec<_>>());
            uv.push(vert.uv.iter().map(|&x| f32r(x)).collect::<Vec<_>>());
            let mut c = vert.col.unwrap_or([1.0; 4]);
            if !keep_alpha {
                c[3] = 1.0;
            }
            col.push(linear(c).iter().map(|&x| f32r(x)).collect::<Vec<_>>());
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
        let mut pbr = Map::new();
        pbr.insert("baseColorFactor".into(), json!(linear(mat.color).map(f32r)));
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
        let mut ordered = Map::new();
        ordered.insert("name".into(), json!(format!("mat{mi:03}")));
        ordered.insert("pbrMetallicRoughness".into(), Value::Object(pbr));
        ordered.extend(extra);
        g.arr("materials").push(Value::Object(ordered));
        let material = g.arr("materials").len() - 1;
        prims.push(json!({"attributes": attrs, "indices": indices, "material": material}));
    }
    g.arr("meshes").push(json!({"name": name, "primitives": prims}));
    if !any {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    Ok((g.glb(), lo, hi))
}

/// The axis a flat model has no extent along (`plane` in the Python exporter).
fn plane(lo: &[f64; 3], hi: &[f64; 3]) -> Value {
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let mut big = ext[0];
    for e in &ext[1..] {
        if *e > big {
            big = *e;
        }
    }
    for (k, e) in ext.iter().enumerate() {
        if big > 0.0 && *e <= 0.01 * big {
            return json!(["x", "y", "z"][k]);
        }
    }
    Value::Null
}

/// `export_models()`: every model of [`OBJECTS`] into `<out>/objects` and `<out>/objects.json`.
pub fn export_models(files: &Files, decomp: &Path, names: &HashMap<String, usize>, out: &Path, log: &mut dyn FnMut(&str)) -> R<Value> {
    let objs = out.join("objects");
    std::fs::create_dir_all(&objs).map_err(|e| format!("{}: {e}", objs.display()))?;
    let mut index = Map::new();
    for (name, kind, spec, opts) in OBJECTS {
        let (mesh, joints) = model(files, names, spec, opts)?;
        let (data, lo, hi) = glb(&mesh, name)?;
        let path = objs.join(format!("{name}.glb"));
        std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
        let tris: usize = mesh.tris.iter().map(|t| t.len()).sum::<usize>() / 3;
        let source = match spec {
            Attr(f, ..) | Ef(f, ..) => f,
        };
        index.insert(
            name.into(),
            json!({"kind": kind, "file": format!("objects/{name}.glb"), "source": source, "joints": joints, "triangles": tris,
                   "materials": mesh.materials.len(), "billboard": opts.billboard, "plane": plane(&lo, &hi), "min": lo, "max": hi}),
        );
        if !mesh.unknown.is_empty() {
            log(&format!("{name}: unhandled display list opcodes {:?}", mesh.unknown));
        }
    }
    // FTAttributes.joint_itemlight_id / joint_itemheavy_id (+0x33C / +0x334): the joints that hold items
    let tables = fighter_tables(decomp, names)?;
    let mut hands = Map::new();
    for f in &FIGHTERS[..12] {
        let t = &tables[*f];
        let file = files.get(t.main)?;
        let at = |off: usize| -> R<i64> { Ok(be32(&file.data, t.attr + off)? as i32 as i64) };
        hands.insert(f.to_lowercase(), json!({"light": at(0x33C)?, "heavy": at(0x334)?}));
    }
    let doc = sort_keys(json!({"game": "ssb64", "unit": UNIT, "objects": Value::Object(index), "hands": Value::Object(hands)}));
    std::fs::write(out.join("objects.json"), dumps(&doc, Some(1)) + "\n").map_err(|e| e.to_string())?;
    Ok(doc)
}

/// `export_sounds()`: the [`ITEM_FGM`] sounds into `out` with `manifest-ssb64-items.json`. Returns the number written.
pub fn export_sounds(source: &Path, z64: &[u8], out: &Path) -> R<usize> {
    use crate::ssb64_sounds::{voice_names, Engine, RATE};
    let names = voice_names(&decomp_dir(source))?;
    let ids: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let mut engine = Engine::new(z64)?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut clips = Map::new();
    for name in ITEM_FGM {
        let id = *ids.get(name).ok_or_else(|| format!("{name}: not in gmFGMVoiceID"))?;
        let (pcm, cut) = engine.render(id).map_err(|e| format!("{name}: {e}"))?;
        if pcm.is_empty() {
            continue;
        }
        let path = out.join(format!("{name}_0.wav"));
        std::fs::write(&path, crate::oot::voices::wav_bytes(&pcm, RATE)).map_err(|e| format!("{}: {e}", path.display()))?;
        let peak = pcm.iter().map(|x| x.abs()).max().unwrap_or(0);
        clips.insert(name.into(), json!({"id": id, "frames": pcm.len(), "duration": crate::oot::round_to(pcm.len() as f64 / RATE as f64, 4), "peak": peak, "cut": cut}));
    }
    let n = clips.len();
    let doc = sort_keys(json!({"sampleRate": RATE, "clips": Value::Object(clips)}));
    std::fs::write(out.join("manifest-ssb64-items.json"), dumps(&doc, Some(1)) + "\n").map_err(|e| e.to_string())?;
    Ok(n)
}

/// Both, for `ssb64::run`: models into `<public>/models/ssb64`, sounds into `sounds`.
pub fn export(source: &Path, z64: &[u8], public: &Path, sounds: &Path) -> R<()> {
    let files = Files::new(z64);
    let decomp = decomp_dir(source);
    let names = file_names(&decomp)?;
    export_models(&files, &decomp, &names, &public.join("models/ssb64"), &mut |m| eprintln!("{m}"))?;
    export_sounds(source, z64, sounds)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tables here and in the Python exporter are the same (names, kinds, offsets, options).
    #[test]
    fn tables_agree_with_python() {
        let py = include_str!("../../../scripts/export-ssb64-weapons.py");
        let block = &py[py.find("OBJECTS = [").unwrap()..];
        let block = &block[..block.find("\n]\n").unwrap()];
        let lines: Vec<&str> = block.lines().filter(|l| l.trim_start().starts_with("('")).collect();
        assert_eq!(lines.len(), OBJECTS.len());
        for (line, (name, kind, spec, opts)) in lines.iter().zip(OBJECTS) {
            assert!(line.contains(&format!("('{name}', '{kind}', ")), "{line}");
            // the spec tuple: ('attr', 'File', off, tree) / ('ef', 'File', root, tree, mobjsubs, matanim)
            let tuple = &line[line.find("(('").map(|i| i + 1).or_else(|| line.find(", ('").map(|i| i + 2)).unwrap()..];
            let tuple = &tuple[1..tuple.find(')').unwrap()];
            let parts: Vec<&str> = tuple.split(", ").collect();
            let num = |s: &str| if let Some(h) = s.strip_prefix("0x") { usize::from_str_radix(h, 16).unwrap() } else { s.parse().unwrap() };
            let (f, nums, t) = match spec {
                Attr(f, off, t) => (f, vec![off], t),
                Ef(f, r, t, m, a) => (f, vec![r, m, a], t),
            };
            assert_eq!(parts[1], format!("'{f}'"), "{line}");
            let got: Vec<usize> = parts.iter().skip(2).filter(|p| *p != &"True" && *p != &"False").map(|p| num(p)).collect();
            assert_eq!(got, nums, "{line}");
            assert_eq!(parts[3] == "True", t, "{line}");
            assert_eq!(line.contains("'billboard': True"), opts.billboard, "{line}");
            assert_eq!(line.contains("'env0': True"), opts.env0, "{line}");
            assert_eq!(line.contains("'solid': True"), opts.solid, "{line}");
            assert_eq!(opts.palette.map(|p| format!("'palette': {p}")).is_some_and(|p| line.contains(&p)), opts.palette.is_some(), "{line}");
        }
        let fgm = &py[py.find("ITEM_FGM = [").unwrap()..];
        let fgm: Vec<&str> = fgm[..fgm.find(']').unwrap()].split('\'').skip(1).step_by(2).collect();
        assert_eq!(fgm, ITEM_FGM);
    }

    #[test]
    fn shade_alpha_reads_the_alpha_inputs() {
        // G_CC_MODULATEIA (TEXEL0 * SHADE, alpha TEXEL0 * SHADE) in both cycles: reads shade alpha
        let modulate = (0x00121824u32, 0xFF33FFFFu32);
        assert!(shade_alpha(modulate, false));
        // alpha = TEXEL0 only (a=1, b=7, c=6... d): `0 0 0 TEXEL0` -> aa=7 ab=7 ac=7 ad=1
        let tex_alpha = (0x00FFFE00 | (7 << 12) | (7 << 9), (7 << 12) | (1 << 9));
        assert!(!shade_alpha(tex_alpha, false));
    }

    #[test]
    fn plane_of_a_flat_quad() {
        assert_eq!(plane(&[-1.0, -1.0, 0.0], &[1.0, 1.0, 0.0]), json!("z"));
        assert_eq!(plane(&[0.0, -1.0, -1.0], &[0.0, 1.0, 1.0]), json!("x"));
        assert_eq!(plane(&[-1.0; 3], &[1.0; 3]), Value::Null);
    }
}
