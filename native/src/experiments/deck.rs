//! Star Deck: an original deckbuilder roguelike on the selected character.
//!
//! Cards are Mario moves, combat runs on *momentum* (see `deck_core.rs`), and the run is a branching map of
//! fights, shops, campfires, events and a boss (`deck_run.rs`). This file is the glue: the experiment shell, the
//! 3D stage on the real castle grounds (a level patch of lawn found by [`stage_on`]; the hero is the real player model
//! driven through the sim, foes are pooled SM64 actor scenes with their animation clips), the event playback that turns the core's instantaneous results into a readable
//! timeline, the save file and the env test hooks. The screens live in `deck_ui.rs`.
//!
//! The experiment itself is thin: [`DeckGame`] is an ordinary resource inserted on enter and removed on exit, so
//! none of this needs a field on the experiment host.
//!
//! Env hooks (screenshots/tests): `JUMPMAN_DECK_SEED=<n>|daily`, `_START=map|battle|elite|boss|shop|rest|event[:n]|
//! treasure|reward|won|lost`, `_FOES=scuttler,swooper`, `_GIVE=long_jump+,mega_quake`, `_RELICS=spring_boots,...`,
//! `_HP`, `_COINS`, `_DEPTH=<rows already cleared>`, `_SCRIPT=1.5=play.0.0,3=end,4=node.0`, `_AUTOPLAY=1` (a bot plays
//! the run), `_FAST=1` (quick animations), `_TEMPO=<k>` (animation time scale, 4 = slow motion). Runs using them never touch the saved bests.
use super::deck_core::*;
use super::deck_run::*;
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride, XpEntity};
use crate::audio::SfxCue;
use crate::characters::Character;
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot};
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::state::*;
use std::collections::{HashMap, VecDeque};

/// The fights play on the real castle grounds, on a level patch of lawn in front of the castle ([`stage_on`]).
pub const STAGE_COURSE: &str = "castle_grounds";
/// Where the stage search starts (course x, z): the lawn between the spawn and the castle.
const STAGE_NEAR: [f64; 2] = [-2400.0, 3000.0];
/// The stage centre, set on entering (a stand-in far from everything until then and in unit tests).
static STAGE: std::sync::RwLock<[f64; 3]> = std::sync::RwLock::new([-150000.0, 1000.0, -150000.0]);

/// The stage centre in course coordinates (the hero stands left of it, the foes right, the camera looks at -z).
pub fn stage() -> [f64; 3] {
    *STAGE.read().unwrap_or_else(|e| e.into_inner())
}

/// The most level open patch of lawn near [`STAGE_NEAR`] for a fight: every actor mark within a few units of the same
/// height. Deterministic for a course.
pub fn stage_on(world: &CollisionWorld) -> [f64; 3] {
    let marks: Vec<[f64; 2]> = (-7..=10).flat_map(|i| (-3..=4).map(move |j| [i as f64 * 100.0, j as f64 * 100.0])).collect();
    let mut best: Option<(f64, [f64; 3])> = None;
    for gi in -10i32..=10 {
        for gj in -10i32..=10 {
            let (cx, cz) = (STAGE_NEAR[0] + gi as f64 * 200.0, STAGE_NEAR[1] + gj as f64 * 200.0);
            let f = world.find_floor(cx, 20000.0, cz);
            if f.surface.is_none() {
                continue;
            }
            let h0 = f.height;
            let mut spread = 0.0f64;
            for m in &marks {
                let g = world.find_floor(cx + m[0], h0 + 400.0, cz + m[1]);
                let ok = g.surface.as_ref().is_some_and(|s| s.normal[1] > 0.95) && world.water_level(cx + m[0], cz + m[1]) < g.height - 30.0;
                spread = spread.max(if ok { (g.height - h0).abs() } else { 1e6 });
            }
            let score = spread + (gi.abs() + gj.abs()) as f64 * 0.5;
            if best.map_or(true, |(b, _)| score < b) {
                best = Some((score, [cx, h0, cz]));
            }
        }
    }
    best.map_or([STAGE_NEAR[0], 0.0, STAGE_NEAR[1]], |(_, p)| p)
}
/// The hero faces +x (toward the foes).
const HERO_YAW: f64 = 0x4000 as f64;
/// Model scale factor for foes (their glb units are SM64 units).
const FOE_K: f32 = 1.7;

fn at(p: [f64; 3], dx: f64, dy: f64, dz: f64) -> [f64; 3] {
    [p[0] + dx, p[1] + dy, p[2] + dz]
}

pub fn hero_home(combat: bool) -> [f64; 3] {
    hero_home_at(stage(), combat)
}

pub fn hero_home_at(stage: [f64; 3], combat: bool) -> [f64; 3] {
    if combat { at(stage, -560.0, 0.0, 0.0) } else { at(stage, -330.0, 0.0, 70.0) }
}

/// Where foe `i` stands when `n` foes opened the fight.
pub fn foe_slot(i: usize, n: usize) -> [f64; 3] {
    foe_slot_at(stage(), i, n)
}

pub fn foe_slot_at(stage: [f64; 3], i: usize, n: usize) -> [f64; 3] {
    let table: &[[f64; 2]] = match n {
        0 | 1 => &[[480.0, 0.0]],
        2 => &[[300.0, 90.0], [720.0, -110.0]],
        _ => &[[190.0, 110.0], [540.0, -100.0], [880.0, 90.0]],
    };
    if i < table.len() {
        return at(stage, table[i][0], 0.0, table[i][1]);
    }
    let extra: [[f64; 2]; 3] = [[260.0, 260.0], [760.0, 250.0], [200.0, -260.0]];
    let e = extra[(i - table.len()).min(2)];
    at(stage, e[0], 0.0, e[1])
}

// ---------------------------------------------------------------------------------------------
// Commands, display state, playback
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pile {
    Draw,
    Discard,
    Exhaust,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modal {
    None,
    Pile(Pile),
    Deck,
    PickUpgrade,
    PickRemove,
    Help,
}

/// What the UI asks of the game.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    Node(usize),
    Play(usize, Option<usize>),
    EndTurn,
    Potion(usize, Option<usize>),
    RewardCard(usize),
    SkipCard,
    TakeGold,
    TakeRelic,
    TakePotion,
    LeaveReward,
    BuyCard(usize),
    BuyRelic(usize),
    BuyPotion(usize),
    RemoveCard(usize),
    LeaveShop,
    RestHeal,
    RestUpgrade(usize),
    LeaveRest,
    EventChoice(usize),
    LeaveEvent,
    TakeTreasure,
    Retry,
    Menu,
    Open(Modal),
    Close,
    Select(Option<usize>),
    /// Switch between random and the shared daily seed (a fresh map re-rolls at once; otherwise it applies to the next run).
    ToggleDaily,
    /// Keyboard / gamepad aiming driven by script or tests: 0 = begin aiming the selected card, -1 / 1 = move the aim, 9 = play at the aim.
    Aim(i8),
    DrinkTonic(usize),
    ShowRelic(usize),
}

#[derive(Clone, Debug)]
pub struct DFoe {
    pub kind: EnemyKind,
    pub hp: i32,
    pub max_hp: i32,
    pub shell: i32,
    pub st: Statuses,
    pub intent: Intent,
    pub alive: bool,
}

/// What the screen shows; advanced event by event so numbers move with the animation.
#[derive(Clone, Debug, Default)]
pub struct Disp {
    pub hp: i32,
    pub max_hp: i32,
    pub shell: i32,
    pub st: Statuses,
    pub momentum: i32,
    pub chain: i32,
    pub draw_n: usize,
    pub discard_n: usize,
    pub exhaust_n: usize,
    pub hand: Vec<Inst>,
    pub foes: Vec<DFoe>,
    pub turn: u32,
    pub potions: [Option<Potion>; POTION_SLOTS],
    pub enemy_turn: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Leave {
    Play,
    Discard,
    Exhaust,
}

#[derive(Clone, Debug)]
pub struct Floater {
    pub pos: Vec3,
    pub text: String,
    pub color: Color,
    pub t0: f64,
    pub big: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeroAnim {
    /// Dash to a foe, strike, hop back.
    Lunge { act: u32, to: [f64; 3], arc: f64 },
    /// In-place move.
    InPlace { act: u32 },
    Hurt,
    Cheer,
}

#[derive(Clone, Copy, Debug)]
pub struct Choreo {
    pub anim: HeroAnim,
    pub t0: f64,
    pub dur: f64,
    pub started: bool,
}

pub struct Foe3d {
    pub entity: Entity,
    pub kind: EnemyKind,
    pub home: [f64; 3],
    pub born: f64,
    pub lunge: Option<(f64, f64)>,
    pub hurt: f64,
    pub died: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FoeScreen {
    pub center: Vec2,
    pub top: Vec2,
    pub bottom: Vec2,
    pub half_w: f32,
    pub on: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Saved {
    pub best_depth: u32,
    pub best_score: i32,
    pub wins: u32,
    pub runs: u32,
    /// New runs use the shared daily seed (the toggle on the map and summary screens).
    pub daily: bool,
}

fn save_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("stardeck.json")
}

pub fn load_saved() -> Saved {
    let v: serde_json::Value = std::fs::read_to_string(save_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    Saved { best_depth: v["best_depth"].as_u64().unwrap_or(0) as u32, best_score: v["best_score"].as_i64().unwrap_or(0) as i32, wins: v["wins"].as_u64().unwrap_or(0) as u32, runs: v["runs"].as_u64().unwrap_or(0) as u32, daily: v["daily"].as_bool().unwrap_or(false) }
}

fn write_saved(s: Saved) {
    let path = save_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "best_depth": s.best_depth, "best_score": s.best_score, "wins": s.wins, "runs": s.runs, "daily": s.daily }).to_string());
}

/// Test hooks mark a run as not eligible for the records.
fn cheating() -> bool {
    ["JUMPMAN_DECK_START", "JUMPMAN_DECK_GIVE", "JUMPMAN_DECK_RELICS", "JUMPMAN_DECK_HP", "JUMPMAN_DECK_COINS", "JUMPMAN_DECK_DEPTH", "JUMPMAN_DECK_FOES", "JUMPMAN_DECK_AUTOPLAY"].iter().any(|k| std::env::var_os(k).is_some())
}

/// Whole UTC days since the epoch.
pub fn today() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() / 86400)
}

