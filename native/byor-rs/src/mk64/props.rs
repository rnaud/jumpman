//! Course props of Mario Kart 64 as the decomp places them: the foliage (`spawn_foliage` in `racing/actors.c`:
//! trees of Mario Raceway, Yoshi Valley, Royal Raceway, Luigi Raceway, Moo Moo Farm and Frappe Snowland), exported as
//! `<course>-props.json` plus one PNG per tree picture (`prop-<course>-<n>.png`) in `public/mk64`. Rust only: the Python
//! exporter never wrote these files.
//!
//! The data comes from three places of the decomp:
//! * the `ActorSpawnData` array of the course (`{ { x, y, z }, { someId } }`, `-32768` ends it),
//! * the tree display list of `course_data.c` (`gsDPLoadTextureBlock(0x0300XXXX, ...)`, `gsSPVertex`, triangles). Segment 3
//!   holds the textures `actors.c` DMAs for the course in order from `0x9000`, `0x800` each, so the address picks the picture,
//! * the picture itself (`assets/trees.json` and the course's own asset table: CI8 32 x 64 plus its TLUT).
use super::common::{decode, rgba16, sl, Blocks};
use super::courses::{commands, Course, Textures};
use crate::pyc::{int, int0, re, read, R};
use crate::zpng::png_rgba;
use serde_json::{json, Value};
use std::path::Path;

/// What one course places.
struct Foliage {
    course: &'static str,
    /// The spawn array of `spawn_foliage`.
    spawns: &'static str,
    /// `(someId, display list)`: `someId` -1 matches every row (a course with one tree kind).
    kinds: &'static [(i64, &'static str)],
    /// The textures DMA'd into segment 3 from `0x9000` on, `0x800` apart (`actors.c`), with the asset table that
    /// describes them and the TLUT symbol.
    textures: &'static [&'static str],
    table: &'static str,
    tlut: &'static str,
    /// The Piranha Plants of the course (`spawn_piranha_plants`), drawn with 9 animation pictures.
    piranha: Option<Piranha>,
    /// More props that continue the segment-3 list after `textures` (Moo Moo Farm's cows).
    extra: Option<Extra>,
}

/// A second prop family of a course (`render_cows`): its own spawn array, display lists, pictures and TLUT. Its pictures follow
/// `Foliage::textures` in segment 3, so the display lists' addresses index one shared picture list.
struct Extra {
    spawns: &'static str,
    /// `(someId, display list)`; the model id exported is `100 + someId`.
    kinds: &'static [(i64, &'static str)],
    textures: &'static [&'static str],
    table: &'static str,
    tlut: &'static str,
}

/// `render_actor_piranha_plant`: a 30 x 30 quad whose picture is one of nine (`D_802BA058 + frame * 0x800`).
struct Piranha {
    spawns: &'static str,
    model: &'static str,
    table: &'static str,
    tlut: &'static str,
}

