//! Relic Descent presentation, UI half (bevy_ui): the HUD (health, shield, spark, XP, floor,
//! gold, potions, minimap, boss bar, desktop skill bar), the pack screen (equipment, bag, item
//! compare card with equip / sell), the skill-tree screen and the end-of-run summary.
//! Data flows through [`RelicBridge`]; nothing here touches the rules.
//!
//! Screens are rebuilt when the bridge's snapshot generation or the selection changes (a few
//! dozen nodes, only on a tap); the HUD only updates values that changed.
use crate::button_marker::{Button, BorderRadius};
use super::relic_core::*;
use super::relic_loot::*;
use super::relic_view::*;
use super::XpEntity;
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::shell::{AppState, ReturnToMenu};
use crate::touch::TouchControls;
use bevy::prelude::*;

pub struct RelicUiPlugin;

impl Plugin for RelicUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>().add_systems(
            Update,
            (manage_ui, update_hud, screen_buttons, screen_keys, overlay_display).chain().after(super::run_frame).run_if(|b: Res<RelicBridge>| b.active),
        );
        app.add_systems(Update, ui_cleanup.after(super::run_frame).run_if(|b: Res<RelicBridge>| !b.active));
    }
}

#[derive(Component)]
struct Overlay;
#[derive(Component)]
struct ScreenRoot;
#[derive(Component)]
struct SummaryRoot;
#[derive(Component, Clone, Copy, PartialEq)]
enum Act {
    Equip(u32),
    Sell(u32),
    Unequip(usize),
    SellUpTo(Rarity),
    Filter,
    Close,
    OpenTree,
    OpenBag,
    SelectBag(u32),
    SelectEquip(usize),
    Rank(Perk),
    Retry,
    Menu,
}

#[derive(Component)]
struct Selectable;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Sel {
    #[default]
    None,
    Bag(u32),
    Equip(usize),
}

/// Entities the HUD updates in place.
#[derive(Default, Clone, Copy)]
struct Refs {
    hp_fill: Option<Entity>,
    hp_text: Option<Entity>,
    shield_fill: Option<Entity>,
    spark_fill: Option<Entity>,
    xp_fill: Option<Entity>,
    level_text: Option<Entity>,
    info_text: Option<Entity>,
    boss_root: Option<Entity>,
    boss_fill: Option<Entity>,
    boss_name: Option<Entity>,
    stairs_root: Option<Entity>,
    stairs_fill: Option<Entity>,
    hint: Option<Entity>,
    skillbar: Option<Entity>,
    minimap: Option<Entity>,
    mini_player: Option<Entity>,
    mini_exit: Option<Entity>,
    vignette: Option<Entity>,
    skill_fill: [Option<Entity>; 4],
    skill_label: [Option<Entity>; 4],
    mini_rooms: [Option<Entity>; 12],
}

#[derive(Resource, Default)]
struct UiState {
    hud: Option<Entity>,
    refs: Refs,
    screen: Option<(Entity, Screen, u32, Sel)>,
    summary: Option<Entity>,
    sel: Sel,
}

fn root_node(area: &SafeArea) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        padding: UiRect { left: Val::Px(area.left), right: Val::Px(area.right), top: Val::Px(area.top), bottom: Val::Px(area.bottom) },
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn text(a: &UiAssets, s: &str, size: f32, color: Color) -> impl Bundle {
    (Text::new(clean(s.to_string())), font(&a.body, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY })
}

fn bar(p: &mut ChildSpawnerCommands, width: f32, height: f32, fill: Color) -> Entity {
    let mut inner = Entity::PLACEHOLDER;
    p.spawn((Node { width: Val::Px(width), height: Val::Px(height), border: UiRect::all(Val::Px(2.0)), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgba(0.03, 0.05, 0.16, 0.8)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(height / 2.0)))).with_children(|b| {
        inner = b.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(fill))).id();
    });
    inner
}

fn rar(r: Rarity) -> Color {
    rarity_color(r as u8)
}

