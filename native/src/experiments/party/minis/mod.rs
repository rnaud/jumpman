//! The mini-game collection. Each game is its own engine-free module with a deterministic seeded core
//! and a bot policy; [`Mini`] is the catalogue and [`Session`] steps a game with humans and bots mixed.
use super::common::*;

pub mod barrel_hop;
pub mod block_stack;
pub mod bomb_drizzle;
pub mod bumper_brawl;
pub mod coin_scramble;
pub mod crumble_floor;
pub mod echo_pads;
pub mod mash_mountain;
pub mod odd_one_out;
pub mod plank_balance;
pub mod pound_grid;
pub mod power_meter;
pub mod quick_draw;
pub mod shell_lane;
pub mod star_count;

/// How a game is controlled (shown on the intro card and used to pick the touch layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Left stick (touch pad) to move.
    Stick,
    /// Stick plus the A button.
    StickA,
    /// One big A / tap button.
    Tap,
    /// Tap the tiles on the screen.
    Tiles,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mini {
    BarrelHop,
    MashMountain,
    EchoPads,
    CoinScramble,
    BombDrizzle,
    PoundGrid,
    PlankBalance,
    ShellLane,
    OddOneOut,
    BumperBrawl,
    CrumbleFloor,
    QuickDraw,
    BlockStack,
    StarCount,
    PowerMeter,
}

pub const ALL_MINIS: [Mini; 15] = [
    Mini::BarrelHop,
    Mini::MashMountain,
    Mini::EchoPads,
    Mini::CoinScramble,
    Mini::BombDrizzle,
    Mini::PoundGrid,
    Mini::PlankBalance,
    Mini::ShellLane,
    Mini::OddOneOut,
    Mini::BumperBrawl,
    Mini::CrumbleFloor,
    Mini::QuickDraw,
    Mini::BlockStack,
    Mini::StarCount,
    Mini::PowerMeter,
];

pub struct MiniInfo {
    pub key: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub control: Control,
    /// What the score means, for the results list.
    pub unit: &'static str,
}

impl Mini {
    pub fn info(self) -> MiniInfo {
        use Control::*;
        let (key, name, blurb, control, unit) = match self {
            Mini::BarrelHop => ("barrelhop", "Barrel Hop", "Barrels roll down your lane. Tap JUMP in time to hop them. Cleared barrels score, hits cost.", Tap, "net hops"),
            Mini::MashMountain => ("mash", "Mash Mountain", "Tap as fast as you can to climb the pole. Mash too hard and you tire out.", Tap, "climbed"),
            Mini::EchoPads => ("echo", "Echo Pads", "Watch the pads flash, then tap them in the same order. One slip and you are out.", Tiles, "longest run"),
            Mini::CoinScramble => ("coins", "Coin Scramble", "Run around with the stick and grab coins. Gold coins are worth three.", Stick, "points"),
            Mini::BombDrizzle => ("bombs", "Bomb Drizzle", "Bombs fall where the shadows grow. Stay clear of them. Last one standing wins.", Stick, "seconds alive"),
            Mini::PoundGrid => ("pound", "Pound Grid", "Tap the glowing pad before anyone else. A wrong pad costs a point.", Tiles, "points"),
            Mini::PlankBalance => ("plank", "Plank Balance", "Keep the plank level: push the stick against the tilt. Last one balanced wins.", Stick, "seconds up"),
            Mini::ShellLane => ("lane", "Shell Lane", "Race down your lane. Push the stick up or down to swap rows: dodge rocks, hit gold pads.", Stick, "distance"),
            Mini::OddOneOut => ("odd", "Odd One Out", "One tile is different from the rest. Tap it first. Wrong taps cost a point.", Tiles, "points"),
            Mini::BumperBrawl => ("bumper", "Bumper Brawl", "Steer your bumper car and shove rivals off the shrinking ring. A dashes.", StickA, "seconds on"),
            Mini::CrumbleFloor => ("crumble", "Crumble Floor", "Tiles flash and drop away. Keep running onto solid floor.", Stick, "seconds alive"),
            Mini::QuickDraw => ("draw", "Quick Draw", "Wait for the flag to go green, then tap. Tap early and you are out of the round.", Tap, "points"),
            Mini::BlockStack => ("stack", "Block Stack", "Tap to drop the sliding block. Only the overlap stays. Stack the tallest tower.", Tap, "blocks"),
            Mini::StarCount => ("stars", "Star Count", "Count the stars that flit by, then tap the right number. Faster is better.", Tiles, "points"),
            Mini::PowerMeter => ("meter", "Power Meter", "Stop the swinging marker inside the gold zone. Three tries.", Tap, "points"),
        };
        MiniInfo { key, name, blurb, control, unit }
    }

