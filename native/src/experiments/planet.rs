//! Tiny Planet (`planet.ts`): run all the way around a little world. Mario's physics run on a
//! flat floor; each tick the planet turns beneath him by exactly the arc he moved, and he is
//! shifted back to the centre of his frame, so "down" always points at the planet's core.
//!
//! The game: a coin rush. A lap is the warm-up (and the first goal); the real target is every coin on the planet, the
//! high ones on the pillar tops included, against the clock. Best lap and best all-coins time are kept.
//!
//! Not ported: the SM64 textures (flat colours), the moon and the cloud layer of the sky dome (a dark clear
//! colour plus a star field instead).
use super::geometry::add_face;
use super::mario_crowd::Rng;
use super::{Experiment, ExperimentId, Host};
use crate::render::{scene, SCALE};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use jumpman_sim::collision::{CollisionWorld, Surface, SurfaceOptions};
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::math::Vec3 as V3;
use std::collections::HashSet;
use std::sync::Arc;

pub const PLANET_RADIUS: f64 = 2200.0;
/// Features closer than this (along the surface) get collision in Mario's frame.
const COLLIDE_RANGE: f64 = 2600.0;
const TREE: &str = "actors/tree/bubbly_tree_geo.glb";
const COIN: &str = "actors/coin/yellow_coin_no_shadow_geo.glb";

struct Pillar {
    dir: DVec3,
    radius: f64,
    height: f64,
}

struct PlanetCoin {
    dir: DVec3,
    height: f64,
    entity: Entity,
    taken: bool,
}

pub struct FlatPoint {
    pub x: f64,
    pub z: f64,
    pub angle: f64,
}

/// Map a direction on the planet (already rotated into Mario's frame) into his flat frame (log
/// map): the arc length becomes flat distance, so gravity and SM64 physics stay untouched.
pub fn to_flat(local: DVec3, radius: f64) -> FlatPoint {
    let angle = local.y.clamp(-1.0, 1.0).acos();
    let h = local.x.hypot(local.z);
    if h < 1e-6 {
        return FlatPoint { x: 0.0, z: 0.0, angle };
    }
    FlatPoint { x: local.x / h * angle * radius, z: local.z / h * angle * radius, angle }
}

/// The rotation that rolls the planet under Mario after he moved `(dx, dz)` on the flat floor.
pub fn roll_for(dx: f64, dz: f64, radius: f64) -> Option<DQuat> {
    let distance = dx.hypot(dz);
    if distance <= 0.01 {
        return None;
    }
    let axis = DVec3::Y.cross(DVec3::new(dx / distance, 0.0, dz / distance)).normalize();
    // planet-in-frame rotates the opposite way to Mario's motion.
    Some(DQuat::from_axis_angle(axis, distance / radius).inverse())
}

/// Counts trips all the way around: a lap is going to the far side of the planet and coming
/// back to the spot you started on.
pub struct LapTracker {
    start: DVec3,
    armed: bool,
    pub laps: u32,
}

impl LapTracker {
    pub fn new(start: DVec3) -> Self {
        Self { start, armed: false, laps: 0 }
    }
    /// `orientation` is the planet's rotation in Mario's frame; true when a lap just completed.
    pub fn armed_flag(&self) -> bool {
        self.armed
    }
    pub fn update(&mut self, orientation: DQuat) -> bool {
        let angle = (orientation * self.start).angle_between(DVec3::Y);
        if angle > std::f64::consts::PI * 0.8 {
            self.armed = true;
        }
        if self.armed && angle < 0.12 {
            self.armed = false;
            self.laps += 1;
            return true;
        }
        false
    }
}

/// The pillars of the planet, drawn from `rng` in the order the world is built.
fn roll_pillars(rng: &mut Rng, start_dir: DVec3) -> Vec<(Pillar, f64)> {
    let mut random_dir = |rng: &mut Rng| DVec3::new(rng.next() * 2.0 - 1.0, rng.next() * 2.0 - 1.0, rng.next() * 2.0 - 1.0).normalize();
    let mut out = vec![];
    for _ in 0..9 {
        let mut dir = random_dir(rng);
        while dir.angle_between(start_dir) <= 0.35 {
            dir = random_dir(rng);
        }
        let radius = 160.0 + rng.next() * 160.0;
        let height = 180.0 + rng.next() * 320.0;
        let spin = rng.next() * std::f64::consts::TAU;
        out.push((Pillar { dir, radius, height }, spin));
    }
    out
}

