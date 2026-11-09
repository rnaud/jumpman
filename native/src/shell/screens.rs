//! The menu screens (title, pause, settings, controls, save files, game over) and the handler for
//! [`UiAction`]s. Screens are rebuilt from scratch whenever [`ShellUi::dirty`] is set or the app
//! state changes; focus survives rebuilds by remembering the focused action and scroll offsets by
//! [`ScrollMemory`].
//!
//! The title screen and the pause menu share one *hub*: a header (logo / PAUSED + utility buttons),
//! a tab bar (GAMES | LEVELS | EXPERIMENTS, with the hero picker on the right) and the tab's
//! content. GAMES is a grid of mode cards with thumbnails, the headline modes first; LEVELS is the
//! course picker; EXPERIMENTS holds the tech demos in their own (violet) grid.
//!
//! Look: an N64-era front end (see `theme`): a scrolling checkerboard, bouncing title letters,
//! glossy bevelled slabs, a star cursor on the focused button and a quick spring-in when a screen
//! or tab opens. Every list scrolls (wheel,
//! touch drag, or focus following), and the layout comes in a compact (phone) and a desktop size,
//! see [`MenuLayout`].
use crate::button_marker::BorderRadius;
use super::nav::UiActionEvent;
use super::scroll::{restored, ScrollArea, ScrollMemory};
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

const PER_PAGE: usize = 18;
const BACKDROP_ASPECT: f32 = 1280.0 / 853.0;

/// [`ScrollArea::id`]s.
const SCROLL_MODES: u8 = 1;
const SCROLL_COURSES: u8 = 2;
const SCROLL_COURSE_INFO: u8 = 3;
const SCROLL_PAGE: u8 = 4;

fn pages() -> usize {
    COURSES.len().div_ceil(PER_PAGE)
}

fn full() -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }
}

/// Sizes that differ between the phone and desktop layouts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub compact: bool,
    /// Usable width / height in layout px.
    pub w: f32,
    pub h: f32,
    /// Height of a tappable button (>= 44 on phones).
    pub tap: f32,
    /// Button label size.
    pub font: f32,
    /// Screen padding.
    pub pad: f32,
    /// Mode card thumbnail size (16:10).
    pub thumb: Vec2,
}

impl Metrics {
    pub fn new(l: &MenuLayout) -> Self {
        if l.compact {
            Self { compact: true, w: l.width, h: l.height, tap: 44.0, font: 14.0, pad: 8.0, thumb: Vec2::new(120.0, 75.0) }
        } else {
            Self { compact: false, w: l.width, h: l.height, tap: 40.0, font: 15.0, pad: 18.0, thumb: Vec2::new(150.0, 94.0) }
        }
    }

    /// Mode cards per row for the hub's content width.
    pub fn card_columns(&self) -> usize {
        let min_card = if self.compact { 300.0 } else { 310.0 };
        (((self.content_width() + 8.0) / (min_card + 8.0)).floor() as usize).max(1)
    }

    /// Room inside the hub's content plate (padding, border and the scroller's right gutter removed).
    fn content_width(&self) -> f32 {
        let plate = if self.compact { 6.0 } else { 10.0 };
        self.w - 2.0 * self.pad - 2.0 * (plate + 2.0) - 5.0
    }

    fn card_width(&self) -> f32 {
        let cols = self.card_columns() as f32;
        ((self.content_width() - (cols - 1.0) * 8.0) / cols).floor()
    }

    /// Width of the LEVELS tab's info column.
    fn info_width(&self) -> f32 {
        if self.compact { 224.0 } else { 330.0 }
    }

    /// Course chips per row.
    pub fn course_columns(&self) -> usize {
        let avail = self.content_width() - self.info_width() - 10.0;
        if avail >= 470.0 { 2 } else { 1 }
    }
}

type RebuildKey = (AppState, Character, Option<ExperimentId>, u32, bool, u32);

#[allow(clippy::too_many_arguments)]
pub fn rebuild_screen(
    mut commands: Commands,
    mut ui: ResMut<ShellUi>,
    state: Res<AppState>,
    assets: Option<Res<UiAssets>>,
    settings: Res<Settings>,
    (character, xp): (Option<Res<characters::Character>>, Option<Res<ExperimentSlot>>),
    progress: Option<Res<Progress>>,
    slots: Res<SaveSlotSummaries>,
    pads: Query<&Gamepad>,
    bindings: Option<Res<crate::bindings::Bindings>>,
    layout: Res<MenuLayout>,
    memory: Res<ScrollMemory>,
    content: Option<Res<crate::content::AvailableContent>>,
    retro: Option<Res<super::retro::RetroEnabled>>,
    roots: Query<Entity, With<ScreenRoot>>,
    mut memo: Local<(Option<RebuildKey>, Option<(Screen, Tab)>)>,
) {
    let (last, last_view) = &mut *memo;
    let Some(a) = assets else { return };
    let character = character.map_or(Character::Mario, |c| Character::from_theirs(*c));
    let experiment = xp.and_then(|x| x.id).and_then(|x| ExperimentId::from_id(x.key()));
    let file = progress.map_or(1, |p| p.save.slot);
    // Rebuild when the layout class or the column count changes, not on every pixel of a resize.
    let m = Metrics::new(&layout);
    let key = (*state, character, experiment, file, m.compact, (m.card_columns() * 8 + m.course_columns()) as u32);
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
    // The title screen is drawn by the v3 front end (`retro`), not by bevy_ui.
    if super::retro::wants(retro.is_some_and(|r| r.0), *state, screen) {
        *last_view = None;
        return;
    }
    let hub_tab = if screen == Screen::Courses { Tab::Levels } else { ui.tab };
    // Spring in when the screen changes; slide the tray sideways when only the tab changes.
    let view = (screen, hub_tab);
    let pop = match *last_view {
        Some((s, t)) if s == screen && t != hub_tab => {
            let d = (Tab::ALL.iter().position(|x| *x == hub_tab).unwrap_or(0) as f32) - (Tab::ALL.iter().position(|x| *x == t).unwrap_or(0) as f32);
            Pop::Tab(d.signum())
        }
        Some(v) if v == view => Pop::None,
        _ => Pop::Screen,
    };
    *last_view = Some(view);
    ui.default_focus = Some(match screen {
        Screen::Title | Screen::Courses => if hub_tab == Tab::Levels { UiAction::Start } else { UiAction::Tab(hub_tab) },
        Screen::Pause => UiAction::Resume,
        Screen::GameOver => UiAction::MainMenu,
        _ => UiAction::Back,
    });
    let paused = *state == AppState::Paused;
    let mut root = commands.spawn((ScreenRoot, ShellRoot, GlobalZIndex(50), Node { align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..full() }));
    let ctx = HubCtx { pause: screen != Screen::Title, character, experiment, pad: !pads.is_empty(), tab: hub_tab, content: content.as_deref().cloned().unwrap_or_default(), pop };
    match screen {
        Screen::Title => {
            root.insert(BackgroundColor(hex(0x1b3fb4)));
            root.with_children(|p| {
                backdrop(p, &a, 1.0);
                backdrop_stars(p, &a);
                hub(p, &a, &m, &ui, &memory, &ctx);
            });
        }
        Screen::Pause | Screen::Courses => {
            root.insert(BackgroundColor(Color::srgba(0.02, 0.05, 0.18, 0.55)));
            root.with_children(|p| {
                backdrop(p, &a, 0.5);
                hub(p, &a, &m, &ui, &memory, &ctx);
            });
        }
        Screen::GameOver => {
            root.insert(BackgroundColor(Color::srgba(0.02, 0.03, 0.12, 0.92)));
            root.with_children(|p| game_over(p, &a, &m));
        }
        other => {
            root.insert(BackgroundColor(Color::srgba(0.02, 0.05, 0.18, if paused { 0.55 } else { 0.9 })));
            root.with_children(|p| {
                backdrop(p, &a, if paused { 0.5 } else { 1.0 });
                page(p, &memory, pop, |c| match other {
                    Screen::Settings => settings_screen(c, &a, &m, &settings),
                    Screen::Controls => controls_screen(c, &a, &m, !pads.is_empty(), bindings.as_deref()),
                    Screen::Slots => slots_screen(c, &a, &m, file, &slots, ui.confirm_reset),
                    _ => {}
                })
            });
        }
    }
}

