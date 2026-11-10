//! Port of `Input.ts`: keyboard + gamepad (+ mouse camera) mapped to [`ControllerSnapshot`].
//!
//! Button edges are latched until the next 30 Hz step so a quick tap can never fall between
//! two fixed updates (`consumeStep` in the TS). Touch controls live in `touch.rs` and merge in through `PlayerInput::merge_touch`.
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::input::InputSystems;
use bevy::prelude::*;
use crate::bindings::{Action, Bindings};
use jumpman_sim::state::{ControllerSnapshot, A_BUTTON, B_BUTTON, Z_TRIG};

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInput>().add_systems(PreUpdate, read_input.after(InputSystems));
    }
}

/// Camera deltas for the current render frame (`CameraInput`).
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraInput {
    pub dx: f32,
    pub dy: f32,
    pub wheel: f32,
    pub orbit_x: f32,
    pub orbit_y: f32,
}

#[derive(Resource, Default)]
pub struct PlayerInput {
    // Held state, refreshed every frame.
    pub stick: Vec2,
    pub stick_mag: f32,
    pub down: u32,
    pub focus_held: bool,
    /// Up arrow / right stick up: look up (`lookingUp`), with `head_turn` (-1..1, left positive).
    pub looking_up: bool,
    pub head_turn: f32,
    pub gamepad_connected: bool,
    // Edges latched until consumed by the fixed step.
    pending_a: bool,
    pending_b: bool,
    pending_z: bool,
    /// Banjo-Kazooie's C buttons and R (N64 masks, `state_consts::C_UP` ...): held state and latched edges.
    pub extra_down: u32,
    pending_extra: u32,
    // Camera accumulation until consumed by the camera system.
    mouse_dx: f32,
    mouse_dy: f32,
    wheel: f32,
    orbit_x: f32,
    orbit_y: f32,
    // One-shot UI requests.
    pub reset: bool,
    pub snap_camera: bool,
    pub cycle_camera: bool,
    pub toggle_debug: bool,
    pub start: bool,
}

impl PlayerInput {
    /// `consumeStep`: build the snapshot for one 30 Hz physics step and clear the latched edges.
    pub fn consume_step(&mut self) -> ControllerSnapshot {
        let mut pressed = 0;
        if self.pending_a {
            pressed |= A_BUTTON;
        }
        if self.pending_b {
            pressed |= B_BUTTON;
        }
        if self.pending_z {
            pressed |= Z_TRIG;
        }
        // Keep held state coherent with the latched edge.
        let pressed = pressed | self.pending_extra;
        let down = self.down | pressed | self.extra_down;
        self.pending_extra = 0;
        self.pending_a = false;
        self.pending_b = false;
        self.pending_z = false;
        ControllerSnapshot {
            focus_yaw: None,
            stick_x: self.stick.x as f64,
            stick_y: self.stick.y as f64,
            stick_mag: self.stick_mag as f64,
            button_down: down,
            button_pressed: pressed,
        }
    }

    /// Drop everything latched or held (menus swallow input; leaving them must not jump Mario).
    pub fn clear_edges(&mut self) {
        self.extra_down = 0;
        self.pending_extra = 0;
        self.pending_a = false;
        self.pending_b = false;
        self.pending_z = false;
        self.mouse_dx = 0.0;
        self.mouse_dy = 0.0;
        self.wheel = 0.0;
        self.orbit_x = 0.0;
        self.orbit_y = 0.0;
        self.stick = Vec2::ZERO;
        self.stick_mag = 0.0;
        self.down = 0;
        self.reset = false;
        self.snap_camera = false;
        self.cycle_camera = false;
        self.toggle_debug = false;
        self.start = false;
    }

    /// Buttons latched since the last step, without consuming them (for triggers that must
    /// decide before the tick runs).
    pub fn peek_pressed(&self) -> u32 {
        (if self.pending_a { A_BUTTON } else { 0 }) | (if self.pending_b { B_BUTTON } else { 0 })
    }

    /// Latch button edges (A/B/Z masks) from touch.
    pub fn latch(&mut self, pressed: u32) {
        self.pending_a |= pressed & A_BUTTON != 0;
        self.pending_b |= pressed & B_BUTTON != 0;
        self.pending_z |= pressed & Z_TRIG != 0;
    }

    /// Latch C-button / R edges (`state_consts::C_*`, `R_TRIG`) for the next step.
    pub fn latch_extra(&mut self, pressed: u32) {
        self.pending_extra |= pressed;
    }

    /// Add camera-drag pixels (touch swipe).
    pub fn add_camera_drag(&mut self, dx: f32, dy: f32) {
        self.mouse_dx += dx;
        self.mouse_dy += dy;
    }

