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
    /// Round soft glow drawn additively (flames, flashes), from the procedural soft texture.
    Glow,
}

/// What hit a kart, for [`KartFx::impact`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Impact {
    Shell,
    Banana,
    /// Blue shell / fake box: a blast.
    Blast,
    Thunder,
    /// Two karts touched.
    Bump,
    Other,
}

/// Drift spark tint for mini-turbo charge `state` (0 none yet, 1 first stage, 2 full); `r` in 0..1 varies it.
pub fn spark_colour(state: i32, r: f64) -> u32 {
    let pair = match state {
        i32::MIN..=0 => [0xfff6d8, 0xffe9a0],
        1 => [0xffd23a, 0xfff08a],
        _ => [0xff7a22, 0xffb040],
    };
    pair[(r >= 0.5) as usize]
}

/// Sparks a second thrown by a drifting kart at charge `state` (CPUs throw half as many).
pub fn spark_rate(state: i32, human: bool) -> f64 {
    [18., 32., 48.][state.clamp(0, 2) as usize] * if human { 1. } else { 0.5 }
}
/// How much of a particle `size` tall still shows `dist` from the eye: it fades out as the camera drives into it,
/// so a burst the kart passes through never washes over the whole screen.
pub fn near_fade(size: f64, dist: f64) -> f64 {
    ((dist / size.max(1.) - 1.2) / 2.4).clamp(0., 1.)
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
        matches!(self, Kind::Fire | Kind::Spark | Kind::Streak | Kind::Glow)
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
    /// Per kart: was drifting last frame and at which charge (to catch the mini-turbo release), emission carry for
    /// the drift sparks / boost flame / star sparkle, and whether the kart has a star.
    prev_drift: Vec<(bool, i32)>,
    juice_carry: Vec<[f64; 3]>,
    stars: Vec<bool>,
    /// Each kart's horizontal velocity as of the last update, and the share of one that the burst being spawned
    /// rides along with (so a pickup's shards fly on with the kart instead of being left behind in a blink).
    vels: Vec<[f64; 2]>,
    ride: [f64; 2],
}

