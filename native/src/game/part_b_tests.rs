//! SM64 round 2 part B (castle Toads, Mips, blue coins, ! boxes, 1-ups, Koopa the Quick in THI, the PSS
//! timer): scripted scenarios on the real exported courses, checked against the decomp constants.
use super::game2_tests::{assets, ctl};
use super::runner::{GameSim, SimEvent};
use super::world::CourseRequest;
use crate::progress::castle_npcs;
use crate::progress::data::ProgressData;
use crate::progress::save::SaveFile;
use crate::progress::stars::StarDirector;
use jumpman_sim::behaviors::part_b::{pss, toad};
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::*;
use serde_json::Value;

fn load(id: &str, area: i32, act: i32) -> GameSim {
    let mut req = CourseRequest::new(id);
    req.area = area;
    req.act = act;
    req.g2 = true;
    super::course_data::set_extensions(true);
    GameSim::load(&assets(), req).unwrap_or_else(|e| panic!("{id} area {area}: {e}"))
}

fn json(id: &str, area: i32) -> Value {
    let name = if area == 1 { format!("{id}.json") } else { format!("{id}.{area}.json") };
    serde_json::from_slice(&std::fs::read(assets().join("courses").join(name)).unwrap()).unwrap()
}

fn idle() -> ControllerSnapshot {
    ctl(0.0, 0.0, 0, 0)
}

fn find(sim: &GameSim, kind: ObjectBehaviorKind) -> Vec<ObjId> {
    (0..sim.mario.objects.objects.len()).filter(|&i| sim.mario.objects.objects[i].behavior == kind).map(|i| ObjId(i as u32)).collect()
}

/// Run `n` idle ticks, returning every object event seen.
fn run(sim: &mut GameSim, n: usize) -> Vec<ObjectEvent> {
    let mut seen = vec![];
    for _ in 0..n {
        sim.tick(&idle());
        for e in &sim.events {
            if let SimEvent::Object(o) = e {
                seen.push(o.clone());
            }
        }
    }
    seen
}

fn box_star_acts(events: &[ObjectEvent]) -> Vec<i32> {
    events.iter().filter_map(|e| if let ObjectEvent::BoxStar { act, .. } = e { Some(*act as i32) } else { None }).collect()
}

/// A save with `n` stars (bob, wf, jrb, ccm ... acts).
fn save_with(n: usize) -> SaveFile {
    let mut s = SaveFile::in_memory(1);
    let mut left = n;
    for course in ["bob", "wf", "jrb", "ccm", "bbh", "hmc", "lll", "ssl", "ddd", "sl", "wdw", "ttm", "thi", "ttc", "rr"] {
        for act in 1..=6 {
            if left > 0 {
                s.collect_star(course, act, 0);
                left -= 1;
            }
        }
    }
    assert_eq!(s.stars() as usize, n);
    s
}

// ----------------------------------------------------------------------------------------------- export

#[test]
fn the_exporter_places_every_blue_coin_and_box_of_the_decomp_macros() {
    // (course json, hiddenBlueCoin, movingBlueCoin) counted from levels/*/areas/*/macro.inc.c.
    for (id, area, hidden, moving) in [
        ("wf", 1, 4, 0), ("jrb", 1, 6, 0), ("ccm", 1, 2, 0), ("ccm", 2, 0, 1), ("bbh", 1, 4, 0), ("hmc", 1, 7, 0), ("ssl", 2, 3, 0),
        ("ddd", 2, 6, 0), ("wdw", 1, 6, 0), ("ttm", 3, 0, 3), ("thi", 3, 2, 0), ("ttc", 1, 7, 0), ("rr", 1, 6, 0), ("pss", 1, 6, 0),
    ] {
        let d = json(id, area);
        let count = |k: &str| d["objects"].as_array().unwrap().iter().filter(|o| o["behavior"] == k).count();
        assert_eq!(count("hiddenBlueCoin"), hidden, "{id}.{area} hidden blue coins");
        assert_eq!(count("movingBlueCoin"), moving, "{id}.{area} moving blue coins");
        if hidden > 0 {
            assert_eq!(count("blueCoinSwitch"), 1, "{id}.{area} switch");
        }
    }
    // Every box macro of the decomp: 14 x 3 coins, 10 x 10 coins, 15 x 1-up, 14 x running-away 1-up, 2 x shell.
    let mut counts = std::collections::HashMap::<i64, usize>::new();
    for entry in std::fs::read_dir(assets().join("courses")).unwrap() {
        let path = entry.unwrap().path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.ends_with(".json") || name.contains(".ext.") || name.starts_with(['b', 'o', 'm', 'z']) && name.starts_with("bk_") {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()) else { continue };
        for o in v["objects"].as_array().into_iter().flatten().filter(|o| o["behavior"] == "exclamationBox") {
            *counts.entry(o["param"].as_i64().unwrap()).or_default() += 1;
        }
    }
    assert_eq!(counts.get(&5), Some(&14), "box_three_coins");
    assert_eq!(counts.get(&6), Some(&10), "box_ten_coins");
    assert_eq!(counts.get(&7), Some(&15), "box_1up");
    assert_eq!(counts.get(&9), Some(&14), "box_1up_running_away");
    assert_eq!(counts.get(&3), Some(&2), "box_koopa_shell");
}

