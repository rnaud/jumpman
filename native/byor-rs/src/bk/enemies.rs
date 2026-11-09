//! Port of `scripts/export-bk-enemies.py`: the Banjo-Kazooie enemies as rigged models + clips + the decomp's per-state animation tables.
//! Writes `public/models/bk/enemies/<key>.json`.
use super::banjo::{b64, build_clip, f32_at, i16_at, i32_at, u16_at, u32_at, u8_at, vertex_row};
use super::levels::decode_texture;
use super::Assets;
use crate::pyc::{dumps, re, read, R};
use crate::zpng::{round_f, Num};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

struct Species {
    key: &'static str,
    model: &'static str,
    src: &'static str,
    table: &'static str,
    selectors: &'static [(i64, i64)],
}

const fn sp(key: &'static str, model: &'static str, src: &'static str, table: &'static str, selectors: &'static [(i64, i64)]) -> Species {
    Species { key, model, src, table, selectors }
}

const HOOD: &str = "src/CCW/ch/grublinhood.c";
const SPECIES: &[Species] = &[
    sp("grublin", "ASSET_3C5_MODEL_GRUBLIN", "src/MM/ch/grublin.c", "chGrublinAnimations", &[]),
    sp("grublin_hood_spring", "ASSET_52C_MODEL_GRUBLIN_HOOD", HOOD, "chGrublinHood_animations", &[(3, 2), (4, 2), (5, 1), (6, 1), (7, 1), (8, 1), (9, 0), (10, 0), (11, 0), (12, 1), (13, 0), (14, 1)]),
    sp("grublin_hood_summer", "ASSET_52C_MODEL_GRUBLIN_HOOD", HOOD, "chGrublinHood_animations", &[(3, 1), (4, 1), (5, 1), (6, 1), (7, 1), (8, 1), (9, 1), (10, 0), (11, 0), (12, 1), (13, 0), (14, 1)]),
    sp("grublin_hood_autumn", "ASSET_52C_MODEL_GRUBLIN_HOOD", HOOD, "chGrublinHood_animations", &[(3, 2), (4, 2), (5, 2), (6, 2), (7, 2), (8, 2), (9, 0), (10, 1), (11, 1), (12, 1), (13, 0), (14, 1)]),
    sp("grublin_hood_winter", "ASSET_52C_MODEL_GRUBLIN_HOOD", HOOD, "chGrublinHood_animations", &[(3, 2), (4, 2), (5, 2), (6, 2), (7, 2), (8, 2), (9, 0), (10, 2), (11, 2), (12, 2), (13, 1), (14, 1)]),
    sp("limbo", "ASSET_4CC_MODEL_LIMBO", "src/core2/ch/skeleton.c", "chSkeletonAnimations", &[]),
    sp("termite", "ASSET_350_MODEL_TERMITE", "src/core2/ch/termite.c", "chTermiteAnimations", &[]),
    sp("bigbutt", "ASSET_353_MODEL_BIGBUTT", "src/core2/ch/bigbutt.c", "chBullAnimations", &[]),
    sp("snippet", "ASSET_358_SNIPPET", "src/core2/ch/crab.c", "chCrabAnimations", &[]),
    sp("mutie_snippet", "ASSET_38F_MODEL_MUTIE_SNIPPET", "src/core2/ch/crab.c", "chCrabMutantAnimations", &[]),
    sp("sir_slush", "ASSET_377_MODEL_SIR_SLUSH", "src/core2/ch/snowman.c", "chSnowmanAnimations", &[(1, 0), (2, 1)]),
    sp("bat", "ASSET_3CA_MODEL_BAT", "src/core2/ch/bat.c", "sChBatAnimations", &[]),
    sp("shrapnel", "ASSET_3EC_MODEL_SHRAPNEL", "src/core2/ch/shrapnel.c", "chShrapnelAnimations", &[]),
    sp("snarebear", "ASSET_440_MODEL_SNAREBEAR", "src/CCW/ch/snarebear.c", "chSnarebearAnimations", &[]),
    sp("beehive", "ASSET_364_MODEL_BEEHIVE", "src/core2/ch/beehive.c", "chBeehiveAnimations", &[]),
    // These three set their clips with `skeletalAnim_set` instead of an `ActorAnimationInfo` table: the table is written out here
    // as `=` + one `ASSET:seconds` per actor state (empty = none), in the order of the decomp's state enums.
    // Clucker 1-3 hold the first short bite pose, 4 = short bite, 5/6 = die, 7 = long bite (`chClucker_setNextState`).
    sp("clucker", "ASSET_482_MODEL_CLUCKER", "", "=,ASSET_184_ANIM_CLUCKER_ATTACK_SHORT:0.8,ASSET_184_ANIM_CLUCKER_ATTACK_SHORT:0.8,ASSET_184_ANIM_CLUCKER_ATTACK_SHORT:0.8,ASSET_184_ANIM_CLUCKER_ATTACK_SHORT:0.8,ASSET_186_ANIM_CLUCKER_DIE:2.0,ASSET_186_ANIM_CLUCKER_DIE:2.0,ASSET_185_ANIM_CLUCKER_ATTACK_LONG:2.5", &[]),
    sp("grille_chompa", "ASSET_430_MODEL_GRILLE_CHOMPA", "", "=,ASSET_15A_ANIM_GRILL_CHOMPA_ATTACK:2.5,ASSET_15A_ANIM_GRILL_CHOMPA_ATTACK:2.5,ASSET_15A_ANIM_GRILL_CHOMPA_ATTACK:2.5,ASSET_15A_ANIM_GRILL_CHOMPA_ATTACK:2.5,ASSET_15B_ANIM_GRILL_CHOMPA_DIE:2.0,ASSET_15B_ANIM_GRILL_CHOMPA_DIE:2.0", &[]),
    // Flibbit: 1 idle, 2 hop, 3 turn, 4 bumped Banjo (idle), 5 ow, 6 die, 7 dead (`enum chFlibbitState`).
    // Mumbo's transformations of Banjo (`bs/ant.c croc.c walrus.c pumpkin.c bee*.c`): 1 idle, 2 walk, 3 jump, 4 ow / bounce, 5 die, 6 special
    // (croc bite, walrus sled, bee fly). The pumpkin has no idle clip and stands in its walk.
    sp("form_termite", "ASSET_34F_MODEL_BANJO_TERMITE", "", "=,ASSET_5E_ANIM_BSANT_IDLE:1.5,ASSET_5F_ANIM_BSANT_WALK:0.4,ASSET_60_ANIM_BSANT_JUMP:0.8,ASSET_28_ANIM_BSANT_OW:0.7,ASSET_29_ANIM_BSANT_DIE:1.5", &[]),
    sp("form_croc", "ASSET_374_MODEL_BANJO_CROC", "", "=,ASSET_E1_ANIM_BSCROC_IDLE:1.0,ASSET_E0_ANIM_BSCROC_WALK:0.4,ASSET_11C_ANIM_BSCROC_JUMP:0.8,ASSET_11D_ANIM_BSCROC_BOUNCE:0.7,ASSET_11E_ANIM_BSCROC_DIE:1.5,ASSET_122_ANIM_BSCROC_BITE:0.25", &[]),
    sp("form_walrus", "ASSET_359_MODEL_BANJO_WALRUS", "", "=,ASSET_11F_ANIM_BSWALRUS_IDLE:1.5,ASSET_120_ANIM_BSWALRUS_WALK:0.5,ASSET_121_ANIM_BSWALRUS_JUMP:0.8,ASSET_19C_ANIM_BSWALRUS_BOUNCE:0.7,ASSET_19D_ANIM_BSWALRUS_DIE:1.5,ASSET_19E_ANIM_BSWALRUS_SLED:0.6", &[]),
    sp("form_pumpkin", "ASSET_36F_MODEL_BANJO_PUMPKIN", "", "=,ASSET_A0_ANIM_BSPUMPKIN_WALK:1.0,ASSET_A0_ANIM_BSPUMPKIN_WALK:0.4,ASSET_A1_ANIM_BSPUMPKIN_JUMP:0.8,ASSET_236_ANIM_BSPUMPKIN_REBOUND:0.7,ASSET_188_ANIM_BSPUMPKIN_DIE:1.5", &[]),
    sp("form_bee", "ASSET_362_MODEL_BANJO_BEE", "", "=,ASSET_1DE_ANIM_BEE_IDLE:1.0,ASSET_1DD_ANIM_BEE_WALK:0.4,ASSET_1E2_ANIM_BEE_JUMP:0.8,ASSET_1E0_ANIM_BEE_OW:0.7,ASSET_1E1_ANIM_BEE_DIE:1.5,ASSET_1DC_ANIM_BEE_FLY:0.2", &[]),
    // Buzzbomb: 1 hover, 2 drift, 3 dive (charge clip), 4 climb away, 5 hit, 6 dying, 7 landed (`func_802CC640`; clips 0xF9 fly, 0x10F charge,
    // 0x110 / 0x111 die and land have no names in the decomp's enum).
    sp("buzzbomb", "ASSET_3ED_MODEL_BUZZBOMB", "", "=,ASSET_F9_ANIM_unk:0.45,ASSET_F9_ANIM_unk:0.45,ASSET_10F_ANIM_unk:0.45,ASSET_F9_ANIM_unk:0.45,ASSET_F9_ANIM_unk:1.0,ASSET_110_ANIM_unk:0.45,ASSET_111_ANIM_unk:1.0", &[]),
    sp("flibbit", "ASSET_375_MODEL_FLIBBIT_RED", "", "=,ASSET_FA_ANIM_FLIBBIT_IDLE:1.5,ASSET_DB_ANIM_FLIBBIT_HOP:0.8,ASSET_FB_ANIM_FLIBBIT_TURN:1.0,ASSET_FA_ANIM_FLIBBIT_IDLE:1.5,ASSET_288_ANIM_FLIBBIT_OW:0.65,ASSET_112_ANIM_FLIBBIT_DIE:0.4,ASSET_113_ANIM_FLIBBIT_DEAD:1.0", &[]),
];

