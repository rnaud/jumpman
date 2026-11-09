//! Port of `scripts/export-mk64-modes.py`: cups, Grand Prix points, the battle arenas' start grids, kart files and minimaps
//! (decomp sources only; runs after the courses so the walkable arenas exist in the stage).
use super::courses::Course;
use crate::pyc::{dumps, int, int0, re, read, round, R};
use crate::zpng::{png_rgba, round_f};
use serde_json::{json, Map, Value};
use std::path::Path;

const ARENAS: [&str; 4] = ["big_donut", "block_fort", "double_deck", "skyscraper"];
const NAMES: [&str; 4] = ["Big Donut", "Block Fort", "Double Deck", "Skyscraper"];
const CUP_NAMES: [&str; 4] = ["Mushroom", "Flower", "Star", "Special"];

fn const_dir(c: &str) -> R<&'static str> {
    Ok(match c {
        "COURSE_LUIGI_RACEWAY" => "luigi_raceway",
        "COURSE_MOO_MOO_FARM" => "moo_moo_farm",
        "COURSE_KOOPA_BEACH" => "koopa_troopa_beach",
        "COURSE_KALAMARI_DESERT" => "kalimari_desert",
        "COURSE_TOADS_TURNPIKE" => "toads_turnpike",
        "COURSE_FRAPPE_SNOWLAND" => "frappe_snowland",
        "COURSE_CHOCO_MOUNTAIN" => "choco_mountain",
        "COURSE_MARIO_RACEWAY" => "mario_raceway",
        "COURSE_WARIO_STADIUM" => "wario_stadium",
        "COURSE_SHERBET_LAND" => "sherbet_land",
        "COURSE_ROYAL_RACEWAY" => "royal_raceway",
        "COURSE_BOWSER_CASTLE" => "bowsers_castle",
        "COURSE_DK_JUNGLE" => "dks_jungle_parkway",
        "COURSE_YOSHI_VALLEY" => "yoshi_valley",
        "COURSE_BANSHEE_BOARDWALK" => "banshee_boardwalk",
        "COURSE_RAINBOW_ROAD" => "rainbow_road",
        "COURSE_BIG_DONUT" => "big_donut",
        "COURSE_BLOCK_FORT" => "block_fort",
        "COURSE_DOUBLE_DECK" => "double_deck",
        "COURSE_SKYSCRAPER" => "skyscraper",
        other => return Err(format!("unknown course constant {other}")),
    })
}

fn cups(source: &Path) -> R<Vec<Vec<&'static str>>> {
    let text = read(&source.join("src/menus.c"))?;
    let body = re(r"(?s)gCupCourseOrder\[5\]\[4\]\s*=\s*\{(.*?)\n\};").captures(&text).ok_or("gCupCourseOrder")?[1].to_string();
    let mut rows = Vec::new();
    for r in re(r"\{([^{}]*)\}").captures_iter(&body) {
        rows.push(re(r"COURSE_\w+").find_iter(&r[1]).map(|m| const_dir(m.as_str())).collect::<R<Vec<_>>>()?);
    }
    if rows.len() != 5 || rows.iter().any(|r| r.len() != 4) {
        return Err(format!("unexpected gCupCourseOrder: {rows:?}"));
    }
    Ok(rows)
}

fn point_rewards(source: &Path) -> R<Vec<i64>> {
    let text = read(&source.join("src/menu_items.c"))?;
    let m = re(r"gGPPointRewards\[\]\s*=\s*\{([^}]*)\}").captures(&text).ok_or("gGPPointRewards")?;
    m[1].split(',').filter(|v| !v.trim().is_empty()).map(int).collect()
}

struct Start {
    height: i64,
    x: [f64; 4],
    z: [f64; 4],
    rotation: Vec<i64>,
}

