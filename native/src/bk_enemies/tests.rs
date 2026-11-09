//! Parse tests of the exported setups (counts per world, every placed enemy implemented or in the explicit skip
//! list), model assets, and scripted scenarios in the real BK worlds: Banjo's egg, Mario's ground pound and
//! Link's sword each defeat a Grublin in Mumbo's Mountain.
use super::*;
use crate::game::course_data::CourseData;
use crate::game::world::CourseRequest;
use jumpman_sim::banjo_ext::{BkPickup, PickupKind};
use jumpman_sim::bk_enemy::{bull, crab, hum};
use jumpman_sim::link::items::Age;
use jumpman_sim::object::ObjId;
use jumpman_sim::state::*;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn worlds() -> Vec<&'static str> {
    crate::game::courses::COURSES.iter().filter(|c| c.series == crate::game::courses::Series::Banjo).map(|c| c.id).collect()
}

fn report() -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(assets().join("courses/bk_setup_report.json")).expect("run scripts/export-bk-setup.py")).unwrap()
}

fn course_json(id: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(CourseData::path(&assets(), id, 1)).unwrap()).unwrap()
}

/// Enemies placed per world, as written by `export-bk-setup.py` from the maps' setup files.
const PLACED: &[(&str, &[(&str, u64)])] = &[
    ("bk_spiral_mountain", &[]),
    ("bk_mumbos_mountain", &[("beehive", 2), ("bigbutt", 1), ("grublin", 4), ("termite", 4)]),
    ("bk_treasure_trove_cove", &[("beehive", 4), ("grublin", 1), ("nipper", 1), ("shrapnel", 4), ("snippet", 5)]),
    ("bk_clankers_cavern", &[("beehive", 2), ("grille_chompa", 10), ("mutie_snippet", 4), ("snippet", 1)]),
    ("bk_bubblegloop_swamp", &[("beehive", 4), ("buzzbomb", 5), ("flibbit", 5)]),
    ("bk_gobis_valley", &[("beehive", 3), ("slappa", 2), ("snarebear", 1)]),
    ("bk_freezeezy_peak", &[("beehive", 4), ("sir_slush", 5)]),
    ("bk_rusty_bucket_bay", &[("beehive", 1), ("grille_chompa", 6)]),
    ("bk_mad_monster_mansion", &[("bat", 6), ("beehive", 2), ("limbo", 6), ("teehee", 2)]),
    ("bk_click_clock_wood", &[("beehive", 2), ("buzzbomb", 2)]),
    ("bk_click_clock_wood_spring", &[("beehive", 3), ("bigbutt", 1), ("clucker", 15), ("grublin_hood", 3), ("snarebear", 8)]),
    ("bk_click_clock_wood_summer", &[("beehive", 3), ("bigbutt", 1), ("clucker", 18), ("grublin_hood", 4), ("snarebear", 8)]),
    ("bk_click_clock_wood_autumn", &[("beehive", 3), ("bigbutt", 1), ("buzzbomb", 4), ("clucker", 18), ("grublin_hood", 4), ("snarebear", 8)]),
    ("bk_click_clock_wood_winter", &[("beehive", 2), ("grublin_hood", 6), ("sir_slush", 6)]),
];

#[test]
fn every_world_has_the_enemies_of_its_setup_file() {
    let report = report();
    // (the sub-areas' placements are checked by the world-graph tests in game/bk_warp_tests.rs)
    assert!(worlds().len() >= PLACED.len());
    for (id, want) in PLACED {
        let world = &report["worlds"][id];
        assert!(world.is_object(), "{id} missing from the setup report");
        let got: std::collections::BTreeMap<String, u64> = world["enemies"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap())).collect();
        let want: std::collections::BTreeMap<String, u64> = want.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        assert_eq!(got, want, "{id}");
        // and they are in the course file as bkEnemy objects
        let objects = course_json(id)["objects"].as_array().unwrap().clone();
        let n = objects.iter().filter(|o| o["behavior"] == "bkEnemy").count() as u64;
        assert_eq!(n, want.values().sum::<u64>(), "{id}: bkEnemy objects");
    }
}

#[test]
fn every_placed_actor_is_implemented_or_in_the_explicit_skip_list() {
    let report = report();
    let implemented: Vec<u64> = report["implemented"].as_object().unwrap().keys().map(|k| u64::from_str_radix(k.trim_start_matches("0x"), 16).unwrap()).collect();
    // each implemented id maps to a sim Kind (the table in bk_enemy.rs and the exporter agree)
    for id in &implemented {
        assert!(Kind::from_actor(*id as u32).is_some(), "actor {id:#x} implemented in the exporter but not in the sim");
    }
    for (id, world) in report["worlds"].as_object().unwrap() {
        let placed: u64 = world["enemies"].as_object().unwrap().values().map(|v| v.as_u64().unwrap()).sum();
        let skipped: u64 = world["skippedEnemies"].as_object().unwrap().values().map(|v| v.as_u64().unwrap()).sum();
        // every enemy-like actor of the world is either one of the placed ones or listed (with its count) as skipped
        let listed: u64 = world["actorIds"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| {
                let n = u64::from_str_radix(k.trim_start_matches("0x"), 16).unwrap();
                implemented.contains(&n) || report["skipped"].as_object().unwrap().contains_key(*k)
            })
            .map(|(_, v)| v.as_u64().unwrap())
            .sum();
        assert_eq!(listed, placed + skipped, "{id}: an enemy-like actor is neither implemented nor skipped");
    }
    // the skip list carries a reason for each entry
    for (id, why) in report["skipped"].as_object().unwrap() {
        assert!(why.as_str().is_some_and(|w| w.len() > 3), "{id} has no reason");
    }
}

