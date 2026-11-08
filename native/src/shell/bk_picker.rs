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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::courses::{self, Series, COURSES};

    #[test]
    fn sub_areas_are_banjo_courses_after_the_worlds_and_the_toggle_flips() {
        assert!(courses::BK_SUBAREAS.len() >= 80, "{}", courses::BK_SUBAREAS.len());
        let first_sub = COURSES.iter().position(|c| courses::is_bk_subarea(c.id)).unwrap();
        for (i, c) in COURSES.iter().enumerate() {
            if courses::is_bk_subarea(c.id) {
                assert_eq!(c.series, Series::Banjo, "{}", c.id);
                assert!(i >= first_sub);
            }
        }
        // the 14 worlds come first, in one block right before the sub-areas
        assert_eq!(COURSES[first_sub - 14..first_sub].iter().filter(|c| c.series == Series::Banjo).count(), 14);
        assert!(!courses::is_bk_subarea("bk_spiral_mountain") && courses::is_bk_subarea("bk_gl_mm_lobby"));
        assert_eq!(group_of("Treasure Trove Cove: Blubber’s Ship"), "Treasure Trove Cove");
        let before = is_open();
        toggle();
        assert_eq!(is_open(), !before);
        toggle();
        assert_eq!(is_open(), before);
    }
}
