//! Port of `scripts/export-bk-setup.py`: places the Banjo-Kazooie enemies, collectibles, climbs, moles, note doors and warps
//! in the exported worlds from the ROM's map setup files, and writes `bk_setup_report.json` / `bk_warp_report.json`.
use super::banjo::{i16_at, u16_at, u32_at, u8_at};
use super::levels::WORLD_SCALE;
use super::maps::{exported_maps, group_of, is_subarea, skip_reason, skipped_maps, Catalogue};
use super::Assets;
use crate::pyc::{dumps, round, R};
use crate::zpng::round_f;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// actor id -> species key
const ENEMIES: &[(i64, &str)] = &[
    (0x04, "bigbutt"),
    (0x05, "termite"),
    (0x06, "grublin"),
    (0x12, "beehive"),
    (0x56, "shrapnel"),
    (0x67, "snippet"),
    (0xF5, "mutie_snippet"),
    (0x124, "sir_slush"),
    (0x163, "bat"),
    (0x1E9, "snarebear"),
    (0x34E, "limbo"),
    (0x375, "grublin_hood"),
    (0x133, "flibbit"),
    (0x1CC, "grille_chompa"),
    (0x29F, "clucker"),
];

const SKIPPED_ENEMIES: &[(i64, &str)] = &[
    (0xE, "small bull (unplaced)"),
    (0xA, "chump fish (spawned by script)"),
    (0x69, "clam (minigame prop)"),
    (0x117, "Nipper boss"),
    (0x137, "yellow Flibbit (minigame)"),
    (0x134, "Buzzbomb (skeletal-animation model)"),
    (0x13B, "Flotsam (skeletal-animation model)"),
    (0x1C6, "Grimlet (skeletal-animation model)"),
    (0x1FA, "Croctus (feeding minigame)"),
    (0x147, "Ancient One (statue)"),
    (0x380, "Scarab Beetle (skeletal-animation model)"),
    (0x120, "Slappa (boss)"),
    (0x2A2, "Caterpillar (skeletal-animation model)"),
    (0x337, "Twinkly Muncher (minigame)"),
    (0x336, "Twinkly box (minigame)"),
    (0x37D, "ice cube (scenery)"),
    (0x34D, "bee swarm (effect)"),
    (0xCA, "Tee-Hee (ghost)"),
    (0x354, "drips (effect)"),
    (0xC7, "gravestone (scenery)"),
    (0x30F, "Whipcrack (scenery)"),
    (0x36D, "Collywobble (tutorial boss)"),
    (0x36E, "Bawl (tutorial boss)"),
    (0x36F, "Topper (tutorial boss)"),
    (0x167, "attack tutorial"),
    (0x37E, "dead Snare-bear (scenery)"),
    (0xF6, "big alligator (boss)"),
    (0xE8, "Tanktup (boss)"),
    (0x118, "Grabba (boss)"),
];

/// sprite id (sprite asset - 0x572) -> pickup kind index
const SPRITE_PICKUPS: &[(i64, i64)] = &[(0x164, 0), (0x165, 1), (0xE, 2), (0x15F, 3)];
const ACTOR_PICKUPS: &[(i64, i64)] = &[(0x52, 1), (0x47, 4), (0x46, 5), (0x2D, 6)];
const PICKUP_NAMES: [&str; 7] = ["note", "egg", "redFeather", "goldFeather", "honeycomb", "jiggy", "mumboToken"];

const CLIMB_BASE: i64 = 0x26;
const CLIMB_TOP_1: i64 = 0x27;
const CLIMB_TOP_2: i64 = 0x28;
const MOLE_SM: i64 = 0x12B;
const MOLE_WORLD: i64 = 0x37A;
const NOTE_DOOR: i64 = 0x203;