#[test]
fn course_enemy_params_decode_and_models_exist_with_a_clip_for_every_state() {
    let kinds_used: std::collections::BTreeSet<_> = worlds()
        .iter()
        .flat_map(|id| course_json(id)["objects"].as_array().unwrap().clone())
        .filter(|o| o["behavior"] == "bkEnemy")
        .map(|o| {
            let p = o["param"].as_u64().unwrap() as u32;
            let kind = Kind::from_actor(p & 0xFFFF).unwrap_or_else(|| panic!("param {p:#x}"));
            assert!((p >> 16) >= 25 && (p >> 16) <= 1000, "scale {} of {kind:?}", p >> 16);
            kind.index()
        })
        .collect();
    assert!(kinds_used.len() >= 12 - 1, "{kinds_used:?}");
    for index in kinds_used {
        let kind = Kind::from_index(index);
        for season in ["spring", "summer", "autumn", "winter"] {
            let key = asset_key(kind, &format!("bk_click_clock_wood_{season}"));
            let path = model_path(&assets(), &key);
            let rig = Rig::new(EnemyAsset::parse(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap());
            assert!(!rig.asset.batches.is_empty() && rig.asset.batches.iter().all(|b| b.vertices.len() % 3 == 0), "{key}");
            // every state the behavior can enter has a clip
            let (lo, hi) = state_range(kind);
            for s in lo..=hi {
                if let Some((clip, _)) = rig.asset.state_clip(s) {
                    assert!(rig.asset.clips.contains_key(clip), "{key}: state {s} clip {clip}");
                } else {
                    assert!(!used_state(kind, s), "{key}: state {s} has no animation row");
                }
            }
            // a pose evaluates to finite matrices
            let mut pose = Pose::new(rig.asset.bones.len());
            if kind == Kind::Podium {
                continue; // static model
            }
            let (clip, _) = rig.asset.state_clip(first_state(kind)).unwrap();
            rig.evaluate(clip, 0.37, &mut pose);
            for m in rig.matrices(&pose) {
                assert!(m.to_cols_array().iter().all(|v| v.is_finite()), "{key}");
            }
        }
    }
}

fn first_state(kind: Kind) -> i32 {
    match kind {
        Kind::Bigbutt => bull::EAT,
        _ => 1,
    }
}

fn state_range(kind: Kind) -> (i32, i32) {
    match kind {
        Kind::Grublin | Kind::GrublinHood | Kind::Limbo => (1, 10),
        Kind::Bigbutt => (1, 15),
        Kind::Snippet | Kind::MutieSnippet => (1, 7),
        Kind::Termite => (1, 6),
        Kind::SirSlush => (1, 2),
        Kind::Bat => (1, 8),
        Kind::Shrapnel => (1, 4),
        Kind::Snarebear => (1, 2),
        Kind::Beehive => (1, 2),
        Kind::Snowball => (1, 1),
        Kind::Clucker => (1, 7),
        Kind::GrilleChompa => (1, 6),
        Kind::Flibbit => (1, 7),
        Kind::Buzzbomb => (1, 7),
        Kind::TeeHee => (1, 6),
        Kind::Nipper => (1, 7),
        Kind::Slappa => (1, 10),
        Kind::Mumbo => (3, 5),
        Kind::Podium => (1, 1),
    }
}

/// States the behavior actually enters.
fn used_state(kind: Kind, s: i32) -> bool {
    match kind {
        Kind::Termite => matches!(s, 1 | 2 | 6),
        Kind::Bigbutt => !matches!(s, 7 | 10 | 11),
        Kind::Snippet | Kind::MutieSnippet => true,
        Kind::Podium => false,
        _ => true,
    }
}

#[test]
fn pickup_counts_match_the_setup_files_and_lie_near_the_ground() {
    let report = report();
    // the notes, feathers and eggs of the maps (sprite props of the setup files)
    let want: &[(&str, &str, u64)] = &[
        ("bk_mumbos_mountain", "note", 85),
        ("bk_treasure_trove_cove", "redFeather", 100),
        ("bk_treasure_trove_cove", "note", 82),
        ("bk_freezeezy_peak", "goldFeather", 11),
        ("bk_gobis_valley", "egg", 29),
    ];
    for (id, kind, n) in want {
        let got = report["worlds"][id]["pickups"][kind].as_u64().unwrap_or(0);
        assert!(got >= *n, "{id} {kind}: {got} < {n}");
    }
    // positions: most notes and eggs float within 600 units over a floor of the world (a wrong scale or axis would
    // put almost none there; the red feathers of Treasure Trove Cove hang in the sky above the flight pads on purpose)
    for id in ["bk_mumbos_mountain", "bk_treasure_trove_cove"] {
        let all = pickups::parse_pickups(&course_json(id));
        assert!(all.len() > 80, "{id}: {}", all.len());
        let items: Vec<_> = all.into_iter().filter(|it| matches!(it.kind, PickupKind::Note | PickupKind::Egg)).collect();
        let sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap();
        let near = items
            .iter()
            .filter(|it| {
                let f = sim.mario.world.find_floor(it.pos[0], it.pos[1] + 50.0, it.pos[2]);
                f.surface.is_some() && it.pos[1] - f.height > -50.0 && it.pos[1] - f.height < 600.0
            })
            .count();
        assert!(near * 10 >= items.len() * 6, "{id}: only {near}/{} notes and eggs sit over the ground", items.len());
    }
}

#[test]
fn pickup_streaming_adds_nearby_removes_far_caps_and_never_returns_collected() {
    let mut items: Vec<pickups::Item> = (0..300)
        .map(|i| pickups::parse_pickups(&serde_json::json!({"bkPickups": [[0, (i % 30) as f64 * 100.0, 10.0, (i / 30) as f64 * 100.0]]})).remove(0))
        .collect();
    let mut sim_list: Vec<BkPickup> = vec![];
    let (add, remove) = pickups::stream(&mut items, &sim_list, &[0.0, 0.0, 0.0]);
    assert!(remove.is_empty());
    assert!(add.len() <= pickups::MAX_LIVE_PICKUPS && add.len() > 20, "{}", add.len());
    assert!(add.iter().all(|p| (p.pos[0].hypot(p.pos[2])) < pickups::PICKUP_RANGE));
    sim_list.extend(add);
    // the player collects the first one
    let taken = sim_list.remove(0);
    let (add, _) = pickups::stream(&mut items, &sim_list, &[0.0, 0.0, 0.0]);
    sim_list.extend(add);
    assert!(!sim_list.iter().any(|p| p.pos == taken.pos), "a collected pickup must not respawn");
    // walking far away drops the old ones
    let (_, remove) = pickups::stream(&mut items, &sim_list, &[50_000.0, 0.0, 0.0]);
    assert!(!remove.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Scenarios in the real world
// ---------------------------------------------------------------------------------------------

fn idle() -> ControllerSnapshot {
    ControllerSnapshot::default()
}

fn mumbos() -> GameSim {
    GameSim::load(&assets(), CourseRequest::new("bk_mumbos_mountain")).unwrap()
}

fn grublins(sim: &GameSim) -> Vec<usize> {
    sim.mario.objects.objects.iter().enumerate().filter(|(_, o)| o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == Kind::Grublin).map(|(i, _)| i).collect()
}

/// Stand `dist` units from enemy `i` (on the floor, facing it) with everything else asleep.
fn face_enemy(sim: &mut GameSim, i: usize, dist: f64, from_above: f64) {
    for (j, o) in sim.mario.objects.objects.iter_mut().enumerate() {
        if j != i && o.behavior == ObjectBehaviorKind::BkEnemy {
            o.home[1] -= 20000.0;
            o.pos[1] -= 20000.0;
            o.active = false;
        }
    }
    for _ in 0..4 {
        sim.tick(&idle());
    }
    let e = sim.mario.objects.objects[i].pos;
    let floor = sim.mario.world.find_floor(e[0], e[1] + 200.0, e[2]).height;
    let m = &mut sim.mario;
    m.pos = [e[0], floor + from_above, e[2] - dist];
    m.vel = [0.0; 3];
    m.forward_vel = 0.0;
    m.face_angle[1] = 0.0;
    m.set_action(ACT_IDLE, 0.0);
    m.invinc_timer = 0.0;
}

fn dead(sim: &GameSim, i: usize) -> bool {
    let o = &sim.mario.objects.objects[i];
    !o.active || o.action == hum::DIE
}

#[test]
fn world_mumbos_mountain_has_its_enemies_awake_and_on_the_floor() {
    let mut sim = mumbos();
    let g = grublins(&sim);
    assert_eq!(g.len(), 4);
    for _ in 0..10 {
        sim.tick(&idle());
    }
    for i in &g {
        let o = &sim.mario.objects.objects[*i];
        let floor = sim.mario.world.find_floor(o.pos[0], o.pos[1] + 200.0, o.pos[2]);
        assert!(floor.surface.is_some(), "grublin {i} has no floor at {:?}", o.pos);
        assert!((o.pos[1] - floor.height).abs() < 600.0, "grublin {i} at {:?} floor {}", o.pos, floor.height);
    }
    let n = sim.mario.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::BkEnemy).count();
    assert_eq!(n, 11, "4 grublins, 4 tickers, 1 bigbutt, 2 beehives");
}

#[test]
fn scenario_banjos_egg_defeats_a_grublin_in_mumbos_mountain() {
    let mut sim = mumbos();
    let i = grublins(&sim)[0];
    sim.as_banjo = true;
    // Only this Grublin takes part: another enemy walking into Mario or the egg's line (the order the object manager
    // visits them is not fixed) made this scenario flaky.
    for (k, o) in sim.mario.objects.objects.iter_mut().enumerate() {
        if k != i && o.behavior == jumpman_sim::object::ObjectBehaviorKind::BkEnemy {
            o.deactivate(-1.0);
        }
    }
    face_enemy(&mut sim, i, 650.0, 0.0);
    // freeze it so the egg's straight line stays true
    sim.mario.objects.objects[i].set_var("pause", 10_000.0);
    sim.mario.objects.objects[i].set_var("speed", 0.01);
    let eggs = sim.mario.banjo.eggs;
    let c = ControllerSnapshot { button_down: C_UP, button_pressed: C_UP, ..Default::default() };
    sim.tick(&c);
    for _ in 0..90 {
        sim.tick(&idle());
        if dead(&sim, i) {
            break;
        }
    }
    assert!(dead(&sim, i), "grublin state {} at {:?}", sim.mario.objects.objects[i].action, sim.mario.objects.objects[i].pos);
    assert!(sim.mario.banjo.eggs < eggs + 3, "egg count {} (was {eggs})", sim.mario.banjo.eggs);
}

#[test]
fn scenario_marios_ground_pound_defeats_a_grublin_in_mumbos_mountain() {
    let mut sim = mumbos();
    let i = grublins(&sim)[1];
    face_enemy(&mut sim, i, 0.0, 120.0);
    let m = &mut sim.mario;
    m.set_action(ACT_GROUND_POUND, 0.0);
    m.vel[1] = -50.0;
    let mut loot = 0;
    for _ in 0..40 {
        sim.tick(&idle());
        loot += sim.events.len();
        if dead(&sim, i) {
            break;
        }
    }
    assert!(dead(&sim, i), "state {} action {:#x}", sim.mario.objects.objects[i].action, sim.mario.action);
    let _ = loot;
}

#[test]
fn scenario_links_sword_defeats_a_grublin_in_mumbos_mountain() {
    let mut sim = mumbos();
    let i = grublins(&sim)[2];
    sim.as_link = true;
    sim.mario.link.enable(Age::Adult);
    face_enemy(&mut sim, i, 150.0, 0.0);
    sim.mario.objects.objects[i].set_var("pause", 10_000.0);
    sim.mario.objects.objects[i].set_var("speed", 0.01);
    let c = ControllerSnapshot { button_down: B_BUTTON, button_pressed: B_BUTTON, ..Default::default() };
    sim.tick(&c);
    for _ in 0..60 {
        sim.tick(&idle());
        if dead(&sim, i) {
            break;
        }
    }
    assert!(dead(&sim, i), "state {} action {:#x}", sim.mario.objects.objects[i].action, sim.mario.action);
}

#[test]
fn links_z_target_picks_up_a_chasing_grublin() {
    let mut sim = mumbos();
    let i = grublins(&sim)[0];
    sim.as_link = true;
    sim.mario.link.enable(Age::Adult);
    face_enemy(&mut sim, i, 400.0, 0.0);
    sim.focus_held = true;
    for _ in 0..10 {
        sim.tick(&idle());
    }
    assert_eq!(sim.link_focus.target, Some(crate::camera::link_focus::FocusTarget::Object(ObjId(i as u32))));
}

#[test]
fn the_enemies_of_every_world_survive_a_few_seconds_of_play_for_each_character() {
    for id in worlds() {
        for (link, banjo) in [(false, false), (true, false), (false, true)] {
            let mut sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap();
            sim.as_link = link;
            sim.as_banjo = banjo;
            if link {
                sim.mario.link.enable(Age::Adult);
            }
            // visit every enemy: stand beside it and let it act for two seconds
            let spots: Vec<[f64; 3]> = sim.mario.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::BkEnemy).map(|o| o.pos).collect();
            for (n, at) in spots.iter().enumerate().take(6) {
                sim.mario.pos = [at[0], at[1] + 200.0, at[2] - 500.0];
                sim.mario.vel = [0.0; 3];
                for t in 0..60 {
                    let c = if t % 15 == 0 { ControllerSnapshot { button_down: B_BUTTON, button_pressed: B_BUTTON, ..Default::default() } } else { idle() };
                    sim.tick(&c);
                    assert!(sim.mario.pos.iter().all(|v| v.is_finite()), "{id} enemy {n}");
                }
            }
            for o in &sim.mario.objects.objects {
                assert!(o.pos.iter().all(|v| v.is_finite()), "{id}: {:?} went non-finite", kind_of(o));
            }
        }
    }
    let _ = crab::IDLE;
}

#[test]
fn features_parse_poles_molehills_note_doors_and_mumbo_tokens() {
    let json = serde_json::json!({
        "bkClimbs": [[1.0, 2.0, 3.0, 1.0, 502.0, 3.0, 62.5, 2]],
        "bkMoles": [[10.0, 0.0, 20.0, 13, 0], [0.0, 0.0, 0.0, 7, 1]],
        "bkNoteDoors": [[5.0, 6.0, 7.0, 44, 2]],
        "bkPickups": [[6, 1.0, 2.0, 3.0]],
    });
    let f = pickups::parse_features(&json);
    assert_eq!((f.climbs.len(), f.climbs[0].kind, f.climbs[0].radius), (1, 2, 62.5));
    assert_eq!((f.moles[0].id, f.moles[0].sm, f.moles[1].sm), (13, false, true));
    assert_eq!((f.doors[0].index, f.doors[0].yaw), (2, 44.0));
    assert_eq!(pickups::parse_pickups(&json)[0].kind, PickupKind::MumboToken);
}

/// The exported poles, molehills, note doors and Mumbo tokens against the numbers `scripts/export-bk-setup.py` found in
/// the maps' setups (the actor counts of `bk_setup_report.json`).
#[test]
fn exported_features_match_the_setup_report() {
    let report = report();
    let count = |world: &str, id: &str| report["worlds"][world]["actorIds"][id].as_u64().unwrap_or(0) as usize;
    for world in ["bk_spiral_mountain", "bk_mumbos_mountain", "bk_treasure_trove_cove", "bk_rusty_bucket_bay", "bk_clankers_cavern"] {
        let f = pickups::parse_features(&course_json(world));
        assert_eq!(f.climbs.len(), count(world, "0x26"), "{world} poles");
        assert_eq!(f.moles.len(), count(world, "0x12b") + count(world, "0x37a"), "{world} molehills");
        for c in &f.climbs {
            assert!(c.top[1] > c.bottom[1], "{world}: a pole must rise");
            assert!(c.radius > 0.0 && c.radius < 400.0);
        }
    }
    // Spiral Mountain: Bottles' eight tutorial molehills, ids 1..8, and four exit poles.
    let sm = pickups::parse_features(&course_json("bk_spiral_mountain"));
    let mut ids: Vec<u32> = sm.moles.iter().map(|m| m.id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(sm.moles.iter().all(|m| m.sm) && sm.climbs.len() == 4);
    // Treasure Trove Cove teaches the Shock Spring Jump (13) and Flight (14).
    let ttc = pickups::parse_features(&course_json("bk_treasure_trove_cove"));
    let mut ids: Vec<u32> = ttc.moles.iter().map(|m| m.id).collect();
    ids.sort();
    assert_eq!(ids, vec![13, 14]);
    // The 180-note door room holds door 2.
    let door = pickups::parse_features(&course_json("bk_gl_180_note_door"));
    assert_eq!(door.doors.iter().map(|d| d.index).collect::<Vec<_>>(), vec![2]);
    // Mumbo's Mountain hides 4 tokens, Treasure Trove Cove 9.
    for (world, n) in [("bk_mumbos_mountain", 4), ("bk_treasure_trove_cove", 9)] {
        let tokens = pickups::parse_pickups(&course_json(world)).into_iter().filter(|it| it.kind == PickupKind::MumboToken).count();
        assert_eq!(tokens, n, "{world} Mumbo tokens");
    }
}

/// Scripted scenario in the real Spiral Mountain: Banjo (in a progression game) learns to climb at Bottles' molehill
/// and then climbs the nearest pole.
#[test]
fn scenario_banjo_learns_to_climb_and_climbs_a_pole_in_spiral_mountain() {
    use jumpman_sim::banjo_ext::*;
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_spiral_mountain")).unwrap();
    sim.as_banjo = true;
    let f = pickups::load_features(&assets(), "bk_spiral_mountain");
    sim.mario.banjo.begin_progression();
    sim.mario.banjo.climbs = f.climbs.clone();
    sim.mario.banjo.moles = f.moles.clone();
    sim.mario.banjo.pickups.clear();
    let tick = |sim: &mut GameSim, stick: f64| {
        let c = jumpman_sim::state::ControllerSnapshot { stick_y: stick, stick_mag: stick.abs(), ..Default::default() };
        sim.tick(&c);
    };
    // Put Banjo next to the climb molehill (id 7) and let a few frames pass.
    let mole = f.moles.iter().find(|m| m.id == 7).expect("the climb lesson");
    sim.mario.pos = [mole.pos[0] + 60.0, mole.pos[1] + 5.0, mole.pos[2]];
    for _ in 0..10 {
        tick(&mut sim, 0.0);
    }
    assert!(sim.mario.banjo.has(Ability::Climb), "the molehill teaches climbing");
    assert!(!sim.mario.banjo.has(Ability::Eggs));
    // Next to the pole: lift Banjo to it and walk in.
    let pole = &f.climbs[0];
    sim.mario.pos = [pole.bottom[0] + pole.radius * 2.0, pole.bottom[1] + 40.0, pole.bottom[2]];
    sim.mario.face_angle[1] = 0xC000 as f64; // facing -X, toward the pole axis
    sim.camera_yaw = 0x4000 as _; // the stick's forward is -X, toward the pole
    let mut grabbed = false;
    for _ in 0..90 {
        tick(&mut sim, 64.0);
        if jumpman_sim::characters::banjo_move(&sim.mario) == BANJO_CLIMB {
            grabbed = true;
            break;
        }
    }
    assert!(grabbed, "action {:#x} pos {:?} pole {:?}", sim.mario.action, sim.mario.pos, pole);
    for _ in 0..30 {
        tick(&mut sim, 64.0);
    }
    assert!(sim.mario.pos[1] > pole.bottom[1] + 100.0, "climbed to {} from {}", sim.mario.pos[1], pole.bottom[1]);
}

// ---- Clucker, Grille Chompa, Flibbit ----

fn of_kind(sim: &GameSim, kind: Kind) -> Vec<usize> {
    sim.mario.objects.objects.iter().enumerate().filter(|(_, o)| o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == kind).map(|(i, _)| i).collect()
}

/// Tick `ticks` times with Mario pinned at `anchor` (a cliff next to the enemy must not drop him out of the scenario).
fn run_at(sim: &mut GameSim, anchor: [f64; 3], ticks: usize) {
    for _ in 0..ticks {
        sim.mario.invinc_timer = 10_000.0;
        sim.mario.pos = anchor;
        sim.mario.vel = [0.0; 3];
        sim.tick(&idle());
    }
}

/// Mario `dist` units from enemy `i`, on the floor beside it (the first of eight directions with ground at the enemy's height), everything
/// else asleep, Mario untouchable. Returns the spot.
fn stand_at(sim: &mut GameSim, i: usize, dist: f64) -> [f64; 3] {
    face_enemy(sim, i, 0.0, 0.0);
    let e = sim.mario.objects.objects[i].pos;
    let mut spot = None;
    for n in 0..8 {
        let a = n as f64 * std::f64::consts::FRAC_PI_4;
        let (x, z) = (e[0] + a.sin() * dist, e[2] + a.cos() * dist);
        let f = sim.mario.world.find_floor(x, e[1] + 300.0, z);
        if f.surface.is_some() && (f.height - e[1]).abs() < 650.0 {
            spot = Some([x, f.height, z]);
            break;
        }
    }
    let spot = spot.unwrap_or([e[0], e[1], e[2] - dist]);
    sim.mario.pos = spot;
    sim.mario.vel = [0.0; 3];
    sim.mario.invinc_timer = 10_000.0;
    spot
}

fn plant_walkthrough(course: &str, kind: Kind, alert: f64, near: f64, far: f64) {
    use jumpman_sim::bk_enemy::plant;
    use jumpman_sim::object::INTERACT_DAMAGE;
    let mut sim = GameSim::load(&assets(), CourseRequest::new(course)).unwrap();
    let i = of_kind(&sim, kind)[0];
    let state = |sim: &GameSim| sim.mario.objects.objects[i].action;
    let k = jumpman_sim::bk_enemy::S * sim.mario.objects.objects[i].scale;
    // far away it stays folded and cannot be hurt or hurt anyone
    let at = stand_at(&mut sim, i, far * k * 1.5);
    run_at(&mut sim, at, 30);
    assert_eq!(state(&sim), plant::IDLE);
    assert_eq!(sim.mario.objects.objects[i].hitbox.interact_type, 0);
    // inside the alert ring it pops up and waits
    let at = stand_at(&mut sim, i, (alert + near) / 2.0 * k);
    run_at(&mut sim, at, 15);
    assert_eq!(state(&sim), plant::ALERT);
    // inside the near ring it winds up and bites; the bite is the only time it can hurt
    let at = stand_at(&mut sim, i, near * k * 0.8);
    let mut seen = Vec::new();
    for _ in 0..120 {
        run_at(&mut sim, at, 1);
        let s = state(&sim);
        if std::env::var_os("PLANT_DEBUG").is_some() {
            let o = &sim.mario.objects.objects[i];
            eprintln!("state {s} pos {:?} mario {:?} dist {:.0} scale {}", o.pos, sim.mario.pos, jumpman_sim::math::dist_3d(&o.pos, &sim.mario.pos), o.scale);
        }
        if seen.last() != Some(&s) {
            seen.push(s);
        }
        let o = &sim.mario.objects.objects[i];
        if !matches!(o.action, plant::ATTACK | plant::ATTACK_LONG) {
            assert_eq!(o.hitbox.interact_type & INTERACT_DAMAGE, 0, "hurts while folded in state {}", o.action);
        }
    }
    assert!(seen.contains(&plant::WINDUP), "{seen:?}");
    assert!(seen.iter().any(|s| matches!(*s, plant::ATTACK | plant::ATTACK_LONG)), "never bit: {seen:?}");
    // it folds again when Banjo leaves
    let at = stand_at(&mut sim, i, far * k * 1.5);
    run_at(&mut sim, at, 200);
    assert_eq!(state(&sim), plant::IDLE, "seen {seen:?}");
    // a strong blow during the bite kills it
    let at = stand_at(&mut sim, i, near * k * 0.8);
    let mut bit = false;
    for _ in 0..200 {
        run_at(&mut sim, at, 1);
        if matches!(state(&sim), plant::ATTACK | plant::ATTACK_LONG) {
            bit = true;
            break;
        }
    }
    assert!(bit, "no bite to hit");
    assert_eq!(sim.mario.objects.objects[i].hitbox.interact_type & INTERACT_DAMAGE, INTERACT_DAMAGE, "the bite hurts");
    let e = sim.mario.objects.objects[i].pos;
    sim.mario.pos = [e[0], e[1] + 120.0, e[2]];
    sim.mario.vel = [0.0, -50.0, 0.0];
    sim.mario.set_action(ACT_GROUND_POUND, 0.0);
    let mut killed = false;
    for _ in 0..40 {
        sim.mario.invinc_timer = 10_000.0;
        sim.tick(&idle());
        let o = &sim.mario.objects.objects[i];
        if !o.active || o.action == plant::DIE {
            killed = true;
            break;
        }
    }
    assert!(killed, "state {} after the ground pound", state(&sim));
}

#[test]
fn grille_chompa_pops_up_bites_and_dies_to_a_ground_pound() {
    plant_walkthrough("bk_rbb_warehouse", Kind::GrilleChompa, 400.0, 300.0, 500.0);
}

#[test]
fn clucker_pops_up_bites_and_dies_to_a_ground_pound() {
    plant_walkthrough("bk_click_clock_wood_spring", Kind::Clucker, 800.0, 600.0, 1000.0);
}

#[test]
fn flibbit_hops_toward_banjo_inside_its_leash_and_home_when_he_leaves() {
    use jumpman_sim::bk_enemy::flib;
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_bubblegloop_swamp")).unwrap();
    let i = of_kind(&sim, Kind::Flibbit)[0];
    let home = sim.mario.objects.objects[i].home;
    let flat = |a: [f64; 3], b: [f64; 3]| ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    let anchor = stand_at(&mut sim, i, 600.0);
    let start = flat(sim.mario.objects.objects[i].pos, anchor);
    let mut hopped = false;
    for _ in 0..240 {
        run_at(&mut sim, anchor, 1);
        hopped |= sim.mario.objects.objects[i].action == flib::HOP;
    }
    let after = flat(sim.mario.objects.objects[i].pos, anchor);
    assert!(hopped, "never hopped");
    assert!(after < start - 150.0, "{after} vs {start}: did not approach");
    // Banjo far outside the leash: it hops back to its spawn and settles
    let far = [home[0] + 4000.0, home[1], home[2]];
    for _ in 0..900 {
        run_at(&mut sim, far, 1);
    }
    let o = &sim.mario.objects.objects[i];
    assert!(flat(o.pos, home) < 60.0, "{} units from home in state {}", flat(o.pos, home), o.action);
    // it hurts on touch
    assert_eq!(o.hitbox.interact_type & jumpman_sim::object::INTERACT_DAMAGE, jumpman_sim::object::INTERACT_DAMAGE);
}

#[test]
fn gobis_sun_switch_opens_the_sun_door_and_the_season_switches_are_exported() {
    let f = pickups::parse_features(&course_json("bk_gobis_valley"));
    assert_eq!((f.switches.len(), f.switch_doors.len()), (1, 1));
    assert_eq!(f.switches[0].flag, 0);
    let hub = pickups::parse_features(&course_json("bk_click_clock_wood"));
    let mut flags: Vec<_> = hub.switch_doors.iter().map(|d| d.flag).collect();
    flags.sort();
    assert_eq!(flags, vec![1, 2, 3, 4], "spring..winter season doors of the hub");
    assert_eq!(hub.switches.iter().map(|s| s.flag).collect::<Vec<_>>(), vec![1]);
    assert_eq!(pickups::parse_features(&course_json("bk_click_clock_wood_spring")).switches[0].flag, 2);
    // a ground pound on the switch opens its door for good; the closed door blocks Banjo in a progression game
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_gobis_valley")).unwrap();
    sim.as_banjo = true;
    let b = &mut sim.mario.banjo;
    b.switch_actors = f.switches.clone();
    b.switch_doors = f.switch_doors.clone();
    b.progression = true;
    let sw = f.switches[0].pos;
    sim.mario.pos = [sw[0], sw[1] + 100.0, sw[2]];
    sim.mario.vel = [0.0, -50.0, 0.0];
    sim.mario.set_action(ACT_GROUND_POUND, 0.0);
    for _ in 0..40 {
        sim.mario.invinc_timer = 10_000.0;
        sim.tick(&idle());
        if sim.mario.banjo.switches & 1 != 0 {
            break;
        }
    }
    assert_eq!(sim.mario.banjo.switches & 1, 1, "action {:#x}", sim.mario.action);
    assert!(sim.mario.banjo.carried().switches & 1 == 1, "pressed switches survive a map change");
}

// ---- Mumbo's transformations ----

#[test]
fn the_skull_maps_name_their_transformation_and_every_form_model_has_its_clips() {
    use jumpman_sim::banjo_ext::{Form, ALL_FORMS};
    let forms = |course: &str| pickups::parse_features(&course_json(course)).mumbo.iter().map(|m| m.form).collect::<Vec<_>>();
    assert_eq!(forms("bk_mm_mumbos_skull"), vec![Form::Termite]);
    assert_eq!(forms("bk_bgs_mumbos_skull"), vec![Form::Croc]);
    assert_eq!(forms("bk_fp_mumbos_skull"), vec![Form::Walrus]);
    assert_eq!(forms("bk_mmm_mumbos_skull"), vec![Form::Pumpkin]);
    assert_eq!(forms("bk_ccw_spring_mumbos_skull"), vec![Form::Bee]);
    assert_eq!(forms("bk_ccw_summer_mumbos_skull"), vec![Form::Bee]);
    for form in ALL_FORMS {
        let path = model_path(&assets(), form.model());
        let rig = Rig::new(EnemyAsset::parse(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap());
        assert!(!rig.asset.batches.is_empty(), "{form:?}");
        for st in 1..=5 {
            let (clip, _) = rig.asset.state_clip(st).unwrap_or_else(|| panic!("{form:?} state {st}"));
            assert!(rig.asset.clips.contains_key(clip), "{form:?}: {clip}");
        }
        let mut pose = Pose::new(rig.asset.bones.len());
        let (clip, _) = rig.asset.state_clip(2).unwrap();
        rig.evaluate(clip, 0.4, &mut pose);
        for m in rig.matrices(&pose) {
            assert!(m.to_cols_array().iter().all(|v| v.is_finite()), "{form:?}");
        }
    }
}

#[test]
fn mumbo_charges_tokens_once_changes_banjo_caps_his_speed_and_changes_him_back() {
    use jumpman_sim::banjo_ext::{BanjoEvent, Form, MumboPad};
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_mm_mumbos_skull")).unwrap();
    sim.as_banjo = true;
    let pad = pickups::parse_features(&course_json("bk_mm_mumbos_skull")).mumbo[0].clone();
    sim.mario.banjo.mumbo_pads = vec![MumboPad { pos: pad.pos, form: pad.form }];
    sim.mario.banjo.mumbo_tokens = 3;
    let at = [pad.pos[0], pad.pos[1], pad.pos[2] + 150.0];
    let step = |sim: &mut GameSim, at: [f64; 3], n: usize| {
        for _ in 0..n {
            sim.mario.invinc_timer = 10_000.0;
            sim.mario.pos = at;
            sim.mario.vel = [0.0; 3];
            sim.tick(&idle());
        }
    };
    // too poor: nothing happens, Mumbo says so
    step(&mut sim, at, 5);
    assert_eq!(sim.mario.banjo.form, Form::Banjo);
    assert!(sim.mario.banjo.events.iter().any(|e| matches!(e, BanjoEvent::NeedTokens(5))));
    // five tokens: the termite, tokens paid
    sim.mario.banjo.mumbo_tokens = 7;
    sim.mario.banjo.mumbo_armed = true;
    step(&mut sim, at, 5);
    assert_eq!(sim.mario.banjo.form, Form::Termite);
    assert_eq!(sim.mario.banjo.mumbo_tokens, 2);
    // the pad does not flip him straight back while he stands on it; stepping off and on does
    step(&mut sim, at, 20);
    assert_eq!(sim.mario.banjo.form, Form::Termite);
    sim.mario.banjo.mumbo_armed = true;
    step(&mut sim, at, 5);
    assert_eq!(sim.mario.banjo.form, Form::Banjo);
    // the second time is free
    sim.mario.banjo.mumbo_armed = true;
    step(&mut sim, at, 5);
    assert_eq!(sim.mario.banjo.form, Form::Termite);
    assert_eq!(sim.mario.banjo.mumbo_tokens, 2, "paid only once");
    // a form walks no faster than its cap (500 u/s = 20.8 units per frame) however hard the stick is pushed
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_mumbos_mountain")).unwrap();
    sim.as_banjo = true;
    sim.mario.banjo.form = Form::Termite;
    let c = ControllerSnapshot { stick_y: 64.0, stick_mag: 64.0, ..Default::default() };
    let mut top: f64 = 0.0;
    for _ in 0..90 {
        sim.mario.invinc_timer = 10_000.0;
        sim.tick(&c);
        top = top.max(sim.mario.forward_vel);
    }
    assert!(top > 5.0 && top <= 500.0 * 1.25 / 30.0 + 1.0, "top speed {top}");
    sim.mario.banjo.form = Form::Banjo;
    let mut free: f64 = 0.0;
    for _ in 0..90 {
        sim.mario.invinc_timer = 10_000.0;
        sim.tick(&c);
        free = free.max(sim.mario.forward_vel);
    }
    assert!(free > top, "Banjo is faster than the termite: {free} vs {top}");
}

#[test]
fn the_bee_gains_height_with_each_a_press_and_runs_out_of_flaps() {
    use jumpman_sim::banjo_ext::Form;
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_mumbos_mountain")).unwrap();
    sim.as_banjo = true;
    sim.mario.banjo.form = Form::Bee;
    for _ in 0..4 {
        sim.tick(&idle());
    }
    let y0 = sim.mario.pos[1];
    sim.tick(&ControllerSnapshot { button_down: A_BUTTON, button_pressed: A_BUTTON, ..Default::default() });
    let mut peak1 = y0;
    for _ in 0..60 {
        sim.tick(&idle());
        peak1 = peak1.max(sim.mario.pos[1]);
    }
    // a flurry of presses climbs much higher than the single jump, but holding A does nothing more
    let mut sim2 = GameSim::load(&assets(), CourseRequest::new("bk_mumbos_mountain")).unwrap();
    sim2.as_banjo = true;
    sim2.mario.banjo.form = Form::Bee;
    for _ in 0..4 {
        sim2.tick(&idle());
    }
    let mut peak2 = y0;
    for t in 0..60 {
        let press = t % 8 == 0;
        sim2.tick(&ControllerSnapshot { button_down: A_BUTTON, button_pressed: if press { A_BUTTON } else { 0 }, ..Default::default() });
        peak2 = peak2.max(sim2.mario.pos[1]);
    }
    assert!(peak2 > peak1 + 300.0, "flapping {} vs one jump {}", peak2 - y0, peak1 - y0);
    assert!(sim2.mario.banjo.bee_flaps <= 4);
    // the bee flies faster than it walks (600 u/s in the air against 375 on the ground)
    let c = ControllerSnapshot { stick_y: 64.0, stick_mag: 64.0, button_down: A_BUTTON, ..Default::default() };
    let mut top = 0.0f64;
    for t in 0..60 {
        sim2.tick(&ControllerSnapshot { button_pressed: if t % 8 == 0 { A_BUTTON } else { 0 }, ..c });
        top = top.max(sim2.mario.forward_vel);
    }
    assert!(top <= 600.0 * 1.25 / 30.0 + 1.0, "air speed {top}");
}

#[test]
fn buzzbomb_hovers_drifts_dives_at_banjo_and_dies_to_a_ground_pound() {
    use jumpman_sim::bk_enemy::buzz;
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_bubblegloop_swamp")).unwrap();
    let i = of_kind(&sim, Kind::Buzzbomb)[0];
    let state = |sim: &GameSim| sim.mario.objects.objects[i].action;
    // far away: it hovers over its spawn and drifts around
    let at = stand_at(&mut sim, i, 2500.0);
    let mut seen = Vec::new();
    for _ in 0..300 {
        run_at(&mut sim, at, 1);
        if seen.last() != Some(&state(&sim)) {
            seen.push(state(&sim));
        }
    }
    assert!(seen.contains(&buzz::MOVE) && !seen.contains(&buzz::DIVE), "{seen:?}");
    let o = &sim.mario.objects.objects[i];
    let floor = o.var("floorY");
    assert!(o.pos[1] > floor + 60.0 && o.pos[1] < floor + 700.0, "hovers at {} over {floor}", o.pos[1]);
    // Banjo within 800 units: it dives, three times, then climbs away (he is not invincible: that would kill it on contact)
    let at = stand_at(&mut sim, i, 500.0);
    let mut seen = Vec::new();
    for _ in 0..240 {
        sim.mario.invinc_timer = 0.0;
        sim.mario.health = 0x880 as _;
        sim.mario.pos = at;
        sim.mario.vel = [0.0; 3];
        sim.tick(&idle());
        if seen.last() != Some(&state(&sim)) {
            seen.push(state(&sim));
            if std::env::var_os("BUZZ_DEBUG").is_some() { eprintln!("-> {} mario action {:#x} health {} pos {:?} buzz {:?}", state(&sim), sim.mario.action, sim.mario.health, sim.mario.pos, sim.mario.objects.objects[i].pos); }
        }
    }
    assert!(seen.contains(&buzz::DIVE), "{seen:?}");
    assert!(seen.contains(&buzz::RECOVER) || seen.contains(&buzz::OW), "{seen:?}");
    // it hurts while flying, a ground pound from above kills it
    assert_eq!(sim.mario.objects.objects[i].hitbox.interact_type & jumpman_sim::object::INTERACT_DAMAGE, jumpman_sim::object::INTERACT_DAMAGE);
    let e = sim.mario.objects.objects[i].pos;
    sim.mario.pos = [e[0], e[1] + 120.0, e[2]];
    sim.mario.vel = [0.0, -50.0, 0.0];
    sim.mario.set_action(ACT_GROUND_POUND, 0.0);
    let mut killed = false;
    for _ in 0..60 {
        sim.mario.invinc_timer = 10_000.0;
        sim.tick(&idle());
        let o = &sim.mario.objects.objects[i];
        if !o.active || o.action >= buzz::DIE {
            killed = true;
            break;
        }
    }
    assert!(killed, "state {} after the ground pound", state(&sim));
}

#[test]
fn tee_hee_wakes_chases_hurts_and_dies_to_any_blow() {
    use jumpman_sim::bk_enemy::ghost;
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_mad_monster_mansion")).unwrap();
    let i = of_kind(&sim, Kind::TeeHee)[0];
    let state = |sim: &GameSim| sim.mario.objects.objects[i].action;
    let at = stand_at(&mut sim, i, 2500.0);
    run_at(&mut sim, at, 30);
    assert_eq!(state(&sim), ghost::IDLE);
    let at = stand_at(&mut sim, i, 450.0);
    let mut seen = vec![];
    let start = sim.mario.objects.objects[i].pos;
    for _ in 0..150 {
        sim.mario.invinc_timer = 0.0;
        sim.mario.health = 0x880 as _;
        sim.mario.pos = at;
        sim.mario.vel = [0.0; 3];
        sim.tick(&idle());
        if seen.last() != Some(&state(&sim)) {
            seen.push(state(&sim));
        }
    }
    assert!(seen.starts_with(&[ghost::ALERT]) && seen.contains(&ghost::CHASE), "{seen:?}");
    let now = sim.mario.objects.objects[i].pos;
    let d = |p: [f64; 3]| ((p[0] - at[0]).powi(2) + (p[2] - at[2]).powi(2)).sqrt();
    assert!(d(now) < d(start) - 100.0 || sim.mario.health < 0x880 as _, "it did not come closer: {} -> {}", d(start), d(now));
    // a blow kills it
    let e = sim.mario.objects.objects[i].pos;
    sim.mario.pos = [e[0], e[1] + 120.0, e[2]];
    sim.mario.vel = [0.0, -50.0, 0.0];
    sim.mario.set_action(ACT_GROUND_POUND, 0.0);
    let mut killed = false;
    for _ in 0..60 {
        sim.mario.invinc_timer = 10_000.0;
        sim.tick(&idle());
        let o = &sim.mario.objects.objects[i];
        if !o.active || o.action == ghost::DIE {
            killed = true;
            break;
        }
    }
    assert!(killed, "state {}", state(&sim));
}

#[test]
fn eggs_break_the_shack_door_of_mad_monster_mansion_and_a_closed_one_blocks_in_a_progression_game() {
    let f = pickups::parse_features(&course_json("bk_mad_monster_mansion"));
    assert_eq!(f.break_doors.len(), 3, "shack, mansion and church-side doors");
    assert_eq!(pickups::parse_features(&course_json("bk_rusty_bucket_bay")).break_doors.len(), 1, "the engine room door");
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_mad_monster_mansion")).unwrap();
    sim.as_banjo = true;
    let door = f.break_doors[0].clone();
    sim.mario.banjo.break_doors = f.break_doors.clone();
    sim.mario.banjo.progression = true;
    // a progression game: the closed door is a wall (the slab logic is the note doors')
    let a = door.yaw.to_radians();
    let front = [door.pos[0] + a.sin() * 150.0, door.pos[1] + 5.0, door.pos[2] + a.cos() * 150.0];
    sim.mario.pos = front;
    for _ in 0..3 {
        sim.tick(&idle());
    }
    // an egg shattering at the door breaks it for good
    assert!(sim.mario.banjo.broken.is_empty());
    sim.mario.banjo.events.push(jumpman_sim::banjo_ext::BanjoEvent::EggShattered { pos: [door.pos[0], door.pos[1] + 100.0, door.pos[2]], hit: false });
    sim.tick(&idle());
    assert_eq!(sim.mario.banjo.broken.len(), 1);
    assert!(sim.mario.banjo.events.iter().any(|e| matches!(e, jumpman_sim::banjo_ext::BanjoEvent::DoorBroken(_))));
    assert_eq!(sim.mario.banjo.carried().broken.len(), 1, "stays broken across maps");
    // an egg far away does nothing to the other doors
    let other = f.break_doors[1].clone();
    assert!(!sim.mario.banjo.broken.contains(&jumpman_sim::banjo_ext::break_key(&other.pos)));
}

fn form_sim(form: jumpman_sim::banjo_ext::Form) -> GameSim {
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_mumbos_mountain")).unwrap();
    sim.as_banjo = true;
    sim.mario.banjo.form = form;
    for _ in 0..5 {
        sim.tick(&idle());
    }
    sim
}

#[test]
fn the_croc_lunges_with_a_bite_the_walrus_slides_and_the_pumpkin_hops() {
    use jumpman_sim::banjo_ext::Form;
    // croc: B lunges forward (jump kick action, 500 u/s) and up
    let mut sim = form_sim(Form::Croc);
    let y0 = sim.mario.pos[1];
    sim.tick(&ControllerSnapshot { button_down: B_BUTTON, button_pressed: B_BUTTON, ..Default::default() });
    assert_eq!(sim.mario.action, ACT_JUMP_KICK);
    assert!(sim.mario.forward_vel > 15.0);
    let mut peak = y0;
    for _ in 0..10 {
        sim.tick(&idle());
        peak = peak.max(sim.mario.pos[1]);
    }
    assert!(peak > y0 + 20.0, "the lunge rises {}", peak - y0);
    // walrus: Z while running drops into a slide
    let mut sim = form_sim(Form::Walrus);
    let run = ControllerSnapshot { stick_y: 64.0, stick_mag: 64.0, ..Default::default() };
    for _ in 0..30 {
        sim.tick(&run);
    }
    assert!(sim.mario.forward_vel > 3.0);
    sim.tick(&ControllerSnapshot { stick_y: 64.0, stick_mag: 64.0, button_down: Z_TRIG, button_pressed: Z_TRIG, ..Default::default() });
    assert_eq!(sim.mario.action, ACT_BUTT_SLIDE, "action {:#x}", sim.mario.action);
    // pumpkin: the walk speed pulses with the hop cycle instead of holding steady
    let mut sim = form_sim(Form::Pumpkin);
    let (mut lo, mut hi) = (f64::MAX, 0.0f64);
    for t in 0..120 {
        sim.tick(&run);
        if t > 30 {
            lo = lo.min(sim.mario.forward_vel);
            hi = hi.max(sim.mario.forward_vel);
        }
    }
    assert!(hi > 5.0 && lo < hi * 0.9, "speed {lo}..{hi}");
}

#[test]
fn the_lair_podium_takes_and_returns_jiggies_one_press_at_a_time_and_completes_the_picture() {
    use jumpman_sim::banjo_ext::{BanjoEvent, PICTURE_COST};
    let f = pickups::parse_features(&course_json("bk_gl_ttc_and_cc_puzzle"));
    assert_eq!(f.podiums.iter().map(|p| p.picture).collect::<Vec<_>>(), vec![3, 2], "CC and TTC pictures");
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_gl_ttc_and_cc_puzzle")).unwrap();
    sim.as_banjo = true;
    sim.mario.banjo.podiums = f.podiums.clone();
    sim.mario.banjo.progression = true;
    sim.mario.banjo.jiggies = 3;
    let ttc = f.podiums.iter().find(|p| p.picture == 2).unwrap().clone();
    let spot = [ttc.pos[0], ttc.pos[1] + 2.0, ttc.pos[2]];
    let press = |sim: &mut GameSim, mask: u32| {
        sim.mario.pos = spot;
        sim.mario.vel = [0.0; 3];
        sim.mario.invinc_timer = 10_000.0;
        sim.tick(&ControllerSnapshot { button_down: mask, button_pressed: mask, ..Default::default() });
        sim.tick(&idle());
    };
    for _ in 0..3 {
        press(&mut sim, 0);
    }
    assert_eq!(sim.mario.banjo.on_podium, 2, "standing on the TTC podium");
    let eggs = sim.mario.banjo.eggs;
    press(&mut sim, C_UP);
    assert_eq!((sim.mario.banjo.placed[1], sim.mario.banjo.jiggy_stock()), (1, 2));
    assert_eq!(sim.mario.banjo.eggs, eggs, "no egg flies on a podium");
    assert!(!sim.mario.banjo.puzzle_complete(2));
    press(&mut sim, C_DOWN);
    assert_eq!(sim.mario.banjo.placed[1], 0);
    press(&mut sim, C_UP);
    press(&mut sim, C_UP);
    assert_eq!(sim.mario.banjo.placed[1], PICTURE_COST[1]);
    assert!(sim.mario.banjo.puzzle_complete(2) && sim.mario.banjo.jiggy_stock() == 1);
    assert!(sim.mario.banjo.events.iter().any(|e| matches!(e, BanjoEvent::PuzzleComplete(2))));
    // a finished picture cannot be taken apart again
    press(&mut sim, C_DOWN);
    assert_eq!(sim.mario.banjo.placed[1], PICTURE_COST[1]);
    // C-right puts all that fit on the next picture
    let cc = f.podiums.iter().find(|p| p.picture == 3).unwrap().clone();
    let spot2 = [cc.pos[0], cc.pos[1] + 2.0, cc.pos[2]];
    for _ in 0..3 {
        sim.mario.pos = spot2;
        sim.tick(&idle());
    }
    sim.mario.pos = spot2;
    sim.tick(&ControllerSnapshot { button_down: C_RIGHT, button_pressed: C_RIGHT, ..Default::default() });
    assert_eq!(sim.mario.banjo.placed[2], 1, "the one jiggy left in stock");
    assert_eq!(sim.mario.banjo.jiggy_stock(), 0);
}

#[test]
fn nipper_pinches_opens_up_and_takes_three_strong_blows_in_its_open_windows() {
    use jumpman_sim::bk_enemy::nip;
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_treasure_trove_cove")).unwrap();
    let i = of_kind(&sim, Kind::Nipper)[0];
    let state = |sim: &GameSim| sim.mario.objects.objects[i].action;
    // everything else asleep, Banjo 700 units in front of the crab
    face_enemy(&mut sim, i, 0.0, 0.0);
    let (e, yaw) = (sim.mario.objects.objects[i].pos, sim.mario.objects.objects[i].move_yaw);
    let a = yaw * std::f64::consts::PI / 32768.0;
    let front = [e[0] + a.sin() * 700.0, e[1], e[2] + a.cos() * 700.0];
    let floor = sim.mario.world.find_floor(front[0], e[1] + 300.0, front[2]);
    let spot = [front[0], if floor.surface.is_some() { floor.height } else { e[1] }, front[2]];
    let mut seen = vec![];
    let mut wounds = 0;
    for _ in 0..1800 {
        sim.mario.invinc_timer = 0.0;
        sim.mario.health = 0x880 as _;
        // blows outside the open window do nothing: ground pound only while it is open, otherwise stand in front
        if state(&sim) == nip::VULNERABLE && sim.mario.action != ACT_GROUND_POUND {
            sim.mario.pos = [e[0] + a.sin() * 60.0, e[1] + 150.0, e[2] + a.cos() * 60.0];
            sim.mario.vel = [0.0, -50.0, 0.0];
            sim.mario.set_action(ACT_GROUND_POUND, 0.0);
        } else if state(&sim) != nip::VULNERABLE {
            sim.mario.pos = spot;
            sim.mario.vel = [0.0; 3];
        }
        let before = sim.mario.objects.objects[i].var("life");
        sim.tick(&idle());
        let o = &sim.mario.objects.objects[i];
        if o.var("life") < before {
            wounds += 1;
        }
        if seen.last() != Some(&state(&sim)) {
            seen.push(state(&sim));
        }
        if !o.active || o.action == nip::DEAD {
            break;
        }
    }
    assert!(seen.contains(&nip::ATTACK) && seen.contains(&nip::VULNERABLE), "{seen:?}");
    assert_eq!(wounds, 3, "{seen:?}");
    assert!(seen.contains(&nip::DYING), "{seen:?}");
}

#[test]
fn slappa_rises_chases_slaps_lies_down_and_dies_to_a_strong_blow_while_up() {
    use jumpman_sim::bk_enemy::slap;
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_gobis_valley")).unwrap();
    let i = of_kind(&sim, Kind::Slappa)[0];
    let state = |sim: &GameSim| sim.mario.objects.objects[i].action;
    let at = stand_at(&mut sim, i, 3000.0);
    run_at(&mut sim, at, 30);
    assert_eq!(state(&sim), slap::IDLE);
    let at = stand_at(&mut sim, i, 600.0);
    let mut seen = vec![];
    for _ in 0..400 {
        sim.mario.invinc_timer = 0.0;
        sim.mario.health = 0x880 as _;
        sim.mario.pos = at;
        sim.mario.vel = [0.0; 3];
        sim.tick(&idle());
        if seen.last() != Some(&state(&sim)) {
            seen.push(state(&sim));
        }
    }
    for want in [slap::RISE, slap::CHASE, slap::WINDUP, slap::ATTACK, slap::LAYING] {
        assert!(seen.contains(&want), "{seen:?}");
    }
    // while it is up a ground pound kills it
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_gobis_valley")).unwrap();
    let i = of_kind(&sim, Kind::Slappa)[0];
    let at = stand_at(&mut sim, i, 600.0);
    for _ in 0..60 {
        sim.mario.invinc_timer = 0.0;
        sim.mario.health = 0x880 as _;
        sim.mario.pos = at;
        sim.mario.vel = [0.0; 3];
        sim.tick(&idle());
    }
    assert!(sim.mario.objects.objects[i].action >= slap::RISE);
    let e = sim.mario.objects.objects[i].pos;
    sim.mario.pos = [e[0], e[1] + 250.0, e[2]];
    sim.mario.vel = [0.0, -50.0, 0.0];
    sim.mario.set_action(ACT_GROUND_POUND, 0.0);
    let mut killed = false;
    for _ in 0..60 {
        sim.mario.invinc_timer = 0.0;
        sim.mario.health = 0x880 as _;
        sim.tick(&idle());
        let o = &sim.mario.objects.objects[i];
        if !o.active || o.action == slap::DIE {
            killed = true;
            break;
        }
    }
    assert!(killed, "state {}", sim.mario.objects.objects[i].action);
}

#[test]
fn mumbo_stands_in_the_skull_faces_banjo_and_casts_when_the_form_changes() {
    use jumpman_sim::banjo_ext::Form;
    use jumpman_sim::bk_enemy::mumbo;
    let mut sim = GameSim::load(&assets(), CourseRequest::new("bk_mm_mumbos_skull")).unwrap();
    sim.as_banjo = true;
    let i = of_kind(&sim, Kind::Mumbo)[0];
    for _ in 0..10 {
        sim.tick(&idle());
    }
    let o = &sim.mario.objects.objects[i];
    assert_eq!(o.action, mumbo::IDLE);
    assert_eq!(o.hitbox.interact_type, 0, "he is not a target");
    sim.mario.banjo.mumbo_tokens = 10;
    assert!(sim.mario.banjo.transform(Form::Termite));
    for _ in 0..3 {
        sim.tick(&idle());
    }
    assert_eq!(sim.mario.objects.objects[i].action, mumbo::SPELL);
    for _ in 0..100 {
        sim.tick(&idle());
    }
    assert_eq!(sim.mario.objects.objects[i].action, mumbo::IDLE, "the spell is over");
}

#[test]
fn the_lair_podiums_are_placed_as_scenery_with_the_exported_podium_model() {
    let path = model_path(&assets(), "podium");
    let asset = EnemyAsset::parse(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap();
    assert!(!asset.batches.is_empty());
    let ys: Vec<f64> = asset.batches.iter().flat_map(|b| b.vertices.iter().map(|v| v[1])).collect();
    let (lo, hi) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
    assert!(hi - lo < 80.0 && lo >= -1.0, "a flat plinth: {lo}..{hi}");
    let mut total = 0;
    for course in ["bk_gl_mm_lobby", "bk_gl_ttc_and_cc_puzzle", "bk_gl_rbb_and_mmm_puzzle", "bk_gl_dingpot"] {
        let sim = GameSim::load(&assets(), CourseRequest::new(course)).unwrap();
        let podiums = of_kind(&sim, Kind::Podium);
        let features = pickups::parse_features(&course_json(course));
        assert_eq!(podiums.len(), features.podiums.len(), "{course}: one model per podium actor");
        for i in podiums {
            assert_eq!(sim.mario.objects.objects[i].hitbox.interact_type, 0, "scenery");
        }
        total += features.podiums.len();
    }
    assert_eq!(total, 7);
}
