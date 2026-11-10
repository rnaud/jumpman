//! Combat and item feedback for Link: the OoT effect set on top of the shared sprite particles (`fx/particles.rs`) and a
//! few translucent meshes.
//!
//! * hit sparks per weapon (`EffectSsHitMark`: white for blades and arrows, fire / ice / light for the elemental ones,
//!   `EffectSpark` for steel on steel), enemy colour filters (`Actor_SetColorFilter`: red damage flash, blue frozen,
//!   flames while burning, yellow sparks while stunned), the death burst (`EffectSsDeadDb`),
//! * the bomb: fuse sparks and smoke (`EffectSsGSpk_SpawnFuse`, `func_8002829C`), the layered fireball
//!   (`EffectSsBomb2_SpawnLayered`), the white shockwave ring (`EffectSsBlast_SpawnWhiteShockwave`) and
//!   `Camera_RequestQuake(2, 11, 8)`,
//! * sword slash trails (`EffectBlure` on the blade, taken from the hand bone), the spin-attack charge glow,
//!   jump-slash and roll dust, arrow trails (fire / ice / light), Din's Fire and Nayru's Love shimmer, Deku Nut flash,
//! * Link: hurt flash and invincibility blink, pickup glints, Navi's glow trail, hit-stop on strong blows.
//!
//! The pure parts (hit looks, blob / ring envelopes, explosion layers, shake, hit-stop, trail ageing) are unit tested.
use super::{link_active, WorldFx};
use crate::camera::MainCamera;
use crate::characters::assets::LinkAsset;
use crate::characters::Character;
use crate::experiments::ExperimentSlot;
use crate::fx::particles::{BurstOptions, Particles, Rng};
use crate::fx::pickup_fx::PickupFx;
use crate::game::runner::GameSim;
use crate::render::{to_bevy, SCALE};
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use jumpman_sim::link::combat::{Dmg, Swing};
use jumpman_sim::link::projectiles::{Kind, Phase};
use jumpman_sim::link::Mv;
use std::collections::VecDeque;
use std::sync::Arc;

pub struct CombatFxPlugin;

impl Plugin for CombatFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatFx>()
            .init_resource::<LinkBones>()
            .add_systems(Startup, spawn_pool.in_set(crate::progress::PersistentSpawn))
            .add_systems(FixedPostUpdate, collect_enemy_events)
            .add_systems(Update, (apply_queue, world_effects, draw, hitstop).chain().after(crate::CameraSet).after(super::aimcam::first_person_camera))
            .add_systems(Update, apply_shake.after(super::aimcam::first_person_camera).after(super::zcam::z_camera).after(crate::CameraSet));
    }
}

// ---------------------------------------------------------------------------------------------
// Pure model

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Element {
    None,
    Fire,
    Ice,
    Light,
}

/// How one blow looks and feels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitLook {
    pub color: u32,
    pub count: usize,
    /// Particle size in sim units.
    pub size: f32,
    pub element: Element,
    /// Hit-stop in real seconds (a couple of frames on strong blows).
    pub stop: f32,
    pub shake: f32,
}

/// `EffectSsHitMark` colours / sizes by damage type: white for blades, arrows and seeds; the elemental types tint it.
pub fn hit_look(d: Dmg) -> HitLook {
    use Dmg::*;
    let base = HitLook { color: 0xfff4c8, count: 7, size: 24.0, element: Element::None, stop: 0.04, shake: 0.012 };
    match d {
        KokiriSword | MasterSword | GiantSword | Stick => base,
        KokiriSpin | MasterSpin | GiantSpin => HitLook { count: 12, size: 30.0, stop: 0.06, shake: 0.022, ..base },
        KokiriJump | MasterJump | GiantJump => HitLook { count: 12, size: 32.0, stop: 0.07, shake: 0.035, ..base },
        HammerSwing | HammerJump => HitLook { color: 0xf0e2c0, count: 14, size: 40.0, stop: 0.08, shake: 0.05, ..base },
        FireArrow | FireMagic => HitLook { color: 0xff7a20, count: 9, size: 30.0, element: Element::Fire, stop: 0.03, ..base },
        IceArrow | IceMagic => HitLook { color: 0x9fe8ff, count: 9, size: 30.0, element: Element::Ice, stop: 0.03, ..base },
        LightArrow | LightMagic => HitLook { color: 0xffffa0, count: 10, size: 32.0, element: Element::Light, stop: 0.03, ..base },
        Arrow | Slingshot | Boomerang | Hookshot | Nut => HitLook { count: 4, size: 20.0, stop: 0.0, shake: 0.0, ..base },
        _ => HitLook { count: 5, size: 20.0, stop: 0.02, shake: 0.0, ..base },
    }
}

/// A growing, fading translucent sphere (fireball, smoke puff, glow). Positions in sim units.
#[derive(Clone, Copy, Debug)]
pub struct Blob {
    pub pos: P3,
    pub rise: f32,
    pub age: f32,
    pub life: f32,
    pub r0: f32,
    pub r1: f32,
    pub rgb: [f32; 3],
    pub a0: f32,
}

pub type P3 = [f32; 3];

impl Blob {
    pub fn done(&self) -> bool {
        self.age >= self.life
    }
    /// Radius: ease-out growth from `r0` to `r1`.
    pub fn radius(&self) -> f32 {
        let t = (self.age / self.life).clamp(0.0, 1.0);
        self.r0 + (self.r1 - self.r0) * (1.0 - (1.0 - t) * (1.0 - t))
    }
    pub fn alpha(&self) -> f32 {
        let t = (self.age / self.life).clamp(0.0, 1.0);
        self.a0 * (1.0 - t).powf(1.4)
    }
    pub fn centre(&self) -> P3 {
        [self.pos[0], self.pos[1] + self.rise * self.age, self.pos[2]]
    }
}

/// An expanding flat ring (shockwave).
#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub pos: P3,
    pub age: f32,
    pub life: f32,
    pub r1: f32,
    pub rgb: [f32; 3],
}

impl Ring {
    pub fn radius(&self) -> f32 {
        let t = (self.age / self.life).clamp(0.0, 1.0);
        self.r1 * (1.0 - (1.0 - t).powi(3))
    }
    pub fn alpha(&self) -> f32 {
        0.8 * (1.0 - (self.age / self.life).clamp(0.0, 1.0))
    }
}

