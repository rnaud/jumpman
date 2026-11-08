//! The app shell: title/main menu, course + character select, pause menu, settings, controls
//! help, save-file slots, game over and the app state machine of `Game.ts`
//! (`started`/`settings.open`/`deathTransition`/`loadCourse`), all on `bevy_ui`.
//!
//! ```text
//! Boot -> Menu <-> Transition -> Playing <-> Paused
//!                      ^             |
//!                      +-------------+--> GameOver -> Menu
//! ```
//! [`AppState`] is a plain resource (the app does not use `bevy_state`). The sim only ticks and the
//! camera only moves in `Playing` (`SimSet`/`CameraSet` are gated here).
//!
//! # How it plugs into the other modules
//! * Course changes go through `progress::Progress::pending` (warps, doors, stars and the shell all
//!   share it, so wipes/save/lives stay consistent): [`StartCourse`] fades out, queues a fresh-run
//!   `Switch` and fades back in after `progress` reports the new course. Other plugins can send
//!   [`StartCourse`] too; `progress::CourseSwitched` fires when the new `GameSim` is live.
//! * Characters: the selection lives in `characters::Character` ([`SwitchCharacter`] writes it).
//! * Experiments: [`OpenExperiment`] / [`LeaveExperiment`] drive `experiments::ExperimentSlot`.
//! * Display: [`settings::DisplaySettings`] is persisted and mirrored into `fx::display::DisplaySettings`.
//! * Save files: the slot screen reads/writes `progress::save` (slot.txt, `SaveFile`) and shows star
//!   counts through [`SaveSlotSummaries`].
//! * [`ReturnToMenu`] - back to the title screen. [`GameOver`] - shows the shell's GAME OVER screen
//!   (the progress flow draws its own game-over wipe/overlay; this is for other modes).
//! * Pausing: `SimSet` and `CameraSet` run only in `Playing`, as do the progress/experiment fixed-step
//!   hooks (they read [`is_playing`]).
#![allow(dead_code)] // public surface for other plugins (events, catalogues)
pub mod nav;
pub mod screens;
pub mod scroll;
pub mod settings;
pub mod theme;

use crate::characters;
use crate::experiments::{self, ExperimentSlot};
use crate::fx::display as fxd;
use crate::game::courses;
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use crate::hud::Toast;
use crate::input::PlayerInput;
use crate::progress::flow::{Progress, Switch};
use crate::progress;
use bevy::prelude::*;
use settings::{Aspect, Settings};

/// Top-level app state (`Game.ts`: not started / playing / settings open / loading / dying).
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppState {
    Boot,
    Menu,
    Playing,
    Paused,
    Transition,
    GameOver,
}

impl AppState {
    /// States whose UI takes keyboard/gamepad navigation.
    pub fn in_menu(self) -> bool {
        matches!(self, AppState::Menu | AppState::Paused | AppState::GameOver)
    }
}

/// Desktop menus are designed for roughly a 1040x680 window; phones get a compact layout designed
/// for [`COMPACT_DESIGN_SIZE`] of usable (inside the safe area) space.
pub const MENU_DESIGN_SIZE: Vec2 = Vec2::new(1040.0, 680.0);
pub const COMPACT_DESIGN_SIZE: Vec2 = Vec2::new(760.0, 360.0);

/// Which menu layout fits the window, and how big it is drawn. `width`/`height` are the usable
/// space in layout px (already divided by `scale`), so screens can pick columns from them.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct MenuLayout {
    pub compact: bool,
    pub scale: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for MenuLayout {
    fn default() -> Self {
        Self { compact: false, scale: 1.0, width: MENU_DESIGN_SIZE.x, height: MENU_DESIGN_SIZE.y }
    }
}

/// Pick the layout class and scale from the logical window size and the safe-area insets: windows
/// under 960x560 (every phone) use the compact layout, drawn near 1.0x so text and tap targets keep
/// their real size; bigger windows use the desktop layout, scaled to fit.
pub fn menu_layout(window: Vec2, area: &crate::safe_area::SafeArea) -> MenuLayout {
    let compact = window.x < 960.0 || window.y < 560.0;
    let usable = Vec2::new(window.x - area.left - area.right, window.y - area.top - area.bottom).max(Vec2::splat(1.0));
    let design = if compact { COMPACT_DESIGN_SIZE } else { MENU_DESIGN_SIZE };
    let fit = (usable.x / design.x).min(usable.y / design.y);
    let scale = if compact { fit.clamp(0.85, 1.5) } else { fit.clamp(0.6, 1.5) };
    MenuLayout { compact, scale, width: usable.x / scale, height: usable.y / scale }
}

pub fn fit_menu_scale(
    state: Res<AppState>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    area: Res<crate::safe_area::SafeArea>,
    mut layout: ResMut<MenuLayout>,
    mut ui_scale: ResMut<UiScale>,
) {
    let Ok(w) = windows.single() else { return };
    let next = menu_layout(Vec2::new(w.width(), w.height()), &area);
    if *layout != next {
        *layout = next;
    }
    let target = if state.in_menu() { next.scale } else { 1.0 };
    if (ui_scale.0 - target).abs() > 1e-3 {
        ui_scale.0 = target;
    }
}

/// Keep menu content clear of the notch / Dynamic Island / home indicator (iOS). Padding is in
/// pre-`UiScale` px, so divide the inset by the scale.
pub fn apply_safe_area(area: Res<crate::safe_area::SafeArea>, ui_scale: Res<UiScale>, mut roots: Query<&mut Node, With<screens::ScreenRoot>>) {
    let s = ui_scale.0.max(1e-3);
    let padding = UiRect { left: Val::Px(area.left / s), right: Val::Px(area.right / s), top: Val::Px(area.top / s), bottom: Val::Px(area.bottom / s) };
    for mut node in &mut roots {
        if node.padding != padding {
            node.padding = padding;
        }
    }
}

pub fn is_playing(state: Res<AppState>) -> bool {
    *state == AppState::Playing
}

/// Playable characters (`data-character` in index.html).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Character {
    Mario,
    AdultLink,
    YoungLink,
    Banjo,
}

impl Character {
    pub const ALL: [Character; 4] = [Character::Mario, Character::AdultLink, Character::YoungLink, Character::Banjo];

