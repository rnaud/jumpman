//! Port of `scripts/mk64_courses.py` + `scripts/mk64_props.py`: the table-driven Mario Kart 64 course exporter
//! (decomp display lists and vertex tables -> `assets/levels/mk64_<dir>/terrain.glb` and `assets/courses/mk64_<dir>.json`).
use super::common::{decode, sl, Blocks};
use crate::pyc::{dumps, int, int0, re, read, round, R};
use crate::zpng::{png_rgba, Num};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// dir -> (texture symbol prefix, display name, kind)
pub const COURSES: [(&str, &str, &str, &str); 20] = [
    ("mario_raceway", "MR", "Mario Raceway", "race"),
    ("luigi_raceway", "LR", "Luigi Raceway", "race"),
    ("koopa_troopa_beach", "KTB", "Koopa Troopa Beach", "race"),
    ("moo_moo_farm", "MMF", "Moo Moo Farm", "race"),
    ("choco_mountain", "CM", "Choco Mountain", "race"),
    ("kalimari_desert", "KD", "Kalimari Desert", "race"),
    ("toads_turnpike", "TT", "Toad's Turnpike", "race"),
    ("frappe_snowland", "FS", "Frappe Snowland", "race"),
    ("wario_stadium", "WS", "Wario Stadium", "race"),
    ("sherbet_land", "SL", "Sherbet Land", "race"),
    ("royal_raceway", "RRW", "Royal Raceway", "race"),
    ("bowsers_castle", "BC", "Bowser's Castle", "race"),
    ("dks_jungle_parkway", "DKJ", "D.K.'s Jungle Parkway", "race"),
    ("yoshi_valley", "YV", "Yoshi Valley", "race"),
    ("banshee_boardwalk", "BB", "Banshee Boardwalk", "race"),
    ("rainbow_road", "RR", "Rainbow Road", "race"),
    ("big_donut", "BD", "Big Donut", "battle"),
    ("block_fort", "BF", "Block Fort", "battle"),
    ("double_deck", "DD", "Double Deck", "battle"),
    ("skyscraper", "SS", "Skyscraper", "battle"),
];

fn course_info(name: &str) -> (&'static str, &'static str, &'static str) {
    let c = COURSES.iter().find(|c| c.0 == name).expect("course");
    (c.1, c.2, c.3)
}

fn extra_collision(name: &str) -> Vec<(i64, &'static str)> {
    match name {
        "mario_raceway" => vec![(0x1140, "-1"), (0x08E8, "-1")],
        "block_fort" => vec![(0x15C0, "ASPHALT")],
        "skyscraper" => vec![(0x1110, "ASPHALT"), (0x0258, "ASPHALT")],
        "double_deck" => vec![(0x0738, "ASPHALT")],
        "big_donut" => vec![(0x1018, "BRIDGE"), (0x0450, "BRIDGE"), (0x0AC0, "BRIDGE"), (0x0B58, "BRIDGE"), (0x0230, "BRIDGE")],
        _ => vec![],
    }
}

fn translucent_lists(name: &str) -> Vec<(i64, i64)> {
    match name {
        "koopa_troopa_beach" => vec![(0xADE0, 150), (0xA540, 150), (0x9E70, 150), (0x0358, 150)],
        "sherbet_land" => vec![(0x1EB8, 180), (0x2308, 150)],
        "rainbow_road" => vec![(0x2068, 150), (0x1E18, 150), (0x1318, 255), (0x1FB8, 150)],
        "wario_stadium" => vec![(0x0C50, 100), (0x0BD8, 100), (0x0B60, 100), (0x0AE8, 100), (0x0CC8, 100), (0x0D50, 100), (0x0DD0, 100), (0x0E48, 100)],
        "dks_jungle_parkway" => vec![(0x3FA8, 120)],
        _ => vec![],
    }
}

const SM64_DEFAULT: i64 = 0x00;
const SM64_NOT_SLIPPERY: i64 = 0x15;
const SM64_SLIPPERY: i64 = 0x14;
const SM64_VERY_SLIPPERY: i64 = 0x13;
const SM64_DEATH_PLANE: i64 = 0x0A;

fn surface_kind(name: &str) -> i64 {
    match name {
        "SAND_OFFROAD" | "GRASS" | "DIRT_OFFROAD" | "SNOW_OFFROAD" => SM64_NOT_SLIPPERY,
        "SNOW" => SM64_SLIPPERY,
        "ICE" => SM64_VERY_SLIPPERY,
        "OUT_OF_BOUNDS" => SM64_DEATH_PLANE,
        _ => SM64_DEFAULT,
    }
}

pub const WALK_SCALE: i64 = 8;

// ------------------------------------------------------------------------------------------------ mk64_props

fn surface_id(name: &str) -> i64 {
    match name {
        "AIRBORNE" => 0,
        "ASPHALT" => 1,
        "DIRT" => 2,
        "SAND" => 3,
        "STONE" => 4,
        "SNOW" => 5,
        "BRIDGE" => 6,
        "SAND_OFFROAD" => 7,
        "GRASS" => 8,
        "ICE" => 9,
        "WET_SAND" => 0xA,
        "SNOW_OFFROAD" => 0xB,
        "CLIFF" => 0xC,
        "DIRT_OFFROAD" => 0xD,
        "TRAIN_TRACK" => 0xE,
        "CAVE" => 0xF,
        "ROPE_BRIDGE" => 0x10,
        "WOOD_BRIDGE" => 0x11,
        "BOOST_RAMP_WOOD" => 0xFC,
        "OUT_OF_BOUNDS" => 0xFD,
        "BOOST_RAMP_ASPHALT" => 0xFE,
        "RAMP" => 0xFF,
        _ => 1,
    }
}

