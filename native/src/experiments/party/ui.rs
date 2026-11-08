//! Sunstone Shindig screens on `bevy_ui`: title, mini-game practice list, board HUD and prompts, mini-game intro / countdown / tiles /
//! results, and the final ceremony. Phone-first at 874x402 logical: 44 px+ targets, safe-area insets, nothing under the pause button
//! (top-right) or the touch pad (bottom corners while a stick game runs). Mouse, touch, keyboard and gamepad all press the same [`UiCmd`]s.
use super::app::*;
use super::board::*;
use super::common::*;
use super::game::*;
use super::minis::*;
use crate::experiments::XpEntity;
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::shell::AppState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

const Z: i32 = 5;
/// Width reserved at the top-right for the pause button.
const PAUSE_W: f32 = 62.0;

#[derive(Component, Clone)]
pub struct Act(pub UiCmd);
#[derive(Component)]
struct ScreenRoot;
#[derive(Component)]
struct ChipCoins(usize);
#[derive(Component)]
struct ChipSun(usize);
#[derive(Component)]
struct ChipItems(usize);
#[derive(Component)]
struct ChipTag(usize);
#[derive(Component)]
struct ChipFrame(usize);
#[derive(Component)]
struct TimerText;
#[derive(Component)]
struct RoundText;
#[derive(Component)]
struct SpaceLabel(usize);
#[derive(Component)]
struct FloaterNode(usize);
#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct ToastText;
#[derive(Component)]
struct DiceText;
#[derive(Component)]
struct BigText;
#[derive(Component)]
struct StageText;
#[derive(Component)]
struct TileBtn(usize);
#[derive(Component)]
struct TileLabel;
#[derive(Component)]
struct RollLabel;

#[derive(Resource, Default)]
pub struct PartyUi {
    key: (u32, u8, u8, u8, u8),
    sw: Vec2,
    root: Option<Entity>,
    pooled: bool,
    focus_list: Vec<Entity>,
    focus: usize,
    nav: bool,
}

pub struct PartyUiPlugin;

impl Plugin for PartyUiPlugin {
    fn build(&self, app: &mut App) {
        let active = resource_exists::<PartyApp>;
        app.init_resource::<PartyUi>().add_systems(Update, (ui_build, hud_update, space_labels, overlays, tiles, button_input).chain().run_if(active).after(super::PartySet)).add_systems(Update, reset_ui.run_if(not(active)));
    }
}

fn reset_ui(mut ui: ResMut<PartyUi>) {
    if ui.root.is_some() || ui.pooled {
        *ui = PartyUi::default();
    }
}

// ---------------------------------------------------------------------------------------------
// Widgets
// ---------------------------------------------------------------------------------------------

fn col(c: u32) -> Color {
    hex(c)
}

fn body(a: &UiAssets, s: &str, size: f32, color: Color) -> impl Bundle {
    (Text::new(clean(s.to_string())), font(&a.body, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY })
}

fn disp(a: &UiAssets, s: &str, size: f32, color: Color) -> impl Bundle {
    (Text::new(clean(s.to_string())), font(&a.display, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY })
}

fn abs_node(left: f32, top: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), ..default() }
}

fn full(area: &SafeArea) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        padding: UiRect { left: Val::Px(area.left.max(10.0)), right: Val::Px(area.right.max(10.0)), top: Val::Px(area.top.max(6.0)), bottom: Val::Px(area.bottom.max(8.0)) },
        flex_direction: FlexDirection::Column,
        ..default()
    }
}

fn panel_bg() -> Color {
    Color::srgba(0.03, 0.06, 0.2, 0.88)
}

fn act_button<'a>(p: &'a mut ChildSpawnerCommands, a: &UiAssets, kind: ButtonKind, w: f32, h: f32, size: f32, text: &str, cmd: UiCmd) -> EntityCommands<'a> {
    let mut b = button(p, a, ButtonSpec::new(kind, Val::Px(w), Val::Px(h), size), text);
    b.insert(Act(cmd));
    b
}

fn dead_button<'a>(p: &'a mut ChildSpawnerCommands, a: &UiAssets, w: f32, h: f32, size: f32, text: &str) -> EntityCommands<'a> {
    let mut b = button(p, a, ButtonSpec::new(ButtonKind::Chip, Val::Px(w), Val::Px(h), size), text);
    b.insert(Disabled);
    b
}

fn pill(p: &mut ChildSpawnerCommands, a: &UiAssets, text: &str, size: f32) -> Entity {
    p.spawn((Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)), border: UiRect::all(Val::Px(2.0)), align_items: AlignItems::Center, ..default() }, BackgroundColor(panel_bg()), BorderColor(NAVY), BorderRadius::all(Val::Px(99.0))))
        .with_children(|c| {
            c.spawn(body(a, text, size, CREAM));
        })
        .id()
}

fn fmt_score(kind: Mini, s: f32) -> String {
    if !s.is_finite() {
        return "-".into();
    }
    let u = kind.info().unit;
    if u.contains("seconds") {
        format!("{:.1}s", s.min(60.0))
    } else if kind == Mini::ShellLane {
        if s > 500.0 {
            format!("finished {:.1}s", (1000.0 - s) / 10.0)
        } else {
            format!("{:.0} m", s)
        }
    } else if kind == Mini::QuickDraw {
        format!("{} pts", (s / 10.0).round().max(0.0))
    } else {
        format!("{:.0}", s)
    }
}

// ---------------------------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------------------------

fn screen_code(app: &PartyApp) -> (u8, u8, u8) {
    let sub = match app.screen {
        Screen::Mini => app.mini.as_ref().map_or(0, |m| m.stage as u8),
        Screen::Results => (((app.t - app.results_t0) / 1.6).max(0.0) as u8).min(5),
        Screen::Board => match app.game.as_ref().map(|g| &g.phase) {
            Some(Phase::TurnStart) => 1,
            Some(Phase::Branch(_)) => 2,
            Some(Phase::KeeperOffer) => 3,
            Some(Phase::ShopMenu) => 4,
            Some(Phase::DuelPick) => 5,
            Some(Phase::MiniResult(_)) => 6,
            Some(Phase::Over) => 7,
            _ => 0,
        },
        _ => 0,
    };
    let flags = (app.busy() as u8) | ((app.dice.as_ref().is_some_and(|d| d.rolling) as u8) << 1) | ((app.pending_item.is_some() as u8) << 2) | ((app.game.as_ref().is_some_and(|g| g.waiting_for_human()) as u8) << 3);
    (app.screen as u8, sub, flags)
}

