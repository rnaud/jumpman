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
use super::courses::{commands, Course};
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
}

const FOLIAGE: &[Foliage] = &[
    Foliage { course: "mario_raceway", spawns: "d_course_mario_raceway_tree_spawns", kinds: &[(-1, "d_course_mario_raceway_dl_tree")], textures: &["gTextureTrees1"], table: "trees", tlut: "gTLUTTrees" },
    Foliage { course: "yoshi_valley", spawns: "d_course_yoshi_valley_tree_spawn", kinds: &[(-1, "d_course_yoshi_valley_dl_tree")], textures: &["gTextureTrees2"], table: "trees", tlut: "gTLUTTrees" },
    Foliage {
        course: "royal_raceway",
        spawns: "d_course_royal_raceway_tree_spawn",
        kinds: &[(7, "d_course_royal_raceway_dl_tree"), (6, "d_course_royal_raceway_dl_castle_tree")],
        textures: &["gTextureTrees3", "gTextureTrees7"],
        table: "trees",
        tlut: "gTLUTTrees",
    },
    Foliage { course: "luigi_raceway", spawns: "d_course_luigi_raceway_tree_spawn", kinds: &[(-1, "d_course_luigi_raceway_dl_FC70")], textures: &["gTextureTrees5Left", "gTextureTrees5Right"], table: "trees", tlut: "gTLUTTrees" },
    Foliage { course: "moo_moo_farm", spawns: "d_course_moo_moo_farm_tree_spawn", kinds: &[(-1, "d_course_moo_moo_farm_dl_tree")], textures: &["gTextureTrees4Left", "gTextureTrees4Right"], table: "trees", tlut: "gTLUTTrees" },
    Foliage {
        course: "frappe_snowland",
        spawns: "d_course_frappe_snowland_tree_spawns",
        kinds: &[(-1, "d_course_frappe_snowland_dl_tree")],
        textures: &["gTextureFrappeSnowlandTreeLeft", "gTextureFrappeSnowlandTreeRight"],
        table: "courses/frappe_snowland",
        tlut: "gTLUTFrappeSnowlandTree",
    },
];

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
    let body = re(&format!(r"(?s)Gfx\s+{}\s*\[\]\s*=\s*\{{(.*?)\n\}};", regex_escape(dl))).captures(text).ok_or_else(|| format!("no Gfx {dl}"))?[1].to_string();
    let mut parts: Vec<Part> = Vec::new();
    let mut model: Vec<[f64; 5]> = Vec::new();
    let mut image = None;
    for (command, args) in commands(&body) {
        match command.as_str() {
            "gsDPLoadTextureBlock" => {
                let address = int0(&args[0])?;
                image = Some(((address - 0x0300_9000) / 0x800) as usize);
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

fn picture(blocks: &mut Blocks, table: &Value, symbol: &str, tlut: &str) -> R<(usize, usize, Vec<u8>)> {
    let entry = table.get(symbol).ok_or_else(|| format!("no asset {symbol}"))?;
    let (w, h) = (entry["width"].as_u64().ok_or("w")? as usize, entry["height"].as_u64().ok_or("h")? as usize);
    let start = int0(entry["rom_offset"].as_str().ok_or("rom_offset")?)? as usize;
    let data = blocks.block(start)?;
    let t = table.get(tlut).ok_or_else(|| format!("no TLUT {tlut}"))?;
    let (tstart, toff) = (int0(t["rom_offset"].as_str().ok_or("tlut rom_offset")?)? as usize, t.get("block_offset").map_or(Ok(0), |b| int0(b.as_str().unwrap_or("0x0")))? as usize);
    let tblock = blocks.block(tstart)?;
    let palette = rgba16(sl(&tblock, toff, toff + 512))?;
    Ok((w, h, decode("ci8", &data, w, h, Some(&palette))?))
}

/// Writes `<course>-props.json` and its pictures when the course has foliage; returns whether it did.
pub fn export(course: &Course, blocks: &mut Blocks, source: &Path, output: &Path) -> R<bool> {
    let Some(f) = FOLIAGE.iter().find(|f| f.course == course.name) else { return Ok(false) };
    let text = course.data_text();
    let table = table(source, f.table)?;
    let mut images = vec![];
    for (i, symbol) in f.textures.iter().enumerate() {
        let (w, h, px) = picture(blocks, &table, symbol, f.tlut)?;
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
    let props: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id = if f.kinds.iter().any(|(k, _)| *k == r[3]) { r[3] } else { -1 };
            json!({"model": id, "pos": [r[0], r[1], r[2]]})
        })
        .collect();
    let payload = json!({"source": "n64decomp/mk64", "images": images, "models": models, "props": props});
    std::fs::write(output.join(format!("{}-props.json", f.course.replace('_', "-"))), serde_json::to_vec(&payload).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
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
}
