//! The smoke and spark sprites MK64 hangs on every kart, simulated like the game does: ring pools of 10 slots per
//! effect, ticked at 30 Hz, where a slot may only start once the previous slot has lived a tick. Engine-free; the
//! positions are re-evaluated every rendered frame from the kart's current position so the puffs stay glued to it.
//!
//! Decomp sources (`src/code_80057C60.c`), all in course units (8 sim units):
//! * exhaust, pool 0 type 1: spawn `func_80060504`, update `func_80062C74`, draw `func_8006538C`
//! * drift smoke, pool 1 type 1 (`DRIFT_PARTICLE`): `set_drift_particles`, `func_80063408`, `render_player_drift_particles`
//! * tyre grass, pool 1 type 3 (`GRASS_PARTICLE`): `setup_tyre_particles`, `func_80063BD4`, `render_player_ground_particles`
//! * mushroom boost sparks, pool 3 type 8: `func_800621BC`, `func_80064C74`, `render_player_boost_spark_particles`

use super::kart_physics::TOP_SPEED;

/// The particle pools run once per game tick.
pub const TICK: f64 = 1. / 30.;
const SLOTS: usize = 10;
const UNIT: f64 = 8.;
/// Every particle quad is 4 course units across at scale 1 (`D_0D008E48`: x, y = -2..2).
pub const QUAD: f64 = 4. * UNIT;

/// `sp48[characterId]` of `func_80062C74`: tailpipe height over the ground per racer (Mario, Luigi, Yoshi, Toad, DK, Wario,
/// Peach, Bowser).
pub const EXHAUST_HEIGHT: [f64; 8] = [4.5, 4.5, 4.5, 4.5, 4.5, 5.5, 4.5, 6.5];

/// What a kart is doing this frame (sim units; `pos` is the kart's feet).
#[derive(Clone, Copy, Debug, Default)]
pub struct PuffIn {
    pub pos: [f64; 3],
    /// Browser heading (`-rotation[1]`) in radians.
    pub yaw: f64,
    /// `unk_0C0` (s16 units).
    pub slip: i32,
    pub current_speed: f64,
    /// `speed / 18 * 216`.
    pub kmh: f64,
    pub character: usize,
    pub driving: bool,
    /// `THROTTLE`: A held.
    pub throttle: bool,
    pub mushroom: bool,
    /// The player's kart (`PLAYER_HUMAN`): only humans drift-smoke and shed boost sparks.
    pub human: bool,
    pub drifting: bool,
    pub hop: bool,
    pub midair: bool,
    /// Spun out, squished, held by Lakitu: the exhaust stops.
    pub disabled: bool,
    pub drift_state: i32,
    pub drift_duration: i32,
    /// Back left and back right tyres, sim units.
    pub tyres: [[f64; 3]; 2],
    pub tyre_grass: [bool; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PuffKind {
    /// `common_texture_particle_smoke_N` cycling through 1..3.
    Exhaust,
    /// 16 x 16 `gTexture69B03C`.
    DriftSmall,
    /// 32 x 32 `gTexture69B140`.
    DriftLarge,
    Grass,
    /// `common_texture_particle_spark`.
    Spark,
}

/// One particle to draw.
#[derive(Clone, Copy, Debug)]
pub struct Puff {
    pub kind: PuffKind,
    pub pos: [f64; 3],
    /// Quad edge, sim units.
    pub size: f64,
    pub color: u32,
    pub alpha: f64,
    pub variant: u8,
}

#[derive(Clone, Copy, Debug, Default)]
struct Slot {
    alive: bool,
    timer: u32,
    /// Heading when it started.
    yaw: f64,
    /// Started on the left side / `unk_010`.
    left: bool,
    /// `unk_040`: 0 with the throttle down, 1 without; drift tier.
    variant: i32,
    mushroom: bool,
    state: i32,
    index: usize,
}

#[derive(Clone, Debug, Default)]
struct KartPuffs {
    exhaust: [Slot; SLOTS],
    ground: [Slot; SLOTS],
    spark: [Slot; SLOTS],
}

#[derive(Debug)]
pub struct PuffSystem {
    karts: Vec<KartPuffs>,
    latest: Vec<PuffIn>,
    acc: f64,
    seed: u32,
}

impl Default for PuffSystem {
    fn default() -> Self {
        Self { karts: vec![], latest: vec![], acc: 0., seed: 0x1234567 }
    }
}

fn prev(i: usize) -> usize {
    (i + SLOTS - 1) % SLOTS
}

impl PuffSystem {
    /// `random_int(n)`: 0..n.
    fn rand(&mut self, n: u32) -> u32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed >> 8) % n
    }

