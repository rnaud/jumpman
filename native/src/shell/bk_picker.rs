//! LEVELS tab state for the Banjo-Kazooie sub-areas (interiors, lair rooms, boss rooms): ~90 courses that would flood
//! the grid, so they sit under a header that expands on tap. The list opens by itself while one of them is selected.
use std::sync::atomic::{AtomicBool, Ordering};

static OPEN: AtomicBool = AtomicBool::new(false);

pub fn is_open() -> bool {
    OPEN.load(Ordering::Relaxed)
}

pub fn toggle() {
    OPEN.fetch_xor(true, Ordering::Relaxed);
}

/// "Treasure Trove Cove: Blubber’s Ship" -> "Treasure Trove Cove"
pub fn group_of(name: &str) -> &str {
    name.split(':').next().unwrap_or(name)
}