const EXIT_SKIP_IDS: &[(i64, &str)] = &[(0x63, "restores a saved position"), (0x65, "placement skipped (a cutscene positions the player)")];
const PAD_RADIUS: i64 = 90;
const LOBBY_RETURNS: &[(&str, &str, i64)] = &[
    ("bk_gl_mm_lobby", "bk_mumbos_mountain", 0x2),
    ("bk_gl_ttc_lobby", "bk_treasure_trove_cove", 0x4),
    ("bk_gl_cc_lobby", "bk_clankers_cavern", 0x2),
    ("bk_gl_bgs_lobby", "bk_bubblegloop_swamp", 0x2),
    ("bk_gl_fp_lobby", "bk_freezeezy_peak", 0x6),
    ("bk_gl_gv_lobby", "bk_gobis_valley", 0x3),
    ("bk_gl_ccw_lobby", "bk_click_clock_wood", 0x6),
    ("bk_gl_rbb_lobby", "bk_rusty_bucket_bay", 0x2),
    ("bk_gl_mmm_lobby", "bk_mad_monster_mansion", 0x2),
];
const UNREACHABLE: &[(&str, &str)] = &[
    ("bk_gl_furnace_fun", "entered from the Furnace Fun minigame start (minigame.c), not from a trigger"),
    ("bk_gl_dingpot", "entered from Furnace Fun / the final boss script"),
    ("bk_gl_battlements", "entered from the final battle script"),
    ("bk_ttc_sharkfood_island", "entered by the Sharkfood Island script (TTC/code_26D0.c); its exit uses a hard-coded position"),
];

const BK_TO_JUMPMAN_YAW: f64 = 65536.0 / 360.0;

// ------------------------------------------------------------------------------------------------ setup file parsing

/// `File` of src/core2/file.c: one-byte look-ahead markers.
struct Stream<'a> {
    d: &'a [u8],
    p: usize,
    last: i32,
}

impl<'a> Stream<'a> {
    fn byte(&mut self) -> R<u8> {
        let b = u8_at(self.d, self.p)?;
        self.p += 1;
        Ok(b)
    }
    fn expect(&mut self, e: u8) -> R<bool> {
        if self.last == -1 {
            let b = self.byte()?;
            if b == e {
                return Ok(true);
            }
            self.last = b as i32;
            return Ok(false);
        }
        if e as i32 == self.last {
            self.last = -1;
            return Ok(true);
        }
        Ok(false)
    }
    fn words(&mut self, n: usize) -> R<Vec<i32>> {
        let mut w = Vec::new();
        for _ in 0..n {
            w.push(u32_at(self.d, self.p)? as i32);
            self.p += 4;
        }
        Ok(w)
    }
    fn raw(&mut self, n: usize) -> Vec<u8> {
        let a = self.p.min(self.d.len());
        let b = (self.p + n).min(self.d.len());
        self.p += n;
        self.d[a..b].to_vec()
    }
}

