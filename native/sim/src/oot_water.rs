//! The Water Temple's water (`Bg_Mizu_Water`, z_bg_mizu_water.c): the main level, which the three Triforce crests move by
//! setting a switch flag, and the three small pools that follow a switch of their own.
//!
//! Engine-free. The actors run at 20 Hz: [`WaterTemple::tick`] executes two actor frames per three sim ticks. Heights are in
//! OoT units (the app multiplies by 4.4 for the water boxes of the collision world).
//!
//! The main level (type 0) works on the flags `WATER_TEMPLE_WATER_F1/F2/F3_FLAG` (0x1C / 0x1D / 0x1E): whichever was set last
//! becomes the target (`F1` first, then `F2`, then `F3`, skipping the level it is already at), the flag of the level it left is
//! cleared and the surface moves 5 units a frame. Entering the temple with no flag set starts at the top (765) and then, because
//! `Init` leaves `F1` set, drains to the lowest (-15): that is what the game does.
use std::cell::Cell;

pub const F3_Y: f64 = 765.0;
pub const F2_Y: f64 = 445.0;
pub const F1_Y: f64 = -15.0;
pub const B1_Y: f64 = -835.0;
pub const F3_FLAG: i32 = 0x1E;
pub const F2_FLAG: i32 = 0x1D;
pub const F1_FLAG: i32 = 0x1C;