/// The shared seed of a UTC day: everyone who plays the daily that day gets the same map, shops and rewards.
pub fn daily_seed_for_day(day: u64) -> u64 {
    day.wrapping_mul(2654435761)
}

pub fn daily_seed() -> u64 {
    daily_seed_for_day(today())
}

fn random_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

fn env_seed() -> u64 {
    match std::env::var("JUMPMAN_DECK_SEED") {
        Ok(s) if s == "daily" => daily_seed(),
        Ok(s) => s.parse().unwrap_or(1),
        Err(_) if load_saved().daily => daily_seed(),
        Err(_) => random_seed(),
    }
}

fn hero_of(c: Character) -> Hero {
    match c {
        Character::Mario => Hero::Mario,
        Character::Link => Hero::Link,
        Character::YoungLink => Hero::YoungLink,
        Character::Banjo => Hero::Banjo,
    }
}

#[derive(Resource)]
pub struct DeckGame {
    pub run: Run,
    pub character: Character,
    pub seed: u64,
    pub runs: u32,
    /// Seconds since entering (render clock).
    pub t: f64,
    pub cmds: Vec<Cmd>,
    pub disp: Disp,
    pub queue: VecDeque<(f64, Ev)>,
    pub q_end: f64,
    pub leave: HashMap<u32, Leave>,
    pub uid_card: HashMap<u32, Card>,
    pub choreo: Option<Choreo>,
    pub hero_hurt_until: f64,
    pub selected: Option<usize>,
    pub modal: Modal,
    /// Bumped whenever a screen must be rebuilt.
    pub rev: u32,
    pub saved: Saved,
    pub record_new: bool,
    pub foes3d: Vec<Foe3d>,
    pub foe_screen: Vec<FoeScreen>,
    pub floaters: Vec<Floater>,
    pub banner: Option<(String, f64, Color)>,
    pub shake: f32,
    pub script: Vec<(f64, String)>,
    pub autoplay: bool,
    pub cheated: bool,
    pub fast: f64,
    pub combat_open: bool,
    pub end_at: Option<f64>,
    pub n_start: usize,
    pub summary_at: f64,
    pub toast: Option<(String, f64)>,
    pub focus: usize,
    pub peek_card: Option<Card>,
    /// The foe a targeted card will hit while keyboard / gamepad aiming (index into `disp.foes`), `None` when not aiming.
    pub aim: Option<usize>,
    /// New runs (and the retry button) use today's shared seed.
    pub daily: bool,
}

/// Next entry of `order` after `cur` in direction `dir` (wrapping); the first (or last for `-1`) when `cur` is not in it.
pub fn cycle_foe(order: &[usize], cur: Option<usize>, dir: i32) -> Option<usize> {
    if order.is_empty() {
        return None;
    }
    let n = order.len() as i32;
    let pos = cur.and_then(|c| order.iter().position(|&o| o == c));
    let next = match pos {
        Some(p) => (p as i32 + dir.signum()).rem_euclid(n),
        None if dir < 0 => n - 1,
        None => 0,
    };
    Some(order[next as usize])
}

impl DeckGame {
    pub fn new(character: Character, runs: u32, seed: u64) -> Self {
        let hero = hero_of(character);
        let mut run = Run::new(seed, hero);
        let mut script: Vec<(f64, String)> = vec![];
        if let Ok(s) = std::env::var("JUMPMAN_DECK_SCRIPT") {
            for item in s.split(',') {
                if let Some((t, c)) = item.split_once('=') {
                    if let Ok(t) = t.parse::<f64>() {
                        script.push((t, c.to_string()));
                    }
                }
            }
            script.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        }
        // Start hooks.
        if let Some(n) = std::env::var("JUMPMAN_DECK_HP").ok().and_then(|v| v.parse::<i32>().ok()) {
            run.hp = n.min(run.max_hp);
        }
        if let Some(n) = std::env::var("JUMPMAN_DECK_COINS").ok().and_then(|v| v.parse::<i32>().ok()) {
            run.coins = n;
        }
        if let Ok(list) = std::env::var("JUMPMAN_DECK_GIVE") {
            for k in list.split(',') {
                let (key, up) = match k.strip_suffix('+') {
                    Some(k) => (k, true),
                    None => (k, false),
                };
                if let Some(id) = find_card(key.trim()) {
                    run.deck.push(Card { id, up });
                }
            }
        }
        if let Ok(list) = std::env::var("JUMPMAN_DECK_RELICS") {
            for k in list.split(',') {
                if let Some(r) = RELICS.iter().find(|r| r.name().to_lowercase().replace(' ', "_") == k.trim()) {
                    run.add_relic(*r);
                }
            }
        }
        if let Some(d) = std::env::var("JUMPMAN_DECK_DEPTH").ok().and_then(|v| v.parse::<usize>().ok()) {
            // Walk the first route down to row d.
            let mut cur = run.map.rows[0][0];
            run.cleared.push(cur);
            for _ in 1..d.min(BOSS_ROW - 1) {
                cur = run.map.nodes[cur].next[0];
                run.cleared.push(cur);
            }
            run.pos = Some(cur);
            run.stats.nodes_cleared = d as u32;
            run.stats.battles = d as u32 / 2;
        }
        let mut g = DeckGame {
            run,
            character,
            seed,
            runs,
            t: 0.0,
            cmds: vec![],
            disp: Disp::default(),
            queue: VecDeque::new(),
            q_end: 0.0,
            leave: HashMap::new(),
            uid_card: HashMap::new(),
            choreo: None,
            hero_hurt_until: 0.0,
            selected: None,
            modal: Modal::None,
            rev: 1,
            saved: load_saved(),
            record_new: false,
            foes3d: vec![],
            foe_screen: vec![],
            floaters: vec![],
            banner: None,
            shake: 0.0,
            script,
            autoplay: std::env::var_os("JUMPMAN_DECK_AUTOPLAY").is_some(),
            cheated: cheating(),
            fast: std::env::var("JUMPMAN_DECK_TEMPO").ok().and_then(|v| v.parse().ok()).unwrap_or(if std::env::var_os("JUMPMAN_DECK_FAST").is_some() { 0.12 } else { 1.0 }),
            combat_open: false,
            end_at: None,
            n_start: 1,
            summary_at: 0.0,
            toast: None,
            focus: 0,
            peek_card: None,
            aim: None,
            daily: false,
        };
        g.daily = match std::env::var("JUMPMAN_DECK_SEED") {
            Ok(s) => s == "daily",
            Err(_) => g.saved.daily,
        };
        g.apply_start_hook();
        g
    }

    fn apply_start_hook(&mut self) {
        let Ok(start) = std::env::var("JUMPMAN_DECK_START") else { return };
        let (what, arg) = start.split_once(':').map_or((start.as_str(), None), |(a, b)| (a, Some(b)));
        let foes: Option<Vec<EnemyKind>> = std::env::var("JUMPMAN_DECK_FOES").ok().map(|s| {
            s.split(',')
                .filter_map(|n| {
                    let n = n.trim().to_lowercase().replace('_', "");
                    ENEMY_KINDS.iter().copied().find(|k| k.name().to_lowercase().replace(' ', "") == n)
                })
                .collect()
        });
        let row = self.run.pos.map_or(0, |p| self.run.map.nodes[p].row + 1).min(BOSS_ROW - 1);
        let node_of = |g: &DeckGame, kind: NodeKind| -> Option<usize> {
            let cands = g.run.available();
            cands.iter().copied().find(|id| g.run.map.nodes[*id].kind == kind).or_else(|| g.run.map.nodes.iter().find(|n| n.kind == kind && n.row >= row).map(|n| n.id))
        };
        match what {
            "battle" | "elite" | "boss" => {
                let kind = match what {
                    "elite" => NodeKind::Elite,
                    "boss" => NodeKind::Boss,
                    _ => NodeKind::Battle,
                };
                if let Some(id) = node_of(self, kind) {
                    self.run.pos = self.run.map.nodes[id].next.first().and(None);
                    // Enter directly (bypassing reachability) so any node can be opened.
                    self.run.pos = None;
                    self.force_enter(id);
                    if let Some(f) = foes.filter(|f| !f.is_empty()) {
                        self.run.start_combat(&f);
                    }
                }
            }
            "shop" | "rest" | "event" | "treasure" => {
                let kind = match what {
                    "shop" => NodeKind::Shop,
                    "rest" => NodeKind::Rest,
                    "event" => NodeKind::Event,
                    _ => NodeKind::Treasure,
                };
                if let Some(id) = node_of(self, kind) {
                    self.force_enter(id);
                    if kind == NodeKind::Event {
                        if let Some(n) = arg.and_then(|a| a.parse::<usize>().ok()) {
                            self.run.event = Some(EVENTS[n % EVENTS.len()]);
                        }
                    }
                }
            }
            "reward" | "elite_reward" => {
                let kind = if what == "reward" { NodeKind::Battle } else { NodeKind::Elite };
                if let Some(id) = node_of(self, kind) {
                    self.force_enter(id);
                    if let Some(c) = self.run.combat.as_mut() {
                        c.bot_fight(60);
                    }
                    self.run.finish_combat();
                }
            }
            "won" | "lost" => {
                self.run.stats = Stats { nodes_cleared: if what == "won" { 7 } else { 4 }, battles: 5, elites: 1, cards_played: 120, damage: 480, best_chain: 4, coins_earned: 140 };
                self.run.screen = if what == "won" { Screen::Won } else { Screen::Lost };
            }
            _ => {}
        }
        // `JUMPMAN_DECK_HAND=dive+,pound,hop` replaces the opening hand (screenshots).
        if let (Ok(list), Some(c)) = (std::env::var("JUMPMAN_DECK_HAND"), self.run.combat.as_mut()) {
            for (i, k) in list.split(',').enumerate() {
                let (key, up) = match k.strip_suffix('+') {
                    Some(k) => (k, true),
                    None => (k, false),
                };
                if let (Some(id), Some(slot)) = (find_card(key.trim()), c.hand.get_mut(i)) {
                    slot.card = Card { id, up };
                }
            }
        }
    }

