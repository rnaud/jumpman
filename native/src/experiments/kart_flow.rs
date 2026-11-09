//! The kart mode's session flow, engine-free: ties a [`Setup`] from the front end (`kart_menu`) to the race machinery.
//! It decides what a launch means (Grand Prix cup in progress, Time Trial against a ghost, hot-seat VS, Balloon Battle,
//! free roam), prepares the [`KartRace`] after `start` (CPUs benched for a solo Time Trial, the arena for a battle),
//! records the player's ghost, and turns the end of a race into the panel the menu shows (standings, trophy, records,
//! VS table, battle table) while keeping the career and ghost saves current.
//!
//! The view (`kart_race_view.rs`) calls [`Flow::begin`] after each `KartRace::start`, [`Flow::before_tick`] /
//! [`Flow::after_tick`] around every `KartRace::tick`, and shows what `after_tick` returns once the finish banner is done.
use super::kart_battle::{Arena, Battle, End};
use super::kart_ghost::{Ghost, GhostStore, Pose, Recorder, Sample, FLAG_DRIFT, FLAG_MIDAIR, FLAG_SPIN};
use super::kart_items::Item;
use super::kart_menu::{Panel, Screen, StandRow};
use super::kart_modes::{save_career, Career, CupRun, Mode, Setup, VsRun, ARENAS, CUPS, CUP_RACES};
use super::kart_physics::{Ground, Kart};
use super::kart_race::{Event, KartRace, Phase, RACERS};
use super::kart_stats::Cc;

/// What ended: the screen to show and its data.
pub type Ended = (Screen, Panel);

pub struct Flow {
    pub setup: Setup,
    pub career: Career,
    pub ghosts: GhostStore,
    pub cup: Option<CupRun>,
    pub vs: Option<VsRun>,
    pub battle: Option<Battle>,
    /// The ghost the Time Trial races (before this run changes the store).
    pub ghost: Option<Ghost>,
    arenas: Vec<Arena>,
    recorder: Recorder,
    ended: bool,
    /// Write the career / ghost saves (off in tests).
    pub persist: bool,
}

fn track_name(dir: &str) -> &'static str {
    super::kart::TRACKS.iter().find(|t| t.dir == dir).map(|t| t.dir).unwrap_or("")
}

impl Flow {
    pub fn new(career: Career, ghosts: GhostStore, arenas: Vec<Arena>) -> Self {
        Self { setup: Setup { mode: Mode::FreeRoam, ..Setup::default() }, career, ghosts, cup: None, vs: None, battle: None, ghost: None, arenas, recorder: Recorder::new(), ended: false, persist: true }
    }

    /// Load the saves and the arenas from `public/mk64`.
    pub fn load(root: &std::path::Path) -> Self {
        Self::new(super::kart_modes::load_career(), super::kart_ghost::load_store(), super::kart_battle::load_arenas(root))
    }

    /// A launch from the front end: a Grand Prix starts its cup, VS its table.
    pub fn launch(&mut self, setup: Setup) {
        super::kart_modes::set_current(setup.clone());
        self.cup = (setup.mode == Mode::GrandPrix).then(|| {
            let mut run = CupRun::new(setup.cup, setup.cc);
            run.race = setup.race.min(CUP_RACES - 1);
            run
        });
        self.vs = (setup.mode == Mode::Vs).then(|| VsRun::new(setup.players));
        self.setup = setup;
        self.battle = None;
    }

