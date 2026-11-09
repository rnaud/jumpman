//! The last six SM64 stars (round "last stars": `behaviors/r3`, the Big Chill Bully rules, the literal fire
//! piranha star, the volcano platforms on track): placements of the exporters, models and clips, and the star
//! routes played on the real exported courses, checked against the decomp constants.
//!
//! * TTM act 2 "Mystery of the Monkey Cage": carry the cage Ukiki (DIALOG_079, Yes), he runs to his cage,
//!   DIALOG_080, jumps on it, it falls and breaks: the act 2 star (ukiki.inc.c, ukiki_cage.inc.c).
//! * HMC act 6 "Watch for Rolling Rocks": the boulder generator of room 4 (boulder.inc.c).
//! * LLL act 6 "Elevator Tour In The Volcano": the two checkerboard platforms on track (platform_on_track.inc.c).
//! * SL act 2 "Chill With The Bully", THI act 1 "Pluck the Piranha Flower": `spawn_default_star` event stars.
use super::game2_tests::{assets, ctl};
use super::runner::{GameSim, SimEvent};
use super::world::{CourseRequest, LoadedCourse};
use crate::progress::castle_npcs;
use crate::progress::cutscene_glue::{dialog_pages, mark_choice, step_auto_dialog, AutoDialog};
use crate::progress::data::ProgressData;
use crate::progress::save::{SaveFile, FLAG_CAP_ON_UKIKI};
use crate::progress::stars::{RewardKind, StarDirector};
use crate::render::g1_view::{anim_table, glb_animation_names};
use jumpman_sim::behaviors::r3::{boulder, ukiki};
use jumpman_sim::cutscene_ext::*;
use jumpman_sim::object::ObjectBehaviorKind as K;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::solids::{SolidMesh, Solids};
use jumpman_sim::state::*;
use serde_json::Value;
use std::collections::HashMap;

fn load(id: &str, area: i32, act: i32) -> GameSim {
    let mut req = CourseRequest::new(id);
    req.area = area;
    req.act = act;
    req.g2 = true;
    super::course_data::set_extensions(true);
    let mut sim = GameSim::load(&assets(), req).unwrap_or_else(|e| panic!("{id} area {area}: {e}"));
    // The app applies the save's cap state on load (progress::castle_npcs::apply_cap).
    castle_npcs::init_cap(&mut sim, &SaveFile::in_memory(1));
    sim
}

fn json(id: &str, area: i32) -> Value {
    let name = if area == 1 { format!("{id}.json") } else { format!("{id}.{area}.json") };
    serde_json::from_slice(&std::fs::read(assets().join("courses").join(name)).unwrap()).unwrap()
}

fn idle() -> jumpman_sim::state::ControllerSnapshot {
    ctl(0.0, 0.0, 0, 0)
}

fn find(sim: &GameSim, kind: K) -> Vec<ObjId> {
    (0..sim.mario.objects.objects.len()).filter(|&i| sim.mario.objects.objects[i].behavior == kind).map(|i| ObjId(i as u32)).collect()
}

/// One tick; returns the object events and the cutscene cues (the app's cue pump drains them every tick).
fn tick(sim: &mut GameSim) -> (Vec<ObjectEvent>, Vec<CutCue>) {
    sim.tick(&idle());
    let events = sim.events.iter().filter_map(|e| if let SimEvent::Object(o) = e { Some(o.clone()) } else { None }).collect();
    (events, sim.mario.cut.take_cues())
}

/// Tick until the dialog `cue` opens, then close it (with the Yes / No `response` of a choice box).
fn talk(sim: &mut GameSim, cue: CutCue, response: Option<i32>, max: usize) -> bool {
    for _ in 0..max {
        let (_, cues) = tick(sim);
        if cues.contains(&cue) {
            for _ in 0..3 {
                tick(sim);
            }
            sim.mario.cut.dialog_open = false;
            if let Some(r) = response {
                sim.mario.cut.dialog_response = r;
            }
            return true;
        }
    }
    false
}

fn director(course: &str, area: i32, act: i32, sim: &GameSim) -> StarDirector {
    let data = ProgressData::load(&assets(), course, area);
    StarDirector::new(course, act, &data, sim.course.spawn.pos, &sim.mario.world, &sim.mario, &SaveFile::in_memory(1))
}