// ------------------------------------------------------------------------------------- blue coin switch

/// bhv_blue_coin_switch_loop + bhv_hidden_blue_coin_loop: ground-pound the switch, the coins appear for
/// 240 frames, each is worth 5 coins, the switch recedes 20 per frame for 6 frames and ticks.
#[test]
fn ground_pounding_the_blue_switch_reveals_its_coins_worth_five() {
    let mut sim = load("wf", 1, 1);
    let sw = find(&sim, ObjectBehaviorKind::BlueCoinSwitch)[0];
    let coins = find(&sim, ObjectBehaviorKind::HiddenBlueCoin);
    assert_eq!(coins.len(), 4);
    let home = sim.mario.objects[sw].pos;
    sim.mario.pos = [home[0], home[1] + 300.0, home[2] + 200.0];
    run(&mut sim, 3);
    // Hidden and intangible before the switch is pressed.
    assert!(coins.iter().all(|&c| !sim.mario.objects[c].visible && sim.mario.objects[c].action == 1));
    // The 3x scaled collision top is 26 * 3 above the base.
    let top = sim.mario.world.find_floor(home[0], home[1] + 200.0, home[2]);
    assert!((top.height - (home[1] + 78.0)).abs() < 1.0, "switch top {}", top.height);
    sim.mario.pos = [home[0], top.height, home[2]];
    sim.mario.vel = [0.0; 3];
    sim.mario.action = ACT_GROUND_POUND_LAND;
    run(&mut sim, 1);
    assert_eq!(sim.mario.objects[sw].action, 1, "RECEDING");
    assert_eq!(sim.mario.objects[sw].vel[1], -20.0);
    run(&mut sim, 7);
    assert_eq!(sim.mario.objects[sw].action, 2, "TICKING");
    assert!(!sim.mario.objects[sw].visible);
    run(&mut sim, 1); // the coins notice the ticking switch one object pass later
    assert!(coins.iter().all(|&c| sim.mario.objects[c].visible && sim.mario.objects[c].action == 2), "coins are out");
    // Touch the first coin: +5 coins.
    let before = sim.coins;
    let p = sim.mario.objects[coins[0]].pos;
    sim.mario.pos = p;
    sim.mario.set_action(ACT_FREEFALL, 0.0);
    run(&mut sim, 1);
    assert_eq!(sim.coins, before + 5);
    assert!(!sim.mario.objects[coins[0]].active);
    // Stay away: the other three blink out after 200 + 2 * 21 frames and the switch goes with them.
    sim.mario.pos = [home[0], home[1] + 3000.0, home[2] + 3000.0];
    sim.mario.set_action(ACT_FREEFALL, 0.0);
    for _ in 0..260 {
        sim.mario.pos = [home[0], home[1] + 3000.0, home[2] + 3000.0];
        sim.mario.vel = [0.0; 3];
        run(&mut sim, 1);
    }
    assert!(coins.iter().all(|&c| !sim.mario.objects[c].active), "all coins are gone");
    assert!(!sim.mario.objects[sw].active, "the switch is deleted with the last coin");
}

