//! Rebindable controls (keyboard + gamepad). The controls screen (`shell/screens.rs`) puts the
//! resource in "listening" mode with [`Bindings::listen`]; [`capture`] then stores the next key /
//! pad button and the choice is persisted to `controls.json` next to `settings.json`.
//! Arrow keys (orbit / look up) and Esc / Start (pause) stay fixed.
use bevy::input::InputSystems;
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
    /// The N64 R button: Link's hold-to-aim, Banjo's R trigger.
    RButton,
    Reset,
    CycleCamera,
    SnapCamera,
    /// Call / park the free-driving kart (`kart_anywhere`).
    Kart,
}

impl Action {
    pub const KEYBOARD: [Action; 16] = [
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
        Action::RButton,
        Action::Reset,
        Action::CycleCamera,
        Action::SnapCamera,
        Action::Kart,
    ];
    pub const PAD: [Action; 9] = [Action::Jump, Action::Punch, Action::Crouch, Action::Focus, Action::RButton, Action::CycleCamera, Action::SnapCamera, Action::Reset, Action::Kart];

    pub fn label(self) -> &'static str {
        match self {
            Action::Jump => "Jump",
            Action::Punch => "Punch, grab, dive",
            Action::Crouch => "Z: crouch / guard / pound",
            Action::MoveUp => "Move forward",
            Action::MoveDown => "Move back",
            Action::MoveLeft => "Move left",
            Action::MoveRight => "Move right",
            Action::Walk => "Walk (half stick)",
            Action::OrbitLeft => "Orbit camera left",
            Action::OrbitRight => "Orbit camera right",
            Action::Focus => "L: lock-on (Link)",
            Action::RButton => "R: aim / Banjo R",
            Action::Reset => "Restart the run",
            Action::CycleCamera => "Cycle camera",
            Action::SnapCamera => "Snap camera behind",
            Action::Kart => "Call / leave kart",
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
            (RButton, vec![KeyCode::KeyH]),
            (Reset, vec![KeyCode::KeyR]),
            (CycleCamera, vec![KeyCode::KeyC]),
            (SnapCamera, vec![KeyCode::KeyX]),
            // kart_anywhere: B is free on foot (Link, Banjo and the modes use other letters).
            (Kart, vec![KeyCode::KeyB]),
        ]);
        let pad = BTreeMap::from([
            (Jump, vec![GamepadButton::South]),
            (Punch, vec![GamepadButton::East]),
            // N64 layout on a modern pad: Z = R1, L = L2, R = R2 (L1 is free in play; the menus keep L1 / R1 for tabs).
            // X / Y stay as extra Z buttons: nothing else in play uses them (Link's C buttons are the D-pad and R3).
            (Crouch, vec![GamepadButton::RightTrigger, GamepadButton::West, GamepadButton::North]),
            (Focus, vec![GamepadButton::LeftTrigger2]),
            (RButton, vec![GamepadButton::RightTrigger2]),
            // Camera: the stick clicks (L3 cycles, R3 snaps behind); Select restarts the run (not inside a mode: see `input::read_input`).
            (CycleCamera, vec![GamepadButton::LeftThumb]),
            (SnapCamera, vec![GamepadButton::RightThumb]),
            (Reset, vec![GamepadButton::Select]),
            // kart_anywhere: no single free button by default (the fixed combo is L1 + R1 held half a second); bindable on the controls screen.
            (Kart, vec![]),
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
        GamepadButton::LeftTrigger => "L1",
        GamepadButton::RightTrigger => "R1",
        GamepadButton::LeftTrigger2 => "L2",
        GamepadButton::RightTrigger2 => "R2",
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

    /// Is any pad button bound to `a` held / just pressed on `pad`? (L2 / R2 arrive as buttons on every platform:
    /// gilrs, the iOS GameController bridge and the Android bridge in `controller/` turn the analog trigger into a
    /// `LeftTrigger2` / `RightTrigger2` button value, and Bevy's default press threshold is 0.75.)
    pub fn pad_pressed(&self, pad: &Gamepad, a: Action) -> bool {
        self.pad_buttons(a).iter().any(|b| pad.pressed(*b))
    }
    pub fn pad_just_pressed(&self, pad: &Gamepad, a: Action) -> bool {
        self.pad_buttons(a).iter().any(|b| pad.just_pressed(*b))
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
        serde_json::json!({ "version": VERSION, "keys": keys, "pad": pad }).to_string()
    }

    /// Overlay a saved file on the defaults (unknown names are ignored). A file from before
    /// [`VERSION`] 2 keeps only the actions the player changed: an action still on its old default
    /// follows the new default (Z = R1, L = L2, R = R2). A new default never takes a button or key
    /// the player deliberately gave to another action.
    pub fn from_json(raw: &str) -> Self {
        let mut out = Self::default();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else { return out };
        let version = v["version"].as_u64().unwrap_or(1);
        let find = |n: &str| Action::KEYBOARD.iter().copied().find(|a| action_name(*a) == n);
        let old = (version < 2).then(Self::defaults_v1);
        let mut saved_keys: BTreeMap<Action, Vec<KeyCode>> = BTreeMap::new();
        let mut saved_pad: BTreeMap<Action, Vec<GamepadButton>> = BTreeMap::new();
        if let Some(map) = v["keys"].as_object() {
            for (name, list) in map {
                let (Some(a), Some(list)) = (find(name), list.as_array()) else { continue };
                let keys: Vec<KeyCode> = list.iter().filter_map(|k| k.as_str()).filter_map(|k| KEY_TABLE.iter().copied().find(|c| format!("{c:?}") == k)).collect();
                saved_keys.insert(a, keys);
            }
        }
        if let Some(map) = v["pad"].as_object() {
            for (name, list) in map {
                let (Some(a), Some(list)) = (find(name), list.as_array()) else { continue };
                let buttons: Vec<GamepadButton> = list.iter().filter_map(|k| k.as_str()).filter_map(|k| PAD_TABLE.iter().copied().find(|c| format!("{c:?}") == k)).collect();
                // Still the old default: let it follow the new one.
                if old.as_ref().is_some_and(|o| o.pad.get(&a) == Some(&buttons)) {
                    continue;
                }
                saved_pad.insert(a, buttons);
            }
        }
        // Actions the file does not set (new ones, or untouched old defaults) get their default minus anything a saved action holds.
        let claimed_k: Vec<KeyCode> = saved_keys.values().flatten().copied().collect();
        let claimed_p: Vec<GamepadButton> = saved_pad.values().flatten().copied().collect();
        for (a, list) in out.keys.iter_mut() {
            match saved_keys.get(a) {
                Some(s) => *list = s.clone(),
                None => list.retain(|k| !claimed_k.contains(k)),
            }
        }
        for (a, list) in out.pad.iter_mut() {
            match saved_pad.get(a) {
                Some(s) => *list = s.clone(),
                None => list.retain(|b| !claimed_p.contains(b)),
            }
        }
        out
    }

    /// The defaults that shipped before the Z = R1 / L = L2 / R = R2 layout (bindings file version 1).
    fn defaults_v1() -> Self {
        let mut b = Self::default();
        b.pad.insert(Action::Crouch, vec![GamepadButton::West, GamepadButton::North, GamepadButton::LeftTrigger2, GamepadButton::RightTrigger2]);
        b.pad.insert(Action::Focus, vec![GamepadButton::LeftTrigger]);
        b.pad.remove(&Action::RButton);
        b
    }
}

/// Version of the saved bindings file. 2 = the Z = R1 / L = L2 / R = R2 default layout.
const VERSION: u64 = 2;

pub fn config_path() -> std::path::PathBuf {
    crate::shell::settings::config_path().with_file_name("controls.json")
}

pub struct BindingsPlugin;

impl Plugin for BindingsPlugin {
    fn build(&self, app: &mut App) {
        let loaded = std::fs::read_to_string(config_path()).map(|s| Bindings::from_json(&s)).unwrap_or_default();
        app.insert_resource(loaded)
            .add_systems(PreUpdate, capture.after(InputSystems).before(crate::input::read_input))
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

    /// Every gameplay action has its own pad buttons: no button is bound twice.
    #[test]
    fn default_pad_layout_is_the_n64_one_without_collisions() {
        let b = Bindings::default();
        assert_eq!(b.pad_buttons(Action::Crouch), &[GamepadButton::RightTrigger, GamepadButton::West, GamepadButton::North], "Z = R1");
        assert_eq!(b.pad_buttons(Action::Focus), &[GamepadButton::LeftTrigger2], "L = L2");
        assert_eq!(b.pad_buttons(Action::RButton), &[GamepadButton::RightTrigger2], "R = R2");
        assert_eq!(b.pad_buttons(Action::Jump), &[GamepadButton::South]);
        assert_eq!(b.pad_buttons(Action::Punch), &[GamepadButton::East]);
        let all: Vec<GamepadButton> = Action::PAD.iter().flat_map(|a| b.pad_buttons(*a).iter().copied()).collect();
        for (i, x) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(x), "{} is bound twice", pad_label(*x));
        }
        // Link's C buttons (the D-pad) are not taken by an action.
        for c in [GamepadButton::DPadLeft, GamepadButton::DPadDown, GamepadButton::DPadRight, GamepadButton::DPadUp] {
            assert!(!all.contains(&c));
        }
        assert_eq!((pad_label(GamepadButton::LeftTrigger), pad_label(GamepadButton::RightTrigger), pad_label(GamepadButton::LeftTrigger2), pad_label(GamepadButton::RightTrigger2)), ("L1", "R1", "L2", "R2"));
        assert_eq!(b.pad_text(Action::Crouch), "R1 / X / Y");
        let keys: Vec<KeyCode> = Action::KEYBOARD.iter().flat_map(|a| b.keys(*a).iter().copied()).collect();
        for (i, x) in keys.iter().enumerate() {
            assert!(!keys[i + 1..].contains(x), "{x:?} is bound twice");
        }
    }

    /// The rows the Controls screen builds (`retro::pages::controls`): one gamepad row per action, in the user's vocabulary.
    #[test]
    fn controls_screen_rows_show_the_new_layout() {
        let b = Bindings::default();
        let rows: Vec<(String, String)> = Action::PAD.iter().map(|&x| (x.label().to_string(), b.pad_text(x))).collect();
        for (label, buttons) in [("Z: crouch / guard / pound", "R1 / X / Y"), ("L: lock-on (Link)", "L2"), ("R: aim / Banjo R", "R2")] {
            assert!(rows.contains(&(label.to_string(), buttons.to_string())), "{label} -> {buttons} in {rows:?}");
        }
        assert!(rows.iter().all(|(l, t)| l.is_ascii() && t.is_ascii() && !t.contains("LB") && !t.contains("RT")));
        assert!(Action::KEYBOARD.contains(&Action::RButton) && Action::PAD.contains(&Action::RButton));
    }

    #[test]
    fn saved_old_defaults_migrate_and_rebinds_survive() {
        // A file written by the previous build with the untouched old defaults (no version field).
        let old = r#"{"keys":{"Jump":["Space"]},"pad":{"Jump":["South"],"Punch":["East"],"Crouch":["West","North","LeftTrigger2","RightTrigger2"],"Focus":["LeftTrigger"],"Reset":["Select"],"Kart":[]}}"#;
        assert_eq!(Bindings::from_json(old), Bindings::default());
        // A deliberate rebind survives; the actions still on the old defaults move to the new ones.
        let old = r#"{"pad":{"Jump":["East"],"Punch":["South"],"Crouch":["West","North","LeftTrigger2","RightTrigger2"],"Focus":["LeftTrigger"]}}"#;
        let b = Bindings::from_json(old);
        assert_eq!(b.pad_buttons(Action::Jump), &[GamepadButton::East]);
        assert_eq!(b.pad_buttons(Action::Punch), &[GamepadButton::South]);
        assert_eq!(b.pad_buttons(Action::Crouch), &[GamepadButton::RightTrigger, GamepadButton::West, GamepadButton::North]);
        assert_eq!(b.pad_buttons(Action::Focus), &[GamepadButton::LeftTrigger2]);
        // A custom Focus stays; a custom Punch on R2 keeps R2 away from the new R default.
        let old = r#"{"pad":{"Punch":["RightTrigger2"],"Focus":["DPadUp"],"Crouch":["West","North","LeftTrigger2"]}}"#;
        let b = Bindings::from_json(old);
        assert_eq!(b.pad_buttons(Action::Focus), &[GamepadButton::DPadUp]);
        assert_eq!(b.pad_buttons(Action::Punch), &[GamepadButton::RightTrigger2]);
        assert!(b.pad_buttons(Action::RButton).is_empty(), "R2 belongs to the player's Punch");
        assert_eq!(b.pad_buttons(Action::Crouch), &[GamepadButton::West, GamepadButton::North, GamepadButton::LeftTrigger2]);
        // A current file is taken as written: Crouch back on a trigger is a choice now.
        let mut b = Bindings::default();
        b.rebind_pad(Action::Crouch, GamepadButton::RightTrigger2);
        let json = b.to_json();
        assert!(json.contains("\"version\":2"));
        assert_eq!(Bindings::from_json(&json), b);
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