fn far_persp(name: &str) -> i64 {
    match name {
        "bowsers_castle" | "banshee_boardwalk" | "rainbow_road" | "block_fort" | "skyscraper" => 2700,
        "choco_mountain" | "double_deck" => 1500,
        "koopa_troopa_beach" => 5000,
        "wario_stadium" => 4800,
        "mario_raceway" | "yoshi_valley" | "frappe_snowland" | "royal_raceway" | "luigi_raceway" | "moo_moo_farm" | "toads_turnpike" | "sherbet_land" | "dks_jungle_parkway" => 4500,
        "kalimari_desert" => 7000,
        _ => 6800,
    }
}

fn rgb(c: [i64; 3]) -> i64 {
    (c[0] << 16) | (c[1] << 8) | c[2]
}

/// -> (sky top, mid, bottom, fog colour, fog near, fog far)
fn look(source: &Path, name: &str) -> R<[i64; 6]> {
    let file = if name == "koopa_troopa_beach" { "koopa_beach" } else { name };
    let meta = read(&source.join("yamls/courses").join(format!("{file}_metadata.yml")))?;
    let m = re(r"(?m)^\s*sky_colors:\s*\[([^\]]*)\]").captures(&meta).ok_or("sky_colors missing")?;
    let values: Vec<i64> = m[1].split(',').map(|v| int(v).map(|x| x & 0xFF)).collect::<R<_>>()?;
    let (mut top, mut bottom) = ([values[0], values[1], values[2]], [values[3], values[4], values[5]]);
    let fog_sky = match name {
        "choco_mountain" => Some([255, 255, 255]),
        "banshee_boardwalk" => Some([0, 0, 0]),
        _ => None,
    };
    if let Some(f) = fog_sky {
        top = f;
        bottom = f;
    }
    let mid = [(top[0] + bottom[0]).div_euclid(2), (top[1] + bottom[1]).div_euclid(2), (top[2] + bottom[2]).div_euclid(2)];
    let fog = fog_sky.or(if name == "toads_turnpike" { Some([43, 13, 4]) } else { None }).unwrap_or(bottom);
    let far = (far_persp(name) * 8) as f64;
    Ok([rgb(top), rgb(mid), rgb(bottom), rgb(fog), (far * 0.8) as i64, far as i64])
}

// ------------------------------------------------------------------------------------------------ textures

pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Rc<Vec<u8>>,
}

pub struct Textures<'a> {
    pub blocks: Blocks<'a>,
    assets: Value,
    aliases: HashMap<String, (String, String)>,
    named: Map<String, Value>,
    cache: HashMap<String, Option<Rc<Img>>>,
}

fn parse_us(entry: &Value) -> R<(usize, usize)> {
    let a = entry["offsets"]["us"].as_array().ok_or("offsets.us missing")?;
    Ok((int0(a[0].as_str().ok_or("offset")?)? as usize, int0(a[1].as_str().ok_or("offset")?)? as usize))
}

impl<'a> Textures<'a> {
    pub fn new(rom: &'a [u8], source: &Path) -> R<Self> {
        let assets: Value = serde_json::from_str(&read(&source.join("assets.json"))?).map_err(|e| e.to_string())?;
        let mut aliases = HashMap::new();
        for c in re(r#""(gTexture\w+)":\s*\("([^"]+)",\s*"(\w+)"\)"#).captures_iter(&read(&source.join("tools/linkonly_generator.py"))?) {
            aliases.insert(c[1].to_string(), (c[2].to_string(), c[3].to_string()));
        }
        let mut named = Map::new();
        let mut files: Vec<PathBuf> = std::fs::read_dir(source.join("assets/courses")).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        files.sort();
        for p in files {
            if let Value::Object(m) = serde_json::from_str::<Value>(&read(&p)?).map_err(|e| e.to_string())? {
                for (k, v) in m {
                    named.insert(k, v);
                }
            }
        }
        Ok(Textures { blocks: Blocks::new(rom), assets, aliases, named, cache: HashMap::new() })
    }

    /// -> (width, height, rgba bytes) or None.
    pub fn image(&mut self, symbol: &str) -> R<Option<Rc<Img>>> {
        if let Some(r) = self.cache.get(symbol) {
            return Ok(r.clone());
        }
        let mut result = None;
        let base = match re(r"^g(RRW|RR|KTB|MMF|DKJ|BB|BD|BF|BC|CM|DD|FS|KD|LR|MR|SL|SS|TT|WS|YV)Texture(\w+)$").captures(symbol) {
            Some(m) => format!("gTexture{}", &m[2]),
            None => symbol.to_string(),
        };
        if let Some((name, kind)) = self.aliases.get(&base).cloned() {
            let entry = self.assets.get(format!("textures/standalone/{name}.{kind}.png")).cloned();
            if let Some(entry) = entry.filter(|e| e.as_object().is_some_and(|o| !o.is_empty())) {
                let d = entry["meta"]["dims"].as_array().ok_or("dims")?;
                let (w, h) = (d[0].as_u64().ok_or("w")? as usize, d[1].as_u64().ok_or("h")? as usize);
                let (start, offset) = parse_us(&entry)?;
                let block = self.blocks.block(start)?;
                let data = sl(&block, offset, offset + w * h * 2);
                result = Some(Rc::new(Img { w, h, px: Rc::new(decode(&kind, data, w, h, None)?) }));
            }
        }
        if result.is_none() {
            if let Some(entry) = self.named.get(&base).cloned() {
                let (w, h, kind) = (entry["width"].as_u64().ok_or("w")? as usize, entry["height"].as_u64().ok_or("h")? as usize, entry["type"].as_str().ok_or("type")?.to_string());
                if kind == "rgba16" || kind == "ia16" {
                    let start = int0(entry["rom_offset"].as_str().ok_or("rom_offset")?)? as usize;
                    let offset = match entry.get("block_offset") {
                        Some(v) => int0(v.as_str().ok_or("block_offset")?)? as usize,
                        None => 0,
                    };
                    let block = self.blocks.block(start)?;
                    let data = sl(&block, offset, offset + w * h * 2);
                    result = Some(Rc::new(Img { w, h, px: Rc::new(decode(&kind, data, w, h, None)?) }));
                }
            }
        }
        self.cache.insert(symbol.to_string(), result.clone());
        Ok(result)
    }
}