fn spawn_hud(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> (Entity, Refs) {
    let mut refs = Refs::default();
    let mut node = root_node(area);
    node.padding.top = Val::Px(area.top.max(0.0) + 6.0);
    node.padding.left = Val::Px(area.left.max(8.0));
    node.align_items = AlignItems::FlexStart;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(12), node)).id();
    commands.entity(root).with_children(|p| {
        // Health, shield, spark, XP, info.
        p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() }).with_children(|col| {
            col.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                r.spawn((Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(GOLD), BorderColor::all(NAVY), BorderRadius::all(Val::Px(99.0)))).with_children(|l| {
                    refs.level_text = Some(l.spawn((Text::new("LV 1"), font(&a.display, 13.0), TextColor(NAVY))).id());
                });
                r.spawn(Node { width: Val::Px(210.0), height: Val::Px(20.0), ..default() }).with_children(|h| {
                    refs.hp_fill = Some(bar(h, 210.0, 20.0, hex(0x39c24a)));
                    h.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|t| {
                        refs.hp_text = Some(t.spawn(text(a, "100 / 100", 12.0, Color::WHITE)).id());
                    });
                });
            });
            col.spawn(Node { margin: UiRect::left(Val::Px(50.0)), flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() }).with_children(|c| {
                refs.shield_fill = Some(bar(c, 210.0, 7.0, hex(0x6fd0ff)));
                refs.spark_fill = Some(bar(c, 210.0, 10.0, hex(0x5a7bff)));
                refs.xp_fill = Some(bar(c, 210.0, 7.0, hex(0xd6a8ff)));
            });
            refs.info_text = Some(col.spawn((text(a, "", 13.0, GOLD_HI), Node { margin: UiRect::left(Val::Px(50.0)), ..default() })).id());
            refs.hint = Some(col.spawn((text(a, "Walk into rooms to wake their foes. B: attack or dive (run + B). Jump then Z: ground pound.", 11.0, CREAM), Node { max_width: Val::Px(340.0), margin: UiRect::new(Val::Px(0.0), Val::Px(0.0), Val::Px(6.0), Val::Px(0.0)), ..default() })).id());
        });
    });
    // Boss bar, top centre.
    let boss = commands.spawn((XpEntity, Overlay, GlobalZIndex(12), Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(Val::Px(-120.0)), top: Val::Px(area.top.max(0.0) + 6.0), width: Val::Px(240.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, display: Display::None, ..default() })).id();
    commands.entity(boss).with_children(|b| {
        refs.boss_name = Some(b.spawn((Text::new("BOMB KING"), font(&a.display, 17.0), TextColor(RED_HI), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY })).id());
        refs.boss_fill = Some(bar(b, 240.0, 14.0, hex(0xe5302a)));
    });
    refs.boss_root = Some(boss);
    // Stairs progress, lower centre.
    let stairs = commands.spawn((XpEntity, Overlay, GlobalZIndex(12), Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(Val::Px(-90.0)), bottom: Val::Px(area.bottom.max(0.0) + 120.0), width: Val::Px(180.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, display: Display::None, ..default() })).id();
    commands.entity(stairs).with_children(|b| {
        b.spawn(text(a, "TAKING THE STAIRS...", 13.0, GOLD_HI));
        refs.stairs_fill = Some(bar(b, 180.0, 10.0, hex(0xffcc33)));
    });
    refs.stairs_root = Some(stairs);
    // Minimap, top right, left of the pause button.
    let mini = commands.spawn((XpEntity, Overlay, GlobalZIndex(11), Node { position_type: PositionType::Absolute, right: Val::Px(area.right.max(14.0) + 54.0), top: Val::Px(area.top.max(0.0) + 8.0), width: Val::Px(122.0), height: Val::Px(92.0), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(Color::srgba(0.02, 0.04, 0.12, 0.62)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(8.0)))).id();
    commands.entity(mini).with_children(|m| {
        for i in 0..12 {
            refs.mini_rooms[i] = Some(m.spawn((Node { position_type: PositionType::Absolute, display: Display::None, ..default() }, BackgroundColor(Color::srgba(0.4, 0.4, 0.6, 0.6)))).id());
        }
        refs.mini_exit = Some(m.spawn((Node { position_type: PositionType::Absolute, width: Val::Px(7.0), height: Val::Px(7.0), display: Display::None, ..default() }, BackgroundColor(GOLD_HI), BorderRadius::MAX)).id());
        refs.mini_player = Some(m.spawn((Node { position_type: PositionType::Absolute, width: Val::Px(7.0), height: Val::Px(7.0), ..default() }, BackgroundColor(Color::WHITE), BorderColor::from(hex(0x2a8f37)), BorderRadius::MAX)).id());
    });
    refs.minimap = Some(mini);
    // Desktop skill bar (hidden when the touch buttons show).
    let bar_root = commands.spawn((XpEntity, Overlay, GlobalZIndex(12), Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(Val::Px(-150.0)), bottom: Val::Px(area.bottom.max(0.0) + 10.0), width: Val::Px(300.0), column_gap: Val::Px(8.0), justify_content: JustifyContent::Center, ..default() })).id();
    commands.entity(bar_root).with_children(|b| {
        for (i, s) in SKILLS.iter().enumerate() {
            b.spawn((Node { width: Val::Px(62.0), height: Val::Px(48.0), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }, BackgroundColor(hex(0x2b1a55)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(10.0)))).with_children(|k| {
                refs.skill_fill[i] = Some(k.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), height: Val::Percent(0.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)))).id());
                refs.skill_label[i] = Some(k.spawn(text(a, &format!("[{}] {}", i + 1, s.short()), 11.0, Color::WHITE)).id());
            });
        }
    });
    refs.skillbar = Some(bar_root);
    // Red flash at low health.
    let v = commands.spawn((XpEntity, Overlay, GlobalZIndex(9), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), border: UiRect::all(Val::Px(26.0)), display: Display::None, ..default() }, BorderColor::from(Color::srgba(0.8, 0.05, 0.05, 0.28)))).id();
    refs.vignette = Some(v);
    // World-projected overlays: floating numbers, loot labels, foe health bars.
    let ov = commands.spawn((XpEntity, Overlay, OverlayRoot, GlobalZIndex(8), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() })).id();
    commands.entity(ov).with_children(|o| {
        for i in 0..24 {
            o.spawn((FloaterNode(i), Visibility::Hidden, Text::new(""), font(&a.display, 17.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, Node { position_type: PositionType::Absolute, ..default() }));
        }
        for i in 0..12 {
            o.spawn((LabelNode(i), Visibility::Hidden, Text::new(""), font(&a.body, 12.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 1.5), color: Color::BLACK }, Node { position_type: PositionType::Absolute, ..default() }));
        }
        for i in 0..20 {
            o.spawn((BarNode(i), Visibility::Hidden, Node { position_type: PositionType::Absolute, height: Val::Px(6.0), border: UiRect::all(Val::Px(1.0)), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)), BorderColor::from(Color::BLACK))).with_children(|b| {
                b.spawn((BarFill(i), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgb(0.35, 0.9, 0.4))));
            });
        }
    });
    let _ = ov;
    (root, refs)
}

