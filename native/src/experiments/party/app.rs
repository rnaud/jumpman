//! Sunstone Shindig, the Bevy side: the experiment shell, the app state ([`PartyApp`]), the flow that plays the core's
//! events back as animations, the mini-game runner, save file and env test hooks. 3D lives in `view.rs`, screens in `ui.rs`.
//!
//! Env hooks (screenshots / tests; runs using them never touch the saved records):
//! `JUMPMAN_PARTY_BOARD=0|1|2` (start that board directly), `_MINIGAME=<key>` (practice one mini-game: barrelhop, mash, echo,
//! coins, bombs, pound, plank, lane, odd, bumper, crumble, draw, stack, stars, meter), `_SEED=<n>`, `_TURNS=10..15`,
//! `_LEVEL=easy|normal|hard`, `_AUTOPLAY=1` (the bots play every seat, including yours), `_SKIP=<rounds>` (fast-forward that many
//! rounds with bots before the first frame), `_START=title|minis|results|overview`, `_SCRIPT=2.0=roll,3.2=stop,5=choose.1,...`
//! (see [`parse_ui_cmd`]), `_FAST=1` (short animations), `_HUMANS=<n>` (seats 0..n are people; extra seats use gamepads).
use super::board::*;
use super::common::*;
use super::game::*;
use super::minis::*;
use crate::experiments::crowd::{init_clips, CrowdMario};
use crate::experiments::geometry::add_box_collision;
use crate::experiments::{Experiment, ExperimentId, Host, StepOverride};
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::state::*;
use std::collections::VecDeque;
use std::sync::Arc;

/// Far from every course object and from the other experiments.
pub const BOARD_O: [f64; 3] = [-300000.0, 1000.0, -300000.0];
pub const MINI_O: [f64; 3] = [-292000.0, 1000.0, -300000.0];
pub const PODIUM_O: [f64; 3] = [-284000.0, 1000.0, -300000.0];

/// SM64 position of a board point (board units are 100 SM64 units).
pub fn board_pos(p: [f32; 2]) -> [f64; 3] {
    [BOARD_O[0] + p[0] as f64 * 100.0, BOARD_O[1], BOARD_O[2] + p[1] as f64 * 100.0]
}

pub fn mini_pos(x: f32, z: f32) -> [f64; 3] {
    [MINI_O[0] + x as f64 * 100.0, MINI_O[1], MINI_O[2] + z as f64 * 100.0]
}

/// Podium step for place `i` (0 = winner) and its pedestal height in SM64 units.
pub fn podium_slot(i: usize) -> ([f64; 3], f64) {
    const X: [f64; 4] = [0.0, 155.0, 310.0, 465.0];
    const H: [f64; 4] = [120.0, 85.0, 55.0, 30.0];
    ([PODIUM_O[0] + X[i], PODIUM_O[1], PODIUM_O[2]], H[i])
}

