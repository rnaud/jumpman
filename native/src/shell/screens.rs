//! The menu screens (title, pause, course picker, settings, controls, save files, game over) and
//! the handler for [`UiAction`]s. Screens are rebuilt from scratch whenever [`ShellUi::dirty`] is
//! set or the app state changes; focus survives rebuilds by remembering the focused action.
use super::nav::{Focus, UiActionEvent};
use super::settings::{Settings, *};
use super::theme::*;
use super::*;
use crate::game::courses::{Series, COURSES};
use bevy::window::PrimaryWindow;

/// Root of the menu screen currently shown (despawned on rebuild).
#[derive(Component)]
pub struct ScreenRoot;
/// Everything the shell owns that must survive course switches.
#[derive(Component)]
pub struct ShellRoot;
/// The title backdrop image (resized to "cover" the window).
#[derive(Component)]
pub struct Backdrop;
/// A text line that describes the focused button (experiment blurb, course note).
#[derive(Component)]
pub struct Caption;

const PER_PAGE: usize = 18;
const BACKDROP_ASPECT: f32 = 1280.0 / 853.0;

fn pages() -> usize {
    COURSES.len().div_ceil(PER_PAGE)
}

fn full() -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }
}

#[allow(clippy::too_many_arguments)]
pub fn rebuild_screen(
    mut commands: Commands,
    mut ui: ResMut<ShellUi>,
    state: Res<AppState>,
    assets: Option<Res<UiAssets>>,
    settings: Res<Settings>,
    character: Option<Res<characters::Character>>,
    xp: Option<Res<ExperimentSlot>>,
    progress: Option<Res<Progress>>,
    slots: Res<SaveSlotSummaries>,
    pads: Query<&Gamepad>,
    bindings: Option<Res<crate::bindings::Bindings>>,
    roots: Query<Entity, With<ScreenRoot>>,
    mut last: Local<Option<(AppState, Character, Option<ExperimentId>, u32)>>,
) {
    let Some(a) = assets else { return };
    let character = character.map_or(Character::Mario, |c| Character::from_theirs(*c));
    let experiment = xp.and_then(|x| x.id).and_then(|x| ExperimentId::from_id(x.key()));
    let file = progress.map_or(1, |p| p.save.slot);
    let key = (*state, character, experiment, file);
    let rebound = bindings.as_ref().is_some_and(|b| b.is_changed());
    if !ui.dirty && !rebound && *last == Some(key) {
        return;
    }
    ui.dirty = false;
    *last = Some(key);
    for r in &roots {
        commands.entity(r).despawn();
    }
    let screen = match *state {
        AppState::Menu | AppState::Paused | AppState::GameOver => ui.screen,
        _ => return,
    };
    ui.default_focus = Some(match screen {
        Screen::Title | Screen::Courses => UiAction::Start,
        Screen::Pause => UiAction::Resume,
        Screen::GameOver => UiAction::MainMenu,
        _ => UiAction::Back,
    });
    let paused = *state == AppState::Paused;
    let mut root = commands.spawn((ScreenRoot, ShellRoot, GlobalZIndex(10), Node { align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..full() }));
    match screen {
        Screen::Title => {
            root.insert(BackgroundColor(hex(0x1b3fb4)));
            title(&mut root, &a, &ui, character, !pads.is_empty());
        }
        Screen::GameOver => {
            root.insert(BackgroundColor(Color::srgba(0.02, 0.03, 0.12, 0.92)));
            root.with_children(|p| game_over(p, &a));
        }
        other => {
            root.insert(BackgroundColor(Color::srgba(0.02, 0.05, 0.18, if paused { 0.62 } else { 0.9 })));
            root.with_children(|p| match other {
                Screen::Pause => pause(p, &a, character, experiment),
                Screen::Courses => courses_screen(p, &a, &ui),
                Screen::Settings => settings_screen(p, &a, &settings),
                Screen::Controls => controls_screen(p, &a, !pads.is_empty(), bindings.as_deref()),
                Screen::Slots => slots_screen(p, &a, file, &slots, ui.confirm_reset),
                _ => {}
            });
        }
    }
}

fn row() -> Node {
    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(8.0), ..default() }
}

fn col(gap: f32) -> Node {
    Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(gap), ..default() }
}

