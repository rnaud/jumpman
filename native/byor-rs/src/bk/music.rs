//! Banjo-Kazooie music data (new in the Rust exporter, no Python counterpart): the compact-MIDI sequences (`comusic_e` tracks,
//! asset `0x1516 + track`, `musicTrack_load` of `core1/code_11AC0.c`), the second sound font (`soundfont2ctl` / `soundfont2tbl`, the
//! instrument bank `musicInstruments_init` loads) and the tables that say which track each map plays (`sCore2_9B650MapMusicList`
//! of `core2/code_9B650.c`) and how loud each track is (`musicTrackInfo`).
//! Writes `assets/audio/music/bk/{seq_<hex>.bin, bank.ctl, bank.tbl, music.json}`; the player is `native/src/bk_music/`.
use super::{maps, Assets};
use crate::pyc::{dumps, int0, re, read, R};
use serde_json::{json, Map};
use std::collections::HashMap;
use std::path::Path;

/// `MUSIC_TRACK_ASSET_BASE_ID` for the US 1.0 ROM.
pub const TRACK_ASSET_BASE: usize = 0x1516;
const CTL: std::ops::Range<usize> = 0xEA3EB0..0xEADE60;
/// `soundfont2tbl` runs to the start of the `core1` code segment.
const TBL: std::ops::Range<usize> = 0xEADE60..0xF19250;

/// `enum <name> { A, B = 3, C ... }` -> name -> value (comments stripped).
pub fn parse_enum(text: &str, which: &str, prefix: &str) -> R<HashMap<String, i64>> {
    let start = text.find(which).ok_or_else(|| format!("{which} not found"))?;
    let body = &text[start..];
    let body = &body[..body.find("};").ok_or("enum end")?];
    let body = re(r"(?s)/\*.*?\*/").replace_all(body, "");
    let body = re(r"//[^\n]*").replace_all(&body, "");
    let mut out = HashMap::new();
    let mut value: i64 = -1;
    for c in re(&format!(r"({prefix}\w+)\s*(?:=\s*(0x[0-9A-Fa-f]+|\d+))?\s*[,\n}}]")).captures_iter(&body) {
        value = match c.get(2) {
            Some(e) => int0(e.as_str())?,
            None => value + 1,
        };
        out.insert(c[1].to_string(), value);
    }
    Ok(out)
}