    pub fn from_key(s: &str) -> Option<Mini> {
        let s = s.to_ascii_lowercase();
        ALL_MINIS.iter().copied().find(|m| m.info().key == s || m.info().name.to_ascii_lowercase().replace(' ', "") == s.replace(['_', '-', ' '], ""))
    }

    /// Build the game for the `active` players.
    pub fn make(self, seed: u64, active: [bool; 4]) -> Box<dyn MiniGame> {
        match self {
            Mini::BarrelHop => Box::new(barrel_hop::BarrelHop::new(seed, active)),
            Mini::MashMountain => Box::new(mash_mountain::MashMountain::new(seed, active)),
            Mini::EchoPads => Box::new(echo_pads::EchoPads::new(seed, active)),
            Mini::CoinScramble => Box::new(coin_scramble::CoinScramble::new(seed, active)),
            Mini::BombDrizzle => Box::new(bomb_drizzle::BombDrizzle::new(seed, active)),
            Mini::PoundGrid => Box::new(pound_grid::PoundGrid::new(seed, active)),
            Mini::PlankBalance => Box::new(plank_balance::PlankBalance::new(seed, active)),
            Mini::ShellLane => Box::new(shell_lane::ShellLane::new(seed, active)),
            Mini::OddOneOut => Box::new(odd_one_out::OddOneOut::new(seed, active)),
            Mini::BumperBrawl => Box::new(bumper_brawl::BumperBrawl::new(seed, active)),
            Mini::CrumbleFloor => Box::new(crumble_floor::CrumbleFloor::new(seed, active)),
            Mini::QuickDraw => Box::new(quick_draw::QuickDraw::new(seed, active)),
            Mini::BlockStack => Box::new(block_stack::BlockStack::new(seed, active)),
            Mini::StarCount => Box::new(star_count::StarCount::new(seed, active)),
            Mini::PowerMeter => Box::new(power_meter::PowerMeter::new(seed, active)),
        }
    }
}

/// Fixed simulation step of every mini-game.
pub const DT: f32 = 1.0 / 30.0;

/// A running mini-game with its players: humans bring their own [`PadIn`], the rest are driven by bots.
pub struct Session {
    pub kind: Mini,
    pub game: Box<dyn MiniGame>,
    pub active: [bool; 4],
    pub human: [bool; 4],
    pub levels: [Level; 4],
    pub steps: u32,
}

impl Session {
    pub fn new(kind: Mini, seed: u64, active: [bool; 4], human: [bool; 4], levels: [Level; 4]) -> Self {
        Session { kind, game: kind.make(seed, active), active, human, levels, steps: 0 }
    }

    /// One fixed step; `human_in` is only read for human players.
    pub fn step(&mut self, human_in: &Inputs) {
        let mut inp = [PadIn::default(); 4];
        for p in 0..4 {
            if !self.active[p] {
                continue;
            }
            inp[p] = if self.human[p] { human_in[p] } else { self.game.bot(p, self.levels[p]) };
        }
        self.game.step(DT, &inp);
        self.steps += 1;
    }

    pub fn done(&self) -> bool {
        self.game.done()
    }

    pub fn ranks(&self) -> [u8; 4] {
        ranks(&self.game.scores(), &self.active)
    }
}

