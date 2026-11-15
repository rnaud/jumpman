//! Start choices of the modes that used to open on their own menu over a course loaded just to show it: Gem Clash (mode,
//! arena, opponents...), Star Swarm (battlefield, difficulty), Hold the Gate (map) and Sunstone Shindig (board, rounds...).
//!
//! The front end shows these rows on its MODE SETUP page before the character select, while no level is loaded. READY
//! then asks for a *direct start* (`ExperimentSlot::direct`): the shell loads the one course the choices need, and the
//! mode enters with `Shell::direct` set and starts the run with these choices right away instead of opening its own start
//! menu over the level. Starts that do not come through the setup page (test hooks, `xp:`, the dev `X` menu, a rematch)
//! are unchanged: the mode's own menu as before.
//!
//! The choices live here (process-wide, like the modes' own last-pick statics) and are written through to the modes'
//! persisted picks where they have one (Gem Clash's arena file, Star Swarm's level / tier), so the course a mode asks
//! for ([`super::course_for`]) already matches the choice.
use super::ExperimentId;
use std::sync::Mutex;

/// One row of a mode's setup: a label and its values (all drawable by the menu's bitmap font).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: &'static str,
    pub values: Vec<String>,
    pub value: usize,
    /// Values that cannot be chosen (their game was not imported, a difficulty not unlocked yet).
    pub locked: Vec<bool>,
    /// The row does not apply with the other choices (shown greyed, skipped by the cursor).
    pub enabled: bool,
}

