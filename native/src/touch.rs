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
            .add_systems(Startup, spawn_ui)
            .add_systems(PreUpdate, drive_touch.after(read_input))
            .add_systems(Update, (update_ui, update_mode_ui));
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
        self.focus = false;
        self.set_stick(Vec2::ZERO);
    }
}

fn drive_touch(
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
    let mut swipe = Vec2::ZERO;
    for t in touches.iter_just_pressed() {
        tc.visible = true;
        if pause_rect(window.size(), area.right, area.top).contains(t.position()) {
            commands.write(StateCmd::TogglePause);
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
        for (i, (action, label)) in BUTTON_GRID.iter().enumerate() {
            let (col, row) = ((i % 2) as f32, (i / 2) as f32);
            let bg = match action {
                Action::Jump => Color::srgba(0.65, 0.18, 0.14, 0.85),
                Action::Punch => Color::srgba(0.13, 0.37, 0.58, 0.85),
                _ => fill,
            };
            p.spawn((TouchPart(Control::Button(*action)), Node { position_type: PositionType::Absolute, width: Val::Px(BUTTON_SIZE), height: Val::Px(BUTTON_SIZE), right: Val::Px(MARGIN_X + (1.0 - col) * (BUTTON_SIZE + BUTTON_GAP)), bottom: Val::Px(MARGIN_Y + (1.0 - row) * (BUTTON_SIZE + BUTTON_GAP)), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(bg), BorderColor(rim), BorderRadius::MAX))
                .with_child((Text::new(*label), TextFont { font_size: 20.0, ..default() }, TextColor(Color::WHITE)));
        }
    });
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
    mut parts: Query<(&TouchPart, &mut BorderColor, &mut Node), (Without<Knob>, Without<PausePart>)>,
    mut knob: Query<&mut Node, (With<Knob>, Without<TouchPart>, Without<PausePart>)>,
    mut pause: Query<&mut Node, (With<PausePart>, Without<TouchPart>, Without<Knob>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<SafeArea>,
    mode: Res<ModeButtons>,
) {
    let show = tc.visible && state.is_none_or(|s| *s == AppState::Playing);
    for mut v in &mut root {
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
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
            let d = if mode.active && mode.tint[i as usize].is_some() { Display::Flex } else { Display::None };
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
    fn layout_fits_iphone_landscape() {
        let size = Vec2::new(844.0, 390.0);
        for (_, r) in layout(size, Vec3::new(47.0, 47.0, 21.0)) {
            assert!(r.min.x >= 0.0 && r.max.x <= size.x && r.min.y >= 0.0 && r.max.y <= size.y);
        }
    }
}
