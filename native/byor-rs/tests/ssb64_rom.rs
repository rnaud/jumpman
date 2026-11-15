//! The Super Smash Bros. import (native/SSB64.md). The table test needs only the `reference/ssb64` submodule; the ROM tests
//! are skipped unless
//!   BYOR_TEST_SSB64_ROM = a Super Smash Bros. (US) ROM in any byte order, and (for the parity test)
//!   BYOR_TEST_SSB64_REF = a data dir written by `python3 scripts/export-ssb64.py --out <dir>/public/models/ssb64`.
//! No ROM or extracted data is committed.
use std::path::{Path, PathBuf};

fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn ssb64_decomp_tables() {
    let decomp = byor_rs::ssb64::decomp_dir(&source());
    if !decomp.join("src/ft/ftdata.c").is_file() {
        eprintln!("skipped: reference/ssb64 not checked out");
        return;
    }
    let names = byor_rs::ssb64::file_names(&decomp).unwrap();
    assert_eq!(names["MarioMain"], 203);
    assert_eq!(names["FTMarioAnimWait"], 499);
    let cm = byor_rs::ssb64::common_motions(&decomp).unwrap();
    assert_eq!((cm[&4].as_str(), cm[&10].as_str(), cm[&165].as_str(), cm[&184].as_str()), ("Wait", "Run", "Attack11", "AttackAirN"));
    let tables = byor_rs::ssb64::fighter_tables(&decomp, &names).unwrap();
    assert_eq!(tables.len(), byor_rs::ssb64::FIGHTERS.len());
    for f in &byor_rs::ssb64::FIGHTERS[..12] {
        let t = &tables[*f];
        assert!(t.motions[164].as_deref().unwrap().ends_with("AnimTaunt"), "{f}");
        assert!(t.motions[165].as_deref().unwrap().ends_with("AnimJab1"), "{f}");
    }
    assert_eq!(tables["Mario"].attr, 0x428);
    assert_eq!(tables["Mario"].specials[3], "SpecialN");
    assert_eq!(tables["Pikachu"].specials.len(), 16);
}

fn rom() -> Option<Vec<u8>> {
    let path = std::env::var_os("BYOR_TEST_SSB64_ROM")?;
    let (game, z64) = byor_rs::rom::identify(Path::new(&path)).unwrap().expect("a known ROM");
    assert_eq!(game, "ssb64");
    Some(z64)
}