/// Play a game to the end with bots in every active seat (tests, balance runs, the board's headless mode).
pub fn run_bots(kind: Mini, seed: u64, active: [bool; 4], levels: [Level; 4]) -> ([f32; 4], f32) {
    let mut s = Session::new(kind, seed, active, [false; 4], levels);
    let none = [PadIn::default(); 4];
    // Every game lasts well under two minutes; the cap guards against a stuck core.
    while !s.done() && s.steps < 30 * 120 {
        s.step(&none);
    }
    assert!(s.done(), "{:?} did not terminate", kind);
    (s.game.scores(), s.steps as f32 * DT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL4: [bool; 4] = [true; 4];

    #[test]
    fn catalogue_has_at_least_fourteen_games_with_unique_keys() {
        assert!(ALL_MINIS.len() >= 14);
        let mut keys: Vec<_> = ALL_MINIS.iter().map(|m| m.info().key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), ALL_MINIS.len());
        for m in ALL_MINIS {
            assert!(!m.info().blurb.is_empty());
            assert_eq!(Mini::from_key(m.info().key), Some(m));
        }
    }

    #[test]
    fn every_game_terminates_under_bot_vs_bot_soak_in_every_format() {
        let formats: [[bool; 4]; 3] = [ALL4, [true, true, false, false], [false, true, false, true]];
        for m in ALL_MINIS {
            for seed in 0..12u64 {
                for lv in Level::ALL {
                    for a in formats {
                        let (s, secs) = run_bots(m, seed * 7 + 1, a, [lv; 4]);
                        assert!(secs <= 80.0, "{m:?} ran {secs}s");
                        for p in 0..4 {
                            if a[p] {
                                assert!(s[p].is_finite(), "{m:?} score for active {p}");
                            } else {
                                assert!(s[p] == f32::NEG_INFINITY, "{m:?} inactive {p} must not score");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn games_are_deterministic_by_seed() {
        for m in ALL_MINIS {
            let a = run_bots(m, 99, ALL4, [Level::Normal; 4]);
            let b = run_bots(m, 99, ALL4, [Level::Normal; 4]);
            assert_eq!(a, b, "{m:?}");
        }
    }

    #[test]
    fn scenes_render_without_panicking_and_cover_all_active_players() {
        for m in ALL_MINIS {
            let mut s = Session::new(m, 5, ALL4, [false; 4], [Level::Normal; 4]);
            let none = [PadIn::default(); 4];
            for i in 0..200 {
                s.step(&none);
                if i % 20 == 0 {
                    let mut sc = Scene::default();
                    s.game.scene(&mut sc);
                    for p in 0..4 {
                        assert!(sc.avatars.iter().any(|a| a.p == p), "{m:?} lacks an avatar for {p}");
                    }
                    for a in &sc.avatars {
                        assert!(a.x.abs() < 12.0 && a.z.abs() < 8.0 && a.x.is_finite() && a.y.is_finite(), "{m:?} avatar off stage {a:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn idle_humans_lose_to_bots_in_skill_games() {
        // A player who never touches the controls must not beat three Normal bots on average.
        for m in ALL_MINIS {
            let mut idle_wins = 0;
            let n = 20;
            for seed in 0..n {
                let mut s = Session::new(m, seed + 3, ALL4, [true, false, false, false], [Level::Normal; 4]);
                let none = [PadIn::default(); 4];
                while !s.done() && s.steps < 30 * 100 {
                    s.step(&none);
                }
                let r = s.ranks();
                if r[0] == 1 {
                    idle_wins += 1;
                }
            }
            assert!(idle_wins <= n / 2, "{m:?}: an idle player wins {idle_wins}/{n}");
        }
    }

    #[test]
    fn harder_bots_beat_easier_bots() {
        // Seat 0 plays Hard against three Easy bots; they should win clearly more than a quarter of the time overall.
        let mut wins = 0.0;
        let mut games = 0.0;
        for m in ALL_MINIS {
            for seed in 0..30u64 {
                let lv = [Level::Hard, Level::Easy, Level::Easy, Level::Easy];
                let (s, _) = run_bots(m, seed + 11, ALL4, lv);
                let r = ranks(&s, &ALL4);
                if r[0] == 1 {
                    wins += 1.0 / (0..4).filter(|&q| r[q] == 1).count() as f32;
                }
                games += 1.0;
            }
        }
        let rate = wins / games;
        assert!(rate > 0.35, "Hard seat wins only {rate:.2} against Easy bots");
    }

    /// `cargo test --release balance_report -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn balance_report() {
        println!("mini-game balance: win share of the seat by level (4 seats, 120 seeds); bot-vs-bot, same level");
        println!("{:<14} {:>6} {:>6} {:>6} {:>6} | {:>6}s  | Hard seat0 vs Easy", "game", "seat0", "seat1", "seat2", "seat3", "len");
        for m in ALL_MINIS {
            let mut wins = [0.0f32; 4];
            let mut len = 0.0;
            let n = 120;
            for seed in 0..n as u64 {
                let (s, secs) = run_bots(m, seed * 13 + 5, ALL4, [Level::Normal; 4]);
                let r = ranks(&s, &ALL4);
                let k = (0..4).filter(|&q| r[q] == 1).count() as f32;
                for p in 0..4 {
                    if r[p] == 1 {
                        wins[p] += 1.0 / k;
                    }
                }
                len += secs;
            }
            let mut hard = 0.0;
            for seed in 0..n as u64 {
                let (s, _) = run_bots(m, seed * 17 + 2, ALL4, [Level::Hard, Level::Easy, Level::Easy, Level::Easy]);
                let r = ranks(&s, &ALL4);
                if r[0] == 1 {
                    hard += 1.0 / (0..4).filter(|&q| r[q] == 1).count() as f32;
                }
            }
            println!("{:<14} {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}% | {:>6.1}s | {:>4.0}%", m.info().name, wins[0] / n as f32 * 100.0, wins[1] / n as f32 * 100.0, wins[2] / n as f32 * 100.0, wins[3] / n as f32 * 100.0, len / n as f32, hard / n as f32 * 100.0);
        }
    }
}
