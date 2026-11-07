//! Port of `src/game/movingPlatform.ts`: rigid platforms whose collision is rebuilt every tick and
//! that carry Mario. Engine-free apart from glam math; the Bevy view lives in `objects::platforms`.
use super::course_extras::PlatformSpec;
use bevy::math::{DMat4, DQuat, DVec3, EulerRot};
use jumpman_sim::collision::SurfaceOptions;
use jumpman_sim::js_math::{js_cos, js_sin};
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::math::*;
use jumpman_sim::state::ACT_FLAG_AIR;
use std::sync::Arc;

/// `Surface::object` tag base for platform surfaces (arena object ids are small indices).
pub const PLATFORM_TAG: u32 = 0x4000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiftSound {
    Moving,
    Stop,
    Lower,
    Upper,
}

pub struct MovingPlatform {
    pub spec: PlatformSpec,
    pub tag: u32,
    home: DVec3,
    base_yaw: f64,
    /// Current model transform (three.js: position, Euler 'YXZ', scale).
    pub pos: DVec3,
    /// Euler angles (x, y, z) in radians, applied in YXZ order.
    pub rot: DVec3,
    pub scale: DVec3,
    pub visible: bool,
    /// Transform at the start of the last step (view interpolation).
    pub prev_pos: DVec3,
    pub prev_rot: DVec3,
    waypoint: usize,
    move_yaw: f64,
    pitch_velocity: f64,
    pitch: f64,
    tick: i64,
    phase: i32,
    speed: f64,
    initial_timer: i64,
    tower: bool,
    checker_face_pitch: f64,
    built: Option<(DVec3, DVec3, bool)>,
}

fn radians(angle: f64) -> f64 {
    angle_to_radians(angle)
}

impl MovingPlatform {
    pub fn new(spec: PlatformSpec, index: usize) -> Self {
        let kind = spec.kind;
        let base_yaw = spec.yaw.to_radians();
        let mut home = DVec3::from(spec.pos);
        let initial_timer = if matches!(kind, "sliding_platform" | "small_bomp" | "large_bomp") { (random() * 100.0).floor() as i64 } else { 0 };
        if kind == "sliding_platform" {
            home.x += 2.0;
        }
        let mut scale = DVec3::splat(spec.scale);
        if kind == "checkerboard_lift" {
            scale *= DVec3::new(0.7, 1.5, 0.7);
        }
        let mut p = Self {
            tag: PLATFORM_TAG + index as u32,
            tower: kind.starts_with("tower_"),
            home,
            base_yaw,
            pos: home,
            rot: DVec3::ZERO,
            scale,
            visible: true,
            prev_pos: home,
            prev_rot: DVec3::ZERO,
            waypoint: 1,
            move_yaw: 0.0,
            pitch_velocity: 0.0,
            pitch: 0.0,
            tick: 0,
            phase: 0,
            speed: 0.0,
            initial_timer,
            checker_face_pitch: 0.0,
            built: None,
            spec,
        };
        p.reset_pose();
        p
    }

    /// `model.matrixWorld` of the blocker (position * YXZ rotation * scale).
    pub fn matrix(&self) -> DMat4 {
        DMat4::from_scale_rotation_translation(self.scale, self.quat(), self.pos)
    }

    pub fn quat(&self) -> DQuat {
        DQuat::from_euler(EulerRot::YXZ, self.rot.y, self.rot.x, self.rot.z)
    }

    fn reset_pose(&mut self) {
        self.waypoint = 1;
        self.checker_face_pitch = 0.0;
        self.move_yaw = 0.0;
        self.pitch = 0.0;
        self.pitch_velocity = 0.0;
        self.phase = 0;
        self.speed = 0.0;
        if self.tower {
            self.visible = false;
        }
        self.tick = self.initial_timer;
        self.pos = self.home;
        let yaw = if matches!(self.spec.kind, "sliding_platform" | "small_bomp") { self.base_yaw - std::f64::consts::FRAC_PI_2 } else { self.base_yaw };
        self.rot = DVec3::new(0.0, yaw, 0.0);
        self.prev_pos = self.pos;
        self.prev_rot = self.rot;
    }

    pub fn reset(&mut self, m: &mut MarioRuntime) {
        self.reset_pose();
        self.refresh(m, true);
    }

    /// Remove this platform's surfaces from the world (`refresh` first half).
    pub fn remove_from(&self, world: &mut jumpman_sim::collision::CollisionWorld) {
        let tag = self.tag;
        world.remove_surfaces(&|s| s.object == Some(tag));
    }

