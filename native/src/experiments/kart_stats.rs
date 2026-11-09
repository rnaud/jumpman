//! Per-racer and per-CC kart constants of Mario Kart 64 (n64decomp/mk64 `src/data/kart_attributes.c`, picked in
//! `spawn_players.c::spawn_player` by `gCCSelection` and the character id). Engine-free.
//!
//! The tables are exported by `scripts/export-mk64-cpu.py` into `public/mk64/cpu-ai.json` (`kart`); `Kart::new`
//! starts with the Mario / 150cc values the golden physics trace was recorded with, so nothing changes until a
//! race installs other stats with `Kart::set_stats`.

use serde::{Deserialize, Serialize};

/// `gCCSelection`: engine class. The numeric values are the decomp's `CC_50 .. CC_EXTRA` (row of every per-CC table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Cc {
    Cc50 = 0,
    Cc100 = 1,
    #[default]
    Cc150 = 2,
    /// Extra mode: the 100cc physics on mirrored courses.
    Extra = 3,
}
impl Cc {
    pub const ALL: [Cc; 4] = [Cc::Cc50, Cc::Cc100, Cc::Cc150, Cc::Extra];
    pub fn index(self) -> usize {
        self as usize
    }
    /// The class as the menu names it.
    pub fn name(self) -> &'static str {
        ["50cc", "100cc", "150cc", "Extra"][self.index()]
    }
    pub fn label(self) -> &'static str {
        match self {
            Cc::Cc50 => "50cc",
            Cc::Cc100 => "100cc",
            Cc::Cc150 => "150cc",
            Cc::Extra => "Extra (mirror)",
        }
    }
    /// The next class in the menu cycle.
    pub fn next(self) -> Cc {
        Cc::ALL[(self.index() + 1) % Cc::ALL.len()]
    }
    /// Extra mode is the mirrored 100cc cup: the game forces `gIsMirrorMode` with it.
    pub fn mirrors(self) -> bool {
        self == Cc::Extra
    }
}

/// Racers in `characterId` order (`include/defines.h`).
pub const CHARACTERS: [&str; 8] = ["Mario", "Luigi", "Yoshi", "Toad", "D.K.", "Wario", "Peach", "Bowser"];

/// Weight class from the decomp's own special-casing (`func_8000B140` excludes WARIO, BOWSER and DK as the
/// "heavyweights"; the acceleration rows fall in three groups).
pub fn weight_class(character: usize) -> &'static str {
    match character {
        4 | 5 | 7 => "Heavy",
        2 | 3 | 6 => "Light",
        _ => "Medium",
    }
}

/// Constants one kart reads (the `Player` fields `topSpeed`, `unk_084`, `unk_088` plus the character tables).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KartStats {
    pub character: usize,
    pub cc: Cc,
    /// `topSpeed` (`gTopSpeedTable[cc][character]`), the cap of `currentSpeed`.
    pub top_speed: f64,
    /// `gKartAcceleration<Character>`: added to `currentSpeed` per tenth of the top speed.
    pub accel: [f64; 10],
    /// `gKartHandlingTable`.
    pub handling: f64,
    /// `D_800E2400[cc]`, sideways grip.
    pub unk_084: f64,
    /// `D_800E24B4[cc]`.
    pub unk_088: f64,
    /// `gKartTurnSpeedReductionTable0` / `1`.
    pub turn_reduction0: f64,
    pub turn_reduction1: f64,
    /// `gKartTopSpeedTable`: cap of `speed` (world units per frame).
    pub speed_cap: f64,
}
impl KartStats {
    /// Mario at 150cc: what the physics port was written and traced with.
    pub const MARIO_150: KartStats = KartStats {
        character: 0,
        cc: Cc::Cc150,
        top_speed: 320.,
        accel: [2.0, 2.0, 2.0, 1.6, 1.4, 1.2, 1.0, 0.8, 0.6, 0.4],
        handling: 1.25,
        unk_084: -20.,
        unk_088: 35.,
        turn_reduction0: 0.,
        turn_reduction1: 0.,
        speed_cap: 9.,
    };
}
impl Default for KartStats {
    fn default() -> Self {
        Self::MARIO_150
    }
}

/// The `kart` block of `cpu-ai.json`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KartTables {
    pub top_speed: Vec<Vec<f64>>,
    pub unk084: Vec<Vec<f64>>,
    pub unk088: Vec<Vec<f64>>,
    pub speed_cap: Vec<f64>,
    pub acceleration: Vec<Vec<f64>>,
    pub handling: Vec<f64>,
    pub turn_speed_reduction0: Vec<f64>,
    pub turn_speed_reduction1: Vec<f64>,
}
impl KartTables {
    /// `spawn_player`: the stats of `character` (0..8) in class `cc`.
    pub fn stats(&self, character: usize, cc: Cc) -> KartStats {
        let c = character.min(7);
        let row = cc.index();
        let mut accel = [0.; 10];
        accel.copy_from_slice(&self.acceleration[c][..10]);
        KartStats {
            character: c,
            cc,
            top_speed: self.top_speed[row][c],
            accel,
            handling: self.handling[c],
            unk_084: self.unk084[row][c],
            unk_088: self.unk088[row][c],
            turn_reduction0: self.turn_speed_reduction0[c],
            turn_reduction1: self.turn_speed_reduction1[c],
            speed_cap: self.speed_cap[c],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn tables() -> KartTables {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64/cpu-ai.json");
        #[derive(Deserialize)]
        struct File {
            kart: KartTables,
        }
        serde_json::from_slice::<File>(&std::fs::read(path).expect("run scripts/export-mk64-cpu.py")).unwrap().kart
    }

    #[test]
    fn mario_150_is_the_traced_default() {
        assert_eq!(tables().stats(0, Cc::Cc150), KartStats::MARIO_150);
    }

    #[test]
    fn classes_and_racers_differ_as_in_kart_attributes_c() {
        let t = tables();
        let (m50, m100, m150) = (t.stats(0, Cc::Cc50), t.stats(0, Cc::Cc100), t.stats(0, Cc::Cc150));
        assert!(m50.top_speed < m100.top_speed && m100.top_speed < m150.top_speed);
        assert_eq!((m50.top_speed, m100.top_speed, m150.top_speed), (290., 310., 320.));
        assert_eq!(t.stats(0, Cc::Extra).top_speed, 310., "Extra uses 100cc speeds");
        // Light racers (Yoshi, Toad, Peach) reach 4 more, heavy ones (D.K., Wario, Bowser) handle 0.1 worse.
        assert_eq!(t.stats(2, Cc::Cc150).top_speed, 324.);
        assert!(t.stats(5, Cc::Cc150).handling < t.stats(0, Cc::Cc150).handling);
        assert!(t.stats(2, Cc::Cc150).accel[3] > t.stats(5, Cc::Cc150).accel[3]);
        assert_eq!(weight_class(5), "Heavy");
        assert_eq!(weight_class(3), "Light");
    }
}