fn spec(kind: ButtonKind, w: f32, h: f32, size: f32) -> ButtonSpec {
    ButtonSpec::new(kind, Val::Px(w), Val::Px(h), size)
}

fn action_button(p: &mut ChildSpawnerCommands, a: &UiAssets, s: ButtonSpec, text: &str, action: UiAction, selected: bool) {
    let mut e = button(p, a, s, text);
    e.insert(action);
    if selected {
        e.insert(Selected);
    }
}

fn character_row(p: &mut ChildSpawnerCommands, a: &UiAssets, current: Character, pill_w: f32) {
    p.spawn(row()).with_children(|r| {
        for c in Character::ALL {
            action_button(r, a, spec(ButtonKind::Blue, pill_w, 34.0, 14.0), c.label(), UiAction::Character(c), c == current);
        }
    });
}

fn experiment_grid(p: &mut ChildSpawnerCommands, a: &UiAssets, active: Option<ExperimentId>, cols: usize, w: f32, h: f32, size: f32) {
    p.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(8.0), row_gap: Val::Px(8.0), ..default() }).with_children(|g| {
        let _ = cols;
        for e in ExperimentId::ALL {
            let mut b = button(g, a, spec(ButtonKind::Green, w, h, size), e.title());
            b.insert(UiAction::Experiment(e));
            if active == Some(e) {
                b.insert(Selected);
            }
            if !e.available() {
                b.insert(Disabled);
            }
        }
    });
}

fn caption(p: &mut ChildSpawnerCommands, a: &UiAssets, width: f32) {
    p.spawn((Caption, Text::new(""), font(&a.body, 12.0), TextColor(CREAM.with_alpha(0.85)), Node { width: Val::Px(width), min_height: Val::Px(30.0), ..default() }));
}

fn course_name(id: &str) -> &'static str {
    courses::info(id).map_or("", |c| c.name)
}

/// Course list (paged) + mission row; shared by the title and the pause "courses" screen.
fn course_picker(p: &mut ChildSpawnerCommands, a: &UiAssets, ui: &ShellUi, chip_h: f32) {
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, margin: UiRect::bottom(Val::Px(6.0)), ..default() }).with_children(|h| {
        outlined(h, a, "COURSES", 24.0, GOLD);
        h.spawn((
            Node { min_width: Val::Px(32.0), padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, ..default() },
            BackgroundColor(GOLD),
            BorderColor(NAVY),
            BorderRadius::all(Val::Px(99.0)),
        ))
        .with_children(|c| {
            c.spawn((Text::new(COURSES.len().to_string()), font(&a.display, 14.0), TextColor(NAVY)));
        });
    });
    p.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(5.0), width: Val::Percent(100.0), ..default() }).with_children(|g| {
        for c in COURSES.iter().skip(ui.page * PER_PAGE).take(PER_PAGE) {
            let mut e = button(g, a, ButtonSpec { display: false, ..ButtonSpec::new(ButtonKind::Chip, Val::Percent(48.4), Val::Px(chip_h), 12.0) }, c.name);
            e.insert(UiAction::SelectCourse(c.id));
            if ui.course == c.id {
                e.insert(Selected);
            }
            if c.series == Series::Zelda {
                e.insert(BorderColor(hex(0x2f8a3a)));
            }
        }
    });
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, margin: UiRect::top(Val::Px(8.0)), ..default() }).with_children(|r| {
        action_button(r, a, spec(ButtonKind::Blue, 44.0, 28.0, 16.0), "<", UiAction::Page(-1), false);
        label(r, a, format!("PAGE {}/{}", ui.page + 1, pages()), 13.0, GOLD_HI);
        action_button(r, a, spec(ButtonKind::Blue, 44.0, 28.0, 16.0), ">", UiAction::Page(1), false);
    });
}

fn mission_row(p: &mut ChildSpawnerCommands, a: &UiAssets, ui: &ShellUi) {
    let missions = courses::missions_for(&ui.course);
    if missions.len() < 2 {
        return;
    }
    p.spawn(row()).with_children(|r| {
        label(r, a, "MISSION", 12.0, GOLD);
        for m in &missions {
            action_button(r, a, spec(ButtonKind::Chip, 32.0, 26.0, 13.0), &m.act.to_string(), UiAction::SelectAct(m.act), m.act == ui.act);
        }
        let name = missions.iter().find(|m| m.act == ui.act).map_or("", |m| m.name);
        label(r, a, name, 12.0, CREAM);
    });
}

