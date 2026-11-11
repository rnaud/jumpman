//! Link's HUD, on-screen C buttons, item screen and ocarina staff on `bevy_ui`.
//!
//! * HUD: hearts (16 units each, `Inventory::health`), the magic meter, rupees / keys and the three
//!   C-button boxes with their item and ammo. The C boxes double as touch buttons: they sit in the
//!   row above the A/B grid (same rects as `touch::mode_layout`), with C-up above C-down and the
//!   ITEMS button left of C-left. Touches on them never start a camera drag.
//! * Item screen (Tab / M / D-pad up / ITEMS): ITEMS, EQUIPMENT, SONGS and SETTINGS tabs. Move with
//!   the arrows / D-pad or tap; assign the highlighted item to a C button with the three buttons at the
//!   bottom (or keys 1-2-3, or tapping a C box on screen); equipment is equipped when selected.
//! * Ocarina: while playing, the staff shows the recorded notes and the learned songs.
use super::icons::{Icon, IconCache};
use super::{link_active, LinkInput, LinkSettings, WorldFx};
use crate::characters::Character;
use crate::experiments::ExperimentSlot;
use crate::game::runner::GameSim;
use crate::input::PlayerInput;
use crate::safe_area::SafeArea;
use crate::shell::AppState;
use crate::sim_plugin::ModalActive;
use crate::touch::{mode_layout, TouchControls, MODE_SIZE};
use bevy::prelude::*;
use crate::button_marker::{Button, BorderRadius};
use bevy::window::PrimaryWindow;
use jumpman_sim::link::items::*;
use jumpman_sim::link::moves::{C_DOWN, C_LEFT, C_RIGHT, C_UP};
use jumpman_sim::link::ocarina::Song;
use jumpman_sim::link::Mv;
use std::collections::HashMap;

pub struct LinkUiPlugin;

impl Plugin for LinkUiPlugin {
    fn build(&self, app: &mut App) {
        let icons = IconCache::for_world(app.world());
        app.init_resource::<LinkMenu>()
            .insert_resource(icons)
            .init_resource::<MenuRects>()
            .add_systems(Startup, spawn_hud.in_set(crate::progress::PersistentSpawn))
            .add_systems(PreUpdate, touch_buttons.after(super::read_link_keys))
            .add_systems(PostUpdate, track_menu_rects.run_if(|| std::env::var_os("JUMPMAN_LINK_DEMO").is_some()).after(bevy::ui::UiSystems::Layout))
            .add_systems(Update, (update_hud, c_icons, punch_hud, menu_toggle, menu_input, menu_build, menu_clicks, update_overlays).chain().before(crate::CameraSet));
    }
}

/// Centres (logical px) of the item screen's buttons, kept only for scripted touch runs (`JUMPMAN_LINK_DEMO` `tap=cell3` and friends).
#[derive(Resource, Default)]
pub struct MenuRects(pub HashMap<String, (Vec2, Entity)>);

fn track_menu_rects(mut rects: ResMut<MenuRects>, menu: Res<LinkMenu>, q: Query<(Entity, &MenuAction, &ComputedNode, &UiGlobalTransform)>) {
    rects.0.clear();
    if !menu.open {
        return;
    }
    for (e, a, c, gt) in &q {
        let key = match a {
            MenuAction::Tab(i) => format!("tab{i}"),
            MenuAction::Cell(i) => format!("cell{i}"),
            MenuAction::Assign(i) => format!("assign{i}"),
            // The backdrop and the X share the action: the X is the smaller one.
            MenuAction::Close => if c.size().x < 400.0 { "close".to_string() } else { "backdrop".to_string() },
        };
        rects.0.insert(key, (gt.translation * c.inverse_scale_factor(), e));
    }
}

const ITEMS_BIT: u32 = 0x1_0000;
const BOXES: [(u32, &str); 4] = [(C_LEFT, "C<"), (C_DOWN, "Cv"), (C_RIGHT, "C>"), (C_UP, "C^")];

/// Rects of the touch buttons: C-left / down / right, C-up and ITEMS.
pub fn button_rects(size: Vec2, inset: Vec3) -> Vec<(u32, Rect)> {
    let base = mode_layout(size, inset);
    let rect = |i: usize| match base[i].1 {
        r => r,
    };
    let (l, d, r) = (rect(0), rect(1), rect(2));
    let up = Rect::from_corners(d.min - Vec2::new(0.0, MODE_SIZE + 6.0), d.max - Vec2::new(0.0, MODE_SIZE + 6.0));
    let items = Rect::from_corners(l.min - Vec2::new(MODE_SIZE + 6.0, 0.0), l.max - Vec2::new(MODE_SIZE + 6.0, 0.0));
    vec![(C_LEFT, l), (C_DOWN, d), (C_RIGHT, r), (C_UP, up), (ITEMS_BIT, items)]
}

#[derive(Component)]
struct HudRoot;
#[derive(Component)]
struct Heart(usize);
#[derive(Component)]
struct MagicFill;
#[derive(Component)]
struct MagicBack;
#[derive(Component)]
struct StatsText;
#[derive(Component)]
struct RupeeText;
/// A HUD piece that bumps when its value changes: 1 just changed .. 0 at rest.
#[derive(Component, Default)]
struct HudPunch(f32);

/// The bump of a HUD piece `punch` (1 .. 0) into its animation: a quick swell that settles.
pub fn punch_scale(punch: f32) -> f32 {
    let p = punch.clamp(0.0, 1.0);
    1.0 + 0.55 * p * p
}

/// OoT's low-health alarm (`Health_IsCritical`): one heart left with up to five containers, one and a half up to
/// ten, two up to fifteen, two and a half beyond.
pub fn health_critical(health: i32, max_health: i32) -> bool {
    let limit = if max_health <= 5 * HEART {
        HEART
    } else if max_health <= 10 * HEART {
        HEART * 3 / 2
    } else if max_health <= 15 * HEART {
        HEART * 2
    } else {
        HEART * 5 / 2
    };
    health > 0 && health <= limit
}

/// The size of the last filled heart `t` seconds into the alarm: a heartbeat, a quick swell and a rest.
pub fn heart_throb(t: f32) -> f32 {
    let beat = (t * 1.7).fract();
    1.0 + 0.28 * (1.0 - (beat / 0.45).min(1.0)).powi(2)
}

/// Which hearts bump when the health goes from `old` to `new` (sixteenths of a heart): every heart the change touches.
pub fn hearts_touched(old: i32, new: i32) -> std::ops::RangeInclusive<usize> {
    let (lo, hi) = (old.min(new).max(0), old.max(new).max(1));
    (lo / HEART) as usize..=((hi - 1) / HEART) as usize
}
#[derive(Component)]
struct CBox(u32);
#[derive(Component)]
struct CBoxText(u32);
#[derive(Component)]
struct CBoxIcon(u32);
#[derive(Component)]
struct StaffText;
#[derive(Component)]
struct NightOverlay;
#[derive(Component)]
struct Flash;
#[derive(Component)]
struct Rain(usize);

const HEART_PX: f32 = 15.0;
const MAX_HEARTS: usize = 20;

