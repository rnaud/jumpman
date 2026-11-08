//! Experiment chrome in the 90s theme (`ui.ts`): clickable dock (segmented controls / actions),
//! title card, experiment menu, plus the shared mouse state experiments read through `Host`.
//!
//! A dock button is just a key: clicking it delivers `keydown(key)` to the experiment, so every
//! keyboard binding is also a click target. Groups (`DockItem::group`) render as segmented rows.
use super::{ExperimentId, ExperimentSlot, XpUi, EXPERIMENT_IDS};
use crate::camera::MainCamera;
use crate::safe_area::SafeArea;
use crate::shell::theme::{self, ButtonKind, ButtonSpec, Selected, UiAssets};
use crate::touch::{TouchControls, BUTTON_GAP, BUTTON_SIZE, MARGIN_X, MARGIN_Y, PAD_SIZE};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

#[derive(Clone, Debug, PartialEq)]
pub struct DockItem {
    pub label: String,
    pub key: KeyCode,
    pub group: u8,
    pub selected: bool,
}

impl DockItem {
    pub fn new(label: &str, key: KeyCode, group: u8) -> Self {
        Self { label: label.into(), key, group, selected: false }
    }
}

#[derive(Resource, Default)]
pub struct XpDock {
    pub items: Vec<DockItem>,
    dirty: bool,
}

impl XpDock {
    pub fn set(&mut self, items: Vec<DockItem>) {
        if self.items != items {
            self.items = items;
            self.dirty = true;
        }
    }
    /// Mark exactly the items whose key is in `keys` as selected.
    pub fn select(&mut self, keys: &[KeyCode]) {
        for i in &mut self.items {
            let on = keys.contains(&i.key);
            if i.selected != on {
                i.selected = on;
                self.dirty = true;
            }
        }
    }
    /// Selected within one group only.
    pub fn select_group(&mut self, group: u8, key: KeyCode) {
        for i in self.items.iter_mut().filter(|i| i.group == group) {
            let on = i.key == key;
            if i.selected != on {
                i.selected = on;
                self.dirty = true;
            }
        }
    }
    pub fn clear(&mut self) {
        if !self.items.is_empty() {
            self.items.clear();
            self.dirty = true;
        }
    }
}

/// Keys queued by clicks on dock buttons.
#[derive(Resource, Default)]
pub struct DockClicks(pub Vec<KeyCode>);

/// Cursor, buttons and camera projection for this frame.
#[derive(Resource, Default)]
pub struct MouseState {
    pub cursor: Option<Vec2>,
    pub left: bool,
    pub left_just_pressed: bool,
    pub left_just_released: bool,
    pub right: bool,
    pub right_just_pressed: bool,
    pub scroll: f32,
    /// Cursor is over one of the experiment's buttons.
    pub over_ui: bool,
    pub window: Vec2,
    camera: Option<(Camera, GlobalTransform)>,
    scale: crate::fx::render_scale::RenderScale,
}

impl MouseState {
    /// World-space ray (SM64 units) under the cursor.
    pub fn ray(&self) -> Option<(DVec3, DVec3)> {
        let (cam, tf) = self.camera.as_ref()?;
        let ray = cam.viewport_to_world(tf, self.scale.to_target(self.cursor?)).ok()?;
        let o = ray.origin / crate::render::SCALE;
        Some((DVec3::new(o.x as f64, o.y as f64, o.z as f64), DVec3::new(ray.direction.x as f64, ray.direction.y as f64, ray.direction.z as f64)))
    }
    /// Screen position (logical px) of an SM64-space point.
    pub fn project(&self, p: [f64; 3]) -> Option<Vec2> {
        let (cam, tf) = self.camera.as_ref()?;
        cam.world_to_viewport(tf, crate::render::to_bevy(p)).ok().map(|v| self.scale.to_window(v))
    }
    /// Normalised cursor in -1..1 (y up).
    pub fn ndc(&self) -> Option<Vec2> {
        let c = self.cursor?;
        Some(Vec2::new(c.x / self.window.x * 2.0 - 1.0, 1.0 - c.y / self.window.y * 2.0))
    }
}

