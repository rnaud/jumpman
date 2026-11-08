//! Port of `TouchControls.ts` (+ the touch parts of `Input.ts` / `theme.css`): a virtual stick,
//! L / Z / B / A buttons and a camera-drag swipe, built on `bevy_ui` and Bevy's `Touches`.
//!
//! The controls appear once a touch has been seen (touchscreens, iOS/Android builds) or when
//! `JUMPMAN_TOUCH=1`, and only while playing. Each button is an [`Action`], so it presses exactly
//! what the keyboard/pad binding for that action would (rebinding changes keys, not these).
//! `JUMPMAN_TOUCH_SIM=1` replays synthetic touches (stick up-right, A held, a camera swipe) so the
//! layout can be screenshotted headlessly together with `JUMPMAN_SCREENSHOT`.
use crate::bindings::Action;
use crate::input::{read_input, PlayerInput};
use crate::safe_area::SafeArea;
use crate::shell::{AppState, StateCmd};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use jumpman_sim::state::{A_BUTTON, B_BUTTON, Z_TRIG};

pub struct TouchPlugin;

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        let simulate = std::env::var_os("JUMPMAN_TOUCH_SIM").is_some();
        let forced = simulate || std::env::var_os("JUMPMAN_TOUCH").is_some();
        app.insert_resource(TouchControls { visible: forced, simulate, ..default() })
            .init_resource::<ModeButtons>()
            .init_resource::<ButtonLabels>()
            .add_systems(Startup, spawn_ui)
            .add_systems(PreUpdate, drive_touch.after(read_input))
            .add_systems(Update, (update_ui, update_mode_ui, update_button_labels));
    }
}

// Layout in logical px, from `theme.css` (`@media (any-pointer: coarse)`).
pub const PAD_SIZE: f32 = 108.0;
pub const KNOB_SIZE: f32 = 40.0;
pub const BUTTON_SIZE: f32 = 48.0;
pub const BUTTON_GAP: f32 = 6.0;
pub const PAUSE_SIZE: f32 = 40.0;
/// Below the HUD's course-name label.
pub const PAUSE_TOP: f32 = 28.0;
// iPhones in landscape have a notch / Dynamic Island on one side and a home indicator at the
// bottom; Bevy 0.16 exposes no safe-area query, so keep the controls clear with fixed insets.
#[cfg(target_os = "ios")]
pub const MARGIN_X: f32 = 52.0;
#[cfg(target_os = "ios")]
pub const MARGIN_Y: f32 = 26.0;
#[cfg(not(target_os = "ios"))]
pub const MARGIN_X: f32 = 14.0;
#[cfg(not(target_os = "ios"))]
pub const MARGIN_Y: f32 = 12.0;

/// A control a touch can land on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    Stick,
    Button(Action),
    /// Extra round button of the running experiment (Last Star Standing's item slots).
    Mode(u8),
    /// A mode-specific on-screen button (Relic Descent's skills, dodge, attack...), see [`extra_layout`].
    Extra(u8),
    /// Banjo-Kazooie's extra buttons (egg, trot, wonderwing, power...), see `banjo_ctl::layout`.
    Banjo(u8),
}

impl Control {
    fn mask(self) -> u32 {
        match self {
            Control::Button(Action::Jump) => A_BUTTON,
            Control::Button(Action::Punch) => B_BUTTON,
            Control::Button(Action::Crouch) => Z_TRIG,
            _ => 0,
        }
    }
}

/// Button grid, row-major, as in `.touch-actions` (2 columns): L Z / B A.
pub const BUTTON_GRID: [(Action, &str); 4] = [(Action::Focus, "L"), (Action::Crouch, "Z"), (Action::Punch, "B"), (Action::Jump, "A")];

