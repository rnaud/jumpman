//! Audit tool: lists every behaviour the SM64 decomp places (script.c `OBJECT` / `OBJECT_WITH_ACTS`, area `macro.inc.c`
//! `MACRO_OBJECT*` through `macro_presets.inc.c`, and behaviours spawned by other behaviours' scripts in
//! `data/behavior_data.c`) and diffs it against what the Rust side handles.
//!
//! "Handled" is a heuristic: the behaviour name appears literally in `native/byor-rs/src`, `native/sim/src` or `scripts/export-*.py`,
//! or its name (without `bhv`, lower case) matches a sim `ObjectBehaviorKind` string. Review the top of the list by hand.
//!
//! Usage: `cargo run -p byor-rs --example audit_sm64_objects -- <repo root> > docs/sm64-object-audit.md`
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

fn strip_comments(t: &str) -> String {
    Regex::new(r"(?s)/\*.*?\*/|//[^\n]*").unwrap().replace_all(t, "").into_owned()
}

fn calls(text: &str, cmd: &str) -> Vec<String> {
    let re = Regex::new(&format!(r"\b{cmd}\s*\(([^()]*(?:\([^()]*\)[^()]*)*)\)")).unwrap();
    re.captures_iter(text).map(|c| c[1].to_string()).collect()
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("repo root"));
    let sm64 = std::env::args().nth(2).map_or_else(|| root.join("reference/sm64"), PathBuf::from);
    // Presets: macro name -> behaviour.
    let presets_src = fs::read_to_string(sm64.join("include/macro_presets.inc.c")).unwrap();
    let preset_re = Regex::new(r"/\*\s*(macro_\w+)\s*\*/\s*\{\s*(\w+)").unwrap();
    let presets: BTreeMap<String, String> = preset_re.captures_iter(&presets_src).map(|c| (c[1].to_string(), c[2].to_string())).collect();
    // placements[bhv][level] = count
    let mut placed: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut add = |b: &str, level: &str, n: usize| *placed.entry(b.to_string()).or_default().entry(level.to_string()).or_default() += n;
    let mut levels: Vec<String> = fs::read_dir(sm64.join("levels")).unwrap().flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    levels.sort();
    for lv in &levels {
        let script = sm64.join("levels").join(lv).join("script.c");
        if let Ok(t) = fs::read_to_string(&script) {
            let t = strip_comments(&t);
            for cmd in ["OBJECT", "OBJECT_WITH_ACTS"] {
                for a in calls(&t, cmd) {
                    if let Some(b) = a.split(',').map(str::trim).find(|s| s.starts_with("bhv")) {
                        add(b, lv, 1);
                    }
                }
            }
        }
        let mut macros = Vec::new();
        walk(&sm64.join("levels").join(lv).join("areas"), "c", &mut macros);
        for m in macros.iter().filter(|m| m.file_name().is_some_and(|n| n == "macro.inc.c")) {
            let t = strip_comments(&fs::read_to_string(m).unwrap());
            for cmd in ["MACRO_OBJECT", "MACRO_OBJECT_WITH_BHV_PARAM"] {
                for a in calls(&t, cmd) {
                    let name = a.split(',').next().unwrap().trim().to_string();
                    match presets.get(&name) {
                        Some(b) => add(b, lv, 1),
                        None => add(&format!("?{name}"), lv, 1),
                    }
                }
            }
        }
    }
    // Spawned by behaviour scripts: parent block -> children.
    let bd = strip_comments(&fs::read_to_string(sm64.join("data/behavior_data.c")).unwrap());
    let block_re = Regex::new(r"const BehaviorScript (bhv\w+)\[\]\s*=\s*\{(.*?)\n\};").unwrap();
    let spawn_re = Regex::new(r"(?:SPAWN_CHILD|SPAWN_OBJ|SPAWN_CHILD_WITH_PARAM)\s*\([^()]*?,\s*(bhv\w+)\s*\)").unwrap();
    let mut children: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in block_re.captures_iter(&bd) {
        for s in spawn_re.captures_iter(&c[2]) {
            children.entry(c[1].to_string()).or_default().insert(s[1].to_string());
        }
    }
    let direct: BTreeMap<String, usize> = placed.iter().map(|(b, l)| (b.clone(), l.values().sum())).collect();
    let mut total = direct.clone();
    let mut via: BTreeMap<String, String> = BTreeMap::new();
    for (p, kids) in &children {
        if let Some(n) = direct.get(p) {
            for k in kids {
                *total.entry(k.clone()).or_default() += n;
                via.entry(k.clone()).or_insert_with(|| p.clone());
            }
        }
    }
    // Spawned from C code (parent not resolved): bhvX references in src/game outside behaviors tables.
    let mut cfiles = Vec::new();
    walk(&sm64.join("src/game"), "c", &mut cfiles);
    let mut code_spawned: BTreeMap<String, String> = BTreeMap::new();
    let code_re = Regex::new(r"spawn_\w+\s*\([^;]*?\b(bhv\w+)\b").unwrap();
    for f in &cfiles {
        let t = strip_comments(&fs::read_to_string(f).unwrap());
        for c in code_re.captures_iter(&t) {
            code_spawned.entry(c[1].to_string()).or_insert_with(|| f.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    // Handled heuristics.
    let mut haystack = String::new();
    let mut files = Vec::new();
    walk(&root.join("native/byor-rs/src"), "rs", &mut files);
    walk(&root.join("native/sim/src"), "rs", &mut files);
    walk(&root.join("native/src"), "rs", &mut files);
    walk(&root.join("scripts"), "py", &mut files);
    for f in &files {
        if let Ok(t) = fs::read_to_string(f) {
            haystack.push_str(&t);
        }
    }
    let kinds_src = fs::read_to_string(root.join("native/sim/src/object.rs")).unwrap();
    let kind_names: BTreeSet<String> = Regex::new(r#"=>\s*"(\w+)""#).unwrap().captures_iter(&kinds_src).map(|c| c[1].to_lowercase()).collect();
    let handled = |b: &str| -> &'static str {
        let tail = b.trim_start_matches("bhv").to_lowercase();
        let literal = Regex::new(&format!(r"\b{}\b", regex::escape(b))).unwrap().is_match(&haystack);
        match (literal, kind_names.contains(&tail)) {
            (true, true) => "exporter+kind",
            (true, false) => "exporter",
            (false, true) => "kind",
            _ => "NO",
        }
    };
    let mut rows: Vec<(&String, usize)> = total.iter().map(|(b, n)| (b, *n)).collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    println!("# SM64 placed-object audit\n");
    println!("Generated by `native/byor-rs/examples/audit_sm64_objects.rs` from `reference/sm64`. Counts are placements in the level scripts (`OBJECT`, `OBJECT_WITH_ACTS`), area macro objects (resolved through `macro_presets.inc.c`) and children spawned by a placed behaviour's script (`SPAWN_CHILD` / `SPAWN_OBJ`, one per parent). `handled` is a heuristic: `exporter` = the name appears in the byor-rs / sim / app source or an export script, `kind` = a sim `ObjectBehaviorKind` string matches the name. Names starting with `?` are macro presets the table could not resolve.\n");
    let unhandled: Vec<_> = rows.iter().filter(|(b, _)| handled(b) == "NO").collect();
    println!("{} distinct behaviours placed, {} without any handling reference ({} placements).\n", rows.len(), unhandled.len(), unhandled.iter().map(|(_, n)| n).sum::<usize>());
    println!("## Unhandled, ranked by placements\n\n| behaviour | placements | levels | spawned by |\n|---|---|---|---|");
    for (b, n) in &unhandled {
        let lv = placed.get(*b).map(|l| l.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")).unwrap_or_default();
        println!("| {b} | {n} | {lv} | {} |", via.get(*b).cloned().unwrap_or_default());
    }
    println!("\n## Handled (any reference), ranked\n\n| behaviour | placements | handled by |\n|---|---|---|");
    for (b, n) in rows.iter().filter(|(b, _)| handled(b) != "NO") {
        println!("| {b} | {n} | {} |", handled(b));
    }
    println!("\n## Spawned from C code only (parent not resolved), no handling reference\n");
    for (b, f) in &code_spawned {
        if !total.contains_key(b) && handled(b) == "NO" {
            println!("- {b} (`{f}`)");
        }
    }
}
