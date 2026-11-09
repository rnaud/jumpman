//! `assets/audio/music/sounds.json` and `courses.json`: ports of `sound_identifiers` (which compiled the decomp's
//! `include/sounds.h` with `cc` and printed every SOUND_* value) and `course_settings` of `scripts/render-music.py`.
//! Both only read decomp source text.
use std::collections::HashMap;
use std::path::Path;

// ---- tiny C expression evaluator for sounds.h ----

struct Eval<'a> {
    t: Vec<String>,
    i: usize,
    defs: &'a HashMap<String, String>,
    depth: u32,
}

fn tokens(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_alphanumeric() || c == '_' {
            let st = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(s[st..i].to_string());
        } else if c == '<' && s[i..].starts_with("<<") {
            out.push("<<".into());
            i += 2;
        } else {
            out.push(c.to_string());
            i += 1;
        }
    }
    out
}

impl Eval<'_> {
    fn peek(&self) -> Option<&str> {
        self.t.get(self.i).map(String::as_str)
    }
    fn or(&mut self) -> Result<u64, String> {
        let mut v = self.shift()?;
        while self.peek() == Some("|") {
            self.i += 1;
            v |= self.shift()?;
        }
        Ok(v)
    }
    fn shift(&mut self) -> Result<u64, String> {
        let mut v = self.add()?;
        while self.peek() == Some("<<") {
            self.i += 1;
            v <<= self.add()?;
        }
        Ok(v)
    }
    fn add(&mut self) -> Result<u64, String> {
        let mut v = self.atom()?;
        while self.peek() == Some("+") {
            self.i += 1;
            v += self.atom()?;
        }
        Ok(v)
    }
    fn atom(&mut self) -> Result<u64, String> {
        let t = self.peek().ok_or("unexpected end of expression")?.to_string();
        self.i += 1;
        if t == "(" {
            // `(u32)` style cast: a type name followed by `)`
            if matches!(self.peek(), Some("u32" | "s32" | "u16" | "u8")) && self.t.get(self.i + 1).map(String::as_str) == Some(")") {
                self.i += 2;
                return self.atom();
            }
            let v = self.or()?;
            if self.peek() != Some(")") {
                return Err("missing )".into());
            }
            self.i += 1;
            return Ok(v);
        }
        if let Some(h) = t.strip_prefix("0x") {
            return u64::from_str_radix(h, 16).map_err(|e| e.to_string());
        }
        if t.as_bytes()[0].is_ascii_digit() {
            return t.parse().map_err(|_| format!("bad number {t}"));
        }
        let body = self.defs.get(&t).ok_or_else(|| format!("undefined {t}"))?.clone();
        if self.depth > 20 {
            return Err("macro recursion".into());
        }
        let mut sub = Eval { t: tokens(&body), i: 0, defs: self.defs, depth: self.depth + 1 };
        sub.or()
    }
}

fn eval(s: &str, defs: &HashMap<String, String>) -> Result<u64, String> {
    let mut e = Eval { t: tokens(s), i: 0, defs, depth: 0 };
    e.or()
}

fn strip_comments(line: &str) -> String {
    let mut s = line.to_string();
    while let (Some(a), Some(b)) = (s.find("/*"), s.find("*/")) {
        if a > b {
            break;
        }
        s.replace_range(a..b + 2, " ");
    }
    if let Some(p) = s.find("//") {
        s.truncate(p);
    }
    s
}