    pub fn id(self) -> &'static str {
        match self {
            Character::Mario => "sm64",
            Character::AdultLink => "link",
            Character::YoungLink => "young-link",
            Character::Banjo => "banjo",
        }
    }

    /// Short caps label for the hero picker.
    pub fn short(self) -> &'static str {
        match self {
            Character::Mario => "MARIO",
            Character::AdultLink => "LINK",
            Character::YoungLink => "YOUNG LINK",
            Character::Banjo => "BANJO",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Character::Mario => "Mario",
            Character::AdultLink => "Adult Link",
            Character::YoungLink => "Young Link",
            Character::Banjo => "Banjo-Kazooie",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    /// The `characters` module's resource value for this choice.
    pub fn theirs(self) -> characters::Character {
        match self {
            Character::Mario => characters::Character::Mario,
            Character::AdultLink => characters::Character::Link,
            Character::YoungLink => characters::Character::YoungLink,
            Character::Banjo => characters::Character::Banjo,
        }
    }

    pub fn from_theirs(c: characters::Character) -> Self {
        match c {
            characters::Character::Mario => Character::Mario,
            characters::Character::Link => Character::AdultLink,
            characters::Character::YoungLink => Character::YoungLink,
            characters::Character::Banjo => Character::Banjo,
        }
    }
}

/// The experiments of the menu (`data-experiment`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExperimentId {
    Dimensions,
    God,
    Runner,
    Hundred,
    Diorama,
    Race,
    Shrink,
    Planet,
    Swarm,
    Kart,
    LastStar,
    Relic,
    Gate,
    Deck,
    Halls,
    Brawl,
    Party,
}

impl ExperimentId {
    pub const ALL: [ExperimentId; 17] = [
        ExperimentId::Dimensions,
        ExperimentId::God,
        ExperimentId::Runner,
        ExperimentId::Hundred,
        ExperimentId::Diorama,
        ExperimentId::Race,
        ExperimentId::Shrink,
        ExperimentId::Planet,
        ExperimentId::Swarm,
        ExperimentId::Kart,
        ExperimentId::LastStar,
        ExperimentId::Relic,
        ExperimentId::Gate,
        ExperimentId::Deck,
        ExperimentId::Halls,
        ExperimentId::Brawl,
        ExperimentId::Party,
    ];

    /// The TS id (`dimensions`, `god`, ...).
    pub fn id(self) -> &'static str {
        match self {
            ExperimentId::Dimensions => "dimensions",
            ExperimentId::God => "god",
            ExperimentId::Runner => "runner",
            ExperimentId::Hundred => "hundred",
            ExperimentId::Diorama => "diorama",
            ExperimentId::Race => "race",
            ExperimentId::Shrink => "shrink",
            ExperimentId::Planet => "planet",
            ExperimentId::Swarm => "swarm",
            ExperimentId::Kart => "kart",
            ExperimentId::LastStar => "laststar",
            ExperimentId::Relic => "relic",
            ExperimentId::Gate => "gate",
            ExperimentId::Deck => "stardeck",
            ExperimentId::Halls => "halls",
            ExperimentId::Brawl => "brawl",
            ExperimentId::Party => "party",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            ExperimentId::Dimensions => "Across Dimensions",
            ExperimentId::God => "Mario vs. God",
            ExperimentId::Runner => "Endless Run",
            ExperimentId::Hundred => "A Hundred Marios",
            ExperimentId::Diorama => "Diorama",
            ExperimentId::Race => "Summit Race",
            ExperimentId::Shrink => "Shrink Ray",
            ExperimentId::Planet => "Tiny Planet",
            ExperimentId::Swarm => "Star Swarm",
            ExperimentId::Kart => "Mario Kart 64",
            ExperimentId::LastStar => "Last Star Standing",
            ExperimentId::Relic => "Relic Descent",
            ExperimentId::Gate => "Hold the Gate",
            ExperimentId::Deck => "Star Deck",
            ExperimentId::Halls => "The Painted Halls",
            ExperimentId::Brawl => "Gem Clash",
            ExperimentId::Party => "Sunstone Shindig",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            ExperimentId::Dimensions => "Flatten Bob-omb Battlefield into a 2D cross-section, or a top-down map.",
            ExperimentId::God => "Drag his platforms away mid-jump. Paint a bridge under him.",
            ExperimentId::Runner => "Mario never stops running. Jump, steer, survive as far as you can.",
            ExperimentId::Hundred => "Racers, coin hunters, followers, brawlers and explorers.",
            ExperimentId::Diorama => "The whole course in a plastic compact. Fly around, poke the cast.",
            ExperimentId::Race => "You against seven rivals to the top of Bob-omb Battlefield.",
            ExperimentId::Shrink => "Same Mario, any size: grass becomes a jungle, the mountain a step.",
            ExperimentId::Planet => "Run all the way around a world fifteen seconds wide.",
            ExperimentId::Swarm => "Survive the swarm. Your stars fire on their own; every level, pick an upgrade.",
            ExperimentId::Kart => "Drive Luigi Raceway, park your kart, and explore on foot as Mario.",
            ExperimentId::LastStar => "Drop in with 39 rivals, scavenge, and outlast the closing Blight. Last Mario standing wins.",
            ExperimentId::Relic => "Descend three dungeon floors, dive-dodge, ground-pound, loot gear and slay the Bomb King.",
            ExperimentId::Gate => "Build towers, fight beside them, and hold the castle gate through 15 waves.",
            ExperimentId::Deck => "A deckbuilder roguelike: chain Mario moves for momentum, pick a route, and topple the Fuse King.",
            ExperimentId::Brawl => "Arena brawler: grab anything, snatch the three gems, go giant and be last one standing. Versus bots or the Gauntlet.",
            ExperimentId::Party => "A four-player board party: roll, race to the Keeper for Sunstones, and win 15 mini-games.",
            ExperimentId::Halls => "A metroidvania in Mario's moves: find wall kicks, ground pound and the power caps, open the gates, free the Heart.",
        }
    }

    /// One short line for the mode card (ASCII only).
    pub fn tagline(self) -> &'static str {
        match self {
            ExperimentId::Dimensions => "Flatten the course into 2D or top-down",
            ExperimentId::God => "Drag his platforms away mid-jump",
            ExperimentId::Runner => "Never stop running. Go as far as you can",
            ExperimentId::Hundred => "A hundred Marios, five ways to play",
            ExperimentId::Diorama => "The course as a plastic toy. Poke the cast",
            ExperimentId::Race => "Race seven rivals up the mountain",
            ExperimentId::Shrink => "Any size you like. Grass becomes a jungle",
            ExperimentId::Planet => "Run around a world fifteen seconds wide",
            ExperimentId::Swarm => "Survive the swarm and pick upgrades",
            ExperimentId::Kart => "Luigi Raceway, then explore on foot",
            ExperimentId::LastStar => "Forty Marios drop in. One survives",
            ExperimentId::Relic => "Dungeon crawl: dive, pound, slay the Bomb King",
            ExperimentId::Gate => "Build towers and hold the castle gate",
            ExperimentId::Deck => "A deckbuilder roguelike of Mario moves",
            ExperimentId::Halls => "A metroidvania in Mario's moves",
            ExperimentId::Brawl => "Arena brawler: grab gems and go giant",
            ExperimentId::Party => "Four-player board party, 15 mini-games",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }
}

