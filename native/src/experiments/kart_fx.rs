//! Effects around the karts: drift sparks (white, then orange, then blue as the mini-turbo charges), tyre smoke while
//! drifting, dust on grass and exhaust puffs at speed, the mini-turbo / mushroom flame and speed streaks, the stars of a
//! spun-out kart and the shards of a smashed item box. A small engine-free particle pool (extends the TS
//! `kartFx.ts`); `kart.rs` draws it as camera-facing unlit quads. Every kart (the player and the CPUs) feeds its own
//! [`FxFrame`]. All lengths are world (sim) units.

/// What the effects need to know about a kart each rendered frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct FxFrame {
    /// World position of the kart's feet and yaw (forward = (sin, cos)).
    pub pos: [f64; 3],
    pub yaw: f64,
    pub driving: bool,
    pub midair: bool,
    pub drifting: bool,
    pub hop: bool,
    pub drift_state: i32,
    pub boosting: bool,
    pub grass: bool,
    /// Spun out by a banana or a shell.
    pub spun: bool,
    /// currentSpeed / top speed.
    pub speed_ratio: f64,
}

/// Particles per second for each kind this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxRates {
    pub spark: f64,
    /// Tyre smoke while sliding.
    pub smoke: f64,
    pub flame: f64,
    /// Dust kicked up by the wheels on grass.
    pub dust: f64,
    /// Light exhaust puff at speed on the road.
    pub puff: f64,
    /// Speed streaks around the kart while boosting.
    pub streak: f64,
    /// Stars circling the head of a spun-out kart.
    pub star: f64,
}

const KINDS: usize = 7;

pub fn fx_rates(f: &FxFrame) -> FxRates {
    if !f.driving {
        return FxRates::default();
    }
    let ground = !f.midair;
    let sliding = ground && f.drifting && !f.hop;
    let rolling = ground && f.speed_ratio > 0.15 && !f.spun;
    FxRates {
        spark: if sliding { 40. + 16. * f.drift_state.clamp(0, 2) as f64 } else { 0. },
        smoke: if sliding { 26. } else { 0. },
        flame: if f.boosting { 70. } else { 0. },
        dust: if rolling && f.grass { 30. * f.speed_ratio } else { 0. },
        puff: if rolling && !f.grass && !sliding && f.speed_ratio > 0.5 { 7. * f.speed_ratio } else { 0. },
        streak: if f.boosting && ground { 34. } else { 0. },
        star: if f.spun { 16. } else { 0. },
    }
}

/// Spark colour for the drift charge: 0xRRGGBB (white-yellow, then orange, then blue as in MK64).
pub fn spark_colour(drift_state: i32) -> u32 {
    match drift_state {
        s if s >= 2 => 0x4fb4ff,
        1 => 0xffa21f,
        _ => 0xfff2a8,
    }
}

/// What a particle looks like; `kart.rs` maps each to a texture and a blend mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// Round soft glow (shards, spark cores).
    #[default]
    Soft,
    /// MK64 smoke puff (`mk64-particle-smoke-N`).
    Smoke,
    /// MK64 flame (`mk64-particle-fire`), taller than wide.
    Fire,
    /// MK64 spark flecks (`mk64-particle-spark-N`).
    Spark,
    /// Four-point star of a spin-out.
    Star,
    /// Thin speed streak.
    Streak,
}
impl Kind {
    /// Width over height of the quad.
    pub fn aspect(self) -> f64 {
        match self {
            Kind::Fire => 0.5,
            Kind::Streak => 0.3,
            _ => 1.,
        }
    }
    /// Drawn additively (glows) rather than alpha blended.
    pub fn additive(self) -> bool {
        matches!(self, Kind::Fire | Kind::Spark | Kind::Streak)
    }
}

pub const POOL: usize = 240;

#[derive(Clone, Copy, Debug, Default)]
pub struct Particle {
    pub alive: bool,
    pub pos: [f64; 3],
    vel: [f64; 3],
    pub age: f64,
    life: f64,
    size: f64,
    grow: f64,
    gravity: f64,
    pub color: u32,
    max_opacity: f64,
    pub kind: Kind,
    /// Which of the 4 smoke / spark texture variants.
    pub variant: u8,
}
impl Particle {
    /// Current on-screen height (world units).
    pub fn size(&self) -> f64 {
        (self.size + self.grow * self.age).max(4.)
    }
    /// Velocity in world units per second.
    pub fn velocity(&self) -> [f64; 3] {
        self.vel
    }
    pub fn opacity(&self) -> f64 {
        self.max_opacity * (1. - self.age / self.life).clamp(0., 1.)
    }
}