const FOLIAGE: &[Foliage] = &[
    Foliage { course: "mario_raceway", spawns: "d_course_mario_raceway_tree_spawns", kinds: &[(-1, "d_course_mario_raceway_dl_tree")], textures: &["gTextureTrees1"], table: "trees", tlut: "gTLUTTrees", piranha: Some(Piranha { spawns: "d_course_mario_raceway_piranha_plant_spawns", model: "d_course_mario_raceway_piranha_plant_model", table: "courses/mario_raceway", tlut: "gTLUTMarioRacewayPiranhaPlant" }), extra: None },
    Foliage { course: "yoshi_valley", spawns: "d_course_yoshi_valley_tree_spawn", kinds: &[(-1, "d_course_yoshi_valley_dl_tree")], textures: &["gTextureTrees2"], table: "trees", tlut: "gTLUTTrees", piranha: None, extra: None },
    Foliage {
        course: "royal_raceway",
        spawns: "d_course_royal_raceway_tree_spawn",
        kinds: &[(7, "d_course_royal_raceway_dl_tree"), (6, "d_course_royal_raceway_dl_castle_tree")],
        textures: &["gTextureTrees3", "gTextureTrees7"],
        table: "trees",
        tlut: "gTLUTTrees",
        piranha: Some(Piranha { spawns: "d_course_royal_raceway_piranha_plant_spawn", model: "d_course_royal_raceway_piranha_plant_model", table: "courses/royal_raceway", tlut: "gTLUTRoyalRacewayPiranhaPlant" }),
        extra: None,
    },
    Foliage { course: "luigi_raceway", spawns: "d_course_luigi_raceway_tree_spawn", kinds: &[(-1, "d_course_luigi_raceway_dl_FC70")], textures: &["gTextureTrees5Left", "gTextureTrees5Right"], table: "trees", tlut: "gTLUTTrees", piranha: None, extra: None },
    Foliage { course: "moo_moo_farm", spawns: "d_course_moo_moo_farm_tree_spawn", kinds: &[(-1, "d_course_moo_moo_farm_dl_tree")], textures: &["gTextureTrees4Left", "gTextureTrees4Right"], table: "trees", tlut: "gTLUTTrees", piranha: None,
        extra: Some(Extra {
            spawns: "d_course_moo_moo_farm_cow_spawn",
            kinds: &[(0, "d_course_moo_moo_farm_dl_cow1"), (1, "d_course_moo_moo_farm_dl_cow2"), (2, "d_course_moo_moo_farm_dl_cow3"), (3, "d_course_moo_moo_farm_dl_cow4"), (4, "d_course_moo_moo_farm_dl_cow5")],
            textures: &["gTextureCow01Left", "gTextureCow01Right", "gTextureCow02Left", "gTextureCow02Right", "gTextureCow03Left", "gTextureCow03Right", "gTextureCow04Left", "gTextureCow04Right", "gTextureCow05Left", "gTextureCow05Right"],
            table: "courses/moo_moo_farm",
            tlut: "gTLUTCow",
        }) },
    Foliage {
        course: "frappe_snowland",
        spawns: "d_course_frappe_snowland_tree_spawns",
        kinds: &[(-1, "d_course_frappe_snowland_dl_tree")],
        textures: &["gTextureFrappeSnowlandTreeLeft", "gTextureFrappeSnowlandTreeRight"],
        table: "courses/frappe_snowland",
        tlut: "gTLUTFrappeSnowlandTree",
        piranha: None,
        extra: None,
    },
    Foliage {
        course: "kalimari_desert",
        spawns: "d_course_kalimari_desert_cactus_spawn",
        kinds: &[(5, "d_course_kalimari_desert_dl_cactus1"), (6, "d_course_kalimari_desert_dl_cactus2"), (7, "d_course_kalimari_desert_dl_cactus3")],
        textures: &["gTextureCactus1Left", "gTextureCactus1Right", "gTextureCactus2Left", "gTextureCactus2Right", "gTextureCactus3"],
        table: "courses/kalimari_desert",
        tlut: "gTLUTCactus",
        piranha: None,
        extra: None,
    },
];

/// Props whose pictures are RGBA16 textures of the course's own data (`gsDPSetTextureImage` of a `u8 name[]` symbol, or the
/// shared shrub picture at segment 3), some of them standing still instead of facing the camera.
struct Plain {
    course: &'static str,
    /// The spawn array.
    spawns: &'static str,
    /// `UnkActorSpawnData` rows (`{ { x, y, z }, someId, unk8 }`, Donkey Kong's Jungle Parkway) instead of `ActorSpawnData`.
    unk: bool,
    /// `(someId & 0xF or -1, display list, faces the camera)`.
    kinds: &'static [(i64, &'static str, bool)],
    /// Raw segment-3 address -> texture symbol (`render_actor_bush_bowser_castle` loads `gTextureShrub` at `0x03009000`).
    segment3: &'static [(i64, &'static str)],
    /// The decomp keeps the spawn height (`pos[1]`); false lets the renderer settle the prop on the floor below it.
    fixed_height: bool,
}

