//! Star Deck screens on `bevy_ui`: the combat table (hand, momentum meter, foe plates with telegraphed
//! intents, piles, potions), the map, rewards, shop, campfire, events, treasure, the deck/pile viewers and the
//! run summary. Phone-first: logical 874x402 with safe-area insets, 100x136 hand cards that grow when selected,
//! 44px tap targets. Mouse, touch (tap or drag a card), keyboard and gamepad all drive the same [`Cmd`]s.
use super::deck::*;
use super::deck_core::*;
use super::deck_run::{NodeKind, Screen, BOSS_ROW};
use super::XpEntity;
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::shell::AppState;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

pub const CARD_W: f32 = 100.0;
pub const CARD_H: f32 = 136.0;
const Z: i32 = 5;

// ---------------------------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------------------------

fn kind_color(k: Kind) -> Color {
    match k {
        Kind::Attack => hex(0xb83a2a),
        Kind::Skill => hex(0x2c63c4),
        Kind::Power => hex(0xc8921a),
        Kind::Curse => hex(0x5a4a66),
    }
}

fn rarity_color(r: Rarity) -> Color {
    match r {
        Rarity::Starter | Rarity::Common => hex(0xdfe6f5),
        Rarity::Uncommon => hex(0x66b0ff),
        Rarity::Rare => hex(0xffcc33),
        Rarity::Curse => hex(0x8a6a9a),
    }
}

fn glyph(m: Move) -> &'static str {
    match m {
        Move::Jump => "^",
        Move::Dive => ">>",
        Move::Pound => "v",
        Move::Strike => "X",
        Move::Spin => "@",
        Move::Brace => "#",
        Move::Trick => "*",
    }
}

fn move_color(m: Move) -> Color {
    match m {
        Move::Jump => hex(0x5cd36b),
        Move::Dive => hex(0x4cc3ff),
        Move::Pound => hex(0xff8f4a),
        Move::Strike => hex(0xff5a5a),
        Move::Spin => hex(0xd36cff),
        Move::Brace => hex(0x9fb4ff),
        Move::Trick => hex(0xffe27a),
    }
}

fn intent_color(i: IntentIcon) -> Color {
    match i {
        IntentIcon::Attack => hex(0xd9382c),
        IntentIcon::Guard => hex(0x3a78d8),
        IntentIcon::Buff => hex(0xd9a21b),
        IntentIcon::Debuff => hex(0x8f4bd0),
        IntentIcon::Warn => hex(0xe07a1a),
    }
}

fn panel_bg() -> Color {
    Color::srgba(0.03, 0.06, 0.2, 0.86)
}

fn body(a: &UiAssets, s: &str, size: f32, color: Color) -> impl Bundle {
    (Text::new(clean(s.to_string())), font(&a.body, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY })
}

fn disp_text(a: &UiAssets, s: &str, size: f32, color: Color) -> impl Bundle {
    (Text::new(clean(s.to_string())), font(&a.display, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY })
}

fn abs(left: f32, top: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), ..default() }
}

fn full(area: &SafeArea) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        padding: UiRect { left: Val::Px(area.left), right: Val::Px(area.right), top: Val::Px(area.top), bottom: Val::Px(area.bottom) },
        flex_direction: FlexDirection::Column,
        ..default()
    }
}

// ---------------------------------------------------------------------------------------------
// Components and UI state
// ---------------------------------------------------------------------------------------------

/// A button that sends a command.
#[derive(Component, Clone, Copy)]
pub struct Act(pub Cmd);

#[derive(Component)]
struct ScreenRoot;
#[derive(Component)]
struct ScrollBox;
#[derive(Component)]
struct HpFill;
#[derive(Component)]
struct HpText;
#[derive(Component)]
struct ShellText;
#[derive(Component)]
struct CoinsText;
#[derive(Component)]
struct TurnText;
#[derive(Component)]
struct HintText;
#[derive(Component)]
struct MomPip(usize);
#[derive(Component)]
struct MomText;
#[derive(Component)]
struct ChainText;
#[derive(Component)]
struct PileCount(Pile);
#[derive(Component)]
struct PotionSlot(usize);
#[derive(Component)]
struct PotionLabel(usize);
#[derive(Component)]
struct StatusRow;
#[derive(Component)]
struct FoePlate(usize);
#[derive(Component)]
struct FoeIntentBadge(usize);
#[derive(Component)]
struct FoeIntentText(usize);
#[derive(Component)]
struct FoeIntentName(usize);
#[derive(Component)]
struct FoeHpFill(usize);
#[derive(Component)]
struct FoeHpText(usize);
#[derive(Component)]
struct FoeShellText(usize);
#[derive(Component)]
struct FoeStatusRow(usize, u64);
#[derive(Component)]
struct FoeBorder(usize);
/// The pulsing ring drawn around the foe a keyboard / gamepad aim has picked.
#[derive(Component)]
struct AimRing;
#[derive(Component)]
struct FloaterNode(usize);
#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct ToastText;
#[derive(Component)]
struct CardEnt {
    uid: u32,
    card: Card,
    pos: Vec2,
    scale: f32,
    rot: f32,
    leaving: Option<(Leave, f64)>,
    born: f64,
}
#[derive(Component)]
struct CardDim(u32);
#[derive(Component)]
struct CardLink(u32);

#[derive(Clone, Copy)]
struct DragInfo {
    uid: u32,
    idx: usize,
    start: Vec2,
    pos: Vec2,
    moved: bool,
}

#[derive(Resource, Default)]
pub struct DeckUi {
    key: (u32, u8, u8),
    root: Option<Entity>,
    hand_root: Option<Entity>,
    plates: Vec<Entity>,
    floaters: Vec<Entity>,
    banner: Option<Entity>,
    toast: Option<Entity>,
    drag: Option<DragInfo>,
    /// Focusable buttons of the current screen, in reading order.
    focus_list: Vec<Entity>,
    focus: usize,
    scroll_drag: Option<(f32, f32)>,
    sw: Vec2,
    nav: bool,
    modal: Option<Entity>,
    modal_key: (u32, u8),
}

fn screen_id(s: Screen) -> u8 {
    s as u8
}

fn modal_id(m: Modal) -> u8 {
    match m {
        Modal::None => 0,
        Modal::Pile(Pile::Draw) => 1,
        Modal::Pile(Pile::Discard) => 2,
        Modal::Pile(Pile::Exhaust) => 3,
        Modal::Deck => 4,
        Modal::PickUpgrade => 5,
        Modal::PickRemove => 6,
        Modal::Help => 7,
    }
}

pub struct DeckUiPlugin;

impl Plugin for DeckUiPlugin {
    fn build(&self, app: &mut App) {
        let active = resource_exists::<DeckGame>;
        app.init_resource::<DeckUi>().add_systems(
            Update,
            (ui_build, hud_update, foe_plates, hand_sync, hand_input, button_input, floaters_and_banner, modal_scroll).chain().run_if(active).after(super::deck::DeckSet),
        )
        .add_systems(Update, reset_ui.run_if(not(active)));
    }
}

fn reset_ui(mut ui: ResMut<DeckUi>) {
    if ui.root.is_some() || ui.key != (0, 0, 0) {
        *ui = DeckUi::default();
    }
}

// ---------------------------------------------------------------------------------------------
// Builders: shared widgets
// ---------------------------------------------------------------------------------------------

fn bar(p: &mut ChildSpawnerCommands, w: f32, h: f32, fill: Color, marker: impl Component) {
    p.spawn((Node { width: Val::Px(w), height: Val::Px(h), border: UiRect::all(Val::Px(2.0)), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgba(0.02, 0.04, 0.14, 0.9)), BorderColor(NAVY), BorderRadius::all(Val::Px(h / 2.0)))).with_children(|b| {
        b.spawn((marker, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(fill)));
    });
}

fn chip(p: &mut ChildSpawnerCommands, a: &UiAssets, text: &str, color: Color, size: f32) {
    p.spawn((Node { padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)), border: UiRect::all(Val::Px(1.5)), align_items: AlignItems::Center, ..default() }, BackgroundColor(color.with_alpha(0.9)), BorderColor(NAVY), BorderRadius::all(Val::Px(8.0)))).with_children(|c| {
        c.spawn(body(a, text, size, Color::WHITE));
    });
}

/// A relic badge; tapping it shows what it does.
fn relic_chip(p: &mut ChildSpawnerCommands, a: &UiAssets, idx: usize, r: Relic, size: f32) {
    p.spawn((Button, Act(Cmd::ShowRelic(idx)), Node { min_width: Val::Px(26.0), min_height: Val::Px(20.0), padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)), border: UiRect::all(Val::Px(1.5)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(hex(0x3a4aa0)), BorderColor(GOLD_LO), BorderRadius::all(Val::Px(8.0)))).with_children(|c| {
        c.spawn(body(a, r.tag(), size, Color::WHITE));
    });
}

fn status_chips(p: &mut ChildSpawnerCommands, a: &UiAssets, st: &Statuses, size: f32) {
    for (s, n) in st.iter() {
        let c = if s.debuff() { hex(0xa6401e) } else { hex(0x2d8a4a) };
        chip(p, a, &format!("{} {}", s.tag(), n), c, size);
    }
}

fn st_hash(st: &Statuses) -> u64 {
    st.iter().fold(1469598103934665603u64, |h, (s, n)| (h ^ (s as u64 * 131 + n as u64)).wrapping_mul(1099511628211))
}