fn title(root: &mut EntityCommands, a: &UiAssets, ui: &ShellUi, character: Character, pad: bool) {
    root.with_children(|p| {
        p.spawn((Backdrop, ImageNode::new(a.backdrop.clone()), Node { position_type: PositionType::Absolute, ..default() }));
        p.spawn((full(), BackgroundColor(Color::srgba(0.03, 0.08, 0.3, 0.3))));
        p.spawn(Node { width: Val::Percent(100.0), height: Val::Percent(100.0), padding: UiRect::all(Val::Px(22.0)), column_gap: Val::Px(20.0), ..default() }).with_children(|c| {
            // Left column: logo, start block, experiments.
            c.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexStart, row_gap: Val::Px(8.0), flex_grow: 1.0, flex_basis: Val::Px(0.0), ..default() }).with_children(|l| {
                kicker(l, a, "NINTENDO 64 - 1997 - PORTED TO RUST");
                l.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(2.0), ..default() }).with_children(|t| {
                    outlined(t, a, "JUMP", 84.0, GOLD);
                    outlined(t, a, "MAN", 84.0, RED_HI);
                    icon(t, &a.star, 64.0);
                });
                label(l, a, "Mario 64's analog stick, action state machine and 30 Hz physics, now native. Play the courses, then bend the engine into something new.", 14.0, Color::WHITE).insert(Node { max_width: Val::Px(560.0), ..default() });
                panel(l, a, Node { padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)), row_gap: Val::Px(6.0), max_width: Val::Px(600.0), ..default() }).with_children(|s| {
                    s.spawn(col(3.0)).with_children(|n| {
                        label(n, a, "COURSE", 12.0, GOLD);
                        label(n, a, course_name(&ui.course), 26.0, Color::WHITE);
                        label(n, a, courses::info(&ui.course).map_or("", |c| c.note), 12.0, CREAM.with_alpha(0.85)).insert(Node { max_width: Val::Px(560.0), min_height: Val::Px(30.0), ..default() });
                    });
                    mission_row(s, a, ui);
                    label(s, a, "CHARACTER", 12.0, GOLD);
                    character_row(s, a, character, 128.0);
                    action_button(s, a, spec(ButtonKind::Gold, 300.0, 54.0, 24.0), "START PLAYING", UiAction::Start, false);
                    label(s, a, if pad { "Gamepad connected: stick to move, A to jump, B to punch, Start to pause." } else { "WASD move, Space jump, Shift punch, Ctrl crouch, Esc pause. Mouse drag orbits." }, 11.0, CREAM.with_alpha(0.7));
                });
                outlined(l, a, "EXPERIMENTS", 17.0, Color::WHITE);
                experiment_grid(l, a, None, 4, 144.0, 38.0, 13.0);
                caption(l, a, 600.0);
            });
            // Right column: course list + utility buttons.
            c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(10.0), width: Val::Px(330.0), flex_shrink: 0.0, ..default() }).with_children(|r| {
                panel(r, a, Node { padding: UiRect::all(Val::Px(16.0)), ..default() }).with_children(|cp| {
                    course_picker(cp, a, ui, 28.0);
                    label(cp, a, "Every native course terrain is loadable. Experiments run on top of the same engine.", 10.0, CREAM.with_alpha(0.7)).insert(Node { margin: UiRect::top(Val::Px(8.0)), ..default() });
                });
                r.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(8.0), row_gap: Val::Px(8.0), ..default() }).with_children(|b| {
                    action_button(b, a, spec(ButtonKind::Blue, 100.0, 34.0, 14.0), "SAVE FILE", UiAction::Open(Screen::Slots), false);
                    action_button(b, a, spec(ButtonKind::Blue, 100.0, 34.0, 14.0), "SETTINGS", UiAction::Open(Screen::Settings), false);
                    action_button(b, a, spec(ButtonKind::Blue, 100.0, 34.0, 14.0), "CONTROLS", UiAction::Open(Screen::Controls), false);
                    action_button(b, a, spec(ButtonKind::Red, 100.0, 34.0, 14.0), "QUIT", UiAction::Quit, false);
                });
            });
        });
    });
}

