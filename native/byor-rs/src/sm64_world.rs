//! Port of `scripts/sm64_world.py`: the SM64 world graph report (`assets/courses/sm64_warp_report.json`) and the course
//! additions (`extend`, `merge`) used by `courses.rs`.
use crate::pyc::{args, clean, int, int0, j, re, read, round, R};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const LEGACY_LEVELS: [&str; 26] = [
    "bob", "castle_grounds", "castle_inside", "wf", "bbh", "ccm", "jrb", "lll", "ssl", "wdw", "sl", "ttm", "thi", "ttc", "rr", "bitdw", "bitfs", "bits", "bowser_1", "bowser_2", "bowser_3", "pss", "hmc", "sa", "ddd", "totwc",
];
pub const NEW_LEVELS: [&str; 4] = ["castle_courtyard", "cotmc", "vcutm", "wmotr"];
pub const NEW_AREAS: [(&str, i64); 10] = [("lll", 2), ("ssl", 2), ("ssl", 3), ("sl", 2), ("thi", 2), ("thi", 3), ("ttm", 2), ("ttm", 3), ("ttm", 4), ("wdw", 2)];
const LEGACY_EXTRA: [(&str, i64); 5] = [("castle_inside", 2), ("castle_inside", 3), ("ddd", 2), ("jrb", 2), ("ccm", 2)];
const UNPLAYABLE: [&str; 3] = ["ending", "intro", "menu"];
const SPAWN_MARKERS: [&str; 12] = [
    "bhvFlyingWarp", "bhvSwimmingWarp", "bhvAirborneWarp", "bhvSpinAirborneCircleWarp", "bhvSpinAirborneWarp", "bhvPaintingStarCollectWarp", "bhvAirborneStarCollectWarp", "bhvLaunchStarCollectWarp", "bhvDeathWarp", "bhvPaintingDeathWarp", "bhvAirborneDeathWarp", "bhvLaunchDeathWarp",
];
const STANDING_MARKERS: [&str; 6] = ["bhvInstantActiveWarp", "bhvExitPodiumWarp", "bhvHardAirKnockBackWarp", "bhvFadingWarp", "bhvWarp", "bhvWarpPipe"];

pub fn is_legacy(level: &str, area: i64) -> bool {
    (area == 1 && LEGACY_LEVELS.contains(&level)) || LEGACY_EXTRA.contains(&(level, area))
}

/// `EXPORTS`: sorted (level, area).
pub fn exports() -> Vec<(String, i64)> {
    let mut s: BTreeSet<(String, i64)> = BTreeSet::new();
    for l in LEGACY_LEVELS {
        s.insert((l.into(), 1));
    }
    for (l, a) in LEGACY_EXTRA {
        s.insert((l.into(), a));
    }
    for l in NEW_LEVELS {
        s.insert((l.into(), 1));
    }
    for (l, a) in NEW_AREAS {
        s.insert((l.into(), a));
    }
    s.into_iter().collect()
}

fn sky(name: &str) -> Value {
    match name {
        "black" => json!({"top":[0.0,0.0,0.0],"mid":[0.0,0.0,0.0],"bottom":[0.0,0.0,0.0],"fog":[0.0,0.0,0.0],"fogStart":30000.0,"fogEnd":60000.0}),
        _ => json!({"top":[0.38,0.62,0.92],"mid":[0.78,0.88,0.97],"bottom":[0.97,0.98,1.0],"fog":[0.9,0.94,1.0],"fogStart":14000.0,"fogEnd":40000.0}),
    }
}

pub struct World {
    pub refd: PathBuf,
    scripts: std::cell::RefCell<HashMap<String, String>>,
}

type Cmd = Vec<String>;

fn yaw_s16(degrees: i64) -> i64 {
    round(degrees as f64 * 65536.0 / 360.0)
}

fn course_of(dest: &str) -> String {
    if dest == "LEVEL_CASTLE" {
        "castle_inside".into()
    } else {
        dest.get(6..).unwrap_or("").to_lowercase()
    }
}

