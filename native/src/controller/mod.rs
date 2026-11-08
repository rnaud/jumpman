//! Controller bridge: GameController.framework (iOS + macOS, via `gc_shim.m`) feeding Bevy `Gamepad`
//! entities, so play input, menu navigation, rebinding and the experiments read pads exactly as they
//! would from gilrs. `JUMPMAN_FAKE_PAD=<n>` scripts n virtual pads through the same pipeline (any platform).
//!
//! Lifecycle: a connect spawns one entity (connect order = player order, also written to
//! `GCController.playerIndex` so the pad's LED shows the seat); a disconnect pauses a running game;
//! the first connect hides the touch overlay (a touch brings it back).
pub mod map;

use bevy::input::gamepad::{
    GamepadConnection, GamepadConnectionEvent, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
};
use bevy::input::InputSystems;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use map::{PadEvent, Slots};

pub struct ControllerPlugin;

impl Plugin for ControllerPlugin {
    fn build(&self, app: &mut App) {
        let fake = std::env::var("JUMPMAN_FAKE_PAD").ok().map(|v| v.parse::<u32>().unwrap_or(1).clamp(1, 4));
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            unsafe { ffi::jm_pad_start() };
            info!("controller bridge ready (GameController.framework)");
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos")))]
        info!("controller bridge ready (no native backend on this platform)");
        if let Some(n) = fake {
            info!("JUMPMAN_FAKE_PAD: {n} scripted pad(s)");
        }
        app.init_resource::<Pads>()
            .insert_resource(FakePad(fake))
            .add_systems(PreUpdate, pump.before(InputSystems))
            .add_systems(PreUpdate, lifecycle.after(InputSystems));
    }
}

#[derive(Resource, Default)]
struct Pads {
    slots: Slots,
    entities: HashMap<u32, Entity>,
}

#[derive(Resource)]
struct FakePad(Option<u32>);

#[cfg(any(target_os = "ios", target_os = "macos"))]
mod ffi {
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct RawEvent {
        pub kind: i32,
        pub uid: i32,
        pub id: i32,
        pub value: f32,
    }
    extern "C" {
        pub fn jm_pad_start();
        pub fn jm_pad_poll(out: *mut RawEvent, max: i32) -> i32;
        pub fn jm_pad_name(uid: i32, buf: *mut u8, len: i32) -> i32;
        pub fn jm_pad_set_player(uid: i32, slot: i32);
    }
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn poll_native() -> Vec<PadEvent> {
    let mut raw = [ffi::RawEvent::default(); 128];
    let mut out = Vec::new();
    loop {
        let n = unsafe { ffi::jm_pad_poll(raw.as_mut_ptr(), raw.len() as i32) } as usize;
        for e in &raw[..n] {
            let name = || {
                let mut buf = [0u8; 128];
                let len = unsafe { ffi::jm_pad_name(e.uid, buf.as_mut_ptr(), buf.len() as i32) }.clamp(0, 127) as usize;
                String::from_utf8_lossy(&buf[..len]).into_owned()
            };
            if let Some(ev) = map::decode(e.kind, e.uid, e.id, e.value, name) {
                out.push(ev);
            }
        }
        if n < raw.len() {
            return out;
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn poll_native() -> Vec<PadEvent> {
    Vec::new()
}

#[allow(clippy::too_many_arguments)]
fn pump(
    mut commands: Commands,
    mut pads: ResMut<Pads>,
    fake: Res<FakePad>,
    time: Res<Time<Real>>,
    mut last_t: Local<f32>,
    mut raw: MessageWriter<RawGamepadEvent>,
    mut connection: MessageWriter<GamepadConnectionEvent>,
    mut buttons: MessageWriter<RawGamepadButtonChangedEvent>,
    mut axes: MessageWriter<RawGamepadAxisChangedEvent>,
) {
    let mut events = poll_native();
    let t = time.elapsed_secs();
    if let Some(n) = fake.0 {
        events.extend(map::fake_events(n, t, *last_t));
    }
    *last_t = t;
    for ev in events {
        match ev {
            PadEvent::Connect { uid, name } => {
                let Some(_slot) = pads.slots.connect(uid) else { continue };
                if pads.entities.contains_key(&uid) {
                    continue;
                }
                let entity = commands.spawn_empty().id();
                pads.entities.insert(uid, entity);
                #[cfg(any(target_os = "ios", target_os = "macos"))]
                if fake.0.is_none() {
                    unsafe { ffi::jm_pad_set_player(uid as i32, _slot as i32) };
                }
                info!("controller {_slot}: {name}");
                let event = GamepadConnectionEvent::new(entity, GamepadConnection::Connected { name, vendor_id: None, product_id: None });
                raw.write(event.clone().into());
                connection.write(event);
            }
            PadEvent::Disconnect { uid } => {
                pads.slots.disconnect(uid);
                let Some(entity) = pads.entities.remove(&uid) else { continue };
                info!("controller disconnected");
                let event = GamepadConnectionEvent::new(entity, GamepadConnection::Disconnected);
                raw.write(event.clone().into());
                connection.write(event);
            }
            PadEvent::Button { uid, element, value } => {
                let (Some(&entity), Some(button)) = (pads.entities.get(&uid), element.bevy()) else { continue };
                let value = map::clean_button(element, value);
                raw.write(RawGamepadButtonChangedEvent::new(entity, button, value).into());
                buttons.write(RawGamepadButtonChangedEvent::new(entity, button, value));
            }
            PadEvent::Axis { uid, axis, value } => {
                let Some(&entity) = pads.entities.get(&uid) else { continue };
                let value = map::clean_axis(value);
                raw.write(RawGamepadAxisChangedEvent::new(entity, axis, value).into());
                axes.write(RawGamepadAxisChangedEvent::new(entity, axis, value));
            }
        }
    }
}

/// Connect hides the touch overlay; losing the last pad while playing pauses (and brings the overlay back on iOS).
fn lifecycle(
    mut events: MessageReader<GamepadConnectionEvent>,
    pads: Query<(), With<Gamepad>>,
    state: Option<Res<crate::shell::AppState>>,
    mut pause: MessageWriter<crate::shell::StateCmd>,
    mut touch: ResMut<crate::touch::TouchControls>,
) {
    for ev in events.read() {
        match ev.connection {
            GamepadConnection::Connected { .. } => touch.visible = false,
            GamepadConnection::Disconnected => {
                if state.as_deref() == Some(&crate::shell::AppState::Playing) {
                    pause.write(crate::shell::StateCmd::TogglePause);
                }
                // `pads` still counts the departing entity until commands flush; only restore when it is the last.
                if cfg!(target_os = "ios") && pads.iter().count() <= 1 {
                    touch.visible = true;
                }
            }
        }
    }
}