/// (track name, default volume) of `musicTrackInfo`, indexed by track id.
pub fn track_meta(code_11ac0: &str) -> Vec<(String, i64)> {
    let start = code_11ac0.find("musicTrackInfo[").unwrap_or(0);
    let body = &code_11ac0[start..];
    let body = &body[..body.find("};").unwrap_or(body.len())];
    re(r#"\{\s*"([^"]*)"\s*,\s*(\d+)\s*\}"#).captures_iter(body).map(|c| (c[1].to_string(), c[2].parse().unwrap_or(15000))).collect()
}

/// `sCore2_9B650MapMusicList`: (map id, track, secondary track, flags); -1 = none.
pub fn map_music(code_9b650: &str, maps: &HashMap<String, i64>, tracks: &HashMap<String, i64>) -> R<Vec<(i64, i64, i64, i64)>> {
    let start = code_9b650.find("sCore2_9B650MapMusicList[").ok_or("map music list")?;
    let body = &code_9b650[start..];
    let body = &body[..body.find("};").ok_or("map music list end")?];
    let val = |s: &str, table: &HashMap<String, i64>| -> R<i64> { table.get(s.trim()).copied().map_or_else(|| int0(s), Ok) };
    let mut out = Vec::new();
    for c in re(r"\{\s*([\w-]+)\s*,\s*([\w-]+)\s*,\s*([\w-]+)\s*,\s*(\d+)\s*\}").captures_iter(body) {
        out.push((val(&c[1], maps)?, val(&c[2], tracks)?, val(&c[3], tracks)?, int0(&c[4])?));
    }
    Ok(out)
}

pub fn export(assets: &Assets, cat: &maps::Catalogue, decomp: &Path, z64: &[u8], out_assets: &Path) -> R<()> {
    let dir = out_assets.join("audio/music/bk");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let enums = read(&decomp.join("include/enums.h"))?;
    let track_ids = parse_enum(&enums, "enum comusic_e", "COMUSIC_")?;
    let map_ids = parse_enum(&enums, "enum map_e", "MAP_")?;
    let count = *track_ids.get("COMUSIC_NUM_TRACKS").ok_or("COMUSIC_NUM_TRACKS")? as usize;
    let meta = track_meta(&read(&decomp.join("src/core1/code_11AC0.c"))?);
    let mut names: Vec<Option<&String>> = vec![None; count];
    for (n, v) in &track_ids {
        if (*v as usize) < count && n != "COMUSIC_NUM_TRACKS" {
            names[*v as usize] = Some(n);
        }
    }
    let mut tracks = Vec::new();
    for id in 0..count {
        let Ok(data) = assets.read(TRACK_ASSET_BASE + id) else { continue };
        // A compact MIDI file starts with 16 track offsets and the division; anything shorter is a placeholder.
        if data.len() < 0x44 {
            continue;
        }
        let file = format!("seq_{id:02x}.bin");
        std::fs::write(dir.join(&file), &data).map_err(|e| e.to_string())?;
        let (name, volume) = meta.get(id).cloned().unwrap_or((String::new(), 15000));
        tracks.push(json!({"id": id, "file": file, "name": name, "symbol": names[id].map(|s| s.as_str()).unwrap_or(""), "volume": volume, "bytes": data.len()}));
    }
    std::fs::write(dir.join("bank.ctl"), z64.get(CTL).ok_or("ROM too small")?).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("bank.tbl"), z64.get(TBL).ok_or("ROM too small")?).map_err(|e| e.to_string())?;
    let list = map_music(&read(&decomp.join("src/core2/code_9B650.c"))?, &map_ids, &track_ids)?;
    let mut by_map = Map::new();
    for (mid, course) in maps::exported_maps(cat)? {
        if let Some((_, t, s, f)) = list.iter().find(|e| e.0 == mid) {
            by_map.insert(course, json!({"map": mid, "track": t, "secondary": s, "flags": f}));
        }
    }
    let doc = json!({"bankRate": 22050, "tracks": tracks, "maps": by_map});
    std::fs::write(dir.join("music.json"), dumps(&doc, Some(1)) + "\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_enums_with_explicit_values_and_comments() {
        let t = "enum comusic_e\n{\n COMUSIC_0_A = 0x00,\n COMUSIC_1_B, /* x */\n COMUSIC_2_C,\n COMUSIC_10_D = 0x10,\n COMUSIC_11_E,\n COMUSIC_NUM_TRACKS\n};";
        let e = parse_enum(t, "enum comusic_e", "COMUSIC_").unwrap();
        assert_eq!((e["COMUSIC_2_C"], e["COMUSIC_11_E"], e["COMUSIC_NUM_TRACKS"]), (2, 17, 18));
    }

    #[test]
    fn parses_the_map_list_and_meta() {
        let maps: HashMap<String, i64> = [("MAP_1_SM".to_string(), 1)].into();
        let tr: HashMap<String, i64> = [("COMUSIC_10_SM".to_string(), 16), ("COMUSIC_56_H".to_string(), 0x56)].into();
        let text = "sCore2_9B650MapMusicList[0x84] = {\n {MAP_1_SM, COMUSIC_10_SM, COMUSIC_56_H, 0},\n {-1, COMUSIC_10_SM, -1, 1},\n {0, 0, 0, 0}\n};";
        assert_eq!(map_music(text, &maps, &tr).unwrap(), vec![(1, 16, 0x56, 0), (-1, 16, -1, 1), (0, 0, 0, 0)]);
        assert_eq!(track_meta("musicTrackInfo[2] = {\n { \"Blank\", 15000 },\n { \"Scrap\", 9 },\n 0\n};"), vec![("Blank".to_string(), 15000), ("Scrap".to_string(), 9)]);
    }
}