#[test]
fn ssb64_fighters_have_the_core_motions() {
    let Some(z64) = rom() else {
        eprintln!("skipped: BYOR_TEST_SSB64_ROM not set");
        return;
    };
    let out = std::env::temp_dir().join(format!("byor-ssb64-core-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    byor_rs::ssb64::export(&source(), &z64, &out, &mut |_, _| {}).unwrap();
    let index: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join("models/ssb64/fighters.json")).unwrap()).unwrap();
    let fighters = index["fighters"].as_object().unwrap();
    assert_eq!(fighters.len(), 13);
    for (name, f) in fighters {
        let glb = std::fs::read(out.join("models/ssb64").join(f["file"].as_str().unwrap())).unwrap();
        assert_eq!(&glb[..4], b"glTF");
        assert!(f["joints"].as_u64().unwrap() >= 20, "{name}");
        assert!(f["triangles"].as_u64().unwrap() >= 250, "{name}");
        if name == "boss" {
            continue;
        }
        let clips = f["clips"].as_object().unwrap();
        let by_motion: std::collections::HashMap<&str, &str> = f["motions"].as_object().unwrap().values().map(|m| (m["motion"].as_str().unwrap(), m["clip"].as_str().unwrap())).collect();
        // idle, walk, dash, run, jump, fall, land, jab, smashes, aerial, damage: what a playable fighter needs first
        for m in ["Wait", "WalkSlow", "WalkMiddle", "WalkFast", "Dash", "Run", "KneeBend", "JumpF", "Fall", "LandingLight", "Attack11", "AttackS4", "AttackHi4", "AttackLw4", "AttackAirN", "DamageHi1", "DamageFlyHi", "Appeal"] {
            let clip = by_motion.get(m).unwrap_or_else(|| panic!("{name} has no {m} motion"));
            assert!(clips.contains_key(*clip), "{name}: motion {m} plays {clip}, which was not exported");
        }
        assert!(clips["Wait"]["frames"].as_u64().unwrap() > 10, "{name}");
    }
    let mario = &fighters["mario"];
    assert_eq!(mario["clips"]["WalkSlow"]["frames"], 91, "the slow walk is walkslow_anim_length (90) frames + the end frame");
    assert_eq!(mario["motions"]["165"]["clip"], "Attack11");
    assert_eq!(mario["motions"]["198"]["motion"], "SpecialN", "fighter motions are named from src/ft/ftchar/ftmario/ftmario.h");
    assert_eq!(mario["motions"]["198"]["clip"], "FireballGround");
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn ssb64_import_matches_python() {
    let (Some(z64), Some(reference)) = (rom(), std::env::var_os("BYOR_TEST_SSB64_REF")) else {
        eprintln!("skipped: BYOR_TEST_SSB64_ROM / BYOR_TEST_SSB64_REF not set");
        return;
    };
    let out = std::env::temp_dir().join(format!("byor-ssb64-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    byor_rs::ssb64::export(&source(), &z64, &out, &mut |_, _| {}).unwrap();
    let want = PathBuf::from(reference).join("public/models/ssb64");
    let mut n = 0;
    for e in std::fs::read_dir(&want).unwrap().flatten() {
        let name = e.file_name();
        let a = std::fs::read(out.join("models/ssb64").join(&name)).unwrap_or_else(|_| panic!("{name:?} not written"));
        assert!(a == std::fs::read(e.path()).unwrap(), "{name:?} differs from the Python output");
        n += 1;
    }
    assert_eq!(n, std::fs::read_dir(out.join("models/ssb64")).unwrap().count());
    assert_eq!(n, 14);
    let _ = std::fs::remove_dir_all(&out);
}

/// The fighter sounds (native/SSB64.md "Sounds"): every playable fighter gets its voices and its special-move effects.
#[test]
fn ssb64_sounds_exist_for_every_fighter() {
    let Some(z64) = rom() else {
        eprintln!("skipped: BYOR_TEST_SSB64_ROM not set");
        return;
    };
    let out = std::env::temp_dir().join(format!("byor-ssb64-sounds-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let n = byor_rs::ssb64_sounds::export(&source(), &z64, &out).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join("manifest-ssb64.json")).unwrap()).unwrap();
    let clips = manifest["clips"].as_object().unwrap();
    assert_eq!(clips.len(), n);
    assert!(n > 200, "{n} sounds");
    for f in byor_rs::ssb64_sounds::FIGHTER_NAMES {
        // every fighter has hurt, star-KO and attack voices (SSB64 gives Jigglypuff no plain Dead voice)
        for v in ["Damage", "DeadUp", "Smash1", "Smash2", "Smash3"] {
            let name = format!("Voice{f}{v}");
            let c = clips.get(&name).unwrap_or_else(|| panic!("{name} not exported"));
            assert!(c["peak"].as_i64().unwrap() > 3000, "{name} is nearly silent");
            assert!(c["frames"].as_u64().unwrap() > 1600, "{name} is shorter than 50 ms");
            let wav = std::fs::read(out.join(format!("{name}_0.wav"))).unwrap();
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 32000);
        }
    }
    for name in byor_rs::ssb64_sounds::EXPORT_FGM {
        let c = clips.get(name).unwrap_or_else(|| panic!("{name} not exported"));
        assert!(c["peak"].as_i64().unwrap() > 3000, "{name} is nearly silent");
    }
    // the two halves of the Falcon Punch and the PK Fire / Thunder Jolt calls are voices of their own
    for name in ["VoiceCaptainSpecialNFalcon", "VoiceCaptainSpecialNPunch", "VoiceNessSpecialN", "VoicePikachuSpecialN"] {
        assert!(clips[name]["duration"].as_f64().unwrap() > 0.3, "{name}");
    }
    let _ = std::fs::remove_dir_all(&out);
}

/// With BYOR_TEST_SSB64_SOUNDS_REF = a folder written by `python3 scripts/export-ssb64-sounds.py --out <dir>`, every
/// file the Rust port writes is byte-identical.
#[test]
fn ssb64_sounds_match_python() {
    let (Some(z64), Some(reference)) = (rom(), std::env::var_os("BYOR_TEST_SSB64_SOUNDS_REF")) else {
        eprintln!("skipped: BYOR_TEST_SSB64_ROM / BYOR_TEST_SSB64_SOUNDS_REF not set");
        return;
    };
    let out = std::env::temp_dir().join(format!("byor-ssb64-sounds-ref-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    byor_rs::ssb64_sounds::export(&source(), &z64, &out).unwrap();
    let mut n = 0;
    for e in std::fs::read_dir(PathBuf::from(reference)).unwrap().flatten() {
        let name = e.file_name();
        let a = std::fs::read(out.join(&name)).unwrap_or_else(|_| panic!("{name:?} not written"));
        assert!(a == std::fs::read(e.path()).unwrap(), "{name:?} differs from the Python output");
        n += 1;
    }
    assert_eq!(n, std::fs::read_dir(&out).unwrap().count());
    assert!(n > 200);
    let _ = std::fs::remove_dir_all(&out);
}
