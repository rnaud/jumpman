//! Dungeon doors of the OoT scenes (`Door_Shutter`, `En_Door`), placed from the room / scene actor lists (`mechanisms` of the
//! placement file). Engine-free: [`Doors::step`] decides, from where Link stands, his keys, the switch flags and the
//! enemies still alive near a door, which doors are open. The app keeps one collision box per closed door.
//!
//! Door types (decomp `z_door_shutter.h`, `z_en_door.h`):
//! * `Door_Shutter` 0 plain, 3 locked from the back, 4 / 6 barred (Phantom Ganon, Gohma block): these never block here;
//!   1 "front clear": shut until the room's enemies are dead; 2 / 7 "front switch": shut until its switch flag is set;
//!   5 boss door: opens with the boss key; 11 small-key door: a key is spent when Link walks into it.
//! * `En_Door` 1 `DOOR_LOCKED`: small-key door (the switch flag in the params remembers it was opened); the other types are
//!   ordinary doors (room loads, checkable, evening) and never block.
use crate::math::Vec3;

/// Scale of the world relative to OoT units.
pub const U: f64 = 4.4;
/// How close (horizontal, world units) Link has to be to a locked door to try it.
pub const TOUCH_RANGE: f64 = 150.0 * U;
/// An enemy this close to a "clear" door keeps it shut (the door's room is not known, so a radius stands in for it).
pub const CLEAR_RADIUS: f64 = 700.0 * U;
/// `unk_168 = 0xC8` frames of attention before the fence moves (20 Hz frames x 1.5).
pub const KOUSI_DELAY: f64 = 300.0;
/// The slide: speed +0.2 per frame up to 2, covering 120-150 units: about 70 frames = 105 ticks.
pub const KOUSI_SLIDE: f64 = 105.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DoorKind {
    /// Never blocks.
    Free,
    /// Shut until no enemy is alive near it.
    Clear,
    /// Shut until the switch flag is set.
    Switch(u8),
    /// `Bg_Hidan_Kousi`: a Fire Temple fence that starts to slide `KOUSI_DELAY` ticks after its switch flag is set and is out of the
    /// way once it has slid its 120-150 units (`KOUSI_SLIDE` ticks).
    SwitchSlide(u8),
    /// Opens with a small key (spent on the first try).
    SmallKey,
    /// Opens with the boss key.
    BossKey,
}

#[derive(Clone, Debug)]
pub struct Door {
    pub kind: DoorKind,
    pub pos: Vec3,
    /// Degrees about Y.
    pub yaw: f64,
    pub open: bool,
    /// Ticks since the switch was set (`SwitchSlide`).
    pub timer: f64,
    /// The actor name, for the log / tests.
    pub actor: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DoorEvent {
    /// A small key was used on door `i`.
    Unlocked(usize),
    /// Link tried a locked door without the key.
    Locked(usize),
    /// A clear / switch door opened by itself.
    Opened(usize),
}

#[derive(Clone, Debug, Default)]
pub struct Doors {
    pub doors: Vec<Door>,
}

/// `(actor name, params, position, yaw degrees)` -> the door it describes, or `None` for any other actor.
pub fn door_from_actor(actor: &str, param: i32, pos: Vec3, yaw: f64) -> Option<Door> {
    let kind = match actor {
        "ACTOR_DOOR_SHUTTER" => match (param >> 6) & 0xF {
            1 => DoorKind::Clear,
            2 | 7 => DoorKind::Switch((param & 0x3F) as u8),
            5 => DoorKind::BossKey,
            11 => DoorKind::SmallKey,
            _ => DoorKind::Free,
        },
        "ACTOR_BG_HIDAN_KOUSI" => DoorKind::SwitchSlide(((param >> 8) & 0xFF) as u8),
        "ACTOR_EN_DOOR" if (param >> 7) & 7 == 1 => DoorKind::SmallKey,
        "ACTOR_EN_DOOR" => DoorKind::Free,
        _ => return None,
    };
    Some(Door { kind, pos, yaw, open: kind == DoorKind::Free, timer: -1.0, actor: actor.to_string() })
}

impl Doors {
    pub fn new(doors: Vec<Door>) -> Self {
        Self { doors }
    }

    /// Doors that currently block the way.
    pub fn closed(&self) -> impl Iterator<Item = (usize, &Door)> {
        self.doors.iter().enumerate().filter(|(_, d)| !d.open)
    }