fn pause(p: &mut ChildSpawnerCommands, a: &UiAssets, character: Character, experiment: Option<ExperimentId>) {
    panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(24.0)), row_gap: Val::Px(12.0), width: Val::Px(640.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        kicker(c, a, "JUMPMAN");
        outlined(c, a, "PAUSED", 52.0, Color::WHITE);
        action_button(c, a, spec(ButtonKind::Gold, 320.0, 54.0, 24.0), "RESUME", UiAction::Resume, false);
        label(c, a, "PLAY AS", 12.0, GOLD);
        character_row(c, a, character, 138.0);
        c.spawn(row()).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Blue, 150.0, 36.0, 15.0), "MAIN MENU", UiAction::MainMenu, false);
            action_button(r, a, spec(ButtonKind::Blue, 150.0, 36.0, 15.0), "COURSES", UiAction::Open(Screen::Courses), false);
            action_button(r, a, spec(ButtonKind::Blue, 150.0, 36.0, 15.0), "SAVE FILE", UiAction::Open(Screen::Slots), false);
        });
        label(c, a, "EXPERIMENTS", 12.0, GOLD);
        experiment_grid(c, a, experiment, 4, 132.0, 34.0, 12.0);
        if experiment.is_some() {
            action_button(c, a, spec(ButtonKind::Red, 220.0, 34.0, 15.0), "LEAVE EXPERIMENT", UiAction::LeaveExperiment, false);
        }
        caption(c, a, 560.0);
        c.spawn(row()).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Blue, 150.0, 36.0, 15.0), "SETTINGS", UiAction::Open(Screen::Settings), false);
            action_button(r, a, spec(ButtonKind::Blue, 150.0, 36.0, 15.0), "CONTROLS", UiAction::Open(Screen::Controls), false);
        });
    });
}

fn courses_screen(p: &mut ChildSpawnerCommands, a: &UiAssets, ui: &ShellUi) {
    panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(22.0)), row_gap: Val::Px(10.0), width: Val::Px(560.0), ..default() }).with_children(|c| {
        label(c, a, "Changing course starts a new run.", 12.0, CREAM.with_alpha(0.8));
        course_picker(c, a, ui, 30.0);
        label(c, a, course_name(&ui.course), 22.0, Color::WHITE);
        mission_row(c, a, ui);
        caption(c, a, 480.0);
        c.spawn(row()).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Gold, 220.0, 48.0, 22.0), "START COURSE", UiAction::Start, false);
            action_button(r, a, spec(ButtonKind::Blue, 120.0, 40.0, 16.0), "BACK", UiAction::Back, false);
        });
    });
}

fn setting_row(p: &mut ChildSpawnerCommands, a: &UiAssets, name: &str, key: DisplayKey, value: &str) {
    p.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(12.0), ..default() }).with_children(|r| {
        r.spawn(Node { width: Val::Px(190.0), ..default() }).with_children(|n| {
            label(n, a, name, 15.0, Color::WHITE);
        });
        action_button(r, a, spec(ButtonKind::Blue, 360.0, 36.0, 15.0), &format!("<   {value}   >"), UiAction::Cycle(key, 1), false);
    });
}

fn settings_screen(p: &mut ChildSpawnerCommands, a: &UiAssets, s: &Settings) {
    let d = &s.display;
    panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(24.0)), row_gap: Val::Px(10.0), width: Val::Px(640.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        outlined(c, a, "SETTINGS", 44.0, GOLD);
        label(c, a, "DISPLAY", 13.0, GOLD);
        c.spawn(row()).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Green, 190.0, 34.0, 14.0), "CLASSIC PRESET", UiAction::Preset(false), d == &DisplaySettings::CLASSIC);
            action_button(r, a, spec(ButtonKind::Green, 190.0, 34.0, 14.0), "MODERN PRESET", UiAction::Preset(true), d == &DisplaySettings::MODERN);
        });
        setting_row(c, a, "Resolution", DisplayKey::Resolution, d.resolution.label());
        setting_row(c, a, "Aspect ratio", DisplayKey::Aspect, d.aspect.label());
        setting_row(c, a, "Lighting", DisplayKey::Look, d.look.label());
        setting_row(c, a, "Movement rendering", DisplayKey::Motion, d.motion.label());
        setting_row(c, a, "Upscaling", DisplayKey::Scaling, d.scaling.label());
        label(c, a, "AUDIO", 13.0, GOLD);
        action_button(c, a, spec(ButtonKind::Blue, 360.0, 36.0, 15.0), if s.muted { "GAME AUDIO: MUTED" } else { "GAME AUDIO: ON" }, UiAction::ToggleMute, s.muted);
        label(c, a, "Display preferences are saved on this device.", 11.0, CREAM.with_alpha(0.7));
        action_button(c, a, spec(ButtonKind::Gold, 200.0, 44.0, 20.0), "BACK", UiAction::Back, false);
    });
}