    /// The course the current race is on.
    pub fn course(&self) -> &'static str {
        match (&self.cup, self.setup.mode) {
            (Some(run), Mode::GrandPrix) => run.track().unwrap_or_else(|| self.setup.course()),
            _ => self.setup.course(),
        }
    }

    pub fn is_battle(&self) -> bool {
        self.setup.mode == Mode::Battle
    }
    pub fn is_time_trial(&self) -> bool {
        self.setup.mode == Mode::TimeTrials
    }

    /// Prepare a freshly started race for the mode. `seed` seeds the battle brains.
    pub fn begin(&mut self, race: &mut KartRace, player: &mut Kart, ground: &dyn Ground, seed: u32) {
        self.ended = false;
        self.battle = None;
        self.ghost = None;
        self.recorder.reset();
        race.kart_collisions = true;
        match self.setup.mode {
            Mode::TimeTrials => {
                // Alone on the course with three mushrooms (`D_800DC5E8` Time Trial start), no boxes, no bumping.
                race.benched = ((1u32 << RACERS) - 1) & !1;
                race.kart_collisions = false;
                race.boxes.clear();
                race.give_item(0, Item::TripleMushroom);
                let dir = self.course();
                self.ghost = self.ghosts.get(dir, self.setup.character).or_else(|| self.ghosts.best_on(dir));
            }
            Mode::Battle => {
                if let Some(arena) = self.arenas.iter().find(|a| a.dir == ARENAS[self.setup.arena.min(3)]).cloned() {
                    self.battle = Some(Battle::setup(race, player, &arena, ground, seed));
                }
            }
            _ => {}
        }
    }

    pub fn before_tick(&mut self, race: &mut KartRace, player: &Kart) {
        if let Some(b) = self.battle.as_mut() {
            b.before_tick(race, player);
        }
    }

    /// After `KartRace::tick`: the ghost sample, the battle rules, the end of the race. Returns the panel once.
    pub fn after_tick(&mut self, race: &mut KartRace, player: &Kart, events: &[Event]) -> Option<Ended> {
        if self.ended {
            return None;
        }
        if self.is_time_trial() && race.phase == Phase::Racing && race.racers.first().is_some_and(|r| r.finish.is_none()) {
            let flags = u8::from(player.drifting) * FLAG_DRIFT | u8::from(race.racers[0].spin > 0) * FLAG_SPIN | u8::from(player.midair) * FLAG_MIDAIR;
            self.recorder.record(race.race_tick, Sample::new(player.pos, player.yaw(), player.slip_raw(), flags));
        }
        if let Some(b) = self.battle.as_mut() {
            b.after_tick(race, events);
            if b.rules.over.is_some() {
                self.ended = true;
                let ended = self.battle_panel();
                if let Some((_, Panel::Battle { winner_slot, .. })) = &ended {
                    self.career.battles += 1;
                    if *winner_slot == 0 {
                        self.career.battle_wins += 1;
                    }
                    self.save();
                }
                return ended;
            }
            return None;
        }
        let results = race.results.as_ref()?;
        self.ended = true;
        if self.setup.mode == Mode::FreeRoam {
            return None;
        }
        Some(self.finish_race(&results.order.clone(), results.total, &results.laps.clone()))
    }

    /// Score a finished race: `order` lists the racers first place first, `total` / `laps` are the player's ticks.
    pub fn finish_race(&mut self, order: &[usize], total: u32, laps: &[u32]) -> Ended {
        let setup = self.setup.clone();
        let dir = self.course();
        let place = order.iter().position(|r| *r == 0).map_or(RACERS, |p| p + 1);
        let ended = match setup.mode {
            Mode::GrandPrix => {
                let mut run = self.cup.take().unwrap_or_else(|| CupRun::new(setup.cup, setup.cc));
                let gained = run.score(order);
                let rows = Self::rows(&run, gained);
                let ended = if run.finished() {
                    let improved = self.career.finish_cup(&run);
                    (Screen::GpTrophy, Panel::Trophy { cup: run.cup, cc: run.cc, trophy: run.trophy(), improved, rows })
                } else {
                    (Screen::GpStandings, Panel::Standings { cup: run.cup, race: run.race - 1, rows, next: run.track() })
                };
                self.cup = Some(run);
                ended
            }
            Mode::TimeTrials => {
                let character = setup.character;
                let outcome = self.career.record_tt(dir, total, laps, character);
                let vs_ghost = self.ghost.as_ref().map(|g| g.total);
                let saved = self.recorder.finish(character as u8, total).is_some_and(|g| self.ghosts.offer(dir, &g));
                let rec = self.career.record(dir).cloned().unwrap_or_default();
                (
                    Screen::TtResult,
                    Panel::TimeTrial {
                        track: track_name(dir),
                        total,
                        laps: laps.to_vec(),
                        best_race: rec.best_race,
                        best_laps: rec.best_laps,
                        new_best_race: outcome.new_best_race,
                        new_laps: outcome.new_laps,
                        ghost_saved: saved,
                        vs_ghost,
                    },
                )
            }
            Mode::Vs => {
                let mut vs = self.vs.take().unwrap_or_else(|| VsRun::new(setup.players));
                vs.record(total, place);
                if vs.finished() {
                    if let Some(w) = vs.ranking().first() {
                        self.career.vs_wins[(*w).min(3)] += 1;
                    }
                }
                let panel = Panel::Vs {
                    players: vs.players,
                    times: vs.times.clone(),
                    points: vs.points(),
                    ranking: vs.ranking(),
                    next_player: (!vs.finished()).then_some(vs.turn),
                    track: track_name(dir),
                };
                self.vs = Some(vs);
                (Screen::VsResult, panel)
            }
            // Battle ends in `after_tick`; free roam has no result screen.
            Mode::Battle | Mode::FreeRoam => (Screen::TtResult, Panel::None),
        };
        self.save();
        ended
    }

    fn rows(run: &CupRun, gained: [u32; 4]) -> Vec<StandRow> {
        run.standings()
            .iter()
            .map(|&slot| StandRow { slot, character: super::kart_setup::racer_character(slot), points: run.points[slot], gained: gained[slot], place: run.shown_place(slot) })
            .collect()
    }

    fn battle_panel(&self) -> Option<Ended> {
        let b = self.battle.as_ref()?;
        let winner_slot = b.rules.winner()?;
        let rows = b.rules.standings().into_iter().map(|s| (s, b.rules.balloons[s])).collect();
        Some((Screen::BattleResult, Panel::Battle { rows, winner_slot, time_up: b.rules.over == Some(End::TimeUp), arena: ARENAS[self.setup.arena.min(3)] }))
    }

    /// After a result screen's "continue": the next race of the cup, or the next VS player. False when there is none.
    pub fn next_race(&mut self) -> bool {
        match self.setup.mode {
            Mode::GrandPrix => {
                let Some(run) = self.cup.as_ref().filter(|r| !r.finished()) else { return false };
                self.setup.race = run.race;
                true
            }
            Mode::Vs => self.vs.as_ref().is_some_and(|v| !v.finished()),
            _ => true,
        }
    }

    /// The ghost's pose at the given (fractional) race tick.
    pub fn ghost_pose(&self, race_tick: f64) -> Option<Pose> {
        self.ghost.as_ref()?.pose_at(race_tick)
    }

    /// HUD line for the mode (balloons, cup race, VS player), if any.
    pub fn status(&self) -> String {
        match self.setup.mode {
            Mode::GrandPrix => self.cup.as_ref().map_or(String::new(), |r| format!("{} Cup  race {}/{}  {}", CUPS[r.cup].name, (r.race + 1).min(CUP_RACES), CUP_RACES, r.cc.name())),
            Mode::Vs => self.vs.as_ref().map_or(String::new(), |v| format!("VS  player {}/{}", (v.turn + 1).min(v.players), v.players)),
            Mode::TimeTrials => match &self.ghost {
                Some(g) => format!("Time Trial  ghost {}", super::kart_race::format_ticks(g.total)),
                None => "Time Trial".into(),
            },
            Mode::Battle => self.battle.as_ref().map_or(String::new(), |b| {
                let left = b.rules.time_left() / 30;
                format!("Battle  balloons {}  {}:{:02}", "o".repeat(b.rules.balloons[0] as usize), left / 60, left % 60)
            }),
            Mode::FreeRoam => String::new(),
        }
    }

    fn save(&self) {
        if self.persist {
            save_career(&self.career);
            super::kart_ghost::save_store(&self.ghosts);
        }
    }
}

