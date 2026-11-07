//! Rebindable controls (keyboard + gamepad). The controls screen (`shell/screens.rs`) puts the
//! resource in "listening" mode with [`Bindings::listen`]; [`capture`] then stores the next key /
//! pad button and the choice is persisted to `controls.json` next to `settings.json`.
//! Arrow keys (orbit / look up) and Esc / Start (pause) stay fixed.
use bevy::input::InputSystem;
use bevy::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Action {
    Jump,
    Punch,
    Crouch,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Walk,
    OrbitLeft,
    OrbitRight,
    Focus,
    Reset,
    CycleCamera,
    SnapCamera,
}

impl Action {
    pub const KEYBOARD: [Action; 14] = [
        Action::MoveUp,
        Action::MoveDown,
        Action::MoveLeft,
        Action::MoveRight,
        Action::Jump,
        Action::Punch,
        Action::Crouch,
        Action::Walk,
        Action::OrbitLeft,
        Action::OrbitRight,
        Action::Focus,
        Action::Reset,
        Action::CycleCamera,
        Action::SnapCamera,
    ];
    pub const PAD: [Action; 4] = [Action::Jump, Action::Punch, Action::Crouch, Action::Focus];

    pub fn label(self) -> &'static str {
        match self {
            Action::Jump => "Jump",
            Action::Punch => "Punch, grab, dive",
            Action::Crouch => "Crouch, ground pound",
            Action::MoveUp => "Move forward",
            Action::MoveDown => "Move back",
            Action::MoveLeft => "Move left",
            Action::MoveRight => "Move right",
            Action::Walk => "Walk (half stick)",
            Action::OrbitLeft => "Orbit camera left",
            Action::OrbitRight => "Orbit camera right",
            Action::Focus => "Focus (Link)",
            Action::Reset => "Restart the run",
            Action::CycleCamera => "Cycle camera",
            Action::SnapCamera => "Snap camera behind",
        }
    }
}

/// What the controls screen is waiting for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Listen {
    Key(Action),
    Pad(Action),
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Bindings {
    pub keys: BTreeMap<Action, Vec<KeyCode>>,
    pub pad: BTreeMap<Action, Vec<GamepadButton>>,
    pub listening: Option<Listen>,
    /// Set when `capture` ended a rebind this frame (Esc cancel or an assigned key), so the menu
    /// navigation that runs later the same frame ignores that key press instead of going Back.
    pub swallow_nav: bool,
}

impl Default for Bindings {
    fn default() -> Self {
        use Action::*;
        let keys = BTreeMap::from([
            (Jump, vec![KeyCode::Space]),
            (Punch, vec![KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::KeyJ]),
            (Crouch, vec![KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::KeyK]),
            (MoveUp, vec![KeyCode::KeyW]),
            (MoveDown, vec![KeyCode::KeyS]),
            (MoveLeft, vec![KeyCode::KeyA]),
            (MoveRight, vec![KeyCode::KeyD]),
            (Walk, vec![KeyCode::AltLeft, KeyCode::AltRight]),
            (OrbitLeft, vec![KeyCode::KeyQ]),
            (OrbitRight, vec![KeyCode::KeyE]),
            (Focus, vec![KeyCode::KeyL]),
            (Reset, vec![KeyCode::KeyR]),
            (CycleCamera, vec![KeyCode::KeyC]),
            (SnapCamera, vec![KeyCode::KeyX]),
        ]);
        let pad = BTreeMap::from([
            (Jump, vec![GamepadButton::South]),
            (Punch, vec![GamepadButton::East]),
            (Crouch, vec![GamepadButton::West, GamepadButton::North, GamepadButton::LeftTrigger2, GamepadButton::RightTrigger2]),
            (Focus, vec![GamepadButton::LeftTrigger]),
        ]);
        Self { keys, pad, listening: None, swallow_nav: false }
    }
}

const KEY_TABLE: &[KeyCode] = &[
    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE, KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ,
    KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN, KeyCode::KeyO, KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT,
    KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY, KeyCode::KeyZ,
    KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
    KeyCode::Space, KeyCode::Enter, KeyCode::Tab, KeyCode::Backspace,
    KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::AltLeft, KeyCode::AltRight,
    KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight,
    KeyCode::Comma, KeyCode::Period, KeyCode::Slash, KeyCode::Semicolon, KeyCode::Quote, KeyCode::BracketLeft, KeyCode::BracketRight, KeyCode::Minus, KeyCode::Equal, KeyCode::Backslash,
    KeyCode::Numpad0, KeyCode::Numpad1, KeyCode::Numpad2, KeyCode::Numpad3, KeyCode::Numpad4, KeyCode::Numpad5, KeyCode::Numpad6, KeyCode::Numpad7, KeyCode::Numpad8, KeyCode::Numpad9,
    KeyCode::Home, KeyCode::End, KeyCode::PageUp, KeyCode::PageDown, KeyCode::Insert, KeyCode::Delete,
];

