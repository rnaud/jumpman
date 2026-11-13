//! Star Swarm's camera: a third-person orbit camera behind the hero (engine-free helpers).
//!
//! Like the 3D roguelite survivors that inspired the mode, the camera hangs above and behind the hero at a moderate
//! pitch and is free-look: the player turns it with the right stick, a touch drag or the mouse, and the hero moves
//! relative to it (the shell's camera rig supplies the yaw, pitch and zoom from those inputs). This module adds the
//! parts the mode needs on top: a distance that grows a little with the size of the crowd, and a collision pass so that
//! a wall, a cliff face or a hill behind the hero pulls the camera in instead of hiding him.
use jumpman_sim::collision::{CollisionWorld, WallData};

/// Pitch (radians above the horizon) the mode starts with, and the range the player may tilt to.
pub const START_PITCH: f64 = 0.5;
pub const MIN_PITCH: f64 = 0.14;
pub const MAX_PITCH: f64 = 1.15;
/// Distance the mode starts with (the rig's zoom changes it within its own limits).
pub const START_DISTANCE: f64 = 900.0;
/// The closest the collision pass may pull the camera.
pub const MIN_DISTANCE: f64 = 170.0;
/// Crowd size at which the extra distance is complete, and how much it adds.
const CROWD_FULL: f64 = 300.0;
const CROWD_EXTRA: f64 = 260.0;
/// Radius of the probe along the view line, and how far under the ground a sample may sink.
const PROBE_RADIUS: f64 = 55.0;
const GROUND_SLACK: f64 = 30.0;
const SAMPLES: usize = 18;
/// How far from a wall the eye itself keeps (the near plane would otherwise slice through it).
const EYE_RADIUS: f64 = 130.0;

/// Camera distance for the rig's own (zoomed) distance and the number of live foes: a bigger crowd pulls the camera back a little so
/// the fight stays readable.
pub fn distance_for(base: f64, crowd: usize) -> f64 {
    base + CROWD_EXTRA * (crowd as f64 / CROWD_FULL).clamp(0.0, 1.0)
}

/// The eye for a focus, yaw (the camera sits at `+(sin yaw, cos yaw)` of the focus, looking back at it), pitch and distance.
pub fn eye_of(focus: [f64; 3], yaw: f64, pitch: f64, dist: f64) -> [f64; 3] {
    [focus[0] + yaw.sin() * pitch.cos() * dist, focus[1] + pitch.sin() * dist, focus[2] + yaw.cos() * pitch.cos() * dist]
}

fn blocked_at(world: &CollisionWorld, p: [f64; 3], d: [f64; 3]) -> bool {
    // Ground rising into the line.
    let floor = world.find_floor_height(p[0], p[1] + 250.0, p[2]);
    if floor > p[1] + GROUND_SLACK && floor < p[1] + 250.0 {
        return true;
    }
    // A wall whose face points at the camera (the back of a wall is not drawn, so it hides nothing).
    let mut w = WallData { x: p[0], y: p[1], z: p[2], offset_y: 0.0, radius: PROBE_RADIUS, walls: vec![] };
    world.find_wall_collisions(&mut w, true) > 0 && w.walls.iter().any(|s| s.normal[0] * d[0] + s.normal[1] * d[1] + s.normal[2] * d[2] > 0.0)
}

/// How far along the line from `from` (the hero's head) to `to` (the wanted eye) the camera may go: a distance in
/// `[min, |to - from|]`, stopping short of the first wall or rise.
pub fn clear_distance(world: &CollisionWorld, from: [f64; 3], to: [f64; 3], min: f64) -> f64 {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
    for i in 1..=SAMPLES {
        let t = (i as f64 + 1.0) / (SAMPLES as f64 + 1.0);
        let p = [from[0] + d[0] * t, from[1] + d[1] * t, from[2] + d[2] * t];
        if blocked_at(world, p, d) {
            // Back off a little from the obstacle (one sample step, and the probe's radius).
            let hit = t * len;
            return (hit - len / SAMPLES as f64 - PROBE_RADIUS).clamp(min.min(len), len);
        }
    }
    len
}