#[allow(clippy::too_many_arguments)]
fn ui_build(mut commands: Commands, app: Res<PartyApp>, assets: Option<Res<UiAssets>>, area: Res<SafeArea>, windows: Query<&Window, With<PrimaryWindow>>, mut ui: ResMut<PartyUi>) {
    let Some(a) = assets else { return };
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    if !ui.pooled {
        ui.pooled = true;
        spawn_pool(&mut commands, &a);
    }
    let (s, sub, flags) = screen_code(&app);
    let key = (app.rev, s, sub, flags, 0);
    let size_changed = (win - ui.sw).abs().max_element() > 1.0;
    if ui.key == key && ui.root.is_some() && !size_changed {
        return;
    }
    ui.sw = win;
    ui.key = key;
    if let Some(r) = ui.root.take() {
        commands.entity(r).despawn();
    }
    ui.focus_list.clear();
    ui.focus = 0;
    let root = match app.screen {
        Screen::Title => build_title(&mut commands, &a, &area, &app, win),
        Screen::Minis => build_minis(&mut commands, &a, &area, &app, win),
        Screen::Board => build_board(&mut commands, &a, &area, &app, win),
        Screen::Mini => build_mini(&mut commands, &a, &area, &app, win),
        Screen::Results => build_results(&mut commands, &a, &area, &app, win),
    };
    ui.root = Some(root);
}

/// Persistent overlay entities: labels over spaces, floaters, banner, toast, dice, tile buttons.
fn spawn_pool(commands: &mut Commands, a: &UiAssets) {
    for i in 0..48 {
        commands.spawn((XpEntity, SpaceLabel(i), GlobalZIndex(Z - 1), Text::new(""), font(&a.display, 15.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY }, abs_node(0.0, 0.0), Visibility::Hidden));
    }
    for i in 0..14 {
        commands.spawn((XpEntity, FloaterNode(i), GlobalZIndex(Z + 3), Text::new(""), font(&a.display, 22.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, abs_node(0.0, 0.0), Visibility::Hidden));
    }
    commands.spawn((XpEntity, BannerText, GlobalZIndex(Z + 3), Text::new(""), font(&a.display, 44.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }, abs_node(0.0, 0.0), Visibility::Hidden));
    commands.spawn((XpEntity, ToastText, GlobalZIndex(Z + 3), Text::new(""), font(&a.body, 14.0), TextColor(CREAM), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, abs_node(0.0, 0.0), Visibility::Hidden));
    commands.spawn((XpEntity, BigText, GlobalZIndex(Z + 3), Text::new(""), font(&a.display, 96.0), TextColor(GOLD_HI), TextShadow { offset: Vec2::new(0.0, 4.0), color: NAVY }, abs_node(0.0, 0.0), Visibility::Hidden));
    commands.spawn((XpEntity, StageText, GlobalZIndex(Z + 1), Text::new(""), font(&a.display, 30.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }, abs_node(0.0, 0.0), Visibility::Hidden));
    // The dice block.
    commands
        .spawn((XpEntity, DiceText, GlobalZIndex(Z + 2), Node { position_type: PositionType::Absolute, width: Val::Px(92.0), height: Val::Px(92.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(4.0)), ..default() }, BackgroundColor(CREAM), BorderColor(GOLD), BorderRadius::all(Val::Px(18.0)), Visibility::Hidden))
        .with_children(|c| {
            c.spawn((Text::new("1"), font(&a.display, 58.0), TextColor(BROWN)));
        });
    for i in 0..12 {
        commands
            .spawn((XpEntity, TileBtn(i), Button, GlobalZIndex(Z + 1), Node { position_type: PositionType::Absolute, justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(4.0)), ..default() }, BackgroundColor(BLUE), BorderColor(NAVY), BorderRadius::all(Val::Px(16.0)), Outline { width: Val::Px(0.0), offset: Val::Px(3.0), color: GOLD_HI }, Visibility::Hidden))
            .with_children(|c| {
                c.spawn((TileLabel, Text::new(""), font(&a.display, 44.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }));
            });
    }
}

fn chips_row(p: &mut ChildSpawnerCommands, a: &UiAssets, app: &PartyApp, win: Vec2, area: &SafeArea, tags: bool) {
    let avail = win.x - area.left.max(10.0) - area.right.max(10.0) - PAUSE_W;
    let w = ((avail - 12.0) / 4.0).clamp(96.0, 168.0);
    p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
        for i in 0..4 {
            let name = app.game.as_ref().map_or(PLAYER_NAMES[i], |g| g.players[i].name);
            r.spawn((ChipFrame(i), Node { width: Val::Px(w), height: Val::Px(44.0), padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)), border: UiRect::all(Val::Px(2.5)), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, ..default() }, BackgroundColor(panel_bg()), BorderColor(NAVY), BorderRadius::all(Val::Px(11.0))))
                .with_children(|c| {
                    c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(5.0), align_items: AlignItems::Center, ..default() }).with_children(|l| {
                        l.spawn((Node { width: Val::Px(12.0), height: Val::Px(12.0), border: UiRect::all(Val::Px(1.5)), ..default() }, BackgroundColor(col(PLAYER_COLORS[i])), BorderColor(Color::WHITE), BorderRadius::all(Val::Px(6.0))));
                        l.spawn(body(a, name, 12.0, Color::WHITE));
                        l.spawn((ChipItems(i), body(a, "", 11.0, GOLD_HI)));
                        if tags {
                            l.spawn((ChipTag(i), body(a, "", 11.0, CREAM)));
                        }
                    });
                    c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.0), align_items: AlignItems::Center, ..default() }).with_children(|l| {
                        icon(l, &a.coin, 13.0);
                        l.spawn((ChipCoins(i), body(a, "0", 13.0, GOLD_HI)));
                        icon(l, &a.star, 13.0);
                        l.spawn((ChipSun(i), body(a, "0", 13.0, Color::WHITE)));
                    });
                });
        }
    });
}

