//! Port of `CameraRig.ts`: third-person orbit camera. The rig's yaw is the direction from Mario
//! to the camera, which is the convention SM64 uses for `m->area->camera->yaw`.
//!
//! Works in SM64 units with `DVec3`; `camera::sync_camera` converts to Bevy space. Instead of
//! raycasting a THREE mesh, the wall pull-in intersects the collision triangles directly.
use super::summit::SummitCamera;
use super::transition::CameraTransition;
use super::wall::{radial_wall_yaw, wall_push};
use bevy::math::DVec3;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::math::{angle_to_radians, atan2s, clamp, coss, sins, to_s16};
use jumpman_sim::state::*;
use jumpman_sim::surfaces::{SURFACE_DEATH_PLANE, SURFACE_FLAG_NO_CAM_COLLISION};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraMode {
    Lakitu,
    Close,
    TopDown,
    Side,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraContext {
    Radial,
    Normal,
    Boss,
    Water,
    Slide,
    Interior,
    Flight,
}

/// Per-frame inputs (`CameraRig.update` parameters).
#[derive(Clone, Debug, Default)]
pub struct RigInput {
    pub dt: f64,
    pub mario_pos: DVec3,
    /// s16
    pub mario_yaw: f64,
    pub moving: bool,
    pub speed: f64,
    pub opponent: Option<DVec3>,
    pub action: u32,
    /// (pos.y, hitbox height) of the pole Mario is on.
    pub pole: Option<(f64, f64)>,
    pub mario_pitch: f64,
    pub spin: f64,
}

pub struct CameraRig {
    pub eye: DVec3,
    pub look_at: DVec3,
    pub yaw: f64,
    pub pitch: f64,
    pub distance: f64,
    pub mode: CameraMode,

    focus: DVec3,
    smoothing_distance: f64,
    manual_timer: f64,
    battlefield_floor: Option<i32>,
    radial_offset: f64,
    radial_ticks: f64,
    radial_return: Option<CameraTransition>,
    leaving_summit: bool,
    rendered_radial_eye: DVec3,
    last_radial_eye: Option<DVec3>,
    pan_distance: f64,
    radial_target: f64,
    boss_focus: Option<DVec3>,
    boss_held: bool,
    summit: Option<SummitCamera>,
    water_mode: bool,
    slide_mode: bool,
    interior_mode: bool,
    flight_mode: bool,
    initialized: bool,
    side_ticks: f64,
    pitch_ticks: f64,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            eye: DVec3::new(0.0, 500.0, 900.0),
            look_at: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.31,
            distance: 900.0,
            mode: CameraMode::Lakitu,
            focus: DVec3::ZERO,
            smoothing_distance: 900.0,
            manual_timer: 0.0,
            battlefield_floor: None,
            radial_offset: 0.0,
            radial_ticks: 0.0,
            radial_return: None,
            leaving_summit: false,
            rendered_radial_eye: DVec3::ZERO,
            last_radial_eye: None,
            pan_distance: 0.0,
            radial_target: 0.0,
            boss_focus: None,
            boss_held: false,
            summit: None,
            water_mode: false,
            slide_mode: false,
            interior_mode: false,
            flight_mode: false,
            initialized: false,
            side_ticks: 0.0,
            pitch_ticks: 0.0,
        }
    }
}

/// Distance along `dir` (unit) from `origin` to the nearest camera-visible triangle, up to `max`.
pub fn camera_ray(world: &CollisionWorld, origin: DVec3, dir: DVec3, max: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for s in world.surfaces() {
        if s.flags & SURFACE_FLAG_NO_CAM_COLLISION != 0 || s.kind == SURFACE_DEATH_PLANE {
            continue;
        }
        let a = DVec3::from(s.v1);
        let e1 = DVec3::from(s.v2) - a;
        let e2 = DVec3::from(s.v3) - a;
        let p = dir.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-9 {
            continue;
        }
        let inv = 1.0 / det;
        let t_vec = origin - a;
        let u = t_vec.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = t_vec.cross(e1);
        let v = dir.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = e2.dot(q) * inv;
        if t >= 0.0 && t <= max && best.map_or(true, |b| t < b) {
            best = Some(t);
        }
    }
    best
}

