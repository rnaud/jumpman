//! Ocarina tags (`En_Okarina_Tag`, z_en_okarina_tag.c), type 7: an invisible spot (a royal crest, the Water Temple's Triforce
//! emblems) that sets a switch flag when Link plays its song within reach: horizontally closer than `90 + interactRange`
//! (`rot.z * 40` units) and less than 80 units above or below it. A tag whose flag is already set is gone (`Actor_Kill`).
use crate::math::Vec3;

pub const U: f64 = 4.4;

/// `ocarinaSong` of the actor: the order of `OCARINA_ACTION_CHECK_SARIA` onward.
pub const SARIA: u8 = 0;
pub const EPONA: u8 = 1;
pub const LULLABY: u8 = 2;
pub const SUN: u8 = 3;
pub const TIME: u8 = 4;
pub const STORMS: u8 = 5;

#[derive(Clone, Debug)]
pub struct Tag {
    /// World units.
    pub pos: Vec3,
    /// `rot.z * 40`, OoT units.
    pub range: f64,
    pub song: u8,
    pub flag: i32,
    pub done: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Tags {
    pub list: Vec<Tag>,
}

impl Tags {
    /// From `(pos, params, rot.z)` of every `En_Okarina_Tag`; only type 7 (`params >> 10 & 0x3F`) with a real song and flag.
    pub fn new(placements: &[(Vec3, i32, i32)], flag: &dyn Fn(i32) -> bool) -> Self {
        let mut list = vec![];
        for (pos, params, rot_z) in placements {
            let (ty, song, sw) = ((params >> 10) & 0x3F, ((params >> 6) & 0xF) as u8, params & 0x3F);
            if ty != 7 || song == 0xF || sw == 0x3F {
                continue;
            }
            let done = flag(sw);
            list.push(Tag { pos: *pos, range: if *rot_z > 0 { *rot_z as f64 * 40.0 } else { 0.0 }, song, flag: sw, done });
        }
        Tags { list }
    }

    /// Link played `song` at `player` (feet): the flags to set.
    pub fn on_song(&mut self, song: u8, player: Vec3, flag: &dyn Fn(i32) -> bool) -> Vec<i32> {
        let mut out = vec![];
        for t in self.list.iter_mut() {
            if t.done || flag(t.flag) {
                t.done = true;
                continue;
            }
            let near = (player[0] - t.pos[0]).hypot(player[2] - t.pos[2]) / U < 90.0 + t.range && (player[1] - t.pos[1]).abs() / U < 80.0;
            if near && t.song == song {
                t.done = true;
                out.push(t.flag);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: fn(i32) -> bool = |_| false;

    fn crest() -> Tags {
        // the Water Temple's top crest: type 7, Zelda's Lullaby, flag 0x1E
        Tags::new(&[([1000.0, 500.0, 2000.0], 0x1C9E, 0), ([5000.0, 0.0, 0.0], 0x1C83, 1), ([0.0; 3], 0x1BFF, 11), ([0.0; 3], 0x13FF, 0)], &NONE)
    }

    #[test]
    fn only_type_7_tags_with_a_song_are_made() {
        let t = crest();
        assert_eq!(t.list.len(), 2, "the graveyard (type 6) and the door of time (type 4) are other mechanisms");
        assert_eq!((t.list[0].song, t.list[0].flag, t.list[0].range), (LULLABY, 0x1E, 0.0));
        assert_eq!(t.list[1].range, 40.0, "rot.z 1 widens the reach by 40");
    }

    #[test]
    fn the_song_sets_the_flag_within_90_units_and_80_of_height_and_only_the_right_song() {
        let mut t = crest();
        let none: &dyn Fn(i32) -> bool = &NONE;
        // wrong song, too far, too high: nothing
        assert!(t.on_song(SARIA, [1000.0, 500.0, 2000.0], none).is_empty());
        assert!(t.on_song(LULLABY, [1000.0 + 91.0 * U, 500.0, 2000.0], none).is_empty());
        assert!(t.on_song(LULLABY, [1000.0, 500.0 + 81.0 * U, 2000.0], none).is_empty());
        // in reach
        assert_eq!(t.on_song(LULLABY, [1000.0 + 89.0 * U, 500.0 + 79.0 * U, 2000.0], none), vec![0x1E]);
        // once only
        assert!(t.on_song(LULLABY, [1000.0, 500.0, 2000.0], none).is_empty());
        // the far one reaches 130 units
        assert_eq!(t.on_song(LULLABY, [5000.0 + 125.0 * U, 0.0, 0.0], none), vec![3]);
    }

    #[test]
    fn a_tag_whose_flag_is_set_is_gone() {
        let t = Tags::new(&[([0.0; 3], 0x1C9E, 0)], &|f| f == 0x1E);
        assert!(t.list[0].done);
    }
}
