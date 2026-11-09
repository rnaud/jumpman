//! Port of `scripts/export-mk64.py`: kart-mode JSON per course, the racers' kart sprites, the HUD / item / Lakitu
//! textures, the item roulette tables and the shell sprites.
use super::common::{decode, mio0, rgba16, sl, Blocks};
use super::courses::{commands, export_walk, Course, Textures, COURSES};
use crate::pyc::{dumps, int, int0, re, read, R};
use crate::zpng::png_rgba;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

const SHA1: &str = "579c48e211ae952530ffc8738709f078d5dd215e";
const COMMON_DATA: usize = 0x132B50;

fn write_png(path: &Path, w: usize, h: usize, px: &[u8]) -> R<()> {
    std::fs::write(path, png_rgba(w, h, px, 6)?).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, data: impl AsRef<[u8]>) -> R<()> {
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

fn json_file(path: &Path) -> R<Value> {
    serde_json::from_str(&read(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

fn u(v: &Value, key: &str) -> R<usize> {
    v[key].as_u64().map(|x| x as usize).ok_or_else(|| format!("missing {key}"))
}

fn offset_of(v: &Value, key: &str) -> R<usize> {
    Ok(int0(v[key].as_str().ok_or_else(|| format!("missing {key}"))?)? as usize)
}

// ------------------------------------------------------------------------------------------------ HUD

const HUD_SOURCES: [(&str, &str); 10] = [
    ("item_window", "hud-item-"),
    ("character_portraits", "hud-portrait-"),
    ("minimap_icons", "hud-minimap-"),
    ("hud_type_c", "hud-rank-"),
    ("lakitu/redlights", "lakitu-red-"),
    ("lakitu/bluelight", "lakitu-blue-"),
    ("lakitu/nolights", "lakitu-none-"),
    ("lakitu/checkeredflag", "lakitu-flag-"),
    ("lakitu/finallap", "lakitu-final-"),
    ("lakitu/secondlap", "lakitu-second-"),
];

fn size_of(kind: &str) -> R<f64> {
    Ok(match kind {
        "rgba16" | "ia16" => 2.0,
        "ci8" | "i8" => 1.0,
        "i4" | "ia4" => 0.5,
        k => return Err(format!("unknown texture type {k}")),
    })
}

fn raw(blocks: &mut Blocks, entry: &Value, size: usize) -> R<Vec<u8>> {
    let start = offset_of(entry, "rom_offset")?;
    let offset = if entry.get("block_offset").is_some() { offset_of(entry, "block_offset")? } else { 0 };
    let b = blocks.block(start)?;
    Ok(sl(&b, offset, offset + size).to_vec())
}

fn export_hud(blocks: &mut Blocks, source: &Path, output: &Path) -> R<()> {
    let assets = source.join("assets");
    for (name, prefix) in HUD_SOURCES {
        let table = json_file(&assets.join(format!("{name}.json")))?;
        for (key, entry) in table.as_object().ok_or("hud table")? {
            if key.contains("_tlut_") {
                continue;
            }
            let (w, h, kind) = (u(entry, "width")?, u(entry, "height")?, entry["type"].as_str().ok_or("type")?);
            let palette = match entry.get("tlut") {
                Some(t) => Some(rgba16(&raw(blocks, &table[t.as_str().ok_or("tlut")?], 512)?)?),
                None => None,
            };
            let data = raw(blocks, entry, (w as f64 * h as f64 * size_of(kind)?) as usize)?;
            let pixels = decode(kind, &data, w, h, palette.as_deref())?;
            let label = re(r"^(common_texture_|gTexture)").replace(key, "").into_owned();
            let label = re(r"^(item_window_|portrait_|minimap_|hud_type_C_|Lakitu(Red|Blue|No|Checkered|Final|Second)\w*?)").replace(&label, "").into_owned();
            let label = if label.is_empty() { key.clone() } else { label };
            write_png(&output.join(format!("{prefix}{}.png", label.to_lowercase().replace('_', "-"))), w, h, &pixels)?;
        }
    }
    // The green shell is drawn as an 8-frame spinning sprite: one 256 x 32 strip.
    let shell = json_file(&assets.join("greenshell.json"))?;
    let palette = rgba16(&raw(blocks, &shell["common_tlut_green_shell"], 512)?)?;
    let mut frames = Vec::new();
    for i in 0..8 {
        frames.push(decode("ci8", &raw(blocks, &shell[format!("texture_green_shell_{i}")], 1024)?, 32, 32, Some(&palette))?);
    }
    write_png(&output.join("mk64-green-shell.png"), 256, 32, &strip(&frames))?;
    // Standalone common textures (hud text, digits, particles, items) from assets.json.
    let listing = json_file(&source.join("assets.json"))?;
    let wanted = re(r"common_texture_(hud_(lap|123|time|total_time|lap_time|normal_digit|\dst|\dnd|\drd|\dth|lap_\d_on_3)|item_box_question_mark|banana|flat_banana|particle_(fire|spark_\d|smoke_\d))\.");
    for (key, entry) in listing.as_object().ok_or("assets.json")? {
        if !wanted.is_match(key) {
            continue;
        }
        let d = entry["meta"]["dims"].as_array().ok_or("dims")?;
        let (w, h) = (d[0].as_u64().ok_or("w")? as usize, d[1].as_u64().ok_or("h")? as usize);
        let parts: Vec<&str> = key.split('.').collect();
        let kind = parts[parts.len() - 2];
        let a = entry["offsets"]["us"].as_array().ok_or("offsets")?;
        let (start, offset) = (int0(a[0].as_str().ok_or("o")?)? as usize, int0(a[1].as_str().ok_or("o")?)? as usize);
        let b = blocks.block(start)?;
        let data = sl(&b, offset, offset + (w as f64 * h as f64 * size_of(kind)?) as usize).to_vec();
        let label = key.rsplit('/').next().unwrap().split('.').next().unwrap().replace("common_texture_", "").replace('_', "-");
        write_png(&output.join(format!("mk64-{label}.png")), w, h, &decode(kind, &data, w, h, None)?)?;
    }
    // The ROM's minimap outline of every race track (I4, 64 x 64 / 64 x 96 / 128 x 64: `gCourseOutlineTextures`). The
    // battle arenas keep their footprint pictures from `modes.rs`. Rust-only: the Python exporter drew the other 15 from
    // the lap path, so these files no longer match it byte for byte.
    let outline = json_file(&assets.join("course_outlines.json"))?;
    for (key, entry) in outline.as_object().ok_or("course_outlines.json")? {
        let name = key.trim_start_matches("minimap_");
        if matches!(name, "block_fort" | "skyscraper" | "double_deck" | "big_donut") {
            continue;
        }
        let (w, h) = (entry["width"].as_u64().ok_or("width")? as usize, entry["height"].as_u64().ok_or("height")? as usize);
        let start = offset_of(entry, "rom_offset")?;
        let b = blocks.block(start)?;
        write_png(&output.join(format!("hud-minimap-{}.png", name.replace('_', "-"))), w, h, &decode("i4", &b, w, h, None)?)?;
    }
    Ok(())
}

fn strip(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..32 {
        for f in frames {
            out.extend_from_slice(&f[y * 128..(y + 1) * 128]);
        }
    }
    out
}

const ITEM_CURVES: [(&str, usize, usize); 6] = [("grandPrixHuman", 0x8150, 8), ("grandPrixCpu", 0x8470, 8), ("versus2", 0x8790, 2), ("versus3", 0x8858, 3), ("versus4", 0x8984, 4), ("battle", 0x8B14, 1)];

fn export_item_curves(common: &[u8], output: &Path) -> R<()> {
    let mut tables = Map::new();
    for (name, off, rows) in ITEM_CURVES {
        let list: Vec<Value> = (0..rows).map(|r| Value::String(sl(common, off + 100 * r, off + 100 * (r + 1)).iter().map(|b| format!("{b:02x}")).collect())).collect();
        tables.insert(name.into(), Value::Array(list));
    }
    write(&output.join("item-curves.json"), dumps(&Value::Object(tables), Some(1)) + "\n")
}

fn export_item_sprites(rom: &[u8], source: &Path, common: &[u8], output: &Path) -> R<()> {
    let assets = source.join("assets");
    let blue = json_file(&assets.join("blueshell.json"))?;
    let off = offset_of(&blue["common_tlut_blue_shell"], "block_offset")?;
    let palette = rgba16(sl(common, off, off + 512))?;
    let mut frames = Vec::new();
    for i in 0..8 {
        let entry = &blue[format!("texture_blue_shell_{i}")];
        let data = mio0(rom, offset_of(entry, "rom_offset")?)?;
        frames.push(decode("ci8", sl(&data, 0, 1024), 32, 32, Some(&palette))?);
    }
    write_png(&output.join("mk64-blue-shell.png"), 256, 32, &strip(&frames))?;
    let green = json_file(&assets.join("greenshell.json"))?;
    let off = offset_of(&green["common_tlut_green_shell"], "block_offset")?;
    let mut rawp = sl(common, off, off + 512).to_vec();
    for i in (0..512.min(rawp.len())).step_by(2) {
        let c = ((rawp[i] as u32) << 8) | rawp[i + 1] as u32;
        let c = ((c & 0xF800) >> 5) | ((c & 0x07C0) << 5) | (c & 0x3F);
        rawp[i] = (c >> 8) as u8;
        rawp[i + 1] = (c & 255) as u8;
    }
    let palette = rgba16(&rawp)?;
    let mut frames = Vec::new();
    for i in 0..8 {
        let entry = &green[format!("texture_green_shell_{i}")];
        let data = mio0(rom, offset_of(entry, "rom_offset")?)?;
        frames.push(decode("ci8", sl(&data, 0, 1024), 32, 32, Some(&palette))?);
    }
    write_png(&output.join("mk64-red-shell.png"), 256, 32, &strip(&frames))
}

// ------------------------------------------------------------------------------------------------ kart-mode JSON

/// Kart-mode JSON of one course; returns whether a file was written (battle arenas have no lap path).
fn export_kart_course(textures: &mut Textures, source: &Path, name: &str, output: &Path) -> R<bool> {
    let course = Course::new(source, name)?;
    let path = course.track_path()?;
    if path.is_empty() {
        return Ok(false);
    }
    super::props::export(&course, &mut textures.blocks, source, output)?;
    let filename = format!("{}.json", name.replace('_', "-"));
    if name != "luigi_raceway" {
        let boxes = course.item_boxes()?;
        let payload = json!({"source": "n64decomp/mk64", "romSha1": SHA1, "model": format!("levels/mk64_{name}/terrain.glb"), "path": path, "scale": 8, "itemBoxes": boxes});
        write(&output.join(&filename), dumps(&payload, None))?;
        return Ok(true);
    }
    let text = read(&source.join("courses").join(name).join("course_vertices.inc.c"))?;
    let pattern = re(r"\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},\s*\{\s*(-?\d+),\s*(-?\d+)\s*\},\s*\{\s*MACRO_COLOR_FLAG\((0x\w+),\s*(0x\w+),\s*(0x\w+)");
    let mut vertices = Vec::new();
    for r in pattern.captures_iter(&text) {
        let row: R<Vec<i64>> = (1..=8).map(|i| if r[i].starts_with("0x") { int0(&r[i]) } else { int(&r[i]) }).collect();
        vertices.push(row?);
    }
    let (mut group_order, mut groups): (Vec<Option<String>>, HashMap<Option<String>, Vec<i64>>) = (vec![], HashMap::new());
    let mut texture_list: Vec<(String, Value)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut texture: Option<String> = None;
    let mut cache: HashMap<i64, i64> = HashMap::new();
    for (command, args) in commands(&read(&source.join("courses").join(name).join("course_displaylists.inc.c"))?) {
        match command.as_str() {
            "gsDPSetTextureImage" => {
                let t = args.get(3).ok_or("gsDPSetTextureImage args")?.clone();
                if !texture_list.iter().any(|(k, _)| *k == t) {
                    let (w, h, pixels) = match textures.image(&t)? {
                        Some(img) => (img.w, img.h, (*img.px).clone()),
                        None => (32, 32, [128u8, 128, 128, 255].repeat(1024)),
                    };
                    let file = format!("{t}.png");
                    write_png(&output.join(&file), w, h, &pixels)?;
                    texture_list.push((t.clone(), json!({"file": file, "width": w, "height": h})));
                }
                texture = Some(t);
            }
            "gsSPVertex" => {
                let (address, count, slot) = (int0(&args[0])?, int0(&args[1])?, int0(&args[2])?);
                let base = (address & 0xffffff).div_euclid(16);
                for i in 0..count {
                    cache.insert(slot + i, base + i);
                }
            }
            "gsSP2Triangles" | "gsSP1Triangle" => {
                let nums: Vec<i64> = args.iter().map(|a| int0(a)).collect::<R<_>>()?;
                for ch in nums.chunks(4) {
                    let tri: Vec<i64> = ch.iter().take(3).map(|v| cache.get(v).copied().ok_or_else(|| format!("vertex slot {v} not loaded"))).collect::<R<_>>()?;
                    let key = (texture.clone(), tri.clone());
                    if seen.insert(key) {
                        if !groups.contains_key(&texture) {
                            group_order.push(texture.clone());
                        }
                        groups.entry(texture.clone()).or_default().extend(tri);
                    }
                }
            }
            _ => {}
        }
    }
    let mut g = Map::new();
    for k in &group_order {
        g.insert(k.clone().unwrap_or_else(|| "null".into()), json!(groups[k]));
    }
    let mut t = Map::new();
    for (k, v) in texture_list {
        t.insert(k, v);
    }
    let payload = json!({"source": "n64decomp/mk64", "romSha1": SHA1, "vertices": vertices, "groups": Value::Object(g), "textures": Value::Object(t), "path": path, "scale": 8});
    write(&output.join(&filename), dumps(&payload, None))?;
    Ok(true)
}

// ------------------------------------------------------------------------------------------------ karts

const KART_CHARACTERS: [&str; 8] = ["mario", "luigi", "yoshi", "toad", "donkeykong", "wario", "peach", "bowser"];
const KART_FRAMES: usize = 321;
const KART_ATLAS_COLUMNS: usize = 16;

fn kart_data(blocks: &mut Blocks, entry: &Value) -> R<Vec<u8>> {
    let start = offset_of(entry, "rom_offset")?;
    let n = u(entry, "width")? * u(entry, "height")? * if entry["type"].as_str() == Some("rgba16") { 2 } else { 1 };
    let b = blocks.block(start)?;
    Ok(sl(&b, 0, n).to_vec())
}

fn frame_pixels(blocks: &mut Blocks, kart: &Value, entry: &Value) -> R<Vec<u8>> {
    let mut tl = Vec::new();
    for t in entry["tlut"].as_array().ok_or("tlut")? {
        tl.extend(kart_data(blocks, &kart[t.as_str().ok_or("tlut name")?])?);
    }
    let palette = rgba16(&tl)?;
    let mut pixels = Vec::new();
    for &index in &kart_data(blocks, entry)? {
        pixels.extend_from_slice(sl(&palette, index as usize * 4, index as usize * 4 + 4));
    }
    Ok(pixels)
}

fn export_kart_frames(blocks: &mut Blocks, source: &Path, output: &Path) -> R<()> {
    let rows = KART_FRAMES.div_ceil(KART_ATLAS_COLUMNS);
    for name in KART_CHARACTERS {
        let kart = json_file(&source.join(format!("assets/karts/{name}_kart.json")))?;
        let width = 64 * KART_ATLAS_COLUMNS;
        let mut atlas = vec![0u8; width * 64 * rows * 4];
        for frame in 0..KART_FRAMES {
            let entry = &kart[format!("{name}_kart_frame{frame:03}")];
            let pixels = frame_pixels(blocks, &kart, entry)?;
            if pixels.len() != 64 * 256 {
                return Err(format!("{name} frame {frame}: {} bytes", pixels.len()));
            }
            let (ox, oy) = ((frame % KART_ATLAS_COLUMNS) * 64, (frame / KART_ATLAS_COLUMNS) * 64);
            for y in 0..64 {
                let dest = ((oy + y) * width + ox) * 4;
                atlas[dest..dest + 256].copy_from_slice(&pixels[y * 256..(y + 1) * 256]);
            }
        }
        write_png(&output.join(format!("{name}-kart-frames.png")), width, 64 * rows, &atlas)?;
    }
    Ok(())
}

const KART_EFFECT_TEXTURES: [(&str, usize, &str, usize, usize); 5] = [
    ("kart-shadow", 0x69A830, "i8", 64, 64),
    ("drift-puff-small", 0x69B03C, "i8", 16, 16),
    ("drift-puff-large", 0x69B140, "i8", 32, 32),
    ("ground-dust", 0x69C4E4, "i8", 32, 32),
    ("smoke-puff", 0x69C354, "ia8", 32, 32),
];

fn export_kart_effect_textures(rom: &[u8], output: &Path) -> R<()> {
    for (label, offset, kind, w, h) in KART_EFFECT_TEXTURES {
        let data = mio0(rom, offset)?;
        let mut pixels = Vec::new();
        if kind == "i8" {
            for &v in sl(&data, 0, w * h) {
                pixels.extend_from_slice(&[255, 255, 255, v]);
            }
        } else {
            for &b in sl(&data, 0, w * h) {
                let i = (b >> 4) * 17;
                pixels.extend_from_slice(&[i, i, i, (b & 15) * 17]);
            }
        }
        write_png(&output.join(format!("mk64-{label}.png")), w, h, &pixels)?;
    }
    let grass = mio0(rom, 0x69B378)?;
    write_png(&output.join("mk64-grass-particle.png"), 32, 64, &rgba16(sl(&grass, 0, 32 * 64 * 2))?)
}

/// `export(rom, output)` of export-mk64.py: everything under `public/mk64` and the walkable courses under `assets`.
pub fn export(rom: &[u8], source: &Path, output: &Path, assets: &Path, progress: &mut dyn FnMut(u32, &str)) -> R<()> {
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut textures = Textures::new(rom, source)?;
    for (i, c) in COURSES.iter().enumerate() {
        progress(5 + (i * 40 / COURSES.len()) as u32, &format!("Exporting {}", c.2));
        export_kart_course(&mut textures, source, c.0, output)?;
        let course = Course::new(source, c.0)?;
        export_walk(&course, &mut textures, source, assets)?;
    }
    progress(46, "Exporting the karts");
    let mut blocks = Blocks::new(rom);
    let mut names = vec![("mario".to_string(), "mario-kart.png".to_string())];
    for n in ["luigi", "peach", "toad", "yoshi", "wario", "bowser", "donkeykong"] {
        names.push((n.to_string(), format!("{n}-kart.png")));
    }
    for (name, out_name) in names {
        let kart = json_file(&source.join(format!("assets/karts/{name}_kart.json")))?;
        let mut atlas = vec![0u8; 64 * 35 * 64 * 4];
        for (column, frame) in (84..105).chain(235..249).enumerate() {
            let entry = &kart[format!("{name}_kart_frame{frame:03}")];
            let pixels = frame_pixels(&mut blocks, &kart, entry)?;
            for y in 0..64 {
                let dest = (y * 64 * 35 + column * 64) * 4;
                atlas[dest..dest + 256].copy_from_slice(&pixels[y * 256..(y + 1) * 256]);
            }
        }
        write_png(&output.join(out_name), 64 * 35, 64, &atlas)?;
    }
    progress(60, "Exporting every kart frame");
    export_kart_frames(&mut blocks, source, output)?;
    export_kart_effect_textures(rom, output)?;
    progress(85, "Exporting HUD textures");
    export_hud(&mut blocks, source, output)?;
    let common = mio0(rom, COMMON_DATA)?;
    export_item_curves(&common, output)?;
    export_item_sprites(rom, source, &common, output)?;
    Ok(())
}