/// Title of a sub-screen; phones get a BACK button beside it so it never needs scrolling to.
fn screen_title(c: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, text: &str) {
    if m.compact {
        c.spawn(Node { width: Val::Percent(100.0), align_items: AlignItems::Center, column_gap: Val::Px(14.0), ..default() }).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Gold, 96.0, m.tap, 18.0), "BACK", UiAction::Back, false);
            outlined(r, a, text, 34.0, GOLD);
        });
    } else {
        outlined(c, a, text, 44.0, GOLD);
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

/// A button as wide as its label.
fn fit_spec(kind: ButtonKind, m: &Metrics) -> ButtonSpec {
    ButtonSpec::new(kind, Val::Auto, Val::Px(m.tap), m.font)
}

fn action_button(p: &mut ChildSpawnerCommands, a: &UiAssets, s: ButtonSpec, text: &str, action: UiAction, selected: bool) {
    let mut e = button(p, a, s, text);
    e.insert(action);
    if selected {
        e.insert(Selected);
    }
}

/// A full-screen vertical scroller that centres its content when there is room (settings, controls...).
fn page(p: &mut ChildSpawnerCommands, memory: &ScrollMemory, pop: Pop, build: impl FnOnce(&mut ChildSpawnerCommands)) {
    let mut e = p.spawn((
        Node { width: Val::Percent(100.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, overflow: Overflow::scroll_y(), padding: UiRect::vertical(Val::Px(10.0)), ..default() },
        ScrollArea { id: SCROLL_PAGE },
        restored(memory, SCROLL_PAGE),
    ));
    if pop == Pop::Screen {
        e.insert(PopIn::new(Vec2::new(0.0, 18.0), 0.9));
    }
    e.with_children(build);
}

/// How a rebuilt screen animates in.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Pop {
    /// Same screen and tab (a selection changed): no motion.
    None,
    /// A new screen: spring up from slightly small.
    Screen,
    /// Same screen, new tab: the tray slides in from the side of the new tab (-1 / +1).
    Tab(f32),
}

/// Everything the hub needs to know that is not layout.
struct HubCtx {
    /// Pause menu rather than the title screen.
    pause: bool,
    character: Character,
    experiment: Option<ExperimentId>,
    pad: bool,
    tab: Tab,
    /// Which imported games are installed (BYOR); everything in the full build.
    content: crate::content::AvailableContent,
    pop: Pop,
}

fn hub(p: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, ui: &ShellUi, memory: &ScrollMemory, ctx: &HubCtx) {
    let mut root = p.spawn(Node { width: Val::Percent(100.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Column, padding: UiRect::all(Val::Px(m.pad)), row_gap: Val::Px(if m.compact { 6.0 } else { 10.0 }), ..default() });
    if ctx.pop == Pop::Screen {
        root.insert(PopIn::new(Vec2::new(0.0, 16.0), 0.94));
    }
    root.with_children(|c| {
        hub_header(c, a, m, ctx);
        tab_row(c, a, m, ctx);
        // Content area: a recessed tray (violet for the lab) so the slabs on it stand out.
        let tint = if ctx.tab == Tab::Experiments { hex(0x2e1870) } else { hex(0x132a80) };
        let mut body = c.spawn((
            Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), min_height: Val::Px(0.0), width: Val::Percent(100.0), padding: UiRect::all(Val::Px(if m.compact { 6.0 } else { 10.0 })), border: UiRect::all(Val::Px(3.0)), ..default() },
            tray(tint),
        ));
        if let Pop::Tab(d) = ctx.pop {
            body.insert(PopIn::new(Vec2::new(46.0 * d, 0.0), 0.985));
        }
        body.with_children(|body| match ctx.tab {
            Tab::Games | Tab::Experiments => modes_tab(body, a, m, memory, ctx),
            Tab::Levels => levels_tab(body, a, m, ui, memory, ctx),
        });
        if !m.compact {
            let hint = if ctx.pad {
                "Gamepad: stick to move, A to jump, B to punch, Start to pause. Shoulder buttons switch tabs."
            } else if cfg!(any(target_os = "ios", target_os = "android")) {
                "Stick moves, A jumps, B punches, Z crouches, drag to orbit, II pauses."
            } else {
                "WASD move, Space jump, Shift punch, Ctrl crouch, Esc pause. Mouse drag orbits. Tab switches tabs."
            };
            label(c, a, hint, 12.0, CREAM.with_alpha(0.75));
        }
    });
}