fn node_of(param: &str) -> Option<String> {
    param.split('|').map(str::trim).find(|f| f.starts_with("WARP_NODE_")).map(str::to_string)
}

fn first_of(param: &str) -> R<i64> {
    match param.split('|').map(str::trim).find(|f| f.starts_with("first_")) {
        Some(f) => int0(&f[6..]),
        None => Ok(0),
    }
}

/// `(model, pos, yaw, param, behavior)` for OBJECT then OBJECT_WITH_ACTS.
pub struct Obj {
    pub model: String,
    pub pos: Vec<i64>,
    pub yaw: i64,
    pub param: String,
    pub bhv: String,
}

pub fn objects_of(commands: &str) -> R<Vec<Obj>> {
    let mut out = Vec::new();
    for o in args(commands, "OBJECT") {
        out.push(Obj { model: o[0].clone(), pos: o[1..4].iter().map(|s| int(s)).collect::<R<_>>()?, yaw: int(&o[5])?, param: o[7].clone(), bhv: if o.len() > 8 { o[8].clone() } else { o[o.len() - 1].clone() } });
    }
    for o in args(commands, "OBJECT_WITH_ACTS") {
        out.push(Obj { model: o[0].clone(), pos: o[1..4].iter().map(|s| int(s)).collect::<R<_>>()?, yaw: int(&o[5])?, param: o[7].clone(), bhv: o[8].clone() });
    }
    Ok(out)
}

pub fn area_commands(script: &str, index: i64) -> R<String> {
    let source = clean(script);
    let area = re(&format!(r"(?s)\bAREA\(\s*{index}\s*,[^)]*\),(.*?)END_AREA\(")).captures(&source).ok_or_else(|| format!("no AREA({index})"))?[1].to_string();
    let blocks: HashMap<String, String> = re(r"(?s)LevelScript\s+(\w+)\[\]\s*=\s*\{(.*?)\};").captures_iter(&source).map(|c| (c[1].to_string(), c[2].to_string())).collect();
    fn expand(body: &str, blocks: &HashMap<String, String>) -> R<String> {
        let rx = re(r"JUMP_LINK\(\s*(\w+)\s*\)");
        let mut out = String::new();
        let mut last = 0;
        for c in rx.captures_iter(body) {
            let m = c.get(0).unwrap();
            out.push_str(&body[last..m.start()]);
            out.push_str(&expand(blocks.get(&c[1]).ok_or_else(|| format!("missing block {}", &c[1]))?, blocks)?);
            last = m.end();
        }
        out.push_str(&body[last..]);
        Ok(out)
    }
    expand(&area, &blocks)
}

impl World {
    pub fn new(refd: &Path) -> World {
        World { refd: refd.to_path_buf(), scripts: Default::default() }
    }
    pub fn level_script(&self, level: &str) -> R<String> {
        if let Some(s) = self.scripts.borrow().get(level) {
            return Ok(s.clone());
        }
        let s = read(&self.refd.join("levels").join(level).join("script.c"))?;
        self.scripts.borrow_mut().insert(level.to_string(), s.clone());
        Ok(s)
    }
    pub fn area_indices(&self, level: &str) -> R<Vec<i64>> {
        re(r"\bAREA\(\s*(\d+)").captures_iter(&clean(&self.level_script(level)?)).map(|c| int(&c[1])).collect()
    }
    fn terrain_text(&self, level: &str, area: i64) -> R<String> {
        read(&self.refd.join(format!("levels/{level}/areas/{area}/collision.inc.c")))
    }

    fn links(&self, commands: &str) -> R<HashMap<String, (String, i64, String)>> {
        args(commands, "WARP_NODE").into_iter().map(|a| Ok((a[0].clone(), (course_of(&a[1]), int(&a[2])?, a[3].clone())))).collect()
    }

