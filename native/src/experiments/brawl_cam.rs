//! Gem Clash camera helpers (engine-free): the low, side-on pitch and the check that keeps real-course walls, slopes and props
//! from hiding the fighters.
//!
//! The brawl is framed from a shallow angle (about 30 degrees below the horizon, like the arcade original) instead of looking down
//! on the arena. A low camera sees more of the course between itself and the fighters, so every frame the view from each fighter
//! to the eye is walked through the course's own collision: if a wall facing the camera or a rise of the ground is in the way,
//! the camera lifts (a steeper pitch, tried in steps) until the line is clear. Walls whose visible face points away from the
//! camera (the rim of a walled plaza seen from outside) are not obstructions: their back faces are not drawn.
use jumpman_sim::collision::{CollisionWorld, WallData};

/// Camera pitch (radians below the horizon). Phones get a touch more so the near fighters do not hide the far ones in the narrow frame.
pub fn brawl_pitch(phone: bool, gearworks: bool) -> f32 {
    (if phone { 0.56 } else { 0.5 }) + if gearworks { 0.1 } else { 0.0 }
}

/// The extra pitches tried, in order, when the base one is blocked (the last is nearly top-down).
pub const LIFT_STEPS: [f32; 5] = [0.0, 0.14, 0.28, 0.44, 0.62];

/// Radius of the probe at each sample (a wall within this of the line counts).
const PROBE_RADIUS: f64 = 35.0;
/// How far under the ground surface a sample may sink before the ground counts as in the way.
const GROUND_SLACK: f64 = 45.0;
/// Samples along one view line.
const SAMPLES: usize = 28;

/// Is the line from `from` (a fighter's chest) to `to` (the eye), both in course coordinates, blocked by the course? Skips the first
/// few percent (the fighter's own surroundings).
pub fn view_blocked(world: &CollisionWorld, from: [f64; 3], to: [f64; 3]) -> bool {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    for i in 1..=SAMPLES {
        let t = 0.06 + 0.94 * i as f64 / SAMPLES as f64;
        let p = [from[0] + d[0] * t, from[1] + d[1] * t, from[2] + d[2] * t];
        // Ground rising into the line: the highest floor within reach above the sample is higher than the sample.
        let floor = world.find_floor_height(p[0], p[1] + 250.0, p[2]);
        if floor > p[1] + GROUND_SLACK && floor < p[1] + 250.0 {
            return true;
        }
        let mut w = WallData { x: p[0], y: p[1], z: p[2], offset_y: 0.0, radius: PROBE_RADIUS, walls: vec![] };
        if world.find_wall_collisions(&mut w, true) > 0 && w.walls.iter().any(|s| s.normal[0] * d[0] + s.normal[1] * d[1] + s.normal[2] * d[2] > 0.0) {
            return true;
        }
    }
    false
}

/// The lowest of the `base + LIFT_STEPS` pitches whose eye (focus minus the forward vector times `dist`) sees every point in
/// `targets` (course coordinates) without the course in the way; the steepest one when none does.
pub fn clear_pitch(world: &CollisionWorld, base: f32, yaw: f32, focus: [f64; 3], dist: f32, targets: &[[f64; 3]]) -> f32 {
    let mut pitch = base;
    for lift in LIFT_STEPS {
        pitch = (base + lift).min(1.25);
        let eye = eye_of(focus, yaw, pitch, dist);
        if !targets.iter().any(|t| view_blocked(world, *t, eye)) {
            return pitch;
        }
    }
    pitch
}