/// [(asset id, seconds, asset name) | None] indexed by state, from the decomp's ActorAnimationInfo array.
fn read_table(decomp: &Path, spec: &Species, ids: &HashMap<String, i64>) -> R<Vec<Option<(i64, f64, String)>>> {
    if let Some(list) = spec.table.strip_prefix('=') {
        let mut rows = Vec::new();
        for item in list.split(',') {
            rows.push(match item.split_once(':') {
                None => None,
                Some((name, secs)) => {
                    let id = *ids.get(name).ok_or_else(|| format!("{name} not in enums.h"))?;
                    Some((id, secs.parse::<f64>().map_err(|e| format!("{name}: {e}"))?, name.to_string()))
                }
            });
        }
        return Ok(rows);
    }
    let text = read(&decomp.join(spec.src))?;
    let pat = format!(r"(?s)ActorAnimationInfo\s+{}\s*(?:\[[^\]]*\])?\s*=\s*\{{(.*?)\}};", spec.table);
    let m = re(&pat).captures(&text).ok_or_else(|| format!("{}: {} not found", spec.src, spec.table))?;
    let mut rows = Vec::new();
    for c in re(r"\{\s*([A-Za-z0-9_]+)\s*,\s*([0-9.eE+\-]+)f?\s*\}").captures_iter(&m[1]) {
        let seconds: f64 = c[2].parse().map_err(|_| format!("bad duration {}", &c[2]))?;
        rows.push(ids.get(&c[1]).map(|&id| (id, seconds, c[1].to_string())));
    }
    Ok(rows)
}

