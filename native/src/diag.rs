//! In-game diagnostics: who is playing, whether Link's faithful controller is actually driving the sim, and what Link's data
//! looks like on this device. Shown on the OPTIONS page (pause menu -> OPTIONS -> DIAGNOSTICS) and written to stderr and
//! `<data>/import.log` whenever the course, the character or the controller state changes, so a bug report from a phone
//! ("B does nothing as Link") carries the cause.
use crate::characters::{Character, Rigs};
use crate::experiments::ExperimentSlot;
use crate::game::runner::GameSim;
use crate::render::AssetRoot;
use crate::shell::AppState;
use bevy::prelude::*;
use std::sync::Mutex;

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The diagnostic lines of the last update (empty before the first frame).
pub fn lines() -> Vec<String> {
    LINES.lock().map(|l| l.clone()).unwrap_or_default()
}

/// The facts the lines are made of, separate from the ECS so tests can build them.
pub struct Facts<'a> {
    pub state: &'a str,
    pub course: &'a str,
    pub character: Character,
    pub mode: Option<&'a str>,
    pub sim_as_link: bool,
    pub controller_enabled: bool,
    pub link_data: String,
}

pub fn describe(f: &Facts) -> Vec<String> {
    let link_wanted = f.character.is_link();
    let active = link_wanted && f.mode.is_none();
    // The one inconsistency that made B and the C buttons do nothing: Link chosen, but the sim not playing as Link.
    let verdict = if !link_wanted {
        "n/a (not Link)"
    } else if f.mode.is_some() {
        "off (a mode drives the controls)"
    } else if f.sim_as_link && f.controller_enabled {
        "OK"
    } else {
        "BROKEN"
    };
    vec![
        format!("PLAYING AS {}   COURSE {}   {}", f.character.label(), f.course.to_uppercase(), f.state.to_uppercase()),
        format!("MODE {}   SIM AS LINK {}   LINK CONTROLLER {}", f.mode.unwrap_or("-").to_uppercase(), yn(f.sim_as_link), yn(f.controller_enabled)),
        format!("LINK BUTTONS {}   CONTROLS {}", yn(active && f.controller_enabled), verdict),
        format!("LINK DATA {}", f.link_data.to_uppercase()),
    ]
}

fn yn(b: bool) -> &'static str {
    if b {
        "YES"
    } else {
        "NO"
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    state: Option<Res<AppState>>,
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    sim: Res<GameSim>,
    rigs: Res<Rigs>,
    root: Res<AssetRoot>,
    mut last: Local<String>,
) {
    let state = state.map_or("?".to_string(), |s| format!("{:?}", *s));
    let mode = slot.id.map(|id| id.title());
    let facts = Facts {
        state: &state,
        course: &sim.course.request.id,
        character: *character,
        mode,
        sim_as_link: sim.as_link,
        controller_enabled: sim.mario.link.enabled,
        link_data: rigs.link_diag(&root.0, *character),
    };
    let lines = describe(&facts);
    if let Ok(mut l) = LINES.lock() {
        *l = lines.clone();
    }
    // One log entry per change of what matters (not per frame, not for the app state alone).
    let key = format!("{}|{}|{:?}|{}|{}|{}", facts.course, facts.mode.unwrap_or("-"), facts.character, facts.sim_as_link, facts.controller_enabled, facts.link_data);
    if *last != key {
        *last = key;
        let text = format!("diag: {}", lines.join(" | "));
        eprintln!("{text}");
        crate::byor::log_line(&text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(character: Character, mode: Option<&'static str>, as_link: bool, enabled: bool) -> Facts<'static> {
        Facts { state: "Playing", course: "bob", character, mode, sim_as_link: as_link, controller_enabled: enabled, link_data: "ok".into() }
    }

    #[test]
    fn the_broken_state_of_the_ios_report_is_named() {
        let text = describe(&facts(Character::Link, None, false, true)).join("\n");
        assert!(text.contains("BROKEN"), "{text}");
        let ok = describe(&facts(Character::Link, None, true, true)).join("\n");
        assert!(ok.contains("CONTROLS OK") && ok.contains("LINK BUTTONS YES"), "{ok}");
        let gem = describe(&facts(Character::Link, Some("Gem Clash"), false, false)).join("\n");
        assert!(gem.contains("a mode drives"), "{gem}");
        assert!(describe(&facts(Character::Mario, None, false, false)).join("\n").contains("not Link"));
    }
}
