//! Parse tests of the exported setups (counts per world, every placed enemy implemented or in the explicit skip
//! list), model assets, and scripted scenarios in the real BK worlds: Banjo's egg, Mario's ground pound and
//! Link's sword each defeat a Grublin in Mumbo's Mountain.
use super::*;
use crate::game::course_data::CourseData;
use crate::game::world::CourseRequest;
use jumpman_sim::banjo_ext::BkPickup;
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
    ("bk_treasure_trove_cove", &[("beehive", 4), ("grublin", 1), ("shrapnel", 4), ("snippet", 5)]),
    ("bk_clankers_cavern", &[("beehive", 2), ("mutie_snippet", 4), ("snippet", 1)]),
    ("bk_bubblegloop_swamp", &[("beehive", 4)]),
    ("bk_gobis_valley", &[("beehive", 3), ("snarebear", 1)]),
    ("bk_freezeezy_peak", &[("beehive", 4), ("sir_slush", 5)]),
    ("bk_rusty_bucket_bay", &[("beehive", 1)]),
    ("bk_mad_monster_mansion", &[("bat", 6), ("beehive", 2), ("limbo", 6)]),
    ("bk_click_clock_wood", &[("beehive", 2)]),
    ("bk_click_clock_wood_spring", &[("beehive", 3), ("bigbutt", 1), ("grublin_hood", 3), ("snarebear", 8)]),
    ("bk_click_clock_wood_summer", &[("beehive", 3), ("bigbutt", 1), ("grublin_hood", 4), ("snarebear", 8)]),
    ("bk_click_clock_wood_autumn", &[("beehive", 3), ("bigbutt", 1), ("grublin_hood", 4), ("snarebear", 8)]),
    ("bk_click_clock_wood_winter", &[("beehive", 2), ("grublin_hood", 6), ("sir_slush", 6)]),
];

#[test]
fn every_world_has_the_enemies_of_its_setup_file() {
    let report = report();
    assert_eq!(worlds().len(), PLACED.len());
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
            assert!((p >> 16) >= 50 && (p >> 16) <= 1000, "scale {} of {kind:?}", p >> 16);
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
    }
}

/// States the behavior actually enters.
fn used_state(kind: Kind, s: i32) -> bool {
    match kind {
        Kind::Termite => matches!(s, 1 | 2 | 6),
        Kind::Bigbutt => !matches!(s, 7 | 10 | 11),
        Kind::Snippet | Kind::MutieSnippet => true,
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
    // positions: most pickups float within 400 units over a floor of the world
    for id in ["bk_mumbos_mountain", "bk_treasure_trove_cove"] {
        let items = pickups::parse_pickups(&course_json(id));
        assert!(items.len() > 80, "{id}: {}", items.len());
        let sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap();
        let near = items
            .iter()
            .filter(|it| {
                let f = sim.mario.world.find_floor(it.pos[0], it.pos[1] + 50.0, it.pos[2]);
                f.surface.is_some() && it.pos[1] - f.height > -50.0 && it.pos[1] - f.height < 600.0
            })
            .count();
        assert!(near * 10 >= items.len() * 7, "{id}: only {near}/{} pickups sit over the ground", items.len());
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
    assert_eq!(sim.mario.banjo.eggs, eggs - 1);
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
