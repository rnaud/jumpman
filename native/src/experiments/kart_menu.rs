//! The MK64-style front end of the kart mode as a pure state machine (no engine): mode select, players (VS),
//! CC class, racer select, cup select, course select and arena select, the in-race pause menu and the result panels
//! (Grand Prix standings, trophy ceremony, Time Trial, VS, Battle). `kart_menu_ui.rs` draws a [`View`] with bevy_ui
//! and feeds it [`Cmd`]s from keyboard, gamepad, mouse and touch; the kart experiment acts on the [`Outcome`]s.
//!
//! Screen order per mode (the order of the original `menus.c` front end: mode, CC class, character, cup, course):
//! Grand Prix: CC, racer, cup. Time Trials: racer, cup, course. VS: players, CC, racer, cup, course.
//! Battle: racer, arena. Free Roam: cup, course.
use super::kart_modes::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Mode,
    Players,
    Cc,
    Character,
    Cup,
    Course,
    Arena,
    Pause,
    GpStandings,
    GpTrophy,
    TtResult,
    VsResult,
    BattleResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
    /// A tap or click on entry `i` (moves the cursor there and accepts).
    Pick(usize),
}

/// What the menu asks the kart mode to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    Launch(Setup),
    Resume,
    Retry,
    NextRace,
    /// Back to the mode select (the session ends).
    ToMenu,
    /// Pause menu: pick another course / arena for the same mode.
    ChangeCourse,
    /// Leave the kart mode altogether (the hub's own Leave Mode does the same).
    Leave,
}

/// A row of a Grand Prix standings table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandRow {
    /// Racer slot (0 = the player) and its character id.
    pub slot: usize,
    pub character: usize,
    pub points: u32,
    pub gained: u32,
    /// Place shown (ties share a number).
    pub place: usize,
}

