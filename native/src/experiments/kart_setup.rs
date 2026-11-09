//! Kart setup of the MK64 mode: engine class (`gCCSelection`: 50cc, 100cc, 150cc, Extra = mirrored 100cc) and the
//! player's racer. Process-wide like the track pick in `kart.rs`; the values are read when a race (or free roam) starts.
//! `JUMPMAN_KART_CC=50|100|150|extra` and `JUMPMAN_KART_RACER=<name or 0..7>` preset them (headless runs, tests).

use std::sync::Mutex;

use super::kart_stats::{Cc, CHARACTERS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setup {
    pub cc: Cc,
    pub character: usize,
}
impl Default for Setup {
    fn default() -> Self {
        Self { cc: Cc::Cc150, character: 0 }
    }
}

static SETUP: Mutex<Option<Setup>> = Mutex::new(None);

fn from_env() -> Setup {
    let mut s = Setup::default();
    if let Ok(v) = std::env::var("JUMPMAN_KART_CC") {
        s.cc = match v.to_lowercase().as_str() {
            "50" | "50cc" => Cc::Cc50,
            "100" | "100cc" => Cc::Cc100,
            "extra" | "mirror" => Cc::Extra,
            _ => Cc::Cc150,
        };
    }
    if let Ok(v) = std::env::var("JUMPMAN_KART_RACER") {
        let by_name = CHARACTERS.iter().position(|n| n.eq_ignore_ascii_case(&v));
        s.character = by_name.or_else(|| v.parse::<usize>().ok()).unwrap_or(0).min(7);
    }
    s
}

pub fn get() -> Setup {
    let mut g = SETUP.lock().unwrap_or_else(|e| e.into_inner());
    *g.get_or_insert_with(from_env)
}

pub fn set(s: Setup) {
    *SETUP.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
}

/// The next engine class. Returns true when the course must be reloaded (entering or leaving the mirror).
pub fn cycle_cc() -> bool {
    let mut s = get();
    let was = s.cc.mirrors();
    s.cc = s.cc.next();
    set(s);
    was != s.cc.mirrors()
}

pub fn cycle_racer() {
    let mut s = get();
    s.character = (s.character + 1) % CHARACTERS.len();
    set(s);
}

pub fn mirrored() -> bool {
    get().cc.mirrors()
}

/// "150cc" / "Extra (mirror)".
pub fn cc_label() -> &'static str {
    get().cc.label()
}

pub fn racer_label() -> &'static str {
    CHARACTERS[get().character]
}

/// `characterId` of racer `i` in grid order (0 = the player, 1.. the CPUs, as `RaceSetup::cpu_character` fills them).
pub fn racer_character(i: usize) -> usize {
    let setup = super::kart_cpu::RaceSetup { character: get().character, ..Default::default() };
    if i == 0 { setup.character } else { setup.cpu_character(i - 1) }
}