#[derive(Debug)]
pub struct KartFx {
    carry: Vec<[f64; KINDS]>,
    /// Last position of each kart, to inherit its velocity.
    last: Vec<Option<[f64; 3]>>,
    pub pool: Vec<Particle>,
    seed: u32,
}

impl Default for KartFx {
    fn default() -> Self {
        Self { carry: vec![], last: vec![], pool: vec![Particle::default(); POOL], seed: 1 }
    }
}

impl KartFx {
    fn rand(&mut self) -> f64 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        self.seed as f64 / 4294967296.
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn(&mut self, kind: Kind, pos: [f64; 3], vel: [f64; 3], life: f64, size: f64, grow: f64, gravity: f64, color: u32, opacity: f64) {
        let variant = (self.rand() * 4.) as u8 % 4;
        if let Some(p) = self.pool.iter_mut().find(|p| !p.alive) {
            *p = Particle { alive: true, pos, vel, age: 0., life, size, grow, gravity, color, max_opacity: opacity, kind, variant };
        }
    }

    pub fn alive(&self) -> usize {
        self.pool.iter().filter(|p| p.alive).count()
    }

    /// Smashed item box: a ring of rainbow shards and a white flash.
    pub fn burst(&mut self, at: [f64; 3]) {
        for _ in 0..18 {
            let (a, up, out) = (self.rand() * std::f64::consts::TAU, 120. + self.rand() * 300., 160. + self.rand() * 380.);
            let (r, g, b) = hsl_to_rgb(self.rand(), 0.9, 0.6);
            let color = ((r * 255.) as u32) << 16 | ((g * 255.) as u32) << 8 | (b * 255.) as u32;
            let size = 40. + self.rand() * 30.;
            self.spawn(Kind::Soft, at, [a.cos() * out, up, a.sin() * out], 0.6, size, -30., 620., color, 1.);
        }
        self.spawn(Kind::Soft, at, [0.; 3], 0.25, 120., 500., 0., 0xffffff, 0.9);
    }

    /// A kart rolled over a banana or was hit by a shell: a puff and a ring of stars.
    pub fn smash(&mut self, at: [f64; 3], color: u32) {
        for _ in 0..8 {
            let (a, out) = (self.rand() * std::f64::consts::TAU, 120. + self.rand() * 200.);
            let up = 160. + self.rand() * 200.;
            self.spawn(Kind::Star, [at[0], at[1] + 50., at[2]], [a.cos() * out, up, a.sin() * out], 0.7, 46., -20., 500., color, 1.);
        }
        self.spawn(Kind::Smoke, [at[0], at[1] + 20., at[2]], [0.; 3], 0.5, 90., 260., 0., 0xffffff, 0.7);
    }

