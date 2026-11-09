//! Mario Kart 64 game structure around the kart mode, engine-free: the four cups, Grand Prix points and standings, the
//! trophy rule, Time Trial records, hot-seat VS scoring, the racer roster / line-up, the CC class, and the saved career
//! (`kart_career.json`). Twin of the decomp's `menus.c` (`gCupCourseOrder`, `sCharacterGridOrder`), `menu_items.c`
//! (`gGPPointRewards`, `func_800A3A10` standings sort, `gCupText`) and `save.c` (`func_800B536C`: the trophy is
//! `3 - rank` and only ever replaces a lower one, kept per CC class and cup).
//!
//! Decisions: the game has eight racers, this port races four (the player and three CPU karts), so the GP table
//! `9 / 6 / 3 / 1` is used as it is (it only ever awarded points to the first four places, so no scaling is
//! needed). Trophies follow the player's rank among the four racers.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// `gGPPointRewards` (`menu_items.c`): points for 1st..4th; 5th and below score nothing.
pub const GP_POINTS: [u32; 4] = [9, 6, 3, 1];
/// Races per cup.
pub const CUP_RACES: usize = 4;
/// VS points per place in the hot-seat VS race.
pub const VS_POINTS: [u32; 4] = [3, 2, 1, 0];

/// One cup: `gCupCourseOrder` rows 0-3 of `menus.c`, as course directory names.
#[derive(Clone, Copy, Debug)]
pub struct Cup {
    pub name: &'static str,
    pub tracks: [&'static str; CUP_RACES],
}
pub const CUPS: [Cup; 4] = [
    Cup { name: "Mushroom", tracks: ["luigi_raceway", "moo_moo_farm", "koopa_troopa_beach", "kalimari_desert"] },
    Cup { name: "Flower", tracks: ["toads_turnpike", "frappe_snowland", "choco_mountain", "mario_raceway"] },
    Cup { name: "Star", tracks: ["wario_stadium", "sherbet_land", "royal_raceway", "bowsers_castle"] },
    Cup { name: "Special", tracks: ["dks_jungle_parkway", "yoshi_valley", "banshee_boardwalk", "rainbow_road"] },
];
/// `gCupCourseOrder[4]`: the battle courses.
pub const ARENAS: [&str; 4] = ["big_donut", "block_fort", "double_deck", "skyscraper"];
pub const ARENA_NAMES: [&str; 4] = ["Big Donut", "Block Fort", "Double Deck", "Skyscraper"];

/// The cup a course belongs to and its place in it (`gCupSelectionByCourseId`).
pub fn cup_of(dir: &str) -> Option<(usize, usize)> {
    CUPS.iter().enumerate().find_map(|(c, cup)| cup.tracks.iter().position(|t| *t == dir).map(|i| (c, i)))
}

pub use super::kart_stats::Cc;

/// A racer of the roster in MK64 `characterId` order (`kart_sprite::CHARACTERS`).
#[derive(Clone, Copy, Debug)]
pub struct RacerInfo {
    pub name: &'static str,
    /// `hud-portrait-<face>.png`, `hud-minimap-kart-<face>.png`.
    pub face: &'static str,
    /// Kart atlas `<atlas>-kart.png` (the player's Mario one is `mario-kart.png`).
    pub atlas: &'static str,
}
pub const ROSTER: [RacerInfo; 8] = [
    RacerInfo { name: "Mario", face: "mario", atlas: "mario" },
    RacerInfo { name: "Luigi", face: "luigi", atlas: "luigi" },
    RacerInfo { name: "Yoshi", face: "yoshi", atlas: "yoshi" },
    RacerInfo { name: "Toad", face: "toad", atlas: "toad" },
    RacerInfo { name: "D.K.", face: "donkey-kong", atlas: "donkeykong" },
    RacerInfo { name: "Wario", face: "wario", atlas: "wario" },
    RacerInfo { name: "Peach", face: "peach", atlas: "peach" },
    RacerInfo { name: "Bowser", face: "bowser", atlas: "bowser" },
];
/// The game modes of the kart front end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    #[default]
    GrandPrix,
    TimeTrials,
    Vs,
    Battle,
    FreeRoam,
}
impl Mode {
    pub const ALL: [Mode; 5] = [Mode::GrandPrix, Mode::TimeTrials, Mode::Vs, Mode::Battle, Mode::FreeRoam];
    pub fn name(self) -> &'static str {
        match self {
            Mode::GrandPrix => "Grand Prix",
            Mode::TimeTrials => "Time Trials",
            Mode::Vs => "VS Race",
            Mode::Battle => "Battle",
            Mode::FreeRoam => "Free Roam",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Mode::GrandPrix => "4 races, points, a trophy",
            Mode::TimeTrials => "beat your best and your ghost",
            Mode::Vs => "1-4 players take turns, best time wins",
            Mode::Battle => "3 balloons each, last kart standing",
            Mode::FreeRoam => "drive any track, items and all",
        }
    }
    pub fn uses_cc(self) -> bool {
        matches!(self, Mode::GrandPrix | Mode::Vs)
    }
}