fn spawn_hud(mut commands: Commands) {
    let root = commands.spawn((HudRoot, Visibility::Hidden, GlobalZIndex(30), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() })).id();
    commands.entity(root).with_children(|p| {
        // Hearts, 10 per row.
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(16.0), top: Val::Px(12.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() }).with_children(|col| {
            for row in 0..2 {
                col.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(3.0), ..default() }).with_children(|r| {
                    for i in 0..10 {
                        r.spawn((Heart(row * 10 + i), HudPunch::default(), UiTransform::IDENTITY, Node { width: Val::Px(HEART_PX), height: Val::Px(HEART_PX), ..default() }, BackgroundColor(Color::srgb(0.9, 0.15, 0.2)), BorderRadius::all(Val::Px(5.0))));
                    }
                });
            }
            col.spawn((MagicBack, Node { width: Val::Px(100.0), height: Val::Px(9.0), margin: UiRect::top(Val::Px(3.0)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), BorderRadius::all(Val::Px(4.0))))
                .with_child((MagicFill, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgb(0.2, 0.8, 0.3)), BorderRadius::all(Val::Px(4.0))));
            col.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(2.0)), ..default() }).with_children(|row| {
                // the rupee counter: a green gem and the number, bumping on every rupee
                row.spawn((HudPunch::default(), UiTransform::IDENTITY, Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(5.0), ..default() })).with_children(|chip| {
                    chip.spawn((Node { width: Val::Px(9.0), height: Val::Px(15.0), ..default() }, BackgroundColor(Color::srgb(0.2, 0.85, 0.3)), BorderRadius::all(Val::Px(4.5))));
                    chip.spawn((RupeeText, Text::new("0"), TextFont { font_size: FontSize::Px(15.0), ..default() }, TextColor(Color::srgb(1.0, 0.97, 0.8)), TextShadow::default()));
                });
                row.spawn((StatsText, Text::new(""), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(Color::srgb(1.0, 0.95, 0.7)), TextShadow::default()));
            });
        });
        for (bit, label) in BOXES {
            p.spawn((CBox(bit), Node { position_type: PositionType::Absolute, width: Val::Px(MODE_SIZE), height: Val::Px(MODE_SIZE), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.35, 0.28, 0.05, 0.8)), BorderColor::all(Color::srgba(1.0, 0.9, 0.4, 0.8)), BorderRadius::MAX))
                .with_children(|b| {
                    // The picture of the item on this C button, with its ammo count (or "-") overlaid at the bottom.
                    b.spawn((CBoxIcon(bit), ImageNode::default(), Visibility::Hidden, Node { position_type: PositionType::Absolute, width: Val::Percent(86.0), height: Val::Percent(86.0), ..default() }));
                    b.spawn((CBoxText(bit), Text::new(label), TextFont { font_size: FontSize::Px(11.0), ..default() }, TextColor(Color::WHITE), TextShadow::default(), TextLayout::justify(Justify::Center), Node { position_type: PositionType::Absolute, bottom: Val::Px(-1.0), right: Val::Px(2.0), ..default() }));
                });
        }
        p.spawn((CBox(ITEMS_BIT), Node { position_type: PositionType::Absolute, width: Val::Px(MODE_SIZE), height: Val::Px(MODE_SIZE), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.09, 0.14, 0.23, 0.8)), BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.6)), BorderRadius::MAX))
            .with_child((Text::new("ITEMS"), TextFont { font_size: FontSize::Px(10.0), ..default() }, TextColor(Color::WHITE)));
        p.spawn((StaffText, Text::new(""), TextFont { font_size: FontSize::Px(20.0), ..default() }, TextColor(Color::srgb(1.0, 0.95, 0.6)), Node { position_type: PositionType::Absolute, bottom: Val::Px(70.0), left: Val::Percent(30.0), ..default() }));
    });
    // Screen overlays: night tint, lightning flash and rain streaks.
    commands.spawn((NightOverlay, Visibility::Hidden, GlobalZIndex(5), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.02, 0.05, 0.25, 0.38))));
    commands.spawn((Flash, Visibility::Hidden, GlobalZIndex(6), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0))));
    for i in 0..48 {
        commands.spawn((Rain(i), Visibility::Hidden, GlobalZIndex(6), Node { position_type: PositionType::Absolute, width: Val::Px(2.0), height: Val::Px(22.0), left: Val::Px(0.0), top: Val::Px(0.0), ..default() }, BackgroundColor(Color::srgba(0.75, 0.85, 1.0, 0.55))));
    }
}

/// The text on a C box: the ammo count for ammo items, "-" for an empty slot, nothing otherwise (the item is a picture).
fn c_label(inv: &Inventory, bit: u32) -> (String, bool) {
    let slot = match bit {
        C_LEFT => 0,
        C_DOWN => 1,
        C_RIGHT => 2,
        _ => return (String::new(), inv.has(Item::Ocarina)),
    };
    match inv.c[slot] {
        None => ("-".into(), false),
        Some(c) => {
            let ammo = match c {
                CItem::Item(Item::Bow | Item::FireArrow | Item::IceArrow | Item::LightArrow) => Some(Ammo::Arrow),
                CItem::Item(Item::Slingshot) => Some(Ammo::Seed),
                CItem::Item(Item::Bomb) => Some(Ammo::Bomb),
                CItem::Item(Item::Bombchu) => Some(Ammo::Bombchu),
                CItem::Item(Item::DekuNut) => Some(Ammo::Nut),
                CItem::Item(Item::DekuStick) => Some(Ammo::Stick),
                CItem::Item(Item::MagicBean) => Some(Ammo::Bean),
                _ => None,
            };
            (ammo.map(|a| inv.ammo(a).to_string()).unwrap_or_default(), true)
        }
    }
}

/// The picture on a C box (`None` for an empty slot).
pub fn c_icon(inv: &Inventory, bit: u32) -> Option<Icon> {
    let slot = match bit {
        C_LEFT => 0,
        C_DOWN => 1,
        C_RIGHT => 2,
        _ => return inv.has(Item::Ocarina).then_some(Icon::Item(Item::Ocarina)),
    };
    match inv.c[slot]? {
        CItem::Item(i) => Some(Icon::Item(i)),
        CItem::Bottle(n) => inv.bottles.get(n as usize).copied().flatten().map(Icon::Bottle),
    }
}