/// Test hook: `JUMPMAN_XP_MOUSE="2.0:640,360:move,3.0:640,360:down,4.0:700,380:up"` (time:x,y:action).
fn scripted_mouse(now: f64) -> Option<(Vec2, bool)> {
    let script = std::env::var("JUMPMAN_XP_MOUSE").ok()?;
    let mut out = None;
    let mut down = false;
    let parts: Vec<&str> = script.split(',').collect();
    for item in parts.chunks(2) {
        let [a, b] = item else { continue };
        let (t, x) = a.split_once(':')?;
        let (y, act) = b.split_once(':')?;
        let (t, x, y): (f64, f32, f32) = (t.parse().ok()?, x.parse().ok()?, y.parse().ok()?);
        if t > now {
            break;
        }
        match act {
            "down" => down = true,
            "up" => down = false,
            _ => {}
        }
        out = Some(Vec2::new(x, y));
    }
    out.map(|p| (p, down))
}

#[allow(clippy::too_many_arguments)]
pub fn update_mouse(
    mut state: ResMut<MouseState>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: EventReader<bevy::input::mouse::MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    ui: Query<&Interaction, With<Button>>,
    time: Res<Time>,
    mut script_down: Local<bool>,
    scale: Option<Res<crate::fx::render_scale::RenderScale>>,
) {
    let now = time.elapsed_secs_f64();
    let mut cursor = window.cursor_position();
    let (mut left, mut left_p, mut left_r) = (buttons.pressed(MouseButton::Left), buttons.just_pressed(MouseButton::Left), buttons.just_released(MouseButton::Left));
    if let Some((p, down)) = scripted_mouse(now) {
        cursor = Some(p);
        left_p |= down && !*script_down;
        left_r |= !down && *script_down;
        left |= down;
        *script_down = down;
    }
    state.cursor = cursor;
    state.left = left;
    state.left_just_pressed = left_p;
    state.left_just_released = left_r;
    state.right = buttons.pressed(MouseButton::Right);
    state.right_just_pressed = buttons.just_pressed(MouseButton::Right);
    state.scroll = wheel.read().map(|e| e.y).sum();
    state.window = Vec2::new(window.width(), window.height());
    state.over_ui = ui.iter().any(|i| *i != Interaction::None);
    state.camera = Some((camera.0.clone(), *camera.1));
    state.scale = scale.map(|s| *s).unwrap_or_default();
}

#[derive(Component)]
pub struct DockRoot;
#[derive(Component)]
pub struct CardRoot;
#[derive(Component)]
pub struct MenuRoot;
#[derive(Component)]
pub struct DockButton(pub KeyCode);
#[derive(Component)]
pub struct MenuButton(pub ExperimentId);