    /// One tick. `keys` / `boss_key` are Link's, `switch(flag)` reads the switch flags, `enemy_near(pos, radius)` says whether
    /// a hostile is still alive within `radius` of `pos`.
    pub fn step(&mut self, player: Vec3, keys: &mut i32, boss_key: bool, switch: &dyn Fn(u8) -> bool, enemy_near: &dyn Fn(Vec3, f64) -> bool) -> Vec<DoorEvent> {
        let mut events = vec![];
        for (i, d) in self.doors.iter_mut().enumerate() {
            if d.open {
                continue;
            }
            let near = (player[0] - d.pos[0]).hypot(player[2] - d.pos[2]) < TOUCH_RANGE && (player[1] - d.pos[1]).abs() < 200.0 * U;
            match d.kind {
                DoorKind::Free => d.open = true,
                DoorKind::Clear => {
                    if !enemy_near(d.pos, CLEAR_RADIUS) {
                        d.open = true;
                        events.push(DoorEvent::Opened(i));
                    }
                }
                DoorKind::Switch(f) => {
                    if switch(f) {
                        d.open = true;
                        events.push(DoorEvent::Opened(i));
                    }
                }
                DoorKind::SwitchSlide(f) => {
                    if d.timer < 0.0 && switch(f) {
                        d.timer = 0.0;
                    }
                    if d.timer >= 0.0 {
                        d.timer += 1.0;
                        if d.timer >= KOUSI_DELAY + KOUSI_SLIDE {
                            d.open = true;
                            events.push(DoorEvent::Opened(i));
                        }
                    }
                }
                DoorKind::SmallKey => {
                    if near {
                        if *keys > 0 {
                            *keys -= 1;
                            d.open = true;
                            events.push(DoorEvent::Unlocked(i));
                        } else {
                            events.push(DoorEvent::Locked(i));
                        }
                    }
                }
                DoorKind::BossKey => {
                    if near {
                        if boss_key {
                            d.open = true;
                            events.push(DoorEvent::Unlocked(i));
                        } else {
                            events.push(DoorEvent::Locked(i));
                        }
                    }
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door(kind: DoorKind, at: Vec3) -> Door {
        Door { kind, pos: at, yaw: 0.0, open: kind == DoorKind::Free, timer: -1.0, actor: "t".into() }
    }

    #[test]
    fn door_types_follow_the_decomp_params() {
        let p = [0.0; 3];
        assert_eq!(door_from_actor("ACTOR_DOOR_SHUTTER", 0, p, 0.0).unwrap().kind, DoorKind::Free);
        assert_eq!(door_from_actor("ACTOR_DOOR_SHUTTER", 1 << 6, p, 0.0).unwrap().kind, DoorKind::Clear);
        assert_eq!(door_from_actor("ACTOR_DOOR_SHUTTER", (2 << 6) | 9, p, 0.0).unwrap().kind, DoorKind::Switch(9));
        assert_eq!(door_from_actor("ACTOR_DOOR_SHUTTER", 5 << 6, p, 0.0).unwrap().kind, DoorKind::BossKey);
        assert_eq!(door_from_actor("ACTOR_DOOR_SHUTTER", 11 << 6, p, 0.0).unwrap().kind, DoorKind::SmallKey);
        assert_eq!(door_from_actor("ACTOR_EN_DOOR", (1 << 7) | 0x3F, p, 0.0).unwrap().kind, DoorKind::SmallKey);
        assert_eq!(door_from_actor("ACTOR_EN_DOOR", 0x3F, p, 0.0).unwrap().kind, DoorKind::Free);
        assert!(door_from_actor("ACTOR_OBJ_TSUBO", 0, p, 0.0).is_none());
    }

    #[test]
    fn a_small_key_door_spends_one_key_once() {
        let mut d = Doors::new(vec![door(DoorKind::SmallKey, [0.0, 0.0, 0.0])]);
        let (none, no_enemy) = (|_: u8| false, |_: Vec3, _: f64| false);
        let mut keys = 0;
        assert_eq!(d.step([0.0, 0.0, 100.0], &mut keys, false, &none, &no_enemy), vec![DoorEvent::Locked(0)]);
        assert!(!d.doors[0].open);
        keys = 2;
        assert_eq!(d.step([0.0, 0.0, 5000.0], &mut keys, false, &none, &no_enemy), vec![], "out of reach");
        assert_eq!(d.step([0.0, 0.0, 100.0], &mut keys, false, &none, &no_enemy), vec![DoorEvent::Unlocked(0)]);
        assert_eq!(keys, 1);
        assert!(d.doors[0].open);
        d.step([0.0, 0.0, 100.0], &mut keys, false, &none, &no_enemy);
        assert_eq!(keys, 1, "an open door costs nothing");
    }

    #[test]
    fn clear_and_switch_doors_open_on_their_own_and_the_boss_door_needs_its_key() {
        let mut d = Doors::new(vec![door(DoorKind::Clear, [0.0; 3]), door(DoorKind::Switch(3), [0.0; 3]), door(DoorKind::BossKey, [0.0; 3])]);
        let mut keys = 0;
        let alive = |_: Vec3, _: f64| true;
        let on = |f: u8| f == 3;
        let off = |_: u8| false;
        let ev = d.step([0.0, 0.0, 100.0], &mut keys, false, &off, &alive);
        assert_eq!(ev, vec![DoorEvent::Locked(2)]);
        assert!(d.doors.iter().all(|x| !x.open));
        let ev = d.step([0.0, 0.0, 9000.0], &mut keys, false, &on, &|_, _| false);
        assert_eq!(ev, vec![DoorEvent::Opened(0), DoorEvent::Opened(1)]);
        let ev = d.step([0.0, 0.0, 100.0], &mut keys, true, &on, &|_, _| false);
        assert_eq!(ev, vec![DoorEvent::Unlocked(2)]);
        assert_eq!(d.closed().count(), 0);
    }

    #[test]
    fn a_fire_temple_fence_slides_away_a_while_after_its_switch() {
        let mut d = Doors::new(vec![door_from_actor("ACTOR_BG_HIDAN_KOUSI", (7 << 8) | 1, [0.0; 3], 0.0).unwrap()]);
        assert_eq!(d.doors[0].kind, DoorKind::SwitchSlide(7));
        let mut keys = 0;
        let off = |_: u8| false;
        let on = |f: u8| f == 7;
        let none = |_: Vec3, _: f64| false;
        for _ in 0..1000 {
            d.step([9000.0, 0.0, 0.0], &mut keys, false, &off, &none);
        }
        assert!(!d.doors[0].open, "no switch, no movement");
        let mut ticks = 0;
        while !d.doors[0].open {
            d.step([9000.0, 0.0, 0.0], &mut keys, false, &on, &none);
            ticks += 1;
            assert!(ticks < 1000);
        }
        assert_eq!(ticks as f64, KOUSI_DELAY + KOUSI_SLIDE);
    }
}
