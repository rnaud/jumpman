//! Sprite particle pool for dust, landing puffs, sparks and splashes (`Particles.ts`).
//! The pool logic is engine-free and unit tested; [`ParticlesPlugin`] draws it as camera-facing
//! unlit quads and feeds it from [`SimEventMsg`]. All quantities are in SM64 units (x0.01 on screen).
use crate::camera::MainCamera;
use crate::game::runner::{GameSim, SimEvent};
use crate::render::SCALE;
use crate::sim_plugin::SimEventMsg;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::MarioEvent;

pub const CAPACITY: usize = 360;

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub alive: bool,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub life: f32,
    pub max_life: f32,
    pub gravity: f32,
    pub size: f32,
    pub spin: f32,
    pub rotation: f32,
    pub color: [f32; 3],
    pub opacity: f32,
    /// Current on-screen size (SM64 units).
    pub scale: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self { alive: false, pos: [0.0; 3], vel: [0.0; 3], life: 0.0, max_life: 1.0, gravity: 0.0, size: 20.0, spin: 0.0, rotation: 0.0, color: [1.0; 3], opacity: 0.0, scale: 20.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BurstOptions {
    pub color: u32,
    pub count: usize,
    pub speed: f32,
    pub life: f32,
    pub gravity: f32,
    pub size: f32,
    pub upward: f32,
    pub spread: f32,
}

impl Default for BurstOptions {
    fn default() -> Self {
        Self { color: 0xf3e6c8, count: 8, speed: 110.0, life: 0.5, gravity: 180.0, size: 26.0, upward: 40.0, spread: 1.0 }
    }
}

/// Small xorshift RNG (the TS uses `Math.random`; visuals only).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
}

pub fn hex_rgb(hex: u32) -> [f32; 3] {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0]
}

#[derive(Resource)]
pub struct Particles {
    pub pool: Vec<Particle>,
    cursor: usize,
    rng: Rng,
}

impl Default for Particles {
    fn default() -> Self {
        Self::new(CAPACITY)
    }
}

impl Particles {
    pub fn new(capacity: usize) -> Self {
        Self { pool: vec![Particle::default(); capacity], cursor: 0, rng: Rng::new(0x9e3779b97f4a7c15) }
    }

    pub fn clear(&mut self) {
        for p in &mut self.pool {
            p.life = 0.0;
            p.alive = false;
        }
    }

    pub fn alive(&self) -> usize {
        self.pool.iter().filter(|p| p.alive).count()
    }

    pub fn burst(&mut self, origin: [f64; 3], o: BurstOptions) {
        let color = hex_rgb(o.color);
        for _ in 0..o.count {
            let i = self.cursor;
            self.cursor = (self.cursor + 1) % self.pool.len();
            let r = &mut self.rng;
            let (a, rad, v, l, s, sp, rot, jx, jz) = (r.next() * std::f32::consts::TAU, r.next() * o.speed, r.next(), r.next(), r.next(), r.next(), r.next(), r.next(), r.next());
            let life = o.life * (0.7 + l * 0.6);
            let size = o.size * (0.6 + s * 0.8);
            self.pool[i] = Particle {
                alive: true,
                pos: [origin[0] as f32 + (jx - 0.5) * 18.0, origin[1] as f32, origin[2] as f32 + (jz - 0.5) * 18.0],
                vel: [a.cos() * rad * o.spread, o.upward + v * o.speed * 0.65, a.sin() * rad * o.spread],
                life,
                max_life: life,
                gravity: o.gravity,
                size,
                spin: (sp - 0.5) * 2.0,
                rotation: rot * std::f32::consts::PI,
                color,
                opacity: 0.9,
                scale: size,
            };
        }
    }

    pub fn dust(&mut self, pos: [f64; 3], count: usize) {
        self.burst(pos, BurstOptions { color: 0xf0e2c0, count, speed: 90.0, life: 0.55, gravity: 150.0, size: 30.0, upward: 30.0, spread: 1.0 });
    }

    pub fn sparks(&mut self, pos: [f64; 3], count: usize) {
        self.burst(pos, BurstOptions { color: 0xffdd70, count, speed: 330.0, life: 0.32, gravity: 720.0, size: 18.0, upward: 120.0, spread: 1.0 });
    }

