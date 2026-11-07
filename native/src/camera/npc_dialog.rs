//! Port of `NpcDialogCamera.ts`: camera.c cutscene_dialog_start, move_mario_shoulder and
//! next_lakitu_state. Not yet wired to a dialog system (see PORTING.md `camera::npc_dialog`).
#![allow(dead_code)]
use super::transition::{polar, CameraTransition};
use bevy::math::DVec3;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::math::{atan2s, coss, sins, to_s16};

pub struct NpcDialogCamera {
    pub saved_eye: DVec3,
    saved_focus: DVec3,
    focus: DVec3,
    pivot: DVec3,
    npc: DVec3,
    yaw: f64,
    transition: Option<CameraTransition>,
    pub frame: u32,
}

impl NpcDialogCamera {
    /// `eye` is the current camera position.
    pub fn new(eye: DVec3, saved_focus: DVec3, mario: [f64; 3], npc: [f64; 3], height: f64) -> Self {
        let pivot = DVec3::new(mario[0], mario[1] + 125.0, mario[2]);
        let npc = DVec3::new(npc[0], npc[1] + height + 200.0, npc[2]);
        let toward = polar(pivot, npc).yaw;
        let yaw = to_s16(toward + if to_s16(polar(pivot, eye).yaw - toward) < 0 { -0x6000 as f64 } else { 0x6000 as f64 }) as f64;
        Self { saved_eye: eye, saved_focus, focus: saved_focus, pivot, npc, yaw, transition: None, frame: 0 }
    }

    /// Returns (eye, focus) given the current eye.
    pub fn step(&mut self, mut eye: DVec3, mario: [f64; 3]) -> (DVec3, DVec3) {
        let mut target = DVec3::from(mario).lerp(self.npc, 0.3);
        target.y += (self.npc.y - target.y) * 0.1;
        self.focus = self.focus.lerp(target, 0.2);
        let (dx, dz) = (eye.x - self.pivot.x, eye.z - self.pivot.z);
        let yaw = atan2s(dz, dx) as f64;
        let next = yaw - (to_s16(yaw - self.yaw) as f64 / 16.0).trunc();
        let dist = 0.95 * dx.hypot(dz) + 9.0;
        eye.x = self.pivot.x + dist * sins(next);
        eye.z = self.pivot.z + dist * coss(next);
        eye.y += (self.pivot.y + 100.0 * sins(polar(self.npc, self.pivot).pitch) - eye.y) * 0.05;
        self.frame += 1;
        (eye, self.focus)
    }

    pub fn begin_return(&mut self, eye: DVec3) {
        self.transition = Some(CameraTransition::new(eye, self.focus, self.saved_focus, 15.0));
    }

    pub fn return_frames(&self) -> f64 {
        self.transition.as_ref().map_or(0.0, |t| t.left)
    }

    pub fn step_return(&mut self, world: &CollisionWorld) -> (DVec3, DVec3) {
        let (e, f) = self.transition.as_mut().expect("begin_return first").step(self.saved_eye, self.saved_focus, Some(world));
        self.focus = f;
        (e, f)
    }
}