/// A card face. `s` scales everything (hand cards are s=1).
pub fn card_face<'a>(p: &'a mut ChildSpawnerCommands, a: &UiAssets, card: Card, s: f32, uid: u32) -> EntityCommands<'a> {
    let d = card.def();
    let mut e = p.spawn((
        Node { width: Val::Px(CARD_W * s), height: Val::Px(CARD_H * s), border: UiRect::all(Val::Px(3.0 * s)), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::axes(Val::Px(4.0 * s), Val::Px(4.0 * s)), row_gap: Val::Px(2.0 * s), overflow: Overflow::visible(), ..default() },
        BackgroundColor(hex(0x16224f)),
        BorderColor(rarity_color(d.rarity)),
        BorderRadius::all(Val::Px(9.0 * s)),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.5), Val::Px(0.0), Val::Px(3.0 * s), Val::Px(0.0), Val::Px(3.0 * s)),
    ));
    e.with_children(|c| {
        // Cost badge.
        let cost = match card.cost() {
            -1 => "X".to_string(),
            n => n.to_string(),
        };
        if !d.unplayable {
            c.spawn((Node { width: Val::Px(22.0 * s), height: Val::Px(22.0 * s), position_type: PositionType::Absolute, left: Val::Px(-7.0 * s), top: Val::Px(-7.0 * s), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(2.0 * s)), ..default() }, BackgroundColor(GOLD), BorderColor(BROWN), BorderRadius::all(Val::Px(99.0)))).with_children(|b| {
                b.spawn((Text::new(cost), font(&a.display, 14.0 * s), TextColor(BROWN)));
            });
        }
        c.spawn((Text::new(clean(card.name())), font(&a.display, 11.5 * s), TextColor(if card.up { hex(0xb6ff9a) } else { CREAM }), TextShadow { offset: Vec2::new(0.0, 1.0), color: NAVY }, TextLayout::new_with_justify(JustifyText::Center), Node { margin: UiRect::top(Val::Px(1.0 * s)), max_width: Val::Percent(88.0), ..default() }));
        // Art strip: the move's emblem.
        c.spawn((Node { width: Val::Percent(100.0), height: Val::Px(42.0 * s), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(1.5 * s)), ..default() }, BackgroundColor(kind_color(d.kind)), BorderColor(NAVY), BorderRadius::all(Val::Px(6.0 * s)))).with_children(|art| {
            art.spawn((Text::new(glyph(d.mv)), font(&a.display, 24.0 * s), TextColor(move_color(d.mv)), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
            art.spawn((Node { position_type: PositionType::Absolute, right: Val::Px(3.0 * s), bottom: Val::Px(0.0), ..default() }, Text::new(d.mv.name()), font(&a.body, 7.0 * s), TextColor(Color::WHITE.with_alpha(0.85))));
        });
        c.spawn((Text::new(clean(card.text())), font(&a.body, 10.6 * s), TextColor(Color::WHITE), TextLayout::new_with_justify(JustifyText::Center), Node { width: Val::Percent(100.0), margin: UiRect::top(Val::Px(3.0 * s)), ..default() }));
        let kind = match d.kind {
            Kind::Attack => "ATTACK",
            Kind::Skill => "SKILL",
            Kind::Power => "POWER",
            Kind::Curse => "CURSE",
        };
        c.spawn((Node { position_type: PositionType::Absolute, bottom: Val::Px(2.0 * s), ..default() }, Text::new(kind), font(&a.body, 7.0 * s), TextColor(rarity_color(d.rarity).with_alpha(0.9))));
        if uid != 0 {
            // Dimmer for "cannot afford" and a ring for "extends the chain".
            c.spawn((CardDim(uid), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.05, 0.0)), BorderRadius::all(Val::Px(7.0 * s))));
            c.spawn((CardLink(uid), Node { position_type: PositionType::Absolute, left: Val::Px(-3.0 * s), right: Val::Px(-3.0 * s), top: Val::Px(-3.0 * s), bottom: Val::Px(-3.0 * s), border: UiRect::all(Val::Px(3.0 * s)), ..default() }, BorderColor(Color::NONE), BorderRadius::all(Val::Px(11.0 * s))));
        }
    });
    e
}

fn screen_root(commands: &mut Commands, area: &SafeArea, tint: Color) -> Entity {
    commands.spawn((XpEntity, ScreenRoot, GlobalZIndex(Z), full(area), BackgroundColor(tint))).id()
}

/// The daily-seed toggle: a button before the first stop of a run (it re-rolls the map), a small chip afterwards.
fn daily_button(r: &mut ChildSpawnerCommands, a: &UiAssets, g: &DeckGame) {
    if g.fresh_start() {
        let (kind, label) = if g.daily { (ButtonKind::Green, "DAILY: ON") } else { (ButtonKind::Blue, "DAILY: OFF") };
        act_button(r, a, kind, 104.0, 28.0, 12.0, label, Cmd::ToggleDaily);
    } else if g.daily {
        chip(r, a, "DAILY", hex(0x2d8a4a), 10.0);
    }
}

fn top_bar(p: &mut ChildSpawnerCommands, a: &UiAssets, g: &DeckGame, with_deck: bool) {
    p.spawn(Node { width: Val::Percent(100.0), height: Val::Px(34.0), align_items: AlignItems::Center, column_gap: Val::Px(8.0), padding: UiRect { left: Val::Px(6.0), right: Val::Px(52.0), top: Val::Px(6.0), bottom: Val::Px(0.0) }, ..default() }).with_children(|r| {
        r.spawn((Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(panel_bg()), BorderColor(GOLD), BorderRadius::all(Val::Px(10.0)))).with_children(|c| {
            c.spawn(disp_text(a, g.hero_name(), 15.0, GOLD_HI));
        });
        r.spawn((Node { align_items: AlignItems::Center, column_gap: Val::Px(4.0), ..default() },)).with_children(|c| {
            c.spawn(body(a, &format!("HP {}/{}", g.run.hp, g.run.max_hp), 13.0, hex(0xff9a8a)));
        });
        r.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(3.0), ..default() }).with_children(|c| {
            icon(c, &a.coin, 16.0);
            c.spawn((CoinsText, body(a, &g.run.coins.to_string(), 13.0, GOLD_HI)));
        });
        for (i, r_) in g.run.relics.iter().enumerate() {
            relic_chip(r, a, i, *r_, 10.0);
        }
        for p_ in g.run.potions.iter().flatten() {
            chip(r, a, &p_.name()[..1], hex(0x2d8a4a), 11.0);
        }
        if with_deck {
            r.spawn(Node { flex_grow: 1.0, ..default() });
            if g.run.screen == Screen::Map {
                daily_button(r, a, g);
            }
            act_button(r, a, ButtonKind::Blue, 34.0, 28.0, 14.0, "?", Cmd::Open(Modal::Help));
            let mut b = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(86.0), Val::Px(28.0), 13.0), &format!("DECK {}", g.run.deck.len()));
            b.insert(Act(Cmd::Open(Modal::Deck)));
        }
    });
}

fn act_button<'a>(p: &'a mut ChildSpawnerCommands, a: &UiAssets, kind: ButtonKind, w: f32, h: f32, size: f32, text: &str, cmd: Cmd) -> EntityCommands<'a> {
    let mut b = button(p, a, ButtonSpec::new(kind, Val::Px(w), Val::Px(h), size), text);
    b.insert(Act(cmd));
    b
}

// ---------------------------------------------------------------------------------------------
// ui_build: (re)create the current screen
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn ui_build(mut commands: Commands, g: Res<DeckGame>, assets: Option<Res<UiAssets>>, area: Res<SafeArea>, windows: Query<&Window, With<PrimaryWindow>>, mut ui: ResMut<DeckUi>) {
    let Some(a) = assets else { return };
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let key = (g.rev, screen_id(g.run.screen), 0);
    let size_changed = (win - ui.sw).abs().max_element() > 1.0;
    let area = *area;
    let main_stale = !(ui.key == key && ui.root.is_some() && !size_changed);
    if main_stale {
        ui.sw = win;
        ui.key = key;
        if let Some(r) = ui.root.take() {
            commands.entity(r).despawn();
        }
        if let Some(r) = ui.hand_root.take() {
            commands.entity(r).despawn();
        }
        ui.plates.clear();
        ui.focus_list.clear();
        ui.focus = 0;
        ui.drag = None;
        let root = match g.run.screen {
            Screen::Combat => {
                let r = build_combat(&mut commands, &a, &area, &g, win);
                let hand = commands.spawn((XpEntity, GlobalZIndex(Z + 1), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() })).id();
                ui.hand_root = Some(hand);
                r
            }
            Screen::Map => build_map(&mut commands, &a, &area, &g, win),
            Screen::Reward => build_reward(&mut commands, &a, &area, &g),
            Screen::Shop => build_shop(&mut commands, &a, &area, &g),
            Screen::Rest => build_rest(&mut commands, &a, &area, &g),
            Screen::Event => build_event(&mut commands, &a, &area, &g),
            Screen::Treasure => build_treasure(&mut commands, &a, &area, &g),
            Screen::Won | Screen::Lost => build_summary(&mut commands, &a, &area, &g),
        };
        ui.root = Some(root);
    }
    // The modal (piles, deck, pickers, help) is its own root so opening it leaves the hand alone.
    let mkey = (g.rev, modal_id(g.modal));
    if main_stale || ui.modal_key != mkey {
        ui.modal_key = mkey;
        if let Some(m) = ui.modal.take() {
            commands.entity(m).despawn();
        }
        if g.modal != Modal::None {
            ui.modal = Some(build_modal(&mut commands, &a, &area, &g, win));
            ui.focus_list.clear();
            ui.focus = 0;
        }
    }
    // Floating text pool, banner and toast live on their own root so they survive screen rebuilds cheaply.
    if ui.floaters.is_empty() {
        for i in 0..14 {
            let e = commands.spawn((XpEntity, FloaterNode(i), GlobalZIndex(Z + 3), Text::new(""), font(&a.display, 18.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, abs(0.0, 0.0), Visibility::Hidden)).id();
            ui.floaters.push(e);
        }
        let b = commands.spawn((XpEntity, BannerText, GlobalZIndex(Z + 3), Text::new(""), font(&a.display, 40.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }, abs(0.0, 0.0), Visibility::Hidden)).id();
        ui.banner = Some(b);
        let t = commands.spawn((XpEntity, ToastText, GlobalZIndex(Z + 3), Text::new(""), font(&a.body, 14.0), TextColor(CREAM), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, abs(0.0, 0.0), Visibility::Hidden)).id();
        ui.toast = Some(t);
    }
}

// ---------------------------------------------------------------------------------------------
// Combat screen
// ---------------------------------------------------------------------------------------------