/// The four coins plus the 100-coin supply: a blue coin run adds 20 of the 100.
#[test]
fn collecting_every_wf_blue_coin_adds_twenty() {
    let mut sim = load("wf", 1, 1);
    let sw = find(&sim, ObjectBehaviorKind::BlueCoinSwitch)[0];
    let home = sim.mario.objects[sw].pos;
    sim.mario.pos = [home[0], home[1] + 300.0, home[2] + 100.0];
    run(&mut sim, 3);
    let top = sim.mario.world.find_floor(home[0], home[1] + 200.0, home[2]).height;
    sim.mario.pos = [home[0], top, home[2]];
    sim.mario.action = ACT_GROUND_POUND_LAND;
    run(&mut sim, 9);
    let before = sim.coins;
    for c in find(&sim, ObjectBehaviorKind::HiddenBlueCoin) {
        sim.mario.pos = sim.mario.objects[c].pos;
        sim.mario.vel = [0.0; 3];
        sim.mario.set_action(ACT_FREEFALL, 0.0);
        run(&mut sim, 1);
    }
    assert_eq!(sim.coins - before, 20);
}

// ------------------------------------------------------------------------------------ moving blue coin

#[test]
fn a_moving_blue_coin_wakes_near_mario_and_slides_down_the_slide() {
    let mut sim = load("ttm", 3, 1);
    let coins = find(&sim, ObjectBehaviorKind::MovingBlueCoin);
    assert_eq!(coins.len(), 3);
    let c = coins[0];
    assert_eq!(sim.mario.objects[c].action, 0, "STILL");
    let p = sim.mario.objects[c].pos;
    sim.mario.pos = [p[0], p[1] + 800.0, p[2] + 800.0];
    run(&mut sim, 3);
    assert_eq!(sim.mario.objects[c].action, 1, "MOVING once Mario is within 1500");
    assert_eq!(sim.mario.objects[c].gravity, 5.0);
    assert_eq!(sim.mario.objects[c].hitbox.damage_or_coin_value, 5.0);
    // Touching it pays 5.
    let before = sim.coins;
    sim.mario.pos = sim.mario.objects[c].pos;
    sim.mario.set_action(ACT_FREEFALL, 0.0);
    run(&mut sim, 1);
    assert_eq!(sim.coins, before + 5);
}

// ------------------------------------------------------------------------------------------ ! boxes