/// The eye for a focus, yaw (0 looks down -z), pitch and distance, in the same coordinates as `focus`.
pub fn eye_of(focus: [f64; 3], yaw: f32, pitch: f32, dist: f32) -> [f64; 3] {
    let (yaw, pitch, dist) = (yaw as f64, pitch as f64, dist as f64);
    // forward = (-sin yaw cos pitch, -sin pitch, -cos yaw cos pitch); eye = focus - forward * dist
    [focus[0] + yaw.sin() * pitch.cos() * dist, focus[1] + pitch.sin() * dist, focus[2] + yaw.cos() * pitch.cos() * dist]
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::SurfaceOptions;

    fn floor(w: &mut CollisionWorld, y: f64) {
        let o = SurfaceOptions::default();
        w.add_triangle([-5000.0, y, -5000.0], [-5000.0, y, 5000.0], [5000.0, y, 5000.0], &o);
        w.add_triangle([-5000.0, y, -5000.0], [5000.0, y, 5000.0], [5000.0, y, -5000.0], &o);
    }

    /// A wall across x at z = `z`, `h` tall, its visible face toward +z (the camera side) when `faces_camera`.
    fn wall(w: &mut CollisionWorld, z: f64, h: f64, faces_camera: bool) {
        let o = SurfaceOptions::default();
        let (a, b, c, d) = ([-800.0, 0.0, z], [800.0, 0.0, z], [800.0, h, z], [-800.0, h, z]);
        if faces_camera {
            w.add_triangle(a, b, c, &o);
            w.add_triangle(a, c, d, &o);
        } else {
            w.add_triangle(a, c, b, &o);
            w.add_triangle(a, d, c, &o);
        }
    }

    #[test]
    fn the_base_pitch_is_shallow_and_phones_get_a_little_more() {
        let desk = brawl_pitch(false, false);
        assert!((0.4..=0.6).contains(&desk), "desktop pitch {desk}");
        assert!(brawl_pitch(true, false) > desk && brawl_pitch(true, false) <= 0.65);
        assert!(brawl_pitch(false, true) > desk);
    }

    #[test]
    fn open_ground_never_blocks_the_view() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        let eye = eye_of([0.0, 40.0, 0.0], 0.0, 0.5, 2500.0);
        assert!(!view_blocked(&w, [0.0, 90.0, 0.0], eye));
        assert_eq!(clear_pitch(&w, 0.5, 0.0, [0.0, 40.0, 0.0], 2500.0, &[[0.0, 90.0, 0.0]]), 0.5);
    }

    #[test]
    fn a_wall_between_the_fighter_and_the_camera_lifts_the_camera_until_it_clears() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        // 700 tall, 800 in front of the fighter on the camera's side.
        wall(&mut w, 800.0, 700.0, true);
        let target = [[0.0, 90.0, 0.0]];
        let low = eye_of([0.0, 40.0, 0.0], 0.0, 0.5, 2500.0);
        assert!(view_blocked(&w, target[0], low), "the low line runs into the wall");
        let p = clear_pitch(&w, 0.5, 0.0, [0.0, 40.0, 0.0], 2500.0, &target);
        assert!(p > 0.5, "lifted: {p}");
        assert!(!view_blocked(&w, target[0], eye_of([0.0, 40.0, 0.0], 0.0, p, 2500.0)));
    }

    #[test]
    fn the_back_of_a_wall_does_not_hide_anybody() {
        // The rim of a walled plaza seen from outside: its drawn face points at the fighters, away from the camera.
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        wall(&mut w, 800.0, 700.0, false);
        let eye = eye_of([0.0, 40.0, 0.0], 0.0, 0.5, 2500.0);
        assert!(!view_blocked(&w, [0.0, 90.0, 0.0], eye));
    }

    #[test]
    fn ground_rising_toward_the_camera_counts() {
        let mut w = CollisionWorld::default();
        floor(&mut w, 0.0);
        // A plateau 650 high that starts 700 in front of the fighter.
        let o = SurfaceOptions::default();
        w.add_triangle([-5000.0, 650.0, 700.0], [-5000.0, 650.0, 5000.0], [5000.0, 650.0, 5000.0], &o);
        w.add_triangle([-5000.0, 650.0, 700.0], [5000.0, 650.0, 5000.0], [5000.0, 650.0, 700.0], &o);
        assert!(view_blocked(&w, [0.0, 90.0, 0.0], eye_of([0.0, 40.0, 0.0], 0.0, 0.5, 2500.0)));
    }

    #[test]
    fn the_eye_sits_behind_and_above_the_focus() {
        let e = eye_of([100.0, 40.0, -200.0], 0.0, 0.5, 1000.0);
        assert!(e[2] > -200.0 && e[1] > 40.0 && (e[0] - 100.0).abs() < 1e-9);
        let high = eye_of([0.0, 0.0, 0.0], 0.0, 1.0, 1000.0);
        assert!(high[1] > e[1] - 40.0);
    }
}