    /// Advance by `dt` seconds with the kart states of this frame (index 0 = the player).
    pub fn update(&mut self, dt: f64, frames: &[PuffIn]) {
        if self.karts.len() != frames.len() {
            self.karts = vec![KartPuffs::default(); frames.len()];
        }
        self.latest = frames.to_vec();
        self.acc += dt;
        // A hitch (tab switch) must not replay seconds of ticks.
        self.acc = self.acc.min(4. * TICK);
        while self.acc >= TICK {
            self.acc -= TICK;
            for k in 0..frames.len() {
                self.tick(k, &frames[k]);
            }
        }
    }

    /// One game tick of the pools of kart `k`.
    pub fn tick(&mut self, k: usize, f: &PuffIn) {
        let mut p = std::mem::take(&mut self.karts[k]);
        if !f.driving {
            self.karts[k] = KartPuffs::default();
            return;
        }
        for i in 0..SLOTS {
            self.tick_exhaust(&mut p.exhaust, i, f);
            self.tick_ground(&mut p.ground, i, f);
            self.tick_spark(&mut p.spark, i, f);
        }
        self.karts[k] = p;
    }

    /// The previous slot allows a start: `(i == 0 && (prev.timer > 0 || !prev.alive)) || prev.timer > 0`.
    fn may_start(slots: &[Slot; SLOTS], i: usize, min_timer: u32) -> bool {
        let before = slots[prev(i)];
        if i == 0 && min_timer == 1 { before.timer >= 1 || !before.alive } else if i == 0 { before.timer >= min_timer || !before.alive } else { before.timer >= min_timer }
    }

    /// `func_8006CEC0` / `func_80062C74` / `func_80060504`.
    fn tick_exhaust(&mut self, slots: &mut [Slot; SLOTS], i: usize, f: &PuffIn) {
        if slots[i].alive {
            slots[i].timer += 1;
            if slots[i].timer == 12 {
                slots[i] = Slot::default();
            }
            return;
        }
        if f.disabled {
            return;
        }
        let r = self.rand(if f.throttle { 5 } else { 14 });
        if (1..=3).contains(&r) && Self::may_start(slots, i, 1) {
            slots[i] = Slot { alive: true, timer: 0, yaw: f.yaw, left: false, variant: if f.throttle { 0 } else { 1 }, mushroom: f.mushroom, state: 0, index: i };
        }
    }

    /// Pool 1: drift smoke while drifting, grass from the tyres otherwise.
    fn tick_ground(&mut self, slots: &mut [Slot; SLOTS], i: usize, f: &PuffIn) {
        if slots[i].alive {
            slots[i].timer += 1;
            if slots[i].timer == 8 || (f.disabled && slots[i].variant < 100) {
                slots[i] = Slot::default();
            }
            return;
        }
        if f.midair || f.hop || f.disabled {
            return;
        }
        let slip = f.slip / (0x10000 / 360);
        if f.drifting && f.human {
            if Self::may_start(slots, i, 3) && (slip >= 7 || slip < -6) {
                slots[i] = Slot { alive: true, timer: 0, yaw: f.yaw, left: f.slip >= 0, variant: 0, mushroom: false, state: f.drift_state.clamp(0, 2), index: i };
            }
            return;
        }
        // `setup_tyre_particles`: random_int(8), 0 and 4 pick the back left tyre, 2 and 6 the back right one.
        let r = self.rand(8);
        let side = match r {
            0 | 4 => Some(true),
            2 | 6 => Some(false),
            _ => None,
        };
        if let Some(left) = side {
            let grass = f.tyre_grass[if left { 0 } else { 1 }];
            if grass && f.kmh >= 10. && Self::may_start(slots, i, 1) {
                // variant 100: a grass particle (the drift smoke is 0).
                slots[i] = Slot { alive: true, timer: 0, yaw: f.yaw, left, variant: 100, mushroom: false, state: 0, index: i };
            }
        }
    }

    /// Pool 3 type 8: the yellow sparks that circle a kart under a mushroom.
    fn tick_spark(&mut self, slots: &mut [Slot; SLOTS], i: usize, f: &PuffIn) {
        if slots[i].alive {
            slots[i].timer += 1;
            if slots[i].timer == 10 {
                slots[i] = Slot::default();
            }
            return;
        }
        if !f.mushroom || !f.human || f.disabled {
            return;
        }
        if Self::may_start(slots, i, 1) {
            let left = self.rand(8) & 1 == 1;
            slots[i] = Slot { alive: true, timer: 0, yaw: f.yaw, left, variant: 0, mushroom: true, state: 0, index: i };
        }
    }

