//! Port of `scripts/export-courses.py` + `scripts/export_sm64_g2.py`: collision, placements and warps of every SM64 course
//! from the decomp sources into `assets/courses/*.json` (+ `.ext.json`, `sm64_warp_report.json`).
use crate::pyc::{args, clean, dumps, float, int, int0, ints, read, re, round, R};
use crate::sm64_world::{self as world, area_commands, World};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) type M = Map<String, Value>;

const LEVELS: [&str; 26] = world::LEGACY_LEVELS;
const SPAWN_BEHAVIORS: [&str; 12] = [
    "bhvFlyingWarp", "bhvSwimmingWarp", "bhvAirborneWarp", "bhvSpinAirborneCircleWarp", "bhvSpinAirborneWarp", "bhvPaintingStarCollectWarp", "bhvAirborneStarCollectWarp", "bhvLaunchStarCollectWarp", "bhvDeathWarp", "bhvPaintingDeathWarp", "bhvAirborneDeathWarp", "bhvLaunchDeathWarp",
];
const G2_LEVELS: [&str; 7] = ["lll", "ssl", "ddd", "sl", "wdw", "ttm", "thi"];

pub struct Courses {
    pub refd: PathBuf,
    /// where the GLB library lives (`<assets>/<model path>`), for the meshless-decor filter
    pub assets: PathBuf,
    pub(crate) surfaces: HashMap<String, i64>,
    pub(crate) params: HashMap<String, String>,
    world: World,
    pub(crate) g2_consts: std::sync::OnceLock<HashMap<String, i64>>,
    pub(crate) g2_symbols: std::sync::OnceLock<HashMap<String, PathBuf>>,
    pub(crate) g2_bhv: std::sync::OnceLock<HashMap<String, String>>,
}

pub(crate) fn i(v: &str) -> R<i64> {
    int(v)
}

pub(crate) fn p3(a: &[String], lo: usize) -> R<Value> {
    Ok(Value::Array(ints(&a[lo..lo + 3])?))
}

pub(crate) fn act_list(expr: &str) -> Value {
    Value::Array(re(r"ACT_(\d+)").captures_iter(expr).map(|c| json!(c[1].parse::<i64>().unwrap())).collect())
}

pub(crate) fn star_act(s: &str) -> R<i64> {
    Ok(re(r"STAR_INDEX_ACT_(\d+)").captures(s).ok_or_else(|| format!("no STAR_INDEX_ACT in {s}"))?[1].parse::<i64>().unwrap())
}

pub(crate) fn model_of<'a>(models: &'a HashMap<String, String>, k: &str) -> R<&'a String> {
    models.get(k).ok_or_else(|| format!("no model {k}"))
}

pub(crate) fn push(data: &mut M, key: &str, v: Value) {
    data.entry(key.to_string()).or_insert_with(|| json!([])).as_array_mut().unwrap().push(v);
}

pub(crate) fn obj(data: &mut M, v: Value) {
    data.get_mut("objects").unwrap().as_array_mut().unwrap().push(v);
}

fn us_version(text: &str) -> String {
    re(r"(?s)#ifdef VERSION_JP(.*?)(?:#else(.*?))?#endif").replace_all(text, |c: &regex::Captures| c.get(2).map_or("", |m| m.as_str()).to_string()).into_owned()
}

impl Courses {
    pub fn new(refd: &Path, assets: &Path) -> R<Courses> {
        let surfaces = re(r"#define\s+(SURFACE_\w+)\s+(0x[0-9A-Fa-f]+)").captures_iter(&read(&refd.join("include/surface_terrains.h"))?).map(|c| (c[1].to_string(), int0(&c[2]).unwrap())).collect();
        let params = re(r"#define\s+(\w+)\s+(-?(?:0x[0-9a-fA-F]+|\d+))\b").captures_iter(&read(&refd.join("include/object_constants.h"))?).map(|c| (c[1].to_string(), c[2].to_string())).collect();
        Ok(Courses { refd: refd.to_path_buf(), assets: assets.to_path_buf(), surfaces, params, world: World::new(refd), g2_consts: Default::default(), g2_symbols: Default::default(), g2_bhv: Default::default() })
    }