/// The event star of `act` rises after `ObjectEvent::DefaultStar` and Mario can collect it as that act.
fn collect_event_star(course: &str, area: i32, act: i32, origin: [f64; 3]) {
    let mut sim = load(course, area, act);
    let mut d = director(course, area, act, &sim);
    assert!(!d.primary().visible, "{course} {act}: no stand-in star for a spawn_default_star act");
    d.on_event(&ObjectEvent::DefaultStar { pos: origin, act });
    let save = SaveFile::in_memory(1);
    sim.mario.pos = [0.0, -20000.0, 0.0];
    for _ in 0..300 {
        d.step(&sim.mario, &save, 0);
    }
    let r = d.rewards.iter().find(|r| r.kind == RewardKind::Event && r.act == act).cloned().expect("event reward");
    assert!(r.visible, "{course} {act}: the event star is out");
    sim.mario.pos = r.pos;
    let got = (0..5).find_map(|_| d.step(&sim.mario, &save, 0).collected);
    assert_eq!(got.map(|c| c.act), Some(act), "{course}: collected as act {act}");
}

// ------------------------------------------------------------------------------------------------ export

#[test]
fn the_exporters_place_ukiki_his_cage_the_boulder_generator_and_the_volcano_lifts() {
    let ttm = json("ttm", 1);
    let objs = ttm["objects"].as_array().unwrap();
    let ukikis: Vec<_> = objs.iter().filter(|o| o["behavior"] == "ukiki").collect();
    assert_eq!(ukikis.len(), 2);
    // ttm/script.c:39-40: the cage Ukiki (BPARAM2 UKIKI_BP_CAGE, act 2) and the cap Ukiki (UKIKI_BP_CAP, all acts).
    assert!(ukikis.iter().any(|o| o["pos"] == serde_json::json!([729, 2307, 335]) && o["param"] == 0 && o["acts"] == serde_json::json!([2])));
    assert!(ukikis.iter().any(|o| o["pos"] == serde_json::json!([1992, -1548, 2944]) && o["param"] == 0x10000));
    let cage = objs.iter().find(|o| o["behavior"] == "ukikiCage").expect("cage");
    assert_eq!(cage["pos"], serde_json::json!([2496, 1670, 1492]));
    assert_eq!(cage["param"], 1 << 24, "BPARAM1(STAR_INDEX_ACT_2)");
    assert!(ttm["extraCollision"].get(ukiki::CAGE_COLLISION).is_some(), "ttm_seg7_collision_ukiki_cage");
    assert_eq!(ttm["eventStars"], serde_json::json!([{"act": 2, "pos": [2500, -1200, 1300]}]));
    for model in ["actors/ukiki/ukiki_geo.glb", "levels/ttm/ttm_geo_000710.glb", boulder::ROLLING_ROCK_MODEL, ukiki::STAR_MODEL] {
        assert!(assets().join(model).is_file(), "{model}");
    }
    let clips = glb_animation_names(&assets().join("actors/ukiki/ukiki_geo.glb"));
    for name in anim_table(K::Ukiki) {
        assert!(clips.iter().any(|c| c == name), "ukiki_geo.glb lacks {name}");
    }
    assert_eq!(anim_table(K::Ukiki).len(), 13);

    let hmc = json("hmc", 1);
    let gen: Vec<_> = hmc["objects"].as_array().unwrap().iter().filter(|o| o["behavior"] == "bigBoulderGenerator").collect();
    assert_eq!(gen.len(), 1);
    assert_eq!(gen[0]["pos"], serde_json::json!([-6093, 3075, -7807]), "hmc/script.c:55");

    let volcano = json("lll", 2);
    let lifts = volcano["solids"].as_array().unwrap();
    assert_eq!(lifts.len(), 2, "lll/script.c:107-108");
    for l in lifts {
        assert_eq!(l["behavior"], "bhvPlatformOnTrack");
        assert_eq!(l["collision"], "checkerboard_platform_seg8_collision_0800D710");
        assert!(l["track"].as_array().unwrap().len() > 10);
    }

    let sl = json("sl", 1);
    let bully = sl["objects"].as_array().unwrap().iter().find(|o| o["behavior"] == "bigChillBully").expect("bigChillBully");
    assert_eq!(bully["pos"], serde_json::json!([315, 1331, -4852]));
    assert_eq!(sl["eventStars"], serde_json::json!([{"act": 2, "pos": [130, 1600, -4335]}]));
    let thi = json("thi", 1);
    assert_eq!(thi["eventStars"], serde_json::json!([{"act": 1, "pos": [-6300, -1850, -6300]}]));
    assert_eq!(thi["objects"].as_array().unwrap().iter().filter(|o| o["behavior"] == "firePiranhaPlant" && o["param"] == 0x10000).count(), 5);
}

