//! The Painted Halls presentation, UI half (bevy_ui): the HUD (hearts, star fragments, ability
//! chips, corner minimap, boss bar, room banner, hint line, fade), the map screen, the
//! new game / continue prompt, the "faded" card and the ending card. Data flows through
//! [`HallsBridge`]; nothing here touches the rules.
use crate::button_marker::BorderRadius;
use super::halls_core::MapRoom;
use super::halls_view::*;
use super::halls_world::*;
use super::XpEntity;
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::shell::{AppState, ReturnToMenu};
use bevy::prelude::*;

pub struct HallsUiPlugin;

impl Plugin for HallsUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>().add_systems(Update, (manage_ui, update_hud, buttons, keys, overlay_display).chain().after(super::run_frame).run_if(|b: Res<HallsBridge>| b.active));
        app.add_systems(Update, ui_cleanup.after(super::run_frame).run_if(|b: Res<HallsBridge>| !b.active));
    }
}

#[derive(Component)]
struct Overlay;
#[derive(Component, Clone, Copy, PartialEq)]
enum Act {
    NewGame,
    Continue,
    CloseMap,
    KeepPlaying,
    Menu,
    OpenMap,
}

fn region_color(r: Region) -> Color {
    match r {
        Region::Hall => hex(0xd9a441),
        Region::Garden => hex(0x4fa84a),
        Region::Cellar => hex(0x9a5b3c),
        Region::Vault => hex(0x2f9ab0),
        Region::Tower => hex(0xe07a3a),
        Region::Gallery => hex(0x9a62d0),
    }
}

pub fn ability_color(a: Ability) -> Color {
    let c = a.color();
    Color::srgb(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)
}

#[derive(Default, Clone, Copy)]
struct Refs {
    hearts: [Option<Entity>; 7],
    frags: Option<Entity>,
    chips: [Option<Entity>; 6],
    chip_text: [Option<Entity>; 6],
    boss_root: Option<Entity>,
    boss_fill: Option<Entity>,
    boss_name: Option<Entity>,
    banner_root: Option<Entity>,
    banner_small: Option<Entity>,
    banner_big: Option<Entity>,
    hint_root: Option<Entity>,
    hint_text: Option<Entity>,
    fade: Option<Entity>,
    vignette: Option<Entity>,
    mini: Option<Entity>,
    keys: Option<Entity>,
}

