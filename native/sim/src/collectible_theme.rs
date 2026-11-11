//! The collectible theme: the small currency pickups of every world (SM64 coins, OoT rupees, BK notes) follow the
//! CHARACTER being played, not the world. Mario sees and counts coins everywhere, Link (adult or young) rupees,
//! Banjo musical notes. The underlying object stays what the world defined (the 100-coin and red-coin star rules, Banjo's
//! note doors...); only the presentation and the counter that is credited change.
//!
//! Engine-free: this is the one table both the sim (which counter gets what) and the views / audio (which model, which
//! sound) read. See `native/COLLECTIBLES.md` for the per-world inventory.
//!
//! ## Worth
//!
//! Every currency pickup has a *worth*, in coin-equivalents (`Source::worth`):
//!
//! | source                     | worth |
//! |----------------------------|-------|
//! | SM64 yellow coin           | 1     |
//! | SM64 red coin              | 2     |
//! | SM64 blue coin             | 5     |
//! | OoT green rupee (1)        | 1     |
//! | OoT blue rupee (5)         | 5     |
//! | OoT red rupee (20)         | 2     |
//! | OoT purple rupee (50)      | 10    |
//! | OoT silver rupee (100)     | 20    |
//! | OoT gold rupee (200)       | 40    |
//! | BK note                    | 1     |
//!
//! Mario's coins and Banjo's notes credit the worth 1:1. Link's wallet credits the face value of the rupee that
//! stands for that worth (1 -> 1, 2 -> 20 red, 5 -> 5, 10 -> 50, 20 -> 100, 40 -> 200), so a red coin of the star hunt is a
//! red rupee (20) and the colour always says how much it is worth to Link. A 200-rupee chest is 40 coins (not 200), so
//! nothing floods Mario's 100-coin star rule.

/// Whose currency is shown and counted.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Theme {
    /// Mario: the SM64 coin (yellow, red, blue).
    #[default]
    Coin,
    /// Link and Young Link: rupees.
    Rupee,
    /// Banjo-Kazooie: musical notes.
    Note,
}

impl Theme {
    /// The theme of whoever is playing, from the sim's two character switches (`link.enabled`, `banjo.active`).
    pub fn of(link: bool, banjo: bool) -> Self {
        if banjo {
            Theme::Note
        } else if link {
            Theme::Rupee
        } else {
            Theme::Coin
        }
    }

    /// The word for one pickup in toasts and messages.
    pub fn word(self) -> &'static str {
        match self {
            Theme::Coin => "COIN",
            Theme::Rupee => "RUPEE",
            Theme::Note => "NOTE",
        }
    }
}

/// What the world defined.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// An SM64 coin of this value (1 yellow, 2 red, 5 blue).
    Coin(i32),
    /// An OoT rupee of this face value (1, 5, 20, 50, 100 or 200).
    Rupee(i32),
    /// A BK musical note.
    Note,
}

/// What is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Skin {
    CoinYellow,
    CoinRed,
    CoinBlue,
    RupeeGreen,
    RupeeBlue,
    RupeeRed,
    RupeePurple,
    RupeeSilver,
    RupeeGold,
    Note,
}

/// The worths that have a rupee colour (ascending) and the rupee face value of each.
const WORTH_TO_RUPEE: [(i32, i32); 6] = [(1, 1), (2, 20), (5, 5), (10, 50), (20, 100), (40, 200)];

impl Source {
    /// Coin-equivalents (see the module table).
    pub fn worth(self) -> i32 {
        match self {
            Source::Coin(v) => v.max(1),
            Source::Rupee(v) => match v {
                ..=1 => 1,
                2..=5 => 5,
                6..=20 => 2,
                21..=50 => 10,
                51..=100 => 20,
                _ => 40,
            },
            Source::Note => 1,
        }
    }
}

/// The face value of the rupee that stands for `worth` coin-equivalents (a worth with no rupee colour keeps its number).
pub fn rupee_face(worth: i32) -> i32 {
    WORTH_TO_RUPEE.iter().find(|(w, _)| *w == worth).map_or(worth.max(1), |(_, r)| *r)
}

/// How many of the theme's units a pickup is credited as: coins for Mario, rupees for Link's wallet, notes for Banjo.
pub fn credit(theme: Theme, source: Source) -> i32 {
    let worth = source.worth();
    match theme {
        Theme::Coin | Theme::Note => worth,
        Theme::Rupee => match source {
            // A rupee is worth its own face value to Link, whatever the other characters would see.
            Source::Rupee(v) => v.max(1),
            _ => rupee_face(worth),
        },
    }
}

