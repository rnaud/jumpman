//! Ports of the four post-processors that run on `assets/courses/*.json` after `export-courses`:
//! `export-sm64-g1-actors.py`, `export-sm64-solids.py`, `export-sm64-bosses.py`, `export-sm64-castle.py`.
//! Each reads the course JSON, replaces the entries it added before (idempotent) and writes it back.
use crate::courses::{act_list, p3, Courses, M};
use crate::pyc::{args, clean, dumps, float, int, int0, ints, read, re, round, R};
use crate::sm64_world::area_commands;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn load(out: &Path, stem: &str) -> R<M> {
    let p = out.join(format!("{stem}.json"));
    match serde_json::from_str::<Value>(&read(&p)?).map_err(|e| format!("{}: {e}", p.display()))? {
        Value::Object(m) => Ok(m),
        _ => Err(format!("{}: not an object", p.display())),
    }
}

fn save(out: &Path, stem: &str, data: &M) -> R<()> {
    std::fs::write(out.join(format!("{stem}.json")), dumps(&Value::Object(data.clone()), None)).map_err(|e| e.to_string())
}

fn truthy_key(o: &Value, k: &str) -> bool {
    o.get(k).is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
}

fn round3(x: f64) -> f64 {
    format!("{x:.3}").parse().unwrap()
}

fn extra(data: &mut M) -> R<&mut M> {
    data.get_mut("extraCollision").and_then(Value::as_object_mut).ok_or_else(|| "no extraCollision".to_string())
}

fn objects_mut(data: &mut M) -> &mut Vec<Value> {
    data.get_mut("objects").unwrap().as_array_mut().unwrap()
}

// ---------- shared C expression evaluation (Python eval of integer defines) ----------

struct Ev<'a> {
    t: Vec<&'a str>,
    i: usize,
}

fn lex(s: &str) -> Option<Vec<String>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            let st = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            out.push(s[st..i].to_string());
        } else if s[i..].starts_with("<<") || s[i..].starts_with(">>") {
            out.push(s[i..i + 2].to_string());
            i += 2;
        } else if "|^&+-*()~%".contains(c) {
            out.push(c.to_string());
            i += 1;
        } else {
            return None;
        }
    }
    Some(out)
}

impl<'a> Ev<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.t.get(self.i).copied()
    }
    fn bin(&mut self, level: usize) -> Option<i128> {
        const OPS: [&[&str]; 5] = [&["|"], &["^"], &["&"], &["<<", ">>"], &["+", "-"]];
        if level == OPS.len() {
            return self.mul();
        }
        let mut v = self.bin(level + 1)?;
        while let Some(op) = self.peek().filter(|o| OPS[level].contains(o)) {
            self.i += 1;
            let r = self.bin(level + 1)?;
            v = match op {
                "|" => v | r,
                "^" => v ^ r,
                "&" => v & r,
                "<<" => v.checked_shl(u32::try_from(r).ok()?)?,
                ">>" => v >> u32::try_from(r).ok()?,
                "+" => v + r,
                _ => v - r,
            };
        }
        Some(v)
    }
    fn mul(&mut self) -> Option<i128> {
        let mut v = self.unary()?;
        while let Some(op) = self.peek().filter(|o| *o == "*" || *o == "%") {
            self.i += 1;
            let r = self.unary()?;
            v = if op == "*" { v * r } else { v.checked_rem_euclid(r)? };
        }
        Some(v)
    }
    fn unary(&mut self) -> Option<i128> {
        let t = self.peek()?;
        self.i += 1;
        match t {
            "-" => Some(-self.unary()?),
            "+" => self.unary(),
            "~" => Some(!self.unary()?),
            "(" => {
                let v = self.bin(0)?;
                if self.peek()? != ")" {
                    return None;
                }
                self.i += 1;
                Some(v)
            }
            n => {
                if let Some(h) = n.strip_prefix("0x").or_else(|| n.strip_prefix("0X")) {
                    i128::from_str_radix(h, 16).ok()
                } else if n.len() > 1 && n.starts_with('0') && n.bytes().any(|c| c != b'0') || !n.bytes().all(|c| c.is_ascii_digit()) {
                    None
                } else {
                    n.parse().ok()
                }
            }
        }
    }
}

fn py_eval(expr: &str) -> Option<i128> {
    let toks = lex(expr)?;
    let mut e = Ev { t: toks.iter().map(String::as_str).collect(), i: 0 };
    let v = e.bin(0)?;
    (e.i == e.t.len()).then_some(v)
}

struct Post<'a> {
    c: &'a Courses,
    out: &'a Path,
    defines: HashMap<String, i128>,
    presets: HashMap<String, (String, String, i128)>,
}

fn deg_to_s16(deg: &str) -> R<i64> {
    let v = round(float(deg)? * 65536.0 / 360.0) & 0xFFFF;
    Ok(if v >= 32768 { v - 65536 } else { v })
}

fn balanced(text: &str, name: &str) -> Vec<Vec<String>> {
    let rx = re(&format!(r"\b{name}\s*\("));
    let b = text.as_bytes();
    let mut out = Vec::new();
    for m in rx.find_iter(text) {
        let (mut depth, mut i) = (1, m.end());
        while depth > 0 && i < b.len() {
            depth += match b[i] {
                b'(' => 1,
                b')' => -1,
                _ => 0,
            };
            i += 1;
        }
        let body = &text[m.end()..i - 1];
        let (mut parts, mut d, mut cur) = (Vec::new(), 0i32, String::new());
        for ch in body.chars() {
            if ch == '(' {
                d += 1;
            } else if ch == ')' {
                d -= 1;
            }
            if ch == ',' && d == 0 {
                parts.push(cur.trim().to_string());
                cur.clear();
            } else {
                cur.push(ch);
            }
        }
        parts.push(cur.trim().to_string());
        out.push(parts);
    }
    out
}

