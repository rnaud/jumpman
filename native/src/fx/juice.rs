//! Game-feel effects for the Mario modes: four-point sparkles (coin, star and 1-up pickups) and expanding rings
//! (ground pounds, stomps, hard landings, hits), all procedural. Two small fixed pools, so the cost is bounded:
//! [`SPARKLES`] quads that share one material per tint (they fade by shrinking, never by alpha, so they batch) and
//! [`RINGS`] quads with a material each. The pool logic is engine-free and unit tested; [`JuicePlugin`] draws it and
//! feeds it from the sim's events, the goomba cues and the sound cues the progression layer and the experiments send.
//! All quantities are in SM64 units (x0.01 on screen).
use super::particles::{BurstOptions, Particles, Rng};
use crate::camera::MainCamera;
use crate::game::runner::{GameSim, SimEvent};
use crate::render::SCALE;
use crate::sim_plugin::SimEventMsg;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::*;

pub const SPARKLES: usize = 96;
pub const RINGS: usize = 6;

/// Sparkle colours (one shared material each).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint {
    Gold,
    Red,
    Blue,
    White,
    Green,
}

impl Tint {
    pub const ALL: [Tint; 5] = [Tint::Gold, Tint::Red, Tint::Blue, Tint::White, Tint::Green];
    fn rgb(self) -> [f32; 3] {
        match self {
            Tint::Gold => [1.0, 0.80, 0.16],
            Tint::Red => [1.0, 0.27, 0.22],
            Tint::Blue => [0.30, 0.56, 1.0],
            Tint::White => [0.92, 0.96, 1.0],
            Tint::Green => [0.36, 0.92, 0.36],
        }
    }
    /// The coin colour for a coin value (yellow 1, red 2, blue 5).
    pub fn for_coin(value: i32) -> Tint {
        match value {
            2 => Tint::Red,
            v if v >= 5 => Tint::Blue,
            _ => Tint::Gold,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sparkle {
    pub alive: bool,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub age: f32,
    pub life: f32,
    pub gravity: f32,
    pub size: f32,
    pub spin: f32,
    pub rotation: f32,
    pub tint: Tint,
}

impl Sparkle {
    const DEAD: Sparkle = Sparkle { alive: false, pos: [0.0; 3], vel: [0.0; 3], age: 0.0, life: 1.0, gravity: 0.0, size: 0.0, spin: 0.0, rotation: 0.0, tint: Tint::Gold };

    /// On-screen size: pops to full size over the first fifth of its life, then shrinks to nothing.
    pub fn scale(&self) -> f32 {
        if !self.alive {
            return 0.0;
        }
        let k = (self.age / self.life).clamp(0.0, 1.0);
        let pop = if k < 0.2 { k / 0.2 } else { 1.0 - (k - 0.2) / 0.8 };
        self.size * pop * (0.85 + 0.15 * (self.age * 38.0).sin())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub alive: bool,
    pub pos: [f32; 3],
    pub age: f32,
    pub life: f32,
    /// Start and end radius.
    pub r0: f32,
    pub r1: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    /// Face the camera (hits, pickups) instead of lying on the ground (pounds, landings).
    pub billboard: bool,
}

impl Ring {
    const DEAD: Ring = Ring { alive: false, pos: [0.0; 3], age: 0.0, life: 1.0, r0: 0.0, r1: 0.0, color: [1.0; 3], alpha: 0.0, billboard: false };

    /// Radius now: fast at first, easing out.
    pub fn radius(&self) -> f32 {
        let k = (self.age / self.life).clamp(0.0, 1.0);
        self.r0 + (self.r1 - self.r0) * (1.0 - (1.0 - k).powi(3))
    }
    pub fn opacity(&self) -> f32 {
        if !self.alive {
            return 0.0;
        }
        let k = (self.age / self.life).clamp(0.0, 1.0);
        self.alpha * (1.0 - k * k)
    }
}

#[derive(Resource)]
pub struct Juice {
    pub sparkles: Vec<Sparkle>,
    pub rings: Vec<Ring>,
    cursor: usize,
    ring_cursor: usize,
    rng: Rng,
}

impl Default for Juice {
    fn default() -> Self {
        Self { sparkles: vec![Sparkle::DEAD; SPARKLES], rings: vec![Ring::DEAD; RINGS], cursor: 0, ring_cursor: 0, rng: Rng::new(0x51ed_270b_9f3c_11a7) }
    }
}

const DUST: [f32; 3] = [0.96, 0.91, 0.78];

impl Juice {
    pub fn clear(&mut self) {
        self.sparkles.fill(Sparkle::DEAD);
        self.rings.fill(Ring::DEAD);
    }

    pub fn alive(&self) -> (usize, usize) {
        (self.sparkles.iter().filter(|s| s.alive).count(), self.rings.iter().filter(|r| r.alive).count())
    }

    /// `count` sparkles flying out of `pos` in every direction (biased upward by `lift`).
    pub fn twinkle(&mut self, pos: [f64; 3], tint: Tint, count: usize, speed: f32, size: f32, life: f32, lift: f32) {
        for _ in 0..count.min(SPARKLES) {
            let r = &mut self.rng;
            let (a, z, v, l, s, sp, rot) = (r.next() * std::f32::consts::TAU, r.next() * 2.0 - 1.0, 0.45 + r.next() * 0.55, r.next(), r.next(), r.next(), r.next());
            let h = (1.0 - z * z).max(0.0).sqrt();
            let i = self.cursor;
            self.cursor = (self.cursor + 1) % self.sparkles.len();
            self.sparkles[i] = Sparkle {
                alive: true,
                pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32],
                vel: [a.cos() * h * speed * v, z * speed * v + lift, a.sin() * h * speed * v],
                age: 0.0,
                life: life * (0.7 + l * 0.6),
                gravity: speed * 1.4,
                size: size * 1.6 * (0.65 + s * 0.7),
                spin: (sp - 0.5) * 9.0,
                rotation: rot * std::f32::consts::PI,
                tint,
            };
        }
    }

    pub fn ring(&mut self, pos: [f64; 3], r0: f32, r1: f32, life: f32, color: [f32; 3], alpha: f32, billboard: bool) {
        let i = self.ring_cursor;
        self.ring_cursor = (self.ring_cursor + 1) % self.rings.len();
        self.rings[i] = Ring { alive: true, pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32], age: 0.0, life, r0, r1, color, alpha, billboard };
    }

    /// A coin worth `value` was taken at `pos`.
    pub fn coin(&mut self, pos: [f64; 3], value: i32) {
        let tint = Tint::for_coin(value);
        self.twinkle(pos, tint, if value >= 5 { 9 } else { 5 }, 210.0, 34.0, 0.38, 120.0);
        self.twinkle(pos, Tint::White, 2, 120.0, 22.0, 0.3, 90.0);
    }

    /// A power star was collected (or a mode was won) at `pos`.
    pub fn star(&mut self, pos: [f64; 3]) {
        self.twinkle(pos, Tint::Gold, 22, 520.0, 58.0, 0.85, 260.0);
        self.twinkle(pos, Tint::White, 10, 330.0, 40.0, 0.65, 200.0);
        self.ring(pos, 30.0, 330.0, 0.5, [1.0, 0.9, 0.45], 0.85, true);
        self.ring(pos, 10.0, 190.0, 0.7, [1.0, 1.0, 0.9], 0.6, true);
    }

    pub fn one_up(&mut self, pos: [f64; 3]) {
        self.twinkle(pos, Tint::Green, 12, 300.0, 44.0, 0.6, 220.0);
        self.twinkle(pos, Tint::White, 4, 180.0, 28.0, 0.45, 160.0);
        self.ring(pos, 20.0, 170.0, 0.4, [0.55, 1.0, 0.55], 0.7, true);
    }

    /// Something popped in or out of existence (teleports, appearing stars, switches).
    pub fn shimmer(&mut self, pos: [f64; 3]) {
        self.twinkle(pos, Tint::White, 10, 260.0, 40.0, 0.5, 140.0);
        self.ring(pos, 20.0, 160.0, 0.35, [0.9, 0.95, 1.0], 0.6, true);
    }

    /// Mario took a hit: a white flash ring and a few stars thrown off him.
    pub fn hurt(&mut self, pos: [f64; 3], damage: f64) {
        let big = damage >= 8.0;
        self.twinkle(pos, Tint::White, if big { 9 } else { 6 }, 380.0, 46.0, 0.45, 180.0);
        self.twinkle(pos, Tint::Red, 3, 260.0, 34.0, 0.4, 160.0);
        self.ring(pos, 20.0, if big { 210.0 } else { 150.0 }, 0.28, [1.0, 0.96, 0.9], 0.8, true);
    }

    /// A shockwave on the floor (ground pound, stomp, heavy landing).
    pub fn shockwave(&mut self, feet: [f64; 3], radius: f32, alpha: f32) {
        self.ring([feet[0], feet[1] + 4.0, feet[2]], radius * 0.18, radius, 0.3 + radius / 1400.0, DUST, alpha, false);
    }

    pub fn update(&mut self, dt: f32) {
        for s in self.sparkles.iter_mut().filter(|s| s.alive) {
            s.age += dt;
            if s.age >= s.life {
                s.alive = false;
                continue;
            }
            // Air drag: they burst out and hang.
            let drag = (1.0 - 3.2 * dt).max(0.0);
            s.vel[0] *= drag;
            s.vel[2] *= drag;
            s.vel[1] = s.vel[1] * drag - s.gravity * dt;
            for k in 0..3 {
                s.pos[k] += s.vel[k] * dt;
            }
            s.rotation += s.spin * dt;
        }
        for r in self.rings.iter_mut().filter(|r| r.alive) {
            r.age += dt;
            if r.age >= r.life {
                r.alive = false;
            }
        }
    }

    /// Reactions to one sim event of a Mario mode. `particles` gets the dust that goes with a ring.
    pub fn on_event(&mut self, event: &SimEvent, mario: [f64; 3], particles: &mut Particles) {
        let feet = mario;
        let body = [mario[0], mario[1] + 70.0, mario[2]];
        match event {
            SimEvent::CoinCollected { value } => self.coin(body, *value),
            SimEvent::Mario(e) => match e {
                MarioEvent::Land { vy, .. } if vy.abs() >= 55.0 => self.shockwave(feet, 150.0 + (vy.abs() as f32 - 55.0).min(60.0) * 1.5, 0.5),
                MarioEvent::GroundpoundLand => self.shockwave(feet, 340.0, 0.8),
                MarioEvent::Hurt { damage } => self.hurt(body, *damage),
                MarioEvent::Wallkick => particles.dust(body, 5),
                MarioEvent::Dive => particles.dust(feet, 3),
                MarioEvent::Step { speed } if speed.abs() >= 26.0 => particles.burst(feet, BurstOptions { count: 1, speed: 40.0, life: 0.32, gravity: 40.0, size: 20.0, upward: 22.0, ..Default::default() }),
                MarioEvent::WaterPlunge => {
                    particles.splash(body);
                    self.ring(body, 20.0, 200.0, 0.45, [0.75, 0.9, 1.0], 0.7, false);
                }
                MarioEvent::CannonFire => {
                    particles.pound(feet);
                    self.shockwave(feet, 260.0, 0.7);
                }
                _ => {}
            },
            SimEvent::Object(e) => match e {
                ObjectEvent::OneUp { pos } => self.one_up(*pos),
                ObjectEvent::Explosion { pos, radius, .. } => self.ring(*pos, 40.0, (*radius as f32).clamp(200.0, 520.0), 0.4, [1.0, 0.8, 0.4], 0.85, true),
                ObjectEvent::Break { pos, .. } => self.ring(*pos, 30.0, 220.0, 0.32, DUST, 0.7, true),
                ObjectEvent::BoxStar { pos, .. } | ObjectEvent::DefaultStar { pos, .. } => self.shimmer(*pos),
                ObjectEvent::CoinBounce { .. } => {}
                _ => {}
            },
        }
    }
}

/// Whether Mario is in a ground action that drags dust (`set_mario_particle_flags(PARTICLE_DUST)` in the slide and brake
/// actions of the decomp).
pub fn drags_dust(action: u32, forward_vel: f64) -> bool {
    matches!(action, ACT_BRAKING | ACT_TURNING_AROUND | ACT_BUTT_SLIDE | ACT_STOMACH_SLIDE | ACT_DIVE_SLIDE | ACT_CROUCH_SLIDE | ACT_SLIDE_KICK_SLIDE) && forward_vel.abs() >= 6.0
}

/// 64x64 four-point star: `tint` at the tips, white-hot core, over a faint round glow.
pub fn sparkle_texture(tint: [f32; 3]) -> Image {
    let size = 64usize;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = ((x as f32 + 0.5) / 32.0 - 1.0, (y as f32 + 0.5) / 32.0 - 1.0);
            let star = (1.0 - (u.abs().sqrt() + v.abs().sqrt())).max(0.0);
            let glow = (1.0 - (u * u + v * v).sqrt()).max(0.0).powi(2) * 0.4;
            let a = (star * 3.0).min(1.0).max(glow);
            let w = (star * 2.2).min(1.0).powi(2);
            for c in tint {
                data.push(((c + (1.0 - c) * w) * 255.0) as u8);
            }
            data.push((a * 255.0) as u8);
        }
    }
    Image::new(Extent3d { width: 64, height: 64, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// 128x128 soft ring: a bright band at 86 % of the radius with a faint fill inside.
pub fn ring_texture() -> Image {
    let size = 128usize;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let r = (((x as f32 + 0.5) / 64.0 - 1.0).powi(2) + ((y as f32 + 0.5) / 64.0 - 1.0).powi(2)).sqrt();
            let band = (1.0 - ((r - 0.86) / 0.11).abs()).max(0.0);
            let fill = if r < 0.86 { 0.10 * r } else { 0.0 };
            let a = (band * band * (3.0 - 2.0 * band)).max(fill);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: 128, height: 128, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

#[derive(Component)]
struct SparkleSprite(usize);
#[derive(Component)]
struct RingSprite(usize);

#[derive(Resource)]
struct SparkleMaterials([Handle<StandardMaterial>; 5]);

pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Juice>()
            .add_systems(Startup, spawn_sprites.in_set(crate::progress::PersistentSpawn))
            .add_systems(FixedUpdate, slide_dust.after(crate::SimSet))
            .add_systems(Update, (feed_events, demo, step, draw).chain().after(crate::CameraSet).after(crate::experiments::apply_camera));
    }
}

fn spawn_sprites(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let mats = Tint::ALL.map(|t| {
        materials.add(StandardMaterial { base_color_texture: Some(images.add(sparkle_texture(t.rgb()))), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() })
    });
    for i in 0..SPARKLES {
        commands.spawn((SparkleSprite(i), super::FxSprite, Mesh3d(quad.clone()), MeshMaterial3d(mats[0].clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
    }
    commands.insert_resource(SparkleMaterials(mats));
    let ring = images.add(ring_texture());
    for i in 0..RINGS {
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(ring.clone()),
            base_color: Color::srgba(1., 1., 1., 0.),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            // Ground rings lie a few units above the floor; the bias keeps them from fighting it at a distance.
            depth_bias: 8.0,
            ..default()
        });
        commands.spawn((RingSprite(i), super::FxSprite, Mesh3d(quad.clone()), MeshMaterial3d(material), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
    }
}

/// The modes this module decorates: the SM64 courses played as Mario and the Mario-based experiments. The other modes
/// (kart, the original games, Link, Banjo) bring their own effects.
fn decorated(sim: &GameSim, slot: &crate::experiments::ExperimentSlot) -> bool {
    use crate::experiments::ExperimentId as X;
    !sim.as_link && !sim.as_banjo && matches!(slot.id, None | Some(X::Dimensions | X::God | X::Runner | X::Hundred | X::Diorama | X::Race | X::Shrink | X::Planet | X::Party))
}

#[allow(clippy::too_many_arguments)]
fn feed_events(
    mut events: MessageReader<SimEventMsg>,
    mut goombas: MessageReader<crate::objects::goombas::GoombaFxMsg>,
    mut cues: MessageReader<crate::audio::SfxCue>,
    sim: Res<GameSim>,
    slot: Res<crate::experiments::ExperimentSlot>,
    mut juice: ResMut<Juice>,
    mut particles: ResMut<Particles>,
) {
    use crate::audio::SfxCue;
    use crate::game::goomba::GoombaEvent;
    if !decorated(&sim, &slot) {
        events.clear();
        goombas.clear();
        cues.clear();
        return;
    }
    let feet = sim.mario.pos;
    let body = [feet[0], feet[1] + 70.0, feet[2]];
    for SimEventMsg(e) in events.read() {
        juice.on_event(e, feet, &mut particles);
    }
    for g in goombas.read() {
        match g.0 {
            GoombaEvent::Stomp => {
                juice.shockwave(feet, 190.0, 0.75);
                juice.twinkle(feet, Tint::White, 5, 260.0, 36.0, 0.35, 170.0);
                particles.dust(feet, 8);
            }
            GoombaEvent::Sparks(pos) => {
                particles.sparks(pos, 10);
                juice.ring(pos, 20.0, 150.0, 0.25, [1.0, 0.95, 0.8], 0.75, true);
            }
            GoombaEvent::Dust(pos) => particles.dust(pos, 9),
            GoombaEvent::Hurt | GoombaEvent::Coin(_) => {}
        }
    }
    for cue in cues.read() {
        match cue {
            SfxCue::Star => juice.star(body),
            SfxCue::OneUp => juice.one_up(body),
            SfxCue::Teleport | SfxCue::CapSwitch => juice.shimmer(body),
            SfxCue::SpawnedStar { pos, appearing: true } => juice.shimmer(*pos),
            SfxCue::Effect { kind, pos } => {
                let at = pos.unwrap_or(body);
                // Far-away cues (the crowd modes fire them all over the map) are not worth a pool slot.
                if (at[0] - feet[0]).hypot(at[2] - feet[2]) > 4000.0 {
                    continue;
                }
                match *kind {
                    "star_sound" | "star" => juice.star(at),
                    "one_up" => juice.one_up(at),
                    "teleport" | "cap_switch" => juice.shimmer(at),
                    "coin" => juice.twinkle(at, Tint::Gold, 3, 190.0, 30.0, 0.34, 110.0),
                    "red_coin" | "secret" => juice.twinkle(at, Tint::Red, 5, 210.0, 34.0, 0.4, 120.0),
                    "stomp" => juice.shockwave(at, 190.0, 0.75),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// One puff every other tick while Mario brakes, turns around or slides.
fn slide_dust(sim: Res<GameSim>, slot: Res<crate::experiments::ExperimentSlot>, mut particles: ResMut<Particles>) {
    let m = &sim.mario;
    if sim.ticks % 2 == 0 && decorated(&sim, &slot) && drags_dust(m.action, m.forward_vel) && m.pos[1] <= m.floor_height + 4.0 {
        particles.burst(m.pos, BurstOptions { count: 1, speed: 55.0, life: 0.4, gravity: 30.0, size: 24.0, upward: 26.0, ..Default::default() });
    }
}

/// Test hook: `JUMPMAN_FX_DEMO=juice` cycles every effect at Mario (one per 0.6 s) for screenshots.
fn demo(time: Res<Time>, sim: Res<GameSim>, mut juice: ResMut<Juice>, mut state: Local<(f32, u32)>) {
    if std::env::var("JUMPMAN_FX_DEMO").map_or(true, |v| v != "juice") || time.elapsed_secs() - state.0 < 0.6 {
        return;
    }
    state.0 = time.elapsed_secs();
    state.1 += 1;
    let feet = sim.mario.pos;
    let body = [feet[0], feet[1] + 70.0, feet[2]];
    let side = |dx: f64| [body[0] + dx, body[1] + 40.0, body[2]];
    match state.1 % 4 {
        0 => {
            juice.coin(side(-220.0), 1);
            juice.coin(side(-120.0), 2);
            juice.coin(side(120.0), 5);
        }
        1 => juice.star(side(0.0)),
        2 => {
            juice.shockwave(feet, 340.0, 0.8);
            juice.one_up(side(220.0));
        }
        _ => juice.hurt(body, 8.0),
    }
}

fn step(time: Res<Time>, mut juice: ResMut<Juice>) {
    juice.update(time.delta_secs().min(0.1));
}

#[allow(clippy::type_complexity)]
fn draw(
    juice: Res<Juice>,
    mats: Option<Res<SparkleMaterials>>,
    camera: Single<&Transform, (With<MainCamera>, Without<SparkleSprite>, Without<RingSprite>)>,
    mut sparkles: Query<(&SparkleSprite, &mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>), Without<RingSprite>>,
    mut rings: Query<(&RingSprite, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), Without<SparkleSprite>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mats) = mats else { return };
    for (SparkleSprite(i), mut tf, mut vis, mut mat) in &mut sparkles {
        let s = &juice.sparkles[*i];
        if !s.alive {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        *vis = Visibility::Inherited;
        let want = &mats.0[s.tint as usize];
        if mat.0 != *want {
            mat.0 = want.clone();
        }
        tf.translation = Vec3::from(s.pos) * SCALE;
        tf.rotation = camera.rotation * Quat::from_rotation_z(s.rotation);
        tf.scale = Vec3::splat((s.scale() * SCALE).max(1e-5));
    }
    for (RingSprite(i), mut tf, mut vis, mat) in &mut rings {
        let r = &juice.rings[*i];
        if !r.alive {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        *vis = Visibility::Inherited;
        tf.translation = Vec3::from(r.pos) * SCALE;
        tf.rotation = if r.billboard { camera.rotation } else { Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2) };
        tf.scale = Vec3::splat(r.radius() * 2.0 * SCALE);
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color = Color::srgba(r.color[0], r.color[1], r.color[2], r.opacity());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pools_are_bounded() {
        let mut j = Juice::default();
        for _ in 0..40 {
            j.star([0.0; 3]);
            j.coin([0.0; 3], 5);
        }
        assert_eq!(j.alive(), (SPARKLES, RINGS));
        assert_eq!((j.sparkles.len(), j.rings.len()), (SPARKLES, RINGS));
    }

    #[test]
    fn sparkles_pop_then_shrink_to_nothing_and_die() {
        let mut j = Juice::default();
        j.twinkle([0.0, 100.0, 0.0], Tint::Gold, 1, 200.0, 40.0, 0.5, 0.0);
        let s = j.sparkles[0];
        assert_eq!(s.scale(), 0.0, "born at zero size");
        j.update(s.life * 0.2);
        let peak = j.sparkles[0].scale();
        assert!(peak > s.size * 0.6 && peak <= s.size * 1.01, "{peak} of {}", s.size);
        j.update(s.life * 0.7);
        assert!(j.sparkles[0].scale() < peak * 0.3);
        j.update(s.life * 0.2);
        assert_eq!(j.alive().0, 0);
        assert_eq!(j.sparkles[0].scale(), 0.0);
    }

    #[test]
    fn rings_grow_and_fade() {
        let mut j = Juice::default();
        j.shockwave([0.0; 3], 300.0, 0.8);
        let (mut last_r, mut last_a) = (j.rings[0].radius(), j.rings[0].opacity());
        assert!((last_a - 0.8).abs() < 1e-6);
        for _ in 0..10 {
            j.update(0.03);
            let r = j.rings[0];
            if !r.alive {
                break;
            }
            assert!(r.radius() >= last_r && r.radius() <= 300.0 + 1e-3);
            assert!(r.opacity() <= last_a);
            (last_r, last_a) = (r.radius(), r.opacity());
        }
        for _ in 0..30 {
            j.update(0.03);
        }
        assert_eq!(j.alive().1, 0);
        assert_eq!(j.rings[0].opacity(), 0.0);
    }

    #[test]
    fn events_map_to_effects() {
        let (mut j, mut p) = (Juice::default(), Particles::new(64));
        j.on_event(&SimEvent::CoinCollected { value: 1 }, [0.0; 3], &mut p);
        assert_eq!(j.alive(), (7, 0));
        assert!(j.sparkles[..5].iter().all(|s| s.tint == Tint::Gold));
        j.clear();
        j.on_event(&SimEvent::CoinCollected { value: 5 }, [0.0; 3], &mut p);
        // The pool is a ring, so the new ones are not necessarily at the front.
        assert_eq!(j.sparkles.iter().filter(|s| s.alive && s.tint == Tint::Blue).count(), 9);
        j.clear();
        // A soft landing leaves the floor alone, a hard one and a ground pound ring it.
        j.on_event(&SimEvent::Mario(MarioEvent::Land { speed: 0.0, vy: -30.0 }), [0.0; 3], &mut p);
        assert_eq!(j.alive(), (0, 0));
        j.on_event(&SimEvent::Mario(MarioEvent::Land { speed: 0.0, vy: -70.0 }), [0.0; 3], &mut p);
        j.on_event(&SimEvent::Mario(MarioEvent::GroundpoundLand), [0.0; 3], &mut p);
        assert_eq!(j.alive(), (0, 2));
        assert!(!j.rings[0].billboard && j.rings[1].r1 > j.rings[0].r1);
        j.on_event(&SimEvent::Mario(MarioEvent::Hurt { damage: 12.0 }), [0.0; 3], &mut p);
        assert_eq!(j.alive(), (12, 3));
        // Running footfalls only kick dust at speed.
        assert_eq!(p.alive(), 0);
        j.on_event(&SimEvent::Mario(MarioEvent::Step { speed: 10.0 }), [0.0; 3], &mut p);
        assert_eq!(p.alive(), 0);
        j.on_event(&SimEvent::Mario(MarioEvent::Step { speed: 30.0 }), [0.0; 3], &mut p);
        assert_eq!(p.alive(), 1);
    }

    #[test]
    fn dust_only_in_fast_ground_slides() {
        assert!(drags_dust(ACT_BRAKING, 20.0));
        assert!(drags_dust(ACT_BUTT_SLIDE, -12.0));
        assert!(!drags_dust(ACT_BRAKING, 2.0));
        assert!(!drags_dust(ACT_WALKING, 30.0));
    }

    #[test]
    fn textures_are_transparent_at_the_rim_and_solid_where_it_counts() {
        let s = sparkle_texture([1.0, 0.8, 0.2]);
        let d = s.data.as_ref().unwrap();
        assert_eq!(d[3], 0, "corner");
        assert_eq!(d[(32 * 64 + 32) * 4 + 3], 255, "core");
        let r = ring_texture();
        let d = r.data.as_ref().unwrap();
        assert_eq!(d[3], 0);
        assert!(d[(64 * 128 + 119) * 4 + 3] > 200, "band at 86 % of the radius");
    }
}
