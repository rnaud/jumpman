//! Port of `scripts/export-mk64-cpu.py`: `public/mk64/cpu-ai.json` (CPU driver tables and the racers' kart tables).
use super::common::mio0;
use crate::pyc::{dumps, re, read, R};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

const COMMON_DATA_ROM: usize = 0x132B50;

const COURSES: [(&str, &str); 16] = [
    ("luigi_raceway", "luigi_raceway"),
    ("moo_moo_farm", "moo_moo_farm"),
    ("koopa_troopa_beach", "koopa_beach"),
    ("kalimari_desert", "kalimari_desert"),
    ("toads_turnpike", "toads_turnpike"),
    ("frappe_snowland", "frappe_snowland"),
    ("choco_mountain", "choco_mountain"),
    ("mario_raceway", "mario_raceway"),
    ("wario_stadium", "wario_stadium"),
    ("sherbet_land", "sherbet_land"),
    ("royal_raceway", "royal_raceway"),
    ("bowsers_castle", "bowsers_castle"),
    ("dks_jungle_parkway", "dks_jungle_parkway"),
    ("yoshi_valley", "yoshi_valley"),
    ("banshee_boardwalk", "banshee_boardwalk"),
    ("rainbow_road", "rainbow_road"),
];
const CHARACTERS: [&str; 8] = ["mario", "luigi", "yoshi", "toad", "dk", "wario", "peach", "bowser"];

fn number(text: &str) -> R<f64> {
    let t = text.trim();
    let t = t.trim_end_matches('f');
    t.parse::<f64>().map_err(|_| format!("could not convert string to float: {text:?}"))
}

fn course_metadata(source: &Path, stem: &str) -> R<(Map<String, Value>, String)> {
    let text = read(&source.join(format!("yamls/courses/{stem}_metadata.yml")))?;
    let scalar = |key: &str| -> R<f64> {
        let m = re(&format!(r"(?m)^\s*{key}:\s*(-?[\d.]+f?)")).captures(&text).ok_or_else(|| format!("{stem}: {key} missing"))?;
        number(&m[1])
    };
    let four = |key: &str| -> R<Value> {
        let m = re(&format!(r"(?m)^\s*{key}:\s*\[([^\]]*)\]")).captures(&text).ok_or_else(|| format!("{stem}: {key} missing"))?;
        let v: Vec<f64> = m[1].split(',').map(number).collect::<R<_>>()?;
        if v.len() != 4 {
            return Err(format!("{stem} {key}: {} values", v.len()));
        }
        Ok(json!(v))
    };
    let pointer = re(r"(?m)^\s*cpu_behaviour_ptr:\s*(D_0D[0-9A-F]+)").captures(&text).ok_or("cpu_behaviour_ptr")?[1].to_string();
    let mut m = Map::new();
    m.insert("maxSeparation".into(), json!(scalar("cpu_maximum_separation")?));
    m.insert("minSeparation".into(), json!(scalar("cpu_minimum_separation")?));
    m.insert("steering".into(), json!(scalar("cpu_steering_sensitivity")? as i64));
    m.insert("normalSpeed".into(), four("cpu_NormalTargetSpeed")?);
    m.insert("curveSpeed".into(), four("cpu_CurveTargetSpeed")?);
    m.insert("offTrackSpeed".into(), four("cpu_OffTrackTargetSpeed")?);
    m.insert("slowSpeed".into(), four("D_0D0096B8")?);
    Ok((m, pointer))
}

