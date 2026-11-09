//! Dungeon hazards of the OoT scenes, placed from the actor lists: icicles (`Bg_Ice_Turara`) and fire walls
//! (`Bg_Hidan_Firewall`). Engine-free; the app draws them and applies the damage they report.
//!
//! * Icicle (z_bg_ice_turara.c): hangs until Link is within 60 units (`xzDistToPlayer < 60`) underneath, then falls with
//!   gravity -3 and shatters on the floor; its AT collider hurts for 0x04 (a quarter heart) while it falls.
//! * Shadow Temple traps (`Bg_Haka_Trap`): guillotines, the spiked crusher and the spiked walls hurt for 0x04 (a quarter heart);
//!   the timings are the actor's own frame counts (see [`TrapKind`]); the fan blade only pushes Link.
//! * Fire wall (z_bg_hidan_firewall.c): a flame column that burns (AT fire 0x04) anything within 100 x 120 units of it.
use crate::math::Vec3;

pub const U: f64 = 4.4;
/// Quarter heart in 1/16 hearts.
pub const HIT_UNITS: i32 = 4;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IcicleState {
    Hanging,
    /// Fall speed (world units per tick, negative).
    Falling(f64),
    Broken,
}

#[derive(Clone, Debug)]
pub struct Icicle {
    pub pos: Vec3,
    pub home_y: f64,
    pub state: IcicleState,
    hit_done: bool,
}

#[derive(Clone, Debug)]
pub struct FireWall {
    pub pos: Vec3,
    pub yaw: f64,
    cooldown: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HazardEvent {
    /// Icicle `i` started to fall / shattered.
    IcicleDropped(usize),
    IcicleShattered(usize),
    /// Link takes `units` (1/16 hearts) from hazard `i` (icicles first, then fire walls by index + offset).
    Hurt { units: i32 },
}

/// `Bg_Haka_Trap` types (`params & 0xFF`). The fan blade (4) only pushes Link and does no damage, so it is not a hazard here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrapKind {
    Guillotine,
    GuillotineFast,
    SpikedCrusher,
    /// East (`params` 2) / west (3) wall; closes in at 0.5 units per frame over 200 units, then crushes.
    SpikedWall { east: bool },
}

impl TrapKind {
    pub fn from_params(p: i32) -> Option<TrapKind> {
        Some(match p & 0xFF {
            0 => TrapKind::Guillotine,
            5 => TrapKind::GuillotineFast,
            1 => TrapKind::SpikedCrusher,
            2 => TrapKind::SpikedWall { east: true },
            3 => TrapKind::SpikedWall { east: false },
            _ => return None,
        })
    }