type BatchKey = (Option<String>, (u32, u32));

struct Model<'a> {
    m: &'a [u8],
    tex_data: usize,
    vertices: usize,
    commands: usize,
    textures: Vec<(String, Value)>,
    batches: Vec<(BatchKey, Vec<Value>)>,
    batch_ids: HashMap<BatchKey, usize>,
    cache: Vec<Option<[i64; 10]>>,
    selectors: HashMap<i64, i64>,
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

impl<'a> Model<'a> {
    fn texture(&mut self) -> Option<String> {
        let (image, tlut, fmt, siz, width, height) = (self.timg, self.tlut, self.fmt, self.siz, self.width, self.height);
        let key = format!("{image:x}_{fmt}{siz}_{width}x{height}");
        if self.textures.iter().any(|(k, _)| *k == key) {
            return Some(key);
        }
        let rgba = decode_texture(self.m, self.tex_data, image as usize, tlut as usize, fmt, siz, width as usize, height as usize).ok()?;
        self.textures.push((key.clone(), json!({"width": width, "height": height, "rgba": b64(&rgba)})));
        Some(key)
    }

    fn run_dl(&mut self, index: i64, bone: i64) -> R<()> {
        let m = self.m;
        let mut p = (self.commands as i64 + 8 * index) as usize;
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
                    let tex = if self.tex_on && !self.texgen { self.texture() } else { None };
                    let key: BatchKey = (tex.clone(), self.wrap);
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
                            let Some(c) = self.cache.get(((word >> shift & 0xFF) / 2) as usize).copied().flatten() else { continue };
                            let (mut u, mut v) = (Num::I(0), Num::I(0));
                            if tex.as_deref().is_some_and(|t| !t.is_empty()) {
                                u = Num::F(round_f((c[3] as f64 * self.scale.0 / 32.0 + 0.5) / self.width as f64, 4));
                                v = Num::F(round_f((c[4] as f64 * self.scale.1 / 32.0 + 0.5) / self.height as f64, 4));
                            }
                            self.batches[id].1.push(vertex_row(&[Num::I(c[0]), Num::I(c[1]), Num::I(c[2]), u, v, Num::I(c[5]), Num::I(c[6]), Num::I(c[7]), Num::I(c[8]), Num::I(c[9])]));
                        }
                    }
                }
                0xBB => {
                    self.tex_on = a & 0xFF != 0;
                    self.scale = ((b >> 16) as f64 / 65536.0, (b & 0xFFFF) as f64 / 65536.0);
                }
                0xB7 => {
                    if b & 0x40000 != 0 {
                        self.texgen = true;
                    }
                }
                0xB6 => {
                    if b & 0x40000 != 0 {
                        self.texgen = false;
                    }
                }
                0xFD => self.timg = b & 0xFFFFFF,
                0xF0 => self.tlut = self.timg,
                0xF5 => {
                    if (b >> 24) & 7 == 0 {
                        self.fmt = (a >> 21) & 7;
                        self.siz = (a >> 19) & 3;
                        self.wrap = ((b >> 8) & 3, (b >> 18) & 3);
                    }
                }
                0xF2 => {
                    if (b >> 24) & 7 == 0 {
                        self.width = ((b >> 12) & 0xFFF) / 4 + 1;
                        self.height = (b & 0xFFF) / 4 + 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn geo(&mut self, mut p: usize, bone: i64) -> R<()> {
        let m = self.m;
        let at = |p: usize, d: i64| (p as i64 + d) as usize;
        loop {
            let (cmd, nxt) = (u32_at(m, p)?, i32_at(m, p + 4)?);
            match cmd {
                0x02 => {
                    let (branch, mtx) = (u8_at(m, p + 8)?, u8_at(m, p + 9)? as i8);
                    if branch != 0 {
                        self.geo(p + branch as usize, mtx as i64)?;
                    }
                }
                0x03 => self.run_dl(i16_at(m, p + 8)? as i64, bone)?,
                0x07 => self.run_dl(i16_at(m, p + 10)? as i64, bone)?,
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
                        self.run_dl(index as i64, bone)?;
                    }
                }
                0x06 => self.geo(at(p, i32_at(m, p + 8)? as i64), bone)?,
                0x08 => {
                    let lod_min = f32_at(m, p + 12)?;
                    let branch = i32_at(m, p + 28)?;
                    if lod_min == 0.0 && branch != 0 {
                        self.geo(at(p, branch as i64), bone)?;
                    }
                }
                0x0C => {
                    let (count, index) = (i16_at(m, p + 8)? as i64, i16_at(m, p + 10)? as i64);
                    let choice = self.selectors.get(&index).copied().unwrap_or(1);
                    if 0 < choice && choice <= count {
                        self.geo(at(p, i32_at(m, p + 12 + 4 * (choice as usize - 1))? as i64), bone)?;
                    }
                }
                0x0D => {
                    let branch = i16_at(m, p + 20)?;
                    if branch != 0 {
                        self.geo(at(p, branch as i64), bone)?;
                    }
                }
                _ => {}
            }
            if nxt == 0 {
                return Ok(());
            }
            p = at(p, nxt as i64);
        }
    }
}