    /// Advance by `dt`, emitting for every kart in `frames` (index = kart; the player first).
    pub fn update(&mut self, dt: f64, frames: &[FxFrame]) {
        if self.carry.len() < frames.len() {
            self.carry.resize(frames.len(), [0.; KINDS]);
            self.last.resize(frames.len(), None);
        }
        for (k, f) in frames.iter().enumerate() {
            let rates = fx_rates(f);
            let (sin, cos) = f.yaw.sin_cos();
            // The kart's own horizontal velocity: the dust and smoke carry a share of it, so the trail hangs behind
            // the kart in view instead of flying out of the bottom of the screen.
            let carried = match self.last[k] {
                Some(l) if dt > 0. && ((f.pos[0] - l[0]).hypot(f.pos[2] - l[2])) < 400. => [(f.pos[0] - l[0]) / dt, (f.pos[2] - l[2]) / dt],
                _ => [0., 0.],
            };
            self.last[k] = Some(f.pos);
            let share = |v: f64| [carried[0] * v, carried[1] * v];
            let list = [rates.spark, rates.smoke, rates.flame, rates.dust, rates.puff, rates.streak, rates.star];
            for (i, rate) in list.into_iter().enumerate() {
                self.carry[k][i] += rate * dt;
                while self.carry[k][i] >= 1. {
                    self.carry[k][i] -= 1.;
                    let side = if self.rand() < 0.5 { -1. } else { 1. };
                    // Rear wheels: 44 across, 60 behind the kart's centre.
                    let wheel = [f.pos[0] + sin * -60. + cos * side * 44., f.pos[1] + 8., f.pos[2] + cos * -60. - sin * side * 44.];
                    match i {
                        0 => {
                            let out = side * (80. + self.rand() * 160.);
                            let c = share(0.6);
                            let vel = [cos * out - sin * 140. + c[0], 120. + self.rand() * 200., -sin * out - cos * 140. + c[1]];
                            let life = 0.3 + self.rand() * 0.2;
                            let colour = spark_colour(f.drift_state);
                            self.spawn(Kind::Spark, wheel, vel, life, 90., -60., 560., colour, 1.);
                            self.spawn(Kind::Soft, wheel, vel, life * 0.8, 44., -30., 560., colour, 1.);
                        }
                        1 => {
                            let c = share(0.75);
                            let vel = [(self.rand() - 0.5) * 90. - sin * 90. + c[0], 40. + self.rand() * 70., (self.rand() - 0.5) * 90. - cos * 90. + c[1]];
                            let life = 0.7 + self.rand() * 0.3;
                            self.spawn(Kind::Smoke, wheel, vel, life, 70., 140., 0., 0xe6e6e6, 0.65);
                        }
                        2 => {
                            // Twin exhausts: 22 either side, 76 behind the centre, low; the flame points back.
                            let mid = [f.pos[0] + sin * -112. + cos * side * 24., f.pos[1] + 14. + self.rand() * 8., f.pos[2] + cos * -112. - sin * side * 22.];
                            let (s1, s2, s3) = (self.rand(), self.rand(), self.rand());
                            let c = share(0.92);
                            let vel = [-sin * (90. + s1 * 90.) + (s2 - 0.5) * 40. + c[0], 30. + s3 * 50., -cos * (90. + s1 * 90.) + (s2 - 0.5) * 40. + c[1]];
                            let color = if self.rand() < 0.5 { 0xffe14a } else { 0xff8a24 };
                            self.spawn(Kind::Fire, mid, vel, 0.3, 84., -45., 0., color, 0.8);
                        }
                        3 => {
                            let c = share(0.7);
                            let vel = [(self.rand() - 0.5) * 120. - sin * 80. + c[0], 50. + self.rand() * 90., (self.rand() - 0.5) * 120. - cos * 80. + c[1]];
                            let life = 0.6 + self.rand() * 0.35;
                            self.spawn(Kind::Smoke, wheel, vel, life, 54., 110., 0., 0xa88c5c, 0.6);
                        }
                        4 => {
                            let pipe = [f.pos[0] + sin * -84. + cos * side * 22., f.pos[1] + 24., f.pos[2] + cos * -84. - sin * side * 22.];
                            let c = share(0.8);
                            let vel = [-sin * 70. + c[0], 30. + self.rand() * 40., -cos * 70. + c[1]];
                            self.spawn(Kind::Smoke, pipe, vel, 0.55, 36., 90., 0., 0xcfcfcf, 0.4);
                        }
                        5 => {
                            // Streaks flying past the kart's sides, from ahead to behind.
                            let across = side * (90. + self.rand() * 150.);
                            let ahead = 40. + self.rand() * 120.;
                            let up = 20. + self.rand() * 150.;
                            let at = [f.pos[0] + sin * ahead + cos * across, f.pos[1] + up, f.pos[2] + cos * ahead - sin * across];
                            self.spawn(Kind::Streak, at, [-sin * 900., 0., -cos * 900.], 0.22, 120., 0., 0., 0xffffff, 0.75);
                        }
                        _ => {
                            let a = self.rand() * std::f64::consts::TAU;
                            let at = [f.pos[0] + a.cos() * 50., f.pos[1] + 120. + self.rand() * 24., f.pos[2] + a.sin() * 50.];
                            let c = share(1.);
                            self.spawn(Kind::Star, at, [-a.sin() * 120. + c[0], 40., a.cos() * 120. + c[1]], 0.5, 46., -20., 0., 0xffe45a, 1.);
                        }
                    }
                }
            }
        }
        for p in self.pool.iter_mut().filter(|p| p.alive) {
            p.age += dt;
            if p.age >= p.life {
                p.alive = false;
                continue;
            }
            p.vel[1] -= p.gravity * dt;
            for k in 0..3 {
                p.pos[k] += p.vel[k] * dt;
            }
        }
    }
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let c = (1. - (2. * l - 1.).abs()) * s;
    let x = c * (1. - ((h * 6.) % 2. - 1.).abs());
    let m = l - c / 2.;
    let (r, g, b) = match (h * 6.) as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    (r + m, g + m, b + m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> FxFrame {
        FxFrame { driving: true, speed_ratio: 0.8, ..Default::default() }
    }

    #[test]
    fn nothing_on_foot() {
        assert_eq!(fx_rates(&FxFrame { driving: false, drifting: true, ..frame() }), FxRates::default());
    }

    #[test]
    fn a_fast_kart_on_asphalt_only_puffs_lightly() {
        let r = fx_rates(&frame());
        assert!(r.puff > 0. && r.dust == 0. && r.spark == 0. && r.smoke == 0.);
        assert_eq!(fx_rates(&FxFrame { speed_ratio: 0.2, ..frame() }).puff, 0.);
    }

    #[test]
    fn drift_sparks_grow_with_charge_and_stop_in_the_air_or_during_the_hop() {
        let drift = |state, midair, hop| fx_rates(&FxFrame { drifting: true, drift_state: state, midair, hop, ..frame() });
        assert!(drift(0, false, false).spark > 0.);
        assert!(drift(2, false, false).spark > drift(0, false, false).spark);
        assert!(drift(0, false, false).smoke > 0.);
        assert_eq!(drift(1, true, false), FxRates::default());
        assert_eq!(drift(1, false, true).spark, 0.);
    }

    #[test]
    fn grass_kicks_up_dust_boosts_light_the_flame_and_spins_throw_stars() {
        assert!(fx_rates(&FxFrame { grass: true, ..frame() }).dust > 0.);
        assert_eq!(fx_rates(&FxFrame { grass: true, speed_ratio: 0.05, ..frame() }).dust, 0.);
        let boost = fx_rates(&FxFrame { boosting: true, ..frame() });
        assert!(boost.flame > 0. && boost.streak > 0.);
        let spun = fx_rates(&FxFrame { spun: true, ..frame() });
        assert!(spun.star > 0. && spun.puff == 0.);
    }

    #[test]
    fn spark_colours_run_white_orange_blue() {
        assert_eq!(spark_colour(0), 0xfff2a8);
        assert_eq!(spark_colour(1), 0xffa21f);
        assert_eq!(spark_colour(2), 0x4fb4ff);
    }

    #[test]
    fn emits_at_the_rear_wheels_expires_and_a_box_bursts_into_shards() {
        let mut fx = KartFx::default();
        let f = FxFrame { drifting: true, drift_state: 2, pos: [100., 0., 100.], ..frame() };
        for _ in 0..30 {
            fx.update(1. / 60., &[f]);
        }
        let n = fx.alive();
        assert!(n >= 10, "{n} particles after half a second of drifting");
        let behind = fx.pool.iter().filter(|p| p.alive).all(|p| p.pos[2] < 110. && p.pos[1] > -10.);
        assert!(behind, "sparks and smoke start behind the kart");
        assert!(fx.pool.iter().filter(|p| p.alive && p.color == 0x4fb4ff).count() > 0, "blue sparks at full charge");
        fx.burst([0.; 3]);
        assert_eq!(fx.alive(), (n + 19).min(POOL));
        for _ in 0..90 {
            fx.update(1. / 60., &[FxFrame::default()]);
        }
        assert_eq!(fx.alive(), 0, "everything fades out");
    }

    #[test]
    fn every_kart_in_the_frame_list_emits_from_its_own_rear() {
        let mut fx = KartFx::default();
        let a = FxFrame { grass: true, pos: [0., 0., 0.], ..frame() };
        let b = FxFrame { grass: true, pos: [5000., 0., 0.], ..frame() };
        for _ in 0..60 {
            fx.update(1. / 60., &[a, b]);
        }
        let near = |x: f64| fx.pool.iter().filter(|p| p.alive && (p.pos[0] - x).abs() < 400.).count();
        assert!(near(0.) > 3 && near(5000.) > 3);
    }

    #[test]
    fn the_pool_never_grows() {
        let mut fx = KartFx::default();
        for _ in 0..40 {
            fx.burst([0.; 3]);
        }
        assert_eq!(fx.alive(), POOL);
    }
}
