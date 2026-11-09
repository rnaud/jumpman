//! Port of `scripts/export-oot-courses.py`: scene collision, room meshes, props, residents, dialogue and the per-course
//! JSON (`assets/courses/<course>.json`, `assets/levels/<course>/terrain.glb`, ...).
use super::model::*;
use super::scenery::{self, Prop};
use super::text::{export_message_art, Messages};
use super::*;
use crate::pyc::{dumps, re, round};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

type Obj = Map<String, Value>;

pub const EXPLORATION: [(&str, &str, usize); 22] = [
    ("hyrule_field", "spot00", 0),
    ("kakariko_village", "spot01", 0),
    ("graveyard", "spot02", 0),
    ("lost_woods", "spot10", 0),
    ("sacred_forest_meadow", "spot05", 0),
    ("zoras_river", "spot03", 0),
    ("zoras_domain", "spot07", 0),
    ("zoras_fountain", "spot08", 5),
    ("lake_hylia", "spot06", 0),
    ("gerudo_valley", "spot09", 0),
    ("gerudos_fortress", "spot12", 0),
    ("haunted_wasteland", "spot13", 0),
    ("desert_colossus", "spot11", 0),
    ("hyrule_castle", "spot15", 0),
    ("death_mountain_trail", "spot16", 0),
    ("death_mountain_crater", "spot17", 0),
    ("goron_city", "spot18", 0),
    ("hyrule_market", "market_day", 0),
    ("temple_of_time", "tokinoma", 0),
    ("dodongos_cavern", "ddan", 0),
    ("jabu_jabu", "bdan", 0),
    ("forest_temple", "Bmori1", 0),
];

/// `COURSES` in dict order: (course, scene, spawn index).
pub fn courses() -> Vec<(&'static str, &'static str, usize)> {
    let mut v = vec![
        ("kokiri_forest", "spot04", 0),
        ("deku_tree", "ydan", 0),
        ("deku_tree.2", "ydan_boss", 0),
        ("lon_lon_ranch", "spot20", 3),
        ("links_house", "link_home", 0),
        ("kokiri_shop", "kokiri_shop", 0),
        ("know_it_all_house", "kokiri_home", 0),
        ("twins_house", "kokiri_home3", 0),
        ("midos_house", "kokiri_home4", 0),
        ("sarias_house", "kokiri_home5", 0),
    ];
    v.extend(EXPLORATION);
    v
}

fn is_overworld(course: &str) -> bool {
    EXPLORATION.iter().any(|e| e.0 == course) || course == "kokiri_forest" || course == "lon_lon_ranch"
}

fn day_texture(scene: &str) -> Option<&'static str> {
    Some(match scene {
        "spot01" => "gKakarikoVillageDayWindowTex",
        "spot07" => "gZorasDomainDayEntranceTex",
        "spot12" => "gGerudoFortressDayWallTex",
        "spot18" => "gGoronCityDayEntranceTex",
        "spot20" => "gLonLonRanchDayWindowTex",
        _ => return None,
    })
}

const FIELD_SUN: [f64; 3] = [60.0, 104.0, 17.0];

/// `overworld_entrances()`
fn overworld_entrances(ctx: &Ctx) -> R<HashMap<i64, (String, String)>> {
    let table = ctx.read_decomp("include/tables/entrance_table.h")?;
    let mut m = HashMap::new();
    for c in re(r"/\* (0x[0-9A-Fa-f]+) \*/ DEFINE_ENTRANCE\([^,]+, SCENE_(\w+), (\d+),").captures_iter(&table) {
        let scene = c[2].to_lowercase();
        if is_overworld(&scene) {
            m.insert(hex(&c[1]) as i64, (scene, format!("spawn_{}", &c[3])));
        }
    }
    Ok(m)
}

// (kind name, anim, first text, repeat text, pose) of EnKo
const KOKIRI_CHILDREN: [(&str, u16, u16, Option<i64>); 13] = [
    ("LiftingRock", 0x1004, 0x1004, None),
    ("StandUp", 0x1005, 0x1006, Some(0)),
    ("Punching", 0x1007, 0x1007, None),
    ("Blocking", 0x1008, 0x1009, None),
    ("CuttingGrass", 0x100A, 0x100B, None),
    ("StandUp", 0x100C, 0x100D, Some(1)),
    ("StandUp", 0x100E, 0x1019, Some(2)),
    ("Standing", 0x1035, 0x1035, None),
    ("SittingArmsUp", 0x1038, 0x1038, None),
    ("StandUp", 0x103C, 0x103C, Some(1)),
    ("StandUp", 0x103D, 0x103D, Some(0)),
    ("Sitting", 0x103E, 0x103E, None),
    ("Idle", 0x10D7, 0x10D8, None),
];

fn day_residents(course: &str) -> &'static [i64] {
    match course {
        "kokiri_forest" => &[0, 1, 2, 3, 4, 5, 6, 12],
        "twins_house" => &[9],
        "know_it_all_house" => &[7, 8, 11],
        "kokiri_shop" => &[10],
        _ => &[],
    }
}

/// `world_prop(actor, param)`: the interactive prop `jumpman_sim::link::props` simulates.
fn world_prop(actor: i64, param: i64) -> Option<Obj> {
    let flag = |shift: u32, bits: u32| (param >> shift) & ((1 << bits) - 1);
    let mut m = Obj::new();
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_string(), v);
    };
    match actor {
        0x125 => {
            if param & 3 == 2 {
                return None;
            }
            put("k", "grass".into());
            put("table", flag(8, 4).into());
        }
        0x111 => {
            let item = param & 0x1f;
            put("k", "pot".into());
            if item < 0x1a {
                put("item", item.into());
            }
        }
        0x1a0 | 0x110 => put("k", "crate".into()),
        0x14e => {
            if param & 1 != 0 {
                put("k", "large_rock".into());
            } else {
                put("k", "rock".into());
                put("table", flag(8, 4).into());
            }
        }
        0x127 => {
            put("k", "boulder".into());
            put("flag", flag(0, 6).into());
        }
        0x4c => put("k", "bomb_flower".into()),
        0x1d2 => {
            put("k", "hammer_rock".into());
            put("flag", flag(0, 6).into());
        }
        0x59 => {
            put("k", "cracked_wall".into());
            put("flag", flag(0, 6).into());
            put("wall", flag(13, 2).into());
        }
        0x5e => {
            put("k", "torch".into());
            put("flag", flag(0, 6).into());
            put("count", flag(6, 4).into());
            put("timed", i64::from(flag(6, 4) != 0 && flag(10, 1) == 0).into());
            put("lit", flag(10, 1).into());
            put("type", flag(12, 4).into());
        }
        0xf => {
            put("k", if flag(12, 4) == 1 { "wall_web" } else { "web" }.into());
            put("flag", flag(0, 6).into());
            put("burn", flag(6, 6).into());
        }
        0x12a => {
            let kind = param & 7;
            if kind > 4 {
                return None;
            }
            put("k", ["floor_switch", "rusty_switch", "eye_switch", "crystal_switch", "crystal_switch"][kind as usize].into());
            put("flag", flag(8, 6).into());
            put("sub", flag(4, 3).into());
        }
        _ => return None,
    }
    Some(m)
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = hypot3(v);
    let length = if length == 0.0 { 1.0 } else { length };
    [v[0] / length, v[1] / length, v[2] / length]
}

/// Collision `table(offset, ...)` header: (count, pointer).
fn tbl(data: &[u8], header: usize) -> (usize, usize) {
    (u16_(data, header) as usize, (u32_(data, header + 4) & MASK) as usize)
}

struct St<'a> {
    ctx: &'a Ctx,
    assets: &'a Path,
    course: &'a str,
    scene: Seg,
    cx: f64,
    cz: f64,
    objects: Vec<Obj>,
    extra_collision: Map<String, Value>,
    scenery_cache: HashMap<(String, String, String, u64, String, bool), String>,
}