fn build_title(commands: &mut Commands, a: &UiAssets, area: &SafeArea, app: &PartyApp, win: Vec2) -> Entity {
    let root = commands.spawn((XpEntity, ScreenRoot, GlobalZIndex(Z), full(area), BackgroundColor(Color::srgba(0.02, 0.04, 0.16, 0.45)))).id();
    let small = win.y < 460.0;
    commands.entity(root).with_children(|r| {
        r.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, margin: UiRect::right(Val::Px(PAUSE_W)), ..default() }).with_children(|h| {
            let t = outlined(h, a, "SUNSTONE SHINDIG", if small { 30.0 } else { 38.0 }, GOLD_HI);
            let _ = t;
            h.spawn(body(a, "A 4-player board party. You + 3 CPUs.", 12.0, CREAM));
        });
        r.spawn(Node { height: Val::Px(4.0), ..default() });
        r.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(8.0), justify_content: JustifyContent::Center, ..default() }).with_children(|row| {
            for id in 0..BOARD_COUNT {
                let b = board(id);
                let sel = app.cfg.board == id;
                let rec = app.saved.best_sun[id];
                let mut e = row.spawn((
                    Button,
                    StyledButton { kind: if sel { ButtonKind::Gold } else { ButtonKind::Blue } },
                    Act(UiCmd::PickBoard(id)),
                    Node { width: Val::Px(((win.x - 60.0) / 3.0).min(270.0)), height: Val::Px(if small { 98.0 } else { 118.0 }), padding: UiRect::all(Val::Px(8.0)), border: UiRect::all(Val::Px(3.0)), flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() },
                    BackgroundColor(if sel { GOLD } else { NAVY_2 }),
                    BorderColor(if sel { GOLD_HI } else { NAVY }),
                    BorderRadius::all(Val::Px(14.0)),
                    Outline { width: Val::Px(0.0), offset: Val::Px(3.0), color: Color::WHITE },
                ));
                if sel {
                    e.insert(Selected);
                }
                e.with_children(|c| {
                    let ink = if sel { BROWN } else { Color::WHITE };
                    c.spawn((Text::new(b.name), font(&a.display, 17.0), TextColor(ink)));
                    c.spawn((Text::new(clean(b.blurb.to_string())), font(&a.body, 11.0), TextColor(if sel { BROWN } else { CREAM })));
                    if rec > 0 {
                        c.spawn((Text::new(format!("Best: {rec} Sunstones")), font(&a.body, 10.0), TextColor(if sel { BROWN } else { GOLD_HI })));
                    }
                });
            }
        });
        r.spawn(Node { height: Val::Px(6.0), ..default() });
        r.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(10.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_wrap: FlexWrap::Wrap, row_gap: Val::Px(4.0), ..default() }).with_children(|row| {
            row.spawn(body(a, "Rounds", 13.0, CREAM));
            for t in [10u32, 12, 15] {
                let sel = app.cfg.turns == t;
                let mut b = act_button(row, a, ButtonKind::Blue, 56.0, 44.0, 17.0, &t.to_string(), UiCmd::PickTurns(t));
                if sel {
                    b.insert(Selected);
                }
            }
            row.spawn(Node { width: Val::Px(16.0), ..default() });
            row.spawn(body(a, "CPU", 13.0, CREAM));
            for l in Level::ALL {
                let sel = app.level == l;
                let mut b = act_button(row, a, ButtonKind::Blue, 82.0, 44.0, 15.0, l.label(), UiCmd::PickLevel(l));
                if sel {
                    b.insert(Selected);
                }
            }
        });
        r.spawn(Node { height: Val::Px(6.0), ..default() });
        r.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(10.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|row| {
            act_button(row, a, ButtonKind::Gold, 190.0, 54.0, 24.0, "PLAY", UiCmd::Start);
            act_button(row, a, ButtonKind::Green, 150.0, 54.0, 16.0, "Mini-games", UiCmd::OpenMinis);
            act_button(row, a, ButtonKind::Chip, 90.0, 54.0, 14.0, "Leave", UiCmd::Back);
        });
        if app.saved.games > 0 {
            r.spawn(Node { justify_content: JustifyContent::Center, margin: UiRect::top(Val::Px(4.0)), ..default() }).with_children(|c| {
                c.spawn(body(a, &format!("Games played: {}   Wins: {}", app.saved.games, app.saved.wins), 11.0, CREAM));
            });
        }
    });
    root
}

fn build_minis(commands: &mut Commands, a: &UiAssets, area: &SafeArea, app: &PartyApp, win: Vec2) -> Entity {
    let root = commands.spawn((XpEntity, ScreenRoot, GlobalZIndex(Z), full(area), BackgroundColor(Color::srgba(0.02, 0.04, 0.16, 0.7)))).id();
    commands.entity(root).with_children(|r| {
        r.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, margin: UiRect::right(Val::Px(PAUSE_W)), ..default() }).with_children(|h| {
            outlined(h, a, "MINI-GAMES", 26.0, GOLD_HI);
            h.spawn(body(a, "Tap one to practise it against 3 CPUs.", 12.0, CREAM));
            act_button(h, a, ButtonKind::Chip, 90.0, 44.0, 14.0, "Back", UiCmd::Back);
        });
        let cols = if win.x > 700.0 { 5 } else { 3 };
        let w = ((win.x - area.left.max(10.0) - area.right.max(10.0) - 8.0 * (cols as f32 - 1.0)) / cols as f32).min(168.0);
        r.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(8.0), row_gap: Val::Px(6.0), margin: UiRect::top(Val::Px(6.0)), justify_content: JustifyContent::Center, ..default() }).with_children(|g| {
            for m in ALL_MINIS {
                let info = m.info();
                let rec = app.saved.records.iter().find(|r| r.0 == info.key).map(|r| fmt_score(m, r.1));
                let ctl = match info.control {
                    Control::Stick => "stick",
                    Control::StickA => "stick + A",
                    Control::Tap => "tap",
                    Control::Tiles => "tiles",
                };
                let label = match rec {
                    Some(r) => format!("{}\n{ctl}  best {r}", info.name),
                    None => format!("{}\n{ctl}", info.name),
                };
                act_button(g, a, ButtonKind::Green, w, if win.y < 460.0 { 56.0 } else { 62.0 }, 12.0, &label, UiCmd::Practice(m));
            }
        });
    });
    root
}