impl ExperimentId {
    /// The `experiments` module's id (same keys).
    pub fn theirs(self) -> Option<experiments::ExperimentId> {
        experiments::ExperimentId::parse(self.id())
    }

    /// Whether the experiments module has ported it yet.
    pub fn available(self) -> bool {
        self.theirs().is_some_and(|x| x.available())
    }
}

#[derive(Message, Clone, Debug)]
pub struct StartCourse(pub CourseRequest);
#[derive(Message, Clone, Copy, Debug)]
pub struct SwitchCharacter(pub Character);
#[derive(Message, Clone, Copy, Debug)]
pub struct OpenExperiment(pub ExperimentId);
#[derive(Message, Clone, Copy, Debug)]
pub struct LeaveExperiment;
#[derive(Message, Clone, Copy, Debug)]
pub struct ReturnToMenu;
#[derive(Message, Clone, Copy, Debug)]
pub struct GameOver;
#[derive(Message, Clone, Copy, Debug)]
pub struct SaveSlotSelected(pub u8);
#[derive(Message, Clone, Copy, Debug)]
pub struct ResetSaveRequested;

/// Star counts per save file for the slot screen (`None` = empty file); filled by the save plugin.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SaveSlotSummaries(pub [Option<u32>; 4]);

/// Pause toggles routed from navigation.
#[derive(Message, Clone, Copy, PartialEq, Eq, Debug)]
pub enum StateCmd {
    TogglePause,
    PadStart,
}

/// Which menu page is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    #[default]
    Title,
    Pause,
    Courses,
    Settings,
    Controls,
    Slots,
    GameOver,
}

/// The hub's tabs (title screen and pause menu).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    /// Game modes: a grid of cards.
    #[default]
    Modes,
    /// The course picker.
    Levels,
}

impl Tab {
    pub const ALL: [Tab; 2] = [Tab::Modes, Tab::Levels];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Modes => "MODES",
            Tab::Levels => "LEVELS",
        }
    }

    /// The tab `d` steps along (wraps).
    pub fn step(self, d: i32) -> Tab {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0) as i32;
        Self::ALL[(i + d).rem_euclid(Self::ALL.len() as i32) as usize]
    }

    pub fn from_name(name: &str) -> Option<Tab> {
        Self::ALL.into_iter().find(|t| t.label().eq_ignore_ascii_case(name))
    }
}

/// Which display setting a cycle row edits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayKey {
    Resolution,
    Aspect,
    Look,
    Motion,
    Scaling,
    /// Lighting quality tier (`settings::Lighting`), not part of `DisplaySettings`.
    Quality,
}

/// What activating a menu button does.
#[derive(Component, Clone, PartialEq, Eq, Debug)]
pub enum UiAction {
    Start,
    Resume,
    MainMenu,
    Quit,
    SelectCourse(&'static str),
    SelectAct(i32),
    Page(i32),
    Character(Character),
    Experiment(ExperimentId),
    Tab(Tab),
    LeaveExperiment,
    Open(Screen),
    Back,
    Cycle(DisplayKey, i32),
    Preset(bool),
    ToggleMute,
    ToggleShowFps,
    ToggleUncap,
    Slot(u8),
    ResetSave,
    /// Controls screen: wait for the next key / pad button for this action.
    Rebind(crate::bindings::Action),
    RebindPad(crate::bindings::Action),
    ResetBindings,
    /// Settings: re-open the BYOR setup window (relaunch with `--setup`).
    ImportRoms,
}

/// Menu UI state.
#[derive(Resource)]
pub struct ShellUi {
    pub screen: Screen,
    pub stack: Vec<Screen>,
    /// Course/act highlighted in the picker.
    pub course: String,
    pub act: i32,
    pub page: usize,
    /// Tab of the title / pause hub.
    pub tab: Tab,
    pub confirm_reset: bool,
    pub last_focus: Option<UiAction>,
    pub default_focus: Option<UiAction>,
    /// Rebuild the screen next frame.
    pub dirty: bool,
}

impl ShellUi {
    pub fn open(&mut self, screen: Screen) {
        self.stack.push(self.screen);
        self.screen = screen;
        self.dirty = true;
        self.last_focus = None;
    }