/// `EffectSsBomb2_SpawnLayered` + `EffectSsBlast_SpawnWhiteShockwave` for a blast of `radius` (sim units): two fireball layers
/// (yellow core, orange body), two rising smoke puffs and the shockwave ring on the floor.
pub fn explosion_layers(pos: P3, radius: f32) -> (Vec<Blob>, Vec<Ring>) {
    let b = |r0: f32, r1: f32, life: f32, rise: f32, rgb: [f32; 3], a0: f32, dy: f32| Blob { pos: [pos[0], pos[1] + dy, pos[2]], rise, age: 0.0, life, r0, r1, rgb, a0 };
    let blobs = vec![
        b(radius * 0.2, radius * 0.75, 0.30, 0.0, [1.0, 0.95, 0.55], 0.95, 0.0),
        b(radius * 0.25, radius * 1.0, 0.45, 0.0, [1.0, 0.5, 0.12], 0.8, 0.0),
        b(radius * 0.3, radius * 1.15, 0.9, 60.0, [0.22, 0.2, 0.2], 0.55, radius * 0.2),
        b(radius * 0.2, radius * 0.9, 1.1, 90.0, [0.35, 0.33, 0.32], 0.4, radius * 0.1),
    ];
    let rings = vec![Ring { pos: [pos[0], pos[1] + 4.0, pos[2]], age: 0.0, life: 0.45, r1: radius * 1.6, rgb: [1.0, 1.0, 1.0] }];
    (blobs, rings)
}

/// A decaying camera shake: `Camera_RequestQuake(level, speed, duration)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shake {
    pub amp: f32,
    pub age: f32,
    pub life: f32,
}

impl Shake {
    pub fn new(amp: f32, life: f32) -> Self {
        Self { amp, age: 0.0, life }
    }
    pub fn active(&self) -> bool {
        self.age < self.life
    }
    /// Camera offset (bevy units) at the current age.
    pub fn offset(&self) -> Vec3 {
        if !self.active() {
            return Vec3::ZERO;
        }
        let k = 1.0 - self.age / self.life;
        let t = self.age * 70.0;
        Vec3::new((t).sin(), (t * 1.3 + 1.0).cos() * 0.7, (t * 0.9 + 2.0).sin() * 0.4) * self.amp * k * k
    }
    /// A new quake replaces a weaker running one.
    pub fn request(&mut self, amp: f32, life: f32) {
        if !self.active() || amp * (1.0 - 0.0) > self.amp * (1.0 - self.age / self.life) {
            *self = Self::new(amp, life);
        }
    }
}

/// Real-time hit-stop: asks for the longest of the running and the new stop.
#[derive(Clone, Copy, Debug, Default)]
pub struct HitStop {
    pub left: f32,
}

impl HitStop {
    pub fn request(&mut self, seconds: f32) {
        self.left = self.left.max(seconds.min(0.12));
    }
    /// Advance by `dt` real seconds; returns the virtual-time speed to use this frame (near zero while stopped).
    pub fn step(&mut self, dt: f32) -> f32 {
        if self.left > 0.0 {
            self.left = (self.left - dt).max(0.0);
            0.04
        } else {
            1.0
        }
    }
}

/// Blade ribbon (`EffectBlure`): (base, tip) world points with their age, newest last.
#[derive(Clone, Debug, Default)]
pub struct Trail {
    pub pts: VecDeque<(Vec3, Vec3, f32)>,
}

pub const TRAIL_LIFE: f32 = 0.22;

impl Trail {
    pub fn push(&mut self, base: Vec3, tip: Vec3) {
        if self.pts.back().is_none_or(|(b, t, _)| b.distance(base) + t.distance(tip) > 1e-5) {
            self.pts.push_back((base, tip, 0.0));
        }
        while self.pts.len() > 40 {
            self.pts.pop_front();
        }
    }
    pub fn age(&mut self, dt: f32) {
        for p in self.pts.iter_mut() {
            p.2 += dt;
        }
        while self.pts.front().is_some_and(|p| p.2 >= TRAIL_LIFE) {
            self.pts.pop_front();
        }
    }
    /// Alpha of a point of age `a` (1 fresh, 0 at the end of its life), eased so the tail melts away instead of
    /// ending on an edge.
    pub fn alpha(a: f32) -> f32 {
        let k = (1.0 - a / TRAIL_LIFE).clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    }
    /// The ribbon as drawn: `sub` Catmull-Rom steps between two samples (the blade turns a long way between two
    /// frames, and straight segments show as a polygon), each with its interpolated age. Newest last.
    pub fn ribbon(&self, sub: usize) -> Vec<(Vec3, Vec3, f32)> {
        let p: Vec<(Vec3, Vec3, f32)> = self.pts.iter().copied().collect();
        let n = p.len();
        if n < 2 || sub < 2 {
            return p;
        }
        let spline = |a: Vec3, b: Vec3, c: Vec3, d: Vec3, t: f32| 0.5 * ((2.0 * b) + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t + (3.0 * b - a - 3.0 * c + d) * t * t * t);
        let mut out = Vec::with_capacity((n - 1) * sub + 1);
        for i in 0..n - 1 {
            let (a, b, c, d) = (p[i.saturating_sub(1)], p[i], p[i + 1], p[(i + 2).min(n - 1)]);
            for k in 0..sub {
                let t = k as f32 / sub as f32;
                out.push((spline(a.0, b.0, c.0, d.0, t), spline(a.1, b.1, c.1, d.1, t), b.2 + (c.2 - b.2) * t));
            }
        }
        out.push(p[n - 1]);
        out
    }
}

/// The ribbon's colour for a swing: steel white for the plain cuts, the charge's blue or gold for the spins, a
/// hotter white for the jump slash.
pub fn trail_rgb(s: Swing) -> [f32; 3] {
    match s {
        Swing::Spin => [0.5, 0.78, 1.0],
        Swing::BigSpin => [1.0, 0.82, 0.35],
        Swing::JumpSlash | Swing::FlipSlash => [1.0, 0.97, 0.7],
        _ => [0.78, 0.9, 1.0],
    }
}