    /// Rebuild the collision at the current transform. `force` re-adds even when unchanged.
    pub fn refresh(&mut self, m: &mut MarioRuntime, force: bool) {
        let state = (self.pos, self.rot, self.visible);
        if !force && self.built == Some(state) {
            return;
        }
        self.built = Some(state);
        let world = Arc::make_mut(&mut m.world);
        self.remove_from(world);
        if self.tower && !self.visible {
            return;
        }
        let matrix = self.matrix();
        let verts: Vec<[f64; 3]> = self.spec.collision.vertices.iter().map(|v| matrix.transform_point3(DVec3::from(*v)).to_array()).collect();
        for &[a, b, c, kind, force] in &self.spec.collision.triangles {
            let (Some(&va), Some(&vb), Some(&vc)) = (verts.get(a as usize), verts.get(b as usize), verts.get(c as usize)) else { continue };
            world.add_triangle(va, vb, vc, &SurfaceOptions { room: None, kind, force, flags: 0, object: Some(self.tag) });
        }
    }

    fn step_ropeway(&mut self, m: &MarioRuntime, riding: bool, track: &[[f64; 3]]) {
        if self.phase == 0 {
            if !riding {
                self.tick = 0;
            } else if self.tick > 20 {
                self.phase = 1;
            }
        } else if self.waypoint == track.len() {
            self.speed = 0.0;
            self.waypoint = 1;
            self.phase = 0;
        } else {
            self.speed = (10.0f64).min((self.speed + 0.1) as f32 as f64);
            let mut remaining = self.speed;
            while remaining > 0.0 && self.waypoint < track.len() {
                let [x, y, z] = track[self.waypoint];
                let (dx, dy, dz) = (x - self.pos.x, y - self.pos.y, z - self.pos.z);
                let length = js_hypot3(dx, dy, dz);
                if length > 0.0 {
                    self.move_yaw = atan2s(dz, dx) as f64;
                }
                if length <= remaining {
                    self.pos = DVec3::new(x, y, z);
                    self.waypoint += 1;
                    remaining -= length;
                } else {
                    let fraction = remaining / length;
                    self.pos = DVec3::new(self.pos.x + fraction * dx, self.pos.y + fraction * dy, self.pos.z + fraction * dz);
                    break;
                }
            }
        }

        // platform_on_track_rock_ski_lift advances roll once before oscillate_toward.
        let (dx, dz) = (m.pos[0] - self.pos.x, m.pos[2] - self.pos.z);
        let distance = js_hypot3(dx, m.pos[1] - self.pos.y, dz);
        let lever = distance * sins(atan2s(dz, dx) as f64 - radians_to_angle(self.base_yaw) as f64);
        let target = if riding { (-50.0 * self.speed * sins(self.move_yaw) + (-4.0 * lever).trunc()).trunc() } else { 0.0 };
        self.pitch += self.pitch_velocity.trunc();
        let previous = self.pitch;
        self.pitch += self.pitch_velocity.trunc();
        if self.pitch == target || ((self.pitch - target) * (previous - target) < 0.0 && self.pitch_velocity.abs() < 5.0) {
            self.pitch = target;
            self.pitch_velocity = 0.0;
        } else {
            let accel = if self.pitch >= target { -6.0 } else { 6.0 };
            self.pitch_velocity = clamp(self.pitch_velocity + accel * if self.pitch_velocity * accel < 0.0 { 1.5 } else { 1.0 }, -100.0, 100.0);
        }
        self.rot.z = radians(self.pitch);
    }

    fn step_checkerboard(&mut self) {
        if self.phase == 0 {
            self.phase = if self.spec.param > 0.0 { 1 } else { 3 };
        } else if self.phase == 1 || self.phase == 3 {
            self.pitch = 0.0;
            self.pos.y += if self.phase == 1 { 10.0 } else { -10.0 };
            if self.tick as f64 > self.spec.param.abs() {
                self.phase += 1;
            }
        } else {
            let direction = if self.phase == 2 { 1.0 } else { -1.0 };
            self.pitch += 512.0;
            self.checker_face_pitch = to_s16(self.checker_face_pitch + 512.0) as f64;
            // On the lower turn, the source masks face pitch to a half-turn boundary each frame.
            if direction < 0.0 {
                self.checker_face_pitch = (self.checker_face_pitch as i32 & !0x7fff) as f64;
            }
            self.rot.x = radians(self.checker_face_pitch);
            let forward = direction * 7.0 * sins(self.pitch);
            self.pos.x += forward * js_sin(self.base_yaw);
            self.pos.z += forward * js_cos(self.base_yaw);
            self.pos.y += direction * 7.0 * coss(self.pitch);
            if self.tick + 1 == 64 {
                self.phase = if direction == 1.0 { 3 } else { 1 };
            }
        }
    }

