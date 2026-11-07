//! Port of `Input.ts`: keyboard + gamepad (+ mouse camera) mapped to [`ControllerSnapshot`].
//!
//! Button edges are latched until the next 30 Hz step so a quick tap can never fall between
//! two fixed updates (`consumeStep` in the TS). Touch controls are not ported.
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::input::InputSystem;
use bevy::prelude::*;
use jumpman_sim::state::{ControllerSnapshot, A_BUTTON, B_BUTTON, Z_TRIG};

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInput>().add_systems(PreUpdate, read_input.after(InputSystem));
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
    pub gamepad_connected: bool,
    // Edges latched until consumed by the fixed step.
    pending_a: bool,
    pending_b: bool,
    pending_z: bool,
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
        let down = self.down | pressed;
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

    /// Test hook (`JUMPMAN_WALK`): hold the stick forward and tap A every two seconds.
    pub fn script_walk(&mut self, seconds: f32) {
        if seconds < 1.5 {
            return;
        }
        self.stick = Vec2::new(0.0, 64.0);
        self.stick_mag = 64.0;
        let tap = (seconds * 30.0) as u32 % 60 == 0;
        self.pending_a |= tap;
        self.down = if (seconds % 2.0) < 0.5 { A_BUTTON } else { 0 };
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
pub fn keyboard_stick(keys: &ButtonInput<KeyCode>) -> (Vec2, f32) {
    let held = |k| f32::from(u8::from(keys.pressed(k)));
    let (x, y) = (held(KeyCode::KeyD) - held(KeyCode::KeyA), held(KeyCode::KeyW) - held(KeyCode::KeyS));
    if x == 0.0 && y == 0.0 {
        return (Vec2::ZERO, 0.0);
    }
    let walk = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
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
    mut input: ResMut<PlayerInput>,
) {
    use KeyCode::*;
    let any = |codes: &[KeyCode]| codes.iter().any(|k| keys.pressed(*k));
    let just = |codes: &[KeyCode]| codes.iter().any(|k| keys.just_pressed(*k));
    let pad = pads.iter().next();

    // Buttons.
    let (a_keys, b_keys, z_keys) = ([Space], [ShiftLeft, ShiftRight, KeyJ], [ControlLeft, ControlRight, KeyK]);
    let mut down = 0;
    if any(&a_keys) {
        down |= A_BUTTON;
    }
    if any(&b_keys) {
        down |= B_BUTTON;
    }
    if any(&z_keys) {
        down |= Z_TRIG;
    }
    if just(&a_keys) {
        input.pending_a = true;
    }
    if just(&b_keys) {
        input.pending_b = true;
    }
    if just(&z_keys) {
        input.pending_z = true;
    }
    input.focus_held = keys.pressed(KeyL);

    // Stick: gamepad wins when deflected.
    let (mut stick, mut mag) = keyboard_stick(&keys);
    input.gamepad_connected = pad.is_some();
    let mut orbit = Vec2::new(
        f32::from(u8::from(any(&[KeyE, ArrowRight]))) - f32::from(u8::from(any(&[KeyQ, ArrowLeft]))),
        f32::from(u8::from(keys.pressed(ArrowDown))) - f32::from(u8::from(keys.pressed(ArrowUp))),
    );
    if let Some(pad) = pad {
        let (s, m) = gamepad_stick(pad.left_stick());
        if m > 0.0 {
            stick = s;
            mag = m;
        }
        if pad.pressed(GamepadButton::South) {
            down |= A_BUTTON;
        }
        if pad.pressed(GamepadButton::East) {
            down |= B_BUTTON;
        }
        let z_buttons = [GamepadButton::West, GamepadButton::North, GamepadButton::LeftTrigger2, GamepadButton::RightTrigger2];
        if z_buttons.iter().any(|b| pad.pressed(*b)) {
            down |= Z_TRIG;
        }
        if pad.just_pressed(GamepadButton::South) {
            input.pending_a = true;
        }
        if pad.just_pressed(GamepadButton::East) {
            input.pending_b = true;
        }
        if z_buttons.iter().any(|b| pad.just_pressed(*b)) {
            input.pending_z = true;
        }
        if pad.just_pressed(GamepadButton::Start) {
            input.start = true;
        }
        if pad.pressed(GamepadButton::LeftTrigger) {
            input.focus_held = true;
        }
        // Right stick orbits (y is up in Bevy, down in the Gamepad API the TS used).
        let r = pad.right_stick();
        if r.x.abs() > 0.16 {
            orbit.x += r.x;
        }
        if r.y.abs() > 0.16 {
            orbit.y -= r.y;
        }
    }
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
    input.reset |= keys.just_pressed(KeyR);
    input.snap_camera |= keys.just_pressed(KeyX);
    input.cycle_camera |= keys.just_pressed(KeyC);
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
        let (s, m) = keyboard_stick(&keys);
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