    pub fn show(&mut self, screen: Screen) {
        self.screen = screen;
        self.stack.clear();
        self.dirty = true;
        self.last_focus = None;
    }
}

/// Where a run starts: the title menu, or straight into a course (CLI course / headless hooks).
pub fn launch_mode(explicit_course: bool, hooks: bool, screen_env: Option<&str>) -> (AppState, Screen) {
    match screen_env {
        Some("title") | Some("menu") => (AppState::Menu, Screen::Title),
        Some("pause") => (AppState::Paused, Screen::Pause),
        Some("courses") => (AppState::Paused, Screen::Courses),
        Some("settings") => (AppState::Paused, Screen::Settings),
        Some("controls") => (AppState::Paused, Screen::Controls),
        Some("slots") => (AppState::Paused, Screen::Slots),
        Some("gameover") => (AppState::GameOver, Screen::GameOver),
        Some("play") => (AppState::Playing, Screen::Title),
        _ if explicit_course || hooks => (AppState::Playing, Screen::Title),
        _ => (AppState::Menu, Screen::Title),
    }
}

fn detect_launch() -> (AppState, Screen) {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let explicit = std::env::args().len() > 1 || env("JUMPMAN_COURSE").is_some();
    let hooks = ["JUMPMAN_EXIT_AFTER", "JUMPMAN_SCREENSHOT", "JUMPMAN_WALK"].iter().any(|k| env(k).is_some());
    launch_mode(explicit, hooks, env("JUMPMAN_SCREEN").as_deref())
}

pub struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        let settings = Settings::load(&settings::config_path());
        app.insert_resource(AppState::Boot)
            .insert_resource(ShellUi {
                screen: Screen::Title,
                stack: vec![],
                course: settings.course.clone(),
                act: settings.act,
                page: 0,
                tab: std::env::var("JUMPMAN_TAB").ok().and_then(|t| Tab::from_name(&t)).unwrap_or_default(),
                confirm_reset: false,
                last_focus: None,
                default_focus: Some(UiAction::Start),
                dirty: true,
            })
            .insert_resource(settings)
            .init_resource::<SaveSlotSummaries>()
            .init_resource::<nav::Focus>()
            .init_resource::<MenuLayout>()
            .init_resource::<scroll::ScrollMemory>()
            .init_resource::<scroll::PointerDrag>()
            .init_resource::<scroll::RevealFocus>()
            .init_resource::<Transition>()
            .add_message::<StartCourse>()
            .add_message::<SwitchCharacter>()
            .add_message::<OpenExperiment>()
            .add_message::<LeaveExperiment>()
            .add_message::<ReturnToMenu>()
            .add_message::<GameOver>()
            .add_message::<SaveSlotSelected>()
            .add_message::<ResetSaveRequested>()
            .add_message::<StateCmd>()
            .add_message::<nav::NavCmd>()
            .add_message::<nav::UiActionEvent>()
            .configure_sets(FixedUpdate, crate::SimSet.run_if(is_playing))
            .configure_sets(Update, crate::CameraSet.run_if(is_playing))
            .add_systems(Startup, (theme::load_ui_assets, boot).chain().in_set(crate::progress::PersistentSpawn))
            .add_systems(
                PreUpdate,
                // After `read_input`: swallow keys the menu used so Resume/Start never jumps Mario.
                (run_script, nav::gather_nav, scroll::track_pointer, nav::pointer, nav::apply_nav, screens::apply_actions, route_events, idle_input_guard).chain().after(crate::input::read_input),
            )
            .add_systems(
                Update,
                (
                    fit_menu_scale,
                    apply_safe_area,
                    sync_external,
                    apply_save_events,
                    screens::rebuild_screen,
                    nav::ensure_focus,
                    scroll::wheel_scroll,
                    scroll::touch_scroll,
                    scroll::scroll_to_focus,
                    scroll::remember,
                    theme::style_buttons,
                    screens::fit_backdrop,
                    step_transition,
                    update_fade,
                    apply_display,
                    persist_settings,
                )
                    .chain(),
            );
    }
}

/// Headless test hook: `JUMPMAN_SHELL_SCRIPT="start@2,pause@6,back@8"` injects menu commands at the
/// given times (seconds). Commands: `start`, `pause`, `back`, `menu`, `gameover`, `course:<id>`,
/// `open:<screen>`, `tab:<modes|levels>`, `xp:<mode key>`, `activate`, and `move:<up|down|left|right>`.
pub fn parse_script(text: &str) -> Vec<(f32, String)> {
    let mut out: Vec<_> = text.split(',').filter_map(|item| {
        let (cmd, at) = item.trim().split_once('@')?;
        Some((at.parse::<f32>().ok()?, cmd.to_string()))
    }).collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

fn run_script(
    time: Res<Time>,
    mut script: Local<Option<Vec<(f32, String)>>>,
    mut nav: MessageWriter<nav::NavCmd>,
    mut actions: MessageWriter<nav::UiActionEvent>,
    mut over: MessageWriter<GameOver>,
    mut keys: MessageWriter<bevy::input::keyboard::KeyboardInput>,
    windows: Query<Entity, With<Window>>,
) {
    let list = script.get_or_insert_with(|| std::env::var("JUMPMAN_SHELL_SCRIPT").map(|t| parse_script(&t)).unwrap_or_default());
    while list.first().is_some_and(|(at, _)| *at <= time.elapsed_secs()) {
        let (_, cmd) = list.remove(0);
        match cmd.as_str() {
            "start" => drop(actions.write(nav::UiActionEvent(UiAction::Start))),
            "pause" => drop(nav.write(nav::NavCmd::Pause)),
            "back" => drop(nav.write(nav::NavCmd::Back)),
            "menu" => drop(actions.write(nav::UiActionEvent(UiAction::MainMenu))),
            "gameover" => drop(over.write(GameOver)),
            "activate" => drop(nav.write(nav::NavCmd::Activate)),
            other => {
                // `key:Escape|KeyJ|...` injects a key press (the controls screen's rebind capture reads it);
                // `rebind:<action>` starts listening for that action's key.
                if let Some(name) = other.strip_prefix("key:") {
                    let code = match name {
                        "Escape" => KeyCode::Escape,
                        "Enter" => KeyCode::Enter,
                        "Space" => KeyCode::Space,
                        "KeyJ" => KeyCode::KeyJ,
                        "KeyK" => KeyCode::KeyK,
                        "KeyF" => KeyCode::KeyF,
                        _ => continue,
                    };
                    if let Ok(window) = windows.single() {
                        keys.write(bevy::input::keyboard::KeyboardInput {
                            key_code: code,
                            logical_key: bevy::input::keyboard::Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                            state: bevy::input::ButtonState::Pressed,
                            text: None,
                            repeat: false,
                            window,
                        });
                    }
                } else if let Some(a) = other.strip_prefix("rebind:") {
                    if let Some(action) = crate::bindings::Action::KEYBOARD.iter().find(|x| format!("{x:?}").to_lowercase() == a) {
                        actions.write(nav::UiActionEvent(UiAction::Rebind(*action)));
                    }
                } else if let Some(t) = other.strip_prefix("tab:").and_then(Tab::from_name) {
                    actions.write(nav::UiActionEvent(UiAction::Tab(t)));
                } else if let Some(e) = other.strip_prefix("xp:").and_then(ExperimentId::from_id) {
                    actions.write(nav::UiActionEvent(UiAction::Experiment(e)));
                } else if let Some(id) = other.strip_prefix("course:").and_then(courses::info) {
                    actions.write(nav::UiActionEvent(UiAction::SelectCourse(id.id)));
                } else if let Some(s) = other.strip_prefix("open:") {
                    let screen = match s { "settings" => Screen::Settings, "controls" => Screen::Controls, "slots" => Screen::Slots, "courses" => Screen::Courses, _ => continue };
                    actions.write(nav::UiActionEvent(UiAction::Open(screen)));
                } else if let Some(d) = other.strip_prefix("move:") {
                    let dir = match d { "up" => IVec2::new(0, -1), "down" => IVec2::new(0, 1), "left" => IVec2::new(-1, 0), _ => IVec2::new(1, 0) };
                    nav.write(nav::NavCmd::Move(dir));
                }
            }
        }
    }
}

fn boot(
    mut state: ResMut<AppState>,
    mut ui: ResMut<ShellUi>,
    mut commands: Commands,
    settings: Res<Settings>,
    fx: Option<ResMut<fxd::DisplaySettings>>,
    character: Option<ResMut<characters::Character>>,
    mut summaries: ResMut<SaveSlotSummaries>,
) {
    let (s, screen) = detect_launch();
    *state = s;
    ui.screen = screen;
    ui.dirty = true;
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        GlobalZIndex(1000),
        FadeOverlay,
        screens::ShellRoot,
    ));
    // Persisted preferences win unless an environment override picked something explicitly.
    let env_set = |keys: &[&str]| keys.iter().any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()));
    if let Some(mut fx) = fx {
        if !env_set(&["JUMPMAN_PRESET", "JUMPMAN_RES", "JUMPMAN_ASPECT", "JUMPMAN_LOOK", "JUMPMAN_MOTION", "JUMPMAN_SCALING"]) {
            settings.display.apply_to_fx(&mut fx);
        }
    }
    if let Some(mut character) = character {
        if !env_set(&["JUMPMAN_CHARACTER"]) {
            if let Some(c) = Character::from_id(&settings.character) {
                *character = c.theirs();
            }
        }
    }
    summaries.0 = read_slot_summaries(&progress::save::data_dir());
}