/// export-bk-banjo.build_model with wider texture formats and per-species appendage selectors.
fn build_model(m: &[u8], selectors: &[(i64, i64)]) -> R<Value> {
    let (geo_off, tex_list, gfx, vtx, anim_list) = (i32_at(m, 4)?, u16_at(m, 8)? as usize, i32_at(m, 12)?, i32_at(m, 16)?, i32_at(m, 24)? as usize);
    let mut bones = Vec::new();
    let mut trans_scale = 1.0f64;
    if anim_list != 0 {
        trans_scale = f32_at(m, anim_list)? as f64;
        let bone_count = i16_at(m, anim_list + 4)?;
        for i in 0..bone_count.max(0) as usize {
            let o = anim_list + 8 + 16 * i;
            bones.push(json!({"pivot": [round_f(f32_at(m, o)? as f64, 3), round_f(f32_at(m, o + 4)? as f64, 3), round_f(f32_at(m, o + 8)? as f64, 3)], "id": i16_at(m, o + 12)?, "parent": i16_at(m, o + 14)?}));
        }
    }
    if bones.is_empty() {
        bones = vec![json!({"pivot": [0.0, 0.0, 0.0], "id": 0, "parent": -1})];
    }
    let tex_count = i16_at(m, tex_list + 4)? as usize;
    let mut model = Model {
        m,
        tex_data: tex_list + 8 + 16 * tex_count,
        vertices: vtx as usize + 0x18,
        commands: gfx as usize + 8,
        textures: vec![],
        batches: vec![],
        batch_ids: HashMap::new(),
        cache: vec![None; 64],
        selectors: selectors.iter().copied().collect(),
        tex_on: false,
        scale: (1.0, 1.0),
        timg: 0,
        tlut: 0,
        fmt: 0,
        siz: 0,
        wrap: (0, 0),
        width: 32,
        height: 32,
        texgen: false,
    };
    model.geo(geo_off as usize, -1)?;
    let mut textures = Map::new();
    for (k, v) in model.textures {
        textures.insert(k, v);
    }
    let batches: Vec<Value> = model.batches.into_iter().map(|((tex, wrap), verts)| json!({"part": "body", "texture": tex, "wrap": [wrap.0, wrap.1], "vertices": verts})).collect();
    Ok(json!({"transScale": trans_scale, "bones": bones, "textures": Value::Object(textures), "batches": batches}))
}

