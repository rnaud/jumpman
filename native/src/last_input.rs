//! Which kind of device the player used last: keyboard + mouse, touch, or gamepad. One shared resource
//! ([`LastInput`]) that every on-screen prompt reads: while a gamepad is the last device used the touch overlay
//! (stick and buttons, not the pause button), the key chips of the experiment dock and the key hints are hidden;
//! a key, a mouse press or move, or a touch brings them back, and so does unplugging the pad.
//!
//! `JUMPMAN_INPUT=pad|keyboard|touch` pins the starting device (off-screen screenshots, tests).
use bevy::input::mouse::{AccumulatedMouseMotion, MouseWheel};
use bevy::input::InputSystems;
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputDevice {
    #[default]
    KeyboardMouse,
    Touch,
    Gamepad,
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LastInput {
    pub device: InputDevice,
    /// The non-pad device to fall back to when the pad goes away.
    fallback: InputDevice,
}

impl LastInput {
    pub fn new(device: InputDevice) -> Self {
        Self { device, fallback: if device == InputDevice::Gamepad { InputDevice::KeyboardMouse } else { device } }
    }
    /// A gamepad was the last device used: hide key and touch prompts.
    pub fn gamepad(&self) -> bool {
        self.device == InputDevice::Gamepad
    }
    pub fn touch(&self) -> bool {
        self.device == InputDevice::Touch
    }
    pub fn set(&mut self, device: InputDevice) {
        if device != InputDevice::Gamepad {
            self.fallback = device;
        }
        self.device = device;
    }
}

pub struct LastInputPlugin;

impl Plugin for LastInputPlugin {
    fn build(&self, app: &mut App) {
        let start = match std::env::var("JUMPMAN_INPUT").as_deref() {
            Ok("pad" | "gamepad") => InputDevice::Gamepad,
            Ok("touch") => InputDevice::Touch,
            _ if std::env::var_os("JUMPMAN_TOUCH").is_some() || std::env::var_os("JUMPMAN_TOUCH_SIM").is_some() => InputDevice::Touch,
            _ => InputDevice::KeyboardMouse,
        };
        crate::hints::set_pad(start == InputDevice::Gamepad);
        app.insert_resource(LastInput::new(start)).add_systems(PreUpdate, (detect, sync_hints).chain().after(InputSystems));
    }
}

/// Stick deflection that counts as "using the pad" (drift stays below it).
const STICK_USE: f32 = 0.5;

#[allow(clippy::too_many_arguments)]
pub fn detect(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    pads: Query<&Gamepad>,
    mut last: ResMut<LastInput>,
    mut forced: Local<Option<bool>>,
) {
    // `JUMPMAN_INPUT` pins the device for screenshots: real input does not flip it.
    let pinned = *forced.get_or_insert_with(|| std::env::var_os("JUMPMAN_INPUT").is_some());
    let wheeled = wheel.read().count() > 0;
    if pinned {
        return;
    }
    let pad_used = pads.iter().any(|p| p.get_just_pressed().next().is_some() || p.left_stick().length() > STICK_USE || p.right_stick().length() > STICK_USE);
    let touched = touches.iter_just_pressed().next().is_some();
    let kb = keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some() || motion.delta.length() > 1.5 || wheeled;
    // Latest wins; if several happen in one frame touch beats the keyboard beats the pad (those are deliberate).
    let next = if touched {
        Some(InputDevice::Touch)
    } else if kb {
        Some(InputDevice::KeyboardMouse)
    } else if pad_used {
        Some(InputDevice::Gamepad)
    } else if last.gamepad() && pads.is_empty() {
        Some(last.fallback)
    } else {
        None
    };
    if let Some(d) = next {
        if last.device != d {
            last.set(d);
        }
    }
}

/// The control-hint switch follows the device (kept out of [`detect`] so tests of that never touch the process-wide flag).
fn sync_hints(last: Res<LastInput>) {
    if crate::hints::pad_active() != last.gamepad() {
        crate::hints::set_pad(last.gamepad());
    }
}

/// Synthetic gamepad input for tests: edit the `Gamepad` component directly (what Bevy's own event system does each frame).
#[cfg(test)]
pub mod testing {
    use bevy::input::gamepad::{GamepadAxis, GamepadButton, GamepadInput};
    use bevy::prelude::*;

    pub fn spawn_pad(app: &mut App) -> Entity {
        app.world_mut().spawn(Gamepad::default()).id()
    }
    /// Press (held until [`release`]); the edge lasts until [`end_frame`].
    pub fn press(app: &mut App, pad: Entity, button: GamepadButton) {
        app.world_mut().get_mut::<Gamepad>(pad).unwrap().digital_mut().press(button);
    }
    pub fn release(app: &mut App, pad: Entity, button: GamepadButton) {
        app.world_mut().get_mut::<Gamepad>(pad).unwrap().digital_mut().release(button);
    }
    pub fn stick(app: &mut App, pad: Entity, x: f32, y: f32) {
        let mut g = app.world_mut().get_mut::<Gamepad>(pad).unwrap();
        g.analog_mut().set(GamepadInput::Axis(GamepadAxis::LeftStickX), x);
        g.analog_mut().set(GamepadInput::Axis(GamepadAxis::LeftStickY), y);
    }
    /// Tap = press, run a frame with `step`, release and clear the edges.
    pub fn tap(app: &mut App, pad: Entity, button: GamepadButton, step: impl Fn(&mut App)) {
        press(app, pad, button);
        step(app);
        release(app, pad, button);
        end_frame(app, pad);
    }
    /// A minimal app that is playing, with a pad connected: for the per-mode gamepad tests.
    pub fn base_app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<ButtonInput<KeyCode>>().insert_resource(crate::shell::AppState::Playing).add_message::<crate::shell::ReturnToMenu>();
        let pad = spawn_pad(&mut app);
        (app, pad)
    }
    /// Tap a pad button for one frame.
    pub fn tap_update(app: &mut App, pad: Entity, button: GamepadButton) {
        tap(app, pad, button, |a| a.update());
    }
    /// Messages of type `T` written since the last call (drains them).
    pub fn sent<T: Message + Clone>(app: &mut App) -> Vec<T> {
        app.world_mut().resource_mut::<Messages<T>>().drain().collect()
    }
    pub fn left_to_hub(app: &mut App) -> bool {
        !sent::<crate::shell::ReturnToMenu>(app).is_empty()
    }
    /// What Bevy does at the start of a frame: forget last frame's edges.
    pub fn end_frame(app: &mut App, pad: Entity) {
        let mut g = app.world_mut().get_mut::<Gamepad>(pad).unwrap();
        g.digital_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::gamepad::GamepadButton;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<Touches>()
            .add_message::<MouseWheel>()
            .insert_resource(LastInput::default())
            .add_systems(PreUpdate, detect);
        app
    }

    #[test]
    fn a_pad_press_switches_to_gamepad_and_a_key_switches_back() {
        let mut app = app();
        let pad = testing::spawn_pad(&mut app);
        app.update();
        assert_eq!(app.world().resource::<LastInput>().device, InputDevice::KeyboardMouse);
        testing::press(&mut app, pad, GamepadButton::South);
        app.update();
        assert!(app.world().resource::<LastInput>().gamepad());
        testing::end_frame(&mut app, pad);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);
        app.update();
        assert_eq!(app.world().resource::<LastInput>().device, InputDevice::KeyboardMouse);
    }

    #[test]
    fn a_stick_deflection_counts_but_drift_does_not() {
        let mut app = app();
        let pad = testing::spawn_pad(&mut app);
        testing::stick(&mut app, pad, 0.2, 0.0);
        app.update();
        assert!(!app.world().resource::<LastInput>().gamepad());
        testing::stick(&mut app, pad, 0.9, 0.0);
        app.update();
        assert!(app.world().resource::<LastInput>().gamepad());
    }

    #[test]
    fn unplugging_the_pad_restores_the_previous_device() {
        let mut app = app();
        app.world_mut().resource_mut::<LastInput>().set(InputDevice::Touch);
        let pad = testing::spawn_pad(&mut app);
        testing::press(&mut app, pad, GamepadButton::North);
        app.update();
        assert!(app.world().resource::<LastInput>().gamepad());
        app.world_mut().despawn(pad);
        app.update();
        assert_eq!(app.world().resource::<LastInput>().device, InputDevice::Touch);
    }
}