/// Rect of every control for a window of `size` logical px; `inset` = safe area (left, right, bottom).
pub fn layout(size: Vec2, inset: Vec3) -> Vec<(Control, Rect)> {
    let left = MARGIN_X.max(inset.x);
    let right = MARGIN_X.max(inset.y);
    let bottom = MARGIN_Y.max(inset.z);
    let mut out = vec![(Control::Stick, Rect::from_corners(Vec2::new(left, size.y - bottom - PAD_SIZE), Vec2::new(left + PAD_SIZE, size.y - bottom)))];
    let grid = 2.0 * BUTTON_SIZE + BUTTON_GAP;
    for (i, (action, _)) in BUTTON_GRID.iter().enumerate() {
        let (col, row) = ((i % 2) as f32, (i / 2) as f32);
        let min = Vec2::new(size.x - right - grid + col * (BUTTON_SIZE + BUTTON_GAP), size.y - bottom - grid + row * (BUTTON_SIZE + BUTTON_GAP));
        out.push((Control::Button(*action), Rect::from_corners(min, min + Vec2::splat(BUTTON_SIZE))));
    }
    out
}

/// Size of the experiment slot buttons (a little smaller than the action buttons).
pub const MODE_SIZE: f32 = 44.0;

/// The three extra buttons of a mode, in a row above the A/B grid (clear of the pause button).
pub fn mode_layout(size: Vec2, inset: Vec3) -> Vec<(Control, Rect)> {
    let right = MARGIN_X.max(inset.y);
    let bottom = MARGIN_Y.max(inset.z);
    let grid = 2.0 * BUTTON_SIZE + BUTTON_GAP;
    let y = size.y - bottom - grid - BUTTON_GAP - MODE_SIZE;
    (0..3u8)
        .map(|i| {
            let x = size.x - right - 3.0 * MODE_SIZE - 2.0 * BUTTON_GAP + i as f32 * (MODE_SIZE + BUTTON_GAP);
            (Control::Mode(i), Rect::from_corners(Vec2::new(x, y), Vec2::new(x + MODE_SIZE, y + MODE_SIZE)))
        })
        .collect()
}

/// Labels and tint for an experiment's extra buttons; the experiment's view fills them and
/// sets `active`, and drains [`TouchControls::mode_pressed`].
#[derive(Resource, Default)]
pub struct ModeButtons {
    pub active: bool,
    pub labels: [String; 3],
    pub tint: [Option<Color>; 3],
}

/// Experiments rename the round action buttons while they are running (the kart shows A as HOP / DRIFT).
/// `None` keeps the default letter. Cleared when the experiment exits.
#[derive(Resource, Default, Clone, PartialEq)]
pub struct ButtonLabels {
    pub jump: Option<String>,
    pub punch: Option<String>,
    pub crouch: Option<String>,
    /// Replaces the A button's fill (drift / mini-turbo charge).
    pub jump_tint: Option<Color>,
}

impl ButtonLabels {
    fn text(&self, action: Action) -> Option<&str> {
        match action {
            Action::Jump => self.jump.as_deref(),
            Action::Punch => self.punch.as_deref(),
            Action::Crouch => self.crouch.as_deref(),
            _ => None,
        }
    }
}

/// Fill of an action button when no experiment tints it.
fn button_fill(action: Action) -> Color {
    match action {
        Action::Jump => Color::srgba(0.65, 0.18, 0.14, 0.85),
        Action::Punch => Color::srgba(0.13, 0.37, 0.58, 0.85),
        _ => Color::srgba(0.09, 0.14, 0.23, 0.74),
    }
}

/// Number of Relic Descent's buttons and their labels (`TouchControls::extra_active` shows them).
pub const EXTRA_LABELS: [&str; 9] = ["SLAM", "WHIRL", "STARS", "AEGIS", "DODGE", "ATK", "POT", "BAG", "TREE"];

