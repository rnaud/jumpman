//! The Super Smash Bros. weapon / effect / item export (native/SSB64.md "Weapons, effects and items"). Skipped unless
//!   BYOR_TEST_SSB64_ROM = a Super Smash Bros. (US) ROM in any byte order, and (for the parity test)
//!   BYOR_TEST_SSB64_WEAPONS_REF = a folder written by
//!     `python3 scripts/export-ssb64-weapons.py --out <dir>/models/ssb64 --sound-out <dir>/sounds`.
//! No ROM or extracted data is committed.
use std::path::{Path, PathBuf};

fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rom() -> Option<Vec<u8>> {
    let path = std::env::var_os("BYOR_TEST_SSB64_ROM")?;
    let (game, z64) = byor_rs::rom::identify(Path::new(&path)).unwrap().expect("a known ROM");
    assert_eq!(game, "ssb64");
    Some(z64)
}

fn export(z64: &[u8], tag: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("byor-ssb64-weapons-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    byor_rs::ssb64::weapons::export(&source(), z64, &out, &out.join("sounds")).unwrap();
    out
}

#[test]
fn ssb64_weapons_items_and_effects() {
    let Some(z64) = rom() else {
        eprintln!("skipped: BYOR_TEST_SSB64_ROM not set");
        return;
    };
    let out = export(&z64, "index");
    let index: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join("models/ssb64/objects.json")).unwrap()).unwrap();
    let objects = index["objects"].as_object().unwrap();
    assert_eq!(objects.len(), byor_rs::ssb64::weapons::OBJECTS.len());
    for (name, o) in objects {
        let glb = std::fs::read(out.join("models/ssb64").join(o["file"].as_str().unwrap())).unwrap();
        assert_eq!(&glb[..4], b"glTF", "{name}");
        assert!(o["triangles"].as_u64().unwrap() >= 1, "{name} has no triangles");
    }
    for item in ["beam_sword", "bat", "fan", "star_rod", "ray_gun", "fire_flower", "hammer", "maxim_tomato", "heart", "star", "bob_omb", "green_shell", "red_shell", "motion_bomb", "bumper", "poke_ball", "capsule", "crate", "barrel"] {
        assert_eq!(objects[item]["kind"], "item", "{item}");
        assert!(objects[item]["materials"].as_u64().unwrap() >= 1, "{item}");
    }
    let sounds: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join("sounds/manifest-ssb64-items.json")).unwrap()).unwrap();
    assert_eq!(sounds["clips"].as_object().unwrap().len(), byor_rs::ssb64::weapons::ITEM_FGM.len());
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn ssb64_weapons_match_python() {
    let (Some(z64), Some(reference)) = (rom(), std::env::var_os("BYOR_TEST_SSB64_WEAPONS_REF")) else {
        eprintln!("skipped: BYOR_TEST_SSB64_ROM / BYOR_TEST_SSB64_WEAPONS_REF not set");
        return;
    };
    let reference = PathBuf::from(reference);
    let out = export(&z64, "parity");
    let mut files = vec![PathBuf::from("models/ssb64/objects.json"), PathBuf::from("sounds/manifest-ssb64-items.json")];
    for (name, ..) in byor_rs::ssb64::weapons::OBJECTS {
        files.push(PathBuf::from(format!("models/ssb64/objects/{name}.glb")));
    }
    for name in byor_rs::ssb64::weapons::ITEM_FGM {
        files.push(PathBuf::from(format!("sounds/{name}_0.wav")));
    }
    for f in &files {
        let a = std::fs::read(out.join(f)).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let b = std::fs::read(reference.join(f)).unwrap_or_else(|e| panic!("reference {}: {e}", f.display()));
        assert!(a == b, "{} differs from the Python export", f.display());
    }
    eprintln!("{} files byte-identical", files.len());
    let _ = std::fs::remove_dir_all(&out);
}