    fn force_enter(&mut self, id: usize) {
        // `enter_node` checks reachability; hooks may jump anywhere, so open the node's row first.
        let row = self.run.map.nodes[id].row;
        if row > 0 {
            let prev: Vec<usize> = self.run.map.rows[row - 1].clone();
            self.run.pos = prev.iter().copied().find(|p| self.run.map.nodes[*p].next.contains(&id));
        } else {
            self.run.pos = None;
        }
        let _ = self.run.enter_node(id);
    }

    pub fn combat(&self) -> Option<&Combat> {
        self.run.combat.as_ref()
    }

    /// True on the map before the first stop is entered: nothing to lose by re-rolling the run.
    pub fn fresh_start(&self) -> bool {
        self.run.screen == Screen::Map && self.run.pos.is_none() && self.run.cleared.is_empty() && !self.run.ended()
    }

    /// Flip the daily-seed setting. On a fresh start the map is regenerated from the new seed immediately.
    pub fn toggle_daily(&mut self, daily: u64, random: u64) {
        self.daily = !self.daily;
        self.saved.daily = self.daily;
        if !self.cheated {
            write_saved(self.saved);
        }
        if self.fresh_start() {
            self.seed = if self.daily { daily } else { random };
            self.run = Run::new(self.seed, hero_of(self.character));
        }
        self.rev += 1;
        self.toast = Some((if self.daily { "Daily seed ON: everyone gets today's map".to_string() } else { "Daily seed OFF: random runs".to_string() }, self.t));
    }