#[test]
fn only_act_2_has_the_cage_and_its_ukiki() {
    let sim = load("ttm", 1, 2);
    assert_eq!(find(&sim, K::Ukiki).len(), 2);
    assert_eq!(find(&sim, K::UkikiCage).len(), 1);
    let sim = load("ttm", 1, 1);
    assert_eq!(find(&sim, K::Ukiki).len(), 1, "the cap Ukiki is in every act");
    assert_eq!(find(&sim, K::UkikiCage).len(), 0);
    assert!(sim.mario.flags & MARIO_CAP_ON_HEAD != 0 && sim.mario.flags & MARIO_NORMAL_CAP != 0, "init_mario cap flags");
}

// ------------------------------------------------------------------------------------------------ TTM act 2

#[test]
fn mystery_of_the_monkey_cage_is_won_by_carrying_ukiki_home() {
    jumpman_sim::math::set_fixed_random(Some(0.5));
    let mut sim = load("ttm", 1, 2);
    let u = *find(&sim, K::Ukiki).iter().find(|&&i| sim.mario.objects[i].param == 0.0).expect("cage ukiki");
    for _ in 0..3 {
        tick(&mut sim);
    }
    let home = sim.mario.objects[u].home;
    assert!((home[1] - 2307.0).abs() < 200.0, "DROP_TO_FLOOR from (729, 2307, 335): {home:?}");
    // Mario picks him up at his home on the mountain top.
    let p = sim.mario.objects[u].pos;
    sim.mario.pos = [p[0], p[1], p[2] - 60.0];
    sim.mario.vel = [0.0; 3];
    sim.mario.set_action(ACT_HOLD_IDLE, 0.0);
    sim.mario.used_obj = Some(u);
    sim.mario.grab_used_object();
    assert!(talk(&mut sim, CutCue::DialogWithResponse(ukiki::DIALOG_079), Some(DIALOG_RESPONSE_YES), 60), "DIALOG_079 asks");
    for _ in 0..60 {
        tick(&mut sim);
    }
    assert_eq!(sim.mario.held_obj, None, "Yes: Mario puts him down");
    assert_eq!(sim.mario.objects[u].action, ukiki::UKIKI_ACT_GO_TO_CAGE);
    // He runs sCageUkikiPath down to the cage ledge.
    let mut waiting = false;
    for _ in 0..1500 {
        tick(&mut sim);
        if sim.mario.objects[u].action_state as i32 == ukiki::UKIKI_SUB_ACT_CAGE_WAIT_FOR_MARIO {
            waiting = true;
            break;
        }
    }
    let at = sim.mario.objects[u].pos;
    assert!(waiting, "reached the cage ledge: {at:?}");
    let end = ukiki::CAGE_PATH[10];
    assert!((at[0] - end[0]).hypot(at[2] - end[2]) < 80.0, "at the last waypoint: {at:?}");
    // Mario follows him: DIALOG_080, then the jump, the spin, the fall and the star.
    let floor = sim.mario.world.find_floor(at[0] - 120.0, at[1] + 200.0, at[2]).height;
    sim.mario.pos = [at[0] - 120.0, floor, at[2]];
    sim.mario.vel = [0.0; 3];
    sim.mario.set_action(ACT_IDLE, 0.0);
    assert!(talk(&mut sim, CutCue::Dialog(ukiki::DIALOG_080), None, 120), "DIALOG_080");
    let cage = find(&sim, K::UkikiCage)[0];
    let mut landed = false;
    let mut star = None;
    for _ in 0..1200 {
        let (events, _) = tick(&mut sim);
        landed |= sim.mario.objects[cage].action >= ukiki::UKIKI_CAGE_ACT_SPIN;
        for e in events {
            if let ObjectEvent::DefaultStar { pos, act } = e {
                star = Some((pos, act));
            }
        }
        if star.is_some() {
            break;
        }
    }
    assert!(landed, "Ukiki landed on the cage (ukiki at {:?}, sub {})", sim.mario.objects[u].pos, sim.mario.objects[u].action_state);
    let (origin, act) = star.expect("the broken cage spawns the star");
    assert_eq!(act, 2, "STAR_INDEX_ACT_2");
    assert_eq!(origin, ukiki::CAGE_STAR_POS, "spawn_default_star(2500, -1200, 1300)");
    assert!(sim.mario.objects[cage].pos[1] < 0.0, "the cage fell off the cliff before breaking: {:?}", sim.mario.objects[cage].pos);
    assert!(!sim.mario.objects[cage].visible, "UKIKI_CAGE_ACT_HIDE");
    jumpman_sim::math::set_fixed_random(None);
    collect_event_star("ttm", 1, 2, origin);
}

