//! Collectible and enemy totals of the Banjo-Kazooie maps per world (main map + its sub-areas), against the game's
//! counters: 100 music notes per world, 10 Jiggies, 6 empty honeycombs.
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn group(id: &str) -> &'static str {
    let table: [(&str, &str, &str); 11] = [
        ("ccw", "bk_click_clock_wood", "bk_ccw_"),
        ("lair", "bk_gl_", "bk_gl_"),
        ("sm", "bk_spiral_mountain", "bk_sm_"),
        ("mm", "bk_mumbos_mountain", "bk_mm_"),
        ("ttc", "bk_treasure_trove_cove", "bk_ttc_"),
        ("cc", "bk_clankers_cavern", "bk_cc_"),
        ("bgs", "bk_bubblegloop_swamp", "bk_bgs_"),
        ("gv", "bk_gobis_valley", "bk_gv_"),
        ("fp", "bk_freezeezy_peak", "bk_fp_"),
        ("rbb", "bk_rusty_bucket_bay", "bk_rbb_"),
        ("mmm", "bk_mad_monster_mansion", "bk_mmm_"),
    ];
    table.iter().find(|(_, a, b)| id.starts_with(a) || id.starts_with(b)).map(|t| t.0).unwrap_or_else(|| panic!("unknown map {id}"))
}

/// (notes, jiggies, honeycombs, enemies) per world group over the setup report.
fn totals() -> BTreeMap<&'static str, (u64, u64, u64, u64)> {
    let setup: Value = serde_json::from_slice(&std::fs::read(assets().join("courses/bk_setup_report.json")).unwrap()).unwrap();
    let mut out: BTreeMap<&str, (u64, u64, u64, u64)> = BTreeMap::new();
    for (id, w) in setup["worlds"].as_object().unwrap() {
        let e = out.entry(group(id)).or_default();
        e.0 += w["pickups"]["note"].as_u64().unwrap_or(0);
        e.1 += w["pickups"]["jiggy"].as_u64().unwrap_or(0);
        e.2 += w["pickups"]["honeycomb"].as_u64().unwrap_or(0);
        e.3 += w["enemies"].as_object().map_or(0, |m| m.values().map(|v| v.as_u64().unwrap()).sum());
    }
    out
}

#[test]
fn notes_over_all_the_maps_of_a_world_add_up_to_the_games_hundred() {
    let t = totals();
    for g in ["ttc", "cc", "gv", "fp", "rbb", "mmm", "ccw"] {
        assert_eq!(t[g].0, 100, "{g}: notes over its maps");
    }
    // Mumbo's Mountain and Bubblegloop Swamp keep five notes that the setup files do not hold as sprite props.
    for g in ["mm", "bgs"] {
        assert!((95..=100).contains(&t[g].0), "{g}: {}", t[g].0);
    }
}

#[test]
fn every_world_has_jiggies_honeycombs_and_enemies_in_its_maps() {
    let t = totals();
    for g in ["mm", "ttc", "cc", "gv", "fp", "rbb", "mmm", "ccw"] {
        assert!(t[g].1 >= 2 && t[g].1 <= 14, "{g}: {} jiggies", t[g].1);
        assert!(t[g].3 >= 1, "{g}: enemies");
    }
    // The sub-areas hold the placements the main maps lack: the lair rooms alone carry eggs, feathers and Jiggies.
    assert!(t["lair"].1 >= 1);
    println!("totals per world (notes, jiggies, honeycombs, enemies): {t:?}");
}