/// Where a piece stands on a space (four slots so pieces on the same space do not overlap).
pub fn slot_offset(p: usize) -> [f32; 2] {
    [[-0.36, -0.3], [0.36, -0.3], [-0.36, 0.34], [0.36, 0.34]][p]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Title,
    Minis,
    Board,
    Mini,
    Results,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MStage {
    Intro,
    Count,
    Play,
    Done,
}

/// What the UI asks of the app.
#[derive(Clone, Debug, PartialEq)]
pub enum UiCmd {
    Start,
    PickBoard(usize),
    PickTurns(u32),
    PickLevel(Level),
    PickHumans(usize),
    OpenMinis,
    Practice(Mini),
    Back,
    /// A core command (item use, branch pick, keeper, shop, duel...).
    Game(Cmd),
    /// Use the item in slot `n`; the Pickpocket asks for a target first.
    UseItem(usize),
    ItemTarget(usize, usize),
    CancelItem,
    RollStart,
    RollStop,
    Overview,
    MiniGo,
    MiniAgain,
    MiniFinish,
    Ack,
    Next,
    Menu,
    /// A tile of the running mini-game was tapped (seat 0).
    Tile(usize),
    /// The big tap button went down.
    Tap,
}

pub struct Floater {
    pub p: usize,
    pub text: String,
    pub color: u32,
    pub t0: f64,
}

#[derive(Clone, Debug)]
pub struct Dice {
    pub die: Die,
    pub rolling: bool,
    pub t0: f64,
    /// Shown after stopping, until the anim ends.
    pub value: u8,
}

pub struct MiniRun {
    pub session: Session,
    pub kind: Mini,
    pub ctx: Option<MiniCtx>,
    pub practice: bool,
    pub stage: MStage,
    pub t: f32,
    pub acc: f32,
    pub inp: Inputs,
    pub scene: Stage,
    pub prev_scores: [f32; 4],
    pub human: [bool; 4],
    pub levels: [Level; 4],
    pub final_scores: [f32; 4],
    pub new_record: bool,
    pub tile_focus: usize,
}

pub struct Piece {
    pub mario: CrowdMario,
    /// The board space the piece stands on (or is walking to).
    pub at: Option<usize>,
    /// Teleport to the target on the next fixed step.
    pub snap: bool,
    /// Jump once on the next fixed step.
    pub jump_next: bool,
    pub air_prev: bool,
    pub target: Option<[f64; 3]>,
    pub hold_a: bool,
    pub hidden: bool,
    pub fx_t: f64,
}

impl Piece {
    pub fn arrived(&self) -> bool {
        match self.target {
            Some(t) => {
                let p = self.mario.runtime.pos;
                (t[0] - p[0]).hypot(t[2] - p[2]) < 70.0
            }
            None => true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Saved {
    pub games: u32,
    pub best_sun: [u32; 3],
    pub wins: u32,
    /// Best score per mini-game key, as played by a person.
    pub records: Vec<(String, f32)>,
}

pub fn save_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("party.json")
}

pub fn load_saved() -> Saved {
    let v: serde_json::Value = std::fs::read_to_string(save_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let mut s = Saved { games: v["games"].as_u64().unwrap_or(0) as u32, wins: v["wins"].as_u64().unwrap_or(0) as u32, ..Default::default() };
    for i in 0..3 {
        s.best_sun[i] = v["best_sun"][i].as_u64().unwrap_or(0) as u32;
    }
    if let Some(o) = v["records"].as_object() {
        for (k, x) in o {
            if let Some(f) = x.as_f64() {
                s.records.push((k.clone(), f as f32));
            }
        }
    }
    s
}

pub fn write_saved(s: &Saved) {
    let path = save_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut rec = serde_json::Map::new();
    for (k, v) in &s.records {
        rec.insert(k.clone(), serde_json::json!(v));
    }
    let _ = std::fs::write(path, serde_json::json!({ "games": s.games, "wins": s.wins, "best_sun": s.best_sun, "records": rec }).to_string());
}

/// Test hooks mark a run as not eligible for the records.
pub fn cheating() -> bool {
    ["JUMPMAN_PARTY_SKIP", "JUMPMAN_PARTY_AUTOPLAY", "JUMPMAN_PARTY_SCRIPT", "JUMPMAN_PARTY_START", "JUMPMAN_PARTY_SEED"].iter().any(|k| std::env::var_os(k).is_some())
}

fn env<T: std::str::FromStr>(k: &str) -> Option<T> {
    std::env::var(k).ok().and_then(|s| s.trim().parse().ok())
}

/// One scripted UI command: `roll`, `stop`, `choose.<space>`, `keeper.1`, `buy.<item 0-3>`, `leave`, `duel.<seat>`, `item.<slot>`,
/// `target.<slot>.<seat>`, `ack`, `go`, `again`, `finish`, `next`, `overview`, `tap`, `tile.<n>`, `back`, `start`, `board.<n>`,
/// `minis`, `practice.<key>`.
pub fn parse_ui_cmd(s: &str) -> Option<UiCmd> {
    let p: Vec<&str> = s.split('.').collect();
    let n = |i: usize| p.get(i).and_then(|v| v.parse::<usize>().ok());
    Some(match p[0] {
        "roll" => UiCmd::RollStart,
        "stop" => UiCmd::RollStop,
        "choose" => UiCmd::Game(Cmd::Choose(n(1)?)),
        "keeper" => UiCmd::Game(Cmd::Keeper(n(1)? != 0)),
        "buy" => UiCmd::Game(Cmd::ShopBuy(*ITEMS.get(n(1)?)?)),
        "leave" => UiCmd::Game(Cmd::ShopLeave),
        "duel" => UiCmd::Game(Cmd::DuelPick(n(1)?)),
        "item" => UiCmd::UseItem(n(1)?),
        "target" => UiCmd::ItemTarget(n(1)?, n(2)?),
        "ack" => UiCmd::Ack,
        "go" => UiCmd::MiniGo,
        "again" => UiCmd::MiniAgain,
        "finish" => UiCmd::MiniFinish,
        "next" => UiCmd::Next,
        "overview" => UiCmd::Overview,
        "tap" => UiCmd::Tap,
        "tile" => UiCmd::Tile(n(1)?),
        "back" => UiCmd::Back,
        "start" => UiCmd::Start,
        "board" => UiCmd::PickBoard(n(1)?),
        "minis" => UiCmd::OpenMinis,
        "practice" => UiCmd::Practice(Mini::from_key(p.get(1)?)?),
        _ => return None,
    })
}

#[derive(Clone, Debug)]
pub struct Anim {
    pub ev: Ev,
    pub dur: f64,
}

#[derive(Resource)]
pub struct PartyApp {
    pub t: f64,
    pub screen: Screen,
    pub cfg: Config,
    pub level: Level,
    pub game: Option<Party>,
    pub queue: VecDeque<Anim>,
    pub cur: Option<(Anim, f64)>,
    pub think: f64,
    pub mini: Option<MiniRun>,
    /// Bumps whenever the UI must be rebuilt.
    pub rev: u32,
    pub cmds: Vec<UiCmd>,
    pub banner: Option<(String, f64, u32)>,
    pub toast: Option<(String, f64)>,
    pub floaters: Vec<Floater>,
    pub dice: Option<Dice>,
    pub pending_item: Option<usize>,
    pub pieces: Vec<Piece>,
    pub world: Arc<CollisionWorld>,
    pub meta: Option<Arc<Vec<crate::game::sm64_animation::ClipMeta>>>,
    pub overview: bool,
    pub saved: Saved,
    pub autoplay: bool,
    pub fast: bool,
    pub script: Vec<(f64, UiCmd)>,
    pub humans: usize,
    pub board_built: Option<usize>,
    pub results_t0: f64,
    pub pulse: Option<(usize, f64)>,
    pub keeper_shown: Option<usize>,
    pub shake: f32,
    pub recorded: bool,
    pub focus: usize,
    /// Pieces need (re)placing for the current screen.
    pub place: bool,
    pub confetti: f64,
    pub last_places: Vec<usize>,
    pub over_since: Option<f64>,
}

impl PartyApp {
    pub fn new(world: Arc<CollisionWorld>, meta: Option<Arc<Vec<crate::game::sm64_animation::ClipMeta>>>, runs: u32) -> Self {
        let seed: u64 = env("JUMPMAN_PARTY_SEED").unwrap_or_else(|| 1000 + runs as u64 * 7919 + (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs()) % 100000));
        let level = std::env::var("JUMPMAN_PARTY_LEVEL").ok().and_then(|s| Level::from_name(&s)).unwrap_or(Level::Normal);
        let humans: usize = env("JUMPMAN_PARTY_HUMANS").unwrap_or(1).clamp(1, 4);
        let autoplay = env::<u32>("JUMPMAN_PARTY_AUTOPLAY").unwrap_or(0) != 0;
        let mut cfg = Config { board: env("JUMPMAN_PARTY_BOARD").unwrap_or(0) % BOARD_COUNT, turns: env::<u32>("JUMPMAN_PARTY_TURNS").unwrap_or(12).clamp(10, 15), seed, players: [None; 4] };
        cfg.players = std::array::from_fn(|i| if i < humans && !autoplay { None } else { Some(level) });
        let script = std::env::var("JUMPMAN_PARTY_SCRIPT")
            .ok()
            .map(|s| {
                s.split(',')
                    .filter_map(|item| {
                        let (t, c) = item.split_once('=')?;
                        Some((t.trim().parse::<f64>().ok()?, parse_ui_cmd(c.trim())?))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        PartyApp {
            t: 0.0,
            screen: Screen::Title,
            cfg,
            level,
            game: None,
            queue: VecDeque::new(),
            cur: None,
            think: 0.0,
            mini: None,
            rev: 0,
            cmds: vec![],
            banner: None,
            toast: None,
            floaters: vec![],
            dice: None,
            pending_item: None,
            pieces: vec![],
            world,
            meta,
            overview: false,
            saved: load_saved(),
            autoplay,
            fast: env::<u32>("JUMPMAN_PARTY_FAST").unwrap_or(0) != 0 || autoplay,
            script,
            humans,
            board_built: None,
            results_t0: 0.0,
            pulse: None,
            keeper_shown: None,
            shake: 0.0,
            recorded: false,
            focus: 0,
            place: true,
            confetti: 0.0,
            last_places: vec![],
            over_since: None,
        }
    }

    pub fn bump(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

    pub fn say(&mut self, text: impl Into<String>, color: u32) {
        self.banner = Some((text.into(), self.t, color));
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), self.t));
    }

    pub fn is_human(&self, seat: usize) -> bool {
        self.game.as_ref().map_or(seat == 0, |g| !g.is_cpu(seat))
    }

    // -- flow ------------------------------------------------------------------------------

    pub fn start_game(&mut self) {
        let mut cfg = self.cfg.clone();
        cfg.players = std::array::from_fn(|i| if i < self.humans && !self.autoplay { None } else { Some(self.level) });
        let mut g = Party::new(cfg);
        if let Some(skip) = env::<u32>("JUMPMAN_PARTY_SKIP") {
            // Fast-forward with bots until a human decision in the wanted round.
            let mut guard = 0;
            while !g.over() && g.round < skip + 1 && guard < 100_000 {
                g.auto_step();
                guard += 1;
            }
            while !g.over() && !(matches!(g.phase, Phase::TurnStart) && g.cur == 0) && guard < 100_000 {
                g.auto_step();
                guard += 1;
            }
            g.drain_events();
        }
        self.queue.clear();
        self.cur = None;
        self.queue.extend(g.drain_events().into_iter().map(|ev| Anim { dur: 0.0, ev }));
        self.game = Some(g);
        self.screen = Screen::Board;
        self.recorded = false;
        self.over_since = None;
        self.overview = false;
        self.dice = None;
        self.pending_item = None;
        self.place = true;
        self.board_built = None;
        self.bump();
    }

    pub fn start_practice(&mut self, kind: Mini) {
        let level = self.level;
        let mut human = [false; 4];
        for h in human.iter_mut().take(self.humans) {
            *h = true;
        }
        if self.autoplay {
            human = [false; 4];
        }
        let seed = self.cfg.seed.wrapping_add(self.t as u64 * 31 + 5);
        let session = Session::new(kind, seed, [true; 4], human, [level; 4]);
        self.mini = Some(MiniRun { session, kind, ctx: None, practice: true, stage: MStage::Intro, t: 0.0, acc: 0.0, inp: [PadIn::default(); 4], scene: Stage::default(), prev_scores: [0.0; 4], human, levels: [level; 4], final_scores: [0.0; 4], new_record: false, tile_focus: 0 });
        self.screen = Screen::Mini;
        self.place = true;
        self.bump();
    }

    pub fn start_board_mini(&mut self, ctx: MiniCtx) {
        let g = self.game.as_ref().unwrap();
        let human: [bool; 4] = std::array::from_fn(|i| !g.is_cpu(i) && ctx.active[i]);
        let levels: [Level; 4] = std::array::from_fn(|i| g.players[i].cpu.unwrap_or(self.level));
        let session = Session::new(ctx.kind, ctx.seed, ctx.active, human, levels);
        self.mini = Some(MiniRun { session, kind: ctx.kind, ctx: Some(ctx), practice: false, stage: MStage::Intro, t: 0.0, acc: 0.0, inp: [PadIn::default(); 4], scene: Stage::default(), prev_scores: [0.0; 4], human, levels, final_scores: [0.0; 4], new_record: false, tile_focus: 0 });
        self.screen = Screen::Mini;
        self.place = true;
        self.bump();
    }

    /// Core command from the UI / bots; the resulting events become animations.
    pub fn send(&mut self, cmd: Cmd) {
        let Some(g) = self.game.as_mut() else { return };
        if let Err(e) = g.apply(cmd) {
            warn!("party: rejected command: {e}");
            self.toast(e);
            return;
        }
        let evs = g.drain_events();
        let fast = self.fast;
        for ev in evs {
            let dur = anim_len(&ev, fast);
            self.queue.push_back(Anim { ev, dur });
        }
        self.bump();
    }

    pub fn busy(&self) -> bool {
        self.cur.is_some() || !self.queue.is_empty()
    }

    pub fn finish_mini_board(&mut self) {
        let Some(run) = self.mini.take() else { return };
        self.screen = Screen::Board;
        self.place = true;
        self.send(Cmd::MiniDone(run.final_scores));
    }

    pub fn record_mini(&mut self, kind: Mini, score: f32) -> bool {
        if cheating() {
            return false;
        }
        let key = kind.info().key.to_string();
        match self.saved.records.iter_mut().find(|r| r.0 == key) {
            Some(r) if r.1 >= score => false,
            Some(r) => {
                r.1 = score;
                write_saved(&self.saved);
                true
            }
            None => {
                self.saved.records.push((key, score));
                write_saved(&self.saved);
                true
            }
        }
    }

    pub fn record_game(&mut self) {
        if let Some(g) = self.game.as_ref() {
            if !self.recorded {
                let order = g.standings();
                info!(
                    "party: game over on {} after {} rounds, {} mini-games; standings {:?}; sunstones {:?}; coins {:?}",
                    g.board.name,
                    g.cfg.turns,
                    g.minis_played,
                    order,
                    g.players.iter().map(|p| p.sun).collect::<Vec<_>>(),
                    g.players.iter().map(|p| p.coins).collect::<Vec<_>>()
                );
            }
        }
        if self.recorded || cheating() {
            self.recorded = true;
            return;
        }
        self.recorded = true;
        let Some(g) = self.game.as_ref() else { return };
        self.saved.games += 1;
        if g.winners().contains(&0) && !g.is_cpu(0) {
            self.saved.wins += 1;
        }
        let b = g.cfg.board % 3;
        self.saved.best_sun[b] = self.saved.best_sun[b].max(g.players[0].sun);
        write_saved(&self.saved);
    }
}

/// Playback length of an event in seconds.
pub fn anim_len(ev: &Ev, fast: bool) -> f64 {
    let d = match ev {
        Ev::RoundStart(_) => 1.1,
        Ev::TurnStart(_) => 0.5,
        Ev::Rolled { .. } => 0.95,
        Ev::Step { .. } => 0.0, // until the piece arrives
        Ev::Landed { .. } => 0.35,
        Ev::Coins { .. } => 0.4,
        Ev::Payday { .. } => 0.4,
        Ev::Sunstone { .. } => 2.1,
        Ev::KeeperMoved { .. } => 0.9,
        Ev::Warped { .. } => 0.9,
        Ev::Gift { .. } => 0.9,
        Ev::ItemUsed { .. } => 0.7,
        Ev::Stolen { .. } => 0.8,
        Ev::Swapped { .. } => 0.9,
        Ev::Text(_) => 1.5,
        Ev::Flip(_) | Ev::Wind(_) | Ev::Tide(_) => 1.3,
        Ev::MiniStart | Ev::MiniEnd | Ev::GameOver | Ev::Bonus { .. } => 0.0,
    };
    if fast {
        d * 0.15
    } else {
        d
    }
}

// ---------------------------------------------------------------------------------------------
// Experiment shell
// ---------------------------------------------------------------------------------------------

pub struct PartyExperiment {
    runs: u32,
}

impl PartyExperiment {
    pub fn new() -> Self {
        Self { runs: 0 }
    }
}

impl Default for PartyExperiment {
    fn default() -> Self {
        Self::new()
    }
}

impl Experiment for PartyExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Party
    }
    fn replaces_course(&self) -> bool {
        true
    }
    fn freezes_player(&self) -> bool {
        true
    }
    fn enter(&mut self, host: &mut Host) {
        host.set_clear(Color::srgb(0.10, 0.14, 0.30));
        host.set_sky(crate::fx::sky::SkyPalette::from_hex(0x2a3a78, 0x6c78c8, 0xf0b890).with_clouds(0.4));
        host.cam.fog = None;
        host.shell.plain_camera = true;
        host.set_player_visible(false);
        let mut world = CollisionWorld::new(1024.0, 1e9);
        let o = SurfaceOptions::default();
        let mut faces = vec![];
        add_box_collision(&mut world, [BOARD_O[0], BOARD_O[1] - 200.0, BOARD_O[2]], [3400.0, 400.0, 1900.0], 0.0, &mut faces, &o);
        add_box_collision(&mut world, [MINI_O[0], MINI_O[1] - 200.0, MINI_O[2]], [2200.0, 400.0, 1300.0], 0.0, &mut faces, &o);
        add_box_collision(&mut world, [PODIUM_O[0], PODIUM_O[1] - 200.0, PODIUM_O[2]], [1800.0, 400.0, 900.0], 0.0, &mut faces, &o);
        for i in 0..4 {
            let (p, h) = podium_slot(i);
            add_box_collision(&mut world, [p[0], p[1] + h / 2.0 - 2.0, p[2]], [150.0, h, 150.0], 0.0, &mut faces, &o);
        }
        world.finalize();
        let world = Arc::new(world);
        let meta = init_clips(host);
        super::view::build_static(host);
        self.runs += 1;
        let runs = self.runs;
        host.commands.queue(move |w: &mut World| {
            let cache = super::view::ViewCache::from_world(w);
            w.insert_resource(cache);
            w.insert_resource(PartyApp::new(world, meta, runs));
            w.resource_mut::<crate::hud::HudMinimal>().0 = true;
            let mut t = w.resource_mut::<crate::touch::TouchControls>();
            t.hide_pad = true;
            t.suppress = true;
        });
    }
    fn exit(&mut self, host: &mut Host) {
        host.commands.queue(|w: &mut World| {
            w.remove_resource::<PartyApp>();
            w.remove_resource::<super::view::ViewCache>();
            w.resource_mut::<crate::hud::HudMinimal>().0 = false;
            let mut t = w.resource_mut::<crate::touch::TouchControls>();
            t.hide_pad = false;
            t.suppress = false;
        });
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        host.set_player_visible(true);
        host.cam.fog = None;
        host.cam.pose = None;
    }
    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None }
    }
    fn debug(&self) -> String {
        format!("sunstone shindig run {}", self.runs)
    }
}

pub fn dvec(v: [f64; 3]) -> DVec3 {
    DVec3::from(v)
}