impl<'a> Post<'a> {
    fn new(c: &'a Courses, out: &'a Path) -> R<Post<'a>> {
        let text = clean(&read(&c.refd.join("include/object_constants.h"))?);
        let pending: Vec<(String, String)> = re(r"#define\s+(\w+)\s+([^\n]+)").captures_iter(&text).map(|m| (m[1].to_string(), m[2].to_string())).collect();
        let mut defines: HashMap<String, i128> = HashMap::new();
        for _ in 0..6 {
            for (name, expr) in &pending {
                if defines.contains_key(name) {
                    continue;
                }
                let e = expr.trim().trim_end_matches('\\');
                let sub = re(r"\b[A-Za-z_]\w*\b").replace_all(e, |m: &regex::Captures| defines.get(&m[0]).map_or(m[0].to_string(), |v| v.to_string())).into_owned();
                if let Some(v) = py_eval(&sub) {
                    defines.insert(name.clone(), v);
                }
            }
        }
        let mut p = Post { c, out, defines, presets: HashMap::new() };
        let raw = read(&c.refd.join("include/macro_presets.inc.c"))?;
        for m in re(r"/\*\s*(macro_\w+)\s*\*/\s*\{\s*(\w+)\s*,\s*(\w+)\s*,\s*([^}]*)\}").captures_iter(&raw) {
            let param = re(r"(?s)/\*.*?\*/|//[^\n]*").replace_all(&m[4], "").trim().trim_end_matches(',').to_string();
            let value = if param.is_empty() { 0 } else { p.eval_bparam(&param).unwrap_or(0) as i128 };
            p.presets.insert(m[1].to_string(), (m[2].to_string(), m[3].to_string(), value));
        }
        Ok(p)
    }

    fn eval_bparam(&self, expr: &str) -> R<i64> {
        let e = re(r"BPARAM([1-4])\(([^()]*(?:\([^()]*\))?[^()]*)\)").replace_all(expr.trim(), |m: &regex::Captures| {
            let shift = match &m[1] {
                "1" => 24,
                "2" => 16,
                "3" => 8,
                _ => 0,
            };
            format!("((({})&0xFF)<<{shift})", &m[2])
        });
        let e = re(r"\b[A-Za-z_]\w*\b").replace_all(&e, |m: &regex::Captures| self.defines.get(&m[0]).map_or("0".to_string(), |v| v.to_string())).into_owned();
        let v = py_eval(&e).ok_or_else(|| format!("cannot evaluate {expr}"))?;
        Ok((v & 0xFFFF_FFFF) as i64)
    }

    /// The g1/castle `collision(path, symbol, scale)`: floats rounded to 3 places, no water boxes, no `#ifdef` handling.
    fn col_scaled(&self, path: &Path, symbol: &str, scale: f64) -> R<Value> {
        let text = clean(&read(path)?);
        let text = re(&format!(r"(?s)\b{symbol}\[\]\s*=\s*\{{(.*?)\}};")).captures(&text).ok_or_else(|| format!("{}: no {symbol}", path.display()))?[1].to_string();
        let vertices: Vec<Value> = args(&text, "COL_VERTEX").iter().map(|a| a.iter().map(|v| int(v).map(|n| json!(round3(n as f64 * scale)))).collect::<R<Vec<_>>>().map(Value::Array)).collect::<R<_>>()?;
        let mut triangles = Vec::new();
        let mut surface = 0;
        for c in re(r"\b(COL_TRI_INIT|COL_TRI_SPECIAL|COL_TRI)\s*\(([^()]*)\)").captures_iter(&text) {
            let a: Vec<String> = c[2].split(',').map(|s| s.trim().to_string()).collect();
            if &c[1] == "COL_TRI_INIT" {
                surface = *self.c.surfaces.get(&a[0]).ok_or("surface")?;
            } else {
                triangles.push(json!([int(&a[0])?, int(&a[1])?, int(&a[2])?, surface, if a.len() == 4 { int0(&a[3])? } else { 0 }]));
            }
        }
        Ok(json!({"vertices":vertices,"triangles":triangles,"waterBoxes":[]}))
    }
}

// ============================ g1 actors ============================

fn actor(n: &str) -> String {
    format!("actors/{n}/{n}_geo.glb")
}

pub fn g1_actors(c: &Courses, out: &Path) -> R<Vec<String>> {
    let script_tbl = |b: &str| -> Option<(&'static str, String, Option<&'static str>)> {
        Some(match b {
            "bhvThwomp" => ("thwomp", actor("thwomp"), Some("thwomp")),
            "bhvThwomp2" => ("thwomp2", actor("thwomp"), Some("thwomp2")),
            "bhvPiranhaPlant" => ("piranhaPlant", actor("piranha_plant"), None),
            "bhvBulletBill" => ("bulletBill", actor("bullet_bill"), None),
            "bhvMips" => ("mips", actor("mips"), None),
            "bhvButterfly" => ("butterfly", actor("butterfly"), None),
            "bhvBird" => ("bird", "actors/bird/birds_geo.glb".into(), None),
            _ => return None,
        })
    };
    let macro_tbl = |n: &str| -> Option<(&'static str, Option<String>, i64)> {
        Some(match n {
            "macro_swoop_1" => ("swoop", Some(actor("swoop")), 0),
            "macro_swoop_2" => ("swoop", Some(actor("swoop")), 1),
            "macro_scuttlebug" => ("scuttlebug", Some(actor("scuttlebug")), 0),
            "macro_scuttlebug_spawner" => ("scuttlebugSpawner", None, 0),
            "macro_spindrift" => ("spindrift", Some(actor("spindrift")), 0),
            "macro_monty_mole" => ("montyMole", Some(actor("monty_mole")), 1),
            "macro_monty_mole_no_rock" => ("montyMole", Some(actor("monty_mole")), 0),
            "macro_monty_mole_hole" => ("montyMoleHole", None, 0),
            "macro_snufit" => ("snufit", Some(actor("snufit")), 0),
            _ => return None,
        })
    };
    let targets = [("castle_grounds", "castle_grounds", 1), ("castle_inside", "castle_inside", 1), ("castle_inside.2", "castle_inside", 2), ("castle_inside.3", "castle_inside", 3), ("bob", "bob", 1), ("wf", "wf", 1), ("jrb", "jrb", 1), ("ccm", "ccm", 1), ("bbh", "bbh", 1), ("hmc", "hmc", 1)];
    let p = Post::new(c, out)?;
    let mut log = Vec::new();
    for (course, level, area) in targets {
        let mut data = load(out, course)?;
        data["objects"] = Value::Array(data["objects"].as_array().unwrap().iter().filter(|o| !truthy_key(o, "g1")).cloned().collect());
        let folder = c.refd.join("levels").join(level);
        let script = read(&folder.join("script.c"))?;
        let area_script = area_commands(&script, area)?;
        let mut added: Vec<Value> = Vec::new();
        for kind in ["OBJECT", "OBJECT_WITH_ACTS"] {
            for a in args(&area_script, kind) {
                let bhv = if kind == "OBJECT" { a[a.len() - 1].clone() } else { a[8].clone() };
                let Some((behavior, model, ckey)) = script_tbl(&bhv) else { continue };
                let pe = &a[7];
                let param = if pe.contains("BIRD_BP_SPAWNER") {
                    1
                } else if re(r"^-?(0x[0-9a-fA-F]+|\d+)$").is_match(pe) {
                    int0(pe)?
                } else {
                    0
                };
                let mut o = json!({"model":model,"behavior":behavior,"pos":p3(&a, 1)?,"yaw":int(&a[5])?,"param":param,"g1":true});
                if kind == "OBJECT_WITH_ACTS" {
                    o["acts"] = act_list(&a[a.len() - 1]);
                }
                if let Some(k) = ckey {
                    o["collisionKey"] = json!(k);
                }
                if behavior == "mips" {
                    o["track"] = mips_paths(&folder)?;
                }
                added.push(o);
            }
        }
        let macro_path = folder.join(format!("areas/{area}/macro.inc.c"));
        if macro_path.exists() {
            for a in args(&read(&macro_path)?, "MACRO_OBJECT") {
                if let Some((behavior, model, param)) = macro_tbl(&a[0]) {
                    let mut o = json!({"behavior":behavior,"pos":p3(&a, 2)?,"yaw":int(&a[1])?,"param":param,"g1":true});
                    if let Some(m) = model {
                        o["model"] = json!(m);
                    }
                    added.push(o);
                }
            }
        }
        objects_mut(&mut data).extend(added.iter().cloned());
        if added.iter().any(|o| matches!(o["behavior"].as_str(), Some("thwomp" | "thwomp2"))) {
            let tp = c.refd.join("actors/thwomp/collision.inc.c");
            let e = extra(&mut data)?;
            e.insert("thwomp".into(), p.col_scaled(&tp, "thwomp_seg5_collision_0500B92C", 1.4)?);
            e.insert("thwomp2".into(), p.col_scaled(&tp, "thwomp_seg5_collision_0500B7D0", 1.4)?);
        }
        save(out, course, &data)?;
        log.push(format!("{course}: {} g1 objects", added.len()));
    }
    Ok(log)
}