fn build_combat(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame, win: Vec2) -> Entity {
    let root = commands.spawn((XpEntity, ScreenRoot, GlobalZIndex(Z), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() })).id();
    let (l, r, t, b) = (area.left, area.right, area.top.max(0.0), area.bottom.max(6.0));
    commands.entity(root).with_children(|p| {
        // Top-left: hero plate.
        p.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(l + 6.0), top: Val::Px(t + 4.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(panel_bg()), BorderColor(NAVY), BorderRadius::all(Val::Px(12.0)))).with_children(|c| {
            c.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), ..default() }).with_children(|row| {
                row.spawn(disp_text(a, g.hero_name(), 14.0, GOLD_HI));
                row.spawn((ShellText, body(a, "", 12.0, hex(0x8fd0ff))));
            });
            c.spawn(Node { width: Val::Px(150.0), height: Val::Px(18.0), ..default() }).with_children(|h| {
                bar(h, 150.0, 18.0, hex(0x39c24a), HpFill);
                h.spawn(Node { position_type: PositionType::Absolute, width: Val::Px(150.0), height: Val::Px(18.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|tt| {
                    tt.spawn((HpText, body(a, "", 11.0, Color::WHITE)));
                });
            });
            c.spawn((StatusRow, Node { column_gap: Val::Px(3.0), min_height: Val::Px(14.0), flex_wrap: FlexWrap::Wrap, ..default() }));
        });
        // Relics, coins and potions row under the plate.
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(l + 6.0), top: Val::Px(t + 78.0), align_items: AlignItems::Center, column_gap: Val::Px(4.0), ..default() }).with_children(|row| {
            row.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(2.0), ..default() }).with_children(|c| {
                icon(c, &a.coin, 14.0);
                c.spawn((CoinsText, body(a, &g.run.coins.to_string(), 12.0, GOLD_HI)));
            });
            for (i, rl) in g.run.relics.iter().enumerate() {
                relic_chip(row, a, i, *rl, 9.5);
            }
        });
        // Potions (tap to drink / throw).
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(l + 6.0), top: Val::Px(t + 96.0), column_gap: Val::Px(5.0), ..default() }).with_children(|row| {
            act_button(row, a, ButtonKind::Blue, 64.0, 34.0, 12.0, &format!("DECK {}", g.run.deck.len()), Cmd::Open(Modal::Deck));
            act_button(row, a, ButtonKind::Blue, 34.0, 34.0, 16.0, "?", Cmd::Open(Modal::Help));
            for i in 0..POTION_SLOTS {
                row.spawn((Button, Act(Cmd::Potion(i, None)), PotionSlot(i), Node { width: Val::Px(34.0), height: Val::Px(34.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(hex(0x1d3a2a)), BorderColor(NAVY), BorderRadius::all(Val::Px(99.0)))).with_children(|b| {
                    b.spawn((PotionLabel(i), Text::new(""), font(&a.display, 14.0), TextColor(Color::WHITE)));
                });
            }
        });
        // Top centre: turn and incoming damage.
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(t + 4.0), justify_content: JustifyContent::Center, ..default() }).with_children(|c| {
            c.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::axes(Val::Px(12.0), Val::Px(2.0)), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(panel_bg()), BorderColor(NAVY), BorderRadius::all(Val::Px(10.0)))).with_children(|c| {
                c.spawn((TurnText, disp_text(a, "TURN 1", 14.0, CREAM)));
                c.spawn((HintText, body(a, "", 10.5, hex(0xffb0a0))));
            });
        });
        // Left column: momentum meter (vertical) and the draw pile.
        let pile_y = win.y - b - 58.0;
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(l + 4.0), top: Val::Px(pile_y - 156.0), width: Val::Px(62.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(2.0), padding: UiRect::all(Val::Px(4.0)), border: UiRect::all(Val::Px(2.0)), ..default() }).insert((BackgroundColor(panel_bg()), BorderColor(NAVY), BorderRadius::all(Val::Px(12.0)))).with_children(|c| {
            c.spawn(body(a, "MOMENTUM", 8.5, hex(0xffe27a)));
            c.spawn((MomText, disp_text(a, "3", 20.0, GOLD_HI)));
            c.spawn(Node { flex_direction: FlexDirection::ColumnReverse, row_gap: Val::Px(2.0), ..default() }).with_children(|pips| {
                for i in 0..MAX_MOMENTUM as usize {
                    pips.spawn((MomPip(i), Node { width: Val::Px(34.0), height: Val::Px(8.0), border: UiRect::all(Val::Px(1.0)), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)), BorderColor(NAVY), BorderRadius::all(Val::Px(4.0))));
                }
            });
            c.spawn((ChainText, body(a, "", 9.5, hex(0xff9a40))));
        });
        pile_button(p, a, l + 4.0, pile_y, "DRAW", Pile::Draw);
        // Right column: end turn, discard pile.
        let rx = win.x - r.max(8.0) - 84.0;
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(rx), top: Val::Px(win.y - b - 140.0), width: Val::Px(80.0), ..default() }).with_children(|c| {
            let mut e = button(c, a, ButtonSpec::new(ButtonKind::Gold, Val::Px(80.0), Val::Px(44.0), 14.0), "END TURN");
            e.insert(Act(Cmd::EndTurn));
            e.with_children(|_| {});
        });
        pile_button(p, a, rx + 6.0, pile_y, "DISCARD", Pile::Discard);
        // Exhaust (small, only when used).
        p.spawn((Button, Act(Cmd::Open(Modal::Pile(Pile::Exhaust))), Node { position_type: PositionType::Absolute, left: Val::Px(rx + 10.0), top: Val::Px(win.y - b - 90.0), width: Val::Px(46.0), height: Val::Px(22.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(1.5)), ..default() }, BackgroundColor(panel_bg()), BorderColor(NAVY), BorderRadius::all(Val::Px(8.0)))).with_children(|c| {
            c.spawn((PileCount(Pile::Exhaust), body(a, "EXH 0", 9.0, hex(0xc8a0ff))));
        });
    });
    root
}

fn pile_button(p: &mut ChildSpawnerCommands, a: &UiAssets, x: f32, y: f32, label: &str, pile: Pile) {
    p.spawn((Button, Act(Cmd::Open(Modal::Pile(pile))), Node { position_type: PositionType::Absolute, left: Val::Px(x), top: Val::Px(y), width: Val::Px(54.0), height: Val::Px(54.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(hex(0x22388a)), BorderColor(GOLD_LO), BorderRadius::all(Val::Px(10.0)), BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.5), Val::Px(0.0), Val::Px(3.0), Val::Px(0.0), Val::Px(2.0)))).with_children(|c| {
        c.spawn((PileCount(pile), disp_text(a, "0", 20.0, Color::WHITE)));
        c.spawn(body(a, label, 8.5, CREAM));
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hud_update(
    g: Res<DeckGame>,
    mut hp_fill: Query<&mut Node, (With<HpFill>, Without<MomPip>)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<HpText>>,
        Query<&mut Text, With<ShellText>>,
        Query<&mut Text, With<TurnText>>,
        Query<&mut Text, With<HintText>>,
        Query<&mut Text, With<MomText>>,
        Query<&mut Text, With<ChainText>>,
        Query<(&PileCount, &mut Text)>,
        Query<(&PotionLabel, &mut Text)>,
    )>,
    mut pips: Query<(&MomPip, &mut BackgroundColor)>,
    mut potion_btn: Query<(&PotionSlot, &mut BackgroundColor, &mut BorderColor), Without<MomPip>>,
    status_row: Query<(Entity, Option<&Children>), With<StatusRow>>,
    mut commands: Commands,
    assets: Option<Res<UiAssets>>,
    mut last_st: Local<u64>,
) {
    if g.run.screen != Screen::Combat {
        return;
    }
    let d = &g.disp;
    let set = |t: &mut Text, s: String| {
        if t.0 != s {
            t.0 = s;
        }
    };
    for mut n in &mut hp_fill {
        let w = Val::Percent((d.hp as f32 / d.max_hp.max(1) as f32 * 100.0).clamp(0.0, 100.0));
        if n.width != w {
            n.width = w;
        }
    }
    for mut t in texts.p0().iter_mut() {
        set(&mut t, format!("{} / {}", d.hp, d.max_hp));
    }
    for mut t in texts.p1().iter_mut() {
        set(&mut t, if d.shell > 0 { format!("SHELL {}", d.shell) } else { String::new() });
    }
    for mut t in texts.p2().iter_mut() {
        set(&mut t, if d.enemy_turn { "FOES ACT".to_string() } else { format!("TURN {}", d.turn.max(1)) });
    }
    let incoming = g.combat().map_or(0, |c| (c.incoming() - d.shell).max(0));
    let hint = if d.enemy_turn || g.busy() {
        String::new()
    } else if let Some(h) = g.selected {
        match g.combat().and_then(|c| c.hand.get(h)) {
            Some(i) if i.card.targeted() && g.combat().is_some_and(|c| c.living() > 1) => if g.aim.is_some() { "Left / Right: pick foe, Enter: strike".to_string() } else { "Tap a foe (or Enter to aim)".to_string() },
            Some(_) => "Tap again or drag up to play".to_string(),
            None => String::new(),
        }
    } else if incoming > 0 {
        format!("Incoming {incoming}")
    } else {
        "Blocked".to_string()
    };
    for mut t in texts.p3().iter_mut() {
        set(&mut t, hint.clone());
    }
    for mut t in texts.p4().iter_mut() {
        set(&mut t, d.momentum.to_string());
    }
    for mut t in texts.p5().iter_mut() {
        set(&mut t, if d.chain >= 1 { format!("CHAIN x{}", d.chain + 1) } else { String::new() });
    }
    for (pc, mut t) in texts.p6().iter_mut() {
        let s = match pc.0 {
            Pile::Draw => d.draw_n.to_string(),
            Pile::Discard => d.discard_n.to_string(),
            Pile::Exhaust => format!("EXH {}", d.exhaust_n),
        };
        set(&mut t, s);
    }
    for (pl, mut t) in texts.p7().iter_mut() {
        set(&mut t, d.potions[pl.0].map_or("-".to_string(), |p| p.name()[..1].to_string()));
    }
    for (pip, mut bg) in &mut pips {
        let on = (pip.0 as i32) < d.momentum;
        let col = if on {
            if d.chain >= 2 { hex(0xff9a2a) } else { hex(0xffd83a) }
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.1)
        };
        if bg.0 != col {
            bg.0 = col;
        }
    }
    for (slot, mut bg, mut border) in &mut potion_btn {
        let has = d.potions[slot.0].is_some();
        let c = if has { hex(0x2e8f55) } else { hex(0x14202a) };
        if bg.0 != c {
            bg.0 = c;
        }
        let bc = if has { GOLD_HI } else { NAVY };
        if border.0 != bc {
            border.0 = bc;
        }
    }
    // Status chips under the HP bar.
    let h = st_hash(&d.st);
    if h != *last_st {
        *last_st = h;
        if let (Some(a), Ok((row, kids))) = (assets, status_row.single()) {
            if let Some(kids) = kids {
                for k in kids.iter() {
                    commands.entity(k).despawn();
                }
            }
            let st = d.st;
            commands.entity(row).with_children(|p| status_chips(p, &a, &st, 9.0));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Foe plates
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn foe_plates(
    mut commands: Commands,
    g: Res<DeckGame>,
    assets: Option<Res<UiAssets>>,
    mut ui: ResMut<DeckUi>,
    mut plates: Query<(&FoePlate, &mut Node, &mut Visibility)>,
    mut texts: ParamSet<(Query<(&FoeIntentText, &mut Text)>, Query<(&FoeIntentName, &mut Text)>, Query<(&FoeHpText, &mut Text)>, Query<(&FoeShellText, &mut Text)>)>,
    mut badges: Query<(&FoeIntentBadge, &mut BackgroundColor), Without<FoeHpFill>>,
    mut fills: Query<(&FoeHpFill, &mut Node), Without<FoePlate>>,
    mut borders: Query<(&FoeBorder, &mut BorderColor)>,
    mut ring: Query<(&mut Node, &mut Visibility, &mut BorderColor), (With<AimRing>, Without<FoeBorder>, Without<FoePlate>, Without<FoeHpFill>)>,
    status_rows: Query<(Entity, &FoeStatusRow, Option<&Children>)>,
    area: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Some(a) = assets else { return };
    if g.run.screen != Screen::Combat {
        return;
    }
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let Some(root) = ui.root else { return };
    // Spawn missing plates (and the aim ring with the first one).
    while ui.plates.len() < g.disp.foes.len() {
        let i = ui.plates.len();
        if i == 0 {
            let r = commands.spawn((XpEntity, AimRing, Node { position_type: PositionType::Absolute, width: Val::Px(0.0), height: Val::Px(0.0), border: UiRect::all(Val::Px(4.0)), ..default() }, BorderColor(GOLD_HI), BorderRadius::MAX, Visibility::Hidden)).id();
            commands.entity(root).add_child(r);
        }
        let e = commands
            .spawn((XpEntity, FoePlate(i), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Px(PLATE_W), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(1.0), ..default() }, Visibility::Hidden))
            .with_children(|p| {
                p.spawn((FoeIntentBadge(i), Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)), border: UiRect::all(Val::Px(2.0)), min_width: Val::Px(54.0), justify_content: JustifyContent::Center, ..default() }, BackgroundColor(hex(0xd9382c)), BorderColor(NAVY), BorderRadius::all(Val::Px(11.0)))).with_children(|b| {
                    b.spawn((FoeIntentText(i), Text::new("ATK"), font(&a.display, 15.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY }));
                });
                p.spawn((FoeIntentName(i), body(&a, "", 10.0, CREAM)));
                p.spawn((FoeBorder(i), Node { width: Val::Px(PLATE_W - 14.0), height: Val::Px(16.0), border: UiRect::all(Val::Px(2.0)), overflow: Overflow::clip(), margin: UiRect::top(Val::Px(1.0)), ..default() }, BackgroundColor(Color::srgba(0.02, 0.04, 0.14, 0.9)), BorderColor(NAVY), BorderRadius::all(Val::Px(8.0)))).with_children(|b| {
                    b.spawn((FoeHpFill(i), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(hex(0xe5502a))));
                    b.spawn(Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|t| {
                        t.spawn((FoeHpText(i), body(&a, "", 10.0, Color::WHITE)));
                    });
                });
                p.spawn((FoeShellText(i), body(&a, "", 10.0, hex(0x8fd0ff))));
                p.spawn((FoeStatusRow(i, 0), Node { column_gap: Val::Px(3.0), flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, max_width: Val::Px(PLATE_W), min_height: Val::Px(12.0), ..default() }));
            })
            .id();
        commands.entity(root).add_child(e);
        ui.plates.push(e);
    }
    for (pl, mut node, mut vis) in &mut plates {
        let i = pl.0;
        let fs = g.foe_screen.get(i).copied().unwrap_or_default();
        let alive = g.disp.foes.get(i).is_some_and(|f| f.alive) && g.foes3d.get(i).is_some_and(|f| f.died.is_none());
        let on = fs.on && alive;
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if on {
            let x = (fs.center.x - PLATE_W / 2.0).clamp(2.0, (win.x - area.right.max(8.0) - 64.0 - PLATE_W).max(2.0));
            let y = (fs.top.y - 74.0).max(36.0);
            node.left = Val::Px(x);
            node.top = Val::Px(y);
        }
    }
    let selected_targeted = g.selected.and_then(|h| g.combat().and_then(|c| c.hand.get(h))).is_some_and(|i| i.card.targeted());
    for (b, mut bg) in &mut badges {
        if let Some(f) = g.disp.foes.get(b.0) {
            let col = intent_color(f.intent.category());
            if bg.0 != col {
                bg.0 = col;
            }
        }
    }
    for (b, mut border) in &mut borders {
        let aimed = g.aim == Some(b.0);
        let c = if aimed { Color::WHITE } else if selected_targeted && g.disp.foes.get(b.0).is_some_and(|f| f.alive) { GOLD_HI } else { NAVY };
        if border.0 != c {
            border.0 = c;
        }
    }
    // The aim ring: a pulsing gold ring around the aimed foe's body.
    for (mut node, mut vis, mut col) in &mut ring {
        let target = g.aim.and_then(|a| g.foe_screen.get(a).copied().filter(|s| s.on));
        if let Some(fs) = target {
            let w = (fs.half_w * 2.0 + 26.0).max(70.0);
            let h = ((fs.bottom.y - fs.top.y) + 26.0).max(70.0);
            node.width = Val::Px(w);
            node.height = Val::Px(h);
            node.left = Val::Px(fs.center.x - w / 2.0);
            node.top = Val::Px((fs.top.y + fs.bottom.y) / 2.0 - h / 2.0);
            let pulse = 0.65 + 0.35 * (g.t as f32 * 7.0).sin();
            col.0 = Color::srgb(1.0, 0.85 * pulse + 0.15, 0.25 + 0.5 * (1.0 - pulse));
            *vis = Visibility::Inherited;
        } else if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
    }
    let set = |t: &mut Text, s: String| {
        if t.0 != s {
            t.0 = s;
        }
    };
    let Some(core) = g.combat() else { return };
    for (it, mut t) in texts.p0().iter_mut() {
        let Some(f) = g.disp.foes.get(it.0) else { continue };
        let text = intent_label(f, core, it.0, &g);
        set(&mut t, text);
    }
    for (it, mut t) in texts.p1().iter_mut() {
        let Some(f) = g.disp.foes.get(it.0) else { continue };
        set(&mut t, format!("{} - {}", f.kind.name(), f.intent.name));
    }
    for (it, mut t) in texts.p2().iter_mut() {
        let Some(f) = g.disp.foes.get(it.0) else { continue };
        set(&mut t, format!("{}/{}", f.hp, f.max_hp));
    }
    for (it, mut t) in texts.p3().iter_mut() {
        let Some(f) = g.disp.foes.get(it.0) else { continue };
        set(&mut t, if f.shell > 0 { format!("SHELL {}", f.shell) } else { String::new() });
    }
    for (fl, mut n) in &mut fills {
        let Some(f) = g.disp.foes.get(fl.0) else { continue };
        let w = Val::Percent((f.hp as f32 / f.max_hp.max(1) as f32 * 100.0).clamp(0.0, 100.0));
        if n.width != w {
            n.width = w;
        }
    }
    for (e, row, kids) in &status_rows {
        let Some(f) = g.disp.foes.get(row.0) else { continue };
        let h = st_hash(&f.st);
        if h != row.1 {
            if let Some(kids) = kids {
                for k in kids.iter() {
                    commands.entity(k).despawn();
                }
            }
            let st = f.st;
            let a2 = a.clone();
            commands.entity(e).insert(FoeStatusRow(row.0, h)).with_children(|p| status_chips(p, &a2, &st, 8.5));
        }
    }
}

