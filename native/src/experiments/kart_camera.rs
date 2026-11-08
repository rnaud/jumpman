//! The Mario Kart 64 one-player chase camera, engine-free. `src/game/experiments/kartCamera.ts` is the browser
//! twin; both are checked by the same numeric tests.
//!
//! Port of n64decomp/mk64 `src/camera.c`: `camera_init` (1P offsets), `func_8001E45C` (heading follow and drift
//! swing), `func_8001CCEC` (position and look-at smoothing, mushroom push, wall-bump kick) and `func_8001F394` /
//! `func_80014EE4` (field of view). Course units and s16 angles, stepped at the game's 60 Hz (twice per 30 Hz tick).
//!
//! * eye = kart + (0, 9.5, -50) turned by the camera heading, target = kart + (0, 0, 70)
//! * eye follows at 0.4 (xz) / 0.15 (y) per frame, the target at 0.4 / 0.5
//! * the heading chases the kart's rotation by 165..500 s16 per frame, swings 12 degrees to the inside of a drift
//!   and holds while the kart spins out
//! * vertical field of view 40 degrees (+8 mushroom, +18 spinout), 1 degree per frame
//!
//! Not ported: the camera's own wall/ground push-out, the Lakitu view, split-screen and the intro fly-bys.
use super::kart_physics::Kart;

pub const CAMERA_HEIGHT: f64 = 9.5;
pub const CAMERA_BACK: f64 = -50.;
pub const CAMERA_AHEAD: f64 = 70.;
pub const FOLLOW_XZ: f64 = 0.4;
pub const FOLLOW_EYE_Y: f64 = 0.15;
pub const FOLLOW_LOOK_Y: f64 = 0.5;
pub const BASE_FOV: f64 = 40.;
pub const MUSHROOM_FOV: f64 = 8.;
pub const SPINOUT_FOV: f64 = 18.;
/// The port's kart origin sits this far below the decomp's `player->pos` (`BOUNDING_BOX`).
pub const ORIGIN_LIFT: f64 = 5.5;
const DEG: i32 = 182;

fn s16(v: f64) -> i32 {
    (v as i32) as i16 as i32
}
fn s16i(v: i32) -> i32 {
    v as i16 as i32
}
/// `move_f32_towards` (snaps tiny values to zero).
fn towards(value: f64, target: f64, rate: f64) -> f64 {
    let v = value - (value - target) * rate;
    if v.abs() < 0.001 { 0. } else { v }
}
/// `move_s16_towards`.
fn towards_s16(value: i32, target: i32, rate: f64) -> i32 {
    s16(value as f64 - (value - target) as f64 * rate)
}

/// `adjust_angle`: move `angle` towards `target` by at most `step` s16 units.
pub fn adjust_angle(angle: i32, target: i32, step: i32) -> i32 {
    let diff = s16i(target - angle);
    let s = step.abs();
    if diff > 0 {
        let left = diff - s;
        if left >= 0 { s16i(target - left) } else { target }
    } else {
        let left = diff + s;
        if left <= 0 { s16i(target - left) } else { target }
    }
}

/// What the camera needs to know about the kart each 60 Hz frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraKart {
    /// Course units (the port's origin; `ORIGIN_LIFT` is added here).
    pub pos: [f64; 3],
    pub rotation: i32,
    pub drifting: bool,
    pub drift_outside: bool,
    /// `unk_078`: signed turning rate.
    pub turn: i32,
    pub kmh: f64,
    pub hop: bool,
    /// Hit a wall since the last camera step.
    pub bumped: bool,
    pub mushroom: bool,
    pub spinout: bool,
}