fn export_species(assets: &Assets, ids: &HashMap<String, i64>, decomp: &Path, spec: &Species) -> R<Value> {
    let model_id = *ids.get(spec.model).ok_or_else(|| format!("{} not in enums.h", spec.model))?;
    let mut asset = build_model(&assets.read(model_id as usize)?, spec.selectors)?;
    let table = read_table(decomp, spec, ids)?;
    let mut clips = Map::new();
    let mut states: Vec<Value> = Vec::new();
    for row in table {
        match row {
            None => states.push(Value::Null),
            Some((0, _, _)) => states.push(Value::Null),
            Some((index, seconds, name)) => {
                let clip_name = re(r"^ASSET_[0-9A-Fa-f]+_(ANIM_)?").replace(&name, "").to_lowercase();
                if !clips.contains_key(&clip_name) {
                    clips.insert(clip_name.clone(), build_clip(&assets.read(index as usize)?, seconds)?);
                }
                states.push(json!([clip_name, seconds]));
            }
        }
    }
    let o = asset.as_object_mut().unwrap();
    o.insert("clips".into(), Value::Object(clips));
    o.insert("states".into(), Value::Array(states));
    Ok(asset)
}

pub fn export(assets: &Assets, ids: &HashMap<String, i64>, decomp: &Path, out: &Path) -> R<()> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    for spec in SPECIES {
        let asset = export_species(assets, ids, decomp, spec)?;
        std::fs::write(out.join(format!("{}.json", spec.key)), dumps(&asset, None)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