    /// Test hook: latch an A/B tap for the next step.
    pub fn tap(&mut self, a: bool, b: bool) {
        self.pending_a |= a;
        self.pending_b |= b;
    }

    /// Test hook (`JUMPMAN_WALK`): hold the stick forward and tap A every two seconds.
    pub fn script_walk(&mut self, seconds: f32) {
        if seconds < 1.5 {
            return;
        }
        // `JUMPMAN_WALK=weave` sways the stick like a thumb steering a run.
        let sway = if std::env::var("JUMPMAN_WALK").is_ok_and(|v| v == "weave") { (seconds * 3.0).sin() * 28.0 } else { 0.0 };
        self.stick = Vec2::new(sway, 64.0);
        self.stick_mag = 64.0;
        let tap = (seconds * 30.0) as u32 % 60 == 0;
        self.pending_a |= tap;
        self.down = if (seconds % 2.0) < 0.5 { A_BUTTON } else { 0 };
    }

    /// The camera drag (pixels; mouse or touch) and the right stick / arrow keys accumulated since the camera last consumed
    /// them, without consuming: Link's first-person aim reads these (`link::aimcam`).
    pub fn look_peek(&self) -> (Vec2, Vec2) {
        (Vec2::new(self.mouse_dx, self.mouse_dy), Vec2::new(self.orbit_x, self.orbit_y))
    }

    /// `consumeCameraInput`.
    pub fn consume_camera(&mut self) -> CameraInput {
        let out = CameraInput { dx: self.mouse_dx, dy: self.mouse_dy, wheel: self.wheel, orbit_x: self.orbit_x, orbit_y: self.orbit_y };
        self.mouse_dx = 0.0;
        self.mouse_dy = 0.0;
        self.wheel = 0.0;
        out
    }
}

/// Keyboard stick: WASD, Alt walks (half magnitude).
pub fn keyboard_stick(keys: &ButtonInput<KeyCode>, b: &Bindings) -> (Vec2, f32) {
    let held = |a| f32::from(u8::from(b.pressed(keys, a)));
    let (x, y) = (held(Action::MoveRight) - held(Action::MoveLeft), held(Action::MoveUp) - held(Action::MoveDown));
    if x == 0.0 && y == 0.0 {
        return (Vec2::ZERO, 0.0);
    }
    let walk = b.pressed(keys, Action::Walk);
    let mag = if walk { 32.0 } else { 64.0 };
    (Vec2::new(x, y).normalize() * mag, mag)
}

/// Radial deadzone with rescaling (`gamepadStick`).
pub fn gamepad_stick(raw: Vec2) -> (Vec2, f32) {
    let magnitude = raw.length();
    if magnitude < 0.16 {
        return (Vec2::ZERO, 0.0);
    }
    let scaled = ((magnitude - 0.16) / 0.84).min(1.0);
    (raw / magnitude * scaled * 64.0, scaled * 64.0)
}