/// Find a course area that has an exclamation box with this content id.
fn box_course(param: i64) -> (String, i32) {
    let mut dirs: Vec<_> = std::fs::read_dir(assets().join("courses")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    dirs.sort();
    for name in dirs {
        if !name.ends_with(".json") || name.contains(".ext.") || name.starts_with("bk_") {
            continue;
        }
        let stem = name.trim_end_matches(".json");
        let (id, area) = match stem.split_once('.') {
            Some((id, a)) => (id.to_string(), a.parse::<i32>().unwrap()),
            None => (stem.to_string(), 1),
        };
        let Ok(v) = serde_json::from_slice::<Value>(&std::fs::read(assets().join("courses").join(&name)).unwrap()) else { continue };
        if v["objects"].as_array().into_iter().flatten().any(|o| o["behavior"] == "exclamationBox" && o["param"].as_i64() == Some(param)) {
            return (id, area);
        }
    }
    panic!("no box with contents {param}");
}

fn break_box(param: i64) -> (GameSim, Vec<ObjectBehaviorKind>) {
    let (id, area) = box_course(param);
    let mut sim = load(&id, area, 1);
    let b = find(&sim, ObjectBehaviorKind::ExclamationBox).into_iter().find(|&b| sim.mario.objects[b].param as i64 == param).unwrap();
    let before = sim.mario.objects.objects.len();
    let p = sim.mario.objects[b].pos;
    sim.mario.pos = [p[0], p[1] + 200.0, p[2] + 300.0];
    sim.mario.objects[b].interact_status |= INT_STATUS_WAS_ATTACKED;
    run(&mut sim, 12);
    assert!(!sim.mario.objects[b].active, "the box is gone after the 8 frame recoil");
    // Only what came out of this box: other actors in the course (Mr. I's body parts, particles...) also spawn objects
    // during these frames, far from the box.
    let near = |q: [f64; 3]| ((q[0] - p[0]).powi(2) + (q[2] - p[2]).powi(2)).sqrt() < 600.0;
    let spawned = sim.mario.objects.objects[before..].iter().filter(|o| near(o.home) || near(o.pos)).map(|o| o.behavior).collect();
    (sim, spawned)
}

#[test]
fn exclamation_boxes_release_their_contents() {
    use ObjectBehaviorKind as K;
    let (_, s) = break_box(5);
    assert_eq!(s.iter().filter(|k| **k == K::MovingYellowCoin).count(), 3, "box_three_coins");
    let (_, s) = break_box(6);
    assert_eq!(s.iter().filter(|k| **k == K::MovingYellowCoin).count(), 10, "box_ten_coins");
    let (_, s) = break_box(7);
    assert_eq!(s, vec![K::OneUpWalking], "box_1up");
    let (_, s) = break_box(9);
    assert_eq!(s, vec![K::OneUpRunning], "box_1up_running_away");
    let (_, s) = break_box(3);
    assert_eq!(s, vec![K::KoopaShell], "box_koopa_shell");
}

// -------------------------------------------------------------------------------------------- 1-ups

#[test]
fn the_walking_one_up_pops_out_then_walks_and_gives_a_life() {
    let (mut sim, _) = break_box(7);
    let one_up = find(&sim, ObjectBehaviorKind::OneUpWalking)[0];
    // Intangible during the 37 frame pop-out, tangible afterwards (action 1, forward velocity 2).
    sim.mario.pos = [sim.mario.objects[one_up].pos[0], sim.mario.objects[one_up].pos[1] + 5000.0, 0.0];
    run(&mut sim, 40);
    assert_eq!(sim.mario.objects[one_up].action, 1);
    let lives = sim.mario.lives;
    let p = sim.mario.objects[one_up].pos;
    sim.mario.pos = [p[0], p[1], p[2]];
    sim.mario.set_action(ACT_FREEFALL, 0.0);
    let ev = run(&mut sim, 1);
    assert_eq!(sim.mario.lives, lives + 1.0);
    assert!(ev.iter().any(|e| matches!(e, ObjectEvent::OneUp { .. })));
}

#[test]
fn hidden_one_up_appears_after_its_triggers() {
    let mut sim = load("castle_grounds", 1, 1);
    let one_up = find(&sim, ObjectBehaviorKind::Hidden1Up)[0];
    let triggers = find(&sim, ObjectBehaviorKind::Hidden1UpTrigger);
    assert_eq!(sim.mario.objects[one_up].param as usize, triggers.len(), "BPARAM2 = number of triggers");
    run(&mut sim, 2);
    assert_eq!(sim.mario.objects[one_up].action, 0);
    assert!(!sim.mario.objects[one_up].visible);
    for t in triggers {
        sim.mario.pos = sim.mario.objects[t].pos;
        sim.mario.set_action(ACT_FREEFALL, 0.0);
        run(&mut sim, 1);
    }
    run(&mut sim, 1);
    assert_eq!(sim.mario.objects[one_up].action, 3, "pop-out");
    assert!(sim.mario.objects[one_up].visible);
}

// ------------------------------------------------------------------------------------------- hearts

#[test]
fn a_recovery_heart_heals_four_per_full_turn() {
    let mut sim = load("bob", 1, 1);
    let h = find(&sim, ObjectBehaviorKind::RecoveryHeart)[0];
    sim.mario.pos = sim.mario.objects[h].pos;
    sim.mario.set_action(ACT_FREEFALL, 0.0);
    sim.mario.heal_counter = 0.0;
    // 0x10000 / (200 * 0 + 1000) = about 66 frames for the first heal while standing still.
    let mut healed_at = None;
    for t in 0..100 {
        sim.mario.pos = sim.mario.objects[h].pos;
        sim.mario.vel = [0.0; 3];
        sim.mario.heal_counter = sim.mario.heal_counter.max(0.0);
        let before = sim.mario.heal_counter;
        run(&mut sim, 1);
        if sim.mario.heal_counter > before {
            healed_at = Some(t);
            break;
        }
    }
    let t = healed_at.expect("the heart healed");
    assert!((60..=70).contains(&t), "first heal after {t} frames");
}

// ------------------------------------------------------------------------------------------- PSS

fn timer_floor(sim: &GameSim, kind: i32) -> Vec3 {
    let s = sim.mario.world.surfaces().iter().find(|s| s.kind == kind).expect("timer surface");
    let (a, b, c) = (s.v1, s.v2, s.v3);
    [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0 + 1.0, (a[2] + b[2] + c[2]) / 3.0]
}

fn pss_run(frames: usize) -> Vec<ObjectEvent> {
    let mut sim = load("pss", 1, 1);
    let start = timer_floor(&sim, pss::SURFACE_TIMER_START);
    let end = timer_floor(&sim, pss::SURFACE_TIMER_END);
    let timer = find(&sim, ObjectBehaviorKind::PssTimer)[0];
    let mut seen = vec![];
    // Mario's floor is the one of the last Mario step, so the end line is held for two ticks.
    let total = frames + 4;
    for t in 0..total {
        sim.mario.pos = if t < 2 { start } else if t + 2 < total { [start[0], start[1] + 200.0, start[2]] } else { end };
        sim.mario.vel = [0.0; 3];
        sim.mario.forward_vel = 0.0;
        if t < 2 || t + 2 >= total {
            sim.mario.set_action(ACT_IDLE, 0.0);
        } else {
            sim.mario.set_action(ACT_FREEFALL, 0.0);
        }
        sim.tick(&idle());
        for e in &sim.events {
            if let SimEvent::Object(o) = e {
                seen.push(o.clone());
            }
        }
        if t == 3 {
            assert_eq!(sim.mario.objects[timer].var("running"), 1.0, "the HUD timer started on the start line");
        }
    }
    seen
}

/// interaction.c `pss_end_slide`: under 630 frames spawns the act 2 star at (-6358, -4300, 4700).
#[test]
fn pss_timer_pays_a_star_under_630_frames_and_not_over() {
    let fast = pss_run(300);
    assert_eq!(box_star_acts(&fast), vec![2]);
    let star = fast.iter().find_map(|e| if let ObjectEvent::BoxStar { pos, .. } = e { Some(*pos) } else { None }).unwrap();
    assert_eq!(star, [-6358.0, -4300.0, 4700.0]);
    assert!(box_star_acts(&pss_run(700)).is_empty(), "630 frames or more: no star");
    assert_eq!(pss::STAR_TIME_LIMIT, 630.0);
}

// --------------------------------------------------------------------------------------------- Toad

#[test]
fn toads_wait_for_their_star_count_and_hand_out_the_castle_stars() {
    // bhv_toad_message_init: 24 stars -> the 25 star Toad deletes itself; 25 -> he stays.
    for (stars, present) in [(24usize, false), (25, true)] {
        let mut sim = load("castle_inside", 2, 1);
        castle_npcs::apply(&mut sim, &save_with(stars));
        run(&mut sim, 2);
        let t = find(&sim, ObjectBehaviorKind::ToadMessage).into_iter().find(|&t| sim.mario.objects[t].param as u32 == toad::TOAD_STAR_2_DIALOG).unwrap();
        assert_eq!(sim.mario.objects[t].active, present, "{stars} stars");
    }
    // Talk to him with 25 stars.
    let save = save_with(25);
    let mut sim = load("castle_inside", 2, 1);
    castle_npcs::apply(&mut sim, &save);
    let t = find(&sim, ObjectBehaviorKind::ToadMessage).into_iter().find(|&t| sim.mario.objects[t].param as u32 == toad::TOAD_STAR_2_DIALOG).unwrap();
    let tp = sim.mario.objects[t].pos;
    sim.mario.pos = [tp[0], tp[1], tp[2] + 110.0];
    sim.mario.face_angle[1] = 0x8000 as f64; // facing -z, towards him
    sim.mario.set_action(ACT_IDLE, 0.0);
    for _ in 0..40 {
        sim.mario.pos = [tp[0], tp[1], tp[2] + 110.0];
        run(&mut sim, 1);
    }
    assert_eq!(sim.mario.objects[t].action, toad::TOAD_MESSAGE_OPAQUE, "opacified within 600 units");
    // B starts the talk (the interaction pass sees this tick's press, like `m->input`); the Toad notices a tick later.
    sim.tick(&ctl(0.0, 0.0, B_BUTTON, 0));
    assert_eq!(sim.mario.action, ACT_WAITING_FOR_DIALOG, "B does not punch him: talk wins");
    sim.tick(&idle());
    assert_eq!(sim.mario.objects[t].action, toad::TOAD_MESSAGE_TALKING);
    let mut events = vec![];
    for _ in 0..80 {
        events.extend(run(&mut sim, 1));
        if sim.mario.cut.dialog_open {
            break;
        }
    }
    assert!(sim.mario.cut.dialog_open, "DIALOG_076 opens once Mario is reading (action {:x})", sim.mario.action);
    assert!(box_star_acts(&events).is_empty(), "the star waits for the end of the dialog");
    sim.mario.cut.dialog_open = false; // the player closes the last page
    for _ in 0..60 {
        events.extend(run(&mut sim, 1));
    }
    assert_eq!(box_star_acts(&events), vec![2], "bhv_spawn_star_no_level_exit(STAR_INDEX_ACT_2)");
    assert_eq!(sim.mario.objects[t].var("dialogId"), toad::TOAD_STAR_2_DIALOG_AFTER as f64);
    assert!(matches!(sim.mario.action, ACT_IDLE | ACT_READING_NPC_DIALOG), "Mario is free again: {:x}", sim.mario.action);
}

/// The star the Toad releases is a castle secret star: the director spawns it, Mario takes it without
/// leaving the castle, and the save counts it.
#[test]
fn the_toad_star_is_collected_in_the_castle_and_counts() {
    let mut save = save_with(25);
    let mut sim = load("castle_inside", 2, 1);
    let data = ProgressData::load(&assets(), "castle_inside", 2);
    let mut d = StarDirector::new("castle_inside", 1, &data, sim.course.spawn.pos, &sim.mario.world, &sim.mario, &save);
    d.on_event(&ObjectEvent::BoxStar { pos: [-977.0, 1363.0, 2569.0], act: 2.0 });
    let mut collected = None;
    for _ in 0..400 {
        let reward = d.rewards.iter().find(|r| r.act == 2 && r.visible).map(|r| r.pos);
        if let Some(p) = reward {
            sim.mario.pos = p;
            sim.mario.set_action(ACT_FREEFALL, 0.0);
        } else {
            sim.mario.pos = [0.0, 5000.0, 0.0];
        }
        let step = d.step(&sim.mario, &save, 0);
        if let Some(c) = step.collected {
            collected = Some(c);
            break;
        }
    }
    let c = collected.expect("the star can be touched after its flight");
    assert_eq!(c.act, 2);
    assert!(c.bonus && d.no_exit, "stays in the castle (INT_SUBTYPE_NO_EXIT)");
    save.collect_star("castle_inside", c.act, 0);
    assert_eq!(save.stars(), 26);
}

// -------------------------------------------------------------------------------------------- Mips

/// bhv_mips_init picks MIPS_BP_15_STARS below 50 stars (until star 4 is owned), MIPS_BP_50_STARS after.
#[test]
fn mips_is_gated_by_the_star_count_and_the_two_flags() {
    let cases = [(14usize, false, false, None), (15, false, false, Some((0.0, 40.0))), (49, true, false, None), (50, true, false, Some((1.0, 45.0))), (50, true, true, None)];
    for (stars, c1, c2, want) in cases {
        let mut sim = load("castle_inside", 3, 1);
        let mut save = save_with(stars.min(110));
        if c1 {
            save.collect_star("castle_inside", 4, 0);
        }
        if c2 {
            save.collect_star("castle_inside", 5, 0);
        }
        // `save_with` counts the castle stars too; top up the test case to the wanted total.
        let m = find(&sim, ObjectBehaviorKind::Mips)[0];
        sim.mario.objects[m].set_var("stars", stars as f64);
        sim.mario.objects[m].set_var("collected1", c1 as i32 as f64);
        sim.mario.objects[m].set_var("collected2", c2 as i32 as f64);
        run(&mut sim, 2);
        match want {
            None => assert!(!sim.mario.objects[m].active, "{stars} stars c1={c1} c2={c2}: hidden"),
            Some((kind, speed)) => {
                assert!(sim.mario.objects[m].active);
                assert_eq!(sim.mario.objects[m].var("starKind"), kind);
                assert_eq!(sim.mario.objects[m].var("forwardVelocity"), speed);
            }
        }
    }
}

/// bhv_mips_held: DIALOG_084, put down at once, the star spawns from the idle action.
#[test]
fn holding_mips_talks_and_then_pays_the_star() {
    let mut sim = load("castle_inside", 3, 1);
    let m = find(&sim, ObjectBehaviorKind::Mips)[0];
    sim.mario.objects[m].set_var("stars", 15.0);
    run(&mut sim, 2);
    let p = sim.mario.objects[m].pos;
    sim.mario.pos = [p[0], p[1], p[2] + 60.0];
    sim.mario.vel = [0.0; 3];
    sim.mario.set_action(ACT_HOLD_IDLE, 0.0);
    sim.mario.used_obj = Some(m);
    sim.mario.grab_used_object();
    assert_eq!(sim.mario.held_obj, Some(m));
    let mut events = vec![];
    let mut opened = false;
    for _ in 0..90 {
        events.extend(run(&mut sim, 1));
        if sim.mario.cut.dialog_open && !opened {
            opened = true;
            run(&mut sim, 3);
            sim.mario.cut.dialog_open = false; // the player closes the box
        }
    }
    assert!(opened, "DIALOG_084 opened while holding Mips");
    assert_eq!(sim.mario.held_obj, None, "INT_SUBTYPE_DROP_IMMEDIATELY: Mario puts him down");
    assert_eq!(box_star_acts(&events), vec![4], "STAR_INDEX_ACT_4 + MIPS_BP_15_STARS");
}

// ------------------------------------------------------------------------------------ Koopa the Quick (THI)

#[test]
fn the_thi_race_is_placed_with_the_decomp_route_and_star() {
    let d = json("thi", 1);
    let koopa = d["objects"].as_array().unwrap().iter().find(|o| o["behavior"] == "koopaTheQuick").expect("koopa");
    assert_eq!(koopa["pos"], serde_json::json!([-1900, -511, 2400]));
    assert_eq!(koopa["acts"], serde_json::json!([3]));
    assert_eq!(koopa["track"].as_array().unwrap().len(), 12);
    let flag = d["objects"].as_array().unwrap().iter().find(|o| o["behavior"] == "koopaFlag").expect("flag");
    assert_eq!(flag["pos"], serde_json::json!([7400, -1537, -6300]));
    assert_eq!(jumpman_sim::behaviors::koopa_the_quick::race_star(1), [7100.0, -1300.0, -6000.0]);
    assert_eq!(crate::progress::stars::race_star_pos("thi", 3), Some([7100.0, -1300.0, -6000.0]));
}

#[test]
fn mario_can_win_the_thi_race() {
    use jumpman_sim::behaviors::koopa_the_quick::*;
    let mut sim = load("thi", 1, 3);
    let k = find(&sim, ObjectBehaviorKind::KoopaTheQuick)[0];
    let flag = find(&sim, ObjectBehaviorKind::KoopaFlag)[0];
    assert_eq!(sim.mario.objects[k].ball_route.as_ref().map(|r| r.waypoints.len()), Some(12));
    // Talk: stand next to him, accept (A).
    let kp = sim.mario.objects[k].pos;
    sim.mario.pos = [kp[0] + 300.0, kp[1], kp[2]];
    for _ in 0..4 {
        sim.tick(&idle());
    }
    assert_eq!(sim.mario.objects[k].action, KOOPA_RACE_INVITATION);
    sim.mario.objects[k].set_var("answer", 1.0);
    sim.tick(&idle());
    sim.mario.objects[k].set_var("answer", 0.0);
    assert_eq!(sim.mario.objects[k].action, KOOPA_RACE_COUNTDOWN);
    let mut won = false;
    let fp = sim.mario.objects[flag].pos;
    let mut ticks = 0;
    while ticks < 6000 {
        ticks += 1;
        if sim.mario.objects[k].action == KOOPA_RACE_COUNTDOWN || (sim.mario.objects[k].action == KOOPA_RACE_RUNNING && ticks < 200) {
            sim.mario.pos = [fp[0], fp[1] + 400.0, fp[2]]; // Mario is already at the finish
            sim.mario.vel = [0.0; 3];
        }
        let a = sim.mario.objects[k].action;
        if a == KOOPA_RACE_INVITATION || a == KOOPA_RACE_RESULT_DIALOG {
            sim.mario.objects[k].set_var("answer", 1.0);
        }
        sim.tick(&idle());
        sim.mario.objects[k].set_var("answer", 0.0);
        if sim.events.iter().any(|e| matches!(e, SimEvent::Object(ObjectEvent::Race { state: "won" }))) {
            won = true;
            break;
        }
    }
    assert!(won, "the race ends in a win; Koopa stopped in action {} at {:?} after {ticks} ticks", sim.mario.objects[k].action, sim.mario.objects[k].pos);
}

// --------------------------------------------------------------------------------------------- moat

/// water_pillar.inc.c: ground-pound both basement pillars (each sinks 4 x 20), the moat flag is reported
/// and water boxes 0 and 2 of the basement drain to -2450 at 5 per frame.
#[test]
fn pounding_both_basement_pillars_drains_the_water() {
    use jumpman_sim::behaviors::part_b::moat;
    let mut sim = load("castle_inside", 3, 1);
    let pillars = find(&sim, ObjectBehaviorKind::WaterLevelPillar);
    assert_eq!(pillars.len(), 2);
    castle_npcs::apply(&mut sim, &SaveFile::in_memory(1));
    let start = sim.mario.world.water_boxes[0].5;
    for &p in &pillars {
        let home = sim.mario.objects[p].pos;
        sim.mario.pos = [home[0], home[1] + 400.0, home[2]];
        run(&mut sim, 2);
        let top = sim.mario.world.find_floor(home[0], home[1] + 400.0, home[2]).height;
        sim.mario.pos = [home[0], top, home[2]];
        sim.mario.vel = [0.0; 3];
        sim.mario.action = ACT_GROUND_POUND_LAND;
        run(&mut sim, 1);
        assert_eq!(sim.mario.objects[p].action, 1, "the pillar sinks");
        sim.mario.set_action(ACT_IDLE, 0.0);
        run(&mut sim, 6);
        assert_eq!(sim.mario.objects[p].pos[1], home[1] - 80.0);
    }
    run(&mut sim, 3);
    assert!(pillars.iter().any(|&p| sim.mario.objects[p].action == 4), "both pressed: draining");
    let mut save = SaveFile::in_memory(1);
    castle_npcs::step(&mut sim, &mut save);
    assert!(save.has_flag(crate::progress::save::FLAG_MOAT_DRAINED));
    run(&mut sim, 300);
    let (b0, b2) = (sim.mario.world.water_boxes[0].5, sim.mario.world.water_boxes[2].5);
    assert!(b0 < start && b0 >= moat::DRAINED_BASEMENT_LEVEL && b2 >= moat::DRAINED_BASEMENT_LEVEL, "{b0} {b2}");
}

/// moat_grill.inc.c / moat_drainer.inc.c: the grills are solid until the moat is drained, then the moat
/// (water boxes 0 and 1 of the grounds) sits at -800.
#[test]
fn the_vanish_cap_grills_and_moat_follow_the_drained_flag() {
    for drained in [false, true] {
        let mut sim = load("castle_grounds", 1, 1);
        let mut save = SaveFile::in_memory(1);
        if drained {
            save.set_flag(crate::progress::save::FLAG_MOAT_DRAINED);
        }
        castle_npcs::apply(&mut sim, &save);
        let g = find(&sim, ObjectBehaviorKind::MoatGrill)[0];
        run(&mut sim, 2);
        let owned = sim.mario.world.surfaces().iter().filter(|s| s.object == Some(g.0)).count();
        assert_eq!(owned > 0, !drained, "grill collision (drained {drained})");
        assert_eq!(sim.mario.objects[g].visible, !drained);
        let moat = sim.mario.world.water_boxes[0].5;
        assert_eq!(moat == -800.0, drained, "moat level {moat}");
    }
}