fn mips_paths(folder: &Path) -> R<Value> {
    let source = clean(&read(&folder.join("areas/3/trajectory.inc.c"))?);
    let names: Vec<String> = match re(r"(?s)inside_castle_seg7_trajectory_mips\[\]\s*=\s*\{(.*?)\};").captures(&source) {
        Some(o) => o[1].split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect(),
        None => (0..10).map(|i| format!("inside_castle_seg7_trajectory_mips_{i}")).collect(),
    };
    let mut flat: Vec<Value> = Vec::new();
    for name in names {
        let body = re(&format!(r"(?s)\b{name}\[\]\s*=\s*\{{(.*?)\}};")).captures(&source).ok_or("mips path")?[1].to_string();
        if !flat.is_empty() {
            flat.push(json!([0, -1000000, 0]));
        }
        for row in args(&body, "TRAJECTORY_POS") {
            flat.push(Value::Array(ints(&row[1..])?));
        }
    }
    Ok(Value::Array(flat))
}

// ============================ solids ============================

const LEVELS: [&str; 11] = ["ttc", "rr", "bitdw", "bitfs", "bits", "bowser_1", "bowser_2", "bowser_3", "hmc", "bbh", "jrb"];
const SLIDING2: [Option<&str>; 8] = [Some("bits_seg7_collision_0701A9A0"), Some("bits_seg7_collision_0701AA0C"), Some("bitfs_seg7_collision_07015714"), Some("bitfs_seg7_collision_07015768"), Some("rr_seg7_collision_070295F8"), Some("rr_seg7_collision_0702967C"), None, Some("bitdw_seg7_collision_0700F688")];
const OCTAGON: [&str; 2] = ["bits_seg7_collision_0701AA84", "rr_seg7_collision_07029508"];
const ACTIVATED: [&str; 3] = ["bits_seg7_collision_0701AD54", "bitfs_seg7_collision_070157E0", "bitfs_seg7_collision_07015124"];
const SEESAW: [Option<&str>; 8] = [Some("bitdw_seg7_collision_0700F70C"), Some("bits_seg7_collision_0701ADD8"), Some("bits_seg7_collision_0701AE5C"), Some("bob_seg7_collision_bridge"), Some("bitfs_seg7_collision_07015928"), Some("rr_seg7_collision_07029750"), Some("rr_seg7_collision_07029858"), None];
const TRACK_PATHS: [(&str, &str); 9] = [
    ("rr", "rr_seg7_trajectory_0702EC3C"),
    ("rr", "rr_seg7_trajectory_0702ECC0"),
    ("ccm", "ccm_seg7_trajectory_0701669C"),
    ("bitfs", "bitfs_seg7_trajectory_070159AC"),
    ("hmc", "hmc_seg7_trajectory_0702B86C"),
    ("lll", "lll_seg7_trajectory_0702856C"),
    ("lll", "lll_seg7_trajectory_07028660"),
    ("rr", "rr_seg7_trajectory_0702ED9C"),
    ("rr", "rr_seg7_trajectory_0702EEE0"),
];
const TRACK_COLLISION: [&str; 4] = ["rr_seg7_collision_07029038", "ccm_seg7_collision_070163F8", "checkerboard_platform_seg8_collision_0800D710", "bitfs_seg7_collision_070157E0"];
const HANDLED: [&str; 31] = [
    "bhvTTCRotatingSolid", "bhvTTC2DRotator", "bhvTTCCog", "bhvTTCElevator", "bhvTTCMovingBar", "bhvTTCPendulum", "bhvTTCPitBlock", "bhvTTCSpinner", "bhvTTCTreadmill", "bhvSlidingPlatform2", "bhvSquarishPathMoving", "bhvOctagonalPlatformRotating", "bhvSwingPlatform", "bhvActivatedBackAndForthPlatform", "bhvFerrisWheelAxle",
    "bhvBitFSSinkingPlatforms", "bhvBitFSSinkingCagePlatform", "bhvSquishablePlatform", "bhvSeesawPlatform", "bhvRRCruiserWing", "bhvRRRotatingBridgePlatform", "bhvRRElevatorPlatform", "bhvHMCElevatorPlatform", "bhvHiddenStaircaseStep", "bhvRockSolid", "bhvBitFSTiltingInvertedPyramid", "bhvTiltingBowserLavaPlatform", "bhvFallingBowserPlatform", "bhvPlatformOnTrack", "bhvAnimatesOnFloorSwitchPress",
    "bhvFloorSwitchAnimatesObject",
];

fn fixed(b: &str) -> Option<&'static str> {
    Some(match b {
        "bhvSwingPlatform" => "rr_seg7_collision_pendulum",
        "bhvRRElevatorPlatform" => "rr_seg7_collision_elevator_platform",
        "bhvHMCElevatorPlatform" => "hmc_seg7_collision_elevator",
        "bhvHiddenStaircaseStep" => "bbh_seg7_collision_staircase_step",
        "bhvRockSolid" => "jrb_seg7_collision_rock_solid",
        "bhvRRRotatingBridgePlatform" => "rr_seg7_collision_rotating_platform_with_fire",
        "bhvSquarishPathMoving" => "bitdw_seg7_collision_moving_pyramid",
        "bhvBitFSTiltingInvertedPyramid" => "bitfs_seg7_collision_inverted_pyramid",
        "bhvBitFSSinkingPlatforms" => "bitfs_seg7_collision_sinking_platform",
        "bhvBitFSSinkingCagePlatform" => "bitfs_seg7_collision_sinking_cage_platform",
        "bhvSquishablePlatform" => "bitfs_seg7_collision_squishable_platform",
        "bhvTiltingBowserLavaPlatform" => "bowser_2_seg7_collision_tilting_platform",
        _ => return None,
    })
}

fn ttc(b: &str, k: i64) -> Option<&'static str> {
    Some(match (b, k) {
        ("bhvTTCRotatingSolid", 0) => "ttc_seg7_collision_07014F70",
        ("bhvTTCRotatingSolid", 1) => "ttc_seg7_collision_07015008",
        ("bhvTTCCog", 0) => "ttc_seg7_collision_07015584",
        ("bhvTTCCog", 2) => "ttc_seg7_collision_07015650",
        ("bhvTTCPitBlock", 0) => "ttc_seg7_collision_07015754",
        ("bhvTTCPitBlock", 1) => "ttc_seg7_collision_070157D8",
        ("bhvTTCTreadmill", 0) => "ttc_seg7_collision_070152B4",
        ("bhvTTCTreadmill", 1) => "ttc_seg7_collision_070153E0",
        ("bhvTTC2DRotator", 0) => "ttc_seg7_collision_clock_main_rotation",
        ("bhvTTCElevator", 0) => "ttc_seg7_collision_clock_platform",
        ("bhvTTCMovingBar", 0) => "ttc_seg7_collision_sliding_surface",
        ("bhvTTCPendulum", 0) => "ttc_seg7_collision_clock_pendulum",
        ("bhvTTCSpinner", 0) => "ttc_seg7_collision_rotating_clock_platform2",
        _ => return None,
    })
}