const PAD_TABLE: &[GamepadButton] = &[
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::West,
    GamepadButton::North,
    GamepadButton::LeftTrigger,
    GamepadButton::RightTrigger,
    GamepadButton::LeftTrigger2,
    GamepadButton::RightTrigger2,
    GamepadButton::Select,
    GamepadButton::LeftThumb,
    GamepadButton::RightThumb,
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
];

pub fn key_label(k: KeyCode) -> String {
    let n = format!("{k:?}");
    if let Some(l) = n.strip_prefix("Key").filter(|l| l.len() == 1) {
        return l.to_string();
    }
    if let Some(d) = n.strip_prefix("Digit").or_else(|| n.strip_prefix("Numpad")) {
        return if n.starts_with("Numpad") { format!("Num {d}") } else { d.to_string() };
    }
    match k {
        KeyCode::ShiftLeft => "Shift (L)".into(),
        KeyCode::ShiftRight => "Shift (R)".into(),
        KeyCode::ControlLeft => "Ctrl (L)".into(),
        KeyCode::ControlRight => "Ctrl (R)".into(),
        KeyCode::AltLeft => "Alt (L)".into(),
        KeyCode::AltRight => "Alt (R)".into(),
        KeyCode::ArrowUp => "Up".into(),
        KeyCode::ArrowDown => "Down".into(),
        KeyCode::ArrowLeft => "Left".into(),
        KeyCode::ArrowRight => "Right".into(),
        _ => n,
    }
}

pub fn pad_label(b: GamepadButton) -> &'static str {
    match b {
        GamepadButton::South => "A",
        GamepadButton::East => "B",
        GamepadButton::West => "X",
        GamepadButton::North => "Y",
        GamepadButton::LeftTrigger => "LB",
        GamepadButton::RightTrigger => "RB",
        GamepadButton::LeftTrigger2 => "LT",
        GamepadButton::RightTrigger2 => "RT",
        GamepadButton::Select => "Select",
        GamepadButton::LeftThumb => "L3",
        GamepadButton::RightThumb => "R3",
        GamepadButton::DPadUp => "D-pad up",
        GamepadButton::DPadDown => "D-pad down",
        GamepadButton::DPadLeft => "D-pad left",
        GamepadButton::DPadRight => "D-pad right",
        _ => "button",
    }
}

fn action_name(a: Action) -> String {
    format!("{a:?}")
}

impl Bindings {
    pub fn keys(&self, a: Action) -> &[KeyCode] {
        self.keys.get(&a).map_or(&[], Vec::as_slice)
    }
    pub fn pad_buttons(&self, a: Action) -> &[GamepadButton] {
        self.pad.get(&a).map_or(&[], Vec::as_slice)
    }
    pub fn pressed(&self, input: &ButtonInput<KeyCode>, a: Action) -> bool {
        self.keys(a).iter().any(|k| input.pressed(*k))
    }
    pub fn just_pressed(&self, input: &ButtonInput<KeyCode>, a: Action) -> bool {
        self.keys(a).iter().any(|k| input.just_pressed(*k))
    }

    pub fn key_text(&self, a: Action) -> String {
        let l: Vec<String> = self.keys(a).iter().map(|k| key_label(*k)).collect();
        if l.is_empty() { "-".into() } else { l.join(" / ") }
    }
    pub fn pad_text(&self, a: Action) -> String {
        let l: Vec<&str> = self.pad_buttons(a).iter().map(|b| pad_label(*b)).collect();
        if l.is_empty() { "-".into() } else { l.join(" / ") }
    }

    /// Bind `key` as the only key of `action` (taking it from any other action).
    pub fn rebind_key(&mut self, action: Action, key: KeyCode) {
        for v in self.keys.values_mut() {
            v.retain(|k| *k != key);
        }
        self.keys.insert(action, vec![key]);
    }
    pub fn rebind_pad(&mut self, action: Action, button: GamepadButton) {
        for v in self.pad.values_mut() {
            v.retain(|b| *b != button);
        }
        self.pad.insert(action, vec![button]);
    }

    pub fn to_json(&self) -> String {
        let keys: BTreeMap<String, Vec<String>> = self.keys.iter().map(|(a, v)| (action_name(*a), v.iter().map(|k| format!("{k:?}")).collect())).collect();
        let pad: BTreeMap<String, Vec<String>> = self.pad.iter().map(|(a, v)| (action_name(*a), v.iter().map(|b| format!("{b:?}")).collect())).collect();
        serde_json::json!({ "keys": keys, "pad": pad }).to_string()
    }