impl St<'_> {
    fn tf(&self, x: f64, y: f64, z: f64) -> [f64; 3] {
        [round_to((x - self.cx) * WORLD_SCALE, 2), round_to(y * WORLD_SCALE, 2), round_to((z - self.cz) * WORLD_SCALE, 2)]
    }

    fn object_collision(&self, bank: &str, symbol: &str, scale: f64) -> R<Value> {
        let data = self.ctx.segment(bank)?;
        let offset = self.ctx.object_defs(bank)?.offset(symbol)?;
        let (vc, vp) = tbl(&data, offset + 12);
        let vertices: Vec<Value> = (0..vc).map(|i| Value::Array((0..3).map(|k| Value::from(round_to(i16_(&data, vp + i * 6 + 2 * k) as f64 * scale * WORLD_SCALE, 3))).collect())).collect();
        let (tc, tp) = tbl(&data, offset + 20);
        let triangles: Vec<Value> = (0..tc).map(|i| json!([u16_(&data, tp + i * 16 + 2) & 8191, u16_(&data, tp + i * 16 + 4) & 8191, u16_(&data, tp + i * 16 + 6) & 8191, 0, 0])).collect();
        Ok(json!({"vertices": vertices, "triangles": triangles}))
    }

    fn write_model(&self, rel: &str, m: &mut SceneModel) -> R<()> {
        m.write(&self.assets.join(rel))
    }

    /// `scenery_asset(prop)` (cached): returns the model path relative to `assets/`.
    fn scenery_asset(&mut self, prop: &Prop) -> R<String> {
        if let Some(a) = self.scenery_cache.get(&prop.key()) {
            return Ok(a.clone());
        }
        let ctx = self.ctx;
        let asset;
        if !prop.bank.is_empty() {
            let bank = prop.bank.as_str();
            let specs = ctx.object_defs(bank)?;
            let slot: u32 = if bank.starts_with("gameplay_") { 5 } else { 6 };
            let (keep, field, bank_seg) = (ctx.segment("gameplay_keep")?, ctx.segment("gameplay_field_keep")?, ctx.segment(bank)?);
            let dangeon;
            let mjin;
            let mut segs: Segs = HashMap::from([(2, self.scene.as_slice()), (4, keep.as_slice()), (5, field.as_slice())]);
            segs.insert(slot, bank_seg.as_slice());
            if self.course == "deku_tree" || prop.name == "golden_torch" {
                dangeon = ctx.segment("gameplay_dangeon_keep")?;
                segs.insert(5, dangeon.as_slice());
            }
            if let Some(emblem) = prop.name.strip_prefix("warp_pad_") {
                mjin = ctx.segment(&format!("object_mjin_{emblem}"))?;
                segs.insert(8, mjin.as_slice());
            }
            let mut pointers: Vec<u32> = Vec::new();
            for symbol in prop.lists.split_whitespace() {
                pointers.push((slot << 24) + specs.offset(symbol)? as u32);
            }
            let mut owned;
            if prop.name == "river_waterfall" {
                let start = bank_seg.len();
                let (material, triangles, spray) = (pointers[0], pointers[1], pointers[2]);
                owned = bank_seg.to_vec();
                for w in [0xde00_0000u32, material, 0x0101_9032, 0x0600_0800, 0xde00_0000, triangles, 0xdf00_0000, 0] {
                    owned.extend_from_slice(&w.to_be_bytes());
                }
                segs.insert(slot, owned.as_slice());
                pointers = vec![(slot << 24) + start as u32, spray];
            }
            let scale = prop.scale;
            let tr = move |x: f64, y: f64, z: f64| [x * scale * WORLD_SCALE, y * scale * WORLD_SCALE, z * scale * WORLD_SCALE];
            let mut model = SceneModel::new();
            for p in pointers {
                Mesh { name: bank, segments: &segs, transform: &tr, layer: u32::from(prop.translucent), joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }.into_model(&mut model, p)?;
            }
            asset = format!("levels/oot_scenery/{}.glb", prop.name);
            model.write(&self.assets.join(&asset))?;
        } else {
            asset = format!("levels/{}.glb", prop.name);
        }
        if !prop.collision.is_empty() {
            let bank = if prop.bank.is_empty() { "object_kibako2" } else { prop.bank.as_str() };
            let c = self.object_collision(bank, &prop.collision, prop.scale)?;
            self.extra_collision.insert(prop.name.clone(), c);
        }
        self.scenery_cache.insert(prop.key(), asset.clone());
        Ok(asset)
    }

    fn place_prop(&mut self, prop: &Prop, x: f64, y: f64, z: f64, yaw: i64, extra: Vec<(&str, Value)>) -> R<()> {
        let model = self.scenery_asset(prop)?;
        let mut obj = Obj::new();
        obj.insert("model".into(), model.into());
        obj.insert("pos".into(), json!(self.tf(x, y, z)));
        obj.insert("yaw".into(), Value::from((yaw * 180) as f64 / 32768.0));
        for (k, v) in extra {
            obj.insert(k.into(), v);
        }
        if !prop.collision.is_empty() {
            obj.insert("collision".into(), prop.name.clone().into());
        }
        self.objects.push(obj);
        Ok(())
    }
}

