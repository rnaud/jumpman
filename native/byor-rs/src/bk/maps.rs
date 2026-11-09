//! Port of `scripts/bk-maps.py` and the cached `scripts/data/bk-catalogue.json` it reads: which BK maps become courses
//! and under which ids.
use crate::pyc::{re, read, R};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// course id -> BK map id (enum map_e)
pub const WORLD_MAPS: [(&str, i64); 14] = [
    ("bk_spiral_mountain", 0x01),
    ("bk_mumbos_mountain", 0x02),
    ("bk_treasure_trove_cove", 0x07),
    ("bk_clankers_cavern", 0x0B),
    ("bk_bubblegloop_swamp", 0x0D),
    ("bk_gobis_valley", 0x12),
    ("bk_freezeezy_peak", 0x27),
    ("bk_rusty_bucket_bay", 0x31),
    ("bk_mad_monster_mansion", 0x1B),
    ("bk_click_clock_wood", 0x40),
    ("bk_click_clock_wood_spring", 0x43),
    ("bk_click_clock_wood_summer", 0x44),
    ("bk_click_clock_wood_autumn", 0x45),
    ("bk_click_clock_wood_winter", 0x46),
];

const CUTSCENE_REASON: &str = "cutscene / intro / ending map (scripted camera, the world model re-used; no gameplay)";

/// map id -> why it is not a course (sorted by map id)
pub fn skipped_maps() -> Vec<(i64, &'static str)> {
    let mut v: Vec<(i64, &'static str)> = vec![(0x1E, "Nintendo logo intro"), (0x1F, "Rareware logo intro"), (0x20, "ending cutscene (beach)")];
    for id in [0x7B, 0x7C, 0x7D, 0x7E, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A] {
        v.push((id, CUTSCENE_REASON));
    }
    v.push((0x91, "file select screen"));
    for id in [0x94, 0x95, 0x96, 0x97, 0x98, 0x99] {
        v.push((id, CUTSCENE_REASON));
    }
    v
}

pub fn skip_reason(id: i64) -> Option<&'static str> {
    skipped_maps().into_iter().find(|(i, _)| *i == id).map(|(_, r)| r)
}

const WORLD_ORDER: [&str; 11] = ["SM", "MM", "TTC", "CC", "BGS", "GV", "MMM", "FP", "RBB", "CCW", "GL"];
const SEASONS: [&str; 4] = ["spring", "summer", "autumn", "winter"];

pub struct MapEntry {
    pub name: String,
    pub opa: i64,
    pub xlu: i64,
}

pub struct Warp {
    pub func: String,
    pub map: i64,
    pub exit: i64,
}

pub struct Catalogue {
    /// map id -> entry, in ascending map id order
    pub maps: Vec<(i64, MapEntry)>,
    /// category-3 node actor id -> handler
    pub warps: HashMap<i64, Warp>,
    /// actor id -> exit id
    pub exit_actors: HashMap<i64, i64>,
}

impl Catalogue {
    pub fn load(source: &Path) -> R<Catalogue> {
        let path = source.join("scripts/data/bk-catalogue.json");
        let v: Value = serde_json::from_str(&read(&path)?).map_err(|e| format!("{}: {e}", path.display()))?;
        let num = |x: &Value| x.as_i64().ok_or_else(|| "bk-catalogue.json: not an integer".to_string());
        let mut maps = Vec::new();
        for (k, e) in v["maps"].as_object().ok_or("maps")? {
            maps.push((k.parse::<i64>().map_err(|e| e.to_string())?, MapEntry { name: e["name"].as_str().ok_or("name")?.to_string(), opa: num(&e["opa"])?, xlu: num(&e["xlu"])? }));
        }
        maps.sort_by_key(|m| m.0);
        let mut warps = HashMap::new();
        for (k, w) in v["warps"].as_object().ok_or("warps")? {
            warps.insert(k.parse::<i64>().map_err(|e| e.to_string())?, Warp { func: w["fn"].as_str().ok_or("fn")?.to_string(), map: num(&w["map"])?, exit: num(&w["exit"])? });
        }
        let mut exit_actors = HashMap::new();
        for (k, a) in v["exitActors"].as_object().ok_or("exitActors")? {
            exit_actors.insert(num(a)?, k.parse::<i64>().map_err(|e| e.to_string())?);
        }
        Ok(Catalogue { maps, warps, exit_actors })
    }