    /// The particles to draw now. `frac` of a tick has elapsed since the last tick.
    pub fn puffs(&self) -> Vec<Puff> {
        let frac = (self.acc / TICK).clamp(0., 1.);
        let mut out = vec![];
        for (p, f) in self.karts.iter().zip(&self.latest) {
            if !f.driving {
                continue;
            }
            for s in p.exhaust.iter().filter(|s| s.alive) {
                out.push(exhaust(s, f, s.timer as f64 + frac));
            }
            for s in p.ground.iter().filter(|s| s.alive) {
                out.push(if s.variant >= 100 { grass(s, f, s.timer as f64 + frac) } else { drift(s, f, s.timer as f64 + frac) });
            }
            for s in p.spark.iter().filter(|s| s.alive) {
                out.push(spark(s, f, s.timer as f64 + frac));
            }
        }
        out
    }

    pub fn alive(&self) -> usize {
        self.karts.iter().map(|p| p.exhaust.iter().chain(&p.ground).chain(&p.spark).filter(|s| s.alive).count()).sum()
    }
}

fn dir(yaw: f64) -> (f64, f64) {
    yaw.sin_cos()
}

/// Exhaust puff at age `t` ticks. `var_f6` is the drift of the puff away from the kart per tick.
pub fn exhaust(s: &Slot, f: &PuffIn, t: f64) -> Puff {
    let unk_098 = f.current_speed * f.current_speed / 25.;
    let per_tick = -(unk_098 / if s.variant == 0 { 5000. } else { 6000. } + 0.1);
    let behind = -5.5 + t * per_tick;
    let heading = s.yaw - f.slip as f64 * std::f64::consts::PI / 32768. / 2.;
    let (sin, cos) = dir(heading);
    let (grow, fade) = if s.variant == 0 { (0.07, 3.) } else { (0.1, 2.) };
    let mut scale = 0.5 + grow * t;
    if s.mushroom {
        scale += 0.06 * (t - 5.).max(0.);
    }
    let start = if s.mushroom { 128. } else { 112. };
    let alpha = (start - fade * (t - 2.).max(0.)).max(0.) / 255.;
    let height = EXHAUST_HEIGHT[f.character.min(7)] + 0.3 * t;
    Puff {
        kind: PuffKind::Exhaust,
        pos: [f.pos[0] + sin * behind * UNIT, f.pos[1] + height * UNIT, f.pos[2] + cos * behind * UNIT],
        size: QUAD * scale,
        // prim colour of `func_8006538C`: near-white, or yellow while boosting
        color: if s.mushroom { 0xffc83c } else { 0xfbfffb },
        alpha,
        variant: (s.index as u32 + t as u32) as u8 % 3,
    }
}

/// The three tints of `render_player_drift_particles`: white, yellow, orange.
pub fn drift_colour(state: i32) -> u32 {
    match state {
        0 => 0xffffff,
        1 => 0xffff00,
        _ => 0xff9600,
    }
}

fn drift(s: &Slot, f: &PuffIn, t: f64) -> Puff {
    let tyre = f.tyres[if s.left { 0 } else { 1 }];
    let (sin, cos) = dir(s.yaw);
    let back = -7. * t * UNIT;
    Puff {
        kind: if f.drift_duration >= 50 { PuffKind::DriftLarge } else { PuffKind::DriftSmall },
        pos: [tyre[0] + sin * back, tyre[1] + (2. + t) * UNIT, tyre[2] + cos * back],
        size: QUAD * (0.35 + 0.08 * t),
        color: drift_colour(s.state),
        alpha: (112. - 16. * (t - 3.).max(0.)).max(0.) / 255.,
        variant: 0,
    }
}

fn grass(s: &Slot, f: &PuffIn, t: f64) -> Puff {
    let tyre = f.tyres[if s.left { 0 } else { 1 }];
    let (sin, cos) = dir(s.yaw);
    let back = -2. * t * UNIT;
    let shade = 255 - (s.index as u32 * 8).min(255);
    Puff {
        kind: PuffKind::Grass,
        pos: [tyre[0] + sin * back, tyre[1] + (0.5 + 0.2 * t) * UNIT, tyre[2] + cos * back],
        size: QUAD * (1. - 0.06 * t).max(0.),
        color: shade << 16 | shade << 8 | shade,
        alpha: 1.,
        variant: 0,
    }
}