pub fn spawn_roots(mut commands: Commands) {
    commands.spawn((DockRoot, Node { position_type: PositionType::Absolute, bottom: Val::Px(14.0), left: Val::Px(0.0), right: Val::Px(0.0), justify_content: JustifyContent::Center, ..default() }, GlobalZIndex(20)));
    commands.spawn((CardRoot, Node { position_type: PositionType::Absolute, top: Val::Px(120.0), left: Val::Px(14.0), max_width: Val::Px(330.0), ..default() }, GlobalZIndex(20)));
    commands.spawn((
        MenuRoot,
        Node { position_type: PositionType::Absolute, top: Val::Px(0.0), bottom: Val::Px(0.0), left: Val::Px(0.0), right: Val::Px(0.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        GlobalZIndex(30),
        Visibility::Hidden,
    ));
}

/// Phones in landscape: the card and dock shrink and keep clear of the notch, stick, buttons and pause button.
pub fn is_compact(window: &Window) -> bool {
    window.height() < 520.0
}

/// Dock root insets (left, right, bottom) with the touch overlay up: between the stick and the A/B grid,
/// above the home indicator.
pub fn touch_dock_insets(area: &SafeArea) -> (f32, f32, f32) {
    let grid = 2.0 * BUTTON_SIZE + BUTTON_GAP;
    (MARGIN_X.max(area.left) + PAD_SIZE + 12.0, MARGIN_X.max(area.right) + grid + 12.0, MARGIN_Y.max(area.bottom + 6.0))
}

/// Position the dock and card for the window and, when the touch overlay is up, between the stick (left) and the
/// A/B grid (right) and above the home indicator.
pub fn place_roots(
    touch: Res<TouchControls>,
    area: Res<SafeArea>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut dock: Single<&mut Node, (With<DockRoot>, Without<CardRoot>)>,
    mut card: Single<&mut Node, (With<CardRoot>, Without<DockRoot>)>,
) {
    let compact = is_compact(&window);
    let (left, right, bottom) = if touch.visible { touch_dock_insets(&area) } else { (0.0, 0.0, 14.0) };
    let want = (Val::Px(left), Val::Px(right), Val::Px(bottom));
    if (dock.left, dock.right, dock.bottom) != want {
        (dock.left, dock.right, dock.bottom) = want;
    }
    let (top, x, width) = if compact { (10.0 + area.top.min(24.0), 14.0f32.max(area.left + 8.0), 290.0) } else { (120.0, 14.0, 330.0) };
    let want = (Val::Px(top), Val::Px(x), Val::Px(width));
    if (card.top, card.left, card.max_width) != want {
        (card.top, card.left, card.max_width) = want;
    }
}

/// Rebuild the dock when its items change.
pub fn rebuild_dock(mut dock: ResMut<XpDock>, ui: Option<Res<UiAssets>>, root: Single<Entity, With<DockRoot>>, mut commands: Commands, touch: Res<TouchControls>, mut last_touch: Local<bool>) {
    let Some(ui) = ui else { return };
    if *last_touch != touch.visible {
        *last_touch = touch.visible;
        dock.dirty = true;
    }
    if !dock.dirty {
        return;
    }
    // 44 pt is the smallest comfortable tap target; keyboards keep the slimmer chips.
    let (height, font) = if touch.visible { (44.0, 15.0) } else { (34.0, 14.0) };
    dock.dirty = false;
    commands.entity(*root).despawn_related::<Children>();
    if dock.items.is_empty() {
        return;
    }
    let items = dock.items.clone();
    commands.entity(*root).with_children(|r| {
        let mut bar_cmd = theme::panel(
            r,
            &ui,
            Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)), column_gap: Val::Px(14.0), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, max_width: Val::Percent(96.0), ..default() },
        );
        bar_cmd.entry::<Node>().and_modify(|mut n| n.flex_direction = FlexDirection::Row);
        bar_cmd.with_children(|bar| {
            let mut group = None;
            let mut seg: Option<Entity> = None;
            for item in &items {
                if group != Some(item.group) || seg.is_none() {
                    group = Some(item.group);
                    seg = Some(bar.spawn(Node { column_gap: Val::Px(4.0), align_items: AlignItems::Center, ..default() }).id());
                }
                bar.commands().entity(seg.unwrap()).with_children(|s| {
                    let mut b = theme::button(s, &ui, ButtonSpec::new(ButtonKind::Chip, Val::Auto, Val::Px(height), font), &item.label);
                    b.insert(DockButton(item.key));
                    b.entry::<Node>().and_modify(|mut n| n.padding = UiRect::horizontal(Val::Px(10.0)));
                    if item.selected {
                        b.insert(Selected);
                    }
                });
            }
        });
    });
}

pub fn dock_clicks(q: Query<(&Interaction, &DockButton), Changed<Interaction>>, mut clicks: ResMut<DockClicks>) {
    for (i, b) in &q {
        if *i == Interaction::Pressed {
            clicks.0.push(b.0);
        }
    }
}

/// Title card: "NN  Title\npitch\n\n[key] action..." -> themed panel.
pub fn rebuild_card(xp: Res<XpUi>, ui: Option<Res<UiAssets>>, root: Single<Entity, With<CardRoot>>, mut commands: Commands, mut last: Local<(String, bool)>, window: Single<&Window, With<PrimaryWindow>>) {
    let Some(ui) = ui else { return };
    let compact = is_compact(&window);
    if last.0 == xp.card && last.1 == compact {
        return;
    }
    *last = (xp.card.clone(), compact);
    commands.entity(*root).despawn_related::<Children>();
    if xp.card.is_empty() {
        return;
    }
    let mut lines = xp.card.lines();
    let head = lines.next().unwrap_or("");
    let (num, title) = head.split_once("  ").unwrap_or(("", head));
    let pitch = lines.next().unwrap_or("").to_string();
    let hints: Vec<(String, String)> = lines.filter_map(|l| l.strip_prefix('[')?.split_once("] ").map(|(k, a)| (k.to_string(), a.to_string()))).collect();
    let (num, title) = (num.to_string(), title.to_string());
    if compact {
        // Phone landscape: name, one short line and tight hint rows, as small as legibility allows.
        commands.entity(*root).with_children(|r| {
            theme::panel(r, &ui, Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), row_gap: Val::Px(2.0), ..default() }).with_children(|c| {
                c.spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Baseline, ..default() }).with_children(|row| {
                    theme::label(row, &ui, num.clone(), 11.0, theme::GOLD);
                    theme::label(row, &ui, title.clone(), 17.0, theme::GOLD);
                });
                if !pitch.is_empty() {
                    theme::label(c, &ui, pitch.clone(), 11.0, theme::CREAM);
                }
                for (k, a) in hints {
                    c.spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                        row.spawn((Node { padding: UiRect::axes(Val::Px(5.0), Val::Px(0.0)), min_width: Val::Px(28.0), justify_content: JustifyContent::Center, ..default() }, BackgroundColor(theme::NAVY), BorderRadius::all(Val::Px(4.0))))
                            .with_children(|kb| {
                                kb.spawn((Text::new(theme::clean(k)), theme::font(&ui.body, 10.0), TextColor(theme::GOLD)));
                            });
                        theme::label(row, &ui, a, 10.0, Color::WHITE);
                    });
                }
            });
        });
        return;
    }
    commands.entity(*root).with_children(|r| {
        theme::panel(r, &ui, Node { padding: UiRect::all(Val::Px(12.0)), row_gap: Val::Px(6.0), ..default() }).with_children(|c| {
            theme::kicker(c, &ui, &format!("EXPERIMENT {num}"));
            theme::outlined(c, &ui, &title, 26.0, theme::GOLD);
            theme::label(c, &ui, pitch, 13.0, theme::CREAM);
            for (k, a) in hints {
                c.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    row.spawn((Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)), min_width: Val::Px(34.0), justify_content: JustifyContent::Center, ..default() }, BackgroundColor(theme::NAVY), BorderRadius::all(Val::Px(5.0))))
                        .with_children(|kb| {
                            kb.spawn((Text::new(theme::clean(k)), theme::font(&ui.body, 12.0), TextColor(theme::GOLD)));
                        });
                    theme::label(row, &ui, a, 12.0, Color::WHITE);
                });
            }
        });
    });
}