    fn triggers(&self, level: &str, area: i64) -> R<Vec<Value>> {
        let script = self.level_script(level)?;
        let commands = area_commands(&script, area)?;
        let links = self.links(&commands)?;
        let mut out = Vec::new();
        let to = |node: &str, links: &HashMap<String, (String, i64, String)>| links[node].clone();
        for o in objects_of(&commands)? {
            let node = node_of(&o.param);
            let Some(node) = node.filter(|n| links.contains_key(n)) else { continue };
            let kind = match o.bhv.as_str() {
                "bhvWarp" => "warp",
                "bhvWarpPipe" => "pipe",
                "bhvFadingWarp" => "fading",
                "bhvBooWithCage" => "cage",
                "bhvDoorWarp" => "door",
                _ => continue,
            };
            let t = to(&node, &links);
            out.push(json!({"kind":kind,"node":node,"pos":o.pos,"to":t.0,"toArea":t.1,"toNode":t.2}));
        }
        for a in args(&self.terrain_text(level, area)?, "SPECIAL_OBJECT_WITH_YAW_AND_PARAM") {
            if a[0].contains("door") && a[0].contains("warp") {
                let node = format!("WARP_NODE_{:02X}", int0(&a[5])?);
                if let Some(t) = links.get(&node) {
                    out.push(json!({"kind":"door","node":node,"pos":[int(&a[1])?,int(&a[2])?,int(&a[3])?],"to":t.0,"toArea":t.1,"toNode":t.2}));
                }
            }
        }
        for a in args(&commands, "INSTANT_WARP") {
            let delta: Vec<i64> = a[2..].iter().map(|s| int(s)).collect::<R<_>>()?;
            out.push(json!({"kind":"instant","node":format!("INSTANT_{}",a[0]),"pos":[0,0,0],"to":level,"toArea":int(&a[1])?,"toNode":"","delta":delta}));
        }
        for a in args(&commands, "PAINTING_WARP_NODE") {
            out.push(json!({"kind":"painting","node":a[0],"pos":[0,0,0],"to":course_of(&a[1]),"toArea":int(&a[2])?,"toNode":a[3]}));
        }
        Ok(out)
    }

    fn specials(&self, level: &str, area: i64) -> R<Map<String, Value>> {
        let commands = area_commands(&self.level_script(level)?, area)?;
        let mut out = Map::new();
        for a in args(&commands, "WARP_NODE") {
            let name = match a[0].as_str() {
                "WARP_NODE_DEATH" => "death",
                "WARP_NODE_SUCCESS" => "success",
                "WARP_NODE_WARP_FLOOR" => "floor",
                _ => continue,
            };
            out.insert(name.into(), json!({"to":course_of(&a[1]),"toArea":int(&a[2])?,"toNode":a[3]}));
        }
        Ok(out)
    }

    fn arrivals(&self, level: &str, area: i64) -> R<Map<String, Value>> {
        let commands = area_commands(&self.level_script(level)?, area)?;
        let mut out = Map::new();
        for o in objects_of(&commands)? {
            if let Some(node) = node_of(&o.param) {
                if (SPAWN_MARKERS.contains(&o.bhv.as_str()) || STANDING_MARKERS.contains(&o.bhv.as_str())) && !out.contains_key(&node) {
                    out.insert(node, Value::from(o.bhv));
                }
            }
        }
        Ok(out)
    }