/// Everything the front end collects before a session starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setup {
    pub mode: Mode,
    pub cc: Cc,
    pub character: usize,
    pub cup: usize,
    /// Race within the cup (Grand Prix starts at 0; the other modes pick one).
    pub race: usize,
    /// Battle arena index.
    pub arena: usize,
    /// Human players for the hot-seat VS race (1-4).
    pub players: usize,
}
impl Default for Setup {
    fn default() -> Self {
        Self { mode: Mode::GrandPrix, cc: Cc::Cc150, character: 0, cup: 0, race: 0, arena: 0, players: 1 }
    }
}
impl Setup {
    /// Course directory of the current race (or arena).
    pub fn course(&self) -> &'static str {
        if self.mode == Mode::Battle { ARENAS[self.arena.min(3)] } else { CUPS[self.cup.min(3)].tracks[self.race.min(CUP_RACES - 1)] }
    }
}

static CURRENT: Mutex<Option<Setup>> = Mutex::new(None);
/// The running session's setup.
pub fn current() -> Setup {
    CURRENT.lock().ok().and_then(|s| s.clone()).unwrap_or_default()
}
/// Remember the session's setup and install its class and racer in the kart setup (`kart_setup`).
pub fn set_current(setup: Setup) {
    super::kart_setup::set(super::kart_setup::Setup { cc: setup.cc, character: setup.character.min(7) });
    if let Ok(mut s) = CURRENT.lock() {
        *s = Some(setup);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Grand Prix
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trophy {
    None,
    Bronze,
    Silver,
    Gold,
}
impl Trophy {
    /// `gCupText` order: none, bronze, silver, gold.
    pub fn level(self) -> u8 {
        self as u8
    }
    pub fn from_level(l: u8) -> Self {
        [Trophy::None, Trophy::Bronze, Trophy::Silver, Trophy::Gold][l.min(3) as usize]
    }
    /// `func_800B536C(rank)`: `3 - rank` for the top three of the cup standings.
    pub fn for_rank(rank: usize) -> Self {
        if rank < 3 { Self::from_level(3 - rank as u8) } else { Trophy::None }
    }
    pub fn name(self) -> &'static str {
        ["No trophy", "Bronze", "Silver", "Gold"][self as usize]
    }
}

/// A cup in progress. Racer slots follow the line-up: 0 is the player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CupRun {
    pub cup: usize,
    pub cc: Cc,
    /// Index of the race being played (`gCourseIndexInCup`); `CUP_RACES` when the cup is over.
    pub race: usize,
    /// `gGPPointsByCharacterId`, by slot.
    pub points: [u32; 4],
    /// Finishing order (slots, first place first) of every race so far.
    pub history: Vec<[usize; 4]>,
}
impl CupRun {
    pub fn new(cup: usize, cc: Cc) -> Self {
        Self { cup: cup.min(3), cc, race: 0, points: [0; 4], history: vec![] }
    }
    pub fn track(&self) -> Option<&'static str> {
        CUPS[self.cup].tracks.get(self.race).copied()
    }
    pub fn finished(&self) -> bool {
        self.race >= CUP_RACES
    }
    /// Score a race: `order` lists the slots first place first. Returns the points each slot got.
    pub fn score(&mut self, order: &[usize]) -> [u32; 4] {
        let mut got = [0; 4];
        if self.finished() {
            return got;
        }
        let mut full = [0usize; 4];
        let mut seen = [false; 4];
        let mut n = 0;
        for &s in order.iter().filter(|s| **s < 4) {
            if !seen[s] && n < 4 {
                seen[s] = true;
                full[n] = s;
                n += 1;
            }
        }
        for s in 0..4 {
            if !seen[s] && n < 4 {
                full[n] = s;
                n += 1;
            }
        }
        for (place, slot) in full.iter().enumerate() {
            got[*slot] = GP_POINTS[place];
            self.points[*slot] += GP_POINTS[place];
        }
        self.history.push(full);
        self.race += 1;
        got
    }
    /// Slots by points, highest first (`func_800A3A10`: equal points keep the lower slot first, so the human wins ties).
    pub fn standings(&self) -> [usize; 4] {
        let mut order = [0, 1, 2, 3];
        order.sort_by(|a, b| self.points[*b].cmp(&self.points[*a]).then(a.cmp(b)));
        order
    }
    /// Place of `slot` in the standings with ties shown as the same number (`func_800A3ADC`).
    pub fn shown_place(&self, slot: usize) -> usize {
        let order = self.standings();
        let pos = order.iter().position(|s| *s == slot).unwrap_or(0);
        (0..pos).rev().take_while(|i| self.points[order[*i]] == self.points[slot]).last().map_or(pos, |i| i) + 1
    }
    pub fn player_rank(&self) -> usize {
        self.standings().iter().position(|s| *s == 0).unwrap_or(3)
    }
    /// The trophy the cup earns the player (only meaningful once finished).
    pub fn trophy(&self) -> Trophy {
        Trophy::for_rank(self.player_rank())
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Hot-seat VS race
// ---------------------------------------------------------------------------------------------------------------

/// 1-4 humans take turns on the same track and race against the CPU karts with the same seed; the finishing times
/// are compared (a split screen of several human karts needs several cameras: not part of this port).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VsRun {
    pub players: usize,
    pub turn: usize,
    /// Race time in ticks per human (`None` until they have played).
    pub times: Vec<Option<u32>>,
    /// Place against the CPU karts per human.
    pub places: Vec<usize>,
}
impl VsRun {
    pub fn new(players: usize) -> Self {
        let n = players.clamp(1, 4);
        Self { players: n, turn: 0, times: vec![None; n], places: vec![4; n] }
    }
    pub fn finished(&self) -> bool {
        self.turn >= self.players
    }
    pub fn record(&mut self, ticks: u32, place: usize) {
        if let Some(t) = self.times.get_mut(self.turn) {
            *t = Some(ticks);
            self.places[self.turn] = place;
            self.turn += 1;
        }
    }
    /// Humans by time, fastest first; players who have not raced are last.
    pub fn ranking(&self) -> Vec<usize> {
        let mut o: Vec<usize> = (0..self.players).collect();
        o.sort_by_key(|p| (self.times[*p].unwrap_or(u32::MAX), *p));
        o
    }
    /// VS points per human (3 / 2 / 1 / 0 by rank).
    pub fn points(&self) -> Vec<u32> {
        let mut p = vec![0; self.players];
        for (rank, who) in self.ranking().into_iter().enumerate() {
            if self.times[who].is_some() {
                p[who] = VS_POINTS[rank.min(3)];
            }
        }
        p
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Career save
// ---------------------------------------------------------------------------------------------------------------

/// Time Trial records of one track (`best 3 laps` and the best race).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackRecord {
    pub best_race: Option<u32>,
    /// The three fastest laps, fastest first.
    pub best_laps: Vec<u32>,
    pub best_character: Option<usize>,
}

/// What a Time Trial run changed in the records.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TtOutcome {
    pub new_best_race: bool,
    /// How many of the run's laps entered the top three.
    pub new_laps: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Career {
    /// `trophies[cc][cup]` as `Trophy::level` (the save of `grandPrixPoints[cc]`).
    pub trophies: [[u8; 4]; 4],
    pub gp_best_points: [[u32; 4]; 4],
    pub gp_runs: u32,
    pub records: BTreeMap<String, TrackRecord>,
    pub vs_wins: [u32; 4],
    pub battle_wins: u32,
    pub battles: u32,
}
impl Career {
    pub fn trophy(&self, cc: Cc, cup: usize) -> Trophy {
        Trophy::from_level(self.trophies[cc.index()][cup.min(3)])
    }
    /// Store a finished cup. Returns true when the trophy improved.
    pub fn finish_cup(&mut self, run: &CupRun) -> bool {
        self.gp_runs += 1;
        let t = run.trophy();
        let slot = &mut self.trophies[run.cc.index()][run.cup];
        let best = &mut self.gp_best_points[run.cc.index()][run.cup];
        *best = (*best).max(run.points[0]);
        if t.level() > *slot {
            *slot = t.level();
            true
        } else {
            false
        }
    }
    /// All four cups gold in one class (`is_cc_mode_complete`).
    pub fn cc_complete(&self, cc: Cc) -> bool {
        self.trophies[cc.index()].iter().all(|t| *t == 3)
    }
    pub fn total_trophies(&self) -> u32 {
        self.trophies.iter().flatten().map(|t| u32::from(*t > 0)).sum()
    }
    pub fn record(&self, track: &str) -> Option<&TrackRecord> {
        self.records.get(track)
    }
    /// Record a finished Time Trial race (three lap times and the total).
    pub fn record_tt(&mut self, track: &str, total: u32, laps: &[u32], character: usize) -> TtOutcome {
        let rec = self.records.entry(track.to_string()).or_default();
        let mut out = TtOutcome::default();
        if rec.best_race.is_none_or(|b| total < b) {
            rec.best_race = Some(total);
            rec.best_character = Some(character);
            out.new_best_race = true;
        }
        for l in laps {
            let before = rec.best_laps.clone();
            rec.best_laps.push(*l);
            rec.best_laps.sort_unstable();
            rec.best_laps.truncate(3);
            if rec.best_laps != before && rec.best_laps.contains(l) {
                out.new_laps += 1;
            }
        }
        out
    }
}

pub fn career_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("kart_career.json")
}
pub fn load_career() -> Career {
    std::fs::read_to_string(career_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
pub fn save_career(c: &Career) {
    let path = career_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_string(c).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cup_tables_match_the_decomp_export() {
        // `public/mk64/modes.json` is generated from `gCupCourseOrder` / `gGPPointRewards` by scripts/export-mk64-modes.py.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64/modes.json");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        for (i, cup) in CUPS.iter().enumerate() {
            assert_eq!(v["cups"][i]["name"], cup.name);
            let tracks: Vec<&str> = v["cups"][i]["tracks"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
            assert_eq!(tracks, cup.tracks, "{}", cup.name);
        }
        let battle: Vec<&str> = v["battle"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        assert_eq!(battle, ARENAS);
        let points: Vec<u32> = v["gpPoints"].as_array().unwrap().iter().map(|p| p.as_u64().unwrap() as u32).collect();
        assert_eq!(points, GP_POINTS);
    }

    #[test]
    fn every_cup_track_is_a_kart_track_and_in_one_cup() {
        for cup in &CUPS {
            for dir in cup.tracks {
                let t = super::super::kart::TRACKS.iter().find(|t| t.dir == dir).unwrap_or_else(|| panic!("{dir}"));
                assert_eq!(t.cup, cup.name);
            }
        }
        assert_eq!(cup_of("rainbow_road"), Some((3, 3)));
        assert_eq!(cup_of("luigi_raceway"), Some((0, 0)));
        assert_eq!(cup_of("big_donut"), None);
    }

    #[test]
    fn grand_prix_points_standings_and_ties() {
        let mut run = CupRun::new(1, Cc::Cc150);
        assert_eq!(run.track(), Some("toads_turnpike"));
        // Race 1: CPU 2 wins, player second.
        assert_eq!(run.score(&[2, 0, 1, 3]), [6, 3, 9, 1]);
        assert_eq!(run.standings(), [2, 0, 1, 3]);
        // Race 2: player wins, then slot 1.
        run.score(&[0, 1, 2, 3]);
        assert_eq!(run.points, [15, 9, 12, 2]);
        assert_eq!(run.standings(), [0, 2, 1, 3]);
        assert_eq!(run.race, 2);
        run.score(&[3, 0, 1, 2]);
        run.score(&[0, 3, 2, 1]);
        assert_eq!(run.points, [30, 13, 16, 17]);
        assert_eq!(run.standings(), [0, 3, 2, 1]);
        assert!(run.finished());
        assert_eq!(run.track(), None);
        assert_eq!(run.points.iter().sum::<u32>(), 4 * 19);
        assert_eq!(run.score(&[0, 1, 2, 3]), [0; 4], "a finished cup scores nothing");
        // Ties: the human (slot 0) stays above a CPU on equal points, and ties show the same place.
        let mut tie = CupRun::new(0, Cc::Cc50);
        tie.points = [9, 9, 3, 3];
        assert_eq!(tie.standings(), [0, 1, 2, 3]);
        assert_eq!((tie.shown_place(0), tie.shown_place(1), tie.shown_place(2), tie.shown_place(3)), (1, 1, 3, 3));
    }

    #[test]
    fn scoring_tolerates_partial_and_duplicate_orders() {
        let mut run = CupRun::new(0, Cc::Cc100);
        // An unfinished CPU missing from the order is last; garbage slots are ignored.
        assert_eq!(run.score(&[1, 1, 7, 0, 3]), [6, 9, 1, 3]);
    }

    #[test]
    fn trophies_follow_the_player_rank_and_never_downgrade() {
        assert_eq!(Trophy::for_rank(0), Trophy::Gold);
        assert_eq!(Trophy::for_rank(1), Trophy::Silver);
        assert_eq!(Trophy::for_rank(2), Trophy::Bronze);
        assert_eq!(Trophy::for_rank(3), Trophy::None);
        let mut career = Career::default();
        let mut gold = CupRun::new(2, Cc::Cc100);
        for _ in 0..4 {
            gold.score(&[0, 1, 2, 3]);
        }
        assert_eq!(gold.trophy(), Trophy::Gold);
        assert!(career.finish_cup(&gold));
        assert_eq!(career.trophy(Cc::Cc100, 2), Trophy::Gold);
        assert_eq!(career.trophy(Cc::Cc50, 2), Trophy::None, "trophies are per class");
        let mut worse = CupRun::new(2, Cc::Cc100);
        for _ in 0..4 {
            worse.score(&[1, 2, 0, 3]);
        }
        assert_eq!(worse.trophy(), Trophy::Bronze);
        assert!(!career.finish_cup(&worse));
        assert_eq!(career.trophy(Cc::Cc100, 2), Trophy::Gold);
        assert_eq!(career.gp_runs, 2);
        assert_eq!(career.total_trophies(), 1);
        assert!(!career.cc_complete(Cc::Cc100));
        for cup in 0..4 {
            career.trophies[1][cup] = 3;
        }
        assert!(career.cc_complete(Cc::Cc100));
    }

    #[test]
    fn time_trial_records_keep_the_three_best_laps() {
        let mut c = Career::default();
        let o = c.record_tt("luigi_raceway", 3000, &[1100, 900, 1000], 0);
        assert_eq!(o, TtOutcome { new_best_race: true, new_laps: 3 });
        let o = c.record_tt("luigi_raceway", 3100, &[1050, 950, 1200], 1);
        assert!(!o.new_best_race);
        assert_eq!(o.new_laps, 2);
        let r = c.record("luigi_raceway").unwrap();
        assert_eq!(r.best_laps, vec![900, 950, 1000]);
        assert_eq!(r.best_race, Some(3000));
        assert_eq!(r.best_character, Some(0));
        assert!(c.record("moo_moo_farm").is_none());
    }

    #[test]
    fn career_round_trips_through_json() {
        let mut c = Career::default();
        c.record_tt("mario_raceway", 2800, &[900, 950, 950], 6);
        c.trophies[2][3] = 2;
        c.vs_wins[1] = 4;
        let back: Career = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn vs_hot_seat_ranks_by_time_and_scores_3_2_1_0() {
        let mut vs = VsRun::new(3);
        assert!(!vs.finished());
        vs.record(3300, 2);
        vs.record(3100, 1);
        vs.record(3500, 4);
        assert!(vs.finished());
        assert_eq!(vs.ranking(), vec![1, 0, 2]);
        assert_eq!(vs.points(), vec![2, 3, 1]);
        assert_eq!(VsRun::new(9).players, 4);
        assert_eq!(VsRun::new(0).players, 1);
    }

    #[test]
    fn setup_course_follows_mode_cup_and_race() {
        let mut s = Setup { cup: 3, race: 2, ..Default::default() };
        assert_eq!(s.course(), "banshee_boardwalk");
        s.mode = Mode::Battle;
        s.arena = 2;
        assert_eq!(s.course(), "double_deck");
        assert!(Cc::Extra.mirrors() && !Cc::Cc150.mirrors());
        assert_eq!(Cc::ALL.map(|c| c.name()), ["50cc", "100cc", "150cc", "Extra"]);
    }
}