impl CameraRig {
    pub fn set_mode(&mut self, mode: CameraMode) {
        self.mode = mode;
        let (distance, pitch) = match mode {
            CameraMode::Lakitu => (900.0, 0.31),
            CameraMode::Close => (480.0, 0.22),
            CameraMode::TopDown => (1350.0, 1.18),
            CameraMode::Side => (900.0, 0.13),
        };
        self.distance = distance;
        self.pitch = pitch;
        self.smoothing_distance = distance;
    }

    pub fn cycle_mode(&mut self) -> CameraMode {
        let order = [CameraMode::Lakitu, CameraMode::Close, CameraMode::TopDown, CameraMode::Side];
        let i = order.iter().position(|m| *m == self.mode).unwrap_or(0);
        self.set_mode(order[(i + 1) % order.len()]);
        self.mode
    }

    pub fn set_battlefield_floor(&mut self, surface: Option<i32>) {
        if surface == self.battlefield_floor {
            return;
        }
        if surface.is_none() {
            self.radial_offset = 0.0;
            self.radial_target = 0.0;
        }
        match surface {
            Some(0x70) => self.radial_target = -PI / 3.0,
            Some(0x6f) => self.radial_target = PI / 3.0,
            Some(0x6e) => self.radial_target = 0.0,
            _ => {}
        }
        self.battlefield_floor = surface;
    }

    fn battlefield_camera(&self) -> bool {
        self.battlefield_floor.is_some() && self.mode == CameraMode::Lakitu && !self.flight_mode
    }

    pub fn set_boss_focus(&mut self, position: Option<DVec3>, held: bool) {
        self.boss_held = held;
        self.boss_focus = position;
    }
    pub fn set_water_mode(&mut self, v: bool) {
        self.water_mode = v;
    }
    pub fn set_slide_mode(&mut self, v: bool) {
        self.slide_mode = v;
    }
    pub fn set_interior_mode(&mut self, v: bool) {
        self.interior_mode = v;
    }
    pub fn set_flight_mode(&mut self, v: bool) {
        self.flight_mode = v;
    }

    pub fn context(&self) -> CameraContext {
        if self.boss_focus.is_some() {
            CameraContext::Boss
        } else if self.water_mode {
            CameraContext::Water
        } else if self.flight_mode {
            CameraContext::Flight
        } else if self.slide_mode {
            CameraContext::Slide
        } else if self.battlefield_camera() && self.battlefield_floor != Some(0x0b) {
            CameraContext::Radial
        } else if self.interior_mode {
            CameraContext::Interior
        } else {
            CameraContext::Normal
        }
    }

    /// Yaw as the s16 the sim expects.
    pub fn yaw_s16(&self) -> i32 {
        to_s16(((self.yaw / TAU) * 65536.0).round())
    }

    pub fn displace(&mut self, d: DVec3) {
        if let Some(e) = self.last_radial_eye.as_mut() {
            *e += d;
            self.rendered_radial_eye += d;
        }
        self.focus += d;
        self.eye += d;
    }

    pub fn snap_behind(&mut self, face_yaw: f64) {
        self.yaw = ((to_s16(face_yaw + 0x8000 as f64) as f64 * TAU) / 65536.0 + TAU) % TAU;
        self.initialized = false;
        self.summit = None;
        self.last_radial_eye = None;
        self.radial_ticks = 0.0;
        self.radial_return = None;
        self.leaving_summit = false;
        self.pan_distance = 0.0;
    }

    /// Mouse drag (pixels).
    pub fn rotate(&mut self, dx: f64, dy: f64) {
        if let Some(s) = self.summit.as_mut() {
            s.drag(dx, dy);
            return;
        }
        if self.battlefield_camera() {
            self.radial_target = clamp(self.radial_target - dx * 0.0042, -PI * 2.0 / 3.0, PI * 2.0 / 3.0);
        }
        self.yaw -= dx * 0.0042;
        self.pitch = clamp(self.pitch + dy * 0.0034, 0.03, 1.25);
        self.manual_timer = 2.0;
    }