/// The picture of a pickup for `theme`.
pub fn skin(theme: Theme, source: Source) -> Skin {
    let worth = source.worth();
    match theme {
        Theme::Note => Skin::Note,
        Theme::Coin => match worth {
            ..=1 => Skin::CoinYellow,
            2 => Skin::CoinRed,
            _ => Skin::CoinBlue,
        },
        Theme::Rupee => match rupee_face(worth) {
            ..=1 => Skin::RupeeGreen,
            2..=5 => Skin::RupeeBlue,
            6..=20 => Skin::RupeeRed,
            21..=50 => Skin::RupeePurple,
            51..=100 => Skin::RupeeSilver,
            _ => Skin::RupeeGold,
        },
    }
}

/// The SM64 coin value of a coin glb (`actors/coin/<colour>_coin*_geo.glb`), `None` for other models.
pub fn coin_model_value(model: &str) -> Option<i32> {
    let name = model.strip_prefix("actors/coin/")?;
    Some(if name.starts_with("red") {
        2
    } else if name.starts_with("blue") {
        5
    } else if name.starts_with("yellow") {
        1
    } else {
        return None;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_follows_the_character_switches() {
        assert_eq!(Theme::of(false, false), Theme::Coin);
        assert_eq!(Theme::of(true, false), Theme::Rupee);
        assert_eq!(Theme::of(false, true), Theme::Note);
    }

    #[test]
    fn sm64_coins_become_rupees_and_notes() {
        // yellow 1, red 2, blue 5
        assert_eq!([1, 2, 5].map(|v| credit(Theme::Rupee, Source::Coin(v))), [1, 20, 5]);
        assert_eq!([1, 2, 5].map(|v| skin(Theme::Rupee, Source::Coin(v))), [Skin::RupeeGreen, Skin::RupeeRed, Skin::RupeeBlue]);
        assert_eq!([1, 2, 5].map(|v| credit(Theme::Note, Source::Coin(v))), [1, 2, 5]);
        assert_eq!([1, 2, 5].map(|v| credit(Theme::Coin, Source::Coin(v))), [1, 2, 5]);
        assert_eq!([1, 2, 5].map(|v| skin(Theme::Coin, Source::Coin(v))), [Skin::CoinYellow, Skin::CoinRed, Skin::CoinBlue]);
        assert_eq!(skin(Theme::Note, Source::Coin(5)), Skin::Note);
    }

    #[test]
    fn rupees_keep_their_face_value_for_link_and_become_worth_for_the_others() {
        let faces = [1, 5, 20, 50, 100, 200];
        assert_eq!(faces.map(|v| credit(Theme::Rupee, Source::Rupee(v))), faces);
        assert_eq!(faces.map(|v| credit(Theme::Coin, Source::Rupee(v))), [1, 5, 2, 10, 20, 40]);
        assert_eq!(faces.map(|v| credit(Theme::Note, Source::Rupee(v))), [1, 5, 2, 10, 20, 40]);
        assert_eq!(
            faces.map(|v| skin(Theme::Rupee, Source::Rupee(v))),
            [Skin::RupeeGreen, Skin::RupeeBlue, Skin::RupeeRed, Skin::RupeePurple, Skin::RupeeSilver, Skin::RupeeGold]
        );
        // Mario sees yellow / blue / red coins; the big rupees are blue coins that count more
        assert_eq!(faces.map(|v| skin(Theme::Coin, Source::Rupee(v))), [Skin::CoinYellow, Skin::CoinBlue, Skin::CoinRed, Skin::CoinBlue, Skin::CoinBlue, Skin::CoinBlue]);
    }

    #[test]
    fn a_gold_rupee_cannot_flood_marios_hundred_coin_star() {
        assert!(credit(Theme::Coin, Source::Rupee(200)) < 100 / 2);
    }

    #[test]
    fn notes_are_one_of_everything() {
        for t in [Theme::Coin, Theme::Note] {
            assert_eq!(credit(t, Source::Note), 1);
        }
        assert_eq!(credit(Theme::Rupee, Source::Note), 1);
        assert_eq!(skin(Theme::Rupee, Source::Note), Skin::RupeeGreen);
        assert_eq!(skin(Theme::Coin, Source::Note), Skin::CoinYellow);
    }

    #[test]
    fn a_rupee_goes_round_trip_through_the_worth_table() {
        for face in [1, 5, 20, 50, 100, 200] {
            assert_eq!(rupee_face(Source::Rupee(face).worth()), face, "{face}");
        }
    }

    #[test]
    fn coin_models_say_their_value() {
        assert_eq!(coin_model_value("actors/coin/yellow_coin_geo.glb"), Some(1));
        assert_eq!(coin_model_value("actors/coin/red_coin_no_shadow_geo.glb"), Some(2));
        assert_eq!(coin_model_value("actors/coin/blue_coin_geo.glb"), Some(5));
        assert_eq!(coin_model_value("actors/goomba/goomba_geo.glb"), None);
    }
}