fn build_board(commands: &mut Commands, a: &UiAssets, area: &SafeArea, app: &PartyApp, win: Vec2) -> Entity {
    let root = commands.spawn((XpEntity, ScreenRoot, GlobalZIndex(Z), full(area))).id();
    let Some(g) = app.game.as_ref() else { return root };
    commands.entity(root).with_children(|r| {
        r.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|top| {
            chips_row(top, a, app, win, area, false);
        });
        r.spawn(Node { flex_direction: FlexDirection::Row, margin: UiRect::top(Val::Px(3.0)), column_gap: Val::Px(6.0), ..default() }).with_children(|row| {
            let id = row.spawn((RoundText, body(a, "", 12.0, CREAM))).id();
            let _ = id;
        });
        // Fill, then the action panel at the bottom.
        r.spawn(Node { flex_grow: 1.0, ..default() });
        r.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(4.0), ..default() }).with_children(|bot| {
            let busy = app.busy();
            let me = g.cur;
            let human = !g.is_cpu(me);
            if busy {
                return;
            }
            if let Some(slot) = app.pending_item {
                bot.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), align_items: AlignItems::Center, padding: UiRect::all(Val::Px(6.0)), ..default() }).insert(BackgroundColor(panel_bg())).insert(BorderRadius::all(Val::Px(14.0))).with_children(|row| {
                    row.spawn(body(a, "Steal from:", 13.0, CREAM));
                    for q in (0..4).filter(|&q| q != me) {
                        act_button(row, a, ButtonKind::Red, 112.0, 46.0, 13.0, &format!("{} ({})", g.players[q].name, g.players[q].coins), UiCmd::ItemTarget(slot, q));
                    }
                    act_button(row, a, ButtonKind::Chip, 70.0, 46.0, 13.0, "Cancel", UiCmd::CancelItem);
                });
                return;
            }
            match &g.phase {
                Phase::TurnStart if human => {
                    bot.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                        let rolling = app.dice.as_ref().is_some_and(|d| d.rolling);
                        if !rolling {
                            for (i, it) in g.players[me].items.iter().enumerate() {
                                if g.used_item {
                                    break;
                                }
                                act_button(row, a, ButtonKind::Green, 118.0, 48.0, 13.0, it.label(), UiCmd::UseItem(i));
                            }
                        }
                        let mut b = act_button(row, a, ButtonKind::Gold, 210.0, 62.0, 26.0, if rolling { "STOP!" } else { "ROLL!" }, if rolling { UiCmd::RollStop } else { UiCmd::RollStart });
                        b.insert(RollLabel);
                        if !rolling {
                            act_button(row, a, ButtonKind::Chip, 64.0, 48.0, 13.0, if app.overview { "Follow" } else { "Map" }, UiCmd::Overview);
                        }
                    });
                    if !app.dice.as_ref().is_some_and(|d| d.rolling) && g.players[me].items.len() > 0 && !g.used_item {
                        bot.spawn(body(a, "Items: one per turn, before you roll.", 10.0, CREAM));
                    }
                }
                Phase::Branch(opts) if human => {
                    pill(bot, a, "Which way? Pick A or B (the glowing spaces).", 12.0);
                    bot.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
                        for (i, &o) in opts.iter().enumerate() {
                            let k = g.space_kind(o);
                            let dist = g.keeper_distance(o);
                            let letter = (b'A' + i as u8) as char;
                            act_button(row, a, ButtonKind::Blue, 200.0, 54.0, 14.0, &format!("{letter}: {}  ({} to Keeper)", k.label(), dist), UiCmd::Game(Cmd::Choose(o)));
                        }
                    });
                }
                Phase::KeeperOffer if human => {
                    bot.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(8.0), align_items: AlignItems::Center, padding: UiRect::all(Val::Px(6.0)), ..default() }).insert(BackgroundColor(panel_bg())).insert(BorderRadius::all(Val::Px(14.0))).with_children(|row| {
                        row.spawn(body(a, &format!("The Keeper sells a Sunstone for {SUNSTONE_COST} coins."), 13.0, Color::WHITE));
                        act_button(row, a, ButtonKind::Gold, 130.0, 52.0, 18.0, "BUY", UiCmd::Game(Cmd::Keeper(true)));
                        act_button(row, a, ButtonKind::Chip, 110.0, 52.0, 14.0, "No thanks", UiCmd::Game(Cmd::Keeper(false)));
                    });
                }
                Phase::ShopMenu if human => {
                    bot.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(4.0), padding: UiRect::all(Val::Px(6.0)), ..default() }).insert(BackgroundColor(panel_bg())).insert(BorderRadius::all(Val::Px(14.0))).with_children(|c| {
                        c.spawn(body(a, &format!("Item shop: you have {} coins, {} of {} item slots used.", g.players[me].coins, g.players[me].items.len(), MAX_ITEMS), 12.0, CREAM));
                        c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), ..default() }).with_children(|row| {
                            for it in ITEMS {
                                let ok = g.players[me].coins >= it.price() && g.players[me].items.len() < MAX_ITEMS;
                                let label = format!("{} {}c", it.label(), it.price());
                                if ok {
                                    act_button(row, a, ButtonKind::Green, 126.0, 48.0, 12.0, &label, UiCmd::Game(Cmd::ShopBuy(it)));
                                } else {
                                    dead_button(row, a, 126.0, 48.0, 12.0, &label);
                                }
                            }
                            act_button(row, a, ButtonKind::Gold, 80.0, 48.0, 15.0, "Done", UiCmd::Game(Cmd::ShopLeave));
                        });
                    });
                }
                Phase::DuelPick if human => {
                    bot.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), align_items: AlignItems::Center, padding: UiRect::all(Val::Px(6.0)), ..default() }).insert(BackgroundColor(panel_bg())).insert(BorderRadius::all(Val::Px(14.0))).with_children(|row| {
                        row.spawn(body(a, "Duel! Challenge a rival (winner takes up to 5 coins):", 12.0, Color::WHITE));
                        for q in (0..4).filter(|&q| q != me) {
                            act_button(row, a, ButtonKind::Red, 100.0, 48.0, 13.0, &format!("{} ({})", g.players[q].name, g.players[q].coins), UiCmd::Game(Cmd::DuelPick(q)));
                        }
                    });
                }
                Phase::MiniResult(rep) => {
                    bot.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(2.0), padding: UiRect::all(Val::Px(8.0)), min_width: Val::Px(360.0), ..default() }).insert(BackgroundColor(panel_bg())).insert(BorderRadius::all(Val::Px(14.0))).with_children(|c| {
                        c.spawn(disp(a, &format!("{}: {}", rep.kind.info().name, rep.format.label()), 15.0, GOLD_HI));
                        let mut order: Vec<usize> = (0..4).filter(|&p| rep.active[p]).collect();
                        order.sort_by_key(|&p| (rep.ranks[p], p));
                        for p in order {
                            c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(10.0), ..default() }).with_children(|r| {
                                r.spawn(body(a, &format!("#{}", rep.ranks[p]), 13.0, CREAM));
                                r.spawn(body(a, g.players[p].name, 13.0, col(PLAYER_COLORS[p])));
                                r.spawn(body(a, &fmt_score(rep.kind, rep.scores[p]), 12.0, Color::WHITE));
                                let cd = rep.coins[p];
                                r.spawn(body(a, &if cd != 0 { format!("{cd:+} coins") } else { String::new() }, 12.0, if cd > 0 { GOLD_HI } else { RED_HI }));
                                if rep.won[p] {
                                    r.spawn(body(a, "WIN", 12.0, GOLD_HI));
                                }
                            });
                        }
                        act_button(c, a, ButtonKind::Gold, 150.0, 46.0, 17.0, "Next", UiCmd::Ack);
                    });
                }
                _ => {
                    let who = if g.cur < 4 { g.players[g.cur].name } else { "" };
                    if g.waiting_for_human() {
                        return;
                    }
                    let txt = match &g.phase {
                        Phase::Over => "Game over!".to_string(),
                        _ => format!("{who} is thinking..."),
                    };
                    pill(bot, a, &txt, 12.0);
                }
            }
        });
    });
    root
}