    pub fn collision(&self, path: &Path, symbol: Option<&str>) -> R<Value> {
        let mut text = us_version(&clean(&read(path)?));
        if let Some(s) = symbol {
            text = re(&format!(r"(?s)\b{s}\[\]\s*=\s*\{{(.*?)\}};")).captures(&text).ok_or_else(|| format!("{}: no {s}", path.display()))?[1].to_string();
        }
        let vertices: Vec<Value> = args(&text, "COL_VERTEX").iter().map(|a| ints(a).map(Value::Array)).collect::<R<_>>()?;
        let mut triangles = Vec::new();
        let mut surface = 0;
        for c in re(r"\b(COL_TRI_INIT|COL_TRI_SPECIAL|COL_TRI)\s*\(([^()]*)\)").captures_iter(&text) {
            let a: Vec<String> = c[2].split(',').map(|s| s.trim().to_string()).collect();
            if &c[1] == "COL_TRI_INIT" {
                surface = *self.surfaces.get(&a[0]).ok_or_else(|| format!("unknown surface {}", a[0]))?;
            } else {
                let idx: Vec<i64> = a[..3].iter().map(|s| i(s)).collect::<R<_>>()?;
                if !idx.iter().all(|x| *x >= 0 && (*x as usize) < vertices.len()) {
                    return Err(format!("{}: triangle index out of range", path.display()));
                }
                triangles.push(json!([idx[0], idx[1], idx[2], surface, if a.len() == 4 { int0(&a[3])? } else { 0 }]));
            }
        }
        if vertices.is_empty() || triangles.is_empty() {
            return Err(format!("{}: empty collision", path.display()));
        }
        let water: Vec<Value> = args(&text, "COL_WATER_BOX").iter().map(|b| b.iter().map(|v| int0(v).map(Value::from)).collect::<R<Vec<_>>>().map(Value::Array)).collect::<R<_>>()?;
        Ok(json!({"vertices":vertices,"triangles":triangles,"waterBoxes":water}))
    }