fn battle_starts(source: &Path) -> R<Vec<(&'static str, Start)>> {
    let text = read(&source.join("src/spawn_players.c"))?;
    let four = re(r"(?s)void spawn_players_4p_battle\(.*?\n\}").find(&text).ok_or("spawn_players_4p_battle")?.as_str().to_string();
    let rot_re = re(r"spawn_player\(gPlayer\w+, \d, arg0\[\d\], arg1\[\d\], arg2, (-?[\d.]+)f");
    let rotations = |block: &str| -> R<Vec<i64>> {
        let r: Vec<i64> = rot_re.captures_iter(block).map(|c| c[1].parse::<f64>().map(|f| f as i64).map_err(|e| e.to_string())).collect::<R<_>>()?;
        if r.len() != 4 {
            return Err(format!("expected 4 rotations, found {r:?}"));
        }
        Ok(r)
    };
    let donut = rotations(&re(r"(?s)COURSE_BIG_DONUT\)\s*\{(.*?)\}\s*else").captures(&four).ok_or("donut branch")?[1])?;
    let other = rotations(&re(r"(?s)else\s*\{(.*?)\n    \}").captures(&four).ok_or("other branch")?[1])?;
    let mut out = Vec::new();
    for course in ["BLOCK_FORT", "SKYSCRAPER", "DOUBLE_DECK", "BIG_DONUT"] {
        // re.findall(r'case COURSE_X:(.*?)(?=case COURSE_|\Z)', text, re.S)
        let label = format!("case COURSE_{course}:");
        let mut cases = Vec::new();
        let mut from = 0;
        while let Some(i) = text[from..].find(&label) {
            let start = from + i + label.len();
            let end = text[start..].find("case COURSE_").map_or(text.len(), |j| start + j);
            cases.push(&text[start..end]);
            from = end.max(start);
        }
        let body = cases.iter().find(|c| c.contains("spawn_players_4p_battle")).ok_or("case body")?;
        let at = re(r"case (SCREEN_MODE_3P_4P_SPLITSCREEN|3):").find(body).ok_or("3P/4P case")?.start();
        let body = &body[at..];
        let body = &body[..body.find("spawn_players_3p_battle").ok_or("3p battle")?];
        let height = int0(&re(r"temp = (0x[0-9A-Fa-f]+|\d+);").captures(body).ok_or("temp")?[1])?;
        let (mut x, mut z) = ([None; 4], [None; 4]);
        for c in re(r"D_8016(5210|5230)\[(\d)\] = (-?[\d.]+)f;").captures_iter(body) {
            let v: f64 = c[3].parse().map_err(|_| "grid value".to_string())?;
            let idx: usize = c[2].parse().map_err(|_| "grid index".to_string())?;
            if &c[1] == "5210" { x[idx] = Some(v) } else { z[idx] = Some(v) }
        }
        if x.iter().chain(z.iter()).any(Option::is_none) {
            return Err(format!("{course}: incomplete start grid"));
        }
        out.push((
            match course {
                "BLOCK_FORT" => "block_fort",
                "SKYSCRAPER" => "skyscraper",
                "DOUBLE_DECK" => "double_deck",
                _ => "big_donut",
            },
            Start { height, x: x.map(|v| v.unwrap()), z: z.map(|v| v.unwrap()), rotation: if course == "BIG_DONUT" { donut.clone() } else { other.clone() } },
        ));
    }
    Ok(out)
}

type Tri = [[i64; 3]; 3];

fn floor_triangles(assets: &Path, arena: &str) -> R<(Value, Vec<Tri>)> {
    let data: Value = serde_json::from_str(&read(&assets.join("courses").join(format!("mk64_{arena}.json")))?).map_err(|e| e.to_string())?;
    let vs: Vec<[i64; 3]> = data["vertices"].as_array().ok_or("vertices")?.iter().map(|v| [v[0].as_i64().unwrap_or(0), v[1].as_i64().unwrap_or(0), v[2].as_i64().unwrap_or(0)]).collect();
    let mut tris = Vec::new();
    for t in data["triangles"].as_array().ok_or("triangles")? {
        let idx = |k: usize| t[k].as_u64().map(|i| vs[i as usize]).ok_or("triangle index");
        let p = [idx(0)?, idx(1)?, idx(2)?];
        let (ux, uz) = ((p[1][0] - p[0][0]) as f64, (p[1][2] - p[0][2]) as f64);
        let (vx, vz) = ((p[2][0] - p[0][0]) as f64, (p[2][2] - p[0][2]) as f64);
        if (ux * vz - uz * vx).abs() > 1e-6 {
            tris.push(p);
        }
    }
    Ok((data, tris))
}

/// Height of the floor triangle under (x, z) closest to `near`.
fn floor_y(tris: &[Tri], x: f64, z: f64, near: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for p in tris {
        let f = |i: usize, k: usize| p[i][k] as f64;
        let d = (f(1, 2) - f(2, 2)) * (f(0, 0) - f(2, 0)) + (f(2, 0) - f(1, 0)) * (f(0, 2) - f(2, 2));
        if d.abs() < 1e-9 {
            continue;
        }
        let a = ((f(1, 2) - f(2, 2)) * (x - f(2, 0)) + (f(2, 0) - f(1, 0)) * (z - f(2, 2))) / d;
        let b = ((f(2, 2) - f(0, 2)) * (x - f(2, 0)) + (f(0, 0) - f(2, 0)) * (z - f(2, 2))) / d;
        let c = 1.0 - a - b;
        if a.min(b).min(c) < -1e-6 {
            continue;
        }
        let y = a * f(0, 1) + b * f(1, 1) + c * f(2, 1);
        if best.is_none_or(|bst| (y - near).abs() < (bst - near).abs()) {
            best = Some(y);
        }
    }
    best
}

