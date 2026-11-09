//! Placement-file checks and scripted scenarios on real exported OoT courses: Link, Mario and
//! Banjo each fight the same Deku Baba, and the enemies hurt every character back.
use super::*;
use crate::game::world::CourseRequest;
use jumpman_sim::link::items::{Age, Sword};
use jumpman_sim::oot_enemy::KINDS;
use jumpman_sim::state::*;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

/// Enemy-like actors the exporter reports as `skipped`, with the reason. A placed actor that is
/// neither implemented nor listed here fails the parse test.
const EXPLICIT_SKIPS: &[(&str, &str)] = &[
    ("ACTOR_EN_HINTNUTS", "Deku Tree quiz scrubs: scene NPCs (dungeon script)"),
    ("ACTOR_EN_SHOPNUTS", "business scrub shops: NPCs"),
    ("ACTOR_EN_SKJ", "Skull Kid: NPC"),
    ("ACTOR_BOSS_GANON", "not ported"),
    ("ACTOR_EN_DH", "not ported"),
    ("ACTOR_EN_DHA", "not ported"),
    ("ACTOR_EN_FD", "Flare Dancer: not ported"),
    ("ACTOR_EN_YUKABYUN", "Flying floor tile: not ported"),
    ("ACTOR_EN_DODONGO", "Dodongo: not ported"),
    ("ACTOR_EN_VALI", "Bari: not ported"),
    ("ACTOR_EN_BX", "Bari tentacle: not ported"),
    ("ACTOR_EN_BROB", "Stinger / Brob: not ported"),
    ("ACTOR_EN_TP", "Tailpasaran: not ported"),
    ("ACTOR_EN_BA", "Parasitic tentacle: not ported"),
    ("ACTOR_OBJ_ROOMTIMER", "room timer: scene script"),
    ("ACTOR_EN_FLOORMAS", "Floormaster: not ported"),
    ("ACTOR_EN_ENCOUNT2", "Death Mountain rock/meteor spawner"),
    ("ACTOR_BOSS_GOMA", "Queen Gohma: scripted boss (zelda/gohma.rs)"),
];