fn falling(i: usize) -> Option<String> {
    let h = ["", "07004B94", "07004C18", "07004C9C", "07004D20", "07004DA4", "07004E28", "07004EAC", "07004F30", "07004FB4", "07005038"];
    h.get(i).map(|h| format!("bowser_3_seg7_collision_{h}"))
}

fn pick_collision(bhv: &str, bp: i64) -> R<Option<String>> {
    let hi = (bp >> 16) & 0xFFFF;
    let b2 = (bp >> 16) & 0xFF;
    let oob = || format!("collision table index out of range for {bhv}");
    let s = |x: &str| Some(x.to_string());
    Ok(match bhv {
        "bhvSlidingPlatform2" => SLIDING2.get(((hi & 0x0380) >> 7) as usize).ok_or_else(oob)?.map(String::from),
        "bhvOctagonalPlatformRotating" => s(OCTAGON.get(b2 as usize).ok_or_else(oob)?),
        "bhvActivatedBackAndForthPlatform" => s(ACTIVATED.get(((hi & 0x0300) >> 8) as usize).ok_or_else(oob)?),
        "bhvSeesawPlatform" => SEESAW.get(b2 as usize).ok_or_else(oob)?.map(String::from),
        "bhvFallingBowserPlatform" => Some(falling(b2 as usize).ok_or_else(oob)?),
        "bhvPlatformOnTrack" => s(TRACK_COLLISION.get(((hi & 0x0070) >> 4) as usize).ok_or_else(oob)?),
        "bhvFloorSwitchAnimatesObject" => s("purple_switch_seg8_collision_0800C7A8"),
        "bhvAnimatesOnFloorSwitchPress" => None,
        b if fixed(b).is_some() => s(fixed(b).unwrap()),
        "bhvTTCCog" => s(ttc(bhv, (bp >> 16) & 2).ok_or_else(oob)?),
        "bhvTTCTreadmill" => s(ttc(bhv, b2 & 1).ok_or_else(oob)?),
        "bhvTTCRotatingSolid" | "bhvTTCPitBlock" => s(ttc(bhv, b2).ok_or_else(oob)?),
        "bhvTTC2DRotator" => {
            if b2 == 0 {
                s(ttc(bhv, 0).unwrap())
            } else {
                None
            }
        }
        _ => ttc(bhv, 0).map(String::from),
    })
}

fn staircase(k: i64) -> R<Vec<(&'static str, &'static str)>> {
    Ok(match k {
        0 => vec![("bits_seg7_collision_0701B734", "MODEL_BITS_STAIRCASE_FRAME4"), ("bits_seg7_collision_0701B59C", "MODEL_BITS_STAIRCASE_FRAME3"), ("bits_seg7_collision_0701B404", "MODEL_BITS_STAIRCASE_FRAME2"), ("bits_seg7_collision_0701B26C", "MODEL_BITS_STAIRCASE_FRAME1"), ("bits_seg7_collision_0701B0D4", "MODEL_BITS_STAIRCASE")],
        1 => vec![("bitdw_seg7_collision_0700FD9C", "MODEL_BITDW_STAIRCASE"), ("bitdw_seg7_collision_0700FC7C", "MODEL_BITDW_STAIRCASE_FRAME1"), ("bitdw_seg7_collision_0700FB5C", "MODEL_BITDW_STAIRCASE_FRAME2"), ("bitdw_seg7_collision_0700FA3C", "MODEL_BITDW_STAIRCASE_FRAME3"), ("bitdw_seg7_collision_0700F91C", "MODEL_BITDW_STAIRCASE_FRAME4")],
        2 => vec![("rr_seg7_collision_0702A6B4", "MODEL_RR_TRICKY_TRIANGLES_FRAME4"), ("rr_seg7_collision_0702A32C", "MODEL_RR_TRICKY_TRIANGLES_FRAME3"), ("rr_seg7_collision_07029FA4", "MODEL_RR_TRICKY_TRIANGLES_FRAME2"), ("rr_seg7_collision_07029C1C", "MODEL_RR_TRICKY_TRIANGLES_FRAME1"), ("rr_seg7_collision_07029924", "MODEL_RR_TRICKY_TRIANGLES")],
        _ => return Err("staircase kind".into()),
    })
}

struct Solids<'a> {
    p: &'a Post<'a>,
    level: &'a str,
    models: HashMap<String, String>,
    cache: Vec<(String, Value)>,
    solids: Vec<Value>,
}

impl Solids<'_> {
    fn model_path(&self, name: &str) -> String {
        let Some(geo) = self.models.get(name).filter(|g| !g.is_empty()) else {
            return if name == "MODEL_CHECKERBOARD_PLATFORM" { "actors/checkerboard_platform/checkerboard_platform_geo.glb".into() } else { String::new() };
        };
        if geo == "warp_pipe_geo" {
            return "actors/warp_pipe/warp_pipe_geo.glb".into();
        }
        let path = format!("levels/{}/{geo}.glb", self.level);
        if self.p.c.assets.join(&path).exists() { path } else { String::new() }
    }
    fn collision_for(&mut self, sym: &str) -> R<String> {
        if !self.cache.iter().any(|(s, _)| s == sym) {
            let path = self.p.c.collision_symbols().get(sym).ok_or_else(|| format!("unknown collision {sym}"))?.clone();
            let v = self.p.c.collision(&path, Some(sym))?;
            self.cache.push((sym.to_string(), v));
        }
        Ok(sym.to_string())
    }
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, bhv: &str, model: &str, pos: &[String], angle: &[i64], bp: i64, collision_sym: Option<&str>, extra: Option<(&str, Value)>) -> R<()> {
        let sym = match collision_sym {
            Some(s) => Some(s.to_string()),
            None => pick_collision(bhv, bp)?,
        };
        let key = match sym {
            Some(s) => Some(self.collision_for(&s)?),
            None => None,
        };
        let model = if model.starts_with("MODEL_") { self.model_path(model) } else { model.to_string() };
        let mut o = json!({"behavior":bhv,"model":model,"collision":key,"pos":ints(pos)?,"angle":angle,"bparams":bp});
        if let Some((k, v)) = extra {
            o[k] = v;
        }
        self.solids.push(o);
        Ok(())
    }
}