const PLATE_W: f32 = 118.0;

fn intent_label(f: &DFoe, core: &Combat, i: usize, g: &DeckGame) -> String {
    let it = f.intent;
    if it.winding {
        return "!!".to_string();
    }
    // Damage as it will land right now (statuses included), from the display state.
    let _ = (core, i, g);
    let mut parts = vec![];
    if it.attacks() {
        let per = apply_damage_mods(it.dmg, &f.st, &g.disp.st);
        parts.push(if it.hits > 1 { format!("{}x{}", per, it.hits) } else { per.to_string() });
    }
    if it.shell > 0 && !it.attacks() {
        parts.push(format!("+{}", it.shell));
    }
    if parts.is_empty() {
        parts.push(
            match it.category() {
                IntentIcon::Buff => "BUFF",
                IntentIcon::Debuff => "HEX",
                IntentIcon::Guard => "GUARD",
                _ => "?",
            }
            .to_string(),
        );
    }
    let tag = match it.category() {
        IntentIcon::Attack => "ATK ",
        IntentIcon::Guard => "DEF ",
        _ => "",
    };
    let extra = if it.attacks() && (it.shell > 0 || it.buff.is_some() || it.debuff.is_some() || it.junk > 0) { "+" } else { "" };
    format!("{tag}{}{extra}", parts.join(" "))
}

// ---------------------------------------------------------------------------------------------
// The hand
// ---------------------------------------------------------------------------------------------

fn hand_layout(n: usize, win: Vec2, area: &SafeArea) -> Vec<(Vec2, f32)> {
    let l = area.left + 68.0;
    let r = win.x - area.right.max(8.0) - 92.0;
    let center = (l + r) / 2.0;
    let step = if n <= 1 { 0.0 } else { (CARD_W + 4.0).min((r - l - CARD_W) / (n as f32 - 1.0)) };
    let by = win.y - area.bottom.max(6.0) - CARD_H / 2.0 - 2.0;
    let mid = (n as f32 - 1.0) / 2.0;
    (0..n)
        .map(|i| {
            let o = i as f32 - mid;
            (Vec2::new(center + o * step, by + o.abs().powi(2) * 1.6), o * 0.035)
        })
        .collect()
}