/// Smooth the camera distance: closing in is quick (the wall must never be seen through), opening out is slow.
pub fn ease_distance(cur: f64, want: f64, dt: f64) -> f64 {
    let rate = if want < cur { 16.0 } else { 2.4 };
    cur + (want - cur) * (1.0 - (-rate * dt).exp())
}

/// Keep the eye clear of walls beside it (the line-of-sight pass only looks along the view line): pushed out sideways like a
/// pillar of radius [`EYE_RADIUS`] would be.
pub fn push_out_of_walls(world: &CollisionWorld, eye: [f64; 3]) -> [f64; 3] {
    let mut w = WallData { x: eye[0], y: eye[1], z: eye[2], offset_y: 0.0, radius: EYE_RADIUS, walls: vec![] };
    world.find_wall_collisions(&mut w, true);
    [w.x, eye[1], w.z]
}

/// Keep an eye above the ground it hangs over.
pub fn lift_above_ground(world: &CollisionWorld, mut eye: [f64; 3], head_y: f64) -> [f64; 3] {
    let floor = world.find_floor_height(eye[0], head_y + 400.0, eye[2]);
    if floor > -1.0e5 && eye[1] < floor + 45.0 {
        eye[1] = floor + 45.0;
    }
    eye
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::SurfaceOptions;

    fn floor(w: &mut CollisionWorld, y: f64) {
        let o = SurfaceOptions::default();
        w.add_triangle([-6000.0, y, -6000.0], [-6000.0, y, 6000.0], [6000.0, y, 6000.0], &o);
        w.add_triangle([-6000.0, y, -6000.0], [6000.0, y, 6000.0], [6000.0, y, -6000.0], &o);
    }

    /// A wall across x at z = `z`, `h` tall, its visible face toward +z (the camera side).
    fn wall(w: &mut CollisionWorld, z: f64, h: f64) {
        let o = SurfaceOptions::default();
        let (a, b, c, d) = ([-900.0, 0.0, z], [900.0, 0.0, z], [900.0, h, z], [-900.0, h, z]);
        w.add_triangle(a, b, c, &o);
        w.add_triangle(a, c, d, &o);
    }

    #[test]
    fn the_crowd_pushes_the_camera_back_a_little_and_no_more() {
        let near = distance_for(START_DISTANCE, 0);
        let mid = distance_for(START_DISTANCE, 150);
        let full = distance_for(START_DISTANCE, 300);
        assert_eq!(near, START_DISTANCE);
        assert!(mid > near && full > mid);
        assert_eq!(distance_for(START_DISTANCE, 5000), full);
        assert!(full - near < 0.4 * START_DISTANCE, "only slightly");
    }

    #[test]
    fn the_start_pose_is_behind_and_above_at_a_moderate_pitch() {
        assert!((0.3..=0.6).contains(&START_PITCH));
        let eye = eye_of([0.0, 100.0, 0.0], 0.0, START_PITCH, START_DISTANCE);
        assert!(eye[2] > 600.0 && eye[1] > 250.0 && eye[0].abs() < 1e-9, "{eye:?}");
        // Yaw swings it round the hero at a constant distance.
        let e2 = eye_of([0.0, 100.0, 0.0], 1.0, START_PITCH, START_DISTANCE);
        let r = |e: [f64; 3]| ((e[0]).powi(2) + (e[1] - 100.0).powi(2) + (e[2]).powi(2)).sqrt();
        assert!((r(eye) - r(e2)).abs() < 1e-6);
    }

    #[test]
    fn open_ground_does_not_move_the_camera() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        let head = [0.0, 120.0, 0.0];
        let eye = eye_of(head, 0.0, START_PITCH, 900.0);
        let d = clear_distance(&w, head, eye, MIN_DISTANCE);
        assert!((d - 900.0).abs() < 1.0, "{d}");
    }

    #[test]
    fn a_wall_behind_the_hero_pulls_the_camera_in_in_front_of_it() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        wall(&mut w, 450.0, 2000.0);
        let head = [0.0, 120.0, 0.0];
        let eye = eye_of(head, 0.0, 0.2, 900.0);
        let d = clear_distance(&w, head, eye, MIN_DISTANCE);
        assert!(d < 450.0 && d >= MIN_DISTANCE, "{d}");
        let e = eye_of(head, 0.0, 0.2, d);
        assert!(e[2] < 450.0, "the eye stays on the hero's side of the wall: {e:?}");
    }

    #[test]
    fn the_back_of_a_wall_hides_nobody() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        let o = SurfaceOptions::default();
        let (a, b, c, d) = ([-900.0, 0.0, 450.0], [900.0, 0.0, 450.0], [900.0, 2000.0, 450.0], [-900.0, 2000.0, 450.0]);
        w.add_triangle(a, c, b, &o);
        w.add_triangle(a, d, c, &o);
        let head = [0.0, 120.0, 0.0];
        let eye = eye_of(head, 0.0, 0.2, 900.0);
        assert!((clear_distance(&w, head, eye, MIN_DISTANCE) - 900.0).abs() < 1.0);
    }

    #[test]
    fn a_hill_rising_behind_the_hero_pulls_the_camera_in() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        let o = SurfaceOptions::default();
        // A ramp climbing from the floor at z = 400 to 700 high at z = 1400, then a plateau.
        w.add_triangle([-6000.0, 0.0, 400.0], [-6000.0, 700.0, 1400.0], [6000.0, 700.0, 1400.0], &o);
        w.add_triangle([-6000.0, 0.0, 400.0], [6000.0, 700.0, 1400.0], [6000.0, 0.0, 400.0], &o);
        w.add_triangle([-6000.0, 700.0, 1400.0], [-6000.0, 700.0, 6000.0], [6000.0, 700.0, 6000.0], &o);
        w.add_triangle([-6000.0, 700.0, 1400.0], [6000.0, 700.0, 6000.0], [6000.0, 700.0, 1400.0], &o);
        let head = [0.0, 120.0, 0.0];
        let eye = eye_of(head, 0.0, 0.25, 1300.0);
        let d = clear_distance(&w, head, eye, MIN_DISTANCE);
        assert!(d < 1300.0, "{d}");
    }

    #[test]
    fn easing_closes_in_fast_and_opens_slowly() {
        let dt = 1.0 / 60.0;
        let closing = ease_distance(900.0, 300.0, dt);
        let opening = ease_distance(300.0, 900.0, dt);
        assert!(900.0 - closing > (opening - 300.0) * 3.0);
        let mut d = 300.0;
        for _ in 0..600 {
            d = ease_distance(d, 900.0, dt);
        }
        assert!((d - 900.0).abs() < 1.0);
    }

    #[test]
    fn an_eye_beside_a_wall_is_pushed_off_it() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        // A wall across z = 450 facing -z (toward the hero): an eye at z = 400 is 50 from it.
        let o = SurfaceOptions::default();
        let (a, b, c, d) = ([-900.0, 0.0, 450.0], [900.0, 0.0, 450.0], [900.0, 2000.0, 450.0], [-900.0, 2000.0, 450.0]);
        w.add_triangle(a, c, b, &o);
        w.add_triangle(a, d, c, &o);
        let e = push_out_of_walls(&w, [0.0, 300.0, 400.0]);
        assert!(e[2] < 400.0 - 40.0 && (e[1] - 300.0).abs() < 1e-9, "{e:?}");
        let clear = push_out_of_walls(&w, [0.0, 300.0, 0.0]);
        assert_eq!(clear, [0.0, 300.0, 0.0]);
    }

    #[test]
    fn a_low_eye_is_lifted_over_the_ground() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 300.0);
        let e = lift_above_ground(&w, [10.0, 250.0, 10.0], 400.0);
        assert!(e[1] >= 345.0, "{e:?}");
    }
}