/// -> (node props, props): the 20-byte actor/trigger nodes and the 12-byte model/sprite props.
fn parse_setup(data: &[u8]) -> R<(Vec<Vec<u8>>, Vec<Vec<u8>>)> {
    let mut f = Stream { d: data, p: 0, last: -1 };
    let (mut nodes, mut props) = (Vec::new(), Vec::new());
    while !f.expect(0)? {
        if f.expect(2)? {
            continue;
        }
        if !f.expect(1)? {
            break;
        }
        let lo = if f.expect(1)? { Some(f.words(3)?) } else { None };
        let hi = f.words(3)?;
        let lo = lo.ok_or("setup cube range without a lower corner")?;
        for _x in lo[0]..=hi[0] {
            for _y in lo[1]..=hi[1] {
                for _z in lo[2]..=hi[2] {
                    while !f.expect(1)? {
                        if f.expect(0)? {
                            f.words(3)?;
                            f.words(3)?;
                        } else {
                            // `not (f.expect(2) and (f.words(3) or True)) and f.expect(3)`
                            let skip = if f.expect(2)? {
                                f.words(3)?;
                                true
                            } else {
                                false
                            };
                            if !skip && f.expect(3)? {
                                if f.expect(0x0A)? {
                                    let n = f.byte()?;
                                    if f.expect(0x0B)? {
                                        for _ in 0..n {
                                            nodes.push(f.raw(20));
                                        }
                                    }
                                } else if f.expect(6)? {
                                    let n = f.byte()?;
                                    if f.expect(7)? {
                                        f.raw(12 * n as usize);
                                    }
                                }
                                if f.expect(8)? {
                                    let n = f.byte()?;
                                    if f.expect(9)? {
                                        for _ in 0..n {
                                            props.push(f.raw(12));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        f.expect(0)?;
    }
    Ok((nodes, props))
}

struct Actor {
    id: i64,
    pos: [i64; 3],
    yaw: i64,
    scale: i64,
    radius: i64,
}

fn decode(nodes: &[Vec<u8>], props: &[Vec<u8>]) -> R<(Vec<Actor>, Vec<(i64, i64, i64, i64)>)> {
    let (mut actors, mut pickups) = (Vec::new(), Vec::new());
    for n in nodes {
        let (x, y, z, bf, actor) = (i16_at(n, 0)? as i64, i16_at(n, 2)? as i64, i16_at(n, 4)? as i64, u16_at(n, 6)? as i64, u16_at(n, 8)? as i64);
        let category = (bf >> 1) & 63;
        if category != 6 {
            continue;
        }
        let word = u32_at(n, 12)? as i64;
        actors.push(Actor { id: actor, pos: [x, y, z], yaw: word >> 23, scale: word & 0x7FFFFF, radius: bf >> 7 });
    }
    for p in props {
        let flags = u8_at(p, 11)?;
        if flags & 2 != 0 {
            continue;
        }
        let (x, y, z) = (i16_at(p, 4)? as i64, i16_at(p, 6)? as i64, i16_at(p, 8)? as i64);
        let sprite = (u32_at(p, 0)? >> 20) as i64;
        if flags & 1 != 0 {
            continue;
        }
        if let Some((_, k)) = SPRITE_PICKUPS.iter().find(|(s, _)| *s == sprite) {
            pickups.push((*k, x, y, z));
        }
    }
    for a in &actors {
        if let Some((_, k)) = ACTOR_PICKUPS.iter().find(|(s, _)| *s == a.id) {
            pickups.push((*k, a.pos[0], a.pos[1], a.pos[2]));
        }
    }
    Ok((actors, pickups))
}

fn r1(c: i64) -> f64 {
    round_f(c as f64 * WORLD_SCALE, 1)
}

/// `bkClimbs`: [bottom xyz, top xyz, radius, kind] rows.
fn find_climbs(actors: &[Actor]) -> Vec<Value> {
    let tops: Vec<&Actor> = actors.iter().filter(|a| a.id == CLIMB_TOP_1 || a.id == CLIMB_TOP_2).collect();
    let mut rows = Vec::new();
    for a in actors {
        if a.id != CLIMB_BASE || tops.is_empty() {
            continue;
        }
        let d2 = |t: &Actor| ((t.pos[0] - a.pos[0]).pow(2) + (t.pos[2] - a.pos[2]).pow(2)) as f64;
        let mut t = tops[0];
        for c in &tops[1..] {
            if d2(c) < d2(t) {
                t = c;
            }
        }
        let kind = if t.id == CLIMB_TOP_1 { 1 } else { 2 };
        rows.push(json!([r1(a.pos[0]), r1(a.pos[1]), r1(a.pos[2]), r1(t.pos[0]), r1(t.pos[1]), r1(t.pos[2]), r1(a.radius), kind]));
    }
    rows
}

struct RawWarp {
    handler: i64,
    pos: [i64; 3],
    radius: i64,
}

struct Exit {
    pos: [i64; 3],
    yaw: i64,
}

/// -> (warps, exits in insertion order)
fn decode_transitions(cat: &Catalogue, nodes: &[Vec<u8>]) -> R<(Vec<RawWarp>, Vec<(i64, Exit)>)> {
    let (mut warps, mut exits): (Vec<RawWarp>, Vec<(i64, Exit)>) = (vec![], vec![]);
    for n in nodes {
        let (x, y, z, bf, actor) = (i16_at(n, 0)? as i64, i16_at(n, 2)? as i64, i16_at(n, 4)? as i64, u16_at(n, 6)? as i64, u16_at(n, 8)? as i64);
        if bf & 1 != 0 {
            continue;
        }
        let category = (bf >> 1) & 63;
        if category == 3 {
            warps.push(RawWarp { handler: actor, pos: [x, y, z], radius: bf >> 7 });
        } else if category == 6 {
            if let Some(&exit_id) = cat.exit_actors.get(&actor) {
                let word = u32_at(n, 12)? as i64;
                if !exits.iter().any(|(e, _)| *e == exit_id) {
                    exits.push((exit_id, Exit { pos: [x, y, z], yaw: word >> 23 }));
                }
            }
        }
    }
    Ok((warps, exits))
}

fn yaw_units(deg: i64) -> i64 {
    let u = round(deg as f64 * BK_TO_JUMPMAN_YAW);
    (u + 32768).rem_euclid(65536) - 32768
}

/// Trigger volume -> (object position, param) of the sim `warp` object.
fn warp_trigger(pos: [i64; 3], radius: i64) -> ([f64; 3], i64) {
    let (x, y, z) = (pos[0] as f64 * WORLD_SCALE, pos[1] as f64 * WORLD_SCALE, pos[2] as f64 * WORLD_SCALE);
    let rw = radius as f64 * WORLD_SCALE;
    let r10 = 254.min(1.max(round(rw / 10.0)));
    let height = if 40.0 > 2.0 * rw - 35.0 { 40.0 } else { 2.0 * rw - 35.0 };
    let h10 = 99.min(round(height / 10.0));
    ([round_f(x, 1), round_f(y - rw + 35.0, 1), round_f(z, 1)], r10 + 1000 * h10)
}

fn trigger_contains(obj: &[f64; 3], param: i64, p: &[f64; 3]) -> bool {
    let r = ((param % 1000) * 10 + 37) as f64;
    let h = ((param / 1000) * 10) as f64;
    ((p[0] - obj[0]).powi(2) + (p[2] - obj[2]).powi(2) < r * r) && obj[1] - 160.0 <= p[1] && p[1] <= obj[1] + h
}

/// Up-facing collision triangles of an exported course on a grid: is there ground under a point?
struct Floors {
    v: Vec<[f64; 3]>,
    cells: HashMap<(i64, i64), Vec<[usize; 3]>>,
}

const CELL: f64 = 250.0;

impl Floors {
    fn new(course: &Value) -> R<Floors> {
        let v: Vec<[f64; 3]> = course["vertices"].as_array().ok_or("vertices")?.iter().map(|p| [p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0), p[2].as_f64().unwrap_or(0.0)]).collect();
        let mut cells: HashMap<(i64, i64), Vec<[usize; 3]>> = HashMap::new();
        for t in course["triangles"].as_array().ok_or("triangles")? {
            let ix = |k: usize| t[k].as_u64().map(|i| i as usize).ok_or("triangle index");
            let (a, b, c) = (ix(0)?, ix(1)?, ix(2)?);
            let (p, q, r) = (v[a], v[b], v[c]);
            let (ux, uy, uz) = (q[0] - p[0], q[1] - p[1], q[2] - p[2]);
            let (vx, vy, vz) = (r[0] - p[0], r[1] - p[1], r[2] - p[2]);
            let ny = uz * vx - ux * vz;
            let norm = ((uy * vz - uz * vy).powi(2) + ny.powi(2) + (ux * vy - uy * vx).powi(2)).sqrt();
            if norm < 1e-6 || ny.abs() / norm < 0.5 {
                continue;
            }
            let xs = [p[0], q[0], r[0]];
            let zs = [p[2], q[2], r[2]];
            let fmin = |a: &[f64; 3]| a.iter().copied().fold(f64::INFINITY, f64::min);
            let fmax = |a: &[f64; 3]| a.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            for ix in (fmin(&xs) / CELL).floor() as i64..=(fmax(&xs) / CELL).floor() as i64 {
                for iz in (fmin(&zs) / CELL).floor() as i64..=(fmax(&zs) / CELL).floor() as i64 {
                    cells.entry((ix, iz)).or_default().push([a, b, c]);
                }
            }
        }
        Ok(Floors { v, cells })
    }

    /// Height of the highest floor at or below y + 150, or None.
    fn below(&self, x: f64, y: f64, z: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for t in self.cells.get(&((x / CELL).floor() as i64, (z / CELL).floor() as i64)).into_iter().flatten() {
            let (p, q, r) = (self.v[t[0]], self.v[t[1]], self.v[t[2]]);
            let den = (q[2] - r[2]) * (p[0] - r[0]) + (r[0] - q[0]) * (p[2] - r[2]);
            if den.abs() < 1e-9 {
                continue;
            }
            let l1 = ((q[2] - r[2]) * (x - r[0]) + (r[0] - q[0]) * (z - r[2])) / den;
            let l2 = ((r[2] - p[2]) * (x - r[0]) + (p[0] - r[0]) * (z - r[2])) / den;
            if l1.min(l2).min(1.0 - l1 - l2) < -1e-6 {
                continue;
            }
            let h = l1 * p[1] + l2 * q[1] + (1.0 - l1 - l2) * r[1];
            if h <= y + 150.0 && best.is_none_or(|b| h > b) {
                best = Some(h);
            }
        }
        best
    }

    fn supports(&self, p: &[f64; 3]) -> bool {
        self.below(p[0], p[1], p[2]).is_some_and(|h| p[1] - h <= 1200.0)
    }
}

struct WarpP {
    pos: [i64; 3],
    radius: i64,
    to: String,
    exit: i64,
    func: String,
    kind: &'static str,
}

struct Spawn {
    pos: [f64; 3],
    face_yaw: i64,
    supported: bool,
}

struct Plan {
    map: i64,
    actors: Vec<Actor>,
    pickups: Vec<(i64, i64, i64, i64)>,
    raw_warps: Vec<RawWarp>,
    exits: Vec<(i64, Exit)>,
    warps: Vec<WarpP>,
    skipped: Vec<Value>,
    triggers: Vec<([f64; 3], i64)>,
    spawns: BTreeMap<i64, Spawn>,
}

fn hex(i: i64) -> String {
    format!("{i:#x}")
}

fn enemy_param(a: &Actor) -> i64 {
    let scale = if a.scale != 0 { a.scale } else { 100 };
    a.id | (scale.min(1000) << 16)
}

pub fn export(assets: &Assets, cat: &Catalogue, out: &Path) -> R<()> {
    let maps = exported_maps(cat)?; // (map id, course id) in order
    let mut plan: Vec<(String, Plan)> = Vec::new();
    for (map_id, wid) in &maps {
        let (nodes, props) = parse_setup(&assets.read((*map_id + 0x71C) as usize)?)?;
        let (actors, pickups) = decode(&nodes, &props)?;
        let (raw_warps, exits) = decode_transitions(cat, &nodes)?;
        plan.push((wid.clone(), Plan { map: *map_id, actors, pickups, raw_warps, exits, warps: vec![], skipped: vec![], triggers: vec![], spawns: BTreeMap::new() }));
    }
    let idx_of = |plan: &Vec<(String, Plan)>, wid: &str| plan.iter().position(|(w, _)| w == wid);
    let course_of = |mid: i64| maps.iter().find(|(m, _)| *m == mid).map(|(_, c)| c.clone());

    // ---- warps the setup files place
    for i in 0..plan.len() {
        let raw: Vec<(i64, [i64; 3], i64)> = plan[i].1.raw_warps.iter().map(|w| (w.handler, w.pos, w.radius)).collect();
        for (handler, pos, radius) in raw {
            let Some(h) = cat.warps.get(&handler) else {
                plan[i].1.skipped.push(json!({"handler": handler, "pos": pos, "reason": "handler is not a map change (orange pad, jiggy pad, cutscene or unused slot)"}));
                continue;
            };
            let target = course_of(h.map);
            let reason: Option<String> = match &target {
                None => Some(format!("target map {} is not exported ({})", hex(h.map), skip_reason(h.map).unwrap_or("no level model"))),
                Some(t) => {
                    let ti = idx_of(&plan, t).ok_or("target plan")?;
                    if let Some((_, why)) = EXIT_SKIP_IDS.iter().find(|(e, _)| *e == h.exit) {
                        Some(format!("exit {}: {}", hex(h.exit), why))
                    } else if h.exit >= 0x80 {
                        Some(format!("exit {}: hard-coded exit position", hex(h.exit)))
                    } else if !plan[ti].1.exits.iter().any(|(e, _)| *e == h.exit) {
                        Some(format!("exit {} has no node in the target map", hex(h.exit)))
                    } else {
                        None
                    }
                }
            };
            if let Some(reason) = reason {
                plan[i].1.skipped.push(json!({"handler": handler, "fn": h.func, "pos": pos, "reason": reason}));
                continue;
            }
            plan[i].1.warps.push(WarpP { pos, radius, to: target.unwrap(), exit: h.exit, func: h.func.clone(), kind: "setup" });
        }
    }

    let has_edge = |plan: &Vec<(String, Plan)>, a: &str, b: &str| idx_of(plan, a).is_some_and(|i| plan[i].1.warps.iter().any(|w| w.to == b));
    let nearest_exit = |plan: &Vec<(String, Plan)>, wid: &str, pos: [i64; 3]| -> Option<i64> {
        let exits = &plan[idx_of(plan, wid)?].1.exits;
        let mut best: Option<(i64, i64)> = None;
        for (e, ex) in exits {
            if *e >= 0x1E {
                continue;
            }
            let d: i64 = (0..3).map(|k| (ex.pos[k] - pos[k]).pow(2)).sum();
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((*e, d));
            }
        }
        best.map(|b| b.0)
    };

    // ---- leaving a world: the pause menu's "exit to the lair"
    for (lobby, world, lobby_exit) in LOBBY_RETURNS {
        let (Some(li), Some(wi)) = (idx_of(&plan, lobby), idx_of(&plan, world)) else { continue };
        let Some(enter) = plan[li].1.warps.iter().find(|w| w.to == *world) else { continue };
        let enter_exit = enter.exit;
        if !plan[li].1.exits.iter().any(|(e, _)| e == lobby_exit) {
            continue;
        }
        let pos = plan[wi].1.exits.iter().find(|(e, _)| *e == enter_exit).ok_or("arrival of the lobby's world door")?.1.pos;
        plan[wi].1.warps.push(WarpP { pos, radius: PAD_RADIUS, to: lobby.to_string(), exit: *lobby_exit, func: "pause menu: exit to the lair".into(), kind: "return" });
    }

    // ---- one-way doors get the mirror trigger at the arrival point they lead back to
    for ai in 0..plan.len() {
        let a = plan[ai].0.clone();
        let snapshot: Vec<(String, i64, [i64; 3], String, &'static str)> = plan[ai].1.warps.iter().map(|w| (w.to.clone(), w.exit, w.pos, w.func.clone(), w.kind)).collect();
        for (b, exit, pos, func, kind) in snapshot {
            if b == a || has_edge(&plan, &b, &a) || kind != "setup" {
                continue;
            }
            let bi = idx_of(&plan, &b).ok_or("mirror target")?;
            let ex_pos = plan[bi].1.exits.iter().find(|(e, _)| *e == exit).map(|(_, e)| e.pos);
            let back = nearest_exit(&plan, &a, pos);
            let (Some(ex_pos), Some(back)) = (ex_pos, back) else { continue };
            if group_of(cat, plan[ai].1.map)? == "GL" && group_of(cat, plan[bi].1.map)? != "GL" {
                continue;
            }
            plan[bi].1.warps.push(WarpP { pos: ex_pos, radius: PAD_RADIUS, to: a.clone(), exit: back, func: format!("mirror of {func}"), kind: "mirror" });
        }
    }

    // ---- triggers and arrival points of every map
    let mut courses: HashMap<String, Value> = HashMap::new();
    for (wid, p) in plan.iter_mut() {
        let path = out.join(format!("{wid}.json"));
        let course: Value = serde_json::from_str(&crate::pyc::read(&path)?).map_err(|e| format!("{}: {e}", path.display()))?;
        let floors = Floors::new(&course)?;
        courses.insert(wid.clone(), course);
        p.triggers = p.warps.iter().map(|w| warp_trigger(w.pos, w.radius)).collect();
        for (ex, e) in {
            let mut v: Vec<&(i64, Exit)> = p.exits.iter().collect();
            v.sort_by_key(|(e, _)| *e);
            v
        }
        .into_iter()
        .map(|(a, b)| (a, b))
        {
            let mut pos = [r1(e.pos[0]), r1(e.pos[1]), r1(e.pos[2])];
            let yaw = e.yaw;
            for _ in 0..60 {
                if !p.triggers.iter().any(|(o, prm)| trigger_contains(o, *prm, &pos)) {
                    break;
                }
                pos[0] = round_f(pos[0] + 40.0 * (yaw as f64).to_radians().sin(), 1);
                pos[2] = round_f(pos[2] + 40.0 * (yaw as f64).to_radians().cos(), 1);
            }
            p.spawns.insert(*ex, Spawn { pos, face_yaw: yaw_units(yaw), supported: floors.supports(&pos) });
        }
    }
    // Arrival points with nothing under them: warps to them land on the map's first supported entry instead.
    let mut retargeted: Vec<Value> = Vec::new();
    for pi in 0..plan.len() {
        for wi in 0..plan[pi].1.warps.len() {
            let (to, exit) = (plan[pi].1.warps[wi].to.clone(), plan[pi].1.warps[wi].exit);
            let ti = idx_of(&plan, &to).ok_or("warp target")?;
            let t = &plan[ti].1.spawns;
            if !t.get(&exit).ok_or("warp target exit")?.supported {
                let safe = t.iter().find(|(ex, s)| **ex < 0x1E && s.supported).map(|(e, _)| *e).or_else(|| t.iter().find(|(_, s)| s.supported).map(|(e, _)| *e));
                retargeted.push(json!({"course": plan[pi].0, "to": to, "exit": exit, "instead": safe, "fn": plan[pi].1.warps[wi].func}));
                if let Some(s) = safe {
                    plan[pi].1.warps[wi].exit = s;
                }
            }
        }
    }

    let mut report = Map::new();
    let mut warp_report = Map::new();
    for (wid, p) in &plan {
        let mut course = courses.remove(wid).ok_or("course")?;
        let obj = course.as_object_mut().ok_or("course object")?;
        let mut objects: Vec<Value> = obj.get("objects").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter(|o| o["behavior"].as_str() != Some("bkEnemy")).collect();
        let mut placed: BTreeMap<&str, i64> = BTreeMap::new();
        for a in &p.actors {
            let Some((_, key)) = ENEMIES.iter().find(|(i, _)| *i == a.id) else { continue };
            *placed.entry(key).or_default() += 1;
            objects.push(json!({"model": "", "pos": [r1(a.pos[0]), r1(a.pos[1]), r1(a.pos[2])], "yaw": a.yaw, "param": enemy_param(a), "behavior": "bkEnemy"}));
        }
        obj.insert("objects".into(), Value::Array(objects));
        obj.insert("bkClimbs".into(), Value::Array(find_climbs(&p.actors)));
        obj.insert("bkMoles".into(), Value::Array(p.actors.iter().filter(|a| a.id == MOLE_SM || a.id == MOLE_WORLD).map(|a| json!([r1(a.pos[0]), r1(a.pos[1]), r1(a.pos[2]), a.radius, if a.id == MOLE_SM { 1 } else { 0 }])).collect()));
        obj.insert("bkNoteDoors".into(), Value::Array(p.actors.iter().filter(|a| a.id == NOTE_DOOR).map(|a| json!([r1(a.pos[0]), r1(a.pos[1]), r1(a.pos[2]), a.yaw, a.radius])).collect()));
        obj.insert("bkPickups".into(), Value::Array(p.pickups.iter().map(|(k, x, y, z)| json!([k, r1(*x), r1(*y), r1(*z)])).collect()));
        let out_warps: Vec<Value> = p.warps.iter().zip(&p.triggers).map(|(w, (o, param))| json!({"pos": o, "param": param, "warpTo": w.to, "warpEntry": format!("bk_exit_{}", w.exit), "fn": w.func, "kind": w.kind})).collect();
        obj.insert("warps".into(), Value::Array(out_warps.clone()));
        let mut spawns = Map::new();
        for (ex, s) in &p.spawns {
            if s.supported {
                spawns.insert(format!("bk_exit_{ex}"), json!({"pos": s.pos, "faceYaw": s.face_yaw, "behavior": ""}));
            }
        }
        obj.insert("warpSpawns".into(), Value::Object(spawns.clone()));
        if is_subarea(wid) {
            let first = p.spawns.iter().find(|(ex, s)| **ex < 0x1E && s.supported).map(|(e, _)| *e);
            if let Some(f) = first {
                let sp = &spawns[&format!("bk_exit_{f}")];
                obj.insert("spawn".into(), json!({"pos": sp["pos"], "faceYaw": sp["faceYaw"]}));
            }
        }
        std::fs::write(out.join(format!("{wid}.json")), dumps(&course, None)).map_err(|e| e.to_string())?;

        let mut exits_sorted: Vec<i64> = p.exits.iter().map(|(e, _)| *e).collect();
        exits_sorted.sort();
        warp_report.insert(wid.clone(), json!({"map": p.map, "warps": out_warps.len(), "exits": exits_sorted, "skippedWarps": p.skipped}));
        let mut every: BTreeMap<i64, i64> = BTreeMap::new();
        for a in &p.actors {
            *every.entry(a.id).or_default() += 1;
        }
        let mut kinds: BTreeMap<i64, i64> = BTreeMap::new();
        for (k, ..) in &p.pickups {
            *kinds.entry(*k).or_default() += 1;
        }
        let mut skipped_enemies = Map::new();
        for (i, c) in &every {
            if let Some((_, why)) = SKIPPED_ENEMIES.iter().find(|(s, _)| s == i) {
                skipped_enemies.insert(format!("{} {}", hex(*i), why), json!(c));
            }
        }
        let mut pickups = Map::new();
        for (k, v) in &kinds {
            pickups.insert(PICKUP_NAMES[*k as usize].to_string(), json!(v));
        }
        let mut actor_ids = Map::new();
        for (i, c) in &every {
            actor_ids.insert(hex(*i), json!(c));
        }
        report.insert(wid.clone(), json!({"map": p.map, "enemies": placed, "skippedEnemies": Value::Object(skipped_enemies), "pickups": Value::Object(pickups), "actorIds": Value::Object(actor_ids)}));
    }

    let mut implemented = Map::new();
    let mut e: Vec<&(i64, &str)> = ENEMIES.iter().collect();
    e.sort_by_key(|(i, _)| *i);
    for (i, k) in e {
        implemented.insert(hex(*i), json!(k));
    }
    let mut skipped = Map::new();
    let mut s: Vec<&(i64, &str)> = SKIPPED_ENEMIES.iter().collect();
    s.sort_by_key(|(i, _)| *i);
    for (i, k) in s {
        skipped.insert(hex(*i), json!(k));
    }
    std::fs::write(out.join("bk_setup_report.json"), dumps(&json!({"implemented": Value::Object(implemented), "skipped": Value::Object(skipped), "worlds": Value::Object(report)}), Some(1))).map_err(|e| e.to_string())?;

    let mut skipped_maps_json = Map::new();
    for (i, r) in skipped_maps() {
        let name = &cat.entry(i).ok_or_else(|| format!("map {i:#x} not in the catalogue"))?.name;
        skipped_maps_json.insert(format!("{} {}", hex(i), name), json!(r));
    }
    let mut unreachable = Map::new();
    for (k, v) in UNREACHABLE {
        unreachable.insert(k.to_string(), json!(v));
    }
    std::fs::write(out.join("bk_warp_report.json"), dumps(&json!({"skippedMaps": Value::Object(skipped_maps_json), "unreachable": Value::Object(unreachable), "retargeted": retargeted, "courses": Value::Object(warp_report)}), Some(1))).map_err(|e| e.to_string())
}