#[derive(Resource, Default)]
struct UiState {
    hud: Option<Entity>,
    refs: Refs,
    screen: Option<(Entity, Screen, u32)>,
    mini_key: Option<(u32, usize)>,
    ready_at: f32,
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

fn txt(a: &UiAssets, s: &str, size: f32, color: Color) -> impl Bundle {
    (Text::new(clean(s.to_string())), font(&a.body, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY })
}

fn spawn_hud(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> (Entity, Refs) {
    let mut refs = Refs::default();
    let mut node = root_node(area);
    node.padding.top = Val::Px(area.top.max(0.0) + 6.0);
    node.padding.left = Val::Px(area.left.max(10.0));
    node.padding.right = Val::Px(area.right.max(10.0));
    node.align_items = AlignItems::FlexStart;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(12), node)).id();
    commands.entity(root).with_children(|p| {
        // Left column: hearts, fragments, ability chips.
        p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() }).with_children(|col| {
            col.spawn(Node { column_gap: Val::Px(3.0), ..default() }).with_children(|r| {
                for i in 0..7 {
                    r.spawn((Node { width: Val::Px(20.0), height: Val::Px(18.0), border: UiRect::all(Val::Px(2.0)), overflow: Overflow::clip(), display: Display::None, ..default() }, BackgroundColor(Color::srgba(0.1, 0.02, 0.05, 0.8)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(6.0)))).with_children(|h| {
                        refs.hearts[i] = Some(h.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(hex(0xe5302a)))).id());
                    });
                }
            });
            col.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(5.0), ..default() }).with_children(|r| {
                icon(r, &a.star, 16.0);
                refs.frags = Some(r.spawn(txt(a, "0/12", 14.0, GOLD_HI)).id());
                refs.keys = Some(r.spawn(txt(a, "", 13.0, CREAM)).id());
            });
            col.spawn(Node { column_gap: Val::Px(3.0), ..default() }).with_children(|r| {
                for (i, ab) in ABILITIES.iter().enumerate() {
                    let _ = ab;
                    r.spawn((Node { width: Val::Px(26.0), height: Val::Px(18.0), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.1, 0.1, 0.18, 0.7)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(5.0)))).with_children(|c| {
                        refs.chip_text[i] = Some(c.spawn((Text::new(ab.short()), font(&a.body, 10.0), TextColor(Color::srgb(0.45, 0.45, 0.55)))).id());
                    });
                    // The chip's own node is the parent we just spawned; remember it.
                }
            });
        });
    });
    // Chip nodes are the parents of the chip texts.
    // (Looked up lazily by update_hud via the text's parent.)
    // Corner minimap sits left of the pause button.
    let mini = commands.spawn((XpEntity, Overlay, GlobalZIndex(11), Node { position_type: PositionType::Absolute, right: Val::Px(area.right.max(14.0) + 52.0), top: Val::Px(area.top.max(0.0) + 8.0), width: Val::Px(132.0), height: Val::Px(84.0), overflow: Overflow::clip(), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(Color::srgba(0.02, 0.03, 0.12, 0.55)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(8.0)))).id();
    refs.mini = Some(mini);
    // Boss bar, banner, hint, vignette, fade.
    let boss = commands.spawn((XpEntity, Overlay, GlobalZIndex(13), Node { position_type: PositionType::Absolute, top: Val::Px(area.top.max(0.0) + 8.0), left: Val::Percent(30.0), width: Val::Percent(40.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, display: Display::None, ..default() })).id();
    commands.entity(boss).with_children(|b| {
        refs.boss_name = Some(b.spawn(txt(a, "", 14.0, RED_HI)).id());
        b.spawn((Node { width: Val::Percent(100.0), height: Val::Px(12.0), border: UiRect::all(Val::Px(2.0)), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgba(0.1, 0.0, 0.0, 0.8)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(6.0)))).with_children(|f| {
            refs.boss_fill = Some(f.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(hex(0xe5302a)))).id());
        });
    });
    refs.boss_root = Some(boss);
    let banner = commands.spawn((XpEntity, Overlay, GlobalZIndex(14), Node { position_type: PositionType::Absolute, top: Val::Percent(24.0), width: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, display: Display::None, ..default() })).id();
    commands.entity(banner).with_children(|b| {
        refs.banner_small = Some(b.spawn(txt(a, "", 14.0, GOLD_HI)).id());
        refs.banner_big = Some(b.spawn((Text::new(""), font(&a.display, 34.0), TextColor(GOLD), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY })).id());
    });
    refs.banner_root = Some(banner);
    let hint = commands.spawn((XpEntity, Overlay, GlobalZIndex(14), Node { position_type: PositionType::Absolute, bottom: Val::Px(area.bottom.max(12.0) + 112.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, display: Display::None, ..default() })).id();
    commands.entity(hint).with_children(|b| {
        b.spawn((Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(5.0)), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(Color::srgba(0.02, 0.03, 0.12, 0.8)), BorderColor::all(GOLD), BorderRadius::all(Val::Px(99.0)))).with_children(|c| {
            refs.hint_text = Some(c.spawn(txt(a, "", 13.0, CREAM)).id());
        });
    });
    refs.hint_root = Some(hint);
    refs.vignette = Some(commands.spawn((XpEntity, Overlay, GlobalZIndex(15), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), border: UiRect::all(Val::Px(26.0)), ..default() }, BorderColor::from(Color::srgba(0.9, 0.05, 0.05, 0.0)))).id());
    refs.fade = Some(commands.spawn((XpEntity, Overlay, GlobalZIndex(40), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)))).id());
    (root, refs)
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