    /// Keyboard / stick orbit (radians).
    pub fn orbit(&mut self, d_yaw: f64, d_pitch: f64) {
        if let Some(s) = self.summit.as_mut() {
            s.orbit(d_yaw, d_pitch);
            return;
        }
        if d_yaw == 0.0 && d_pitch == 0.0 {
            return;
        }
        if self.battlefield_camera() {
            self.radial_target = clamp(self.radial_target + d_yaw, -PI * 2.0 / 3.0, PI * 2.0 / 3.0);
        }
        self.yaw += d_yaw;
        self.pitch = clamp(self.pitch + d_pitch, 0.03, 1.25);
        self.manual_timer = 2.0;
    }

    pub fn zoom(&mut self, delta: f64) {
        if let Some(s) = self.summit.as_mut() {
            s.zoomed_out = delta > 0.0;
            return;
        }
        self.distance = clamp(self.distance + delta * 0.45, 230.0, 1600.0);
        self.manual_timer = 2.0;
    }

    /// Lakitu-style follow: after the player stops orbiting, ease behind Mario.
    fn follow_yaw(&mut self, dt: f64, mario_yaw: f64, moving: bool) {
        self.manual_timer = (self.manual_timer - dt).max(0.0);
        if self.manual_timer > 0.0 || !moving || matches!(self.mode, CameraMode::TopDown | CameraMode::Side) {
            return;
        }
        let target = yaw_for(mario_yaw + 0x8000 as f64);
        let delta = ((target - self.yaw + PI * 3.0) % TAU) - PI;
        let rate = if self.mode == CameraMode::Close { 1.4 } else { 0.85 };
        self.yaw += clamp(delta, -rate * dt, rate * dt);
        self.yaw = ((self.yaw % TAU) + TAU) % TAU;
    }