/// Keyboard + gamepad bindings (mirrors `Input.ts` / `input.rs`).
pub const KEY_BINDINGS: &[(&str, &str)] = &[
    ("W A S D", "Move (analog stick)"),
    ("Space", "Jump (hold for height)"),
    ("Shift / J", "Punch, grab, dive"),
    ("Ctrl / K", "Crouch, ground pound"),
    ("Alt", "Walk (half stick)"),
    ("Mouse drag / wheel", "Orbit / zoom camera"),
    ("Q E / arrows", "Orbit camera"),
    ("C   X", "Cycle camera   Snap behind"),
    ("L", "Focus (Link)"),
    ("R", "Restart the run"),
    ("F3 / `", "Debug panel"),
    ("Esc", "Pause menu"),
];
pub const PAD_BINDINGS: &[(&str, &str)] = &[
    ("Left stick", "Move"),
    ("Right stick", "Orbit camera"),
    ("A (south)", "Jump"),
    ("B (east)", "Punch, grab, dive"),
    ("X / Y / triggers", "Crouch, ground pound"),
    ("L bumper", "Focus (Link)"),
    ("Start", "Pause / resume"),
    ("D-pad / stick", "Menu navigation"),
    ("A / B in menus", "Confirm / back"),
];

fn bindings(p: &mut ChildSpawnerCommands, a: &UiAssets, title: &str, rows: &[(&str, &str)], width: f32) {
    p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), width: Val::Px(width), ..default() }).with_children(|c| {
        outlined(c, a, title, 20.0, GOLD);
        for (input, what) in rows {
            c.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                r.spawn((
                    Node { min_width: Val::Px(128.0), padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, ..default() },
                    BackgroundColor(hex(0x0c1b50)),
                    BorderColor(GOLD_LO),
                    BorderRadius::all(Val::Px(7.0)),
                ))
                .with_children(|k| {
                    k.spawn((Text::new(*input), font(&a.display, 13.0), TextColor(GOLD_HI)));
                });
                label(r, a, *what, 13.0, Color::WHITE);
            });
        }
    });
}

fn controls_screen(p: &mut ChildSpawnerCommands, a: &UiAssets, pad: bool, bound: Option<&crate::bindings::Bindings>) {
    use crate::bindings::{Action, Bindings, Listen};
    let fallback = Bindings::default();
    let b = bound.unwrap_or(&fallback);
    let rebinding = |c: &mut ChildSpawnerCommands, title: &str, rows: Vec<(Action, String, bool, UiAction)>, width: f32| {
        c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), width: Val::Px(width), ..default() }).with_children(|c| {
            outlined(c, a, title, 20.0, GOLD);
            for (action, text, waiting, ui_action) in rows {
                c.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                    label(r, a, action.label(), 13.0, Color::WHITE);
                    let shown = if waiting { "PRESS A KEY...".to_string() } else { text };
                    action_button(r, a, spec(ButtonKind::Blue, 170.0, 26.0, 12.0), &shown, ui_action, waiting);
                });
            }
        });
    };
    panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(18.0)), row_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        outlined(c, a, "CONTROLS", 44.0, GOLD);
        c.spawn(Node { column_gap: Val::Px(28.0), align_items: AlignItems::FlexStart, ..default() }).with_children(|r| {
            let keys = Action::KEYBOARD.iter().map(|&x| (x, b.key_text(x), b.listening == Some(Listen::Key(x)), UiAction::Rebind(x))).collect();
            rebinding(r, "KEYBOARD", keys, 380.0);
            r.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(14.0), ..default() }).with_children(|r| {
                let pads = Action::PAD.iter().map(|&x| (x, b.pad_text(x), b.listening == Some(Listen::Pad(x)), UiAction::RebindPad(x))).collect();
                rebinding(r, "GAMEPAD", pads, 380.0);
                bindings(r, a, "FIXED", FIXED_BINDINGS, 380.0);
            });
        });
        label(c, a, if pad { "Gamepad detected." } else { "No gamepad detected - plug one in and press any button." }, 12.0, if pad { hex(0x9cf06a) } else { CREAM.with_alpha(0.75) });
        label(c, a, "Click a binding, then press the new key or button (Esc cancels).", 12.0, CREAM.with_alpha(0.75));
        c.spawn(row()).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Red, 200.0, 40.0, 16.0), "RESET DEFAULTS", UiAction::ResetBindings, false);
            action_button(r, a, spec(ButtonKind::Gold, 200.0, 40.0, 20.0), "BACK", UiAction::Back, false);
        });
    });
}