#[allow(clippy::too_many_arguments)]
fn update_hud(
    bridge: Res<HallsBridge>,
    ui: Res<UiState>,
    mut texts: Query<&mut Text>,
    mut nodes: Query<&mut Node>,
    mut colors: Query<&mut BackgroundColor>,
    mut borders: Query<&mut BorderColor>,
    mut text_colors: Query<&mut TextColor>,
    parents: Query<&ChildOf>,
) {
    let r = ui.refs;
    let h = &bridge.hud;
    for i in 0..7 {
        let hearts = (h.max_hp / 2) as usize;
        let hp = (h.hp - i as i32 * 2).clamp(0, 2);
        if let Some(e) = r.hearts[i] {
            // r.hearts[i] is the fill node; its parent is the visible heart frame.
            if let Ok(p) = parents.get(e) {
                set_display(&mut nodes, Some(p.parent()), i < hearts);
            }
            set_width(&mut nodes, Some(e), hp as f32 / 2.0);
        }
    }
    set_text(&mut texts, r.frags, format!("{}/{}", h.fragments, h.total_fragments));
    set_text(&mut texts, r.keys, if h.keys & 1 != 0 { "  KEY".to_string() } else { String::new() });
    for (i, a) in ABILITIES.iter().enumerate() {
        let owned = h.abilities & a.bit() != 0;
        let worn = h.cap == Some(*a);
        if let Some(t) = r.chip_text[i] {
            if let Ok(mut c) = text_colors.get_mut(t) {
                let want = if owned { Color::WHITE } else { Color::srgb(0.4, 0.4, 0.5) };
                if c.0 != want {
                    c.0 = want;
                }
            }
            if let Ok(p) = parents.get(t) {
                let chip = p.parent();
                if let Ok(mut bg) = colors.get_mut(chip) {
                    let want = if owned { ability_color(*a).with_alpha(0.9) } else { Color::srgba(0.1, 0.1, 0.18, 0.7) };
                    if bg.0 != want {
                        bg.0 = want;
                    }
                }
                if let Ok(mut bc) = borders.get_mut(chip) {
                    let want = if worn { GOLD_HI } else { NAVY };
                    if bc.top != want {
                        *bc = BorderColor::all(want);
                    }
                }
            }
        }
    }
    // Boss bar.
    set_display(&mut nodes, r.boss_root, h.boss.is_some());
    if let Some(b) = h.boss {
        set_text(&mut texts, r.boss_name, b.name.to_string());
        set_width(&mut nodes, r.boss_fill, b.frac);
    }
    // Banner.
    match &bridge.banner {
        Some((s, age)) if *age < 3.0 => {
            set_display(&mut nodes, r.banner_root, true);
            let (small, big) = s.split_once('|').unwrap_or(("", s.as_str()));
            set_text(&mut texts, r.banner_small, small.to_string());
            set_text(&mut texts, r.banner_big, big.to_string());
            let a = if *age < 0.4 { *age / 0.4 } else if *age > 2.4 { (3.0 - *age) / 0.6 } else { 1.0 };
            for e in [r.banner_small, r.banner_big].into_iter().flatten() {
                if let Ok(mut c) = text_colors.get_mut(e) {
                    let want = c.0.with_alpha(a.clamp(0.0, 1.0));
                    if c.0 != want {
                        c.0 = want;
                    }
                }
            }
        }
        _ => set_display(&mut nodes, r.banner_root, false),
    }
    match &bridge.hint {
        Some((s, age)) if *age < 3.2 => {
            set_display(&mut nodes, r.hint_root, true);
            set_text(&mut texts, r.hint_text, s.clone());
        }
        _ => set_display(&mut nodes, r.hint_root, false),
    }
    if let Some(e) = r.fade {
        if let Ok(mut c) = colors.get_mut(e) {
            let want = Color::srgba(0.0, 0.0, 0.0, bridge.fade.clamp(0.0, 1.0));
            if c.0 != want {
                c.0 = want;
            }
        }
    }
    if let Some(e) = r.vignette {
        if let Ok(mut c) = borders.get_mut(e) {
            let a = (h.hurt_flash * 0.6).max(if h.low_hp { 0.18 + 0.12 * (bridge.time * 6.0).sin().abs() } else { 0.0 });
            let want = Color::srgba(0.9, 0.05, 0.05, a);
            if c.top != want {
                *c = BorderColor::all(want);
            }
        }
    }
}