fn hub_header(c: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, ctx: &HubCtx) {
    c.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, flex_shrink: 0.0, column_gap: Val::Px(10.0), ..default() }).with_children(|h| {
        h.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(12.0), ..default() }).with_children(|l| {
            if ctx.pause {
                outlined(l, a, "PAUSED", if m.compact { 34.0 } else { 52.0 }, Color::WHITE);
                action_button(l, a, ButtonSpec::new(ButtonKind::Gold, Val::Auto, Val::Px(m.tap), if m.compact { 18.0 } else { 22.0 }), "RESUME", UiAction::Resume, false);
            } else {
                l.spawn(col(2.0)).with_children(|t| {
                    if !m.compact {
                        kicker(t, a, "NINTENDO 64 - 1997 - PORTED TO RUST");
                    }
                    t.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(if m.compact { 1.0 } else { 2.0 }), padding: UiRect::bottom(Val::Px(if m.compact { 4.0 } else { 8.0 })), ..default() }).with_children(|t| {
                        let size = if m.compact { 40.0 } else { 70.0 };
                        for (i, (ch, color)) in TITLE_LETTERS.iter().enumerate() {
                            outlined_with(t, a, &ch.to_string(), size, *color, Wave { phase: -(i as f32) * 0.55, amp: size * 0.07 });
                        }
                        t.spawn((
                            ImageNode::new(a.star.clone()),
                            Node { width: Val::Px(size * 0.75), height: Val::Px(size * 0.75), margin: UiRect::left(Val::Px(size * 0.1)), flex_shrink: 0.0, ..default() },
                            Wave { phase: -(TITLE_LETTERS.len() as f32) * 0.55, amp: size * 0.1 },
                        ));
                    });
                });
            }
        });
        h.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), flex_shrink: 0.0, ..default() }).with_children(|r| {
            if ctx.pause {
                action_button(r, a, fit_spec(ButtonKind::Blue, m), "MAIN MENU", UiAction::MainMenu, false);
            }
            action_button(r, a, fit_spec(ButtonKind::Blue, m), "SAVE FILE", UiAction::Open(Screen::Slots), false);
            action_button(r, a, fit_spec(ButtonKind::Blue, m), "SETTINGS", UiAction::Open(Screen::Settings), false);
            action_button(r, a, fit_spec(ButtonKind::Blue, m), "CONTROLS", UiAction::Open(Screen::Controls), false);
            // iOS apps must not close themselves (App Store guideline); the OS home gesture quits.
            if !ctx.pause && !cfg!(target_os = "ios") {
                action_button(r, a, fit_spec(ButtonKind::Red, m), "QUIT", UiAction::Quit, false);
            }
        });
    });
}

/// The title's letters and their colours (bright primaries on a navy outline).
const TITLE_LETTERS: [(char, Color); 7] = [('J', GOLD), ('U', RED_HI), ('M', hex(0x7ee36a)), ('P', hex(0x7cc4ff)), ('M', GOLD), ('A', RED_HI), ('N', hex(0x7ee36a))];

fn tab_row(c: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, ctx: &HubCtx) {
    c.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, flex_shrink: 0.0, column_gap: Val::Px(10.0), ..default() }).with_children(|t| {
        t.spawn(Node { column_gap: Val::Px(6.0), ..default() }).with_children(|tabs| {
            for tab in Tab::ALL {
                let kind = if tab == Tab::Experiments { ButtonKind::LabCard } else { ButtonKind::Blue };
                let (w, text) = if m.compact { (if tab == Tab::Experiments { 70.0 } else { 92.0 }, tab.short()) } else { (if tab == Tab::Experiments { 176.0 } else { 136.0 }, tab.label()) };
                action_button(tabs, a, ButtonSpec::new(kind, Val::Px(w), Val::Px(m.tap), if m.compact { 17.0 } else { 20.0 }), text, UiAction::Tab(tab), tab == ctx.tab);
            }
        });
        t.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), flex_shrink: 0.0, ..default() }).with_children(|r| {
            let mut missing: Vec<&str> = Vec::new();
            for g in [crate::content::Game::Oot, crate::content::Game::Bk] {
                if !ctx.content.has(g) {
                    missing.push(g.reason());
                }
            }
            if !missing.is_empty() && !m.compact {
                label(r, a, missing.join(" / "), 11.0, CREAM.with_alpha(0.6));
            }
            label(r, a, "PLAY AS", 12.0, GOLD);
            for ch in Character::ALL {
                let size = if m.compact { 13.0 } else { 14.0 };
                let mut b = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Auto, Val::Px(m.tap), size), ch.short());
                b.insert(UiAction::Character(ch));
                if ch == ctx.character {
                    b.insert(Selected);
                }
                if ctx.content.character_missing(ch).is_some() {
                    b.insert(Disabled);
                }
            }
        });
    });
}

/// A flat accent colour per mode: the card's thumbnail while its picture is missing, and its backing.
pub fn accent(e: ExperimentId) -> Color {
    let i = ExperimentId::ALL.iter().position(|x| *x == e).unwrap_or(0) as f32;
    Color::hsl(i * 360.0 / ExperimentId::ALL.len() as f32 + 12.0, 0.55, 0.34)
}

fn modes_tab(body: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, memory: &ScrollMemory, ctx: &HubCtx) {
    body.spawn((
        Node { width: Val::Percent(100.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_content: AlignContent::FlexStart, column_gap: Val::Px(8.0), row_gap: Val::Px(8.0), overflow: Overflow::scroll_y(), padding: UiRect::right(Val::Px(4.0)), ..default() },
        ScrollArea { id: SCROLL_MODES },
        restored(memory, SCROLL_MODES),
    ))
    .with_children(|g| {
        if let Some(active) = ctx.experiment {
            g.spawn(Node { width: Val::Percent(100.0), flex_shrink: 0.0, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, column_gap: Val::Px(8.0), padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)), ..default() }).with_children(|b| {
                label(b, a, format!("NOW PLAYING: {}", active.title().to_uppercase()), 14.0, GOLD_HI);
                action_button(b, a, fit_spec(ButtonKind::Red, m), "LEAVE MODE", UiAction::LeaveExperiment, false);
            });
        }
        if ctx.tab == Tab::Experiments {
            g.spawn(Node { width: Val::Percent(100.0), flex_shrink: 0.0, align_items: AlignItems::Center, column_gap: Val::Px(10.0), padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)), ..default() }).with_children(|b| {
                outlined(b, a, "THE LAB", if m.compact { 18.0 } else { 22.0 }, hex(0xd9b8ff));
                label(b, a, "Tech demos and toys - not full games. Poke at them.", 12.0, CREAM.with_alpha(0.85));
            });
        }
        let w = m.card_width();
        for (i, e) in ExperimentId::for_tab(ctx.tab).into_iter().enumerate() {
            let pick = ctx.tab == Tab::Games && i < 3;
            mode_card(g, a, m, e, w, ctx.experiment == Some(e), ctx.content.experiment_missing(e), pick);
        }
    });
}