/// The floor ring and shake of a spin attack let go: (ring radius in sim units, colour, shake).
pub fn spin_release(s: Swing) -> Option<(f32, [f32; 3], f32)> {
    match s {
        Swing::Spin => Some((230.0, [0.6, 0.85, 1.0], 0.016)),
        Swing::BigSpin => Some((340.0, [1.0, 0.85, 0.4], 0.032)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Events and resources

/// What the app wants shown (queued in [`WorldFx`] by the event handler and by `collect_enemy_events`).
#[derive(Clone, Debug)]
pub enum FxEvent {
    Hit { pos: [f64; 3], look: HitLook },
    /// Steel on steel / hard hide / shield (`EffectSpark`).
    Clank { pos: [f64; 3] },
    Died { pos: [f64; 3] },
    Explosion { pos: [f64; 3], radius: f64 },
    Slam { pos: [f64; 3] },
    NutFlash { pos: [f64; 3] },
    Ignite { pos: [f64; 3] },
    Freeze { pos: [f64; 3] },
    /// An arrow / seed / boomerang / hook struck the level.
    Stuck { pos: [f64; 3], kind: Kind },
    Shock { pos: [f64; 3] },
    Dust { pos: [f64; 3], n: usize },
}

#[derive(Resource)]
pub struct CombatFx {
    pub blobs: Vec<Blob>,
    pub rings: Vec<Ring>,
    pub trail: Trail,
    pub shake: Shake,
    pub stop: HitStop,
    /// Screen overlays: white pulse (nut), red hurt flash.
    pub white: f32,
    pub hurt: f32,
    rng: Rng,
    seen_tick: u64,
    last_health: i32,
    last_tick: u64,
    prev_air_slash: bool,
    trail_rgb: [f32; 3],
    prev_swing: Option<Swing>,
    wall_sparked: bool,
    tints: Vec<(Vec3, f32, [f32; 4])>,
}

impl Default for CombatFx {
    fn default() -> Self {
        Self { blobs: vec![], rings: vec![], trail: Trail::default(), shake: Shake::default(), stop: HitStop::default(), white: 0.0, hurt: 0.0, rng: Rng::new(0x51ed), seen_tick: 0, last_health: 0, last_tick: 0, prev_air_slash: false, trail_rgb: [0.78, 0.9, 1.0], prev_swing: None, wall_sparked: false, tints: vec![] }
    }
}

/// Link's skeleton as drawn this frame (`characters::update_rigs` writes it): bone matrices in model space and the rig root.
#[derive(Resource, Default)]
pub struct LinkBones {
    pub mats: Vec<Mat4>,
    pub root: Transform,
    asset: Option<Arc<LinkAsset>>,
    /// Sword blade in the hand bone's frame: (hilt-side point, tip).
    blade: Option<(Vec3, Vec3)>,
    pub valid: bool,
}

pub const HAND_LIMB: usize = 15;

impl LinkBones {
    pub fn set(&mut self, asset: &Arc<LinkAsset>, mats: &[Mat4], root: Transform) {
        if self.asset.as_ref().is_none_or(|a| !Arc::ptr_eq(a, asset)) {
            self.blade = blade_of(asset);
            self.asset = Some(asset.clone());
        }
        self.mats.clear();
        self.mats.extend_from_slice(mats);
        self.root = root;
        self.valid = true;
    }
    /// World-space (base, tip) of the blade, bevy units.
    pub fn blade_world(&self) -> Option<(Vec3, Vec3)> {
        let (b, t) = self.blade?;
        let m = self.mats.get(HAND_LIMB)?;
        Some((self.root.transform_point(m.transform_point3(b)), self.root.transform_point(m.transform_point3(t))))
    }
}

/// The blade of a Link asset: the vertex of the drawn-sword batches (hand limb) farthest from the hand, and a point a fifth of the
/// way to it (above the guard).
pub fn blade_of(asset: &LinkAsset) -> Option<(Vec3, Vec3)> {
    let mut best: Option<Vec3> = None;
    for b in asset.batches.iter().filter(|b| b.mode == "drawn") {
        for v in b.vertices.iter().filter(|v| v[8] as usize == HAND_LIMB) {
            let p = Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32);
            if best.is_none_or(|q| p.length() > q.length()) {
                best = Some(p);
            }
        }
    }
    best.map(|t| (t * 0.2, t))
}

// ---------------------------------------------------------------------------------------------
// Systems

#[derive(Component)]
struct BlobEnt(usize);
#[derive(Component)]
struct RingEnt(usize);
#[derive(Component)]
struct TrailEnt;
#[derive(Component)]
struct WhiteOverlay;
#[derive(Component)]
struct HurtOverlay;

const POOL: usize = 56;
const RING_POOL: usize = 8;

fn spawn_pool(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let sphere = meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap());
    for i in 0..POOL {
        let m = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.0), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
        commands.spawn((BlobEnt(i), Mesh3d(sphere.clone()), MeshMaterial3d(m), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
    }
    let ring = meshes.add(Annulus::new(0.86, 1.0).mesh().resolution(40));
    for i in 0..RING_POOL {
        let m = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.0), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
        commands.spawn((RingEnt(i), Mesh3d(ring.clone()), MeshMaterial3d(m), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
    }
    let mut trail = Mesh::new(PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
    // one collapsed triangle until the first swing: the renderer warns about a mesh without vertices
    trail.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    trail.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
    trail.insert_indices(bevy::render::mesh::Indices::U32(vec![0, 1, 2]));
    let tm = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    commands.spawn((TrailEnt, Mesh3d(meshes.add(trail)), MeshMaterial3d(tm), Transform::default(), Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling, bevy::light::NotShadowCaster));
    commands.spawn((WhiteOverlay, Visibility::Hidden, GlobalZIndex(7), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0))));
    commands.spawn((HurtOverlay, Visibility::Hidden, GlobalZIndex(7), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.9, 0.0, 0.0, 0.0))));
}