/// Layout of Relic Descent's buttons: a cluster of attack / dodge / four skills to the left of the
/// A/B grid, and potion / bag / skill-tree in a column above it. Same insets as [`layout`].
pub fn extra_layout(size: Vec2, inset: Vec3) -> Vec<(u8, Rect)> {
    let right = MARGIN_X.max(inset.y);
    let bottom = MARGIN_Y.max(inset.z);
    let grid = 2.0 * BUTTON_SIZE + BUTTON_GAP;
    let grid_left = size.x - right - grid;
    let grid_top = size.y - bottom - grid;
    let rect = |right_edge: f32, bottom_edge: f32, s: f32| Rect::from_corners(Vec2::new(right_edge - s, bottom_edge - s), Vec2::new(right_edge, bottom_edge));
    let mut out = vec![];
    // Bottom row: ATK (largest, nearest the thumb), DODGE.
    let atk = rect(grid_left - 12.0, size.y - bottom, 60.0);
    let dodge = rect(atk.min.x - 8.0, size.y - bottom, 50.0);
    // Skill row above them; SLAM WHIRL STARS AEGIS read left to right.
    let row_bottom = size.y - bottom - 66.0;
    let mut x = grid_left - 12.0;
    let mut skills = [Rect::default(); 4];
    for i in (0..4).rev() {
        skills[i] = rect(x, row_bottom, 44.0);
        x -= 50.0;
    }
    for (i, r) in skills.iter().enumerate() {
        out.push((i as u8, *r));
    }
    out.push((4, dodge));
    out.push((5, atk));
    // Column above the grid: POT, BAG, TREE (top).
    let col_right = size.x - right;
    let pot = rect(col_right, grid_top - 10.0, 44.0);
    let bag = rect(col_right, pot.min.y - 6.0, 44.0);
    let tree = rect(col_right, bag.min.y - 6.0, 44.0);
    out.push((6, pot));
    out.push((7, bag));
    out.push((8, tree));
    out
}

/// Pause button, top-right (the phone has no Esc key): a tap opens the pause menu.
pub fn pause_rect(size: Vec2, right_inset: f32, top_inset: f32) -> Rect {
    let min = Vec2::new(size.x - MARGIN_X.max(right_inset) - PAUSE_SIZE, MARGIN_Y.max(top_inset) + PAUSE_TOP);
    Rect::from_corners(min, min + Vec2::splat(PAUSE_SIZE))
}

/// `TouchControls.setStick`: offset from the pad centre (x right, y up, in pad radii) clamped to the
/// unit circle and scaled to the 64-unit N64 stick. Returns (x, y, mag).
pub fn stick_from_offset(x: f32, y: f32) -> (f32, f32, f32) {
    let length = x.hypot(y).max(1.0);
    let (x, y) = (x / length, y / length);
    (x * 64.0, y * 64.0, x.hypot(y) * 64.0)
}

#[derive(Clone, Copy, Debug)]
struct Owner {
    id: u64,
    control: Option<Control>, // None = camera drag
    last: Vec2,
}

#[derive(Resource, Default)]
pub struct TouchControls {
    pub visible: bool,
    pub simulate: bool,
    pub stick: Vec2,
    pub stick_mag: f32,
    pub buttons: u32,
    pub focus: bool,
    pub pad_center: Vec2,
    owners: Vec<Owner>,
    /// Button edges latched since the last frame.
    pressed: u32,
    /// Experiment button edges (bit per [`Control::Mode`]) latched since they were last drained.
    pub mode_pressed: u8,
    held: Vec<Control>,
    /// Show and hit-test Relic Descent's buttons ([`extra_layout`]).
    pub extra_active: bool,
    /// Swallow all game touches (a full-screen menu is open).
    pub suppress: bool,
    /// Hide the stick and buttons (Star Deck draws its own UI; the pause button stays).
    pub hide_pad: bool,
    /// Relic buttons currently held / pressed since the last read (bit per id).
    pub extra_down: u16,
    pub extra_pressed: u16,
    /// Banjo buttons shown / held / pressed since the last read (bit per id, see `banjo_ctl`).
    pub banjo_active: bool,
    pub banjo_down: u8,
    pub banjo_pressed: u8,
    /// 0..1 cooldown shade shown on each Relic button.
    pub extra_fill: [f32; 9],
}

impl TouchControls {
    pub fn is_held(&self, c: Control) -> bool {
        self.held.contains(&c)
    }

    fn has_owner(&self, id: u64) -> bool {
        self.owners.iter().any(|o| o.id == id)
    }

    fn set_stick(&mut self, off: Vec2) {
        let (x, y, m) = stick_from_offset(off.x, off.y);
        self.stick = Vec2::new(x, y);
        self.stick_mag = m;
    }