/// One mode: thumbnail, title and a one-line blurb. Tapping it starts the mode fresh. Games are
/// blue slabs, experiments violet; the headline games wear a TOP PICK ribbon.
#[allow(clippy::too_many_arguments)]
fn mode_card(g: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, e: ExperimentId, width: f32, active: bool, missing: Option<crate::content::Game>, top_pick: bool) {
    let kind = if e.category() == crate::experiments::Category::Game { ButtonKind::Card } else { ButtonKind::LabCard };
    let mut card = g.spawn((
        slab(kind, Node { width: Val::Px(width), height: Val::Px(m.thumb.y + 18.0), padding: UiRect::all(Val::Px(6.0)), column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() }),
        UiAction::Experiment(e),
    ));
    if active {
        card.insert(BorderColor::all(GOLD));
    }
    if !e.available() || missing.is_some() {
        card.insert(Disabled);
    }
    card.with_children(|c| {
        bevel(c, 10.0, 0.45, 0.25);
        // Thumbnail in a white plastic frame.
        c.spawn((
            Node { width: Val::Px(m.thumb.x + 6.0), height: Val::Px(m.thumb.y + 6.0), flex_shrink: 0.0, border: UiRect::all(Val::Px(3.0)), overflow: Overflow::clip(), ..default() },
            BackgroundColor(accent(e)),
            BorderColor { top: Color::WHITE, left: hex(0xe8eeff), right: hex(0x9fb0d8), bottom: hex(0x7d8fbf) },
            BorderRadius::all(Val::Px(8.0)),
        ))
        .with_children(|t| {
            if let Some(image) = e.theirs().and_then(|x| a.modes.get(x.key())) {
                t.spawn((ImageNode::new(image.clone()), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }));
            }
        });
        c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), flex_grow: 1.0, flex_basis: Val::Px(0.0), min_width: Val::Px(0.0), ..default() }).with_children(|t| {
            t.spawn((Text::new(clean(e.title().to_uppercase())), font(&a.display, if m.compact { 16.0 } else { 19.0 }), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 2.5), color: hex(0x050b2a) }));
            let line = if let Some(game) = missing { game.reason().to_string() } else if e.available() { e.tagline().to_string() } else { format!("{} (not ported yet)", e.tagline()) };
            t.spawn((Text::new(clean(line)), font(&a.body, 12.0), TextColor(CREAM.with_alpha(0.9)), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY }));
            if active {
                t.spawn((Text::new("NOW PLAYING"), font(&a.display, 12.0), TextColor(GOLD)));
            }
        });
        if top_pick {
            c.spawn((
                Node { position_type: PositionType::Absolute, top: Val::Px(-2.0), right: Val::Px(8.0), padding: UiRect::axes(Val::Px(7.0), Val::Px(1.0)), border: UiRect::all(Val::Px(2.0)), ..default() },
                BackgroundColor(GOLD),
                BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(GOLD_HI, 0.0), ColorStop::percent(GOLD, 50.0), ColorStop::percent(GOLD_LO, 100.0)])),
                BorderColor::all(BROWN),
                BorderRadius::all(Val::Px(6.0)),
                bevy::ui::FocusPolicy::Pass,
            ))
            .with_children(|r| {
                r.spawn((Text::new("TOP PICK"), font(&a.display, 11.0), TextColor(BROWN)));
            });
        }
    });
}

fn course_name(id: &str) -> &'static str {
    courses::info(id).map_or("", |c| c.name)
}