/// OoT enemy events (`oot_enemy::Ev`) -> effects. The events stay in the field until its next step, so dedupe by tick.
fn collect_enemy_events(en: Option<Res<crate::zelda_enemies::OotEnemies>>, sim: Res<GameSim>, mut fx: ResMut<WorldFx>, mut state: ResMut<CombatFx>) {
    use jumpman_sim::oot_enemy::Ev;
    let Some(en) = en else { return };
    if !sim.mario.link.enabled || en.field.tick == state.seen_tick {
        return;
    }
    state.seen_tick = en.field.tick;
    for ev in &en.field.events {
        match ev {
            Ev::Spark { pos, hard } => {
                // The blow's weapon: the enemy nearest the spark remembers it (`killed_by`).
                let dmg = en.field.enemies.iter().filter(|e| e.killed_by.is_some()).min_by(|a, b| {
                    let d = |e: &jumpman_sim::oot_enemy::Enemy| (e.pos[0] - pos[0]).powi(2) + (e.pos[1] - pos[1]).powi(2) + (e.pos[2] - pos[2]).powi(2);
                    d(a).total_cmp(&d(b))
                });
                let look = hit_look(dmg.and_then(|e| e.killed_by).unwrap_or(Dmg::MasterSword));
                if *hard {
                    fx.queue.push(FxEvent::Clank { pos: *pos });
                }
                fx.queue.push(FxEvent::Hit { pos: *pos, look });
            }
            Ev::Clank { pos } => fx.queue.push(FxEvent::Clank { pos: *pos }),
            Ev::Died { pos, .. } => fx.queue.push(FxEvent::Died { pos: *pos }),
            Ev::HurtPlayer { from, .. } => {
                // The shield took it: facing the attacker with the guard up and no damage dealt.
                let m = &sim.mario;
                if jumpman_sim::characters::link_guarding(m) {
                    let toward = jumpman_sim::math::atan2s(from[2] - m.pos[2], from[0] - m.pos[0]) as f64;
                    let d = jumpman_sim::math::to_s16(toward - m.face_angle[1]).abs() as f64;
                    if d < 0x4000 as f64 {
                        let (sx, cz) = (jumpman_sim::math::sins(m.face_angle[1]) * 70.0, jumpman_sim::math::coss(m.face_angle[1]) * 70.0);
                        fx.queue.push(FxEvent::Clank { pos: [m.pos[0] + sx, m.pos[1] + 90.0, m.pos[2] + cz] });
                    }
                }
            }
            _ => {}
        }
    }
}