    pub fn entry(&self, id: i64) -> Option<&MapEntry> {
        self.maps.iter().find(|m| m.0 == id).map(|m| &m.1)
    }
}

/// `MAP_5_TTC_BLUBBERS_SHIP` -> (0x5, "TTC", ["BLUBBERS", "SHIP"])
pub fn split_name(name: &str) -> R<(i64, String, Vec<String>)> {
    let m = re(r"^MAP_([0-9A-F]+)_([A-Z]+)_(.*)$").captures(name).ok_or_else(|| format!("bad map name {name}"))?;
    Ok((i64::from_str_radix(&m[1], 16).map_err(|e| e.to_string())?, m[2].to_string(), m[3].split('_').map(str::to_string).collect()))
}

pub fn course_id(cat: &Catalogue, map_id: i64) -> R<Option<String>> {
    if let Some((cid, _)) = WORLD_MAPS.iter().find(|(_, m)| *m == map_id) {
        return Ok(Some(cid.to_string()));
    }
    let Some(entry) = cat.entry(map_id) else { return Ok(None) };
    if skip_reason(map_id).is_some() {
        return Ok(None);
    }
    let (_, group, words) = split_name(&entry.name)?;
    let mut parts = vec![group.to_lowercase()];
    parts.extend(words.iter().map(|w| w.to_lowercase()));
    Ok(Some(format!("bk_{}", parts.join("_"))))
}

pub fn group_of(cat: &Catalogue, map_id: i64) -> R<String> {
    Ok(split_name(&cat.entry(map_id).ok_or("unknown map")?.name)?.1)
}

/// map id -> course id of every map that becomes a course (worlds first, then sub-areas by map id), in that order.
pub fn exported_maps(cat: &Catalogue) -> R<Vec<(i64, String)>> {
    let mut out: Vec<(i64, String)> = WORLD_MAPS.iter().map(|(c, m)| (*m, c.to_string())).collect();
    let mut ids: Vec<(usize, i64)> = Vec::new();
    for (mid, _) in &cat.maps {
        let g = group_of(cat, *mid)?;
        ids.push((WORLD_ORDER.iter().position(|o| *o == g).unwrap_or(99), *mid));
    }
    ids.sort();
    for (_, mid) in ids {
        if let Some(cid) = course_id(cat, mid)? {
            if !out.iter().any(|(m, _)| *m == mid) {
                out.push((mid, cid));
            }
        }
    }
    Ok(out)
}

pub fn is_subarea(course: &str) -> bool {
    !WORLD_MAPS.iter().any(|(c, _)| *c == course)
}

/// Which of the 14 world skies a map uses (CCW sub-areas follow their season).
pub fn sky_key(cat: &Catalogue, map_id: i64) -> R<&'static str> {
    let (_, group, words) = split_name(&cat.entry(map_id).ok_or("unknown map")?.name)?;
    if group == "CCW" {
        for (s, key) in SEASONS.iter().zip(["bk_click_clock_wood_spring", "bk_click_clock_wood_summer", "bk_click_clock_wood_autumn", "bk_click_clock_wood_winter"]) {
            if words.iter().any(|w| *w == s.to_uppercase()) {
                return Ok(key);
            }
        }
        return Ok("bk_click_clock_wood");
    }
    Ok(match group.as_str() {
        "SM" => "bk_spiral_mountain",
        "MM" => "bk_mumbos_mountain",
        "TTC" => "bk_treasure_trove_cove",
        "CC" => "bk_clankers_cavern",
        "BGS" => "bk_bubblegloop_swamp",
        "GV" => "bk_gobis_valley",
        "FP" => "bk_freezeezy_peak",
        "RBB" => "bk_rusty_bucket_bay",
        "MMM" => "bk_mad_monster_mansion",
        _ => "lair",
    })
}