#[test]
fn the_cap_ukiki_takes_the_cap_into_the_save_and_mario_respawns_without_it() {
    let mut sim = load("ttm", 1, 1);
    let mut save = SaveFile::in_memory(1);
    let u = find(&sim, K::Ukiki)[0];
    tick(&mut sim);
    let p = sim.mario.objects[u].pos;
    sim.mario.pos = [p[0], p[1], p[2] - 60.0];
    sim.mario.vel = [0.0; 3];
    sim.mario.set_action(ACT_HOLD_IDLE, 0.0);
    sim.mario.used_obj = Some(u);
    sim.mario.grab_used_object();
    assert!(talk(&mut sim, CutCue::Dialog(ukiki::DIALOG_100), None, 60), "DIALOG_100");
    castle_npcs::step(&mut sim, &mut save);
    assert!(save.has_flag(FLAG_CAP_ON_UKIKI), "SAVE_FLAG_CAP_ON_UKIKI");
    // Next visit: Mario starts capless and the cap Ukiki still wears the cap (bhv_ukiki_init).
    let mut sim = load("ttm", 1, 1);
    sim.mario.cut.spawn_with_cap = false;
    sim.mario.state.flags = 0;
    castle_npcs::init_cap(&mut sim, &save);
    assert_eq!(sim.mario.flags & (MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD), 0);
    tick(&mut sim);
    let u = find(&sim, K::Ukiki)[0];
    assert_eq!(sim.mario.objects[u].var("textState"), ukiki::UKIKI_TEXT_HAS_CAP);
    assert_eq!(sim.mario.objects[u].var("animState"), ukiki::UKIKI_ANIM_STATE_CAP_ON);
}

// ------------------------------------------------------------------------------------------------ the dialog box

#[test]
fn the_dialog_box_offers_the_yes_no_choice_of_dialog_079() {
    let pages = dialog_pages(ukiki::DIALOG_079);
    let last = pages.last().unwrap();
    assert!(mark_choice(last, DIALOG_RESPONSE_YES).ends_with("\u{2605} Free him     Hold on"), "{}", mark_choice(last, DIALOG_RESPONSE_YES));
    assert!(mark_choice(last, DIALOG_RESPONSE_NO).ends_with("  Free him   \u{2605} Hold on"));
    let mut dlg = AutoDialog { id: ukiki::DIALOG_079, pages: pages.clone(), page: 0, choice: Some(DIALOG_RESPONSE_YES) };
    let mut cut = CutExt { dialog_open: true, ..CutExt::default() };
    for _ in 0..pages.len() - 1 {
        step_auto_dialog(&mut dlg, &mut cut, 64.0, A_BUTTON); // the stick only counts on the last page
    }
    assert_eq!(dlg.choice, Some(DIALOG_RESPONSE_YES));
    step_auto_dialog(&mut dlg, &mut cut, 64.0, 0);
    assert_eq!(dlg.choice, Some(DIALOG_RESPONSE_NO), "stick right picks the second option");
    assert!(cut.dialog_open && cut.dialog_response == DIALOG_RESPONSE_NONE);
    step_auto_dialog(&mut dlg, &mut cut, 0.0, B_BUTTON);
    assert!(!cut.dialog_open);
    assert_eq!(cut.dialog_response, DIALOG_RESPONSE_NO, "gDialogResponse = gMenuLineNum");
}

// ------------------------------------------------------------------------------------------------ HMC act 6

#[test]
fn watch_for_rolling_rocks_the_generator_rolls_boulders_in_room_4() {
    jumpman_sim::math::set_fixed_random(Some(0.5));
    let mut sim = load("hmc", 1, 6);
    let g = find(&sim, K::BigBoulderGenerator)[0];
    let gp = sim.mario.objects[g].pos;
    // A floor of room 4 between 1500 and 6000 away from the generator.
    let spot = sim
        .mario
        .world
        .surfaces()
        .iter()
        .filter(|s| s.room == Some(boulder::BOULDER_ROOM) && s.normal[1] > 0.9)
        .map(|s| [(s.v1[0] + s.v2[0] + s.v3[0]) / 3.0, (s.v1[1] + s.v2[1] + s.v3[1]) / 3.0, (s.v1[2] + s.v2[2] + s.v3[2]) / 3.0])
        .find(|c| {
            let d = ((c[0] - gp[0]).powi(2) + (c[1] - gp[1]).powi(2) + (c[2] - gp[2]).powi(2)).sqrt();
            (2000.0..5500.0).contains(&d)
        })
        .expect("a room 4 floor near the generator");
    let mut seen = 0;
    let mut rolled = 0.0f64;
    for _ in 0..260 {
        sim.mario.pos = [spot[0], spot[1] + 10.0, spot[2]];
        sim.mario.vel = [0.0; 3];
        sim.mario.invinc_timer = 30.0;
        tick(&mut sim);
        for o in sim.mario.objects.objects.iter().filter(|o| o.behavior == K::BigBoulder && o.active) {
            rolled = rolled.max(((o.pos[0] - gp[0]).powi(2) + (o.pos[2] - gp[2]).powi(2)).sqrt());
        }
        seen = seen.max(find(&sim, K::BigBoulder).len());
    }
    assert!(seen >= 3, "a boulder every 64 frames: {seen}");
    assert!(rolled > 500.0, "the boulders roll down the slope: {rolled}");
    jumpman_sim::math::set_fixed_random(None);
}