/// Star counts of the four save files (`None` = no file yet).
pub fn read_slot_summaries(dir: &std::path::Path) -> [Option<u32>; 4] {
    let mut out = [None; 4];
    for (i, slot) in out.iter_mut().enumerate() {
        let n = i as u32 + 1;
        if progress::save::SaveFile::path(dir, n).exists() {
            *slot = Some(progress::save::SaveFile::open(dir, n).stars());
        }
    }
    out
}

/// Keep the persisted settings and the other plugins' resources (display, character) in step:
/// edits made in the menus push out; changes made elsewhere (F6, V) are adopted.
fn sync_external(
    mut settings: ResMut<Settings>,
    fx: Option<ResMut<fxd::DisplaySettings>>,
    character: Option<Res<characters::Character>>,
    mut last_fx: Local<Option<fxd::DisplaySettings>>,
) {
    if let Some(mut fx) = fx {
        let outside_change = last_fx.is_some_and(|l| l != *fx);
        if outside_change {
            let adopted = settings::DisplaySettings::from_fx(&fx);
            if adopted != settings.display {
                settings.display = adopted;
            }
        } else if settings::DisplaySettings::from_fx(&fx) != settings.display {
            let mine = settings.display;
            mine.apply_to_fx(&mut fx);
        }
        *last_fx = Some(*fx);
    }
    if let Some(c) = character {
        let id = Character::from_theirs(*c).id();
        if settings.character != id {
            settings.character = id.to_string();
        }
    }
}

/// Save-file screen actions against `progress::save`.
fn apply_save_events(
    mut picked: MessageReader<SaveSlotSelected>,
    mut reset: MessageReader<ResetSaveRequested>,
    progress: Option<ResMut<Progress>>,
    mut sim: ResMut<GameSim>,
    mut summaries: ResMut<SaveSlotSummaries>,
    state: Res<AppState>,
    mut start: MessageWriter<StartCourse>,
) {
    let Some(mut progress) = progress else { return };
    let dir = progress::save::data_dir();
    for SaveSlotSelected(n) in picked.read() {
        let n = u32::from(*n);
        if progress.save.slot == n {
            continue;
        }
        progress::save::select_slot(&dir, n);
        progress.save = progress::save::SaveFile::open(&dir, n);
        sim.stars = progress.save.stars();
        // A different file starts a new run (the web build reloaded the page).
        if *state == AppState::Paused {
            start.write(StartCourse(sim.course.request.clone()));
        }
        summaries.0 = read_slot_summaries(&dir);
    }
    if reset.read().count() > 0 {
        progress.save.reset();
        sim.stars = 0;
        summaries.0 = read_slot_summaries(&dir);
    }
}

/// Make sure nothing the player pressed in a menu leaks into the game (and the camera/sim stay
/// quiet while no run is active).
fn idle_input_guard(state: Res<AppState>, mut input: ResMut<PlayerInput>, mut last: Local<Option<AppState>>) {
    if *state != AppState::Playing || *last != Some(AppState::Playing) {
        input.clear_edges();
    }
    *last = Some(*state);
}