/// Data of the result panels, filled by the kart flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Panel {
    None,
    Standings { cup: usize, race: usize, rows: Vec<StandRow>, next: Option<&'static str> },
    Trophy { cup: usize, cc: Cc, trophy: Trophy, improved: bool, rows: Vec<StandRow> },
    TimeTrial { track: &'static str, total: u32, laps: Vec<u32>, best_race: Option<u32>, best_laps: Vec<u32>, new_best_race: bool, new_laps: usize, ghost_saved: bool, vs_ghost: Option<u32> },
    Vs { players: usize, times: Vec<Option<u32>>, points: Vec<u32>, ranking: Vec<usize>, next_player: Option<usize>, track: &'static str },
    Battle { rows: Vec<(usize, u8)>, winner_slot: usize, time_up: bool, arena: &'static str },
}

/// One selectable entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    pub sub: String,
    /// A picture in `public/mk64` (portrait, minimap thumbnail).
    pub image: Option<String>,
    /// Small tag (trophy, best time).
    pub badge: String,
}
impl Entry {
    fn new(label: impl Into<String>, sub: impl Into<String>) -> Self {
        Self { label: label.into(), sub: sub.into(), image: None, badge: String::new() }
    }
    fn image(mut self, f: impl Into<String>) -> Self {
        self.image = Some(f.into());
        self
    }
    fn badge(mut self, b: impl Into<String>) -> Self {
        self.badge = b.into();
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    List,
    /// Grid with this many columns.
    Grid(usize),
}

/// What the UI draws.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub screen: Screen,
    pub title: String,
    pub subtitle: String,
    pub layout: Layout,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub can_back: bool,
    /// Selected racer / cup shown beside the list (character id).
    pub preview: Option<usize>,
}

pub struct Menu {
    pub screen: Screen,
    pub setup: Setup,
    pub cursor: usize,
    pub panel: Panel,
    stack: Vec<(Screen, usize)>,
    pub career: Career,
    /// Time Trial course picker: best times shown on the entries.
    pub in_race: bool,
}

/// Screens a mode walks through after the mode select (the last one launches).
pub fn flow(mode: Mode) -> &'static [Screen] {
    match mode {
        Mode::GrandPrix => &[Screen::Cc, Screen::Character, Screen::Cup],
        Mode::TimeTrials => &[Screen::Character, Screen::Cup, Screen::Course],
        Mode::Vs => &[Screen::Players, Screen::Cc, Screen::Character, Screen::Cup, Screen::Course],
        Mode::Battle => &[Screen::Character, Screen::Arena],
        Mode::FreeRoam => &[Screen::Cup, Screen::Course],
    }
}

impl Menu {
    pub fn new(setup: Setup, career: Career) -> Self {
        Self { screen: Screen::Mode, setup, cursor: 0, panel: Panel::None, stack: vec![], career, in_race: false }
    }

    pub fn open(&mut self, screen: Screen) {
        self.screen = screen;
        self.cursor = self.default_cursor();
        self.stack.clear();
    }
    pub fn show_panel(&mut self, screen: Screen, panel: Panel) {
        self.screen = screen;
        self.panel = panel;
        self.cursor = 0;
        self.stack.clear();
    }

    fn default_cursor(&self) -> usize {
        match self.screen {
            Screen::Mode => Mode::ALL.iter().position(|m| *m == self.setup.mode).unwrap_or(0),
            Screen::Players => self.setup.players.clamp(1, 4) - 1,
            Screen::Cc => self.setup.cc.index(),
            Screen::Character => self.setup.character.min(7),
            Screen::Cup => self.setup.cup.min(3),
            Screen::Course => self.setup.race.min(3),
            Screen::Arena => self.setup.arena.min(3),
            _ => 0,
        }
    }

    fn count(&self) -> usize {
        match self.screen {
            Screen::Mode => Mode::ALL.len(),
            Screen::Players | Screen::Cc | Screen::Cup | Screen::Course | Screen::Arena => 4,
            Screen::Character => 8,
            Screen::Pause => 4,
            Screen::GpStandings | Screen::GpTrophy | Screen::VsResult | Screen::BattleResult => 2,
            Screen::TtResult => 2,
        }
    }

    fn cols(&self) -> usize {
        match self.screen {
            Screen::Character => 4,
            Screen::Cup | Screen::Course | Screen::Arena => 4,
            _ => 1,
        }
    }

    pub fn view(&self) -> View {
        let career = &self.career;
        let (title, subtitle, entries, layout): (String, String, Vec<Entry>, Layout) = match self.screen {
            Screen::Mode => (
                "MARIO KART 64".into(),
                format!("Trophies {}/16", career.total_trophies()),
                Mode::ALL.iter().map(|m| Entry::new(m.name(), m.blurb())).collect(),
                Layout::List,
            ),
            Screen::Players => (
                "PLAYERS".into(),
                "Take turns on one screen: each player races the CPU karts, best time wins".into(),
                (1..=4).map(|n| Entry::new(format!("{n} player{}", if n > 1 { "s" } else { "" }), if n == 1 { "just you" } else { "pass the pad" })).collect(),
                Layout::List,
            ),
            Screen::Cc => (
                "ENGINE CLASS".into(),
                format!("{}", self.setup.mode.name()),
                Cc::ALL
                    .iter()
                    .map(|c| {
                        let gold = (0..4).filter(|cup| career.trophy(*c, *cup) == Trophy::Gold).count();
                        Entry::new(c.name(), if *c == Cc::Extra { "mirrored 100cc" } else { "" }).badge(if gold > 0 { format!("{gold} gold") } else { String::new() })
                    })
                    .collect(),
                Layout::List,
            ),
            Screen::Character => (
                "CHOOSE YOUR DRIVER".into(),
                ROSTER[self.setup.character.min(7)].name.to_string(),
                ROSTER.iter().map(|r| Entry::new(r.name, "").image(format!("hud-portrait-{}.png", r.face))).collect(),
                Layout::Grid(4),
            ),
            Screen::Cup => (
                "CHOOSE A CUP".into(),
                self.setup.mode.name().to_string(),
                CUPS.iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let t = career.trophy(self.setup.cc, i);
                        let e = Entry::new(format!("{} Cup", c.name), c.tracks.iter().filter_map(|d| track_name(d)).collect::<Vec<_>>().join(", "));
                        e.image(format!("hud-minimap-{}.png", c.tracks[0].replace('_', "-"))).badge(if t == Trophy::None { String::new() } else { t.name().to_string() })
                    })
                    .collect(),
                Layout::Grid(4),
            ),
            Screen::Course => (
                "CHOOSE A COURSE".into(),
                format!("{} Cup", CUPS[self.setup.cup.min(3)].name),
                CUPS[self.setup.cup.min(3)]
                    .tracks
                    .iter()
                    .map(|d| {
                        let best = career.record(d).and_then(|r| r.best_race).map(|t| super::kart_race::format_ticks(t)).unwrap_or_default();
                        Entry::new(track_name(d).unwrap_or(d), "").image(format!("hud-minimap-{}.png", d.replace('_', "-"))).badge(best)
                    })
                    .collect(),
                Layout::Grid(4),
            ),
            Screen::Arena => (
                "CHOOSE AN ARENA".into(),
                "Balloon Battle".into(),
                ARENAS.iter().zip(ARENA_NAMES).map(|(d, n)| Entry::new(n, "3 balloons each").image(format!("hud-minimap-{}.png", d.replace('_', "-")))).collect(),
                Layout::Grid(4),
            ),
            Screen::Pause => (
                "PAUSED".into(),
                self.setup.mode.name().to_string(),
                vec![
                    Entry::new("Resume", ""),
                    Entry::new("Restart", if self.setup.mode == Mode::GrandPrix { "this race" } else { "" }),
                    Entry::new(if self.setup.mode == Mode::Battle { "Change arena" } else { "Change course" }, ""),
                    Entry::new("Kart menu", "quit to the mode select"),
                ],
                Layout::List,
            ),
            Screen::GpStandings => (
                "STANDINGS".into(),
                match &self.panel {
                    Panel::Standings { cup, race, .. } => format!("{} Cup  race {} of {}", CUPS[*cup].name, race + 1, CUP_RACES),
                    _ => String::new(),
                },
                vec![
                    Entry::new(
                        match &self.panel {
                            Panel::Standings { next: Some(n), .. } => format!("Next: {}", track_name(n).unwrap_or(n)),
                            _ => "Continue".to_string(),
                        },
                        "",
                    ),
                    Entry::new("Kart menu", "quit the cup"),
                ],
                Layout::List,
            ),
            Screen::GpTrophy => (
                "CUP RESULT".into(),
                match &self.panel {
                    Panel::Trophy { cup, cc, .. } => format!("{} Cup  {}", CUPS[*cup].name, cc.name()),
                    _ => String::new(),
                },
                vec![Entry::new("Continue", ""), Entry::new("Race the cup again", "")],
                Layout::List,
            ),
            Screen::TtResult => ("TIME TRIAL".into(), String::new(), vec![Entry::new("Retry", "race your ghost"), Entry::new("Kart menu", "")], Layout::List),
            Screen::VsResult => (
                "VS RACE".into(),
                String::new(),
                vec![
                    Entry::new(
                        match &self.panel {
                            Panel::Vs { next_player: Some(p), .. } => format!("Player {} next", p + 1),
                            _ => "Rematch".to_string(),
                        },
                        "",
                    ),
                    Entry::new("Kart menu", ""),
                ],
                Layout::List,
            ),
            Screen::BattleResult => ("BATTLE".into(), String::new(), vec![Entry::new("Rematch", ""), Entry::new("Kart menu", "")], Layout::List),
        };
        View {
            screen: self.screen,
            title,
            subtitle,
            layout,
            entries,
            cursor: self.cursor,
            can_back: !self.stack.is_empty() || self.screen == Screen::Pause,
            preview: (self.screen == Screen::Character).then_some(self.cursor.min(7)),
        }
    }

    fn goto(&mut self, screen: Screen) {
        self.stack.push((self.screen, self.cursor));
        self.screen = screen;
        self.cursor = self.default_cursor();
    }

    /// Apply the picked entry to the setup and move on (or launch).
    fn accept(&mut self) -> Outcome {
        let c = self.cursor;
        match self.screen {
            Screen::Mode => {
                self.setup.mode = Mode::ALL[c.min(Mode::ALL.len() - 1)];
                self.setup.race = 0;
                let next = flow(self.setup.mode)[0];
                self.goto(next);
                Outcome::None
            }
            Screen::Players => self.advance(|s| s.players = c + 1),
            Screen::Cc => self.advance(|s| s.cc = Cc::ALL[c.min(3)]),
            Screen::Character => self.advance(|s| s.character = c.min(7)),
            Screen::Cup => self.advance(|s| {
                s.cup = c.min(3);
                s.race = 0;
            }),
            Screen::Course => self.advance(|s| s.race = c.min(3)),
            Screen::Arena => self.advance(|s| s.arena = c.min(3)),
            Screen::Pause => match c {
                0 => Outcome::Resume,
                1 => Outcome::Retry,
                2 => Outcome::ChangeCourse,
                _ => Outcome::ToMenu,
            },
            Screen::GpStandings => {
                if c == 0 {
                    Outcome::NextRace
                } else {
                    Outcome::ToMenu
                }
            }
            Screen::GpTrophy => {
                if c == 0 {
                    Outcome::ToMenu
                } else {
                    Outcome::Launch(Setup { race: 0, ..self.setup.clone() })
                }
            }
            Screen::TtResult | Screen::BattleResult => {
                if c == 0 {
                    Outcome::Retry
                } else {
                    Outcome::ToMenu
                }
            }
            Screen::VsResult => {
                if c == 0 {
                    Outcome::NextRace
                } else {
                    Outcome::ToMenu
                }
            }
        }
    }

    fn advance(&mut self, apply: impl FnOnce(&mut Setup)) -> Outcome {
        apply(&mut self.setup);
        // Changing the course from the pause menu goes straight back to launching.
        let order = flow(self.setup.mode);
        match order.iter().position(|s| *s == self.screen) {
            Some(i) if i + 1 < order.len() => {
                self.goto(order[i + 1]);
                Outcome::None
            }
            _ => Outcome::Launch(self.setup.clone()),
        }
    }

    pub fn press(&mut self, cmd: Cmd) -> Outcome {
        let n = self.count();
        let cols = self.cols();
        let wrap = |v: isize| v.rem_euclid(n as isize) as usize;
        match cmd {
            Cmd::Up => self.cursor = if cols > 1 && self.cursor >= cols { self.cursor - cols } else if cols > 1 { self.cursor } else { wrap(self.cursor as isize - 1) },
            Cmd::Down => self.cursor = if cols > 1 && self.cursor + cols < n { self.cursor + cols } else if cols > 1 { self.cursor } else { wrap(self.cursor as isize + 1) },
            Cmd::Left => self.cursor = if cols > 1 { wrap(self.cursor as isize - 1) } else { self.cursor },
            Cmd::Right => self.cursor = if cols > 1 { wrap(self.cursor as isize + 1) } else { self.cursor },
            Cmd::Pick(i) => {
                if i < n {
                    self.cursor = i;
                    return self.accept();
                }
            }
            Cmd::Accept => return self.accept(),
            Cmd::Back => {
                if let Some((s, c)) = self.stack.pop() {
                    self.screen = s;
                    self.cursor = c;
                } else if self.screen == Screen::Pause {
                    return Outcome::Resume;
                }
            }
        }
        Outcome::None
    }
}