    /// `(cycle, dangerous ticks at its start)` in 30 Hz ticks, from the actor's 20 Hz frame counts (`z_bg_haka_trap.c`):
    /// * guillotine: `Fall` runs a 20-frame timer with the fall speed doubling each frame from 0.1 (185 units are covered in about
    ///   11 frames, and the AT collider is on until the blade has stopped), then `Lift` 40 frames: 60 frames = 90 ticks,
    ///   dangerous 11 frames = 16 ticks;
    /// * fast guillotine: speed x3 per frame (about 7 frames), lift 10 frames: 30 frames = 45 ticks, dangerous 10 ticks;
    /// * spiked crusher: fall 30 frames at 0.5 x 1.6 per frame (about 9 frames to the floor), lift 30 frames: 90 ticks, dangerous
    ///   14 ticks (the weight of the block when it lands).
    fn cycle(self) -> (u32, u32) {
        match self {
            TrapKind::Guillotine => (90, 16),
            TrapKind::GuillotineFast => (45, 10),
            TrapKind::SpikedCrusher => (90, 14),
            TrapKind::SpikedWall { .. } => (1, 1),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Trap {
    pub pos: Vec3,
    pub kind: TrapKind,
    /// Starting offset in the cycle (ticks); the real traps start together (their timers are set in `Init`), so 0 unless the room
    /// is known to stagger them.
    pub phase: u32,
    /// Degrees about Y.
    pub yaw: f64,
    cooldown: f64,
}

impl Trap {
    pub fn new(pos: Vec3, kind: TrapKind, phase: u32) -> Self {
        Self { pos, kind, phase, yaw: 0.0, cooldown: 0.0 }
    }

    pub fn with_yaw(mut self, yaw: f64) -> Self {
        self.yaw = yaw;
        self
    }

    /// Is it dangerous at tick `t`?
    pub fn deadly(&self, t: u64) -> bool {
        let (cycle, on) = self.kind.cycle();
        ((t + self.phase as u64) % cycle as u64) < on as u64
    }

    /// Where a spiked wall is along its local x at tick `t`: it closes in 200 units at 0.5 per 20 Hz frame (600 ticks).
    pub fn wall_offset(&self, t: u64) -> f64 {
        match self.kind {
            TrapKind::SpikedWall { east } => (if east { -1.0 } else { 1.0 }) * 200.0 * U * (t as f64 / 600.0).min(1.0),
            _ => 0.0,
        }
    }

    /// Does the AT collider reach a player at `p`? (local x, z relative to the actor, `yaw` about Y.)
    pub fn reaches(&self, p: Vec3, t: u64) -> bool {
        let a = self.yaw.to_radians();
        let (dx, dz) = (p[0] - self.pos[0], p[2] - self.pos[2]);
        let (lx, lz) = (dx * a.cos() - dz * a.sin(), dx * a.sin() + dz * a.cos());
        match self.kind {
            // cylinder 30 x 90 following Link along the blade (x clamped to +-50) at z +-15
            TrapKind::Guillotine | TrapKind::GuillotineFast => lx.abs() < 80.0 * U && lz.abs() < 45.0 * U,
            TrapKind::SpikedCrusher => lx.abs() < 135.0 * U && lz.abs() < 100.0 * U,
            // cylinder 18 x 115 on the wall face, following Link along it (z clamped to +-162)
            TrapKind::SpikedWall { .. } => (lx - self.wall_offset(t)).abs() < 55.0 * U && lz.abs() < 217.0 * U,
        }
    }
}

/// `Bg_Hidan_Curtain` (z_bg_hidan_curtain.c): a wall of fire (AT fire 0x04) in a cylinder of `radius` x `height` that sinks into
/// the floor when its switch is set (types 1-3, 5) or its room is cleared (types 0, 6), and rises again after a timer.
/// `type` = `params >> 12 & 0xF`, switch flag = `params & 0x3F`; types 2 and 4 are the small size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CurtainMode {
    WaitSwitch,
    WaitClear,
    Lowering,
    /// Ticks left down.
    Down(f64),
    Raising,
    /// Type 5: stays down until the switch is off again.
    WaitSwitchOff,
    Gone,
}

#[derive(Clone, Debug)]
pub struct FireCurtain {
    pub pos: Vec3,
    pub kind: u8,
    pub flag: u8,
    /// Height above the sunk position, world units (0 = fully down).
    pub rise: f64,
    pub mode: CurtainMode,
    cooldown: f64,
}

impl FireCurtain {
    pub fn new(pos: Vec3, params: i32) -> Option<FireCurtain> {
        let kind = ((params >> 12) & 0xF) as u8;
        if kind > 6 {
            return None;
        }
        let mut c = FireCurtain { pos, kind, flag: (params & 0x3F) as u8, rise: 0.0, mode: CurtainMode::WaitSwitch, cooldown: 0.0 };
        c.rise = c.rise_dist();
        if kind == 0 || kind == 6 {
            c.mode = CurtainMode::WaitClear;
        }
        if kind == 4 || kind == 5 {
            // `world.pos.y = home.y - riseDist`: they start sunk
            c.rise = 0.0;
        }
        Some(c)
    }

    fn small(&self) -> bool {
        self.kind == 2 || self.kind == 4
    }

    /// `(radius, height, riseDist, riseSpeed per 20 Hz frame)` from `sHCParams`.
    pub fn dims(&self) -> (f64, f64, f64, f64) {
        if self.small() {
            (46.0, 88.0, 88.0, 3.0)
        } else {
            (81.0, 144.0, 144.0, 5.0)
        }
    }

    fn rise_dist(&self) -> f64 {
        self.dims().2 * U
    }

    /// Ticks a lowered curtain waits before rising again (`WaitForTimer`, 20 Hz frames x 1.5): type 1 300, 2 400, 3 160, 4 200.
    fn wait_ticks(&self) -> f64 {
        1.5 * match self.kind {
            2 => 400.0,
            4 => 200.0,
            3 => 160.0,
            _ => 300.0,
        }
    }

    pub fn up(&self) -> bool {
        self.rise >= self.rise_dist() - 1e-6
    }

    pub fn gone(&self) -> bool {
        self.mode == CurtainMode::Gone
    }

    fn step(&mut self, switch: &dyn Fn(u8) -> bool, cleared: bool) {
        let speed = self.dims().3 * U * (20.0 / 30.0);
        let full = self.rise_dist();
        match self.mode {
            CurtainMode::WaitSwitch => {
                if switch(self.flag) {
                    // types 1 and 3 wait out a short cutscene first (50 / 30 frames); the lowering then follows
                    self.mode = CurtainMode::Lowering;
                }
            }
            CurtainMode::WaitClear => {
                if cleared {
                    self.mode = CurtainMode::Lowering;
                }
            }
            CurtainMode::Lowering => {
                self.rise = (self.rise - speed).max(0.0);
                if self.rise <= 0.0 {
                    self.mode = match self.kind {
                        0 | 6 => CurtainMode::Gone,
                        5 => CurtainMode::WaitSwitchOff,
                        _ => CurtainMode::Down(self.wait_ticks()),
                    };
                }
            }
            CurtainMode::Down(t) => {
                if t <= 1.0 {
                    self.mode = CurtainMode::Raising;
                } else {
                    self.mode = CurtainMode::Down(t - 1.0);
                }
            }
            CurtainMode::WaitSwitchOff => {
                if !switch(self.flag) {
                    self.mode = CurtainMode::Raising;
                }
            }
            CurtainMode::Raising => {
                self.rise = (self.rise + speed).min(full);
                if self.rise >= full {
                    self.mode = CurtainMode::WaitSwitch;
                }
            }
            CurtainMode::Gone => {}
        }
    }

    fn burns(&self, player: Vec3) -> bool {
        let (r, h, ..) = self.dims();
        let top = self.pos[1] - self.rise_dist() + self.rise;
        !self.gone() && (player[0] - self.pos[0]).hypot(player[2] - self.pos[2]) < (r + 18.0) * U && player[1] < top + h * U && player[1] + 60.0 * U > top && self.rise > 0.2 * self.rise_dist()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Hazards {
    pub curtains: Vec<FireCurtain>,
    pub icicles: Vec<Icicle>,
    pub walls: Vec<FireWall>,
    pub traps: Vec<Trap>,
    pub clock: u64,
}

impl Icicle {
    pub fn new(pos: Vec3) -> Self {
        Self { pos, home_y: pos[1], state: IcicleState::Hanging, hit_done: false }
    }
}

impl FireWall {
    pub fn new(pos: Vec3, yaw: f64) -> Self {
        Self { pos, yaw, cooldown: 0.0 }
    }
}

impl Hazards {
    /// A weapon blow on icicle `i` knocks it loose / breaks it.
    pub fn strike_icicle(&mut self, i: usize) -> bool {
        match self.icicles.get_mut(i) {
            Some(ic) if ic.state == IcicleState::Hanging => {
                ic.state = IcicleState::Falling(0.0);
                true
            }
            _ => false,
        }
    }

    /// One 30 Hz tick of the fire curtains: `switch(flag)` reads the switch flags, `cleared` says the enemies of the area are dead.
    pub fn step_curtains(&mut self, player: Vec3, fire_immune: bool, switch: &dyn Fn(u8) -> bool, cleared: bool) -> Vec<HazardEvent> {
        let mut ev = vec![];
        for c in self.curtains.iter_mut() {
            c.step(switch, cleared);
            c.cooldown = (c.cooldown - 1.0).max(0.0);
            if !fire_immune && c.cooldown <= 0.0 && c.burns(player) {
                c.cooldown = 20.0;
                ev.push(HazardEvent::Hurt { units: HIT_UNITS });
            }
        }
        ev
    }

    /// One 30 Hz tick. `floor(pos)` is the floor height under a position; `fire_immune` is the Goron Tunic.
    pub fn step(&mut self, player: Vec3, fire_immune: bool, floor: &dyn Fn(Vec3) -> Option<f64>) -> Vec<HazardEvent> {
        let mut ev = vec![];
        for (i, ic) in self.icicles.iter_mut().enumerate() {
            match ic.state {
                IcicleState::Hanging => {
                    let gap = (player[0] - ic.pos[0]).hypot(player[2] - ic.pos[2]);
                    if gap < 60.0 * U && player[1] < ic.pos[1] {
                        ic.state = IcicleState::Falling(0.0);
                        ev.push(HazardEvent::IcicleDropped(i));
                    }
                }
                IcicleState::Falling(vy) => {
                    let vy = vy - 3.0 * U * 0.45;
                    ic.pos[1] += vy;
                    ic.state = IcicleState::Falling(vy);
                    let gap = (player[0] - ic.pos[0]).hypot(player[2] - ic.pos[2]);
                    if !ic.hit_done && gap < 25.0 * U && ic.pos[1] > player[1] && ic.pos[1] < player[1] + 160.0 {
                        ic.hit_done = true;
                        ev.push(HazardEvent::Hurt { units: HIT_UNITS });
                    }
                    let ground = floor(ic.pos).unwrap_or(f64::NEG_INFINITY);
                    if ic.pos[1] <= ground + 5.0 * U {
                        ic.state = IcicleState::Broken;
                        ev.push(HazardEvent::IcicleShattered(i));
                    }
                }
                IcicleState::Broken => {}
            }
        }
        self.clock += 1;
        let clock = self.clock;
        for t in self.traps.iter_mut() {
            t.cooldown = (t.cooldown - 1.0).max(0.0);
            let always = matches!(t.kind, TrapKind::SpikedWall { .. });
            if (always || t.deadly(clock)) && t.reaches(player, clock) && (player[1] - t.pos[1]).abs() < 120.0 * U && t.cooldown <= 0.0 {
                t.cooldown = (t.kind.cycle().1 as f64).max(20.0);
                ev.push(HazardEvent::Hurt { units: HIT_UNITS });
            }
        }
        for w in self.walls.iter_mut() {
            w.cooldown = (w.cooldown - 1.0).max(0.0);
            let a = w.yaw.to_radians();
            let (dx, dz) = (player[0] - w.pos[0], player[2] - w.pos[2]);
            let (lx, lz) = (dx * a.cos() - dz * a.sin(), dx * a.sin() + dz * a.cos());
            if lx.abs() < 100.0 * U && lz.abs() < 120.0 * U && (player[1] - w.pos[1]).abs() < 150.0 * U && w.cooldown <= 0.0 && !fire_immune {
                w.cooldown = 20.0;
                ev.push(HazardEvent::Hurt { units: HIT_UNITS });
            }
        }
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOOR: fn(Vec3) -> Option<f64> = |_| Some(0.0);

    #[test]
    fn an_icicle_falls_when_link_is_underneath_hurts_once_and_shatters() {
        let mut h = Hazards { icicles: vec![Icicle::new([0.0, 600.0, 0.0])], ..Default::default() };
        assert!(h.step([3000.0, 0.0, 0.0], false, &FLOOR).is_empty(), "far away it hangs");
        assert_eq!(h.icicles[0].state, IcicleState::Hanging);
        let mut events = vec![];
        for _ in 0..200 {
            events.extend(h.step([0.0, 0.0, 50.0], false, &FLOOR));
        }
        assert!(events.contains(&HazardEvent::IcicleDropped(0)));
        assert_eq!(events.iter().filter(|e| matches!(e, HazardEvent::Hurt { .. })).count(), 1, "one hit per fall");
        assert!(events.contains(&HazardEvent::IcicleShattered(0)));
        assert_eq!(h.icicles[0].state, IcicleState::Broken);
    }

    #[test]
    fn a_blow_knocks_an_icicle_loose_and_a_fire_wall_burns_unless_the_goron_tunic_is_worn() {
        let mut h = Hazards { icicles: vec![Icicle::new([0.0, 600.0, 0.0])], walls: vec![FireWall::new([1000.0, 0.0, 0.0], 0.0)], ..Default::default() };
        assert!(h.strike_icicle(0) && !h.strike_icicle(0));
        let burn = |h: &mut Hazards, tunic: bool| (0..60).flat_map(|_| h.step([1000.0, 0.0, 100.0], tunic, &FLOOR)).filter(|e| matches!(e, HazardEvent::Hurt { .. })).count();
        assert_eq!(burn(&mut h, false), 3, "every 20 ticks for 60 ticks");
        let mut h2 = Hazards { walls: vec![FireWall::new([1000.0, 0.0, 0.0], 0.0)], ..Default::default() };
        assert_eq!(burn(&mut h2, true), 0);
        let out_of_reach = (0..60).flat_map(|_| h2.step([5000.0, 0.0, 0.0], false, &FLOOR)).count();
        assert_eq!(out_of_reach, 0);
    }

    #[test]
    fn a_fire_curtain_burns_until_its_switch_sinks_it_and_rises_again_after_its_timer() {
        // type 1 (switch 5, large): params = 1 << 12 | 5
        let c = FireCurtain::new([0.0; 3], (1 << 12) | 5).unwrap();
        assert!(c.up() && c.kind == 1 && c.flag == 5);
        assert!(FireCurtain::new([0.0; 3], 7 << 12).is_none());
        let mut h = Hazards { curtains: vec![c], ..Default::default() };
        let off = |_: u8| false;
        let on = |f: u8| f == 5;
        let burn = |h: &mut Hazards, sw: &dyn Fn(u8) -> bool, tunic: bool, n: usize| (0..n).flat_map(|_| h.step_curtains([0.0, 0.0, 0.0], tunic, sw, false)).filter(|e| matches!(e, HazardEvent::Hurt { .. })).count();
        assert_eq!(burn(&mut h, &off, false, 60), 3, "every 20 ticks while it burns");
        assert_eq!(burn(&mut h, &off, true, 60), 0, "the Goron Tunic");
        // the switch sinks it (144 units at 5 per 20 Hz frame = about 29 frames = 43 ticks), then it stops burning
        burn(&mut h, &on, false, 60);
        assert!(matches!(h.curtains[0].mode, CurtainMode::Down(_)), "{:?}", h.curtains[0].mode);
        assert_eq!(burn(&mut h, &on, false, 200), 0);
        // it comes back after 300 frames (450 ticks)
        burn(&mut h, &off, false, 460);
        assert!(h.curtains[0].up() || h.curtains[0].mode == CurtainMode::Raising || h.curtains[0].mode == CurtainMode::WaitSwitch);
        assert!(burn(&mut h, &off, false, 120) >= 1, "burning again");
        // type 0 (room clear) goes for good
        let mut h = Hazards { curtains: vec![FireCurtain::new([0.0; 3], 0).unwrap()], ..Default::default() };
        (0..120).for_each(|_| {
            h.step_curtains([5000.0, 0.0, 0.0], false, &off, true);
        });
        assert!(h.curtains[0].gone());
    }

    #[test]
    fn shadow_temple_traps_follow_the_actors_frame_counts() {
        assert_eq!(TrapKind::from_params(0x105), Some(TrapKind::GuillotineFast));
        assert_eq!(TrapKind::from_params(4), None, "the fan only pushes");
        assert_eq!(TrapKind::from_params(2), Some(TrapKind::SpikedWall { east: true }));
        let g = Trap::new([0.0; 3], TrapKind::Guillotine, 0);
        assert!(g.deadly(0) && g.deadly(15) && !g.deadly(16) && !g.deadly(89) && g.deadly(90));
        let f = Trap::new([0.0; 3], TrapKind::GuillotineFast, 0);
        assert!(f.deadly(9) && !f.deadly(10) && f.deadly(45), "30 frames = 45 ticks");
        // one hit per 90-tick cycle beside the guillotine
        let mut h = Hazards { traps: vec![Trap::new([0.0; 3], TrapKind::Guillotine, 0)], ..Default::default() };
        let hits = (0..269).flat_map(|_| h.step([0.0, 0.0, 100.0], false, &FLOOR)).filter(|e| matches!(e, HazardEvent::Hurt { .. })).count();
        assert_eq!(hits, 3);
        // out of its 45-unit depth it does nothing
        let mut h = Hazards { traps: vec![Trap::new([0.0; 3], TrapKind::Guillotine, 0)], ..Default::default() };
        assert_eq!((0..200).flat_map(|_| h.step([0.0, 0.0, 400.0], false, &FLOOR)).count(), 0);
        // a spiked wall closes in 200 units over 600 ticks and hurts whoever it touches
        let w = Trap::new([1000.0, 0.0, 0.0], TrapKind::SpikedWall { east: true }, 0);
        assert_eq!(w.wall_offset(0), 0.0);
        assert!((w.wall_offset(600) + 200.0 * U).abs() < 1e-9 && w.wall_offset(6000) == w.wall_offset(600));
        assert!(w.reaches([1000.0, 0.0, 0.0], 0) && !w.reaches([1000.0 - 100.0 * U, 0.0, 0.0], 0));
        assert!(w.reaches([1000.0 - 100.0 * U, 0.0, 0.0], 300), "half way in");
    }
}