fn levels_tab(body: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, ui: &ShellUi, memory: &ScrollMemory, ctx: &HubCtx) {
    body.spawn(Node { width: Val::Percent(100.0), height: Val::Percent(100.0), column_gap: Val::Px(10.0), ..default() }).with_children(|r| {
        // Left: header art, the chosen course and its missions, and START pinned underneath.
        r.spawn(Node { width: Val::Px(m.info_width()), height: Val::Percent(100.0), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() }).with_children(|l| {
            l.spawn((
                Node { width: Val::Percent(100.0), flex_grow: 1.0, flex_basis: Val::Px(0.0), min_height: Val::Px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), overflow: Overflow::scroll_y(), ..default() },
                ScrollArea { id: SCROLL_COURSE_INFO },
                restored(memory, SCROLL_COURSE_INFO),
            ))
            .with_children(|info| {
                // Header art (16:10): full width on desktop, a small badge beside the name on phones.
                let art_w = if m.compact { 100.0 } else { m.info_width() };
                let art = |parent: &mut ChildSpawnerCommands| {
                    parent
                        .spawn((
                            Node { width: Val::Px(art_w), height: Val::Px((art_w * 0.625).round()), flex_shrink: 0.0, overflow: Overflow::clip(), ..default() },
                            BackgroundColor(hex(0x2a4fb0)),
                            BorderRadius::all(Val::Px(12.0)),
                        ))
                        .with_children(|t| {
                            if let Some(image) = &a.levels {
                                t.spawn((ImageNode::new(image.clone()), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }));
                            }
                        });
                };
                let name = course_name(&ui.course);
                if m.compact {
                    info.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(8.0), flex_shrink: 0.0, ..default() }).with_children(|r| {
                        art(r);
                        r.spawn(Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), min_width: Val::Px(0.0), ..default() }).with_children(|t| {
                            label(t, a, name, 17.0, Color::WHITE);
                        });
                    });
                } else {
                    art(info);
                    label(info, a, name, 24.0, Color::WHITE);
                }
                label(info, a, courses::info(&ui.course).map_or("", |c| c.note), 12.0, CREAM.with_alpha(0.85));
                mission_row(info, a, m, ui);
            });
            let (text, size) = if ctx.pause { ("START COURSE", 20.0) } else { ("START PLAYING", 22.0) };
            action_button(l, a, ButtonSpec::new(ButtonKind::Gold, Val::Percent(100.0), Val::Px(m.tap + 6.0), size), text, UiAction::Start, false);
            if ctx.pause {
                label(l, a, "Starts a new run.", 12.0, CREAM.with_alpha(0.7));
            }
        });
        // Right: every course, scrolling.
        r.spawn((
            Node { width: Val::Percent(100.0), min_width: Val::Px(0.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_content: AlignContent::FlexStart, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), overflow: Overflow::scroll_y(), padding: UiRect::right(Val::Px(4.0)), ..default() },
            ScrollArea { id: SCROLL_COURSES },
            restored(memory, SCROLL_COURSES),
        ))
        .with_children(|g| {
            let cols = m.course_columns();
            let sub_open = super::bk_picker::is_open() || courses::is_bk_subarea(&ui.course);
            let mut section = "";
            let mut group = "";
            for c in COURSES {
                let sub = courses::is_bk_subarea(c.id);
                // Banjo-Kazooie: a "WORLDS" header, then a collapsed "SUB-AREAS" list (expands on tap or when one is selected).
                if c.series == Series::Banjo {
                    let wanted = if sub { "sub" } else { "worlds" };
                    if section != wanted {
                        section = wanted;
                        if sub {
                            let n = courses::BK_SUBAREAS.len();
                            let text = format!("BANJO-KAZOOIE · SUB-AREAS ({n})   {}", if sub_open { "HIDE" } else { "SHOW" });
                            let mut e = button(g, a, ButtonSpec { display: false, ..ButtonSpec::new(ButtonKind::Chip, Val::Percent(100.0), Val::Px(if m.compact { 44.0 } else { 36.0 }), 13.0) }, &text);
                            e.insert(UiAction::ToggleBkSubAreas);
                            e.insert(BorderColor::all(hex(0xd98a1f)));
                        } else {
                            g.spawn(Node { width: Val::Percent(100.0), margin: UiRect::top(Val::Px(6.0)), ..default() }).with_children(|h| {
                                label(h, a, "BANJO-KAZOOIE · WORLDS", 13.0, GOLD);
                            });
                        }
                    }
                }
                if sub && !sub_open {
                    continue;
                }
                if sub && super::bk_picker::group_of(c.name) != group {
                    group = super::bk_picker::group_of(c.name);
                    g.spawn(Node { width: Val::Percent(100.0), margin: UiRect::top(Val::Px(4.0)), ..default() }).with_children(|h| {
                        label(h, a, group.to_uppercase(), 12.0, CREAM.with_alpha(0.7));
                    });
                }
                let width = if cols == 2 { Val::Percent(48.2) } else { Val::Percent(100.0) };
                let missing = ctx.content.course_missing(c.id);
                let text = if missing.is_some() { format!("{} (needs ROM)", c.name) } else { c.name.to_string() };
                let mut e = button(g, a, ButtonSpec { display: false, ..ButtonSpec::new(ButtonKind::Chip, width, Val::Px(if m.compact { 44.0 } else { 36.0 }), 13.0) }, &text);
                e.insert(UiAction::SelectCourse(c.id));
                if ui.course == c.id {
                    e.insert(Selected);
                }
                if c.series == Series::Zelda {
                    e.insert(BorderColor::from(hex(0x2f8a3a)));
                } else if c.series == Series::Banjo {
                    e.insert(BorderColor::from(hex(0xd98a1f)));
                } else if c.series == Series::MarioKart {
                    e.insert(BorderColor::from(hex(0xc8281e)));
                }
                if ctx.content.course_missing(c.id).is_some() {
                    e.insert(Disabled);
                }
            }
        });
    });
}

fn mission_row(p: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, ui: &ShellUi) {
    let missions = courses::missions_for(&ui.course);
    if missions.len() < 2 {
        return;
    }
    p.spawn(col(4.0)).with_children(|r| {
        let name = missions.iter().find(|x| x.act == ui.act).map_or("", |x| x.name);
        label(r, a, format!("MISSION: {name}"), 12.0, GOLD);
        r.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|w| {
            for mission in &missions {
                action_button(w, a, spec(ButtonKind::Chip, m.tap, m.tap, 15.0), &mission.act.to_string(), UiAction::SelectAct(mission.act), mission.act == ui.act);
            }
        });
    });
}

fn setting_row(p: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, name: &str, key: DisplayKey, value: &str) {
    p.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(12.0), ..default() }).with_children(|r| {
        r.spawn(Node { width: Val::Px(if m.compact { 170.0 } else { 190.0 }), ..default() }).with_children(|n| {
            label(n, a, name, 15.0, Color::WHITE);
        });
        action_button(r, a, spec(ButtonKind::Blue, if m.compact { 320.0 } else { 360.0 }, m.tap, 15.0), &format!("<   {value}   >"), UiAction::Cycle(key, 1), false);
    });
}