/// Inputs that cannot be rebound.
pub const FIXED_BINDINGS: &[(&str, &str)] = &[
    ("Mouse drag / wheel", "Orbit / zoom camera"),
    ("Arrows", "Orbit camera, look up"),
    ("F3 / `", "Debug panel"),
    ("Esc / Start", "Pause menu"),
    ("Right stick", "Orbit camera"),
    ("D-pad / stick", "Menu navigation"),
];

fn slots_screen(p: &mut ChildSpawnerCommands, a: &UiAssets, active: u32, slots: &SaveSlotSummaries, confirm: bool) {
    panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(24.0)), row_gap: Val::Px(10.0), width: Val::Px(520.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        outlined(c, a, "SAVE FILE", 44.0, GOLD);
        for n in 1..=4u8 {
            let text = match slots.0[n as usize - 1] {
                Some(stars) => format!("FILE {}      {} STARS", (b'A' + n - 1) as char, stars),
                None => format!("FILE {}      NEW", (b'A' + n - 1) as char),
            };
            action_button(c, a, spec(ButtonKind::Blue, 380.0, 42.0, 18.0), &text, UiAction::Slot(n), active == u32::from(n));
        }
        action_button(c, a, spec(ButtonKind::Red, 380.0, 36.0, 14.0), if confirm { "PRESS AGAIN TO ERASE THIS FILE" } else { "RESET STARS & SAVED COINS" }, UiAction::ResetSave, confirm);
        label(c, a, "Star and coin records are kept per file.", 11.0, CREAM.with_alpha(0.7));
        action_button(c, a, spec(ButtonKind::Gold, 200.0, 44.0, 20.0), "BACK", UiAction::Back, false);
    });
}

fn game_over(p: &mut ChildSpawnerCommands, a: &UiAssets) {
    p.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(24.0), ..default() }).with_children(|c| {
        kicker(c, a, "NO LIVES LEFT");
        outlined(c, a, "GAME OVER", 110.0, RED_HI);
        action_button(c, a, spec(ButtonKind::Gold, 300.0, 54.0, 24.0), "CONTINUE", UiAction::MainMenu, false);
    });
}