const MINI: f32 = 13.0;

/// The corner minimap: the rooms within a few cells of the current one.
fn rebuild_minimap(commands: &mut Commands, parent: Entity, children: &Query<&Children>, bridge: &HallsBridge) {
    if let Ok(ch) = children.get(parent) {
        for c in ch.iter() {
            commands.entity(c).despawn();
        }
    }
    let Some(here) = bridge.map.iter().find(|m| m.idx == bridge.here) else { return };
    let (hx, hy) = (here.rect.0 as f32 + here.rect.2 as f32 / 2.0, here.rect.1 as f32 + here.rect.3 as f32 / 2.0);
    let rooms = rooms();
    commands.entity(parent).with_children(|p| {
        for m in &bridge.map {
            let (x, y, w, h) = m.rect;
            let left = 66.0 + (x as f32 - hx) * MINI;
            let top = 42.0 + (y as f32 - hy) * MINI;
            map_cell(p, m, &rooms, left, top, w as f32 * MINI, h as f32 * MINI, true, None);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn map_cell(p: &mut ChildSpawnerCommands, m: &MapRoom, rooms: &[RoomDef], left: f32, top: f32, w: f32, h: f32, mini: bool, a: Option<&UiAssets>) {
    let region = rooms[m.idx].region;
    let base = region_color(region);
    let fill = if m.discovered { base.with_alpha(if mini { 0.85 } else { 0.9 }) } else { Color::srgba(0.12, 0.14, 0.28, 0.55) };
    let border = if m.here { GOLD_HI } else if m.discovered { NAVY } else { hex(0x4a5594) };
    let bw = if m.here { 2.0 } else { 1.5 };
    p.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(left + 1.0), top: Val::Px(top + 1.0), width: Val::Px(w - 2.0), height: Val::Px(h - 2.0), border: UiRect::all(Val::Px(bw)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(1.0), row_gap: Val::Px(1.0), ..default() }, BackgroundColor(fill), BorderColor::all(border), BorderRadius::all(Val::Px(3.0)))).with_children(|c| {
        if !m.discovered {
            if let Some(a) = a {
                c.spawn((Text::new("?"), font(&a.display, (h * 0.5).min(22.0)), TextColor(hex(0x8f9be0))));
            }
            return;
        }
        let dot = if mini { 4.0 } else { (w.min(h) * 0.22).clamp(6.0, 10.0) };
        let mut dots: Vec<Color> = vec![];
        for (ab, have) in m.gates.iter().flatten() {
            dots.push(if *have { ability_color(*ab).with_alpha(0.35) } else { ability_color(*ab) });
        }
        if m.save {
            dots.push(Color::WHITE);
        }
        if m.boss {
            dots.push(hex(0xff2a2a));
        }
        if m.locked {
            dots.push(hex(0xb0b0b0));
        }
        for col in dots {
            c.spawn((Node { width: Val::Px(dot), height: Val::Px(dot), ..default() }, BackgroundColor(col), BorderRadius::all(Val::Px(dot / 2.0))));
        }
        if m.here && !mini {
            c.spawn((Node { width: Val::Px(dot * 1.3), height: Val::Px(dot * 1.3), border: UiRect::all(Val::Px(2.0)), ..default() }, BorderColor::all(GOLD_HI), BorderRadius::all(Val::Px(dot))));
        }
    });
}

fn spawn_map(commands: &mut Commands, a: &UiAssets, area: &SafeArea, bridge: &HallsBridge) -> Entity {
    let mut node = root_node(area);
    node.padding.top = Val::Px(area.top.max(0.0) + 6.0);
    node.padding.bottom = Val::Px(area.bottom.max(0.0) + 6.0);
    node.padding.left = Val::Px(area.left.max(10.0));
    node.padding.right = Val::Px(area.right.max(10.0));
    node.row_gap = Val::Px(6.0);
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(24), node, BackgroundColor(Color::srgb(0.02, 0.03, 0.12)))).id();
    let rooms = rooms();
    // Bounds of everything shown.
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for m in &bridge.map {
        x0 = x0.min(m.rect.0);
        y0 = y0.min(m.rect.1);
        x1 = x1.max(m.rect.0 + m.rect.2);
        y1 = y1.max(m.rect.1 + m.rect.3);
    }
    if x0 > x1 {
        (x0, y0, x1, y1) = (0, 0, 1, 1);
    }
    let (cols, rows) = ((x1 - x0) as f32, (y1 - y0) as f32);
    let cell = (520.0 / cols).min(262.0 / rows).clamp(14.0, 34.0);
    commands.entity(root).with_children(|p| {
        p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|h| {
            h.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(12.0), ..default() }).with_children(|l| {
                l.spawn((Text::new("THE PAINTED HALLS"), font(&a.display, 22.0), TextColor(GOLD), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                icon(l, &a.star, 16.0);
                l.spawn(txt(a, &format!("{}/{}", bridge.hud.fragments, bridge.hud.total_fragments), 15.0, GOLD_HI));
            });
            let mut b = button(h, a, ButtonSpec::new(ButtonKind::Red, Val::Px(92.0), Val::Px(32.0), 15.0), "CLOSE");
            b.insert(Act::CloseMap);
        });
        p.spawn(Node { width: Val::Percent(100.0), flex_grow: 1.0, column_gap: Val::Px(12.0), ..default() }).with_children(|row| {
            // The grid.
            row.spawn(Node { width: Val::Px(cols * cell + 6.0), height: Val::Px(rows * cell + 6.0), ..default() }).with_children(|g| {
                for m in &bridge.map {
                    let (x, y, w, h) = m.rect;
                    map_cell(g, m, &rooms, (x - x0) as f32 * cell, (y - y0) as f32 * cell, w as f32 * cell, h as f32 * cell, false, Some(a));
                }
            });
            // Legend and checklist.
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), flex_grow: 1.0, ..default() }).with_children(|l| {
                l.spawn(txt(a, "ABILITIES", 12.0, GOLD));
                for ab in ABILITIES {
                    let owned = bridge.hud.abilities & ab.bit() != 0;
                    l.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                        r.spawn((Node { width: Val::Px(11.0), height: Val::Px(11.0), ..default() }, BackgroundColor(if owned { ability_color(ab) } else { ability_color(ab).with_alpha(0.25) }), BorderRadius::all(Val::Px(6.0))));
                        r.spawn(txt(a, ab.name(), 12.0, if owned { Color::WHITE } else { hex(0x7f88b8) }));
                        if !owned {
                            r.spawn(txt(a, "not yet", 10.0, hex(0x7f88b8)));
                        }
                    });
                }
                l.spawn(txt(a, "MAP KEY", 12.0, GOLD));
                for (c, s) in [(Color::WHITE, "save pedestal"), (hex(0xff2a2a), "boss"), (hex(0xb0b0b0), "locked door"), (hex(0xe0e0e0), "coloured dot = gate that needs that ability")] {
                    l.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                        r.spawn((Node { width: Val::Px(9.0), height: Val::Px(9.0), ..default() }, BackgroundColor(c), BorderRadius::all(Val::Px(5.0))));
                        r.spawn(txt(a, s, 11.0, CREAM));
                    });
                }
                l.spawn(txt(a, "Unvisited exits show as ?", 11.0, hex(0x8f9be0)));
            });
        });
    });
    root
}