/// Replace the pillar collision in Mario's flat frame (the ones within reach of him).
fn sync_pillars(world: &mut CollisionWorld, old: &HashSet<usize>, pillars: &[Pillar], orientation: DQuat) -> HashSet<usize> {
    world.remove_surfaces(&|s| old.contains(&(s as *const Surface as usize)));
    let mut faces = vec![];
    for pillar in pillars {
        let flat = to_flat(orientation * pillar.dir, PLANET_RADIUS);
        if flat.x.hypot(flat.z) > COLLIDE_RANGE {
            continue;
        }
        let segments = 14;
        let ring: Vec<V3> = (0..segments)
            .map(|i| {
                let a = i as f64 / segments as f64 * std::f64::consts::TAU;
                [flat.x + a.sin() * pillar.radius, pillar.height, flat.z + a.cos() * pillar.radius]
            })
            .collect();
        let o = SurfaceOptions::default();
        add_face(world, &ring, [0.0, 1.0, 0.0], &mut faces, &o);
        for i in 0..segments {
            let (a, b) = (ring[i], ring[(i + 1) % segments]);
            let mid = (i as f64 + 0.5) / segments as f64 * std::f64::consts::TAU;
            add_face(world, &[a, b, [b[0], -50.0, b[2]], [a[0], -50.0, a[2]]], [mid.sin(), 0.0, mid.cos()], &mut faces, &o);
        }
    }
    faces.iter().map(id).collect()
}

pub struct TinyPlanetExperiment {
    ground: HashSet<usize>,
    feature_surfaces: HashSet<usize>,
    /// Planet orientation relative to Mario's frame.
    orientation: DQuat,
    previous: DQuat,
    spinning: Option<Entity>,
    pillars: Vec<Pillar>,
    coins: Vec<PlanetCoin>,
    travelled: f64,
    laps: u32,
    collected: u32,
    start_dir: DVec3,
    laps_tracker: LapTracker,
    ticks: u32,
    /// Goal: one trip all the way around. (ticks, coins) when it was done.
    done: Option<(u32, u32)>,
    /// Ticks when the last coin was taken, and the kept bests (seconds).
    all_coins: Option<u32>,
    best_lap: Option<f64>,
    best_rush: Option<f64>,
    t: f64,
}

fn id(s: &Arc<Surface>) -> usize {
    Arc::as_ptr(s) as usize
}

impl TinyPlanetExperiment {
    pub fn new() -> Self {
        Self {
            ground: HashSet::new(),
            feature_surfaces: HashSet::new(),
            orientation: DQuat::IDENTITY,
            previous: DQuat::IDENTITY,
            spinning: None,
            pillars: vec![],
            coins: vec![],
            travelled: 0.0,
            laps: 0,
            collected: 0,
            start_dir: DVec3::Y,
            laps_tracker: LapTracker::new(DVec3::Y),
            ticks: 0,
            done: None,
            all_coins: None,
            best_lap: None,
            best_rush: None,
            t: 0.0,
        }
    }

    fn place(host: &mut Host, parent: Entity, mut bundle: impl FnMut(&mut Commands) -> Entity, dir: DVec3, height: f64, spin: f64) -> Entity {
        let e = bundle(host.commands);
        let rot = DQuat::from_rotation_arc(DVec3::Y, dir) * DQuat::from_rotation_y(spin);
        let p = dir * (PLANET_RADIUS + height) * SCALE as f64;
        host.commands.entity(e).insert(Transform { translation: p.as_vec3(), rotation: rot.as_quat(), scale: Vec3::ONE });
        host.commands.entity(parent).add_child(e);
        e
    }