impl CameraKart {
    pub fn of(k: &Kart, mushroom: bool, spinout: bool, bumped: bool) -> Self {
        Self {
            pos: k.pos,
            rotation: k.rotation,
            drifting: k.drifting,
            drift_outside: k.drift_outside,
            turn: k.unk_078,
            kmh: k.kmh(),
            hop: k.hop,
            bumped,
            mushroom,
            spinout,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraPose {
    /// Eye and look-at in course units.
    pub eye: [f64; 3],
    pub look: [f64; 3],
    /// Vertical field of view, degrees.
    pub fov: f64,
}

impl CameraPose {
    pub fn lerp(&self, other: &Self, t: f64) -> Self {
        let mix = |a: [f64; 3], b: [f64; 3]| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        Self { eye: mix(self.eye, other.eye), look: mix(self.look, other.look), fov: self.fov + (other.fov - self.fov) * t }
    }
}

/// Forward of an s16 rotation in the port's frame: `(-sin r, cos r)`.
fn forward_of(rotation: i32) -> [f64; 2] {
    let a = -(rotation as f64) * std::f64::consts::PI / 32768.;
    [a.sin(), a.cos()]
}

#[derive(Clone, Debug, Default)]
pub struct KartCamera {
    pub eye: [f64; 3],
    pub look: [f64; 3],
    pub fov: f64,
    /// `unk_2C`: the camera's own heading (s16).
    pub heading: i32,
    /// `unk_B0`: drift swing (s16).
    pub swing: i32,
    push_t: f64,
    push_amp: f64,
    kick_t: f64,
    kick_amp: f64,
    fov_add: f64,
    was_mushroom: bool,
    ready: bool,
}

impl KartCamera {
    pub fn new() -> Self {
        Self { fov: BASE_FOV, ..Self::default() }
    }

    /// Put the camera straight behind the kart (`camera_init`).
    pub fn snap(&mut self, k: &CameraKart) {
        self.heading = k.rotation;
        self.swing = 0;
        self.push_t = 0.;
        self.push_amp = 0.;
        self.kick_t = 0.;
        self.kick_amp = 0.;
        self.fov_add = 0.;
        self.fov = BASE_FOV;
        self.was_mushroom = k.mushroom;
        let [fx, fz] = forward_of(self.heading);
        let y = k.pos[1] + ORIGIN_LIFT;
        self.eye = [k.pos[0] + fx * CAMERA_BACK, y + CAMERA_HEIGHT, k.pos[2] + fz * CAMERA_BACK];
        self.look = [k.pos[0] + fx * CAMERA_AHEAD, y, k.pos[2] + fz * CAMERA_AHEAD];
        self.ready = true;
    }

    pub fn initialised(&self) -> bool {
        self.ready
    }

    /// One 60 Hz camera update (`func_8001E45C` -> `func_8001CCEC`).
    pub fn step(&mut self, k: &CameraKart) {
        if !self.ready {
            return self.snap(k);
        }
        let step;
        if k.drifting {
            if k.turn == 0 {
                step = 100;
                self.swing = 0;
            } else if k.turn < 0 {
                step = 0xa5 - k.turn / 2;
                self.swing = towards_s16(self.swing, if k.drift_outside { -0xb60 } else { -12 * DEG }, 0.1);
            } else {
                step = k.turn / 2 + 0xa5;
                self.swing = towards_s16(self.swing, if k.drift_outside { 0xb60 } else { 12 * DEG }, 0.1);
            }
        } else {
            self.swing = towards_s16(self.swing, 0, 0.05);
            let diff = s16i(self.heading) / DEG - s16i(k.rotation) / DEG;
            step = if k.turn == 0 {
                0x1f4
            } else if k.turn < 0 {
                if diff <= -70 || diff >= 70 { 0xb4 - k.turn } else { 0xa5 - k.turn / 2 }
            } else if diff <= -70 || diff >= 0x46 {
                k.turn + 0xb4
            } else {
                k.turn / 2 + 0xa5
            };
        }
        if !k.spinout {
            self.heading = adjust_angle(self.heading, s16i(k.rotation + self.swing), step);
        }

        if k.mushroom && !self.was_mushroom {
            self.push_t = 0.;
            self.push_amp = 8.;
        }
        self.was_mushroom = k.mushroom;
        if k.bumped && k.kmh > 30. {
            self.kick_t = 0.;
            self.kick_amp = 3.;
        }
        let mut t = self.push_t + 1.;
        let mut amp = self.push_amp;
        let mut push = amp * t - 0.7 * t * t;
        if push < 0. {
            t = 0.;
            amp *= 0.8;
            if amp <= 0.1 {
                amp = 0.;
            }
            push = 0.;
        }
        self.push_t = t;
        self.push_amp = amp;
        let mut kt = self.kick_t + 1.;
        let mut kamp = self.kick_amp;
        let mut kick = kt * kamp - 1.25 * kt * kt;
        if kick < 0. {
            kt = 0.;
            kamp *= 0.9;
            if kamp <= 0.1 {
                kamp = 0.;
            }
            kick = 0.;
        }
        self.kick_t = kt;
        self.kick_amp = kamp;

        let [fx, fz] = forward_of(self.heading);
        let lift = k.pos[1] + ORIGIN_LIFT;
        let eye_back = CAMERA_BACK + push;
        let look_ahead = CAMERA_AHEAD + push;
        let eye_y = lift + CAMERA_HEIGHT + kick / 2.;
        let look_y = lift + kick;
        let slow = k.kmh <= 5. && k.hop;
        self.look[0] += (k.pos[0] + fx * look_ahead - self.look[0]) * FOLLOW_XZ;
        self.look[2] += (k.pos[2] + fz * look_ahead - self.look[2]) * FOLLOW_XZ;
        self.look[1] += (look_y - self.look[1]) * if slow { 0.02 } else { FOLLOW_LOOK_Y };
        self.eye[0] += (k.pos[0] + fx * eye_back - self.eye[0]) * FOLLOW_XZ;
        self.eye[2] += (k.pos[2] + fz * eye_back - self.eye[2]) * FOLLOW_XZ;
        self.eye[1] += (eye_y - self.eye[1]) * if slow { 0.01 } else { FOLLOW_EYE_Y };

        if k.spinout {
            self.fov_add = towards(self.fov_add, SPINOUT_FOV, 0.2);
        } else if k.mushroom {
            self.fov_add = towards(self.fov_add, MUSHROOM_FOV, 0.2);
        } else {
            self.fov_add = if self.fov_add > 1. { self.fov_add - 2. } else { 0. };
        }
        let target = BASE_FOV + self.fov_add;
        if self.fov < target {
            self.fov = target.min(self.fov + 1.);
        } else if self.fov > target {
            self.fov = target.max(self.fov - 1.);
        }
    }

    pub fn pose(&self) -> CameraPose {
        CameraPose { eye: self.eye, look: self.look, fov: self.fov }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kart(pos: [f64; 3], rotation: i32) -> CameraKart {
        CameraKart { pos, rotation, ..default_kart() }
    }
    fn default_kart() -> CameraKart {
        CameraKart { kmh: 100., ..Default::default() }
    }
    /// Distance of the eye behind a kart heading along `forward_of(rotation)`.
    fn behind(c: &KartCamera, k: &CameraKart) -> f64 {
        let f = forward_of(k.rotation);
        -((c.eye[0] - k.pos[0]) * f[0] + (c.eye[2] - k.pos[2]) * f[1])
    }

    #[test]
    fn adjust_angle_takes_the_short_way_and_never_overshoots() {
        assert_eq!(adjust_angle(0, 1000, 300), 300);
        assert_eq!(adjust_angle(0, 100, 300), 100);
        assert_eq!(adjust_angle(0, -1000, 300), -300);
        // Across the s16 wrap: 32000 -> -32000 is a +1536 step, not -64000.
        assert_eq!(adjust_angle(32000, -32000, 500), s16i(32000 + 500));
        assert_eq!(adjust_angle(7, 7, 100), 7);
    }

    #[test]
    fn straight_driving_settles_fifty_behind_and_nine_and_a_half_up() {
        let mut c = KartCamera::new();
        let mut k = kart([100., 0., 200.], 0);
        c.snap(&k);
        for i in 0..120 {
            k.pos[2] += 5.;
            c.step(&k);
            if i == 119 {
                assert!((behind(&c, &k) - 50.).abs() < 5. * 0.6 / 0.4 + 0.5, "behind {}", behind(&c, &k));
            }
        }
        assert!((c.eye[1] - (ORIGIN_LIFT + CAMERA_HEIGHT)).abs() < 1e-6);
        assert!((c.look[1] - ORIGIN_LIFT).abs() < 1e-6);
        // Looking 70 ahead of the kart (less the lag of a moving kart).
        let ahead = (c.look[2] - k.pos[2]).abs();
        assert!(ahead > 60. && ahead < 72., "ahead {ahead}");
        assert!((c.fov - 40.).abs() < 1e-9);
    }

    #[test]
    fn parked_kart_camera_is_exactly_at_the_decomp_offsets() {
        let mut c = KartCamera::new();
        let k = kart([0., 0., 0.], 0);
        c.snap(&k);
        for _ in 0..60 {
            c.step(&k);
        }
        assert!((behind(&c, &k) - 50.).abs() < 1e-6);
        let f = forward_of(0);
        assert!((c.look[0] * f[0] + c.look[2] * f[1] - 70.).abs() < 1e-6);
    }

    #[test]
    fn heading_lags_a_turn_and_catches_up() {
        let mut c = KartCamera::new();
        let mut k = kart([0., 0., 0.], 0);
        c.snap(&k);
        k.rotation = 8000;
        k.turn = 30;
        c.step(&k);
        assert!(c.heading > 0 && c.heading < 8000, "lags: {}", c.heading);
        for _ in 0..120 {
            c.step(&k);
        }
        assert_eq!(c.heading, 8000, "catches up");
        // The follow speed grows with the turn rate.
        let mut slow = KartCamera::new();
        let mut hard = KartCamera::new();
        let mut a = kart([0., 0., 0.], 0);
        slow.snap(&a);
        hard.snap(&a);
        a.rotation = 20000;
        a.turn = 10;
        slow.step(&a);
        a.turn = 120;
        hard.step(&a);
        assert!(hard.heading > slow.heading);
    }

    #[test]
    fn drifting_swings_the_camera_towards_the_inside() {
        let mut c = KartCamera::new();
        let mut k = kart([0., 0., 0.], 0);
        c.snap(&k);
        k.drifting = true;
        k.turn = 60;
        for _ in 0..60 {
            c.step(&k);
        }
        assert!((c.swing - 12 * DEG).abs() < 30, "swing {}", c.swing);
        assert!(c.heading > 0, "heading follows rotation + swing");
        k.turn = -60;
        for _ in 0..60 {
            c.step(&k);
        }
        assert!((c.swing + 12 * DEG).abs() < 30);
        k.drifting = false;
        for _ in 0..200 {
            c.step(&k);
        }
        assert!(c.swing.abs() < 5, "swing relaxes: {}", c.swing);
    }

    #[test]
    fn spinout_freezes_the_heading_and_widens_the_view() {
        let mut c = KartCamera::new();
        let mut k = kart([0., 0., 0.], 0);
        c.snap(&k);
        k.spinout = true;
        k.rotation = 12000;
        k.turn = 50;
        for _ in 0..30 {
            c.step(&k);
        }
        assert_eq!(c.heading, 0);
        assert!((c.fov - 58.).abs() < 0.05, "fov {}", c.fov);
        k.spinout = false;
        for _ in 0..40 {
            c.step(&k);
        }
        assert!((c.fov - 40.).abs() < 0.05, "fov relaxes: {}", c.fov);
    }

    #[test]
    fn mushroom_opens_the_view_one_degree_a_frame_and_pushes_the_camera_in() {
        let mut c = KartCamera::new();
        let mut k = kart([0., 0., 0.], 0);
        c.snap(&k);
        k.mushroom = true;
        c.step(&k);
        assert!((c.fov - 41.).abs() < 1e-9);
        let mut closest = f64::MAX;
        for _ in 0..40 {
            c.step(&k);
            closest = closest.min(behind(&c, &k));
        }
        assert!((c.fov - 48.).abs() < 0.05);
        assert!(closest < 45., "pushed in by the mushroom arc: {closest}");
    }

    #[test]
    fn wall_bump_above_30_kmh_kicks_the_camera_up_once() {
        let mut c = KartCamera::new();
        let mut k = kart([0., 0., 0.], 0);
        c.snap(&k);
        k.bumped = true;
        c.step(&k);
        k.bumped = false;
        let mut peak = f64::MIN;
        for _ in 0..12 {
            c.step(&k);
            peak = peak.max(c.look[1]);
        }
        assert!(peak > ORIGIN_LIFT + 0.5, "look-at lifted: {peak}");
        for _ in 0..200 {
            c.step(&k);
        }
        assert!((c.look[1] - ORIGIN_LIFT).abs() < 1e-3, "settles");
        // Too slow: nothing happens.
        let mut slow = KartCamera::new();
        let mut s = kart([0., 0., 0.], 0);
        s.kmh = 10.;
        slow.snap(&s);
        s.bumped = true;
        slow.step(&s);
        assert!((slow.look[1] - ORIGIN_LIFT).abs() < 1e-9);
    }

    #[test]
    fn pose_lerp_blends_between_ticks() {
        let a = CameraPose { eye: [0., 0., 0.], look: [10., 0., 0.], fov: 40. };
        let b = CameraPose { eye: [2., 4., 6.], look: [10., 0., 8.], fov: 48. };
        let m = a.lerp(&b, 0.5);
        assert_eq!(m.eye, [1., 2., 3.]);
        assert_eq!(m.look, [10., 0., 4.]);
        assert_eq!(m.fov, 44.);
    }
}