// ---------------------------------------------------------------------------------------------
// Action handling
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn apply_actions(
    mut events: EventReader<UiActionEvent>,
    mut ui: ResMut<ShellUi>,
    mut settings: ResMut<Settings>,
    mut state: ResMut<AppState>,
    mut start: EventWriter<StartCourse>,
    mut chars: EventWriter<SwitchCharacter>,
    mut experiments: EventWriter<OpenExperiment>,
    mut leave: EventWriter<LeaveExperiment>,
    mut menu: EventWriter<ReturnToMenu>,
    mut slot: EventWriter<SaveSlotSelected>,
    mut reset: EventWriter<ResetSaveRequested>,
    mut exit: EventWriter<AppExit>,
    mut toast: ResMut<Toast>,
    bindings: Option<ResMut<crate::bindings::Bindings>>,
) {
    let mut bindings = bindings;
    for UiActionEvent(action) in events.read() {
        match action {
            UiAction::Rebind(a) => {
                if let Some(b) = bindings.as_deref_mut() {
                    b.listening = Some(crate::bindings::Listen::Key(*a));
                }
            }
            UiAction::RebindPad(a) => {
                if let Some(b) = bindings.as_deref_mut() {
                    b.listening = Some(crate::bindings::Listen::Pad(*a));
                }
            }
            UiAction::ResetBindings => {
                if let Some(b) = bindings.as_deref_mut() {
                    *b = crate::bindings::Bindings::default();
                }
            }
            UiAction::Start => {
                let mut request = CourseRequest::new(&ui.course);
                request.act = ui.act;
                if settings.course != ui.course || settings.act != ui.act {
                    settings.course = ui.course.clone();
                    settings.act = ui.act;
                }
                start.write(StartCourse(request));
            }
            UiAction::Resume => {
                *state = AppState::Playing;
            }
            UiAction::MainMenu => {
                menu.write(ReturnToMenu);
            }
            UiAction::Quit => {
                exit.write(AppExit::Success);
            }
            UiAction::SelectCourse(id) => {
                ui.course = (*id).to_string();
                ui.act = courses::missions_for(id).first().map_or(1, |m| m.act);
                ui.dirty = true;
            }
            UiAction::SelectAct(act) => {
                ui.act = *act;
                ui.dirty = true;
            }
            UiAction::Page(d) => {
                ui.page = (ui.page as i32 + d).rem_euclid(pages() as i32) as usize;
                ui.dirty = true;
            }
            UiAction::Character(c) => {
                chars.write(SwitchCharacter(*c));
                ui.dirty = true;
            }
            UiAction::Experiment(e) => {
                experiments.write(OpenExperiment(*e));
            }
            UiAction::LeaveExperiment => {
                leave.write(LeaveExperiment);
                *state = AppState::Playing;
            }
            UiAction::Open(screen) => ui.open(*screen),
            UiAction::Back => match ui.stack.pop() {
                Some(prev) => {
                    ui.screen = prev;
                    ui.dirty = true;
                    ui.last_focus = None;
                }
                None => match *state {
                    AppState::Paused => *state = AppState::Playing,
                    AppState::GameOver => {
                        menu.write(ReturnToMenu);
                    }
                    _ => {}
                },
            },
            UiAction::Cycle(key, dir) => {
                let d = &mut settings.display;
                match key {
                    DisplayKey::Resolution => d.resolution = d.resolution.cycle(*dir),
                    DisplayKey::Aspect => d.aspect = d.aspect.cycle(*dir),
                    DisplayKey::Look => d.look = d.look.cycle(*dir),
                    DisplayKey::Motion => d.motion = d.motion.cycle(*dir),
                    DisplayKey::Scaling => d.scaling = d.scaling.cycle(*dir),
                }
                ui.dirty = true;
            }
            UiAction::Preset(modern) => {
                // The native default keeps the window's own aspect; a preset keeps whatever the player chose for it only for "modern".
                settings.display = if *modern { DisplaySettings::MODERN } else { DisplaySettings::CLASSIC };
                ui.dirty = true;
            }
            UiAction::ToggleMute => {
                settings.muted = !settings.muted;
                ui.dirty = true;
            }
            UiAction::Slot(n) => {
                slot.write(SaveSlotSelected(*n));
                ui.dirty = true;
            }
            UiAction::ResetSave => {
                if ui.confirm_reset {
                    ui.confirm_reset = false;
                    reset.write(ResetSaveRequested);
                    toast.show("SAVE FILE ERASED", 1.8);
                } else {
                    ui.confirm_reset = true;
                }
                ui.dirty = true;
            }
        }
        if !matches!(action, UiAction::ResetSave) && ui.confirm_reset {
            ui.confirm_reset = false;
            ui.dirty = true;
        }
    }
}

pub fn update_caption(focus: Res<Focus>, actions: Query<&UiAction>, mut captions: Query<&mut Text, With<Caption>>) {
    let text = match focus.0.and_then(|e| actions.get(e).ok()) {
        Some(UiAction::Experiment(e)) if !e.available() => format!("{} - not ported to the native build yet.", e.blurb()),
        Some(UiAction::Experiment(e)) => e.blurb().to_string(),
        Some(UiAction::SelectCourse(id)) => courses::info(id).map_or(String::new(), |c| c.note.to_string()),
        _ => String::new(),
    };
    let text = clean(text);
    for mut t in &mut captions {
        if t.0 != text {
            t.0 = text.clone();
        }
    }
}

