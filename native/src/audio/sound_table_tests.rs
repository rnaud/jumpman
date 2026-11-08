//! The complete SM64 sound table (`sounds.json`, every `SOUND_*` of the decomp's `include/sounds.h`): its shape, that
//! every sound id the code asks for exists, and that new sounds render audio through the wasm engine.
use super::engine::Sm64Engine;
use super::processor::{Cmd, MasterGain, Processor};
use super::sfx::{course_music, PlayerState, Sfx, MAX_PER_TICK};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

const SOUNDS_JSON: &str = include_str!("../../../assets/audio/music/sounds.json");

fn table() -> HashMap<String, u64> {
    serde_json::from_str(SOUNDS_JSON).expect("sounds.json")
}

/// The `native/` directory (the app crate's manifest dir).
fn native() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The repository root.
fn root() -> PathBuf {
    native().join("..")
}

#[test]
fn table_covers_every_bank_with_unique_ids() {
    let t = table();
    // `SOUND_ARG_LOAD(bank, id, priority, flags)` packs the bank in the top nibble: 10 banks, nothing above.
    let mut per_bank: BTreeMap<u64, usize> = BTreeMap::new();
    for (name, &id) in &t {
        assert!(id <= u32::MAX as u64, "{name}: {id} is not a u32");
        assert!(id >> 28 < 10, "{name}: bank {}", id >> 28);
        *per_bank.entry(id >> 28).or_default() += 1;
    }
    let ids: HashSet<u64> = t.values().copied().collect();
    assert_eq!(ids.len(), t.len(), "two names share one id (bank, sound, priority and flags)");
    // action, moving, voice, general, env, obj, air, menu, general2, obj2 (counted from the US build of sounds.h).
    let expected: BTreeMap<u64, usize> = [(0, 56), (1, 12), (2, 52), (3, 105), (4, 25), (5, 104), (6, 15), (7, 38), (8, 15), (9, 19)].into();
    assert_eq!(per_bank, expected);
    assert_eq!(t.len(), 441);
    // Spot checks against values printed by the C compiler for the US header (including priority and flag bits).
    // (the low nibble is the WAITING status `SOUND_ARG_LOAD` sets).
    assert_eq!(t["GENERAL_COIN"], 0x3811_8081);
    assert_eq!(t["OBJ_THWOMP"], 0x500C_A081);
    assert_eq!(t["MARIO_YAHOO"], 0x2404_8081);
}