    pub fn pound(&mut self, pos: [f64; 3]) {
        self.burst(pos, BurstOptions { color: 0xf6e5b8, count: 26, speed: 380.0, life: 0.65, gravity: 260.0, size: 34.0, upward: 160.0, spread: 1.0 });
        self.burst(pos, BurstOptions { color: 0xffd35e, count: 10, speed: 260.0, life: 0.4, gravity: 600.0, size: 16.0, upward: 220.0, spread: 1.0 });
    }

    /// Splash when entering/leaving the water volume (`updateEnvironment`).
    pub fn splash(&mut self, pos: [f64; 3]) {
        self.burst(pos, BurstOptions { color: 0x7fd5ff, count: 18, speed: 90.0, life: 0.7, gravity: 120.0, size: 7.0, upward: 80.0, spread: 1.0 });
    }

    /// Water-bomb burst.
    pub fn water_bomb(&mut self, pos: [f64; 3]) {
        self.burst(pos, BurstOptions { color: 0xbbe5ff, count: 5, speed: 700.0, life: 0.5, gravity: 1800.0, size: 35.0, upward: 300.0, spread: 1.0 });
    }

    pub fn update(&mut self, dt: f32) {
        for p in self.pool.iter_mut().filter(|p| p.alive) {
            p.life -= dt;
            if p.life <= 0.0 {
                p.alive = false;
                p.opacity = 0.0;
                continue;
            }
            p.vel[1] -= p.gravity * dt;
            for k in 0..3 {
                p.pos[k] += p.vel[k] * dt;
            }
            p.rotation += p.spin * dt;
            let ratio = p.life / p.max_life;
            p.opacity = (ratio * 1.6).min(1.0) * 0.85;
            p.scale = p.size * (0.7 + (1.0 - ratio) * 0.9);
        }
    }

    /// `processEvents` / `processObjectEvents` particle reactions.
    pub fn on_event(&mut self, event: &SimEvent, mario_pos: [f64; 3]) {
        let feet = mario_pos;
        let head = [mario_pos[0], mario_pos[1] + 95.0, mario_pos[2]];
        match event {
            SimEvent::Mario(e) => match e {
                MarioEvent::Jump { .. } | MarioEvent::Longjump => self.dust(feet, 4),
                MarioEvent::Triplejump => self.sparks(feet, 12),
                MarioEvent::Land { vy, .. } => {
                    let strength = (vy.abs() / 45.0).min(1.7);
                    self.dust(feet, (5.0 + strength * 6.0).round() as usize);
                }
                MarioEvent::Bonk { .. } => self.sparks(head, 8),
                MarioEvent::Wallkick => self.sparks(head, 12),
                MarioEvent::Groundpound => self.dust(feet, 3),
                MarioEvent::GroundpoundLand => self.pound(feet),
                MarioEvent::Respawn => self.dust(feet, 12),
                _ => {}
            },
            SimEvent::Object(e) => match e {
                ObjectEvent::WaterBomb { pos, phase } if *phase == "burst" => self.water_bomb(*pos),
                ObjectEvent::Explosion { pos, .. } => {
                    self.pound(*pos);
                    self.sparks(*pos, 18);
                }
                ObjectEvent::Break { pos, .. } => self.dust(*pos, 16),
                ObjectEvent::Bowser { pos } => {
                    self.pound(*pos);
                    self.sparks(*pos, 40);
                }
                ObjectEvent::BigBully { pos } | ObjectEvent::WhompKing { pos } | ObjectEvent::BigBoo { pos } | ObjectEvent::Boss { pos } => {
                    self.pound(*pos);
                    self.sparks(*pos, 30);
                }
                ObjectEvent::OneUp { pos } => self.sparks(*pos, 20),
                ObjectEvent::BowlingBall { pos, cue } if *cue == "end" => self.dust(*pos, 16),
                _ => {}
            },
            SimEvent::CoinCollected { .. } => {}
        }
    }
}

#[derive(Component)]
pub struct ParticleSprite(usize);

impl ParticleSprite {
    /// The pool slot this sprite draws.
    pub fn index(&self) -> usize {
        self.0
    }
}

/// 128x128 white disc with the TS radial gradient alpha (1 at r=2px, .75 at 45 %, 0 at the rim).
pub fn soft_texture() -> Image {
    let size = 128usize;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let d = (((x as f32 + 0.5 - 64.0).powi(2) + (y as f32 + 0.5 - 64.0).powi(2)).sqrt() - 2.0).max(0.0) / 62.0;
            let a = if d <= 0.45 { 1.0 - 0.25 * d / 0.45 } else { (0.75 * (1.0 - (d - 0.45) / 0.55)).max(0.0) };
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: 128, height: 128, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Particles>().add_systems(Startup, spawn_sprites.in_set(crate::progress::PersistentSpawn)).add_systems(Update, (feed_events, demo_bursts, step_particles, draw_particles).chain().after(crate::CameraSet).after(crate::experiments::apply_camera));
    }
}