fn settings_screen(p: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, s: &Settings) {
    let d = &s.display;
    panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(if m.compact { 14.0 } else { 24.0 })), row_gap: Val::Px(if m.compact { 6.0 } else { 10.0 }), width: Val::Px(640.0), max_width: Val::Percent(100.0), flex_shrink: 0.0, margin: UiRect::vertical(Val::Auto), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        screen_title(c, a, m, "SETTINGS");
        label(c, a, "DISPLAY", 13.0, GOLD);
        c.spawn(row()).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Green, 190.0, m.tap, 14.0), "CLASSIC PRESET", UiAction::Preset(false), d == &DisplaySettings::CLASSIC);
            action_button(r, a, spec(ButtonKind::Green, 190.0, m.tap, 14.0), "MODERN PRESET", UiAction::Preset(true), d == &DisplaySettings::MODERN);
        });
        setting_row(c, a, m, "Resolution", DisplayKey::Resolution, d.resolution.label());
        setting_row(c, a, m, "Aspect ratio", DisplayKey::Aspect, d.aspect.label());
        setting_row(c, a, m, "Lighting", DisplayKey::Look, d.look.label());
        setting_row(c, a, m, "Movement rendering", DisplayKey::Motion, d.motion.label());
        setting_row(c, a, m, "Upscaling", DisplayKey::Scaling, d.scaling.label());
        label(c, a, "AUDIO AND PERFORMANCE", 13.0, GOLD);
        c.spawn(row()).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Blue, 188.0, m.tap, 13.0), if s.muted { "AUDIO: MUTED" } else { "AUDIO: ON" }, UiAction::ToggleMute, s.muted);
            action_button(r, a, spec(ButtonKind::Blue, 188.0, m.tap, 13.0), if s.show_fps { "SHOW FPS: ON" } else { "SHOW FPS: OFF" }, UiAction::ToggleShowFps, s.show_fps);
            action_button(r, a, spec(ButtonKind::Blue, 188.0, m.tap, 13.0), if s.uncap_fps { "UNCAP FPS: ON" } else { "UNCAP FPS: OFF" }, UiAction::ToggleUncap, s.uncap_fps);
        });
        c.spawn(row()).with_children(|r| {
            action_button(r, a, spec(ButtonKind::Blue, 300.0, m.tap, 13.0), if s.classic_intro { "CLASSIC SM64 EXTRAS: ON" } else { "CLASSIC SM64 EXTRAS: OFF" }, UiAction::ToggleClassicIntro, s.classic_intro);
        });
        setting_row(c, a, m, "Light quality", DisplayKey::Quality, s.lighting.label());
        setting_row(c, a, m, "Camera", DisplayKey::Camera, s.camera.label());
        label(c, a, &crate::lighting::ray_tracing_note(), 12.0, CREAM.with_alpha(0.6));
        label(c, a, "Display preferences are saved on this device.", 12.0, CREAM.with_alpha(0.7));
        if crate::byor::is_byor_install() {
            label(c, a, "ROM IMPORT", 13.0, GOLD);
            let have = |g: &str| if crate::byor::installed(g) { "installed" } else if crate::byor::game_stale(g) { "update available" } else { "not imported" };
            label(c, a, format!("Super Mario 64: {}   Mario Kart 64: {}   Ocarina of Time: {}   Banjo-Kazooie: {}", have("sm64"), have("mk64"), have("oot"), have("bk")), 12.0, CREAM.with_alpha(0.8));
            action_button(c, a, spec(ButtonKind::Green, 360.0, m.tap, 15.0), "IMPORT / RE-EXTRACT ROMS", UiAction::ImportRoms, false);
        }
        if !m.compact {
            action_button(c, a, spec(ButtonKind::Gold, 200.0, m.tap, 20.0), "BACK", UiAction::Back, false);
        }
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
                    BorderColor::all(GOLD_LO),
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

fn controls_screen(p: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, pad: bool, bound: Option<&crate::bindings::Bindings>) {
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
                    action_button(r, a, spec(ButtonKind::Blue, 170.0, if m.compact { 34.0 } else { 26.0 }, 13.0), &shown, ui_action, waiting);
                });
            }
        });
    };
    panel(p, a, Node { padding: UiRect::axes(Val::Px(if m.compact { 16.0 } else { 30.0 }), Val::Px(18.0)), row_gap: Val::Px(8.0), max_width: Val::Percent(100.0), flex_shrink: 0.0, margin: UiRect::vertical(Val::Auto), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        screen_title(c, a, m, "CONTROLS");
        // Side by side on desktop; stacked on a phone.
        c.spawn(Node { flex_direction: if m.compact { FlexDirection::Column } else { FlexDirection::Row }, row_gap: Val::Px(14.0), column_gap: Val::Px(28.0), align_items: AlignItems::FlexStart, ..default() }).with_children(|r| {
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
            action_button(r, a, spec(ButtonKind::Red, 200.0, m.tap, 16.0), "RESET DEFAULTS", UiAction::ResetBindings, false);
            if !m.compact {
                action_button(r, a, spec(ButtonKind::Gold, 200.0, m.tap, 20.0), "BACK", UiAction::Back, false);
            }
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

fn slots_screen(p: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics, active: u32, slots: &SaveSlotSummaries, confirm: bool) {
    panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(if m.compact { 14.0 } else { 24.0 })), row_gap: Val::Px(if m.compact { 6.0 } else { 10.0 }), width: Val::Px(520.0), max_width: Val::Percent(100.0), flex_shrink: 0.0, margin: UiRect::vertical(Val::Auto), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        screen_title(c, a, m, "SAVE FILE");
        for n in 1..=4u8 {
            let text = match slots.0[n as usize - 1] {
                Some(stars) => format!("FILE {}      {} STARS", (b'A' + n - 1) as char, stars),
                None => format!("FILE {}      NEW", (b'A' + n - 1) as char),
            };
            action_button(c, a, spec(ButtonKind::Blue, 380.0, m.tap + 2.0, 18.0), &text, UiAction::Slot(n), active == u32::from(n));
        }
        action_button(c, a, spec(ButtonKind::Red, 380.0, m.tap, 14.0), if confirm { "PRESS AGAIN TO ERASE THIS FILE" } else { "RESET STARS & SAVED COINS" }, UiAction::ResetSave, confirm);
        label(c, a, "Star and coin records are kept per file.", 12.0, CREAM.with_alpha(0.7));
        if !m.compact {
            action_button(c, a, spec(ButtonKind::Gold, 200.0, m.tap, 20.0), "BACK", UiAction::Back, false);
        }
    });
}

fn game_over(p: &mut ChildSpawnerCommands, a: &UiAssets, m: &Metrics) {
    p.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(if m.compact { 12.0 } else { 24.0 }), ..default() }).with_children(|c| {
        kicker(c, a, "NO LIVES LEFT");
        outlined(c, a, "GAME OVER", if m.compact { 72.0 } else { 110.0 }, RED_HI);
        action_button(c, a, spec(ButtonKind::Gold, 300.0, 54.0, 24.0), "CONTINUE", UiAction::MainMenu, false);
    });
}