/// The lines of a result panel (the front end prints them above the entries).
pub fn panel_lines(panel: &Panel) -> Vec<String> {
    use super::kart_race::format_ticks as t;
    let name = |character: usize| super::kart_stats::CHARACTERS[character.min(7)];
    let mut out = vec![];
    match panel {
        Panel::None => {}
        Panel::Standings { rows, .. } | Panel::Trophy { rows, .. } => {
            for r in rows {
                out.push(format!("{}. {:<7} {:>3} pts  (+{})", r.place, name(r.character), r.points, r.gained));
            }
            if let Panel::Trophy { trophy, improved, .. } = panel {
                out.push(format!("Trophy: {}{}", trophy.name(), if *improved { "  NEW" } else { "" }));
            }
        }
        Panel::TimeTrial { total, laps, best_race, best_laps, new_best_race, new_laps, ghost_saved, vs_ghost, .. } => {
            out.push(format!("Time {}{}", t(*total), if *new_best_race { "  NEW RECORD" } else { "" }));
            for (i, l) in laps.iter().enumerate() {
                out.push(format!("Lap {} {}", i + 1, t(*l)));
            }
            if let Some(b) = best_race {
                out.push(format!("Record {}", t(*b)));
            }
            if !best_laps.is_empty() {
                out.push(format!("Best laps {}", best_laps.iter().map(|l| t(*l)).collect::<Vec<_>>().join("  ")));
            }
            if *new_laps > 0 {
                out.push(format!("{new_laps} new best lap{}", if *new_laps > 1 { "s" } else { "" }));
            }
            if let Some(g) = vs_ghost {
                out.push(format!("Ghost {} ({})", t(*g), if total < g { "beaten" } else { "not beaten" }));
            }
            if *ghost_saved {
                out.push("Ghost saved".to_string());
            }
        }
        Panel::Vs { ranking, times, points, .. } => {
            for (rank, who) in ranking.iter().enumerate() {
                let time = times.get(*who).copied().flatten().map_or("--".to_string(), t);
                out.push(format!("{}. Player {}  {}  {} pts", rank + 1, who + 1, time, points.get(*who).copied().unwrap_or(0)));
            }
        }
        Panel::Battle { rows, winner_slot, time_up, .. } => {
            out.push(if *time_up { "Time up".to_string() } else { "Last kart standing".to_string() });
            for (place, (slot, balloons)) in rows.iter().enumerate() {
                let who = if *slot == 0 { "You".to_string() } else { name(super::kart_setup::racer_character(*slot)).to_string() };
                out.push(format!("{}. {:<7} {}{}", place + 1, who, "o".repeat(*balloons as usize), if slot == winner_slot { "  WINNER" } else { "" }));
            }
        }
    }
    out
}