    /// Living foes ordered left to right on screen (index order until they have been projected).
    pub fn foes_by_x(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.disp.foes.iter().enumerate().filter(|(_, f)| f.alive).map(|(i, _)| i).collect();
        v.sort_by(|a, b| {
            let xa = self.foe_screen.get(*a).map_or(*a as f32, |s| s.center.x);
            let xb = self.foe_screen.get(*b).map_or(*b as f32, |s| s.center.x);
            xa.partial_cmp(&xb).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b))
        });
        v
    }

    /// Start keyboard / gamepad aiming for the selected targeted card (`None` when it is not targeted or only one foe lives).
    pub fn begin_aim(&mut self) {
        let order = self.foes_by_x();
        self.aim = if order.len() > 1 { order.first().copied() } else { None };
    }

    /// Move the aim one living foe left (`-1`) or right (`+1`), wrapping.
    pub fn move_aim(&mut self, dir: i32) {
        let order = self.foes_by_x();
        self.aim = cycle_foe(&order, self.aim, dir);
    }

    pub fn busy(&self) -> bool {
        !self.queue.is_empty() || self.t < self.q_end
    }

    pub fn hero_name(&self) -> &'static str {
        self.run.hero.name()
    }

    pub fn say(&mut self, text: &str, color: Color) {
        self.banner = Some((text.to_string(), self.t, color));
    }

    pub fn float(&mut self, pos: Vec3, text: impl Into<String>, color: Color, big: bool) {
        if self.floaters.len() > 24 {
            self.floaters.remove(0);
        }
        self.floaters.push(Floater { pos, text: text.into(), color, t0: self.t, big });
    }

    pub fn foe_world(&self, i: usize) -> Vec3 {
        self.foes3d.get(i).map_or(Vec3::ZERO, |f| to_bevy(f.home))
    }

    pub fn hero_world(&self) -> Vec3 {
        to_bevy(hero_home(self.run.screen == Screen::Combat))
    }

    // Command handling ----------------------------------------------------------------------

    pub fn start_combat_view(&mut self) {
        let Some(c) = self.run.combat.as_ref() else { return };
        self.n_start = c.foes.len();
        self.disp = Disp {
            hp: c.hp,
            max_hp: c.max_hp,
            shell: 0,
            st: c.st,
            momentum: c.momentum,
            chain: 0,
            draw_n: c.draw.len() + c.hand.len(),
            discard_n: 0,
            exhaust_n: 0,
            hand: vec![],
            foes: c.foes.iter().map(|f| DFoe { kind: f.kind, hp: f.hp, max_hp: f.max_hp, shell: f.shell, st: f.st, intent: f.intent, alive: true }).collect(),
            turn: 0,
            potions: c.potions,
            enemy_turn: false,
        };
        self.queue.clear();
        self.leave.clear();
        self.foes3d.clear();
        self.selected = None;
        self.aim = None;
        self.end_at = None;
        self.combat_open = true;
        self.q_end = self.t + 0.6;
        self.drain_events();
    }

    /// Move the core's pending events onto the timeline.
    pub fn drain_events(&mut self) {
        let Some(c) = self.run.combat.as_mut() else { return };
        let evs = c.take_events();
        for inst in c.draw.iter().chain(&c.hand).chain(&c.discard).chain(&c.exhaust) {
            self.uid_card.insert(inst.uid, inst.card);
        }
        let mut when = self.q_end.max(self.t);
        let k = self.fast;
        for e in evs {
            let dur = match &e {
                Ev::TurnStart { .. } => 0.35,
                Ev::Draw { .. } => 0.09,
                Ev::Reshuffle => 0.3,
                Ev::Play { target, attack, .. } => {
                    if *attack && target.is_some() { 0.55 } else if *attack { 0.5 } else { 0.32 }
                }
                Ev::Discard { .. } => 0.035,
                Ev::Exhaust { .. } => 0.06,
                Ev::Hit { from: None, .. } => 0.2,
                Ev::Hit { .. } => 0.28,
                Ev::GainShell { .. } | Ev::Heal { .. } => 0.18,
                Ev::Status { .. } => 0.16,
                Ev::FoeActs { .. } => 0.5,
                Ev::Died { .. } => 0.4,
                Ev::Summoned { .. } => 0.4,
                Ev::Won | Ev::Lost => 0.2,
                _ => 0.05,
            } * k;
            self.queue.push_back((when, e));
            when += dur;
        }
        self.q_end = when;
    }

    pub fn hand_index_of(&self, uid: u32) -> Option<usize> {
        self.combat()?.hand.iter().position(|i| i.uid == uid)
    }

    pub fn push(&mut self, c: Cmd) {
        self.cmds.push(c);
    }

    pub fn execute(&mut self, cmd: Cmd) {
        let busy = self.busy();
        match cmd {
            Cmd::Open(m) => {
                self.modal = m;
                return;
            }
            Cmd::Close => {
                self.modal = Modal::None;
                return;
            }
            Cmd::ShowRelic(i) => {
                if let Some(r) = self.run.relics.get(i) {
                    self.toast = Some((format!("{}: {}", r.name(), r.desc()), self.t));
                }
                return;
            }
            Cmd::Select(s) => {
                self.selected = s;
                self.aim = None;
                return;
            }
            Cmd::ToggleDaily => {
                self.toggle_daily(daily_seed(), random_seed());
                return;
            }
            Cmd::Aim(a) => {
                match a {
                    0 => self.begin_aim(),
                    9 => {
                        if let (Some(h), Some(t)) = (self.selected, self.aim) {
                            self.cmds.push(Cmd::Play(h, Some(t)));
                        }
                    }
                    d => self.move_aim(d as i32),
                }
                return;
            }
            _ => {}
        }
        if busy && matches!(cmd, Cmd::Play(..) | Cmd::EndTurn | Cmd::Potion(..)) {
            return;
        }
        let before = self.run.screen;
        match cmd {
            Cmd::Node(id) => {
                if self.run.enter_node(id).is_ok() && self.run.screen == Screen::Combat {
                    self.start_combat_view();
                }
            }
            Cmd::Play(h, t) => {
                if let Some(c) = self.run.combat.as_mut() {
                    if c.play(h, t).is_ok() {
                        self.selected = None;
                        self.aim = None;
                    }
                }
            }
            Cmd::EndTurn => {
                self.selected = None;
                self.aim = None;
                if let Some(c) = self.run.combat.as_mut() {
                    c.end_turn();
                }
                self.disp.enemy_turn = true;
            }
            Cmd::Potion(s, t) => {
                if let Some(c) = self.run.combat.as_mut() {
                    let _ = c.use_potion(s, t);
                }
            }
            Cmd::RewardCard(i) => {
                self.run.take_card(i);
            }
            Cmd::SkipCard => self.run.skip_card(),
            Cmd::TakeGold => self.run.take_gold(),
            Cmd::TakeRelic => self.run.take_relic(),
            Cmd::TakePotion => {
                self.run.take_potion();
            }
            Cmd::LeaveReward => self.run.leave_reward(),
            Cmd::BuyCard(i) => {
                self.run.buy_card(i);
            }
            Cmd::BuyRelic(i) => {
                self.run.buy_relic(i);
            }
            Cmd::BuyPotion(i) => {
                self.run.buy_potion(i);
            }
            Cmd::RemoveCard(i) => {
                if self.run.remove_card(i) {
                    self.modal = Modal::None;
                }
            }
            Cmd::LeaveShop => self.run.leave_shop(),
            Cmd::RestHeal => {
                self.run.rest_heal();
            }
            Cmd::RestUpgrade(i) => {
                if self.run.rest_upgrade(i) {
                    self.modal = Modal::None;
                }
            }
            Cmd::LeaveRest => self.run.leave_rest(),
            Cmd::EventChoice(i) => {
                self.run.event_choose(i);
            }
            Cmd::LeaveEvent => self.run.leave_event(),
            Cmd::TakeTreasure => self.run.take_treasure(),
            Cmd::DrinkTonic(s) => {
                self.run.drink_tonic(s);
            }
            Cmd::Retry | Cmd::Menu | Cmd::Open(_) | Cmd::Close | Cmd::Select(_) | Cmd::ToggleDaily | Cmd::Aim(_) | Cmd::EndTurn | Cmd::ShowRelic(_) => {}
        }
        self.drain_events();
        if self.run.screen != before || matches!(cmd, Cmd::RewardCard(_) | Cmd::SkipCard | Cmd::TakeGold | Cmd::TakeRelic | Cmd::TakePotion | Cmd::BuyCard(_) | Cmd::BuyRelic(_) | Cmd::BuyPotion(_) | Cmd::RemoveCard(_) | Cmd::RestHeal | Cmd::RestUpgrade(_) | Cmd::EventChoice(_) | Cmd::DrinkTonic(_)) {
            self.rev += 1;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Experiment shell
// ---------------------------------------------------------------------------------------------

pub struct StarDeckExperiment {
    runs: u32,
}

impl StarDeckExperiment {
    pub fn new() -> Self {
        Self { runs: 0 }
    }
}

impl Default for StarDeckExperiment {
    fn default() -> Self {
        Self::new()
    }
}

impl Experiment for StarDeckExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Deck
    }
    /// The fights play on the real castle grounds.
    fn course(&self) -> Option<&'static str> {
        Some(STAGE_COURSE)
    }
    fn enter(&mut self, host: &mut Host) {
        host.shell.plain_camera = true;
        let world = host.course_world();
        if !world.surfaces().is_empty() {
            *STAGE.write().unwrap_or_else(|e| e.into_inner()) = stage_on(&world);
        }
        build_stage(host);
        host.place_mario(at(hero_home(true), 0.0, 20.0, 0.0), Some(HERO_YAW));
        host.set_player_visible(true);
        self.runs += 1;
        let runs = self.runs;
        let seed = env_seed();
        host.commands.queue(move |world: &mut World| {
            let character = world.get_resource::<Character>().copied().unwrap_or_default();
            world.insert_resource(DeckGame::new(character, runs, seed));
            world.resource_mut::<crate::hud::HudMinimal>().0 = true;
            {
                let mut t = world.resource_mut::<crate::touch::TouchControls>();
                t.hide_pad = true;
                t.suppress = true;
            }
        });
    }
    fn exit(&mut self, host: &mut Host) {
        host.commands.queue(|world: &mut World| {
            world.remove_resource::<DeckGame>();
            world.resource_mut::<crate::hud::HudMinimal>().0 = false;
            {
                let mut t = world.resource_mut::<crate::touch::TouchControls>();
                t.hide_pad = false;
                t.suppress = false;
            }
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
        format!("star deck run {}", self.runs)
    }
}

/// The stage on the real course: a warm spot light over the fighting line (the lawn, the trees and the castle behind
/// are the course's own).
fn build_stage(host: &mut Host) {
    host.spawn((PointLight { intensity: 2_000_000.0, range: 120.0, color: Color::srgb(1.0, 0.85, 0.7), shadow_maps_enabled: false, ..default() }, Transform::from_translation(to_bevy(at(stage(), 100.0, 1400.0, 700.0)))));
}

// ---------------------------------------------------------------------------------------------
// Plugin: systems
// ---------------------------------------------------------------------------------------------

/// Ordering handle: the UI systems run after the game logic.
#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct DeckSet;

pub struct DeckPlugin;

impl Plugin for DeckPlugin {
    fn build(&self, app: &mut App) {
        let active = resource_exists::<DeckGame>;
        app.add_systems(
            Update,
            (clock_and_script, process_cmds, pump_events, flow, sync_foes, update_props, camera_and_ambience, project_foes).chain().in_set(DeckSet).run_if(active).after(crate::CameraSet).before(crate::experiments::apply_camera),
        )
        .add_systems(FixedUpdate, drive_hero.after(crate::SimSet).run_if(active));
    }
}

fn clock_and_script(mut g: ResMut<DeckGame>, time: Res<Time>, state: Res<crate::shell::AppState>) {
    if *state != crate::shell::AppState::Playing {
        return;
    }
    let dt = time.delta_secs_f64().min(0.1);
    g.t += dt;
    g.shake = (g.shake - dt as f32 * 3.0).max(0.0);
    // Scripted commands for tests/screenshots.
    // A scripted command waits for the previous one's animation to finish.
    if g.script.first().is_some_and(|(t, _)| *t <= g.t) && !g.busy() && g.cmds.is_empty() {
        let (_, c) = g.script.remove(0);
        if let Some(cmd) = parse_cmd(&c) {
            g.cmds.push(cmd);
        }
    }
    // Autoplay: act whenever idle.
    if g.autoplay && !g.busy() && g.cmds.is_empty() {
        let step = autoplay_step(&mut g);
        if let Some(c) = step {
            g.cmds.push(c);
        }
    }
    if let Some((_, t0, _)) = &g.banner {
        if g.t - t0 > 1.4 {
            g.banner = None;
        }
    }
    if g.toast.as_ref().is_some_and(|(_, t0)| g.t - t0 > 2.2) {
        g.toast = None;
    }
}

pub(super) fn parse_cmd(s: &str) -> Option<Cmd> {
    let p: Vec<&str> = s.split('.').collect();
    let n = |i: usize| p.get(i).and_then(|v| v.parse::<usize>().ok());
    Some(match p[0] {
        "node" => Cmd::Node(n(1)?),
        "play" => Cmd::Play(n(1)?, n(2)),
        "end" => Cmd::EndTurn,
        "potion" => Cmd::Potion(n(1)?, n(2)),
        "card" => Cmd::RewardCard(n(1)?),
        "skip" => Cmd::SkipCard,
        "gold" => Cmd::TakeGold,
        "relic" => Cmd::TakeRelic,
        "leave" => Cmd::LeaveReward,
        "buycard" => Cmd::BuyCard(n(1)?),
        "buyrelic" => Cmd::BuyRelic(n(1)?),
        "buypotion" => Cmd::BuyPotion(n(1)?),
        "leaveshop" => Cmd::LeaveShop,
        "heal" => Cmd::RestHeal,
        "upgrade" => Cmd::RestUpgrade(n(1)?),
        "leaverest" => Cmd::LeaveRest,
        "event" => Cmd::EventChoice(n(1)?),
        "leaveevent" => Cmd::LeaveEvent,
        "treasure" => Cmd::TakeTreasure,
        "retry" => Cmd::Retry,
        "select" => Cmd::Select(n(1)),
        "daily" => Cmd::ToggleDaily,
        "aim" => Cmd::Aim(match p.get(1).copied() {
            Some("l") => -1,
            Some("r") => 1,
            Some("go") => 9,
            _ => 0,
        }),
        "open" => Cmd::Open(match p.get(1).copied() {
            Some("draw") => Modal::Pile(Pile::Draw),
            Some("discard") => Modal::Pile(Pile::Discard),
            Some("exhaust") => Modal::Pile(Pile::Exhaust),
            Some("upgrade") => Modal::PickUpgrade,
            Some("remove") => Modal::PickRemove,
            Some("help") => Modal::Help,
            _ => Modal::Deck,
        }),
        "close" => Cmd::Close,
        _ => return None,
    })
}

/// One bot decision for `JUMPMAN_DECK_AUTOPLAY`, mirroring what a player does through the UI.
pub(super) fn autoplay_step(g: &mut DeckGame) -> Option<Cmd> {
    if g.modal != Modal::None {
        return Some(Cmd::Close);
    }
    match g.run.screen {
        Screen::Map => {
            let av = g.run.available();
            let id = av.iter().copied().min_by_key(|id| match g.run.map.nodes[*id].kind {
                NodeKind::Rest => 0,
                NodeKind::Event => 1,
                NodeKind::Shop => 2,
                NodeKind::Battle => 3,
                NodeKind::Treasure => 0,
                NodeKind::Elite => 5,
                NodeKind::Boss => 0,
            })?;
            Some(Cmd::Node(id))
        }
        Screen::Combat => {
            let c = g.run.combat.as_mut()?;
            if c.over() {
                return None;
            }
            // Re-use the bot's choice without mutating: clone, ask, replay on the real combat.
            let mut probe = c.clone();
            let before = probe.cards_played;
            let hand_before: Vec<u32> = probe.hand.iter().map(|i| i.uid).collect();
            if probe.bot_step() && probe.cards_played > before {
                let uid = hand_before.iter().copied().find(|u| !probe.hand.iter().any(|i| i.uid == *u))?;
                let idx = hand_before.iter().position(|u| *u == uid)?;
                let target = c.alive_foes().min_by_key(|i| c.foes[*i].hp + c.foes[*i].shell);
                return Some(Cmd::Play(idx, target));
            }
            Some(Cmd::EndTurn)
        }
        Screen::Reward => {
            let r = g.run.reward.clone()?;
            if !r.gold_taken {
                return Some(Cmd::TakeGold);
            }
            if r.relic.is_some() && !r.relic_taken {
                return Some(Cmd::TakeRelic);
            }
            if !r.card_done {
                let pick = r.cards.iter().position(|c| c.def().kind == Kind::Attack && c.def().rarity != Rarity::Common).or_else(|| r.cards.iter().position(|c| c.def().cost <= 1 && c.def().kind != Kind::Power));
                return Some(pick.map_or(Cmd::SkipCard, Cmd::RewardCard));
            }
            Some(Cmd::LeaveReward)
        }
        Screen::Shop => Some(Cmd::LeaveShop),
        Screen::Rest => {
            if !g.run.rested {
                if g.run.hp * 10 < g.run.max_hp * 7 {
                    return Some(Cmd::RestHeal);
                }
                if let Some(i) = (0..g.run.deck.len()).find(|i| g.run.deck[*i].can_upgrade() && g.run.deck[*i].def().rarity != Rarity::Starter) {
                    return Some(Cmd::RestUpgrade(i));
                }
                return Some(Cmd::RestHeal);
            }
            Some(Cmd::LeaveRest)
        }
        Screen::Event => {
            if g.run.event_result.is_some() {
                Some(Cmd::LeaveEvent)
            } else {
                Some(Cmd::EventChoice(g.run.event.map_or(0, |e| e.options().len() - 1)))
            }
        }
        Screen::Treasure => Some(Cmd::TakeTreasure),
        Screen::Won | Screen::Lost => None,
    }
}

fn process_cmds(mut g: ResMut<DeckGame>, state: Res<crate::shell::AppState>, mut menu: MessageWriter<crate::shell::ReturnToMenu>, mut sfx: MessageWriter<SfxCue>, mut touch: ResMut<crate::touch::TouchControls>) {
    if !touch.hide_pad {
        touch.hide_pad = true;
    }
    if !touch.suppress {
        touch.suppress = true;
    }
    if *state != crate::shell::AppState::Playing {
        g.cmds.clear();
        return;
    }
    let cmds = std::mem::take(&mut g.cmds);
    for cmd in cmds {
        match cmd {
            Cmd::Menu => {
                menu.write(crate::shell::ReturnToMenu);
            }
            Cmd::Retry => {
                if g.run.ended() {
                    g.runs += 1;
                    let seed = if g.daily { daily_seed() } else { g.seed.wrapping_add(g.runs as u64 * 7919) };
                    let daily = g.daily;
                    let (runs, character, saved, cheated, fast, auto) = (g.runs, g.character, g.saved, g.cheated, g.fast, g.autoplay);
                    let t = g.t;
                    let mut fresh = DeckGame::new(character, runs, seed);
                    fresh.saved = saved;
                    fresh.t = t;
                    fresh.cheated = cheated;
                    fresh.daily = daily;
                    fresh.fast = fast;
                    fresh.autoplay = auto;
                    // A retry starts clean: ignore the one-off start hook state.
                    fresh.run = Run::new(seed, hero_of(character));
                    fresh.rev = g.rev + 1;
                    fresh.script.clear();
                    *g = fresh;
                    sfx.write(SfxCue::SelectMission);
                }
            }
            c => {
                match c {
                    Cmd::EndTurn | Cmd::Play(..) | Cmd::Node(_) | Cmd::LeaveReward | Cmd::LeaveShop | Cmd::LeaveRest | Cmd::LeaveEvent | Cmd::RestHeal | Cmd::RewardCard(_) | Cmd::SkipCard => {
                        if !g.busy() || !matches!(c, Cmd::EndTurn | Cmd::Play(..)) {
                            sfx.write(SfxCue::MoveMenu);
                        }
                    }
                    Cmd::BuyCard(_) | Cmd::BuyRelic(_) | Cmd::BuyPotion(_) | Cmd::TakeGold => {
                        sfx.write(SfxCue::Coin { underwater: false });
                    }
                    _ => {}
                }
                g.execute(c);
            }
        }
    }
}

/// Fire due playback events: they advance the displayed numbers and trigger the visuals.
fn pump_events(mut g: ResMut<DeckGame>, mut sfx: MessageWriter<SfxCue>, mut particles: ResMut<crate::fx::particles::Particles>, state: Res<crate::shell::AppState>) {
    if *state != crate::shell::AppState::Playing {
        return;
    }
    while g.queue.front().is_some_and(|(t, _)| *t <= g.t) {
        let (_, ev) = g.queue.pop_front().unwrap();
        fire(&mut g, ev, &mut sfx, &mut particles);
    }
    // Self-heal the bookkeeping once everything has played.
    if g.queue.is_empty() && g.t >= g.q_end {
        let snap = g.run.combat.as_ref().map(|c| (c.draw.len(), c.discard.len(), c.exhaust.len(), c.hp, c.shell, c.momentum, c.chain, c.potions));
        if let Some((d, di, ex, hp, shell, m, ch, pots)) = snap {
            g.disp.draw_n = d;
            g.disp.discard_n = di;
            g.disp.exhaust_n = ex;
            g.disp.hp = hp;
            g.disp.shell = shell;
            g.disp.momentum = m;
            g.disp.chain = ch;
            g.disp.potions = pots;
            g.disp.enemy_turn = false;
        }
    }
}

fn st_color(s: Status) -> Color {
    if s.debuff() { Color::srgb(1.0, 0.55, 0.35) } else { Color::srgb(0.6, 1.0, 0.65) }
}

fn fire(g: &mut DeckGame, ev: Ev, sfx: &mut MessageWriter<SfxCue>, particles: &mut crate::fx::particles::Particles) {
    let hero_pos = g.hero_world() + Vec3::new(0.0, 2.0, 0.0);
    match ev {
        Ev::TurnStart { turn } => {
            g.disp.turn = turn;
            g.disp.enemy_turn = false;
            g.say(&format!("TURN {turn}"), Color::srgb(1.0, 0.93, 0.55));
        }
        Ev::Draw { uid } => {
            if let Some(card) = g.uid_card.get(&uid).copied() {
                g.disp.hand.push(Inst { card, uid });
            }
            g.disp.draw_n = g.disp.draw_n.saturating_sub(1);
        }
        Ev::Reshuffle => {
            g.disp.draw_n += g.disp.discard_n;
            g.disp.discard_n = 0;
        }
        Ev::Play { uid, target, mv, attack } => {
            g.disp.hand.retain(|i| i.uid != uid);
            g.leave.insert(uid, Leave::Play);
            let home = hero_home(true);
            let t = g.t;
            let fast = g.fast;
            let foe_home = |i: usize| g.foes3d.get(i).map(|f| f.home);
            let mid = foe_home(target.unwrap_or(0)).or_else(|| g.foes3d.first().map(|f| f.home)).unwrap_or(at(stage(), 480.0, 0.0, 0.0));
            let anim = if attack {
                // Dash to just in front of the target (or the middle of the group).
                let dest = if target.is_some() { at(mid, -250.0, 0.0, 0.0) } else { at(stage(), 80.0, 0.0, 0.0) };
                match mv {
                    Move::Jump => HeroAnim::Lunge { act: ACT_DOUBLE_JUMP, to: dest, arc: 260.0 },
                    Move::Dive => HeroAnim::Lunge { act: ACT_DIVE, to: dest, arc: 90.0 },
                    Move::Pound => HeroAnim::Lunge { act: ACT_GROUND_POUND, to: at(dest, 130.0, 0.0, 0.0), arc: 380.0 },
                    Move::Strike => HeroAnim::Lunge { act: ACT_PUNCHING, to: dest, arc: 0.0 },
                    Move::Spin => HeroAnim::Lunge { act: ACT_TRIPLE_JUMP, to: dest, arc: 200.0 },
                    _ => HeroAnim::Lunge { act: ACT_JUMP_KICK, to: dest, arc: 160.0 },
                }
            } else {
                match mv {
                    Move::Jump => HeroAnim::InPlace { act: ACT_SIDE_FLIP },
                    Move::Brace => HeroAnim::InPlace { act: ACT_START_CROUCHING },
                    Move::Spin => HeroAnim::InPlace { act: ACT_TRIPLE_JUMP },
                    _ => HeroAnim::InPlace { act: ACT_BACKFLIP },
                }
            };
            let dur = if attack { 1.15 * fast.max(0.35) } else { 0.8 * fast.max(0.35) };
            let _ = home;
            g.choreo = Some(Choreo { anim, t0: t, dur, started: false });
        }
        Ev::Discard { uid } => {
            if g.disp.hand.iter().any(|i| i.uid == uid) {
                g.disp.hand.retain(|i| i.uid != uid);
                g.leave.insert(uid, Leave::Discard);
            }
            g.disp.discard_n += 1;
        }
        Ev::Exhaust { uid } => {
            if g.disp.hand.iter().any(|i| i.uid == uid) {
                g.disp.hand.retain(|i| i.uid != uid);
                g.leave.insert(uid, Leave::Exhaust);
            }
            g.disp.exhaust_n += 1;
        }
        Ev::Hit { to, from, dealt, blocked, hp, shell } => {
            let pos = match to {
                Who::Player => hero_pos + Vec3::new(0.0, 1.4, 0.0),
                Who::Foe(i) => g.foe_world(i) + Vec3::new(0.0, 1.2, 0.0),
            };
            match to {
                Who::Player => {
                    g.disp.hp = hp;
                    g.disp.shell = shell;
                    if dealt > 0 {
                        g.hero_hurt_until = g.t + 0.5;
                        g.choreo = Some(Choreo { anim: HeroAnim::Hurt, t0: g.t, dur: 0.6, started: false });
                        g.shake = 0.8;
                        sfx.write(SfxCue::Bonk);
                    } else {
                        sfx.write(SfxCue::Effect { kind: "bonk", pos: None });
                    }
                }
                Who::Foe(i) => {
                    if let Some(f) = g.disp.foes.get_mut(i) {
                        f.hp = hp;
                        f.shell = shell;
                    }
                    let t = g.t;
                    if let Some(f) = g.foes3d.get_mut(i) {
                        f.hurt = t;
                    }
                    let home = g.foes3d.get(i).map_or(stage(), |f| f.home);
                    particles.sparks(at(home, 0.0, 90.0, 0.0), if dealt > 0 { 12 } else { 5 });
                    g.shake = g.shake.max(if dealt > 0 { 0.35 } else { 0.1 });
                    sfx.write(SfxCue::StompGoomba);
                }
            }
            let _ = from;
            if dealt > 0 {
                g.float(pos, format!("-{dealt}"), if to == Who::Player { Color::srgb(1.0, 0.4, 0.35) } else { Color::srgb(1.0, 0.95, 0.5) }, dealt >= 10);
            } else if blocked > 0 {
                g.float(pos, "BLOCKED", Color::srgb(0.6, 0.85, 1.0), false);
            }
        }
        Ev::GainShell { who, amount, shell } => {
            let pos = match who {
                Who::Player => hero_pos + Vec3::new(0.0, 1.6, 0.0),
                Who::Foe(i) => g.foe_world(i) + Vec3::new(0.0, 1.4, 0.0),
            };
            match who {
                Who::Player => g.disp.shell = shell,
                Who::Foe(i) => {
                    if let Some(f) = g.disp.foes.get_mut(i) {
                        f.shell = shell;
                    }
                }
            }
            g.float(pos, format!("+{amount} Shell"), Color::srgb(0.55, 0.85, 1.0), false);
            sfx.write(SfxCue::Effect { kind: "break_box", pos: None });
        }
        Ev::Heal { amount, hp, .. } => {
            g.disp.hp = hp;
            if amount > 0 {
                g.float(hero_pos + Vec3::new(0.0, 1.6, 0.0), format!("+{amount} HP"), Color::srgb(0.5, 1.0, 0.55), false);
                sfx.write(SfxCue::OneUp);
            }
        }
        Ev::Status { who, status, now } => {
            let (prev, pos) = match who {
                Who::Player => (g.disp.st.get(status), hero_pos + Vec3::new(0.0, 2.1, 0.0)),
                Who::Foe(i) => (g.disp.foes.get(i).map_or(0, |f| f.st.get(status)), g.foe_world(i) + Vec3::new(0.0, 2.0, 0.0)),
            };
            match who {
                Who::Player => g.disp.st.set(status, now),
                Who::Foe(i) => {
                    if let Some(f) = g.disp.foes.get_mut(i) {
                        f.st.set(status, now);
                    }
                }
            }
            if now > prev {
                g.float(pos, format!("{} +{}", status.name(), now - prev), st_color(status), false);
            }
        }
        Ev::Momentum { now, chain, link } => {
            g.disp.momentum = now;
            g.disp.chain = chain;
            if link && chain >= 1 {
                let c = if chain >= 3 { Color::srgb(1.0, 0.6, 0.2) } else { Color::srgb(1.0, 0.9, 0.35) };
                g.float(hero_pos + Vec3::new(0.0, 2.5, 0.0), format!("CHAIN x{}", chain + 1), c, true);
                particles.sparks(at(hero_home(true), 0.0, 120.0, 0.0), 10);
                sfx.write(SfxCue::Effect { kind: "coin", pos: None });
            }
        }
        Ev::Intent { foe } => {
            let intent = g.run.combat.as_ref().and_then(|c| c.foes.get(foe)).map(|f| f.intent);
            if let (Some(i), Some(f)) = (intent, g.disp.foes.get_mut(foe)) {
                f.intent = i;
            }
        }
        Ev::FoeActs { foe, attack } => {
            g.disp.enemy_turn = true;
            let t = g.t;
            let fast = g.fast;
            if let Some(f) = g.foes3d.get_mut(foe) {
                f.lunge = Some((t, if attack { 0.55 } else { 0.5 } * fast.max(0.35)));
            }
        }
        Ev::Died { foe } => {
            if let Some(f) = g.disp.foes.get_mut(foe) {
                f.alive = false;
                f.hp = 0;
            }
            let t = g.t;
            let home = g.foes3d.get(foe).map_or(stage(), |f| f.home);
            if let Some(f) = g.foes3d.get_mut(foe) {
                f.died = Some(t);
            }
            particles.pound(at(home, 0.0, 20.0, 0.0));
            particles.sparks(at(home, 0.0, 120.0, 0.0), 18);
            sfx.write(SfxCue::Effect { kind: "break_box", pos: None });
            sfx.write(SfxCue::Effect { kind: "coin", pos: None });
        }
        Ev::Summoned { foe } => {
            if let Some(c) = g.run.combat.as_ref() {
                if let Some(f) = c.foes.get(foe) {
                    let d = DFoe { kind: f.kind, hp: f.hp, max_hp: f.max_hp, shell: f.shell, st: f.st, intent: f.intent, alive: true };
                    while g.disp.foes.len() <= foe {
                        g.disp.foes.push(DFoe { alive: false, ..d.clone() });
                    }
                    g.disp.foes[foe] = d;
                }
            }
            sfx.write(SfxCue::Teleport);
        }
        Ev::Junk { count } => {
            g.disp.discard_n += count as usize;
            g.float(hero_pos + Vec3::new(0.0, 2.2, 0.0), format!("+{count} Bruise"), Color::srgb(0.85, 0.5, 0.9), false);
        }
        Ev::Potion { slot } => {
            if let Some(p) = g.disp.potions.get_mut(slot) {
                *p = None;
            }
            sfx.write(SfxCue::Effect { kind: "teleport", pos: None });
        }
        Ev::Won => {
            g.say("VICTORY!", Color::srgb(1.0, 0.85, 0.2));
            let t = g.t;
            g.choreo = Some(Choreo { anim: HeroAnim::Cheer, t0: t, dur: 1.4, started: false });
            sfx.write(SfxCue::Star);
        }
        Ev::Lost => {
            g.say("DEFEATED", Color::srgb(1.0, 0.4, 0.35));
            sfx.write(SfxCue::DeathWarp);
        }
    }
}

/// Combat bookkeeping between the timeline and the run: close the fight, save records.
fn flow(mut g: ResMut<DeckGame>, state: Res<crate::shell::AppState>) {
    if *state != crate::shell::AppState::Playing {
        return;
    }
    if g.run.screen == Screen::Combat && !g.combat_open {
        g.start_combat_view();
    }
    if g.run.screen == Screen::Combat {
        let over = g.run.combat.as_ref().is_some_and(|c| c.over());
        if over && !g.busy() {
            let now = g.t;
            let at = *g.end_at.get_or_insert(now + 1.1);
            if now >= at {
                g.end_at = None;
                g.combat_open = false;
                g.run.finish_combat();
                g.rev += 1;
                g.selected = None;
                g.aim = None;
            }
        }
    }
    if g.run.ended() && g.summary_at == 0.0 {
        g.summary_at = g.t;
        // Records.
        if !g.cheated {
            let depth = g.run.stats.nodes_cleared;
            let score = g.run.score();
            let mut s = g.saved;
            s.runs += 1;
            s.wins += g.run.won() as u32;
            g.record_new = score > s.best_score || depth > s.best_depth;
            s.best_depth = s.best_depth.max(depth);
            s.best_score = s.best_score.max(score);
            g.saved = s;
            write_saved(s);
        }
        g.rev += 1;
    }
    if !g.run.ended() {
        g.summary_at = 0.0;
    }
}

// ---------------------------------------------------------------------------------------------
// Foe models
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct FoeBody(pub usize);

fn smooth(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

#[allow(clippy::too_many_arguments)]
fn sync_foes(
    mut g: ResMut<DeckGame>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tfs: Query<(&mut Transform, &mut Visibility), With<FoeBody>>,
    mut rigs: Query<&mut crate::render::object_view::AnimRig, With<FoeBody>>,
) {
    let combat = g.run.screen == Screen::Combat;
    // Spawn bodies for foes that appear (initial and summoned).
    if combat {
        let n = g.run.combat.as_ref().map_or(0, |c| c.foes.len());
        while g.foes3d.len() < n {
            let i = g.foes3d.len();
            let Some(kind) = g.run.combat.as_ref().map(|c| c.foes[i].kind) else { break };
            let info = kind.info();
            let home = foe_slot(i, g.n_start);
            let entity = if root.exists(info.model) {
                let mut e = crate::render::object_view::spawn_model(&mut commands, &assets, &root, info.model, true);
                e.insert((XpEntity, FoeBody(i), Visibility::Hidden));
                e.id()
            } else {
                let mesh = meshes.add(Sphere::new(0.7).mesh().ico(1).unwrap());
                let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.8, 0.3, 0.3), ..default() });
                commands.spawn((XpEntity, FoeBody(i), Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden)).id()
            };
            let born = g.t + 0.15 * i as f64;
            g.foes3d.push(Foe3d { entity, kind, home, born, lunge: None, hurt: -10.0, died: None });
        }
    } else if !g.foes3d.is_empty() {
        for f in g.foes3d.drain(..) {
            commands.entity(f.entity).despawn();
        }
        g.foe_screen.clear();
        return;
    }
    let t = g.t;
    let targeted = g.selected.and_then(|h| g.run.combat.as_ref().and_then(|c| c.hand.get(h)).map(|i| i.card.targeted())).unwrap_or(false);
    let _ = targeted;
    for (i, f) in g.foes3d.iter().enumerate() {
        let Ok((mut tf, mut vis)) = tfs.get_mut(f.entity) else { continue };
        let info = f.kind.info();
        let alive_u = ((t - f.born) / 0.4).clamp(0.0, 1.0);
        let mut scale = FOE_K * info.scale * smooth(alive_u) as f32;
        let mut pos = to_bevy(f.home);
        pos.y += info.hover * 100.0 * 0.01 * 1.5 + (t as f32 * 2.0 + i as f32 * 1.7).sin() * if info.hover > 0.0 { 0.12 } else { 0.0 };
        let mut yaw = -std::f32::consts::FRAC_PI_2;
        let mut visible = alive_u > 0.0;
        if let Some((t0, dur)) = f.lunge {
            let u = ((t - t0) / dur).clamp(0.0, 1.0);
            if u < 1.0 {
                // Wind back, then lunge toward the hero and return.
                let k = if u < 0.35 { -0.25 * smooth(u / 0.35) as f32 } else if u < 0.6 { -0.25 + 1.25 * smooth((u - 0.35) / 0.25) as f32 } else { 1.0 - smooth((u - 0.6) / 0.4) as f32 };
                pos.x -= k * 3.4;
                pos.y += (u * std::f64::consts::PI) as f32 * 0.5;
            }
        }
        if let Some(d) = f.died {
            let u = ((t - d) / 0.45).clamp(0.0, 1.0);
            scale *= (1.0 - smooth(u)) as f32;
            yaw += u as f32 * 9.0;
            pos.y += (u as f32) * 1.2;
            visible = u < 1.0;
        } else if t - f.hurt < 0.3 {
            let u = ((t - f.hurt) / 0.3) as f32;
            pos.x += (u * 40.0).sin() * 0.2 * (1.0 - u);
            scale *= 1.0 + 0.16 * (1.0 - u);
        }
        tf.translation = pos;
        tf.rotation = Quat::from_rotation_y(yaw);
        tf.scale = Vec3::splat(scale.max(0.0001));
        let want = if visible { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let Ok(mut rig) = rigs.get_mut(f.entity) {
            rig.speed = if f.lunge.is_some_and(|(t0, d)| t - t0 < d) { 2.4 } else { 1.0 };
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Props that dress the non-combat screens
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct Prop {
    screens: &'static [Screen],
    spin: f32,
}

#[derive(Component)]
struct PropLight;

#[derive(Component)]
struct Flicker(f32);

#[allow(clippy::too_many_arguments)]
fn update_props(
    g: Res<DeckGame>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    mut spawned: Local<bool>,
    mut props: Query<(&Prop, &mut Visibility, &mut Transform), (Without<PropLight>, Without<Flicker>)>,
    mut lights: Query<&mut Visibility, (With<PropLight>, Without<Prop>, Without<Flicker>)>,
    mut flickers: Query<(&Flicker, &mut Transform), (Without<Prop>, Without<PropLight>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut particles: ResMut<crate::fx::particles::Particles>,
    mut ember_t: Local<f64>,
) {
    if g.run.screen == Screen::Rest && g.t - *ember_t > 0.09 {
        *ember_t = g.t;
        let o = at(stage(), -60.0, 60.0, 170.0);
        particles.burst(o, crate::fx::particles::BurstOptions { color: 0xffa238, count: 1, speed: 35.0, life: 1.1, gravity: -90.0, size: 12.0, upward: 110.0, spread: 1.0 });
        if ((g.t * 11.0) as u64).is_multiple_of(4) {
            particles.burst(o, crate::fx::particles::BurstOptions { color: 0xffe9a0, count: 1, speed: 20.0, life: 0.7, gravity: -60.0, size: 9.0, upward: 140.0, spread: 1.0 });
        }
    }
    if !*spawned {
        *spawned = true;
        // Campfire: a ring of stones, crossed logs, a glow on the ground and layered additive flame tongues (red outside, orange,
        // yellow, white-hot core) that flicker out of phase, plus rising embers (see below).
        let log = materials.add(StandardMaterial { base_color: Color::srgb(0.35, 0.2, 0.1), perceptual_roughness: 1.0, ..default() });
        let stone = materials.add(StandardMaterial { base_color: Color::srgb(0.45, 0.43, 0.46), perceptual_roughness: 1.0, ..default() });
        let glow = |r: f32, g: f32, b: f32, a: f32| StandardMaterial { base_color: Color::srgba(r, g, b, a), emissive: LinearRgba::new(r * 5.0, g * 5.0, b * 5.0, 1.0), unlit: true, alpha_mode: AlphaMode::Add, cull_mode: None, ..default() };
        let layers = [
            (materials.add(glow(0.9, 0.18, 0.04, 0.55)), 0.62, 1.35, 0.0),
            (materials.add(glow(1.0, 0.45, 0.08, 0.7)), 0.46, 1.2, 0.0),
            (materials.add(glow(1.0, 0.8, 0.2, 0.8)), 0.30, 1.05, 0.0),
            (materials.add(glow(1.0, 1.0, 0.8, 0.9)), 0.16, 0.7, 0.0),
        ];
        let ground_glow = materials.add(glow(1.0, 0.45, 0.1, 0.35));
        let log_mesh = meshes.add(Cuboid::new(0.2, 0.2, 1.4));
        let stone_mesh = meshes.add(Sphere::new(0.17));
        let disc = meshes.add(Circle::new(1.25));
        commands
            .spawn((XpEntity, Prop { screens: &[Screen::Rest], spin: 0.0 }, Visibility::Hidden, Transform::from_translation(to_bevy(at(stage(), -60.0, 0.0, 170.0)))))
            .with_children(|p| {
                p.spawn((Mesh3d(disc), MeshMaterial3d(ground_glow), Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)), Visibility::Inherited));
                for i in 0..3 {
                    p.spawn((Mesh3d(log_mesh.clone()), MeshMaterial3d(log.clone()), Transform::from_xyz(0.0, 0.14, 0.0).with_rotation(Quat::from_rotation_y(i as f32 * 1.05) * Quat::from_rotation_x(0.2)), Visibility::Inherited));
                }
                for i in 0..9 {
                    let a = i as f32 / 9.0 * std::f32::consts::TAU;
                    p.spawn((Mesh3d(stone_mesh.clone()), MeshMaterial3d(stone.clone()), Transform::from_xyz(a.cos() * 0.95, 0.1, a.sin() * 0.95).with_scale(Vec3::new(1.1, 0.7, 1.0)), Visibility::Inherited));
                }
                // Tongues: each layer is a few cones around the centre, leaning outward, shorter toward the middle layers' tips.
                for (li, (mat, r, h, _)) in layers.iter().enumerate() {
                    let cone = meshes.add(Cone { radius: *r * 0.55, height: *h });
                    let n = if li == 0 { 5 } else if li == 3 { 1 } else { 4 };
                    for k in 0..n {
                        let a = k as f32 / n as f32 * std::f32::consts::TAU + li as f32 * 0.7;
                        let off = if n == 1 { 0.0 } else { 0.22 - li as f32 * 0.04 };
                        let lean = if n == 1 { 0.0 } else { 0.18 };
                        p.spawn((
                            Flicker(li as f32 * 1.3 + k as f32 * 2.1),
                            Mesh3d(cone.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(a.cos() * off, 0.12 + h * 0.5, a.sin() * off).with_rotation(Quat::from_rotation_z(-a.cos() * lean) * Quat::from_rotation_x(a.sin() * lean)),
                            Visibility::Inherited,
                        ));
                    }
                }
            });
        let mut put = |model: &str, pos: [f64; 3], scale: f32, yaw: f32, screens: &'static [Screen], spin: f32| {
            if !root.exists(model) {
                return;
            }
            let mut e = crate::render::object_view::spawn_model(&mut commands, &assets, &root, model, true);
            e.insert((XpEntity, Prop { screens, spin }, Visibility::Hidden, Transform::from_translation(to_bevy(pos)).with_scale(Vec3::splat(scale)).with_rotation(Quat::from_rotation_y(yaw))));
        };
        // Positions are chosen against the phone layout (874x402): the shop panel covers the middle, so Toad and his coins stand in the
        // free strip at the right edge; the rest / event / treasure panels fill the right 40%, so those props sit left of x = 120.
        put("actors/toad/toad_geo.glb", at(stage(), 740.0, 0.0, 230.0), 2.2, -0.7, &[Screen::Shop], 0.0);
        put("actors/coin/yellow_coin_geo.glb", at(stage(), 560.0, 30.0, 330.0), 2.2, 0.0, &[Screen::Shop], 2.0);
        put("actors/coin/yellow_coin_geo.glb", at(stage(), 250.0, 30.0, 360.0), 2.2, 0.0, &[Screen::Shop], 2.5);
        put("actors/treasure_chest/treasure_chest_base_geo.glb", at(stage(), 20.0, 0.0, 120.0), 1.5, -0.5, &[Screen::Treasure], 0.0);
        put("actors/exclamation_box/exclamation_box_geo.glb", at(stage(), -40.0, 120.0, 120.0), 2.4, 0.0, &[Screen::Event], 1.2);
        put("actors/star/star_geo.glb", at(stage(), 260.0, 190.0, 0.0), 3.0, 0.0, &[Screen::Reward, Screen::Won], 2.4);
        commands.spawn((XpEntity, PropLight, PointLight { intensity: 3_000_000.0, range: 12.0, color: Color::srgb(1.0, 0.6, 0.25), ..default() }, Transform::from_translation(to_bevy(at(stage(), -40.0, 120.0, 120.0))), Visibility::Hidden));
    }
    let screen = g.run.screen;
    for (p, mut v, mut tf) in &mut props {
        let want = if p.screens.contains(&screen) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
        if p.spin != 0.0 {
            tf.rotation = Quat::from_rotation_y(g.t as f32 * p.spin);
        }
    }
    for (f, mut tf) in &mut flickers {
        let k = 1.0 + 0.12 * (g.t as f32 * 11.0 + f.0 * 3.0).sin() + 0.06 * (g.t as f32 * 23.0 + f.0).sin();
        tf.scale = Vec3::new(1.0 / k.sqrt(), k, 1.0 / k.sqrt());
    }
    for mut v in &mut lights {
        let want = if screen == Screen::Rest { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Camera, ambience, projection
// ---------------------------------------------------------------------------------------------

fn camera_and_ambience(g: Res<DeckGame>, mut shell: ResMut<super::Shell>, mut cam: ResMut<super::CameraOverride>, mut palette: ResMut<crate::fx::sky::SkyPalette>, mut clear: ResMut<ClearColor>, mut state: Local<Option<(Vec3, Vec3)>>, time: Res<Time>) {
    let combat = g.run.screen == Screen::Combat;
    let show_hero = g.run.screen != Screen::Map;
    if shell.player_visible != show_hero {
        shell.player_visible = show_hero;
    }
    // Eye / look targets in Bevy units.
    let (eye_t, look_t) = if combat {
        (to_bevy(at(stage(), 150.0, 560.0, 1500.0)), to_bevy(at(stage(), 150.0, 20.0, 0.0)))
    } else {
        match g.run.screen {
            Screen::Map | Screen::Won | Screen::Lost => (to_bevy(at(stage(), -80.0, 330.0, 1250.0)), to_bevy(at(stage(), -90.0, 150.0, 0.0))),
            _ => (to_bevy(at(stage(), 40.0, 380.0, 1350.0)), to_bevy(at(stage(), 60.0, 110.0, 0.0))),
        }
    };
    let k = (time.delta_secs() * 2.4).min(1.0);
    let (mut eye, mut look) = state.unwrap_or((eye_t, look_t));
    eye = eye.lerp(eye_t, k);
    look = look.lerp(look_t, k);
    *state = Some((eye, look));
    let sway = (g.t as f32 * 0.3).sin() * 0.25;
    let shake = g.shake;
    let jitter = Vec3::new((g.t as f32 * 90.0).sin(), (g.t as f32 * 77.0).cos(), 0.0) * shake * 0.08;
    let e = DVec3::new((eye.x + sway) as f64 / 0.01, (eye.y + jitter.y) as f64 / 0.01, eye.z as f64 / 0.01);
    let l = DVec3::new((look.x + jitter.x) as f64 / 0.01, look.y as f64 / 0.01, look.z as f64 / 0.01);
    cam.pose = Some(CamPose { eye: e, look: l, up: DVec3::Y, fov: 36.0, near: 20.0, far: 60000.0 });
    // Mood per node kind.
    let kind = g.run.pos.map(|p| g.run.map.nodes[p].kind);
    let (top, mid, bot, bg) = match kind {
        Some(NodeKind::Boss) => (0x1a0510, 0x8a1630, 0xff6a2a, Color::srgb(0.25, 0.04, 0.08)),
        Some(NodeKind::Elite) => (0x0f1f4a, 0x3a5fb8, 0xc7a6ff, Color::srgb(0.10, 0.12, 0.30)),
        Some(NodeKind::Rest) => (0x0a1030, 0x2f3d8a, 0xf0a060, Color::srgb(0.08, 0.10, 0.25)),
        Some(NodeKind::Shop) => (0x1d3a66, 0x4a9ad0, 0xffe4a0, Color::srgb(0.2, 0.3, 0.5)),
        _ => (0x2a1f66, 0xc2578f, 0xffb87a, Color::srgb(0.20, 0.14, 0.38)),
    };
    let want = crate::fx::sky::SkyPalette::from_hex(top, mid, bot).with_clouds(0.3);
    if *palette != want {
        palette.lerp_to(&want, (time.delta_secs() * 2.0).min(1.0));
        let d = (palette.top[0] - want.top[0]).abs() + (palette.mid[0] - want.mid[0]).abs() + (palette.bottom[0] - want.bottom[0]).abs();
        if d < 0.004 {
            *palette = want;
        }
    }
    if clear.0 != bg {
        clear.0 = bg;
    }
}

fn project_foes(mut g: ResMut<DeckGame>, cameras: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>) {
    let Ok((camera, gt)) = cameras.single() else { return };
    let n = g.foes3d.len();
    let mut out = vec![FoeScreen::default(); n];
    for (i, f) in g.foes3d.iter().enumerate() {
        let info = f.kind.info();
        let base = to_bevy(f.home);
        let h = info.height * FOE_K * info.scale + info.hover * 0.01 * 1.5 * 100.0 * 0.0 + if info.hover > 0.0 { info.hover * 1.4 } else { 0.0 };
        let (Ok(b), Ok(t), Ok(c)) = (camera.world_to_viewport(gt, base), camera.world_to_viewport(gt, base + Vec3::Y * h), camera.world_to_viewport(gt, base + Vec3::Y * h * 0.5)) else { continue };
        let half_w = ((b.y - t.y).abs() * 0.45).clamp(22.0, 80.0);
        out[i] = FoeScreen { center: c, top: t, bottom: b, half_w, on: true };
    }
    g.foe_screen = out;
}

// ---------------------------------------------------------------------------------------------
// The hero: the real player model, driven through the sim
// ---------------------------------------------------------------------------------------------

fn put_hero(sim: &mut GameSim, pos: [f64; 3], action: u32, yaw: f64) {
    let m = &mut sim.mario;
    m.pos = pos;
    m.vel = [0.0; 3];
    m.forward_vel = 0.0;
    m.slide_vel_x = 0.0;
    m.slide_vel_z = 0.0;
    m.face_angle = [0.0, yaw, 0.0];
    m.intended_yaw = yaw;
    m.peak_height = pos[1];
    let floor = m.world.find_floor(pos[0], pos[1] + 100.0, pos[2]);
    m.floor_height = floor.height;
    m.floor = floor.surface;
    m.set_action(action, 0.0);
    sim.curr = sim.snapshot();
    sim.prev = sim.curr.clone();
}

fn drive_hero(mut g: ResMut<DeckGame>, mut sim: ResMut<GameSim>) {
    let combat = g.run.screen == Screen::Combat;
    let home = hero_home(combat);
    let now = g.t;
    let floor_y = stage()[1];
    let Some(mut c) = g.choreo else {
        // Idle at the home mark; snap back if anything nudged us off it.
        let d = (sim.mario.pos[0] - home[0]).hypot(sim.mario.pos[2] - home[2]);
        let idleish = matches!(sim.mario.action, ACT_IDLE | ACT_FREEFALL | ACT_JUMP_LAND_STOP | ACT_BACKFLIP_LAND_STOP | ACT_SIDE_FLIP_LAND_STOP | ACT_BACKFLIP_LAND | ACT_SIDE_FLIP_LAND | ACT_CROUCHING | ACT_START_CROUCHING);
        let low_hp = combat && g.disp.hp > 0 && g.disp.hp * 4 < g.disp.max_hp;
        let _ = low_hp;
        if d > 40.0 || !idleish || sim.mario.pos[1] > floor_y + 400.0 {
            put_hero(&mut sim, at(home, 0.0, 10.0, 0.0), ACT_IDLE, HERO_YAW);
        }
        return;
    };
    let u = (now - c.t0) / c.dur;
    if u >= 1.0 {
        g.choreo = None;
        put_hero(&mut sim, at(home, 0.0, 10.0, 0.0), ACT_IDLE, HERO_YAW);
        return;
    }
    match c.anim {
        HeroAnim::Lunge { act, to, arc } => {
            // 0..0.4 go, 0.4..0.62 strike, then hop back.
            let (from, y0) = (home, floor_y);
            if !c.started {
                c.started = true;
                g.choreo = Some(c);
                put_hero(&mut sim, at(from, 0.0, 10.0, 0.0), act, HERO_YAW);
            }
            let (px, pz, py) = if u < 0.4 {
                let k = smooth(u / 0.4);
                (from[0] + (to[0] - from[0]) * k, from[2] + (to[2] - from[2]) * k, y0 + (arc * (k * std::f64::consts::PI).sin()))
            } else if u < 0.62 {
                (to[0], to[2], y0)
            } else {
                let k = smooth((u - 0.62) / 0.38);
                (to[0] + (from[0] - to[0]) * k, to[2] + (from[2] - to[2]) * k, y0 + 120.0 * (k * std::f64::consts::PI).sin())
            };
            if u >= 0.4 && u < 0.5 && sim.mario.action == ACT_DOUBLE_JUMP {
                sim.mario.set_action(ACT_JUMP_KICK, 0.0);
            }
            if u >= 0.62 && c.started && !matches!(sim.mario.action, ACT_JUMP | ACT_FREEFALL) {
                sim.mario.set_action(ACT_JUMP, 0.0);
            }
            let m = &mut sim.mario;
            m.pos = [px, py.max(y0), pz];
            m.vel = [0.0, if u < 0.4 { 0.0 } else { m.vel[1] }, 0.0];
            m.forward_vel = 0.0;
            m.face_angle[1] = if u < 0.62 { HERO_YAW } else { HERO_YAW };
            sim.curr.pos = sim.mario.pos;
        }
        HeroAnim::InPlace { act } => {
            if !c.started {
                c.started = true;
                g.choreo = Some(c);
                put_hero(&mut sim, at(home, 0.0, 10.0, 0.0), act, HERO_YAW);
            }
            let m = &mut sim.mario;
            m.pos[0] = home[0];
            m.pos[2] = home[2];
            sim.curr.pos = sim.mario.pos;
        }
        HeroAnim::Hurt => {
            if !c.started {
                c.started = true;
                g.choreo = Some(c);
                put_hero(&mut sim, at(home, 0.0, 10.0, 0.0), ACT_SOFT_BACKWARD_GROUND_KB, HERO_YAW);
            }
            let m = &mut sim.mario;
            m.pos[0] = home[0] - 60.0 * (u * std::f64::consts::PI).sin();
            m.pos[2] = home[2];
            sim.curr.pos = sim.mario.pos;
        }
        HeroAnim::Cheer => {
            if !c.started || sim.mario.action == ACT_IDLE || sim.mario.action == ACT_JUMP_LAND_STOP {
                c.started = true;
                g.choreo = Some(c);
                put_hero(&mut sim, at(home, 0.0, 10.0, 0.0), ACT_TRIPLE_JUMP, HERO_YAW);
            }
            let m = &mut sim.mario;
            m.pos[0] = home[0];
            m.pos[2] = home[2];
            sim.curr.pos = sim.mario.pos;
        }
    }
}