/// "Cover" the window with the backdrop image, centred.
pub fn fit_backdrop(windows: Query<&Window, With<PrimaryWindow>>, mut backdrop: Query<&mut Node, With<Backdrop>>) {
    let (Ok(window), Ok(mut node)) = (windows.single(), backdrop.single_mut()) else { return };
    let (w, h) = (window.width(), window.height());
    let (bw, bh) = if w / h > BACKDROP_ASPECT { (w, w / BACKDROP_ASPECT) } else { (h * BACKDROP_ASPECT, h) };
    let (left, top, width, height) = (Val::Px((w - bw) / 2.0), Val::Px((h - bh) / 2.0), Val::Px(bw), Val::Px(bh));
    if node.width != width || node.height != height || node.left != left || node.top != top {
        node.left = left;
        node.top = top;
        node.width = width;
        node.height = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_drive_navigation_settings_and_events() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(AppState::Menu)
            .insert_resource(ShellUi { screen: Screen::Title, stack: vec![], course: "castle_grounds".into(), act: 1, page: 0, confirm_reset: false, last_focus: None, default_focus: None, dirty: false })
            .insert_resource(Settings::default())
            .init_resource::<Toast>()
            .add_event::<UiActionEvent>()
            .add_event::<StartCourse>()
            .add_event::<SwitchCharacter>()
            .add_event::<OpenExperiment>()
            .add_event::<LeaveExperiment>()
            .add_event::<ReturnToMenu>()
            .add_event::<SaveSlotSelected>()
            .add_event::<ResetSaveRequested>()
            .add_systems(Update, apply_actions);
        let send = |app: &mut App, a: UiAction| {
            app.world_mut().send_event(UiActionEvent(a));
            app.update();
        };
        send(&mut app, UiAction::SelectCourse("bob"));
        assert_eq!(app.world().resource::<ShellUi>().course, "bob");
        send(&mut app, UiAction::SelectAct(3));
        send(&mut app, UiAction::Start);
        let started: Vec<_> = app.world().resource::<Events<StartCourse>>().iter_current_update_events().map(|e| (e.0.id.clone(), e.0.act)).collect();
        assert_eq!(started, vec![("bob".to_string(), 3)]);
        // Pages wrap both ways.
        send(&mut app, UiAction::Page(-1));
        assert_eq!(app.world().resource::<ShellUi>().page, pages() - 1);
        send(&mut app, UiAction::Page(1));
        assert_eq!(app.world().resource::<ShellUi>().page, 0);
        // Sub-screens push and pop.
        send(&mut app, UiAction::Open(Screen::Settings));
        assert_eq!(app.world().resource::<ShellUi>().screen, Screen::Settings);
        send(&mut app, UiAction::Cycle(DisplayKey::Look, 1));
        send(&mut app, UiAction::ToggleMute);
        assert_eq!(app.world().resource::<Settings>().display.look, Look::Classic);
        assert!(app.world().resource::<Settings>().muted);
        send(&mut app, UiAction::Back);
        assert_eq!(app.world().resource::<ShellUi>().screen, Screen::Title);
        // Erasing the save needs a second press.
        send(&mut app, UiAction::ResetSave);
        assert!(app.world().resource::<ShellUi>().confirm_reset);
        send(&mut app, UiAction::Slot(3));
        assert!(!app.world().resource::<ShellUi>().confirm_reset);
        let picked: Vec<_> = app.world().resource::<Events<SaveSlotSelected>>().iter_current_update_events().map(|e| e.0).collect();
        assert_eq!(picked, vec![3]);
        send(&mut app, UiAction::Preset(false));
        assert_eq!(app.world().resource::<Settings>().display, DisplaySettings::CLASSIC);
    }

    #[test]
    fn course_pages_cover_the_catalogue() {
        assert_eq!(pages(), COURSES.len().div_ceil(PER_PAGE));
        assert!(pages() * PER_PAGE >= COURSES.len());
    }

    #[test]
    fn binding_tables_are_nonempty_and_ascii() {
        for (k, v) in KEY_BINDINGS.iter().chain(PAD_BINDINGS) {
            assert!(!k.is_empty() && !v.is_empty());
            assert!(k.is_ascii() && v.is_ascii(), "{k} {v}");
        }
    }

    #[test]
    fn every_course_name_survives_font_cleaning() {
        for c in COURSES {
            assert!(clean(c.name.to_string()).is_ascii(), "{}", c.name);
            assert!(clean(c.note.to_string()).is_ascii(), "{}", c.note);
        }
        for e in ExperimentId::ALL {
            assert!(clean(e.blurb().to_string()).is_ascii());
        }
    }
}