    pub fn report(&self) -> R<Value> {
        let ex = exports();
        let mut exported: Vec<String> = ex.iter().map(|(l, a)| format!("{l}.{a}")).collect();
        exported.sort();
        let mut areas = Map::new();
        for (l, a) in &ex {
            areas.insert(format!("{l}.{a}"), json!({"triggers":self.triggers(l, *a)?,"specials":self.specials(l, *a)?,"arrivals":self.arrivals(l, *a)?}));
        }
        let mut levels: Vec<String> = Vec::new();
        for e in std::fs::read_dir(self.refd.join("levels")).map_err(|e| e.to_string())?.flatten() {
            if e.path().is_dir() && e.path().join("script.c").exists() {
                levels.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        levels.sort();
        let mut la = Map::new();
        for l in levels.iter().filter(|l| !UNPLAYABLE.contains(&l.as_str())) {
            la.insert(l.clone(), json!(self.area_indices(l)?));
        }
        Ok(json!({"exported":exported,"unplayable":UNPLAYABLE,"areas":areas,"levelsInDecomp":levels,"levelAreasInDecomp":la}))
    }

    fn door_objects(&self, level: &str, area: i64, data: &Map<String, Value>) -> R<(Vec<Value>, Vec<Value>)> {
        let commands = area_commands(&self.level_script(level)?, area)?;
        let links = self.links(&commands)?;
        let have: HashSet<String> = data.get("doors").and_then(Value::as_array).map(|d| d.iter().filter_map(|x| x["node"].as_str().map(str::to_string)).collect()).unwrap_or_default();
        let (mut doors, mut objects) = (Vec::new(), Vec::new());
        for a in args(&self.terrain_text(level, area)?, "SPECIAL_OBJECT_WITH_YAW_AND_PARAM") {
            if !(a[0].contains("door") && a[0].contains("warp")) {
                continue;
            }
            let node = format!("WARP_NODE_{:02X}", int0(&a[5])?);
            if have.contains(&node) || !links.contains_key(&node) {
                continue;
            }
            let (dest, dest_area, entry) = links[&node].clone();
            let model = match a[0].as_str() {
                "special_metal_door_warp" => "metal_door_geo",
                "special_wooden_door_warp" => "wooden_door_geo",
                _ => "castle_door_geo",
            };
            let pos = json!([int(&a[1])?, int(&a[2])?, int(&a[3])?]);
            let yaw = int0(&a[4])? as f64 * 360.0 / 256.0;
            doors.push(json!({"pos":pos,"yaw":yaw,"node":node,"destNode":entry,"destArea":dest_area,"destination":dest}));
            objects.push(json!({"model":format!("actors/door/{model}.glb"),"pos":pos,"yaw":yaw}));
        }
        Ok((doors, objects))
    }

    #[allow(clippy::too_many_lines)]
    pub fn extend(&self, level: &str, area: i64, data: &Map<String, Value>, script: &str, folder: &Path, collision: &dyn Fn(&Path, Option<&str>) -> R<Value>) -> R<Map<String, Value>> {
        let mut ext = Map::new();
        let links = self.links(script)?;
        let exported: HashSet<(String, i64)> = exports().into_iter().collect();
        let have_pos: HashSet<Vec<i64>> = data.get("warps").and_then(Value::as_array).map(|w| w.iter().map(|x| x["pos"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect()).collect()).unwrap_or_default();
        let objs = objects_of(script)?;
        let mut pads: HashMap<String, (Vec<i64>, i64)> = HashMap::new();
        for o in &objs {
            if o.bhv == "bhvFadingWarp" {
                if let Some(n) = node_of(&o.param) {
                    pads.insert(n, (o.pos.clone(), o.yaw));
                }
            }
        }
        let push = |ext: &mut Map<String, Value>, key: &str, v: Value| {
            ext.entry(key.to_string()).or_insert_with(|| json!([])).as_array_mut().unwrap().push(v);
        };
        let data_spawns = data.get("warpSpawns").and_then(Value::as_object);
        for o in &objs {
            let node = node_of(&o.param);
            if let Some(n) = &node {
                if STANDING_MARKERS.contains(&o.bhv.as_str()) && !data_spawns.is_some_and(|d| d.contains_key(n)) {
                    let e = ext.entry("warpSpawns".to_string()).or_insert_with(|| json!({}));
                    e.as_object_mut().unwrap().insert(n.clone(), json!({"pos":o.pos,"faceYaw":yaw_s16(o.yaw),"behavior":o.bhv}));
                }
            }
            let linked = node.as_ref().filter(|n| links.contains_key(*n));
            if (o.bhv == "bhvWarp" || o.bhv == "bhvWarpPipe") && linked.is_some() {
                let (dest, dest_area, entry) = links[linked.unwrap()].clone();
                if !exported.contains(&(dest.clone(), dest_area)) || have_pos.contains(&o.pos) {
                    continue;
                }
                push(&mut ext, "warps", json!({"pos":o.pos,"param":first_of(&o.param)?,"warpTo":dest,"warpEntry":entry,"warpArea":dest_area}));
                if o.bhv == "bhvWarpPipe" {
                    push(&mut ext, "objects", json!({"model":"actors/warp_pipe/warp_pipe_geo.glb","pos":o.pos,"yaw":o.yaw,"collision":"warp_pipe"}));
                    if !data.get("extraCollision").and_then(Value::as_object).is_some_and(|m| m.contains_key("warp_pipe")) {
                        let e = ext.entry("extraCollision".to_string()).or_insert_with(|| json!({}));
                        e.as_object_mut().unwrap().insert("warp_pipe".into(), collision(&self.refd.join("actors/warp_pipe/collision.inc.c"), None)?);
                    }
                }
            } else if o.bhv == "bhvFadingWarp" && linked.is_some() && !data.get("fadingWarps").is_some_and(truthy) {
                let (dest, dest_area, entry) = links[linked.unwrap()].clone();
                if dest != level || dest_area != area || !pads.contains_key(&entry) {
                    continue;
                }
                let target = &pads[&entry];
                push(&mut ext, "fadingWarps", json!({"pos":o.pos,"target":target.0,"faceYaw":yaw_s16(target.1)}));
            } else if o.bhv == "bhvBooWithCage" && linked.is_some() {
                let (dest, dest_area, entry) = links[linked.unwrap()].clone();
                push(&mut ext, "warps", json!({"pos":o.pos,"param":12 + 30 * 1000,"warpTo":dest,"warpEntry":entry,"warpArea":dest_area,"minStars":12}));
                let e = ext.entry("warpGates".to_string()).or_insert_with(|| json!({}));
                e.as_object_mut().unwrap().insert(dest, json!(12));
            }
        }
        let (doors, door_objects) = self.door_objects(level, area, data)?;
        if !doors.is_empty() {
            ext.insert("doors".into(), json!(doors));
            for d in door_objects {
                push(&mut ext, "objects", d);
            }
            if !data.get("extraCollision").and_then(|m| m.get("door")).is_some_and(truthy) {
                let e = ext.entry("extraCollision".to_string()).or_insert_with(|| json!({}));
                e.as_object_mut().unwrap().insert("door".into(), collision(&self.refd.join("actors/warp_collision/collision.inc.c"), Some("door_seg3_collision_0301CE78"))?);
            }
        }
        let sp = self.specials(level, area)?;
        let known = |d: &Value| exported.contains(&(d["to"].as_str().unwrap().to_string(), d["toArea"].as_i64().unwrap()));
        if let Some(d) = sp.get("death") {
            if !data.get("deathWarp").is_some_and(truthy) && known(d) {
                ext.insert("deathWarp".into(), json!({"course":d["to"],"area":d["toArea"],"entry":d["toNode"]}));
            }
        }
        if let Some(d) = sp.get("floor") {
            if !data.get("fallWarp").is_some_and(truthy) && known(d) && level != "jrb" {
                ext.insert("fallWarp".into(), json!({"course":d["to"],"area":d["toArea"],"entry":d["toNode"]}));
            }
        }
        if let Some(d) = sp.get("success") {
            let to = d["to"].as_str().unwrap();
            if to != "castle_inside" && (LEGACY_LEVELS.contains(&to) || NEW_LEVELS.contains(&to)) {
                ext.insert("successCourse".into(), d["to"].clone());
                ext.insert("successEntry".into(), d["toNode"].clone());
                ext.insert("successArea".into(), d["toArea"].clone());
            }
        }
        let mut table = Map::new();
        for a in args(script, "PAINTING_WARP_NODE") {
            let index = i64::from_str_radix(&a[0][a[0].len() - 2..], 16).map_err(|e| e.to_string())?;
            let da = int(&a[2])?;
            let has = data.get("paintings").and_then(Value::as_object).is_some_and(|p| p.contains_key(&index.to_string()));
            if exported.contains(&(course_of(&a[1]), da)) && !has {
                table.insert(index.to_string(), Value::from(if da == 1 { course_of(&a[1]) } else { format!("{}:{}", course_of(&a[1]), a[2]) }));
            }
        }
        if !table.is_empty() {
            ext.insert("paintings".into(), Value::Object(table));
        }
        for o in &objs {
            if o.bhv == "bhvCapSwitch" && level != "totwc" {
                ext.insert("wingSwitch".into(), json!(o.pos));
                ext.insert("capSwitchKind".into(), json!(if level == "cotmc" { 1 } else { 2 }));
            }
        }
        let macro_path = folder.join(format!("areas/{area}/macro.inc.c"));
        let macro_text = if macro_path.exists() { Some(read(&macro_path)?) } else { None };
        if let Some(mt) = &macro_text {
            if level != "bob" && !data.get("secretStar").is_some_and(truthy) {
                for o in &objs {
                    if o.bhv == "bhvHiddenStar" {
                        let triggers: Vec<Value> = args(mt, "MACRO_OBJECT").iter().filter(|a| a[0] == "macro_hidden_star_trigger").map(|a| Ok(json!([int(&a[2])?, int(&a[3])?, int(&a[4])?]))).collect::<R<_>>()?;
                        let act = re(r"STAR_INDEX_ACT_(\d)").captures(&o.param).ok_or("STAR_INDEX_ACT")?[1].parse::<i64>().unwrap();
                        ext.insert("secretStar".into(), json!({"act":act,"pos":o.pos,"triggers":triggers}));
                    }
                }
            }
            let cap_box = |n: &str| match n {
                "macro_box_wing_cap" => Some(0),
                "macro_box_metal_cap" => Some(1),
                "macro_box_vanish_cap" => Some(2),
                _ => None,
            };
            let macros = args(mt, "MACRO_OBJECT");
            for a in &macros {
                if let Some(kind) = cap_box(&a[0]).filter(|k| *k > 0) {
                    push(&mut ext, "objects", json!({"model":format!("actors/exclamation_box/variants/exclamation_box_geo/geo_switch_anim_state_{kind}.glb"),"behavior":"wingBox","param":kind,"pos":[int(&a[2])?,int(&a[3])?,int(&a[4])?],"yaw":int(&a[1])?}));
                }
            }
            if !["bob", "jrb", "pss"].contains(&level) {
                for a in &macros {
                    if let Some(m) = re(r"^macro_box_star_act_(\d)$").captures(&a[0]) {
                        push(&mut ext, "objects", json!({"model":"actors/exclamation_box/variants/exclamation_box_geo/geo_switch_anim_state_3.glb","behavior":"starBox","pos":[int(&a[2])?,int(&a[3])?,int(&a[4])?],"yaw":int(&a[1])?,"param":int(&m[1])?}));
                    }
                }
            }
            let has_box = ext.get("objects").and_then(Value::as_array).is_some_and(|o| o.iter().any(|x| matches!(x["behavior"].as_str(), Some("wingBox" | "starBox"))));
            if has_box && !data.get("extraCollision").and_then(|m| m.get("starBox")).is_some() {
                let mut b = collision(&self.refd.join("actors/exclamation_box_outline/collision.inc.c"), None)?;
                for v in b["vertices"].as_array_mut().unwrap() {
                    for x in v.as_array_mut().unwrap() {
                        *x = json!(2 * x.as_i64().unwrap());
                    }
                }
                let e = ext.entry("extraCollision".to_string()).or_insert_with(|| json!({}));
                e.as_object_mut().unwrap().insert("starBox".into(), b);
            }
        }
        if !is_legacy(level, area) {
            let geo = read(&folder.join(format!("areas/{area}/geo.inc.c")))?;
            if re(r"GEO_BACKGROUND_COLOR\(\s*0x0001\s*\)").is_match(&geo) {
                ext.insert("sky".into(), sky("black"));
            } else if geo.contains("BACKGROUND_BELOW_CLOUDS") {
                ext.insert("sky".into(), sky("below_clouds"));
            }
            if area > 1 {
                let spawns_has = |m: Option<&Value>| m.and_then(|m| m.get("WARP_NODE_0A")).cloned();
                let spawn = spawns_has(data.get("warpSpawns")).or_else(|| spawns_has(ext.get("warpSpawns")));
                if let Some(s) = spawn {
                    ext.insert("spawn".into(), json!({"pos":s["pos"],"faceYaw":s["faceYaw"]}));
                } else {
                    let pos = match self.instant_arrival(level, area, collision)? {
                        Some(p) => p,
                        None => flat_floor_centre(data),
                    };
                    ext.insert("spawn".into(), json!({"pos":pos,"faceYaw":0}));
                }
            }
        }
        Ok(ext)
    }

    fn instant_arrival(&self, level: &str, area: i64, collision: &dyn Fn(&Path, Option<&str>) -> R<Value>) -> R<Option<Vec<i64>>> {
        let script = self.level_script(level)?;
        for other in self.area_indices(level)? {
            if other == area {
                continue;
            }
            let oc = area_commands(&script, other)?;
            for a in args(&oc, "INSTANT_WARP") {
                if int(&a[1])? != area {
                    continue;
                }
                let delta: Vec<i64> = a[2..].iter().map(|s| int(s)).collect::<R<_>>()?;
                let terrain = args(&oc, "TERRAIN")[0][0].clone();
                let data = collision(&self.refd.join(format!("levels/{level}/areas/{other}/collision.inc.c")), Some(&terrain))?;
                let kind = 0x1B + int(&a[0])?;
                let v = data["vertices"].as_array().unwrap();
                for t in data["triangles"].as_array().unwrap() {
                    let t: Vec<i64> = t.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
                    if t[3] == kind {
                        let c = |k: usize| -> i64 { (0..3).map(|i| v[t[i] as usize][k].as_i64().unwrap()).sum() };
                        return Ok(Some((0..3).map(|k| round(c(k) as f64 / 3.0) + delta[k]).collect()));
                    }
                }
            }
        }
        Ok(None)
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
    }
}

fn flat_floor_centre(data: &Map<String, Value>) -> Vec<i64> {
    let (mut best, mut centre) = (0.0f64, vec![0i64, 0, 0]);
    let v: Vec<Vec<i64>> = data["vertices"].as_array().unwrap().iter().map(|x| x.as_array().unwrap().iter().map(|y| y.as_i64().unwrap()).collect()).collect();
    for t in data["triangles"].as_array().unwrap() {
        let t: Vec<i64> = t.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
        if t[3] != 0 {
            continue;
        }
        let (a, b, c) = (&v[t[0] as usize], &v[t[1] as usize], &v[t[2] as usize]);
        let (ux, uy, uz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
        let (wx, wy, wz) = (c[0] - a[0], c[1] - a[1], c[2] - a[2]);
        let (nx, ny, nz) = (uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
        let length = ((nx * nx + ny * ny + nz * nz) as f64).powf(0.5);
        if length == 0.0 || (ny as f64 / length < 0.95 && -(ny as f64) / length < 0.95) {
            continue;
        }
        if ny <= 0 {
            continue;
        }
        if length > best {
            best = length;
            centre = (0..3).map(|k| round((a[k] + b[k] + c[k]) as f64 / 3.0)).collect();
        }
    }
    centre
}

/// `merge(base, ext)`: arrays append (skipping duplicates), objects merge key-wise, scalars replace.
pub fn merge(base: &mut Map<String, Value>, ext: Map<String, Value>) {
    for (k, v) in ext {
        match (&v, base.get(&k)) {
            (Value::Array(add), Some(Value::Array(cur))) => {
                let mut out = cur.clone();
                for x in add {
                    if !cur.contains(x) {
                        out.push(x.clone());
                    }
                }
                base.insert(k, Value::Array(out));
            }
            (Value::Object(add), Some(Value::Object(cur))) => {
                let mut out = cur.clone();
                for (a, b) in add {
                    out.insert(a.clone(), b.clone());
                }
                base.insert(k, Value::Object(out));
            }
            _ => {
                base.insert(k, v);
            }
        }
    }
}