    fn build_planet(&mut self, host: &mut Host) {
        let r = PLANET_RADIUS;
        let spinning = host.spawn((Transform::from_xyz(0.0, -(r as f32) * SCALE, 0.0), Visibility::Inherited));
        self.spinning = Some(spinning);
        let sphere_mesh = host.meshes.add(Sphere::new(r as f32 * SCALE).mesh().uv(96, 64));
        let grass = host.lit(Color::srgb(0.5, 0.78, 0.35));
        let sphere = host.commands.spawn((Mesh3d(sphere_mesh), MeshMaterial3d(grass), Transform::default())).id();
        host.commands.entity(spinning).add_child(sphere);
        let mut rng = Rng::new(0xb1a5e7);
        let mut random_dir = |rng: &mut Rng| DVec3::new(rng.next() * 2.0 - 1.0, rng.next() * 2.0 - 1.0, rng.next() * 2.0 - 1.0).normalize();
        let away = |d: DVec3, s: DVec3| d.angle_between(s) > 0.35;
        // Stone pillars to climb, each topped with coins.
        let stone = host.lit(Color::srgb(0.72, 0.68, 0.6));
        for (pillar, spin) in roll_pillars(&mut rng, self.start_dir) {
            let (dir, radius, height) = (pillar.dir, pillar.radius, pillar.height);
            let mesh = host.meshes.add(Cylinder::new(radius as f32 * SCALE, height as f32 * SCALE));
            // The cylinder is centred; lift it so its base sits on the ground.
            let mat = stone.clone();
            Self::place(host, spinning, |c| c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default())).id(), dir, height / 2.0, spin);
            self.pillars.push(pillar);
            for k in 0..3 {
                self.add_coin(host, dir, height + 90.0 + k as f64 * 70.0);
            }
        }
        // Trees and a belt of coins around the equator.
        for _ in 0..26 {
            let mut dir = random_dir(&mut rng);
            while !away(dir, self.start_dir) || self.pillars.iter().any(|p| p.dir.angle_between(dir) < 0.2) {
                dir = random_dir(&mut rng);
            }
            let spin = rng.next() * std::f64::consts::TAU;
            let scale = 0.7 + rng.next() as f32 * 0.4;
            let assets = host.assets;
            let e = Self::place(host, spinning, |c| c.spawn((scene(assets, TREE), Transform::default())).id(), dir, 0.0, spin);
            host.commands.entity(e).entry::<Transform>().and_modify(move |mut t| t.scale = Vec3::splat(scale));
        }
        for i in 0..24 {
            let a = i as f64 / 24.0 * std::f64::consts::TAU;
            self.add_coin(host, DVec3::new(a.cos(), 0.05 * (a * 3.0).sin(), a.sin()).normalize(), 90.0);
        }
        // A star field wheeling past as you run.
        let star = host.meshes.add(Sphere::new(140.0 * SCALE).mesh().ico(1).unwrap());
        let star_mat = host.unlit(Color::WHITE);
        for _ in 0..300 {
            let d = random_dir(&mut rng) * (60000.0 + rng.next() * 20000.0);
            let e = host.commands.spawn((Mesh3d(star.clone()), MeshMaterial3d(star_mat.clone()), Transform::from_translation((d * SCALE as f64).as_vec3()))).id();
            host.commands.entity(spinning).add_child(e);
        }
    }

    fn add_coin(&mut self, host: &mut Host, dir: DVec3, height: f64) {
        let spinning = self.spinning.unwrap();
        let assets = host.assets;
        let e = Self::place(host, spinning, |c| c.spawn((scene(assets, COIN), Transform::default())).id(), dir, height, 0.0);
        self.coins.push(PlanetCoin { dir, height, entity: e, taken: false });
    }

    /// Rebuild collision for nearby pillars in Mario's flat frame.
    fn sync_features(&mut self, host: &mut Host) {
        let old = std::mem::take(&mut self.feature_surfaces);
        self.feature_surfaces = sync_pillars(host.world_mut(), &old, &self.pillars, self.orientation);
    }

    /// How far round the planet the start point has travelled, 0..1 (out to the far side and back).
    fn progress(&self) -> f64 {
        let angle = (self.orientation * self.start_dir).angle_between(DVec3::Y);
        if self.laps_tracker.armed_flag() {
            1.0 - angle / std::f64::consts::PI * 0.2
        } else {
            (angle / (std::f64::consts::PI * 0.8)).min(1.0) * 0.8
        }
    }

    fn collect(&mut self, host: &mut Host) {
        let m = &host.sim.mario;
        let (px, py, pz) = (m.pos[0], m.pos[1], m.pos[2]);
        let mut taken = vec![];
        for coin in self.coins.iter_mut().filter(|c| !c.taken) {
            let local = to_flat(self.orientation * coin.dir, PLANET_RADIUS);
            if (local.x - px).hypot(local.z - pz) < 110.0 && (coin.height - (py + 70.0)).abs() < 120.0 {
                coin.taken = true;
                taken.push(coin.entity);
            }
        }
        for e in &taken {
            host.commands.entity(*e).despawn();
        }
        if !taken.is_empty() {
            host.fx("coin", [px, py + 70.0, pz]);
            self.collected += taken.len() as u32;
            host.add_coins(taken.len() as u32);
            if self.all_coins.is_none() && self.collected as usize == self.coins.len() {
                self.all_coins = Some(self.ticks);
                let secs = self.ticks as f64 / 30.0;
                let (before, new) = super::records::submit("planet.rush.time", secs, true);
                self.best_rush = super::records::best("planet.rush.time");
                host.toast(&format!("EVERY COIN IN {} - {}", super::race::format_time(secs), super::records::verdict(before, new, super::race::format_time).to_uppercase()), 3.0);
                host.fx("star_sound", [px, py, pz]);
            } else if (self.coins.len() as u32).saturating_sub(self.collected) <= 5 {
                host.toast(&format!("{} COINS LEFT", self.coins.len() as u32 - self.collected), 0.9);
            }
        }
    }

    /// Goal: one lap all the way around the planet.
    fn count_laps(&mut self, host: &mut Host) {
        self.ticks += 1;
        if self.laps_tracker.update(self.orientation) {
            self.laps = self.laps_tracker.laps;
            if self.done.is_none() {
                self.done = Some((self.ticks, self.collected));
                super::records::submit("planet.lap.time", self.ticks as f64 / 30.0, true);
                self.best_lap = super::records::best("planet.lap.time");
                host.toast("ALL THE WAY AROUND! NOW GRAB EVERY COIN", 2.4);
                host.fx("star_sound", host.sim.mario.pos);
            } else {
                host.toast(&format!("LAP {} - ALL THE WAY AROUND!", self.laps), 1.8);
            }
        }
    }

    /// Fresh planet, Mario back on top, coins restored.
    fn start_over(&mut self, host: &mut Host) {
        if let Some(root) = self.spinning.take() {
            host.commands.entity(root).despawn();
        }
        self.pillars.clear();
        self.coins.clear();
        self.orientation = DQuat::IDENTITY;
        self.previous = DQuat::IDENTITY;
        self.travelled = 0.0;
        self.laps = 0;
        self.collected = 0;
        self.ticks = 0;
        self.done = None;
        self.all_coins = None;
        self.laps_tracker = LapTracker::new(self.start_dir);
        host.place_mario([0.0; 3], Some(0.0));
        self.build_planet(host);
        self.sync_features(host);
        host.toast("NEW PLANET - RUN ALL THE WAY AROUND", 1.6);
    }
}