    /// One 30 Hz step. Returns the lift sound cue, if any.
    pub fn step(&mut self, m: &mut MarioRuntime) -> Option<LiftSound> {
        let mut lift_sound = None;
        self.prev_pos = self.pos;
        self.prev_rot = self.rot;
        if self.tower {
            let threshold = self.home.y - 100.0 * self.spec.param - 1000.0;
            if self.visible && m.pos[1] < threshold {
                self.reset(m);
            }
            if !self.visible && m.pos[1] <= threshold {
                return None;
            }
            self.visible = true;
        }
        let riding = m.floor.as_ref().map_or(false, |f| f.object == Some(self.tag)) && m.action & ACT_FLAG_AIR == 0 && (m.pos[1] - m.floor_height).abs() < 4.0;
        let old_inverse = self.matrix().inverse();
        let rider = riding.then(|| old_inverse.transform_point3(DVec3::from(m.pos)));
        let yaw = self.rot.y;
        let old_phase = self.phase;
        match self.spec.kind {
            "checkerboard_lift" => {
                if js_hypot3(m.pos[0] - self.pos.x, m.pos[1] - self.pos.y, m.pos[2] - self.pos.z) < 1000.0 {
                    lift_sound = Some(if self.spec.param > 0.0 { LiftSound::Lower } else { LiftSound::Upper });
                }
                self.step_checkerboard();
            }
            "ropeway_lift" => {
                if self.phase == 1 {
                    lift_sound = Some(if Some(self.waypoint) == self.spec.track.as_ref().map(|t| t.len()) { LiftSound::Stop } else { LiftSound::Moving });
                }
                if let Some(track) = self.spec.track.clone() {
                    self.step_ropeway(m, riding, &track);
                }
            }
            "tower_slide" => {
                let velocity = if self.phase == 0 { -3.0 } else { 3.0 };
                self.pos.x += velocity * js_sin(yaw);
                self.pos.z += velocity * js_cos(yaw);
                if self.tick as f64 > (380.0f64 / 3.0).trunc() {
                    self.phase = 1 - self.phase;
                }
            }
            "tower_elevator" => {
                if self.phase == 0 && riding {
                    self.phase = 1;
                } else if self.phase == 1 || self.phase == 3 {
                    if self.tick > 140 {
                        self.phase = if self.phase == 1 { 2 } else { 0 };
                    } else {
                        self.pos.y += if self.phase == 1 { 5.0 } else { -5.0 };
                    }
                } else if self.phase == 2 && self.tick > 60 {
                    self.phase = 3;
                }
            }
            "seesaw_platform" => {
                self.pitch += self.pitch_velocity.trunc();
                if rider.is_some() {
                    let lever = (m.pos[0] - self.home.x) * js_sin(yaw) + (m.pos[2] - self.home.z) * js_cos(yaw);
                    self.pitch_velocity = clamp(self.pitch_velocity + lever * if self.pitch_velocity * lever < 0.0 { 0.04 } else { 0.02 }, -50.0, 50.0);
                } else {
                    let before = self.pitch;
                    self.pitch += self.pitch_velocity.trunc();
                    if self.pitch == 0.0 || (before * self.pitch < 0.0 && self.pitch_velocity.abs() < 6.0) {
                        self.pitch = 0.0;
                        self.pitch_velocity = 0.0;
                    } else {
                        let accel = if self.pitch >= 0.0 { -3.0 } else { 3.0 };
                        self.pitch_velocity += accel * if self.pitch_velocity * accel < 0.0 { 3.0 } else { 1.0 };
                    }
                }
                self.rot = DVec3::new(radians(self.pitch), yaw, 0.0);
            }
            "rotating_wooden_platform" => {
                if self.phase == 0 {
                    if self.tick > 60 {
                        self.phase = 1;
                    }
                } else {
                    self.rot.y += radians(0x100 as f64);
                    if self.tick > 126 {
                        self.phase = 0;
                    }
                }
            }
            "rotating_platform" => self.rot.y += radians(((self.spec.param as i64) << 4) as f64),
            "merry_go_round" => self.rot.y += radians(0x60 as f64),
            "mesh_elevator" => self.pos.y = self.home.y + js_sin(self.tick as f64 * 0.025) * 420.0,
            "moving_bookshelf" => self.pos.x = self.home.x + js_sin(self.tick as f64 * 0.018) * 240.0,
            "tilting_trap_platform" => self.rot = DVec3::new(js_sin(self.tick as f64 * 0.03) * 0.2, yaw, 0.0),
            "sliding_platform" => {
                let speed = [0.0, 10.0, 15.0, 20.0].get(self.spec.param as usize).copied().unwrap_or(15.0);
                if self.phase == 0 && self.tick > 100 {
                    self.phase = 1;
                    self.speed = speed;
                } else if self.phase == 1 {
                    if self.tick as f64 >= 500.0 / speed {
                        self.speed = 0.0;
                        self.pos.x = self.home.x + 510.0;
                    }
                    if self.tick == 60 {
                        self.phase = 2;
                        self.speed = -speed;
                    }
                } else if self.phase == 2 {
                    if self.tick as f64 >= 500.0 / speed {
                        self.speed = 0.0;
                        self.pos.x = self.home.x;
                    }
                    if self.tick == 90 {
                        self.phase = 1;
                        self.speed = speed;
                    }
                }
                self.pos.x += self.speed;
            }
            "small_bomp" | "large_bomp" => {
                if self.phase == 0 && self.tick > 100 {
                    self.phase = 1;
                    self.speed = 30.0;
                } else if self.phase == 1 {
                    if self.pos.x > 3450.0 {
                        self.pos.x = 3450.0;
                        self.speed = 0.0;
                    }
                    if self.tick == 15 {
                        self.phase = 2;
                        self.speed = if self.spec.kind == "large_bomp" { 10.0 } else { 40.0 };
                    }
                } else if self.phase == 2 {
                    if self.pos.x > 3830.0 {
                        self.pos.x = 3830.0;
                        self.speed = 0.0;
                    }
                    if self.tick == 60 {
                        self.phase = 3;
                        self.speed = -10.0;
                    }
                } else if self.phase == 3 {
                    if self.pos.x < 3330.0 {
                        self.pos.x = 3330.0;
                        self.speed = 0.0;
                    }
                    if self.tick == 90 {
                        self.phase = 1;
                        self.speed = 25.0;
                    }
                }
                self.pos.x += self.speed;
            }
            _ => {}
        }
        self.tick = if old_phase == self.phase { self.tick + 1 } else { 0 };
        if let Some(local) = rider {
            let moved = self.matrix().transform_point3(local);
            m.pos = moved.to_array();
            m.face_angle[1] = to_s16(m.face_angle[1] + js_round((self.rot.y - yaw) * 32768.0 / std::f64::consts::PI)) as f64;
        }
        self.refresh(m, false);
        lift_sound
    }
}