    /// Finger down at `pos`: grab the control under it, or start a camera drag.
    pub fn begin(&mut self, id: u64, pos: Vec2, rects: &[(Control, Rect)]) {
        if self.has_owner(id) {
            return;
        }
        let hit = rects.iter().find(|(c, r)| r.contains(pos) && !self.held.contains(c)).map(|(c, r)| (*c, *r));
        match hit {
            Some((c, r)) => {
                self.held.push(c);
                if let Control::Mode(i) = c {
                    self.mode_pressed |= 1 << i;
                }
                if let Control::Extra(i) = c {
                    self.extra_down |= 1 << i;
                    self.extra_pressed |= 1 << i;
                }
                if let Control::Banjo(i) = c {
                    self.banjo_down |= 1 << i;
                    self.banjo_pressed |= 1 << i;
                }
                self.buttons |= c.mask();
                self.pressed |= c.mask();
                if c == Control::Button(Action::Focus) {
                    self.focus = true;
                }
                if c == Control::Stick {
                    self.pad_center = r.center();
                    self.move_to(pos);
                }
                self.owners.push(Owner { id, control: Some(c), last: pos });
            }
            None => self.owners.push(Owner { id, control: None, last: pos }),
        }
    }

    fn move_to(&mut self, pos: Vec2) {
        let radius = PAD_SIZE / 2.0;
        let d = pos - self.pad_center;
        self.set_stick(Vec2::new(d.x / radius, -d.y / radius));
    }

    /// Finger moved; returns the camera drag delta for swipe touches.
    pub fn moved(&mut self, id: u64, pos: Vec2) -> Vec2 {
        let Some(i) = self.owners.iter().position(|o| o.id == id) else { return Vec2::ZERO };
        let o = self.owners[i];
        self.owners[i].last = pos;
        match o.control {
            Some(Control::Stick) => {
                self.move_to(pos);
                Vec2::ZERO
            }
            Some(_) => Vec2::ZERO,
            None => pos - o.last,
        }
    }

    pub fn end(&mut self, id: u64) {
        let Some(i) = self.owners.iter().position(|o| o.id == id) else { return };
        let o = self.owners.remove(i);
        if let Some(c) = o.control {
            self.held.retain(|h| *h != c);
            self.buttons &= !c.mask();
            if let Control::Extra(i) = c {
                self.extra_down &= !(1 << i);
            }
            if let Control::Banjo(i) = c {
                self.banjo_down &= !(1 << i);
            }
            if c == Control::Button(Action::Focus) {
                self.focus = false;
            }
            if c == Control::Stick {
                self.set_stick(Vec2::ZERO);
            }
        }
    }

    /// `clear()`: menus swallow input.
    pub fn clear(&mut self) {
        self.owners.clear();
        self.held.clear();
        self.buttons = 0;
        self.pressed = 0;
        self.extra_down = 0;
        self.extra_pressed = 0;
        self.banjo_down = 0;
        self.banjo_pressed = 0;
        self.focus = false;
        self.set_stick(Vec2::ZERO);
    }
}

pub(crate) fn drive_touch(
    touches: Res<Touches>,
    state: Option<Res<AppState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
    mut tc: ResMut<TouchControls>,
    mut input: ResMut<PlayerInput>,
    mut commands: EventWriter<StateCmd>,
    area: Res<SafeArea>,
    mode: Res<ModeButtons>,
) {
    let Ok(window) = windows.single() else { return };
    if state.is_some_and(|s| *s != AppState::Playing) {
        if !tc.owners.is_empty() {
            tc.clear();
        }
        return;
    }
    let mut rects = layout(window.size(), Vec3::new(area.left, area.right, area.bottom));
    if mode.active {
        rects.extend(mode_layout(window.size(), Vec3::new(area.left, area.right, area.bottom)));
    }
    if tc.extra_active {
        rects.extend(extra_layout(window.size(), Vec3::new(area.left, area.right, area.bottom)).into_iter().map(|(i, r)| (Control::Extra(i), r)));
    }
    if tc.banjo_active {
        rects.extend(crate::banjo_ctl::layout(window.size(), Vec3::new(area.left, area.right, area.bottom)).into_iter().map(|(i, r)| (Control::Banjo(i), r)));
    }
    if tc.suppress && !tc.owners.is_empty() {
        tc.clear();
    }
    let mut swipe = Vec2::ZERO;
    for t in touches.iter_just_pressed() {
        tc.visible = true;
        if pause_rect(window.size(), area.right, area.top).contains(t.position()) {
            commands.write(StateCmd::TogglePause);
            continue;
        }
        if tc.suppress {
            continue;
        }
        tc.begin(t.id(), t.position(), &rects);
    }
    for t in touches.iter() {
        swipe += tc.moved(t.id(), t.position());
    }
    for t in touches.iter_just_released().chain(touches.iter_just_canceled()) {
        tc.end(t.id());
    }
    if tc.simulate {
        swipe += simulate(&mut tc, &rects, time.elapsed_secs());
    }
    input.merge_touch(&mut tc, swipe);
}

