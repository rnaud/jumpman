//! Port of `TouchControls.ts` (+ the touch parts of `Input.ts` / `theme.css`): a virtual stick,
//! L / Z / B / A buttons and a camera-drag swipe, built on `bevy_ui` and Bevy's `Touches`.
//!
//! The controls appear once a touch has been seen (touchscreens, iOS/Android builds) or when
//! `JUMPMAN_TOUCH=1`, and only while playing. Each button is an [`Action`], so it presses exactly
//! what the keyboard/pad binding for that action would (rebinding changes keys, not these).
//! `JUMPMAN_TOUCH_SIM=1` replays synthetic touches (stick up-right, A held, a camera swipe) so the
//! layout can be screenshotted headlessly together with `JUMPMAN_SCREENSHOT`.
use crate::button_marker::BorderRadius;
use crate::shell::retro::hud_kit::{Framed, HudKit, PixelText, Plate, COL_GOLD, COL_WHITE};
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
        crate::shell::retro::hud_kit::ensure(app);
        app.insert_resource(TouchControls { visible: forced, simulate, ..default() })
            .init_resource::<ModeButtons>()
            .init_resource::<ButtonLabels>()
            .add_systems(Startup, spawn_ui)
            .add_systems(PreUpdate, drive_touch.after(read_input))
            .add_systems(PreUpdate, log_raw_touches.run_if(touch_log_on))
            .add_systems(Update, (update_ui, update_mode_ui, update_button_labels));
    }
}

/// `JUMPMAN_TOUCH_LOG=1`: print every raw `TouchInput` (logical px, with the frame and the clock) plus the window size, scale
/// factor and safe area, and which control or layer claimed each finger. Meant for `simctl launch --console` on a simulator or a
/// device: it shows what the game really received, not what the finger meant.
pub fn touch_log_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("JUMPMAN_TOUCH_LOG").is_some())
}

/// One line of the touch log (no-op unless `JUMPMAN_TOUCH_LOG` is set).
pub fn touch_log(what: std::fmt::Arguments) {
    if touch_log_on() {
        eprintln!("TOUCHLOG {what}");
    }
}