fn pill(p: &mut ChildSpawnerCommands, a: &UiAssets, label: &str, color: Color, act: Act) {
    p.spawn((Button, act, Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(color), BorderColor::all(NAVY), BorderRadius::all(Val::Px(99.0)))).with_children(|b| {
        b.spawn((Text::new(clean(label.to_string())), font(&a.body, 12.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY }));
    });
}

fn page_root(commands: &mut Commands, area: &SafeArea, title: &str, a: &UiAssets, screen_gold: u32, extra: &str) -> Entity {
    let mut node = root_node(area);
    node.padding.top = Val::Px(area.top.max(0.0) + 6.0);
    node.padding.bottom = Val::Px(area.bottom.max(0.0) + 6.0);
    node.padding.left = Val::Px(area.left.max(10.0));
    node.padding.right = Val::Px(area.right.max(10.0));
    node.row_gap = Val::Px(6.0);
    let root = commands.spawn((XpEntity, Overlay, ScreenRoot, GlobalZIndex(24), node, BackgroundColor(Color::srgba(0.02, 0.03, 0.12, 0.96)))).id();
    commands.entity(root).with_children(|p| {
        p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|h| {
            h.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(12.0), ..default() }).with_children(|l| {
                l.spawn((Text::new(clean(title.to_string())), font(&a.display, 24.0), TextColor(GOLD), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                l.spawn(text(a, &format!("{} gold", screen_gold), 14.0, GOLD_HI));
                l.spawn(text(a, extra, 12.0, CREAM));
            });
            h.spawn(Node { column_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                if title == "PACK" {
                    pill(r, a, "SKILLS", hex(0x5a3aa0), Act::OpenTree);
                } else {
                    pill(r, a, "PACK", hex(0x2f6fe6), Act::OpenBag);
                }
                pill(r, a, "CLOSE", hex(0xc02a24), Act::Close);
            });
        });
    });
    root
}

fn slot_letter(s: Slot) -> &'static str {
    match s {
        Slot::Weapon => "W",
        Slot::Armor => "A",
        Slot::Boots => "B",
        Slot::Charm => "C",
    }
}

fn delta_color(d: f32) -> Color {
    if d > 0.0 { hex(0x6fe07a) } else if d < 0.0 { hex(0xff7a62) } else { CREAM }
}