/// Event handlers that change [`AppState`].
#[allow(clippy::too_many_arguments)]
fn route_events(
    mut state: ResMut<AppState>,
    mut ui: ResMut<ShellUi>,
    mut cmds: MessageReader<StateCmd>,
    mut returns: MessageReader<ReturnToMenu>,
    mut over: MessageReader<GameOver>,
    mut starts: MessageReader<StartCourse>,
    mut chars: MessageReader<SwitchCharacter>,
    mut experiments: MessageReader<OpenExperiment>,
    mut leave: MessageReader<LeaveExperiment>,
    mut slot: Option<ResMut<ExperimentSlot>>,
    mut character: Option<ResMut<characters::Character>>,
    mut transition: ResMut<Transition>,
    mut toast: ResMut<Toast>,
    mut actions: MessageWriter<nav::UiActionEvent>,
) {
    for cmd in cmds.read() {
        match (*cmd, *state) {
            (StateCmd::TogglePause | StateCmd::PadStart, AppState::Playing) => {
                *state = AppState::Paused;
                ui.show(Screen::Pause);
            }
            (StateCmd::TogglePause | StateCmd::PadStart, AppState::Paused) => {
                if ui.screen == Screen::Pause {
                    *state = AppState::Playing;
                } else {
                    actions.write(nav::UiActionEvent(UiAction::Back));
                }
            }
            (StateCmd::PadStart, AppState::Menu) => {
                actions.write(nav::UiActionEvent(if matches!(ui.screen, Screen::Title | Screen::Courses) { UiAction::Start } else { UiAction::Back }));
            }
            (StateCmd::PadStart, AppState::GameOver) => {
                actions.write(nav::UiActionEvent(UiAction::MainMenu));
            }
            _ => {}
        }
    }
    for _ in returns.read() {
        if let Some(slot) = slot.as_mut() {
            slot.exit |= slot.active.is_some();
        }
        *state = AppState::Menu;
        ui.show(Screen::Title);
    }
    for _ in leave.read() {
        // LEAVE EXPERIMENT hands the player back a clean course, not the experiment's leftovers.
        if let Some(slot) = slot.as_mut() {
            if slot.active.is_some() {
                slot.exit = true;
                slot.restart = true;
            }
        }
    }
    if over.read().count() > 0 {
        *state = AppState::GameOver;
        ui.show(Screen::GameOver);
    }
    for StartCourse(request) in starts.read() {
        // Choosing a course ends whatever experiment is running before the world is rebuilt.
        if let Some(slot) = slot.as_mut() {
            slot.exit |= slot.active.is_some();
        }
        let fallback = if *state == AppState::Transition { transition.fallback } else { *state };
        transition.begin(request.clone(), if fallback == AppState::Boot { AppState::Menu } else { fallback });
        *state = AppState::Transition;
    }
    for SwitchCharacter(c) in chars.read() {
        if let Some(character) = character.as_mut() {
            **character = c.theirs();
        }
        if *state != AppState::Menu {
            toast.show(format!("PLAYING AS {}", c.label().to_uppercase()), 1.4);
        }
    }
    for OpenExperiment(id) in experiments.read() {
        let Some(slot) = slot.as_mut() else { continue };
        match id.theirs().filter(|x| x.available()) {
            // Always a fresh run: the experiments module exits the running mode, reloads the course
            // from scratch through `StartCourse` (the transition sets the state) and then enters.
            Some(x) => slot.request = Some(x),
            None => toast.show(format!("{} IS NOT PORTED YET", id.title().to_uppercase()), 2.0),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Course transition: fade out -> queue the switch in `progress` -> wait -> fade in.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    #[default]
    Idle,
    Out,
    /// Fully black: queue the switch.
    Load,
    /// Waiting for `progress` to perform it.
    Wait,
    In,
}

#[derive(Resource)]
pub struct Transition {
    pub phase: Phase,
    pub t: f32,
    pub pending: Option<CourseRequest>,
    /// State to return to if loading fails.
    pub fallback: AppState,
}

impl Default for Transition {
    fn default() -> Self {
        Self { phase: Phase::Idle, t: 0.0, pending: None, fallback: AppState::Menu }
    }
}

impl Transition {
    pub fn begin(&mut self, request: CourseRequest, fallback: AppState) {
        self.pending = Some(request);
        self.fallback = fallback;
        if matches!(self.phase, Phase::Idle | Phase::In) {
            self.phase = Phase::Out;
            self.t = 0.0;
        }
    }

    /// Fade alpha for the current phase.
    pub fn alpha(&self) -> f32 {
        match self.phase {
            Phase::Idle => 0.0,
            Phase::Out => self.t,
            Phase::Load | Phase::Wait => 1.0,
            Phase::In => 1.0 - self.t,
        }
    }
}

const FADE_OUT: f32 = 0.25;
const FADE_IN: f32 = 0.2;

#[derive(Component)]
pub struct FadeOverlay;

fn update_fade(transition: Res<Transition>, mut overlay: Query<&mut BackgroundColor, With<FadeOverlay>>) {
    for mut c in &mut overlay {
        let a = transition.alpha();
        if (c.0.alpha() - a).abs() > 1e-3 {
            c.0 = Color::srgba(0.0, 0.0, 0.0, a);
        }
    }
}

fn step_transition(mut transition: ResMut<Transition>, mut state: ResMut<AppState>, time: Res<Time<Real>>, progress: Option<ResMut<Progress>>, mut toast: ResMut<Toast>) {
    let dt = time.delta_secs().min(0.1);
    match transition.phase {
        Phase::Idle => {}
        Phase::Out => {
            transition.t += dt / FADE_OUT;
            if transition.t >= 1.0 {
                transition.t = 1.0;
                transition.phase = Phase::Load;
            }
        }
        // One frame fully black first, then queue the load.
        Phase::Load => {
            let (Some(request), Some(mut progress)) = (transition.pending.take(), progress) else {
                // Nothing to load (or no progress plugin): give up and return.
                *state = transition.fallback;
                transition.phase = Phase::In;
                transition.t = 0.0;
                return;
            };
            progress.pending = Some(Switch { request, carry: None, reveal: true, completed: None, fresh_run: true });
            transition.phase = Phase::Wait;
            transition.t = 0.0;
        }
        Phase::Wait => {
            transition.t += dt;
            let done = progress.is_none_or(|p| p.pending.is_none());
            if done || transition.t > 8.0 {
                // `progress` toasts "COURSE COULD NOT LOAD" itself on failure.
                if !done {
                    toast.show("COURSE UNAVAILABLE", 2.5);
                }
                *state = AppState::Playing;
                transition.phase = Phase::In;
                transition.t = 0.0;
            }
        }
        Phase::In => {
            transition.t += dt / FADE_IN;
            if transition.t >= 1.0 {
                transition.phase = Phase::Idle;
                transition.t = 0.0;
            }
        }
    }
}

/// Apply the part of the display settings the shell owns: the 4:3 letterbox (`aspect`). `look`
/// feeds `fx`; resolution/motion/scaling are published in `fx::display::DisplaySettings`.
fn apply_display(
    display: Option<Res<fxd::DisplaySettings>>,
    state: Res<AppState>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<Camera3d>>,
    mut last: Local<Option<(Aspect, bool, UVec2)>>,
) {
    let Some(display) = display else { return };
    let Ok(window) = windows.single() else { return };
    let aspect = settings::DisplaySettings::from_fx(&display).aspect;
    let letterbox = aspect == Aspect::Classic && matches!(*state, AppState::Playing | AppState::Paused | AppState::Transition);
    let size = window.physical_size();
    let key = (aspect, letterbox, size);
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    for mut camera in &mut cameras {
        camera.viewport = letterbox.then(|| {
            let (w, h) = (size.x as f32, size.y as f32);
            let (vw, vh) = if w / h > 4.0 / 3.0 { (h * 4.0 / 3.0, h) } else { (w, w * 3.0 / 4.0) };
            bevy::camera::Viewport { physical_position: UVec2::new(((w - vw) / 2.0) as u32, ((h - vh) / 2.0) as u32), physical_size: UVec2::new(vw as u32, vh as u32), ..default() }
        });
    }
}

fn persist_settings(settings: Res<Settings>) {
    if settings.is_changed() && !settings.is_added() {
        if let Err(e) = settings.save(&settings::config_path()) {
            warn!("could not save settings: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_modes() {
        assert_eq!(launch_mode(false, false, None), (AppState::Menu, Screen::Title));
        assert_eq!(launch_mode(true, false, None), (AppState::Playing, Screen::Title));
        assert_eq!(launch_mode(false, true, None).0, AppState::Playing);
        assert_eq!(launch_mode(false, true, Some("title")), (AppState::Menu, Screen::Title));
        assert_eq!(launch_mode(false, false, Some("pause")), (AppState::Paused, Screen::Pause));
        assert_eq!(launch_mode(false, false, Some("gameover")).0, AppState::GameOver);
    }

    #[test]
    fn catalogues_round_trip() {
        for c in Character::ALL {
            assert_eq!(Character::from_id(c.id()), Some(c));
        }
        for e in ExperimentId::ALL {
            assert_eq!(ExperimentId::from_id(e.id()), Some(e));
        }
        assert!(Character::from_id("wario").is_none());
    }

    #[test]
    fn transition_alpha_runs_black_then_clear() {
        let mut t = Transition::default();
        assert_eq!(t.alpha(), 0.0);
        t.begin(CourseRequest::new("bob"), AppState::Menu);
        assert_eq!(t.phase, Phase::Out);
        t.t = 0.5;
        assert_eq!(t.alpha(), 0.5);
        t.phase = Phase::Load;
        assert_eq!(t.alpha(), 1.0);
        t.phase = Phase::In;
        t.t = 0.25;
        assert_eq!(t.alpha(), 0.75);
        // A second request mid-fade replaces the pending course without restarting the fade.
        t.phase = Phase::Out;
        t.t = 0.4;
        t.begin(CourseRequest::new("wf"), AppState::Playing);
        assert_eq!(t.t, 0.4);
        assert_eq!(t.pending.as_ref().unwrap().id, "wf");
    }

    #[test]
    fn scripts_parse_sorted() {
        let s = parse_script("pause@6, start@2,bad,x@y");
        assert_eq!(s, vec![(2.0, "start".to_string()), (6.0, "pause".to_string())]);
    }

    /// The pause toggle, return-to-menu, game-over and character routing without a window.
    #[test]
    fn state_routing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(AppState::Playing)
            .insert_resource(ShellUi { screen: Screen::Title, stack: vec![], course: "bob".into(), act: 1, page: 0, tab: Tab::Modes, confirm_reset: false, last_focus: None, default_focus: None, dirty: false })
            .insert_resource(Settings::default())
            .insert_resource(characters::Character::Mario)
            .init_resource::<Transition>()
            .init_resource::<Toast>()
            .add_message::<StateCmd>()
            .add_message::<ReturnToMenu>()
            .add_message::<GameOver>()
            .add_message::<StartCourse>()
            .add_message::<SwitchCharacter>()
            .add_message::<OpenExperiment>()
            .add_message::<LeaveExperiment>()
            .add_message::<nav::UiActionEvent>()
            .add_systems(Update, route_events);
        let state = |app: &App| *app.world().resource::<AppState>();
        app.world_mut().write_message(StateCmd::TogglePause);
        app.update();
        assert_eq!(state(&app), AppState::Paused);
        assert_eq!(app.world().resource::<ShellUi>().screen, Screen::Pause);
        app.world_mut().write_message(StateCmd::TogglePause);
        app.update();
        assert_eq!(state(&app), AppState::Playing);
        app.world_mut().write_message(ReturnToMenu);
        app.update();
        assert_eq!(state(&app), AppState::Menu);
        // Starting a course enters the fade; failing back returns to the menu.
        app.world_mut().write_message(StartCourse(CourseRequest::new("bob")));
        app.update();
        assert_eq!(state(&app), AppState::Transition);
        assert_eq!(app.world().resource::<Transition>().fallback, AppState::Menu);
        *app.world_mut().resource_mut::<AppState>() = AppState::Playing;
        app.world_mut().write_message(GameOver);
        app.update();
        assert_eq!(state(&app), AppState::GameOver);
        // Character swaps write the characters module's resource.
        app.world_mut().write_message(SwitchCharacter(Character::Banjo));
        app.update();
        assert_eq!(*app.world().resource::<characters::Character>(), characters::Character::Banjo);
    }

    #[test]
    fn character_and_experiment_ids_map_to_the_other_modules() {
        for c in Character::ALL {
            assert_eq!(Character::from_theirs(c.theirs()), c);
            assert_eq!(characters::Character::from_name(c.id()), Some(c.theirs()));
        }
        for e in ExperimentId::ALL {
            assert!(e.theirs().is_some(), "{}", e.id());
        }
        assert!(ExperimentId::Shrink.available());
        assert!(ExperimentId::Swarm.available());
        assert!(ExperimentId::LastStar.available());
        assert_eq!(ExperimentId::from_id("laststar"), Some(ExperimentId::LastStar));
        assert_eq!(ExperimentId::LastStar.title(), "Last Star Standing");
        assert_eq!(ExperimentId::from_id("swarm"), Some(ExperimentId::Swarm));
        assert_eq!(ExperimentId::Swarm.title(), "Star Swarm");
        assert!(ExperimentId::Relic.available());
        assert_eq!(ExperimentId::from_id("relic"), Some(ExperimentId::Relic));
        assert_eq!(ExperimentId::Relic.title(), "Relic Descent");
        assert!(ExperimentId::Gate.available());
        assert_eq!(ExperimentId::from_id("gate"), Some(ExperimentId::Gate));
        assert_eq!(ExperimentId::Gate.title(), "Hold the Gate");
        assert!(ExperimentId::Brawl.available());
        assert_eq!(ExperimentId::from_id("brawl"), Some(ExperimentId::Brawl));
        assert!(ExperimentId::Deck.available());
        assert_eq!(ExperimentId::from_id("stardeck"), Some(ExperimentId::Deck));
        assert_eq!(ExperimentId::Deck.title(), "Star Deck");
    }

    #[test]
    fn app_state_menu_membership() {
        assert!(AppState::Paused.in_menu());
        assert!(!AppState::Playing.in_menu());
        assert!(!AppState::Transition.in_menu());
    }

    #[test]
    fn phones_get_the_compact_layout_near_native_scale() {
        use crate::safe_area::SafeArea;
        let notch = SafeArea { left: 59.0, right: 59.0, bottom: 21.0, top: 0.0 };
        // iPhone 16 Pro landscape.
        let phone = menu_layout(Vec2::new(874.0, 402.0), &notch);
        assert!(phone.compact);
        assert!((0.95..=1.1).contains(&phone.scale), "{phone:?}");
        // A 44 px button stays >= 44 pt after scaling, 12 px text stays legible.
        let m = screens::Metrics::new(&phone);
        assert!(m.tap * phone.scale >= 43.5);
        // Smaller iPhones (SE) still get a sane scale.
        let se = menu_layout(Vec2::new(667.0, 375.0), &SafeArea::default());
        assert!(se.compact && se.scale >= 0.85 && se.scale <= 1.0, "{se:?}");
        // Desktop windows keep the roomy layout.
        let desk = menu_layout(Vec2::new(1040.0, 680.0), &SafeArea::default());
        assert!(!desk.compact);
        assert_eq!(desk.scale, 1.0);
        let small_desk = menu_layout(Vec2::new(960.0, 560.0), &SafeArea::default());
        assert!(!small_desk.compact && small_desk.scale < 1.0);
    }

    #[test]
    fn mode_grid_columns_follow_the_width() {
        use crate::safe_area::SafeArea;
        let phone = screens::Metrics::new(&menu_layout(Vec2::new(874.0, 402.0), &SafeArea { left: 59.0, right: 59.0, bottom: 21.0, top: 0.0 }));
        assert_eq!(phone.card_columns(), 2);
        let desk = screens::Metrics::new(&menu_layout(Vec2::new(1040.0, 680.0), &SafeArea::default()));
        assert_eq!(desk.card_columns(), 3);
        let portrait = screens::Metrics::new(&menu_layout(Vec2::new(402.0, 874.0), &SafeArea::default()));
        assert_eq!(portrait.card_columns(), 1);
    }

    #[test]
    fn tabs_step_and_parse() {
        assert_eq!(Tab::Modes.step(1), Tab::Levels);
        assert_eq!(Tab::Levels.step(1), Tab::Modes);
        assert_eq!(Tab::Modes.step(-1), Tab::Levels);
        assert_eq!(Tab::from_name("levels"), Some(Tab::Levels));
        assert_eq!(Tab::from_name("nope"), None);
    }

    #[test]
    fn every_mode_has_a_one_line_ascii_tagline() {
        for e in ExperimentId::ALL {
            let t = e.tagline();
            assert!(!t.is_empty() && t.is_ascii() && t.len() <= 52, "{}: {t}", e.id());
        }
    }

    fn experiment_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(AppState::Paused)
            .insert_resource(ShellUi { screen: Screen::Pause, stack: vec![], course: "bob".into(), act: 1, page: 0, tab: Tab::Modes, confirm_reset: false, last_focus: None, default_focus: None, dirty: false })
            .insert_resource(characters::Character::Mario)
            .insert_resource(ExperimentSlot::default())
            .init_resource::<Transition>()
            .init_resource::<Toast>()
            .add_message::<StateCmd>()
            .add_message::<ReturnToMenu>()
            .add_message::<GameOver>()
            .add_message::<StartCourse>()
            .add_message::<SwitchCharacter>()
            .add_message::<OpenExperiment>()
            .add_message::<LeaveExperiment>()
            .add_message::<nav::UiActionEvent>()
            .add_systems(Update, route_events);
        app
    }

    /// Picking a mode never enters it directly: it files a fresh-start request (and keeps the state
    /// machine out of the way) so `experiments::process_requests` can exit, reload and then enter.
    #[test]
    fn opening_a_mode_files_a_fresh_start_request() {
        let mut app = experiment_app();
        app.world_mut().write_message(OpenExperiment(ExperimentId::Race));
        app.update();
        let slot = app.world().resource::<ExperimentSlot>();
        assert_eq!(slot.request, Some(experiments::ExperimentId::Race));
        assert!(slot.pending.is_none(), "no direct entry on the old world");
        assert_eq!(*app.world().resource::<AppState>(), AppState::Paused);
    }

    #[test]
    fn starting_a_course_or_leaving_ends_the_running_experiment() {
        let mut app = experiment_app();
        app.world_mut().resource_mut::<ExperimentSlot>().active = experiments::create(experiments::ExperimentId::Shrink, "bob");
        app.world_mut().write_message(StartCourse(CourseRequest::new("wf")));
        app.update();
        assert!(app.world().resource::<ExperimentSlot>().exit, "choosing a course exits the experiment first");
        assert!(!app.world().resource::<ExperimentSlot>().restart);
        // LEAVE EXPERIMENT also reloads the course so nothing the mode did is left behind.
        {
            let mut slot = app.world_mut().resource_mut::<ExperimentSlot>();
            slot.exit = false;
        }
        app.world_mut().write_message(LeaveExperiment);
        app.update();
        let slot = app.world().resource::<ExperimentSlot>();
        assert!(slot.exit && slot.restart);
    }
}