impl Row {
    fn new(label: &'static str, values: Vec<String>, value: usize) -> Self {
        let n = values.len();
        Row { label, values, value: value.min(n.saturating_sub(1)), locked: vec![false; n], enabled: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Picks {
    /// Gem Clash: mode (0 versus, 1 gauntlet), arena, opponents (1..=3), bot level, monsters.
    brawl: Option<[u8; 5]>,
    /// Star Swarm: battlefield, difficulty tier.
    swarm: Option<(usize, usize)>,
    /// Hold the Gate: map.
    gate: usize,
    /// Sunstone Shindig: board, rounds (index into `TURN_CHOICES`), CPU level, people (1..=4), start with the mini-games.
    party: [usize; 5],
}

static PICKS: Mutex<Picks> = Mutex::new(Picks { brawl: None, swarm: None, gate: 0, party: [0, 1, 1, 1, 0] });

fn picks() -> std::sync::MutexGuard<'static, Picks> {
    PICKS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether the mode has start choices (and so a MODE SETUP page before the character select).
pub fn has_setup(id: ExperimentId) -> bool {
    matches!(id, ExperimentId::Brawl | ExperimentId::Swarm | ExperimentId::Gate | ExperimentId::Party)
}

fn brawl_picks(p: &mut Picks) -> [u8; 5] {
    *p.brawl.get_or_insert_with(|| [0, super::brawl_region::load_choice(), 2, 0, 1])
}

fn swarm_picks(p: &mut Picks) -> (usize, usize) {
    *p.swarm.get_or_insert_with(|| (super::swarm::level_index(), super::swarm::tier_index()))
}

/// The rows of a mode's setup. `course_ok(id)` says whether a course's data was imported (an arena or battlefield on a
/// course of a game that was not imported is locked).
pub fn rows(id: ExperimentId, course_ok: &dyn Fn(&str) -> bool) -> Vec<Row> {
    let mut p = picks();
    match id {
        ExperimentId::Brawl => {
            let b = brawl_picks(&mut p);
            let versus = b[0] == 0;
            let regions = super::brawl_region::REGIONS;
            let mut arena = Row::new(if versus { "ARENA" } else { "WORLD" }, regions.iter().map(|r| r.name.to_ascii_uppercase()).collect(), b[1] as usize);
            arena.locked = regions.iter().map(|r| !course_ok(r.course)).collect();
            let mut bots = Row::new("OPPONENTS", (1..=3).map(|n| n.to_string()).collect(), b[2].saturating_sub(1) as usize);
            bots.enabled = versus;
            let mut diff = Row::new("BOT LEVEL", ["EASY", "NORMAL", "HARD"].map(String::from).to_vec(), b[3] as usize);
            diff.enabled = versus;
            let monsters = Row::new("MONSTERS", super::brawl_core::Monsters::ALL.iter().map(|m| m.label().to_ascii_uppercase()).collect(), b[4] as usize);
            vec![Row::new("MODE", vec!["VERSUS".into(), "GAUNTLET".into()], b[0] as usize), arena, bots, diff, monsters]
        }
        ExperimentId::Swarm => {
            let (level, tier) = swarm_picks(&mut p);
            let levels = super::swarm_levels::LEVELS;
            let mut field = Row::new("BATTLEFIELD", levels.iter().map(|l| l.name.to_ascii_uppercase()).collect(), level);
            field.locked = levels.iter().map(|l| !course_ok(l.course)).collect();
            let meta = super::swarm::load_meta();
            let mut diff = Row::new("DIFFICULTY", (0..super::swarm_meta::TIERS).map(|i| super::swarm_meta::tier(i).name.to_string()).collect(), tier);
            diff.locked = (0..super::swarm_meta::TIERS).map(|i| !meta.tier_unlocked(i)).collect();
            vec![field, diff]
        }
        ExperimentId::Gate => vec![Row::new("MAP", super::gate_core::MAPS.iter().map(|m| m.name.to_ascii_uppercase()).collect(), p.gate)],
        ExperimentId::Party => {
            let q = p.party;
            let board = Row::new("BOARD", (0..super::party::board::BOARD_COUNT).map(|i| super::party::board::board(i).name.to_ascii_uppercase()).collect(), q[0]);
            let rounds = Row::new("ROUNDS", super::party::app::TURN_CHOICES.iter().map(|t| t.to_string()).collect(), q[1]);
            let cpu = Row::new("CPU", super::party::common::Level::ALL.iter().map(|l| l.label().to_ascii_uppercase()).collect(), q[2]);
            let people = Row::new("PEOPLE", (1..=4).map(|n| n.to_string()).collect(), q[3].saturating_sub(1));
            let start = Row::new("PLAY", vec!["BOARD PARTY".into(), "MINI-GAMES".into()], q[4]);
            vec![board, rounds, cpu, people, start]
        }
        _ => Vec::new(),
    }
}

/// Choose value `value` of row `row` (out-of-range rows / values are ignored; locked values are the caller's to refuse).
pub fn set(id: ExperimentId, row: usize, value: usize) {
    let mut p = picks();
    match id {
        ExperimentId::Brawl => {
            let mut b = brawl_picks(&mut p);
            let v = value as u8;
            match row {
                0 if v < 2 => b[0] = v,
                1 if (v as usize) < super::brawl_region::arena_count() => {
                    b[1] = v;
                    // The arena decides the course Gem Clash loads (`GemClashExperiment::course` reads the saved choice).
                    super::brawl_region::save_choice(v);
                }
                2 if v < 3 => b[2] = v + 1,
                3 if v < 3 => b[3] = v,
                4 if v < 3 => b[4] = v,
                _ => return,
            }
            p.brawl = Some(b);
        }
        ExperimentId::Swarm => {
            let (mut level, mut tier) = swarm_picks(&mut p);
            match row {
                0 if value < super::swarm_levels::LEVELS.len() => level = value,
                1 if value < super::swarm_meta::TIERS => tier = value,
                _ => return,
            }
            p.swarm = Some((level, tier));
            // The battlefield decides the course Star Swarm loads (`StarSwarmExperiment::course` reads the pick).
            super::swarm::set_pick(level, tier);
        }
        ExperimentId::Gate if row == 0 && value < super::gate_core::MAPS.len() => p.gate = value,
        ExperimentId::Party => {
            let max = [super::party::board::BOARD_COUNT, super::party::app::TURN_CHOICES.len(), 3, 4, 2];
            if row < 5 && value < max[row] {
                p.party[row] = if row == 3 { value + 1 } else { value };
            }
        }
        _ => {}
    }
}

/// Gem Clash's choices: mode, arena, opponents, bot level, monsters.
pub fn brawl() -> [u8; 5] {
    brawl_picks(&mut picks())
}

/// Hold the Gate's map.
pub fn gate_map() -> usize {
    picks().gate
}

/// Sunstone Shindig's choices: board, rounds, CPU level index, people, and whether to open the mini-games instead.
pub fn party() -> (usize, u32, usize, usize, bool) {
    let q = picks().party;
    (q[0], super::party::app::TURN_CHOICES[q[1].min(super::party::app::TURN_CHOICES.len() - 1)], q[2].min(2), q[3].clamp(1, 4), q[4] == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_modes_with_a_start_menu_have_a_setup() {
        for id in [ExperimentId::Brawl, ExperimentId::Swarm, ExperimentId::Gate, ExperimentId::Party] {
            assert!(has_setup(id));
            let rows = rows(id, &|_| true);
            assert!(!rows.is_empty(), "{id:?}");
            for r in rows {
                assert!(r.value < r.values.len() && r.locked.len() == r.values.len(), "{id:?} {r:?}");
                for v in r.values.iter().chain(std::iter::once(&r.label.to_string())) {
                    assert!(v.is_ascii() && *v == v.to_ascii_uppercase(), "{v}");
                }
            }
        }
        for id in [ExperimentId::Race, ExperimentId::Kart, ExperimentId::God] {
            assert!(!has_setup(id) && rows(id, &|_| true).is_empty());
        }
    }

    #[test]
    fn choices_are_kept_and_clamped() {
        set(ExperimentId::Gate, 0, 1);
        assert_eq!(gate_map(), 1);
        set(ExperimentId::Gate, 0, 99);
        assert_eq!(gate_map(), 1, "out of range: ignored");
        set(ExperimentId::Party, 3, 2);
        set(ExperimentId::Party, 4, 1);
        let (_, rounds, _, people, minis) = party();
        assert_eq!((people, minis), (3, true));
        assert!(super::super::party::app::TURN_CHOICES.contains(&rounds));
        assert_eq!(rows(ExperimentId::Party, &|_| true)[3].value, 2);
        set(ExperimentId::Party, 4, 0);
        // Gauntlet greys out the versus-only rows.
        let before = brawl();
        set(ExperimentId::Brawl, 0, 1);
        let r = rows(ExperimentId::Brawl, &|_| true);
        assert_eq!(r[0].value, 1);
        assert!(!r[2].enabled && !r[3].enabled && r[4].enabled);
        set(ExperimentId::Brawl, 0, before[0] as usize);
    }

    #[test]
    fn arenas_and_battlefields_of_games_not_imported_are_locked() {
        let only_sm64 = |c: &str| !c.starts_with("mk64_") && !matches!(c, "hyrule_field" | "kokiri_forest" | "graveyard");
        let r = rows(ExperimentId::Swarm, &only_sm64);
        let levels = super::super::swarm_levels::LEVELS;
        for (i, l) in levels.iter().enumerate() {
            assert_eq!(r[0].locked[i], !only_sm64(l.course), "{}", l.name);
        }
        assert!(!r[1].locked[0], "the first difficulty is always open");
    }
}