// ------------------------------------------------------------------------------------------------ LLL act 6

#[test]
fn the_volcano_lifts_carry_mario_along_their_tracks() {
    let mut req = CourseRequest::new("lll");
    req.area = 2;
    let course = LoadedCourse::load(&assets(), req).unwrap();
    let mut m = jumpman_sim::mario_runtime::MarioRuntime::new(course.world.clone(), course.spawn.clone(), course.death_height);
    let meshes: HashMap<String, SolidMesh> = course
        .extras
        .extra_collision
        .iter()
        .map(|(k, c)| (k.clone(), SolidMesh { vertices: c.vertices.clone(), triangles: c.triangles.clone() }))
        .collect();
    let mut solids = Solids::new(&course.extras.solids, &meshes, 0);
    assert_eq!(solids.list.len(), 2);
    assert_eq!(solids.unported(), 0);
    solids.install(&mut m);
    for _ in 0..2 {
        solids.step(&mut m);
    }
    // PLATFORM_ON_TRACK_TYPE_CHECKERED: drawn and collided twice as tall.
    assert_eq!(solids.list[0].o.scale3, [1.0, 2.0, 1.0]);
    let start = solids.list[0].o.pos;
    let top = m.world.find_floor(start[0], start[1] + 300.0, start[2]);
    assert!(top.height > start[1], "the lift has collision: floor {} at lift {}", top.height, start[1]);
    m.pos = [start[0], top.height, start[2]];
    m.set_action(ACT_IDLE, 0.0);
    for _ in 0..200 {
        m.step(&idle(), 0.0, false, false);
        solids.step(&mut m);
    }
    let now = solids.list[0].o.pos;
    let moved = ((now[0] - start[0]).powi(2) + (now[1] - start[1]).powi(2) + (now[2] - start[2]).powi(2)).sqrt();
    assert!(moved > 1000.0, "rides 10 units per frame along lll_seg7_trajectory_0702856C: {moved}");
    let carried = ((m.pos[0] - now[0]).powi(2) + (m.pos[2] - now[2]).powi(2)).sqrt();
    assert!(carried < 200.0, "Mario stays on the lift: mario {:?} lift {now:?}", m.pos);
}

// ------------------------------------------------------------------------------------------------ SL act 2, THI act 1

#[test]
fn the_big_chill_bully_falling_off_the_ice_pays_the_act_2_star() {
    let mut sim = load("sl", 1, 2);
    let b = find(&sim, K::BigChillBully)[0];
    tick(&mut sim);
    let mut p = sim.mario.objects[b].pos;
    p[1] = 1000.0; // knocked off the ice: below y 1030 (bully_step)
    sim.mario.objects[b].pos = p;
    let mut star = None;
    for _ in 0..5 {
        let (events, _) = tick(&mut sim);
        for e in events {
            if let ObjectEvent::DefaultStar { pos, act } = e {
                star = Some((pos, act));
            }
        }
    }
    let (origin, act) = star.expect("spawn_default_star");
    assert_eq!(act, 2);
    collect_event_star("sl", 1, 2, origin);
}

#[test]
fn the_fifth_fire_piranha_star_is_the_thi_act_1_star() {
    let sim = load("thi", 1, 1);
    let plants = find(&sim, K::FirePiranhaPlant);
    assert_eq!(plants.len(), 5);
    for p in plants {
        assert_eq!(((sim.mario.objects[p].param as u32) >> 24) + 1, 1, "BPARAM1(STAR_INDEX_ACT_1)");
    }
    collect_event_star("thi", 1, 1, [-6865.0, -2047.0, -4568.0]);
}