// ------------------------------------------------------------------------------------------------ course parsing

fn to_int(v: &str) -> i64 {
    int0(v).unwrap_or(0)
}

fn wrap_mode(flags: &str) -> &'static str {
    if flags.contains("G_TX_CLAMP") {
        "clamp"
    } else if flags.contains("G_TX_MIRROR") && !flags.contains("G_TX_NOMIRROR") {
        "mirror"
    } else {
        "wrap"
    }
}

pub type Tri = (i64, i64, i64);

pub struct State {
    cache: HashMap<i64, i64>,
    pub texture: Option<String>,
    pub wrap_s: &'static str,
    pub wrap_t: &'static str,
}

impl State {
    fn new() -> Self {
        State { cache: HashMap::new(), texture: None, wrap_s: "wrap", wrap_t: "wrap" }
    }
}

type Cmd = (String, Vec<String>);

pub struct Course {
    pub name: String,
    /// x y z u v r g b flag
    pub vertices: Vec<[i64; 9]>,
    lists: HashMap<String, Vec<Cmd>>,
    order: Vec<String>,
    data_text: String,
}

pub fn commands(body: &str) -> Vec<Cmd> {
    re(r"(?s)(gs\w+)\((.*?)\)").captures_iter(body).map(|c| (c[1].to_string(), c[2].split(',').map(|a| a.trim().to_string()).collect())).collect()
}

impl Course {
    pub fn new(source: &Path, name: &str) -> R<Course> {
        let dir = source.join("courses").join(name);
        let text = read(&dir.join("course_vertices.inc.c"))?;
        let vre = re(r"\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},\s*\{\s*(-?\d+),\s*(-?\d+)\s*\},\s*\{\s*MACRO_COLOR_FLAG\((0x\w+),\s*(0x\w+),\s*(0x\w+),\s*(\w+)\)");
        let mut vertices = Vec::new();
        for r in vre.captures_iter(&text) {
            vertices.push([int(&r[1])?, int(&r[2])?, int(&r[3])?, int(&r[4])?, int(&r[5])?, to_int(&r[6]), to_int(&r[7]), to_int(&r[8]), to_int(&r[9])]);
        }
        let displaylists = read(&dir.join("course_displaylists.inc.c"))?;
        let mut lists = HashMap::new();
        let mut order = Vec::new();
        for c in re(r"(?s)Gfx (\w+)\[\] = \{(.*?)\n\};").captures_iter(&displaylists) {
            lists.insert(c[1].to_string(), commands(&c[2]));
            order.push(c[1].to_string());
        }
        Ok(Course { name: name.to_string(), vertices, lists, order, data_text: read(&dir.join("course_data.c"))? })
    }

    fn packed(&self, offset: i64) -> String {
        format!("d_course_{}_packed_dl_{:X}", self.name, offset)
    }

    /// Display lists in the order the game reaches them from the course's sector lists, then the rest.
    pub fn draw_order(&self) -> Vec<String> {
        fn visit(c: &Course, dl: &str, seen: &mut HashSet<String>, order: &mut Vec<String>) {
            if seen.contains(dl) || !c.lists.contains_key(dl) {
                return;
            }
            seen.insert(dl.to_string());
            order.push(dl.to_string());
            for (command, args) in &c.lists[dl] {
                if command == "gsSPDisplayList" {
                    visit(c, &args[0], seen, order);
                }
            }
        }
        let (mut seen, mut order) = (HashSet::new(), Vec::new());
        for m in re(r"gsSPDisplayList\((d_course_\w+)\)").captures_iter(&self.data_text) {
            visit(self, &m[1], &mut seen, &mut order);
        }
        for dl in &self.order {
            visit(self, dl, &mut seen, &mut order);
        }
        order
    }

    /// [(display list, surface name or "-1", section id, flags)] from the `TrackSections` table + extras.
    pub fn sections(&self) -> R<Vec<(String, String, i64, i64)>> {
        let mut rows = Vec::new();
        if let Some(m) = re(r"(?s)TrackSections\s+\w+\[\]\s*=\s*\{(.*?)\n\};").captures(&self.data_text) {
            for c in re(r"\{\s*(d_course_\w+),\s*(\w+),\s*(\d+),\s*(0x[0-9A-Fa-f]+)\s*\}").captures_iter(&m[1]) {
                rows.push((c[1].to_string(), c[2].to_string(), int(&c[3])?, i64::from_str_radix(&c[4][2..], 16).map_err(|e| e.to_string())?));
            }
        }
        for (offset, surface) in extra_collision(&self.name) {
            rows.push((self.packed(offset), surface.to_string(), 0xFF, 0));
        }
        Ok(rows)
    }