#[allow(clippy::too_many_arguments)]
pub fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    pads: Query<&Gamepad>,
    bindings: Res<Bindings>,
    mut input: ResMut<PlayerInput>,
    slot: Option<Res<crate::experiments::ExperimentSlot>>,
) {
    use KeyCode::*;
    let any = |codes: &[KeyCode]| codes.iter().any(|k| keys.pressed(*k));
    let pad = pads.iter().next();
    let b = &*bindings;
    let (pressed, just) = (|a| b.pressed(&keys, a), |a| b.just_pressed(&keys, a));

    // Buttons.
    let mut down = 0;
    if pressed(Action::Jump) {
        down |= A_BUTTON;
    }
    if pressed(Action::Punch) {
        down |= B_BUTTON;
    }
    if pressed(Action::Crouch) {
        down |= Z_TRIG;
    }
    if just(Action::Jump) {
        input.pending_a = true;
    }
    if just(Action::Punch) {
        input.pending_b = true;
    }
    if just(Action::Crouch) {
        input.pending_z = true;
    }
    input.focus_held = pressed(Action::Focus);

    // Stick: gamepad wins when deflected.
    let (mut stick, mut mag) = keyboard_stick(&keys, b);
    input.gamepad_connected = pad.is_some();
    let mut orbit = Vec2::new(
        f32::from(u8::from(pressed(Action::OrbitRight) || keys.pressed(ArrowRight))) - f32::from(u8::from(pressed(Action::OrbitLeft) || keys.pressed(ArrowLeft))),
        f32::from(u8::from(keys.pressed(ArrowDown))) - f32::from(u8::from(keys.pressed(ArrowUp))),
    );
    if let Some(pad) = pad {
        let (s, m) = gamepad_stick(pad.left_stick());
        if m > 0.0 {
            stick = s;
            mag = m;
        }
        let held = |a| b.pad_buttons(a).iter().any(|x| pad.pressed(*x));
        let tapped = |a| b.pad_buttons(a).iter().any(|x| pad.just_pressed(*x));
        if held(Action::Jump) {
            down |= A_BUTTON;
        }
        if held(Action::Punch) {
            down |= B_BUTTON;
        }
        if held(Action::Crouch) {
            down |= Z_TRIG;
        }
        if tapped(Action::Jump) {
            input.pending_a = true;
        }
        if tapped(Action::Punch) {
            input.pending_b = true;
        }
        if tapped(Action::Crouch) {
            input.pending_z = true;
        }
        if pad.just_pressed(GamepadButton::Start) {
            input.start = true;
        }
        // Camera and restart. The modes own Select (dock cursor, bag, map, help) and the stick clicks, so these are course-only.
        if slot.is_none_or(|s| s.id.is_none()) {
            input.cycle_camera |= tapped(Action::CycleCamera);
            input.snap_camera |= tapped(Action::SnapCamera);
            input.reset |= tapped(Action::Reset);
        }
        if held(Action::Focus) {
            input.focus_held = true;
        }
        // Right stick orbits (y is up in Bevy, down in the Gamepad API the TS used).
        let o = stick_orbit(pad.right_stick());
        orbit.x += o.x;
        orbit.y += o.y;
    }
    input.looking_up = keys.pressed(ArrowUp) || pad.is_some_and(|p| p.right_stick().y > 0.6);
    input.head_turn = f32::from(u8::from(keys.pressed(ArrowLeft))) - f32::from(u8::from(keys.pressed(ArrowRight))) - pad.map_or(0.0, |p| p.right_stick().x);
    input.stick = stick;
    input.stick_mag = mag;
    input.down = down;
    input.orbit_x = orbit.x;
    input.orbit_y = orbit.y;

    // Mouse camera: drag with either button.
    if mouse_buttons.pressed(MouseButton::Left) || mouse_buttons.pressed(MouseButton::Right) {
        input.mouse_dx += motion.delta.x;
        input.mouse_dy += motion.delta.y;
    }
    input.wheel += match scroll.unit {
        MouseScrollUnit::Line => -scroll.delta.y * 40.0,
        MouseScrollUnit::Pixel => -scroll.delta.y,
    };

    // One-shot commands.
    input.reset |= just(Action::Reset);
    input.snap_camera |= just(Action::SnapCamera);
    input.cycle_camera |= just(Action::CycleCamera);
    input.toggle_debug |= keys.just_pressed(Backquote) || keys.just_pressed(F3);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_latch_until_consumed() {
        let mut input = PlayerInput::default();
        input.pending_a = true;
        let c = input.consume_step();
        assert_eq!(c.button_pressed, A_BUTTON);
        assert_eq!(c.button_down & A_BUTTON, A_BUTTON);
        let c = input.consume_step();
        assert_eq!(c.button_pressed, 0);
    }

    #[test]
    fn keyboard_stick_normalizes_diagonals() {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyD);
        let (s, m) = keyboard_stick(&keys, &Bindings::default());
        assert_eq!(m, 64.0);
        assert!((s.length() - 64.0).abs() < 1e-3);
        assert!(s.x > 0.0 && s.y > 0.0);
    }

    #[test]
    fn gamepad_deadzone_rescales() {
        assert_eq!(gamepad_stick(Vec2::new(0.1, 0.0)).1, 0.0);
        assert!((gamepad_stick(Vec2::new(1.0, 0.0)).1 - 64.0).abs() < 1e-3);
    }
}

/// Right stick -> camera orbit input (the same channel as the orbit keys, where +x moves the camera right and so turns the
/// view LEFT). The stick is a look control: pushing right must turn the view right, so x is inverted here. y: stick up looks up.
pub fn stick_orbit(r: Vec2) -> Vec2 {
    Vec2::new(if r.x.abs() > 0.16 { -r.x } else { 0.0 }, if r.y.abs() > 0.16 { -r.y } else { 0.0 })
}

#[cfg(test)]
mod stick_orbit_tests {
    use super::*;

    #[test]
    fn right_stick_turns_the_view_the_way_it_is_pushed() {
        // pushing right gives a negative orbit.x (camera moves left = view turns right), left the opposite
        assert!(stick_orbit(Vec2::new(1.0, 0.0)).x < 0.0);
        assert!(stick_orbit(Vec2::new(-1.0, 0.0)).x > 0.0);
        // up looks up (orbit.y negative, like the Up arrow); dead zone ignored
        assert!(stick_orbit(Vec2::new(0.0, 1.0)).y < 0.0);
        assert_eq!(stick_orbit(Vec2::new(0.1, 0.1)), Vec2::ZERO);
    }
}