fn item_card(p: &mut ChildSpawnerCommands, a: &UiAssets, snap: &Snap, item: &Item, equipped_slot: Option<usize>) {
    let eq = Equipment { slots: snap.equip };
    let against = match equipped_slot {
        Some(_) => None,
        None => eq.slots[eq.target(item.slot)],
    };
    let cmp = compare(item, against.as_ref());
    p.spawn((Text::new(clean(item.name())), font(&a.display, 16.0), TextColor(rar(item.rarity)), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
    p.spawn(text(a, &format!("{} {}  -  item level {}", item.rarity.name(), item.slot.name(), item.ilvl), 11.0, CREAM));
    let base_delta = if against.is_some() || equipped_slot.is_none() { cmp.base_delta } else { 0.0 };
    let mut base = item.base_text();
    if equipped_slot.is_none() && against.is_some() && base_delta != 0.0 {
        base.push_str(&format!("   ({:+.0})", base_delta));
    }
    p.spawn(text(a, &base, 13.0, if equipped_slot.is_none() && against.is_some() { delta_color(base_delta) } else { Color::WHITE }));
    for af in item.affixes() {
        let d = cmp.rows.iter().find(|r| r.stat == af.stat).map_or(0.0, |r| r.delta);
        let mut line = format!("+{} {}", fmt_value(af.stat, af.value), af.stat.label());
        if equipped_slot.is_none() && against.is_some() && d != 0.0 {
            line.push_str(&format!("  ({}{})", if d > 0.0 { "+" } else { "" }, fmt_value(af.stat, d)));
        }
        p.spawn(text(a, &line, 12.0, if equipped_slot.is_none() && against.is_some() { delta_color(d) } else { hex(0x9fe0ff) }));
    }
    if equipped_slot.is_none() && against.is_some() {
        for r in cmp.rows.iter().filter(|r| r.value == 0.0 && r.delta < 0.0) {
            p.spawn(text(a, &format!("lose {} {}", fmt_value(r.stat, -r.delta), r.stat.label()), 12.0, hex(0xff7a62)));
        }
    }
    if let Some(l) = item.legendary {
        p.spawn((text(a, l.text(), 12.0, rar(Rarity::Legendary)), Node { max_width: Val::Px(240.0), ..default() }));
    }
    if equipped_slot.is_none() && against.is_some() {
        let (txt, c) = if cmp.power_delta > 0.5 { ("UPGRADE", hex(0x6fe07a)) } else if cmp.power_delta < -0.5 { ("DOWNGRADE", hex(0xff7a62)) } else { ("SIMILAR", CREAM) };
        p.spawn(text(a, &format!("{} vs worn ({:+.0})", txt, cmp.power_delta), 12.0, c));
    } else if equipped_slot.is_none() {
        p.spawn(text(a, "Empty slot - free upgrade", 12.0, hex(0x6fe07a)));
    }
}

/// `touch`: the round pause button floats over the top-right of a page, so keep content clear of it.
fn spawn_pack(commands: &mut Commands, a: &UiAssets, area: &SafeArea, snap: &Snap, sel: Sel, touch: bool) -> Entity {
    let root = page_root(commands, area, "PACK", a, snap.gold, &format!("{} - {} | {} / {} items", snap.hero, snap.attack, snap.bag.len(), BAG_CAP));
    commands.entity(root).with_children(|p| {
        p.spawn(Node { width: Val::Percent(100.0), flex_grow: 1.0, column_gap: Val::Px(10.0), ..default() }).with_children(|row| {
            // Equipped.
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), width: Val::Px(176.0), ..default() }).with_children(|col| {
                col.spawn(text(a, "WORN", 11.0, GOLD));
                for i in 0..EQUIP_SLOTS {
                    let it = snap.equip[i];
                    let selected = sel == Sel::Equip(i);
                    col.spawn((Button, Act::SelectEquip(i), Selectable, Node { width: Val::Percent(100.0), height: Val::Px(38.0), border: UiRect::all(Val::Px(2.0)), padding: UiRect::horizontal(Val::Px(6.0)), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, ..default() }, BackgroundColor(if selected { hex(0x24409a) } else { hex(0x101a44) }), BorderColor::from(it.map_or(hex(0x2a3466), |x| rar(x.rarity))), BorderRadius::all(Val::Px(8.0)))).with_children(|b| {
                        b.spawn(text(a, equip_slot_kind(i).name(), 9.0, Color::srgba(1.0, 1.0, 1.0, 0.55)));
                        match it {
                            Some(x) => b.spawn(text(a, &x.name(), 11.0, rar(x.rarity))),
                            None => b.spawn(text(a, "- empty -", 11.0, Color::srgba(1.0, 1.0, 1.0, 0.4))),
                        };
                    });
                }
                col.spawn(text(a, &format!("HP {:.0}  DMG {:.0}  DEF {:.0}", snap.stats.max_hp, snap.stats.base_damage * snap.stats.dmg_mult, snap.stats.defence), 10.0, CREAM));
                col.spawn(text(a, &format!("CRIT {:.0}%  SPD +{:.0}%", snap.stats.crit * 100.0, snap.stats.move_bonus * 100.0), 10.0, CREAM));
            });
            // Bag grid.
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), ..default() }).with_children(|col| {
                col.spawn(text(a, "BAG", 11.0, GOLD));
                col.spawn(Node { width: Val::Px(6.0 * 50.0 + 5.0 * 4.0), flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(4.0), row_gap: Val::Px(4.0), ..default() }).with_children(|g| {
                    for i in 0..BAG_CAP {
                        let it = snap.bag.get(i);
                        let selected = it.is_some_and(|x| sel == Sel::Bag(x.id));
                        let mut cell = g.spawn((Node { width: Val::Px(50.0), height: Val::Px(44.0), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_direction: FlexDirection::Column, ..default() }, BackgroundColor(if selected { hex(0x24409a) } else { hex(0x101a44) }), BorderColor::from(it.map_or(hex(0x1b2554), |x| rar(x.rarity))), BorderRadius::all(Val::Px(8.0))));
                        if let Some(x) = it {
                            cell.insert((Button, Act::SelectBag(x.id), Selectable));
                            cell.with_children(|c| {
                                c.spawn((Text::new(slot_letter(x.slot)), font(&a.display, 16.0), TextColor(rar(x.rarity))));
                                c.spawn(text(a, &format!("i{}", x.ilvl), 9.0, Color::srgba(1.0, 1.0, 1.0, 0.6)));
                            });
                        }
                    }
                });
                col.spawn(Node { column_gap: Val::Px(6.0), margin: UiRect::top(Val::Px(2.0)), flex_wrap: FlexWrap::Wrap, row_gap: Val::Px(4.0), ..default() }).with_children(|r| {
                    pill(r, a, if snap.filter { "COMMONS: HIDDEN" } else { "COMMONS: SHOWN" }, if snap.filter { hex(0x2a8f37) } else { hex(0x5a5a70) }, Act::Filter);
                    pill(r, a, "SELL COMMONS", hex(0x7a5a10), Act::SellUpTo(Rarity::Common));
                    pill(r, a, "SELL <= UNCOMMON", hex(0x7a5a10), Act::SellUpTo(Rarity::Uncommon));
                });
            });
            // Detail card.
            row.spawn((Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), padding: UiRect::all(Val::Px(8.0)), border: UiRect::all(Val::Px(2.0)), min_width: Val::Px(200.0), max_width: Val::Px(380.0), align_self: AlignSelf::FlexStart, margin: UiRect::top(Val::Px(if touch { 46.0 } else { 0.0 })), ..default() }, BackgroundColor(Color::srgba(0.05, 0.08, 0.25, 0.9)), BorderColor::all(GOLD_LO), BorderRadius::all(Val::Px(12.0)))).with_children(|card| {
                let chosen = match sel {
                    Sel::Bag(id) => snap.bag.iter().find(|x| x.id == id).map(|x| (*x, None)),
                    Sel::Equip(i) => snap.equip[i].map(|x| (x, Some(i))),
                    Sel::None => None,
                };
                match chosen {
                    None => {
                        card.spawn(text(a, "Tap an item to compare it with what you wear.", 12.0, CREAM));
                        card.spawn(text(a, "Rarity colours: grey, green, blue, violet, orange.", 11.0, Color::srgba(1.0, 1.0, 1.0, 0.6)));
                    }
                    Some((item, equipped)) => {
                        item_card(card, a, snap, &item, equipped);
                        card.spawn(Node { column_gap: Val::Px(6.0), margin: UiRect::top(Val::Px(6.0)), ..default() }).with_children(|r| match equipped {
                            None => {
                                pill(r, a, "EQUIP", hex(0x2a8f37), Act::Equip(item.id));
                                pill(r, a, &format!("SELL {}g", item.sell_value()), hex(0x9a6a10), Act::Sell(item.id));
                            }
                            Some(i) => pill(r, a, "UNEQUIP", hex(0x2f6fe6), Act::Unequip(i)),
                        });
                    }
                }
            });
        });
    });
    root
}