/// `sWaterLevels`: the switch flag of a level and its height relative to the top one.
const LEVELS: [(i32, f64); 4] = [(0x00, 0.0), (F3_FLAG, 0.0), (F2_FLAG, F2_Y - F3_Y), (F1_FLAG, F1_Y - F3_Y)];
/// `sWaterBoxIndices`: the water boxes of the scene the main level moves.
pub const MAIN_BOXES: [usize; 8] = [2, 3, 5, 7, 12, 20, 21, 22];
/// The boxes of the types 2, 3 and 4.
const POOL_BOXES: [usize; 3] = [6, 8, 16];
const POOL_RISE: [f64; 3] = [85.0, 110.0, 160.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Out {
    Set(i32),
    Unset(i32),
    /// A sound by `NA_SE_` name without the prefix.
    Sfx(&'static str),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    Wait,
    Change,
}

#[derive(Clone, Debug)]
pub struct Water {
    /// `type` (params low byte): 0 main, 1 nothing, 2 / 3 / 4 pools.
    pub kind: u8,
    pub switch_flag: i32,
    base_y: f64,
    target_y: f64,
    pub y: f64,
    /// Type 0: the flag of the level it is at (`actor.params`).
    params: i32,
    act: Act,
}

/// The switch flags with this tick's own changes overlaid (an actor sees what the one before it did).
struct Flags<'a> {
    base: &'a dyn Fn(i32) -> bool,
    set: Cell<u64>,
    unset: Cell<u64>,
    out: std::cell::RefCell<Vec<Out>>,
}

impl<'a> Flags<'a> {
    fn new(base: &'a dyn Fn(i32) -> bool) -> Self {
        Flags { base, set: Cell::new(0), unset: Cell::new(0), out: Default::default() }
    }

    fn get(&self, f: i32) -> bool {
        if !(0..64).contains(&f) {
            return false;
        }
        let bit = 1u64 << f;
        ((self.base)(f) || self.set.get() & bit != 0) && self.unset.get() & bit == 0
    }

    fn put(&self, f: i32) {
        if (0..64).contains(&f) {
            self.set.set(self.set.get() | 1 << f);
            self.unset.set(self.unset.get() & !(1 << f));
            self.out.borrow_mut().push(Out::Set(f));
        }
    }

    fn clear(&self, f: i32) {
        if (0..64).contains(&f) {
            self.unset.set(self.unset.get() | 1 << f);
            self.set.set(self.set.get() & !(1 << f));
            self.out.borrow_mut().push(Out::Unset(f));
        }
    }
}

/// `BgMizuWater_GetWaterLevelActionIndex`.
fn level_index(switch_flag: i32, flags: &Flags) -> usize {
    if flags.get(F1_FLAG) && switch_flag != F1_FLAG {
        3
    } else if flags.get(F2_FLAG) && switch_flag != F2_FLAG {
        2
    } else if flags.get(F3_FLAG) && switch_flag != F3_FLAG {
        1
    } else {
        0
    }
}

/// `Math_StepToF`: true when the value arrived.
fn step_to(value: &mut f64, target: f64, step: f64) -> bool {
    if *value == target {
        return true;
    }
    if *value < target {
        *value = (*value + step).min(target);
    } else {
        *value = (*value - step).max(target);
    }
    *value == target
}

impl Water {
    /// `params` is the actor's, `y` its placed height (OoT units).
    fn new(params: i32, y: f64, flags: &Flags) -> Self {
        let kind = (params & 0xFF) as u8;
        let switch_flag = (params >> 8) & 0xFF;
        let mut w = Water { kind, switch_flag, base_y: y, target_y: y, y, params: 0, act: Act::Wait };
        match kind {
            0 => {
                let idx = level_index(-1, flags);
                w.y = LEVELS[idx].1 + w.base_y;
                w.params = LEVELS[idx].0;
                flags.clear(F1_FLAG);
                flags.clear(F2_FLAG);
                flags.clear(F3_FLAG);
                match w.params {
                    F3_FLAG => flags.put(F3_FLAG),
                    F2_FLAG => flags.put(F2_FLAG),
                    _ => flags.put(F1_FLAG),
                }
                w.target_y = w.y;
            }
            2..=4 => {
                let rise = POOL_RISE[kind as usize - 2];
                if flags.get(switch_flag) {
                    w.y = w.base_y + rise;
                    if kind != 2 {
                        w.target_y = w.y;
                    }
                }
            }
            _ => {}
        }
        w
    }

    /// The part of `BgMizuWater_WaitForAction` / `ChangeWaterLevel` that picks the target.
    fn aim(&mut self, flags: &Flags) {
        match self.kind {
            0 => {
                let prev = self.params;
                let idx = level_index(self.params, flags);
                if idx != 0 && prev != LEVELS[idx].0 {
                    self.params = LEVELS[idx].0;
                    self.target_y = LEVELS[idx].1 + self.base_y;
                }
                if prev != self.params && prev != 0 {
                    flags.clear(prev);
                }
            }
            2..=4 => {
                let rise = POOL_RISE[self.kind as usize - 2];
                self.target_y = if flags.get(self.switch_flag) { self.base_y + rise } else { self.base_y };
            }
            _ => {}
        }
    }

    fn frame(&mut self, flags: &Flags) {
        match self.act {
            Act::Wait => {
                self.aim(flags);
                if self.target_y != self.y {
                    self.act = Act::Change;
                }
            }
            Act::Change => {
                self.aim(flags);
                let step = if self.kind == 0 { 5.0 } else { 1.0 };
                if self.kind != 1 && step_to(&mut self.y, self.target_y, step) {
                    self.act = Act::Wait;
                }
                if self.target_y != self.y {
                    flags.out.borrow_mut().push(Out::Sfx("EV_WATER_LEVEL_DOWN"));
                }
            }
        }
    }

    /// The water boxes this actor moves and where it holds them.
    fn boxes(&self) -> Vec<(usize, f64)> {
        match self.kind {
            0 => MAIN_BOXES.iter().map(|b| (*b, self.y)).collect(),
            2..=4 => vec![(POOL_BOXES[self.kind as usize - 2], self.y)],
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WaterTemple {
    pub actors: Vec<Water>,
    acc: f64,
}

impl WaterTemple {
    /// `placements`: `(params, placed height)` of every `Bg_Mizu_Water`. The main level exists once (the game has one loaded at a
    /// time); the first placement is it. Returns the flag changes `Init` makes.
    pub fn new(placements: &[(i32, f64)], flag: &dyn Fn(i32) -> bool) -> (Self, Vec<Out>) {
        let flags = Flags::new(flag);
        let mut actors: Vec<Water> = vec![];
        for (params, y) in placements {
            if params & 0xFF == 0 && actors.iter().any(|a| a.kind == 0) {
                continue;
            }
            actors.push(Water::new(*params, *y, &flags));
        }
        let out = flags.out.take();
        (WaterTemple { actors, acc: 0.0 }, out)
    }

    /// One 30 Hz tick: the flag changes and sounds of the actor frames that ran.
    pub fn tick(&mut self, flag: &dyn Fn(i32) -> bool) -> Vec<Out> {
        self.acc += 20.0 / 30.0;
        let flags = Flags::new(flag);
        while self.acc >= 1.0 {
            self.acc -= 1.0;
            for a in self.actors.iter_mut() {
                a.frame(&flags);
            }
        }
        flags.out.take()
    }

    /// `(water box index, ySurface in OoT units)` for every box the actors move.
    pub fn boxes(&self) -> Vec<(usize, f64)> {
        self.actors.iter().flat_map(Water::boxes).collect()
    }

    /// The height of the main level.
    pub fn level(&self) -> Option<f64> {
        self.actors.iter().find(|a| a.kind == 0).map(|a| a.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A little world of switch flags that applies the outputs.
    struct Flags2 {
        bits: RefCell<u64>,
    }

    impl Flags2 {
        fn new() -> Self {
            Flags2 { bits: RefCell::new(0) }
        }

        fn get(&self, f: i32) -> bool {
            *self.bits.borrow() >> f & 1 != 0
        }

        fn apply(&self, outs: &[Out]) {
            for o in outs {
                match o {
                    Out::Set(f) => *self.bits.borrow_mut() |= 1 << f,
                    Out::Unset(f) => *self.bits.borrow_mut() &= !(1 << f),
                    Out::Sfx(_) => {}
                }
            }
        }
    }

    fn run(w: &mut WaterTemple, f: &Flags2, ticks: usize) {
        for _ in 0..ticks {
            let outs = w.tick(&|x| f.get(x));
            f.apply(&outs);
        }
    }

    fn start() -> (WaterTemple, Flags2) {
        let f = Flags2::new();
        let (w, outs) = WaterTemple::new(&[(0xFF00, 765.0), (0xFF00, 765.0), (0x1A02, 100.0), (0x3103, 200.0), (0x1804, 300.0)], &|x| f.get(x));
        f.apply(&outs);
        (w, f)
    }

    #[test]
    fn the_temple_starts_at_the_top_and_drains_to_the_lowest_level_because_init_leaves_f1_set() {
        let (mut w, f) = start();
        assert_eq!(w.actors.iter().filter(|a| a.kind == 0).count(), 1, "one main level");
        assert_eq!(w.level(), Some(765.0));
        assert!(f.get(F1_FLAG) && !f.get(F2_FLAG) && !f.get(F3_FLAG), "Init sets F1");
        // 780 units at 5 a frame = 156 frames = 234 ticks
        run(&mut w, &f, 100);
        let mid = w.level().unwrap();
        assert!(mid < 765.0 && mid > F1_Y, "{mid}");
        run(&mut w, &f, 200);
        assert_eq!(w.level(), Some(F1_Y));
        assert!(f.get(F1_FLAG), "the flag of the level it is at stays set");
    }

    #[test]
    fn a_lullaby_at_a_crest_sets_a_flag_and_the_level_moves_there_clearing_the_one_it_left() {
        let (mut w, f) = start();
        run(&mut w, &f, 400);
        assert_eq!(w.level(), Some(F1_Y));
        // the middle crest: F2
        *f.bits.borrow_mut() |= 1 << F2_FLAG;
        run(&mut w, &f, 400);
        assert_eq!(w.level(), Some(F2_Y));
        assert!(!f.get(F1_FLAG), "the old flag is cleared");
        // and the top one
        *f.bits.borrow_mut() |= 1 << F3_FLAG;
        run(&mut w, &f, 400);
        assert_eq!(w.level(), Some(F3_Y));
        assert!(!f.get(F2_FLAG));
        // F1 is tested first, but with F2 also left set the level retargets to F2 at once and settles there
        *f.bits.borrow_mut() |= (1 << F1_FLAG) | (1 << F2_FLAG);
        run(&mut w, &f, 400);
        assert_eq!(w.level(), Some(F2_Y));
        assert!(f.get(F2_FLAG) && !f.get(F1_FLAG) && !f.get(F3_FLAG));
    }

    #[test]
    fn the_main_level_moves_eight_boxes_and_each_pool_follows_its_own_switch() {
        let (mut w, f) = start();
        let boxes = w.boxes();
        for b in MAIN_BOXES {
            assert!(boxes.contains(&(b, 765.0)), "box {b}");
        }
        assert!(boxes.contains(&(6, 100.0)) && boxes.contains(&(8, 200.0)) && boxes.contains(&(16, 300.0)));
        // switch 0x1A raises the first pool 85 units at 1 a frame, and lowers it again when the switch goes off
        *f.bits.borrow_mut() |= 1 << 0x1A;
        run(&mut w, &f, 200);
        assert!(w.boxes().contains(&(6, 185.0)), "{:?}", w.boxes());
        *f.bits.borrow_mut() &= !(1 << 0x1A);
        run(&mut w, &f, 200);
        assert!(w.boxes().contains(&(6, 100.0)));
        // the others rise 110 and 160
        *f.bits.borrow_mut() |= (1 << 0x31) | (1 << 0x18);
        run(&mut w, &f, 400);
        assert!(w.boxes().contains(&(8, 310.0)) && w.boxes().contains(&(16, 460.0)), "{:?}", w.boxes());
    }

    #[test]
    fn a_pool_whose_switch_is_already_on_starts_risen() {
        let f = Flags2::new();
        *f.bits.borrow_mut() |= 1 << 0x1A;
        let (w, _) = WaterTemple::new(&[(0x1A02, 100.0)], &|x| f.get(x));
        assert!(w.boxes().contains(&(6, 185.0)));
    }
}