impl Post<'_> {
    fn load_trajectory(&self, level: &str, symbol: &str) -> R<Value> {
        let dir = self.c.refd.join("levels").join(level).join("areas");
        let mut names: Vec<_> = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).collect();
        names.sort();
        let mut text = String::new();
        for d in names {
            let f = d.join("trajectory.inc.c");
            if f.is_file() {
                text.push_str(&clean(&read(&f)?));
            }
        }
        let body = re(&format!(r"(?s)\b{symbol}\[\]\s*=\s*\{{(.*?)\}};")).captures(&text).ok_or("trajectory")?[1].to_string();
        let mut rows: Vec<Value> = args(&body, "TRAJECTORY_POS").iter().map(|a| ints(a).map(Value::Array)).collect::<R<_>>()?;
        rows.push(json!([-1, 0, 0, 0]));
        Ok(Value::Array(rows))
    }

    #[allow(clippy::too_many_lines)]
    fn build_solids(&self, level: &str, area_index: i64) -> R<(Vec<Value>, Vec<(String, Value)>, Vec<Value>)> {
        let folder = self.c.refd.join("levels").join(level);
        let script = read(&folder.join("script.c"))?;
        let models: HashMap<String, String> = args(&script, "LOAD_MODEL_FROM_GEO").into_iter().map(|a| (a[0].clone(), a[1].clone())).collect();
        let area = area_commands(&script, area_index)?;
        let mut s = Solids { p: self, level, models, cache: Vec::new(), solids: Vec::new() };
        for cmd in ["OBJECT", "OBJECT_WITH_ACTS"] {
            for a in balanced(&area, cmd) {
                let (model, x, y, z, rx, ry, rz, bparam, bhv) = (&a[0], &a[1], &a[2], &a[3], &a[4], &a[5], &a[6], &a[7], &a[8]);
                if !HANDLED.contains(&bhv.as_str()) {
                    continue;
                }
                let bp = self.eval_bparam(bparam)?;
                let angle = [deg_to_s16(rx)?, deg_to_s16(ry)?, deg_to_s16(rz)?];
                let pos = [x.clone(), y.clone(), z.clone()];
                if bhv == "bhvFerrisWheelAxle" {
                    let (axle, plat, plat_model) = match level {
                        "bits" => ("bits_seg7_collision_0701ACAC", "bits_seg7_collision_0701AC28", "MODEL_BITS_BLUE_PLATFORM"),
                        "bitdw" => ("bitdw_seg7_collision_0700F7F0", "bitdw_seg7_collision_0700F898", "MODEL_BITDW_BLUE_PLATFORM"),
                        _ => return Err("ferris wheel level".into()),
                    };
                    s.add(bhv, model, &pos, &angle, bp, Some(axle), None)?;
                    for i in 0..4 {
                        s.add("bhvFerrisWheelPlatform", plat_model, &pos, &angle, i << 16, Some(plat), None)?;
                    }
                    continue;
                }
                if bhv == "bhvAnimatesOnFloorSwitchPress" {
                    let mut variants = Vec::new();
                    for (c, m) in staircase((bp >> 16) & 0xFF)? {
                        let model = s.model_path(m);
                        let col = s.collision_for(c)?;
                        variants.push(json!({"model":model,"collision":col}));
                    }
                    s.add(bhv, "", &pos, &angle, bp, None, Some(("variants", json!(variants))))?;
                    continue;
                }
                if bhv == "bhvFloorSwitchAnimatesObject" {
                    s.add(bhv, "actors/purple_switch/purple_switch_geo.glb", &pos, &angle, 1 << 16, None, None)?;
                    continue;
                }
                if bhv == "bhvPlatformOnTrack" {
                    let (lvl, sym) = TRACK_PATHS.get((((bp >> 16) & 0xFFFF) & 0xF) as usize).ok_or("track path index")?;
                    let track = self.load_trajectory(lvl, sym)?;
                    s.add(bhv, model, &pos, &angle, bp, None, Some(("track", track)))?;
                    continue;
                }
                // OBJECT_WITH_ACTS: the solid exists in these acts only (BBH's hidden staircase appears in act 2).
                let acts = if cmd == "OBJECT_WITH_ACTS" { Some(("acts", act_list(&a[a.len() - 1]))) } else { None };
                s.add(bhv, model, &pos, &angle, bp, None, acts)?;
                if bhv == "bhvBitFSSinkingCagePlatform" {
                    let m = s.model_path("MODEL_BITFS_BLUE_POLE");
                    s.solids.push(json!({"behavior":"bhvDDDMovingPole","model":m,"collision":null,"pos":ints(&pos)?,"angle":angle,"bparams":bp}));
                }
            }
        }
        let mut flames = Vec::new();
        for cmd in ["OBJECT", "OBJECT_WITH_ACTS"] {
            for a in balanced(&area, cmd) {
                if a[8] == "bhvFlamethrower" {
                    flames.push(json!({"model":"","pos":[int(&a[1])?,int(&a[2])?,int(&a[3])?],"yaw":deg_to_s16(&a[5])? as f64 * 360.0 / 65536.0,"behavior":"sm64Flamethrower","param":(self.eval_bparam(&a[7])? >> 16) & 0xFF}));
                }
            }
        }
        let macro_path = folder.join(format!("areas/{area_index}/macro.inc.c"));
        if macro_path.exists() {
            let text = clean(&read(&macro_path)?);
            for cmd in ["MACRO_OBJECT", "MACRO_OBJECT_WITH_BHV_PARAM"] {
                for a in balanced(&text, cmd) {
                    let Some((bhv, model, pparam)) = self.presets.get(&a[0]).cloned() else { continue };
                    let given = if a.len() > 5 { self.eval_bparam(&a[5])? } else { 0 };
                    if !HANDLED.contains(&bhv.as_str()) {
                        continue;
                    }
                    let pparam = pparam as i64;
                    let params = if pparam == 0 { given } else { (given & 0xFF00) + (pparam & 0xFF) };
                    let bp = ((params & 0xFF) << 16) + (params & 0xFF00);
                    let mut my = ((int(&a[1])? as f64 * 16.0 / 45.0) as i64 & 127) << 9;
                    if my >= 32768 {
                        my -= 65536;
                    }
                    s.add(&bhv, &model, &[a[2].clone(), a[3].clone(), a[4].clone()], &[0, my, 0], bp, None, None)?;
                }
            }
        }
        Ok((s.solids, s.cache, flames))
    }
}

pub fn solids(c: &Courses, out: &Path) -> R<Vec<String>> {
    let p = Post::new(c, out)?;
    let mut log = Vec::new();
    for level in LEVELS {
        let mut data = load(out, level)?;
        if level == "rr" {
            let script = read(&c.refd.join("levels/rr/script.c"))?;
            let terrain = c.collision(&c.refd.join("levels/rr/areas/1/collision.inc.c"), Some(&args(&area_commands(&script, 1)?, "TERRAIN")[0][0]))?;
            for k in ["vertices", "triangles", "waterBoxes"] {
                data.insert(k.into(), terrain[k].clone());
            }
        }
        let (solids, cache, flames) = p.build_solids(level, 1)?;
        log.push(format!("{level}: {} solids", solids.len()));
        data.insert("solids".into(), json!(solids));
        data.insert("sm64Objects".into(), json!(flames));
        if !data.contains_key("extraCollision") {
            data.insert("extraCollision".into(), json!({}));
        }
        let e = extra(&mut data)?;
        for (sym, mesh) in cache {
            e.insert(sym, json!({"vertices":mesh["vertices"],"triangles":mesh["triangles"],"waterBoxes":mesh.get("waterBoxes").cloned().unwrap_or(json!([]))}));
        }
        save(out, level, &data)?;
    }
    for (stem, level, area) in [("lll.2", "lll", 2)] {
        let mut data = load(out, stem)?;
        let (solids, cache, _) = p.build_solids(level, area)?;
        let keep: Vec<Value> = solids.into_iter().filter(|s| s["behavior"] == "bhvPlatformOnTrack").collect();
        data.insert("solids".into(), json!(keep));
        if !data.contains_key("extraCollision") {
            data.insert("extraCollision".into(), json!({}));
        }
        let e = extra(&mut data)?;
        for (sym, mesh) in cache {
            e.insert(sym, json!({"vertices":mesh["vertices"],"triangles":mesh["triangles"],"waterBoxes":mesh.get("waterBoxes").cloned().unwrap_or(json!([]))}));
        }
        save(out, stem, &data)?;
    }
    Ok(log)
}

