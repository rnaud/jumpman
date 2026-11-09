//! The heat of the Fire Temple: without the Goron Tunic Link takes a quarter heart (`colChkInfo.damage = 4` in
//! `z_player.c`, the `floorTypeTimer` of 120 frames at 20 Hz = 6 s) while he stands in a hot area. The decomp marks hot
//! rooms with `environmentType == ROOM_ENV_HOT` in the room header; the exporter does not read that yet, so the whole of
//! the Fire Temple counts as hot.
use crate::link::items::Tunic;

/// Seconds between two burns.
pub const BURN_INTERVAL: f64 = 6.0;
/// 1/16 hearts per burn.
pub const BURN_UNITS: i32 = 4;

#[derive(Clone, Debug, Default)]
pub struct Heat {
    elapsed: f64,
}

impl Heat {
    /// Advance `dt` seconds in an area that is `hot`. Returns the damage (1/16 hearts) to apply now, 0 for none.
    pub fn step(&mut self, dt: f64, hot: bool, tunic: Tunic) -> i32 {
        if !hot || tunic == Tunic::Goron {
            self.elapsed = 0.0;
            return 0;
        }
        self.elapsed += dt;
        if self.elapsed >= BURN_INTERVAL - 1e-6 {
            self.elapsed = 0.0;
            return BURN_UNITS;
        }
        0
    }
}

/// Courses whose rooms are hot.
pub fn is_hot_course(course: &str) -> bool {
    course == "fire_temple"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fire_temple_burns_a_quarter_heart_every_six_seconds_unless_the_goron_tunic_is_worn() {
        assert!(is_hot_course("fire_temple") && !is_hot_course("forest_temple"));
        let mut h = Heat::default();
        let burns: i32 = (0..180).map(|_| h.step(0.1, true, Tunic::Kokiri)).sum();
        assert_eq!(burns, 3 * BURN_UNITS, "18 s = three burns");
        let mut h = Heat::default();
        assert_eq!((0..600).map(|_| h.step(0.1, true, Tunic::Goron)).sum::<i32>(), 0);
        let mut h = Heat::default();
        assert_eq!((0..600).map(|_| h.step(0.1, false, Tunic::Kokiri)).sum::<i32>(), 0);
        // leaving the heat resets the clock
        let mut h = Heat::default();
        h.step(5.5, true, Tunic::Kokiri);
        h.step(1.0, false, Tunic::Kokiri);
        assert_eq!(h.step(1.0, true, Tunic::Kokiri), 0);
    }
}