    pub fn update(&mut self, i: &RigInput, world: &CollisionWorld) {
        let dt = i.dt;
        let mario = i.mario_pos;
        let battlefield = self.battlefield_camera() && i.opponent.is_none();
        let radial = battlefield && self.boss_focus.is_none() && self.battlefield_floor != Some(0x0b);
        if !radial {
            self.last_radial_eye = None;
            self.radial_return = None;
            self.leaving_summit = false;
        }
        if battlefield {
            if let Some(boss) = self.boss_focus {
                if !self.initialized {
                    self.focus = mario + DVec3::new(0.0, 125.0, 0.0);
                    self.eye = self.focus + DVec3::new(0.0, 0.0, self.distance);
                    self.initialized = true;
                }
                let (eye, focus) = (self.eye, self.focus);
                let yaw = self.yaw;
                let summit = self.summit.get_or_insert_with(|| SummitCamera::new(eye, focus, yaw));
                let (e, f) = summit.update(dt, mario, boss, self.boss_held, i.mario_yaw, i.mario_pitch, i.spin, Some(world));
                self.yaw = summit.yaw;
                self.eye = e;
                self.focus = f;
                self.look_at = f;
                return;
            }
        }
        if self.summit.is_some() {
            self.leaving_summit = radial;
            self.radial_offset = 0.0;
            self.radial_target = 0.0;
            self.summit = None;
        }
        if radial {
            self.radial_ticks += dt * 30.0;
            if self.last_radial_eye.is_none() {
                self.radial_ticks = self.radial_ticks.max(1.0);
            }
            while self.radial_ticks >= 1.0 {
                self.radial_ticks -= 1.0;
                self.update_radial(i, world);
            }
            self.eye = self.rendered_radial_eye;
            self.look_at = self.focus;
            return;
        }
        if let Some(opp) = i.opponent {
            let behind = (mario.x - opp.x).atan2(mario.z - opp.z);
            let delta = behind - self.yaw;
            self.yaw += delta.sin().atan2(delta.cos()) * (1.0 - (-8.0 * dt).exp());
        } else {
            self.follow_yaw(dt, i.mario_yaw, i.moving);
        }
        if !self.initialized {
            self.focus = DVec3::new(mario.x, mario.y + 92.0, mario.z);
            self.smoothing_distance = self.distance;
            self.initialized = true;
        }

        let mut target = DVec3::new(mario.x, mario.y + 92.0, mario.z);
        if battlefield {
            target.y = mario.y + 125.0;
        }
        let look_ahead = if battlefield { 0.0 } else { 90.0f64.min(i.speed.max(0.0) * 2.2) };
        let ang = to_s16(i.mario_yaw) as f64 * TAU / 65536.0;
        target.x += ang.sin() * look_ahead;
        target.z += ang.cos() * look_ahead;
        if let Some(opp) = i.opponent {
            target = DVec3::new(mario.x, mario.y + 95.0, mario.z).lerp(opp, 0.35);
        } else if let Some(boss) = self.boss_focus {
            target.x += (boss.x - target.x) * 0.35;
            target.y += (boss.y + 80.0 - target.y) * 0.25;
            target.z += (boss.z - target.z) * 0.35;
        }
        self.focus = self.focus.lerp(target, 1.0 - (-14.0 * dt).exp());

        let mut distance = self.distance;
        let mut pitch = self.pitch;
        let lakitu = self.mode == CameraMode::Lakitu;
        if let Some(opp) = i.opponent {
            distance = clamp(450.0 + mario.distance(opp) * 0.65, 520.0, 1600.0);
            pitch = 0.29;
        } else if battlefield {
            distance = (distance - 100.0).max(230.0);
        } else if lakitu && self.boss_focus.is_some() {
            distance = distance.max(1100.0);
            pitch = pitch.max(0.36);
        } else if lakitu && self.water_mode {
            distance = distance.min(700.0);
            pitch = pitch.min(0.18);
        } else if lakitu && self.flight_mode {
            distance = distance.max(980.0);
            pitch = pitch.max(0.28);
        } else if lakitu && self.slide_mode {
            distance = distance.min(620.0);
            pitch = pitch.min(0.14);
        } else if lakitu && self.interior_mode {
            distance = distance.min(620.0);
            pitch = pitch.min(0.2);
        }
        let (cp, sp) = (pitch.cos(), pitch.sin());
        let mut desired = DVec3::new(
            self.focus.x + self.yaw.sin() * cp * distance,
            self.focus.y + sp * distance,
            self.focus.z + self.yaw.cos() * cp * distance,
        );

        // Pull the camera in if a wall is between it and Mario.
        let dir = desired - self.focus;
        let length = dir.length();
        let mut allowed = length;
        if length > 1.0 {
            if let Some(hit) = camera_ray(world, self.focus, dir / length, length) {
                allowed = (hit - 22.0).max(0.01);
            }
        }
        if allowed < self.smoothing_distance {
            self.smoothing_distance = allowed;
        } else {
            self.smoothing_distance += (allowed - self.smoothing_distance) * (1.0 - (-5.0 * dt).exp());
        }
        let scale = if length != 0.0 { self.smoothing_distance / length } else { 0.0 };
        desired = DVec3::new(
            self.focus.x + self.yaw.sin() * cp * distance * scale,
            self.focus.y + sp * distance * scale,
            self.focus.z + self.yaw.cos() * cp * distance * scale,
        );
        if battlefield {
            desired.x = clamp(desired.x, -8000.0, 8000.0);
            desired.z = clamp(desired.z, -8000.0, 7050.0);
            self.yaw = (desired.x - self.focus.x).atan2(desired.z - self.focus.z);
        }
        self.eye = desired;

        // Keep the camera above the terrain.
        let floor = world.find_floor(self.eye.x, self.eye.y, self.eye.z);
        if floor.surface.is_some() {
            self.eye.y = self.eye.y.max(floor.height + 30.0);
        }
        self.pan_distance = 0.0;
        self.look_at = self.focus;
    }