fn pile_pos(win: Vec2, area: &SafeArea, discard: bool) -> Vec2 {
    let y = win.y - area.bottom.max(6.0) - 31.0;
    if discard { Vec2::new(win.x - area.right.max(8.0) - 84.0 + 33.0, y) } else { Vec2::new(area.left + 4.0 + 27.0, y) }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hand_sync(
    mut commands: Commands,
    g: Res<DeckGame>,
    assets: Option<Res<UiAssets>>,
    ui: Res<DeckUi>,
    area: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
    mut cards: Query<(Entity, &mut CardEnt, &mut Node, &mut Transform, &mut ZIndex)>,
    mut dims: Query<(&CardDim, &mut BackgroundColor), Without<CardEnt>>,
    mut links: Query<(&CardLink, &mut BorderColor), (Without<CardEnt>, Without<CardDim>)>,
) {
    let Some(a) = assets else { return };
    let Some(hand_root) = ui.hand_root else { return };
    if g.run.screen != Screen::Combat {
        return;
    }
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let dt = time.delta_secs().min(0.05);
    let draw_pos = pile_pos(win, &area, false);
    let disc_pos = pile_pos(win, &area, true);
    let existing: HashMap<u32, Entity> = cards.iter().map(|(e, c, ..)| (c.uid, e)).collect();
    // Spawn new hand cards at the draw pile.
    for inst in &g.disp.hand {
        if !existing.contains_key(&inst.uid) {
            let e = commands
                .spawn((XpEntity, CardEnt { uid: inst.uid, card: inst.card, pos: draw_pos, scale: 0.3, rot: -0.4, leaving: None, born: g.t }, Node { position_type: PositionType::Absolute, left: Val::Px(draw_pos.x - CARD_W / 2.0), top: Val::Px(draw_pos.y - CARD_H / 2.0), ..default() }, Transform::from_scale(Vec3::splat(0.3)), ZIndex(0)))
                .id();
            commands.entity(e).with_children(|p| {
                card_face(p, &a, inst.card, 1.0, inst.uid);
            });
            commands.entity(hand_root).add_child(e);
        }
    }
    let layout = hand_layout(g.disp.hand.len(), win, &area);
    let sel_uid = g.selected.and_then(|h| g.disp.hand.get(h)).map(|i| i.uid);
    let core = g.combat();
    let busy = g.busy();
    let momentum = g.disp.momentum;
    for (e, mut c, mut node, mut tf, mut z) in &mut cards {
        // Where should it be?
        let slot = g.disp.hand.iter().position(|i| i.uid == c.uid);
        let drag = ui.drag.filter(|d| d.uid == c.uid && d.moved);
        let (target, tscale, trot) = if let Some(i) = slot {
            if c.leaving.is_some() {
                c.leaving = None;
            }
            let (p, r) = layout[i];
            let mut p = p;
            let mut s = 1.0;
            if sel_uid == Some(c.uid) {
                p.y -= 44.0;
                s = 1.32;
            }
            if let Some(d) = drag {
                p = d.pos;
                s = 1.15;
            }
            (p, s, if sel_uid == Some(c.uid) || drag.is_some() { 0.0 } else { r })
        } else {
            if c.leaving.is_none() {
                let how = g.leave.get(&c.uid).copied().unwrap_or(Leave::Discard);
                c.leaving = Some((how, g.t));
            }
            let (how, t0) = c.leaving.unwrap();
            match how {
                Leave::Play => (Vec2::new(win.x * 0.5, win.y * 0.45), if g.t - t0 > 0.22 { 0.0 } else { 1.3 }, 0.0),
                Leave::Discard => (disc_pos, 0.15, 0.6),
                Leave::Exhaust => (c.pos + Vec2::new(0.0, -50.0), if g.t - t0 > 0.12 { 0.0 } else { 1.4 }, 0.3),
            }
        };
        let k = 1.0 - (-(if c.leaving.is_some() { 16.0 } else { 20.0 }) * dt).exp();
        let k = if drag.is_some() { 1.0 } else { k };
        c.pos = c.pos.lerp(target, k);
        c.scale += (tscale - c.scale) * k;
        c.rot += (trot - c.rot) * k;
        node.left = Val::Px(c.pos.x - CARD_W / 2.0);
        node.top = Val::Px(c.pos.y - CARD_H / 2.0);
        tf.scale = Vec3::splat(c.scale.max(0.001));
        tf.rotation = Quat::from_rotation_z(-c.rot);
        let zi = if drag.is_some() { 60 } else if sel_uid == Some(c.uid) { 50 } else { slot.map_or(0, |i| i as i32 + 1) };
        if z.0 != zi {
            z.0 = zi;
        }
        if let Some((_, t0)) = c.leaving {
            if g.t - t0 > 0.5 || c.scale < 0.02 {
                commands.entity(e).despawn();
            }
        }
        let _ = c.born;
    }
    // Affordability dimming and chain-link glow.
    if let Some(core) = core {
        for (dim, mut bg) in &mut dims {
            let Some(idx) = core.hand.iter().position(|i| i.uid == dim.0) else { continue };
            let playable = !busy && core.can_play(idx).is_ok();
            let want = Color::srgba(0.0, 0.0, 0.05, if playable { 0.0 } else { 0.5 });
            if bg.0 != want {
                bg.0 = want;
            }
        }
        for (l, mut bc) in &mut links {
            let Some(idx) = core.hand.iter().position(|i| i.uid == l.0) else { continue };
            let glow = !busy && core.can_play(idx).is_ok() && core.links(idx) && !core.hand[idx].card.def().mv.neutral();
            let want = if glow { Color::srgb(1.0, 0.72, 0.15) } else { Color::NONE };
            if bc.0 != want {
                bc.0 = want;
            }
        }
    }
    let _ = momentum;
}

struct Ptr {
    pos: Option<Vec2>,
    down: bool,
    pressed: bool,
    released: bool,
}

fn pointer(windows: &Query<&Window, With<PrimaryWindow>>, mouse: &ButtonInput<MouseButton>, touches: &Touches) -> Ptr {
    if let Some(t) = touches.iter().next() {
        return Ptr { pos: Some(t.position()), down: true, pressed: touches.iter_just_pressed().next().is_some(), released: false };
    }
    if let Some(t) = touches.iter_just_released().next() {
        return Ptr { pos: Some(t.position()), down: false, pressed: false, released: true };
    }
    let pos = windows.single().ok().and_then(|w| w.cursor_position());
    Ptr { pos, down: mouse.pressed(MouseButton::Left), pressed: mouse.just_pressed(MouseButton::Left), released: mouse.just_released(MouseButton::Left) }
}

#[allow(clippy::too_many_arguments)]
fn hand_input(
    mut g: ResMut<DeckGame>,
    mut ui: ResMut<DeckUi>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    state: Res<AppState>,
    area: Res<SafeArea>,
    cards: Query<&CardEnt>,
) {
    if *state != AppState::Playing || g.run.screen != Screen::Combat || g.modal != Modal::None {
        ui.drag = None;
        return;
    }
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let ptr = pointer(&windows, &mouse, &touches);
    let idle = !g.busy() && g.combat().is_some_and(|c| !c.over());
    // Keyboard / gamepad.
    let n = g.combat().map_or(0, |c| c.hand.len());
    let mut left = keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA);
    let mut right = keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD);
    let mut confirm = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space);
    let mut cancel = keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyQ);
    let mut end = keys.just_pressed(KeyCode::KeyE);
    for pad in &pads {
        left |= pad.just_pressed(GamepadButton::DPadLeft);
        right |= pad.just_pressed(GamepadButton::DPadRight);
        confirm |= pad.just_pressed(GamepadButton::South);
        cancel |= pad.just_pressed(GamepadButton::East);
        end |= pad.just_pressed(GamepadButton::North);
    }
    if idle {
        for (i, k) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9].iter().enumerate() {
            if keys.just_pressed(*k) && i < n {
                activate(&mut g, i);
            }
        }
        // Aiming a targeted card at one of several foes: left / right walk the foes, confirm plays, cancel goes back to the hand.
        let sel_targeted = g.selected.and_then(|h| g.combat().and_then(|c| c.hand.get(h))).is_some_and(|i| i.card.targeted());
        let living = g.combat().map_or(0, |c| c.living());
        if !sel_targeted || living < 2 || g.selected.is_none() {
            g.aim = None;
        } else if g.aim.is_some_and(|a| !g.disp.foes.get(a).is_some_and(|f| f.alive)) {
            g.begin_aim();
        }
        if g.aim.is_some() {
            if left || right {
                g.move_aim(if right { 1 } else { -1 });
                left = false;
                right = false;
            }
            if confirm {
                if let (Some(h), Some(t)) = (g.selected, g.aim) {
                    if g.combat().is_some_and(|c| c.can_play(h).is_ok()) {
                        g.cmds.push(Cmd::Play(h, Some(t)));
                    }
                }
                confirm = false;
            }
            if cancel {
                g.aim = None;
                cancel = false;
            }
        }
        if left || right {
            let cur = g.selected.unwrap_or(if right { n.saturating_sub(1) } else { 0 });
            let next = if right { (cur + 1) % n.max(1) } else { (cur + n.max(1) - 1) % n.max(1) };
            if n > 0 {
                g.selected = Some(next);
                g.aim = None;
            }
        }
        if confirm {
            if let Some(h) = g.selected {
                activate(&mut g, h);
            }
        }
        if cancel {
            g.selected = None;
        }
        if end {
            g.cmds.push(Cmd::EndTurn);
        }
    }
    // Pointer.
    let layout = hand_layout(g.disp.hand.len(), win, &area);
    let sel_uid = g.selected.and_then(|h| g.disp.hand.get(h)).map(|i| i.uid);
    let hit = |p: Vec2| -> Option<usize> {
        // Topmost first: the selected card, then right to left.
        let rect = |i: usize| -> (Vec2, Vec2) {
            let uid = g.disp.hand[i].uid;
            let (c, scale) = cards.iter().find(|c| c.uid == uid).map_or((layout[i].0, 1.0), |c| (c.pos, c.scale.max(0.5)));
            (c, Vec2::new(CARD_W, CARD_H) * scale / 2.0)
        };
        let mut order: Vec<usize> = (0..g.disp.hand.len()).collect();
        order.sort_by_key(|i| if Some(g.disp.hand[*i].uid) == sel_uid { 1000 } else { *i });
        order.into_iter().rev().find(|i| {
            let (c, h) = rect(*i);
            (p.x - c.x).abs() <= h.x && (p.y - c.y).abs() <= h.y
        })
    };
    if let Some(p) = ptr.pos {
        if ptr.pressed && idle {
            if let Some(i) = hit(p) {
                ui.drag = Some(DragInfo { uid: g.disp.hand[i].uid, idx: i, start: p, pos: p, moved: false });
            }
        } else if let Some(d) = ui.drag.as_mut() {
            if ptr.down || ptr.released {
                d.pos = p;
                if (p - d.start).length() > 16.0 {
                    d.moved = true;
                }
            }
        }
    }
    if ptr.released {
        let released_at = ptr.pos;
        if let Some(d) = ui.drag.take() {
            if let Some(core_idx) = g.hand_index_of(d.uid) {
                if d.moved {
                    let p = released_at.unwrap_or(d.pos);
                    let hand_top = win.y - area.bottom.max(6.0) - CARD_H - 10.0;
                    if p.y < hand_top - 8.0 && idle {
                        let card = g.combat().unwrap().hand[core_idx].card;
                        if card.targeted() {
                            let target = foe_at(&g, p, 40.0).or_else(|| nearest_foe(&g, p));
                            if let Some(t) = target {
                                g.cmds.push(Cmd::Play(core_idx, Some(t)));
                            }
                        } else {
                            g.cmds.push(Cmd::Play(core_idx, None));
                        }
                    }
                } else {
                    activate(&mut g, core_idx);
                }
            }
        } else if idle {
            // A tap on a foe with a targeted card selected plays it; a tap on empty stage deselects.
            if let (Some(p), Some(h)) = (released_at, g.selected) {
                let targeted = g.combat().and_then(|c| c.hand.get(h)).is_some_and(|i| i.card.targeted());
                if targeted {
                    if let Some(t) = foe_at(&g, p, 14.0) {
                        g.cmds.push(Cmd::Play(h, Some(t)));
                        return;
                    }
                }
                if p.y < win.y - area.bottom - CARD_H - 20.0 && p.y > 70.0 {
                    g.selected = None;
                }
            }
        }
    }
}

fn foe_at(g: &DeckGame, p: Vec2, pad: f32) -> Option<usize> {
    g.disp.foes.iter().enumerate().filter(|(i, f)| f.alive && g.foe_screen.get(*i).is_some_and(|s| s.on)).find_map(|(i, _)| {
        let s = g.foe_screen[i];
        let (x0, x1) = (s.center.x - s.half_w - pad, s.center.x + s.half_w + pad);
        let (y0, y1) = (s.top.y - 70.0 - pad, s.bottom.y + pad);
        (p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1).then_some(i)
    })
}

fn nearest_foe(g: &DeckGame, p: Vec2) -> Option<usize> {
    g.disp.foes.iter().enumerate().filter(|(i, f)| f.alive && g.foe_screen.get(*i).is_some_and(|s| s.on)).min_by(|(a, _), (b, _)| g.foe_screen[*a].center.distance(p).partial_cmp(&g.foe_screen[*b].center.distance(p)).unwrap()).map(|(i, _)| i)
}

/// Tap on card `h`: select it; tap the selected card again to play it (untargeted or a single foe).
fn activate(g: &mut DeckGame, h: usize) {
    let Some(core) = g.combat() else { return };
    let Some(inst) = core.hand.get(h) else { return };
    if g.selected == Some(h) {
        if core.can_play(h).is_err() {
            return;
        }
        if inst.card.targeted() {
            let alive: Vec<usize> = core.alive_foes().collect();
            if alive.len() == 1 {
                g.cmds.push(Cmd::Play(h, Some(alive[0])));
            } else if g.aim.is_none() {
                // Several foes: confirming a selected targeted card starts aiming (left / right, confirm) instead of doing nothing.
                g.begin_aim();
            }
        } else {
            g.cmds.push(Cmd::Play(h, None));
        }
    } else {
        g.selected = Some(h);
        g.aim = None;
    }
}