static FLOW: std::sync::Mutex<Option<Flow>> = std::sync::Mutex::new(None);

/// The process-wide session (the kart experiment is rebuilt for every course load; the cup in progress, the career
/// and the ghosts live here). `root` is `public/mk64`, read on first use.
pub fn with<R>(root: &std::path::Path, f: impl FnOnce(&mut Flow) -> R) -> R {
    let mut guard = FLOW.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(|| Flow::load(root)))
}

/// The class of the current setup (physics reads it through `kart_setup`; kept here for the menu's labels).
pub fn class_of(setup: &Setup) -> Cc {
    setup.cc
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::kart_physics::{Floor, Input, Wall};

    struct Flat;
    impl Ground for Flat {
        fn floor(&self, _x: f64, _y: f64, _z: f64) -> Option<Floor> {
            Some(Floor { height: 0., normal: [0., 1., 0.], grass: false })
        }
        fn wall(&self, _pos: [f64; 3]) -> Option<Wall> {
            None
        }
    }
    fn ring(r: f64) -> Vec<[f64; 3]> {
        (0..32).map(|k| { let a = k as f64 / 32. * std::f64::consts::TAU; [r * a.sin(), 0., r * a.cos()] }).collect()
    }
    fn flow() -> Flow {
        let mut f = Flow::new(Career::default(), GhostStore::default(), vec![]);
        f.persist = false;
        f
    }
    fn setup(mode: Mode) -> Setup {
        Setup { mode, ..Default::default() }
    }

    #[test]
    fn a_grand_prix_scores_four_races_and_awards_the_trophy() {
        let mut f = flow();
        f.launch(Setup { cup: 2, ..setup(Mode::GrandPrix) });
        assert_eq!(f.course(), "wario_stadium");
        let mut last = None;
        for race in 0..4 {
            // The player wins three races and is second in the last: 9 + 9 + 9 + 6 = 33 against CPU 1's 27.
            let order: [usize; 4] = if race < 3 { [0, 1, 2, 3] } else { [1, 0, 2, 3] };
            let ended = f.finish_race(&order, 3000, &[1000; 3]);
            if race < 3 {
                let (Screen::GpStandings, Panel::Standings { race: r, next, rows, .. }) = &ended else { panic!("{ended:?}") };
                assert_eq!(*r, race);
                assert_eq!(*next, Some(CUPS[2].tracks[race + 1]));
                assert_eq!(rows.len(), 4);
                assert!(f.next_race());
                assert_eq!(f.course(), CUPS[2].tracks[race + 1]);
            }
            last = Some(ended);
        }
        let (Screen::GpTrophy, Panel::Trophy { trophy, improved, rows, cc, .. }) = last.unwrap() else { panic!() };
        assert_eq!(cc, Cc::Cc150);
        assert_eq!(rows.iter().map(|r| r.points).sum::<u32>(), 4 * 19);
        assert_eq!((rows[0].slot, rows[0].points, rows[1].slot, rows[1].points), (0, 33, 1, 27));
        assert_eq!(trophy, super::super::kart_modes::Trophy::Gold);
        assert!(improved);
        assert_eq!(f.career.trophy(Cc::Cc150, 2), super::super::kart_modes::Trophy::Gold);
        assert!(!f.next_race(), "the cup is over");
    }

    #[test]
    fn time_trial_benches_the_cpus_records_and_keeps_only_better_ghosts() {
        let mut f = flow();
        f.launch(Setup { cup: 0, race: 0, ..setup(Mode::TimeTrials) });
        let mut race = KartRace::new(&ring(300.), 3);
        let mut player = Kart::new([0.; 3], 0);
        race.start(&mut player, &Flat);
        f.begin(&mut race, &mut player, &Flat, 1);
        assert_eq!(race.benched, 0b1110);
        assert!(race.boxes.is_empty());
        assert_eq!(race.racers[0].item, Some(Item::TripleMushroom));
        let cpu_start: Vec<[f64; 3]> = race.cpus.iter().map(|c| c.kart.pos).collect();
        // Drive straight for a while: the ghost recorder samples the run, the CPUs stay on the grid.
        for _ in 0..400 {
            race.tick(&Input { raw_y: 80, accel: true, ..Default::default() }, false, &mut player, &Flat);
            let ev = race.drain_events();
            assert!(f.after_tick(&mut race, &player, &ev).is_none());
        }
        assert_eq!(race.cpus.iter().map(|c| c.kart.pos).collect::<Vec<_>>(), cpu_start, "benched CPUs do not move");
        let (screen, panel) = f.finish_race(&[0, 1, 2, 3], 2900, &[1000, 950, 950]);
        assert_eq!(screen, Screen::TtResult);
        let Panel::TimeTrial { new_best_race, ghost_saved, vs_ghost, .. } = panel else { panic!() };
        assert!(new_best_race && ghost_saved && vs_ghost.is_none());
        assert_eq!(f.career.record("luigi_raceway").unwrap().best_race, Some(2900));
        assert!(f.ghosts.get("luigi_raceway", 0).is_some());
        // A second, slower run keeps the first ghost; the next start races it.
        race.start(&mut player, &Flat);
        f.begin(&mut race, &mut player, &Flat, 1);
        assert_eq!(f.ghost.as_ref().map(|g| g.total), Some(2900));
        let pose = f.ghost_pose(100.).expect("the ghost has a pose");
        assert!(pose.pos[2].abs() > 1. || pose.pos[0].abs() > 1.);
        let (_, panel) = f.finish_race(&[0, 1, 2, 3], 3100, &[1100; 3]);
        let Panel::TimeTrial { new_best_race, ghost_saved, vs_ghost, .. } = panel else { panic!() };
        assert!(!new_best_race && !ghost_saved);
        assert_eq!(vs_ghost, Some(2900));
        assert_eq!(f.ghosts.get("luigi_raceway", 0).unwrap().total, 2900);
    }

    #[test]
    fn vs_passes_the_pad_and_ranks_by_time() {
        let mut f = flow();
        f.launch(Setup { players: 2, cup: 1, race: 3, ..setup(Mode::Vs) });
        assert_eq!(f.course(), "mario_raceway");
        let (_, Panel::Vs { next_player, .. }) = f.finish_race(&[0, 1, 2, 3], 3300, &[1100; 3]) else { panic!() };
        assert_eq!(next_player, Some(1));
        assert!(f.next_race());
        let (_, Panel::Vs { next_player, ranking, points, .. }) = f.finish_race(&[1, 0, 2, 3], 3200, &[1000; 3]) else { panic!() };
        assert_eq!((next_player, ranking, points), (None, vec![1, 0], vec![2, 3]));
        assert!(!f.next_race());
        assert_eq!(f.career.vs_wins[1], 1);
    }

    fn arena() -> Arena {
        Arena {
            dir: "block_fort".into(),
            name: "Block Fort".into(),
            grid: vec![([0., 0., 200.], 32768), ([0., 0., -200.], 0), ([-200., 0., 0.], -16384), ([200., 0., 0.], 16384)],
            boxes: vec![[100., 0., 100.], [-100., 0., -100.]],
        }
    }

    #[test]
    fn a_battle_ends_with_a_panel_when_the_clock_runs_out() {
        let mut f = Flow::new(Career::default(), GhostStore::default(), vec![Arena { dir: "block_fort".into(), ..arena() }]);
        f.persist = false;
        f.launch(Setup { arena: 1, ..setup(Mode::Battle) });
        assert!(f.is_battle() && f.course() == "block_fort");
        let mut race = KartRace::new(&ring(200.), 9);
        let mut player = Kart::new([0.; 3], 0);
        race.start(&mut player, &Flat);
        f.begin(&mut race, &mut player, &Flat, 5);
        f.battle.as_mut().unwrap().rules.limit = 150;
        let mut ended = None;
        for _ in 0..400 {
            f.before_tick(&mut race, &player);
            race.tick(&Input::default(), false, &mut player, &Flat);
            let ev = race.drain_events();
            if let Some(e) = f.after_tick(&mut race, &player, &ev) {
                assert!(ended.is_none(), "the panel comes once");
                ended = Some(e);
            }
        }
        let (Screen::BattleResult, Panel::Battle { rows, time_up, arena, .. }) = ended.expect("the battle ended") else { panic!() };
        assert!(time_up);
        assert_eq!(arena, "block_fort");
        assert_eq!(rows.len(), 4);
        assert_eq!(f.career.battles, 1);
    }
}