const PLAIN: &[Plain] = &[
    // `spawn_foliage` with `actorType` 0x21 and the bush display list.
    Plain { course: "bowsers_castle", spawns: "d_course_bowsers_castle_tree_spawn", unk: false, kinds: &[(-1, "d_course_bowsers_castle_dl_bush", true)], segment3: &[(0x0300_9000, "gTextureShrub")], fixed_height: false },
    // `render_palm_trees`: someId 0 / 4 / 5 are camera-facing trees, 6 the crossed-quad palm turned by `mtxf_rotate_zxy_translate` (no turn).
    Plain {
        course: "dks_jungle_parkway",
        spawns: "d_course_dks_jungle_parkway_tree_spawn",
        unk: true,
        kinds: &[(0, "d_course_dks_jungle_parkway_dl_tree1", true), (4, "d_course_dks_jungle_parkway_dl_tree2", true), (5, "d_course_dks_jungle_parkway_dl_tree3", true), (6, "d_course_dks_jungle_parkway_dl_palm_tree", false)],
        segment3: &[],
        fixed_height: true,
    },
];

/// `u8 symbol[] = { #include "assets/courses/<dir>/<asset>.inc.c" };` -> `(symbol, asset)`.
pub fn picture_symbols(text: &str) -> Vec<(String, String)> {
    re(r#"u8\s+(\w+)\[\]\s*=\s*\{\s*#include\s+"assets/courses/\w+/(\w+)\.inc\.c""#).captures_iter(text).map(|c| (c[1].to_string(), c[2].to_string())).collect()
}

/// `UnkActorSpawnData` rows `(x, y, z, someId)`; the array ends with `-32768`.
pub fn unk_spawn_rows(text: &str, name: &str) -> R<Vec<[i64; 4]>> {
    let body = re(&format!(r"(?s)struct\s+UnkActorSpawnData\s+{}\s*\[\]\s*=\s*\{{(.*?)\n\}};", regex_escape(name))).captures(text).ok_or_else(|| format!("no spawn array {name}"))?[1].to_string();
    let mut out = vec![];
    for c in re(r"\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},\s*(-?\d+),\s*-?\d+\s*\}").captures_iter(&body) {
        let row = [int(&c[1])?, int(&c[2])?, int(&c[3])?, int(&c[4])? & 0xF];
        if row[0] != -32768 {
            out.push(row);
        }
    }
    Ok(out)
}

/// One textured part of a prop model: vertices as `[x, y, z, u, v]` with `u` / `v` in texels and triangles into them.
#[derive(Debug, PartialEq)]
pub struct Part {
    pub image: usize,
    pub verts: Vec<[f64; 5]>,
    pub tris: Vec<[usize; 3]>,
}

/// `gsSPVertex` source: `Vtx name[] = { { { x, y, z }, 0, { s, t }, { r, g, b, a } }, ... }` -> `[x, y, z, s, t]`.
pub fn vtx_array(text: &str, name: &str) -> R<Vec<[f64; 5]>> {
    let body = re(&format!(r"(?s)Vtx\s+{}\s*\[\d*\]\s*=\s*\{{(.*?)\n\}};", regex_escape(name))).captures(text).ok_or_else(|| format!("no Vtx {name}"))?[1].to_string();
    let row = re(r"\{\s*\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},\s*\d+,\s*\{\s*(-?\d+),\s*(-?\d+)\s*\}");
    row.captures_iter(&body).map(|c| Ok([int(&c[1])? as f64, int(&c[2])? as f64, int(&c[3])? as f64, int(&c[4])? as f64, int(&c[5])? as f64])).collect()
}

fn regex_escape(s: &str) -> String {
    s.chars().flat_map(|c| if c.is_alphanumeric() || c == '_' { vec![c] } else { vec!['\\', c] }).collect()
}

/// Walks a tree display list: each `gsDPLoadTextureBlock(0x0300XXXX, ...)` starts a part whose picture is
/// `(address - 0x03009000) / 0x800` of the course's segment-3 list.
pub fn parse_tree_dl(text: &str, dl: &str) -> R<Vec<Part>> {
    parse_dl(text, dl, |token| {
        let Ok(address) = int0(token) else { return Ok(None) };
        Ok((0x0300_9000..0x0301_0000).contains(&address).then(|| ((address - 0x0300_9000) / 0x800) as usize))
    })
}

/// The display-list walker behind every prop: `picture` turns the token of a texture load (`gsDPLoadTextureBlock`'s first
/// argument, or the address of a raw `gsDPSetTextureImage` + `gsDPLoadBlock`; a number or a symbol) into a picture index, or
/// `None` for a load that is not a prop picture.
pub fn parse_dl(text: &str, dl: &str, mut picture: impl FnMut(&str) -> R<Option<usize>>) -> R<Vec<Part>> {
    let body = re(&format!(r"(?s)Gfx\s+{}\s*\[\]\s*=\s*\{{(.*?)\n\}};", regex_escape(dl))).captures(text).ok_or_else(|| format!("no Gfx {dl}"))?[1].to_string();
    let mut parts: Vec<Part> = Vec::new();
    let mut model: Vec<[f64; 5]> = Vec::new();
    let mut image = None;
    for (command, args) in commands(&body) {
        match command.as_str() {
            "gsDPLoadTextureBlock" => {
                if let Some(i) = picture(&args[0])? {
                    image = Some(i);
                }
            }
            // The cows load their pictures with a raw `gsDPSetTextureImage(fmt, size, 1, address)` + `gsDPLoadBlock`.
            "gsDPSetTextureImage" if args.len() > 3 => {
                if let Some(i) = picture(&args[3])? {
                    image = Some(i);
                }
            }
            "gsSPVertex" => model = vtx_array(text, &args[0])?,
            "gsSP1Triangle" | "gsSP2Triangles" => {
                let img = image.ok_or("triangles before a texture")?;
                if parts.last().is_none_or(|p| p.image != img) {
                    parts.push(Part { image: img, verts: vec![], tris: vec![] });
                }
                let part = parts.last_mut().unwrap();
                let nums: Vec<i64> = args.iter().map(|a| int0(a)).collect::<R<_>>()?;
                for ch in nums.chunks(4) {
                    let mut tri = [0usize; 3];
                    for (k, v) in ch.iter().take(3).enumerate() {
                        let vert = *model.get(*v as usize).ok_or("vertex slot not loaded")?;
                        let at = part.verts.iter().position(|p| *p == vert).unwrap_or_else(|| {
                            part.verts.push(vert);
                            part.verts.len() - 1
                        });
                        tri[k] = at;
                    }
                    part.tris.push(tri);
                }
            }
            _ => {}
        }
    }
    Ok(parts)
}

/// `ActorSpawnData` rows `(x, y, z, someId)` of a course's array (the `-32768` terminator excluded).
pub fn spawn_rows(text: &str, name: &str) -> R<Vec<[i64; 4]>> {
    let body = re(&format!(r"(?s)ActorSpawnData\s+{}\s*\[\]\s*=\s*\{{(.*?)\}};", regex_escape(name))).captures(text).ok_or_else(|| format!("no spawn array {name}"))?[1].to_string();
    let mut out = vec![];
    for c in re(r"\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},\s*\{\s*(-?\d+)\s*\}").captures_iter(&body) {
        let row = [int(&c[1])?, int(&c[2])?, int(&c[3])?, int(&c[4])?];
        if row[0] != -32768 {
            out.push(row);
        }
    }
    Ok(out)
}

fn table(source: &Path, name: &str) -> R<Value> {
    serde_json::from_str(&read(&source.join("assets").join(format!("{name}.json")))?).map_err(|e| e.to_string())
}

fn picture(blocks: &mut Blocks, table: &Value, symbol: &str, tlut_table: &Value, tlut: &str) -> R<(usize, usize, Vec<u8>)> {
    let entry = table.get(symbol).ok_or_else(|| format!("no asset {symbol}"))?;
    let (w, h) = (entry["width"].as_u64().ok_or("w")? as usize, entry["height"].as_u64().ok_or("h")? as usize);
    let start = int0(entry["rom_offset"].as_str().ok_or("rom_offset")?)? as usize;
    let data = blocks.block(start)?;
    let t = tlut_table.get(tlut).ok_or_else(|| format!("no TLUT {tlut}"))?;
    let (tstart, toff) = (int0(t["rom_offset"].as_str().ok_or("tlut rom_offset")?)? as usize, t.get("block_offset").map_or(Ok(0), |b| int0(b.as_str().unwrap_or("0x0")))? as usize);
    let tblock = blocks.block(tstart)?;
    let palette = rgba16(sl(&tblock, toff, toff + 512))?;
    Ok((w, h, decode("ci8", &data, w, h, Some(&palette))?))
}

/// Writes `<course>-props.json` and its pictures when the course has foliage; returns whether it did.
pub fn export(course: &Course, textures: &mut Textures, source: &Path, output: &Path) -> R<bool> {
    if let Some(p) = PLAIN.iter().find(|p| p.course == course.name) {
        return export_plain(course, p, textures, output);
    }
    let blocks = &mut textures.blocks;
    let Some(f) = FOLIAGE.iter().find(|f| f.course == course.name) else { return Ok(false) };
    let text = course.data_text();
    let ftable = table(source, f.table)?;
    let mut images = vec![];
    for (i, symbol) in f.textures.iter().enumerate() {
        let (w, h, px) = picture(blocks, &ftable, symbol, &ftable, f.tlut)?;
        let file = format!("prop-{}-{}.png", f.course.replace('_', "-"), i);
        std::fs::write(output.join(&file), png_rgba(w, h, &px, 6)?).map_err(|e| e.to_string())?;
        images.push(json!({"file": file, "width": w, "height": h}));
    }
    let mut models = vec![];
    for (id, dl) in f.kinds {
        let parts = parse_tree_dl(text, dl)?;
        models.push(json!({"id": id, "parts": parts.iter().map(|p| json!({"image": p.image, "verts": p.verts, "tris": p.tris})).collect::<Vec<_>>()}));
    }
    let rows = spawn_rows(text, f.spawns)?;
    let mut props: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id = if f.kinds.iter().any(|(k, _)| *k == r[3]) { r[3] } else { -1 };
            json!({"model": id, "pos": [r[0], r[1], r[2]]})
        })
        .collect();
    if let Some(x) = &f.extra {
        // The cows' pictures continue the segment-3 list after the trees (indices 2 on).
        let xtable = table(source, x.table)?;
        for symbol in x.textures {
            let (w, h, px) = picture(blocks, &xtable, symbol, &xtable, x.tlut)?;
            let file = format!("prop-{}-{}.png", f.course.replace('_', "-"), images.len());
            std::fs::write(output.join(&file), png_rgba(w, h, &px, 6)?).map_err(|e| e.to_string())?;
            images.push(json!({"file": file, "width": w, "height": h}));
        }
        for (id, dl) in x.kinds {
            let parts = parse_tree_dl(text, dl)?;
            models.push(json!({"id": 100 + id, "parts": parts.iter().map(|p| json!({"image": p.image, "verts": p.verts, "tris": p.tris})).collect::<Vec<_>>()}));
        }
        for r in spawn_rows(text, x.spawns)? {
            props.push(json!({"model": 100 + r[3], "pos": [r[0], r[1], r[2]]}));
        }
    }
    if let Some(p) = &f.piranha {
        // Nine pictures after the trees; the quad shows the first and the renderer swaps through all nine near a kart.
        // The nine pictures are Mario Raceway's (Royal Raceway reuses them with its own TLUT).
        let ptable = table(source, "courses/mario_raceway")?;
        let tlut_table = table(source, p.table)?;
        let first = images.len();
        for n in 1..=9 {
            let (w, h, px) = picture(blocks, &ptable, &format!("gTexturePiranhaPlant{n}"), &tlut_table, p.tlut)?;
            let file = format!("prop-{}-{}.png", f.course.replace('_', "-"), images.len());
            std::fs::write(output.join(&file), png_rgba(w, h, &px, 6)?).map_err(|e| e.to_string())?;
            // The quad's s runs 0..2048 over a 32 texel wide picture with a mirrored wrap.
            images.push(json!({"file": file, "width": w, "height": h, "mirrorU": true}));
        }
        let verts = vtx_array(text, p.model)?;
        models.push(json!({"id": -2, "parts": [{"image": first, "verts": verts, "tris": [[0, 1, 2], [0, 2, 3]]}], "frames": (first..first + 9).collect::<Vec<_>>(), "animDistance": 300, "frameTicks": 6}));
        for r in spawn_rows(text, p.spawns)? {
            props.push(json!({"model": -2, "pos": [r[0], r[1], r[2]]}));
        }
    }
    let payload = json!({"source": "n64decomp/mk64", "images": images, "models": models, "props": props});
    std::fs::write(output.join(format!("{}-props.json", f.course.replace('_', "-"))), serde_json::to_vec(&payload).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(true)
}