// ---------------------------------------------------------------------------------------------
// Buttons: pointer, keyboard focus, gamepad
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn button_input(
    mut g: ResMut<DeckGame>,
    mut ui: ResMut<DeckUi>,
    pressed: Query<(&Interaction, &Act), Changed<Interaction>>,
    all: Query<(Entity, &Act)>,
    mut outlines: Query<(Entity, &mut Outline), With<Act>>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    state: Res<AppState>,
) {
    if *state != AppState::Playing {
        return;
    }
    for (i, act) in &pressed {
        if *i == Interaction::Pressed {
            let cmd = match act.0 {
                Cmd::Potion(s, _) => {
                    // Potions: drink out of combat; in combat throw at the only foe or first living one.
                    if g.run.screen == Screen::Combat {
                        let t = g.combat().and_then(|c| c.alive_foes().next());
                        Cmd::Potion(s, t)
                    } else {
                        Cmd::DrinkTonic(s)
                    }
                }
                c => c,
            };
            g.cmds.push(cmd);
        }
    }
    // Keyboard / gamepad focus over Act buttons (non-combat screens and modals).
    let combat_idle = g.run.screen == Screen::Combat && g.modal == Modal::None;
    let order: Vec<Entity> = if ui.focus_list.is_empty() { all.iter().map(|(e, _)| e).collect() } else { ui.focus_list.clone() };
    if ui.focus_list.is_empty() {
        ui.focus_list = order.clone();
    }
    // Drop dead entities.
    let live: Vec<Entity> = order.into_iter().filter(|e| all.get(*e).is_ok()).collect();
    let n = live.len();
    if combat_idle || n == 0 {
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
    let mut esc = false;
    for pad in &pads {
        if pad.just_pressed(GamepadButton::DPadRight) || pad.just_pressed(GamepadButton::DPadDown) {
            step = 1;
        }
        if pad.just_pressed(GamepadButton::DPadLeft) || pad.just_pressed(GamepadButton::DPadUp) {
            step = -1;
        }
        go |= pad.just_pressed(GamepadButton::South);
        esc |= pad.just_pressed(GamepadButton::East);
    }
    if esc && g.modal != Modal::None {
        g.cmds.push(Cmd::Close);
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
            g.cmds.push(act.0);
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

// ---------------------------------------------------------------------------------------------
// Floaters, banner, toast
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn floaters_and_banner(
    mut g: ResMut<DeckGame>,
    ui: Res<DeckUi>,
    cameras: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut nodes: Query<(&FloaterNode, &mut Node, &mut Text, &mut TextColor, &mut Visibility, &mut TextFont), (Without<BannerText>, Without<ToastText>)>,
    mut banner: Query<(&mut Node, &mut Text, &mut TextColor, &mut Visibility), (With<BannerText>, Without<FloaterNode>, Without<ToastText>)>,
    mut toast: Query<(&mut Node, &mut Text, &mut Visibility), (With<ToastText>, Without<FloaterNode>, Without<BannerText>)>,
) {
    let _ = &ui;
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let t = g.t;
    g.floaters.retain(|f| t - f.t0 < 1.3);
    let cam = cameras.single().ok();
    for (fl, mut node, mut text, mut color, mut vis, mut tf) in &mut nodes {
        let f = g.floaters.get(fl.0);
        match (f, cam) {
            (Some(f), Some((camera, gt))) => {
                let age = (t - f.t0) as f32;
                let rise = age * 34.0;
                if let Ok(p) = camera.world_to_viewport(gt, f.pos) {
                    if text.0 != f.text {
                        text.0 = f.text.clone();
                    }
                    let size = if f.big { 26.0 } else { 17.0 };
                    if tf.font_size != size {
                        tf.font_size = size;
                    }
                    let w = f.text.len() as f32 * size * 0.28;
                    node.left = Val::Px(p.x - w);
                    node.top = Val::Px(p.y - rise - 20.0);
                    let alpha = (1.0 - ((age - 0.8) / 0.5).clamp(0.0, 1.0)).clamp(0.0, 1.0);
                    color.0 = f.color.with_alpha(alpha);
                    *vis = Visibility::Inherited;
                    continue;
                }
                *vis = Visibility::Hidden;
            }
            _ => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
    if let Ok((mut node, mut text, mut color, mut vis)) = banner.single_mut() {
        match &g.banner {
            Some((s, t0, c)) => {
                let age = (t - t0) as f32;
                if text.0 != *s {
                    text.0 = s.clone();
                }
                let w = s.len() as f32 * 11.0;
                node.left = Val::Px(win.x / 2.0 - w);
                node.top = Val::Px(win.y * 0.30 - age * 8.0);
                let alpha = (1.0 - ((age - 0.9) / 0.5).clamp(0.0, 1.0)).min((age / 0.12).clamp(0.0, 1.0));
                color.0 = c.with_alpha(alpha);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
    if let Ok((mut node, mut text, mut vis)) = toast.single_mut() {
        match &g.toast {
            Some((s, _)) => {
                if text.0 != *s {
                    text.0 = s.clone();
                }
                node.left = Val::Px(win.x / 2.0 - s.len() as f32 * 3.4);
                node.top = Val::Px(win.y * 0.5);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Map
// ---------------------------------------------------------------------------------------------

fn node_color(k: NodeKind) -> Color {
    match k {
        NodeKind::Battle => hex(0xc9462e),
        NodeKind::Elite => hex(0x7b3fd0),
        NodeKind::Shop => hex(0xd9a21b),
        NodeKind::Rest => hex(0xe8742a),
        NodeKind::Event => hex(0x2f9fd8),
        NodeKind::Treasure => hex(0x2d9d5a),
        NodeKind::Boss => hex(0xb01c1c),
    }
}

pub fn map_pos(n: &Node_, g: &DeckGame, rect: Rect) -> Vec2 {
    let _ = g;
    let cols = (BOSS_ROW + 1) as f32;
    let x = rect.min.x + (n.row as f32 + 0.5) / cols * rect.width();
    let y = rect.min.y + n.x * rect.height();
    Vec2::new(x, y)
}

type Node_ = super::deck_run::Node;

fn build_map(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame, win: Vec2) -> Entity {
    let root = screen_root(commands, area, Color::srgba(0.02, 0.03, 0.12, 0.8));
    let top = 40.0;
    let bottom = 34.0;
    let rect = Rect::new(area.left + 10.0, area.top + top + 8.0, win.x - area.right - 10.0, win.y - area.bottom - bottom - 6.0);
    let avail = g.run.available();
    commands.entity(root).with_children(|p| {
        top_bar(p, a, g, true);
        // Edges as dotted trails.
        for n in &g.run.map.nodes {
            for t in &n.next {
                let (p0, p1) = (map_pos(n, g, rect_local(rect, area, top)), map_pos(&g.run.map.nodes[*t], g, rect_local(rect, area, top)));
                let walked = g.run.cleared.contains(&n.id) && (g.run.pos == Some(*t) || g.run.cleared.contains(t));
                let live = g.run.pos == Some(n.id) && avail.contains(t) || (g.run.pos.is_none() && false);
                let dots = ((p1 - p0).length() / 9.0) as i32;
                for k in 1..dots {
                    let q = p0.lerp(p1, k as f32 / dots as f32);
                    p.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(q.x - 2.0), top: Val::Px(q.y - 2.0), width: Val::Px(4.0), height: Val::Px(4.0), ..default() }, BackgroundColor(if walked { GOLD_HI } else if live { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.28) }), BorderRadius::all(Val::Px(2.0))));
                }
            }
        }
        for n in &g.run.map.nodes {
            let q = map_pos(n, g, rect_local(rect, area, top));
            let can = avail.contains(&n.id);
            let done = g.run.cleared.contains(&n.id);
            let here = g.run.pos == Some(n.id);
            let size = if n.kind == NodeKind::Boss { 58.0 } else { 44.0 };
            let col = node_color(n.kind);
            let mut e = p.spawn((
                Button,
                Node { position_type: PositionType::Absolute, left: Val::Px(q.x - size / 2.0), top: Val::Px(q.y - size / 2.0), width: Val::Px(size), height: Val::Px(size), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(if can { 4.0 } else { 2.0 })), ..default() },
                BackgroundColor(if done { col.with_alpha(0.35) } else if can { col } else { col.with_alpha(0.55) }),
                BorderColor(if can { Color::WHITE } else if here { GOLD_HI } else { NAVY }),
                BorderRadius::all(Val::Px(99.0)),
                Outline { width: Val::Px(0.0), offset: Val::Px(3.0), color: GOLD_HI },
            ));
            if can {
                e.insert(Act(Cmd::Node(n.id)));
            }
            e.with_children(|b| {
                b.spawn((Text::new(if done && !here { "ok" } else { n.kind.glyph() }), font(&a.display, if n.kind == NodeKind::Boss { 28.0 } else { 22.0 }), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
            });
            if can || here {
                p.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(q.x - 40.0), top: Val::Px(q.y + size / 2.0 + 1.0), width: Val::Px(80.0), justify_content: JustifyContent::Center, ..default() })).with_children(|c| {
                    c.spawn(body(a, if here { "YOU ARE HERE" } else { n.kind.name() }, 9.5, if here { GOLD_HI } else { CREAM }));
                });
            }
        }
        // Legend.
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(area.left + 10.0), bottom: Val::Px(area.bottom + 4.0), column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
            for k in [NodeKind::Battle, NodeKind::Elite, NodeKind::Shop, NodeKind::Rest, NodeKind::Event, NodeKind::Treasure, NodeKind::Boss] {
                r.spawn(Node { column_gap: Val::Px(3.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
                    c.spawn((Node { width: Val::Px(14.0), height: Val::Px(14.0), ..default() }, BackgroundColor(node_color(k)), BorderRadius::all(Val::Px(7.0))));
                    c.spawn(body(a, k.name(), 10.0, CREAM));
                });
            }
        });
        p.spawn(Node { position_type: PositionType::Absolute, right: Val::Px(area.right + 10.0), bottom: Val::Px(area.bottom + 4.0), ..default() }).with_children(|r| {
            r.spawn(body(a, &format!("Best: {} stops   Score {}", g.saved.best_depth, g.saved.best_score), 10.0, GOLD_HI));
        });
    });
    root
}

/// The map rect is expressed in window space; nodes are children of the padded root, so shift by the padding.
fn rect_local(rect: Rect, area: &SafeArea, _top: f32) -> Rect {
    Rect::new(rect.min.x - area.left, rect.min.y - area.top, rect.max.x - area.left, rect.max.y - area.top)
}

// ---------------------------------------------------------------------------------------------
// Reward
// ---------------------------------------------------------------------------------------------

fn build_reward(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame) -> Entity {
    let root = screen_root(commands, area, Color::srgba(0.02, 0.03, 0.12, 0.55));
    let Some(r) = g.run.reward.clone() else { return root };
    let boss = g.run.pos.is_some_and(|p| g.run.map.nodes[p].kind == NodeKind::Boss);
    commands.entity(root).with_children(|p| {
        top_bar(p, a, g, true);
        p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::Center, margin: UiRect::top(Val::Px(2.0)), ..default() }).with_children(|c| {
            outlined(c, a, if boss { "THE FUSE KING FALLS!" } else { "VICTORY!" }, 26.0, GOLD);
        });
        p.spawn(Node { width: Val::Percent(100.0), flex_grow: 1.0, align_items: AlignItems::Center, justify_content: JustifyContent::Center, column_gap: Val::Px(16.0), ..default() }).with_children(|row| {
            // Loot column.
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), width: Val::Px(150.0), ..default() }).with_children(|c| {
                let gold_label = if r.gold_taken { "Gold taken".to_string() } else { format!("+{} COINS", r.gold) };
                let mut b = act_button(c, a, if r.gold_taken { ButtonKind::Chip } else { ButtonKind::Gold }, 150.0, 44.0, 16.0, &gold_label, Cmd::TakeGold);
                if r.gold_taken {
                    b.insert(Disabled);
                }
                if let Some(rl) = r.relic {
                    let mut b = act_button(c, a, if r.relic_taken { ButtonKind::Chip } else { ButtonKind::Blue }, 150.0, 56.0, 12.0, &format!("RELIC: {}", rl.name()), Cmd::TakeRelic);
                    if r.relic_taken {
                        b.insert(Disabled);
                    }
                    c.spawn(body(a, rl.desc(), 9.5, CREAM)).insert(Node { max_width: Val::Px(150.0), ..default() });
                }
                if let Some(po) = r.potion {
                    let full = !g.run.potions.iter().any(|s| s.is_none());
                    let mut b = act_button(c, a, if r.potion_taken || full { ButtonKind::Chip } else { ButtonKind::Green }, 150.0, 44.0, 12.0, &format!("POTION: {}", po.name()), Cmd::TakePotion);
                    if r.potion_taken || full {
                        b.insert(Disabled);
                    }
                }
            });
            // Card choices.
            if !boss && !r.card_done {
                row.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(4.0), ..default() }).with_children(|c| {
                    c.spawn(body(a, "Add a card to your deck", 12.0, CREAM));
                    c.spawn(Node { column_gap: Val::Px(12.0), ..default() }).with_children(|cards| {
                        for (i, card) in r.cards.iter().enumerate() {
                            cards.spawn((Button, Act(Cmd::RewardCard(i)), Node { ..default() }, Outline { width: Val::Px(0.0), offset: Val::Px(3.0), color: GOLD_HI }, BackgroundColor(Color::NONE))).with_children(|b| {
                                card_face(b, a, *card, 1.18, 0);
                            });
                        }
                    });
                    act_button(c, a, ButtonKind::Blue, 120.0, 30.0, 13.0, "SKIP CARD", Cmd::SkipCard);
                });
            } else if !boss {
                row.spawn(body(a, if r.cards.is_empty() { "" } else { "Card added to your deck." }, 14.0, hex(0xb6ff9a)));
            }
        });
        p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::Center, margin: UiRect::bottom(Val::Px(4.0)), ..default() }).with_children(|c| {
            act_button(c, a, ButtonKind::Gold, 190.0, 40.0, 17.0, if boss { "CLAIM VICTORY" } else { "CONTINUE" }, Cmd::LeaveReward);
        });
    });
    root
}