fn spawn_tree(commands: &mut Commands, a: &UiAssets, area: &SafeArea, snap: &Snap, touch: bool) -> Entity {
    let root = page_root(commands, area, "SKILLS", a, snap.gold, &format!("{} skill points to spend", snap.tree.points));
    let groups: [(&str, Option<Skill>, &[Perk]); 5] = [
        ("QUAKE SLAM", Some(Skill::Slam), &[Perk::QuakeForce, Perk::Aftershock]),
        ("WHIRL", Some(Skill::Whirl), &[Perk::Whirl, Perk::Cyclone, Perk::Gale]),
        ("STAR VOLLEY", Some(Skill::Volley), &[Perk::Volley, Perk::Barrage, Perk::Piercing]),
        ("AEGIS", Some(Skill::Aegis), &[Perk::Aegis, Perk::Bulwark, Perk::Reprisal]),
        ("BODY", None, &[Perk::Vigor]),
    ];
    commands.entity(root).with_children(|p| {
        p.spawn(Node { width: Val::Percent(100.0), flex_grow: 1.0, column_gap: Val::Px(8.0), justify_content: JustifyContent::Center, padding: UiRect::right(Val::Px(if touch { 46.0 } else { 0.0 })), ..default() }).with_children(|row| {
            for (title, skill, nodes) in groups {
                row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(5.0), width: Val::Px(if touch { 134.0 } else { 144.0 }), ..default() }).with_children(|col| {
                    let sub = match skill {
                        Some(Skill::Slam) => "Free. Ground pound.".to_string(),
                        Some(s) => format!("{:.0} spark, {:.0}s", s.cost(), s.base_cooldown_secs()),
                        None => "Always on".to_string(),
                    };
                    col.spawn((Node { padding: UiRect::all(Val::Px(4.0)), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(hex(0x1c1050)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(8.0)))).with_children(|h| {
                        h.spawn((Text::new(skill.map_or(title, |s| s.name()).to_uppercase()), font(&a.display, 13.0), TextColor(GOLD)));
                        h.spawn(text(a, &sub, 10.0, CREAM));
                    });
                    for n in nodes {
                        let info = n.info();
                        let rank = snap.tree.rank(*n);
                        let can = snap.tree.can_rank(*n);
                        let locked = info.req.is_some_and(|r| snap.tree.rank(r) == 0);
                        let border = if rank >= info.max { GOLD } else if can { hex(0x6fe07a) } else if rank > 0 { hex(0x5aa0ff) } else { hex(0x2a3466) };
                        p_node(col, a, *n, rank, can, locked, border);
                    }
                });
            }
        });
        p.spawn(text(a, "Tap a node to spend a point. Each level gives one. Unlock nodes open their skill's upgrades.", 11.0, CREAM));
    });
    root
}