    fn radial_yaw(&mut self, mario: DVec3, speed: f64, world: &CollisionWorld) -> f64 {
        let old = self.last_radial_eye;
        let area_yaw = atan2s(mario.z + 4608.0, mario.x - 3072.0) as f64;
        let avoid = old.and_then(|o| radial_wall_yaw(mario, o, world));
        let mut safe_offset: Option<f64> = None;
        if let Some(avoid) = avoid {
            let tangent = if avoid - area_yaw + (0x4000 as f64) < 0.0 { to_s16(avoid + 0x8000 as f64) as f64 } else { avoid };
            safe_offset = Some(angle_to_radians(clamp(to_s16(tangent - area_yaw) as f64, -0x4aaa as f64, 0x4aaa as f64)));
        }
        let rotation = self.radial_target - self.radial_offset;
        if rotation != 0.0 {
            match safe_offset {
                Some(safe)
                    if (rotation > 0.0 && safe + angle_to_radians(16.0) < self.radial_offset)
                        || (rotation < 0.0 && safe - angle_to_radians(16.0) > self.radial_offset) =>
                {
                    self.radial_offset = safe;
                    self.radial_target = safe;
                }
                _ => {
                    let step = angle_to_radians(0x200 as f64);
                    self.radial_offset += clamp(rotation, -step, step);
                }
            }
        } else if let Some(old) = old {
            if let Some(safe) = safe_offset {
                let diff = to_s16(self.radial_offset * 32768.0 / PI - safe * 32768.0 / PI) as f64;
                self.radial_offset -= angle_to_radians((diff / 10.0).trunc());
            } else {
                let rel = to_s16(atan2s(old.z - mario.z, old.x - mario.x) as f64 - area_yaw) as f64;
                let goal = angle_to_radians(clamp(rel, -0x2aaa as f64, 0x2aaa as f64));
                let step = angle_to_radians((speed.abs() * 4.0).trunc());
                self.radial_offset += clamp(goal - self.radial_offset, -step, step);
            }
            self.radial_target = self.radial_offset;
        }
        to_s16(area_yaw + self.radial_offset * 32768.0 / PI) as f64
    }

    fn update_radial(&mut self, i: &RigInput, world: &CollisionWorld) {
        let mario = i.mario_pos;
        let (speed, action) = (i.speed, i.action);
        let old = self.last_radial_eye;
        let yaw = self.radial_yaw(mario, speed, world);
        let floor = world.find_floor_ex(mario.x, mario.y + 10.0, mario.z, false, false).height;
        let water = world.water_level(mario.x, mario.z);
        let height = (if action & ACT_FLAG_METAL_WATER != 0 { floor } else { floor.max(water) }) - mario.y;
        let on_low_pole = action & ACT_FLAG_ON_POLE != 0 && i.pole.map_or(false, |(py, ph)| floor >= py && mario.y < py + 0.7 * ph);
        let probe = world.find_floor_ex(mario.x + 40.0 * sins(yaw), mario.y, mario.z + 40.0 * coss(yaw), false, true);
        let rise = if probe.surface.is_some() { probe.height } else { mario.y } - mario.y;
        let slope = match &probe.surface {
            Some(s) if s.kind != 0x28 && rise > 0.0 && (s.normal[2] != 0.0 || rise >= 100.0) => atan2s(40.0, rise) as f64,
            _ => 0.0,
        };
        let pitch = 0x5b0 as f64 + slope + (self.pitch - 0.31) * 32768.0 / PI;
        let distance = self.distance + 100.0;
        let mut eye = DVec3::ZERO;
        eye.x = clamp(mario.x + distance * coss(pitch) * sins(yaw), -8000.0, 8000.0);
        eye.z = clamp(mario.z + distance * coss(pitch) * coss(yaw), -8000.0, 7050.0);
        let bound = if on_low_pole { 1200.0 } else { 200.0 };
        let goal_height = mario.y + 125.0 + clamp(height, -bound, bound) + distance * sins(pitch);
        eye.y = self.radial_height(mario, action, floor, goal_height, old.map_or(goal_height, |o| o.y), eye, world);
        self.yaw = angle_to_radians(atan2s(eye.z - mario.z, eye.x - mario.x) as f64);
        let mut target = mario;
        target.y += 125.0 + clamp(0.9 * height, -200.0, 200.0);
        self.pan_ahead(mario, i.mario_yaw, action, eye, &mut target);
        self.last_radial_eye = Some(eye);
        if self.leaving_summit {
            self.radial_return = Some(CameraTransition::new(self.eye, self.focus, target, 60.0));
            self.rendered_radial_eye = self.eye;
            self.leaving_summit = false;
        }
        let mut eye = eye;
        if let Some(t) = self.radial_return.as_mut().filter(|t| t.left > 0.0) {
            let (e, f) = t.step(eye, target, Some(world));
            eye = e;
            target = f;
            self.yaw = angle_to_radians(atan2s(eye.z - target.z, eye.x - target.x) as f64);
        }
        if old.is_some() || self.radial_return.is_some() {
            self.focus.x += (target.x - self.focus.x) * 0.8;
            self.focus.y += (target.y - self.focus.y) * 0.3;
            self.focus.z += (target.z - self.focus.z) * 0.8;
            self.rendered_radial_eye = self.rendered_radial_eye.lerp(eye, 0.3);
        } else {
            self.focus = target;
            self.rendered_radial_eye = eye;
            self.initialized = true;
        }
        let pos = &mut self.rendered_radial_eye;
        let below = world.find_floor_ex(pos.x, pos.y + 20.0, pos.z, false, true);
        if below.surface.is_some() {
            pos.y = pos.y.max(below.height + 100.0);
        }
    }