fn spawn_prompt(commands: &mut Commands, a: &UiAssets, area: &SafeArea, c: &PromptCard) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(26), node, BackgroundColor(Color::srgba(0.02, 0.0, 0.08, 0.8)))).id();
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(14.0)), row_gap: Val::Px(6.0), width: Val::Px(500.0), align_items: AlignItems::Center, ..default() }).with_children(|b| {
            kicker(b, a, "A METROIDVANIA OF MARIO'S MOVES");
            outlined(b, a, "THE PAINTED HALLS", 32.0, GOLD);
            b.spawn(txt(a, "Wake the old manor. Find your moves, open the gates, free the Heart.", 13.0, CREAM));
            if c.has_save {
                b.spawn(txt(a, &format!("SAVED: {}/{} stars - {} rooms - {} moves - {}", c.fragments, total_fragments(), c.rooms, c.abilities, clock(c.secs)), 13.0, GOLD_HI));
            }
            b.spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(8.0)), ..default() }).with_children(|r| {
                if c.has_save {
                    let mut x = button(r, a, ButtonSpec::new(ButtonKind::Gold, Val::Px(150.0), Val::Px(44.0), 19.0), "CONTINUE");
                    x.insert(Act::Continue);
                    let mut y = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(130.0), Val::Px(44.0), 17.0), "NEW GAME");
                    y.insert(Act::NewGame);
                } else {
                    let mut x = button(r, a, ButtonSpec::new(ButtonKind::Gold, Val::Px(170.0), Val::Px(44.0), 19.0), "BEGIN");
                    x.insert(Act::NewGame);
                }
                let mut m = button(r, a, ButtonSpec::new(ButtonKind::Chip, Val::Px(84.0), Val::Px(44.0), 14.0), "MENU");
                m.insert(Act::Menu);
            });
            b.spawn(txt(a, "Enter continue - N new game - Tab map - F cap", 11.0, hex(0x8f9be0)));
        });
    });
    root
}