impl Experiment for TinyPlanetExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Planet
    }
    fn replaces_course(&self) -> bool {
        true
    }

    fn enter(&mut self, host: &mut Host) {
        host.set_clear(Color::srgb(0.02, 0.02, 0.1));
        host.set_sky(crate::fx::sky::SkyPalette::from_hex(0x05051a, 0x1b2a6b, 0x6a5acd));
        // An infinite flat floor in Mario's frame; the curvature lives in rendering.
        let mut world = CollisionWorld::new(1024.0, 1e9);
        let mut ground = vec![];
        add_face(&mut world, &[[-40000.0, 0.0, -40000.0], [40000.0, 0.0, -40000.0], [40000.0, 0.0, 40000.0], [-40000.0, 0.0, 40000.0]], [0.0, 1.0, 0.0], &mut ground, &SurfaceOptions::default());
        self.ground = ground.iter().map(id).collect();
        world.finalize();
        host.set_mario_world(Arc::new(world), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1e9);
        host.place_mario([0.0; 3], Some(0.0));
        self.build_planet(host);
        host.cam.far_scale = 3.0;
        self.best_lap = super::records::best("planet.lap.time");
        self.best_rush = super::records::best("planet.rush.time");
        host.card("08", "Tiny Planet", "A world small enough to run around in fifteen seconds. Gravity always pulls toward its core.", &[("Goal", "run all the way around, then grab every coin against the clock"), ("WASD Space", "run / jump"), ("Pillars", "climb them for the high coins"), ("X", "leave")]);
        host.set_dock(vec![super::dock::DockItem::new("New planet", KeyCode::KeyR, 0).primary()]);
        host.toast("TINY PLANET - RUN ALL THE WAY AROUND", 2.6);
        self.sync_features(host);
    }

    fn exit(&mut self, host: &mut Host) {
        host.cam.far_scale = 1.0;
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if key == KeyCode::KeyR {
            self.start_over(host);
            return true;
        }
        false
    }

    fn after_step(&mut self, host: &mut Host) {
        let (dx, dz) = (host.sim.mario.pos[0], host.sim.mario.pos[2]);
        self.previous = self.orientation;
        if let Some(roll) = roll_for(dx, dz, PLANET_RADIUS) {
            // Roll the planet under Mario by the arc he just covered.
            self.orientation = (roll * self.orientation).normalize();
            host.shift_mario([-dx, 0.0, -dz]);
            self.travelled += dx.hypot(dz);
        }
        self.sync_features(host);
        self.collect(host);
        self.count_laps(host);
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.t += dt;
        let Some(spinning) = self.spinning else { return };
        // Interpolate the planet's roll between physics ticks, like Mario's position.
        let q = self.previous.slerp(self.orientation, host.alpha);
        host.commands.entity(spinning).insert(Transform { translation: Vec3::new(0.0, -(PLANET_RADIUS as f32) * SCALE, 0.0), rotation: q.as_quat(), scale: Vec3::ONE });
        let fmt = |b: Option<f64>| b.map_or("-".to_string(), super::race::format_time);
        let clock = super::race::format_time(self.all_coins.unwrap_or(self.ticks) as f64 / 30.0);
        let goal = match (self.done, self.all_coins) {
            (_, Some(_)) => format!("\nEVERY COIN in {clock} (best {}) - {} to rush again", fmt(self.best_rush), host.prompt("press R", "tap NEW PLANET", "RB")),
            (Some(_), None) => format!("\nCOIN RUSH: grab every coin, pillar tops too - {clock} - best {}", fmt(self.best_rush)),
            (None, None) => format!("\nGOAL: run all the way around ({}% there), then grab every coin - {clock} - best lap {}", (100.0 * self.progress()).round(), fmt(self.best_lap)),
        };
        host.status(format!("laps {}   travelled {} m   coins {} / {}{}", self.laps, (self.travelled / 100.0).round(), self.collected, self.coins.len(), goal));
    }

    fn debug(&self) -> String {
        format!("travelled={:.0} laps={} coins={}/{} pillars={} feature_surfaces={}", self.travelled, self.laps, self.collected, self.coins.len(), self.pillars.len(), self.feature_surfaces.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_map_turns_arc_length_into_flat_distance() {
        let r = PLANET_RADIUS;
        // A point a quarter of the way round (90 degrees) lies pi/2 * R away, in its own direction.
        let f = to_flat(DVec3::new(1.0, 0.0, 0.0), r);
        assert!((f.x - std::f64::consts::FRAC_PI_2 * r).abs() < 1e-6 && f.z.abs() < 1e-6);
        let f = to_flat(DVec3::new(0.0, 0.0, -1.0), r);
        assert!((f.z + std::f64::consts::FRAC_PI_2 * r).abs() < 1e-6);
        // The point under Mario maps to the origin.
        let f = to_flat(DVec3::Y, r);
        assert!(f.x == 0.0 && f.z == 0.0 && f.angle == 0.0);
    }

    #[test]
    fn running_the_circumference_brings_the_start_back() {
        let r = PLANET_RADIUS;
        let mut orientation = DQuat::IDENTITY;
        let step = 50.0;
        let steps = (std::f64::consts::TAU * r / step).round() as usize;
        let mut max_angle = 0.0f64;
        for _ in 0..steps {
            // Mario runs toward +X every tick.
            orientation = (roll_for(step, 0.0, r).unwrap() * orientation).normalize();
            max_angle = max_angle.max((orientation * DVec3::Y).angle_between(DVec3::Y));
        }
        let back = (orientation * DVec3::Y).angle_between(DVec3::Y);
        assert!(max_angle > 3.0, "start should reach the far side, got {max_angle}");
        assert!(back < 0.05, "start should come back, got {back}");
    }

    #[test]
    fn a_feature_ahead_approaches_at_running_speed() {
        let r = PLANET_RADIUS;
        let dir = DVec3::new((500.0 / r).sin(), (500.0 / r).cos(), 0.0);
        let before = to_flat(dir, r);
        assert!((before.x - 500.0).abs() < 1e-6);
        let orientation = roll_for(100.0, 0.0, r).unwrap();
        let after = to_flat(orientation * dir, r);
        assert!((after.x - 400.0).abs() < 1e-6, "{}", after.x);
    }
}

#[cfg(test)]
mod goal_tests {
    use super::*;
    use crate::experiments::god::test_bot::Bot;
    use jumpman_sim::mario_runtime::MarioRuntime;

    /// A direction of travel (radians in Mario's flat frame) whose great circle keeps clear of every pillar.
    fn clear_heading(pillars: &[Pillar]) -> Option<f64> {
        let steps = (std::f64::consts::TAU * PLANET_RADIUS / 50.0).round() as usize;
        (0..64).map(|k| k as f64 * std::f64::consts::TAU / 64.0).find(|theta| {
            let (dx, dz) = (theta.cos() * 50.0, theta.sin() * 50.0);
            let mut o = DQuat::IDENTITY;
            (0..steps).all(|_| {
                o = (roll_for(dx, dz, PLANET_RADIUS).unwrap() * o).normalize();
                pillars.iter().all(|p| {
                    let f = to_flat(o * p.dir, PLANET_RADIUS);
                    f.x.hypot(f.z) > p.radius + 260.0
                })
            })
        })
    }

    #[test]
    fn lap_tracker_needs_the_far_side_before_it_counts() {
        let mut t = LapTracker::new(DVec3::Y);
        assert!(!t.update(DQuat::IDENTITY), "standing at the start is not a lap");
        assert!(!t.update(DQuat::from_rotation_z(1.0)));
        assert!(!t.update(DQuat::from_rotation_z(0.05)), "short out-and-back is not a lap");
        assert!(!t.update(DQuat::from_rotation_z(3.0)));
        assert!(t.update(DQuat::from_rotation_z(0.05)), "far side then home is a lap");
        assert_eq!(t.laps, 1);
    }

    /// A real Mario runs the planet's flat floor with the pillars' collision streaming in and the planet
    /// rolling beneath him, exactly as in the experiment, until the lap goal fires.
    #[test]
    fn mario_runs_all_the_way_around_the_planet() {
        let pillars: Vec<Pillar> = roll_pillars(&mut Rng::new(0xb1a5e7), DVec3::Y).into_iter().map(|p| p.0).collect();
        let theta = clear_heading(&pillars).expect("some route round the planet is clear of pillars");
        let mut world = CollisionWorld::new(1024.0, 1e9);
        add_face(&mut world, &[[-40000.0, 0.0, -40000.0], [40000.0, 0.0, -40000.0], [40000.0, 0.0, 40000.0], [-40000.0, 0.0, 40000.0]], [0.0, 1.0, 0.0], &mut vec![], &SurfaceOptions::default());
        world.finalize();
        let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1e9);
        let mut orientation = DQuat::IDENTITY;
        let mut features = HashSet::new();
        let mut tracker = LapTracker::new(DVec3::Y);
        let mut bot = Bot::default();
        let heading = jumpman_sim::math::atan2s(theta.sin(), theta.cos()) as f64;
        let mut lap_tick = None;
        for tick in 0..30 * 120 {
            bot.step(&mut m, heading, false);
            let (dx, dz) = (m.pos[0], m.pos[2]);
            if let Some(roll) = roll_for(dx, dz, PLANET_RADIUS) {
                orientation = (roll * orientation).normalize();
                m.pos[0] -= dx;
                m.pos[2] -= dz;
            }
            features = sync_pillars(Arc::make_mut(&mut m.world), &features, &pillars, orientation);
            if tracker.update(orientation) {
                lap_tick = Some(tick);
                break;
            }
        }
        let tick = lap_tick.expect("Mario never completed a lap");
        eprintln!("lap completed after {:.1}s", tick as f64 / 30.0);
    }
}
