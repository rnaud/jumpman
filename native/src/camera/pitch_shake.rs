//! Port of `CameraPitchShake.ts`: SHAKE_POS_BOWLING_BALL / cannon raise from camera.c.
use bevy::math::DVec3;
use jumpman_sim::math::{angle_to_radians, atan2s, sins, to_s16};

#[derive(Default, Debug)]
pub struct CameraPitchShake {
    magnitude: f64,
    phase: f64,
    pub radians: f64,
}

impl CameraPitchShake {
    /// `camera_pos`/`view` are the current eye and view direction (sim units).
    pub fn request_bowling_ball(&mut self, camera_pos: DVec3, view: DVec3, pos: [f64; 3]) {
        let (dx, dy, dz) = (pos[0] - camera_pos.x, pos[1] - camera_pos.y, pos[2] - camera_pos.z);
        if dx * dx + dy * dy + dz * dz >= 2000.0 * 2000.0 {
            return;
        }
        let pitch = to_s16(atan2s(jumpman_sim::math::js_hypot(dx, dz), dy) as f64 - atan2s(jumpman_sim::math::js_hypot(view.x, view.z), view.y) as f64);
        let yaw = to_s16(atan2s(dz, dx) as f64 - atan2s(view.z, view.x) as f64);
        let off_axis = pitch < -0x1800 || pitch > 0x400 || yaw.abs() > 0x1800;
        self.magnitude = self.magnitude.max(if off_axis { 0x14 as f64 } else { 0x28 as f64 });
    }

    pub fn request_cannon_raise(&mut self) {
        self.magnitude = self.magnitude.max(0x60 as f64);
    }

    /// One 30 Hz tick.
    pub fn step(&mut self) {
        self.radians = angle_to_radians(self.magnitude * sins(self.phase));
        self.magnitude = (self.magnitude - 8.0).max(0.0);
        self.phase = if self.magnitude != 0.0 { to_s16(self.phase + 0x4000 as f64) as f64 } else { 0.0 };
    }
}