fn clock(secs: u64) -> String {
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn spawn_dead(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(26), node, BackgroundColor(Color::srgba(0.06, 0.0, 0.04, 0.78)))).id();
    commands.entity(root).with_children(|p| {
        outlined(p, a, "YOU FADED INTO THE PAINT", 34.0, RED_HI);
        p.spawn(txt(a, "Back to the last pedestal. What you found stays found.", 14.0, CREAM));
    });
    root
}

fn spawn_win(commands: &mut Commands, a: &UiAssets, area: &SafeArea, w: &WinCard) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(26), node, BackgroundColor(Color::srgba(0.1, 0.07, 0.0, 0.7)))).id();
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(12.0)), row_gap: Val::Px(4.0), width: Val::Px(420.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
            kicker(c, a, "THE PAINTED HALLS");
            outlined(c, a, "THE HEART IS FREE!", 32.0, GOLD);
            let mut row = |l: &str, v: String| {
                c.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                    r.spawn(txt(a, l, 14.0, CREAM));
                    r.spawn((Text::new(clean(v)), font(&a.display, 17.0), TextColor(GOLD_HI), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                });
            };
            row("Star fragments", format!("{}/{}", w.fragments, w.total));
            row("Rooms found", format!("{}/{}", w.rooms, ROOM_COUNT));
            row("Abilities", format!("{}/6", w.abilities));
            row("Times faded", w.deaths.to_string());
            row("Time", clock(w.secs));
            c.spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(8.0)), ..default() }).with_children(|r| {
                let mut x = button(r, a, ButtonSpec::new(ButtonKind::Gold, Val::Px(180.0), Val::Px(44.0), 18.0), "KEEP EXPLORING");
                x.insert(Act::KeepPlaying);
                let mut y = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(140.0), Val::Px(44.0), 18.0), "MAIN MENU");
                y.insert(Act::Menu);
            });
        });
    });
    root
}

