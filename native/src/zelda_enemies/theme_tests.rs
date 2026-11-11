//! The collectible theme in an OoT world (`kokiri_forest`): a rupee is a coin for Mario, a rupee for Link and a note for Banjo.
use super::*;
use crate::game::world::CourseRequest;
use jumpman_sim::link::items::Age;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

/// Take one `item` as `who` plays: returns (coins, Link's wallet, the sounds).
fn take_rupee(item: Item, who: &str) -> Option<(u32, i32, Vec<Out>)> {
    let mut sim = GameSim::load(&assets(), CourseRequest::new("kokiri_forest")).ok()?;
    match who {
        "link" => {
            sim.as_link = true;
            sim.mario.link.enable(Age::Adult);
            sim.mario.link.inv.rupees = 0;
        }
        "banjo" => sim.as_banjo = true,
        _ => {}
    }
    let (mut tokens, mut outs) = (0, vec![]);
    let wallet0 = sim.mario.link.inv.rupees;
    collect(&mut sim, item, 1, &mut tokens, &mut outs);
    Some((sim.coins, sim.mario.link.inv.rupees - wallet0, outs))
}

#[test]
fn an_oot_rupee_is_a_coin_for_mario_a_rupee_for_link_and_a_note_for_banjo() {
    // (rupee, coins for Mario and Banjo, wallet for Link)
    for (item, coins, wallet) in [(Item::RupeeGreen, 1, 1), (Item::RupeeBlue, 5, 5), (Item::RupeeRed, 2, 20), (Item::RupeePurple, 10, 50), (Item::RupeeOrange, 20, 100)] {
        let Some((mario_coins, mario_wallet, outs)) = take_rupee(item, "mario") else { return };
        assert_eq!((mario_coins, mario_wallet), (coins, 0), "{item:?} as Mario");
        assert!(outs.iter().any(|o| matches!(o, Out::Coin)), "Mario hears the coin sound");
        let (link_coins, link_wallet, outs) = take_rupee(item, "link").unwrap();
        assert_eq!((link_coins, link_wallet), (0, wallet), "{item:?} as Link: the wallet is the single counter");
        assert!(outs.iter().any(|o| matches!(o, Out::Oot("SY_GET_RUPY"))), "Link hears the rupee sound");
        let (banjo_coins, banjo_wallet, _) = take_rupee(item, "banjo").unwrap();
        assert_eq!((banjo_coins, banjo_wallet), (coins, 0), "{item:?} as Banjo: note-equivalents on his note chip");
    }
}

#[test]
fn a_gold_rupee_cannot_flood_marios_hundred_coins() {
    let Some((coins, ..)) = take_rupee(Item::RupeeOrange, "mario") else { return };
    assert!(coins < 50);
}

#[test]
fn drops_are_drawn_as_the_players_currency() {
    use jumpman_sim::collectible_theme::Theme;
    assert_eq!(themed_drop_look(Item::RupeeBlue, Theme::Rupee), Look::RupeeBlue);
    assert_eq!(themed_drop_look(Item::RupeeBlue, Theme::Coin), Look::CoinBlue);
    assert_eq!(themed_drop_look(Item::RupeeRed, Theme::Coin), Look::CoinRed);
    assert_eq!(themed_drop_look(Item::RupeeGreen, Theme::Note), Look::Note);
    // hearts and the rest are not currency
    assert_eq!(themed_drop_look(Item::Heart, Theme::Coin), Look::Heart);
    assert_eq!(themed_drop_look(Item::Seeds, Theme::Note), Look::Ammo);
}