/// `<course>-props.json` of a `Plain` course: the models walked from the display lists, their RGBA16 pictures read through
/// the course texture reader, one prop per spawn row.
fn export_plain(course: &Course, p: &Plain, textures: &mut Textures, output: &Path) -> R<bool> {
    let text = course.data_text();
    let symbols = picture_symbols(text);
    let mut assets: Vec<String> = vec![];
    let mut models = vec![];
    let mut pick = |token: &str| -> R<Option<usize>> {
        let asset = match int0(token) {
            Ok(address) => match p.segment3.iter().find(|(a, _)| *a == address) {
                Some((_, a)) => a.to_string(),
                None => return Ok(None),
            },
            Err(_) => match symbols.iter().find(|(s, _)| s == token) {
                Some((_, a)) => a.clone(),
                None => return Ok(None),
            },
        };
        Ok(Some(assets.iter().position(|a| *a == asset).unwrap_or_else(|| {
            assets.push(asset);
            assets.len() - 1
        })))
    };
    for (id, dl, billboard) in p.kinds {
        let parts = parse_dl(text, dl, &mut pick)?;
        models.push(json!({"id": id, "billboard": billboard, "parts": parts.iter().map(|p| json!({"image": p.image, "verts": p.verts, "tris": p.tris})).collect::<Vec<_>>()}));
    }
    let mut images = vec![];
    for (i, asset) in assets.iter().enumerate() {
        let img = textures.image(asset)?.ok_or_else(|| format!("no picture {asset}"))?;
        let file = format!("prop-{}-{}.png", p.course.replace('_', "-"), i);
        std::fs::write(output.join(&file), png_rgba(img.w, img.h, &img.px, 6)?).map_err(|e| e.to_string())?;
        images.push(json!({"file": file, "width": img.w, "height": img.h}));
    }
    let rows = if p.unk { unk_spawn_rows(text, p.spawns)? } else { spawn_rows(text, p.spawns)? };
    let props: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id = if p.kinds.iter().any(|(k, _, _)| *k == r[3]) { r[3] } else { -1 };
            json!({"model": id, "pos": [r[0], r[1], r[2]], "fixed": p.fixed_height})
        })
        .collect();
    let payload = json!({"source": "n64decomp/mk64", "images": images, "models": models, "props": props});
    std::fs::write(output.join(format!("{}-props.json", p.course.replace('_', "-"))), serde_json::to_vec(&payload).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARIO: &str = r#"
Vtx d_course_mario_raceway_tree_model[4] = {
    { { { 40, 10, 0 }, 0, { 1536, 1706 }, { 0xFF, 0xFE, 0xFE, 0xFF } } },
    { { { 0, 100, 0 }, 0, { 512, -341 }, { 0xFF, 0xFE, 0xFE, 0xFF } } },
    { { { -40, 10, 0 }, 0, { -512, 1706 }, { 0xFF, 0xFE, 0xFE, 0xFF } } },
    { { { 0, -10, 0 }, 0, { 512, 2161 }, { 0xFF, 0xFE, 0xFE, 0xFF } } },
};

Gfx d_course_mario_raceway_dl_tree[] = {
    gsDPLoadTextureBlock(0x03009000, G_IM_FMT_CI, G_IM_SIZ_8b, 32, 64, 0, G_TX_NOMIRROR | G_TX_CLAMP,
                         G_TX_NOMIRROR | G_TX_CLAMP, G_TX_NOMASK, G_TX_NOMASK, G_TX_NOLOD, G_TX_NOLOD),
    gsSPVertex(d_course_mario_raceway_tree_model, 4, 0),
    gsSP1Triangle(0, 1, 2, 0),
    gsSP1Triangle(0, 2, 3, 0),
    gsSPEndDisplayList(),
};

struct ActorSpawnData d_course_mario_raceway_tree_spawns[] = {
    { { 228, 0, -1069 }, { 7 } }, { { 372, 2, -1046 }, { 7 } },
    { { -32768, 0, 0 }, { 0 } },
};
"#;

    #[test]
    fn a_single_picture_tree_is_one_quad_of_two_triangles() {
        let parts = parse_tree_dl(MARIO, "d_course_mario_raceway_dl_tree").unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].image, 0);
        assert_eq!(parts[0].verts.len(), 4);
        assert_eq!(parts[0].tris, vec![[0, 1, 2], [0, 2, 3]]);
        assert_eq!(parts[0].verts[1], [0., 100., 0., 512., -341.]);
    }

    #[test]
    fn spawn_rows_stop_at_the_terminator() {
        assert_eq!(spawn_rows(MARIO, "d_course_mario_raceway_tree_spawns").unwrap(), vec![[228, 0, -1069, 7], [372, 2, -1046, 7]]);
    }

    #[test]
    fn two_picture_trees_split_into_left_and_right_parts() {
        let text = r#"
Vtx m[8] = {
    { { { 0, 0, 0 }, 0, { 0, 0 }, { 1, 1, 1, 1 } } }, { { { 1, 0, 0 }, 0, { 1, 0 }, { 1, 1, 1, 1 } } },
    { { { 1, 1, 0 }, 0, { 1, 1 }, { 1, 1, 1, 1 } } }, { { { 0, 1, 0 }, 0, { 0, 1 }, { 1, 1, 1, 1 } } },
    { { { 2, 0, 0 }, 0, { 0, 0 }, { 1, 1, 1, 1 } } }, { { { 3, 0, 0 }, 0, { 1, 0 }, { 1, 1, 1, 1 } } },
    { { { 3, 1, 0 }, 0, { 1, 1 }, { 1, 1, 1, 1 } } }, { { { 2, 1, 0 }, 0, { 0, 1 }, { 1, 1, 1, 1 } } },
};
Gfx t[] = {
    gsDPLoadTextureBlock(0x03009000, G_IM_FMT_CI, G_IM_SIZ_8b, 32, 64, 0),
    gsSPVertex(m, 8, 0),
    gsSP2Triangles(0, 1, 2, 0, 0, 2, 3, 0),
    gsDPLoadTextureBlock(0x03009800, G_IM_FMT_CI, G_IM_SIZ_8b, 32, 64, 0),
    gsSP2Triangles(4, 5, 6, 0, 4, 6, 7, 0),
    gsSPEndDisplayList(),
};
"#;
        let parts = parse_tree_dl(text, "t").unwrap();
        assert_eq!(parts.iter().map(|p| p.image).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(parts[1].verts.len(), 4);
        assert_eq!(parts[1].tris, vec![[0, 1, 2], [0, 2, 3]]);
    }

    #[test]
    fn a_course_local_display_list_resolves_its_pictures_by_symbol() {
        let text = r#"
// 0xFCC0
u8 d_course_x_tree2_top[] = {
#include "assets/courses/x/gTextureXTree2Top.inc.c"
};
u8 d_course_x_trunk[] = {
#include "assets/courses/x/gTextureXTrunk.inc.c"
};
Vtx d_course_x_model[] = {
    { { { 23, 75, 0 }, 0, { 1024, 0 }, { 0xFF, 0xFF, 0xFF, 0xFF } } },
    { { { -23, 75, 0 }, 0, { 0, 0 }, { 0xFF, 0xFF, 0xFF, 0xFF } } },
    { { { -23, 45, 0 }, 0, { 0, 1024 }, { 0xFF, 0xFF, 0xFF, 0xFF } } },
    { { { 23, 45, 0 }, 0, { 1024, 1024 }, { 0xFF, 0xFF, 0xFF, 0xFF } } },
    { { { 23, 45, 0 }, 0, { 1664, 0 }, { 0x73, 0x73, 0x73, 0xFF } } },
    { { { -23, 45, 0 }, 0, { -640, 0 }, { 0x73, 0x73, 0x73, 0xFF } } },
    { { { -23, 0, 0 }, 0, { -640, 1024 }, { 0x73, 0x73, 0x73, 0xFF } } },
    { { { 23, 0, 0 }, 0, { 1664, 1024 }, { 0x73, 0x73, 0x73, 0xFF } } },
};
Gfx d_course_x_dl_tree[] = {
    gsSPVertex(d_course_x_model, 8, 0),
    gsDPSetTextureImage(G_IM_FMT_RGBA, G_IM_SIZ_16b, 1, d_course_x_tree2_top),
    gsDPLoadBlock(G_TX_LOADTILE, 0, 0, 1023, 256),
    gsSP2Triangles(0, 1, 2, 0, 0, 2, 3, 0),
    gsDPSetTextureImage(G_IM_FMT_RGBA, G_IM_SIZ_16b, 1, d_course_x_trunk),
    gsSP2Triangles(4, 5, 6, 0, 4, 6, 7, 0),
    gsSPEndDisplayList(),
};
struct UnkActorSpawnData d_course_x_tree_spawn[] = {
    { { 141, 10, 89 }, 0, 10 },      { { -104, 12, 35 }, 4, 12 },     { { 7, -10, 7 }, 1030, -10 },
    { { -32768, 0, 0 }, 0, 0 },
};
"#;
        let symbols = picture_symbols(text);
        assert_eq!(symbols, vec![("d_course_x_tree2_top".to_string(), "gTextureXTree2Top".to_string()), ("d_course_x_trunk".to_string(), "gTextureXTrunk".to_string())]);
        let parts = parse_dl(text, "d_course_x_dl_tree", |t| Ok(symbols.iter().position(|(s, _)| s == t))).unwrap();
        assert_eq!(parts.iter().map(|p| p.image).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(parts[1].verts[2], [-23., 0., 0., -640., 1024.]);
        // The terminator ends the rows; only the low nibble of someId is the kind (the high byte holds flags).
        assert_eq!(unk_spawn_rows(text, "d_course_x_tree_spawn").unwrap(), vec![[141, 10, 89, 0], [-104, 12, 35, 4], [7, -10, 7, 6]]);
    }

    #[test]
    fn the_foliage_walker_ignores_loads_that_are_not_prop_pictures() {
        // A TLUT load or a picture outside segment 3 leaves the current picture alone.
        let text = "Vtx v[3] = {\n    { { { 0, 0, 0 }, 0, { 0, 0 }, { 1, 1, 1, 1 } } }, { { { 1, 0, 0 }, 0, { 1, 0 }, { 1, 1, 1, 1 } } }, { { { 1, 1, 0 }, 0, { 1, 1 }, { 1, 1, 1, 1 } } },\n};\nGfx g[] = {\n    gsDPSetTextureImage(G_IM_FMT_RGBA, G_IM_SIZ_16b, 1, 0x0A000000),\n    gsDPLoadTextureBlock(0x03009000, G_IM_FMT_CI, G_IM_SIZ_8b, 32, 64, 0),\n    gsSPVertex(v, 3, 0),\n    gsSP1Triangle(0, 1, 2, 0),\n    gsSPEndDisplayList(),\n};\n";
        let parts = parse_tree_dl(text, "g").unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].image, 0);
    }
}