fn track_name(dir: &str) -> Option<&'static str> {
    super::kart::TRACKS.iter().find(|t| t.dir == dir).map(|t| t.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Menu {
        Menu::new(Setup::default(), Career::default())
    }
    fn press_all(m: &mut Menu, cmds: &[Cmd]) -> Outcome {
        let mut out = Outcome::None;
        for c in cmds {
            out = m.press(*c);
        }
        out
    }

    #[test]
    fn grand_prix_flow_collects_cc_driver_and_cup() {
        let mut m = menu();
        assert_eq!(m.view().screen, Screen::Mode);
        assert_eq!(m.view().entries.len(), 5);
        m.press(Cmd::Accept); // Grand Prix
        assert_eq!(m.screen, Screen::Cc);
        m.press(Cmd::Pick(2)); // 150cc
        assert_eq!(m.screen, Screen::Character);
        assert_eq!(m.view().layout, Layout::Grid(4));
        assert_eq!(m.view().entries.len(), 8);
        assert_eq!(m.view().entries[6].image.as_deref(), Some("hud-portrait-peach.png"));
        m.press(Cmd::Right);
        m.press(Cmd::Down);
        assert_eq!(m.cursor, 5, "right then down in the 4 x 2 grid lands on Wario");
        let out = press_all(&mut m, &[Cmd::Accept, Cmd::Pick(3)]);
        let Outcome::Launch(s) = out else { panic!("{out:?}") };
        assert_eq!((s.mode, s.cc, s.character, s.cup, s.race), (Mode::GrandPrix, Cc::Cc150, 5, 3, 0));
        assert_eq!(s.course(), "dks_jungle_parkway");
    }

    #[test]
    fn time_trial_vs_battle_and_free_roam_flows() {
        let mut m = menu();
        m.press(Cmd::Pick(1)); // Time Trials
        assert_eq!(m.screen, Screen::Character);
        m.press(Cmd::Pick(1)); // Luigi
        assert_eq!(m.screen, Screen::Cup);
        m.press(Cmd::Pick(1)); // Flower
        assert_eq!(m.screen, Screen::Course);
        assert_eq!(m.view().subtitle, "Flower Cup");
        let Outcome::Launch(s) = m.press(Cmd::Pick(3)) else { panic!() };
        assert_eq!((s.mode, s.character, s.course()), (Mode::TimeTrials, 1, "mario_raceway"));

        let mut m = menu();
        m.press(Cmd::Pick(2)); // VS
        assert_eq!(m.screen, Screen::Players);
        m.press(Cmd::Pick(2)); // 3 players
        assert_eq!(m.screen, Screen::Cc);
        m.press(Cmd::Pick(0));
        m.press(Cmd::Pick(0));
        m.press(Cmd::Pick(0));
        let Outcome::Launch(s) = m.press(Cmd::Pick(0)) else { panic!() };
        assert_eq!((s.mode, s.players, s.cc), (Mode::Vs, 3, Cc::Cc50));

        let mut m = menu();
        m.press(Cmd::Pick(3)); // Battle
        assert_eq!(m.screen, Screen::Character);
        m.press(Cmd::Pick(0));
        assert_eq!(m.screen, Screen::Arena);
        assert_eq!(m.view().entries[0].label, "Big Donut");
        let Outcome::Launch(s) = m.press(Cmd::Pick(2)) else { panic!() };
        assert_eq!((s.mode, s.course()), (Mode::Battle, "double_deck"));

        let mut m = menu();
        m.press(Cmd::Pick(4)); // Free roam
        assert_eq!(m.screen, Screen::Cup);
        m.press(Cmd::Pick(2));
        let Outcome::Launch(s) = m.press(Cmd::Pick(0)) else { panic!() };
        assert_eq!((s.mode, s.course()), (Mode::FreeRoam, "wario_stadium"));
    }

    #[test]
    fn back_restores_the_previous_screen_and_cursor_and_lists_wrap() {
        let mut m = menu();
        m.press(Cmd::Down);
        m.press(Cmd::Down);
        assert_eq!(m.cursor, 2);
        m.press(Cmd::Accept); // VS -> Players
        assert_eq!(m.screen, Screen::Players);
        assert!(m.view().can_back);
        m.press(Cmd::Back);
        assert_eq!((m.screen, m.cursor), (Screen::Mode, 2));
        for _ in 0..3 {
            m.press(Cmd::Down);
        }
        assert_eq!(m.cursor, 0, "wraps past the last mode");
        m.press(Cmd::Up);
        assert_eq!(m.cursor, 4);
        assert!(!menu().view().can_back);
        // Back on the first screen does nothing.
        assert_eq!(menu().press(Cmd::Back), Outcome::None);
    }

    #[test]
    fn pause_menu_outcomes() {
        let mut m = menu();
        m.in_race = true;
        m.open(Screen::Pause);
        assert!(m.view().can_back);
        assert_eq!(m.press(Cmd::Back), Outcome::Resume);
        assert_eq!(m.press(Cmd::Pick(0)), Outcome::Resume);
        assert_eq!(m.press(Cmd::Pick(1)), Outcome::Retry);
        assert_eq!(m.press(Cmd::Pick(2)), Outcome::ChangeCourse);
        assert_eq!(m.press(Cmd::Pick(3)), Outcome::ToMenu);
    }

    #[test]
    fn result_panels_offer_next_retry_and_menu() {
        let mut m = menu();
        m.show_panel(Screen::GpStandings, Panel::Standings { cup: 0, race: 1, rows: vec![], next: Some("koopa_troopa_beach") });
        assert_eq!(m.view().subtitle, "Mushroom Cup  race 2 of 4");
        assert_eq!(m.view().entries[0].label, "Next: Koopa Troopa Beach");
        assert_eq!(m.press(Cmd::Accept), Outcome::NextRace);
        assert_eq!(m.press(Cmd::Pick(1)), Outcome::ToMenu);
        m.show_panel(Screen::GpTrophy, Panel::Trophy { cup: 3, cc: Cc::Cc150, trophy: Trophy::Gold, improved: true, rows: vec![] });
        assert_eq!(m.view().subtitle, "Special Cup  150cc");
        assert_eq!(m.press(Cmd::Pick(0)), Outcome::ToMenu);
        m.setup.mode = Mode::GrandPrix;
        let Outcome::Launch(s) = m.press(Cmd::Pick(1)) else { panic!() };
        assert_eq!(s.race, 0, "racing the cup again starts at race 1");
        m.show_panel(Screen::TtResult, Panel::None);
        assert_eq!(m.press(Cmd::Pick(0)), Outcome::Retry);
        m.show_panel(Screen::BattleResult, Panel::None);
        assert_eq!(m.press(Cmd::Pick(1)), Outcome::ToMenu);
        m.show_panel(Screen::VsResult, Panel::Vs { players: 2, times: vec![], points: vec![], ranking: vec![], next_player: Some(1), track: "luigi_raceway" });
        assert_eq!(m.view().entries[0].label, "Player 2 next");
    }

    #[test]
    fn panels_print_their_tables() {
        let lines = panel_lines(&Panel::TimeTrial { track: "luigi_raceway", total: 3000, laps: vec![1000; 3], best_race: Some(2900), best_laps: vec![900], new_best_race: false, new_laps: 0, ghost_saved: false, vs_ghost: Some(2900) });
        assert_eq!(lines[0], "Time 1:40.00");
        assert!(lines.iter().any(|l| l.contains("not beaten")));
        let lines = panel_lines(&Panel::Battle { rows: vec![(2, 1), (0, 0)], winner_slot: 2, time_up: false, arena: "big_donut" });
        assert!(lines[1].contains("WINNER") && lines[2].starts_with("2. You"));
        let lines = panel_lines(&Panel::Vs { players: 2, times: vec![Some(3300), Some(3100)], points: vec![2, 3], ranking: vec![1, 0], next_player: None, track: "luigi_raceway" });
        assert!(lines[0].starts_with("1. Player 2"));
    }

    #[test]
    fn cup_and_course_screens_show_trophies_and_best_times() {
        let mut career = Career::default();
        career.trophies[2][1] = 3;
        career.record_tt("moo_moo_farm", 2700, &[900, 900, 900], 0);
        let mut m = Menu::new(Setup { cc: Cc::Cc150, mode: Mode::TimeTrials, ..Default::default() }, career);
        m.open(Screen::Cup);
        let v = m.view();
        assert_eq!(v.entries[1].badge, "Gold");
        assert_eq!(v.entries[0].badge, "");
        assert!(v.entries[0].sub.contains("Luigi Raceway") && v.entries[3].sub.contains("Rainbow Road"));
        m.setup.cup = 0;
        m.open(Screen::Course);
        let v = m.view();
        assert_eq!(v.entries[1].label, "Moo Moo Farm");
        assert_eq!(v.entries[1].badge, "1:30.00");
        assert_eq!(v.entries[1].image.as_deref(), Some("hud-minimap-moo-moo-farm.png"));
        // The classes show how many cups are gold.
        m.open(Screen::Cc);
        assert_eq!(m.view().entries[2].badge, "1 gold");
    }
}