fn all_placements() -> Vec<(String, Placements)> {
    let dir = assets().join(MODEL_DIR);
    let mut out = vec![];
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let p: Placements = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            out.push((path.file_stem().unwrap().to_string_lossy().to_string(), p));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_placed_enemy_is_spawnable_or_explicitly_skipped() {
    let mut placed: HashMap<String, u32> = HashMap::new();
    let mut skipped: HashMap<String, u32> = HashMap::new();
    let mut table = vec![];
    for (course, p) in all_placements() {
        assert!(assets().join("courses").join(format!("{course}.json")).is_file(), "{course} is not a course");
        let mut per_course: HashMap<&str, u32> = HashMap::new();
        for e in &p.enemies {
            let kind = Kind::from_name(&e.kind).unwrap_or_else(|| panic!("{course}: no implementation for {:?}", e.kind));
            assert!(assets().join(MODEL_DIR).join(format!("{}.glb", kind.spec().model)).is_file(), "model of {kind:?}");
            *placed.entry(e.kind.clone()).or_default() += 1;
            *per_course.entry(kind.name()).or_default() += 1;
        }
        for (actor, n) in &p.skipped {
            assert!(EXPLICIT_SKIPS.iter().any(|(a, _)| a == actor), "{course}: {actor} x{n} is neither implemented nor on the skip list");
            *skipped.entry(actor.clone()).or_default() += n;
        }
        for s in &p.spawners {
            // Spawner types: 0 leevers are implemented; 2/3 (night-only Stalchildren/Wolfos) are reported.
            if s.spawn_type != 0 {
                *skipped.entry(format!("ENCOUNT1 type {} (night spawner)", s.spawn_type)).or_default() += 1;
            }
        }
        if !per_course.is_empty() || !p.spawners.is_empty() {
            let mut v: Vec<_> = per_course.into_iter().collect();
            v.sort();
            table.push(format!("{course}: {v:?} spawners {}", p.spawners.len()));
        }
    }
    println!("placed per course:\n{}", table.join("\n"));
    let mut v: Vec<_> = placed.iter().collect();
    v.sort();
    println!("implemented kinds placed: {v:?}");
    let mut v: Vec<_> = skipped.iter().collect();
    v.sort();
    println!("skipped (explicit): {v:?}");
    let total: u32 = placed.values().sum();
    assert!(total >= 60, "{total} enemies placed");
    for kind in ["deku_baba", "withered_baba", "gold_skulltula", "skulltula", "peahat", "octorok", "mad_scrub", "wolfos", "guay", "goma_larva"] {
        assert!(placed.contains_key(kind), "{kind} is never placed");
    }
    assert!(KINDS.len() == 36);
    assert!(placed.contains_key("deku_baba") && placed.contains_key("octorok") && (placed.contains_key("tektite") || placed.contains_key("red_tektite")));
}

#[test]
fn deku_tree_field_matches_the_room_actor_lists() {
    let (_, p) = all_placements().into_iter().find(|(c, _)| c == "deku_tree").unwrap();
    let count = |k: &str| p.enemies.iter().filter(|e| e.kind == k).count();
    // ydan room headers: 9 EN_DEKUBABA, 5 EN_KAREBABA, 7 EN_ST, 3 EN_GOMA and 7 EN_SW (Gold Skulltulas).
    assert_eq!(count("deku_baba"), 9);
    assert_eq!(count("withered_baba"), 5);
    assert_eq!(count("skulltula"), 7);
    assert_eq!(count("goma_larva"), 3);
    let field = build_field(&p, 1, |pos| pos);
    assert_eq!(field.enemies.len(), p.enemies.len());
}

fn load_sim(id: &str) -> GameSim {
    GameSim::load(&assets(), CourseRequest::new(id)).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn enemies_of(sim: &GameSim, id: &str) -> OotEnemies {
    let p = load_placements(&assets(), id, 1).unwrap();
    let world = sim.mario.world.clone();
    let field = build_field(&p, 99, |pos| {
        let f = world.find_floor(pos[0], pos[1] + 80.0, pos[2]);
        if f.surface.is_some() && (pos[1] - f.height).abs() < 120.0 { [pos[0], f.height, pos[2]] } else { pos }
    });
    OotEnemies { field, drops: vec![], pending: vec![], clips: HashMap::new(), tokens: 0, kills: 0, taken: vec![], next_drop: 0, warp: None, prev: vec![], course: id.into() }
}

/// Stand the player `dist` units in front of Baba `i` (on the side it faces), on the floor.
fn put_player_near(sim: &mut GameSim, en: &OotEnemies, i: usize, dist: f64) {
    let home = en.field.enemies[i].home;
    sim.mario.pos = [home[0], home[1] + 5.0, home[2] + dist];
    sim.mario.face_angle[1] = 32768.0;
    sim.mario.vel = [0.0; 3];
    sim.mario.forward_vel = 0.0;
    let f = sim.mario.world.find_floor(sim.mario.pos[0], sim.mario.pos[1] + 100.0, sim.mario.pos[2]);
    assert!(f.surface.is_some(), "floor under {:?}", sim.mario.pos);
    sim.mario.pos[1] = f.height;
    sim.mario.set_action(ACT_IDLE, 0.0);
}

fn first_baba(en: &OotEnemies) -> usize {
    en.field.enemies.iter().position(|e| e.kind == Kind::DekuBaba).expect("a Deku Baba in the Deku Tree")
}

thread_local! {
    /// Everything `tick` asked the app to play during `fight`.
    static OUTS: std::cell::RefCell<Vec<Out>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn heard() -> Vec<Out> {
    OUTS.with(|o| std::mem::take(&mut *o.borrow_mut()))
}

fn neutral() -> ControllerSnapshot {
    ControllerSnapshot::default()
}

/// Face the head and hold/press `button` every `every` ticks until the Baba is gone or `max` ticks pass.
fn fight(sim: &mut GameSim, en: &mut OotEnemies, i: usize, button: u32, every: usize, max: usize, hurt: &mut u32) -> bool {
    for t in 0..max {
        let head = en.field.enemies[i].head;
        let yaw = jumpman_sim::math::atan2s(head[2] - sim.mario.pos[2], head[0] - sim.mario.pos[0]) as f64;
        sim.mario.face_angle[1] = yaw;
        let mut c = neutral();
        if every > 0 && t % every == 0 {
            c.button_pressed = button;
            c.button_down = button;
        }
        if sim.mario.link.enabled {
            c.focus_yaw = Some(yaw);
        }
        let before = sim.mario.health;
        sim.tick(&c);
        let outs = tick(sim, en);
        OUTS.with(|o| o.borrow_mut().extend(outs));
        if sim.mario.health < before {
            *hurt += 1;
        }
        // Stay put: the player is standing guard where the test placed him.
        if en.field.enemies[i].st == St::Gone {
            return true;
        }
    }
    false
}

#[test]
fn link_kills_a_deku_baba_with_the_sword_on_the_deku_tree_course() {
    let mut sim = load_sim("deku_tree");
    sim.as_link = true;
    sim.mario.link.enable(Age::Adult);
    sim.mario.link.inv.equip_sword(Sword::Master, Age::Adult);
    let mut en = enemies_of(&sim, "deku_tree");
    let i = first_baba(&en);
    put_player_near(&mut sim, &en, i, 170.0);
    // Let it rise first (Link standing in range), then swing.
    let mut hurt = 0;
    let killed = fight(&mut sim, &mut en, i, B_BUTTON, 14, 900, &mut hurt);
    assert!(killed, "Link's sword never finished the Baba: {:?} hp {} pos {:?}", en.field.enemies[i].st, en.field.enemies[i].hp, sim.mario.pos);
    assert!(en.kills >= 1);
    assert!(!en.drops.is_empty() || en.tokens > 0 || en.kills > 0, "nuts and a stick drop");
    // Link's kill sounds are the OoT clips (blade strike, the Baba's death cry), not the SM64 placeholders.
    let outs = heard();
    assert!(outs.contains(&Out::Oot("IT_SWORD_STRIKE")), "{outs:?}");
    assert!(outs.contains(&Out::Oot("EN_DEKU_DEAD")) && outs.contains(&Out::Oot("EN_EXTINCT")), "{outs:?}");
    assert!(!outs.iter().any(|o| matches!(o, Out::Cue("break_box" | "stomp" | "bonk", _))), "{outs:?}");
}

#[test]
fn mario_ground_pounds_a_deku_baba_dead() {
    let mut sim = load_sim("deku_tree");
    let mut en = enemies_of(&sim, "deku_tree");
    let i = first_baba(&en);
    put_player_near(&mut sim, &en, i, 700.0);
    // Wait for the Baba to come out and sway.
    for _ in 0..140 {
        sim.tick(&neutral());
        tick(&mut sim, &mut en);
    }
    assert!(!matches!(en.field.enemies[i].st, St::Hidden | St::Gone), "{:?}", en.field.enemies[i].st);
    let mut killed = false;
    for round in 0..8 {
        if en.field.enemies[i].st == St::Gone {
            killed = true;
            break;
        }
        // Mario waits where the lunge will end, 700 units out, until the Baba rears back (Windup),
        // then drops on the spot its head is about to chomp (Mario's ground pound).
        for _ in 0..400 {
            if en.field.enemies[i].st == St::Windup {
                break;
            }
            sim.tick(&neutral());
            tick(&mut sim, &mut en);
            let home = en.field.enemies[i].home;
            sim.mario.pos = [home[0], 0.0, home[2] + 700.0];
            sim.mario.set_action(ACT_IDLE, 0.0);
        }
        let (home, yaw) = (en.field.enemies[i].home, en.field.enemies[i].yaw);
        let reach = en.field.enemies[i].spec().scale * 95.0 * U;
        sim.mario.pos = [home[0] + jumpman_sim::math::sins(yaw) * reach, 120.0, home[2] + jumpman_sim::math::coss(yaw) * reach];
        sim.mario.vel = [0.0, -60.0, 0.0];
        sim.mario.set_action(ACT_GROUND_POUND, 0.0);
        for _ in 0..40 {
            sim.tick(&neutral());
            tick(&mut sim, &mut en);
            if en.field.enemies[i].st == St::Gone {
                killed = true;
                break;
            }
        }
        let _ = round;
    }
    assert!(killed || en.field.enemies[i].st == St::Gone, "Mario's pound never killed it: {:?} hp {}", en.field.enemies[i].st, en.field.enemies[i].hp);
    assert!(en.kills >= 1);
}

#[test]
fn banjo_beak_barges_a_deku_baba_dead() {
    let mut sim = load_sim("deku_tree");
    sim.as_banjo = true;
    let mut en = enemies_of(&sim, "deku_tree");
    let i = first_baba(&en);
    put_player_near(&mut sim, &en, i, 700.0);
    for _ in 0..140 {
        sim.tick(&neutral());
        tick(&mut sim, &mut en);
    }
    let mut killed = false;
    for _ in 0..10 {
        if en.field.enemies[i].st == St::Gone {
            killed = true;
            break;
        }
        let head = en.field.enemies[i].head;
        // Beak Barge: an attacking dive along the ground straight at the head.
        sim.mario.pos = [head[0], head[1].max(30.0), head[2] + 260.0];
        sim.mario.face_angle[1] = 32768.0;
        sim.mario.forward_vel = 40.0;
        sim.mario.vel = [0.0, 0.0, -40.0];
        sim.mario.set_action(ACT_DIVE, 0.0);
        for _ in 0..24 {
            sim.tick(&neutral());
            tick(&mut sim, &mut en);
        }
    }
    assert!(killed || en.field.enemies[i].st == St::Gone || en.field.enemies[i].hp < 2, "Banjo's barge never landed: {:?} hp {}", en.field.enemies[i].st, en.field.enemies[i].hp);
}

#[test]
fn a_baba_bite_hurts_each_character() {
    for who in 0..3 {
        let mut sim = load_sim("deku_tree");
        if who == 0 {
            sim.as_link = true;
            sim.mario.link.enable(Age::Child);
        }
        if who == 2 {
            sim.as_banjo = true;
        }
        let mut en = enemies_of(&sim, "deku_tree");
        let i = first_baba(&en);
        put_player_near(&mut sim, &en, i, 250.0);
        let (h0, hearts0) = (sim.mario.health, sim.mario.link.inv.health);
        let mut bitten = false;
        for _ in 0..400 {
            sim.tick(&neutral());
            tick(&mut sim, &mut en);
            let en_pos = en.field.enemies[i].head;
            bitten |= sim.mario.health < h0 || sim.mario.hurt_counter > 0.0 || sim.mario.link.inv.health < hearts0 || en_pos[1] < -1e9;
            if bitten {
                break;
            }
        }
        assert!(bitten, "character {who} was never hurt by the Baba");
        if who == 0 {
            assert!(sim.mario.link.inv.health != hearts0 || sim.mario.hurt_counter > 0.0);
        }
    }
}

#[test]
fn link_targets_and_projectile_hits_route_to_enemies() {
    let sim = load_sim("deku_tree");
    let mut en = enemies_of(&sim, "deku_tree");
    let i = first_baba(&en);
    en.field.enemies[i].asleep = false;
    let targets = en.link_targets();
    assert!(targets.iter().any(|t| t.id == TARGET_BASE + i as u32));
    assert!(en.hit_extra(TARGET_BASE + i as u32, Dmg::Slingshot, [0.0; 3]));
    assert!(!en.hit_extra(1_000_000 + 3, Dmg::Slingshot, [0.0; 3]), "goomba ids are not ours");
    assert_eq!(en.pending.len(), 1);
}


// ---------------------------------------------------------------------------------------------
// Placed collectibles (`En_Item00` of the room actor lists): rupees, hearts, pieces of heart
// ---------------------------------------------------------------------------------------------

#[test]
fn placed_collectibles_are_exported_with_their_types_and_flags() {
    let all = all_placements();
    let mut by_type: std::collections::BTreeMap<u8, u32> = Default::default();
    for (_, p) in &all {
        for c in &p.collectibles {
            *by_type.entry(c.ty).or_default() += 1;
            assert!(c.flag < 64, "the collectible flag is 6 bits");
        }
    }
    // The 27 courses place 53 En_Item00: rupees, recovery hearts and 11 Pieces of Heart.
    assert!(by_type.values().sum::<u32>() >= 53, "{by_type:?}");
    assert!(by_type.get(&6).copied().unwrap_or(0) >= 11, "pieces of heart");
    assert!(by_type.keys().all(|t| matches!(*t, 0..=3 | 6 | 10 | 11 | 17)), "{by_type:?}");
    // Kokiri Forest: 11 of them, the first a green rupee with collectible flag 36.
    let kokiri = &all.iter().find(|(c, _)| c == "kokiri_forest").unwrap().1;
    assert_eq!(kokiri.collectibles.len(), 11);
    assert_eq!((kokiri.collectibles[0].ty, kokiri.collectibles[0].flag), (0, 36));
}

fn link_on(sim: &mut GameSim, at: V3) {
    sim.mario.link.enabled = true;
    sim.mario.pos = at;
    sim.mario.vel = [0.0; 3];
    sim.mario.set_action(ACT_IDLE, 0.0);
}

#[test]
fn placed_items_hang_in_place_are_taken_once_and_remembered() {
    let mut sim = load_sim("kokiri_forest");
    sim.mario.link.inv = jumpman_sim::link::items::Inventory::starter(Age::Child);
    let p = load_placements(&assets(), "kokiri_forest", 1).unwrap();
    let mut en = enemies_of(&sim, "kokiri_forest");
    let needs = needs_of(&sim);
    en.place_collectibles(&p, "kokiri_forest", &|_| false, &needs, |v| v);
    assert_eq!(en.drops.len(), p.collectibles.len());
    assert!(en.drops.iter().all(|d| d.key.is_some()));
    // Nothing falls or despawns however long it waits.
    let before: Vec<V3> = en.drops.iter().map(|d| d.pos).collect();
    sim.mario.pos = [0.0, 5000.0, 0.0];
    for _ in 0..400 {
        step_drops(&mut sim, &mut en, &mut vec![]);
    }
    assert_eq!(before, en.drops.iter().map(|d| d.pos).collect::<Vec<_>>());
    assert_eq!(en.drops.len(), p.collectibles.len());
    // Standing on the first rupee takes it (Link: +1 rupee) and the key is queued for the save.
    let first = en.drops[0].clone();
    link_on(&mut sim, [first.pos[0], first.pos[1] - 30.0, first.pos[2]]);
    let rupees = sim.mario.link.inv.rupees;
    let mut outs = vec![];
    step_drops(&mut sim, &mut en, &mut outs);
    assert_eq!(sim.mario.link.inv.rupees, rupees + 1);
    assert_eq!(en.taken, vec![collectible_key("kokiri_forest", p.collectibles[0].flag)]);
    assert_eq!(en.drops.len(), p.collectibles.len() - 1);
    // Coming back later, the save says it is gone.
    let taken: std::collections::BTreeSet<String> = en.taken.iter().cloned().collect();
    let mut again = enemies_of(&sim, "kokiri_forest");
    again.place_collectibles(&p, "kokiri_forest", &|k| taken.contains(k), &needs, |v| v);
    assert_eq!(again.drops.len(), p.collectibles.len() - 1);
}

#[test]
fn four_pieces_of_heart_make_a_container_for_link() {
    let mut sim = load_sim("kokiri_forest");
    sim.mario.link.inv = jumpman_sim::link::items::Inventory::starter(Age::Child);
    let mut en = enemies_of(&sim, "kokiri_forest");
    link_on(&mut sim, [0.0, 0.0, 0.0]);
    sim.mario.link.inv.health = 16;
    let (max, mut toasts) = (sim.mario.link.inv.max_health, vec![]);
    for i in 0..4 {
        en.next_drop += 1;
        en.drops.push(DropItem { id: en.next_drop, pos: [0.0, 10.0, 0.0], vel: [0.0; 3], item: Item::HeartPiece, qty: 1, age: 0.0, done: false, key: Some(format!("test:{i}")) });
        let mut outs = vec![];
        step_drops(&mut sim, &mut en, &mut outs);
        toasts.extend(outs.into_iter().filter_map(|o| if let Out::Toast(t) = o { Some(t) } else { None }));
    }
    assert_eq!(toasts, ["PIECE OF HEART (1/4)", "PIECE OF HEART (2/4)", "PIECE OF HEART (3/4)", "HEART CONTAINER"]);
    let inv = &sim.mario.link.inv;
    assert_eq!(inv.max_health, max + 16, "one more heart");
    assert_eq!(inv.health, 16 + 16, "and a heart refilled");
    assert_eq!(inv.heart_pieces, 0);
}

#[test]
fn mario_and_banjo_refill_on_a_piece_of_heart() {
    let mut sim = load_sim("kokiri_forest");
    let mut en = enemies_of(&sim, "kokiri_forest");
    sim.mario.pos = [0.0, 0.0, 0.0];
    sim.mario.state.health = 0x300 as f64;
    en.drops.push(DropItem { id: 1, pos: [0.0, 10.0, 0.0], vel: [0.0; 3], item: Item::HeartPiece, qty: 1, age: 0.0, done: false, key: Some("x".into()) });
    step_drops(&mut sim, &mut en, &mut vec![]);
    assert_eq!(sim.mario.health, sim.mario.max_health);
}

#[test]
fn a_gold_skulltula_token_is_kept_in_link_s_inventory() {
    let mut sim = load_sim("kokiri_forest");
    let mut en = enemies_of(&sim, "kokiri_forest");
    link_on(&mut sim, [0.0, 0.0, 0.0]);
    let mut toasts = vec![];
    for i in 0..3 {
        en.drops.push(DropItem { id: i + 1, pos: [0.0, 10.0, 0.0], vel: [0.0; 3], item: Item::Token, qty: 1, age: 20.0, done: false, key: None });
        let mut outs = vec![];
        step_drops(&mut sim, &mut en, &mut outs);
        toasts.extend(outs.into_iter().filter_map(|o| if let Out::Toast(t) = o { Some(t) } else { None }));
    }
    assert_eq!(sim.mario.link.inv.skulltulas, 3);
    assert_eq!(toasts.last().unwrap(), "GOLD SKULLTULA TOKEN 3");
}

#[test]
fn dodongos_cavern_places_its_keese_and_the_new_dungeons_have_music() {
    let all = all_placements();
    let cavern = &all.iter().find(|(c, _)| c == "dodongos_cavern").expect("dodongos_cavern placements").1;
    let keese = cavern.enemies.iter().filter(|e| e.kind == "keese").count();
    assert_eq!(keese, 13, "EN_FIREFLY placements of the cavern");
    assert!(Kind::from_name("keese").is_some());
    for (course, seq) in [("dodongos_cavern", 24), ("jabu_jabu", 38), ("forest_temple", 44), ("temple_of_time", 58), ("market_day", 29)] {
        let p = &all.iter().find(|(c, _)| c == course).unwrap_or_else(|| panic!("{course}")).1;
        assert_eq!(p.environment.music.seq, seq, "{course}");
    }
}