    /// set_camera_height raises the eye immediately over higher ground, but limits its vertical
    /// speed over depressions and when Mario hangs from a ceiling.
    #[allow(clippy::too_many_arguments)]
    fn radial_height(&self, mario: DVec3, action: u32, floor: f64, mut goal_y: f64, old_y: f64, eye: DVec3, world: &CollisionWorld) -> f64 {
        let eye_floor = world.find_floor_ex(eye.x, old_y + 100.0, eye.z, false, true).height;
        let eye_ceil = world.find_ceil(eye.x, old_y - 50.0, eye.z);
        let mario_ceil = world.find_ceil(mario.x, mario.y - 10.0, mario.z);
        if action & ACT_FLAG_HANGING != 0 && mario_ceil.surface.is_some() {
            let base = floor.max(mario_ceil.height - 400.0);
            goal_y = (base + (mario_ceil.height - base) * 0.4).max(mario.y - 400.0);
            return old_y + clamp(goal_y - old_y, -5.0, 5.0);
        }
        let min = floor.max(eye_floor) + 125.0;
        let y = if goal_y < min { min } else { old_y + clamp(goal_y - old_y, -20.0, 20.0) };
        if eye_ceil.surface.is_some() && (floor + 125.0 < eye_ceil.height - 125.0 || (mario_ceil.surface.is_some() && mario_ceil.height > eye_ceil.height - 125.0)) {
            return y.min(eye_ceil.height - 125.0);
        }
        y
    }

    fn pan_ahead(&mut self, mario: DVec3, face_yaw: f64, action: u32, eye: DVec3, target: &mut DVec3) {
        let camera_yaw = atan2s(mario.z - eye.z, mario.x - eye.x) as f64;
        let side = sins(face_yaw) * coss(camera_yaw) - coss(face_yaw) * sins(camera_yaw);
        let reverse = action == ACT_LONG_JUMP || (action & ACT_FLAG_ON_POLE != 0 && action != ACT_TOP_OF_POLE);
        let sleeping = action == ACT_SLEEPING || action == ACT_START_SLEEPING;
        let pan = if sleeping { 0.0 } else { side * sins(0xc00 as f64) * eye.distance(mario) * if reverse { -1.0 } else { 1.0 } };
        self.pan_distance += (pan - self.pan_distance) * 0.025;
        target.x += self.pan_distance * coss(camera_yaw);
        target.z -= self.pan_distance * sins(camera_yaw);
    }

