//! On-screen control hints and explanatory text (the "WASD / Press X to..." lines, mode cards and titles, tutorial and room banners,
//! context prompts, coin/ouch toasts, the action readout) behind one switch. Gameplay readouts (counters, timers, meters) are never gated.
//!
//! They take screen space and the same information lives in Settings > Controls and, per mode, on the pause menu's HOW TO PLAY page,
//! so they are off by default (`Settings::show_hints`, or `JUMPMAN_HINTS=1`). A hint is spawned as usual with the [`ControlHint`]
//! marker on its root node and registered with [`note`]; [`HintsPlugin`] then hides every marked node while the switch is off. The
//! hint *text* is still produced (tests read it); only its display is switched.
use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static SHOW: AtomicBool = AtomicBool::new(false);
/// A gamepad was the last device used (see `last_input`): button prompts are hidden.
static PAD: AtomicBool = AtomicBool::new(false);
static NOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CARD: Mutex<String> = Mutex::new(String::new());

/// Remember the running mode's info card text (`NN  Title\npitch\n\n[key] action...`), set by the card builder, for HOW TO PLAY.
pub fn set_card(text: &str) {
    *CARD.lock().unwrap() = text.to_string();
}

/// What the HOW TO PLAY page shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Help {
    pub pitch: String,
    pub keys: Vec<(String, String)>,
    pub notes: Vec<String>,
}

pub fn help() -> Help {
    let card = CARD.lock().unwrap().clone();
    let mut lines = card.lines();
    let _head = lines.next();
    let pitch = lines.next().unwrap_or("").to_string();
    let keys = lines.filter_map(|l| l.strip_prefix('[')?.split_once("] ").map(|(k, a)| (k.to_string(), a.to_string()))).collect();
    Help { pitch, keys, notes: notes() }
}

/// A running mode has something to teach (its card or hint notes): the pause menu then offers HOW TO PLAY.
pub fn has_help() -> bool {
    !CARD.lock().unwrap().is_empty() || !NOTES.lock().unwrap().is_empty()
}

/// Whether control hints are shown right now.
pub fn show() -> bool {
    SHOW.load(Ordering::Relaxed) && !pad_active()
}

/// The hints switch itself (the setting), whatever device is in use: for explanatory text that names no button (coin / ouch toasts).
pub fn setting() -> bool {
    SHOW.load(Ordering::Relaxed)
}

/// A gamepad is the active device: key and touch prompts stay hidden whatever the setting says.
pub fn pad_active() -> bool {
    PAD.load(Ordering::Relaxed)
}

pub fn set_pad(on: bool) {
    PAD.store(on, Ordering::Relaxed);
}

pub fn set_show(on: bool) {
    SHOW.store(on, Ordering::Relaxed);
}

/// Marker on the root node of a control hint: hidden unless [`show`].
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ControlHint;

/// Register the text of a hint the running mode shows (or would show) so HOW TO PLAY can list it. Duplicates are ignored.
pub fn note(text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let mut n = NOTES.lock().unwrap();
    if !n.iter().any(|t| t == text) {
        n.push(text.to_string());
    }
}

pub fn notes() -> Vec<String> {
    NOTES.lock().unwrap().clone()
}

pub fn clear_notes() {
    NOTES.lock().unwrap().clear();
}

/// A mode ended.
pub fn clear_help() {
    NOTES.lock().unwrap().clear();
    CARD.lock().unwrap().clear();
}

pub struct HintsPlugin;

impl Plugin for HintsPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("JUMPMAN_HINTS").is_ok_and(|v| v != "0") {
            set_show(true);
        }
        app.add_systems(Update, (sync_setting, apply_display).chain());
    }
}

/// The saved setting drives the switch (the env override stays on).
fn sync_setting(settings: Option<Res<crate::shell::settings::Settings>>) {
    if let Some(s) = settings {
        if s.is_changed() {
            set_show(s.show_hints || std::env::var("JUMPMAN_HINTS").is_ok_and(|v| v != "0"));
        }
    }
}

fn apply_display(mut hints: Query<&mut Node, With<ControlHint>>) {
    let want = if show() { Display::Flex } else { Display::None };
    for mut n in &mut hints {
        if n.display != want {
            n.display = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The notes and the card are process-wide: tests touching them take turns.
    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn marked_nodes_are_hidden_until_hints_are_switched_on() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, apply_display);
        let e = app.world_mut().spawn((ControlHint, Node::default())).id();
        let other = app.world_mut().spawn(Node::default()).id();
        set_show(false);
        app.update();
        assert_eq!(app.world().get::<Node>(e).unwrap().display, Display::None);
        assert_eq!(app.world().get::<Node>(other).unwrap().display, Display::Flex, "unmarked UI stays");
        set_show(true);
        app.update();
        assert_eq!(app.world().get::<Node>(e).unwrap().display, Display::Flex);
        set_show(false);
    }

    #[test]
    fn an_active_gamepad_hides_hints_even_when_they_are_switched_on() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, apply_display);
        let e = app.world_mut().spawn((ControlHint, Node::default())).id();
        set_show(true);
        set_pad(true);
        app.update();
        assert_eq!(app.world().get::<Node>(e).unwrap().display, Display::None);
        set_pad(false);
        app.update();
        assert_eq!(app.world().get::<Node>(e).unwrap().display, Display::Flex);
        set_show(false);
    }

    #[test]
    fn notes_are_deduplicated_and_clearable() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_notes();
        note("Hold A to dive");
        note(" Hold A to dive ");
        note("");
        note("Tap B");
        assert_eq!(notes(), vec!["Hold A to dive".to_string(), "Tap B".to_string()]);
        clear_notes();
        assert!(notes().is_empty());
    }

    #[test]
    fn the_how_to_play_help_is_parsed_from_the_mode_card_and_notes() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_help();
        assert!(!has_help());
        set_card("10  Kart\nDrive through the item boxes.\n\n[W/S] accelerate\n[A/D] steer");
        note("Tap a map");
        let h = help();
        assert_eq!(h.pitch, "Drive through the item boxes.");
        assert_eq!(h.keys, vec![("W/S".to_string(), "accelerate".to_string()), ("A/D".to_string(), "steer".to_string())]);
        assert_eq!(h.notes, vec!["Tap a map".to_string()]);
        assert!(has_help());
        clear_help();
        assert!(!has_help() && help() == Help::default());
    }

    #[test]
    fn hints_are_off_by_default_in_the_saved_settings() {
        let d = crate::shell::settings::Settings::default();
        assert!(!d.show_hints);
        let on = crate::shell::settings::Settings::from_json(r#"{"show_hints":true}"#);
        assert!(on.show_hints);
        assert!(!crate::shell::settings::Settings::from_json(r#"{"show_hints":"yes"}"#).show_hints);
        assert!(crate::shell::settings::Settings::from_json(&on.to_json()).show_hints, "round trip");
    }
}