fn build_mini(commands: &mut Commands, a: &UiAssets, area: &SafeArea, app: &PartyApp, win: Vec2) -> Entity {
    let root = commands.spawn((XpEntity, ScreenRoot, GlobalZIndex(Z), full(area))).id();
    let Some(run) = app.mini.as_ref() else { return root };
    let info = run.kind.info();
    commands.entity(root).with_children(|r| {
        r.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|top| {
            chips_row(top, a, app, win, area, true);
        });
        r.spawn(Node { flex_direction: FlexDirection::Row, margin: UiRect::top(Val::Px(3.0)), column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
            pill(row, a, info.name, 12.0);
            row.spawn((TimerText, body(a, "", 14.0, GOLD_HI)));
            if let Some(ctx) = &run.ctx {
                row.spawn(body(a, ctx.format.label(), 11.0, CREAM));
            }
        });
        r.spawn(Node { flex_grow: 1.0, ..default() });
        match run.stage {
            MStage::Intro => {
                r.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|c| {
                    c.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(6.0), padding: UiRect::all(Val::Px(12.0)), max_width: Val::Px(560.0), border: UiRect::all(Val::Px(3.0)), ..default() })
                        .insert(BackgroundColor(Color::srgba(0.03, 0.06, 0.2, 0.93)))
                        .insert(BorderColor(GOLD))
                        .insert(BorderRadius::all(Val::Px(18.0)))
                        .with_children(|card| {
                            card.spawn(disp(a, info.name, 26.0, GOLD_HI));
                            if let Some(ctx) = &run.ctx {
                                let who: Vec<String> = (0..4).filter(|&p| ctx.active[p]).map(|p| app.game.as_ref().map_or(PLAYER_NAMES[p], |g| g.players[p].name).to_string()).collect();
                                let teams = if matches!(ctx.format, Format::TwoVsTwo | Format::OneVsThree) {
                                    let t0: Vec<&str> = (0..4).filter(|&p| ctx.teams[p] == 0).map(|p| PLAYER_NAMES[p]).collect();
                                    let t1: Vec<&str> = (0..4).filter(|&p| ctx.teams[p] == 1).map(|p| PLAYER_NAMES[p]).collect();
                                    format!("{}: {} vs {}", ctx.format.label(), t0.join(" + "), t1.join(" + "))
                                } else {
                                    format!("{}: {}", ctx.format.label(), who.join(", "))
                                };
                                card.spawn(body(a, &teams, 12.0, CREAM));
                            }
                            card.spawn(Node { max_width: Val::Px(520.0), ..default() }).with_children(|t| {
                                t.spawn(body(a, info.blurb, 15.0, Color::WHITE));
                            });
                            let touch = true;
                            let ctl = match info.control {
                                Control::Stick => "Controls: drag the stick (or WASD / arrows / left stick).",
                                Control::StickA => "Controls: drag the stick to steer, tap A (or Space) to dash.",
                                Control::Tap => "Controls: tap the big button (or Space / Enter / A).",
                                Control::Tiles => "Controls: tap the tiles (or 1-9, or arrows + Space).",
                            };
                            let _ = touch;
                            card.spawn(body(a, ctl, 12.0, GOLD_HI));
                            card.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(10.0), ..default() }).with_children(|row| {
                                act_button(row, a, ButtonKind::Gold, 190.0, 54.0, 24.0, "START", UiCmd::MiniGo);
                                if run.practice {
                                    act_button(row, a, ButtonKind::Chip, 90.0, 54.0, 14.0, "Back", UiCmd::Back);
                                }
                            });
                        });
                });
            }
            MStage::Count => {}
            MStage::Play => {
                if info.control == Control::Tap && run.human[0] {
                    r.spawn(Node { justify_content: JustifyContent::Center, margin: UiRect::bottom(Val::Px(2.0)), ..default() }).with_children(|c| {
                        let w = (win.x * 0.56).clamp(220.0, 520.0);
                        act_button(c, a, ButtonKind::Gold, w, 68.0, 30.0, match run.kind {
                            Mini::BarrelHop => "JUMP!",
                            Mini::QuickDraw => "TAP!",
                            Mini::BlockStack => "DROP!",
                            Mini::PowerMeter => "STOP!",
                            _ => "TAP!",
                        }, UiCmd::Tap);
                    });
                }
            }
            MStage::Done => {
                r.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|c| {
                    c.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(3.0), padding: UiRect::all(Val::Px(12.0)), min_width: Val::Px(360.0), border: UiRect::all(Val::Px(3.0)), ..default() })
                        .insert(BackgroundColor(Color::srgba(0.03, 0.06, 0.2, 0.93)))
                        .insert(BorderColor(GOLD))
                        .insert(BorderRadius::all(Val::Px(18.0)))
                        .with_children(|card| {
                            card.spawn(disp(a, "Finished!", 24.0, GOLD_HI));
                            let ranks = ranks(&run.final_scores, &run.session.active);
                            let mut order: Vec<usize> = (0..4).filter(|&p| run.session.active[p]).collect();
                            order.sort_by_key(|&p| (ranks[p], p));
                            for p in order {
                                card.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(10.0), ..default() }).with_children(|r| {
                                    r.spawn(body(a, &format!("#{}", ranks[p]), 14.0, CREAM));
                                    r.spawn(body(a, app.game.as_ref().map_or(PLAYER_NAMES[p], |g| g.players[p].name), 14.0, col(PLAYER_COLORS[p])));
                                    r.spawn(body(a, &fmt_score(run.kind, run.final_scores[p]), 13.0, Color::WHITE));
                                    if ranks[p] == 1 {
                                        r.spawn(body(a, "WIN", 13.0, GOLD_HI));
                                    }
                                });
                            }
                            if run.new_record {
                                card.spawn(body(a, "New personal best!", 13.0, GOLD_HI));
                            }
                            card.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(10.0), margin: UiRect::top(Val::Px(4.0)), ..default() }).with_children(|row| {
                                if run.practice {
                                    act_button(row, a, ButtonKind::Gold, 130.0, 48.0, 16.0, "Again", UiCmd::MiniAgain);
                                    act_button(row, a, ButtonKind::Blue, 130.0, 48.0, 16.0, "List", UiCmd::MiniFinish);
                                } else {
                                    act_button(row, a, ButtonKind::Gold, 170.0, 48.0, 18.0, "Continue", UiCmd::MiniFinish);
                                }
                            });
                        });
                });
            }
        }
    });
    root
}

