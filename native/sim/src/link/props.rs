//! World props that Link's items act on, placed from the OoT room actor lists by `scripts/export-oot-courses.py`
//! (`props` in the course json): cuttable grass (`EnKusa`), pots (`ObjTsubo`), crates (`ObjKibako2`), rocks (`EnIshi`),
//! bombable boulders (`ObjBombiwa`), hammer rocks (`ObjHamishi`), cracked walls (`BgBreakwall`), webs (`BgYdanSp`),
//! torches (`ObjSyokudai`) and switches (`ObjSwitch`: floor, rusty floor, eye, crystal).
//!
//! Engine-free: [`Props::hit`] takes a weapon row ([`Dmg`], the same 32-row damage types the enemies use), [`Props::blast`] an
//! explosion, [`Props::fire`] a flame (burning Deku Stick tip, Din's Fire sphere, fire arrow), [`Props::step`] the player's
//! weight on floor switches and the torch / web timers. Results come out as [`PropEvent`]s (broken, burned, lit, switch flags set,
//! drops to roll, sounds); the app applies them (hides the model, rolls the drop table, opens whatever listens to the flag).
//!
//! Who can do what (damage masks of the actors, simplified to the damage types):
//! * grass / pots: any blade, spin, jump slash, Deku Stick, hammer, boomerang, explosion, arrows, seeds (pots: not seeds / nuts);
//! * crates: hammer, explosion, spin and jump slashes (`ObjKibako2` takes strong blows only);
//! * rocks (large `EnIshi`): hammer, explosion; boulders: explosion; hammer rocks: hammer;  cracked walls: explosion (hammer for type 1);
//! * webs: fire arrow, Din's Fire, a burning Deku Stick in range (`BgYdanSp`: AC mask 0x00020800 = fire arrow | fire magic);
//! * torches: the same fire sources light them; wooden torches stay lit, timed ones burn `count * 50 + 100` OoT frames;
//! * eye switches: seeds, arrows, boomerang; crystal switches: any weapon; floor switches: weight (Link standing on it) or the
//!   hammer; rusty switches: the hammer only. `subtype` 0 stays on, 1 toggles, 2 holds while stood on.
use super::combat::Dmg;
use super::projectiles::{Target, TargetKind};
use crate::math::Vec3;

/// Target ids handed to the projectile set: `PROP_TARGET_BASE + prop index`.
pub const PROP_TARGET_BASE: u32 = 400_000;
pub const PROP_TARGET_END: u32 = 500_000;
/// Scale of the world relative to OoT units.
pub const U: f64 = 4.4;
/// Ticks (30 Hz) a lit Deku Stick burns (about 10 s; the original stick burns until it burns out or breaks).
pub const STICK_BURN_TICKS: f64 = 300.0;
/// Link's burning stick lights a torch or web within this distance of the tip.
pub const STICK_LIGHT_RANGE: f64 = 28.0 * U;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Grass,
    Pot,
    Crate,
    Rock,
    LargeRock,
    Boulder,
    HammerRock,
    CrackedWall,
    FloorWeb,
    WallWeb,
    Torch,
    EyeSwitch,
    CrystalSwitch,
    FloorSwitch,
    RustySwitch,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "grass" => Kind::Grass,
            "pot" => Kind::Pot,
            "crate" => Kind::Crate,
            "rock" => Kind::Rock,
            "large_rock" => Kind::LargeRock,
            "boulder" => Kind::Boulder,
            "hammer_rock" => Kind::HammerRock,
            "cracked_wall" => Kind::CrackedWall,
            "web" => Kind::FloorWeb,
            "wall_web" => Kind::WallWeb,
            "torch" => Kind::Torch,
            "eye_switch" => Kind::EyeSwitch,
            "crystal_switch" => Kind::CrystalSwitch,
            "floor_switch" => Kind::FloorSwitch,
            "rusty_switch" => Kind::RustySwitch,
            _ => return None,
        })
    }

    /// Collision radius and height (world units; the actors' colliders times 4.4).
    pub fn size(self) -> (f64, f64) {
        match self {
            Kind::Grass => (12.0 * U, 30.0 * U),
            Kind::Pot => (14.0 * U, 30.0 * U),
            Kind::Crate => (30.0 * U, 60.0 * U),
            Kind::Rock => (10.0 * U, 20.0 * U),
            Kind::LargeRock => (30.0 * U, 60.0 * U),
            Kind::Boulder => (40.0 * U, 80.0 * U),
            Kind::HammerRock => (40.0 * U, 80.0 * U),
            Kind::CrackedWall => (80.0 * U, 150.0 * U),
            Kind::FloorWeb => (75.0 * U, 30.0 * U),
            Kind::WallWeb => (80.0 * U, 150.0 * U),
            Kind::Torch => (20.0 * U, 70.0 * U),
            Kind::EyeSwitch => (15.0 * U, 30.0 * U),
            Kind::CrystalSwitch => (15.0 * U, 30.0 * U),
            Kind::FloorSwitch | Kind::RustySwitch => (20.0 * U, 12.0 * U),
        }
    }

    fn is_switch(self) -> bool {
        matches!(self, Kind::EyeSwitch | Kind::CrystalSwitch | Kind::FloorSwitch | Kind::RustySwitch)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Intact,
    /// A web burning / a prop in its last frames.
    Burning(f64),
    Gone,
    /// A torch alight: ticks left (`f64::INFINITY` for the ones that stay lit).
    Lit(f64),
    /// A switch that is on.
    On,
}

