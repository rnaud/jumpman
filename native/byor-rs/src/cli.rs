//! Command line / in-process entry point with the same arguments and `@@` output protocol as `byor/pipeline.py`, so the
//! game's setup screen (`src/byor.rs`) drives either implementation the same way:
//!
//!   byor-rs --source-root DIR --data-dir DIR [--sm64 ROM] [--mk64 ROM] [--oot ROM] [--bk ROM]
//!   byor-rs --source-root . --data-dir . --identify ROM
//!   byor-rs --source-root DIR --data-dir DIR --status
//!   byor-rs --extract-sm64-assets ROM --out DIR [--source-root DIR]     (developer / test command)
//!
//! `@@PROGRESS <game> <percent> <message>`, `@@DONE <game>`, `@@FAIL <game> <message>`, `@@STATUS <game> <state> [rom]`.
use crate::{pipeline, rom, sm64};
use std::io::Write;
use std::path::PathBuf;

fn arg_value(it: &mut std::vec::IntoIter<String>, flag: &str) -> Result<String, String> {
    it.next().ok_or_else(|| format!("{flag} needs a value"))
}

/// Runs the tool; returns the exit status (0 ok, 1 a game failed, 2 usage / nothing to do, 3 unknown ROM).
pub fn run(args: Vec<String>, out: &mut dyn Write) -> i32 {
    match run_inner(args, out) {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(out, "@@FAIL all {e}");
            2
        }
    }
}

fn run_inner(args: Vec<String>, out: &mut dyn Write) -> Result<i32, String> {
    let mut it = args.into_iter();
    let (mut source, mut data) = (PathBuf::from("."), PathBuf::from("."));
    let (mut identify, mut status, mut hashes) = (None, false, false);
    let mut roms: Vec<(String, PathBuf)> = Vec::new();
    let (mut extract_sm64, mut out_dir) = (None, None);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--source-root" => source = arg_value(&mut it, &a)?.into(),
            "--data-dir" => data = arg_value(&mut it, &a)?.into(),
            "--identify" => identify = Some(PathBuf::from(arg_value(&mut it, &a)?)),
            "--status" => status = true,
            "--print-hashes" => hashes = true,
            "--extract-sm64-assets" => extract_sm64 = Some(PathBuf::from(arg_value(&mut it, &a)?)),
            "--out" => out_dir = Some(PathBuf::from(arg_value(&mut it, &a)?)),
            g if g.starts_with("--") && rom::ROMS.iter().any(|r| r.id == &g[2..]) => {
                let p = PathBuf::from(arg_value(&mut it, &a)?);
                roms.push((g[2..].to_string(), p));
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let say = |out: &mut dyn Write, s: &str| {
        let _ = writeln!(out, "{s}");
        let _ = out.flush();
    };
    if let Some(p) = identify {
        return match rom::identify(&p) {
            Ok(Some((g, _))) => {
                say(out, g);
                Ok(0)
            }
            Ok(None) => {
                say(out, "unknown");
                Ok(3)
            }
            Err(e) => {
                say(out, &format!("{}: {e}", p.display()));
                Ok(3)
            }
        };
    }
    if let Some(romp) = extract_sm64 {
        let dest = out_dir.ok_or("--extract-sm64-assets needs --out DIR")?;
        let (g, z64) = rom::identify(&romp).map_err(|e| e.to_string())?.ok_or("not a recognised ROM")?;
        if g != "sm64" {
            return Err(format!("{} is {g}, not sm64", romp.display()));
        }
        let json = std::fs::read(pipeline::sm64_assets_json(&source)).map_err(|e| format!("assets.json: {e}"))?;
        let stats = sm64::extract(&z64, &json, &dest, |d, t| {
            let _ = writeln!(out, "@@PROGRESS sm64 {} extracting textures", d * 100 / t.max(1));
        })?;
        say(out, &format!("{stats:?}"));
        return Ok(0);
    }
    if hashes {
        say(out, &pipeline::print_hashes());
        return Ok(0);
    }
    if status {
        for line in pipeline::status_lines(&data) {
            say(out, &line);
        }
        return Ok(0);
    }
    if roms.is_empty() {
        say(out, "@@FAIL all no valid ROMs given");
        return Ok(2);
    }
    Ok(pipeline::run(&source, &data, &roms, out))
}