fn build_results(commands: &mut Commands, a: &UiAssets, area: &SafeArea, app: &PartyApp, _win: Vec2) -> Entity {
    let root = commands.spawn((XpEntity, ScreenRoot, GlobalZIndex(Z), full(area))).id();
    let Some(g) = app.game.as_ref() else { return root };
    let stage = (((app.t - app.results_t0) / 1.6).max(0.0) as usize).min(5);
    commands.entity(root).with_children(|r| {
        r.spawn(Node { margin: UiRect::right(Val::Px(PAUSE_W)), ..default() }).with_children(|h| {
            outlined(h, a, if stage >= 4 { "FINAL RESULTS" } else { "BONUS AWARDS" }, 26.0, GOLD_HI);
        });
        r.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), margin: UiRect::top(Val::Px(6.0)), max_width: Val::Px(360.0), ..default() }).with_children(|c| {
            for (i, aw) in g.awards.iter().enumerate() {
                if stage <= i {
                    break;
                }
                c.spawn((Node { flex_direction: FlexDirection::Column, padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(panel_bg()), BorderColor(GOLD), BorderRadius::all(Val::Px(12.0)))).with_children(|b| {
                    b.spawn(disp(a, aw.name, 16.0, GOLD_HI));
                    let who = if aw.winners.is_empty() { "nobody".to_string() } else { aw.winners.iter().map(|&p| g.players[p].name).collect::<Vec<_>>().join(" & ") };
                    b.spawn(body(a, &format!("{}: {} ({})", aw.desc, who, aw.value), 11.0, Color::WHITE));
                    if !aw.winners.is_empty() {
                        b.spawn(body(a, "+1 Sunstone", 11.0, GOLD_HI));
                    }
                });
            }
            if stage >= 4 {
                let order = g.standings();
                let winners = g.winners();
                c.spawn((Node { flex_direction: FlexDirection::Column, padding: UiRect::all(Val::Px(8.0)), border: UiRect::all(Val::Px(3.0)), row_gap: Val::Px(2.0), ..default() }, BackgroundColor(Color::srgba(0.03, 0.06, 0.2, 0.92)), BorderColor(GOLD), BorderRadius::all(Val::Px(14.0)))).with_children(|b| {
                    let headline = if winners.len() > 1 { format!("{} share the win!", winners.iter().map(|&p| g.players[p].name).collect::<Vec<_>>().join(" & ")) } else { format!("{} wins!", g.players[winners[0]].name) };
                    b.spawn(disp(a, &headline, 20.0, GOLD_HI));
                    for (i, &p) in order.iter().enumerate() {
                        b.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
                            row.spawn(body(a, &format!("{}.", i + 1), 13.0, CREAM));
                            row.spawn(body(a, g.players[p].name, 13.0, col(PLAYER_COLORS[p])));
                            row.spawn(body(a, &format!("{} Sunstones", g.players[p].sun), 13.0, Color::WHITE));
                            row.spawn(body(a, &format!("{} coins", g.players[p].coins), 12.0, GOLD_HI));
                            row.spawn(body(a, &format!("{} mini wins", g.players[p].wins), 11.0, CREAM));
                        });
                    }
                });
            }
        });
        r.spawn(Node { flex_grow: 1.0, ..default() });
        r.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(10.0), ..default() }).with_children(|row| {
            if stage < 4 {
                act_button(row, a, ButtonKind::Blue, 150.0, 48.0, 16.0, "Skip", UiCmd::Next);
            } else {
                act_button(row, a, ButtonKind::Gold, 170.0, 52.0, 18.0, "Play again", UiCmd::Start);
                act_button(row, a, ButtonKind::Blue, 120.0, 52.0, 16.0, "Menu", UiCmd::Menu);
            }
        });
    });
    root
}