#[derive(Clone, Debug)]
pub struct Prop {
    pub kind: Kind,
    pub pos: Vec3,
    pub yaw: f64,
    /// Switch flag it sets or reads (`-1`: none).
    pub flag: i32,
    /// Torch: `count` (timer), switch: sub type.
    pub param: i32,
    /// Drop table row (grass, small rocks) or a fixed `ITEM00_*` (pots, `item`).
    pub table: Option<u8>,
    pub item: Option<u8>,
    /// Torches: a wooden torch stays lit once lit (`type` 1 / 0), a timed one (type 2) does not.
    pub timed: bool,
    /// Index of the course object that draws it (hidden when the prop is gone), set by the app.
    pub object: Option<usize>,
    /// The dungeon logic's name for it (`web`, `lowerWeb`, `eye`, `switch`), set by the app.
    pub role: Option<String>,
    pub state: State,
}

impl Prop {
    pub fn new(kind: Kind, pos: Vec3) -> Self {
        Self { kind, pos, yaw: 0.0, flag: -1, param: 0, table: None, item: None, timed: false, object: None, role: None, state: State::Intact }
    }

    pub fn gone(&self) -> bool {
        self.state == State::Gone
    }

    pub fn is_lit(&self) -> bool {
        matches!(self.state, State::Lit(_))
    }