// ============================ bosses ============================

pub fn bosses(c: &Courses, out: &Path) -> R<Vec<String>> {
    // bhv -> (kind, model, collisions)
    let bhv_tbl: Vec<(&str, &str, Option<&str>, Vec<&str>)> = vec![
        ("bhvEyerokBoss", "eyerokBoss", None, vec!["ssl_seg7_collision_07028274", "ssl_seg7_collision_070282F8", "ssl_seg7_collision_07028370", "ssl_seg7_collision_070284B0"]),
        ("bhvWigglerHead", "wigglerHead", Some("actors/wiggler_head/wiggler_head_geo.glb"), vec![]),
        ("bhvMrI", "mrI", Some("actors/mr_i_eyeball/mr_i_geo.glb"), vec![]),
        ("bhvMerryGoRoundBooManager", "merryGoRoundBooManager", None, vec![]),
        ("bhvDorrie", "dorrie", Some("actors/dorrie/dorrie_geo.glb"), vec!["dorrie_seg6_collision_0600F644", "dorrie_seg6_collision_0600FBB8"]),
        ("bhvUnagi", "unagi", Some("actors/unagi/unagi_geo.glb"), vec![]),
        ("bhvMantaRay", "mantaRay", Some("actors/manta/manta_seg5_geo_05008D14.glb"), vec![]),
        ("bhvJetStreamRingSpawner", "jetStreamRingSpawner", None, vec![]),
        ("bhvTweester", "tweester", Some("actors/tornado/tweester_geo.glb"), vec![]),
        ("bhvHoot", "hoot", Some("actors/hoot/hoot_geo.glb"), vec![]),
        ("bhvUkiki", "ukiki", Some("actors/ukiki/ukiki_geo.glb"), vec![]),
        ("bhvUkikiCage", "ukikiCage", Some("levels/ttm/ttm_geo_000710.glb"), vec!["ttm_seg7_collision_ukiki_cage"]),
        ("bhvBigBoulderGenerator", "bigBoulderGenerator", None, vec![]),
        // BBH furniture (2026-11-09, Rust-only: the Python exporter does not place these).
        ("bhvTumblingBridge", "tumblingBridge", None, vec!["wf_seg7_collision_tumbling_bridge", "bitfs_seg7_collision_07015288"]),
        ("bhvBBHTumblingBridge", "tumblingBridge", None, vec!["bbh_seg7_collision_07026B1C"]),
        ("bhvChirpChirp", "chirpChirp", None, vec![]),
        ("bhvCourtyardBooTriplet", "courtyardBooTriplet", None, vec![]),
        ("bhvCoffinSpawner", "coffinSpawner", None, vec!["bbh_seg7_collision_coffin"]),
        ("bhvBubba", "bubba", Some("actors/bubba/bubba_geo.glb"), vec![]),
        ("bhvSkeeter", "skeeter", Some("actors/skeeter/skeeter_geo.glb"), vec![]),
        ("bhvMadPiano", "madPiano", Some("actors/mad_piano/mad_piano_geo.glb"), vec![]),
        ("bhvHauntedChair", "hauntedChair", Some("actors/chair/haunted_chair_geo.glb"), vec![]),
        ("bhvFlyingBookend", "flyingBookend", Some("actors/book/bookend_geo.glb"), vec![]),
        ("bhvBookendSpawn", "bookendSpawn", None, vec![]),
    ];
    let targets = [("ssl.3", "ssl", 3), ("thi.3", "thi", 3), ("thi.2", "thi", 2), ("wdw.2", "wdw", 2), ("bits", "bits", 1), ("rr", "rr", 1), ("ttc", "ttc", 1), ("vcutm", "vcutm", 1), ("bbh", "bbh", 1), ("lll", "lll", 1), ("hmc", "hmc", 1), ("jrb", "jrb", 1), ("jrb.2", "jrb", 2), ("ddd", "ddd", 1), ("ddd.2", "ddd", 2), ("ssl", "ssl", 1), ("wf", "wf", 1), ("ttm", "ttm", 1), ("thi", "thi", 1), ("sl", "sl", 1), ("wdw", "wdw", 1), ("bitfs", "bitfs", 1), ("castle_courtyard", "castle_courtyard", 1), ("bob", "bob", 1), ("castle_grounds", "castle_grounds", 1), ("ccm", "ccm", 1), ("wmotr", "wmotr", 1), ("lll.2", "lll", 2)];
    let event_stars: Vec<(&str, Vec<(i64, [i64; 3])>)> = vec![
        ("ssl.3", vec![(4, [0, -900, -3700])]),
        ("thi.3", vec![(6, [0, 2048, 0])]),
        ("bbh", vec![(2, [-1600, -2100, 205]), (6, [1370, 2000, -320])]),
        ("jrb", vec![(2, [6833, -3654, 2230])]),
        ("ddd", vec![(5, [-3180, -3600, 120])]),
        ("ddd.2", vec![(4, [3400, -3200, -500])]),
        ("ttm", vec![(2, [2500, -1200, 1300])]),
        ("thi", vec![(1, [-6300, -1850, -6300])]),
        ("sl", vec![(2, [130, 1600, -4335])]),
    ];
    let mut log = Vec::new();
    for (stem, level, area) in targets {
        let mut data = load(out, stem)?;
        data["objects"] = Value::Array(data["objects"].as_array().unwrap().iter().filter(|o| !truthy_key(o, "r2a")).cloned().collect());
        let folder = c.refd.join("levels").join(level);
        let area_script = area_commands(&read(&folder.join("script.c"))?, area)?;
        let mut added: Vec<Value> = Vec::new();
        for (command, a) in crate::courses_g2::call_args(&area_script, &["OBJECT_WITH_ACTS", "OBJECT"]) {
            let bhv = if command == "OBJECT_WITH_ACTS" { &a[a.len() - 2] } else { &a[a.len() - 1] };
            let Some((_, kind, model, _)) = bhv_tbl.iter().find(|(b, _, _, _)| b == bhv) else { continue };
            let pos: Vec<i64> = a[1..4].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
            let mut o = json!({"behavior":kind,"pos":pos,"yaw":int(&a[5])?,"param":c.evaluate(&a[7]),"g2":true,"r2a":true});
            if let Some(m) = model {
                o["model"] = json!(m);
            }
            if *kind == "tumblingBridge" {
                // The intact-bridge model of the level; the BBH spawner sets `oBhvParams2ndByte` to TUMBLING_BRIDGE_BP_BBH itself.
                o["model"] = json!(match level {
                    "wf" => "levels/wf/wf_geo_000AC8.glb",
                    "bbh" => "levels/bbh/geo_bbh_0005E0.glb",
                    _ => "levels/bitfs/bitfs_geo_0006F0.glb",
                });
                o["param"] = json!(match level {
                    "wf" => 0,
                    "bbh" => 1 << 16,
                    _ => 3 << 16,
                });
            }
            if command == "OBJECT_WITH_ACTS" {
                o["acts"] = act_list(&a[a.len() - 1]);
            }
            added.push(o);
        }
        let macro_path = folder.join(format!("areas/{area}/macro.inc.c"));
        if macro_path.exists() {
            for (command, m) in crate::courses_g2::call_args(&read(&macro_path)?, &["MACRO_OBJECT", "MACRO_OBJECT_WITH_BHV_PARAM"]) {
                if m[0].trim() == "macro_fire_spitter" {
                    let pos: Vec<i64> = m[2..5].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
                    added.push(json!({"behavior":"fireSpitter","pos":pos,"yaw":int(&m[1])?,"param":0,"g2":true,"r2a":true,"model":"actors/bowling_ball/bowling_ball_geo.glb"}));
                }
                if matches!(m[0].trim(), "macro_fly_guy" | "macro_fire_fly_guy") {
                    let pos: Vec<i64> = m[2..5].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
                    let param = if m[0].trim() == "macro_fire_fly_guy" { 1 << 16 } else { 0 };
                    added.push(json!({"behavior":"flyGuy","pos":pos,"yaw":int(&m[1])?,"param":param,"g2":true,"r2a":true,"model":"actors/flyguy/flyguy_geo.glb"}));
                }
                if matches!(m[0].trim(), "macro_chuckya" | "macro_heave_ho") {
                    let pos: Vec<i64> = m[2..5].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
                    let (kind, model) = if m[0].trim() == "macro_chuckya" { ("chuckya", "actors/chuckya/chuckya_geo.glb") } else { ("heaveHo", "actors/heave_ho/heave_ho_geo.glb") };
                    added.push(json!({"behavior":kind,"pos":pos,"yaw":int(&m[1])?,"param":0,"g2":true,"r2a":true,"model":model}));
                }
                if m[0].trim() == "macro_hidden_1up_in_pole" {
                    let pos: Vec<i64> = m[2..5].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
                    added.push(json!({"behavior":"hidden1UpInPoleSpawner","pos":pos,"yaw":int(&m[1])?,"param":0,"g2":true,"r2a":true}));
                }
                if m[0].trim() == "macro_clam_shell" {
                    let pos: Vec<i64> = m[2..5].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
                    added.push(json!({"behavior":"clamShell","pos":pos,"yaw":int(&m[1])?,"param":0,"g2":true,"r2a":true,"model":"actors/clam_shell/clam_shell_geo.glb"}));
                }
                if m[0].trim() == "macro_mr_i" {
                    let param = if command == "MACRO_OBJECT_WITH_BHV_PARAM" { (c.evaluate(&m[5]) & 0xFF) << 16 } else { 0 };
                    let pos: Vec<i64> = m[2..5].iter().map(|v| float(v).map(|f| f as i64)).collect::<R<_>>()?;
                    added.push(json!({"behavior":"mrI","pos":pos,"yaw":int(&m[1])?,"param":param,"g2":true,"r2a":true,"model":"actors/mr_i_eyeball/mr_i_geo.glb"}));
                }
            }
        }
        objects_mut(&mut data).extend(added.iter().cloned());
        let mut symbols: Vec<&str> = Vec::new();
        for (_, kind, _, cols) in &bhv_tbl {
            if added.iter().any(|o| o["behavior"] == *kind) {
                symbols.extend(cols.iter().copied());
            }
        }
        symbols.sort_unstable();
        symbols.dedup();
        for sym in symbols {
            if let Some(mesh) = c.parse_collision(sym)? {
                extra(&mut data)?.insert(format!("g2/{sym}"), mesh);
            }
        }
        if let Some((_, stars)) = event_stars.iter().find(|(s, _)| *s == stem) {
            data.insert("eventStars".into(), json!(stars.iter().map(|(act, pos)| json!({"act":act,"pos":pos})).collect::<Vec<_>>()));
        } else {
            data.shift_remove("eventStars");
        }
        save(out, stem, &data)?;
        log.push(format!("{stem}: {} r2a objects", added.len()));
    }
    Ok(log)
}