impl Default for KartFx {
    fn default() -> Self {
        Self { carry: vec![], last: vec![], pool: vec![Particle::default(); POOL], seed: 1, puffs: PuffSystem::default(), view: vec![], prev_drift: vec![], juice_carry: vec![], stars: vec![], vels: vec![], ride: [0.; 2] }
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
        let vel = [vel[0] + self.ride[0], vel[1], vel[2] + self.ride[1]];
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

    /// Bursts spawned until [`Self::settle`] ride along with the kart nearest `at` (within reach), at `share` of
    /// its speed.
    fn ride_with(&mut self, at: [f64; 3], share: f64) {
        let near = self.last.iter().zip(&self.vels).filter_map(|(l, v)| l.map(|l| ((l[0] - at[0]).hypot(l[2] - at[2]), *v))).filter(|(d, _)| *d < 320.).min_by(|a, b| a.0.total_cmp(&b.0));
        self.ride = near.map_or([0.; 2], |(_, v)| [v[0] * share, v[1] * share]);
    }
    fn settle(&mut self) {
        self.ride = [0.; 2];
    }

    fn rainbow(&mut self, light: f64) -> u32 {
        let (r, g, b) = hsl_to_rgb(self.rand(), 0.9, light);
        ((r * 255.) as u32) << 16 | ((g * 255.) as u32) << 8 | (b * 255.) as u32
    }

    /// A flat ring of `n` streaks flying outward from `at`; `color` 0 means rainbow.
    fn ring(&mut self, at: [f64; 3], n: usize, speed: f64, size: f64, life: f64, color: u32) {
        let turn = self.rand();
        for i in 0..n {
            let a = (i as f64 + turn) / n as f64 * std::f64::consts::TAU;
            let c = if color == 0 { self.rainbow(0.65) } else { color };
            self.spawn(Kind::Streak, at, [a.cos() * speed, speed * 0.08, a.sin() * speed], life, size, 0., 0., c, 1.);
        }
    }

    /// `n` little stars drifting up and out from around `at`; `color` 0 means rainbow.
    pub fn sparkle(&mut self, at: [f64; 3], color: u32, n: usize) {
        for _ in 0..n {
            let (a, out) = (self.rand() * std::f64::consts::TAU, 30. + self.rand() * 110.);
            let c = if color == 0 { self.rainbow(0.7) } else { color };
            let (up, life, size) = (90. + self.rand() * 160., 0.45 + self.rand() * 0.3, 22. + self.rand() * 14.);
            let lift = self.rand() * 40.;
            self.spawn(Kind::Star, [at[0] + a.cos() * out * 0.3, at[1] + lift, at[2] + a.sin() * out * 0.3], [a.cos() * out, up, a.sin() * out], life, size, -20., 120., c, 1.);
        }
    }

    /// Smashed item box: rainbow shards, a ring racing outward, sparkles hanging where it was and a white flash.
    pub fn burst(&mut self, at: [f64; 3]) {
        self.ride_with(at, 0.85);
        for _ in 0..16 {
            let (a, up, out) = (self.rand() * std::f64::consts::TAU, 120. + self.rand() * 300., 160. + self.rand() * 380.);
            let color = self.rainbow(0.6);
            let size = 40. + self.rand() * 30.;
            self.spawn(Kind::Soft, at, [a.cos() * out, up, a.sin() * out], 0.6, size, -30., 620., color, 1.);
        }
        self.ring(at, 12, 520., 40., 0.28, 0);
        self.sparkle(at, 0, 6);
        self.spawn(Kind::Glow, at, [0.; 3], 0.2, 90., 380., 0., 0xffffff, 0.8);
        self.settle();
    }

    /// The roulette stopped: the item's colour twinkles over the kart.
    pub fn landed(&mut self, at: [f64; 3], color: u32) {
        self.ride_with(at, 0.95);
        self.sparkle([at[0], at[1] + 70., at[2]], color, 7);
        self.spawn(Kind::Glow, [at[0], at[1] + 80., at[2]], [0.; 3], 0.2, 40., 300., 0., color, 0.6);
        self.settle();
    }

    /// "GO!": a green ring and sparkles off the start line.
    pub fn go(&mut self, at: [f64; 3]) {
        let at = [at[0], at[1] + 20., at[2]];
        self.ring(at, 16, 640., 44., 0.32, 0x7dff6a);
        self.sparkle(at, 0xeaffd8, 8);
    }

    /// Across the line: a fountain of coloured flecks over the kart.
    pub fn confetti(&mut self, at: [f64; 3]) {
        self.ride_with(at, 0.8);
        for _ in 0..34 {
            let (a, out, up) = (self.rand() * std::f64::consts::TAU, 60. + self.rand() * 260., 320. + self.rand() * 380.);
            let color = self.rainbow(0.62);
            let (life, size) = (1.1 + self.rand() * 0.5, 22. + self.rand() * 16.);
            self.spawn(Kind::Soft, [at[0], at[1] + 60., at[2]], [a.cos() * out, up, a.sin() * out], life, size, -6., 560., color, 1.);
        }
        self.sparkle([at[0], at[1] + 80., at[2]], 0xfff1a8, 8);
        self.settle();
    }

    /// A new lap: a gold ring around the kart (bigger with sparkles for the final lap).
    pub fn lap(&mut self, at: [f64; 3], last: bool) {
        self.ride_with(at, 0.9);
        let at = [at[0], at[1] + 30., at[2]];
        self.ring(at, if last { 16 } else { 10 }, if last { 560. } else { 420. }, 40., 0.3, 0xffd84a);
        if last {
            self.sparkle(at, 0xfff1a8, 8);
        }
        self.settle();
    }

    /// Something hit a kart at `at`: what it looks like depends on what it was (`color` is the item's).
    pub fn impact(&mut self, at: [f64; 3], what: Impact, color: u32) {
        let mid = [at[0], at[1] + 28., at[2]];
        match what {
            Impact::Shell => {
                self.smash(at, color);
                for _ in 0..10 {
                    let (a, out, up) = (self.rand() * std::f64::consts::TAU, 140. + self.rand() * 300., 140. + self.rand() * 280.);
                    let size = 30. + self.rand() * 24.;
                    self.spawn(Kind::Soft, mid, [a.cos() * out, up, a.sin() * out], 0.55, size, -24., 700., color, 1.);
                }
                self.ring(mid, 10, 460., 38., 0.24, 0xffffff);
                self.spawn(Kind::Glow, mid, [0.; 3], 0.16, 90., 520., 0., 0xffffff, 0.8);
            }
            Impact::Banana => {
                self.smash(at, color);
                for i in 0..3 {
                    let a = (i as f64 + self.rand()) * 2.1;
                    self.spawn(Kind::Smoke, [at[0], at[1] + 18., at[2]], [a.cos() * 90., 50., a.sin() * 90.], 0.55, 70., 220., 0., 0xfff3b0, 0.75);
                }
                for _ in 0..8 {
                    let (a, out, up) = (self.rand() * std::f64::consts::TAU, 100. + self.rand() * 220., 160. + self.rand() * 240.);
                    self.spawn(Kind::Soft, mid, [a.cos() * out, up, a.sin() * out], 0.5, 26., -16., 760., 0xffe23a, 1.);
                }
            }
            Impact::Blast => {
                self.spawn(Kind::Glow, mid, [0.; 3], 0.3, 160., 1100., 0., 0xfff4c8, 1.);
                self.ring(mid, 16, 760., 56., 0.34, 0xffa53a);
                for _ in 0..9 {
                    let (a, out, up) = (self.rand() * std::f64::consts::TAU, 60. + self.rand() * 200., 180. + self.rand() * 320.);
                    let c = if self.rand() < 0.5 { 0xff7a22 } else { 0xffc84a };
                    let (life, size) = (0.4 + self.rand() * 0.25, 80. + self.rand() * 50.);
                    self.spawn(Kind::Glow, mid, [a.cos() * out, up, a.sin() * out], life, size, -90., 300., c, 0.9);
                }
                for i in 0..4 {
                    let a = i as f64 * 1.6 + self.rand();
                    self.spawn(Kind::Smoke, [at[0], at[1] + 60., at[2]], [a.cos() * 70., 130., a.sin() * 70.], 1.0, 110., 180., 0., 0x55504c, 0.7);
                }
                self.smash(at, color);
            }
            Impact::Thunder => {
                self.spawn(Kind::Glow, [at[0], at[1] + 90., at[2]], [0.; 3], 0.2, 140., 700., 0., 0xfff6a0, 0.9);
                for _ in 0..8 {
                    let (a, out) = (self.rand() * std::f64::consts::TAU, 200. + self.rand() * 260.);
                    self.spawn(Kind::Streak, [at[0], at[1] + 150., at[2]], [a.cos() * out, -260., a.sin() * out], 0.22, 60., 0., 0., 0xfff06a, 1.);
                }
            }
            Impact::Bump => {
                self.ride_with(at, 0.85);
                for _ in 0..7 {
                    let (a, out, up) = (self.rand() * std::f64::consts::TAU, 160. + self.rand() * 260., 80. + self.rand() * 260.);
                    let c = if self.rand() < 0.5 { 0xfff6d8 } else { 0xffd23a };
                    self.spawn(Kind::Streak, mid, [a.cos() * out, up, a.sin() * out], 0.22, 34., -30., 900., c, 1.);
                }
                self.spawn(Kind::Glow, mid, [0.; 3], 0.12, 50., 420., 0., 0xfff2c0, 0.8);
                self.settle();
            }
            Impact::Other => self.smash(at, color),
        }
    }

    /// Kart `k` has (or lost) a star: it trails rainbow sparkles while it does.
    pub fn set_star(&mut self, k: usize, on: bool) {
        if self.stars.len() <= k {
            self.stars.resize(k + 1, false);
        }
        self.stars[k] = on;
    }

    /// The effects that follow a kart's state: sparks off the rear tyres while it drifts (stepping white, yellow,
    /// orange with the mini-turbo charge), a burst from the exhaust when a charged drift is let go, a flame while
    /// it boosts and sparkles while it has a star. `carried` is the kart's horizontal velocity.
    fn juice(&mut self, k: usize, f: &FxFrame, dt: f64, carried: [f64; 2]) {
        let q = f.puff;
        let (sin, cos) = f.yaw.sin_cos();
        let live = f.driving && !f.spun;
        let state = q.drift_state.clamp(0, 2);
        let drifting = live && q.drifting;
        let (was, was_state) = self.prev_drift[k];
        self.prev_drift[k] = (drifting, state);
        let rear = [f.pos[0] - sin * 30., f.pos[1] + 26., f.pos[2] - cos * 30.];
        if drifting && !q.midair && !q.hop && q.kmh > 20. {
            self.juice_carry[k][0] += spark_rate(state, q.human) * dt;
            while self.juice_carry[k][0] >= 1. {
                self.juice_carry[k][0] -= 1.;
                let tyre = q.tyres[(self.rand() * 2.) as usize % 2];
                let (side, back) = (self.rand() * 2. - 1., 160. + self.rand() * 240.);
                let vel = [-sin * back + cos * side * 150. + carried[0] * 0.6, 110. + self.rand() * 240., -cos * back - sin * side * 150. + carried[1] * 0.6];
                let (life, size, color) = (0.2 + self.rand() * 0.14, 12. + state as f64 * 4. + self.rand() * 5., spark_colour(state, self.rand()));
                self.spawn(Kind::Streak, [tyre[0], tyre[1] + 8., tyre[2]], vel, life, size, -30., 950., color, 1.);
            }
            // The charge glows at the wheels once there is one.
            if state >= 1 {
                self.juice_carry[k][1] += 24. * dt;
                while self.juice_carry[k][1] >= 1. {
                    self.juice_carry[k][1] -= 1.;
                    let tyre = q.tyres[(self.rand() * 2.) as usize % 2];
                    let color = spark_colour(state, 0.);
                    self.spawn(Kind::Glow, [tyre[0], tyre[1] + 10., tyre[2]], [carried[0] * 0.9, 30., carried[1] * 0.9], 0.12, 16. + state as f64 * 7., 40., 0., color, 0.6);
                }
            }
        } else if was && was_state >= 1 && live && !q.drifting {
            // Let go with a charge: the mini-turbo kicks out of the exhaust.
            let color = spark_colour(was_state, 0.);
            for _ in 0..10 {
                let (side, back) = (self.rand() * 2. - 1., 260. + self.rand() * 320.);
                let vel = [-sin * back + cos * side * 220. + carried[0] * 0.7, 60. + self.rand() * 240., -cos * back - sin * side * 220. + carried[1] * 0.7];
                let c = spark_colour(was_state, self.rand());
                self.spawn(Kind::Streak, rear, vel, 0.3, 22., -24., 700., c, 1.);
            }
            self.spawn(Kind::Glow, rear, [carried[0] * 0.8, 0., carried[1] * 0.8], 0.2, 40., 300., 0., color, 0.9);
        }
        if live && q.mushroom {
            self.juice_carry[k][1] += if q.human { 44. } else { 22. } * dt;
            while self.juice_carry[k][1] >= 1. {
                self.juice_carry[k][1] -= 1.;
                let (side, back) = (if self.rand() < 0.5 { -9. } else { 9. }, 200. + self.rand() * 180.);
                let at = [rear[0] + cos * side, rear[1] + self.rand() * 8., rear[2] - sin * side];
                let color = [0xfff0a0, 0xffb428, 0xff6a1e][(self.rand() * 3.) as usize % 3];
                let (life, size) = (0.14 + self.rand() * 0.1, 20. + self.rand() * 10.);
                let up = 10. + self.rand() * 40.;
                self.spawn(Kind::Glow, at, [-sin * back + carried[0] * 0.8, up, -cos * back + carried[1] * 0.8], life, size, -60., 0., color, 0.85);
            }
        }
        if f.driving && self.stars.get(k).copied().unwrap_or(false) {
            self.juice_carry[k][2] += 20. * dt;
            while self.juice_carry[k][2] >= 1. {
                self.juice_carry[k][2] -= 1.;
                let (a, r) = (self.rand() * std::f64::consts::TAU, 30. + self.rand() * 50.);
                let color = self.rainbow(0.72);
                let at = [f.pos[0] + a.cos() * r * 0.6, f.pos[1] + 10. + self.rand() * 70., f.pos[2] + a.sin() * r * 0.6];
                self.spawn(Kind::Star, at, [carried[0] * 0.85, 50., carried[1] * 0.85], 0.4, 24., -16., 0., color, 1.);
            }
        }
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
        if self.prev_drift.len() < frames.len() {
            self.prev_drift.resize(frames.len(), (false, 0));
            self.juice_carry.resize(frames.len(), [0.; 3]);
        }
        if self.vels.len() < frames.len() {
            self.vels.resize(frames.len(), [0.; 2]);
        }
        for (k, f) in frames.iter().enumerate() {
            let rates = fx_rates(f);
            // The kart's own horizontal velocity: the dust and smoke carry a share of it, so the trail hangs behind
            // the kart in view instead of flying out of the bottom of the screen.
            let carried = match self.last[k] {
                Some(l) if dt > 0. && ((f.pos[0] - l[0]).hypot(f.pos[2] - l[2])) < 400. => [(f.pos[0] - l[0]) / dt, (f.pos[2] - l[2]) / dt],
                _ => [0., 0.],
            };
            self.last[k] = Some(f.pos);
            // The position may only step at the 30 Hz tick (4x the speed one frame, nothing the next three), so what
            // rides along with the kart uses a smoothed velocity.
            let blend = 1. - (-dt * 10.).exp();
            let steady = [self.vels[k][0] + (carried[0] - self.vels[k][0]) * blend, self.vels[k][1] + (carried[1] - self.vels[k][1]) * blend];
            self.vels[k] = steady;
            let share = |v: f64| [carried[0] * v, carried[1] * v];
            self.juice(k, f, dt, steady);
            self.carry[k][0] += rates.star * dt;
            while self.carry[k][0] >= 1. {
                self.carry[k][0] -= 1.;
                let a = self.rand() * std::f64::consts::TAU;
                // A tight halo of stars circling just over the driver's head.
                let at = [f.pos[0] + a.cos() * 30., f.pos[1] + 64. + self.rand() * 12., f.pos[2] + a.sin() * 30.];
                let c = steady;
                let color = if self.rand() < 0.3 { 0xffffff } else { 0xffe45a };
                self.spawn(Kind::Star, at, [-a.sin() * 80. + c[0], 14., a.cos() * 80. + c[1]], 0.45, 34., -20., 0., color, 1.);
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

    fn live(fx: &KartFx) -> usize {
        fx.alive()
    }

    #[test]
    fn drift_sparks_step_up_with_the_charge() {
        assert!(spark_rate(0, true) < spark_rate(1, true) && spark_rate(1, true) < spark_rate(2, true));
        assert!(spark_rate(2, false) < spark_rate(2, true));
        assert_eq!(spark_rate(9, true), spark_rate(2, true));
        let colours: Vec<u32> = (0..3).map(|s| spark_colour(s, 0.)).collect();
        assert!(colours[0] != colours[1] && colours[1] != colours[2]);
        // Not drifting: no sparks at all. Drifting: more of them the higher the charge.
        let count = |drifting: bool, state: i32| {
            let mut fx = KartFx::default();
            let mut f = frame();
            f.puff.drifting = drifting;
            f.puff.drift_state = state;
            for _ in 0..12 {
                fx.update(1. / 60., &[f]);
            }
            fx.pool.iter().filter(|p| p.alive && matches!(p.kind, Kind::Streak)).count()
        };
        assert_eq!(count(false, 2), 0);
        assert!(count(true, 0) > 0);
        assert!(count(true, 2) > count(true, 0));
    }

    #[test]
    fn letting_go_of_a_charged_drift_kicks_a_burst() {
        let release = |state: i32| {
            let mut fx = KartFx::default();
            let mut f = frame();
            f.puff.drifting = true;
            f.puff.drift_state = state;
            fx.update(1. / 60., &[f]);
            let before = fx.pool.iter().filter(|p| p.alive && matches!(p.kind, Kind::Streak)).count();
            f.puff.drifting = false;
            f.puff.drift_state = 0;
            fx.update(1. / 60., &[f]);
            fx.pool.iter().filter(|p| p.alive && matches!(p.kind, Kind::Streak)).count() as i64 - before as i64
        };
        assert!(release(2) >= 8);
        assert!(release(0) <= 0);
    }

    #[test]
    fn bursts_stay_inside_the_pool() {
        let mut fx = KartFx::default();
        for i in 0..40 {
            let at = [i as f64, 0., 0.];
            fx.burst(at);
            fx.confetti(at);
            fx.go(at);
            fx.lap(at, true);
            fx.landed(at, 0xff0000);
            for what in [Impact::Shell, Impact::Banana, Impact::Blast, Impact::Thunder, Impact::Bump, Impact::Other] {
                fx.impact(at, what, 0x33cc33);
            }
        }
        assert!(live(&fx) <= POOL);
        // Everything fades out in the end.
        for _ in 0..240 {
            fx.update(1. / 60., &[]);
        }
        assert_eq!(live(&fx), 0);
    }

    #[test]
    fn particles_fade_as_the_camera_reaches_them() {
        assert_eq!(near_fade(50., 40.), 0.);
        assert_eq!(near_fade(50., 400.), 1.);
        assert!(near_fade(50., 120.) > 0. && near_fade(50., 120.) < 1.);
    }

    #[test]
    fn a_pickup_burst_rides_along_with_the_kart_that_took_it() {
        let mut fx = KartFx::default();
        let mut f = frame();
        for _ in 0..60 {
            f.pos[0] += 20.;
            fx.update(1. / 60., &[f]);
        }
        let before = fx.alive();
        fx.burst(f.pos);
        assert!(before + 35 <= POOL);
        let mean = fx.pool.iter().filter(|p| p.alive && p.age == 0.).map(|p| p.velocity()[0]).sum::<f64>() / 35.;
        assert!(mean > 600., "the shards keep most of the kart's 1200 u/s: {mean}");
        // A box smashed far from any kart bursts in place.
        let mut far = KartFx::default();
        far.update(1. / 60., &[f]);
        far.burst([9000., 0., 9000.]);
        let mean = far.pool.iter().filter(|p| p.alive).map(|p| p.velocity()[0]).sum::<f64>() / 35.;
        assert!(mean.abs() < 150., "{mean}");
    }

    #[test]
    fn a_star_trails_sparkles() {
        let mut fx = KartFx::default();
        fx.set_star(0, true);
        for _ in 0..30 {
            fx.update(1. / 60., &[frame()]);
        }
        assert!(fx.pool.iter().any(|p| p.alive && matches!(p.kind, Kind::Star)));
        let mut fx = KartFx::default();
        for _ in 0..30 {
            fx.update(1. / 60., &[frame()]);
        }
        assert!(!fx.pool.iter().any(|p| p.alive && matches!(p.kind, Kind::Star)));
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
        assert_eq!(fx.alive(), 35);
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