#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);

/// Experiment menu: one green tile per experiment.
pub fn rebuild_menu(open: Res<MenuOpen>, ui: Option<Res<UiAssets>>, root: Single<(Entity, &mut Visibility), With<MenuRoot>>, mut commands: Commands, mut built: Local<bool>) {
    let Some(ui) = ui else { return };
    let (entity, mut vis) = root.into_inner();
    *vis = if open.0 { Visibility::Inherited } else { Visibility::Hidden };
    if !open.0 {
        *built = false;
        return;
    }
    if *built {
        return;
    }
    *built = true;
    commands.entity(entity).despawn_related::<Children>();
    commands.entity(entity).with_children(|r| {
        theme::panel(r, &ui, Node { padding: UiRect::all(Val::Px(18.0)), row_gap: Val::Px(8.0), min_width: Val::Px(340.0), ..default() }).with_children(|c| {
            theme::kicker(c, &ui, "EXPERIMENTS  (X to close)");
            for (i, id) in EXPERIMENT_IDS.iter().enumerate() {
                let kind = if id.available() { ButtonKind::Green } else { ButtonKind::Chip };
                theme::button(c, &ui, ButtonSpec::new(kind, Val::Percent(100.0), Val::Px(40.0), 18.0), &format!("{}  {}", i + 1, id.title())).insert(MenuButton(*id));
            }
        });
    });
}

pub fn menu_clicks(q: Query<(&Interaction, &MenuButton), Changed<Interaction>>, mut slot: ResMut<ExperimentSlot>, mut open: ResMut<MenuOpen>) {
    for (i, b) in &q {
        if *i == Interaction::Pressed && b.0.available() {
            slot.pending = Some(b.0);
            open.0 = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::touch::{layout, pause_rect};

    #[test]
    fn touch_dock_zone_is_clear_of_stick_buttons_and_pause() {
        let sizes = [
            (Vec2::new(844.0, 390.0), SafeArea { left: 47.0, right: 47.0, bottom: 21.0, top: 0.0 }),
            (Vec2::new(874.0, 402.0), SafeArea { left: 52.0, right: 52.0, bottom: 21.0, top: 0.0 }),
            (Vec2::new(667.0, 375.0), SafeArea::default()),
        ];
        for (size, area) in sizes {
            let (left, right, bottom) = touch_dock_insets(&area);
            // Tallest the dock gets: 44 px chips + panel padding and border.
            let zone = Rect::new(left, size.y - bottom - 44.0 - 16.0 - 8.0, size.x - right, size.y - bottom);
            assert!(zone.width() >= 200.0, "{size:?}: room for Get out + Reset, got {}", zone.width());
            for (c, r) in layout(size, Vec3::new(area.left, area.right, area.bottom)) {
                assert!(zone.intersect(r).is_empty(), "{size:?}: dock overlaps {c:?}");
            }
            assert!(zone.intersect(pause_rect(size, area.right, area.top)).is_empty(), "{size:?}: dock overlaps pause");
            assert!(zone.max.y <= size.y - area.bottom, "{size:?}: above the home indicator");
        }
    }
}