fn behaviour_offsets(source: &Path) -> R<HashMap<String, usize>> {
    let text = read(&source.join("yamls/us/common_data.yml"))?;
    let mut out = HashMap::new();
    for m in re(r"symbol:\s*(D_0D[0-9A-F]+)\n\s*type:\s*mk64:driving_behaviour\n\s*offset:\s*(0x[0-9A-Fa-f]+)").captures_iter(&text) {
        out.insert(m[1].to_string(), usize::from_str_radix(&m[2][2..], 16).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn behaviours(common: &[u8], offset: usize) -> R<Vec<Value>> {
    let mut rows = Vec::new();
    for i in 0..64 {
        let o = offset + i * 8;
        let b = common.get(o..o + 8).ok_or("behaviour table outside the common data")?;
        let (start, end, kind) = (i16::from_be_bytes([b[0], b[1]]), i16::from_be_bytes([b[2], b[3]]), i32::from_be_bytes([b[4], b[5], b[6], b[7]]));
        if start == -1 && end == -1 {
            break;
        }
        rows.push(json!([start, end, kind]));
    }
    Ok(rows)
}

fn float_arrays(text: &str) -> R<HashMap<String, Vec<f64>>> {
    let mut arrays = HashMap::new();
    for m in re(r"(?s)f32\s+(\w+)\[\]\s*=\s*\{(.*?)\};").captures_iter(text) {
        let body = re(r"//[^\n]*").replace_all(&m[2], "").replace('\n', " ");
        let v: Vec<f64> = body.split(',').filter(|s| !s.trim().is_empty()).map(number).collect::<R<_>>()?;
        arrays.insert(m[1].to_string(), v);
    }
    Ok(arrays)
}

fn pointer_tables(text: &str) -> HashMap<String, Vec<String>> {
    let mut tables = HashMap::new();
    for m in re(r"(?s)f32\*\s+(\w+)\[\d*\]\s*=\s*\{(.*?)\};").captures_iter(text) {
        tables.insert(m[1].to_string(), m[2].replace('\n', " ").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect());
    }
    tables
}

fn kart_tables(source: &Path) -> R<Value> {
    let text = read(&source.join("src/data/kart_attributes.c"))?;
    let (arrays, pointers) = (float_arrays(&text)?, pointer_tables(&text));
    let get = |n: &str| arrays.get(n).cloned().ok_or_else(|| format!("kart_attributes.c: {n} missing"));
    let per_cc = |table: &str| -> R<Vec<Vec<f64>>> { pointers.get(table).ok_or_else(|| format!("{table} missing"))?.iter().map(|n| get(n)).collect() };
    let mut m = Map::new();
    m.insert("characters".into(), json!(CHARACTERS));
    m.insert("topSpeed".into(), json!(per_cc("gTopSpeedTable")?));
    m.insert("unk084".into(), json!(per_cc("D_800E2400")?));
    m.insert("unk088".into(), json!(per_cc("D_800E24B4")?));
    m.insert("unk210".into(), json!(per_cc("D_800E2568")?));
    m.insert("friction".into(), json!(get("gKartFrictionTable")?));
    m.insert("gravity".into(), json!(get("gKartGravityTable")?));
    m.insert("speedCap".into(), json!(get("gKartTopSpeedTable")?));
    m.insert("boundingBox".into(), json!(get("gKartBoundingBoxSizeTable")?));
    m.insert("acceleration".into(), json!(per_cc("gKartAccelerationTables")?));
    m.insert("handling".into(), json!(get("gKartHandlingTable")?));
    m.insert("turnSpeedReduction0".into(), json!(get("gKartTurnSpeedReductionTable0")?));
    m.insert("turnSpeedReduction1".into(), json!(get("gKartTurnSpeedReductionTable1")?));
    m.insert("tripleABoost".into(), json!(get("gKartTripleABoost")?));
    Ok(Value::Object(m))
}

pub fn export(rom: &[u8], source: &Path, output: &Path) -> R<()> {
    let common = mio0(rom, COMMON_DATA_ROM)?;
    let offsets = behaviour_offsets(source)?;
    let mut courses = Map::new();
    for (directory, stem) in COURSES {
        let (mut meta, pointer) = course_metadata(source, stem)?;
        let off = *offsets.get(&pointer).ok_or_else(|| format!("{pointer} not in common_data.yml"))?;
        meta.insert("behaviour".into(), Value::Array(behaviours(&common, off)?));
        courses.insert(directory.into(), Value::Object(meta));
    }
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let data = json!({"courses": Value::Object(courses), "kart": kart_tables(source)?});
    std::fs::write(output.join("cpu-ai.json"), dumps(&data, None) + "\n").map_err(|e| e.to_string())?;
    Ok(())
}
