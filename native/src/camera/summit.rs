//! Port of `SummitCamera.ts`: the Battlefield branch of camera.c's `update_boss_fight_camera`,
//! stepped at 30 Hz.
use super::transition::CameraTransition;
use bevy::math::DVec3;
use jumpman_sim::collision::{CollisionWorld, CELL_HEIGHT_LIMIT};
use jumpman_sim::math::{angle_to_radians, atan2s, clamp, coss, sins, to_s16};

pub struct SummitCamera {
    goal_focus: DVec3,
    goal_eye: DVec3,
    next_eye: DVec3,
    next_focus: DVec3,
    eye: DVec3,
    focus: DVec3,
    transition: Option<CameraTransition>,
    entering: bool,
    ticks: f64,
    offset_yaw: f64,
    side_frames: f64,
    side_input: f64,
    zoom_dist: f64,
    zoom_pitch: f64,
    pub zoomed_out: bool,
    /// Radians.
    pub yaw: f64,
}

impl SummitCamera {
    pub fn new(eye: DVec3, focus: DVec3, yaw: f64) -> Self {
        Self {
            goal_focus: DVec3::ZERO,
            goal_eye: DVec3::ZERO,
            next_eye: DVec3::ZERO,
            next_focus: DVec3::ZERO,
            eye,
            focus,
            transition: None,
            entering: true,
            ticks: 0.0,
            offset_yaw: 0.0,
            side_frames: 0.0,
            side_input: 0.0,
            zoom_dist: 0.0,
            zoom_pitch: 0.0,
            zoomed_out: false,
            yaw,
        }
    }

    pub fn orbit(&mut self, side: f64, vertical: f64) {
        let direction = side.signum() * f64::from(side != 0.0);
        if direction != 0.0 && direction != self.side_input {
            self.side_frames = direction * 16.0;
        }
        self.side_input = direction;
        if vertical != 0.0 {
            self.zoomed_out = vertical > 0.0;
        }
    }

    pub fn drag(&mut self, dx: f64, dy: f64) {
        self.offset_yaw = to_s16(self.offset_yaw - dx * 0.0042 * 32768.0 / std::f64::consts::PI) as f64;
        if dy != 0.0 {
            self.zoomed_out = dy > 0.0;
        }
    }

    /// Returns (eye, focus).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f64, mario: DVec3, boss: DVec3, held: bool, yaw: f64, pitch: f64, spin: f64, world: Option<&CollisionWorld>) -> (DVec3, DVec3) {
        self.ticks += dt * 30.0;
        while self.ticks >= 1.0 {
            self.ticks -= 1.0;
            let distance = clamp(mario.distance(boss) * 1.6, 800.0, 5000.0);
            let mut focus = boss;
            if held {
                // offset_rotated flips local Z before applying Mario's yaw.
                let forward = 125.0 * coss(pitch) - 150.0 * sins(pitch);
                focus = DVec3::new(mario.x + forward * sins(yaw), mario.y - 125.0 * sins(pitch) - 150.0 * coss(pitch), mario.z + forward * coss(yaw));
            }
            focus = focus.lerp(mario, 0.5);
            focus.y += 100.0 + if held { 300.0 * sins(spin.abs()) } else { 0.0 };
            self.goal_focus = focus;
            let bearing = self.offset_yaw + 0x2000 as f64;
            let horizontal = distance * coss(0x1000 as f64);
            let mut eye = DVec3::new(focus.x + horizontal * sins(bearing), 0.0, focus.z + horizontal * coss(bearing));
            let center = world.map(|w| w.find_floor_ex(3072.0, CELL_HEIGHT_LIMIT, -4608.0, false, true));
            eye.y = match center.as_ref().and_then(|c| c.surface.as_ref().map(|s| (s, c.height))) {
                Some((plane, _)) => 550.0 - (plane.normal[0] * eye.x + plane.normal[2] * eye.z + plane.origin_offset) / plane.normal[1],
                None => center.map_or(mario.y + 550.0, |c| c.height),
            };

            let side = self.side_frames.signum() * f64::from(self.side_frames != 0.0);
            self.offset_yaw = to_s16(self.offset_yaw + side * 0x200 as f64) as f64;
            self.side_frames -= side;
            let target = if self.zoom_dist > distance || self.zoomed_out { distance } else { 0.0 };
            self.zoom_dist += clamp(target - self.zoom_dist, -30.0, 30.0);
            self.zoom_pitch = clamp(self.zoom_pitch + if self.zoomed_out { 1.0 } else { -1.0 } * (0x1800 as f64 / 13.0).trunc(), 0.0, 0x1800 as f64);
            let zoom_angle = self.zoom_pitch + 0x1000 as f64;
            eye.x += self.zoom_dist * coss(zoom_angle) * sins(bearing);
            eye.y += self.zoom_dist * sins(zoom_angle);
            eye.z += self.zoom_dist * coss(zoom_angle) * coss(bearing);
            self.goal_eye = eye;

            if self.entering {
                self.transition = Some(CameraTransition::new(self.eye, self.focus, focus, 15.0));
                self.entering = false;
            }
            match self.transition.as_mut().filter(|t| t.left > 0.0) {
                Some(t) => {
                    let (e, f) = t.step(eye, focus, world);
                    self.next_eye = e;
                    self.next_focus = f;
                    self.yaw = angle_to_radians(atan2s(e.z - f.z, e.x - f.x) as f64);
                }
                None => {
                    self.next_eye = eye;
                    self.next_focus = focus;
                    self.yaw = angle_to_radians(bearing);
                }
            }
            self.eye = self.eye.lerp(self.next_eye, 0.3);
            self.focus.x += 0.8 * (self.next_focus.x - self.focus.x);
            self.focus.z += 0.8 * (self.next_focus.z - self.focus.z);
            self.focus.y += 0.3 * (self.next_focus.y - self.focus.y);
        }
        let mut out_eye = self.eye;
        if let Some(w) = world {
            let floor = w.find_floor_ex(out_eye.x, out_eye.y + 20.0, out_eye.z, false, true);
            if floor.surface.is_some() {
                out_eye.y = out_eye.y.max(floor.height + 100.0);
            }
        }
        (out_eye, self.focus)
    }
}