// ============================ castle (part B) ============================

pub fn castle(c: &Courses, out: &Path) -> R<Vec<String>> {
    let excl = "actors/exclamation_box/exclamation_box_geo.glb".to_string();
    let one_up = "actors/mushroom_1up/mushroom_1up_geo.glb".to_string();
    let blue = "actors/coin/blue_coin_geo.glb".to_string();
    let macro_tbl = |n: &str| -> Option<(&'static str, Option<String>, i64)> {
        Some(match n {
            "macro_blue_coin_switch" => ("blueCoinSwitch", Some("actors/blue_coin_switch/blue_coin_switch_geo.glb".into()), 0),
            "macro_hidden_blue_coin" => ("hiddenBlueCoin", Some(blue.clone()), 0),
            "macro_moving_blue_coin" => ("movingBlueCoin", Some(blue.clone()), 0),
            "macro_recovery_heart" => ("recoveryHeart", Some("actors/heart/heart_geo.glb".into()), 0),
            "macro_box_koopa_shell" => ("exclamationBox", Some(excl.clone()), 3),
            "macro_box_one_coin" => ("exclamationBox", Some(excl.clone()), 4),
            "macro_box_three_coins" => ("exclamationBox", Some(excl.clone()), 5),
            "macro_box_ten_coins" => ("exclamationBox", Some(excl.clone()), 6),
            "macro_box_1up" => ("exclamationBox", Some(excl.clone()), 7),
            "macro_box_1up_running_away" => ("exclamationBox", Some(excl.clone()), 9),
            "macro_1up" => ("oneUp", Some(one_up.clone()), 0),
            "macro_1up_sliding" => ("oneUpSliding", Some(one_up.clone()), 0),
            "macro_hidden_1up" => ("hidden1Up", Some(one_up.clone()), 0),
            "macro_hidden_1up_trigger" => ("hidden1UpTrigger", None, 0),
            _ => return None,
        })
    };
    let p = Post::new(c, out)?;
    let switch_collision = "blue_coin_switch_seg8_collision_08000E98";
    let mut box_collision = p.col_scaled(&c.refd.join("actors/exclamation_box_outline/collision.inc.c"), "exclamation_box_outline_seg8_collision_08025F78", 1.0)?;
    for v in box_collision["vertices"].as_array_mut().unwrap() {
        for x in v.as_array_mut().unwrap() {
            *x = json!(2.0 * x.as_f64().unwrap());
        }
    }
    let mut stems: Vec<String> = std::fs::read_dir(out).map_err(|e| e.to_string())?.flatten().filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(str::to_string)).collect();
    stems.sort();
    let mut log = Vec::new();
    for stem in stems {
        if stem.ends_with(".ext") || ["bk_", "oot_", "mk64_", "z_"].iter().any(|p| stem.starts_with(p)) {
            continue;
        }
        let Some(m) = re(r"^([a-z_0-9]+?)(?:\.(\d+))?$").captures(&stem) else { continue };
        let (level, area) = (m[1].to_string(), m.get(2).map_or(1, |a| a.as_str().parse::<i64>().unwrap()));
        if !c.refd.join("levels").join(&level).join("script.c").exists() {
            continue;
        }
        let mut data = load(out, &stem)?;
        if !data.contains_key("objects") {
            continue;
        }
        data["objects"] = Value::Array(data["objects"].as_array().unwrap().iter().filter(|o| !truthy_key(o, "pb")).cloned().collect());
        let folder = c.refd.join("levels").join(&level);
        let mut added = castle_script_objects(c, &level, area, &folder)?;
        let macro_path = folder.join(format!("areas/{area}/macro.inc.c"));
        if macro_path.exists() {
            let text = read(&macro_path)?;
            for command in ["MACRO_OBJECT", "MACRO_OBJECT_WITH_BHV_PARAM"] {
                for a in args(&text, command) {
                    let Some((behavior, model, mut param)) = macro_tbl(&a[0]) else { continue };
                    if command == "MACRO_OBJECT_WITH_BHV_PARAM" {
                        param = int0(&a[5])?;
                    }
                    let mut o = json!({"behavior":behavior,"pos":p3(&a, 2)?,"yaw":int(&a[1])?,"param":param});
                    if let Some(m) = model {
                        o["model"] = json!(m);
                    }
                    added.push(o);
                }
            }
        }
        for o in &mut added {
            o["pb"] = json!(true);
            if o.get("acts").is_some_and(Value::is_null) {
                o.as_object_mut().unwrap().remove("acts");
            }
        }
        objects_mut(&mut data).extend(added.iter().cloned());
        let kinds: Vec<&str> = added.iter().filter_map(|o| o["behavior"].as_str()).collect();
        if kinds.contains(&"blueCoinSwitch") {
            let v = p.col_scaled(&c.refd.join("actors/blue_coin_switch/collision.inc.c"), switch_collision, 1.0)?;
            extra(&mut data)?.insert(format!("g2/{switch_collision}"), v);
        }
        if kinds.contains(&"moatGrill") {
            let v = p.col_scaled(&c.refd.join("levels/castle_grounds/areas/1/7/collision.inc.c"), "castle_grounds_seg7_collision_moat_grills", 1.0)?;
            extra(&mut data)?.insert("g2/castle_grounds_seg7_collision_moat_grills".into(), v);
        }
        if kinds.contains(&"waterLevelPillar") {
            let v = p.col_scaled(&c.refd.join("levels/castle_inside/water_level_pillar/collision.inc.c"), "inside_castle_seg7_collision_water_level_pillar", 1.0)?;
            extra(&mut data)?.insert("g2/inside_castle_seg7_collision_water_level_pillar".into(), v);
        }
        if kinds.contains(&"exclamationBox") && !extra(&mut data)?.contains_key("starBox") {
            extra(&mut data)?.insert("starBox".into(), box_collision.clone());
        }
        save(out, &stem, &data)?;
        if !added.is_empty() {
            log.push(format!("{stem}: {} pb objects", added.len()));
        }
    }
    let _ = PathBuf::new();
    Ok(log)
}