/// All platforms of a course.
pub struct Platforms {
    pub list: Vec<MovingPlatform>,
}

impl Platforms {
    pub fn new(specs: &[PlatformSpec]) -> Self {
        Self { list: specs.iter().cloned().enumerate().map(|(i, s)| MovingPlatform::new(s, i)).collect() }
    }

    /// Build the initial collision (the TS constructor's `refresh` through `syncTransform`).
    pub fn install(&mut self, m: &mut MarioRuntime) {
        for p in &mut self.list {
            p.refresh(m, true);
        }
    }

    pub fn reset(&mut self, m: &mut MarioRuntime) {
        for p in &mut self.list {
            p.reset(m);
        }
    }

    pub fn step(&mut self, m: &mut MarioRuntime) -> Vec<(DVec3, LiftSound)> {
        let mut sounds = vec![];
        for p in &mut self.list {
            if let Some(s) = p.step(m) {
                sounds.push((p.pos, s));
            }
        }
        sounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::course_data::CollisionData;
    use jumpman_sim::collision::CollisionWorld;
    use jumpman_sim::mario_runtime::SpawnPoint;

    /// A 200x200 square floor at y = 0 around the origin.
    fn slab() -> CollisionData {
        CollisionData {
            rooms: vec![],
            vertices: vec![[-100.0, 0.0, -100.0], [100.0, 0.0, -100.0], [100.0, 0.0, 100.0], [-100.0, 0.0, 100.0]],
            triangles: vec![[0, 2, 1, 0, 0], [0, 3, 2, 0, 0]],
        }
    }

    fn spec(kind: &'static str, param: f64, pos: [f64; 3]) -> PlatformSpec {
        PlatformSpec { model: String::new(), pos, yaw: 0.0, kind, param, track: None, scale: 1.0, collision: slab() }
    }

    fn mario_on(pos: [f64; 3]) -> MarioRuntime {
        let mut ground = CollisionWorld::default();
        ground.add_triangle([-5000.0, -500.0, -5000.0], [5000.0, -500.0, 5000.0], [5000.0, -500.0, -5000.0], &SurfaceOptions::default());
        ground.add_triangle([-5000.0, -500.0, -5000.0], [-5000.0, -500.0, 5000.0], [5000.0, -500.0, 5000.0], &SurfaceOptions::default());
        MarioRuntime::new(Arc::new(ground), SpawnPoint { action: None, pos, face_yaw: 0.0 }, -9000.0)
    }

    #[test]
    fn collision_follows_a_lift_and_carries_mario() {
        let mut m = mario_on([0.0, 100.0, 0.0]);
        let mut platforms = Platforms::new(&[spec("mesh_elevator", 0.0, [0.0, 100.0, 0.0])]);
        platforms.install(&mut m);
        assert_eq!(m.world.find_floor(0.0, 150.0, 0.0).height, 100.0);
        let idle = jumpman_sim::state::ControllerSnapshot::default();
        // Let Mario settle on the (still) platform first.
        // `MarioRuntime::new` snapped him to the ground below; put him on the platform.
        m.pos = [0.0, 100.0, 0.0];
        for _ in 0..10 {
            m.step(&idle, 0.0, false, false);
        }
        assert_eq!(m.floor_height, 100.0);
        for _ in 0..40 {
            m.step(&idle, 0.0, false, false);
            platforms.step(&mut m);
        }
        let top = platforms.list[0].pos.y;
        assert!(top > 100.0 + 100.0, "elevator rose to {top}");
        let floor = m.world.find_floor(0.0, top + 50.0, 0.0);
        assert!((floor.height - top).abs() < 1e-6);
        // Mario stands on the platform and has been carried with it.
        assert!((m.pos[1] - top).abs() < 8.0, "mario {} platform {top}", m.pos[1]);
    }

    #[test]
    fn rotating_platform_turns_and_keeps_surfaces_tagged() {
        let mut m = mario_on([3000.0, 0.0, 3000.0]);
        let mut platforms = Platforms::new(&[spec("rotating_platform", 8.0, [0.0, 0.0, 0.0])]);
        platforms.install(&mut m);
        let count = m.world.surfaces().iter().filter(|s| s.object == Some(PLATFORM_TAG)).count();
        assert_eq!(count, 2);
        for _ in 0..10 {
            platforms.step(&mut m);
        }
        let expected = radians((8i64 << 4) as f64 * 10.0);
        assert!((platforms.list[0].rot.y - expected).abs() < 1e-9);
        // Rebuilding never leaks surfaces.
        assert_eq!(m.world.surfaces().iter().filter(|s| s.object == Some(PLATFORM_TAG)).count(), 2);
    }

    #[test]
    fn tower_platform_hidden_until_mario_climbs() {
        let mut m = mario_on([0.0, 0.0, 0.0]);
        let mut platforms = Platforms::new(&[spec("tower_solid", 0.0, [0.0, 3000.0, 0.0])]);
        platforms.install(&mut m);
        assert!(m.world.surfaces().iter().all(|s| s.object != Some(PLATFORM_TAG)));
        // threshold = 3000 - 1000 = 2000; Mario above it reveals the platform.
        m.pos[1] = 2500.0;
        platforms.step(&mut m);
        assert!(platforms.list[0].visible);
        assert!(m.world.surfaces().iter().any(|s| s.object == Some(PLATFORM_TAG)));
    }

    #[test]
    fn checkerboard_lift_rises_then_flips() {
        let mut m = mario_on([3000.0, 0.0, 3000.0]);
        let mut platforms = Platforms::new(&[spec("checkerboard_lift", 40.0, [0.0, 300.0, 0.0])]);
        platforms.install(&mut m);
        for _ in 0..30 {
            platforms.step(&mut m);
        }
        // param 40: ten units per tick for 41 ticks.
        assert!(platforms.list[0].pos.y > 300.0 + 200.0);
    }
}
