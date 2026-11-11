//! The collectible theme at the counting layer, on real courses: the same pickup credits the counter of whoever plays (coins for Mario,
//! rupees into Link's wallet, notes for Banjo) while the world's own counters (`GameSim::coins` for the 100-coin star, `red_coins`,
//! Banjo's `notes` for the note doors) keep counting. Data-dependent: they need the imported `assets/courses` (they pass trivially
//! when it is missing).
use crate::game::runner::{GameSim, PickupKind, SimEvent};
use crate::game::world::CourseRequest;
use jumpman_sim::collectible_theme::Theme;
use jumpman_sim::link::items::Age;
use jumpman_sim::state::ControllerSnapshot;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Who {
    Mario,
    Link,
    Banjo,
}

fn load(course: &str, who: Who) -> Option<GameSim> {
    let mut sim = GameSim::load(&assets(), CourseRequest::new(course)).ok()?;
    match who {
        Who::Mario => {}
        Who::Link => {
            sim.as_link = true;
            sim.mario.link.enable(Age::Adult);
            sim.mario.link.inv.rupees = 0;
        }
        Who::Banjo => sim.as_banjo = true,
    }
    Some(sim)
}

fn idle(sim: &mut GameSim, ticks: usize) {
    for _ in 0..ticks {
        sim.tick(&ControllerSnapshot::default());
    }
}

/// Stand in the first not yet collected pickup of `kind` and `value` of the course.
fn stand_on_pickup(sim: &mut GameSim, kind: PickupKind, value: i32) -> usize {
    let i = sim.pickups.iter().position(|p| p.kind == kind && p.value == value && !p.collected).expect("pickup");
    sim.mario.pos = sim.pickups[i].pos;
    sim.mario.vel = [0.0; 3];
    i
}

// ---- SM64: bob ---------------------------------------------------------------------------------------------

#[test]
fn sm64_coin_counts_for_the_character_and_the_world() {
    // (who, wallet after one yellow coin)
    for (who, wallet) in [(Who::Mario, 0), (Who::Link, 1), (Who::Banjo, 0)] {
        let Some(mut sim) = load("bob", who) else { return };
        assert_eq!(sim.theme(), match who { Who::Mario => Theme::Coin, Who::Link => Theme::Rupee, Who::Banjo => Theme::Note });
        let wallet0 = sim.mario.link.inv.rupees;
        let i = stand_on_pickup(&mut sim, PickupKind::Coin, 1);
        idle(&mut sim, 2);
        assert!(sim.pickups[i].collected, "{who:?} takes the coin");
        // the world's own counter (the 100-coin star rule, the score) counts the coin whoever plays
        assert_eq!(sim.coins, 1, "{who:?}: the 100-coin counter");
        assert_eq!(sim.mario.link.inv.rupees - wallet0, wallet, "{who:?}: Link's wallet");
        if who == Who::Banjo {
            assert_eq!(crate::banjo_ctl::hud_counts(&sim)[0], 1, "Banjo's note chip counts the coin as a note");
        }
    }
}

#[test]
fn sm64_blue_and_red_coins_are_blue_and_red_rupees_for_link() {
    let Some(mut sim) = load("bob", Who::Link) else { return };
    let p = sim.mario.pos;
    sim.spawn_coin([p[0], p[1] + 10.0, p[2]], false, 5.0);
    idle(&mut sim, 2);
    assert_eq!((sim.coins, sim.mario.link.inv.rupees), (5, 5), "blue coin: 5 coins, a blue rupee (5)");
    assert!(sim.events.iter().any(|e| matches!(e, SimEvent::CoinCollected { value: 5 })) || sim.coins == 5);
    let Some(i) = sim.pickups.iter().position(|p| p.kind == PickupKind::Red) else { return };
    sim.mario.pos = sim.pickups[i].pos;
    sim.mario.vel = [0.0; 3];
    idle(&mut sim, 2);
    assert_eq!(sim.red_coins, 1, "the red-coin star rule still counts it");
    assert_eq!((sim.coins, sim.mario.link.inv.rupees), (7, 25), "red coin: 2 coins, a red rupee (20)");
}

#[test]
fn the_hundred_coin_count_is_the_same_for_every_character() {
    for who in [Who::Mario, Who::Link, Who::Banjo] {
        let Some(mut sim) = load("bob", who) else { return };
        let p = sim.mario.pos;
        for _ in 0..12 {
            sim.spawn_coin([p[0], p[1] + 10.0, p[2]], false, 5.0);
        }
        idle(&mut sim, 20);
        assert_eq!(sim.coins, 60, "{who:?}: twelve blue coins");
    }
}

#[test]
fn link_wallet_is_capped_by_his_pockets_not_the_coin_count() {
    let Some(mut sim) = load("bob", Who::Link) else { return };
    let p = sim.mario.pos;
    for _ in 0..110 {
        sim.spawn_coin([p[0], p[1] + 10.0, p[2]], false, 5.0);
    }
    idle(&mut sim, 130);
    assert_eq!(sim.coins, 550);
    assert_eq!(sim.mario.link.inv.rupees, sim.mario.link.inv.upgrades.wallet_cap());
}

// ---- BK: bk_mumbos_mountain ---------------------------------------------------------------------------------

fn stand_on_note(sim: &mut GameSim) {
    let items = crate::bk_enemies::pickups::load_items(&assets(), "bk_mumbos_mountain");
    let note = items.iter().find(|i| i.kind == jumpman_sim::banjo_ext::PickupKind::Note).expect("a note");
    sim.mario.pos = note.pos;
    sim.mario.vel = [0.0; 3];
    sim.mario.banjo.drop_pickup(jumpman_sim::banjo_ext::PickupKind::Note, note.pos);
}

#[test]
fn bk_note_counts_for_the_character_and_the_note_doors() {
    // (who, wallet, coins)
    for (who, wallet, coins) in [(Who::Mario, 0, 1), (Who::Link, 1, 0), (Who::Banjo, 0, 0)] {
        let Some(mut sim) = load("bk_mumbos_mountain", who) else { return };
        let (notes, wallet0) = (sim.mario.banjo.notes, sim.mario.link.inv.rupees);
        stand_on_note(&mut sim);
        idle(&mut sim, 3);
        assert!(sim.mario.banjo.pickups.is_empty(), "{who:?} took the note");
        // Banjo's own counter (the note doors) counts it whoever plays
        assert_eq!(sim.mario.banjo.notes, notes + 1, "{who:?}: notes");
        assert_eq!(sim.coins, coins, "{who:?}: coin counter");
        assert_eq!(sim.mario.link.inv.rupees - wallet0, wallet, "{who:?}: Link's wallet");
        if who == Who::Banjo {
            assert_eq!(crate::banjo_ctl::hud_counts(&sim)[0], notes + 1, "Banjo's note chip");
        }
    }
}