// ---------------------------------------------------------------------------------------------
// Shop
// ---------------------------------------------------------------------------------------------

fn build_shop(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame) -> Entity {
    let root = screen_root(commands, area, Color::srgba(0.02, 0.03, 0.12, 0.5));
    let Some(shop) = g.run.shop.clone() else { return root };
    let coins = g.run.coins;
    commands.entity(root).with_children(|p| {
        top_bar(p, a, g, true);
        p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() }).with_children(|c| {
            outlined(c, a, "TOAD'S SHOP", 22.0, GOLD);
        });
        p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::Center, column_gap: Val::Px(8.0), margin: UiRect::top(Val::Px(4.0)), ..default() }).with_children(|row| {
            for (i, (card, price, sold)) in shop.cards.iter().enumerate() {
                row.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(3.0), ..default() }).with_children(|c| {
                    c.spawn((Button, Act(Cmd::BuyCard(i)), Node::default(), Outline { width: Val::Px(0.0), offset: Val::Px(3.0), color: GOLD_HI }, BackgroundColor(Color::NONE))).with_children(|b| {
                        card_face(b, a, *card, 0.9, 0);
                        if *sold {
                            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.05, 0.7)), BorderRadius::all(Val::Px(9.0)))).with_children(|s| {
                                s.spawn(disp_text(a, "SOLD", 18.0, Color::WHITE));
                            });
                        }
                    });
                    price_tag(c, a, *price, coins >= *price && !*sold);
                });
            }
        });
        p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, column_gap: Val::Px(8.0), margin: UiRect::top(Val::Px(6.0)), ..default() }).with_children(|row| {
            for (i, (rl, price, sold)) in shop.relics.iter().enumerate() {
                shop_item(row, a, &format!("{}\n{}", rl.name(), rl.desc()), *price, coins, *sold, Cmd::BuyRelic(i), 150.0);
            }
            for (i, (po, price, sold)) in shop.potions.iter().enumerate() {
                shop_item(row, a, &format!("{}\n{}", po.name(), po.desc()), *price, coins, *sold, Cmd::BuyPotion(i), 110.0);
            }
            let rp = g.run.removal_price();
            shop_item(row, a, "Remove a card\nPurge one from your deck", rp, coins, shop.removal_used, Cmd::Open(Modal::PickRemove), 120.0);
        });
        p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::Center, margin: UiRect::top(Val::Px(4.0)), ..default() }).with_children(|c| {
            act_button(c, a, ButtonKind::Gold, 170.0, 36.0, 16.0, "LEAVE SHOP", Cmd::LeaveShop);
        });
    });
    root
}

fn price_tag(p: &mut ChildSpawnerCommands, a: &UiAssets, price: i32, ok: bool) {
    p.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(3.0), ..default() }).with_children(|c| {
        icon(c, &a.coin, 14.0);
        c.spawn(body(a, &price.to_string(), 13.0, if ok { GOLD_HI } else { hex(0xff8a7a) }));
    });
}

#[allow(clippy::too_many_arguments)]
fn shop_item(row: &mut ChildSpawnerCommands, a: &UiAssets, text: &str, price: i32, coins: i32, sold: bool, cmd: Cmd, w: f32) {
    let (title, desc) = text.split_once('\n').unwrap_or((text, ""));
    row.spawn((Button, Act(cmd), Node { width: Val::Px(w), height: Val::Px(66.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, padding: UiRect::all(Val::Px(4.0)), border: UiRect::all(Val::Px(2.0)), row_gap: Val::Px(1.0), ..default() }, BackgroundColor(if sold { hex(0x10183a) } else { hex(0x1d2f78) }), BorderColor(if sold { NAVY } else { GOLD_LO }), BorderRadius::all(Val::Px(10.0)), Outline { width: Val::Px(0.0), offset: Val::Px(3.0), color: GOLD_HI })).with_children(|c| {
        c.spawn(disp_text(a, title, 12.0, if sold { hex(0x7080a8) } else { CREAM }));
        c.spawn((Text::new(clean(desc.to_string())), font(&a.body, 8.5), TextColor(Color::WHITE.with_alpha(0.85)), TextLayout::new_with_justify(JustifyText::Center), Node { max_width: Val::Px(w - 10.0), ..default() }));
        if sold {
            c.spawn(body(a, "SOLD", 10.0, hex(0x7080a8)));
        } else {
            price_tag(c, a, price, coins >= price);
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Rest, event, treasure
// ---------------------------------------------------------------------------------------------

fn side_panel(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame, fill: impl FnOnce(&mut ChildSpawnerCommands)) -> Entity {
    let root = screen_root(commands, area, Color::NONE);
    commands.entity(root).with_children(|p| {
        top_bar(p, a, g, true);
        p.spawn(Node { width: Val::Percent(100.0), flex_grow: 1.0, justify_content: JustifyContent::FlexEnd, align_items: AlignItems::Center, padding: UiRect::right(Val::Px(14.0)), ..default() }).with_children(|row| {
            panel(row, a, Node { width: Val::Px(360.0), padding: UiRect::axes(Val::Px(18.0), Val::Px(12.0)), row_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|c| fill(c));
        });
    });
    root
}

fn build_rest(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame) -> Entity {
    let rested = g.run.rested;
    let heal = (g.run.max_hp as f32 * 0.3).round() as i32;
    side_panel(commands, a, area, g, |c| {
        kicker(c, a, "CAMPFIRE");
        outlined(c, a, "A quiet fire", 24.0, GOLD);
        c.spawn((Text::new(clean("The flames hold off the dark. Rest your bones, or sharpen a move.".to_string())), font(&a.body, 12.0), TextColor(CREAM), TextLayout::new_with_justify(JustifyText::Center), Node { width: Val::Percent(100.0), ..default() }));
        if !rested {
            act_button(c, a, ButtonKind::Green, 300.0, 48.0, 16.0, &format!("REST: heal {heal} HP"), Cmd::RestHeal);
            let can_up = g.run.deck.iter().any(|d| d.can_upgrade());
            let mut b = act_button(c, a, ButtonKind::Blue, 300.0, 48.0, 16.0, "SMITH: upgrade a card", Cmd::Open(Modal::PickUpgrade));
            if !can_up {
                b.insert(Disabled);
            }
        } else {
            c.spawn(body(a, "You feel better.", 15.0, hex(0xb6ff9a)));
        }
        act_button(c, a, ButtonKind::Gold, 200.0, 40.0, 16.0, "BREAK CAMP", Cmd::LeaveRest);
    })
}

fn build_event(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame) -> Entity {
    let Some(ev) = g.run.event else { return screen_root(commands, a_none(), Color::NONE) };
    let result = g.run.event_result.clone();
    let run = g.run.clone();
    side_panel(commands, a, area, g, move |c| {
        kicker(c, a, "MYSTERY");
        outlined(c, a, ev.title(), 20.0, GOLD);
        c.spawn((Text::new(clean(ev.text().to_string())), font(&a.body, 12.0), TextColor(CREAM), TextLayout::new_with_justify(JustifyText::Center), Node { width: Val::Percent(100.0), ..default() }));
        match &result {
            None => {
                for (i, (label, detail)) in ev.options().iter().enumerate() {
                    let enabled = run.event_enabled(i);
                    let mut b = button(c, a, ButtonSpec::new(if i + 1 == ev.options().len() { ButtonKind::Chip } else { ButtonKind::Blue }, Val::Px(320.0), Val::Px(46.0), 13.0), "");
                    b.insert(Act(Cmd::EventChoice(i)));
                    if !enabled {
                        b.insert(Disabled);
                    }
                    b.despawn_related::<Children>();
                    b.with_children(|bb| {
                        bb.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() }).with_children(|t| {
                            t.spawn(disp_text(a, label, 14.0, Color::WHITE));
                            t.spawn(body(a, detail, 9.5, CREAM));
                        });
                    });
                }
            }
            Some(msg) => {
                c.spawn((Text::new(clean(msg.clone())), font(&a.display, 16.0), TextColor(hex(0xb6ff9a)), TextLayout::new_with_justify(JustifyText::Center), Node { width: Val::Percent(100.0), margin: UiRect::vertical(Val::Px(6.0)), ..default() }));
                act_button(c, a, ButtonKind::Gold, 200.0, 40.0, 16.0, "CONTINUE", Cmd::LeaveEvent);
            }
        }
    })
}

fn a_none() -> &'static SafeArea {
    static ZERO: SafeArea = SafeArea { left: 0.0, right: 0.0, bottom: 0.0, top: 0.0 };
    &ZERO
}

fn build_treasure(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame) -> Entity {
    let relic = g.run.treasure;
    side_panel(commands, a, area, g, |c| {
        kicker(c, a, "TREASURE");
        outlined(c, a, "A dusty chest", 24.0, GOLD);
        match relic {
            Some(r) => {
                c.spawn(disp_text(a, r.name(), 20.0, GOLD_HI));
                c.spawn((Text::new(clean(r.desc().to_string())), font(&a.body, 13.0), TextColor(CREAM), TextLayout::new_with_justify(JustifyText::Center), Node { width: Val::Percent(100.0), ..default() }));
            }
            None => {
                c.spawn(body(a, "Only coins inside.", 14.0, CREAM));
            }
        }
        act_button(c, a, ButtonKind::Gold, 240.0, 44.0, 17.0, "TAKE EVERYTHING", Cmd::TakeTreasure);
    })
}

// ---------------------------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------------------------

fn build_summary(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame) -> Entity {
    let root = screen_root(commands, area, Color::srgba(0.04, 0.0, 0.1, 0.45));
    let won = g.run.won();
    let s = g.run.stats;
    commands.entity(root).with_children(|p| {
        p.spawn(Node { width: Val::Percent(100.0), flex_grow: 1.0, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|row| {
            panel(row, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(12.0)), row_gap: Val::Px(4.0), width: Val::Px(420.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
                kicker(c, a, "STAR DECK");
                outlined(c, a, if won { "ACT CLEARED!" } else { "RUN OVER" }, 34.0, if won { GOLD } else { RED_HI });
                if g.record_new {
                    c.spawn(disp_text(a, "NEW BEST!", 16.0, hex(0xb6ff9a)));
                }
                let mut row = |label: &str, value: String, color: Color| {
                    c.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                        r.spawn(body(a, label, 13.0, CREAM));
                        r.spawn(disp_text(a, &value, 16.0, color));
                    });
                };
                row("Hero", g.hero_name().to_string(), Color::WHITE);
                row("Stops cleared", format!("{} / {}", s.nodes_cleared, BOSS_ROW + 1), Color::WHITE);
                row("Battles won", s.battles.to_string(), Color::WHITE);
                row("Longest chain", format!("x{}", s.best_chain + 1), Color::WHITE);
                row("Cards played", s.cards_played.to_string(), Color::WHITE);
                row("Deck size", g.run.deck.len().to_string(), Color::WHITE);
                row("Score", g.run.score().to_string(), GOLD_HI);
                row("Best score", g.saved.best_score.to_string(), GOLD_HI);
                c.spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(6.0)), ..default() }).with_children(|r| {
                    act_button(r, a, ButtonKind::Gold, 116.0, 44.0, 18.0, "RETRY", Cmd::Retry);
                    act_button(r, a, if g.daily { ButtonKind::Green } else { ButtonKind::Blue }, 116.0, 44.0, 13.0, if g.daily { "DAILY: ON" } else { "DAILY: OFF" }, Cmd::ToggleDaily);
                    act_button(r, a, ButtonKind::Blue, 116.0, 44.0, 13.0, "MAIN MENU", Cmd::Menu);
                });
                c.spawn(body(a, if g.daily { "Retry replays today's shared seed" } else { "Daily: one shared map per UTC day" }, 10.0, CREAM));
            });
        });
    });
    root
}