/// Synthetic touches: stick pushed up-right and a camera swipe from 1 s, A pressed and held from 1.5 s.
fn simulate(tc: &mut TouchControls, rects: &[(Control, Rect)], t: f32) -> Vec2 {
    const STICK: u64 = 9001;
    const BUTTON: u64 = 9002;
    const CAMERA: u64 = 9003;
    let centre = |c: Control| rects.iter().find(|(k, _)| *k == c).map(|(_, r)| r.center()).unwrap_or_default();
    if t <= 1.0 {
        return Vec2::ZERO;
    }
    if !tc.has_owner(STICK) {
        tc.begin(STICK, centre(Control::Stick), rects);
    }
    tc.moved(STICK, centre(Control::Stick) + Vec2::new(30.0, -40.0));
    if t > 1.5 && !tc.has_owner(BUTTON) {
        tc.begin(BUTTON, centre(Control::Button(Action::Jump)), rects);
    }
    if !tc.has_owner(CAMERA) {
        tc.begin(CAMERA, Vec2::new(400.0, 200.0), rects);
    }
    if t < 2.0 {
        tc.moved(CAMERA, Vec2::new(400.0 + (t - 1.0) * 60.0, 200.0))
    } else {
        Vec2::ZERO
    }
}

impl PlayerInput {
    /// `consumeStep` touch merge: touch stick wins when deflected, buttons OR in, edges latch.
    pub fn merge_touch(&mut self, tc: &mut TouchControls, swipe: Vec2) {
        if tc.stick_mag > 0.0 {
            self.stick = tc.stick;
            self.stick_mag = tc.stick_mag;
        }
        self.down |= tc.buttons;
        let p = std::mem::take(&mut tc.pressed);
        self.latch(p);
        self.focus_held |= tc.focus;
        self.add_camera_drag(swipe.x, swipe.y);
    }
}

