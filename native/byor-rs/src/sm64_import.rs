//! The complete Super Mario 64 import in Rust: `do_sm64` of `byor/pipeline.py` with every exporter ported.
//! Builds `<stage>/assets` and `<stage>/public` (the same file tree the Python pipeline stages); `pipeline::run` installs it.
use crate::courses::Courses;
use crate::pyc::{read, R};
use crate::{courses_post, models, music, rom, sm64};
use std::path::{Path, PathBuf};

/// Where the decomp checkout lives inside a repo checkout or a BYOR bundle (`byor-src`).
pub fn decomp_dir(source: &Path) -> PathBuf {
    crate::pipeline::sm64_assets_json(source).parent().unwrap().to_path_buf()
}

fn first_file(source: &Path, rels: &[&str]) -> R<PathBuf> {
    rels.iter().map(|r| source.join(r)).find(|p| p.is_file()).ok_or_else(|| format!("{} not found under {}", rels[0], source.display()))
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
    } else if src.is_file() {
        std::fs::create_dir_all(dst.parent().unwrap())?;
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// `copy_original` of pipeline.py: the non-Nintendo assets every data dir carries.
pub fn copy_original(source: &Path, stage: &Path) -> R<()> {
    let list = ["assets/ui", "assets/audio/kart-voices.json", "assets/audio/character-sounds.json", "public/models/quaternius", "public/manifest.webmanifest"];
    for rel in list {
        copy_tree(&source.join(rel), &stage.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
    }
    for (dir, prefix, suffix) in [("public/mk64", "item-", ""), ("public", "icon-", ""), ("public", "favicon.", "")] {
        if let Ok(rd) = std::fs::read_dir(source.join(dir)) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.starts_with(prefix) && n.ends_with(suffix) && (prefix != "item-" || n.ends_with(".png")) && e.path().is_file() {
                    copy_tree(&e.path(), &stage.join(dir).join(&n)).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}

pub fn run(source: &Path, stage: &Path, z64: &[u8], progress: &mut dyn FnMut(u32, &str)) -> R<()> {
    let decomp = decomp_dir(source);
    let assets_json = std::fs::read(decomp.join("assets.json")).map_err(|e| format!("assets.json: {e}"))?;
    let ext = stage.join("ext");
    let out_assets = stage.join("assets");
    std::fs::create_dir_all(out_assets.join("courses")).map_err(|e| e.to_string())?;
    progress(2, "Extracting textures, sequences and samples");
    let stats = sm64::extract(z64, &assets_json, &ext, |d, t| progress(2 + (d * 36 / t.max(1)) as u32, "Extracting the game's textures and sounds"))?;
    if stats.textures == 0 {
        return Err("no textures extracted (is this the US ROM?)".into());
    }
    let courses = Courses::new(&decomp, &out_assets)?;
    progress(42, "Exporting courses");
    courses.export(&out_assets.join("courses"))?;
    progress(50, "Placing SM64 enemies, platforms, bosses and castle NPCs");
    courses_post::run_all(&courses, &out_assets.join("courses"))?;
    progress(50, "Exporting models and animations");
    let mut n = 0;
    models::export_all(&decomp, &ext, stage, &assets_json, &mut |_| {
        n += 1;
        if n % 100 == 0 {
            progress((50 + n / 60).min(90) as u32, "Exporting models and animations");
        }
    })?;
    progress(91, "Finalising courses");
    courses.export(&out_assets.join("courses"))?;
    courses_post::run_all(&courses, &out_assets.join("courses"))?;
    progress(92, "Converting audio samples");
    sm64::export_audio(&ext.join("sound/samples"), &out_assets.join("audio"))?;
    progress(96, "Building the music engine");
    let music_dir = out_assets.join("audio/music");
    std::fs::create_dir_all(&music_dir).map_err(|e| e.to_string())?;
    std::fs::write(music_dir.join("sounds.json"), music::sounds_json(&decomp)?).map_err(|e| e.to_string())?;
    let template = first_file(source, &["scripts/music/sm64-us-template.wasm"])?;
    let layout = first_file(source, &["scripts/music/sm64-us-template.json"])?;
    let player = first_file(source, &["native/byor/sm64-us-sound-player.bin", "byor/sm64-us-sound-player.bin"])?;
    let wasm = sm64::music_wasm(z64, &assets_json, &ext, &decomp, &std::fs::read(template).map_err(|e| e.to_string())?, &read(&layout)?, &std::fs::read(player).map_err(|e| e.to_string())?)?;
    std::fs::write(music_dir.join("sm64-us.wasm"), wasm).map_err(|e| e.to_string())?;
    std::fs::write(music_dir.join("courses.json"), music::courses_json(&decomp)?).map_err(|e| e.to_string())?;
    // the stage's extraction tree is only needed during the import
    let _ = std::fs::remove_dir_all(&ext);
    let _ = rom::ROMS.len();
    Ok(())
}