/// With the decomp checked out, the table is exactly the set of `SOUND_ARG_LOAD` macros of `sounds.h`.
#[test]
fn table_matches_the_decomp_header() {
    let header = root().join("reference/sm64/include/sounds.h");
    let Ok(text) = std::fs::read_to_string(&header) else { return };
    let mut names = HashSet::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("#define SOUND_") {
            if line.contains("SOUND_ARG_LOAD(") && !rest.starts_with("ARG_LOAD") {
                names.insert(rest.split_whitespace().next().unwrap().to_string());
            }
        }
    }
    let t = table();
    let json: HashSet<String> = t.keys().cloned().collect();
    assert_eq!(names, json);
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every string literal in the sim and app sources that looks like a `SOUND_*` name (a bank prefix followed by
/// upper-case words) must be a key of `sounds.json`: the sim crate names sounds as plain strings, so a typo would
/// otherwise be a silent sound.
#[test]
fn every_sound_name_referenced_by_code_exists() {
    let t = table();
    let banks = ["ACTION_", "MOVING_", "MARIO_", "GENERAL_", "GENERAL2_", "ENV_", "OBJ_", "OBJ2_", "AIR_", "MENU_", "PEACH_"];
    let mut files = vec![];
    rust_files(&native().join("sim/src"), &mut files);
    rust_files(&native().join("src"), &mut files);
    let mut checked = 0;
    let mut bad = vec![];
    for f in files {
        // Documentation of the table itself and this test quote names that are not requests.
        if f.file_name().is_some_and(|n| n == "sound_table_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or(line);
            let mut rest = code;
            while let Some(i) = rest.find('"') {
                let after = &rest[i + 1..];
                let Some(j) = after.find('"') else { break };
                let lit = &after[..j];
                rest = &after[j + 1..];
                if lit.len() > 4
                    && !lit.ends_with('_')
                    && lit.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    && banks.iter().any(|b| lit.starts_with(b))
                {
                    checked += 1;
                    if !t.contains_key(lit) {
                        bad.push(format!("{}:{}: {lit}", f.strip_prefix(native()).unwrap_or(&f).display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(checked > 150, "only {checked} sound names found: the scan is broken");
    assert!(bad.is_empty(), "sound names missing from sounds.json:\n{}", bad.join("\n"));
}

#[test]
fn mario_and_ambient_tables_use_real_names() {
    let t = table();
    for n in super::mario_sounds::NAMES {
        assert!(t.contains_key(*n), "{n}");
    }
    for (course, name, _) in super::ambient::EMITTERS {
        assert!(t.contains_key(*name), "{course}: {name}");
        assert!(course_music(course, 1).level > 0 || *course == "bob", "{course} is not a course");
    }
}

fn sfx() -> (Sfx, std::sync::mpsc::Receiver<Cmd>) {
    let (tx, rx) = channel();
    let mut s = Sfx::new(tx, MasterGain::new(0.0), false);
    s.set_player(&PlayerState { camera_space_pos: [0.0; 3], metal: false, floor_type: None, underwater_floor: false });
    (s, rx)
}

fn sounds_in(rx: &std::sync::mpsc::Receiver<Cmd>) -> Vec<[f64; 6]> {
    rx.try_iter().filter_map(|c| if let Cmd::Sound(s) = c { Some(s) } else { None }).collect()
}

#[test]
fn named_sounds_resolve_case_terrain_and_voice_rules() {
    let t = table();
    let (mut s, rx) = sfx();
    s.named("general_door_turn_key", None);
    s.named("OBJ_THWOMP", Some([10.0, 20.0, -30.0]));
    let got = sounds_in(&rx);
    assert_eq!(got[0][0], t["GENERAL_DOOR_TURN_KEY"] as f64);
    assert_eq!(got[0][2], 0.0, "Mario's sounds use source 0");
    assert_eq!(got[1][0], t["OBJ_THWOMP"] as f64);
    assert_eq!(&got[1][3..], &[10.0, 20.0, -30.0]);
    assert!(got[1][2] >= 2.0 && got[1][2] <= 13.0, "object sounds rotate through the one-shot slots");
    // `terrain_landing` is `SOUND_ACTION_TERRAIN_LANDING + terrain`: the id is the base plus the floor variant (grass here).
    s.named("terrain_landing", None);
    let landing = sounds_in(&rx)[0][0] as u64;
    assert_eq!(landing & !0x00FF_0000, t["ACTION_TERRAIN_LANDING"] & !0x00FF_0000);
    // Mario's voices only play for his voice set; the other characters keep their own clips.
    s.set_character("link");
    s.named("mario_yahoo", None);
    assert!(sounds_in(&rx).is_empty());
    s.set_character("sm64");
    s.named("mario_yahoo", None);
    assert_eq!(sounds_in(&rx)[0][0], t["MARIO_YAHOO"] as f64);
    assert!(s.missing.is_empty(), "{:?}", s.missing);
}

#[test]
fn identical_positional_sounds_are_rate_limited_per_tick() {
    let (mut s, rx) = sfx();
    s.limit_per_tick(true);
    for _ in 0..20 {
        s.object_sound_named("OBJ_BOBOMB_WALK", [100.0, 0.0, -500.0]);
    }
    assert_eq!(sounds_in(&rx).len(), MAX_PER_TICK as usize);
    // The next tick starts a new budget.
    s.set_player(&PlayerState { camera_space_pos: [0.0; 3], metal: false, floor_type: None, underwater_floor: false });
    s.object_sound_named("OBJ_BOBOMB_WALK", [100.0, 0.0, -500.0]);
    assert_eq!(sounds_in(&rx).len(), 1);
}

#[test]
fn looping_sounds_keep_one_source_per_emitter() {
    let (mut s, rx) = sfx();
    for _ in 0..5 {
        s.object_sound_keyed("AIR_BOBOMB_LIT_FUSE", [0.0, 0.0, -400.0], 7);
    }
    s.object_sound_keyed("AIR_BOBOMB_LIT_FUSE", [0.0, 0.0, -400.0], 8);
    let got = sounds_in(&rx);
    assert!(got[..5].iter().all(|g| g[2] == got[0][2]), "a loop must reuse its source so the engine does not restart it");
    assert_ne!(got[5][2], got[0][2]);
    assert!(got.iter().all(|g| (14.0..18.0).contains(&g[2])));
}

fn big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(128 << 20).spawn(f).unwrap().join().unwrap();
}

fn energy(frames: &[(f32, f32)]) -> f32 {
    frames.iter().map(|(l, r)| l.abs() + r.abs()).sum()
}

/// Sounds the game requests that the old 99-name table lacked, played through the wasm engine from silence.
#[test]
fn new_sounds_render_audio_headlessly() {
    big_stack(|| {
        let Ok(bytes) = std::fs::read(root().join("assets/audio/music/sm64-us.wasm")) else { return };
        let t = table();
        let names = [
            "OBJ_THWOMP",
            "OBJ_BOBOMB_WALK",
            "OBJ_BOO_LAUGH_LONG",
            "OBJ_BULLY_WALKING",
            "OBJ_KING_BOBOMB_JUMP",
            "OBJ2_BOWSER_ROAR",
            "OBJ_BOWSER_WALK",
            "GENERAL_CHAIN_CHOMP1",
            "GENERAL_COIN_SPURT",
            "GENERAL2_PURPLE_SWITCH",
            "GENERAL_CAGE_OPEN",
            "MARIO_PUNCH_YAH",
            "MARIO_WHOA",
            "MOVING_LAVA_BURN",
            "ENV_WATERFALL2",
        ];
        let t3 = course_music("bob", 1);
        let mut silent = vec![];
        for name in names {
            let mut p = Processor::new(Sm64Engine::new(&bytes).expect("engine"));
            // A bare sound player (no course music) so that only the effect is measured.
            p.render_offline(&[Cmd::Location([t3.level as f64, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]), Cmd::Track { sequence: 0, preset: 0 }], 32000 / 4);
            let id = t[name] as f64;
            let mut out = Vec::new();
            // Loops are re-requested every tick like the game does; one-shots once.
            for _ in 0..24 {
                out.extend(p.render_offline(&[Cmd::Sound([id, 1.0, 5.0, 0.0, 0.0, -300.0])], 32000 / 16));
            }
            if energy(&out) < 1.0 {
                silent.push(name);
            }
        }
        assert!(silent.is_empty(), "silent through the engine: {silent:?}");
    });
}
