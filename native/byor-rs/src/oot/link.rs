//! Ports of `export-oot-link.py` (Adult and Young Link), `export-oot-items.py` (item models) and `export-oot-epona.py`
//! (Epona and Link's riding clips): JSON "Link format" assets with base64 RGBA textures.
use super::*;
use crate::pyc::dumps;
use serde_json::{json, Map, Value};

pub fn base64(data: &[u8]) -> String {
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

/// `rgba16()` of export-oot-link.py.
pub fn rgba16(value: u16) -> [u8; 4] {
    let c = |shift: u32| ((value as u32 >> shift & 31) * 255 / 31) as u8;
    [c(11), c(6), c(1), 255 * (value & 1) as u8]
}

/// `texture(data, spec)` of export-oot-link.py (`items`: the superset of export-oot-items.py that adds ia8 / i4 / ia4).
pub fn texture(data: &[u8], spec: &Attrs, items: bool) -> R<Value> {
    let start = spec.offset();
    let (w, h) = (spec.int("Width"), spec.int("Height"));
    let fmt = spec.get("Format").unwrap_or("");
    let mut px = Vec::with_capacity(w * h * 4);
    for i in 0..w * h {
        match fmt {
            "i8" => px.extend_from_slice(&[data[start + i], data[start + i], data[start + i], 255]),
            "ia16" => px.extend_from_slice(&[data[start + 2 * i], data[start + 2 * i], data[start + 2 * i], data[start + 2 * i + 1]]),
            "rgba16" | "ci8" => {
                let p = if fmt == "ci8" { hex(spec.get("TlutOffset").ok_or("KeyError: TlutOffset")?) + 2 * data[start + i] as usize } else { start + 2 * i };
                px.extend_from_slice(&rgba16(u16_(data, p)));
            }
            "ia8" if items => {
                let b = data[start + i];
                let (inten, alpha) = ((b >> 4) * 17, (b & 15) * 17);
                px.extend_from_slice(&[inten, inten, inten, alpha]);
            }
            "i4" | "ia4" if items => {
                let b = data[start + i / 2];
                let n = if i % 2 == 0 { b >> 4 } else { b & 15 };
                let (inten, alpha) = if fmt == "i4" { (n as u32 * 17, n as u32 * 17) } else { ((n >> 1) as u32 * 255 / 7, 255 * (n & 1) as u32) };
                px.extend_from_slice(&[inten as u8, inten as u8, inten as u8, alpha as u8]);
            }
            other => return Err(format!("ValueError: {other}")),
        }
    }
    Ok(json!({"width": w, "height": h, "rgba": base64(&px)}))
}

fn ints<T: Copy + Into<i64>>(v: &[T]) -> Value {
    Value::Array(v.iter().map(|&x| Value::from(x.into())).collect())
}

/// Dict keyed by an ordered tuple, insertion-ordered (Python dict semantics).
pub struct OrderedBatches<K: PartialEq> {
    keys: Vec<K>,
    pub vals: Vec<Vec<Value>>,
}

impl<K: PartialEq> OrderedBatches<K> {
    pub fn new() -> Self {
        OrderedBatches { keys: Vec::new(), vals: Vec::new() }
    }
    pub fn entry(&mut self, k: K) -> &mut Vec<Value> {
        let i = match self.keys.iter().position(|x| *x == k) {
            Some(i) => i,
            None => {
                self.keys.push(k);
                self.vals.push(Vec::new());
                self.keys.len() - 1
            }
        };
        &mut self.vals[i]
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &Vec<Value>)> {
        self.keys.iter().zip(self.vals.iter())
    }
}

type SpecMap = HashMap<u32, (usize, Attrs)>;

const EXTRA_CLIPS: [(&str, &str); 70] = [
    ("slashR", "fighter_Lside_kiru"),
    ("slashL", "fighter_Rside_kiru"),
    ("stab", "fighter_pierce_kiru"),
    ("slashRFinish", "fighter_Lside_kiru_finsh"),
    ("slashLFinish", "fighter_Rside_kiru_finsh"),
    ("stabFinish", "fighter_pierce_kiru_finsh"),
    ("flipSlash", "fighter_jump_rollkiru"),
    ("jumpSlash", "fighter_Lpower_jump_kiru"),
    ("spin", "fighter_rolling_kiru"),
    ("bigSpin", "fighter_Wrolling_kiru"),
    ("spinCharge", "fighter_power_kiru_start"),
    ("spinChargeWait", "fighter_power_kiru_wait"),
    ("hammer", "hammer_hit"),
    ("hammerSide", "hammer_side_hit"),
    ("bowReady", "bow_bow_ready"),
    ("bowWait", "bow_bow_wait"),
    ("bowShoot", "bow_bow_shoot"),
    ("hookReady", "hook_shot_ready"),
    ("hookWait", "hook_wait"),
    ("hookFly", "hook_fly_wait"),
    ("boomThrow", "boom_throwR"),
    ("boomCatch", "boom_catch"),
    ("throw", "normal_throw"),
    ("lightBomb", "normal_light_bom"),
    ("ocarina", "normal_okarina_swing"),
    ("ocarinaStart", "normal_okarina_start"),
    ("ocarinaEnd", "normal_okarina_end"),
    ("dinsCast", "magic_honoo1"),
    ("farore", "magic_kaze1"),
    ("nayru", "magic_tamashii1"),
    ("drink", "bottle_drink_demo_start"),
    ("bottleSwing", "bottle_fish_out"),
    ("jumpEnd", "normal_run_jump_end"),
    ("landing", "normal_landing"),
    ("shortLanding", "normal_short_landing"),
    ("frontFlip", "fighter_front_jump"),
    ("frontFlipEnd", "fighter_front_jump_end"),
    ("pushWait", "normal_push_wait"),
    ("pushing", "normal_pushing"),
    ("pulling", "normal_pulling_free"),
    ("held", "normal_re_dead_attack"),
    ("heldWait", "normal_re_dead_attack_wait"),
    ("diveStart", "swimer_swim_deep_start"),
    ("swimWait", "swimer_swim_wait"),
    ("slashEndZ", "fighter_normal_kiru_endR"),
    ("slashREnd", "fighter_Lside_kiru_end"),
    ("slashREndZ", "anchor_Lside_kiru_endR"),
    ("slashLEnd", "fighter_Rside_kiru_end"),
    ("slashLEndZ", "anchor_Rside_kiru_endR"),
    ("stabEnd", "fighter_pierce_kiru_end"),
    ("stabEndZ", "anchor_pierce_kiru_endR"),
    ("finishEndZ", "anchor_normal_kiru_finsh_endR"),
    ("slashRFinishEnd", "fighter_Lside_kiru_finsh_end"),
    ("slashRFinishEndZ", "anchor_Lside_kiru_finsh_endR"),
    ("slashLFinishEnd", "fighter_Rside_kiru_finsh_end"),
    ("slashLFinishEndZ", "anchor_Rside_kiru_finsh_endR"),
    ("stabFinishEnd", "fighter_pierce_kiru_finsh_end"),
    ("stabFinishEndZ", "anchor_pierce_kiru_finsh_endR"),
    ("spinEnd", "fighter_rolling_kiru_end"),
    ("spinEndZ", "anchor_rolling_kiru_endR"),
    ("bigSpinEnd", "fighter_Wrolling_kiru_end"),
    ("hammerEnd", "hammer_hit_end"),
    ("hammerSideEnd", "hammer_side_hit_end"),
    ("jumpSlashHit", "fighter_Lpower_jump_kiru_hit"),
    ("jumpSlashEnd", "fighter_power_jump_kiru_end"),
    ("flipSlashHit", "fighter_jump_kiru_finsh"),
    ("flipSlashEnd", "fighter_jump_kiru_finsh_end"),
    ("swordIdle", "fighter_wait_long"),
    ("swordWalk", "fighter_walk_long"),
    ("swordRun", "fighter_run_long"),
];

const BASE_CLIPS: [(&str, &str); 20] = [
    ("backflip", "fighter_backturn_jump"),
    ("leftHop", "fighter_Lside_jump"),
    ("rightHop", "fighter_Rside_jump"),
    ("climb", "normal_climb_up"),
    ("crawl", "child_tunnel_start"),
    ("strafe", "fighter_side_walk_long"),
    ("guard", "fighter_defense_wait"),
    ("idle", "normal_wait_free"),
    ("walk", "normal_walk_free"),
    ("run", "normal_run_free"),
    ("roll", "normal_landing_roll_free"),
    ("jump", "normal_run_jump"),
    ("fall", "normal_fall_wait"),
    ("focus", "fighter_waitL_long"),
    ("finish", "fighter_normal_kiru_finsh"),
    ("finishRecover", "fighter_normal_kiru_finsh_end"),
    ("slash", "fighter_normal_kiru"),
    ("recover", "fighter_normal_kiru_end"),
    ("swim", "swimer_swim"),
    ("hang", "normal_jump_climb_wait_free"),
];

/// Link's animation frames: `FrameCount` rows of 66 big-endian s16 at 134-byte strides.
pub fn link_frames(motion: &[u8], animations: &Defs, suffix: &str) -> R<Value> {
    let entry = animations.get(&format!("gPlayerAnim_link_{suffix}_Data"))?;
    let base = entry.offset();
    Ok(Value::Array((0..entry.int("FrameCount")).map(|i| Value::Array((0..66).map(|k| Value::from(i16_(motion, base + i * 134 + 2 * k))).collect())).collect()))
}

fn spec_map(parts: &[(u32, usize, &Defs)]) -> SpecMap {
    let mut m = SpecMap::new();
    for (seg, which, entries) in parts {
        for v in entries.values() {
            if v.has("Format") && v.has("Width") && v.has_hex_offset() {
                m.insert(seg + v.offset() as u32, (*which, v.clone()));
            }
        }
    }
    m
}

fn is_tlut(spec: &Attrs) -> bool {
    spec.name().to_lowercase().contains("tlut")
}

fn build_link(model: &[u8], keep: &[u8], motion: &[u8], objects: &Defs, shared: &Defs, animations: &Defs, child: bool) -> R<Value> {
    let blobs = [model, keep];
    let specs = spec_map(&[(0x0600_0000, 0, objects), (0x0400_0000, 1, shared)]);
    let mut textures = Map::new();
    let prefix = if child { "gLinkChild" } else { "gLinkAdult" };
    let mut limbs = Vec::new();
    let mut matrices: Vec<usize> = Vec::new();
    let mut lists: Vec<(usize, usize, String)> = Vec::new();
    let root = objects.offset(&format!("{prefix}RootLimb"))?;
    for i in 0..21 {
        let p = root + 16 * i;
        let (x, y, z, first_child, sibling, dl) = (i16_(model, p), i16_(model, p + 2), i16_(model, p + 4), u8_(model, p + 6), u8_(model, p + 7), u32_(model, p + 8));
        limbs.push(json!({"pos": [x, y, z], "child": first_child, "sibling": sibling}));
        if dl != 0 {
            matrices.push(i);
            lists.push((i, (dl & MASK) as usize, "base".into()));
        }
    }
    let equipment: Vec<(usize, (&str, &str))> = if child {
        vec![(15, ("LeftHandNear", "LeftFistAndKokiriSwordNear")), (18, ("RightHandNear", "RightFistAndDekuShieldNear")), (19, ("BackDekuShieldKokiriSwordInSheathNear", "BackKokiriSheathNear"))]
    } else {
        vec![(15, ("LeftHandNear", "LeftHandHoldingMasterSwordNear")), (19, ("BackHylianShieldSwordInSheathNear", "BackMasterSheathNear")), (18, ("RightHandNear", "RightHandHoldingHylianShieldNear"))]
    };
    lists.retain(|item| !equipment.iter().any(|(l, _)| *l == item.0));
    for (limb, (a, b)) in &equipment {
        for (mode, suffix) in [("sheathed", a), ("drawn", b)] {
            lists.push((*limb, objects.offset(&format!("{prefix}{suffix}DL"))?, mode.into()));
        }
    }
    let holds: Vec<(usize, &str, &str)> = if child {
        vec![(18, "RightHandHoldingSlingshotNear", "hold:slingshot"), (15, "LeftFistAndBoomerangNear", "hold:boomerang")]
    } else {
        vec![(18, "RightHandHoldingBowNear", "hold:bow"), (18, "RightHandHoldingHookshotNear", "hold:hookshot"), (15, "LeftHandHoldingHammerNear", "hold:hammer")]
    };
    for (limb, suffix, mode) in holds {
        lists.push((limb, objects.offset(&format!("{prefix}{suffix}DL"))?, mode.into()));
    }

    let mut batches: OrderedBatches<(String, Option<String>, u32, u32, u32)> = OrderedBatches::new();
    for (limb, mut p, mode) in lists {
        let mut cache: HashMap<i64, Vec<i64>> = HashMap::new();
        let (mut wrap_s, mut wrap_t, mut bone, mut tex, mut color) = (0u32, 0u32, limb, None::<String>, 0xff_ffffu32);
        loop {
            let (a, b) = (u32_(model, p), u32_(model, p + 4));
            p += 8;
            let op = a >> 24;
            match op {
                0xdf => break,
                0xda => bone = matrices[((b & MASK) / 64) as usize],
                0xfd => {
                    let addr = match b {
                        0x0800_0000 => 0x0600_0000,
                        0x0900_0000 => 0x0600_4000,
                        o => o,
                    };
                    let (which, spec) = specs.get(&addr).ok_or_else(|| format!("KeyError: {addr:#x}"))?;
                    if !is_tlut(spec) {
                        let name = spec.name().to_string();
                        if !textures.contains_key(&name) {
                            textures.insert(name.clone(), texture(blobs[*which], spec, false)?);
                        }
                        tex = Some(name);
                    }
                }
                0xf5 if b >> 24 == 0 => {
                    wrap_s = (b >> 8) & 3;
                    wrap_t = (b >> 18) & 3;
                }
                0xfa => color = b >> 8,
                1 => {
                    let n = ((a >> 12) & 255) as i64;
                    let first = ((a & 255) >> 1) as i64 - n;
                    for i in 0..n {
                        let q = (b & MASK) as usize + i as usize * 16;
                        cache.insert(first + i, vec![i16_(model, q) as i64, i16_(model, q + 2) as i64, i16_(model, q + 4) as i64, i16_(model, q + 8) as i64, i16_(model, q + 10) as i64, i8_(model, q + 12) as i64, i8_(model, q + 13) as i64, i8_(model, q + 14) as i64, bone as i64]);
                    }
                }
                5 | 6 => {
                    let batch = batches.entry((mode.clone(), tex.clone(), color, wrap_s, wrap_t));
                    let words: &[u32] = if op == 5 { &[a] } else { &[a, b] };
                    for &w in words {
                        for shift in [16, 8, 0] {
                            let key = ((w >> shift & 255) / 2) as i64;
                            batch.push(ints(cache.get(&key).ok_or_else(|| format!("KeyError: {key}"))?));
                        }
                    }
                }
                0xde if b >> 24 != 12 => return Err(format!("Unsupported nested display list {b:x}")),
                _ => {}
            }
        }
    }

    let mut clips = Map::new();
    for (key, suffix) in BASE_CLIPS.iter().chain(EXTRA_CLIPS.iter()) {
        clips.insert(key.to_string(), link_frames(motion, animations, suffix)?);
    }
    let batches: Vec<Value> = batches
        .iter()
        .map(|((mode, tex, color, ws, wt), v)| json!({"mode": mode, "texture": tex, "color": color, "wrap": [ws, wt], "vertices": v}))
        .collect();
    Ok(json!({"limbs": limbs, "textures": textures, "clips": clips, "batches": batches}))
}

pub fn export_link(ctx: &Ctx, dir: &Path) -> R<()> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for (child, filename) in [(false, "link.json"), (true, "young-link.json")] {
        let bank = if child { "object_link_child" } else { "object_link_boy" };
        let (model, motion, keep) = (ctx.segment(bank)?, ctx.segment("link_animetion")?, ctx.segment("gameplay_keep")?);
        let (objects, animations, shared) = (ctx.object_defs(bank)?, ctx.defs("misc/link_animetion.xml")?, ctx.object_defs("gameplay_keep")?);
        let asset = build_link(&model, &keep, &motion, &objects, &shared, &animations, child)?;
        std::fs::write(dir.join(filename), dumps(&asset, None)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ items

const ITEMS: [(&str, &str, &str); 9] = [
    ("bomb_body", "gameplay_keep", "gBombBodyDL"),
    ("bomb_cap", "gameplay_keep", "gBombCapDL"),
    ("bombchu", "gameplay_keep", "gBombchuDL"),
    ("boomerang", "gameplay_keep", "gBoomerangDL"),
    ("arrow", "gameplay_keep", "gArrowNearDL"),
    ("hookshot_tip", "object_link_boy", "gLinkAdultHookshotTipDL"),
    ("hookshot_chain", "object_link_boy", "gLinkAdultHookshotChainDL"),
    ("deku_nut", "object_gi_nuts", "gGiNutDL"),
    ("deku_stick", "object_gi_stick", "gGiStickDL"),
];

struct ItemDl<'a> {
    segments: HashMap<u32, &'a [u8]>,
    specs: &'a SpecMap,
    blobs: [&'a [u8]; 2],
    textures: &'a mut Map<String, Value>,
    batches: &'a mut OrderedBatches<(Option<String>, u32, u32, u32)>,
    cache: HashMap<i64, Vec<i64>>,
    tex: Option<String>,
    color: u32,
    ws: u32,
    wt: u32,
}

impl ItemDl<'_> {
    fn run(&mut self, seg: u32, mut p: usize) -> R<()> {
        loop {
            let data = *self.segments.get(&seg).ok_or_else(|| format!("KeyError: {seg}"))?;
            let (a, b) = (u32_(data, p), u32_(data, p + 4));
            p += 8;
            let op = a >> 24;
            match op {
                0xdf => return Ok(()),
                0xde => {
                    let nseg = b >> 24;
                    if self.segments.contains_key(&nseg) {
                        self.run(nseg, (b & MASK) as usize)?;
                    }
                    if a & 0x0001_0000 != 0 {
                        return Ok(());
                    }
                }
                0xfd => {
                    if let Some((which, spec)) = self.specs.get(&b) {
                        if !is_tlut(spec) {
                            let name = spec.name().to_string();
                            if !self.textures.contains_key(&name) {
                                self.textures.insert(name.clone(), texture(self.blobs[*which], spec, true)?);
                            }
                            self.tex = Some(name);
                        }
                    }
                }
                0xf5 if b >> 24 == 0 => {
                    self.ws = (b >> 8) & 3;
                    self.wt = (b >> 18) & 3;
                }
                0xfa => self.color = b >> 8,
                1 => {
                    let n = ((a >> 12) & 255) as i64;
                    let first = ((a & 255) >> 1) as i64 - n;
                    let vseg = b >> 24;
                    let vd = *self.segments.get(&vseg).ok_or_else(|| format!("KeyError: {vseg}"))?;
                    for i in 0..n {
                        let q = (b & MASK) as usize + i as usize * 16;
                        self.cache.insert(first + i, vec![i16_(vd, q) as i64, i16_(vd, q + 2) as i64, i16_(vd, q + 4) as i64, i16_(vd, q + 8) as i64, i16_(vd, q + 10) as i64, i8_(vd, q + 12) as i64, i8_(vd, q + 13) as i64, i8_(vd, q + 14) as i64, 0]);
                    }
                }
                5 | 6 => {
                    let batch = self.batches.entry((self.tex.clone(), self.color, self.ws, self.wt));
                    let words: &[u32] = if op == 5 { &[a] } else { &[a, b] };
                    for &w in words {
                        for shift in [16, 8, 0] {
                            let key = ((w >> shift & 255) / 2) as i64;
                            batch.push(ints(self.cache.get(&key).ok_or_else(|| format!("KeyError: {key}"))?));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn export_items(ctx: &Ctx, dir: &Path) -> R<()> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let keep = ctx.segment("gameplay_keep")?;
    let keep_defs = ctx.object_defs("gameplay_keep")?;
    for (name, bank, dl) in ITEMS {
        let data = ctx.segment(bank)?;
        let defs = ctx.object_defs(bank)?;
        let specs = spec_map(&[(0x0400_0000, 0, &keep_defs), (0x0600_0000, 1, &defs)]);
        let mut textures = Map::new();
        let mut batches = OrderedBatches::new();
        let start = (if bank == "gameplay_keep" { 0x0400_0000u32 } else { 0x0600_0000 }) + defs.offset(dl)? as u32;
        {
            let mut s = ItemDl {
                segments: HashMap::from([(4, keep.as_slice()), (6, data.as_slice())]),
                specs: &specs,
                blobs: [keep.as_slice(), data.as_slice()],
                textures: &mut textures,
                batches: &mut batches,
                cache: HashMap::new(),
                tex: None,
                color: 0xff_ffff,
                ws: 0,
                wt: 0,
            };
            s.run(start >> 24, (start & MASK) as usize)?;
        }
        let b: Vec<Value> = batches.iter().map(|((tex, color, ws, wt), v)| json!({"mode": "base", "texture": tex, "color": color, "wrap": [ws, wt], "vertices": v})).collect();
        let asset = json!({"limbs": [{"pos": [0, 0, 0], "child": 255, "sibling": 255}], "textures": textures, "clips": {}, "batches": b});
        std::fs::write(dir.join(format!("{name}.json")), dumps(&asset, None)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ Epona

const LIMB_COUNT: usize = 47;
const EPONA_CLIPS: [(&str, &str); 9] = [
    ("idle", "gEponaIdleAnim"),
    ("whinny", "gEponaWhinnyAnim"),
    ("refuse", "gEponaRefuseAnim"),
    ("rearing", "gEponaRearingAnim"),
    ("walk", "gEponaWalkingAnim"),
    ("trot", "gEponaTrottingAnim"),
    ("gallop", "gEponaGallopingAnim"),
    ("jump", "gEponaJumpingAnim"),
    ("jumpHigh", "gEponaJumpingHighAnim"),
];
const RIDER: [(&str, &str); 19] = [
    ("stop", "uma_anim_stop"),
    ("stand", "uma_anim_stand"),
    ("walk", "uma_anim_walk"),
    ("slowrun", "uma_anim_slowrun"),
    ("fastrun", "uma_anim_fastrun"),
    ("jump100", "uma_anim_jump100"),
    ("jump200", "uma_anim_jump200"),
    ("walkMuti", "uma_anim_walk_muti"),
    ("slowrunMuti", "uma_anim_slowrun_muti"),
    ("fastrunMuti", "uma_anim_fastrun_muti"),
    ("stopMuti", "uma_stop_muti"),
    ("walk2", "uma_anim_walk2"),
    ("wait1", "uma_wait_1"),
    ("wait2", "uma_wait_2"),
    ("wait3", "uma_wait_3"),
    ("mountLeft", "uma_left_up"),
    ("mountRight", "uma_right_up"),
    ("dismountLeft", "uma_left_down"),
    ("dismountRight", "uma_right_down"),
];

struct EponaLimb {
    pos: [i16; 3],
    child: u8,
    sibling: u8,
    kind: i32,
    seg: usize,
}

type SkinPts = Vec<[i64; 5]>;
type SkinRec = (i64, i64, [i64; 3], i64, SkinPts);

pub fn export_epona(ctx: &Ctx, path: &Path) -> R<()> {
    let m = ctx.segment("object_horse")?;
    let motion = ctx.segment("link_animetion")?;
    let objects = ctx.object_defs("object_horse")?;
    let animations = ctx.defs("misc/link_animetion.xml")?;
    let m: &[u8] = &m;
    let base = (u32_(m, objects.offset("gEponaSkel")?) & MASK) as usize;
    let mut limbs = Vec::new();
    for i in 0..LIMB_COUNT {
        let p = (u32_(m, base + 4 * i) & MASK) as usize;
        limbs.push(EponaLimb { pos: [i16_(m, p), i16_(m, p + 2), i16_(m, p + 4)], child: u8_(m, p + 6), sibling: u8_(m, p + 7), kind: i32_(m, p + 8), seg: (u32_(m, p + 12) & MASK) as usize });
    }
    let mut specs: HashMap<u32, Attrs> = HashMap::new();
    for v in objects.values() {
        if v.has("Format") && v.has("Width") {
            specs.insert(0x0600_0000 + v.offset() as u32, v.clone());
        }
    }
    let by_name = |n: &str| -> R<Attrs> { Ok(objects.values().find(|v| v.name() == n).ok_or_else(|| format!("KeyError: {n}"))?.clone()) };
    specs.insert(0x0800_0000, by_name("gEponaEyeOpenTex")?);
    let animated: Vec<usize> = limbs.iter().enumerate().filter(|(_, l)| l.kind == 4).map(|(i, _)| i).collect();
    if animated.len() != 1 {
        return Err("AssertionError: one animated limb".into());
    }
    let skin_limb = animated[0];
    // skin_vertices
    let l = &limbs[skin_limb];
    let (count, modifs) = (u16_(m, l.seg + 2) as usize, (u32_(m, l.seg + 4) & MASK) as usize);
    let mut skin: HashMap<usize, SkinRec> = HashMap::new();
    for i in 0..count {
        let q = modifs + 16 * i;
        let (vtx_count, trans_count, hub, sv, lt) = (u16_(m, q) as usize, u16_(m, q + 2) as usize, u16_(m, q + 4) as usize, (u32_(m, q + 8) & MASK) as usize, (u32_(m, q + 12) & MASK) as usize);
        let trans: Vec<[i64; 5]> = (0..trans_count).map(|k| [u8_(m, lt + 10 * k) as i64, i16_(m, lt + 10 * k + 2) as i64, i16_(m, lt + 10 * k + 4) as i64, i16_(m, lt + 10 * k + 6) as i64, u8_(m, lt + 10 * k + 8) as i64]).collect();
        let pts: SkinPts = if trans_count == 1 { vec![[trans[0][0], trans[0][1], trans[0][2], trans[0][3], 100]] } else { trans.clone() };
        for k in 0..vtx_count {
            let r = sv + 10 * k;
            let index = u16_(m, r) as usize;
            skin.insert(index, (i16_(m, r + 2) as i64, i16_(m, r + 4) as i64, [i8_(m, r + 6) as i64, i8_(m, r + 7) as i64, i8_(m, r + 8) as i64], trans[hub][0], pts.clone()));
        }
    }
    let mut dl_addrs = Vec::new();
    for (i, l) in limbs.iter().enumerate() {
        if l.kind == 11 {
            dl_addrs.push((i, l.seg));
        } else if l.kind == 4 {
            dl_addrs.push((i, (u32_(m, l.seg + 8) & MASK) as usize));
        }
    }
    let mut textures = Map::new();
    let mut batches: OrderedBatches<(Option<String>, u32, u32, u32)> = OrderedBatches::new();
    for (limb, dl) in dl_addrs {
        let mut st = EponaState { tex: None, color: 0xff_ffff, ws: 0, wt: 0, cache: HashMap::new() };
        epona_run(m, &specs, &skin, limb, dl, &mut st, &mut textures, &mut batches)?;
    }
    for eye in ["gEponaEyeHalfTex", "gEponaEyeClosedTex"] {
        textures.insert(eye.to_string(), texture(m, &by_name(eye)?, false)?);
    }
    let mut clips = Map::new();
    for (k, name) in EPONA_CLIPS {
        let p = objects.offset(name)?;
        let frames = i16_(m, p) as i64;
        let data = (u32_(m, p + 4) & MASK) as usize;
        let joints = (u32_(m, p + 8) & MASK) as usize;
        let static_max = u16_(m, p + 12) as usize;
        let mut out = Vec::new();
        for f in 0..frames.max(0) as usize {
            let mut row = Vec::new();
            for j in 0..LIMB_COUNT + 1 {
                for axis in 0..3 {
                    let idx = u16_(m, joints + 6 * j + 2 * axis) as usize;
                    row.push(Value::from(i16_(m, data + 2 * (if idx < static_max { idx } else { idx + f }))));
                }
            }
            out.push(Value::Array(row));
        }
        clips.insert(k.to_string(), Value::Array(out));
    }
    let mut rider = Map::new();
    for (key, suffix) in RIDER {
        rider.insert(key.to_string(), link_frames(&motion, &animations, suffix)?);
    }
    let limbs_json: Vec<Value> = limbs.iter().map(|l| json!({"pos": l.pos, "child": l.child, "sibling": l.sibling})).collect();
    let b: Vec<Value> = batches.iter().map(|((tex, color, ws, wt), v)| json!({"texture": tex, "color": color, "wrap": [ws, wt], "vertices": v})).collect();
    let asset = json!({"limbs": limbs_json, "skinLimb": skin_limb, "textures": textures, "clips": clips, "riderClips": rider, "batches": b});
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(path, dumps(&asset, None)).map_err(|e| e.to_string())
}

struct EponaState {
    tex: Option<String>,
    color: u32,
    ws: u32,
    wt: u32,
    cache: HashMap<i64, Vec<i64>>,
}

#[allow(clippy::too_many_arguments)]
fn epona_run(m: &[u8], specs: &HashMap<u32, Attrs>, skin: &HashMap<usize, SkinRec>, limb: usize, mut p: usize, st: &mut EponaState, textures: &mut Map<String, Value>, batches: &mut OrderedBatches<(Option<String>, u32, u32, u32)>) -> R<()> {
    loop {
        let (a, b) = (u32_(m, p), u32_(m, p + 4));
        p += 8;
        let op = a >> 24;
        match op {
            0xdf => return Ok(()),
            0xde => {
                epona_run(m, specs, skin, limb, (b & MASK) as usize, st, textures, batches)?;
                if (a >> 16) & 0xff != 0 {
                    return Ok(());
                }
            }
            0xfd => {
                let s = specs.get(&b).ok_or_else(|| format!("KeyError: {b:#x}"))?;
                if !is_tlut(s) {
                    let name = s.name().to_string();
                    if !textures.contains_key(&name) {
                        textures.insert(name.clone(), texture(m, s, false)?);
                    }
                    st.tex = Some(name);
                }
            }
            0xf5 if b >> 24 == 0 => {
                st.ws = (b >> 8) & 3;
                st.wt = (b >> 18) & 3;
            }
            0xfa => st.color = b >> 8,
            1 => {
                let n = ((a >> 12) & 255) as i64;
                let first = ((a & 255) >> 1) as i64 - n;
                let seg = b >> 24;
                let addr = (b & MASK) as usize;
                for k in 0..n as usize {
                    let rec = if seg == 8 {
                        let idx = addr / 16 + k;
                        let (s, t, nrm, nlimb, pts) = skin.get(&idx).ok_or_else(|| format!("KeyError: {idx}"))?;
                        let mut rec = vec![*s, *t, nrm[0], nrm[1], nrm[2], *nlimb, pts.len() as i64];
                        for pt in pts {
                            rec.extend_from_slice(pt);
                        }
                        rec
                    } else {
                        let q = addr + 16 * k;
                        vec![i16_(m, q + 8) as i64, i16_(m, q + 10) as i64, i8_(m, q + 12) as i64, i8_(m, q + 13) as i64, i8_(m, q + 14) as i64, limb as i64, 1, limb as i64, i16_(m, q) as i64, i16_(m, q + 2) as i64, i16_(m, q + 4) as i64, 100]
                    };
                    st.cache.insert(first + k as i64, rec);
                }
            }
            5 | 6 | 7 => {
                let batch = batches.entry((st.tex.clone(), st.color, st.ws, st.wt));
                let words: &[u32] = if op == 5 { &[a] } else { &[a, b] };
                for &w in words {
                    for shift in [16, 8, 0] {
                        let key = ((w >> shift & 255) / 2) as i64;
                        batch.push(ints(st.cache.get(&key).ok_or_else(|| format!("KeyError: {key}"))?));
                    }
                }
            }
            _ => {}
        }
    }
}