fn log_raw_touches(
    mut events: MessageReader<bevy::input::touch::TouchInput>,
    time: Res<Time<Real>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<SafeArea>,
    mut frame: Local<u64>,
    mut last: Local<Option<(Vec2, f32, SafeArea)>>,
) {
    *frame += 1;
    if let Ok(w) = windows.single() {
        let now = (w.size(), w.scale_factor(), *area);
        if *last != Some(now) {
            touch_log(format_args!("window {:?} logical px, scale {}, physical {}x{}, safe area l{} r{} b{} t{}", now.0, now.1, w.physical_width(), w.physical_height(), area.left, area.right, area.bottom, area.top));
            *last = Some(now);
        }
    }
    for e in events.read() {
        touch_log(format_args!("t={:.3} frame={} {:?} id={} pos=({:.1},{:.1})", time.elapsed_secs(), *frame, e.phase, e.id, e.position.x, e.position.y));
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
    /// Cooldown / charge shade rising from the bottom of each button, 0 (none) to 1 (covers it).
    pub fill: [f32; 3],
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
    /// kart_anywhere hook: a second top-bar button (beside pause) while it is shown. A tap inside is latched in
    /// `top_tapped` and, like a tap on pause, starts neither a control nor a camera drag.
    pub top_extra: Option<Rect>,
    pub top_tapped: bool,
}

impl TouchControls {
    pub fn is_held(&self, c: Control) -> bool {
        self.held.contains(&c)
    }

    /// A finger went down at `pos`: true if the extra top-bar button took it (see `top_extra`).
    pub fn claim_top(&mut self, pos: Vec2) -> bool {
        let hit = self.top_extra.is_some_and(|r| r.contains(pos));
        self.top_tapped |= hit;
        hit
    }

    /// No finger is on a control or dragging the camera (test hook).
    #[cfg(test)]
    pub fn owners_empty(&self) -> bool {
        self.owners.is_empty()
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
    mut commands: MessageWriter<StateCmd>,
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
            touch_log(format_args!("finger {} at ({:.1},{:.1}): pause button", t.id(), t.position().x, t.position().y));
            commands.write(StateCmd::TogglePause);
            continue;
        }
        if tc.suppress || tc.claim_top(t.position()) {
            touch_log(format_args!("finger {} at ({:.1},{:.1}): swallowed (menu open or top-bar button)", t.id(), t.position().x, t.position().y));
            continue;
        }
        tc.begin(t.id(), t.position(), &rects);
        if touch_log_on() {
            let owner = tc.owners.iter().find(|o| o.id == t.id()).and_then(|o| o.control);
            touch_log(format_args!("finger {} at ({:.1},{:.1}): {}", t.id(), t.position().x, t.position().y, owner.map_or("camera drag".to_string(), |c| format!("{c:?}"))));
        }
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
struct ModeFill(u8);
#[derive(Component)]
struct ButtonLabel(Action, &'static str);
/// The round analogue stick base (the one control that stays a disc).
#[derive(Component)]
struct StickBase;
/// The plate a button wears while it is not held (the pressed look is always gold).
#[derive(Component, Clone, Copy)]
struct Face(Plate);

/// The slab colour that stands in for a tint an experiment asked for (red / green / blue families, else navy).
pub fn plate_for_tint(c: Color) -> Plate {
    let c = c.to_srgba();
    let (r, g, b) = (c.red, c.green, c.blue);
    let max = r.max(g).max(b);
    if max < 0.25 || max - r.min(g).min(b) < 0.12 {
        Plate::Dark
    } else if r >= g && r >= b {
        if g > 0.6 * r { Plate::Gold } else { Plate::Red }
    } else if g >= b {
        Plate::Green
    } else {
        Plate::Blue
    }
}

/// Plate of an action button when nothing tints it.
fn button_plate(action: Action) -> Plate {
    match action {
        Action::Jump => Plate::Red,
        Action::Punch => Plate::Blue,
        _ => Plate::Dark,
    }
}

/// Label text of a button: one or two characters in big letters (2x on a phone; the plates stay 48 layout px, so from `k` = 3 on
/// the big letters would not fit and drop to 1x), longer words small enough to stay on the plate.
fn button_label(text: &str, width: f32, k: u32) -> PixelText {
    let t = PixelText::new(text, COL_WHITE);
    if text.chars().count() <= 2 {
        t.big(if k <= 2 { 2 } else { 1 })
    } else {
        t.wrapped(5).fit_width(width)
    }
}

fn spawn_ui(mut commands: Commands, kit: HudKit) {
    let fill = Color::srgba(0.07, 0.10, 0.34, 0.62);
    let rim = Color::srgba(0.435, 0.525, 0.878, 0.9);
    let k = kit.k();
    let root = commands.spawn((TouchRoot, Visibility::Hidden, GlobalZIndex(40), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() })).id();
    commands.entity(root).with_children(|p| {
        p.spawn((TouchPart(Control::Stick), StickBase, Node { position_type: PositionType::Absolute, width: Val::Px(PAD_SIZE), height: Val::Px(PAD_SIZE), left: Val::Px(MARGIN_X), bottom: Val::Px(MARGIN_Y), border: UiRect::all(Val::Px(2.0 * k as f32)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(fill), BorderColor::all(rim), BorderRadius::MAX))
            .with_children(|b| {
                b.spawn((Knob, Node { width: Val::Px(KNOB_SIZE), height: Val::Px(KNOB_SIZE), border: UiRect::all(Val::Px(k as f32)), ..default() }, BackgroundColor(Color::srgb(1.0, 0.82, 0.23)), BorderColor::all(Color::srgb_u8(58, 26, 4)), BorderRadius::MAX));
            });
        p.spawn((PausePart, Face(Plate::Dark), Framed(Plate::Dark), Node { position_type: PositionType::Absolute, width: Val::Px(PAUSE_SIZE), height: Val::Px(PAUSE_SIZE), right: Val::Px(MARGIN_X), top: Val::Px(MARGIN_Y + PAUSE_TOP), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }))
            .with_child(kit.text_big("II", COL_GOLD, 1));
        for i in 0..3u8 {
            p.spawn((TouchPart(Control::Mode(i)), Face(Plate::Dark), Framed(Plate::Dark), Node { display: Display::None, position_type: PositionType::Absolute, width: Val::Px(MODE_SIZE), height: Val::Px(MODE_SIZE), justify_content: JustifyContent::Center, align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }))
                .with_children(|b| {
                    b.spawn((ModeFill(i), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), height: Val::Percent(0.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6))));
                    b.spawn((ModeLabel(i), PixelText::new("", COL_WHITE).wrapped(5).fit_width(MODE_SIZE), ImageNode::default(), Node::default()));
                });
        }
        for (i, label) in EXTRA_LABELS.iter().enumerate() {
            let plate = match i {
                0..=3 => Plate::Blue,
                4 => Plate::Green,
                5 | 6 => Plate::Red,
                _ => Plate::Dark,
            };
            p.spawn((ExtraPart(i as u8), Visibility::Hidden, Face(plate), Framed(plate), Node { position_type: PositionType::Absolute, width: Val::Px(44.0), height: Val::Px(44.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }))
                .with_children(|b| {
                    // Cooldown shade rises from the bottom while the skill recharges.
                    b.spawn((ExtraFill(i as u8), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), height: Val::Percent(0.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62))));
                    let t = PixelText::new(*label, COL_WHITE).fit_width(40.0);
                    let size = t.size_px(k);
                    b.spawn((t, ImageNode::default(), Node { width: Val::Px(size.x), height: Val::Px(size.y), ..default() }));
                });
        }
        for (i, (action, label)) in BUTTON_GRID.iter().enumerate() {
            let (col, row) = ((i % 2) as f32, (i / 2) as f32);
            let plate = button_plate(*action);
            let t = button_label(label, BUTTON_SIZE - 4.0, k);
            let size = t.size_px(k);
            p.spawn((TouchPart(Control::Button(*action)), Face(plate), Framed(plate), Node { position_type: PositionType::Absolute, width: Val::Px(BUTTON_SIZE), height: Val::Px(BUTTON_SIZE), right: Val::Px(MARGIN_X + (1.0 - col) * (BUTTON_SIZE + BUTTON_GAP)), bottom: Val::Px(MARGIN_Y + (1.0 - row) * (BUTTON_SIZE + BUTTON_GAP)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }))
                .with_child((ButtonLabel(*action, label), t, ImageNode::default(), Node { width: Val::Px(size.x), height: Val::Px(size.y), ..default() }));
        }
    });
}

/// Rename / tint the A, B and Z buttons for the running experiment; long names shrink to fit the plate.
fn update_button_labels(labels: Res<ButtonLabels>, scale: Res<crate::shell::retro::hud_kit::HudScale>, mut texts: Query<(&ButtonLabel, &mut PixelText)>, mut parts: Query<(&TouchPart, &mut Face)>) {
    if !labels.is_changed() {
        return;
    }
    for (label, mut text) in &mut texts {
        let want = labels.text(label.0).unwrap_or(label.1);
        let t = button_label(want, BUTTON_SIZE - 4.0, scale.0);
        if *text != t {
            *text = t;
        }
    }
    for (part, mut face) in &mut parts {
        if let Control::Button(action) = part.0 {
            let want = if action == Action::Jump { labels.jump_tint.map_or_else(|| button_plate(action), plate_for_tint) } else { button_plate(action) };
            if face.0 != want {
                face.0 = want;
            }
        }
    }
}

/// Colour and text of an experiment's extra buttons.
fn update_mode_ui(mode: Res<ModeButtons>, mut parts: Query<(&TouchPart, &mut Face)>, mut labels: Query<(&ModeLabel, &mut PixelText)>, mut fills: Query<(&ModeFill, &mut Node)>) {
    if !mode.active {
        return;
    }
    for (part, mut face) in &mut parts {
        if let Control::Mode(i) = part.0 {
            if let Some(c) = mode.tint[i as usize] {
                let want = plate_for_tint(c);
                if face.0 != want {
                    face.0 = want;
                }
            }
        }
    }
    for (f, mut node) in &mut fills {
        let h = Val::Percent(mode.fill[f.0 as usize].clamp(0.0, 1.0) * 100.0);
        if node.height != h {
            node.height = h;
        }
    }
    for (l, mut t) in &mut labels {
        PixelText::set(&mut t, mode.labels[l.0 as usize].clone());
    }
}

fn update_ui(
    tc: Res<TouchControls>,
    state: Option<Res<AppState>>,
    mut root: Query<&mut Visibility, With<TouchRoot>>,
    mut parts: Query<(&TouchPart, Option<&mut BorderColor>, Option<(&mut Framed, &Face)>, &mut Node), (Without<Knob>, Without<PausePart>, Without<ExtraPart>, Without<ExtraFill>)>,
    mut knob: Query<&mut Node, (With<Knob>, Without<TouchPart>, Without<PausePart>, Without<ExtraPart>, Without<ExtraFill>)>,
    mut pause: Query<&mut Node, (With<PausePart>, Without<TouchPart>, Without<Knob>, Without<ExtraPart>, Without<ExtraFill>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<SafeArea>,
    mode: Res<ModeButtons>,
    mut extras: Query<(&ExtraPart, &mut Visibility, &mut Node, &mut Framed, &Face), (Without<TouchPart>, Without<Knob>, Without<PausePart>, Without<TouchRoot>)>,
    mut fills: Query<(&ExtraFill, &mut Node), (Without<ExtraPart>, Without<TouchPart>, Without<Knob>, Without<PausePart>)>,
    last: Option<Res<crate::last_input::LastInput>>,
) {
    // A gamepad in use: the stick and every button disappear, the pause button stays (`last_input`).
    let pad = last.is_some_and(|l| l.gamepad());
    let show = tc.visible && state.is_none_or(|s| *s == AppState::Playing);
    for mut v in &mut root {
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
    }
    let extra_rects = windows.single().ok().map(|w| extra_layout(w.size(), Vec3::new(area.left, area.right, area.bottom)));
    for (part, mut vis, mut node, mut framed, face) in &mut extras {
        let want = if tc.extra_active && !tc.suppress && !pad { Visibility::Inherited } else { Visibility::Hidden };
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
        let want = if tc.extra_down & (1 << part.0) != 0 { Plate::Gold } else { face.0 };
        if framed.0 != want {
            framed.0 = want;
        }
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
    for (part, border, framed, mut node) in &mut parts {
        if let Control::Mode(i) = part.0 {
            let d = if mode.active && mode.tint[i as usize].is_some() && !tc.hide_pad && !pad { Display::Flex } else { Display::None };
            if node.display != d {
                node.display = d;
            }
        } else {
            let d = if tc.hide_pad || pad { Display::None } else { Display::Flex };
            if node.display != d {
                node.display = d;
            }
        }
        let held = tc.is_held(part.0);
        if let Some(mut border) = border {
            *border = BorderColor::all(if held { Color::srgb(1.0, 0.82, 0.29) } else { Color::srgba(0.435, 0.525, 0.878, 0.9) });
        }
        if let Some((mut framed, face)) = framed {
            let want = if held { Plate::Gold } else { face.0 };
            if framed.0 != want {
                framed.0 = want;
            }
        }
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

    #[test]
    fn experiment_tints_map_to_slab_plates() {
        assert_eq!(plate_for_tint(Color::srgba(0.7, 0.18, 0.14, 0.85)), Plate::Red);
        assert_eq!(plate_for_tint(Color::srgba(0.1, 0.6, 0.3, 0.8)), Plate::Green);
        assert_eq!(plate_for_tint(Color::srgba(0.13, 0.37, 0.8, 0.85)), Plate::Blue);
        assert_eq!(plate_for_tint(Color::srgba(0.9, 0.7, 0.1, 0.85)), Plate::Gold);
        assert_eq!(plate_for_tint(Color::srgba(0.09, 0.14, 0.23, 0.74)), Plate::Dark);
        assert_eq!(button_plate(Action::Jump), Plate::Red);
        assert_eq!(button_plate(Action::Punch), Plate::Blue);
    }

    #[test]
    fn every_button_label_fits_inside_its_plate() {
        // The labels the experiments put on A / B / Z (and the mode buttons): none may spill over the plate.
        for t in ["A", "B", "Z", "L", "HOP", "DRIFT", "BRAKE", "SWORD", "HIT", "JUMP", "GRAB", "HOP DRIFT", "ITEM"] {
            for k in [2u32, 3] {
                let label = button_label(t, BUTTON_SIZE - 4.0, k);
                let size = label.size_px(k);
                assert!(size.x <= BUTTON_SIZE * k as f32 / 2.0 * 2.0 || label.max_w > 0.0, "{t}");
                if label.max_w > 0.0 {
                    assert!(size.x <= label.max_w + 0.5, "{t} at k={k}: {} > {}", size.x, label.max_w);
                }
                assert!(size.y <= BUTTON_SIZE + 0.5, "{t} is {} tall", size.y);
            }
        }
        for t in ["SHOCK WAVE", "SEEK BEAM", "SLAM", "WHIRL", "AEGIS", "DODGE"] {
            let l = PixelText::new(t, COL_WHITE).wrapped(5).fit_width(MODE_SIZE);
            assert!(l.size_px(2).x <= MODE_SIZE + 0.5 && l.size_px(2).y <= MODE_SIZE, "{t}");
        }
    }

    #[test]
    fn the_buttons_wear_plates_and_labels_after_spawning() {
        let app = overlay_app(crate::last_input::InputDevice::Touch);
        let mut app = app;
        let world = app.world_mut();
        let framed = world.query::<&Framed>().iter(world).count();
        // stick has none; pause, 3 mode, 9 extra and 4 grid buttons each have one
        assert_eq!(framed, 1 + 3 + 9 + 4);
        let labels = world.query::<&PixelText>().iter(world).filter(|t| !t.text.is_empty()).count();
        assert!(labels >= 4 + 9 + 1, "{labels}");
    }

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

    // ---- last-used input device ----

    fn overlay_app(device: crate::last_input::InputDevice) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TouchControls { visible: true, ..default() })
            .init_resource::<SafeArea>()
            .init_resource::<ModeButtons>()
            .insert_resource(AppState::Playing)
            .insert_resource(crate::last_input::LastInput::new(device));
        crate::shell::retro::hud_kit::test_resources(&mut app);
        app.add_systems(Startup, spawn_ui)
            .add_systems(Update, update_ui);
        app.update();
        app.update();
        app
    }

    fn displays(app: &mut App) -> (Vec<Display>, Vec<Display>, Visibility) {
        let world = app.world_mut();
        let parts: Vec<Display> = world.query_filtered::<&Node, With<TouchPart>>().iter(world).map(|n| n.display).collect();
        let pause: Vec<Display> = world.query_filtered::<&Node, With<PausePart>>().iter(world).map(|n| n.display).collect();
        let root = *world.query_filtered::<&Visibility, With<TouchRoot>>().single(world).unwrap();
        (parts, pause, root)
    }

    #[test]
    fn a_gamepad_hides_the_stick_and_buttons_but_keeps_the_pause_button() {
        use crate::last_input::InputDevice;
        let mut app = overlay_app(InputDevice::Touch);
        let (parts, pause, root) = displays(&mut app);
        assert_eq!(root, Visibility::Inherited);
        assert!(parts.iter().any(|d| *d == Display::Flex), "touch: stick and A/B/Z/L are drawn");
        assert_eq!(pause, vec![Display::Flex]);

        app.insert_resource(crate::last_input::LastInput::new(InputDevice::Gamepad));
        app.update();
        let (parts, pause, root) = displays(&mut app);
        assert_eq!(root, Visibility::Inherited, "the root stays up for the pause button");
        assert!(parts.iter().all(|d| *d == Display::None), "no stick or buttons with a pad: {parts:?}");
        assert_eq!(pause, vec![Display::Flex], "the pause button stays");

        // a touch brings it all back
        app.insert_resource(crate::last_input::LastInput::new(InputDevice::Touch));
        app.update();
        assert!(displays(&mut app).0.iter().any(|d| *d == Display::Flex));
    }
}
