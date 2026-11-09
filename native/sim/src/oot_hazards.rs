//! Dungeon hazards of the OoT scenes, placed from the actor lists: icicles (`Bg_Ice_Turara`) and fire walls
//! (`Bg_Hidan_Firewall`). Engine-free; the app draws them and applies the damage they report.
//!
//! * Icicle (z_bg_ice_turara.c): hangs until Link is within 60 units (`xzDistToPlayer < 60`) underneath, then falls with
//!   gravity -3 and shatters on the floor; its AT collider hurts for 0x04 (a quarter heart) while it falls.
//! * Shadow Temple traps (`Bg_Haka_Trap`: guillotine, spiked crusher, spiked walls, fan blade): every kind has an AT collider of
//!   0x04; the moving ones are dangerous for part of their cycle, the fan blade always.
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

/// `Bg_Haka_Trap` types (`params & 0xFF`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrapKind {
    Guillotine,
    GuillotineFast,
    SpikedCrusher,
    SpikedWall,
    FanBlade,
}

impl TrapKind {
    pub fn from_params(p: i32) -> Option<TrapKind> {
        Some(match p & 0xFF {
            0 => TrapKind::Guillotine,
            5 => TrapKind::GuillotineFast,
            1 => TrapKind::SpikedCrusher,
            2 | 3 => TrapKind::SpikedWall,
            4 => TrapKind::FanBlade,
            _ => return None,
        })
    }

    /// `(cycle ticks, dangerous ticks at the start of the cycle, reach in OoT units)`.
    fn cycle(self) -> (u32, u32, f64) {
        match self {
            TrapKind::Guillotine => (90, 24, 45.0),
            TrapKind::GuillotineFast => (60, 18, 45.0),
            TrapKind::SpikedCrusher => (120, 36, 60.0),
            TrapKind::SpikedWall => (150, 60, 70.0),
            TrapKind::FanBlade => (1, 1, 85.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Trap {
    pub pos: Vec3,
    pub kind: TrapKind,
    /// Starting offset in the cycle (ticks), so the traps of a room are out of step.
    pub phase: u32,
    cooldown: f64,
}

impl Trap {
    pub fn new(pos: Vec3, kind: TrapKind, phase: u32) -> Self {
        Self { pos, kind, phase, cooldown: 0.0 }
    }

    /// Is it dangerous at tick `t`?
    pub fn deadly(&self, t: u64) -> bool {
        let (cycle, on, _) = self.kind.cycle();
        ((t + self.phase as u64) % cycle as u64) < on as u64
    }
}

#[derive(Clone, Debug, Default)]
pub struct Hazards {
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
            let reach = t.kind.cycle().2 * U;
            let gap = (player[0] - t.pos[0]).hypot(player[2] - t.pos[2]);
            if t.deadly(clock) && gap < reach && (player[1] - t.pos[1]).abs() < 120.0 * U && t.cooldown <= 0.0 {
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
    fn shadow_temple_traps_are_dangerous_only_part_of_their_cycle_and_the_fan_always() {
        assert_eq!(TrapKind::from_params(0x105), Some(TrapKind::GuillotineFast));
        assert_eq!(TrapKind::from_params(9), None);
        let g = Trap::new([0.0; 3], TrapKind::Guillotine, 0);
        assert!(g.deadly(0) && g.deadly(23) && !g.deadly(24) && !g.deadly(89) && g.deadly(90));
        let mut h = Hazards { traps: vec![Trap::new([0.0; 3], TrapKind::Guillotine, 0), Trap::new([2000.0, 0.0, 0.0], TrapKind::FanBlade, 0)], ..Default::default() };
        let near_guillotine = (0..269).flat_map(|_| h.step([0.0, 0.0, 100.0], false, &FLOOR)).filter(|e| matches!(e, HazardEvent::Hurt { .. })).count();
        assert_eq!(near_guillotine, 3, "once per 90-tick cycle");
        let mut h = Hazards { traps: vec![Trap::new([2000.0, 0.0, 0.0], TrapKind::FanBlade, 0)], ..Default::default() };
        let fan = (0..60).flat_map(|_| h.step([2000.0, 0.0, 100.0], false, &FLOOR)).filter(|e| matches!(e, HazardEvent::Hurt { .. })).count();
        assert_eq!(fan, 3, "the fan cuts every 20 ticks");
    }
}