fn p_node(col: &mut ChildSpawnerCommands, a: &UiAssets, n: Perk, rank: u8, can: bool, locked: bool, border: Color) {
    let info = n.info();
    let mut e = col.spawn((Node { width: Val::Percent(100.0), height: Val::Px(66.0), border: UiRect::all(Val::Px(3.0)), padding: UiRect::all(Val::Px(4.0)), flex_direction: FlexDirection::Column, row_gap: Val::Px(1.0), ..default() }, BackgroundColor(if locked { hex(0x0a1030) } else if can { hex(0x1c3a60) } else { hex(0x101a44) }), BorderColor::all(border), BorderRadius::all(Val::Px(10.0))));
    if can {
        e.insert((Button, Act::Rank(n)));
    }
    e.with_children(|c| {
        c.spawn(Node { justify_content: JustifyContent::SpaceBetween, width: Val::Percent(100.0), ..default() }).with_children(|r| {
            r.spawn((Text::new(clean(info.name.to_string())), font(&a.body, 12.0), TextColor(if locked { Color::srgba(1.0, 1.0, 1.0, 0.4) } else { Color::WHITE })));
            r.spawn(text(a, &format!("{}/{}", rank, info.max), 12.0, if rank >= info.max { GOLD } else { hex(0x9fe0ff) }));
        });
        c.spawn((Text::new(clean(info.desc.to_string())), font(&a.body, 10.0), TextColor(Color::srgba(1.0, 1.0, 1.0, if locked { 0.35 } else { 0.8 })), Node { width: Val::Percent(100.0), ..default() }));
    });
}

fn spawn_summary(commands: &mut Commands, a: &UiAssets, area: &SafeArea, s: &SummaryView) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, SummaryRoot, GlobalZIndex(26), node, BackgroundColor(if s.victory { Color::srgba(0.1, 0.07, 0.0, 0.66) } else { Color::srgba(0.06, 0.0, 0.08, 0.66) }))).id();
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(28.0), Val::Px(10.0)), row_gap: Val::Px(3.0), width: Val::Px(420.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
            kicker(c, a, "RELIC DESCENT");
            outlined(c, a, if s.victory { "RELIC CLAIMED!" } else { "YOU FELL" }, 34.0, if s.victory { GOLD } else { RED_HI });
            let mut row = |label: &str, value: String, color: Color| {
                c.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                    r.spawn(text(a, label, 14.0, CREAM));
                    r.spawn((Text::new(clean(value)), font(&a.display, 17.0), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                });
            };
            row("Floor reached", format!("{} / {}", s.floor, FLOORS), Color::WHITE);
            row("Level", s.level.to_string(), Color::WHITE);
            row("Foes defeated", s.kills.to_string(), Color::WHITE);
            row("Gear found", format!("{} (best {})", s.items, RARITIES[(s.best_rarity as usize).min(4)].name()), rarity_color(s.best_rarity));
            row("Time", clock(s.secs), Color::WHITE);
            row("Gold banked", format!("+{}", s.banked), GOLD_HI);
            row("Stash", format!("{}  (+{}% power)", s.stash, s.legacy_pct), GOLD_HI);
            if s.new_best_floor {
                row("New deepest floor", s.best_floor.to_string(), GOLD);
            }
            c.spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(6.0)), ..default() }).with_children(|r| {
                for (label, kind, act, w) in [("RETRY", ButtonKind::Gold, Act::Retry, 170.0), ("MAIN MENU", ButtonKind::Blue, Act::Menu, 150.0)] {
                    let mut b = button(r, a, ButtonSpec::new(kind, Val::Px(w), Val::Px(44.0), 19.0), label);
                    b.insert(act);
                }
            });
        });
    });
    root
}

#[allow(clippy::too_many_arguments)]
fn manage_ui(mut commands: Commands, bridge: Res<RelicBridge>, assets: Option<Res<UiAssets>>, area: Res<SafeArea>, touch: Res<crate::touch::TouchControls>, mut ui: ResMut<UiState>) {
    let Some(a) = assets else { return };
    if ui.hud.is_none() {
        let (root, refs) = spawn_hud(&mut commands, &a, &area);
        ui.hud = Some(root);
        ui.refs = refs;
    }
    // Pages.
    // Test hook: preselect a bag item (`JUMPMAN_RELIC_SELECT=<index>`) or a worn one (`worn<slot>`).
    if ui.sel == Sel::None && bridge.screen == Screen::Bag {
        if let Ok(v) = std::env::var("JUMPMAN_RELIC_SELECT") {
            if let Some(slot) = v.strip_prefix("worn").and_then(|n| n.parse::<usize>().ok()) {
                ui.sel = Sel::Equip(slot.min(EQUIP_SLOTS - 1));
            } else if let Some(it) = v.parse::<usize>().ok().and_then(|i| bridge.snap.bag.get(i)) {
                ui.sel = Sel::Bag(it.id);
            }
        }
    }
    let want = (bridge.screen, bridge.snap_gen, ui.sel);
    let have = ui.screen.map(|(_, s, g, sel)| (s, g, sel));
    if have != Some(want) && !(bridge.screen == Screen::None && ui.screen.is_none()) {
        if let Some((e, ..)) = ui.screen.take() {
            commands.entity(e).despawn();
        }
        let e = match bridge.screen {
            Screen::None => None,
            Screen::Bag => Some(spawn_pack(&mut commands, &a, &area, &bridge.snap, ui.sel, touch.visible)),
            Screen::Tree => Some(spawn_tree(&mut commands, &a, &area, &bridge.snap, touch.visible)),
        };
        ui.screen = e.map(|e| (e, bridge.screen, bridge.snap_gen, ui.sel));
    }
    match (ui.summary, bridge.summary) {
        (None, Some(s)) => ui.summary = Some(spawn_summary(&mut commands, &a, &area, &s)),
        (Some(e), None) => {
            commands.entity(e).despawn();
            ui.summary = None;
        }
        _ => {}
    }
}

