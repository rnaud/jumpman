//! Effects around the karts: the stars of a spun-out kart and the shards of a smashed item box (a small engine-free
//! particle pool, extends the TS `kartFx.ts`), plus MK64's own kart particles from [`super::kart_puffs`] (exhaust, drift
//! smoke white / yellow / orange, tyre grass, mushroom sparks), which `kart.rs` draws together as camera-facing unlit
//! quads. Every kart (the player and the CPUs) feeds its own [`FxFrame`]. All lengths are world (sim) units.

use super::kart_puffs::{PuffIn, PuffKind, PuffSystem};

/// What the effects need to know about a kart each rendered frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct FxFrame {
    /// World position of the kart's feet and yaw (forward = (sin, cos)).
    pub pos: [f64; 3],
    pub yaw: f64,
    pub driving: bool,
    pub midair: bool,
    /// Spun out by a banana or a shell.
    pub spun: bool,
    /// The same kart as [`super::kart_puffs`] needs it.
    pub puff: PuffIn,
}

/// Particles per second for each kind this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxRates {
    /// Stars circling the head of a spun-out kart.
    pub star: f64,
}

const KINDS: usize = 1;

pub fn fx_rates(f: &FxFrame) -> FxRates {
    FxRates { star: if f.driving && f.spun { 16. } else { 0. } }
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
    /// MK64 drift smoke textures (16 x 16 and 32 x 32).
    DriftSmall,
    DriftLarge,
    /// The leaf-like fleck a tyre throws on grass (32 x 64).
    Grass,
}
impl Kind {
    /// Width over height of the quad.
    pub fn aspect(self) -> f64 {
        match self {
            Kind::Fire | Kind::Grass => 0.5,
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
/// Extra drawn particles from the MK64 pools (4 karts x 3 pools of 10).
pub const VIEW: usize = 120;

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
    puffs: PuffSystem,
    /// The MK64 pool particles as of the last update, drawn like the pool.
    view: Vec<Particle>,
}

impl Default for KartFx {
    fn default() -> Self {
        Self { carry: vec![], last: vec![], pool: vec![Particle::default(); POOL], seed: 1, puffs: PuffSystem::default(), view: vec![] }
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

    /// Every particle to draw: the free pool, then the MK64 kart pools.
    pub fn drawn(&self) -> impl Iterator<Item = &Particle> {
        self.pool.iter().chain(self.view.iter())
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
            self.carry[k][0] += rates.star * dt;
            while self.carry[k][0] >= 1. {
                self.carry[k][0] -= 1.;
                let a = self.rand() * std::f64::consts::TAU;
                let at = [f.pos[0] + a.cos() * 50., f.pos[1] + 120. + self.rand() * 24., f.pos[2] + a.sin() * 50.];
                let c = share(1.);
                self.spawn(Kind::Star, at, [-a.sin() * 120. + c[0], 40., a.cos() * 120. + c[1]], 0.5, 46., -20., 0., 0xffe45a, 1.);
            }
        }
        // MK64's own pools: 30 Hz ring buffers evaluated at the current kart position.
        let puff_in: Vec<PuffIn> = frames.iter().map(|f| PuffIn { driving: f.driving && !f.spun, ..f.puff }).collect();
        self.puffs.update(dt, &puff_in);
        self.view = self
            .puffs
            .puffs()
            .into_iter()
            .take(VIEW)
            .map(|q| {
                let kind = match q.kind {
                    PuffKind::Exhaust => Kind::Smoke,
                    PuffKind::DriftSmall => Kind::DriftSmall,
                    PuffKind::DriftLarge => Kind::DriftLarge,
                    PuffKind::Grass => Kind::Grass,
                    PuffKind::Spark => Kind::Spark,
                };
                Particle { alive: true, pos: q.pos, vel: [0.; 3], age: 0., life: f64::MAX, size: q.size, grow: 0., gravity: 0., color: q.color, max_opacity: q.alpha, kind, variant: q.variant }
            })
            .collect();
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
        FxFrame { driving: true, puff: PuffIn { driving: true, throttle: true, current_speed: 250., kmh: 90., human: true, tyres: [[40., 0., -20.], [-40., 0., -20.]], ..Default::default() }, ..Default::default() }
    }

    #[test]
    fn nothing_on_foot_and_stars_only_when_spun() {
        assert_eq!(fx_rates(&FxFrame { driving: false, spun: true, ..frame() }), FxRates::default());
        assert_eq!(fx_rates(&frame()), FxRates::default());
        assert!(fx_rates(&FxFrame { spun: true, ..frame() }).star > 0.);
    }

    #[test]
    fn a_driving_kart_exhausts_behind_its_own_pipe_every_kart_in_the_list() {
        let mut fx = KartFx::default();
        let a = FxFrame { pos: [0., 0., 0.], puff: PuffIn { pos: [0., 0., 0.], ..frame().puff }, ..frame() };
        let b = FxFrame { pos: [5000., 0., 0.], puff: PuffIn { pos: [5000., 0., 0.], ..frame().puff }, ..frame() };
        for _ in 0..90 {
            fx.update(1. / 60., &[a, b]);
        }
        let near = |x: f64| fx.drawn().filter(|p| p.alive && (p.pos[0] - x).abs() < 400.).count();
        assert!(near(0.) >= 3 && near(5000.) >= 3, "{} {}", near(0.), near(5000.));
        // behind the kart (forward is +z at yaw 0) and above the ground
        assert!(fx.drawn().filter(|p| p.alive).all(|p| p.pos[2] <= -30. && p.pos[1] > 20.));
    }

    #[test]
    fn spun_out_karts_stop_puffing_and_throw_stars() {
        let mut fx = KartFx::default();
        let f = FxFrame { spun: true, ..frame() };
        for _ in 0..60 {
            fx.update(1. / 60., &[f]);
        }
        assert!(fx.pool.iter().any(|p| p.alive && p.kind == Kind::Star));
        assert!(fx.drawn().all(|p| !p.alive || p.kind != Kind::Smoke));
    }

    #[test]
    fn a_box_bursts_into_shards_that_fade_out() {
        let mut fx = KartFx::default();
        fx.burst([0.; 3]);
        assert_eq!(fx.alive(), 19);
        for _ in 0..90 {
            fx.update(1. / 60., &[FxFrame::default()]);
        }
        assert_eq!(fx.alive(), 0, "everything fades out");
    }

    #[test]
    fn the_pool_never_grows() {
        let mut fx = KartFx::default();
        for _ in 0..40 {
            fx.burst([0.; 3]);
        }
        assert_eq!(fx.alive(), POOL);
        assert!(fx.drawn().count() <= POOL + VIEW);
    }
}