    pub(crate) fn sign_pages(&self, dialog: &str) -> R<Value> {
        let text = read(&self.refd.join("text/us/dialogs.h"))?;
        let m = re(&format!(r#"(?s)DEFINE_DIALOG\({dialog},\s*1,\s*(\d+),[^_]+_\("(.*?)"\)\)"#)).captures(&text).ok_or("dialog not found")?;
        let joined = m[2].replace("\\\n", "");
        let rows: Vec<&str> = joined.split("\\n").collect();
        let count: usize = m[1].parse().unwrap();
        Ok(Value::Array(rows.chunks(count).map(|c| json!(c.join("\n").trim_end())).collect()))
    }

    fn door_motion(&self, clip: &str) -> R<Value> {
        let source = read(&self.refd.join(format!("assets/anims/anim_{clip}.inc.c")))?;
        let words = |name: &str| -> R<Vec<i64>> {
            let body = re(&format!(r"(?s)_{name}\[\]\s*=\s*\{{(.*?)\}};")).captures(&source).ok_or("anim table")?[1].to_string();
            re(r"0x[\da-fA-F]+").find_iter(&body).map(|m| i64::from_str_radix(&m.as_str()[2..], 16).map_err(|e| e.to_string())).collect()
        };
        let (indices, values) = (words("indices")?, words("values")?);
        let component = |axis: usize, frame: i64| -> f64 {
            let (count, offset) = (indices[axis * 2], indices[axis * 2 + 1]);
            let word = values[(offset + (count - 1).min(frame)) as usize];
            (if word < 32768 { word } else { word - 65536 }) as f64 / 4.0
        };
        Ok(Value::Array((0..indices[0]).map(|f| json!([component(0, f), component(2, f)])).collect()))
    }

    fn castle_paintings(&self, folder: &Path, area: i64) -> R<Value> {
        let painting = clean(&read(&folder.join("painting.inc.c"))?);
        let geo = clean(&read(&folder.join(format!("areas/{area}/geo.inc.c")))?);
        let visible: Vec<i64> = re(r"PAINTING_ID\(\s*(\d+),\s*1\)").captures_iter(&geo).map(|c| c[1].parse().unwrap()).collect();
        let arrays: HashMap<String, String> = re(r"(?s)\b(\w+)\[\]\s*=\s*\{(.*?)\};").captures_iter(&painting).map(|c| (c[1].to_string(), c[2].to_string())).collect();
        let images: HashMap<String, String> = re(r#"(\w+)\[\]\s*=\s*\{\s*#include "levels/castle_inside/([^"]+)\.inc\.c""#).captures_iter(&read(&folder.join("texture.inc.c"))?).map(|c| (c[1].to_string(), c[2].to_string())).collect();
        let seg2 = clean(&read(&self.refd.join("bin/segment2.c"))?);
        let mesh_body = re(r"(?s)seg2_painting_triangle_mesh\[\]\s*=\s*\{(.*?)\};").captures(&seg2).ok_or("painting mesh")?[1].to_string();
        let mesh: Vec<i64> = re(r"-?\d+").find_iter(&mesh_body).map(|m| m.as_str().parse().unwrap()).collect();
        let (mut shapes, mut layouts) = (M::new(), Vec::new());
        for (index, c) in re(r"(?s)struct Painting (\w+)_painting = \{(.*?)\};").captures_iter(&painting).enumerate() {
            if !visible.contains(&(index as i64)) {
                continue;
            }
            let (name, f): (String, Vec<String>) = (c[1].to_string(), c[2].split(',').map(|v| v.trim().to_string()).collect());
            let names = |key: &str| -> Vec<String> { arrays[key].split(',').map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect() };
            let (maps, texs) = (names(&f[28]), names(&f[29]));
            if !(maps.len() == texs.len() && maps.len() as i64 == int0(&f[1])?) {
                return Err("painting part count".into());
            }
            let mut parts = Vec::new();
            for (mapping, texture) in maps.iter().zip(&texs) {
                if !shapes.contains_key(mapping) {
                    let values: Vec<i64> = re(r"-?\d+").find_iter(&arrays[mapping]).map(|m| m.as_str().parse().unwrap()).collect();
                    let stop = (1 + 3 * values[0]) as usize;
                    let mut vertices = Vec::new();
                    for jx in (1..stop).step_by(3) {
                        let (point, u, v) = (values[jx] as usize, values[jx + 1], values[jx + 2]);
                        let (x, y, movable) = (mesh[1 + point * 3], mesh[2 + point * 3], mesh[3 + point * 3]);
                        vertices.push(json!([x, y, movable, u, v]));
                    }
                    shapes.insert(mapping.clone(), json!({"vertices":vertices,"triangles":values[stop + 1..]}));
                }
                parts.push(json!({"shape":mapping,"texture":format!("{}.png", images[texture])}));
            }
            let nums = |start: usize, count: usize| -> R<Vec<f64>> { f[start..start + count].iter().map(|v| float(v.trim_end_matches('f'))).collect() };
            layouts.push(json!({"name":name,"index":index,"id":int0(&f[0])?,"pos":nums(9, 3)?,"rotation":nums(7, 2)?,"size":nums(38, 1)?[0],"dimensions":nums(30, 2)?,"opacity":int0(&f[34])? as f64 / 255.0,"environment":f[2] == "PAINTING_ENV_MAP","magnitude":nums(13, 2)?,"decay":nums(16, 2)?,"rate":nums(19, 2)?,"dispersion":nums(22, 2)?,"parts":parts}));
        }
        Ok(json!({"shapes":shapes,"layouts":layouts}))
    }

    pub(crate) fn platform_param(&self, expression: &str, behavior: &str) -> R<i64> {
        let fields: Vec<&str> = expression.split('|').map(str::trim).collect();
        if behavior == "bhvRotatingPlatform" {
            let field = fields.iter().find_map(|v| v.strip_prefix("first_")).unwrap_or("0");
            let value = int0(self.params.get(field).map_or(field, String::as_str))? & 255;
            return Ok(if value < 128 { value } else { value - 256 });
        }
        let field = fields.iter().find(|v| !v.starts_with("first_")).copied().unwrap_or("0");
        int0(self.params.get(field).map_or(field, String::as_str))
    }

    fn glb_has_meshes(&self, model: &str) -> bool {
        let Ok(data) = std::fs::read(self.assets.join(model)) else { return true };
        if data.get(..4) != Some(b"glTF") {
            return false;
        }
        let Some(size) = data.get(12..16).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize) else { return true };
        let Some(chunk) = data.get(20..20 + size) else { return true };
        match serde_json::from_slice::<Value>(chunk) {
            Ok(doc) => doc.get("meshes").is_some_and(|m| m.as_array().is_some_and(|a| !a.is_empty())),
            Err(_) => true,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn export(&self, out: &Path) -> R<Vec<String>> {
        std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
        let mut log = Vec::new();
        let mut list: Vec<(String, i64)> = LEVELS.iter().chain(world::NEW_LEVELS.iter()).map(|l| (l.to_string(), 1)).collect();
        list.extend([("castle_inside", 2), ("castle_inside", 3), ("ddd", 2), ("jrb", 2), ("ccm", 2)].map(|(l, a)| (l.to_string(), a)));
        list.extend(world::NEW_AREAS.iter().map(|(l, a)| (l.to_string(), *a)));
        for (level, area_index) in list {
            let (line, data, ext) = self.export_one(&level, area_index)?;
            let name = if area_index == 1 { level.clone() } else { format!("{level}.{area_index}") };
            std::fs::write(out.join(format!("{name}.json")), dumps(&Value::Object(data), None)).map_err(|e| e.to_string())?;
            if !ext.is_empty() {
                std::fs::write(out.join(format!("{name}.ext.json")), dumps(&Value::Object(ext), None)).map_err(|e| e.to_string())?;
            }
            log.push(line);
        }
        std::fs::write(out.join("sm64_warp_report.json"), dumps(&self.world.report()?, None)).map_err(|e| e.to_string())?;
        Ok(log)
    }

    #[allow(clippy::too_many_lines)]
    fn export_one(&self, level: &str, area_index: i64) -> R<(String, M, M)> {
        let folder = self.refd.join("levels").join(level);
        let full_script = read(&folder.join("script.c"))?;
        let geo = args(&full_script, "AREA").into_iter().filter(|a| a.len() == 2).find(|a| a[0] == area_index.to_string()).ok_or("no AREA")?[1].clone();
        let mut spawn = args(&full_script, "MARIO_POS").into_iter().next().unwrap_or_else(|| ["1", "0", "0", "0", "0"].map(String::from).to_vec());
        spawn.truncate(5);
        let terrain_path = folder.join(format!("areas/{area_index}/collision.inc.c"));
        let script = area_commands(&full_script, area_index)?;
        let mut data = match self.collision(&terrain_path, Some(&args(&script, "TERRAIN")[0][0]))? {
            Value::Object(m) => m,
            _ => unreachable!(),
        };
        data.insert("model".into(), json!(format!("levels/{level}/{geo}.glb")));
        data.insert("spawn".into(), json!({"pos":p3(&spawn, 2)?,"faceYaw":round(i(&spawn[1])? as f64 * 65536.0 / 360.0)}));
        data.insert("objects".into(), json!([]));
        data.insert("coins".into(), json!([]));
        data.insert("goombas".into(), json!([]));
        let models: HashMap<String, String> = args(&full_script, "LOAD_MODEL_FROM_GEO").into_iter().map(|a| (a[0].clone(), a[1].clone())).collect();
        let script = script.as_str();

        let whirlpools: Vec<Value> = args(script, "WHIRLPOOL").iter().map(|a| Ok(json!({"index":i(&a[0])?,"cond":i(&a[1])?,"pos":p3(a, 2)?,"strength":i(&a[5])?}))).collect::<R<_>>()?;
        if !whirlpools.is_empty() {
            data.insert("whirlpools".into(), json!(whirlpools));
        }
        if !args(script, "ROOMS").is_empty() {
            let room_source = clean(&read(&folder.join(format!("areas/{area_index}/room.inc.c")))?);
            let inner = room_source.split_once('{').ok_or("room")?.1.split('}').next().unwrap().to_string();
            let rooms: Vec<i64> = re(r"-?\d+").find_iter(&inner).map(|m| m.as_str().parse().unwrap()).collect();
            if rooms.len() != data["triangles"].as_array().unwrap().len() {
                return Err(format!("{level}: room count"));
            }
            data.insert("rooms".into(), json!(rooms));
            let geo_source = clean(&read(&folder.join(format!("areas/{area_index}/geo.inc.c")))?);
            let count: i64 = re(r"GEO_SWITCH_CASE\(\s*(\d+),\s*geo_switch_area").captures(&geo_source).ok_or("switch case")?[1].parse().unwrap();
            let mut rm = vec![data["model"].clone()];
            rm.extend((1..count).map(|k| json!(format!("levels/{level}/variants/{geo}/geo_switch_area_{k}.glb"))));
            data.insert("roomModels".into(), json!(rm));
            if level == "castle_inside" {
                let layouts: HashMap<String, String> = re(r"(?s)GeoLayout (\w+)\[\] = \{(.*?)\};").captures_iter(&geo_source).map(|c| (c[1].to_string(), c[2].to_string())).collect();
                let branches = args(&layouts[&geo], "GEO_BRANCH");
                let rp: Vec<Value> = branches.iter().map(|b| json!(re(r"PAINTING_ID\(\s*(\d+),\s*1\)").captures_iter(&layouts[&b[1]]).map(|c| c[1].parse::<i64>().unwrap()).collect::<Vec<_>>())).collect();
                data.insert("roomPaintings".into(), json!(rp));
                if branches.len() as i64 != count {
                    return Err("branch count".into());
                }
            }
        }
        let terrain_types = args(script, "TERRAIN_TYPE");
        if terrain_types.iter().any(|t| t.len() == 1 && t[0] == "TERRAIN_SLIDE") {
            data.insert("slideTerrain".into(), json!(true));
        }
        if terrain_types.iter().any(|t| t.len() == 1 && t[0] == "TERRAIN_SNOW") {
            data.insert("snowTerrain".into(), json!(true));
        }
        let objects = args(script, "OBJECT");
        for o in &objects {
            if SPAWN_BEHAVIORS.contains(&o[o.len() - 1].as_str()) {
                let e = data.entry("warpSpawns".to_string()).or_insert_with(|| json!({}));
                e.as_object_mut().unwrap().insert(o[7].clone(), json!({"pos":p3(o, 1)?,"faceYaw":round(i(&o[5])? as f64 * 65536.0 / 360.0),"behavior":o[o.len() - 1]}));
            }
        }
        for a in args(script, "WARP_NODE") {
            let (node, dest, area, entry) = (&a[0], &a[1], &a[2], &a[3]);
            if node == "WARP_NODE_WARP_FLOOR" && dest == "LEVEL_JRB" {
                data.insert("floorWarp".into(), json!({"area":i(area)?,"entry":entry}));
            }
            if node == "WARP_NODE_DEATH" || node == "WARP_NODE_WARP_FLOOR" {
                let destination = if dest == "LEVEL_CASTLE" { "castle_inside".to_string() } else { dest[6..].to_lowercase() };
                if LEVELS.contains(&destination.as_str()) {
                    data.insert(if node == "WARP_NODE_DEATH" { "deathWarp" } else { "fallWarp" }.into(), json!({"course":destination,"area":i(area)?,"entry":entry}));
                }
            }
            if node == "WARP_NODE_SUCCESS" && dest == "LEVEL_CASTLE" {
                data.insert("successEntry".into(), json!(entry));
                data.insert("successArea".into(), json!(i(area)?));
            }
        }
        let instant = args(script, "INSTANT_WARP");
        if !instant.is_empty() {
            let mut m = M::new();
            for a in &instant {
                m.insert(i(&a[0])?.to_string(), json!({"area":i(&a[1])?,"delta":ints(&a[2..])?}));
            }
            data.insert("instantWarps".into(), Value::Object(m));
        }
        if level == "castle_inside" {
            data.insert("paintingArt".into(), self.castle_paintings(&folder, area_index)?);
            let mut m = M::new();
            for a in args(script, "PAINTING_WARP_NODE") {
                let d = a[1][6..].to_lowercase();
                if LEVELS.contains(&d.as_str()) && a[2] == "1" {
                    m.insert(i64::from_str_radix(&a[0][a[0].len() - 2..], 16).map_err(|e| e.to_string())?.to_string(), json!(d));
                }
            }
            data.insert("paintings".into(), Value::Object(m));
        }
        let warp_links: HashMap<String, (String, i64, String)> = args(script, "WARP_NODE").into_iter().map(|a| Ok((a[0].clone(), (a[1][6..].to_lowercase(), i(&a[2])?, a[3].clone())))).collect::<R<_>>()?;
        if level == "bob" {
            let pads: Vec<(String, Vec<String>)> = objects.iter().filter(|o| o[o.len() - 1] == "bhvFadingWarp").map(|o| (o[7].clone(), o.clone())).collect();
            let get = |k: &str| pads.iter().rev().find(|(n, _)| n == k).map(|(_, o)| o.clone());
            let mut fw = Vec::new();
            let mut seen: Vec<&String> = Vec::new();
            for (node, _) in &pads {
                if seen.contains(&node) {
                    continue;
                }
                seen.push(node);
                let pad = get(node).unwrap();
                let (destination, dest_area, entry) = warp_links.get(node).ok_or("fading warp link")?.clone();
                if !(destination == level && dest_area == area_index) {
                    return Err("fading warp target".into());
                }
                let target = get(&entry).ok_or("fading warp pad")?;
                fw.push(json!({"pos":p3(&pad, 1)?,"target":p3(&target, 1)?,"faceYaw":round(i(&target[5])? as f64 * 65536.0 / 360.0)}));
            }
            data.insert("fadingWarps".into(), json!(fw));
        }
        let chest_sets = [("bhvTreasureChestsJRB", "jrb"), ("bhvTreasureChestsDDD", "ddd"), ("bhvTreasureChestsShip", "ship")];
        let acts_objs = args(script, "OBJECT_WITH_ACTS");
        for o in objects.iter().chain(acts_objs.iter()) {
            let Some((_, kind)) = chest_sets.iter().find(|(b, _)| *b == o[8]) else { continue };
            let source = clean(&read(&self.refd.join("src/game/behaviors/treasure_chest.inc.c"))?);
            let init = re(&format!(r"(?s)void bhv_treasure_chest_{kind}_init\(void\) \{{(.*?)\}}")).captures(&source).ok_or("chest init")?[1].to_string();
            let marker = format!("void bhv_treasure_chest_{kind}_loop(void)");
            let loop_ = source.split_once(&marker).ok_or("chest loop")?.1.to_string();
            if *kind == "ship" {
                data.insert("drainWater".into(), json!(true));
            } else {
                let target = args(&loop_, "spawn_default_star")[0].clone();
                let pos: Vec<f64> = target.iter().map(|v| float(v.trim_end_matches('f'))).collect::<R<_>>()?;
                data.insert("chestStar".into(), json!({"act":star_act(&o[7])?,"pos":pos}));
            }
            for a in args(&init, "spawn_treasure_chest") {
                obj(&mut data, json!({"model":"actors/treasure_chest/treasure_chest_base_geo.glb","behavior":"chest","param":i(&a[0])?,"pos":[i(&a[1])?,i(&a[2])?,i(&a[3])?],"yaw":int0(&a[4])? as f64 * 360.0 / 65536.0}));
            }
        }
        for o in &objects {
            let last = &o[o.len() - 1];
            if last == "bhvCapSwitch" && level == "totwc" {
                data.insert("wingSwitch".into(), p3(o, 1)?);
            }
            if (last == "bhvHiddenRedCoinStar" || last == "bhvBowserCourseRedCoinStar") && ["sa", "totwc", "bitdw", "bitfs", "bits", "cotmc", "vcutm", "wmotr"].contains(&level) {
                data.insert("redCoinStar".into(), p3(o, 1)?);
            }
            if last != "bhvWarp" && last != "bhvWarpPipe" {
                continue;
            }
            let fields: Vec<&str> = o[7].split('|').map(str::trim).collect();
            let node = fields.iter().find(|f| f.starts_with("WARP_NODE_")).map(|s| s.to_string());
            let (destination, dest_area, entry) = node.as_ref().and_then(|n| warp_links.get(n)).cloned().map_or((None, None, None), |(d, a, e)| (Some(d), Some(a), Some(e)));
            let slide = level == "ccm" && destination.as_deref() == Some(level) && dest_area == Some(2);
            let ok_dest = dest_area == Some(1) && destination.as_deref().is_some_and(|d| ["sa", "pss", "bitdw", "bitfs", "bits", "bowser_1", "bowser_2", "bowser_3"].contains(&d));
            if !slide && !ok_dest {
                continue;
            }
            let pos = p3(o, 1)?;
            let param = match fields.iter().find(|f| f.starts_with("first_")) {
                Some(f) => int0(&f[6..])?,
                None => 0,
            };
            let mut w = json!({"pos":pos,"param":param,"warpTo":destination,"warpEntry":entry});
            if slide {
                w["warpArea"] = json!(dest_area);
            }
            push(&mut data, "warps", w);
            if last == "bhvWarpPipe" {
                obj(&mut data, json!({"model":"actors/warp_pipe/warp_pipe_geo.glb","pos":pos,"yaw":i(&o[5])?,"collision":"warp_pipe"}));
            }
        }
        if ["castle_grounds", "castle_inside", "ccm"].contains(&level) {
            self.doors(level, area_index, script, &terrain_path, &models, &folder, &mut data)?;
        }
        self.objects(level, area_index, script, &models, &folder, &terrain_path, &mut data)?;
        // extension / merge
        let ext = self.world.extend(level, area_index, &data, script, &folder, &|p, s| self.collision(p, s))?;
        let extension = if world::is_legacy(level, area_index) {
            ext
        } else {
            world::merge(&mut data, ext);
            M::new()
        };
        self.extra_collision(level, script, &folder, &terrain_path, &mut data)?;
        if G2_LEVELS.contains(&level) {
            self.g2_add_objects(level, area_index, script, &mut data, &models)?;
        }
        let mut kept = Vec::new();
        let mut skipped = 0;
        for o in data["objects"].as_array().unwrap().clone() {
            if let Some(m) = o.get("model").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                if !self.glb_has_meshes(m) {
                    skipped += 1;
                    continue;
                }
            }
            kept.push(o);
        }
        data.insert("objects".into(), Value::Array(kept));
        let line = format!("{level}: {} terrain triangles, {} static objects, {skipped} meshless decor skipped", data["triangles"].as_array().unwrap().len(), data["objects"].as_array().unwrap().len());
        Ok((line, data, extension))
    }

    #[allow(clippy::too_many_arguments)]
    fn doors(&self, level: &str, area_index: i64, script: &str, terrain_path: &Path, models: &HashMap<String, String>, _folder: &Path, data: &mut M) -> R<()> {
        let links: HashMap<String, Vec<String>> = args(script, "WARP_NODE").into_iter().map(|a| (a[0].clone(), a[1..4].to_vec())).collect();
        let mut doors: Vec<Value> = Vec::new();
        let terrain_text = read(terrain_path)?;
        let mut add_door = |data: &mut M, pos: Value, yaw: Value, node: &str, model: &str, key: Option<i64>| -> R<()> {
            let node = if node.starts_with("WARP_NODE_") { node.to_string() } else { format!("WARP_NODE_{:02X}", int0(node)?) };
            let Some(l) = links.get(&node) else { return Ok(()) };
            let (dest, area, entry) = (&l[0], &l[1], &l[2]);
            if !["LEVEL_CASTLE", "LEVEL_CASTLE_GROUNDS", "LEVEL_CCM"].contains(&dest.as_str()) {
                return Ok(());
            }
            let mut door = json!({"pos":pos,"yaw":yaw,"node":node,"destNode":entry,"destArea":i(area)?,"destination":if dest == "LEVEL_CASTLE" { "castle_inside".to_string() } else { dest[6..].to_lowercase() }});
            if let Some(k) = key {
                door["requiredKey"] = json!(k);
            }
            doors.push(door);
            obj(data, json!({"model":format!("actors/door/{model}.glb"),"pos":pos,"yaw":yaw}));
            Ok(())
        };
        for a in args(&terrain_text, "SPECIAL_OBJECT_WITH_YAW_AND_PARAM") {
            if a[0].contains("door") && a[0].contains("warp") {
                let model = match a[0].as_str() {
                    "special_metal_door_warp" => "metal_door_geo",
                    "special_wooden_door_warp" => "wooden_door_geo",
                    _ => "castle_door_geo",
                };
                add_door(data, p3(&a, 1)?, json!(int0(&a[4])? as f64 * 360.0 / 256.0), &a[5], model, None)?;
            }
        }
        for a in args(script, "OBJECT") {
            if a[a.len() - 1] != "bhvDoorWarp" {
                continue;
            }
            let fields: Vec<&str> = a[7].split('|').map(str::trim).collect();
            let Some(node) = fields.iter().find(|f| f.starts_with("WARP_NODE_")) else { continue };
            if a[0] == "MODEL_CASTLE_KEY_DOOR" {
                let door_id = match fields.iter().find(|f| f.starts_with("first_")) {
                    Some(f) => int0(&f[6..])?,
                    None => 0,
                };
                add_door(data, p3(&a, 1)?, json!(i(&a[5])?), node, "key_door_geo", match door_id { 1 => Some(2), 2 => Some(1), _ => None })?;
            } else if a[0].contains("CASTLE_DOOR") {
                add_door(data, p3(&a, 1)?, json!(i(&a[5])?), node, "castle_door_geo", None)?;
            }
        }
        let mut room_doors: Vec<Value> = Vec::new();
        let door_models = [("special_0stars_door", "castle_door_0_star_geo", 0), ("special_1star_door", "castle_door_1_star_geo", 1), ("special_3star_door", "castle_door_3_stars_geo", 3), ("special_wooden_door", "wooden_door_geo", 0)];
        for a in args(&terrain_text, "SPECIAL_OBJECT_WITH_YAW") {
            if let Some((_, model, stars)) = door_models.iter().find(|(p, _, _)| *p == a[0]) {
                room_doors.push(json!({"pos":[i(&a[1])?,i(&a[2])?,i(&a[3])?],"yaw":int0(&a[4])? as f64 * 360.0 / 256.0,"stars":stars,"sliding":false,"model":format!("actors/door/{model}.glb")}));
            }
        }
        for o in args(script, "OBJECT") {
            if o[o.len() - 1] == "bhvStarDoor" {
                room_doors.push(json!({"pos":p3(&o, 1)?,"yaw":i(&o[5])?,"stars":i(&o[7][6..])?,"sliding":true,"model":format!("levels/{level}/{}.glb", model_of(models, &o[0])?)}));
            }
        }
        for d in &room_doors {
            obj(data, json!({"pos":d["pos"],"yaw":d["yaw"],"model":d["model"]}));
        }
        if !room_doors.is_empty() {
            data.insert("roomDoors".into(), json!(room_doors));
            data.insert("doorMotion".into(), json!({"pull":self.door_motion("5F")?,"push":self.door_motion("60")?}));
        }
        data.insert("doors".into(), json!(doors));
        if level == "castle_inside" && area_index > 1 {
            data.insert("spawn".into(), json!({"pos":doors[0]["pos"],"faceYaw":0}));
        }
        Ok(())
    }
}
