//! Port of `CameraTransition.ts`: `next_lakitu_state` transitions around a new focus in
//! spherical coordinates. All values are SM64 units (f64), angles are s16.
use super::wall::wall_push;
use bevy::math::DVec3;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::math::{atan2s, clamp, coss, sins, to_s16};

#[derive(Clone, Copy, Debug, Default)]
pub struct Polar {
    pub dist: f64,
    pub pitch: f64,
    pub yaw: f64,
}

pub fn polar(from: DVec3, to: DVec3) -> Polar {
    let d = to - from;
    Polar {
        dist: jumpman_sim::math::js_hypot3(d.x, d.y, d.z),
        pitch: atan2s(jumpman_sim::math::js_hypot(d.x, d.z), d.y) as f64,
        yaw: atan2s(d.z, d.x) as f64,
    }
}

pub fn place(origin: DVec3, p: Polar) -> DVec3 {
    DVec3::new(
        origin.x + p.dist * coss(p.pitch) * sins(p.yaw),
        origin.y + p.dist * sins(p.pitch),
        origin.z + p.dist * coss(p.pitch) * coss(p.yaw),
    )
}

fn advance_angle(current: f64, target: f64, step: f64) -> f64 {
    let limit = (to_s16(step.trunc()) as f64).abs();
    to_s16(current + clamp(to_s16(target - current) as f64, -limit, limit)) as f64
}

#[derive(Clone, Debug)]
pub struct CameraTransition {
    eye: Polar,
    focus: Polar,
    /// Frames left.
    pub left: f64,
}

impl CameraTransition {
    pub fn new(old_eye: DVec3, old_focus: DVec3, goal_focus: DVec3, left: f64) -> Self {
        Self { eye: polar(goal_focus, old_eye), focus: polar(goal_focus, old_focus), left }
    }

    /// Returns the transitioned (eye, focus).
    pub fn step(&mut self, goal_eye: DVec3, goal_focus: DVec3, world: Option<&CollisionWorld>) -> (DVec3, DVec3) {
        let left = self.left;
        let goal = polar(goal_focus, goal_eye);
        self.eye.dist += (goal.dist - self.eye.dist) / left;
        self.eye.pitch = advance_angle(self.eye.pitch, goal.pitch, (goal.pitch - self.eye.pitch) / left);
        self.eye.yaw = advance_angle(self.eye.yaw, goal.yaw, (goal.yaw - self.eye.yaw) / left);
        let reverse = polar(goal_eye, goal_focus);
        self.focus.dist -= self.focus.dist / left;
        self.focus.pitch = advance_angle(self.focus.pitch, reverse.pitch, self.focus.pitch / left);
        self.focus.yaw = advance_angle(self.focus.yaw, reverse.yaw, self.focus.yaw / left);
        let mut out_eye = place(goal_focus, self.eye);
        let out_focus = place(goal_focus, self.focus);
        if let Some(world) = world {
            let floor = world.find_floor_ex(out_eye.x, out_eye.y, out_eye.z, false, true);
            if floor.surface.is_some() {
                out_eye.y = out_eye.y.max(floor.height + 125.0);
            }
            out_eye = wall_push(world, out_eye, 0.0, 100.0);
        }
        self.left -= 1.0;
        (out_eye, out_focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_to_goal() {
        let goal_focus = DVec3::new(0.0, 100.0, 0.0);
        let goal_eye = DVec3::new(0.0, 300.0, 800.0);
        let mut t = CameraTransition::new(DVec3::new(500.0, 200.0, -500.0), DVec3::new(100.0, 0.0, 0.0), goal_focus, 15.0);
        let mut eye = DVec3::ZERO;
        while t.left > 0.0 {
            eye = t.step(goal_eye, goal_focus, None).0;
        }
        assert!(eye.distance(goal_eye) < 60.0, "{eye:?}");
    }
}