// ---------------------------------------------------------------------------------------------
// Per-frame updates
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hud_update(
    app: Res<PartyApp>,
    mut coins: Query<(&ChipCoins, &mut Text), (Without<ChipSun>, Without<ChipItems>, Without<ChipTag>, Without<TimerText>, Without<RoundText>)>,
    mut suns: Query<(&ChipSun, &mut Text), (Without<ChipCoins>, Without<ChipItems>, Without<ChipTag>, Without<TimerText>, Without<RoundText>)>,
    mut items: Query<(&ChipItems, &mut Text), (Without<ChipCoins>, Without<ChipSun>, Without<ChipTag>, Without<TimerText>, Without<RoundText>)>,
    mut tags: Query<(&ChipTag, &mut Text), (Without<ChipCoins>, Without<ChipSun>, Without<ChipItems>, Without<TimerText>, Without<RoundText>)>,
    mut timer: Query<&mut Text, (With<TimerText>, Without<ChipCoins>, Without<ChipSun>, Without<ChipItems>, Without<ChipTag>, Without<RoundText>)>,
    mut round: Query<&mut Text, (With<RoundText>, Without<ChipCoins>, Without<ChipSun>, Without<ChipItems>, Without<ChipTag>, Without<TimerText>)>,
    mut frames: Query<(&ChipFrame, &mut BorderColor)>,
) {
    let g = app.game.as_ref();
    for (c, mut t) in &mut coins {
        let v = g.map_or(0, |g| g.players[c.0].coins);
        let s = v.to_string();
        if t.0 != s {
            t.0 = s;
        }
    }
    for (c, mut t) in &mut suns {
        let v = g.map_or(0, |g| g.players[c.0].sun);
        let s = v.to_string();
        if t.0 != s {
            t.0 = s;
        }
    }
    for (c, mut t) in &mut items {
        let s = g.map_or(String::new(), |g| g.players[c.0].items.iter().map(|i| i.glyph()).collect::<Vec<_>>().join(" "));
        if t.0 != s {
            t.0 = s;
        }
    }
    for (c, mut t) in &mut tags {
        let s = app.mini.as_ref().map_or(String::new(), |m| {
            let s = m.scene.tags[c.0].clone();
            match &m.ctx {
                Some(ctx) if ctx.active[c.0] && matches!(ctx.format, Format::TwoVsTwo | Format::OneVsThree) => format!("T{} {}", ctx.teams[c.0] + 1, s),
                _ => s,
            }
        });
        if t.0 != s {
            t.0 = s;
        }
    }
    for mut t in &mut timer {
        let s = app.mini.as_ref().map_or(String::new(), |m| match m.stage {
            MStage::Play => format!("{:.0}s", m.session.game.time_left().ceil()),
            _ => String::new(),
        });
        if t.0 != s {
            t.0 = s;
        }
    }
    for mut t in &mut round {
        let s = g.map_or(String::new(), |g| format!("Round {} of {}  ·  {}  ·  Sunstone {}c", g.round.min(g.cfg.turns), g.cfg.turns, g.board.name, SUNSTONE_COST));
        let s = clean(s.replace('·', "-"));
        if t.0 != s {
            t.0 = s;
        }
    }
    for (f, mut b) in &mut frames {
        let active = match app.screen {
            Screen::Board => g.is_some_and(|g| g.cur == f.0 && !matches!(g.phase, Phase::Over)),
            Screen::Mini => app.mini.as_ref().is_some_and(|m| m.human[f.0]),
            _ => false,
        };
        let want = if active { GOLD } else { NAVY };
        if b.0 != want {
            b.0 = want;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn space_labels(app: Res<PartyApp>, cameras: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>, mut labels: Query<(&SpaceLabel, &mut Node, &mut Text, &mut Visibility, &mut TextColor, &mut TextFont)>) {
    let show = matches!(app.screen, Screen::Board | Screen::Title | Screen::Minis | Screen::Results) && app.board_built.is_some();
    let Ok((camera, gt)) = cameras.single() else { return };
    let id = app.board_built.unwrap_or(0);
    let board = board(id);
    let cond = app.game.as_ref().map_or(Conditions::default(), |g| g.cond);
    let origin = crate::render::to_bevy(BOARD_O);
    let options: Vec<usize> = match app.game.as_ref().map(|g| &g.phase) {
        Some(Phase::Branch(o)) => o.clone(),
        _ => vec![],
    };
    let keeper = app.game.as_ref().map_or(usize::MAX, |g| g.keeper);
    for (l, mut node, mut text, mut vis, mut color, mut tf) in &mut labels {
        let i = l.0;
        if !show || i >= board.spaces.len() || app.screen == Screen::Results {
            *vis = Visibility::Hidden;
            continue;
        }
        let s = &board.spaces[i];
        let wp = origin + Vec3::new(s.pos[0], 0.32, s.pos[1]);
        let Ok(p) = camera.world_to_viewport(gt, wp) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let flooded = board.flooded(i, &cond);
        let kind = if flooded { Kind::Pit } else { s.kind };
        let (glyph, size, c) = if let Some(k) = options.iter().position(|&o| o == i) {
            (((b'A' + k as u8) as char).to_string(), 26.0, GOLD_HI)
        } else if kind == Kind::Keeper && i == keeper {
            ("*".to_string(), 24.0, BROWN)
        } else if kind == Kind::Keeper {
            ("+2".to_string(), 13.0, BROWN)
        } else {
            (kind.glyph().to_string(), if s.span { 12.0 } else { 15.0 }, if matches!(kind, Kind::Start | Kind::Coin | Kind::Item | Kind::Warp | Kind::Shop) { if kind == Kind::Start { BROWN } else { Color::WHITE } } else { Color::WHITE })
        };
        if text.0 != glyph {
            text.0 = glyph.clone();
        }
        if tf.font_size != size {
            tf.font_size = size;
        }
        if color.0 != c {
            color.0 = c;
        }
        let w = glyph.len() as f32 * size * 0.3;
        node.left = Val::Px(p.x - w);
        node.top = Val::Px(p.y - size * 0.55);
        *vis = Visibility::Inherited;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn overlays(
    app: Res<PartyApp>,
    area: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>,
    mut floaters: Query<(&FloaterNode, &mut Node, &mut Text, &mut TextColor, &mut Visibility), (Without<BannerText>, Without<ToastText>, Without<BigText>, Without<StageText>, Without<DiceText>)>,
    mut banner: Query<(&mut Node, &mut Text, &mut TextColor, &mut Visibility), (With<BannerText>, Without<FloaterNode>, Without<ToastText>, Without<BigText>, Without<StageText>, Without<DiceText>)>,
    mut toast: Query<(&mut Node, &mut Text, &mut Visibility), (With<ToastText>, Without<FloaterNode>, Without<BannerText>, Without<BigText>, Without<StageText>, Without<DiceText>)>,
    mut big: Query<(&mut Node, &mut Text, &mut Visibility), (With<BigText>, Without<FloaterNode>, Without<BannerText>, Without<ToastText>, Without<StageText>, Without<DiceText>)>,
    mut stage: Query<(&mut Node, &mut Text, &mut Visibility), (With<StageText>, Without<FloaterNode>, Without<BannerText>, Without<ToastText>, Without<BigText>, Without<DiceText>)>,
    mut dice: Query<(&mut Node, &mut Visibility, &Children), (With<DiceText>, Without<FloaterNode>, Without<BannerText>, Without<ToastText>, Without<BigText>, Without<StageText>)>,
    mut dice_text: Query<&mut Text, (Without<FloaterNode>, Without<BannerText>, Without<ToastText>, Without<BigText>, Without<StageText>)>,
) {
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let cam = cameras.single().ok();
    // Floaters over pieces.
    for (f, mut node, mut text, mut color, mut vis) in &mut floaters {
        match (app.floaters.get(f.0), cam) {
            (Some(fl), Some((camera, gt))) => {
                let Some(pc) = app.pieces.get(fl.p) else {
                    *vis = Visibility::Hidden;
                    continue;
                };
                let age = (app.t - fl.t0) as f32;
                let wp = crate::render::to_bevy(pc.mario.pos()) + Vec3::new(0.0, 1.5 + age * 0.9, 0.0);
                if let Ok(p) = camera.world_to_viewport(gt, wp) {
                    node.left = Val::Px(p.x - 16.0);
                    node.top = Val::Px(p.y - 12.0);
                    if text.0 != fl.text {
                        text.0 = fl.text.clone();
                    }
                    color.0 = hex(fl.color).with_alpha((1.4 - age).clamp(0.0, 1.0));
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    for (mut node, mut text, mut color, mut vis) in &mut banner {
        match &app.banner {
            Some((s, t0, c)) => {
                let age = (app.t - t0) as f32;
                if text.0 != *s {
                    text.0 = s.clone();
                }
                color.0 = hex(*c).with_alpha(((1.6 - age) * 2.5).clamp(0.0, 1.0));
                let w = s.len() as f32 * 11.5;
                node.left = Val::Px((win.x - w) / 2.0);
                node.top = Val::Px(win.y * 0.30 - age.min(0.4) * 20.0);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
    for (mut node, mut text, mut vis) in &mut toast {
        match (&app.toast, app.screen) {
            (Some((s, _)), Screen::Board | Screen::Mini) => {
                if text.0 != *s {
                    text.0 = s.clone();
                }
                let w = s.len() as f32 * 3.6;
                node.left = Val::Px((win.x - w * 2.0) / 2.0);
                node.top = Val::Px(win.y - area.bottom.max(8.0) - 112.0);
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    // Big countdown.
    for (mut node, mut text, mut vis) in &mut big {
        let s = match app.mini.as_ref() {
            Some(m) if app.screen == Screen::Mini && m.stage == MStage::Count => {
                let left = 2.4 - m.t;
                if left > 1.6 {
                    "3"
                } else if left > 0.8 {
                    "2"
                } else if left > 0.0 {
                    "1"
                } else {
                    "GO!"
                }
            }
            Some(m) if app.screen == Screen::Mini && m.stage == MStage::Play && m.t < 0.6 => "GO!",
            _ => "",
        };
        if s.is_empty() {
            *vis = Visibility::Hidden;
        } else {
            if text.0 != s {
                text.0 = s.to_string();
            }
            node.left = Val::Px(win.x / 2.0 - s.len() as f32 * 26.0);
            node.top = Val::Px(win.y * 0.5 - 70.0);
            *vis = Visibility::Inherited;
        }
    }
    // Stage banner text inside mini-games ("Watch!", "TAP NOW!").
    for (mut node, mut text, mut vis) in &mut stage {
        match app.mini.as_ref() {
            Some(m) if app.screen == Screen::Mini && !m.scene.banner.is_empty() && m.stage != MStage::Done => {
                if text.0 != m.scene.banner {
                    text.0 = m.scene.banner.clone();
                }
                node.left = Val::Px(win.x / 2.0 - m.scene.banner.len() as f32 * 7.5);
                node.top = Val::Px(win.y * 0.17);
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    // The die.
    for (mut node, mut vis, children) in &mut dice {
        match (&app.dice, app.screen) {
            (Some(d), Screen::Board) => {
                node.left = Val::Px(win.x / 2.0 - 46.0);
                node.top = Val::Px(win.y * 0.30);
                *vis = Visibility::Inherited;
                let face = super::flow::dice_face(d, app.t);
                for c in children.iter() {
                    if let Ok(mut t) = dice_text.get_mut(c) {
                        let s = face.to_string();
                        if t.0 != s {
                            t.0 = s;
                        }
                    }
                }
            }
            _ => *vis = Visibility::Hidden,
        }
    }
}

#[allow(clippy::type_complexity)]
fn tiles(app: Res<PartyApp>, area: Res<SafeArea>, windows: Query<&Window, With<PrimaryWindow>>, mut btns: Query<(&TileBtn, &mut Node, &mut BackgroundColor, &mut Visibility, &mut Outline, &Children)>, mut labels: Query<&mut Text, With<TileLabel>>) {
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let run = app.mini.as_ref().filter(|m| app.screen == Screen::Mini && m.stage == MStage::Play);
    let tap = run.is_some_and(|m| m.kind.info().control == Control::Tap);
    let rect_l = area.left.max(10.0) + 10.0;
    let rect_r = win.x - area.right.max(10.0) - 10.0;
    let rect_t = 72.0 + area.top.max(6.0);
    let rect_b = win.y - area.bottom.max(8.0) - if tap { 84.0 } else { 14.0 };
    for (b, mut node, mut bg, mut vis, mut outline, children) in &mut btns {
        let tile = run.and_then(|m| m.scene.tiles.get(b.0));
        match tile {
            Some(t) => {
                node.left = Val::Px(rect_l + t.x * (rect_r - rect_l));
                node.top = Val::Px(rect_t + t.y * (rect_b - rect_t));
                node.width = Val::Px(t.w * (rect_r - rect_l));
                node.height = Val::Px((t.h * (rect_b - rect_t)).max(46.0));
                bg.0 = hex(t.col);
                *vis = Visibility::Inherited;
                let focus = run.is_some_and(|m| m.tile_focus % m.scene.tiles.len().max(1) == b.0 && m.human[0]);
                outline.width = Val::Px(if focus { 3.0 } else { 0.0 });
                for c in children.iter() {
                    if let Ok(mut l) = labels.get_mut(c) {
                        if l.0 != t.text {
                            l.0 = t.text.clone();
                        }
                    }
                }
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn button_input(
    mut app: ResMut<PartyApp>,
    mut ui: ResMut<PartyUi>,
    pressed: Query<(&Interaction, &Act), Changed<Interaction>>,
    tiles: Query<(&Interaction, &TileBtn, &Visibility), Changed<Interaction>>,
    all: Query<(Entity, &Act)>,
    mut outlines: Query<(Entity, &mut Outline), (With<Act>, Without<TileBtn>)>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    state: Res<AppState>,
) {
    if *state != AppState::Playing {
        return;
    }
    for (i, act) in &pressed {
        if *i == Interaction::Pressed {
            app.cmds.push(act.0.clone());
        }
    }
    for (i, t, v) in &tiles {
        if *i == Interaction::Pressed && *v != Visibility::Hidden {
            if let Some(m) = app.mini.as_ref() {
                if let Some(tile) = m.scene.tiles.get(t.0) {
                    let id = tile.id;
                    app.cmds.push(UiCmd::Tile(id));
                }
            }
        }
    }
    // Keyboard / gamepad focus over the Act buttons.
    if ui.focus_list.is_empty() {
        ui.focus_list = all.iter().map(|(e, _)| e).collect();
    }
    let live: Vec<Entity> = ui.focus_list.iter().copied().filter(|e| all.get(*e).is_ok()).collect();
    let n = live.len();
    // During a stick / tile game the keys belong to the game.
    let playing = app.screen == Screen::Mini && app.mini.as_ref().is_some_and(|m| m.stage == MStage::Play);
    if n == 0 || playing {
        return;
    }
    let mut step = 0i32;
    let mut go = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space);
    if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::Tab) {
        step = 1;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::ArrowUp) {
        step = -1;
    }
    for pad in &pads {
        if pad.just_pressed(GamepadButton::DPadRight) || pad.just_pressed(GamepadButton::DPadDown) {
            step = 1;
        }
        if pad.just_pressed(GamepadButton::DPadLeft) || pad.just_pressed(GamepadButton::DPadUp) {
            step = -1;
        }
        go |= pad.just_pressed(GamepadButton::South);
        if pad.just_pressed(GamepadButton::East) {
            app.cmds.push(UiCmd::Back);
        }
    }
    if step != 0 || go {
        ui.nav = true;
    }
    if step != 0 {
        ui.focus = ((ui.focus as i32 + step).rem_euclid(n as i32)) as usize;
    }
    ui.focus = ui.focus.min(n - 1);
    if go {
        if let Ok((_, act)) = all.get(live[ui.focus]) {
            app.cmds.push(act.0.clone());
        }
    }
    let f = live[ui.focus];
    for (e, mut o) in outlines.iter_mut() {
        let on = e == f && ui.nav;
        let w = Val::Px(if on { 3.0 } else { 0.0 });
        if o.width != w {
            o.width = w;
            o.color = GOLD_HI;
        }
    }
}