    pub fn track_path(&self) -> R<Vec<[i64; 3]>> {
        let Some(m) = re(r"(?s)TrackPathPoint\s+d_course_\w+_track_path\[\]\s*=\s*\{(.*?)\};").captures(&self.data_text) else {
            return Ok(vec![]);
        };
        re(r"\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+),\s*\d+\s*\}").captures_iter(&m[1]).map(|c| Ok([int(&c[1])?, int(&c[2])?, int(&c[3])?])).collect()
    }

    pub fn item_boxes(&self) -> R<Vec<[i64; 3]>> {
        let Some(m) = re(r"(?s)ActorSpawnData\s+d_course_\w+_item_box_spawns\[\]\s*=\s*\{(.*?)\};").captures(&self.data_text) else {
            return Ok(vec![]);
        };
        let mut out = Vec::new();
        for c in re(r"\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},").captures_iter(&m[1]) {
            let row = [int(&c[1])?, int(&c[2])?, int(&c[3])?];
            if row[0] != -32768 {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// Interprets a display list (recursing into sub-lists); `visit(tri, state)` is called per triangle.
    pub fn run(&self, dl: &str, visit: &mut dyn FnMut(Tri, &State) -> R<()>, state: &mut State, depth: usize) -> R<()> {
        let Some(cmds) = self.lists.get(dl) else { return Ok(()) };
        for (command, args) in cmds {
            let arg = |i: usize| args.get(i).map(String::as_str).ok_or_else(|| format!("{command}: missing argument {i}"));
            match command.as_str() {
                "gsSPDisplayList" => {
                    if depth < 16 {
                        self.run(arg(0)?, visit, state, depth + 1)?;
                    }
                }
                "gsDPSetTextureImage" => state.texture = Some(arg(3)?.to_string()),
                "gsDPSetTile" if arg(4)? == "G_TX_RENDERTILE" => {
                    state.wrap_t = wrap_mode(arg(6)?);
                    state.wrap_s = wrap_mode(arg(9)?);
                }
                "gsDPSetCombineMode" if arg(0)?.contains("SHADE") && !arg(0)?.contains("TEXEL") => state.texture = None,
                "gsSPVertex" => {
                    let (address, count, slot) = (int0(arg(0)?)?, int0(arg(1)?)?, int0(arg(2)?)?);
                    let base = (address & 0xffffff).div_euclid(16);
                    for i in 0..count {
                        state.cache.insert(slot + i, base + i);
                    }
                }
                "gsSP2Triangles" | "gsSP1Triangle" => {
                    let nums: Vec<i64> = args.iter().map(|a| int0(a)).collect::<R<_>>()?;
                    for ch in nums.chunks(4) {
                        let tri: Option<Vec<i64>> = ch.iter().take(3).map(|v| state.cache.get(v).copied()).collect();
                        if let Some(t) = tri.filter(|t| t.len() == 3) {
                            visit((t[0], t[1], t[2]), state)?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------------ triangles, collision, decal layers

/// `(v2 - v1) x (v3 - v2)` as exact integers (the Python code works on ints) and its length.
fn normal_raw(p1: [i64; 3], p2: [i64; 3], p3: [i64; 3]) -> [i128; 3] {
    let d = |a: [i64; 3], b: [i64; 3], k: usize| (b[k] - a[k]) as i128;
    [d(p1, p2, 1) * d(p2, p3, 2) - d(p1, p2, 2) * d(p2, p3, 1), d(p1, p2, 2) * d(p2, p3, 0) - d(p1, p2, 0) * d(p2, p3, 2), d(p1, p2, 0) * d(p2, p3, 1) - d(p1, p2, 1) * d(p2, p3, 0)]
}

/// The game's collision normal, unit length (None when degenerate).
pub fn normal_of(p1: [i64; 3], p2: [i64; 3], p3: [i64; 3]) -> Option<[f64; 3]> {
    let n = normal_raw(p1, p2, p3);
    let sq = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
    // Python: `(nx*nx + ny*ny + nz*nz) ** .5` converts the int to a float and calls libm pow
    let mag = (sq as f64).powf(std::hint::black_box(0.5));
    if mag == 0.0 {
        return None;
    }
    Some([n[0] as f64 / mag, n[1] as f64 / mag, n[2] as f64 / mag])
}

struct VisualTri {
    tri: Tri,
    texture: Option<String>,
    wrap_s: &'static str,
    wrap_t: &'static str,
    alpha: Option<i64>,
}

fn collect_visual(course: &Course) -> R<Vec<VisualTri>> {
    let mut translucent: HashMap<Tri, i64> = HashMap::new();
    for (offset, alpha) in translucent_lists(&course.name) {
        course.run(&course.packed(offset), &mut |tri, _| { translucent.insert(tri, alpha); Ok(()) }, &mut State::new(), 0)?;
    }
    let mut seen: HashSet<(Option<String>, Tri)> = HashSet::new();
    let mut out = Vec::new();
    for dl in course.draw_order() {
        course.run(
            &dl,
            &mut |tri, state| {
                let key = (state.texture.clone(), tri);
                if seen.contains(&key) {
                    return Ok(());
                }
                seen.insert(key);
                out.push(VisualTri { tri, texture: state.texture.clone(), wrap_s: state.wrap_s, wrap_t: state.wrap_t, alpha: translucent.get(&tri).copied() });
                Ok(())
            },
            &mut State::new(),
            0,
        )?;
    }
    Ok(out)
}

fn collect_collision(course: &Course) -> R<Vec<(Tri, String)>> {
    let mut seen: HashSet<Tri> = HashSet::new();
    let mut out = Vec::new();
    let vertices = &course.vertices;
    for (dl, surface, _section, flags) in course.sections()? {
        course.run(
            &dl,
            &mut |tri, _| {
                if seen.contains(&tri) {
                    return Ok(());
                }
                let idx = [tri.0, tri.1, tri.2];
                let v = |i: i64| vertices.get(i as usize).ok_or_else(|| format!("vertex {i} out of range"));
                if idx.iter().all(|&i| v(i).map(|x| x[8] == 4).unwrap_or(false)) {
                    return Ok(());
                }
                let mut p = [[0i64; 3]; 3];
                for (k, &i) in idx.iter().enumerate() {
                    let x = v(i)?;
                    p[k] = [x[0], x[1], x[2]];
                }
                if p[0][0] == p[1][0] && p[0][2] == p[1][2] {
                    p = [p[0], p[2], p[1]];
                }
                let Some(n) = normal_of(p[0], p[1], p[2]) else { return Ok(()) };
                if flags & 0x8000 != 0 && n[1].abs() > 0.9 {
                    return Ok(());
                }
                if flags & 0x2000 != 0 && n[1].abs() < 0.1 {
                    return Ok(());
                }
                seen.insert(tri);
                out.push((tri, surface.clone()));
                Ok(())
            },
            &mut State::new(),
            0,
        )?;
    }
    Ok(out)
}

fn inside<P: Copy + Into<[f64; 3]>>(tri: &[P; 3], c: [f64; 3], u: usize, v: usize) -> bool {
    let t: Vec<[f64; 3]> = tri.iter().map(|&p| p.into()).collect();
    let side = |a: [f64; 3], b: [f64; 3]| (b[u] - a[u]) * (c[v] - a[v]) - (b[v] - a[v]) * (c[u] - a[u]);
    let s = [side(t[0], t[1]), side(t[1], t[2]), side(t[2], t[0])];
    s.iter().all(|&x| x >= -1e-6) || s.iter().all(|&x| x <= 1e-6)
}

#[derive(Clone, Copy)]
struct P3([i64; 3]);
impl From<P3> for [f64; 3] {
    fn from(p: P3) -> Self {
        [p.0[0] as f64, p.0[1] as f64, p.0[2] as f64]
    }
}

struct TriInfo {
    p: [[i64; 3]; 3],
    n: Option<[f64; 3]>,
}

/// Layer index per triangle: a triangle drawn over earlier ones lying in its plane is stacked above them.
fn decal_layers(tris: &[TriInfo]) -> Vec<i64> {
    let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut layers: Vec<i64> = Vec::new();
    let cell = 64i64;
    for (index, t) in tris.iter().enumerate() {
        let (p, n) = (&t.p, t.n);
        let mut layer = 0i64;
        if let Some(n) = n {
            let c = [0, 1, 2].map(|k| (p[0][k] + p[1][k] + p[2][k]) as f64 / 3.0);
            let mut ax = 0;
            for k in 1..3 {
                if n[k].abs() > n[ax].abs() {
                    ax = k;
                }
            }
            let (u, v) = match ax {
                0 => (1, 2),
                1 => (0, 2),
                _ => (0, 1),
            };
            if let Some(list) = cells.get(&(((c[0] / cell as f64).floor()) as i64, ((c[2] / cell as f64).floor()) as i64)) {
                for &j in list {
                    let other = &tris[j];
                    let Some(on) = other.n else { continue };
                    let dot = on[0] * n[0] + on[1] * n[1] + on[2] * n[2];
                    if dot.abs() < 0.98 {
                        continue;
                    }
                    let mut s = 0.0f64;
                    for k in 0..3 {
                        s += (c[k] - other.p[0][k] as f64) * on[k];
                    }
                    if s.abs() > 1.2 {
                        continue;
                    }
                    if inside(&[P3(other.p[0]), P3(other.p[1]), P3(other.p[2])], c, u, v) {
                        layer = layer.max(layers[j] + 1);
                    }
                }
            }
        }
        layers.push(layer);
        let (x0, x1) = (p.iter().map(|q| q[0]).min().unwrap(), p.iter().map(|q| q[0]).max().unwrap());
        let (z0, z1) = (p.iter().map(|q| q[2]).min().unwrap(), p.iter().map(|q| q[2]).max().unwrap());
        for cx in x0.div_euclid(cell)..=x1.div_euclid(cell) {
            for cz in z0.div_euclid(cell)..=z1.div_euclid(cell) {
                cells.entry((cx, cz)).or_default().push(index);
            }
        }
    }
    layers
}

// ------------------------------------------------------------------------------------------------ glTF

fn png_bytes(w: usize, h: usize, px: &[u8]) -> R<Vec<u8>> {
    png_rgba(w, h, px, 9)
}

struct Glb {
    data: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
    images: Vec<Value>,
    textures: Vec<Value>,
    samplers: Vec<Value>,
    materials: Vec<Value>,
    primitives: Vec<Value>,
    image_ids: HashMap<String, usize>,
    material_ids: HashMap<(Option<String>, &'static str, &'static str, bool), usize>,
}

fn pack_num(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&(v as f32).to_le_bytes());
}

impl Glb {
    fn new() -> Self {
        Glb { data: vec![], buffer_views: vec![], accessors: vec![], images: vec![], textures: vec![], samplers: vec![], materials: vec![], primitives: vec![], image_ids: HashMap::new(), material_ids: HashMap::new() }
    }

    fn view(&mut self, data: &[u8]) -> usize {
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        self.buffer_views.push(json!({"buffer": 0, "byteOffset": self.data.len(), "byteLength": data.len()}));
        self.data.extend_from_slice(data);
        self.buffer_views.len() - 1
    }

    fn accessor(&mut self, flat: Vec<u8>, kind: &str, component: u32, count: usize, bounds: Option<(Vec<Num>, Vec<Num>)>, normalized: bool) -> usize {
        let mut acc = Map::new();
        acc.insert("bufferView".into(), json!(self.view(&flat)));
        acc.insert("componentType".into(), json!(component));
        acc.insert("count".into(), json!(count));
        acc.insert("type".into(), json!(kind));
        if let Some((lo, hi)) = bounds {
            acc.insert("min".into(), Value::Array(lo.iter().map(|n| n.json()).collect()));
            acc.insert("max".into(), Value::Array(hi.iter().map(|n| n.json()).collect()));
        }
        if normalized {
            acc.insert("normalized".into(), json!(true));
        }
        self.accessors.push(Value::Object(acc));
        self.accessors.len() - 1
    }

    fn material(&mut self, texture: &Option<String>, image: Option<&Img>, wrap_s: &'static str, wrap_t: &'static str, blend: bool) -> R<usize> {
        let key = (texture.clone(), wrap_s, wrap_t, blend);
        if let Some(&m) = self.material_ids.get(&key) {
            return Ok(m);
        }
        let mut material = Map::new();
        material.insert("doubleSided".into(), json!(true));
        material.insert("extensions".into(), json!({"KHR_materials_unlit": {}}));
        material.insert("alphaMode".into(), json!(if blend { "BLEND" } else { "MASK" }));
        if !blend {
            material.insert("alphaCutoff".into(), json!(0.5));
        }
        if let Some(img) = image {
            let tex = texture.clone().unwrap_or_default();
            if !self.image_ids.contains_key(&tex) {
                let bv = self.view(&png_bytes(img.w, img.h, &img.px)?);
                self.images.push(json!({"bufferView": bv, "mimeType": "image/png"}));
                self.image_ids.insert(tex.clone(), self.images.len() - 1);
            }
            let gl = |m: &str| match m {
                "wrap" => 10497,
                "mirror" => 33648,
                _ => 33071,
            };
            self.samplers.push(json!({"magFilter": 9729, "minFilter": 9729, "wrapS": gl(wrap_s), "wrapT": gl(wrap_t)}));
            self.textures.push(json!({"source": self.image_ids[&tex], "sampler": self.samplers.len() - 1}));
            material.insert("pbrMetallicRoughness".into(), json!({"baseColorTexture": {"index": self.textures.len() - 1}, "metallicFactor": 0, "roughnessFactor": 1}));
        } else {
            material.insert("pbrMetallicRoughness".into(), json!({"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0, "roughnessFactor": 1}));
        }
        self.materials.push(Value::Object(material));
        let id = self.materials.len() - 1;
        self.material_ids.insert(key, id);
        Ok(id)
    }

    fn primitive(&mut self, positions: &[[Num; 3]], uvs: &[[f64; 2]], colors: &[[u8; 4]], indices: &[u32], material: usize) {
        let mut flat = Vec::new();
        for p in positions {
            for n in p {
                pack_num(&mut flat, n.f());
            }
        }
        let mut lo = Vec::new();
        let mut hi = Vec::new();
        for k in 0..3 {
            let (mut l, mut h) = (positions[0][k], positions[0][k]);
            for p in &positions[1..] {
                if p[k].f() < l.f() {
                    l = p[k];
                }
                if p[k].f() > h.f() {
                    h = p[k];
                }
            }
            lo.push(l);
            hi.push(h);
        }
        let pos = self.accessor(flat, "VEC3", 5126, positions.len(), Some((lo, hi)), false);
        let mut flat = Vec::new();
        for u in uvs {
            pack_num(&mut flat, u[0]);
            pack_num(&mut flat, u[1]);
        }
        let uv = self.accessor(flat, "VEC2", 5126, uvs.len(), None, false);
        let flat: Vec<u8> = colors.iter().flatten().copied().collect();
        let col = self.accessor(flat, "VEC4", 5121, colors.len(), None, true);
        let flat: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let idx = self.accessor(flat, "SCALAR", 5125, indices.len(), None, false);
        self.primitives.push(json!({"attributes": {"POSITION": pos, "TEXCOORD_0": uv, "COLOR_0": col}, "indices": idx, "material": material}));
    }

    fn write(mut self, path: &Path) -> R<usize> {
        let doc = json!({
            "asset": {"version": "2.0", "extras": {"sm64UnitsToMeters": 1}},
            "buffers": [{"byteLength": self.data.len()}],
            "bufferViews": self.buffer_views,
            "accessors": self.accessors,
            "images": self.images,
            "textures": self.textures,
            "samplers": self.samplers,
            "materials": self.materials,
            "meshes": [{"primitives": self.primitives}],
            "nodes": [{"mesh": 0}],
            "scenes": [{"nodes": [0]}],
            "scene": 0,
            "extensionsUsed": ["KHR_materials_unlit"],
        });
        let mut meta = dumps(&doc, None).into_bytes();
        while meta.len() % 4 != 0 {
            meta.push(b' ');
        }
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&((28 + meta.len() + self.data.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&meta);
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&self.data);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(path, &out).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(out.len())
    }
}

// ------------------------------------------------------------------------------------------------ the walkable course

/// Highest floor of `tris` (triples of world points, upward normals) at (x, z) not above y_max.
fn floor_height(tris: &[[[i64; 3]; 3]], x: Num, z: Num, y_max: f64) -> Option<f64> {
    let (xf, zf) = (x.f(), z.f());
    let mut best: Option<f64> = None;
    for p in tris {
        let (x0, x1) = (p.iter().map(|q| q[0]).min().unwrap() as f64, p.iter().map(|q| q[0]).max().unwrap() as f64);
        let (z0, z1) = (p.iter().map(|q| q[2]).min().unwrap() as f64, p.iter().map(|q| q[2]).max().unwrap() as f64);
        if !(x0 <= xf && xf <= x1 && z0 <= zf && zf <= z1) {
            continue;
        }
        let Some(n) = normal_of(p[0], p[1], p[2]) else { continue };
        if n[1] <= 0.05 || !inside(&[P3(p[0]), P3(p[1]), P3(p[2])], [xf, 0.0, zf], 0, 2) {
            continue;
        }
        let y = p[0][1] as f64 - (n[0] * (xf - p[0][0] as f64) + n[2] * (zf - p[0][2] as f64)) / n[1];
        if y <= y_max && best.is_none_or(|b| y > b) {
            best = Some(y);
        }
    }
    best
}

fn cross_ok(p: &[[i64; 3]; 3]) -> bool {
    p[0][0] == p[1][0] && p[0][2] == p[1][2]
}

/// Writes `assets/courses/mk64_<dir>.json` and `assets/levels/mk64_<dir>/terrain.glb`.
pub fn export_walk(course: &Course, textures: &mut Textures, source: &Path, assets_dir: &Path) -> R<()> {
    let name = course.name.as_str();
    let scale = WALK_SCALE;
    let course_id = format!("mk64_{name}");
    let verts = &course.vertices;
    let visual = collect_visual(course)?;
    let mut glb = Glb::new();
    let vtx = |i: i64| verts.get(i as usize).ok_or_else(|| format!("vertex {i} out of range"));
    let mut tri_info = Vec::new();
    for t in &visual {
        let mut p = [[0i64; 3]; 3];
        for (k, i) in [t.tri.0, t.tri.1, t.tri.2].into_iter().enumerate() {
            let v = vtx(i)?;
            p[k] = [v[0], v[1], v[2]];
        }
        tri_info.push(TriInfo { p, n: normal_of(p[0], p[1], p[2]) });
    }
    let layers = decal_layers(&tri_info);

    // --- visual: one primitive per material
    type Key = (Option<String>, &'static str, &'static str, bool);
    let mut group_order: Vec<Key> = Vec::new();
    let mut groups: HashMap<Key, Vec<usize>> = HashMap::new();
    for (i, t) in visual.iter().enumerate() {
        let key = (t.texture.clone(), t.wrap_s, t.wrap_t, t.alpha.is_some());
        groups.entry(key.clone()).or_insert_with(|| {
            group_order.push(key.clone());
            vec![]
        }).push(i);
    }
    for key in &group_order {
        let (texture, wrap_s, wrap_t, blend) = key;
        let image = match texture {
            Some(t) => textures.image(t)?,
            None => None,
        };
        let (w, h) = image.as_ref().map(|i| (i.w, i.h)).unwrap_or((32, 32));
        let mut positions: Vec<[Num; 3]> = Vec::new();
        let (mut uvs, mut colors, mut indices) = (Vec::new(), Vec::new(), Vec::<u32>::new());
        let mut lookup: HashMap<(i64, i64, i64), usize> = HashMap::new();
        for &ti in &groups[key] {
            let (t, info, layer) = (&visual[ti], &tri_info[ti], layers[ti]);
            let lift: [Num; 3] = match info.n {
                Some(n) if layer != 0 => {
                    let sign = if n[1].abs() > 0.5 && n[1] < 0.0 { -1.0 } else { 1.0 };
                    [0, 1, 2].map(|k| Num::F(n[k] * sign * layer as f64 * 0.9))
                }
                _ => [Num::I(0); 3],
            };
            for index in [t.tri.0, t.tri.1, t.tri.2] {
                let v = vtx(index)?;
                let alpha = t.alpha.unwrap_or(255);
                let lk = (index, layer, alpha);
                let id = match lookup.get(&lk) {
                    Some(&id) => id,
                    None => {
                        let id = positions.len();
                        lookup.insert(lk, id);
                        let add = |c: i64, l: Num| match l {
                            Num::I(li) => Num::I(c * scale + li),
                            Num::F(lf) => Num::F((c * scale) as f64 + lf),
                        };
                        positions.push([add(v[0], lift[0]), add(v[1], lift[1]), add(v[2], lift[2])]);
                        uvs.push([v[3] as f64 / (32.0 * w as f64), v[4] as f64 / (32.0 * h as f64)]);
                        let byte = |x: i64| u8::try_from(x).map_err(|_| format!("vertex colour {x} out of range"));
                        colors.push([byte(v[5])?, byte(v[6])?, byte(v[7])?, byte(alpha)?]);
                        id
                    }
                };
                indices.push(id as u32);
            }
        }
        let mat = glb.material(texture, image.as_deref(), wrap_s, wrap_t, *blend)?;
        glb.primitive(&positions, &uvs, &colors, &indices, mat);
    }
    glb.write(&assets_dir.join("levels").join(&course_id).join("terrain.glb"))?;

    // --- collision
    let hits = collect_collision(course)?;
    let mut vertex_index: HashMap<[i64; 3], usize> = HashMap::new();
    let mut out_vertices: Vec<[i64; 3]> = Vec::new();
    let mut triangles: Vec<Value> = Vec::new();
    let mut floors: Vec<[[i64; 3]; 3]> = Vec::new();
    let mut vid = |p: [i64; 3]| {
        let key = [p[0] * scale, p[1] * scale, p[2] * scale];
        *vertex_index.entry(key).or_insert_with(|| {
            out_vertices.push(key);
            out_vertices.len() - 1
        })
    };
    for (tri, surface) in &hits {
        let mut p = [[0i64; 3]; 3];
        for (k, i) in [tri.0, tri.1, tri.2].into_iter().enumerate() {
            let v = vtx(i)?;
            p[k] = [v[0], v[1], v[2]];
        }
        if cross_ok(&p) {
            p = [p[0], p[2], p[1]];
        }
        let n = normal_of(p[0], p[1], p[2]).ok_or("degenerate collision triangle")?;
        let kind = surface_kind(surface);
        let sid = surface_id(surface);
        let (a, b, c) = (vid(p[0]), vid(p[1]), vid(p[2]));
        let sc = |q: [i64; 3]| [q[0] * scale, q[1] * scale, q[2] * scale];
        if n[1].abs() >= 0.1 {
            if n[1] > 0.0 {
                triangles.push(json!([a, b, c, kind, sid]));
                floors.push([sc(p[0]), sc(p[1]), sc(p[2])]);
            } else {
                triangles.push(json!([a, c, b, kind, sid]));
                triangles.push(json!([a, b, c, kind, sid]));
                floors.push([sc(p[0]), sc(p[2]), sc(p[1])]);
            }
        } else {
            triangles.push(json!([a, b, c, kind, sid]));
            triangles.push(json!([a, c, b, kind, sid]));
        }
    }

    // --- spawn on the start line, facing down the track
    let path: Vec<[i64; 3]> = course.track_path()?.into_iter().filter(|p| p[0] > -30000).collect();
    let xs: Vec<i64> = if out_vertices.is_empty() { vec![0] } else { out_vertices.iter().map(|v| v[0]).collect() };
    let zs: Vec<i64> = if out_vertices.is_empty() { vec![0] } else { out_vertices.iter().map(|v| v[2]).collect() };
    let (mut sx, mut sz, yaw, mut sy);
    if !path.is_empty() {
        let (start, ahead) = (path[0], path[3.min(path.len() - 1)]);
        sx = Num::I(start[0] * scale);
        sz = Num::I(start[2] * scale);
        let (dx, dz) = (ahead[0] - start[0], ahead[2] - start[2]);
        yaw = if dx != 0 || dz != 0 { round((dx as f64).atan2(dz as f64) * 32768.0 / std::f64::consts::PI) } else { 0 };
        sy = floor_height(&floors, sx, sz, (start[1] * scale + 400) as f64);
    } else {
        yaw = 0;
        let boxes = course.item_boxes()?;
        if !boxes.is_empty() {
            let cx = boxes.iter().map(|b| b[0]).sum::<i64>() as f64 / boxes.len() as f64 * scale as f64;
            let cz = boxes.iter().map(|b| b[2]).sum::<i64>() as f64 / boxes.len() as f64 * scale as f64;
            let mut best: Option<(i64, i64, f64)> = None;
            for b in &boxes {
                let (bx, bz) = (b[0] * scale, b[2] * scale);
                let d = (bx as f64 - cx).powi(2) + (bz as f64 - cz).powi(2);
                if best.is_none_or(|(_, _, bd)| d < bd) {
                    best = Some((bx, bz, d));
                }
            }
            let (bx, bz, _) = best.unwrap();
            sx = Num::I(bx);
            sz = Num::I(bz);
        } else {
            sx = Num::F(xs.iter().sum::<i64>() as f64 / xs.len() as f64);
            sz = Num::F(zs.iter().sum::<i64>() as f64 / zs.len() as f64);
        }
        sy = floor_height(&floors, sx, sz, 1e9);
    }
    if sy.is_none() {
        let (sxf, szf) = (sx.f(), sz.f());
        let dist = |p: &[[i64; 3]; 3]| (p.iter().map(|q| q[0]).sum::<i64>() as f64 / 3.0 - sxf).powi(2) + (p.iter().map(|q| q[2]).sum::<i64>() as f64 / 3.0 - szf).powi(2);
        let mut best = &floors[0];
        for f in &floors[1..] {
            if dist(f) < dist(best) {
                best = f;
            }
        }
        sx = Num::F(best.iter().map(|q| q[0]).sum::<i64>() as f64 / 3.0);
        sz = Num::F(best.iter().map(|q| q[2]).sum::<i64>() as f64 / 3.0);
        sy = floor_height(&floors, sx, sz, 1e9);
    }
    let sy = sy.ok_or("no floor under the spawn")?;
    let boundary = xs.iter().map(|v| v.abs()).max().unwrap().max(zs.iter().map(|v| v.abs()).max().unwrap()) + 3000;
    let [top, mid, bottom, fog, near, far] = look(source, name)?;
    let (_, display, kind) = course_info(name);
    let boxes: Vec<Value> = course.item_boxes()?.iter().map(|b| json!([b[0] * scale, b[1] * scale, b[2] * scale])).collect();
    let data = json!({
        "model": format!("levels/{course_id}/terrain.glb"),
        "vertices": out_vertices.iter().map(|v| json!([v[0], v[1], v[2]])).collect::<Vec<_>>(),
        "triangles": triangles,
        "spawn": {"pos": [sx.round_to(1).json(), Num::F(sy + 4.0).round_to(1).json(), sz.round_to(1).json()], "faceYaw": yaw},
        "waterBoxes": [], "horizontalBoundary": boundary, "explorationOnly": true, "objects": [], "coins": [], "extraCollision": {},
        "mk64": {"course": name, "name": display, "kind": kind, "unit": scale, "sky": [top, mid, bottom], "fog": [fog, near, far], "itemBoxes": boxes},
    });
    let out = assets_dir.join("courses").join(format!("{course_id}.json"));
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&out, dumps(&data, None) + "\n").map_err(|e| e.to_string())?;
    Ok(())
}