#[allow(clippy::too_many_arguments)]
fn manage_ui(mut commands: Commands, bridge: Res<HallsBridge>, assets: Option<Res<UiAssets>>, area: Res<SafeArea>, mut ui: ResMut<UiState>, children: Query<&Children>) {
    let Some(a) = assets else { return };
    if ui.hud.is_none() {
        let (root, refs) = spawn_hud(&mut commands, &a, &area);
        ui.hud = Some(root);
        ui.refs = refs;
        ui.mini_key = None;
    }
    // Minimap.
    let key = (bridge.map_gen, bridge.here);
    if ui.mini_key != Some(key) && !bridge.map.is_empty() {
        if let Some(m) = ui.refs.mini {
            rebuild_minimap(&mut commands, m, &children, &bridge);
            ui.mini_key = Some(key);
        }
    }
    // Full-screen layers.
    let want = (bridge.screen, if bridge.screen == Screen::Map { bridge.map_gen } else { 0 });
    let have = ui.screen.map(|(_, s, g)| (s, g));
    if have != Some(want) && !(bridge.screen == Screen::None && ui.screen.is_none()) {
        if let Some((e, ..)) = ui.screen.take() {
            commands.entity(e).despawn();
        }
        let e = match bridge.screen {
            Screen::None => None,
            Screen::Map => Some(spawn_map(&mut commands, &a, &area, &bridge)),
            Screen::Prompt => Some(spawn_prompt(&mut commands, &a, &area, &bridge.prompt)),
            Screen::Dead => Some(spawn_dead(&mut commands, &a, &area)),
            Screen::Win => Some(spawn_win(&mut commands, &a, &area, &bridge.win)),
        };
        ui.screen = e.map(|e| (e, want.0, want.1));
    }
}

fn buttons(mut bridge: ResMut<HallsBridge>, q: Query<(&Interaction, &Act), Changed<Interaction>>, state: Res<AppState>, mut menu: MessageWriter<ReturnToMenu>) {
    if *state != AppState::Playing {
        return;
    }
    for (i, act) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        match *act {
            Act::NewGame => bridge.cmds.push(UiCmd::NewGame),
            Act::Continue => bridge.cmds.push(UiCmd::Continue),
            Act::CloseMap => bridge.cmds.push(UiCmd::CloseMap),
            Act::OpenMap => bridge.cmds.push(UiCmd::OpenMap),
            Act::KeepPlaying => bridge.cmds.push(UiCmd::KeepPlaying),
            Act::Menu => {
                menu.write(ReturnToMenu);
            }
        }
    }
}

fn keys(mut bridge: ResMut<HallsBridge>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, state: Res<AppState>) {
    if *state != AppState::Playing {
        return;
    }
    let confirm = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) || pads.iter().any(|p| p.just_pressed(GamepadButton::South));
    let back = pads.iter().any(|p| p.just_pressed(GamepadButton::East));
    match bridge.screen {
        Screen::Prompt => {
            if confirm {
                bridge.cmds.push(UiCmd::Continue);
            } else if keys.just_pressed(KeyCode::KeyN) || pads.iter().any(|p| p.just_pressed(GamepadButton::West)) {
                bridge.cmds.push(UiCmd::NewGame);
            }
        }
        Screen::Map => {
            if back || keys.just_pressed(KeyCode::Escape) {
                bridge.cmds.push(UiCmd::CloseMap);
            }
        }
        Screen::Win => {
            if confirm {
                bridge.cmds.push(UiCmd::KeepPlaying);
            }
        }
        _ => {}
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
    if ui.hud.is_some() || ui.screen.is_some() {
        *ui = UiState::default();
    }
}