// ---------------------------------------------------------------------------------------------
// Modals: piles, deck, pickers
// ---------------------------------------------------------------------------------------------

fn build_help(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> Entity {
    let root = commands
        .spawn((XpEntity, GlobalZIndex(Z + 2), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), padding: UiRect { left: Val::Px(area.left + 10.0), right: Val::Px(area.right + 10.0), top: Val::Px(area.top + 6.0), bottom: Val::Px(area.bottom + 4.0) }, flex_direction: FlexDirection::Column, ..default() }, BackgroundColor(Color::srgba(0.02, 0.03, 0.1, 0.94))))
        .id();
    let rules = [
        ("Momentum", "Cards spend momentum (the meter, 0-8). It never fades below your stride (3)."),
        ("Chains", "Play Jump, then Dive, then Pound, then Jump...: every link refunds 1 momentum and adds +2 damage per chain level. A gold ring marks a card that links."),
        ("Breaks", "Strike and Spin cards break the chain. Brace and Trick cards leave it alone. Star Spin (X) spends everything."),
        ("Shell", "Blocks damage; clears at the start of its owner's turn. Spiky punishes attackers."),
        ("Intents", "Each foe shows what it will do next: damage as it will land after statuses, guard, buff, hex, or !! for a warning."),
    ];
    commands.entity(root).with_children(|p| {
        p.spawn(Node { width: Val::Percent(100.0), height: Val::Px(36.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, padding: UiRect::right(Val::Px(52.0)), ..default() }).with_children(|r| {
            r.spawn(disp_text(a, "HOW TO PLAY", 20.0, GOLD));
            act_button(r, a, ButtonKind::Blue, 90.0, 30.0, 14.0, "CLOSE", Cmd::Close);
        });
        p.spawn(Node { width: Val::Percent(100.0), flex_grow: 1.0, column_gap: Val::Px(18.0), ..default() }).with_children(|cols| {
            cols.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), width: Val::Percent(52.0), ..default() }).with_children(|c| {
                for (t, d) in rules {
                    c.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|b| {
                        b.spawn(disp_text(a, t, 13.0, GOLD_HI));
                        b.spawn((Text::new(clean(d.to_string())), font(&a.body, 11.0), TextColor(CREAM), Node { width: Val::Percent(100.0), ..default() }));
                    });
                }
            });
            cols.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), flex_grow: 1.0, ..default() }).with_children(|c| {
                c.spawn(disp_text(a, "Statuses", 13.0, GOLD_HI));
                for s in STATUSES {
                    c.spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                        chip(r, a, s.tag(), if s.debuff() { hex(0xa6401e) } else { hex(0x2d8a4a) }, 9.5);
                        r.spawn((Text::new(clean(format!("{}: {}", s.name(), s.desc()))), font(&a.body, 10.0), TextColor(CREAM), Node { max_width: Val::Px(300.0), ..default() }));
                    });
                }
            });
        });
    });
    root
}

fn build_modal(commands: &mut Commands, a: &UiAssets, area: &SafeArea, g: &DeckGame, win: Vec2) -> Entity {
    if g.modal == Modal::Help {
        return build_help(commands, a, area);
    }
    let _ = win;
    let (title, cards, pick): (String, Vec<Card>, Option<fn(usize) -> Cmd>) = match g.modal {
        Modal::Pile(pile) => {
            let mut cards: Vec<Card> = match (pile, g.combat()) {
                (Pile::Draw, Some(c)) => c.draw.iter().map(|i| i.card).collect(),
                (Pile::Discard, Some(c)) => c.discard.iter().map(|i| i.card).collect(),
                (Pile::Exhaust, Some(c)) => c.exhaust.iter().map(|i| i.card).collect(),
                _ => vec![],
            };
            // The draw pile is shown sorted: its order is a secret.
            cards.sort_by_key(|c| (c.def().rarity, c.id));
            (format!("{} PILE ({})", match pile { Pile::Draw => "DRAW", Pile::Discard => "DISCARD", Pile::Exhaust => "EXHAUST" }, cards.len()), cards, None)
        }
        Modal::PickUpgrade => ("UPGRADE WHICH CARD?".to_string(), g.run.deck.iter().map(|c| if c.can_upgrade() { Card { up: true, ..*c } } else { *c }).collect(), Some(Cmd_RestUpgrade as fn(usize) -> Cmd)),
        Modal::PickRemove => (format!("REMOVE WHICH CARD? ({} coins)", g.run.removal_price()), g.run.deck.clone(), Some(Cmd_RemoveCard as fn(usize) -> Cmd)),
        _ => (format!("YOUR DECK ({})", g.run.deck.len()), g.run.deck.clone(), None),
    };
    let upgrade_mode = g.modal == Modal::PickUpgrade;
    let deck = g.run.deck.clone();
    let root = commands
        .spawn((XpEntity, GlobalZIndex(Z + 2), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), padding: UiRect { left: Val::Px(area.left + 8.0), right: Val::Px(area.right + 8.0), top: Val::Px(area.top + 6.0), bottom: Val::Px(area.bottom + 4.0) }, flex_direction: FlexDirection::Column, ..default() }, BackgroundColor(Color::srgba(0.02, 0.03, 0.1, 0.92))))
        .id();
    commands.entity(root).with_children(|p| {
        p.spawn(Node { width: Val::Percent(100.0), height: Val::Px(36.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, padding: UiRect::right(Val::Px(52.0)), ..default() }).with_children(|r| {
            r.spawn(disp_text(a, &title, 20.0, GOLD));
            act_button(r, a, ButtonKind::Blue, 90.0, 30.0, 14.0, "CLOSE", Cmd::Close);
        });
        p.spawn((ScrollBox, Node { width: Val::Percent(100.0), flex_grow: 1.0, min_height: Val::Px(0.0), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(8.0), row_gap: Val::Px(10.0), overflow: Overflow::scroll_y(), padding: UiRect::all(Val::Px(6.0)), align_content: AlignContent::FlexStart, justify_content: JustifyContent::Center, ..default() }, ScrollPosition::default())).with_children(|grid| {
            if cards.is_empty() {
                grid.spawn(body(a, "Nothing here.", 16.0, CREAM));
            }
            for (i, c) in cards.iter().enumerate() {
                let enabled = match g.modal {
                    Modal::PickUpgrade => deck.get(i).is_some_and(|d| d.can_upgrade()),
                    _ => true,
                };
                let mut b = grid.spawn((Button, Node::default(), Outline { width: Val::Px(0.0), offset: Val::Px(3.0), color: GOLD_HI }, BackgroundColor(Color::NONE)));
                if let Some(f) = pick {
                    if enabled {
                        b.insert(Act(f(i)));
                    }
                }
                b.with_children(|bb| {
                    card_face(bb, a, *c, 0.92, 0);
                    if !enabled && upgrade_mode {
                        bb.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.05, 0.55)), BorderRadius::all(Val::Px(9.0))));
                    }
                });
            }
        });
    });
    root
}

#[allow(non_snake_case)]
fn Cmd_RestUpgrade(i: usize) -> Cmd {
    Cmd::RestUpgrade(i)
}
#[allow(non_snake_case)]
fn Cmd_RemoveCard(i: usize) -> Cmd {
    Cmd::RemoveCard(i)
}

/// Drag / wheel scrolling for the card grids.
fn modal_scroll(mut boxes: Query<(&mut ScrollPosition, &ComputedNode, &Children), With<ScrollBox>>, nodes: Query<&ComputedNode, Without<ScrollBox>>, mut wheel: EventReader<MouseWheel>, mut ui: ResMut<DeckUi>, windows: Query<&Window, With<PrimaryWindow>>, mouse: Res<ButtonInput<MouseButton>>, touches: Res<Touches>) {
    let Ok((mut pos, cn, kids)) = boxes.single_mut() else { return };
    let mut delta = 0.0;
    for e in wheel.read() {
        delta -= e.y * 30.0;
    }
    let ptr = pointer(&windows, &mouse, &touches);
    if ptr.pressed {
        ui.scroll_drag = ptr.pos.map(|p| (p.y, pos.offset_y));
    }
    if let (Some(p), true, Some((y0, off0))) = (ptr.pos, ptr.down, ui.scroll_drag) {
        pos.offset_y = off0 + (y0 - p.y);
    }
    if !ptr.down {
        ui.scroll_drag = None;
    }
    pos.offset_y += delta;
    // Clamp to the content.
    let inv = cn.inverse_scale_factor();
    let content_h = kids.iter().filter_map(|k| nodes.get(k).ok()).map(|n| n.size().y).fold(0.0f32, f32::max);
    let _ = content_h;
    let content = cn.content_size().y * inv;
    let view = cn.size().y * inv;
    let max = (content - view).max(0.0);
    pos.offset_y = pos.offset_y.clamp(0.0, max);
}