fn castle_script_objects(c: &Courses, level: &str, area: i64, folder: &Path) -> R<Vec<Value>> {
    let script = read(&folder.join("script.c"))?;
    if !re(&format!(r"\bAREA\(\s*{area}\s*,")).is_match(&clean(&script)) {
        return Ok(vec![]);
    }
    let area_script = area_commands(&script, area)?;
    let trajectory = |name: &str| -> R<Value> {
        let source = clean(&read(&folder.join("areas/1/trajectory.inc.c"))?);
        let body = re(&format!(r"(?s)\b{name}\[\]\s*=\s*\{{(.*?)\}};")).captures(&source).ok_or("trajectory")?[1].to_string();
        Ok(Value::Array(args(&body, "TRAJECTORY_POS").iter().map(|r| ints(&r[1..]).map(Value::Array)).collect::<R<_>>()?))
    };
    let mut out = Vec::new();
    for kind in ["OBJECT", "OBJECT_WITH_ACTS"] {
        for a in args(&area_script, kind) {
            let bhv = if kind == "OBJECT" { a[a.len() - 1].clone() } else { a[8].clone() };
            let acts = if kind == "OBJECT_WITH_ACTS" { act_list(&a[a.len() - 1]) } else { Value::Null };
            let (pos, yaw) = (p3(&a, 1)?, int(&a[5])?);
            if bhv == "bhvToadMessage" {
                let dialog: i64 = re(r"DIALOG_(\d+)").captures(&a[7]).ok_or("dialog")?[1].parse().unwrap();
                out.push(json!({"model":"actors/toad/toad_geo.glb","behavior":"toadMessage","pos":pos,"yaw":yaw,"param":dialog}));
            } else if level == "thi" && bhv == "bhvKoopa" && a[7].contains("KOOPA_BP_KOOPA_THE_QUICK_THI") {
                out.push(json!({"model":"actors/koopa/koopa_with_shell_geo.glb","behavior":"koopaTheQuick","pos":pos,"yaw":yaw,"param":1,"acts":acts,"track":trajectory("thi_seg7_trajectory_koopa")?}));
            } else if level == "thi" && bhv == "bhvKoopaRaceEndpoint" {
                out.push(json!({"model":"actors/koopa_flag/koopa_flag_geo.glb","behavior":"koopaFlag","pos":pos,"yaw":yaw,"param":1,"acts":acts}));
            }
        }
    }
    if level == "castle_grounds" && area == 1 {
        out.push(json!({"model":"levels/castle_grounds/castle_grounds_geo_00070C.glb","behavior":"moatGrill","pos":[0,0,0],"yaw":0,"param":0}));
    }
    if level == "castle_inside" && area == 3 {
        for a in args(&area_script, "OBJECT") {
            if a[a.len() - 1] == "bhvWaterLevelPillar" {
                out.push(json!({"model":"levels/castle_inside/castle_geo_001940.glb","behavior":"waterLevelPillar","pos":p3(&a, 1)?,"yaw":int(&a[5])?,"param":0}));
            }
        }
    }
    if level == "pss" && area == 1 {
        out.push(json!({"behavior":"pssTimer","pos":[0,0,0],"yaw":0,"param":0}));
    }
    let _ = c;
    Ok(out)
}

/// The four post-processors in pipeline order (g1 actors, solids, bosses, castle part B).
pub fn run_all(c: &Courses, out: &Path) -> R<Vec<String>> {
    let mut log = g1_actors(c, out)?;
    log.extend(solids(c, out)?);
    log.extend(bosses(c, out)?);
    log.extend(castle(c, out)?);
    Ok(log)
}
