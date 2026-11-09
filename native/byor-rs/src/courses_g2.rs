//! Port of `scripts/export_sm64_g2.py`: group-2 placements (lll ssl ddd sl wdw ttm thi) appended to a course.
use crate::courses::{act_list, model_of, Courses, M};
use crate::pyc::{clean, float, int, int0, read, re, R};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// bhv -> (sim kind, model, collision override). model: "level" = the object's own MODEL_*, "" = invisible.
fn table(level: &str) -> Vec<(&'static str, &'static str, &'static str, Option<&'static str>)> {
    match level {
        "lll" => vec![
            ("bhvLLLRotatingHexagonalPlatform", "lllHexPlatform", "level", None),
            ("bhvLLLRotatingHexagonalRing", "lllHexRing", "level", None),
            ("bhvLLLSinkingRectangularPlatform", "lllSinkRect", "level", None),
            ("bhvLLLSinkingSquarePlatforms", "lllSinkSquare", "level", None),
            ("bhvLLLTiltingInvertedPyramid", "lllTiltPyramid", "level", None),
            ("bhvLLLMovingOctagonalMeshPlatform", "lllOctMesh", "level", None),
            ("bhvLLLSinkingRockBlock", "lllSinkRock", "level", None),
            ("bhvLLLRollingLog", "rollingLogLll", "level", None),
            ("bhvLLLFloatingWoodBridge", "lllWoodBridge", "", None),
            ("bhvLLLDrawbridgeSpawner", "lllDrawbridgeSpawner", "", None),
            ("bhvLLLRotatingBlockWithFireBars", "lllFireBars", "level", None),
            ("bhvLLLVolcanoFallingTrap", "volcanoTrap", "level", None),
            ("bhvLLLBowserPuzzle", "bowserPuzzle", "", None),
            ("bhvFlamethrower", "flamethrower", "", None),
            ("bhvBouncingFireball", "bouncingFireball", "", None),
            ("bhvPoleGrabbing", "pole", "", None),
        ],
        "ssl" => vec![
            ("bhvPyramidTop", "pyramidTop", "level", None),
            ("bhvPyramidElevator", "pyramidElevator", "level", None),
            ("bhvSSLMovingPyramidWall", "pyramidWall", "level", None),
            ("bhvToxBox", "toxBox", "level", None),
            ("bhvGrindel", "grindel", "level", None),
            ("bhvHorizontalGrindel", "horizontalGrindel", "level", None),
            ("bhvSpindel", "spindel", "level", None),
            ("bhvPoleGrabbing", "pole", "", None),
        ],
        "ddd" => vec![("bhvDDDPole", "dddPole", "level", None), ("bhvSushiShark", "sushiShark", "actors/sushi/sushi_geo.glb", Some(""))],
        "sl" => vec![("bhvBigChillBully", "bigChillBully", "actors/chillychief/chilly_chief_big_geo.glb", Some(""))],
        "ttm" => vec![("bhvTTMRollingLog", "rollingLogTtm", "level", None)],
        "thi" => vec![
            ("bhvTHIHugeIslandTop", "thiHugeIslandTop", "level", None),
            ("bhvTHITinyIslandTop", "thiTinyIslandTop", "level", None),
            ("bhvFirePiranhaPlant", "firePiranhaPlant", "actors/piranha_plant/piranha_plant_geo.glb", None),
            ("bhvPoleGrabbing", "pole", "", None),
        ],
        "wdw" => vec![
            ("bhvWDWSquareFloatingPlatform", "squareFloatingPlatform", "level", None),
            ("bhvWDWRectangularFloatingPlatform", "rectFloatingPlatform", "level", None),
            ("bhvArrowLift", "arrowLift", "level", None),
            ("bhvInitializeChangingWaterLevel", "waterLevelInit", "", Some("")),
            ("bhvWaterLevelDiamond", "waterLevelDiamond", "level", None),
            ("bhvWDWExpressElevator", "expressElevator", "level", None),
            ("bhvWDWExpressElevatorPlatform", "expressElevatorPlatform", "level", None),
            ("bhvRotatingPlatform", "rotatingPlatformWdw", "level", Some("wdw_seg7_collision_070186B4")),
            ("bhvFloorSwitchHiddenObjects", "floorSwitchHidden", "actors/purple_switch/purple_switch_geo.glb", Some("purple_switch_seg8_collision_0800C7A8")),
            ("bhvHiddenObject", "hiddenObject", "level", Some("wdw_seg7_collision_07018528")),
            ("bhvPoleGrabbing", "pole", "", None),
        ],
        _ => vec![],
    }
}

fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut depth, mut cur) = (Vec::new(), 0i32, String::new());
    for ch in s.chars() {
        if ch == '(' {
            depth += 1;
        }
        if ch == ')' {
            depth -= 1;
        }
        if ch == ',' && depth == 0 {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn call_args(text: &str, names: &[&str]) -> Vec<(String, Vec<String>)> {
    let text = clean(text);
    let rx = re(&format!(r"\b({})\s*\(", names.join("|")));
    let b = text.as_bytes();
    let mut found = Vec::new();
    for m in rx.captures_iter(&text) {
        let start = m.get(0).unwrap().end();
        let (mut depth, mut j) = (1, start);
        while depth > 0 && j < b.len() {
            depth += match b[j] {
                b'(' => 1,
                b')' => -1,
                _ => 0,
            };
            j += 1;
        }
        found.push((m[1].to_string(), split_args(&text[start..j - 1])));
    }
    found
}

impl Courses {
    fn consts(&self) -> &HashMap<String, i64> {
        self.g2_consts.get_or_init(|| {
            let mut c = HashMap::new();
            for name in ["include/object_constants.h", "include/behavior_data.h"] {
                if let Ok(t) = read(&self.refd.join(name)) {
                    for m in re(r"#define\s+(\w+)\s+(-?(?:0x[0-9a-fA-F]+|\d+))\b").captures_iter(&t) {
                        if let Ok(v) = int0(&m[2]) {
                            c.insert(m[1].to_string(), v);
                        }
                    }
                }
            }
            c
        })
    }

    fn evaluate(&self, expr: &str) -> i64 {
        let consts = self.consts();
        let mut expr = expr.trim().to_string();
        loop {
            let rx = re(r"BPARAM([1-4])\(([^()]*(?:\([^()]*\)[^()]*)*)\)");
            let Some(m) = rx.captures(&expr) else { break };
            let shift = match &m[1] {
                "1" => 24,
                "2" => 16,
                "3" => 8,
                _ => 0,
            };
            let rep = ((self.evaluate(&m[2]) & 0xFF) << shift).to_string();
            let r = m.get(0).unwrap().range();
            expr.replace_range(r, &rep);
        }
        let mut total = 0;
        for term in expr.split('|') {
            let term = term.trim();
            if term.is_empty() {
                continue;
            }
            if re(r"^-?(0x[0-9a-fA-F]+|\d+)$").is_match(term) {
                total |= int0(term).unwrap_or(0);
            } else if let Some(v) = consts.get(term) {
                total |= v;
            }
        }
        total
    }

    fn collision_symbols(&self) -> &HashMap<String, PathBuf> {
        self.g2_symbols.get_or_init(|| {
            let mut out = HashMap::new();
            fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
                if let Ok(rd) = std::fs::read_dir(dir) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            walk(&p, out);
                        } else if p.file_name().is_some_and(|n| n == "collision.inc.c") {
                            out.push(p);
                        }
                    }
                }
            }
            let mut files = Vec::new();
            walk(&self.refd.join("levels"), &mut files);
            if let Ok(rd) = std::fs::read_dir(self.refd.join("actors")) {
                for e in rd.flatten() {
                    let p = e.path().join("collision.inc.c");
                    if p.is_file() {
                        files.push(p);
                    }
                }
            }
            files.sort();
            for p in files {
                if let Ok(t) = read(&p) {
                    for m in re(r"const Collision (\w+)\[\]").captures_iter(&t) {
                        out.insert(m[1].to_string(), p.clone());
                    }
                }
            }
            out
        })
    }

    fn parse_collision(&self, symbol: &str) -> R<Option<Value>> {
        let Some(path) = self.collision_symbols().get(symbol) else { return Ok(None) };
        let text = clean(&read(path)?);
        let body = re(&format!(r"(?s)\b{symbol}\[\]\s*=\s*\{{(.*?)\}};")).captures(&text).ok_or("collision body")?[1].to_string();
        let vertices: Vec<Value> = call_args(&body, &["COL_VERTEX"]).iter().map(|(_, a)| a.iter().map(|v| int(v).map(Value::from)).collect::<R<Vec<_>>>().map(Value::Array)).collect::<R<_>>()?;
        let mut triangles = Vec::new();
        let mut surface = 0;
        for (command, a) in call_args(&body, &["COL_TRI_INIT", "COL_TRI_SPECIAL", "COL_TRI"]) {
            if command == "COL_TRI_INIT" {
                surface = *self.surfaces.get(&a[0]).ok_or("surface")?;
            } else {
                let idx: Vec<i64> = a[..3].iter().map(|s| int(s)).collect::<R<_>>()?;
                triangles.push(json!([idx[0], idx[1], idx[2], surface, if a.len() == 4 { int0(&a[3])? } else { 0 }]));
            }
        }
        Ok(Some(json!({"vertices":vertices,"triangles":triangles})))
    }

    fn behavior_collision(&self, bhv: &str) -> Option<String> {
        self.g2_bhv
            .get_or_init(|| {
                let mut m = HashMap::new();
                if let Ok(t) = read(&self.refd.join("data/behavior_data.c")) {
                    let text = clean(&t);
                    for c in re(r"(?s)const BehaviorScript (\w+)\[\]\s*=\s*\{(.*?)\n\};").captures_iter(&text) {
                        if let Some(l) = re(r"LOAD_COLLISION_DATA\((\w+)\)").captures(&c[2]) {
                            m.insert(c[1].to_string(), l[1].to_string());
                        }
                    }
                }
                m
            })
            .get(bhv)
            .cloned()
    }

    pub(crate) fn g2_add_objects(&self, level: &str, area_index: i64, area_script: &str, data: &mut M, models: &HashMap<String, String>) -> R<()> {
        let tbl = table(level);
        let mut added = 0;
        for (command, a) in call_args(area_script, &["OBJECT_WITH_ACTS", "OBJECT"]) {
            let bhv = if command == "OBJECT_WITH_ACTS" { &a[a.len() - 2] } else { &a[a.len() - 1] };
            let Some((_, kind, model, over)) = tbl.iter().find(|(b, _, _, _)| b == bhv) else { continue };
            let model: Option<String> = if *model == "level" { models.get(&a[0]).map(|g| format!("levels/{level}/{g}.glb")) } else if model.is_empty() { None } else { Some(model.to_string()) };
            let pos: Vec<i64> = a[1..4].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
            let mut e = json!({"behavior":kind,"pos":pos,"yaw":int(&a[5])?,"param":self.evaluate(&a[7]),"g2":true});
            if let Some(m) = model {
                e["model"] = json!(m);
            }
            if command == "OBJECT_WITH_ACTS" {
                e["acts"] = act_list(&a[a.len() - 1]);
            }
            let symbol = match over {
                Some(s) => Some(s.to_string()),
                None => self.behavior_collision(bhv),
            };
            if let Some(s) = symbol.filter(|s| !s.is_empty()) {
                e["collision"] = json!(format!("g2/{s}"));
            }
            data["objects"].as_array_mut().unwrap().push(e);
            added += 1;
        }
        let macro_path = self.refd.join(format!("levels/{level}/areas/{area_index}/macro.inc.c"));
        if macro_path.exists() {
            for (command, m) in call_args(&read(&macro_path)?, &["MACRO_OBJECT", "MACRO_OBJECT_WITH_BHV_PARAM"]) {
                if m[0].trim() == "macro_circling_amp" {
                    let param = if command == "MACRO_OBJECT_WITH_BHV_PARAM" { (self.evaluate(&m[5]) & 0xFF) << 16 } else { 0 };
                    let pos: Vec<i64> = m[2..5].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
                    data["objects"].as_array_mut().unwrap().push(json!({"behavior":"circlingAmp","model":"actors/amp/dAmpGeo.glb","pos":pos,"yaw":int(&m[1])?,"param":param,"g2":true}));
                    added += 1;
                }
            }
        }
        let mut keys: Vec<String> = data["objects"].as_array().unwrap().iter().filter(|o| o.get("g2").is_some_and(|g| g == true)).filter_map(|o| o.get("collision").and_then(Value::as_str).map(str::to_string)).collect();
        if added > 0 && level == "lll" {
            keys.extend(["lll_seg7_collision_wood_piece", "lll_seg7_collision_drawbridge", "lll_seg7_collision_puzzle_piece"].map(|s| format!("g2/{s}")));
        }
        keys.sort();
        keys.dedup();
        for key in keys {
            if let Some(mesh) = self.parse_collision(&key[3..])? {
                data.get_mut("extraCollision").unwrap().as_object_mut().unwrap().insert(key, mesh);
            }
        }
        let _ = model_of;
        Ok(())
    }
}