/// Every `SOUND_*` defined through `SOUND_ARG_LOAD`, in file order, with its US-build value, as the JSON the Python wrote.
pub fn sounds_json(decomp: &Path) -> Result<String, String> {
    let header = std::fs::read_to_string(decomp.join("include/sounds.h")).map_err(|e| format!("sounds.h: {e}"))?;
    let mut names: Vec<String> = Vec::new();
    let mut defs: HashMap<String, String> = HashMap::new();
    let mut loads: HashMap<String, String> = HashMap::new();
    // preprocessor state: (this branch active, some earlier branch of the group was taken, parent active)
    let mut stack: Vec<(bool, bool, bool)> = Vec::new();
    let defined = |n: &str| n == "VERSION_US";
    let cond = |d: &str| -> bool {
        let d = d.trim();
        if let Some(x) = d.strip_prefix("defined(") { defined(x.trim_end_matches(')').trim()) } else { defined(d) }
    };
    for raw in header.lines() {
        let line = raw.trim_start();
        let active = stack.last().is_none_or(|s| s.0);
        if let Some(d) = line.strip_prefix('#') {
            let d = d.trim_start();
            let (kw, rest) = d.split_once(|c: char| c.is_whitespace()).unwrap_or((d, ""));
            match kw {
                "ifdef" | "ifndef" | "if" => {
                    let mut c = if kw == "ifndef" { !defined(rest.trim()) } else { cond(rest) };
                    if kw == "if" && !rest.contains("defined") {
                        c = rest.trim() != "0";
                    }
                    stack.push((active && c, c, active));
                }
                "elif" => {
                    let top = stack.last_mut().ok_or("stray #elif")?;
                    let c = cond(rest);
                    top.0 = top.2 && !top.1 && c;
                    top.1 |= c;
                }
                "else" => {
                    let top = stack.last_mut().ok_or("stray #else")?;
                    top.0 = top.2 && !top.1;
                    top.1 = true;
                }
                "endif" => {
                    stack.pop();
                }
                "define" => {
                    let (name, body) = rest.trim_start().split_once(|c: char| c.is_whitespace() || c == '(').map_or((rest.trim(), ""), |(n, b)| (n, b));
                    if name.is_empty() || rest.trim_start()[name.len()..].starts_with('(') {
                        continue;
                    }
                    if let Some(n) = name.strip_prefix("SOUND_") {
                        // names come from every definition (the Python regex ignored #if), values from active ones
                        let after_comment = body.trim_start().strip_prefix("/*").and_then(|b| b.split_once("*/")).map(|x| x.1.trim_start());
                        if let Some(a) = after_comment.filter(|a| a.starts_with("SOUND_ARG_LOAD(")) {
                            if !names.iter().any(|x| x == n) {
                                names.push(n.to_string());
                            }
                            if active {
                                loads.insert(n.to_string(), a.to_string());
                            }
                            continue;
                        }
                    }
                    if active {
                        defs.insert(name.to_string(), strip_comments(body).trim().to_string());
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = String::from("{\n");
    for (k, n) in names.iter().enumerate() {
        let call = loads.get(n).ok_or_else(|| format!("SOUND_{n} has no US definition"))?;
        let inner = &call["SOUND_ARG_LOAD(".len()..];
        // arguments up to the matching close paren, split on top-level commas
        let (mut depth, mut args, mut cur) = (0i32, Vec::new(), String::new());
        for ch in strip_comments(inner).chars() {
            match ch {
                '(' => {
                    depth += 1;
                    cur.push(ch);
                }
                ')' if depth == 0 => break,
                ')' => {
                    depth -= 1;
                    cur.push(ch);
                }
                ',' if depth == 0 => args.push(std::mem::take(&mut cur)),
                _ => cur.push(ch),
            }
        }
        args.push(cur);
        if args.len() != 4 {
            return Err(format!("SOUND_{n}: unexpected arguments"));
        }
        let v: Vec<u64> = args.iter().map(|a| eval(a, &defs)).collect::<Result<_, _>>().map_err(|e| format!("SOUND_{n}: {e}"))?;
        let value = ((v[0] << 28) | (v[1] << 16) | (v[2] << 8) | v[3] | 1) as u32;
        out += &format!("  \"{n}\": {value}{}\n", if k + 1 < names.len() { "," } else { "" });
    }
    out += "}\n";
    Ok(out)
}

// ---- courses.json ----

fn is_w(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn take_w(s: &str) -> (&str, &str) {
    let n = s.find(|c: char| !is_w(c)).unwrap_or(s.len());
    s.split_at(n)
}

fn after_ws(s: &str) -> &str {
    s.trim_start()
}

pub fn courses_json(decomp: &Path) -> Result<String, String> {
    let rd = |p: &str| std::fs::read_to_string(decomp.join(p)).map_err(|e| format!("{p}: {e}"));
    // `(SEQ_\w+),\s*// (0x\w+)`
    let seq_ids = rd("include/seq_ids.h")?;
    let mut ids: HashMap<String, u64> = HashMap::new();
    let mut rest = seq_ids.as_str();
    while let Some(p) = rest.find("SEQ_") {
        let (name, after) = take_w(&rest[p..]);
        rest = &rest[p + name.len().max(1)..];
        if let Some(a) = after.strip_prefix(',') {
            if let Some(c) = after_ws(a).strip_prefix("// 0x") {
                let (hex, _) = take_w(c);
                if let Ok(v) = u64::from_str_radix(hex, 16) {
                    ids.insert(name.to_string(), v);
                }
            }
        }
    }
    let defs = rd("levels/level_defines.h")?;
    let terrain_ids = ["GRASS", "STONE", "SNOW", "SAND", "SPOOKY", "WATER", "SLIDE"];
    let mut out = String::from("{");
    let mut first = true;
    let levels: Vec<&str> = defs.lines().filter(|l| l.starts_with("STUB_LEVEL(") || l.starts_with("DEFINE_LEVEL(")).collect();
    for (n, line) in levels.iter().enumerate() {
        if line.starts_with("STUB_LEVEL") {
            continue;
        }
        let name = line.split(',').nth(3).ok_or("bad DEFINE_LEVEL")?.trim();
        let script = rd(&format!("levels/{name}/script.c"))?;
        let mut terrains = Vec::new();
        let key = "TERRAIN_TYPE(/*terrainType*/";
        let mut s = script.as_str();
        while let Some(p) = s.find(key) {
            s = &s[p + key.len()..];
            if let Some(t) = after_ws(s).strip_prefix("TERRAIN_") {
                let (w, r) = take_w(t);
                if r.starts_with(')') && !w.is_empty() {
                    terrains.push(w.to_string());
                }
            }
        }
        let mut areas = Vec::new();
        let key = "SET_BACKGROUND_MUSIC(/*settingsPreset*/";
        let mut s = script.as_str();
        while let Some(p) = s.find(key) {
            s = &s[p + key.len()..];
            let t = after_ws(s);
            let Some(h) = t.strip_prefix("0x") else { continue };
            let (hex, r) = take_w(h);
            let Some(r) = r.strip_prefix(',') else { continue };
            let Some(r) = after_ws(r).strip_prefix("/*seq*/") else { continue };
            let r = after_ws(r);
            let r = r.strip_prefix('(').unwrap_or(r);
            if !r.starts_with("SEQ_") {
                continue;
            }
            let (seq, r) = take_w(r);
            let variation = after_ws(r).strip_prefix('|').is_some_and(|v| after_ws(v).starts_with("SEQ_VARIATION"));
            let label = seq.strip_prefix("SEQ_LEVEL_").unwrap_or(seq).to_lowercase();
            let label = match label.as_str() {
                "inside_castle" => "inside".to_string(),
                "seq_sound_player" => "ambient".to_string(),
                _ => label,
            };
            let terrain = terrain_ids.iter().position(|t| Some(*t) == terrains.get(areas.len()).map(String::as_str)).ok_or_else(|| format!("{name}: terrain for area {}", areas.len()))?;
            let sequence = ids.get(seq).ok_or_else(|| format!("unknown {seq}"))? | if variation { 128 } else { 0 };
            let preset = u64::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
            areas.push(format!("{{\"terrain\":{terrain},\"name\":\"{label}\",\"sequence\":{sequence},\"preset\":{preset}}}"));
        }
        if !areas.is_empty() {
            if !first {
                out.push(',');
            }
            first = false;
            out += &format!("\"{name}\":{{\"level\":{},\"areas\":[{}]}}", n + 1, areas.join(","));
        }
    }
    out += "}\n";
    Ok(out)
}
