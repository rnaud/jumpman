//! Action dispatch, mirroring execute_mario_action's group switch (src/sim/actions/index.ts).
//!
//! Handlers return true when they changed Mario's action; the runtime then runs the loop
//! again so the new action executes in the same 30 Hz frame, exactly like the decompilation.
//!
//! One module per TS action file. Each exposes `<name>_action(m) -> Option<bool>`, a `match`
//! on `m.action` (the TS `*_ACTIONS` record): `None` = no handler. The groups below chain
//! modules that share a TS table entry set (cannon/holding/ending were separate TS files
//! referenced from the tables), so the five engineers filling these in never touch this file
//! or `mario_runtime.rs`.
pub mod airborne;
pub mod automatic;
pub mod cannon;
pub mod cutscene;
pub mod drowning;
pub mod ending;
pub mod holding;
pub mod moving;
pub mod object;
pub mod stationary;
pub mod submerged;

use crate::mario_runtime::MarioRuntime;
use crate::state::*;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn warn_once(action: u32, fallback: u32) {
    static WARNED: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
    let mut set = WARNED.get_or_init(|| Mutex::new(HashSet::new())).lock().unwrap();
    if set.insert(action) {
        eprintln!("[jumpman] no handler for action 0x{action:x}; falling back to 0x{fallback:x}");
    }
}

fn run_action(m: &mut MarioRuntime, handlers: &[fn(&mut MarioRuntime) -> Option<bool>], fallback: u32) -> bool {
    for h in handlers {
        if let Some(changed) = h(m) { return changed; }
    }
    warn_once(m.action, fallback);
    m.set_action(fallback, 0.0);
    true
}

/// Run the handler for Mario's current action; true if the action changed.
pub fn execute_mario_action(m: &mut MarioRuntime) -> bool {
    match m.action & 0x1c0 {
        ACT_GROUP_STATIONARY => run_action(m, &[stationary::stationary_action], ACT_IDLE),
        ACT_GROUP_MOVING => run_action(m, &[moving::moving_action], ACT_WALKING),
        ACT_GROUP_AIRBORNE => run_action(m, &[airborne::airborne_action, cannon::cannon_action], ACT_FREEFALL),
        ACT_GROUP_AUTOMATIC => run_action(m, &[automatic::automatic_action, cannon::cannon_action], ACT_FREEFALL),
        ACT_GROUP_OBJECT => run_action(m, &[object::object_action, holding::holding_action], ACT_IDLE),
        ACT_GROUP_CUTSCENE => run_action(m, &[cutscene::cutscene_action, ending::ending_action], ACT_STANDING_DEATH),
        ACT_GROUP_SUBMERGED => run_action(m, &[drowning::drowning_action], ACT_IDLE),
        _ => false,
    }
}