    /// Overlay a saved file on the defaults (unknown names are ignored).
    pub fn from_json(raw: &str) -> Self {
        let mut out = Self::default();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else { return out };
        let find = |n: &str| Action::KEYBOARD.iter().copied().find(|a| action_name(*a) == n);
        if let Some(map) = v["keys"].as_object() {
            for (name, list) in map {
                let (Some(a), Some(list)) = (find(name), list.as_array()) else { continue };
                let keys = list.iter().filter_map(|k| k.as_str()).filter_map(|k| KEY_TABLE.iter().copied().find(|c| format!("{c:?}") == k)).collect();
                out.keys.insert(a, keys);
            }
        }
        if let Some(map) = v["pad"].as_object() {
            for (name, list) in map {
                let (Some(a), Some(list)) = (find(name), list.as_array()) else { continue };
                let buttons = list.iter().filter_map(|k| k.as_str()).filter_map(|k| PAD_TABLE.iter().copied().find(|c| format!("{c:?}") == k)).collect();
                out.pad.insert(a, buttons);
            }
        }
        out
    }
}

pub fn config_path() -> std::path::PathBuf {
    crate::shell::settings::config_path().with_file_name("controls.json")
}

pub struct BindingsPlugin;

impl Plugin for BindingsPlugin {
    fn build(&self, app: &mut App) {
        let loaded = std::fs::read_to_string(config_path()).map(|s| Bindings::from_json(&s)).unwrap_or_default();
        app.insert_resource(loaded)
            .add_systems(PreUpdate, capture.after(InputSystem).before(crate::input::read_input))
            .add_systems(Update, persist);
    }
}

/// While listening, the next key / pad button becomes the binding; Esc / Start cancels.
fn capture(mut bindings: ResMut<Bindings>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>) {
    if bindings.swallow_nav {
        bindings.swallow_nav = false;
    }
    let Some(listen) = bindings.listening else { return };
    bindings.swallow_nav = true;
    match listen {
        Listen::Key(action) => {
            if keys.just_pressed(KeyCode::Escape) {
                bindings.listening = None;
                return;
            }
            if let Some(k) = keys.get_just_pressed().find(|k| KEY_TABLE.contains(k)) {
                bindings.rebind_key(action, *k);
                bindings.listening = None;
            }
        }
        Listen::Pad(action) => {
            if keys.just_pressed(KeyCode::Escape) || pads.iter().any(|p| p.just_pressed(GamepadButton::Start)) {
                bindings.listening = None;
                return;
            }
            if let Some(b) = pads.iter().flat_map(|p| p.get_just_pressed()).find(|b| PAD_TABLE.contains(b)) {
                bindings.rebind_pad(action, *b);
                bindings.listening = None;
            }
        }
    }
}

fn persist(bindings: Res<Bindings>, mut saved: Local<Option<String>>) {
    if !bindings.is_changed() {
        return;
    }
    let json = bindings.to_json();
    if saved.is_none() {
        *saved = Some(json);
        return;
    }
    if saved.as_deref() != Some(json.as_str()) {
        let path = config_path();
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(path, &json);
        *saved = Some(json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trips_and_rebinding_steals() {
        let mut b = Bindings::default();
        b.rebind_key(Action::Jump, KeyCode::KeyJ);
        assert_eq!(b.keys(Action::Jump), &[KeyCode::KeyJ]);
        assert!(!b.keys(Action::Punch).contains(&KeyCode::KeyJ));
        b.rebind_pad(Action::Punch, GamepadButton::South);
        assert!(b.pad_buttons(Action::Jump).is_empty());
        let back = Bindings::from_json(&b.to_json());
        assert_eq!(back, b);
        assert_eq!(Bindings::from_json("garbage"), Bindings::default());
    }

    /// Esc cancelling a rebind must not also reach the menu navigation (it would go Back a screen).
    #[test]
    fn escape_cancel_swallows_the_nav_press() {
        let mut app = App::new();
        app.insert_resource(Bindings::default()).init_resource::<ButtonInput<KeyCode>>().add_systems(Update, capture);
        app.world_mut().resource_mut::<Bindings>().listening = Some(Listen::Key(Action::Jump));
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.update();
        let b = app.world().resource::<Bindings>();
        assert!(b.listening.is_none() && b.swallow_nav, "the cancelling frame still hides the key from nav");
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        app.update();
        assert!(!app.world().resource::<Bindings>().swallow_nav, "the next frame navigates normally again");
    }

    #[test]
    fn labels_are_ascii() {
        for k in KEY_TABLE {
            assert!(key_label(*k).is_ascii());
        }
        let b = Bindings::default();
        assert_eq!(b.key_text(Action::Punch), "Shift (L) / Shift (R) / J");
    }
}
