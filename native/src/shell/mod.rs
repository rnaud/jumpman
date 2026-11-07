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
//! # Events other plugins use
//! * [`StartCourse`] - load a course (fade out, rebuild the world, fade in). The menu emits it; warps,
//!   doors and paintings should too. On success [`CourseLoaded`] is sent.
//! * [`SwitchCharacter`] - the player picked a character (title or pause menu).
//! * [`OpenExperiment`] / [`LeaveExperiment`] - start / stop an experiment from the menus.
//! * [`ReturnToMenu`] - back to the title screen (clean up experiments here).
//! * [`GameOver`] - emit when the last life is lost; shows the GAME OVER screen.
//! * [`SaveSlotSelected`] / [`ResetSaveRequested`] - save-file UI; publish counts in [`SaveSlotSummaries`].
//!
//! Course switching despawns every root entity except windows, cameras, the shell's own UI, and
//! anything tagged [`Persistent`], re-runs the world/Mario/HUD spawn systems and resets the object and
//! pickup views. Plugins that keep their own world entities should listen to [`CourseLoaded`].
#![allow(dead_code)] // public surface for other plugins (events, catalogues)
pub mod nav;
pub mod screens;
pub mod settings;
pub mod theme;

use crate::camera::CameraRig;
use crate::game::courses;
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use crate::hud::Toast;
use crate::input::PlayerInput;
use crate::render::AssetRoot;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use settings::{Aspect, DisplaySettings, Settings};

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

pub fn is_playing(state: Res<AppState>) -> bool {
    *state == AppState::Playing
}

/// Marks root entities that survive course switches (see the module docs).
#[derive(Component)]
pub struct Persistent;

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
}

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct ActiveCharacter(pub Character);

/// The eight experiments of the menu (`data-experiment`).
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
}

impl ExperimentId {
    pub const ALL: [ExperimentId; 8] = [
        ExperimentId::Dimensions,
        ExperimentId::God,
        ExperimentId::Runner,
        ExperimentId::Hundred,
        ExperimentId::Diorama,
        ExperimentId::Race,
        ExperimentId::Shrink,
        ExperimentId::Planet,
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
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }
}

/// The experiment currently running, if any (set by the shell on [`OpenExperiment`]; the
/// experiments plugin clears it by sending [`LeaveExperiment`] or [`ReturnToMenu`]).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct ActiveExperiment(pub Option<ExperimentId>);

#[derive(Event, Clone, Debug)]
pub struct StartCourse(pub CourseRequest);
#[derive(Event, Clone, Copy, Debug)]
pub struct SwitchCharacter(pub Character);
#[derive(Event, Clone, Copy, Debug)]
pub struct OpenExperiment(pub ExperimentId);
#[derive(Event, Clone, Copy, Debug)]
pub struct LeaveExperiment;
#[derive(Event, Clone, Copy, Debug)]
pub struct ReturnToMenu;
#[derive(Event, Clone, Copy, Debug)]
pub struct GameOver;
/// A course finished loading (after a [`StartCourse`]); the `GameSim` resource is the new one.
#[derive(Event, Clone, Copy, Debug)]
pub struct CourseLoaded;
#[derive(Event, Clone, Copy, Debug)]
pub struct SaveSlotSelected(pub u8);
#[derive(Event, Clone, Copy, Debug)]
pub struct ResetSaveRequested;

/// Star counts per save file for the slot screen (`None` = empty file); filled by the save plugin.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SaveSlotSummaries(pub [Option<u32>; 4]);

/// Pause toggles routed from navigation.
#[derive(Event, Clone, Copy, PartialEq, Eq, Debug)]
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

