//! Byte-identical port of `scripts/export-ssb64-motions.py`: the Super Smash Bros. fighters' motion event scripts
//! (hitboxes, sounds, effects, flags) from the user's ROM, run through `ftMainParseMotionEvent`'s interpreter offline, and
//! each motion's attack collisions as windows of frames placed on the animated skeleton (native/SSB64.md "Motion event
//! scripts"). Writes `public/models/ssb64/motion-events.json`.
use crate::oot::round_to;
use crate::pyc::{dumps, re, read, R};
use crate::ssb64::{self, apply, mat_mul, mat_trs, Files, Fighter, Pose, Ref, M4, FIGHTERS};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

const NO_SCRIPT: u64 = 0x8000_0000;
const SUBMOTION: u64 = 0x10;
const MAX_FRAMES: i64 = 1200;
const MAX_EVENTS: usize = 4000;

/// Words per event (`FTMotionEvent*` struct sizes / 4); anything else is one word.
fn size_of(op: &str) -> usize {
    match op {
        "MakeAttackColl" | "MakeAttackCollScaled" => 5,
        "SetDamageCollPartID" | "Effect" | "EffectItemHold" => 4,
        "SetAttackCollOffset" | "SetThrow" | "SetDamageThrown" | "Subroutine" | "Goto" | "SetParallelScript" => 2,
        _ => 1,
    }
}

fn flag(name: &str) -> R<u64> {
    Ok(match name {
        "FTANIM_FLAG_TRANSN_JOINT" => 0x8000_0000,
        "FTANIM_FLAG_XROTN_JOINT" => 0x4000_0000,
        "FTANIM_FLAG_YROTN_JOINT" => 0x2000_0000,
        "FTANIM_FLAG_SUBMOTION_SCRIPT" => 0x10,
        "FTANIM_FLAG_ANIMJOINT" => 0x08,
        "FTANIM_FLAG_TRANSLATE_SCALES" => 0x04,
        "FTANIM_FLAG_SHIELDPOSE" => 0x02,
        "FTANIM_FLAG_ANIMLOCKS" => 0x01,
        "FTANIM_FLAG_NONE" => 0,
        _ => return Err(format!("unknown motion flag {name}")),
    })
}