// ---------------------------------------------------------------------------------------------
// Action handling
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn apply_actions(
    mut events: MessageReader<UiActionEvent>,
    mut ui: ResMut<ShellUi>,
    mut settings: ResMut<Settings>,
    mut state: ResMut<AppState>,
    mut start: MessageWriter<StartCourse>,
    mut chars: MessageWriter<SwitchCharacter>,
    mut experiments: MessageWriter<OpenExperiment>,
    mut leave: MessageWriter<LeaveExperiment>,
    mut menu: MessageWriter<ReturnToMenu>,
    mut slot: MessageWriter<SaveSlotSelected>,
    mut reset: MessageWriter<ResetSaveRequested>,
    mut exit: MessageWriter<AppExit>,
    mut toast: ResMut<Toast>,
    bindings: Option<ResMut<crate::bindings::Bindings>>,
    content: Option<Res<crate::content::AvailableContent>>,
    mut sfx: Option<ResMut<MenuSfx>>,
) {
    let mut bindings = bindings;
    let content = content.as_deref().cloned().unwrap_or_default();
    for UiActionEvent(action) in events.read() {
        let blocked = match action {
            UiAction::Start => content.course_missing(&ui.course).is_some(),
            UiAction::Character(c) => content.character_missing(*c).is_some(),
            UiAction::SelectCourse(id) => content.course_missing(id).is_some(),
            UiAction::Experiment(e) => content.experiment_missing(*e).is_some(),
            _ => false,
        };
        // Back with nothing to go back to on the title screen stays silent.
        let silent = matches!(action, UiAction::Back) && ui.stack.is_empty() && *state == AppState::Menu;
        if !silent {
            MenuSfx::push(&mut sfx, if blocked { MenuSfx::DENIED } else { MenuSfx::for_action(action) });
        }
        match action {
            UiAction::Start if content.course_missing(&ui.course).is_some() => {
                toast.show("IMPORT THE MISSING ROM IN SETUP (SETTINGS)", 2.4);
            }
            UiAction::ImportRoms => crate::byor::reopen_setup(&mut exit),
            UiAction::Character(c) if content.character_missing(*c).is_some() => {}
            UiAction::SelectCourse(id) if content.course_missing(id).is_some() => {}
            UiAction::Experiment(e) if content.experiment_missing(*e).is_some() => {}
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
            UiAction::Tab(t) => {
                ui.tab = *t;
                ui.dirty = true;
                ui.last_focus = Some(UiAction::Tab(*t));
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
            UiAction::Cycle(DisplayKey::Camera, dir) => {
                settings.camera = settings.camera.cycle(*dir);
                ui.dirty = true;
            }
            UiAction::Cycle(DisplayKey::Quality, dir) => {
                settings.lighting = settings.lighting.cycle(*dir);
                if settings.lighting == Lighting::RayTraced && !crate::lighting::rt_support().supported {
                    settings.lighting = settings.lighting.cycle(*dir);
                }
                ui.dirty = true;
            }
            UiAction::Cycle(key, dir) => {
                let d = &mut settings.display;
                match key {
                    DisplayKey::Resolution => d.resolution = d.resolution.cycle(*dir),
                    DisplayKey::Aspect => d.aspect = d.aspect.cycle(*dir),
                    DisplayKey::Look => d.look = d.look.cycle(*dir),
                    DisplayKey::Motion => d.motion = d.motion.cycle(*dir),
                    DisplayKey::Scaling => d.scaling = d.scaling.cycle(*dir),
                    DisplayKey::Quality | DisplayKey::Camera => {}
                }
                ui.dirty = true;
            }
            UiAction::Preset(modern) => {
                // The native default keeps the window's own aspect; a preset keeps whatever the player chose for it only for "modern".
                settings.display = if *modern { DisplaySettings::MODERN } else { DisplaySettings::CLASSIC };
                ui.dirty = true;
            }
            UiAction::ToggleShowFps => {
                settings.show_fps = !settings.show_fps;
                ui.dirty = true;
            }
            UiAction::ToggleBkSubAreas => {
                super::bk_picker::toggle();
                ui.dirty = true;
            }
            UiAction::ToggleUncap => {
                settings.uncap_fps = !settings.uncap_fps;
                ui.dirty = true;
            }
            UiAction::ToggleClassicIntro => {
                settings.classic_intro = !settings.classic_intro;
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

/// "Cover" the window with the backdrop image, centred.
pub fn fit_backdrop(windows: Query<&Window, With<PrimaryWindow>>, mut backdrop: Query<&mut Node, With<Backdrop>>, ui_scale: Option<Res<UiScale>>) {
    let (Ok(window), Ok(mut node)) = (windows.single(), backdrop.single_mut()) else { return };
    // `Val::Px` is multiplied by `UiScale`; divide it back out so the backdrop still covers the window.
    let scale = ui_scale.map_or(1.0, |s| s.0).max(1e-3);
    let (w, h) = (window.width() / scale, window.height() / scale);
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
            .insert_resource(ShellUi { screen: Screen::Title, stack: vec![], course: "castle_grounds".into(), act: 1, page: 0, tab: Tab::Games, confirm_reset: false, last_focus: None, default_focus: None, dirty: false })
            .insert_resource(Settings::default())
            .init_resource::<Toast>()
            .add_message::<UiActionEvent>()
            .add_message::<StartCourse>()
            .add_message::<SwitchCharacter>()
            .add_message::<OpenExperiment>()
            .add_message::<LeaveExperiment>()
            .add_message::<ReturnToMenu>()
            .add_message::<SaveSlotSelected>()
            .add_message::<ResetSaveRequested>()
            .add_systems(Update, apply_actions);
        let send = |app: &mut App, a: UiAction| {
            app.world_mut().write_message(UiActionEvent(a));
            app.update();
        };
        send(&mut app, UiAction::SelectCourse("bob"));
        assert_eq!(app.world().resource::<ShellUi>().course, "bob");
        send(&mut app, UiAction::SelectAct(3));
        send(&mut app, UiAction::Start);
        let started: Vec<_> = app.world().resource::<Messages<StartCourse>>().iter_current_update_messages().map(|e| (e.0.id.clone(), e.0.act)).collect();
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
        send(&mut app, UiAction::ToggleShowFps);
        send(&mut app, UiAction::ToggleUncap);
        send(&mut app, UiAction::Cycle(DisplayKey::Quality, 1));
        let st = app.world().resource::<Settings>();
        assert!(st.show_fps && st.uncap_fps && st.lighting == Lighting::Enhanced);
        assert_eq!(st.display.look, Look::Classic, "the lighting tier does not touch the display look");
        send(&mut app, UiAction::Back);
        assert_eq!(app.world().resource::<ShellUi>().screen, Screen::Title);
        // Erasing the save needs a second press.
        send(&mut app, UiAction::ResetSave);
        assert!(app.world().resource::<ShellUi>().confirm_reset);
        send(&mut app, UiAction::Slot(3));
        assert!(!app.world().resource::<ShellUi>().confirm_reset);
        let picked: Vec<_> = app.world().resource::<Messages<SaveSlotSelected>>().iter_current_update_messages().map(|e| e.0).collect();
        assert_eq!(picked, vec![3]);
        send(&mut app, UiAction::Preset(false));
        assert_eq!(app.world().resource::<Settings>().display, DisplaySettings::CLASSIC);
    }

    #[test]
    fn actions_queue_their_menu_sounds() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(AppState::Menu)
            .insert_resource(ShellUi { screen: Screen::Title, stack: vec![], course: "bob".into(), act: 1, page: 0, tab: Tab::Games, confirm_reset: false, last_focus: None, default_focus: None, dirty: false })
            .insert_resource(Settings::default())
            .insert_resource(crate::content::AvailableContent::probe(std::path::Path::new("/nonexistent/assets")))
            .init_resource::<Toast>()
            .init_resource::<MenuSfx>()
            .add_message::<UiActionEvent>()
            .add_message::<StartCourse>()
            .add_message::<SwitchCharacter>()
            .add_message::<OpenExperiment>()
            .add_message::<LeaveExperiment>()
            .add_message::<ReturnToMenu>()
            .add_message::<SaveSlotSelected>()
            .add_message::<ResetSaveRequested>()
            .add_systems(Update, apply_actions);
        let mut send = |a: UiAction| {
            app.world_mut().write_message(UiActionEvent(a));
            app.update();
            std::mem::take(&mut app.world_mut().resource_mut::<MenuSfx>().0)
        };
        assert_eq!(send(UiAction::Tab(Tab::Experiments)), vec![MenuSfx::TAB]);
        assert_eq!(send(UiAction::Open(Screen::Settings)), vec![MenuSfx::OPEN]);
        assert_eq!(send(UiAction::Back), vec![MenuSfx::BACK]);
        assert_eq!(send(UiAction::Back), Vec::<&str>::new(), "nothing to go back to");
        assert_eq!(send(UiAction::Experiment(ExperimentId::Kart)), vec![MenuSfx::DENIED], "needs the MK64 ROM");
        assert_eq!(send(UiAction::Experiment(ExperimentId::Brawl)), vec![MenuSfx::START]);
    }

    #[test]
    fn tab_actions_switch_the_hub_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(AppState::Menu)
            .insert_resource(ShellUi { screen: Screen::Title, stack: vec![], course: "bob".into(), act: 1, page: 0, tab: Tab::Games, confirm_reset: false, last_focus: None, default_focus: None, dirty: false })
            .insert_resource(Settings::default())
            .init_resource::<Toast>()
            .add_message::<UiActionEvent>()
            .add_message::<StartCourse>()
            .add_message::<SwitchCharacter>()
            .add_message::<OpenExperiment>()
            .add_message::<LeaveExperiment>()
            .add_message::<ReturnToMenu>()
            .add_message::<SaveSlotSelected>()
            .add_message::<ResetSaveRequested>()
            .add_systems(Update, apply_actions);
        app.world_mut().write_message(UiActionEvent(UiAction::Tab(Tab::Levels)));
        app.update();
        let ui = app.world().resource::<ShellUi>();
        assert_eq!(ui.tab, Tab::Levels);
        assert!(ui.dirty);
        // A mode card opens that mode (the experiments module does the fresh start).
        app.world_mut().write_message(UiActionEvent(UiAction::Experiment(ExperimentId::Kart)));
        app.update();
        let opened: Vec<_> = app.world().resource::<Messages<OpenExperiment>>().iter_current_update_messages().map(|e| e.0).collect();
        assert_eq!(opened, vec![ExperimentId::Kart]);
    }

    #[test]
    fn missing_games_block_their_menu_actions() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(AppState::Menu)
            .insert_resource(ShellUi { screen: Screen::Title, stack: vec![], course: "bob".into(), act: 1, page: 0, tab: Tab::Games, confirm_reset: false, last_focus: None, default_focus: None, dirty: false })
            .insert_resource(Settings::default())
            // An SM64-only install: nothing optional is present.
            .insert_resource(crate::content::AvailableContent::probe(std::path::Path::new("/nonexistent/assets")))
            .init_resource::<Toast>()
            .add_message::<UiActionEvent>()
            .add_message::<StartCourse>()
            .add_message::<SwitchCharacter>()
            .add_message::<OpenExperiment>()
            .add_message::<LeaveExperiment>()
            .add_message::<ReturnToMenu>()
            .add_message::<SaveSlotSelected>()
            .add_message::<ResetSaveRequested>()
            .add_systems(Update, apply_actions);
        let send = |app: &mut App, a: UiAction| {
            app.world_mut().write_message(UiActionEvent(a));
            app.update();
        };
        send(&mut app, UiAction::Experiment(ExperimentId::Kart));
        assert_eq!(app.world().resource::<Messages<OpenExperiment>>().iter_current_update_messages().count(), 0, "Mario Kart needs its ROM");
        send(&mut app, UiAction::Experiment(ExperimentId::Swarm));
        assert_eq!(app.world().resource::<Messages<OpenExperiment>>().iter_current_update_messages().count(), 1, "SM64-only modes stay available");
        send(&mut app, UiAction::Character(Character::Banjo));
        send(&mut app, UiAction::Character(Character::AdultLink));
        assert_eq!(app.world().resource::<Messages<SwitchCharacter>>().iter_current_update_messages().count(), 0);
        send(&mut app, UiAction::Character(Character::Mario));
        assert_eq!(app.world().resource::<Messages<SwitchCharacter>>().iter_current_update_messages().count(), 1);
        send(&mut app, UiAction::SelectCourse("kokiri_forest"));
        assert_eq!(app.world().resource::<ShellUi>().course, "bob", "Zelda courses need the OoT ROM");
        send(&mut app, UiAction::SelectCourse("ccm"));
        assert_eq!(app.world().resource::<ShellUi>().course, "ccm");
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