    pub fn center(&self) -> Vec3 {
        [self.pos[0], self.pos[1] + self.kind.size().1 * 0.5, self.pos[2]]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropEvent {
    /// Smashed / cut / blasted open; `Drop`s follow.
    Broke { id: usize, kind: Kind, pos: Vec3 },
    /// A web burned away.
    Burned { id: usize, kind: Kind, pos: Vec3 },
    /// A torch lit or went out.
    Torch { id: usize, pos: Vec3, lit: bool },
    /// A switch changed: `flag` is the switch flag it drives.
    Switch { id: usize, flag: i32, on: bool },
    Drop { pos: Vec3, table: Option<u8>, item: Option<u8> },
    /// A sound by `NA_SE_` name without the prefix.
    Sfx { name: &'static str, pos: Vec3 },
}

#[derive(Clone, Debug, Default)]
pub struct Props {
    pub list: Vec<Prop>,
    /// Switch flags 0..63 (`Flags_GetSwitch`).
    pub flags: u64,
    pub events: Vec<PropEvent>,
    /// Burning-stick tip last tick, to avoid re-announcing the same hit.
    clock: u64,
}

fn dist(a: &Vec3, b: &Vec3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn fire_dmg(d: Dmg) -> bool {
    matches!(d, Dmg::FireArrow | Dmg::FireMagic)
}

fn arrow(d: Dmg) -> bool {
    matches!(d, Dmg::Arrow | Dmg::FireArrow | Dmg::IceArrow | Dmg::LightArrow | Dmg::UnkArrow1 | Dmg::UnkArrow2 | Dmg::UnkArrow3)
}

fn blade(d: Dmg) -> bool {
    d.is_sword() || d == Dmg::Stick
}

fn strong_blow(d: Dmg) -> bool {
    matches!(d, Dmg::KokiriSpin | Dmg::GiantSpin | Dmg::MasterSpin | Dmg::KokiriJump | Dmg::GiantJump | Dmg::MasterJump | Dmg::GiantSword)
}

impl Props {
    pub fn new(list: Vec<Prop>) -> Self {
        Self { list, ..Default::default() }
    }

    pub fn flag(&self, f: i32) -> bool {
        f >= 0 && f < 64 && self.flags >> f & 1 != 0
    }

    fn set_flag(&mut self, f: i32, on: bool) {
        if (0..64).contains(&f) {
            if on {
                self.flags |= 1 << f;
            } else {
                self.flags &= !(1 << f);
            }
        }
    }

    /// Projectile targets for the props a seed / arrow / boomerang / hookshot can touch.
    pub fn targets(&self) -> Vec<Target> {
        self.list
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.gone() && !matches!(p.kind, Kind::FloorSwitch | Kind::RustySwitch | Kind::FloorWeb))
            .map(|(i, p)| {
                let (r, h) = p.kind.size();
                let kind = if p.kind.is_switch() { TargetKind::Switch } else { TargetKind::Breakable };
                Target { id: PROP_TARGET_BASE + i as u32, pos: p.pos, radius: r, height: h, kind }
            })
            .collect()
    }

    /// A weapon blow on prop `id`. Returns whether the prop reacted.
    pub fn hit(&mut self, id: usize, dmg: Dmg) -> bool {
        let Some(p) = self.list.get(id) else { return false };
        if p.gone() {
            return false;
        }
        let kind = p.kind;
        match kind {
            Kind::Grass => {
                if blade(dmg) || arrow(dmg) || matches!(dmg, Dmg::HammerSwing | Dmg::HammerJump | Dmg::Boomerang | Dmg::Explosive | Dmg::Slingshot) {
                    return self.break_prop(id, "EV_PLANT_BROKEN");
                }
            }
            Kind::Pot => {
                if blade(dmg) || arrow(dmg) || matches!(dmg, Dmg::HammerSwing | Dmg::HammerJump | Dmg::Boomerang | Dmg::Explosive) {
                    return self.break_prop(id, "EV_POT_BROKEN");
                }
            }
            Kind::Crate => {
                if strong_blow(dmg) || matches!(dmg, Dmg::HammerSwing | Dmg::HammerJump | Dmg::Explosive) {
                    return self.break_prop(id, "EV_WOODBOX_BREAK");
                }
            }
            Kind::Rock => {
                if matches!(dmg, Dmg::HammerSwing | Dmg::HammerJump | Dmg::Explosive) || strong_blow(dmg) {
                    return self.break_prop(id, "EV_ROCK_BROKEN");
                }
            }
            Kind::LargeRock => {
                if matches!(dmg, Dmg::HammerSwing | Dmg::HammerJump | Dmg::Explosive) {
                    return self.break_prop(id, "EV_ROCK_BROKEN");
                }
            }
            Kind::Boulder => {
                if dmg == Dmg::Explosive {
                    return self.break_prop(id, "EV_WALL_BROKEN");
                }
            }
            Kind::HammerRock => {
                if matches!(dmg, Dmg::HammerSwing | Dmg::HammerJump) {
                    return self.break_prop(id, "EV_WALL_BROKEN");
                }
            }
            Kind::CrackedWall => {
                if matches!(dmg, Dmg::Explosive | Dmg::HammerSwing | Dmg::HammerJump) {
                    return self.break_prop(id, "EV_WALL_BROKEN");
                }
            }
            Kind::FloorWeb | Kind::WallWeb => {
                if fire_dmg(dmg) {
                    return self.burn_web(id);
                }
            }
            Kind::Torch => {
                if fire_dmg(dmg) {
                    return self.light(id);
                }
            }
            Kind::EyeSwitch => {
                if matches!(dmg, Dmg::Slingshot | Dmg::Boomerang) || arrow(dmg) {
                    return self.press(id, true);
                }
            }
            Kind::CrystalSwitch => {
                if !matches!(dmg, Dmg::Shield | Dmg::Hookshot | Dmg::Nut | Dmg::Unknown1 | Dmg::Unknown2) {
                    return self.press(id, true);
                }
            }
            Kind::FloorSwitch | Kind::RustySwitch => {
                if matches!(dmg, Dmg::HammerSwing | Dmg::HammerJump) {
                    return self.press(id, true);
                }
            }
        }
        false
    }

    /// An explosion (bomb, bombchu): everything whose collider is within `radius` is blasted.
    pub fn blast(&mut self, pos: Vec3, radius: f64) -> usize {
        let ids: Vec<usize> = (0..self.list.len()).filter(|&i| !self.list[i].gone() && dist(&self.list[i].center(), &pos) < radius + self.list[i].kind.size().0).collect();
        ids.into_iter().filter(|&i| self.hit(i, Dmg::Explosive)).count()
    }

    /// A flame at `pos` (`radius` around it): lights torches and burns webs. Din's Fire passes a big radius, a fire arrow
    /// its point of impact, a burning stick the tip ([`STICK_LIGHT_RANGE`]).
    pub fn fire(&mut self, pos: Vec3, radius: f64) -> usize {
        let ids: Vec<usize> =
            (0..self.list.len()).filter(|&i| matches!(self.list[i].kind, Kind::Torch | Kind::FloorWeb | Kind::WallWeb) && !self.list[i].gone() && dist(&self.list[i].center(), &pos) < radius + self.list[i].kind.size().0).collect();
        ids.into_iter().filter(|&i| self.hit(i, Dmg::FireMagic)).count()
    }

    /// Is there a flame source (a lit torch) within `range` of `pos`? A Deku Stick held there catches fire.
    pub fn flame_near(&self, pos: &Vec3, range: f64) -> bool {
        self.list.iter().any(|p| p.is_lit() && dist(&p.center(), pos) < range + p.kind.size().0)
    }

    fn break_prop(&mut self, id: usize, sfx: &'static str) -> bool {
        let p = &mut self.list[id];
        p.state = State::Gone;
        let (kind, pos, flag, table, item) = (p.kind, p.pos, p.flag, p.table, p.item);
        self.events.push(PropEvent::Sfx { name: sfx, pos });
        self.events.push(PropEvent::Broke { id, kind, pos });
        if table.is_some() || item.is_some() {
            self.events.push(PropEvent::Drop { pos: [pos[0], pos[1] + 20.0, pos[2]], table, item });
        }
        if matches!(kind, Kind::Boulder | Kind::HammerRock | Kind::CrackedWall) && flag >= 0 {
            self.set_flag(flag, true);
            self.events.push(PropEvent::Switch { id, flag, on: true });
        }
        true
    }

    fn burn_web(&mut self, id: usize) -> bool {
        let p = &mut self.list[id];
        if p.state != State::Intact {
            return false;
        }
        p.state = State::Burning(30.0 * 1.5);
        let (kind, pos, flag) = (p.kind, p.pos, p.flag);
        self.events.push(PropEvent::Sfx { name: "SY_CORRECT_CHIME", pos });
        self.events.push(PropEvent::Sfx { name: "EV_WEB_BROKEN", pos });
        self.events.push(PropEvent::Burned { id, kind, pos });
        if flag >= 0 {
            self.set_flag(flag, true);
            self.events.push(PropEvent::Switch { id, flag, on: true });
        }
        true
    }

    fn light(&mut self, id: usize) -> bool {
        let p = &mut self.list[id];
        if p.is_lit() {
            // A timed torch relit gets its time back.
            if p.timed {
                p.state = State::Lit(torch_ticks(p.param));
            }
            return false;
        }
        p.state = State::Lit(if p.timed { torch_ticks(p.param) } else { f64::INFINITY });
        let (pos, flag) = (p.pos, p.flag);
        self.events.push(PropEvent::Sfx { name: "EV_FLAME_IGNITION", pos });
        self.events.push(PropEvent::Torch { id, pos, lit: true });
        if flag >= 0 {
            self.set_flag(flag, true);
            self.events.push(PropEvent::Switch { id, flag, on: true });
        }
        true
    }

    fn press(&mut self, id: usize, down: bool) -> bool {
        let p = &mut self.list[id];
        let (kind, pos, flag, sub) = (p.kind, p.pos, p.flag, p.param);
        let toggles = sub == 1 || kind == Kind::CrystalSwitch && sub != 0;
        let was_on = p.state == State::On;
        let on = match (down, toggles) {
            (true, true) => !was_on,
            (true, false) => true,
            (false, _) => false,
        };
        if on == was_on {
            return false;
        }
        p.state = if on { State::On } else { State::Intact };
        self.events.push(PropEvent::Sfx {
            name: match kind {
                Kind::EyeSwitch => "EV_RED_EYE",
                Kind::CrystalSwitch => "EV_DIAMOND_SWITCH",
                Kind::RustySwitch => "EV_HAMMER_SWITCH",
                _ => "EV_FOOT_SWITCH",
            },
            pos,
        });
        self.events.push(PropEvent::Switch { id, flag, on });
        self.set_flag(flag, on);
        true
    }

    /// One 30 Hz tick: floor switches feel Link's weight (`standing` = he is on the ground at `player`), webs finish burning,
    /// timed torches burn down.
    pub fn step(&mut self, player: Vec3, standing: bool) {
        self.clock += 1;
        for i in 0..self.list.len() {
            let p = &mut self.list[i];
            match p.state {
                State::Burning(t) => p.state = if t <= 1.0 { State::Gone } else { State::Burning(t - 1.0) },
                State::Lit(t) if t.is_finite() => {
                    if t <= 1.0 {
                        p.state = State::Intact;
                        let (pos, flag) = (p.pos, p.flag);
                        self.events.push(PropEvent::Torch { id: i, pos, lit: false });
                        self.events.push(PropEvent::Sfx { name: "EV_BURN_OUT", pos });
                        if flag >= 0 {
                            self.set_flag(flag, false);
                            self.events.push(PropEvent::Switch { id: i, flag, on: false });
                        }
                    } else {
                        p.state = State::Lit(t - 1.0);
                    }
                }
                _ => {}
            }
            let p = &self.list[i];
            if p.kind == Kind::FloorSwitch {
                let on_it = standing && (player[0] - p.pos[0]).hypot(player[2] - p.pos[2]) < p.kind.size().0 && (player[1] - p.pos[1]).abs() < 40.0;
                let sub = p.param;
                if on_it && p.state != State::On {
                    self.press(i, true);
                } else if !on_it && sub == 2 && p.state == State::On {
                    self.press(i, false);
                }
            }
        }
    }

    pub fn take_events(&mut self) -> Vec<PropEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn count(&self, kind: Kind) -> usize {
        self.list.iter().filter(|p| p.kind == kind).count()
    }
}

/// `ObjSyokudai`: a timed torch burns `count * 50 + 100` OoT frames.
pub fn torch_ticks(count: i32) -> f64 {
    (count as f64 * 50.0 + 100.0) * 1.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(kind: Kind, pos: Vec3) -> Prop {
        Prop::new(kind, pos)
    }

    fn world(list: Vec<Prop>) -> Props {
        Props::new(list)
    }

    #[test]
    fn grass_is_cut_by_blades_and_drops_from_its_table() {
        let mut g = prop(Kind::Grass, [0.0; 3]);
        g.table = Some(1);
        let mut w = world(vec![g]);
        assert!(!w.hit(0, Dmg::Nut), "a nut does not cut grass");
        assert!(w.hit(0, Dmg::KokiriSword));
        assert!(w.list[0].gone());
        let ev = w.take_events();
        assert!(ev.iter().any(|e| matches!(e, PropEvent::Broke { kind: Kind::Grass, .. })));
        assert!(ev.iter().any(|e| matches!(e, PropEvent::Drop { table: Some(1), .. })));
        assert!(ev.iter().any(|e| matches!(e, PropEvent::Sfx { name: "EV_PLANT_BROKEN", .. })));
        assert!(!w.hit(0, Dmg::KokiriSword), "cut once");
    }

    #[test]
    fn pots_drop_their_fixed_item_and_crates_need_a_strong_blow() {
        let mut pot = prop(Kind::Pot, [0.0; 3]);
        pot.item = Some(3);
        let mut w = world(vec![pot, prop(Kind::Crate, [500.0, 0.0, 0.0])]);
        assert!(w.hit(0, Dmg::Arrow));
        assert!(w.take_events().iter().any(|e| matches!(e, PropEvent::Drop { item: Some(3), .. })));
        assert!(!w.hit(1, Dmg::KokiriSword), "a plain slash does not break a large crate");
        assert!(w.hit(1, Dmg::MasterSpin));
    }

    #[test]
    fn bombs_break_boulders_and_cracked_walls_and_set_their_switch_flag() {
        let mut b = prop(Kind::Boulder, [0.0; 3]);
        b.flag = 7;
        let mut wall = prop(Kind::CrackedWall, [1000.0, 0.0, 0.0]);
        wall.flag = 9;
        let mut far = prop(Kind::Boulder, [9000.0, 0.0, 0.0]);
        far.flag = 8;
        let mut w = world(vec![b, wall, far]);
        assert!(!w.hit(0, Dmg::KokiriSword) && !w.hit(0, Dmg::HammerSwing));
        assert_eq!(w.blast([500.0, 20.0, 0.0], 400.0), 2, "both within the blast, the far one not");
        assert!(w.flag(7) && w.flag(9) && !w.flag(8));
        assert!(w.list[2].state == State::Intact);
    }

    #[test]
    fn webs_burn_only_from_fire_and_set_their_flag() {
        let mut web = prop(Kind::FloorWeb, [0.0; 3]);
        web.flag = 3;
        let mut w = world(vec![web]);
        for d in [Dmg::KokiriSword, Dmg::Explosive, Dmg::Arrow, Dmg::Slingshot, Dmg::IceArrow] {
            assert!(!w.hit(0, d), "{d:?}");
        }
        assert!(w.hit(0, Dmg::FireArrow));
        assert!(w.flag(3));
        assert!(matches!(w.list[0].state, State::Burning(_)));
        for _ in 0..60 {
            w.step([5000.0, 0.0, 0.0], true);
        }
        assert!(w.list[0].gone(), "the web is gone after its burn time");
        assert!(w.take_events().iter().any(|e| matches!(e, PropEvent::Burned { .. })));
    }

    #[test]
    fn a_burning_stick_lights_a_torch_that_lights_the_next_stick() {
        let mut t = prop(Kind::Torch, [0.0; 3]);
        t.flag = 5;
        let mut w = world(vec![t]);
        let stick_tip = [10.0, 80.0, 0.0];
        assert!(!w.flame_near(&stick_tip, STICK_LIGHT_RANGE), "an unlit torch lights nothing");
        assert_eq!(w.fire(stick_tip, STICK_LIGHT_RANGE), 1, "a flame from a fire arrow, Din's or a stick lights it");
        assert!(w.list[0].is_lit() && w.flag(5));
        assert!(w.flame_near(&[20.0, 90.0, 0.0], STICK_LIGHT_RANGE), "a wooden torch keeps burning");
        for _ in 0..5000 {
            w.step([9000.0, 0.0, 0.0], true);
        }
        assert!(w.list[0].is_lit(), "wooden torches stay lit");
    }

    #[test]
    fn timed_torches_burn_down_and_clear_their_flag() {
        let mut t = prop(Kind::Torch, [0.0; 3]);
        t.timed = true;
        t.param = 2;
        t.flag = 4;
        let mut w = world(vec![t]);
        assert!(w.hit(0, Dmg::FireArrow));
        assert!(w.flag(4));
        let ticks = torch_ticks(2) as usize;
        for _ in 0..ticks - 3 {
            w.step([9000.0, 0.0, 0.0], true);
        }
        assert!(w.list[0].is_lit());
        for _ in 0..6 {
            w.step([9000.0, 0.0, 0.0], true);
        }
        assert!(!w.list[0].is_lit() && !w.flag(4), "{:?}", w.list[0].state);
    }

    #[test]
    fn eye_and_crystal_switches_react_to_the_right_items() {
        let mut eye = prop(Kind::EyeSwitch, [0.0; 3]);
        eye.flag = 1;
        let mut crystal = prop(Kind::CrystalSwitch, [300.0, 0.0, 0.0]);
        crystal.flag = 2;
        crystal.param = 1;
        let mut w = world(vec![eye, crystal]);
        assert!(!w.hit(0, Dmg::KokiriSword), "an eye switch ignores the sword");
        assert!(w.hit(0, Dmg::Slingshot) && w.flag(1));
        assert!(w.hit(1, Dmg::KokiriSword) && w.flag(2));
        assert!(w.hit(1, Dmg::Boomerang) && !w.flag(2), "a toggling crystal turns off again");
        assert!(w.targets().iter().all(|t| t.kind == TargetKind::Switch));
    }

    #[test]
    fn floor_switches_feel_weight_and_the_hammer_and_rusty_ones_only_the_hammer() {
        let mut f = prop(Kind::FloorSwitch, [0.0; 3]);
        f.flag = 6;
        f.param = 2;
        let mut r = prop(Kind::RustySwitch, [1000.0, 0.0, 0.0]);
        r.flag = 7;
        let mut w = world(vec![f, r]);
        w.step([1000.0, 0.0, 0.0], true);
        assert!(!w.flag(6) && !w.flag(7), "standing on the rusty switch does nothing");
        w.step([5.0, 0.0, 5.0], true);
        assert!(w.flag(6));
        w.step([500.0, 0.0, 0.0], true);
        assert!(!w.flag(6), "a hold switch releases when Link steps off");
        assert!(!w.hit(1, Dmg::KokiriSword) && w.hit(1, Dmg::HammerSwing) && w.flag(7));
    }
}