fn spawn_sprites(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>, particles: Res<Particles>) {
    let texture = images.add(soft_texture());
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    for i in 0..particles.pool.len() {
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(texture.clone()),
            base_color: Color::srgba(1., 1., 1., 0.),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        });
        commands.spawn((ParticleSprite(i), super::FxSprite, Mesh3d(quad.clone()), MeshMaterial3d(material), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
    }
}

fn feed_events(mut events: MessageReader<SimEventMsg>, sim: Res<GameSim>, mut particles: ResMut<Particles>) {
    for SimEventMsg(e) in events.read() {
        particles.on_event(e, sim.mario.pos);
    }
}

/// Test hook: `JUMPMAN_FX_DEMO=1` fires a ground-pound burst at Mario every 0.5 s.
fn demo_bursts(time: Res<Time>, sim: Res<GameSim>, mut particles: ResMut<Particles>, mut last: Local<f32>) {
    if std::env::var_os("JUMPMAN_FX_DEMO").is_none() || time.elapsed_secs() - *last < 0.5 {
        return;
    }
    *last = time.elapsed_secs();
    particles.pound(sim.mario.pos);
}

fn step_particles(time: Res<Time>, mut particles: ResMut<Particles>) {
    particles.update(time.delta_secs().min(0.1));
}

/// The soft sprite's material is absent while a minigame draws the pool in the N64 look (`fx::n64_modes`).
fn draw_particles(
    particles: Res<Particles>,
    camera: Single<&Transform, (With<MainCamera>, Without<ParticleSprite>)>,
    mut sprites: Query<(&ParticleSprite, &mut Transform, &mut Visibility, Option<&MeshMaterial3d<StandardMaterial>>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (ParticleSprite(i), mut tf, mut vis, mat) in &mut sprites {
        let p = &particles.pool[*i];
        if !p.alive {
            // `set_if_neq`: a plain assignment marks the component changed every frame (360 pooled sprites)
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        vis.set_if_neq(Visibility::Inherited);
        tf.translation = Vec3::from(p.pos) * SCALE;
        tf.rotation = camera.rotation * Quat::from_rotation_z(p.rotation);
        tf.scale = Vec3::splat(p.scale * SCALE);
        if let Some(mut m) = mat.and_then(|mat| materials.get_mut(&mat.0)) {
            m.base_color = Color::srgba(p.color[0], p.color[1], p.color[2], p.opacity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_spawns_and_expires() {
        let mut p = Particles::new(50);
        p.dust([0.0; 3], 6);
        assert_eq!(p.alive(), 6);
        for _ in 0..20 {
            p.update(0.05);
        }
        assert_eq!(p.alive(), 0);
    }

    #[test]
    fn gravity_and_fade() {
        let mut p = Particles::new(4);
        p.burst([0.0; 3], BurstOptions { count: 1, speed: 0.0, upward: 0.0, gravity: 100.0, life: 1.0, ..Default::default() });
        p.update(0.1);
        let q = p.pool[0];
        assert!(q.vel[1] < -9.9 && q.pos[1] < 0.0);
        assert!(q.opacity <= 0.85 + 1e-6 && q.opacity > 0.0);
    }

    #[test]
    fn pool_wraps() {
        let mut p = Particles::new(10);
        p.sparks([0.0; 3], 25);
        assert_eq!(p.alive(), 10);
    }

    #[test]
    fn events_map_to_bursts() {
        let mut p = Particles::new(CAPACITY);
        p.on_event(&SimEvent::Mario(MarioEvent::GroundpoundLand), [0.0; 3]);
        assert_eq!(p.alive(), 36);
        p.clear();
        // strength = min(1.7, 90/45) = 1.7 -> round(5 + 10.2) = 15
        p.on_event(&SimEvent::Mario(MarioEvent::Land { speed: 0.0, vy: -90.0 }), [0.0; 3]);
        assert_eq!(p.alive(), 15);
    }

    #[test]
    fn rng_in_unit_range() {
        let mut r = Rng::new(1);
        for _ in 0..1000 {
            let v = r.next();
            assert!((0.0..1.0).contains(&v));
        }
    }
}
