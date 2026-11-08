//! Effects around the player's kart: drift sparks (white, then orange, then blue as the mini-turbo charges), tyre
//! smoke while drifting and on grass, the mini-turbo / mushroom exhaust flame and the shards of a smashed item box.
//! A small engine-free particle pool (twin of `src/game/experiments/kartFx.ts`); `kart.rs` draws it as camera-facing
//! unlit quads. All lengths are world (sim) units.

/// What the effects need to know about the kart each rendered frame.
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
    /// currentSpeed / top speed.
    pub speed_ratio: f64,
}

/// Particles per second for each kind this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxRates {
    pub spark: f64,
    pub smoke: f64,
    pub flame: f64,
}

pub fn fx_rates(f: &FxFrame) -> FxRates {
    if !f.driving {
        return FxRates::default();
    }
    let ground = !f.midair;
    let sliding = ground && f.drifting && !f.hop;
    FxRates {
        spark: if sliding { 38. + 14. * f.drift_state.clamp(0, 2) as f64 } else { 0. },
        smoke: if ground && f.speed_ratio > 0.15 {
            if sliding { 30. } else if f.grass { 22. * f.speed_ratio } else { 0. }
        } else {
            0.
        },
        flame: if f.boosting { 90. } else { 0. },
    }
}

/// Spark colour for the drift charge: 0xRRGGBB.
pub fn spark_colour(drift_state: i32) -> u32 {
    match drift_state {
        s if s >= 2 => 0x4fb4ff,
        1 => 0xffa21f,
        _ => 0xfff2a8,
    }
}

pub const POOL: usize = 120;

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
}
impl Particle {
    /// Current on-screen size (world units).
    pub fn size(&self) -> f64 {
        (self.size + self.grow * self.age).max(4.)
    }
    pub fn opacity(&self) -> f64 {
        self.max_opacity * (1. - self.age / self.life).clamp(0., 1.)
    }
}

#[derive(Debug)]
pub struct KartFx {
    carry: [f64; 3],
    pub pool: Vec<Particle>,
    seed: u32,
}

impl Default for KartFx {
    fn default() -> Self {
        Self { carry: [0.; 3], pool: vec![Particle::default(); POOL], seed: 1 }
    }
}

impl KartFx {
    fn rand(&mut self) -> f64 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        self.seed as f64 / 4294967296.
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn(&mut self, pos: [f64; 3], vel: [f64; 3], life: f64, size: f64, grow: f64, gravity: f64, color: u32, opacity: f64) {
        if let Some(p) = self.pool.iter_mut().find(|p| !p.alive) {
            *p = Particle { alive: true, pos, vel, age: 0., life, size, grow, gravity, color, max_opacity: opacity };
        }
    }

    pub fn alive(&self) -> usize {
        self.pool.iter().filter(|p| p.alive).count()
    }

    /// Smashed item box: a ring of rainbow shards.
    pub fn burst(&mut self, at: [f64; 3]) {
        for _ in 0..16 {
            let (a, up, out) = (self.rand() * std::f64::consts::TAU, 80. + self.rand() * 220., 120. + self.rand() * 260.);
            let (r, g, b) = hsl_to_rgb(self.rand(), 0.9, 0.6);
            let color = ((r * 255.) as u32) << 16 | ((g * 255.) as u32) << 8 | (b * 255.) as u32;
            let size = 24. + self.rand() * 18.;
            self.spawn(at, [a.cos() * out, up, a.sin() * out], 0.55, size, -20., 520., color, 1.);
        }
    }

    pub fn update(&mut self, dt: f64, f: &FxFrame) {
        let rates = fx_rates(f);
        let (sin, cos) = f.yaw.sin_cos();
        for (i, rate) in [rates.spark, rates.smoke, rates.flame].into_iter().enumerate() {
            self.carry[i] += rate * dt;
            while self.carry[i] >= 1. {
                self.carry[i] -= 1.;
                let side = if self.rand() < 0.5 { -1. } else { 1. };
                // Rear wheels: 40 across, 56 behind the kart's centre.
                let wheel = [f.pos[0] + sin * -56. + cos * side * 40., f.pos[1] + 10., f.pos[2] + cos * -56. - sin * side * 40.];
                match i {
                    0 => {
                        let out = side * (60. + self.rand() * 120.);
                        let vel = [cos * out - sin * 120., 90. + self.rand() * 160., -sin * out - cos * 120.];
                        let life = 0.28 + self.rand() * 0.18;
                        self.spawn(wheel, vel, life, 22., -30., 520., spark_colour(f.drift_state), 1.);
                    }
                    1 => {
                        let vel = [(self.rand() - 0.5) * 80. - sin * 140., 30. + self.rand() * 60., (self.rand() - 0.5) * 80. - cos * 140.];
                        let life = 0.6 + self.rand() * 0.3;
                        self.spawn(wheel, vel, life, 40., 90., 0., if f.grass { 0x9a8660 } else { 0xdcdcdc }, 0.55);
                    }
                    _ => {
                        // Twin exhausts: 18 either side, 80 behind the centre, low.
                        let mid = [f.pos[0] + sin * -80. + cos * side * 18., f.pos[1] + 24. + self.rand() * 8., f.pos[2] + cos * -80. - sin * side * 18.];
                        let (s1, s2, s3) = (self.rand(), self.rand(), self.rand());
                        let vel = [-sin * (300. + s1 * 200.) + (s2 - 0.5) * 60., (s3 - 0.5) * 40., -cos * (300. + s1 * 200.) + (s2 - 0.5) * 60.];
                        let color = if self.rand() < 0.5 { 0xffd23a } else { 0xff7a1c };
                        self.spawn(mid, vel, 0.2, 30., -70., 0., color, 0.85);
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
    fn nothing_on_foot_or_cruising_on_asphalt() {
        assert_eq!(fx_rates(&FxFrame { driving: false, drifting: true, ..frame() }), FxRates::default());
        assert_eq!(fx_rates(&frame()), FxRates::default());
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
    fn grass_kicks_up_smoke_and_boosts_light_the_flame() {
        assert!(fx_rates(&FxFrame { grass: true, ..frame() }).smoke > 0.);
        assert_eq!(fx_rates(&FxFrame { grass: true, speed_ratio: 0.05, ..frame() }).smoke, 0.);
        assert!(fx_rates(&FxFrame { boosting: true, ..frame() }).flame > 0.);
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
            fx.update(1. / 60., &f);
        }
        let n = fx.alive();
        assert!(n >= 10, "{n} particles after half a second of drifting");
        let behind = fx.pool.iter().filter(|p| p.alive).all(|p| p.pos[2] < 110. && p.pos[1] > -10.);
        assert!(behind, "sparks and smoke start behind the kart");
        assert!(fx.pool.iter().filter(|p| p.alive && p.color == 0x4fb4ff).count() > 0, "blue sparks at full charge");
        fx.burst([0.; 3]);
        assert_eq!(fx.alive(), (n + 16).min(POOL));
        for _ in 0..90 {
            fx.update(1. / 60., &FxFrame::default());
        }
        assert_eq!(fx.alive(), 0, "everything fades out");
    }

    #[test]
    fn the_pool_never_grows() {
        let mut fx = KartFx::default();
        for _ in 0..20 {
            fx.burst([0.; 3]);
        }
        assert_eq!(fx.alive(), POOL);
    }
}