fn hex_or_dec(s: &str) -> Option<u64> {
    if let Some(h) = s.strip_prefix("0x") {
        u64::from_str_radix(h, 16).ok()
    } else if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

/// src/ft/ftdef.h enum FTMotionEvent: opcode -> name without the nFTMotionEvent prefix.
pub fn event_names(decomp: &Path) -> R<Vec<String>> {
    let text = read(&decomp.join("src/ft/ftdef.h"))?;
    let body = &text[text.find("typedef enum FTMotionEvent").ok_or("FTMotionEvent")?..];
    let body = &body[body.find('{').ok_or("{")? + 1..body.find('}').ok_or("}")?];
    let comment = re(r"//[^\n]*");
    let item = re(r"^nFTMotionEvent(\w+)$");
    let mut out = vec![];
    for raw in body.split(',') {
        let line = comment.replace_all(raw, "");
        if let Some(c) = item.captures(line.trim()) {
            out.push(c[1].to_string());
        }
    }
    Ok(out)
}

/// include/ft/motiondesc_offsets.h: script symbol -> byte offset in its file.
pub fn offsets(decomp: &Path) -> R<HashMap<String, u64>> {
    let text = read(&decomp.join("include/ft/motiondesc_offsets.h"))?;
    let mut out = HashMap::new();
    for c in re(r"#define (\w+) (0x[0-9A-Fa-f]+)").captures_iter(&text) {
        out.insert(c[1].to_string(), u64::from_str_radix(&c[2][2..], 16).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub struct Scripts {
    pub main: Option<usize>,
    pub sub: Option<usize>,
    /// (motion id, script offset, FTANIM flags)
    pub rows: Vec<(usize, u64, u64)>,
}

/// src/ft/ftdata.c: per fighter the main / sub motion files and every motion's script offset and flags.
pub fn motion_scripts(decomp: &Path, names: &HashMap<String, usize>) -> R<HashMap<String, Scripts>> {
    let text = read(&decomp.join("src/ft/ftdata.c"))?;
    let syms = offsets(decomp)?;
    let value = |expr: &str| -> R<u64> {
        let mut total = 0;
        for term in expr.split('+') {
            let term = term.trim();
            total += match hex_or_dec(term) {
                Some(v) => v,
                None => *syms.get(term).ok_or_else(|| format!("unknown script symbol {term}"))?,
            };
        }
        Ok(total)
    };
    let sym = re(r"^&ll(\w+)FileID");
    let file_id = |s: &str| -> R<Option<usize>> {
        match sym.captures(s) {
            Some(c) => Ok(Some(*names.get(&c[1]).ok_or_else(|| format!("no file {}", &c[1]))?)),
            None => Ok(None),
        }
    };
    let strip = re(r"(?s)//[^\n]*|/\*.*?\*/|[{}]");
    let mut out = HashMap::new();
    for f in FIGHTERS {
        let m = re(&format!(r"(?s)FTData dFT{f}Data =\s*\{{(.*?)\}};")).captures(&text).ok_or_else(|| format!("FTData for {f}"))?;
        let fields: Vec<String> = m[1].split(',').map(|x| x.trim().to_string()).collect();
        let tname = regex::escape(fields[25].trim_start_matches('&'));
        let table = re(&format!(r"(?s)FTMotionDesc {tname}\[\]\s*=\s*\{{(.*?)\n\}};")).captures(&text).ok_or("motion table")?[1].to_string();
        let flat = strip.replace_all(&table, " ");
        let items: Vec<&str> = flat.split(',').map(str::trim).filter(|x| !x.is_empty()).collect();
        let mut rows = vec![];
        let mut k = 0;
        while k + 2 < items.len() {
            let off = value(items[k + 1])?;
            if off != NO_SCRIPT {
                let mut flags = 0;
                for term in items[k + 2].split('|') {
                    let term = term.trim();
                    flags |= match term.strip_prefix("0x") {
                        Some(h) => u64::from_str_radix(h, 16).map_err(|e| e.to_string())?,
                        None => flag(term)?,
                    };
                }
                rows.push((k / 3, off, flags));
            }
            k += 3;
        }
        out.insert(f.to_string(), Scripts { main: file_id(&fields[1])?, sub: file_id(&fields[2])?, rows });
    }
    Ok(out)
}

fn bits(w: u32, lo: u32, n: u32, signed: bool) -> i64 {
    let v = ((w >> lo) as u64 & ((1u64 << n) - 1)) as i64;
    if signed && v >> (n - 1) != 0 {
        v - (1 << n)
    } else {
        v
    }
}

fn s16(w: u32, hi: bool) -> i64 {
    bits(w, if hi { 16 } else { 0 }, 16, true)
}

/// One event's fields from its words (`ws[0]` holds the opcode). Pointer events are handled by the interpreter.
fn decode(op: &str, ws: &[u32], fgm: &[String]) -> Map<String, Value> {
    let w = ws[0];
    let value = bits(w, 0, 26, false);
    let v = match op {
        "MakeAttackColl" | "MakeAttackCollScaled" => json!({
            "id": bits(w, 23, 3, false), "group": bits(w, 20, 3, false), "joint": bits(w, 13, 7, true), "damage": bits(w, 5, 8, false),
            "can_rebound": bits(w, 4, 1, false) == 1, "element": bits(w, 0, 4, false), "size": bits(ws[1], 16, 16, false),
            "offset": [s16(ws[1], false), s16(ws[2], true), s16(ws[2], false)],
            "angle": bits(ws[3], 22, 10, true), "knockback_scale": bits(ws[3], 12, 10, false), "knockback_weight": bits(ws[3], 2, 10, false),
            "hit_air": bits(ws[3], 0, 1, false) == 1, "hit_ground": bits(ws[3], 1, 1, false) == 1,
            "shield_damage": bits(ws[4], 24, 8, true), "fgm_level": bits(ws[4], 21, 3, false), "fgm_kind": bits(ws[4], 17, 4, false),
            "knockback_base": bits(ws[4], 7, 10, false)}),
        "SetAttackCollOffset" => json!({"id": bits(w, 23, 3, false), "offset": [bits(w, 7, 16, true), s16(ws[1], true), s16(ws[1], false)]}),
        "SetAttackCollDamage" => json!({"id": bits(w, 23, 3, false), "damage": bits(w, 15, 8, false)}),
        "SetAttackCollSize" => json!({"id": bits(w, 23, 3, false), "size": bits(w, 7, 16, false)}),
        "SetAttackCollSoundLevel" => json!({"id": bits(w, 23, 3, false), "fgm_level": bits(w, 20, 3, false)}),
        "ClearAttackCollID" | "RefreshAttackCollID" => json!({"id": value}),
        "PlayFGM" | "PlayLoopSFXStoreInfo" | "PlayVoiceStoreInfo" | "PlayLoopVoiceStoreInfo" | "PlayFGMStoreInfo" => {
            json!({"fgm": value, "name": fgm.get(value as usize).cloned().unwrap_or_default()})
        }
        "Effect" | "EffectItemHold" => json!({
            "joint": bits(w, 19, 7, true), "effect": bits(w, 10, 9, false), "flag": bits(w, 0, 10, false),
            "offset": [s16(ws[1], true), s16(ws[1], false), s16(ws[2], true)],
            "scatter": [s16(ws[2], false), s16(ws[3], true), s16(ws[3], false)]}),
        "SetHitStatusPartID" => json!({"joint": bits(w, 19, 7, true), "hitstatus": bits(w, 0, 19, false)}),
        "SetDamageCollPartID" => json!({
            "joint": bits(w, 19, 7, true), "offset": [s16(ws[1], true), s16(ws[1], false), s16(ws[2], true)],
            "size": [s16(ws[2], false), s16(ws[3], true), s16(ws[3], false)]}),
        "SetModelPartID" => json!({"joint": bits(w, 19, 7, true), "modelpart": bits(w, 0, 19, true)}),
        "SetTexturePartID" => json!({"texturepart": bits(w, 20, 6, false), "frame": bits(w, 0, 20, false)}),
        "SetColAnim" => json!({"colanim": bits(w, 18, 8, false), "length": bits(w, 0, 18, false)}),
        "SetSlopeContour" => json!({"flags": bits(w, 0, 3, false)}),
        "SetAfterImage" => json!({"is_itemswing": bits(w, 18, 8, false), "drawstatus": bits(w, 0, 18, true)}),
        "MakeRumble" => json!({"length": bits(w, 13, 13, false), "rumble": bits(w, 0, 13, false)}),
        "End" | "ClearAttackCollAll" | "StopLoopSFX" | "PlaySmashVoice" | "ResetDamageCollPartAll" | "ResetModelPartAll" | "HideModelPartAll"
        | "ResetColAnim" | "PauseScript" | "SetAirJumpAdd" | "SetAirJumpMax" => json!({}),
        _ => json!({"value": value}),
    };
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

struct Thread {
    pc: Option<Ref>,
    wait: Option<i64>,
    /// (is a loop, return / loop address, loop count)
    stack: Vec<(bool, Ref, i64)>,
    seen: HashMap<Ref, i64>,
}

impl Thread {
    fn new(pc: Option<Ref>) -> Self {
        Thread { pc, wait: Some(1), stack: vec![], seen: HashMap::new() }
    }
}

type Events = Vec<Map<String, Value>>;

/// `ftMainUpdateMotionEventsAll` over one motion: (events, end frame, loops).
fn run_script(files: &Files, start: Ref, ops: &[String], fgm: &[String]) -> R<(Events, Option<i64>, Vec<Value>)> {
    let mut events: Events = vec![];
    let mut loops = vec![];
    let mut threads = vec![Thread::new(Some(start))];
    let mut end = None;
    for frame in 0..MAX_FRAMES {
        if threads.iter().all(|t| t.pc.is_none()) {
            break;
        }
        let mut ti = 0;
        while ti < threads.len() {
            if threads[ti].pc.is_some() {
                if let Some(w) = threads[ti].wait.as_mut() {
                    *w -= 1;
                }
            }
            loop {
                let t = &threads[ti];
                let (Some(pc), Some(wait)) = (t.pc, t.wait) else { break };
                if wait > 0 {
                    break;
                }
                if events.len() >= MAX_EVENTS {
                    return Err(format!("script at {start:?}: more than {MAX_EVENTS} events"));
                }
                let file = files.get(pc.0)?;
                let word = |k: usize| -> R<u32> {
                    let o = pc.1 + 4 * k;
                    file.data.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("script word outside file {}", pc.0))
                };
                let w0 = word(0)?;
                let op = ops.get((w0 >> 26) as usize).ok_or_else(|| format!("opcode {}", w0 >> 26))?.as_str();
                let n = size_of(op);
                let ws: Vec<u32> = (0..n).map(word).collect::<R<_>>()?;
                threads[ti].seen.entry(pc).or_insert(frame);
                let nxt = (pc.0, pc.1 + 4 * n);
                let ptr = if n == 2 { files.ptr(Some(pc), 4)? } else { None };
                let value = (ws[0] & 0x3FF_FFFF) as i64;
                let mut ev: Option<Map<String, Value>> = None;
                let t = &mut threads[ti];
                match op {
                    "End" => {
                        t.pc = None;
                        ev = Some(Map::new());
                    }
                    "SyncWait" => {
                        t.wait = Some(wait + value);
                        t.pc = Some(nxt);
                    }
                    "AsyncWait" => {
                        t.wait = Some(value - frame);
                        t.pc = Some(nxt);
                    }
                    "LoopBegin" => {
                        t.stack.push((true, nxt, value));
                        t.pc = Some(nxt);
                    }
                    "LoopEnd" => {
                        let (kind, back, count) = *t.stack.last().ok_or("LoopEnd without LoopBegin")?;
                        let count = count - 1;
                        if count != 0 {
                            *t.stack.last_mut().unwrap() = (kind, back, count);
                            t.pc = Some(back);
                        } else {
                            t.stack.pop();
                            t.pc = Some(nxt);
                        }
                    }
                    "Subroutine" => {
                        t.stack.push((false, nxt, 0));
                        t.pc = ptr;
                    }
                    "Return" => {
                        t.pc = Some(t.stack.pop().ok_or("Return without Subroutine")?.1);
                    }
                    "Goto" => match ptr.and_then(|p| t.seen.get(&p).copied()) {
                        Some(first) => {
                            loops.push(json!({"thread": ti, "from": first, "period": frame - first}));
                            t.pc = None;
                        }
                        None => t.pc = ptr,
                    },
                    "SetParallelScript" => {
                        t.pc = Some(nxt);
                        if threads.len() == 1 {
                            threads.push(Thread::new(ptr));
                        }
                    }
                    "PauseScript" => {
                        t.wait = None;
                        t.pc = Some(nxt);
                        ev = Some(Map::new());
                    }
                    "SetThrow" => {
                        let p = ptr.ok_or("SetThrow without a pointer")?;
                        let d = files.get(p.0)?;
                        let mut m = Map::new();
                        for (k, key) in ["status", "damage", "angle", "knockback_scale", "knockback_weight", "knockback_base", "element"].iter().enumerate() {
                            let o = p.1 + 4 * k;
                            let b = d.data.get(o..o + 4).ok_or("throw outside file")?;
                            m.insert(key.to_string(), json!(i32::from_be_bytes([b[0], b[1], b[2], b[3]])));
                        }
                        ev = Some(m);
                        t.pc = Some(nxt);
                    }
                    "SetDamageThrown" => {
                        ev = Some(Map::new());
                        t.pc = Some(nxt);
                    }
                    _ => {
                        ev = Some(decode(op, &ws, fgm));
                        t.pc = Some(nxt);
                    }
                }
                if let Some(mut e) = ev {
                    e.insert("frame".into(), json!(frame));
                    e.insert("op".into(), json!(op));
                    if ti != 0 {
                        e.insert("thread".into(), json!(ti));
                    }
                    events.push(e);
                }
                if op == "End" && ti == 0 {
                    end = Some(frame);
                }
            }
            ti += 1;
        }
    }
    Ok((events, end, loops))
}

const PARAMS: [&str; 16] = [
    "group", "joint", "damage", "can_rebound", "element", "size", "offset", "angle", "knockback_scale", "knockback_weight", "knockback_base", "hit_air", "hit_ground",
    "shield_damage", "fgm_level", "fgm_kind",
];

fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

/// The attack collisions the events leave on after each frame, as windows of constant values (see the Python).
fn windows(events: &Events, loop_stop: Option<i64>) -> Vec<Map<String, Value>> {
    let mut frames: BTreeMap<i64, Vec<&Map<String, Value>>> = BTreeMap::new();
    for ev in events {
        frames.entry(int(&ev["frame"])).or_default().push(ev);
    }
    let mut state: BTreeMap<i64, Map<String, Value>> = BTreeMap::new();
    let mut live: BTreeMap<i64, Map<String, Value>> = BTreeMap::new();
    let mut out: Vec<Map<String, Value>> = vec![];
    let mut sets = 0i64;
    let last = frames.keys().next_back().map(|f| f + 1).unwrap_or(0);
    for f in 0..last {
        for ev in frames.get(&f).map(|v| v.as_slice()).unwrap_or(&[]) {
            let op = ev["op"].as_str().unwrap_or("");
            match op {
                "MakeAttackColl" | "MakeAttackCollScaled" => {
                    let i = int(&ev["id"]);
                    let mut p = Map::new();
                    for k in PARAMS {
                        p.insert(k.to_string(), ev[k].clone());
                    }
                    p.insert("scaled".into(), json!(op == "MakeAttackCollScaled"));
                    let same = state.get(&i).is_some_and(|c| c["group"] == ev["group"]);
                    let set = if same {
                        state[&i]["set"].clone()
                    } else if let Some(s) = state.iter().find(|(j, s)| **j != i && s["group"] == ev["group"]).map(|(_, s)| s["set"].clone()) {
                        s
                    } else {
                        sets += 1;
                        json!(sets - 1)
                    };
                    p.insert("set".into(), set);
                    state.insert(i, p);
                }
                "SetAttackCollOffset" | "SetAttackCollDamage" | "SetAttackCollSize" | "SetAttackCollSoundLevel" => {
                    if let Some(s) = state.get_mut(&int(&ev["id"])) {
                        for k in ["offset", "damage", "size", "fgm_level"] {
                            if let Some(v) = ev.get(k) {
                                s.insert(k.to_string(), v.clone());
                            }
                        }
                    }
                }
                "RefreshAttackCollID" => {
                    if let Some(s) = state.get_mut(&int(&ev["id"])) {
                        s.insert("set".into(), json!(sets));
                        sets += 1;
                    }
                }
                "ClearAttackCollID" => {
                    state.remove(&int(&ev["id"]));
                }
                "ClearAttackCollAll" => state.clear(),
                _ => {}
            }
        }
        let ids: BTreeSet<i64> = state.keys().chain(live.keys()).copied().collect();
        for i in ids {
            let cur = state.get(&i);
            if let Some(w) = live.get(&i) {
                if cur.is_none_or(|c| c.iter().any(|(k, v)| &w[k] != v)) {
                    let mut w = live.remove(&i).unwrap();
                    w.insert("to".into(), json!(f));
                    out.push(w);
                }
            }
            if let Some(c) = cur {
                if !live.contains_key(&i) {
                    let mut w = c.clone();
                    w.insert("id".into(), json!(i));
                    w.insert("from".into(), json!(f));
                    live.insert(i, w);
                }
            }
        }
    }
    for (_, mut w) in live {
        w.insert("to".into(), json!(loop_stop));
        out.push(w);
    }
    out.sort_by_key(|w| (int(&w["from"]), int(&w["id"])));
    out
}

fn r1(x: f64) -> f64 {
    round_to(x, 1) + 0.0
}

/// Each window's centre per frame in fighter space (see `place` in the Python).
fn place(fighter: &Fighter, tracks: Option<&Vec<Option<Vec<Pose>>>>, length: usize, root: bool, wins: &mut [Map<String, Value>], size: f64) {
    let by_index: HashMap<usize, usize> = fighter.joints.iter().enumerate().map(|(k, j)| (j.index, k)).collect();
    let world = |k: usize, frame: usize| -> M4 {
        let mut chain = vec![];
        let mut c = Some(k);
        while let Some(i) = c {
            chain.push(i);
            c = fighter.joints[i].parent;
        }
        let mut m: Option<M4> = None;
        for &i in chain.iter().rev() {
            let j = &fighter.joints[i];
            let tr = tracks.and_then(|t| t[i].as_ref()).filter(|t| !t.is_empty());
            let (mut t, r, s) = match tr {
                Some(tr) => tr[frame.min(tr.len() - 1)],
                None => (j.t, j.r, j.s),
            };
            if root && j.index == 0 {
                t = [0.0, 0.0, 0.0];
            }
            let local = mat_trs(t, r, s);
            m = Some(match m {
                Some(p) => mat_mul(&p, &local),
                None => local,
            });
        }
        m.unwrap()
    };
    for w in wins.iter_mut() {
        let from = int(&w["from"]);
        let end = match w["to"].as_i64() {
            Some(t) => t,
            None => (length as i64).max(from + 1),
        };
        let raw: Vec<f64> = w["offset"].as_array().map(|a| a.iter().map(|x| int(x) as f64).collect()).unwrap_or_default();
        let off = if w["scaled"] == json!(true) { [raw[0] / size, raw[1] / size, raw[2] / size] } else { [raw[0], raw[1], raw[2]] };
        let joint = int(&w["joint"]);
        let k = if joint >= 4 { by_index.get(&((joint - 4) as usize)).copied() } else { None };
        let mut pos = vec![];
        for frame in from..end {
            let p = match k {
                Some(k) => apply(&world(k, frame as usize), off, 1.0),
                None => off,
            };
            pos.push(json!([r1(p[0] * size), r1(p[1] * size), r1(p[2] * size)]));
        }
        w.insert("positions".into(), Value::Array(pos));
        w.insert("on_joint".into(), json!(k.is_some()));
    }
}

/// `export()` of export-ssb64-motions.py: `motion-events.json` into `out` (the `public/models/ssb64` folder).
pub fn export(source: &Path, z64: &[u8], out: &Path) -> R<Value> {
    if crate::hash::hex(&crate::hash::sha1(z64)) != ssb64::SHA1 {
        return Err("not Super Smash Bros. (US)".into());
    }
    let decomp = ssb64::decomp_dir(source);
    let files = Files::new(z64);
    let names = ssb64::file_names(&decomp)?;
    let cmotions = ssb64::common_motions(&decomp)?;
    let special = *cmotions.iter().find(|(_, v)| *v == "SpecialStart").ok_or("no SpecialStart")?.0;
    let ops = event_names(&decomp)?;
    let fgm = crate::ssb64_sounds::voice_names(&decomp)?;
    let tables = motion_scripts(&decomp, &names)?;
    let ftables = ssb64::fighter_tables(&decomp, &names)?;
    let mut fighters = Map::new();
    for f in FIGHTERS {
        let sc = &tables[f];
        let specials = ssb64::special_motions(&decomp, f)?;
        let table = &ftables[f];
        let fighter = Fighter::new(&files, table)?;
        let mut anims: HashMap<String, (Vec<Option<Vec<Pose>>>, usize)> = HashMap::new();
        let mut motions = Map::new();
        for &(mid, off, flags) in &sc.rows {
            let fid = if flags & SUBMOTION != 0 { sc.sub } else { sc.main }.ok_or_else(|| format!("{f}: no motion file"))?;
            let (events, end, loops) = run_script(&files, (fid, off as usize), &ops, &fgm).map_err(|e| format!("{f} motion {mid}: {e}"))?;
            let m = mid as i64;
            let name = if m < special {
                cmotions.get(&m).cloned().unwrap_or_else(|| format!("Motion{mid}"))
            } else {
                specials.get((m - special) as usize).cloned().unwrap_or_else(|| format!("Special{}", m - special))
            };
            let anim = table.motions.get(mid).cloned().flatten();
            let stop = loops.iter().find(|l| int(&l["thread"]) == 0).map(|l| int(&l["from"]) + int(&l["period"]));
            let mut wins = windows(&events, stop);
            let mut root = false;
            if !wins.is_empty() {
                let mut tracks = None;
                let mut length = 0;
                if let Some(a) = anim.as_ref().filter(|a| names.contains_key(*a)) {
                    if !anims.contains_key(a) {
                        anims.insert(a.clone(), fighter.animation(names[a])?);
                    }
                    let (t, l) = &anims[a];
                    tracks = Some(t);
                    length = *l;
                }
                let k0 = fighter.joints.iter().position(|j| j.index == 0);
                if let (Some(t), Some(k0)) = (tracks, k0) {
                    if t[k0].as_ref().is_some_and(|tr| !tr.is_empty() && tr[0].0[1].abs() < 20.0) {
                        root = true;
                    }
                }
                place(&fighter, tracks, length, root, &mut wins, fighter.size);
            }
            motions.insert(
                mid.to_string(),
                json!({"motion": name, "script": off, "end": end, "loops": loops, "events": events, "hitboxes": wins, "root_motion": root}),
            );
        }
        fighters.insert(f.to_lowercase(), json!({"size": fighter.size, "motions": motions}));
    }
    let index = ssb64::sort_keys(json!({"game": "ssb64", "fps": 60, "fighters": fighters}));
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    std::fs::write(out.join("motion-events.json"), dumps(&index, None) + "\n").map_err(|e| e.to_string())?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attack(id: u32, group: u32, joint: i32, damage: u32, size: u32, angle: i32, kbs: u32, kbw: u32, kbb: u32) -> [u32; 5] {
        [
            (3 << 26) | (id << 23) | (group << 20) | (((joint as u32) & 0x7F) << 13) | (damage << 5) | (1 << 4),
            size << 16,
            0,
            (((angle as u32) & 0x3FF) << 22) | (kbs << 12) | (kbw << 2) | 3,
            kbb << 7,
        ]
    }

    /// `ftMotionCommandMakeAttackColl(0, 0, 10, 2, 1, 0, 160, 0, 0, 0, 361, 50, 0, 3, 0, 0, 0, 8)`: Mario's first jab.
    #[test]
    fn decodes_mario_jab_attack() {
        let ws = attack(0, 0, 10, 2, 160, 361, 50, 0, 8);
        let e = decode("MakeAttackColl", &ws, &[]);
        assert_eq!(e["joint"], 10);
        assert_eq!(e["damage"], 2);
        assert_eq!(e["angle"], 361);
        assert_eq!(e["knockback_scale"], 50);
        assert_eq!(e["knockback_base"], 8);
        assert_eq!(e["size"], 160);
        assert_eq!(e["hit_air"], true);
        assert_eq!(e["hit_ground"], true);
        // Fox's down air: angle 954 in the decomp's unsigned spelling is -70 (s32 angle : 10)
        assert_eq!(decode("MakeAttackColl", &attack(0, 0, 20, 2, 310, -70, 100, 30, 0), &[])["angle"], -70);
    }

    #[test]
    fn windows_follow_create_refresh_and_clear() {
        let mut events: Events = vec![];
        let mut push = |frame: i64, op: &str, extra: Value| {
            let mut m = extra.as_object().cloned().unwrap_or_default();
            m.insert("frame".into(), json!(frame));
            m.insert("op".into(), json!(op));
            events.push(m);
        };
        let hit = decode("MakeAttackColl", &attack(0, 0, 10, 2, 160, 361, 50, 0, 8), &[]);
        let hit1 = decode("MakeAttackColl", &attack(1, 0, 9, 2, 160, 361, 50, 0, 8), &[]);
        push(2, "MakeAttackColl", Value::Object(hit.clone()));
        push(2, "MakeAttackColl", Value::Object(hit1));
        push(3, "RefreshAttackCollID", json!({"id": 0}));
        push(4, "ClearAttackCollAll", json!({}));
        let w = windows(&events, None);
        let span = |w: &Map<String, Value>| (int(&w["id"]), int(&w["from"]), int(&w["to"]), int(&w["set"]));
        assert_eq!(w.iter().map(span).collect::<Vec<_>>(), vec![(0, 2, 3, 0), (1, 2, 4, 0), (0, 3, 4, 1)]);
    }
}