/// Which display setting a cycle row edits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayKey {
    Resolution,
    Aspect,
    Look,
    Motion,
    Scaling,
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
    LeaveExperiment,
    Open(Screen),
    Back,
    Cycle(DisplayKey, i32),
    Preset(bool),
    ToggleMute,
    Slot(u8),
    ResetSave,
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
        let character = Character::from_id(&settings.character).unwrap_or(Character::Mario);
        app.insert_resource(AppState::Boot)
            .insert_resource(settings.display)
            .insert_resource(ActiveCharacter(character))
            .insert_resource(ShellUi {
                screen: Screen::Title,
                stack: vec![],
                course: settings.course.clone(),
                act: settings.act,
                page: 0,
                confirm_reset: false,
                last_focus: None,
                default_focus: Some(UiAction::Start),
                dirty: true,
            })
            .insert_resource(settings)
            .init_resource::<ActiveExperiment>()
            .init_resource::<SaveSlotSummaries>()
            .init_resource::<nav::Focus>()
            .init_resource::<Transition>()
            .add_event::<StartCourse>()
            .add_event::<SwitchCharacter>()
            .add_event::<OpenExperiment>()
            .add_event::<LeaveExperiment>()
            .add_event::<ReturnToMenu>()
            .add_event::<GameOver>()
            .add_event::<CourseLoaded>()
            .add_event::<SaveSlotSelected>()
            .add_event::<ResetSaveRequested>()
            .add_event::<StateCmd>()
            .add_event::<nav::NavCmd>()
            .add_event::<nav::UiActionEvent>()
            .configure_sets(FixedUpdate, crate::SimSet.run_if(is_playing))
            .configure_sets(Update, crate::CameraSet.run_if(is_playing))
            .add_systems(Startup, (theme::load_ui_assets, boot).chain())
            .add_systems(
                PreUpdate,
                // After `read_input`: swallow keys the menu used so Resume/Start never jumps Mario.
                (run_script, nav::gather_nav, nav::pointer, nav::apply_nav, screens::apply_actions, route_events, idle_input_guard).chain().after(crate::input::read_input),
            )
            .add_systems(
                Update,
                (
                    screens::rebuild_screen,
                    nav::ensure_focus,
                    theme::style_buttons,
                    screens::update_caption,
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
/// `open:<screen>`, `activate`, and `move:<up|down|left|right>`.
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
    mut nav: EventWriter<nav::NavCmd>,
    mut actions: EventWriter<nav::UiActionEvent>,
    mut over: EventWriter<GameOver>,
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
                if let Some(id) = other.strip_prefix("course:").and_then(courses::info) {
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

fn boot(mut state: ResMut<AppState>, mut ui: ResMut<ShellUi>, mut commands: Commands, mut toast: ResMut<Toast>) {
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
    if s == AppState::Playing {
        toast.show("GO!  A = JUMP - HOLD FOR HEIGHT", 2.6);
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
    mut cmds: EventReader<StateCmd>,
    mut returns: EventReader<ReturnToMenu>,
    mut over: EventReader<GameOver>,
    mut starts: EventReader<StartCourse>,
    mut chars: EventReader<SwitchCharacter>,
    mut experiments: EventReader<OpenExperiment>,
    mut leave: EventReader<LeaveExperiment>,
    mut active: ResMut<ActiveExperiment>,
    mut character: ResMut<ActiveCharacter>,
    mut settings: ResMut<Settings>,
    mut transition: ResMut<Transition>,
    mut toast: ResMut<Toast>,
    mut actions: EventWriter<nav::UiActionEvent>,
) {
    for cmd in cmds.read() {
        match (*cmd, *state) {
            (StateCmd::TogglePause, AppState::Playing) | (StateCmd::PadStart, AppState::Playing) => {
                *state = AppState::Paused;
                ui.show(Screen::Pause);
            }
            (StateCmd::TogglePause, AppState::Paused) | (StateCmd::PadStart, AppState::Paused) => {
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
        *active = ActiveExperiment(None);
        *state = AppState::Menu;
        ui.show(Screen::Title);
    }
    for _ in leave.read() {
        *active = ActiveExperiment(None);
    }
    if over.read().count() > 0 {
        *state = AppState::GameOver;
        ui.show(Screen::GameOver);
    }
    for StartCourse(request) in starts.read() {
        let fallback = if *state == AppState::Transition { transition.fallback } else { *state };
        transition.begin(request.clone(), if fallback == AppState::Boot { AppState::Menu } else { fallback });
        *state = AppState::Transition;
    }
    for SwitchCharacter(c) in chars.read() {
        character.0 = *c;
        if settings.character != c.id() {
            settings.character = c.id().to_string();
        }
        if *state != AppState::Menu {
            toast.show(format!("PLAYING AS {}", c.label().to_uppercase()), 1.4);
        }
    }
    for OpenExperiment(id) in experiments.read() {
        *active = ActiveExperiment(Some(*id));
        *state = AppState::Playing;
        toast.show(id.title().to_uppercase(), 1.8);
    }
}

// ---------------------------------------------------------------------------------------------
// Course transition: fade out -> rebuild the world -> fade in.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    #[default]
    Idle,
    Out,
    Load,
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
        if self.phase == Phase::Idle || self.phase == Phase::In {
            self.phase = Phase::Out;
            self.t = 0.0;
        }
    }

    /// Fade alpha for the current phase.
    pub fn alpha(&self) -> f32 {
        match self.phase {
            Phase::Idle => 0.0,
            Phase::Out => self.t,
            Phase::Load => 1.0,
            Phase::In => 1.0 - self.t,
        }
    }
}

const FADE_OUT: f32 = 0.28;
const FADE_IN: f32 = 0.35;

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

fn step_transition(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs().min(0.1);
    let phase = world.resource::<Transition>().phase;
    match phase {
        Phase::Idle => {}
        Phase::Out => {
            let mut t = world.resource_mut::<Transition>();
            t.t += dt / FADE_OUT;
            if t.t >= 1.0 {
                t.t = 1.0;
                t.phase = Phase::Load;
            }
        }
        // One frame fully black first, then the (blocking) load.
        Phase::Load => {
            let Some(request) = world.resource_mut::<Transition>().pending.take() else {
                world.resource_mut::<Transition>().phase = Phase::In;
                return;
            };
            let root = world.resource::<AssetRoot>().0.clone();
            match GameSim::load(&root, request.clone()) {
                Ok(sim) => {
                    install_course(world, sim);
                    world.send_event(CourseLoaded);
                    let mut settings = world.resource_mut::<Settings>();
                    if courses::info(&request.id).is_some() && (settings.course != request.id || settings.act != request.act) {
                        settings.course = request.id.clone();
                        settings.act = request.act;
                    }
                    *world.resource_mut::<AppState>() = AppState::Transition;
                }
                Err(e) => {
                    error!("could not load course '{}': {e}", request.id);
                    world.resource_mut::<Toast>().show("COURSE UNAVAILABLE", 2.5);
                    let fallback = world.resource::<Transition>().fallback;
                    *world.resource_mut::<AppState>() = fallback;
                    let mut t = world.resource_mut::<Transition>();
                    t.phase = Phase::In;
                    t.t = 0.0;
                    return;
                }
            }
            let mut t = world.resource_mut::<Transition>();
            t.phase = Phase::In;
            t.t = 0.0;
            // Enter the run behind the fading-in black.
            *world.resource_mut::<AppState>() = AppState::Playing;
            world.resource_mut::<Toast>().show("GO!  A = JUMP - HOLD FOR HEIGHT", 2.6);
        }
        Phase::In => {
            let mut t = world.resource_mut::<Transition>();
            t.t += dt / FADE_IN;
            if t.t >= 1.0 {
                t.phase = Phase::Idle;
                t.t = 0.0;
            }
        }
    }
}

/// Swap in a freshly loaded [`GameSim`] and rebuild everything that was spawned from the old one.
fn install_course(world: &mut World, sim: GameSim) {
    let yaw = sim.mario.face_angle[1];
    let stale: Vec<Entity> = world
        .query_filtered::<Entity, (Without<ChildOf>, Without<Window>, Without<Camera>, Without<screens::ShellRoot>, Without<Persistent>, Without<bevy::ecs::observer::Observer>, Without<bevy::render::view::screenshot::Screenshot>)>()
        .iter(world)
        .collect();
    for e in stale {
        if let Ok(e) = world.get_entity_mut(e) {
            e.despawn();
        }
    }
    world.insert_resource(sim);
    crate::render::reset_views(world);
    let _ = world.run_system_once(crate::render::course_view::spawn_course);
    let _ = world.run_system_once(crate::render::mario_view::spawn_mario);
    let _ = world.run_system_once(crate::hud::spawn_hud);
    world.resource_mut::<CameraRig>().snap_behind(yaw);
    world.resource_mut::<PlayerInput>().clear_edges();
}

/// Apply the part of the display settings the shell owns: the 4:3 letterbox (`aspect`). The other
/// fields are published in [`DisplaySettings`] for the renderer.
fn apply_display(
    settings: Res<Settings>,
    mut display: ResMut<DisplaySettings>,
    state: Res<AppState>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<Camera3d>>,
    mut last: Local<Option<(Aspect, bool, UVec2)>>,
) {
    if *display != settings.display {
        *display = settings.display;
    }
    let Ok(window) = windows.single() else { return };
    let letterbox = settings.display.aspect == Aspect::Classic && matches!(*state, AppState::Playing | AppState::Paused | AppState::Transition);
    let size = window.physical_size();
    let key = (settings.display.aspect, letterbox, size);
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    for mut camera in &mut cameras {
        camera.viewport = letterbox.then(|| {
            let (w, h) = (size.x as f32, size.y as f32);
            let (vw, vh) = if w / h > 4.0 / 3.0 { (h * 4.0 / 3.0, h) } else { (w, w * 3.0 / 4.0) };
            bevy::render::camera::Viewport { physical_position: UVec2::new(((w - vw) / 2.0) as u32, ((h - vh) / 2.0) as u32), physical_size: UVec2::new(vw as u32, vh as u32), ..default() }
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

    /// The pause toggle, return-to-menu and game-over routing without a window.
    #[test]
    fn state_routing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(AppState::Playing)
            .insert_resource(ShellUi { screen: Screen::Title, stack: vec![], course: "bob".into(), act: 1, page: 0, confirm_reset: false, last_focus: None, default_focus: None, dirty: false })
            .insert_resource(Settings::default())
            .insert_resource(ActiveCharacter(Character::Mario))
            .init_resource::<ActiveExperiment>()
            .init_resource::<Transition>()
            .init_resource::<Toast>()
            .add_event::<StateCmd>()
            .add_event::<ReturnToMenu>()
            .add_event::<GameOver>()
            .add_event::<StartCourse>()
            .add_event::<SwitchCharacter>()
            .add_event::<OpenExperiment>()
            .add_event::<LeaveExperiment>()
            .add_event::<nav::UiActionEvent>()
            .add_systems(Update, route_events);
        let state = |app: &App| *app.world().resource::<AppState>();
        app.world_mut().send_event(StateCmd::TogglePause);
        app.update();
        assert_eq!(state(&app), AppState::Paused);
        assert_eq!(app.world().resource::<ShellUi>().screen, Screen::Pause);
        app.world_mut().send_event(StateCmd::TogglePause);
        app.update();
        assert_eq!(state(&app), AppState::Playing);
        // Experiments start a run and are remembered until they leave.
        app.world_mut().send_event(OpenExperiment(ExperimentId::Runner));
        app.update();
        assert_eq!(app.world().resource::<ActiveExperiment>().0, Some(ExperimentId::Runner));
        app.world_mut().send_event(ReturnToMenu);
        app.update();
        assert_eq!(state(&app), AppState::Menu);
        assert_eq!(app.world().resource::<ActiveExperiment>().0, None);
        // Starting a course enters the fade; failing back returns to the menu.
        app.world_mut().send_event(StartCourse(CourseRequest::new("bob")));
        app.update();
        assert_eq!(state(&app), AppState::Transition);
        assert_eq!(app.world().resource::<Transition>().fallback, AppState::Menu);
        *app.world_mut().resource_mut::<AppState>() = AppState::Playing;
        app.world_mut().send_event(GameOver);
        app.update();
        assert_eq!(state(&app), AppState::GameOver);
        // Character swaps update the resource and persisted setting.
        app.world_mut().send_event(SwitchCharacter(Character::Banjo));
        app.update();
        assert_eq!(app.world().resource::<ActiveCharacter>().0, Character::Banjo);
        assert_eq!(app.world().resource::<Settings>().character, "banjo");
    }

    #[test]
    fn app_state_menu_membership() {
        assert!(AppState::Paused.in_menu());
        assert!(!AppState::Playing.in_menu());
        assert!(!AppState::Transition.in_menu());
    }
}