/// Puts the item pictures on the C boxes.
fn c_icons(sim: Res<GameSim>, mut cache: ResMut<IconCache>, mut images: ResMut<Assets<Image>>, mut icons: Query<(&CBoxIcon, &mut ImageNode, &mut Visibility)>) {
    let inv = &sim.mario.link.inv;
    for (b, mut img, mut vis) in &mut icons {
        match c_icon(inv, b.0) {
            Some(icon) => {
                let handle = cache.get(icon, &mut images);
                if img.image != handle {
                    img.image = handle;
                }
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Bumps the hearts and the rupee counter when their values change, and plays the bumps out.
fn punch_hud(
    time: Res<Time>,
    sim: Res<GameSim>,
    hearts: Query<(Entity, &Heart)>,
    mut rupees: Query<(&mut Text, &ChildOf), With<RupeeText>>,
    mut pieces: Query<(&mut HudPunch, &mut UiTransform)>,
    mut last: Local<Option<(i32, i32)>>,
) {
    let inv = &sim.mario.link.inv;
    // nothing bumps on the first frame
    let (old_health, old_rupees) = last.unwrap_or((inv.health, inv.rupees));
    *last = Some((inv.health, inv.rupees));
    if old_health != inv.health {
        let touched = hearts_touched(old_health, inv.health);
        for (e, h) in &hearts {
            if touched.contains(&h.0) {
                if let Ok((mut punch, _)) = pieces.get_mut(e) {
                    punch.0 = if inv.health > old_health { 1.0 } else { 0.7 };
                }
            }
        }
    }
    for (mut t, parent) in &mut rupees {
        if t.0.parse::<i32>().ok() != Some(inv.rupees) {
            t.0 = inv.rupees.to_string();
        }
        if inv.rupees > old_rupees {
            if let Ok((mut punch, _)) = pieces.get_mut(parent.parent()) {
                punch.0 = 1.0;
            }
        }
    }
    let dt = time.delta_secs();
    for (mut punch, mut xf) in &mut pieces {
        if punch.0 <= 0.0 && xf.scale == Vec2::ONE {
            continue;
        }
        punch.0 = (punch.0 - dt / 0.3).max(0.0);
        xf.scale = Vec2::splat(punch_scale(punch.0));
    }
    // Low health: the last heart with anything in it beats.
    if health_critical(inv.health, inv.max_health) {
        let beating = ((inv.health - 1) / HEART) as usize;
        for (e, h) in &hearts {
            if h.0 == beating {
                if let Ok((punch, mut xf)) = pieces.get_mut(e) {
                    if punch.0 <= 0.0 {
                        xf.scale = Vec2::splat(heart_throb(time.elapsed_secs()));
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_hud(
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    state: Option<Res<AppState>>,
    sim: Res<GameSim>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<SafeArea>,
    mut root: Query<&mut Visibility, With<HudRoot>>,
    mut hearts: Query<(&Heart, &mut BackgroundColor, &mut Node), (Without<CBox>, Without<MagicFill>, Without<MagicBack>)>,
    mut magic: Query<&mut Node, (With<MagicFill>, Without<Heart>, Without<CBox>, Without<MagicBack>)>,
    mut magic_back: Query<&mut Node, (With<MagicBack>, Without<Heart>, Without<CBox>, Without<MagicFill>)>,
    mut stats: Query<&mut Text, (With<StatsText>, Without<CBoxText>, Without<StaffText>, Without<RupeeText>)>,
    mut boxes: Query<(&CBox, &mut Node, &mut BackgroundColor, &mut BorderColor, &mut Visibility), (Without<CBoxIcon>, Without<Heart>, Without<MagicFill>, Without<MagicBack>, Without<HudRoot>)>,
    mut box_text: Query<(&CBoxText, &mut Text), (Without<StatsText>, Without<StaffText>, Without<RupeeText>)>,
    mut staff: Query<&mut Text, (With<StaffText>, Without<StatsText>, Without<CBoxText>, Without<RupeeText>)>,
    li: Res<LinkInput>,
) {
    let active = link_active(&character, &slot) && sim.mario.link.enabled && state.is_none_or(|s| *s == AppState::Playing || *s == AppState::Paused);
    for mut v in &mut root {
        *v = if active { Visibility::Inherited } else { Visibility::Hidden };
    }
    if !active {
        return;
    }
    let link = &sim.mario.link;
    let inv = &link.inv;
    for (h, mut bg, mut node) in &mut hearts {
        let container = h.0 as i32 * HEART;
        let show = container < inv.max_health;
        node.display = if show { Display::Flex } else { Display::None };
        let filled = (inv.health - container).clamp(0, HEART);
        bg.0 = if filled >= HEART {
            Color::srgb(0.9, 0.15, 0.2)
        } else if filled >= HEART / 2 {
            Color::srgb(0.9, 0.55, 0.3)
        } else if filled > 0 {
            Color::srgb(0.6, 0.25, 0.25)
        } else {
            Color::srgba(0.15, 0.05, 0.08, 0.7)
        };
    }
    for mut n in &mut magic_back {
        n.display = if inv.magic_cap > 0 { Display::Flex } else { Display::None };
        n.width = Val::Px(inv.magic_cap as f32 * 1.6);
    }
    for mut n in &mut magic {
        n.width = Val::Percent(if inv.magic_cap > 0 { 100.0 * inv.magic as f32 / inv.magic_cap as f32 } else { 0.0 });
    }
    for mut t in &mut stats {
        let text = format!("KEYS {}{}{}", inv.small_keys, if link.lens { "   LENS" } else { "" }, if link.nayru > 0.0 { "   NAYRU" } else { "" });
        if t.0 != text {
            t.0 = text;
        }
    }
    let rects = windows.single().ok().map(|w| button_rects(w.size(), Vec3::new(area.left, area.right, area.bottom)));
    for (b, mut node, mut bg, mut border, mut vis) in &mut boxes {
        if let Some(r) = rects.as_ref().and_then(|rs| rs.iter().find(|(bit, _)| *bit == b.0)).map(|(_, r)| *r) {
            node.left = Val::Px(r.min.x);
            node.top = Val::Px(r.min.y);
        }
        if b.0 == C_UP {
            *vis = if inv.has(Item::Ocarina) { Visibility::Inherited } else { Visibility::Hidden };
        }
        if b.0 != ITEMS_BIT {
            let (_, usable) = c_label(inv, b.0);
            let held = li.c_down & b.0 != 0;
            bg.0 = if held { Color::srgba(0.8, 0.65, 0.1, 0.9) } else if usable { Color::srgba(0.35, 0.28, 0.05, 0.8) } else { Color::srgba(0.12, 0.12, 0.12, 0.6) };
            *border = BorderColor::all(if held { Color::srgb(1.0, 0.9, 0.3) } else { Color::srgba(1.0, 0.9, 0.4, 0.8) });
        }
    }
    for (b, mut t) in &mut box_text {
        let (label, _) = c_label(inv, b.0);
        if t.0 != label {
            t.0 = label;
        }
    }
    for mut t in &mut staff {
        let text = if link.mv == Mv::Ocarina {
            let notes: String = link.staff.notes.iter().map(|n| n.glyph()).collect::<Vec<_>>().join(" ");
            format!("OCARINA   {}   (A  <  v  >  ^ ; B to put away)", if notes.is_empty() { "-".to_string() } else { notes })
        } else {
            String::new()
        };
        if t.0 != text {
            t.0 = text;
        }
    }
}

/// On-screen C buttons: claim touches that land on them and keep them away from the camera drag.
pub(crate) fn touch_buttons(
    touches: Res<Touches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<SafeArea>,
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    state: Option<Res<AppState>>,
    menu: Res<LinkMenu>,
    mut li: ResMut<LinkInput>,
    mut tc: ResMut<TouchControls>,
    mut claimed: Local<HashMap<u64, u32>>,
) {
    let on = link_active(&character, &slot) && !menu.open && state.is_none_or(|s| *s == AppState::Playing);
    if !on {
        claimed.clear();
        li.touch_bits = 0;
        return;
    }
    let Ok(window) = windows.single() else { return };
    let rects = button_rects(window.size(), Vec3::new(area.left, area.right, area.bottom));
    for t in touches.iter_just_pressed() {
        if let Some((bit, _)) = rects.iter().find(|(_, r)| r.contains(t.position())) {
            tc.visible = true;
            claimed.insert(t.id(), *bit);
            if *bit == ITEMS_BIT {
                li.menu_toggle = true;
            } else {
                li.c_pressed |= *bit;
            }
        }
    }
    claimed.retain(|id, _| touches.get_pressed(*id).is_some());
    let mut bits = 0;
    for (id, bit) in claimed.iter() {
        tc.end(*id);
        if *bit != ITEMS_BIT {
            bits |= *bit;
        }
    }
    li.touch_bits = bits;
    li.c_down |= bits;
}

// ---------------------------------------------------------------------------------------------
// Item screen
// ---------------------------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct LinkMenu {
    pub open: bool,
    pub tab: usize,
    pub cursor: usize,
    dirty: bool,
    root: Option<Entity>,
    message: String,
}

impl LinkMenu {
    /// Switch to a tab (scripted demos and tests).
    pub fn show_tab(&mut self, tab: usize) {
        self.tab = tab.min(TABS.len() - 1);
        self.cursor = 0;
        self.dirty = true;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellKind {
    Item(Item),
    Bottle(u8),
    Sword(Sword),
    Shield(Shield),
    Tunic(Tunic),
    Boots(Boots),
    Song(Song),
    LockMode,
    Refill,
    Hearts,
    /// Item tab: play Epona's Song for Link (summons her).
    EponaCall,
    /// Settings tab: a ridable Epona stands near the start of every course.
    EponaFollows,
    /// Settings: invert the vertical aim of the bow, slingshot and hookshot.
    InvertAim,
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub label: String,
    pub sub: String,
    pub enabled: bool,
    pub selected: bool,
    pub kind: CellKind,
}

pub const TABS: [&str; 4] = ["ITEMS", "EQUIPMENT", "SONGS", "SETTINGS"];

/// The grid of a tab. Pure: the item screen and its tests share it.
pub fn cells(tab: usize, inv: &Inventory, age: Age, toggle_lock: bool) -> Vec<Cell> {
    let mut out = vec![];
    let cell = |label: String, sub: String, enabled: bool, selected: bool, kind: CellKind| Cell { label, sub, enabled, selected, kind };
    match tab {
        0 => {
            for item in Item::ALL {
                if !inv.has(item) {
                    continue;
                }
                let on_c = inv.c.iter().position(|c| *c == Some(CItem::Item(item)));
                let name = inv.name_of(CItem::Item(item)).to_string();
                let sub = match item {
                    Item::DekuStick => format!("x{}", inv.ammo(Ammo::Stick)),
                    Item::DekuNut => format!("x{}", inv.ammo(Ammo::Nut)),
                    Item::Bomb => format!("x{}", inv.ammo(Ammo::Bomb)),
                    Item::Bow | Item::FireArrow | Item::IceArrow | Item::LightArrow => format!("x{}", inv.ammo(Ammo::Arrow)),
                    Item::Slingshot => format!("x{}", inv.ammo(Ammo::Seed)),
                    Item::Bombchu => format!("x{}", inv.ammo(Ammo::Bombchu)),
                    Item::MagicBean => format!("x{}", inv.ammo(Ammo::Bean)),
                    _ if item.magic_cost() > 0 => format!("MP {}", item.magic_cost()),
                    _ => String::new(),
                };
                let sub = match on_c {
                    Some(s) => format!("{sub} [{}]", ["C<", "Cv", "C>"][s]),
                    None => sub,
                };
                out.push(cell(name, sub, item.usable_by(age), on_c.is_some(), CellKind::Item(item)));
            }
            for (i, b) in inv.bottles.iter().enumerate() {
                if let Some(b) = b {
                    let on_c = inv.c.iter().position(|c| *c == Some(CItem::Bottle(i as u8)));
                    out.push(cell(b.label().to_string(), on_c.map(|s| format!("[{}]", ["C<", "Cv", "C>"][s])).unwrap_or_default(), true, on_c.is_some(), CellKind::Bottle(i as u8)));
                }
            }
        }
        1 => {
            for (i, s) in [Sword::Kokiri, Sword::Master, Sword::Biggoron].into_iter().enumerate() {
                if inv.swords[i] {
                    out.push(cell(s.label().into(), String::new(), s.age_req().allows(age), inv.sword == Some(s), CellKind::Sword(s)));
                }
            }
            for (i, s) in [Shield::Deku, Shield::Hylian, Shield::Mirror].into_iter().enumerate() {
                if inv.shields[i] {
                    out.push(cell(s.label().into(), String::new(), s.age_req().allows(age), inv.shield == Some(s), CellKind::Shield(s)));
                }
            }
            for (i, t) in [Tunic::Kokiri, Tunic::Goron, Tunic::Zora].into_iter().enumerate() {
                if inv.tunics[i] {
                    out.push(cell(t.label().into(), String::new(), t.age_req().allows(age), inv.tunic == t, CellKind::Tunic(t)));
                }
            }
            for (i, b) in [Boots::Kokiri, Boots::Iron, Boots::Hover].into_iter().enumerate() {
                if inv.boots_owned[i] {
                    out.push(cell(b.label().into(), String::new(), b.age_req().allows(age), inv.boots == b, CellKind::Boots(b)));
                }
            }
        }
        2 => {
            for s in Song::ALL {
                let glyphs: Vec<&str> = s.notes().iter().map(|n| n.glyph()).collect();
                out.push(cell(s.label().into(), glyphs.join(" "), true, false, CellKind::Song(s)));
            }
        }
        _ => {
            out.push(cell("Z-TARGETING".into(), if toggle_lock { "TOGGLE".into() } else { "HOLD".into() }, true, false, CellKind::LockMode));
            out.push(cell("REFILL AMMO, HEARTS, MAGIC".into(), String::new(), true, false, CellKind::Refill));
            out.push(cell("HEART CONTAINERS".into(), format!("{}", inv.max_health / HEART), true, false, CellKind::Hearts));
        }
    }
    out
}

/// [`cells`] plus Epona's entries: the EPONA item at the end of the items tab (adult Link only: a child cannot ride)
/// and the 'Epona follows' setting.
pub fn cells_with(tab: usize, inv: &Inventory, age: Age, settings: &LinkSettings) -> Vec<Cell> {
    let mut out = cells(tab, inv, age, settings.toggle_lock);
    let cell = |label: String, sub: String, enabled: bool, selected: bool, kind: CellKind| Cell { label, sub, enabled, selected, kind };
    match tab {
        0 => out.push(cell("EPONA".into(), if age == Age::Adult { "CALL".into() } else { "ADULT ONLY".into() }, age == Age::Adult, false, CellKind::EponaCall)),
        3 => {
            out.push(cell("INVERT AIM".into(), if settings.invert_aim_y { "ON".into() } else { "OFF".into() }, true, settings.invert_aim_y, CellKind::InvertAim));
            out.push(cell("EPONA FOLLOWS".into(), if settings.epona_follows { "ON".into() } else { "OFF".into() }, true, settings.epona_follows, CellKind::EponaFollows));
        }
        _ => {}
    }
    out
}

/// Epona from the menu (the EPONA item or Epona's Song in the song list): she is called like the ocarina song does.
/// Returns the toast text when the cell was one of hers.
pub fn call_epona_from_menu(kind: &CellKind, sim: &mut GameSim) -> Option<String> {
    let is_call = matches!(kind, CellKind::EponaCall) || matches!(kind, CellKind::Song(s) if *s == Song::Eponas);
    if !is_call {
        return None;
    }
    if sim.mario.link.age != Age::Adult {
        return Some("ONLY ADULT LINK CAN RIDE EPONA".into());
    }
    Some(super::summon_epona(sim))
}

/// Select a cell (A / tap). Returns a message for the toast line.
pub fn activate(kind: &CellKind, inv: &mut Inventory, age: Age, settings: &mut LinkSettings) -> String {
    match kind {
        CellKind::Item(i) => {
            if !i.usable_by(age) {
                return format!("{} CAN'T BE USED BY {}", i.label(), if age == Age::Child { "YOUNG LINK" } else { "ADULT LINK" });
            }
            // Auto-assign to the first free C button, otherwise show how to assign.
            if inv.c.contains(&Some(CItem::Item(*i))) {
                return format!("{} IS ON A C BUTTON - PICK C< Cv C> TO MOVE IT", i.label());
            }
            match inv.c.iter().position(|c| c.is_none()) {
                Some(s) => {
                    inv.set_c(s, Some(CItem::Item(*i)), age);
                    format!("{} -> {}", i.label(), ["C<", "Cv", "C>"][s])
                }
                None => format!("{}: PICK C< Cv C> TO ASSIGN", i.label()),
            }
        }
        CellKind::Bottle(n) => match inv.c.iter().position(|c| c.is_none()) {
            Some(s) => {
                inv.set_c(s, Some(CItem::Bottle(*n)), age);
                format!("BOTTLE -> {}", ["C<", "Cv", "C>"][s])
            }
            None => "PICK C< Cv C> TO ASSIGN".into(),
        },
        CellKind::Sword(s) => if inv.equip_sword(*s, age) { s.label().to_string() } else { "CAN'T EQUIP".into() },
        CellKind::Shield(s) => if inv.equip_shield(*s, age) { s.label().to_string() } else { "CAN'T EQUIP".into() },
        CellKind::Tunic(t) => if inv.equip_tunic(*t, age) { t.label().to_string() } else { "CAN'T EQUIP".into() },
        CellKind::Boots(b) => if inv.equip_boots(*b, age) { b.label().to_string() } else { "CAN'T EQUIP".into() },
        CellKind::Song(s) => {
            let glyphs: Vec<&str> = s.notes().iter().map(|n| n.glyph()).collect();
            format!("{}: {}", s.label(), glyphs.join(" "))
        }
        CellKind::LockMode => {
            settings.toggle_lock = !settings.toggle_lock;
            settings.save();
            if settings.toggle_lock { "Z-TARGETING: TOGGLE".into() } else { "Z-TARGETING: HOLD".into() }
        }
        CellKind::Refill => {
            inv.refill();
            inv.health = inv.max_health;
            inv.magic = inv.magic_cap;
            "REFILLED".into()
        }
        CellKind::EponaCall => "EPONA".into(),
        CellKind::InvertAim => {
            settings.invert_aim_y = !settings.invert_aim_y;
            settings.save();
            if settings.invert_aim_y { "INVERT AIM: ON (UP LOOKS DOWN)".into() } else { "INVERT AIM: OFF".into() }
        }
        CellKind::EponaFollows => {
            settings.epona_follows = !settings.epona_follows;
            settings.save();
            if settings.epona_follows { "EPONA FOLLOWS: ON (SHE WAITS NEAR THE START)".into() } else { "EPONA FOLLOWS: OFF".into() }
        }
        CellKind::Hearts => {
            inv.max_health = if inv.max_health >= 20 * HEART { 3 * HEART } else { inv.max_health + HEART };
            inv.health = inv.max_health;
            format!("{} HEARTS", inv.max_health / HEART)
        }
    }
}

/// Put the highlighted item on C button `slot`.
pub fn assign_cell(kind: &CellKind, slot: usize, inv: &mut Inventory, age: Age) -> String {
    let c = match kind {
        CellKind::Item(i) => CItem::Item(*i),
        CellKind::Bottle(n) => CItem::Bottle(*n),
        _ => return "SELECT AN ITEM FIRST".into(),
    };
    if inv.set_c(slot, Some(c), age) {
        format!("{} -> {}", inv.name_of(c), ["C<", "Cv", "C>"][slot])
    } else {
        "CAN'T BE USED BY THIS LINK".into()
    }
}

#[derive(Component, Clone, Copy, Debug)]
enum MenuAction {
    Tab(usize),
    Cell(usize),
    Assign(usize),
    Close,
}

#[allow(clippy::too_many_arguments)]
fn menu_toggle(
    mut li: ResMut<LinkInput>,
    mut menu: ResMut<LinkMenu>,
    mut modal: ResMut<ModalActive>,
    mut tc: ResMut<TouchControls>,
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    state: Option<Res<AppState>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let toggle = std::mem::take(&mut li.menu_toggle);
    // Never leave the screen stuck over another character / course (the shell can switch away while it is open).
    if menu.open && !link_active(&character, &slot) {
        menu.open = false;
        menu.dirty = true;
        modal.0 = false;
        tc.suppress = false;
        return;
    }
    let esc = menu.open && keys.just_pressed(KeyCode::Escape);
    if !(toggle || esc) {
        return;
    }
    if !link_active(&character, &slot) || state.is_some_and(|s| *s != AppState::Playing) {
        return;
    }
    menu.open = !menu.open && !esc;
    menu.dirty = true;
    modal.0 = menu.open;
    tc.suppress = menu.open;
    li.c_pressed = 0;
}

#[allow(clippy::too_many_arguments)]
fn menu_input(
    mut menu: ResMut<LinkMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut sim: ResMut<GameSim>,
    mut settings: ResMut<LinkSettings>,
    mut input: ResMut<PlayerInput>,
    mut li: ResMut<LinkInput>,
    mut modal: ResMut<ModalActive>,
    mut tc: ResMut<TouchControls>,
) {
    if !menu.open {
        return;
    }
    input.clear_edges();
    li.c_pressed = 0;
    let pad = pads.iter().next();
    let kp = |k: KeyCode| keys.just_pressed(k);
    let pp = |b: GamepadButton| pad.is_some_and(|p| p.just_pressed(b));
    let age = sim.mario.link.age;
    let inv_snapshot = sim.mario.link.inv.clone();
    let n = cells_with(menu.tab, &inv_snapshot, age, &settings).len();
    let mut changed = false;
    if kp(KeyCode::ArrowRight) || kp(KeyCode::KeyD) || pp(GamepadButton::DPadRight) {
        menu.cursor = (menu.cursor + 1) % n.max(1);
        changed = true;
    }
    if kp(KeyCode::ArrowLeft) || kp(KeyCode::KeyA) || pp(GamepadButton::DPadLeft) {
        menu.cursor = (menu.cursor + n.max(1) - 1) % n.max(1);
        changed = true;
    }
    if kp(KeyCode::ArrowDown) || kp(KeyCode::KeyS) || pp(GamepadButton::DPadDown) {
        menu.cursor = (menu.cursor + 4).min(n.saturating_sub(1));
        changed = true;
    }
    if kp(KeyCode::ArrowUp) || kp(KeyCode::KeyW) || pp(GamepadButton::DPadUp) {
        menu.cursor = menu.cursor.saturating_sub(4);
        changed = true;
    }
    if kp(KeyCode::KeyE) || pp(GamepadButton::RightTrigger) {
        menu.tab = (menu.tab + 1) % TABS.len();
        menu.cursor = 0;
        changed = true;
    }
    if kp(KeyCode::KeyQ) || pp(GamepadButton::LeftTrigger) {
        menu.tab = (menu.tab + TABS.len() - 1) % TABS.len();
        menu.cursor = 0;
        changed = true;
    }
    let list = cells_with(menu.tab, &inv_snapshot, age, &settings);
    let current = list.get(menu.cursor).map(|c| c.kind.clone());
    if let Some(kind) = current {
        if kp(KeyCode::Enter) || kp(KeyCode::Space) || pp(GamepadButton::South) {
            menu.message = match call_epona_from_menu(&kind, &mut sim) {
                Some(m) => {
                    menu.open = false;
                    modal.0 = false;
                    tc.suppress = false;
                    m
                }
                None => activate(&kind, &mut sim.mario.link.inv, age, &mut settings),
            };
            changed = true;
        }
        for (key, slot) in [(KeyCode::Digit1, 0), (KeyCode::Digit2, 1), (KeyCode::Digit3, 2)] {
            if kp(key) {
                menu.message = assign_cell(&kind, slot, &mut sim.mario.link.inv, age);
                changed = true;
            }
        }
        if pp(GamepadButton::West) {
            menu.message = assign_cell(&kind, 0, &mut sim.mario.link.inv, age);
            changed = true;
        }
        if pp(GamepadButton::North) {
            menu.message = assign_cell(&kind, 1, &mut sim.mario.link.inv, age);
            changed = true;
        }
        if pp(GamepadButton::RightTrigger2) {
            menu.message = assign_cell(&kind, 2, &mut sim.mario.link.inv, age);
            changed = true;
        }
    }
    if kp(KeyCode::Tab) || kp(KeyCode::KeyN) || pp(GamepadButton::East) || pp(GamepadButton::Start) {
        menu.open = false;
        modal.0 = false;
        tc.suppress = false;
        changed = true;
    }
    if changed {
        menu.dirty = true;
    }
}

/// The picture of an item-screen cell (`None` for the settings rows, which stay text).
pub fn cell_icon(kind: &CellKind, inv: &Inventory) -> Option<Icon> {
    Some(match kind {
        CellKind::Item(i) => Icon::Item(*i),
        CellKind::Bottle(n) => inv.bottles.get(*n as usize).copied().flatten().map_or(Icon::Bottle(Bottle::Empty), Icon::Bottle),
        CellKind::Sword(s) => Icon::Sword(*s),
        CellKind::Shield(s) => Icon::Shield(*s),
        CellKind::Tunic(t) => Icon::Tunic(*t),
        CellKind::Boots(b) => Icon::Boots(*b),
        CellKind::Song(s) => Icon::Song(*s),
        CellKind::EponaCall => Icon::Song(Song::Eponas),
        CellKind::LockMode | CellKind::Refill | CellKind::Hearts | CellKind::EponaFollows | CellKind::InvertAim => return None,
    })
}

const CELL: f32 = 58.0;

#[allow(clippy::too_many_arguments)]
fn menu_build(mut commands: Commands, mut menu: ResMut<LinkMenu>, sim: Res<GameSim>, settings: Res<LinkSettings>, mut cache: ResMut<IconCache>, mut images: ResMut<Assets<Image>>, device: Option<Res<crate::last_input::LastInput>>) {
    if !menu.dirty {
        return;
    }
    menu.dirty = false;
    if let Some(e) = menu.root.take() {
        commands.entity(e).despawn();
    }
    if !menu.open {
        return;
    }
    let link = &sim.mario.link;
    let list = cells_with(menu.tab, &link.inv, link.age, &settings);
    menu.cursor = menu.cursor.min(list.len().saturating_sub(1));
    let cursor = menu.cursor;
    let tab = menu.tab;
    let message = menu.message.clone();
    let desc = if !message.is_empty() { message } else { list.get(cursor).map(|c| if c.sub.is_empty() { c.label.clone() } else { format!("{}  {}", c.label, c.sub) }).unwrap_or_default() };
    let who = if link.age == Age::Child { "YOUNG LINK" } else { "ADULT LINK" };
    let touch = device.is_some_and(|d| d.device == crate::last_input::InputDevice::Touch);
    // The whole backdrop is a button: tapping outside the panel closes the screen, so it can never trap a touch player.
    let root = commands
        .spawn((Button, MenuAction::Close, GlobalZIndex(60), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.05, 0.82))))
        .id();
    commands.entity(root).with_children(|p| {
        // The panel blocks taps (they must not fall through to the backdrop) and never grows past the screen: the header and the
        // footer keep their size and the item grid is the part that shrinks.
        p.spawn((bevy::ui::FocusPolicy::Block, Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), padding: UiRect::all(Val::Px(10.0)), max_width: Val::Percent(98.0), max_height: Val::Percent(96.0), ..default() }, BackgroundColor(Color::srgba(0.04, 0.06, 0.12, 0.6)), BorderRadius::all(Val::Px(10.0)))).with_children(|panel| {
            panel.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }).with_children(|row| {
                row.spawn((Text::new(who), TextFont { font_size: FontSize::Px(15.0), ..default() }, TextColor(Color::srgb(1.0, 0.9, 0.4)), Node { margin: UiRect::right(Val::Px(8.0)), ..default() }));
                for (i, name) in TABS.iter().enumerate() {
                    button(row, MenuAction::Tab(i), name, i == tab, true, 13.0, 92.0, 38.0);
                }
                button(row, MenuAction::Close, "X CLOSE", false, true, 14.0, 92.0, 38.0);
            });
            panel.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), max_width: Val::Px(760.0), min_height: Val::Px(0.0), flex_shrink: 1.0, overflow: Overflow::clip(), ..default() }).with_children(|grid| {
                for (i, c) in list.iter().enumerate() {
                    match cell_icon(&c.kind, &link.inv) {
                        Some(icon) => icon_button(grid, MenuAction::Cell(i), cache.get(icon, &mut images), &c.sub, i == cursor, c.enabled),
                        None => {
                            let label = if c.sub.is_empty() { c.label.clone() } else { format!("{}\n{}", c.label, c.sub) };
                            button(grid, MenuAction::Cell(i), &label, i == cursor, c.enabled, 12.0, 164.0, CELL);
                        }
                    }
                }
            });
            panel.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(8.0), align_items: AlignItems::Center, flex_wrap: FlexWrap::Wrap, flex_shrink: 0.0, ..default() }).with_children(|row| {
                row.spawn((Text::new(desc), TextFont { font_size: FontSize::Px(15.0), ..default() }, TextColor(Color::srgb(0.85, 1.0, 0.85)), Node { min_width: Val::Px(180.0), flex_grow: 1.0, ..default() }));
                if tab == 0 {
                    row.spawn((Text::new("ASSIGN TO"), TextFont { font_size: FontSize::Px(13.0), ..default() }, TextColor(Color::WHITE)));
                    for (s, name) in ["C <", "C v", "C >"].iter().enumerate() {
                        button(row, MenuAction::Assign(s), name, false, true, 15.0, 58.0, 38.0);
                    }
                }
            });
            let hint = if touch { "Tap an item to use it or put it on a C button. Tap outside, or X CLOSE, to go back." } else { "Arrows/D-pad move   Enter/A select   1 2 3 / X Y R2 assign   Q E / L1 R1 tabs   Tab/B close" };
            panel.spawn((Text::new(hint), TextFont { font_size: FontSize::Px(11.0), ..default() }, TextColor(Color::srgba(1.0, 1.0, 1.0, 0.55)), Node { flex_shrink: 0.0, ..default() }));
        });
    });
    menu.root = Some(root);
}

