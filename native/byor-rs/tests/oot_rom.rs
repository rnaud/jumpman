//! OoT importer against a `pipeline.py --oot` data dir. Needs BYOR_TEST_OOT_ROM (US 1.2 ROM), BYOR_TEST_OOT_SRC (a source
//! root with reference/oot, as `pipeline.py --source-root` takes) and BYOR_TEST_DATA_REF (the Python pipeline's data dir);
//! skipped when unset. BYOR_OOT_STAGES=voices,sfx,... limits the stages; BYOR_TEST_PREFIX=assets/audio limits the comparison.
use std::path::{Path, PathBuf};

fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, out);
        } else {
            out.push(p.strip_prefix(base).unwrap().to_path_buf());
        }
    }
}

#[test]
fn oot_matches_the_python_pipeline() {
    let (Some(rom), Some(src), Some(data)) = (std::env::var_os("BYOR_TEST_OOT_ROM"), std::env::var_os("BYOR_TEST_OOT_SRC"), std::env::var_os("BYOR_TEST_DATA_REF")) else {
        eprintln!("skipped");
        return;
    };
    let (src, data) = (PathBuf::from(src), PathBuf::from(data));
    let prefix = std::env::var("BYOR_TEST_PREFIX").unwrap_or_default();
    let out = std::env::var_os("BYOR_TEST_OOT_OUT").map_or_else(|| std::env::temp_dir().join(format!("byor-rs-oot-{}", std::process::id())), PathBuf::from);
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let (_, z64) = byor_rs::rom::identify(Path::new(&rom)).unwrap().expect("not a recognised ROM");
    let t = std::time::Instant::now();
    byor_rs::oot::run(&src, &out, &z64, &mut |p, m| eprintln!("[{p}%] {m}")).unwrap();
    eprintln!("import took {:?}", t.elapsed());
    let mut files = Vec::new();
    for top in ["assets", "public"] {
        walk(&out.join(top), &out, &mut files);
    }
    let mut bad = Vec::new();
    let mut checked = 0;
    for rel in files.iter().filter(|r| r.to_string_lossy().starts_with(&prefix)) {
        checked += 1;
        match std::fs::read(data.join(rel)) {
            Ok(w) if w == std::fs::read(out.join(rel)).unwrap() => {}
            Ok(_) => bad.push(format!("DIFF {}", rel.display())),
            Err(_) => bad.push(format!("EXTRA {}", rel.display())),
        }
    }
    let mut want = Vec::new();
    for top in ["assets", "public"] {
        walk(&data.join(top), &data, &mut want);
    }
    let stages = std::env::var("BYOR_OOT_STAGES").unwrap_or_default();
    let missing: Vec<_> = want.iter().filter(|r| r.to_string_lossy().starts_with(&prefix) && !out.join(r).exists() && !(r.starts_with("assets/ui") || r.starts_with("public/models/quaternius") || r.starts_with("assets/audio/character-sounds") || r.starts_with("assets/audio/kart-voices") || r.starts_with("stamps"))).collect();
    eprintln!("stages {stages:?}: {checked} files checked, {} differ/extra, {} missing", bad.len(), missing.len());
    for b in bad.iter().take(30) {
        eprintln!("  {b}");
    }
    for m in missing.iter().take(30) {
        eprintln!("  MISSING {}", m.display());
    }
    if !stages.is_empty() {
        // Partial run: only differences matter.
        assert!(bad.is_empty(), "{} files differ", bad.len());
    } else {
        assert!(bad.is_empty() && missing.is_empty(), "{} differ, {} missing", bad.len(), missing.len());
    }
}