fn spark(s: &Slot, f: &PuffIn, t: f64) -> Puff {
    let side = if s.left { 1. } else { -1. };
    let angle = s.yaw + side * 12f64.to_radians() * (t + 1.) - f.slip as f64 * std::f64::consts::PI / 32768.;
    let (sin, cos) = dir(angle);
    Puff {
        kind: PuffKind::Spark,
        pos: [f.pos[0] + sin * 5. * UNIT, f.pos[1] + 2. * UNIT, f.pos[2] + cos * 5. * UNIT],
        size: QUAD * (1. + 0.4 * t),
        color: 0xffe880,
        alpha: 96. / 255.,
        variant: (s.index % 4) as u8,
    }
}

/// How hard the kart is going (for tests and for callers without the physics at hand).
pub fn speed_ratio(f: &PuffIn) -> f64 {
    f.current_speed / TOP_SPEED
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kart() -> PuffIn {
        PuffIn { driving: true, throttle: true, current_speed: 300., kmh: 100., human: true, tyres: [[40., 0., -20.], [-40., 0., -20.]], ..Default::default() }
    }

    fn run(sys: &mut PuffSystem, f: PuffIn, ticks: usize) {
        for _ in 0..ticks {
            sys.update(TICK, &[f]);
        }
    }

    #[test]
    fn a_kart_under_throttle_keeps_up_to_ten_exhaust_puffs_alive_each_for_twelve_ticks() {
        let mut sys = PuffSystem::default();
        run(&mut sys, kart(), 90);
        let alive = sys.puffs().len();
        assert!((6..=10).contains(&alive), "{alive} puffs");
        // none outlives 12 ticks
        assert!(sys.karts[0].exhaust.iter().all(|s| s.timer < 12));
    }

    #[test]
    fn coasting_emits_far_fewer_puffs_than_the_throttle() {
        let (mut on, mut off) = (PuffSystem::default(), PuffSystem::default());
        let (mut n_on, mut n_off) = (0, 0);
        for _ in 0..300 {
            on.update(TICK, &[kart()]);
            off.update(TICK, &[PuffIn { throttle: false, ..kart() }]);
            n_on += on.alive();
            n_off += off.alive();
        }
        // 3 in 14 against 3 in 5 per tick, but the ring of ten slots saturates under the throttle
        assert!(n_off * 10 < n_on * 6, "{n_off} vs {n_on}");
    }

    #[test]
    fn exhaust_hangs_behind_the_tailpipe_at_the_kart_centre_and_rises_and_grows() {
        let f = PuffIn { pos: [1000., 0., 500.], yaw: 0., ..kart() };
        let s = Slot { alive: true, timer: 0, yaw: 0., variant: 0, ..Default::default() };
        let young = exhaust(&s, &f, 0.);
        let old = exhaust(&s, &f, 10.);
        // forward is +z at yaw 0: 5.5 course units = 44 sim units behind, 4.5 up, centred across
        assert!((young.pos[2] - (500. - 5.5 * 8.)).abs() < 1e-9 && (young.pos[0] - 1000.).abs() < 1e-9);
        assert!((young.pos[1] - 4.5 * 8.).abs() < 1e-9);
        assert!(old.pos[2] < young.pos[2] && old.pos[1] > young.pos[1] + 2. * 8.);
        assert!((young.size - 16.).abs() < 1e-9, "scale 0.5 of a 4 unit quad = 2 units = 16 sim units");
        assert!((old.size - 32. * 1.2).abs() < 1e-9);
        assert!((young.alpha - 112. / 255.).abs() < 1e-9 && old.alpha < young.alpha);
    }

    #[test]
    fn the_exhaust_turns_with_the_kart_heading_and_half_the_slip() {
        let f = PuffIn { yaw: std::f64::consts::FRAC_PI_2, ..kart() };
        let s = Slot { alive: true, yaw: std::f64::consts::FRAC_PI_2, ..Default::default() };
        let p = exhaust(&s, &f, 0.);
        assert!(p.pos[0] < -40. && p.pos[2].abs() < 1e-6, "behind a kart that faces +x is at -x: {:?}", p.pos);
        let drifting = PuffIn { slip: 0x1C70, ..f };
        let q = exhaust(&s, &drifting, 0.);
        assert!(q.pos[2].abs() > 1., "half the 40 degree slip moves the puff off the axis");
    }

    #[test]
    fn mushroom_exhaust_is_yellow_and_more_opaque() {
        let f = kart();
        let plain = exhaust(&Slot { alive: true, variant: 0, ..Default::default() }, &f, 0.);
        let boosted = exhaust(&Slot { alive: true, variant: 0, mushroom: true, ..Default::default() }, &f, 0.);
        assert!(boosted.alpha > plain.alpha && boosted.color != plain.color);
    }

    #[test]
    fn nothing_when_not_driving_or_disabled() {
        let mut sys = PuffSystem::default();
        run(&mut sys, PuffIn { driving: false, ..kart() }, 30);
        assert_eq!(sys.alive(), 0);
        run(&mut sys, PuffIn { disabled: true, ..kart() }, 30);
        assert_eq!(sys.karts[0].exhaust.iter().filter(|s| s.alive).count(), 0);
    }

    #[test]
    fn drift_smoke_comes_from_the_outside_tyre_every_three_ticks_and_takes_the_charge_colour() {
        let mut sys = PuffSystem::default();
        let f = PuffIn { drifting: true, slip: 20 * 182, drift_state: 1, ..kart() };
        run(&mut sys, f, 40);
        let smoke: Vec<_> = sys.puffs().into_iter().filter(|p| matches!(p.kind, PuffKind::DriftSmall | PuffKind::DriftLarge)).collect();
        // each slot restarts 3 ticks after its predecessor (8 tick life), so the ring fills with staggered puffs
        assert!((6..=10).contains(&smoke.len()), "{} drift puffs", smoke.len());
        let first = sys.karts[0].ground.iter().filter(|s| s.alive).map(|s| s.timer).collect::<Vec<_>>();
        assert!(first.iter().all(|t| *t < 8), "{first:?}");
        assert!(smoke.iter().all(|p| p.color == 0xffff00));
        assert_eq!(drift_colour(0), 0xffffff);
        assert_eq!(drift_colour(2), 0xff9600);
        // a positive slip uses the back left tyre (x = +40 here), then trails 7 course units per tick behind
        assert!(smoke.iter().all(|p| (p.pos[0] - 40.).abs() < 1e-6));
        // below 7 degrees of slip there is no smoke
        let mut small = PuffSystem::default();
        run(&mut small, PuffIn { slip: 3 * 182, ..f }, 40);
        assert!(small.puffs().iter().all(|p| !matches!(p.kind, PuffKind::DriftSmall | PuffKind::DriftLarge)));
    }

    #[test]
    fn a_long_drift_switches_to_the_big_smoke_texture() {
        let mut sys = PuffSystem::default();
        let f = PuffIn { drifting: true, slip: 20 * 182, drift_duration: 60, ..kart() };
        run(&mut sys, f, 20);
        assert!(sys.puffs().iter().any(|p| p.kind == PuffKind::DriftLarge));
    }

    #[test]
    fn grass_is_thrown_from_the_back_tyres_above_10_kmh_and_only_on_grass() {
        let mut sys = PuffSystem::default();
        run(&mut sys, PuffIn { tyre_grass: [true, true], ..kart() }, 60);
        assert!(sys.puffs().iter().any(|p| p.kind == PuffKind::Grass));
        let mut road = PuffSystem::default();
        run(&mut road, kart(), 60);
        assert!(road.puffs().iter().all(|p| p.kind != PuffKind::Grass), "no dust on asphalt");
        let mut slow = PuffSystem::default();
        run(&mut slow, PuffIn { tyre_grass: [true, true], kmh: 5., ..kart() }, 60);
        assert!(slow.puffs().iter().all(|p| p.kind != PuffKind::Grass));
    }

    #[test]
    fn mushroom_sparks_circle_the_player_only() {
        let mut sys = PuffSystem::default();
        run(&mut sys, PuffIn { mushroom: true, ..kart() }, 30);
        assert!(sys.puffs().iter().any(|p| p.kind == PuffKind::Spark));
        let mut cpu = PuffSystem::default();
        run(&mut cpu, PuffIn { mushroom: true, human: false, ..kart() }, 30);
        assert!(cpu.puffs().iter().all(|p| p.kind != PuffKind::Spark));
    }

    #[test]
    fn a_frame_hitch_does_not_replay_a_burst_of_ticks() {
        let mut sys = PuffSystem::default();
        sys.update(5., &[kart()]);
        assert!(sys.alive() <= 3 * SLOTS);
        assert!(sys.karts[0].exhaust.iter().all(|s| s.timer <= 4));
    }
}