fn icon_button(parent: &mut ChildSpawnerCommands, action: MenuAction, image: Handle<Image>, sub: &str, selected: bool, enabled: bool) {
    let bg = if selected { Color::srgba(0.75, 0.6, 0.1, 0.95) } else if enabled { Color::srgba(0.12, 0.17, 0.28, 0.95) } else { Color::srgba(0.12, 0.12, 0.12, 0.8) };
    let tint = if enabled { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.35) };
    parent
        .spawn((Button, action, Node { width: Val::Px(CELL), height: Val::Px(CELL), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(bg), BorderColor::all(if selected { Color::srgb(1.0, 0.95, 0.5) } else { Color::srgba(1.0, 1.0, 1.0, 0.35) }), BorderRadius::all(Val::Px(8.0))))
        .with_children(|b| {
            let mut img = ImageNode::new(image);
            img.color = tint;
            b.spawn((img, Node { width: Val::Px(CELL - 12.0), height: Val::Px(CELL - 12.0), ..default() }));
            if !sub.is_empty() && sub.len() <= 9 {
                // "x20" ammo and "[C<]" slot marks sit in the corner.
                b.spawn((Text::new(sub), TextFont { font_size: FontSize::Px(10.0), ..default() }, TextColor(Color::WHITE), TextShadow::default(), Node { position_type: PositionType::Absolute, bottom: Val::Px(0.0), right: Val::Px(2.0), ..default() }));
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn button(parent: &mut ChildSpawnerCommands, action: MenuAction, label: &str, selected: bool, enabled: bool, font: f32, w: f32, h: f32) {
    let bg = if selected { Color::srgba(0.75, 0.6, 0.1, 0.95) } else if enabled { Color::srgba(0.12, 0.17, 0.28, 0.95) } else { Color::srgba(0.12, 0.12, 0.12, 0.8) };
    parent
        .spawn((Button, action, Node { width: Val::Px(w), height: Val::Px(h), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(bg), BorderColor::all(if selected { Color::srgb(1.0, 0.95, 0.5) } else { Color::srgba(1.0, 1.0, 1.0, 0.35) }), BorderRadius::all(Val::Px(8.0))))
        .with_child((Text::new(label), TextFont { font_size: FontSize::Px(font), ..default() }, TextColor(if enabled { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.4) }), TextLayout::justify(Justify::Center)));
}

#[allow(clippy::too_many_arguments)]
fn menu_clicks(
    q: Query<(&Interaction, &MenuAction), Changed<Interaction>>,
    mut menu: ResMut<LinkMenu>,
    mut sim: ResMut<GameSim>,
    mut settings: ResMut<LinkSettings>,
    mut modal: ResMut<ModalActive>,
    mut tc: ResMut<TouchControls>,
) {
    for (i, a) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        let age = sim.mario.link.age;
        let list = cells_with(menu.tab, &sim.mario.link.inv, age, &settings);
        match *a {
            MenuAction::Tab(t) => {
                menu.tab = t;
                menu.cursor = 0;
            }
            MenuAction::Cell(c) => {
                menu.cursor = c;
                if let Some(cell) = list.get(c) {
                    let kind = cell.kind.clone();
                    menu.message = match call_epona_from_menu(&kind, &mut sim) {
                        Some(m) => {
                            menu.open = false;
                            modal.0 = false;
                            tc.suppress = false;
                            m
                        }
                        None => activate(&kind, &mut sim.mario.link.inv, age, &mut settings),
                    };
                }
            }
            MenuAction::Assign(s) => {
                if let Some(cell) = list.get(menu.cursor) {
                    let kind = cell.kind.clone();
                    menu.message = assign_cell(&kind, s, &mut sim.mario.link.inv, age);
                }
            }
            MenuAction::Close => {
                menu.open = false;
                modal.0 = false;
                tc.suppress = false;
            }
        }
        menu.dirty = true;
    }
}

/// Night tint (Sun's Song), rain and lightning (Song of Storms).
#[allow(clippy::too_many_arguments)]
fn update_overlays(
    mut fx: ResMut<WorldFx>,
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut night: Query<&mut Visibility, (With<NightOverlay>, Without<Flash>, Without<Rain>)>,
    mut flash: Query<(&mut Visibility, &mut BackgroundColor), (With<Flash>, Without<NightOverlay>, Without<Rain>)>,
    mut rain: Query<(&Rain, &mut Visibility, &mut Node), (Without<Flash>, Without<NightOverlay>)>,
) {
    let dt = time.delta_secs();
    for mut v in &mut night {
        *v = if fx.night || fx.storm > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
    }
    fx.storm = (fx.storm - dt).max(0.0);
    fx.lightning = (fx.lightning - dt * 1.4).max(0.0);
    if fx.storm > 0.0 && fx.storm < 22.0 && (fx.storm * 7.0).fract() < dt * 0.4 {
        fx.lightning = 0.8;
    }
    for (mut v, mut bg) in &mut flash {
        *v = if fx.lightning > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        bg.0 = Color::srgba(1.0, 1.0, 1.0, fx.lightning * 0.55);
    }
    let size = windows.single().map(|w| w.size()).unwrap_or(Vec2::new(800.0, 450.0));
    let t = time.elapsed_secs();
    for (r, mut v, mut node) in &mut rain {
        *v = if fx.storm > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        let seed = r.0 as f32 * 12.9898;
        let x = (seed.sin() * 43758.547).fract().abs() * size.x;
        let speed = 500.0 + (seed.cos() * 7919.0).fract().abs() * 300.0;
        node.left = Val::Px(x);
        node.top = Val::Px(((t * speed) + seed * 37.0) % (size.y + 40.0) - 20.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::{Rect, Vec2, Vec3};

    #[test]
    fn the_last_heart_beats_when_health_is_critical() {
        assert!(health_critical(HEART, 3 * HEART) && health_critical(1, 3 * HEART));
        assert!(!health_critical(HEART + 1, 3 * HEART) && !health_critical(0, 3 * HEART), "dead is not an alarm");
        assert!(health_critical(HEART * 3 / 2, 8 * HEART) && !health_critical(HEART * 2, 8 * HEART));
        assert!(health_critical(HEART * 5 / 2, 20 * HEART));
        // a beat swells at once and is back at rest for the second half of the period
        assert!((heart_throb(0.0) - 1.28).abs() < 1e-5);
        let period = 1.0 / 1.7;
        assert_eq!(heart_throb(period * 0.6), 1.0);
        let mut t = 0.0;
        while t < 2.0 {
            assert!((1.0..=1.2801).contains(&heart_throb(t)));
            t += 0.013;
        }
    }

    #[test]
    fn a_health_change_bumps_the_hearts_it_touches() {
        // half a heart lost from three full hearts: only the third heart
        assert_eq!(hearts_touched(3 * HEART, 3 * HEART - HEART / 2), 2..=2);
        // a fairy refills from a quarter heart to three hearts: all three
        assert_eq!(hearts_touched(HEART / 4, 3 * HEART), 0..=2);
        // losing exactly the last heart
        assert_eq!(hearts_touched(2 * HEART, HEART), 1..=1);
        // dying from one heart touches the first only
        assert_eq!(hearts_touched(HEART, 0), 0..=0);
        assert_eq!(punch_scale(0.0), 1.0);
        assert!(punch_scale(1.0) > punch_scale(0.4) && punch_scale(1.0) < 1.6);
    }

    #[test]
    fn touch_buttons_fit_and_do_not_overlap_the_base_controls() {
        for (size, inset) in [(Vec2::new(844.0, 390.0), Vec3::new(47.0, 47.0, 21.0)), (Vec2::new(874.0, 402.0), Vec3::new(52.0, 52.0, 21.0)), (Vec2::new(1040.0, 680.0), Vec3::ZERO)] {
            let base = crate::touch::layout(size, inset);
            let pause = crate::touch::pause_rect(size, inset.y, 0.0);
            let mine = button_rects(size, inset);
            assert_eq!(mine.len(), 5);
            for (i, (bit, r)) in mine.iter().enumerate() {
                assert!(r.min.x >= 0.0 && r.max.x <= size.x && r.min.y >= 0.0 && r.max.y <= size.y - inset.z, "{bit:x} {r:?} outside {size:?}");
                assert!(r.intersect(pause).is_empty(), "{bit:x} overlaps pause");
                for (c, b) in &base {
                    assert!(r.intersect(*b).is_empty(), "{bit:x} overlaps {c:?}");
                }
                for (b2, o) in &mine[i + 1..] {
                    assert!(r.intersect(*o).is_empty(), "{bit:x} and {b2:x} overlap");
                }
            }
        }
        let _ = Rect::default();
    }

    #[test]
    fn item_grid_lists_owned_items_and_marks_age() {
        let inv = Inventory::full(Age::Adult);
        let list = cells(0, &inv, Age::Adult, true);
        assert!(list.iter().any(|c| c.kind == CellKind::Item(Item::Bow) && c.enabled));
        assert!(list.iter().any(|c| c.kind == CellKind::Item(Item::Slingshot) && !c.enabled), "child item greyed for adult");
        assert_eq!(list.iter().filter(|c| matches!(c.kind, CellKind::Bottle(_))).count(), 4);
        assert!(list.iter().find(|c| c.kind == CellKind::Item(Item::Bow)).unwrap().selected, "on a C button");
    }

    #[test]
    fn activating_an_item_fills_a_free_c_button_and_assign_moves_it() {
        let mut inv = Inventory::full(Age::Adult);
        let mut settings = LinkSettings::default();
        inv.c = [None; 3];
        let msg = activate(&CellKind::Item(Item::Hookshot), &mut inv, Age::Adult, &mut settings);
        assert!(msg.contains("C<"));
        assert_eq!(inv.c[0], Some(CItem::Item(Item::Hookshot)));
        let msg = assign_cell(&CellKind::Item(Item::Hookshot), 2, &mut inv, Age::Adult);
        assert!(msg.contains("C>"));
        assert_eq!(inv.c, [None, None, Some(CItem::Item(Item::Hookshot))]);
        let msg = activate(&CellKind::Item(Item::Slingshot), &mut inv, Age::Adult, &mut settings);
        assert!(msg.contains("CAN'T"), "{msg}");
        assert_eq!(assign_cell(&CellKind::Sword(Sword::Master), 0, &mut inv, Age::Adult), "SELECT AN ITEM FIRST");
    }

    #[test]
    fn equipment_tab_equips_by_age() {
        let mut inv = Inventory::full(Age::Child);
        let mut s = LinkSettings::default();
        assert!(activate(&CellKind::Sword(Sword::Master), &mut inv, Age::Child, &mut s).contains("CAN'T"));
        assert_eq!(inv.sword, Some(Sword::Kokiri));
        activate(&CellKind::Shield(Shield::Hylian), &mut inv, Age::Child, &mut s);
        assert_eq!(inv.shield, Some(Shield::Hylian));
        let list = cells(1, &inv, Age::Child, true);
        assert!(list.iter().any(|c| c.kind == CellKind::Boots(Boots::Iron) && !c.enabled));
    }

    #[test]
    fn songs_tab_and_settings_tab() {
        let inv = Inventory::full(Age::Adult);
        assert_eq!(cells(2, &inv, Age::Adult, true).len(), 12);
        let mut s = LinkSettings::default();
        assert!(s.toggle_lock, "toggle is the default");
        let mut inv2 = inv.clone();
        let list = cells(3, &inv2, Age::Adult, s.toggle_lock);
        assert_eq!(list[0].sub, "TOGGLE");
        // Do not touch the user's settings file in tests.
        let before = s.toggle_lock;
        s.toggle_lock = !before;
        assert_eq!(cells(3, &inv2, Age::Adult, s.toggle_lock)[0].sub, "HOLD");
        inv2.health = 1;
        activate(&CellKind::Refill, &mut inv2, Age::Adult, &mut LinkSettings { toggle_lock: s.toggle_lock, ..Default::default() });
        assert_eq!(inv2.health, inv2.max_health);
        // The aim-invert toggle is a settings cell that mirrors `invert_aim_y` (not activated here: it would write link.json).
        let with = cells_with(3, &inv2, Age::Adult, &LinkSettings { invert_aim_y: true, ..Default::default() });
        let invert = with.iter().find(|c| c.kind == CellKind::InvertAim).expect("invert aim cell");
        assert_eq!((invert.sub.as_str(), invert.selected), ("ON", true));
        assert_eq!(cells_with(3, &inv2, Age::Adult, &LinkSettings::default()).iter().find(|c| c.kind == CellKind::InvertAim).unwrap().sub, "OFF");
    }
    #[test]
    fn item_and_equipment_cells_all_have_pictures_and_settings_stay_text() {
        let inv = Inventory::full(Age::Adult);
        let settings = LinkSettings::default();
        for tab in 0..3 {
            for c in cells_with(tab, &inv, Age::Adult, &settings) {
                assert!(cell_icon(&c.kind, &inv).is_some(), "{} has no icon", c.label);
            }
        }
        for c in cells_with(3, &inv, Age::Adult, &settings) {
            assert!(cell_icon(&c.kind, &inv).is_none(), "{} is a text row", c.label);
        }
        // An empty bottle still draws a bottle.
        assert_eq!(cell_icon(&CellKind::Bottle(0), &Inventory::full(Age::Adult)), Some(Icon::Bottle(Bottle::RedPotion)));
    }

    #[test]
    fn c_boxes_show_the_item_picture_and_only_ammo_text() {
        let mut inv = Inventory::full(Age::Adult);
        inv.c = [Some(CItem::Item(Item::Bomb)), Some(CItem::Bottle(2)), None];
        assert_eq!(c_icon(&inv, C_LEFT), Some(Icon::Item(Item::Bomb)));
        assert_eq!(c_icon(&inv, C_DOWN), Some(Icon::Bottle(Bottle::BluePotion)));
        assert_eq!(c_icon(&inv, C_RIGHT), None);
        assert_eq!(c_icon(&inv, C_UP), Some(Icon::Item(Item::Ocarina)));
        assert_eq!(c_label(&inv, C_LEFT).0, inv.ammo(Ammo::Bomb).to_string());
        assert_eq!(c_label(&inv, C_DOWN).0, "");
        assert_eq!(c_label(&inv, C_RIGHT).0, "-");
    }

    fn menu_app(character: Character) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<LinkInput>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ExperimentSlot>()
            .init_resource::<ModalActive>()
            .init_resource::<TouchControls>()
            .insert_resource(character)
            .insert_resource(AppState::Playing)
            .insert_resource(LinkMenu { open: true, ..default() });
        app.world_mut().resource_mut::<ModalActive>().0 = true;
        app.world_mut().resource_mut::<TouchControls>().suppress = true;
        app
    }

    #[test]
    fn the_item_screen_cannot_outlive_link() {
        // The shell switched the character (or an experiment started) while the screen was open: it closes by itself.
        let mut app = menu_app(Character::Mario);
        app.add_systems(Update, menu_toggle);
        app.update();
        assert!(!app.world().resource::<LinkMenu>().open);
        assert!(!app.world().resource::<ModalActive>().0, "the game is not left modal");
        assert!(!app.world().resource::<TouchControls>().suppress, "touch controls are not left suppressed");
    }

    #[test]
    fn tapping_the_backdrop_or_close_dismisses_the_screen_on_touch() {
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let sim = GameSim::load(&assets, crate::game::world::CourseRequest::new("kokiri_forest")).unwrap();
        let mut app = menu_app(Character::Link);
        app.insert_resource(sim).init_resource::<LinkSettings>().add_systems(Update, menu_clicks);
        // The backdrop button and the X CLOSE button both carry `MenuAction::Close`.
        app.world_mut().spawn((Interaction::Pressed, MenuAction::Close));
        app.update();
        assert!(!app.world().resource::<LinkMenu>().open);
        assert!(!app.world().resource::<ModalActive>().0);
        assert!(!app.world().resource::<TouchControls>().suppress);
    }
}