fn set_text(texts: &mut Query<&mut Text>, e: Option<Entity>, s: String) {
    if let Some(e) = e {
        if let Ok(mut t) = texts.get_mut(e) {
            if t.0 != s {
                t.0 = s;
            }
        }
    }
}

fn set_width(nodes: &mut Query<&mut Node>, e: Option<Entity>, frac: f32) {
    if let Some(e) = e {
        if let Ok(mut n) = nodes.get_mut(e) {
            let w = Val::Percent(frac.clamp(0.0, 1.0) * 100.0);
            if n.width != w {
                n.width = w;
            }
        }
    }
}

fn set_display(nodes: &mut Query<&mut Node>, e: Option<Entity>, on: bool) {
    if let Some(e) = e {
        if let Ok(mut n) = nodes.get_mut(e) {
            let d = if on { Display::Flex } else { Display::None };
            if n.display != d {
                n.display = d;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_hud(bridge: Res<RelicBridge>, ui: Res<UiState>, tc: Res<TouchControls>, mut texts: Query<&mut Text>, mut nodes: Query<&mut Node>, mut colors: Query<&mut BackgroundColor>) {
    let r = ui.refs;
    let h = bridge.hud;
    set_text(&mut texts, r.level_text, format!("LV {}", h.level));
    set_text(&mut texts, r.hp_text, format!("{} / {}", h.hp.max(0.0).ceil() as i32, h.max_hp.ceil() as i32));
    let frac = if h.max_hp > 0.0 { (h.hp / h.max_hp).clamp(0.0, 1.0) } else { 0.0 };
    set_width(&mut nodes, r.hp_fill, frac);
    if let Some(e) = r.hp_fill {
        if let Ok(mut c) = colors.get_mut(e) {
            let col = if frac > 0.5 { hex(0x39c24a) } else if frac > 0.25 { hex(0xf0b429) } else { hex(0xe5302a) };
            if c.0 != col {
                c.0 = col;
            }
        }
    }
    set_width(&mut nodes, r.shield_fill, if h.shield_max > 0.0 { h.shield / h.shield_max } else { 0.0 });
    set_width(&mut nodes, r.spark_fill, if h.max_spark > 0.0 { h.spark / h.max_spark } else { 0.0 });
    set_width(&mut nodes, r.xp_fill, if h.xp_need > 0 { h.xp as f32 / h.xp_need as f32 } else { 0.0 });
    let pts = if h.points > 0 { format!("   * {} SKILL PTS", h.points) } else { String::new() };
    set_text(&mut texts, r.info_text, format!("FLOOR {}/{}   {}g   POTIONS {}{}", h.floor, FLOORS, h.gold, h.potions, pts));
    set_display(&mut nodes, r.hint, h.hint);
    set_display(&mut nodes, r.boss_root, h.boss.is_some());
    if let Some(b) = h.boss {
        set_width(&mut nodes, r.boss_fill, b.frac);
        set_text(&mut texts, r.boss_name, format!("BOMB KING{}", ["", "  - ENRAGED SOON", "  - ENRAGED"][b.phase.min(2) as usize]));
    }
    set_display(&mut nodes, r.stairs_root, h.stairs > 0.0);
    set_width(&mut nodes, r.stairs_fill, h.stairs);
    set_display(&mut nodes, r.vignette, h.low_hp);
    // Desktop skill bar when no touch buttons are around.
    set_display(&mut nodes, r.skillbar, !tc.visible);
    for i in 0..4 {
        set_width_h(&mut nodes, r.skill_fill[i], if !h.skill_unlocked[i] { 1.0 } else if h.skill_cd[i] > 0.0 { h.skill_cd[i] } else if !h.skill_ready[i] { 0.5 } else { 0.0 });
    }
    // Minimap.
    for (i, room) in bridge.mini.iter().enumerate().take(12) {
        let Some(e) = r.mini_rooms[i] else { continue };
        let Ok(mut n) = nodes.get_mut(e) else { continue };
        let k = 1.9;
        n.display = Display::Flex;
        n.left = Val::Px(room.x * k + 2.0);
        n.top = Val::Px(room.y * k + 2.0);
        n.width = Val::Px(room.w * k);
        n.height = Val::Px(room.h * k);
        if let Ok(mut c) = colors.get_mut(e) {
            let col = match (room.kind, room.seen) {
                (4, _) => Color::srgba(0.9, 0.2, 0.2, if room.seen { 0.9 } else { 0.5 }),
                (3, _) => Color::srgba(1.0, 0.85, 0.3, if room.seen { 0.9 } else { 0.5 }),
                (2, _) => Color::srgba(1.0, 0.7, 0.2, if room.seen { 0.8 } else { 0.35 }),
                (_, true) => if room.cleared { Color::srgba(0.35, 0.8, 0.5, 0.85) } else { Color::srgba(0.5, 0.65, 1.0, 0.85) },
                _ => Color::srgba(0.35, 0.35, 0.55, 0.45),
            };
            if c.0 != col {
                c.0 = col;
            }
        }
    }
    if let Some(e) = r.mini_player {
        if let Ok(mut n) = nodes.get_mut(e) {
            n.left = Val::Px(bridge.mini_player[0] * 1.9 - 1.0);
            n.top = Val::Px(bridge.mini_player[1] * 1.9 - 1.0);
        }
    }
    set_display(&mut nodes, r.mini_exit, false);
}

fn set_width_h(nodes: &mut Query<&mut Node>, e: Option<Entity>, frac: f32) {
    if let Some(e) = e {
        if let Ok(mut n) = nodes.get_mut(e) {
            let v = Val::Percent(frac.clamp(0.0, 1.0) * 100.0);
            if n.height != v {
                n.height = v;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn screen_buttons(mut bridge: ResMut<RelicBridge>, mut ui: ResMut<UiState>, buttons: Query<(&Interaction, &Act), Changed<Interaction>>, state: Res<AppState>, mut menu: MessageWriter<ReturnToMenu>) {
    if *state != AppState::Playing {
        return;
    }
    for (i, act) in &buttons {
        if *i != Interaction::Pressed {
            continue;
        }
        match *act {
            Act::SelectBag(id) => ui.sel = Sel::Bag(id),
            Act::SelectEquip(s) => ui.sel = Sel::Equip(s),
            Act::Equip(id) => {
                bridge.cmds.push(UiCmd::Equip(id));
                ui.sel = Sel::None;
            }
            Act::Sell(id) => {
                bridge.cmds.push(UiCmd::Sell(id));
                ui.sel = Sel::None;
            }
            Act::Unequip(s) => {
                bridge.cmds.push(UiCmd::Unequip(s));
                ui.sel = Sel::None;
            }
            Act::SellUpTo(r) => {
                bridge.cmds.push(UiCmd::SellUpTo(r));
                ui.sel = Sel::None;
            }
            Act::Filter => bridge.cmds.push(UiCmd::ToggleFilter),
            Act::Close => bridge.cmds.push(UiCmd::Close),
            Act::OpenTree => bridge.cmds.push(UiCmd::Open(Screen::Tree)),
            Act::OpenBag => bridge.cmds.push(UiCmd::Open(Screen::Bag)),
            Act::Rank(n) => bridge.cmds.push(UiCmd::Rank(n)),
            Act::Retry => {
                if bridge.summary_ready {
                    bridge.cmds.push(UiCmd::Retry);
                }
            }
            Act::Menu => {
                if bridge.summary_ready {
                    menu.write(ReturnToMenu);
                }
            }
        }
    }
}

/// Keyboard / gamepad on the pack page: move the selection, equip, sell, close.
fn screen_keys(mut bridge: ResMut<RelicBridge>, mut ui: ResMut<UiState>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, state: Res<AppState>) {
    if *state != AppState::Playing {
        return;
    }
    if bridge.summary.is_some() && bridge.summary_ready {
        let confirm = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) || pads.iter().any(|p| p.just_pressed(GamepadButton::South));
        if confirm {
            bridge.cmds.push(UiCmd::Retry);
        }
        return;
    }
    if bridge.screen != Screen::Bag {
        return;
    }
    let (mut step, mut equip, mut sell) = (0i32, keys.just_pressed(KeyCode::Enter), keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::Delete));
    if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::ArrowDown) {
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
        equip |= pad.just_pressed(GamepadButton::South);
        sell |= pad.just_pressed(GamepadButton::West);
        if pad.just_pressed(GamepadButton::East) {
            bridge.cmds.push(UiCmd::Close);
        }
    }
    let ids: Vec<u32> = bridge.snap.bag.iter().map(|i| i.id).collect();
    if step != 0 && !ids.is_empty() {
        let cur = match ui.sel {
            Sel::Bag(id) => ids.iter().position(|x| *x == id),
            _ => None,
        };
        let next = match cur {
            Some(i) => (i as i32 + step).rem_euclid(ids.len() as i32) as usize,
            None => 0,
        };
        ui.sel = Sel::Bag(ids[next]);
    }
    if let Sel::Bag(id) = ui.sel {
        if equip {
            bridge.cmds.push(UiCmd::Equip(id));
            ui.sel = Sel::None;
        } else if sell {
            bridge.cmds.push(UiCmd::Sell(id));
            ui.sel = Sel::None;
        }
    }
}

/// Menus (pause, settings) draw over the game; hide ours meanwhile so it cannot take their taps.
fn overlay_display(state: Res<AppState>, mut roots: Query<&mut Visibility, With<Overlay>>) {
    let v = if *state == AppState::Playing { Visibility::Inherited } else { Visibility::Hidden };
    for mut vis in &mut roots {
        if *vis != v {
            *vis = v;
        }
    }
}

fn ui_cleanup(mut ui: ResMut<UiState>) {
    if ui.hud.is_some() || ui.screen.is_some() || ui.summary.is_some() {
        *ui = UiState::default();
    }
}