/// 64 x 96 white-on-transparent top-down footprint of every floor triangle; returns the fit.
fn footprint_png(path: &Path, tris: &[Tri]) -> R<Value> {
    let xs: Vec<i64> = tris.iter().flatten().map(|q| q[0]).collect();
    let zs: Vec<i64> = tris.iter().flatten().map(|q| q[2]).collect();
    let (xmax, xmin, zmax, zmin) = (*xs.iter().max().unwrap(), *xs.iter().min().unwrap(), *zs.iter().max().unwrap(), *zs.iter().min().unwrap());
    let scale = (56.0 / ((xmax - xmin).max(1) as f64)).min(88.0 / ((zmax - zmin).max(1) as f64));
    let (cx, cz) = ((xmax + xmin) as f64 / 2.0, (zmax + zmin) as f64 / 2.0);
    let mut pixels = vec![0u8; 64 * 96 * 4];
    for p in tris {
        let pts: Vec<(f64, f64)> = p.iter().map(|q| (31.5 + (q[0] as f64 - cx) * scale, 47.5 + (q[2] as f64 - cz) * scale)).collect();
        let x0 = pts.iter().map(|q| q.0).fold(f64::INFINITY, f64::min) as i64;
        let x1 = pts.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max) as i64 + 1;
        let y0 = pts.iter().map(|q| q.1).fold(f64::INFINITY, f64::min) as i64;
        let y1 = pts.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max) as i64 + 1;
        for py in y0.max(0)..=y1.min(95) {
            for px in x0.max(0)..=x1.min(63) {
                let (a, b, c) = (pts[0], pts[1], pts[2]);
                let d = (b.1 - c.1) * (a.0 - c.0) + (c.0 - b.0) * (a.1 - c.1);
                if d.abs() < 1e-9 {
                    continue;
                }
                let (fx, fy) = (px as f64 + 0.5, py as f64 + 0.5);
                let u = ((b.1 - c.1) * (fx - c.0) + (c.0 - b.0) * (fy - c.1)) / d;
                let v = ((c.1 - a.1) * (fx - c.0) + (a.0 - c.0) * (fy - c.1)) / d;
                if u.min(v).min(1.0 - u - v) >= -0.02 {
                    let i = (py as usize * 64 + px as usize) * 4;
                    pixels[i..i + 4].copy_from_slice(&[255; 4]);
                }
            }
        }
    }
    std::fs::write(path, png_rgba(64, 96, &pixels, 6)?).map_err(|e| e.to_string())?;
    Ok(json!({"cx": cx, "cz": cz, "scale": scale}))
}

pub fn export(source: &Path, out: &Path, assets: &Path) -> R<()> {
    let cup_rows = cups(source)?;
    let mut modes = Map::new();
    modes.insert("source".into(), json!("n64decomp/mk64"));
    modes.insert("cups".into(), Value::Array((0..4).map(|i| json!({"name": CUP_NAMES[i], "tracks": cup_rows[i]})).collect()));
    modes.insert("battle".into(), json!(cup_rows[4]));
    modes.insert("gpPoints".into(), json!(point_rewards(source)?));
    modes.insert("ccNames".into(), json!(["50cc", "100cc", "150cc", "Extra"]));
    modes.insert("trophies".into(), json!(["none", "bronze", "silver", "gold"]));
    let starts = battle_starts(source)?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut arenas = Map::new();
    for (k, arena) in ARENAS.iter().enumerate() {
        let (data, tris) = floor_triangles(assets, arena)?;
        let scale = data["mk64"]["unit"].as_i64().ok_or("unit")? as f64;
        let s = &starts.iter().find(|(n, _)| n == arena).ok_or("start grid")?.1;
        let mut grid = Vec::new();
        let mut gpos: Vec<(f64, f64, f64)> = Vec::new();
        for i in 0..4 {
            let (x, z) = (s.x[i], s.z[i]);
            let y = floor_y(&tris, x * scale, z * scale, (s.height + 50) as f64 * scale).unwrap_or(s.height as f64 * scale);
            let yy = round_f(y / scale, 2);
            gpos.push((x, yy, z));
            grid.push(json!({"pos": [x, yy, z], "rotation": s.rotation[i]}));
        }
        let radius = gpos.iter().map(|g| g.0.hypot(g.2)).sum::<f64>() / 4.0;
        let mut ring = Vec::new();
        for k in 0..32 {
            let ang = 2.0 * std::f64::consts::PI * k as f64 / 32.0;
            let (x, z) = (radius * ang.sin(), radius * ang.cos());
            let y = floor_y(&tris, x * scale, z * scale, gpos[0].1 * scale);
            ring.push(json!([round(x), round(y.unwrap_or(gpos[0].1 * scale) / scale), round(z)]));
        }
        let boxes: Vec<Value> = Course::new(source, arena)?.item_boxes()?.iter().map(|b| json!(b)).collect();
        let view = footprint_png(&out.join(format!("hud-minimap-{}.png", arena.replace('_', "-"))), &tris)?;
        arenas.insert(arena.to_string(), json!({"name": NAMES[k], "grid": grid, "boxes": boxes, "height": s.height, "minimap": view}));
        let kart = json!({"source": "n64decomp/mk64", "romSha1": "579c48e211ae952530ffc8738709f078d5dd215e", "model": format!("levels/mk64_{arena}/terrain.glb"), "path": ring, "scale": 8, "itemBoxes": boxes});
        std::fs::write(out.join(format!("{}.json", arena.replace('_', "-"))), dumps(&kart, None)).map_err(|e| e.to_string())?;
    }
    modes.insert("arenas".into(), Value::Object(arenas));
    std::fs::write(out.join("modes.json"), dumps(&Value::Object(modes), None) + "\n").map_err(|e| e.to_string())?;
    Ok(())
}