#[derive(Component)]
struct TouchRoot;
#[derive(Component)]
struct TouchPart(Control);
#[derive(Component)]
struct Knob;
#[derive(Component)]
struct ModeLabel(u8);
#[derive(Component)]
struct PausePart;
#[derive(Component)]
struct ExtraPart(u8);
#[derive(Component)]
struct ExtraFill(u8);
#[derive(Component)]
struct ButtonLabel(Action, &'static str);

fn spawn_ui(mut commands: Commands) {
    let fill = Color::srgba(0.09, 0.14, 0.23, 0.74);
    let rim = Color::srgba(1.0, 1.0, 1.0, 0.6);
    let root = commands.spawn((TouchRoot, Visibility::Hidden, GlobalZIndex(40), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() })).id();
    commands.entity(root).with_children(|p| {
        p.spawn((TouchPart(Control::Stick), Node { position_type: PositionType::Absolute, width: Val::Px(PAD_SIZE), height: Val::Px(PAD_SIZE), left: Val::Px(MARGIN_X), bottom: Val::Px(MARGIN_Y), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(fill), BorderColor(rim), BorderRadius::MAX))
            .with_children(|k| {
                k.spawn((Knob, Node { width: Val::Px(KNOB_SIZE), height: Val::Px(KNOB_SIZE), ..default() }, BackgroundColor(Color::srgba(1.0, 0.88, 0.61, 0.85)), BorderRadius::MAX));
            });
        p.spawn((PausePart, Node { position_type: PositionType::Absolute, width: Val::Px(PAUSE_SIZE), height: Val::Px(PAUSE_SIZE), right: Val::Px(MARGIN_X), top: Val::Px(MARGIN_Y + PAUSE_TOP), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(fill), BorderColor(rim), BorderRadius::MAX))
            .with_child((Text::new("II"), TextFont { font_size: 18.0, ..default() }, TextColor(Color::WHITE)));
        for i in 0..3u8 {
            p.spawn((TouchPart(Control::Mode(i)), Node { display: Display::None, position_type: PositionType::Absolute, width: Val::Px(MODE_SIZE), height: Val::Px(MODE_SIZE), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(fill), BorderColor(rim), BorderRadius::MAX))
                .with_child((ModeLabel(i), Text::new(""), TextFont { font_size: 11.0, ..default() }, TextColor(Color::WHITE), TextLayout::new_with_justify(JustifyText::Center)));
        }
        for (i, label) in EXTRA_LABELS.iter().enumerate() {
            let bg = match i {
                0..=3 => Color::srgba(0.30, 0.16, 0.50, 0.85),
                4 => Color::srgba(0.10, 0.45, 0.35, 0.85),
                5 => Color::srgba(0.70, 0.30, 0.10, 0.88),
                6 => Color::srgba(0.60, 0.12, 0.20, 0.85),
                _ => fill,
            };
            p.spawn((ExtraPart(i as u8), Visibility::Hidden, Node { position_type: PositionType::Absolute, width: Val::Px(44.0), height: Val::Px(44.0), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }, BackgroundColor(bg), BorderColor(rim), BorderRadius::MAX))
                .with_children(|b| {
                    // Cooldown shade rises from the bottom while the skill recharges.
                    b.spawn((ExtraFill(i as u8), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), height: Val::Percent(0.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62))));
                    b.spawn((Text::new(*label), TextFont { font_size: if i == 5 { 15.0 } else { 11.0 }, ..default() }, TextColor(Color::WHITE)));
                });
        }
        for (i, (action, label)) in BUTTON_GRID.iter().enumerate() {
            let (col, row) = ((i % 2) as f32, (i / 2) as f32);
            let bg = button_fill(*action);
            p.spawn((TouchPart(Control::Button(*action)), Node { position_type: PositionType::Absolute, width: Val::Px(BUTTON_SIZE), height: Val::Px(BUTTON_SIZE), right: Val::Px(MARGIN_X + (1.0 - col) * (BUTTON_SIZE + BUTTON_GAP)), bottom: Val::Px(MARGIN_Y + (1.0 - row) * (BUTTON_SIZE + BUTTON_GAP)), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(bg), BorderColor(rim), BorderRadius::MAX))
                .with_child((ButtonLabel(*action, label), Text::new(*label), TextFont { font_size: 20.0, ..default() }, TextColor(Color::WHITE), TextLayout::new_with_justify(JustifyText::Center)));
        }
    });
}

/// Rename / tint the A, B and Z buttons for the running experiment; long names shrink to fit the circle.
fn update_button_labels(labels: Res<ButtonLabels>, mut texts: Query<(&ButtonLabel, &mut Text, &mut TextFont)>, mut parts: Query<(&TouchPart, &mut BackgroundColor)>) {
    if !labels.is_changed() {
        return;
    }
    for (label, mut text, mut font) in &mut texts {
        let (want, size) = match labels.text(label.0) {
            Some(t) => (t, if t.len() > 3 { 11.0 } else { 16.0 }),
            None => (label.1, 20.0),
        };
        if text.0 != want {
            text.0 = want.to_string();
        }
        if font.font_size != size {
            font.font_size = size;
        }
    }
    for (part, mut bg) in &mut parts {
        if let Control::Button(action) = part.0 {
            let want = if action == Action::Jump { labels.jump_tint.unwrap_or_else(|| button_fill(action)) } else { button_fill(action) };
            if bg.0 != want {
                bg.0 = want;
            }
        }
    }
}

/// Colour and text of an experiment's extra buttons.
fn update_mode_ui(mode: Res<ModeButtons>, mut parts: Query<(&TouchPart, &mut BackgroundColor)>, mut labels: Query<(&ModeLabel, &mut Text)>) {
    if !mode.active {
        return;
    }
    for (part, mut bg) in &mut parts {
        if let Control::Mode(i) = part.0 {
            if let Some(c) = mode.tint[i as usize] {
                if bg.0 != c {
                    bg.0 = c;
                }
            }
        }
    }
    for (l, mut t) in &mut labels {
        if t.0 != mode.labels[l.0 as usize] {
            t.0 = mode.labels[l.0 as usize].clone();
        }
    }
}

fn update_ui(
    tc: Res<TouchControls>,
    state: Option<Res<AppState>>,
    mut root: Query<&mut Visibility, With<TouchRoot>>,
    mut parts: Query<(&TouchPart, &mut BorderColor, &mut Node), (Without<Knob>, Without<PausePart>, Without<ExtraPart>, Without<ExtraFill>)>,
    mut knob: Query<&mut Node, (With<Knob>, Without<TouchPart>, Without<PausePart>, Without<ExtraPart>, Without<ExtraFill>)>,
    mut pause: Query<&mut Node, (With<PausePart>, Without<TouchPart>, Without<Knob>, Without<ExtraPart>, Without<ExtraFill>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<SafeArea>,
    mode: Res<ModeButtons>,
    mut extras: Query<(&ExtraPart, &mut Visibility, &mut Node, &mut BorderColor), (Without<TouchPart>, Without<Knob>, Without<PausePart>, Without<TouchRoot>)>,
    mut fills: Query<(&ExtraFill, &mut Node), (Without<ExtraPart>, Without<TouchPart>, Without<Knob>, Without<PausePart>)>,
) {
    let show = tc.visible && state.is_none_or(|s| *s == AppState::Playing);
    for mut v in &mut root {
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
    }
    let extra_rects = windows.single().ok().map(|w| extra_layout(w.size(), Vec3::new(area.left, area.right, area.bottom)));
    for (part, mut vis, mut node, mut border) in &mut extras {
        let want = if tc.extra_active && !tc.suppress { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let Some(rects) = &extra_rects {
            if let Some((_, r)) = rects.iter().find(|(i, _)| *i == part.0) {
                node.left = Val::Px(r.min.x);
                node.top = Val::Px(r.min.y);
                node.width = Val::Px(r.width());
                node.height = Val::Px(r.height());
            }
        }
        border.0 = if tc.extra_down & (1 << part.0) != 0 { Color::srgb(1.0, 0.82, 0.29) } else { Color::srgba(1.0, 1.0, 1.0, 0.6) };
    }
    for (f, mut node) in &mut fills {
        let h = Val::Percent(tc.extra_fill[f.0 as usize].clamp(0.0, 1.0) * 100.0);
        if node.height != h {
            node.height = h;
        }
    }
    // Place every control from the same layout the hit-testing uses (honours the safe area).
    let layout_rects = windows.single().ok().map(|w| {
        let inset = Vec3::new(area.left, area.right, area.bottom);
        let mut rects = layout(w.size(), inset);
        rects.extend(mode_layout(w.size(), inset));
        (rects, pause_rect(w.size(), area.right, area.top))
    });
    for (part, mut border, mut node) in &mut parts {
        if let Control::Mode(i) = part.0 {
            let d = if mode.active && mode.tint[i as usize].is_some() && !tc.hide_pad { Display::Flex } else { Display::None };
            if node.display != d {
                node.display = d;
            }
        } else {
            let d = if tc.hide_pad { Display::None } else { Display::Flex };
            if node.display != d {
                node.display = d;
            }
        }
        border.0 = if tc.is_held(part.0) { Color::srgb(1.0, 0.82, 0.29) } else { Color::srgba(1.0, 1.0, 1.0, 0.6) };
        if let Some((rects, _)) = &layout_rects {
            if let Some((_, r)) = rects.iter().find(|(c, _)| *c == part.0) {
                node.left = Val::Px(r.min.x);
                node.top = Val::Px(r.min.y);
                node.right = Val::Auto;
                node.bottom = Val::Auto;
            }
        }
    }
    if let Some((_, pr)) = &layout_rects {
        for mut node in &mut pause {
            node.left = Val::Px(pr.min.x);
            node.top = Val::Px(pr.min.y);
            node.right = Val::Auto;
            node.bottom = Val::Auto;
        }
    }
    // Knob follows the stick by up to 32 px (`--stick-x/-y`).
    for mut n in &mut knob {
        n.left = Val::Px(tc.stick.x / 64.0 * 32.0);
        n.top = Val::Px(-tc.stick.y / 64.0 * 32.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values computed with the TS `setStick` (node):
    //   (2,0)->(64,0,64)  (0,-3)->(0,-64,64)  (0.5,0.5)->(32,32,45.2548)  (0,0)->0
    #[test]
    fn stick_math_matches_ts() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert_eq!(stick_from_offset(2.0, 0.0), (64.0, 0.0, 64.0));
        assert_eq!(stick_from_offset(0.0, -3.0), (0.0, -64.0, 64.0));
        let (x, y, m) = stick_from_offset(0.5, 0.5);
        assert!(near(x, 32.0) && near(y, 32.0) && near(m, 45.254833));
        assert_eq!(stick_from_offset(0.0, 0.0), (0.0, 0.0, 0.0));
    }

    #[test]
    fn touches_route_to_controls_and_camera() {
        let size = Vec2::new(844.0, 390.0);
        let rects = layout(size, Vec3::ZERO);
        let mut tc = TouchControls::default();
        tc.begin(1, rects[0].1.center() + Vec2::new(54.0, -54.0), &rects);
        assert!(tc.stick.x > 40.0 && tc.stick.y > 40.0 && (tc.stick_mag - 64.0).abs() < 1e-3);
        let a = rects.iter().find(|(c, _)| *c == Control::Button(Action::Jump)).unwrap().1;
        tc.begin(2, a.center(), &rects);
        assert_eq!(tc.buttons, A_BUTTON);
        tc.begin(3, Vec2::new(400.0, 100.0), &rects);
        assert_eq!(tc.moved(3, Vec2::new(410.0, 95.0)), Vec2::new(10.0, -5.0));
        tc.end(1);
        assert_eq!(tc.stick_mag, 0.0);
        tc.end(2);
        assert_eq!(tc.buttons, 0);
    }

    #[test]
    fn extra_buttons_fit_and_do_not_overlap_the_base_controls() {
        let sizes = [(Vec2::new(844.0, 390.0), Vec3::new(47.0, 47.0, 21.0)), (Vec2::new(874.0, 402.0), Vec3::new(52.0, 52.0, 21.0)), (Vec2::new(1040.0, 680.0), Vec3::ZERO)];
        for (size, inset) in sizes {
            let base = layout(size, inset);
            let extras = extra_layout(size, inset);
            assert_eq!(extras.len(), EXTRA_LABELS.len());
            let pause = pause_rect(size, inset.y, 0.0);
            for (i, (id, r)) in extras.iter().enumerate() {
                assert_eq!(*id as usize, i);
                assert!(r.min.x >= inset.x && r.max.x <= size.x && r.min.y >= 0.0 && r.max.y <= size.y - inset.z, "{size:?} {id}: {r:?}");
                assert!(r.intersect(pause).is_empty(), "{id} overlaps the pause button");
                for (c, b) in &base {
                    assert!(r.intersect(*b).is_empty(), "extra {id} overlaps {c:?}");
                }
                for (j, o) in &extras[i + 1..] {
                    assert!(r.intersect(*o).is_empty(), "extras {id} and {j} overlap");
                }
            }
        }
    }

    #[test]
    fn extra_touches_latch_and_release() {
        let size = Vec2::new(874.0, 402.0);
        let mut rects = layout(size, Vec3::ZERO);
        rects.extend(extra_layout(size, Vec3::ZERO).into_iter().map(|(i, r)| (Control::Extra(i), r)));
        let target = rects.iter().find(|(c, _)| *c == Control::Extra(5)).unwrap().1;
        let mut tc = TouchControls::default();
        tc.begin(1, target.center(), &rects);
        assert_eq!(tc.extra_down, 1 << 5);
        assert_eq!(tc.extra_pressed, 1 << 5);
        assert_eq!(tc.buttons, 0, "extras never press the SM64 buttons");
        tc.end(1);
        assert_eq!(tc.extra_down, 0);
        assert_eq!(tc.extra_pressed, 1 << 5, "the edge survives until read");
    }

    #[test]
    fn layout_fits_iphone_landscape() {
        let size = Vec2::new(844.0, 390.0);
        for (_, r) in layout(size, Vec3::new(47.0, 47.0, 21.0)) {
            assert!(r.min.x >= 0.0 && r.max.x <= size.x && r.min.y >= 0.0 && r.max.y <= size.y);
        }
    }
}