fn v3(p: [f64; 3]) -> P3 {
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

fn burst(parts: &mut Particles, at: [f64; 3], color: u32, count: usize, speed: f32, life: f32, gravity: f32, size: f32, upward: f32) {
    parts.burst(at, BurstOptions { color, count, speed, life, gravity, size, upward, spread: 1.0 });
}

/// Turn queued events into particles, blobs, rings, shakes, stops and flashes.
fn apply_queue(mut fx: ResMut<WorldFx>, mut state: ResMut<CombatFx>, mut parts: ResMut<Particles>, mut pfx: ResMut<PickupFx>) {
    let events = std::mem::take(&mut fx.queue);
    for e in events {
        match e {
            FxEvent::Hit { pos, look } => {
                // `EffectSsHitMark`: a bright flash plus the spark spray.
                burst(&mut parts, pos, look.color, look.count, 260.0, 0.28, 520.0, look.size * 0.8, 90.0);
                burst(&mut parts, pos, 0xffffff, 2, 20.0, 0.16, 0.0, look.size * 2.4, 0.0);
                match look.element {
                    Element::Fire => burst(&mut parts, pos, 0xffc040, 5, 90.0, 0.5, -200.0, 26.0, 60.0),
                    Element::Ice => burst(&mut parts, pos, 0xe8ffff, 6, 140.0, 0.6, 300.0, 16.0, 80.0),
                    Element::Light => burst(&mut parts, pos, 0xffffff, 6, 200.0, 0.4, 0.0, 24.0, 0.0),
                    Element::None => {}
                }
                // the impact ring, and stars off the heavier blows
                let c = crate::fx::particles::hex_rgb(look.color);
                pfx.halo(to_bevy(pos), c, look.size * 3.4, 0.2);
                if look.count >= 12 {
                    pfx.sparkle(to_bevy(pos), [1.0, 1.0, 0.85], 4, 260.0, look.size);
                }
                state.stop.request(look.stop);
                if look.shake > 0.0 {
                    state.shake.request(look.shake, 0.18);
                }
            }
            FxEvent::Clank { pos } => {
                // `EffectSpark`: a fan of fast yellow-white sparks.
                burst(&mut parts, pos, 0xfff7b0, 10, 420.0, 0.3, 700.0, 14.0, 120.0);
                burst(&mut parts, pos, 0xffffff, 2, 20.0, 0.12, 0.0, 70.0, 0.0);
                pfx.halo(to_bevy(pos), [1.0, 0.97, 0.7], 75.0, 0.16);
                pfx.sparkle(to_bevy(pos), [1.0, 1.0, 0.8], 3, 300.0, 22.0);
                state.stop.request(0.03);
                state.shake.request(0.012, 0.14);
            }
            FxEvent::Died { pos } => {
                // `EffectSsDeadDb`: a flame burst that blows out into smoke.
                burst(&mut parts, pos, 0xff9a30, 12, 160.0, 0.55, -120.0, 44.0, 110.0);
                burst(&mut parts, pos, 0xb8b0a8, 8, 90.0, 0.8, -60.0, 50.0, 70.0);
                let (blobs, _) = explosion_layers(v3(pos), 70.0);
                state.blobs.extend(blobs.into_iter().skip(1).take(2));
                // the last of it: a ring thrown out and a few stars
                let at = to_bevy([pos[0], pos[1] + 30.0, pos[2]]);
                pfx.halo(at, [1.0, 0.85, 0.55], 170.0, 0.32);
                pfx.sparkle(at, [1.0, 0.95, 0.7], 6, 280.0, 28.0);
                state.stop.request(0.07);
                state.shake.request(0.02, 0.2);
            }
            FxEvent::Explosion { pos, radius } => {
                let (b, r) = explosion_layers(v3(pos), radius as f32);
                state.blobs.extend(b);
                state.rings.extend(r);
                burst(&mut parts, pos, 0xffb040, 16, 520.0, 0.5, 600.0, 36.0, 200.0);
                burst(&mut parts, pos, 0x8a8480, 10, 200.0, 0.9, -80.0, 56.0, 120.0);
                state.shake.request(0.09, 0.55);
                state.white = state.white.max(0.28);
            }
            FxEvent::Slam { pos } => {
                state.rings.push(Ring { pos: [pos[0] as f32, pos[1] as f32 + 4.0, pos[2] as f32], age: 0.0, life: 0.4, r1: 260.0, rgb: [1.0, 0.95, 0.8] });
                burst(&mut parts, pos, 0xf0e2c0, 14, 300.0, 0.6, 200.0, 40.0, 140.0);
                state.shake.request(0.07, 0.35);
                state.stop.request(0.08);
            }
            FxEvent::Shock { pos } => {
                state.rings.push(Ring { pos: [pos[0] as f32, pos[1] as f32 + 4.0, pos[2] as f32], age: 0.0, life: 0.35, r1: 170.0, rgb: [1.0, 1.0, 1.0] });
                burst(&mut parts, pos, 0xf0e2c0, 10, 240.0, 0.5, 160.0, 32.0, 70.0);
                state.stop.request(0.05);
                state.shake.request(0.03, 0.2);
            }
            FxEvent::NutFlash { pos } => {
                state.white = state.white.max(0.7);
                burst(&mut parts, pos, 0xffffd0, 14, 380.0, 0.35, 0.0, 30.0, 0.0);
            }
            FxEvent::Ignite { pos } => burst(&mut parts, pos, 0xff8a20, 12, 120.0, 0.7, -160.0, 34.0, 100.0),
            FxEvent::Freeze { pos } => {
                burst(&mut parts, pos, 0xbfefff, 14, 200.0, 0.8, 380.0, 20.0, 120.0);
                burst(&mut parts, pos, 0xffffff, 4, 20.0, 0.3, 0.0, 60.0, 0.0);
            }
            FxEvent::Stuck { pos, kind } => {
                let (c, n) = match kind {
                    Kind::FireArrow => (0xff8a20, 8),
                    Kind::IceArrow => (0xbfefff, 8),
                    Kind::LightArrow => (0xffffa0, 8),
                    _ => (0xffe8b0, 4),
                };
                burst(&mut parts, pos, c, n, 150.0, 0.25, 500.0, 14.0, 40.0);
            }
            FxEvent::Dust { pos, n } => parts.dust(pos, n),
        }
    }
}

/// Everything that follows the world frame by frame.
#[allow(clippy::too_many_arguments)]
fn world_effects(
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    sim: Res<GameSim>,
    time: Res<Time>,
    bones: Res<LinkBones>,
    oot: Option<Res<crate::zelda_enemies::OotEnemies>>,
    navi: Query<&GlobalTransform, With<super::zcam::Navi>>,
    mut state: ResMut<CombatFx>,
    mut parts: ResMut<Particles>,
    mut fx: ResMut<WorldFx>,
    mut pfx: ResMut<PickupFx>,
) {
    let dt = time.delta_secs().min(0.1);
    // Age the transient effects.
    for b in state.blobs.iter_mut() {
        b.age += dt;
    }
    state.blobs.retain(|b| !b.done());
    for r in state.rings.iter_mut() {
        r.age += dt;
    }
    state.rings.retain(|r| r.age < r.life);
    state.trail.age(dt);
    state.shake.age += dt;
    state.white = (state.white - dt * 2.2).max(0.0);
    state.hurt = (state.hurt - dt * 2.4).max(0.0);
    state.tints.clear();
    if !link_active(&character, &slot) || !sim.mario.link.enabled {
        return;
    }
    let link = &sim.mario.link;
    let m = &sim.mario;
    let pos = m.pos;
    // Whole sim ticks since the last frame (the effects that follow the 30 Hz state spawn once per tick).
    let ticks = sim.ticks.saturating_sub(state.last_tick).min(4);
    state.last_tick = sim.ticks;
    let tick_now = sim.ticks;

    // Blade trail while swinging; the glow while the spin charges.
    let swinging = link.mv == Mv::Swing && link.swing.is_some();
    let blade = bones.valid.then(|| bones.blade_world()).flatten();
    if let (true, Some((base, tip))) = (swinging, blade) {
        state.trail.push(base, tip);
        if let Some(s) = link.swing {
            state.trail_rgb = trail_rgb(s);
        }
        // The blade's tip in a wall: sparks, once a swing.
        if ticks > 0 && !state.wall_sparked {
            let at = [tip.x as f64 / SCALE as f64, tip.y as f64 / SCALE as f64, tip.z as f64 / SCALE as f64];
            let mut probe = at;
            if m.world.f32_find_wall_collision(&mut probe, 0.0, 14.0) > 0 {
                state.wall_sparked = true;
                burst(&mut parts, at, 0xfff7b0, 7, 340.0, 0.26, 700.0, 13.0, 100.0);
                pfx.halo(tip, [1.0, 0.97, 0.7], 55.0, 0.15);
                state.shake.request(0.008, 0.12);
            }
        }
    }
    if !swinging {
        state.wall_sparked = false;
    }
    // A spin attack let go: the ring it cuts on the floor.
    if link.swing != state.prev_swing {
        if let Some((r1, rgb, shake)) = link.swing.and_then(spin_release) {
            state.rings.push(Ring { pos: [pos[0] as f32, pos[1] as f32 + 6.0, pos[2] as f32], age: 0.0, life: 0.32, r1, rgb });
            burst(&mut parts, [pos[0], pos[1] + 50.0, pos[2]], if r1 > 300.0 { 0xfff0a0 } else { 0xa8d8ff }, 10, 300.0, 0.35, 0.0, 18.0, 30.0);
            state.shake.request(shake, 0.22);
        }
        state.prev_swing = link.swing;
    }
    if link.mv == Mv::Charge && link.charge.value > 0.2 {
        if let Some((_, tip)) = bones.blade_world() {
            let k = link.charge.value as f32;
            let big = link.charge.is_big();
            let pulse = 1.0 + 0.15 * (tick_now as f32 * 0.9).sin();
            let rgb = if big { [1.0, 0.95, 0.5] } else { [0.55, 0.8, 1.0] };
            state.tints.push((tip, (10.0 + 18.0 * k) * pulse * SCALE, [rgb[0], rgb[1], rgb[2], 0.28 + 0.3 * k]));
            if ticks > 0 && (tick_now % 2 == 0 || big) {
                let p = [tip.x as f64 / SCALE as f64, tip.y as f64 / SCALE as f64, tip.z as f64 / SCALE as f64];
                burst(&mut parts, p, if big { 0xfff0a0 } else { 0xa8d8ff }, if big { 2 } else { 1 }, 90.0, 0.35, 0.0, 14.0, 20.0);
            }
        }
    }
    // Jump slash / flip slash: the landing shockwave.
    let air_slash = link.swing.is_some_and(|s| matches!(s, Swing::JumpSlash | Swing::FlipSlash)) && m.action & jumpman_sim::state::ACT_FLAG_AIR != 0;
    if state.prev_air_slash && !air_slash && m.action & jumpman_sim::state::ACT_FLAG_AIR == 0 {
        fx.queue.push(FxEvent::Shock { pos });
    }
    state.prev_air_slash = air_slash;
    // Roll dust (`Actor_SpawnFloorDustRing`-style puffs behind Link).
    if ticks > 0 && jumpman_sim::characters::link_rolling(m) && tick_now % 3 == 0 {
        parts.dust(pos, 2);
    }

    // Bombs: fuse sparks on even frames (`EffectSsGSpk_SpawnFuse`) and smoke once the fuse is short (`timer < 63`).
    for p in &link.projectiles.list {
        let at = [p.pos[0], p.pos[1], p.pos[2]];
        match (p.kind, p.phase) {
            (Kind::Bomb, Phase::Fuse) if ticks > 0 => {
                let frames = p.timer / 1.5;
                if frames < 63.0 {
                    let tip = [at[0], at[1] + 6.0 * 4.4 + 17.0 * 4.4, at[2]];
                    if tick_now % 3 != 2 {
                        burst(&mut parts, tip, 0xffc040, 2, 110.0, 0.22, 300.0, 14.0, 90.0);
                    }
                    burst(&mut parts, [tip[0], tip[1] + 14.0, tip[2]], 0x8c8c8c, 1, 20.0, 0.6, -40.0, 18.0, 60.0);
                }
            }
            (Kind::FireArrow, Phase::Fly) if ticks > 0 => burst(&mut parts, at, 0xff7a20, 2, 40.0, 0.3, -120.0, 20.0, 10.0),
            (Kind::IceArrow, Phase::Fly) if ticks > 0 => burst(&mut parts, at, 0xbfefff, 2, 40.0, 0.35, 60.0, 14.0, 0.0),
            (Kind::LightArrow, Phase::Fly) if ticks > 0 => burst(&mut parts, at, 0xffffa0, 2, 60.0, 0.3, 0.0, 18.0, 0.0),
            (Kind::Boomerang, _) if ticks > 0 && tick_now % 2 == 0 => burst(&mut parts, at, 0xe8f0ff, 1, 10.0, 0.25, 0.0, 16.0, 0.0),
            (Kind::DinsFire, _) if p.radius > 1.0 => {
                // Din's Fire: a sphere of flame with flames tumbling off it.
                let r = p.radius as f32;
                state.tints.push((to_bevy(p.pos), r * SCALE, [1.0, 0.35, 0.08, 0.35]));
                if ticks > 0 {
                    burst(&mut parts, at, 0xff9a30, 2, r * 1.6, 0.5, -100.0, 34.0, 60.0);
                }
            }
            _ => {}
        }
    }
    // Nayru's Love: a shimmer on the shell.
    if link.nayru > 0.0 && ticks > 0 && tick_now % 2 == 0 {
        let a = tick_now as f64 * 0.37;
        burst(&mut parts, [pos[0] + a.cos() * 140.0, pos[1] + 90.0 + (a * 1.7).sin() * 90.0, pos[2] + a.sin() * 140.0], 0x9fc8ff, 1, 10.0, 0.5, 0.0, 16.0, 20.0);
    }

    // OoT enemies: the colour filter (`Actor_SetColorFilter`) and status effects.
    if let Some(en) = oot.as_ref() {
        for e in &en.field.enemies {
            if e.asleep || !e.visible {
                continue;
            }
            let (c, r, h) = e.body();
            let centre = to_bevy([c[0], c[1] + h * 0.5, c[2]]);
            let rad = (r.max(h * 0.5) * 1.15) as f32 * SCALE;
            if e.frozen > 0.0 {
                state.tints.push((centre, rad, [0.55, 0.85, 1.0, 0.5]));
            } else if e.flash > 0.0 {
                let k = (e.flash / 8.0).clamp(0.0, 1.0) as f32;
                // a wash over the body, not a ball hiding it and the sparks
                state.tints.push((centre, rad * 0.8, [1.0, 0.16, 0.08, 0.26 * k]));
            }
            if e.burn > 0.0 && ticks > 0 {
                burst(&mut parts, [c[0], c[1] + h * 0.5, c[2]], 0xff8a20, 2, 40.0, 0.4, -200.0, 28.0, 60.0);
            }
            if e.stun > 0.0 && ticks > 0 && tick_now % 3 == 0 {
                let a = tick_now as f64 * 0.8;
                burst(&mut parts, [c[0] + a.cos() * r, c[1] + h, c[2] + a.sin() * r], 0xffe860, 1, 10.0, 0.3, 0.0, 14.0, 0.0);
            }
        }
    }
    // Navi's glow trail.
    if let Ok(n) = navi.single() {
        if ticks > 0 && tick_now % 2 == 0 {
            let p = n.translation() / SCALE;
            parts.burst([p.x as f64, p.y as f64, p.z as f64], BurstOptions { color: 0xa8d0ff, count: 1, speed: 8.0, life: 0.5, gravity: 0.0, size: 12.0, upward: 0.0, spread: 1.0 });
        }
    }
    // Pickup glints (`EffectSsKiraKira`): a rupee or heart near Link twinkles now and then.
    if ticks > 0 && tick_now % 12 == 0 {
        let i = (state.rng.next() * sim.pickups.len().max(1) as f32) as usize;
        if let Some(p) = sim.pickups.get(i).filter(|p| !p.collected) {
            let d = (p.pos[0] - pos[0]).powi(2) + (p.pos[2] - pos[2]).powi(2);
            if d < 2500.0 * 2500.0 {
                burst(&mut parts, [p.pos[0] + (state.rng.next() as f64 - 0.5) * 40.0, p.pos[1] + 10.0 + state.rng.next() as f64 * 40.0, p.pos[2]], 0xffffff, 1, 6.0, 0.45, 0.0, 22.0, 10.0);
            }
        }
    }
    // Link hurt: a red flash and a small shake when hearts are lost (`Player_...` damage).
    let health = link.inv.health;
    if state.last_health > 0 && health < state.last_health {
        state.hurt = 0.34;
        state.shake.request(0.035, 0.25);
        // the blow itself, on his body, and a beat to take it in
        let at = [pos[0], pos[1] + 90.0, pos[2]];
        burst(&mut parts, at, 0xffb0a0, 8, 260.0, 0.3, 500.0, 20.0, 110.0);
        pfx.halo(to_bevy(at), [1.0, 0.45, 0.35], 130.0, 0.26);
        state.stop.request(0.06);
    }
    state.last_health = health;
}

fn draw(
    mut state: ResMut<CombatFx>,
    mut blobs: Query<(&BlobEnt, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), (Without<RingEnt>, Without<TrailEnt>)>,
    mut rings: Query<(&RingEnt, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), (Without<BlobEnt>, Without<TrailEnt>)>,
    mut trail: Query<(&Mesh3d, &mut Visibility), (With<TrailEnt>, Without<BlobEnt>, Without<RingEnt>)>,
    mut white: Query<(&mut BackgroundColor, &mut Visibility), (With<WhiteOverlay>, Without<HurtOverlay>, Without<BlobEnt>, Without<RingEnt>, Without<TrailEnt>)>,
    mut hurt: Query<(&mut BackgroundColor, &mut Visibility), (With<HurtOverlay>, Without<WhiteOverlay>, Without<BlobEnt>, Without<RingEnt>, Without<TrailEnt>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Blobs first, then the frame's tinted spheres.
    let mut draws: Vec<(Vec3, f32, [f32; 4])> = state.blobs.iter().map(|b| (to_bevy([b.centre()[0] as f64, b.centre()[1] as f64, b.centre()[2] as f64]), b.radius() * SCALE, [b.rgb[0], b.rgb[1], b.rgb[2], b.alpha()])).collect();
    draws.extend(state.tints.iter().copied());
    for (BlobEnt(i), mut tf, mut vis, mat) in &mut blobs {
        if let Some((p, r, c)) = draws.get(*i) {
            tf.translation = *p;
            tf.scale = Vec3::splat(*r);
            *vis = Visibility::Inherited;
            if let Some(mut m) = materials.get_mut(&mat.0) {
                m.base_color = Color::srgba(c[0], c[1], c[2], c[3]);
            }
        } else {
            *vis = Visibility::Hidden;
        }
    }
    for (RingEnt(i), mut tf, mut vis, mat) in &mut rings {
        if let Some(r) = state.rings.get(*i) {
            tf.translation = to_bevy([r.pos[0] as f64, r.pos[1] as f64, r.pos[2] as f64]);
            tf.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            tf.scale = Vec3::splat(r.radius().max(0.01) * SCALE);
            *vis = Visibility::Inherited;
            if let Some(mut m) = materials.get_mut(&mat.0) {
                m.base_color = Color::srgba(r.rgb[0], r.rgb[1], r.rgb[2], r.alpha());
            }
        } else {
            *vis = Visibility::Hidden;
        }
    }
    // The blade ribbon.
    if let Ok((mesh, mut vis)) = trail.single_mut() {
        if state.trail.pts.len() < 2 {
            *vis = Visibility::Hidden;
        } else {
            *vis = Visibility::Inherited;
            let (mut pos, mut col, mut idx) = (vec![], vec![], vec![]);
            let rgb = state.trail_rgb;
            for (i, (b, t, age)) in state.trail.ribbon(3).iter().enumerate() {
                let a = Trail::alpha(*age);
                // the ribbon narrows toward the tip's path as it ages
                pos.push(t.lerp(*b, 0.3 + 0.7 * a).to_array());
                pos.push(t.to_array());
                col.push([rgb[0], rgb[1], rgb[2], 0.0]);
                col.push([0.5 + 0.5 * rgb[0], 0.5 + 0.5 * rgb[1], 0.5 + 0.5 * rgb[2], 0.7 * a]);
                if i > 0 {
                    let k = (i * 2) as u32;
                    idx.extend([k - 2, k - 1, k, k - 1, k + 1, k]);
                }
            }
            if let Some(mut m) = meshes.get_mut(&mesh.0) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
                m.insert_indices(bevy::render::mesh::Indices::U32(idx));
            }
        }
    }
    for (mut bg, mut vis) in &mut white {
        *vis = if state.white > 0.01 { Visibility::Inherited } else { Visibility::Hidden };
        bg.0 = Color::srgba(1.0, 1.0, 0.95, state.white.min(0.9));
    }
    for (mut bg, mut vis) in &mut hurt {
        *vis = if state.hurt > 0.01 { Visibility::Inherited } else { Visibility::Hidden };
        bg.0 = Color::srgba(0.85, 0.0, 0.0, state.hurt * 0.55);
    }
}

fn hitstop(mut state: ResMut<CombatFx>, real: Res<Time<Real>>, mut virt: ResMut<Time<Virtual>>) {
    let speed = state.stop.step(real.delta_secs().min(0.1));
    if (virt.relative_speed() - speed).abs() > 1e-4 {
        virt.set_relative_speed(speed);
    }
}

fn apply_shake(state: Res<CombatFx>, mut camera: Single<&mut Transform, With<MainCamera>>) {
    if state.shake.active() {
        let o = state.shake.offset();
        let r = camera.rotation;
        camera.translation += r * o;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_looks_scale_with_the_blow() {
        let sword = hit_look(Dmg::MasterSword);
        assert!(hit_look(Dmg::MasterSpin).count > sword.count);
        assert!(hit_look(Dmg::MasterJump).stop > sword.stop);
        assert!(hit_look(Dmg::HammerSwing).shake > 0.0);
        assert_eq!(hit_look(Dmg::FireArrow).element, Element::Fire);
        assert_eq!(hit_look(Dmg::IceArrow).element, Element::Ice);
        assert_eq!(hit_look(Dmg::Arrow).stop, 0.0, "arrows do not stop time");
        assert!(hit_look(Dmg::Slingshot).count < sword.count);
    }

    #[test]
    fn explosion_has_fire_smoke_and_a_ring_that_all_end() {
        let (blobs, rings) = explosion_layers([0.0; 3], 100.0);
        assert_eq!(blobs.len(), 4);
        assert_eq!(rings.len(), 1);
        // Fire first, smoke after: the smoke rises and outlives the fire.
        assert!(blobs[2].life > blobs[1].life && blobs[2].rise > 0.0 && blobs[1].rise == 0.0);
        for b in &blobs {
            assert!(b.radius() <= b.r1 + 1e-3 && b.alpha() <= b.a0);
            let mut late = *b;
            late.age = late.life + 0.01;
            assert!(late.done());
        }
        let mut r = rings[0];
        let (r0, a0) = (r.radius(), r.alpha());
        r.age = r.life * 0.5;
        assert!(r.radius() > r0 && r.alpha() < a0);
        r.age = r.life;
        assert!((r.radius() - r.r1).abs() < 1e-3 && r.alpha() == 0.0);
    }

    #[test]
    fn blobs_grow_and_fade_monotonically() {
        let mut b = Blob { pos: [0.0; 3], rise: 0.0, age: 0.0, life: 1.0, r0: 10.0, r1: 50.0, rgb: [1.0; 3], a0: 1.0 };
        let (mut r, mut a) = (b.radius(), b.alpha());
        for _ in 0..10 {
            b.age += 0.1;
            assert!(b.radius() >= r && b.alpha() <= a);
            r = b.radius();
            a = b.alpha();
        }
        assert!(b.done());
    }

    #[test]
    fn shake_decays_to_nothing_and_a_stronger_quake_replaces_a_weaker_one() {
        let mut s = Shake::new(0.1, 0.5);
        assert!(s.offset().length() > 0.0 || s.age == 0.0);
        let early = s.offset().length();
        s.age = 0.45;
        assert!(s.offset().length() <= early + 1e-6);
        s.age = 0.5;
        assert!(!s.active() && s.offset() == Vec3::ZERO);
        let mut q = Shake::new(0.02, 0.3);
        q.request(0.1, 0.5);
        assert_eq!(q.amp, 0.1);
        q.age = 0.1;
        q.request(0.01, 0.5);
        assert_eq!(q.amp, 0.1, "a weaker quake does not replace a running one");
    }

    #[test]
    fn hit_stop_lasts_its_frames_then_releases() {
        let mut h = HitStop::default();
        assert_eq!(h.step(0.016), 1.0);
        h.request(0.05);
        h.request(0.02);
        assert_eq!(h.left, 0.05, "the longer stop wins");
        let mut frames = 0;
        while h.step(1.0 / 60.0) < 1.0 {
            frames += 1;
            assert!(frames < 20);
        }
        assert!((3..=4).contains(&frames), "0.05 s at 60 fps is 3 to 4 frames, got {frames}");
        h.request(5.0);
        assert!(h.left <= 0.12, "capped");
    }

    #[test]
    fn trail_ages_out_and_fades() {
        let mut t = Trail::default();
        for i in 0..5 {
            t.push(Vec3::new(i as f32, 0.0, 0.0), Vec3::new(i as f32, 1.0, 0.0));
            t.age(0.02);
        }
        assert_eq!(t.pts.len(), 5);
        assert!(Trail::alpha(0.0) == 1.0 && Trail::alpha(TRAIL_LIFE) == 0.0);
        // The same point twice is not duplicated.
        t.push(Vec3::new(4.0, 0.0, 0.0), Vec3::new(4.0, 1.0, 0.0));
        assert_eq!(t.pts.len(), 5);
        t.age(TRAIL_LIFE);
        assert!(t.pts.is_empty());
    }

    #[test]
    fn ribbon_passes_through_the_samples_and_stays_bounded() {
        let mut t = Trail::default();
        for i in 0..6 {
            let a = i as f32 * 0.5;
            t.push(Vec3::new(a.cos(), 0.0, a.sin()), Vec3::new(2.0 * a.cos(), 0.0, 2.0 * a.sin()));
            t.age(0.01);
        }
        let r = t.ribbon(3);
        assert_eq!(r.len(), 5 * 3 + 1);
        for (i, p) in t.pts.iter().enumerate() {
            assert!((r[i * 3].1 - p.1).length() < 1e-5, "sample {i} is on the ribbon");
        }
        // the points in between bulge outward with the arc instead of cutting the chord, and ages run monotonic
        let chord = (t.pts[2].1 + t.pts[3].1) * 0.5;
        assert!(r[7].1.length() > chord.length());
        assert!(r.windows(2).all(|w| w[0].2 >= w[1].2 - 1e-6));
        assert!(r.iter().all(|p| p.1.length() < 2.2 && p.0.length() < 1.2));
        assert_eq!(Trail::default().ribbon(3).len(), 0);
    }

    #[test]
    fn every_sword_blow_shakes_a_little_and_heavier_ones_more() {
        let (cut, spin, jump) = (hit_look(Dmg::MasterSword), hit_look(Dmg::MasterSpin), hit_look(Dmg::MasterJump));
        assert!(cut.shake > 0.0 && cut.shake < spin.shake && spin.shake < jump.shake && jump.shake < 0.05);
        assert!(cut.stop < spin.stop && spin.stop < jump.stop && jump.stop <= 0.12);
        assert_eq!(hit_look(Dmg::Arrow).shake, 0.0);
        assert!(spin_release(Swing::Forward).is_none());
        let (small, big) = (spin_release(Swing::Spin).unwrap(), spin_release(Swing::BigSpin).unwrap());
        assert!(small.0 < big.0 && small.2 < big.2);
        assert_ne!(trail_rgb(Swing::Spin), trail_rgb(Swing::BigSpin));
    }

    #[test]
    fn blade_is_the_farthest_hand_vertex() {
        use crate::characters::assets::LinkBatch;
        let asset = LinkAsset {
            limbs: vec![],
            textures: Default::default(),
            clips: Default::default(),
            batches: vec![
                LinkBatch { mode: "drawn".into(), texture: None, color: 0, wrap: [0, 0], vertices: vec![[0.0, 10.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 15.0], [0.0, 1000.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 15.0], [0.0, 5000.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 3.0]] },
                LinkBatch { mode: "base".into(), texture: None, color: 0, wrap: [0, 0], vertices: vec![[0.0, 9000.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 15.0]] },
            ],
        };
        let (base, tip) = blade_of(&asset).unwrap();
        assert_eq!(tip.y, 1000.0);
        assert!((base.y - 200.0).abs() < 1e-4);
    }
}