fn obj(pairs: Vec<(&str, Value)>) -> Obj {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn yawf(angle: i64) -> Value {
    Value::from((angle * 180) as f64 / 32768.0)
}

fn lambda_scale(s: f64) -> impl Fn(f64, f64, f64) -> [f64; 3] {
    move |x, y, z| [x * s * WORLD_SCALE, y * s * WORLD_SCALE, z * s * WORLD_SCALE]
}

/// Standalone `display_mesh` of a list of pointers into a fresh model, written under `assets/`.
fn glb(assets: &Path, rel: &str, name: &str, segs: &Segs, pointers: &[u32], tr: &Transform, layer: u32) -> R<()> {
    let mut m = SceneModel::new();
    for &p in pointers {
        Mesh { name, segments: segs, transform: tr, layer, joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }.into_model(&mut m, p)?;
    }
    m.write(&assets.join(rel))
}

enum PropSym {
    Name(&'static str),
    Off(usize),
}

pub fn export_all(ctx: &Ctx, stage: &Path, progress: &mut Progress) -> R<()> {
    let assets = stage.join("assets");
    std::fs::create_dir_all(assets.join("courses")).map_err(|e| e.to_string())?;
    let list = courses();
    let n = list.len();
    let overworld = overworld_entrances(ctx)?;
    let only: Vec<String> = std::env::var("BYOR_OOT_ONLY").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    for (i, (course, scene, spawn)) in list.into_iter().enumerate() {
        if !only.is_empty() && !only.iter().any(|o| o == course) {
            continue;
        }
        progress(30 + (66 * i / n) as u32, "Exporting Kokiri Forest, Deku Tree and Lon Lon Ranch");
        export(ctx, &assets, &overworld, course, scene, spawn)?;
    }
    Ok(())
}

fn export(ctx: &Ctx, assets: &Path, connections: &HashMap<i64, (String, String)>, course: &str, scene_name: &str, spawn_index: usize) -> R<()> {
    let scene = ctx.segment(&format!("{scene_name}_scene"))?;
    let header = commands(&scene)?;
    let hd = |op: u8| -> R<(u8, u32)> { header.get(&op).copied().ok_or_else(|| format!("KeyError: {op}")) };
    let col = hd(3)?.1 as usize;
    let (xmin, zmin, xmax, zmax) = (i16_(&scene, col) as i64, i16_(&scene, col + 4) as i64, i16_(&scene, col + 6) as i64, i16_(&scene, col + 10) as i64);
    let boundary = round(WORLD_SCALE * (xmax - xmin).max(zmax - zmin) as f64 / 2.0) + 512;
    let mut st = St { ctx, assets, course, scene: scene.clone(), cx: (xmin + xmax) as f64 / 2.0, cz: (zmin + zmax) as f64 / 2.0, objects: vec![], extra_collision: Map::new(), scenery_cache: HashMap::new() };
    let scene: &[u8] = &scene;
    let (vcount, vptr) = tbl(scene, col + 12);
    let vertices: Vec<Value> = (0..vcount).map(|i| json!(st.tf(i16_(scene, vptr + i * 6) as f64, i16_(scene, vptr + i * 6 + 2) as f64, i16_(scene, vptr + i * 6 + 4) as f64))).collect();
    let (tcount, tptr) = tbl(scene, col + 20);
    let polys: Vec<(usize, u16, u16, u16)> = (0..tcount).map(|i| (u16_(scene, tptr + i * 16) as usize, u16_(scene, tptr + i * 16 + 2), u16_(scene, tptr + i * 16 + 4), u16_(scene, tptr + i * 16 + 6))).collect();
    let solid: Vec<_> = polys.iter().filter(|p| p.1 & 0x4000 == 0).collect();
    let triangles: Vec<Value> = solid.iter().map(|p| json!([p.1 & 8191, p.2 & 8191, p.3 & 8191, 0, 0])).collect();
    let surface_types = (u32_(scene, col + 28) & MASK) as usize;
    let hookable: Vec<Value> = solid.iter().enumerate().filter(|(_, p)| (u32_(scene, surface_types + 8 * p.0 + 4) >> 17) & 1 != 0).map(|(i, _)| i.into()).collect();
    let mut water: Vec<Vec<Value>> = Vec::new();
    let (wc, wp) = tbl(scene, col + 36);
    for i in 0..wc {
        let q = wp + i * 16;
        let (x, y, z, dx, dz) = (i16_(scene, q) as f64, i16_(scene, q + 2) as f64, i16_(scene, q + 4) as f64, i16_(scene, q + 6) as f64, i16_(scene, q + 8) as f64);
        let [x1, level, z1] = st.tf(x, y, z);
        let [x2, _, z2] = st.tf(x + dx, y, z + dz);
        water.push(vec![0.into(), x1.into(), z1.into(), x2.into(), z2.into(), level.into()]);
    }
    let starts = hd(0)?.1 as usize;
    let sp = starts + spawn_index * 16 + 2;
    let (mut x, mut y, mut z, mut yaw) = (i16_(scene, sp) as i64, i16_(scene, sp + 2) as i64, i16_(scene, sp + 4) as i64, i16_(scene, sp + 8) as i64);
    let mut model = SceneModel::new();
    let mut face_yaw: Value = Value::Null;
    let mut talks: Vec<Obj> = Vec::new();
    let mut exit_rooms: Map<String, Value> = Map::new();
    let mut arrivals: Map<String, Value> = Map::new();
    let mut sim_props: Vec<Value> = Vec::new();
    let mut rupees: Vec<Value> = Vec::new();
    let mut treasure_chests: Vec<Value> = Vec::new();
    let mut warp_pad: Option<Value> = None;
    let mut quest: Obj = Obj::new();
    let msgs = Messages::new(ctx)?;
    let mut exported_residents: HashSet<String> = HashSet::new();
    let mut resident_positions: HashSet<(i64, [u64; 3])> = HashSet::new();
    let mut fixed: Value = Value::Null;
    let names = ctx.names().to_vec();
    let keep = ctx.segment("gameplay_keep")?;

    if course == "kokiri_forest" {
        let c = st.object_collision("object_box", "gTreasureChestCol", 0.01)?;
        st.extra_collision.insert("sword_chest".into(), c);
        let c = st.object_collision("object_spot04_objects", "gDekuTreeMouthCol", 1.0)?;
        st.extra_collision.insert("tree_mouth".into(), c);
        actor_model(ctx, &Actor { asset: "sword_chest", body: "object_box", skeleton: "gTreasureChestSkel".into(), animation: "gTreasureChestAnim_000128".into(), pose: Some(0), ..Default::default() }, assets)?;
        for (asset, bank, symbol, scale) in [("kokiri_sword", "object_gi_sword_1", "gGiKokiriSwordDL", 1.1), ("deku_tree_mouth", "object_spot04_objects", "gDekuTreeMouthDL", WORLD_SCALE)] {
            let dl = ctx.object_defs(bank)?.offset(symbol)?;
            let b = ctx.segment(bank)?;
            let segs: Segs = HashMap::from([(6, b.as_slice()), (4, keep.as_slice())]);
            glb(assets, &format!("levels/kokiri_forest/{asset}.glb"), bank, &segs, &[0x0600_0000 + dl as u32], &move |x, y, z| [x * scale, y * scale, z * scale], 0)?;
        }
        actor_model(ctx, &Actor { asset: "fairy", body: "gameplay_keep", skeleton: 0x16EE8usize.into(), animation: 0x15044usize.into(), ..Default::default() }, assets)?;
        actor_model(ctx, &Actor { asset: "saria", body: "object_sa", skeleton: "gSariaSkel".into(), animation: "gSariaWaitArmsToSideAnim".into(), ..Default::default() }, assets)?;
        let segs: Segs = HashMap::from([(4, keep.as_slice())]);
        glb(assets, "levels/kokiri_forest/sign.glb", "sign", &segs, &[0x0403_D560], &|x, y, z| [x * 0.044, y * 0.044, (z - 100.0) * 0.044], 0)?;
        for (idx, color) in ["green", "blue"].iter().enumerate() {
            let texture = 0x44F10 - (6 - idx) * 32;
            let segs: Segs = HashMap::from([(4, keep.as_slice()), (8, &keep[texture..])]);
            glb(assets, &format!("levels/kokiri_forest/rupee_{color}.glb"), color, &segs, &[0x0404_5150], &|x, y, z| [x * 0.015 * WORLD_SCALE, (y + 750.0) * 0.015 * WORLD_SCALE, z * 0.015 * WORLD_SCALE], 0)?;
        }
        export_message_art(ctx, assets)?;
    }
    if course == "kokiri_shop" {
        actor_model(ctx, &Actor { asset: "keeper", body: "object_km1", skeleton: "gKm1Skel".into(), animation: "object_masterkokiri_Anim_0004A8".into(), animation_bank: Some("object_masterkokiri"), head: Some("object_masterkokirihead"), ..Default::default() }, assets)?;
        let mut prop = SceneModel::new();
        let shop = ctx.segment("object_shop_dungen")?;
        let segs: Segs = HashMap::from([(6, shop.as_slice())]);
        Mesh { name: "shelves", segments: &segs, transform: &lambda_world(), layer: 0, joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }.into_model(&mut prop, 0x0600_0B80)?;
        let stock: [(&str, &str, i64, i64, i64); 8] = [
            ("shield_1", "gGiDekuShieldDL", 50, 52, -20),
            ("nuts", "gGiNutDL", 50, 76, -20),
            ("nuts", "gGiNutDL", 80, 52, -3),
            ("stick", "gGiStickDL", 80, 76, -3),
            ("seed", "gGiSeedDL", -50, 52, -20),
            ("arrow", "gGiArrowMediumDL", -50, 76, -20),
            ("arrow", "gGiArrowLargeDL", -80, 52, -3),
            ("heart", "gGiRecoveryHeartDL", -80, 76, -3),
        ];
        for (item, symbol, dx, dy, dz) in stock {
            let bank = format!("object_gi_{item}");
            let dl = ctx.object_defs(&bank)?.offset(symbol)?;
            let magnitude = 0x1554 as f64 * PI / 32768.0;
            let angle = if -(dx as f64) < 0.0 { -magnitude } else { magnitude };
            let (dxf, dyf, dzf) = (dx as f64, dy as f64, dz as f64);
            let on_shelf = move |x: f64, y: f64, z: f64| -> [f64; 3] {
                [WORLD_SCALE * (dxf + 0.25 * (x * libm::cos_(angle) + z * libm::sin_(angle))), WORLD_SCALE * (dyf + 0.25 * (y + 24.0)), WORLD_SCALE * (dzf + 0.25 * (z * libm::cos_(angle) - x * libm::sin_(angle)))]
            };
            let b = ctx.segment(&bank)?;
            let segs: Segs = HashMap::from([(6, b.as_slice()), (4, keep.as_slice())]);
            Mesh { name: &bank, segments: &segs, transform: &on_shelf, layer: 0, joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }.into_model(&mut prop, 0x0600_0000 + dl as u32)?;
        }
        prop.write(&assets.join("levels/kokiri_forest/shelves.glb"))?;
    }
    if course == "deku_tree" {
        let bank = "object_ydan_objects";
        let specs = ctx.object_defs(bank)?;
        let b = ctx.segment(bank)?;
        let segs: Segs = HashMap::from([(6, b.as_slice())]);
        glb(assets, "levels/deku_tree/web.glb", bank, &segs, &[0x0600_0000 + specs.offset("gDTWebFloorDL")? as u32], &lambda_scale(0.1), 1)?;
        let c = st.object_collision(bank, "gDTWebFloorCol", 0.1)?;
        st.extra_collision.insert("web".into(), c);
        let dungeon_prop = |asset: &str, bank: &str, symbol: &str, scale: f64, eye: bool| -> R<()> {
            let table = ctx.object_defs(bank)?;
            let slot: u32 = if bank == "gameplay_dangeon_keep" { 5 } else { 6 };
            let data = ctx.segment(bank)?;
            let mut segs: Segs = HashMap::from([(slot, data.as_slice()), (4, keep.as_slice())]);
            if asset == "push_block" {
                segs.insert(8, &data[table.offset("gPushBlockBaseTex")?..]);
            }
            if eye {
                let eye_off = table.offset("gEyeSwitchGoldOpenTex")?;
                segs.insert(8, &data[eye_off..]);
                segs.insert(9, &data[eye_off..]);
            }
            glb(assets, &format!("levels/deku_tree/{asset}.glb"), bank, &segs, &[(slot << 24) + table.offset(symbol)? as u32], &lambda_scale(scale), 0)
        };
        dungeon_prop("ferry", bank, "gDTSlidingPlatformDL", 0.15, false)?;
        let c = st.object_collision(bank, "gDTSlidingPlatformCol", 0.15)?;
        st.extra_collision.insert("ferry".into(), c);
        dungeon_prop("eye", "gameplay_dangeon_keep", "gEyeSwitch1DL", 0.1, true)?;
        dungeon_prop("switch", "gameplay_dangeon_keep", "gFloorSwitch1DL", 0.1, false)?;
        dungeon_prop("slingshot", "object_gi_pachinko", "gGiSlingshotDL", 0.22, false)?;
        dungeon_prop("gate", bank, "gDTDungeonDoor1DL", 1.0, false)?;
        st.extra_collision.insert("deku_gate".into(), json!({"vertices": [[-176, 0, 0], [176, 0, 0], [176, 484, 0], [-176, 484, 0]], "triangles": [[0, 1, 2, 0, 0], [0, 2, 3, 0, 0], [2, 1, 0, 0, 0], [3, 2, 0, 0, 0]]}));
        dungeon_prop("push_block", "gameplay_dangeon_keep", "gPushBlockDL", 0.1, false)?;
        let c = st.object_collision("gameplay_dangeon_keep", "gPushBlockCol", 0.1)?;
        st.extra_collision.insert("push_block".into(), c);
        dungeon_prop("stick", "object_gi_stick", "gGiStickDL", 0.22, false)?;
        for (role, asset, pos, collider) in [("block", "push_block", (-1365.0, -880.0, 1200.0), Some("push_block")), ("lowerWeb", "web", (-635.0, -820.0, 0.0), Some("web")), ("bossDoor", "gate", (-896.0, -1880.0, -964.0), Some("deku_gate")), ("stick", "stick", (-465.0, -780.0, 430.0), None)] {
            st.objects.push(obj(vec![("model", format!("levels/deku_tree/{asset}.glb").into()), ("pos", json!(st.tf(pos.0, pos.1, pos.2))), ("yaw", if role == "bossDoor" { 23 } else { 0 }.into()), ("dungeonRole", role.into()), ("collision", collider.map_or(Value::Null, Value::from))]));
        }
        st.objects.push(obj(vec![("model", "levels/deku_tree/gate.glb".into()), ("pos", json!(st.tf(-335.0, -880.0, 960.0))), ("yaw", 90.into()), ("dungeonRole", "gate".into()), ("collision", "deku_gate".into())]));
    }
    if scene_name == "ydan_boss" {
        actor_model(ctx, &Actor { asset: "gohma", body: "object_goma", skeleton: 0x1C8F8usize.into(), animation: "gGohmaHangAnim".into(), output_folder: Some("deku_tree"), ..Default::default() }, assets)?;
        st.objects.push(obj(vec![("model", "levels/deku_tree/gohma.glb".into()), ("pos", json!(st.tf(-160.0, -300.0, -371.0))), ("yaw", 0.into()), ("dungeonRole", "gohma".into())]));
        let bank = "object_gi_hearts";
        let specs = ctx.object_defs(bank)?;
        let b = ctx.segment(bank)?;
        let segs: Segs = HashMap::from([(6, b.as_slice()), (4, keep.as_slice())]);
        let mut ptrs = Vec::new();
        for symbol in ["gGiHeartContainerDL", "gGiHeartBorderDL"] {
            ptrs.push(0x0600_0000 + specs.offset(symbol)? as u32);
        }
        glb(assets, "levels/deku_tree/heart_container.glb", bank, &segs, &ptrs, &|x, y, z| [x * 0.35 * WORLD_SCALE, y * 0.35 * WORLD_SCALE, z * 0.35 * WORLD_SCALE], 0)?;
        st.objects.push(obj(vec![("model", "levels/deku_tree/heart_container.glb".into()), ("pos", json!(st.tf(-160.0, -610.0, -371.0))), ("yaw", 0.into()), ("dungeonRole", "container".into())]));
        x = 150;
        y = -640;
        z = 300;
        yaw = -28000;
        let icons = ctx.segment("icon_item_24_static")?;
        for (name, offset) in [("emerald", 0x3600usize), ("heart", 0x6C00)] {
            std::fs::create_dir_all(assets.join("levels/deku_tree")).map_err(|e| e.to_string())?;
            std::fs::write(assets.join(format!("levels/deku_tree/{name}.png")), png(24, 24, &icons[offset..offset + 24 * 24 * 4])).map_err(|e| e.to_string())?;
        }
        let mut pages = msgs.get(0x1091)?;
        pages.extend(msgs.get(0x1093)?);
        pages.push(json!({"text": "You received the Kokiri Emerald!", "style": 3}));
        pages.extend(msgs.get(0x102A)?);
        std::fs::write(assets.join("levels/deku_tree/farewell.json"), dumps(&Value::Array(pages), None) + "\n").map_err(|e| e.to_string())?;
    }
    let (lcount, lights) = hd(15)?;
    let loff = lights as usize + if lcount > 1 && course != "deku_tree" { 22 } else { 0 };
    let rd3u = |o: usize| [u8_(scene, o) as i64, u8_(scene, o + 1) as i64, u8_(scene, o + 2) as i64];
    let rd3i = |o: usize| [i8_(scene, o) as i64, i8_(scene, o + 1) as i64, i8_(scene, o + 2) as i64];
    let (ambient, mut d1, c1, mut d2, c2) = (rd3u(loff), rd3i(loff + 3), rd3u(loff + 6), rd3i(loff + 9), rd3u(loff + 12));
    if d1.iter().all(|&v| v == 0) && d2.iter().all(|&v| v == 0) {
        d1 = FIELD_SUN.map(|v| v as i64);
        d2 = FIELD_SUN.map(|v| -v as i64);
    }
    let terrain_light: TerrainLight = (ambient, c1, c2, unit(d1.map(|v| v as f64)), unit(d2.map(|v| v as f64)));

    face_yaw = Value::from(yaw);

    // Kakariko's child-day actor setup
    let kak_props: Vec<(i64, &str, &str, PropSym, f64, Option<&str>)> = vec![
        (0x102, "windmill_sails", "object_spot01_objects", PropSym::Name("gKakarikoWindmillSailsDL"), 0.1, None),
        (0x103, "well_arch", "object_spot01_objects", PropSym::Name("gKakarikoWellArchDL"), 0.1, Some("gKakarikoWellArchCol")),
        (0x104, "well_water", "object_spot01_objects", PropSym::Name("gKakarikoWellWaterDL"), 0.1, None),
        (0x19D, "construction", "object_spot01_matoyab", PropSym::Name("gKakarikoConstructionSiteDL"), 0.1, Some("object_spot01_matoyab_col")),
        (0x100, "trail_gate", "object_spot01_matoyab", PropSym::Name("gKakarikoGuardGateDL"), 1.0, Some("gKakarikoGuardGateCol")),
        (0x111, "pot", "object_tsubo", PropSym::Name("object_tsubo_DL_0017C0"), 0.15, None),
        (0x1A0, "crate", "object_kibako2", PropSym::Name("gLargeCrateDL"), 0.1, Some("gLargeCrateCol")),
        (0x14E, "rock", "gameplay_field_keep", PropSym::Name("gFieldKakeraDL"), 0.1, None),
        (0x125, "shrub", "gameplay_field_keep", PropSym::Name("gFieldBushDL"), 0.4, None),
        (0x77, "tree", "object_wood02", PropSym::Name("object_wood02_DL_0078D0"), 1.0, None),
        (0x39, "arrow_sign", "gameplay_keep", PropSym::Off(0x3DAC0), 0.01, None),
        (9, "house_door", "gameplay_field_keep", PropSym::Name("gFieldDoorLeftDL"), 0.01, None),
    ];
    if course == "kakariko_village" {
        let field = ctx.segment("gameplay_field_keep")?;
        for (actor, asset, bank, symbol, scale, collider) in &kak_props {
            let specs = ctx.object_defs(bank)?;
            let slot: u32 = match *bank {
                "gameplay_keep" => 4,
                "gameplay_field_keep" => 5,
                _ => 6,
            };
            let bank_seg = ctx.segment(bank)?;
            let mut segs: Segs = HashMap::from([(2, scene), (4, keep.as_slice()), (5, field.as_slice())]);
            segs.insert(slot, bank_seg.as_slice());
            let actor = *actor;
            let local = move |x: f64, y: f64, z: f64| -> [f64; 3] {
                let (x, y, z) = if actor == 9 { (x - 2700.0, z, -y) } else { (x, y, z) };
                [x * scale * WORLD_SCALE, y * scale * WORLD_SCALE, z * scale * WORLD_SCALE]
            };
            let mut parts: Vec<usize> = vec![match symbol {
                PropSym::Name(n) => specs.offset(n)?,
                PropSym::Off(o) => *o,
            }];
            if actor == 0x77 {
                parts.push(specs.offset("object_wood02_DL_007968")?);
            }
            if actor == 9 {
                parts.push(specs.offset("gFieldDoorRightDL")?);
            }
            let mut prop = SceneModel::new();
            for dl in parts {
                Mesh { name: bank, segments: &segs, transform: &local, layer: u32::from(actor == 0x104), joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }.into_model(&mut prop, (slot << 24) + dl as u32)?;
            }
            prop.write(&assets.join(format!("levels/kakariko_village/{asset}.glb")))?;
            if let Some(c) = collider {
                let v = st.object_collision(bank, c, *scale)?;
                st.extra_collision.insert(asset.to_string(), v);
            }
        }
        let (count, doors) = hd(14)?;
        for i in 0..count as usize {
            let q = doors as usize + 16 * i + 4;
            let (actor, dx, dy, dz, angle) = (u16_(scene, q), i16_(scene, q + 2) as f64, i16_(scene, q + 4) as f64, i16_(scene, q + 6) as f64, i16_(scene, q + 8) as i64);
            if actor == 9 {
                st.objects.push(obj(vec![("model", "levels/kakariko_village/house_door.glb".into()), ("pos", json!(st.tf(dx, dy, dz))), ("yaw", yawf(angle))]));
            }
        }
    }

    if is_overworld(course) && course != "kakariko_village" && course != "kokiri_forest" {
        let (count, doors) = header.get(&14).copied().unwrap_or((0, 0));
        for i in 0..count as usize {
            let q = doors as usize + 16 * i + 4;
            let (actor, dx, dy, dz, angle) = (u16_(scene, q), i16_(scene, q + 2) as f64, i16_(scene, q + 4) as f64, i16_(scene, q + 6) as f64, i16_(scene, q + 8) as i64);
            if actor == 9 {
                st.place_prop(&Prop::new("kakariko_village/house_door"), dx, dy, dz, angle, vec![])?;
            }
        }
    }

    let in_exploration = EXPLORATION.iter().any(|e| e.0 == course);
    for name in &names {
        if !name.starts_with(&format!("{scene_name}_room_")) {
            continue;
        }
        let room_seg = ctx.segment(name)?;
        let room: &[u8] = &room_seg;
        let extra_tex;
        let mut segments: Segs = HashMap::from([(2, scene), (3, room)]);
        if scene_name == "ydan" {
            let specs = ctx.defs("scenes/dungeons/ydan.xml")?;
            segments.insert(8, &scene[specs.offset("gDekuTreeDayEntranceTex")?..]);
        }
        if let Some(sym) = day_texture(scene_name) {
            extra_tex = ctx.defs(&format!("scenes/overworld/{scene_name}.xml"))?;
            segments.insert(8, &scene[extra_tex.offset(sym)?..]);
        }
        let rh = commands(room)?;
        let shape = rh.get(&0x0a).ok_or("KeyError: 10")?.1 as usize;
        if room[shape] == 1 {
            let image = (u32_(room, shape + 8) & MASK) as usize;
            let end = room[image..].windows(2).position(|w| w == [0xff, 0xd9]).ok_or("ValueError: subsection not found")? + image + 2;
            let folder = assets.join("levels").join(course);
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            std::fs::write(folder.join("background.jpg"), &room[image..end]).map_err(|e| e.to_string())?;
            let cam_table = (u32_(scene, col + 32) & MASK) as usize;
            let cam = (u32_(scene, cam_table + 4) & MASK) as usize;
            let (ex, ey, ez, pitch, yaw_cam, fov) = (i16_(scene, cam), i16_(scene, cam + 2), i16_(scene, cam + 4), i16_(scene, cam + 6), i16_(scene, cam + 8), i16_(scene, cam + 12));
            fixed = json!({"eye": st.tf(ex as f64, ey as f64, ez as f64), "pitch": pitch, "yaw": yaw_cam, "fov": fov as f64 / 100.0});
        }
        {
            let tr = |x: f64, y: f64, z: f64| st.tf(x, y, z);
            room_mesh(&mut model, name, &segments, &tr, &terrain_light)?;
        }
        let (count, actor_list) = rh.get(&1).copied().unwrap_or((0, 0));
        let actor_list = actor_list as usize;
        let mut placements: Vec<[i64; 8]> = (0..count as usize).map(|i| std::array::from_fn(|k| i16_(room, actor_list + 16 * i + 2 * k) as i64)).collect();
        let mut i = 0;
        while i < placements.len() {
            let [actor, ax, ay, az, _, angle, _, param] = placements[i];
            let (axf, ayf, azf) = (ax as f64, ay as f64, az as f64);
            let mut pos = st.tf(axf, ayf, azf);
            if let Some(mut prop) = world_prop(actor, param & 0xffff) {
                let k = prop.shift_remove("k").unwrap();
                let mut o = Obj::new();
                o.insert("k".into(), k);
                o.insert("pos".into(), json!(pos));
                o.insert("yaw".into(), round_to((angle * 180) as f64 / 32768.0, 2).into());
                o.extend(prop);
                sim_props.push(Value::Object(o));
            }
            let idx = i;
            i += 1;
            let key = format!("{name}-{idx}");
            let h12 = |idx: usize| i16_(room, actor_list + 16 * idx + 12) as i64;
            if course == "deku_tree" {
                let mut role: Option<&str> = None;
                if name == "ydan_room_5" {
                    if actor == 0x50 && param & 255 == 0 {
                        role = Some("ferry");
                    }
                    if actor == 0x12a && param == 0x3e00 {
                        role = Some("switch");
                    }
                }
                if name == "ydan_room_4" && actor == 0x12a {
                    role = Some("eye");
                }
                if let Some(role) = role {
                    let mut o = obj(vec![("model", format!("levels/deku_tree/{role}.glb").into()), ("pos", json!(pos)), ("yaw", yawf(angle)), ("dungeonRole", role.into())]);
                    if role == "ferry" {
                        o.insert("collision".into(), "ferry".into());
                    }
                    st.objects.push(o);
                    continue;
                }
            }
            if course != "kakariko_village" {
                if actor == 0x26 {
                    if !st.extra_collision.contains_key("flagpole") {
                        actor_model(ctx, &Actor { asset: "flagpole", body: "object_hata", skeleton: "gFlagpoleSkel".into(), animation: "gFlagpoleFlapAnim".into(), model_scale: Some(1.0 / 75.0), output_folder: Some("oot_scenery"), ..Default::default() }, assets)?;
                        let c = st.object_collision("object_hata", "gFlagpoleCol", 1.0 / 75.0)?;
                        st.extra_collision.insert("flagpole".into(), c);
                    }
                    st.objects.push(obj(vec![("model", "levels/oot_scenery/flagpole.glb".into()), ("pos", json!(pos)), ("yaw", yawf(angle)), ("collision", "flagpole".into()), ("idle", true.into())]));
                    continue;
                }
                if actor == 0x15 && param & 255 == 3 && (course == "sarias_house" || course == "deku_tree") {
                    st.place_prop(&scenery::recovery_heart(), axf, ayf + 8.6, azf, angle, vec![("recoveryHeart", true.into())])?;
                    continue;
                }
                if actor == 0x77 && matches!(param & 255, 3 | 6 | 8 | 13 | 15) {
                    let kind = param & 255;
                    for (dx, dz, heading) in scenery::wood_offsets(kind, angle) {
                        let prop = scenery::prop_for(actor, param)?.ok_or("wood prop")?;
                        st.place_prop(&prop, axf + dx, ayf, azf + dz, heading, vec![("groundedProp", true.into()), ("actorScale", if kind == 15 { json!(1.5) } else { json!(1) })])?;
                    }
                    continue;
                }
                if actor == 0x151 && param & 3 < 3 {
                    let kind = param & 3;
                    let prop = scenery::prop_for(if kind == 2 { 0x14e } else { 0x125 }, 0)?.ok_or("cluster prop")?;
                    for (dx, dz) in scenery::cluster_offsets(kind) {
                        st.place_prop(&prop, axf + dx, ayf, azf + dz, 0, vec![("groundedProp", true.into())])?;
                    }
                    continue;
                }
                let prop = if (actor, course) != (0x141, "kokiri_forest") { scenery::prop_for(actor, param & 65535)? } else { None };
                if let Some(prop) = prop {
                    let mut extra: Vec<(&str, Value)> = Vec::new();
                    if actor == 0x77 {
                        extra.push(("actorScale", match param & 255 {
                            0 | 12 | 15 => json!(1.5),
                            2 => json!(0.6),
                            _ => json!(1),
                        }));
                    }
                    if actor == 0x102 {
                        extra.push(("spinZ", (100.0 * 20.0 * PI / 32768.0).into()));
                    }
                    if ["river_waterfall", "domain_falls_0", "domain_falls_1", "crater_smoke"].contains(&prop.name.as_str()) {
                        extra.push(("scrollT", (if actor == 0xbf { -0.2 } else { 0.32 }).into()));
                    }
                    if actor == 0xbf {
                        extra.push(("billboard", true.into()));
                    }
                    if matches!(actor, 0x125 | 0x14e | 0x127) {
                        extra.push(("groundedProp", true.into()));
                    }
                    let dy = match actor {
                        0x190 => 200,
                        0x1c4 => 180,
                        0x18d => {
                            if param & 1 != 0 {
                                -120
                            } else {
                                0
                            }
                        }
                        _ => 0,
                    };
                    st.place_prop(&prop, axf, (ay + dy) as f64, azf, angle, extra)?;
                    if actor == 0x6e {
                        warp_pad = Some(json!({"pos": pos, "course": course}));
                        arrivals.insert("warp-pad".into(), json!({"pos": pos, "faceYaw": angle}));
                    }
                    if actor == 0x4a {
                        let yaw_r = angle as f64 * PI / 32768.0;
                        // the Python rebinds its spawn `yaw` here, so the spawn's faceYaw becomes this float
                        face_yaw = Value::from(yaw_r);
                        for side in [-158.0f64, 158.0] {
                            st.place_prop(&scenery::drawbridge_chain(), axf + side * libm::cos_(yaw_r) + 400.0 * libm::sin_(yaw_r), ayf + 10.0, azf - side * libm::sin_(yaw_r) + 400.0 * libm::cos_(yaw_r), angle, vec![])?;
                        }
                    }
                    if actor == 0x39 || actor == 0x141 {
                        let text_id = if actor == 0x39 { 0x300 | (param >> 8) } else { 0x300 | param };
                        if (0x300..0x400).contains(&text_id) {
                            talks.push(obj(vec![("kind", "sign".into()), ("pos", json!(pos)), ("pages", Value::Array(msgs.greet(text_id as u16)?))]));
                        }
                    }
                    continue;
                }
                if actor == 10 && course != "kokiri_forest" {
                    let key = format!("{course}:{}", param & 31);
                    treasure_chests.push(json!({"key": key, "pos": pos, "item": (param >> 5) & 127}));
                    let small = matches!((param >> 12) & 15, 5..=8);
                    let collider = if small { "small_chest" } else { "large_chest" };
                    if !st.extra_collision.contains_key(collider) {
                        let c = st.object_collision("object_box", "gTreasureChestCol", if small { 0.005 } else { 0.01 })?;
                        st.extra_collision.insert(collider.into(), c);
                    }
                    st.objects.push(obj(vec![("model", "levels/kokiri_forest/sword_chest.glb".into()), ("pos", json!(pos)), ("collision", collider.into()), ("yaw", yawf(angle)), ("actorScale", if small { json!(0.5) } else { json!(1) }), ("treasureKey", key.into())]));
                    continue;
                }
            }
            if course == "kakariko_village" {
                if let Some((_, asset, _, _, _, collider)) = kak_props.iter().find(|p| p.0 == actor) {
                    if actor == 0x100 {
                        pos = st.tf(-89.0, ayf, -1375.0);
                    }
                    if actor == 0x104 {
                        pos = st.tf(axf, 52.0, azf);
                        water[0][5] = pos[1].into();
                    }
                    if actor == 0x19D {
                        let path = hd(13)?.1 as usize + 8 * ((param >> 8) & 255) as usize;
                        let points = (u32_(scene, path + 4) & MASK) as usize;
                        let (cx, cy, cz) = (i16_(scene, points) as i64, i16_(scene, points + 2) as i64, i16_(scene, points + 4) as i64);
                        placements.push([0x1BC, cx, cy, cz, 0, angle, 0, (param & 0xff00) | 1]);
                    }
                    let mut o = obj(vec![("model", format!("levels/kakariko_village/{asset}.glb").into()), ("pos", json!(pos)), ("yaw", yawf(angle))]);
                    if collider.is_some() {
                        o.insert("collision".into(), (*asset).into());
                    }
                    if actor == 0x102 {
                        o.insert("spinZ".into(), (100.0 * 20.0 * PI / 32768.0).into());
                    }
                    if actor == 0x39 {
                        talks.push(obj(vec![("kind", "sign".into()), ("pos", json!(pos)), ("pages", Value::Array(msgs.get((0x300 | (param >> 8)) as u16)?))]));
                    }
                    st.objects.push(o);
                    continue;
                }
                if actor == 0x141 {
                    st.objects.push(obj(vec![("model", "levels/kokiri_forest/sign.glb".into()), ("pos", json!(pos)), ("yaw", yawf(angle))]));
                    talks.push(obj(vec![("kind", "sign".into()), ("pos", json!(pos)), ("pages", Value::Array(msgs.get((param | 0x300) as u16)?))]));
                    continue;
                }
            }
            if in_exploration || course == "lon_lon_ranch" || course == "deku_tree" {
                if actor == 0x146 && course == "sacred_forest_meadow" {
                    st.objects.push(obj(vec![("model", "levels/kokiri_forest/saria.glb".into()), ("pos", json!(pos)), ("yaw", yawf(angle)), ("idle", true.into())]));
                    talks.push(obj(vec![("pos", json!(pos)), ("kind", "resident".into()), ("id", key.clone().into()), ("label", "Saria".into()), ("pages", Value::Array(msgs.greet(0x10AD)?))]));
                    continue;
                }
                if let Some(resident) = scenery::resident(actor).filter(|_| scenery::visible_actor(actor, param)) {
                    let placement = (actor, pos.map(f64::to_bits));
                    if resident_positions.contains(&placement) {
                        continue;
                    }
                    resident_positions.insert(placement);
                    let mut asset = resident.bank.to_string();
                    if actor == 0x1BC {
                        asset += &format!("_{}", param & 3);
                    }
                    if !exported_residents.contains(&asset) {
                        let r = Resident { scale: resident.scale, textures: resident.textures.iter().map(|s| s.to_string()).collect() };
                        let body = format!("object_{}", resident.bank);
                        actor_model(ctx, &Actor { asset: &asset, body: &body, skeleton: resident.skeleton.into(), animation: resident.idle.into(), animation_bank: if resident.motion_bank.is_empty() { None } else { Some(resident.motion_bank) }, resident: Some(&r), ..Default::default() }, assets)?;
                        exported_residents.insert(asset.clone());
                    }
                    let mut o = obj(vec![("model", format!("levels/oot_residents/{asset}.glb").into()), ("pos", json!(pos)), ("yaw", yawf(angle)), ("idle", true.into())]);
                    if actor == 0x13D {
                        o.insert("actorScale".into(), 5.into());
                    } else if actor == 0x1AE && matches!(param & 31, 0 | 4) {
                        o.insert("actorScale".into(), (if param & 31 == 0 { 2.6 } else { 1.5 }).into());
                    }
                    st.objects.push(o);
                    let greeting = scenery::greeting_for(actor, param, course)?;
                    let label = match actor {
                        0x124 => "Diving Game Host",
                        0x17B => "Pierre",
                        0x13D => "Medigoron",
                        _ => resident.label,
                    };
                    if greeting != 0 {
                        st.objects.last_mut().unwrap().insert("speaker".into(), key.clone().into());
                        let mut t = obj(vec![("pos", json!(pos)), ("kind", "resident".into()), ("id", key.clone().into()), ("label", label.into())]);
                        if actor == 0x14D {
                            t.insert("range".into(), 1150.into());
                            t.insert("height".into(), 2000.into());
                        }
                        t.insert("pages".into(), Value::Array(msgs.greet(greeting as u16)?));
                        talks.push(t);
                    }
                    continue;
                }
            }
            if course == "deku_tree" && name == "ydan_room_0" && actor == 15 {
                st.objects.push(obj(vec![("model", "levels/deku_tree/web.glb".into()), ("pos", json!(pos)), ("yaw", yawf(angle)), ("collision", "web".into())]));
            }
            if course == "kokiri_forest" && actor == 10 && (param >> 5) & 127 == 0x27 {
                let mut reward = msgs.get(0xA4)?;
                let first = reward[0]["text"].as_str().unwrap_or("").split('\n').next().unwrap_or("").to_string();
                reward[0]["text"] = first.into();
                quest.insert("chest".into(), json!(pos));
                quest.insert("reward".into(), Value::Array(reward));
                st.objects.push(obj(vec![("model", "levels/kokiri_forest/sword_chest.glb".into()), ("pos", json!(pos)), ("yaw", yawf(angle)), ("collision", "sword_chest".into())]));
                st.objects.push(obj(vec![("model", "levels/kokiri_forest/kokiri_sword.glb".into()), ("pos", json!(pos)), ("yaw", 0.into())]));
                continue;
            }
            if course == "kokiri_forest" && actor == 0x3e {
                st.objects.push(obj(vec![("model", "levels/kokiri_forest/deku_tree_mouth.glb".into()), ("pos", json!(st.tf(4029.0, 136.0, -1255.0))), ("yaw", 0.into())]));
            }
            if course == "kokiri_forest" && actor == 0x15 && matches!(param & 255, 0 | 1) {
                let blue = param & 255 == 1;
                let flag = (param >> 8) & 63;
                rupees.push(json!({"key": key, "pos": pos, "value": if blue { 5 } else { 1 }, "permanent": flag < 32}));
                st.objects.push(obj(vec![("model", format!("levels/kokiri_forest/rupee_{}.glb", if blue { "blue" } else { "green" }).into()), ("pos", json!(pos)), ("yaw", 0.into()), ("rupee", key.clone().into())]));
                continue;
            }
            if (course == "kokiri_forest" || course == "kokiri_shop") && actor == 0x112 && (param >> 11) & 31 == 2 {
                let drop = (param >> 6) & 31;
                if matches!(drop, 8 | 9 | 10) {
                    let base = match drop {
                        8 => 1,
                        9 => 5,
                        _ => 20,
                    };
                    let value = base * 1.max(h12(idx) & 255);
                    rupees.push(json!({"key": key, "pos": pos, "value": value, "hidden": true, "permanent": (param & 63) < 32}));
                }
                continue;
            }
            let asset: String;
            if actor == 0x163 && day_residents(course).contains(&(param & 255)) {
                let kid = param & 255;
                if kid == 7 && h12(idx) != 0 {
                    continue;
                }
                asset = format!("kokiri_{kid}");
                let girl = matches!(kid, 1 | 5 | 6 | 9 | 10 | 12);
                let body = if girl { "object_kw1" } else { "object_km1" };
                let (anim, first, repeat, pose) = KOKIRI_CHILDREN[kid as usize];
                let anim_sym = format!("gKokiri{anim}Anim");
                actor_model(ctx, &Actor { asset: &asset, body, skeleton: (if girl { "gKw1Skel" } else { "gKm1Skel" }).into(), animation: anim_sym.as_str().into(), animation_bank: Some("object_os_anime"), head: Some(if kid == 12 { "object_fa" } else { body }), pose, ..Default::default() }, assets)?;
                let label = if kid == 12 {
                    "Fado"
                } else if course == "know_it_all_house" {
                    "Know-It-All Brother"
                } else {
                    "Kokiri"
                };
                talks.push(obj(vec![("pos", json!(pos)), ("kind", "resident".into()), ("id", asset.clone().into()), ("label", label.into()), ("pages", Value::Array(msgs.get(first)?)), ("repeat", Value::Array(msgs.get(repeat)?))]));
            } else if course == "kokiri_forest" && actor == 0x16d {
                asset = "mido".into();
                let path = hd(13)?.1 as usize + 8 * ((param >> 8) & 255) as usize;
                let (length, points) = (u8_(scene, path) as usize, (u32_(scene, path + 4) & MASK) as usize);
                let pts: Vec<Value> = (0..length).map(|j| json!(st.tf(i16_(scene, points + 6 * j) as f64, i16_(scene, points + 6 * j + 2) as f64, i16_(scene, points + 6 * j + 4) as f64))).collect();
                quest.insert("path".into(), Value::Array(pts));
                quest.insert("approved".into(), Value::Array(msgs.get(0x1033)?));
                quest.insert("passed".into(), Value::Array(msgs.get(0x1034)?));
                actor_model(ctx, &Actor { asset: "mido", body: "object_md", skeleton: "gMidoSkel".into(), animation: "gMidoIdleAnim".into(), pose: Some(0), ..Default::default() }, assets)?;
                talks.push(obj(vec![("pos", json!(pos)), ("kind", "resident".into()), ("id", "mido".into()), ("label", "Mido".into()), ("pages", Value::Array(msgs.get(0x102f)?)), ("repeat", Value::Array(msgs.get(0x1030)?))]));
            } else if course == "kokiri_forest" && (actor == 0x141 || actor == 0x146) {
                let saria = actor == 0x146;
                asset = if saria { "saria" } else { "sign" }.into();
                talks.push(obj(vec![("pos", json!(pos)), ("kind", asset.clone().into()), ("pages", Value::Array(msgs.get(if saria { 0x1002 } else { (param | 0x300) as u16 })?))]));
            } else if course == "kokiri_shop" && (actor == 0x3d || actor == 0xc2) {
                asset = if actor == 0x3d { "keeper" } else { "shelves" }.into();
                if asset == "keeper" {
                    pos = st.tf(axf, ayf, azf + 33.0);
                    talks.push(obj(vec![("pos", json!(st.tf(axf, ayf, 35.0))), ("kind", "resident".into()), ("id", "keeper".into()), ("label", "Shopkeeper".into()), ("pages", Value::Array(msgs.get(0x9e)?))]));
                }
            } else {
                continue;
            }
            let decor = asset == "sign" || asset == "shelves";
            if !decor {
                let mut o = obj(vec![("model", "levels/kokiri_forest/fairy.glb".into()), ("pos", json!(pos)), ("yaw", 0.into()), ("idle", true.into()), ("fairy", true.into())]);
                if asset == "mido" {
                    o.insert("owner".into(), asset.clone().into());
                }
                st.objects.push(o);
            }
            st.objects.push(obj(vec![("model", format!("levels/kokiri_forest/{asset}.glb").into()), ("pos", json!(pos)), ("yaw", yawf(angle)), ("idle", (!decor).into())]));
        }
    }

    // Scene exit indices live on the floor polygons.
    let mut rooms: Vec<Value> = Vec::new();
    let conns: HashMap<i64, (String, String)> = if is_overworld(course) { connections.clone() } else { HashMap::new() };
    for p in &polys {
        if p.1 & 0x4000 != 0 {
            continue;
        }
        let idx = ((u32_(scene, surface_types + p.0 * 8) >> 8) & 31) as usize;
        rooms.push(idx.into());
        if idx != 0 {
            let entrance = u16_(scene, hd(0x13)?.1 as usize + 2 * (idx - 1)) as i64;
            let route: Option<(String, String)> = match entrance {
                0x000 => Some(("deku_tree", "door")),
                0x209 => Some(("kokiri_forest", "tree")),
                0x272 => Some(("links_house", "door")),
                0xc1 => Some(("kokiri_shop", "door")),
                0x211 => Some(("kokiri_forest", "house")),
                0x266 => Some(("kokiri_forest", "shop")),
                0xc9 => Some(("know_it_all_house", "door")),
                0x26a => Some(("kokiri_forest", "brothers")),
                0x9c => Some(("twins_house", "door")),
                0x33c => Some(("kokiri_forest", "twins")),
                0x433 => Some(("midos_house", "door")),
                0x443 => Some(("kokiri_forest", "mido")),
                0x437 => Some(("sarias_house", "door")),
                0x447 => Some(("kokiri_forest", "saria")),
                _ => None,
            }
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .or_else(|| conns.get(&entrance).cloned());
            if let Some((c, e)) = route {
                exit_rooms.insert(idx.to_string(), json!({"course": c, "entry": e}));
            }
        }
    }
    let arrival = |i: usize| -> Value {
        let q = starts + 16 * i + 2;
        json!({"pos": st.tf(i16_(scene, q) as f64, i16_(scene, q + 2) as f64, i16_(scene, q + 4) as f64), "faceYaw": i16_(scene, q + 8)})
    };
    if course == "kokiri_forest" {
        for (key, i) in [("tree", 1), ("house", 3), ("shop", 4), ("brothers", 5), ("twins", 8), ("mido", 9), ("saria", 10)] {
            arrivals.insert(key.into(), arrival(i));
        }
    }
    if is_overworld(course) {
        for i in 0..hd(0)?.0 as usize {
            arrivals.insert(format!("spawn_{i}"), arrival(i));
        }
    }
    if course == "links_house" {
        arrivals.insert("door".into(), arrival(1));
    }
    if course == "deku_tree" {
        let brothers = [json!(st.tf(-662.0, -1880.0, -957.0)), json!(st.tf(-311.0, -1880.0, -904.0)), json!(st.tf(-946.0, -1880.0, -767.0))];
        let scrub = json!(st.tf(-66.0, -880.0, 962.0));
        for o in st.objects.iter_mut() {
            let prop_name = o["model"].as_str().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
            if !o.contains_key("dungeonRole") {
                let role = match prop_name.as_str() {
                    "deku_ladder.glb" => Some("ladder"),
                    "web.glb" => Some("web"),
                    "deku_spike_log.glb" => Some("log"),
                    "collapsing_platform.glb" => Some("steppingStone"),
                    _ => None,
                };
                if let Some(r) = role {
                    o.insert("dungeonRole".into(), r.into());
                }
            }
            for (i, b) in brothers.iter().enumerate() {
                if prop_name == "hintnuts.glb" && o["pos"] == *b {
                    o.insert("dungeonRole".into(), format!("brother{}", i + 1).into());
                }
            }
            if prop_name == "hintnuts.glb" && o["pos"] == scrub {
                o.insert("dungeonRole".into(), "scrub".into());
            }
            if prop_name == "hintnuts.glb" && o.get("dungeonRole").is_some_and(|v| !v.is_null() && v.as_str() != Some("")) {
                let speaker = o.shift_remove("speaker");
                talks.retain(|t| t.get("id") != speaker.as_ref());
            }
        }
    }
    let path = format!("levels/{course}/terrain.glb");
    model.write(&assets.join(&path))?;
    let mut data = Obj::new();
    data.insert("model".into(), path.into());
    data.insert("props".into(), Value::Array(sim_props));
    data.insert("vertices".into(), Value::Array(vertices));
    data.insert("triangles".into(), Value::Array(triangles));
    data.insert("spawn".into(), json!({"pos": st.tf(x as f64, y as f64, z as f64), "faceYaw": face_yaw}));
    data.insert("waterBoxes".into(), Value::Array(water.into_iter().map(Value::Array).collect()));
    data.insert("horizontalBoundary".into(), boundary.into());
    data.insert("explorationOnly".into(), true.into());
    data.insert("objects".into(), Value::Array(st.objects.iter().cloned().map(Value::Object).collect()));
    data.insert("coins".into(), json!([]));
    data.insert("extraCollision".into(), Value::Object(st.extra_collision.clone()));
    data.insert("hookable".into(), Value::Array(hookable.clone()));
    let mut kokiri: Option<Obj> = None;
    if !exit_rooms.is_empty() || !talks.is_empty() || is_overworld(course) || scene_name == "ydan_boss" {
        data.insert("rooms".into(), Value::Array(rooms));
        kokiri = Some(obj(vec![("exits", Value::Object(exit_rooms)), ("talks", Value::Array(talks.into_iter().map(Value::Object).collect())), ("arrivals", Value::Object(arrivals)), ("fixed", fixed), ("rupees", Value::Array(rupees))]));
    }
    if !treasure_chests.is_empty() {
        kokiri.as_mut().ok_or("KeyError: kokiri")?.insert("treasureChests".into(), Value::Array(treasure_chests));
    }
    if let Some(w) = warp_pad {
        kokiri.as_mut().ok_or("KeyError: kokiri")?.insert("warpPad".into(), w);
    }
    if let Some(k) = kokiri.as_mut() {
        if ["deku_tree", "deku_tree.2", "zoras_domain", "goron_city"].contains(&course) {
            k.insert("enclosed".into(), true.into());
        }
        k.insert("light".into(), json!({"ambient": ambient, "key": c1, "keyDir": d1, "fill": c2, "fillDir": d2}));
        if course == "kokiri_shop" {
            k.insert("counter".into(), json!({"shield": st.tf(50.0, 0.0, 35.0), "heart": st.tf(-80.0, 0.0, 35.0)}));
        }
        if course == "kokiri_forest" {
            quest.insert("mouth".into(), json!({"closed": st.tf(4029.0, 136.0, -1255.0), "opened": st.tf(3869.0, -263.0, -1163.0), "meeting": st.tf(3827.0, -161.0, -1142.0), "eye": st.tf(3740.0, -141.0, -530.0), "lookFrom": st.tf(3777.0, -89.0, -605.0), "lookTo": st.tf(3763.0, -126.0, -621.0), "first": msgs.get(0x1015)?, "retry": msgs.get(0x1016)?}));
            quest.insert("crawl".into(), json!([st.tf(-785.0, 120.0, 1030.0), st.tf(-785.0, 120.0, 1408.0)]));
            k.insert("quest".into(), Value::Object(quest));
            k.insert("ladder".into(), json!([st.tf(-29.0, -80.0, 983.0), st.tf(-29.0, 100.0, 1037.0)]));
            k.insert("repeat".into(), Value::Array(msgs.get(0x1003)?));
            k.insert("greeting".into(), Value::Array(msgs.get(0x1001)?));
            k.insert("afterMido".into(), json!([msgs.get(0x1031)?, msgs.get(0x1032)?]));
        }
    }
    if let Some(k) = kokiri {
        data.insert("kokiri".into(), Value::Object(k));
    }
    std::fs::write(assets.join("courses").join(format!("{course}.json")), dumps(&Value::Object(data), None) + "\n").map_err(|e| e.to_string())
}

fn lambda_world() -> impl Fn(f64, f64, f64) -> [f64; 3] {
    |x, y, z| [x * WORLD_SCALE, y * WORLD_SCALE, z * WORLD_SCALE]
}