    /// mode_behind_mario, one fixed tick after Mario advances his cannon trajectory.
    pub fn follow_cannon_shot(&mut self, pos: [f64; 3], face: [f64; 3], side: f64, vertical: f64, launch: bool, world: &CollisionWorld) {
        if launch {
            self.pan_distance = 0.0;
            self.pitch_ticks = 0.0;
            self.side_ticks = 0.0;
        }
        let mario = DVec3::from(pos);
        self.focus = mario + DVec3::new(0.0, 125.0, 0.0);
        let mut eye = self.eye;
        let (dx, dy, dz) = (eye.x - self.focus.x, eye.y - self.focus.y, eye.z - self.focus.z);
        let mut distance = 800.0f64.min((dx * dx + dy * dy + dz * dz).sqrt());
        let mut yaw = atan2s(dz, dx) as f64;
        let mut pitch = atan2s(jumpman_sim::math::js_hypot(dx, dz), dy) as f64;
        let mut yaw_divisor = clamp(32.0 - (pitch.abs() / 512.0).trunc(), 1.0, 32.0);
        let mut yaw_goal = face[1] + 0x8000 as f64;
        let mut pitch_goal = -face[0];
        let mut pitch_step = 128.0;
        if self.side_ticks > 0.0 {
            self.side_ticks -= 1.0;
            yaw_divisor = 8.0;
        }
        if self.pitch_ticks > 0.0 {
            self.pitch_ticks -= 1.0;
            pitch_goal = 0.0;
            pitch_step = 0x800 as f64;
        }
        if side != 0.0 {
            distance = 800.0f64.min(distance + 5.0);
            yaw_goal += side * 0x3ff8 as f64;
            self.side_ticks = 30.0;
            yaw_divisor = 2.0;
        }
        if vertical != 0.0 {
            distance = 800.0f64.min(distance + 5.0);
            pitch_goal = -vertical * 0x3000 as f64;
            self.pitch_ticks = 30.0;
            pitch_step = 0x800 as f64;
        }
        yaw = to_s16(yaw - (to_s16(yaw - yaw_goal) as f64 / yaw_divisor).trunc()) as f64;
        pitch = to_s16(pitch + clamp(to_s16(pitch_goal - pitch) as f64, -pitch_step, pitch_step)) as f64;
        distance = distance.max(300.0);
        eye.x = self.focus.x + distance * coss(pitch) * sins(yaw);
        eye.z = self.focus.z + distance * coss(pitch) * coss(yaw);
        eye.y += clamp(self.focus.y + distance * sins(pitch) - eye.y, -50.0, 50.0);
        eye = wall_push(world, eye, 0.0, 100.0);
        let floor = world.find_floor_ex(eye.x, eye.y + 50.0, eye.z, false, true);
        let ceil = world.find_ceil(eye.x, eye.y - 50.0, eye.z);
        let low = if floor.surface.is_some() { floor.height + 125.0 } else { f64::NEG_INFINITY };
        let high = if ceil.surface.is_some() { ceil.height - 125.0 } else { f64::INFINITY };
        if eye.y <= low && eye.y < high {
            eye.y = low;
        }
        if eye.y > low && eye.y >= high {
            eye.y = high;
        }
        if eye.y <= low && eye.y >= high {
            eye.y = (low + high) / 2.0;
        }
        let d = (eye - self.focus).clamp_length_max(800.0);
        eye = self.focus + d;
        self.eye = eye;
        self.yaw = angle_to_radians(yaw);
        let mut look = self.focus;
        self.pan_ahead(mario, face[1], 0, eye, &mut look);
        self.look_at = look;
    }
}

fn yaw_for(face_yaw: f64) -> f64 {
    ((to_s16(face_yaw) as f64 * TAU) / 65536.0 + TAU) % TAU
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::SurfaceOptions;

    fn flat_world() -> CollisionWorld {
        let mut w = CollisionWorld::default();
        let o = SurfaceOptions::default();
        w.add_quad([-4000., 0., -4000.], [-4000., 0., 4000.], [4000., 0., 4000.], [4000., 0., -4000.], &o);
        w
    }

    #[test]
    fn orbits_behind_and_above_mario() {
        let world = flat_world();
        let mut rig = CameraRig::default();
        rig.snap_behind(0.0);
        let input = RigInput { dt: 1.0 / 60.0, mario_pos: DVec3::ZERO, ..Default::default() };
        for _ in 0..60 {
            rig.update(&input, &world);
        }
        assert!(rig.eye.y > 100.0);
        // yaw ~ pi: camera sits at -Z of Mario facing +Z (face yaw 0 -> camera behind).
        assert!(rig.eye.z < -300.0, "{:?}", rig.eye);
        assert_eq!(rig.yaw_s16().abs(), 32768);
    }

    #[test]
    fn camera_ray_hits_wall() {
        let mut w = flat_world();
        let o = SurfaceOptions::default();
        w.add_quad([-500., 0., 500.], [500., 0., 500.], [500., 800., 500.], [-500., 800., 500.], &o);
        let d = camera_ray(&w, DVec3::new(0.0, 100.0, 0.0), DVec3::Z, 2000.0).expect("hit");
        assert!((d - 500.0).abs() < 1e-6);
    }
}
