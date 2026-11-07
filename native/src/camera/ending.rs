//! Port of the cameras in `castleEnding.ts`: `EndingCamera` (the Peach cutscene shots, US timing of
//! `sCutsceneEnding`) and `GrandStarCamera` (`cutscene_grand_star` / `_fly` in camera.c).
use bevy::math::{DQuat, DVec3, EulerRot};

type Path = &'static [[f64; 4]];

const FLY_TO_WINDOW_POS: Path = &[
    [0., -86., 876., 640.], [0., -86., 876., 610.], [0., -66., 945., 393.],
    [0., -80., 976., 272.], [0., -66., 1306., -36.], [0., -70., 1869., -149.],
    [0., -10., 2093., -146.], [0., -10., 2530., -248.], [0., -10., 2530., -263.],
    [0., -10., 2530., -273.],
];
const FLY_TO_WINDOW_FOCUS: Path = &[
    [50., -33., 889., -7.], [35., -33., 889., -7.], [31., -17., 1070., -193.],
    [25., -65., 1182., -272.], [20., -64., 1559., -542.], [25., -68., 2029., -677.],
    [25., -9., 2204., -673.], [25., -8., 2529., -772.], [0., -8., 2529., -772.],
    [0., -8., 2529., -772.], [0., -8., 2529., -772.],
];
const PEACH_DESCENT_CAM_POS: Path = &[
    [50., 1., 120., -1150.], [50., 1., 120., -1150.], [40., 118., 121., -1199.],
    [40., 147., 74., -1306.], [40., 162., 95., -1416.], [40., 25., 111., -1555.],
    [40., -188., 154., -1439.], [40., -203., 181., -1242.], [40., 7., 191., -1057.],
    [40., 262., 273., -1326.], [40., -4., 272., -1627.], [35., -331., 206., -1287.],
    [30., -65., 219., -877.], [25., 6., 216., -569.], [25., -8., 157., 40.],
    [25., -4., 106., 200.], [25., -6., 72., 574.], [0., -6., 72., 574.],
    [0., -6., 72., 574.], [0., -6., 72., 574.],
];
const MARIO_TO_PEACH_POS: Path = &[
    [0., -130., 1111., -1815.], [0., -131., 1052., -1820.], [0., -271., 1008., -1651.],
    [0., -439., 1043., -1398.], [0., -433., 1040., -1120.], [0., -417., 1040., -1076.],
    [0., -417., 1040., -1076.], [0., -417., 1040., -1076.], [0., -417., 1040., -1076.],
];
const MARIO_TO_PEACH_FOCUS: Path = &[
    [50., -37., 1020., -1332.], [20., -36., 1012., -1330.], [20., -24., 1006., -1215.],
    [20., 28., 1002., -1224.], [24., 45., 1013., -1262.], [35., 34., 1000., -1287.],
    [0., 34., 1000., -1287.], [0., 34., 1000., -1287.], [0., 34., 1000., -1287.],
];
const LOOK_UP_AT_CASTLE: Path = &[
    [50., 200., 1066., -1414.], [50., 200., 1066., -1414.], [30., 198., 1078., -1412.],
    [33., 15., 1231., -1474.], [39., -94., 1381., -1368.], [0., -92., 1374., -1379.],
    [0., -92., 1374., -1379.], [0., -92., 1374., -1379.],
];
const LOOK_AT_SKY_FOCUS: Path = &[
    [50., 484., 1368., -888.], [72., 479., 1372., -892.], [50., 351., 1817., -918.],
    [50., 351., 1922., -598.], [0., 636., 2027., -415.], [0., 636., 2027., -415.],
    [0., 636., 2027., -415.],
];

#[derive(Default)]
pub struct EndingCamera {
    frame: u32,
    segment: usize,
    fraction: f64,
    pub eye: DVec3,
    pub focus: DVec3,
}

impl EndingCamera {
    pub fn new(eye: DVec3) -> Self {
        Self { eye, ..Default::default() }
    }

    pub fn reset(&mut self) {
        self.frame = 0;
        self.segment = 0;
        self.fraction = 0.0;
    }

    /// `move_point_along_spline`: the position and focus paths share a segment and clock.
    fn point(&mut self, path: Path) -> DVec3 {
        let s = self.segment.min(path.len() - 4);
        let (t, u) = (self.fraction, 1.0 - self.fraction);
        let w = [u * u * u / 6.0, t * t * t / 2.0 - t * t + 2.0 / 3.0, -t * t * t / 2.0 + t * t / 2.0 + t / 2.0 + 1.0 / 6.0, t * t * t / 6.0];
        let mut out = DVec3::ZERO;
        for i in 0..4 {
            let p = path[s + i];
            out += DVec3::new(p[1], p[2], p[3]) * w[i];
        }
        let (a, b) = (path[s + 1][0], path[s + 2][0]);
        self.fraction += if a != 0.0 { u / a } else { 0.0 } + if b != 0.0 { t / b } else { 0.0 };
        if self.fraction >= 1.0 {
            self.segment += 1;
            self.fraction -= 1.0;
        }
        out
    }

    /// One 30 Hz step; returns (eye, focus).
    pub fn step(&mut self, mario: DVec3, peach_y: f64) -> (DVec3, DVec3) {
        let t = self.frame;
        self.frame += 1;
        if [315, 840, 1430, 1525, 1950, 2186, 2431].contains(&t) {
            self.segment = 0;
            self.fraction = 0.0;
        }
        if t < 170 {
            self.eye = DVec3::new(165., 4725., 324.);
            self.focus = DVec3::new(mario.x, mario.y + 80.0, mario.z + 100f64.max((mario.y - self.eye.y).abs() * 0.1));
        } else if t < 240 {
            self.eye = DVec3::new(-86., 876., 790.);
            self.focus = DVec3::new(-33., 969., -7.);
        } else if t < 315 {
            self.eye = DVec3::new(-51., 988., -202.);
            self.focus = DVec3::new(85., 826., 250.);
        } else if t < 701 {
            self.eye = self.point(FLY_TO_WINDOW_POS);
            self.focus = self.point(FLY_TO_WINDOW_FOCUS);
        } else if t < 840 {
            self.eye = DVec3::new(179., 2463., -1216.);
            self.focus = DVec3::new(0., peach_y + 125.0, -1300.);
        } else if t < 1430 {
            self.eye = self.point(PEACH_DESCENT_CAM_POS);
            self.eye.y += peach_y;
            self.focus = DVec3::new(0., peach_y + 125.0, -1300.);
        } else if t < 1525 {
            self.eye = self.point(MARIO_TO_PEACH_POS);
            self.focus = self.point(MARIO_TO_PEACH_FOCUS);
        } else if t < 1950 {
            self.eye = DVec3::new(-163., 978., -1082.);
            if t == 1525 || t >= 1775 {
                self.focus = self.point(LOOK_UP_AT_CASTLE);
            }
        } else if t < 2186 {
            self.eye = DVec3::new(-473., 970., -1152.);
            self.focus = DVec3::new(11., 983., -1273.);
        } else if t < 2431 {
            if t < 2341 {
                self.eye = DVec3::new(-149., 1021., -1216.);
                self.focus = DVec3::new(350., 1034., -1216.);
            } else {
                self.eye = self.eye.lerp(DVec3::new(-250., 966., -1111.), 0.05);
                self.focus = self.focus.lerp(DVec3::new(233., 1068., -1298.), 0.05);
            }
        } else {
            self.eye = DVec3::new(699., 1680., -703.);
            if t == 2431 || t >= 2931 {
                self.focus = self.point(LOOK_AT_SKY_FOCUS);
            }
        }
        (self.eye, self.focus)
    }
}

fn wrap_pi(a: f64) -> f64 {
    a.sin().atan2(a.cos())
}

/// `cutscene_grand_star` and `cutscene_grand_star_fly`.
#[derive(Default)]
pub struct GrandStarCamera {
    frame: u32,
    pub eye: DVec3,
    pub focus: DVec3,
    mv: DVec3,
    offset: DVec3,
    focus_offset: DVec3,
    speed: f64,
}

impl GrandStarCamera {
    pub fn new(eye: DVec3) -> Self {
        Self { eye, ..Default::default() }
    }

    /// `face_*` in s16 units. Returns (eye, focus).
    pub fn step(&mut self, pos: DVec3, face_pitch: f64, face_yaw: f64) -> (DVec3, DVec3) {
        let t = self.frame;
        self.frame += 1;
        let p = &mut self.eye;
        if t < 360 {
            if t == 0 {
                self.focus = pos;
            }
            if t < 110 {
                let diff = *p - pos;
                let dist = diff.length();
                let yaw = diff.x.atan2(diff.z);
                let target_yaw = -std::f64::consts::PI + 0x1200 as f64 * std::f64::consts::PI / 32768.0;
                let dyaw = wrap_pi(target_yaw - yaw);
                let goal_y = 457.0 - pos.y;
                let angle = diff.y.atan2(jumpman_sim::math::js_hypot(diff.x, diff.z));
                let pitch = angle + (goal_y.atan2(600.0) - angle) / 32.0;
                let d = dist + (jumpman_sim::math::js_hypot(goal_y, 600.0) - dist) * 0.1;
                *p = DVec3::new(
                    pos.x + d * pitch.cos() * (yaw + dyaw / 20.0).sin(),
                    pos.y + d * pitch.sin(),
                    pos.z + d * pitch.cos() * (yaw + dyaw / 20.0).cos(),
                );
            } else {
                if t < 160 {
                    self.mv.x = 5.0;
                    self.mv.z = (-40f64).max(self.mv.z - 2.0);
                } else {
                    self.mv.x = 0.0;
                    self.mv.z = 0f64.min(self.mv.z + 2.0);
                    p.x += (pos.x - p.x) * 0.01;
                    p.z += (pos.z - p.z) * 0.01;
                }
                p.x -= self.mv.x;
                p.z += self.mv.z;
            }
            self.focus.x += (pos.x - self.focus.x) * 0.5;
            self.focus.y += ((pos.y - 307.0) * 0.5 + 407.0 - self.focus.y) * 0.8;
            self.focus.z += (pos.z - self.focus.z) * 0.5;
        } else {
            if t == 360 {
                self.focus_offset.y = self.focus.y - pos.y;
            }
            if t <= 500 {
                self.offset = self.offset.lerp(DVec3::new(-600., 0., -400.), 0.05);
                self.focus_offset.y += (-self.focus_offset.y).clamp(-2.0, 2.0);
                self.focus_offset.z = (-200f64).max(self.focus_offset.z - 6.0);
            } else {
                let v = 15f64.min((t as f64 - 500.0) * 0.1);
                self.offset.x = (-2000f64).max(self.offset.x - v);
                self.offset.y = 1200f64.min(self.offset.y + v / 10.0);
                self.offset.z = 1000f64.min(self.offset.z + v / 10.0);
                self.focus_offset.y = 1200f64.min(self.focus_offset.y + v / 2.0);
                self.focus_offset.z = 1000f64.min(self.focus_offset.z + v / 1.5);
            }
            let rot = DQuat::from_euler(EulerRot::YXZ, face_yaw * std::f64::consts::PI / 32768.0, -face_pitch * std::f64::consts::PI / 32768.0, 0.0);
            let rotate = |v: DVec3| rot * DVec3::new(v.x, v.y, -v.z) + pos;
            let goal = rotate(self.offset);
            let focus = rotate(self.focus_offset);
            self.speed = 90f64.min(self.speed + 2.5);
            let moved = *p + (goal - *p).clamp_length_max(self.speed);
            *p = moved.lerp(goal, 0.01);
            self.focus.x += (focus.x - self.focus.x) * 0.5;
            self.focus.y += (focus.y - self.focus.y) * 0.8;
            self.focus.z += (focus.z - self.focus.z) * 0.5;
        }
        (self.eye, self.focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ending_camera_shots_follow_the_timeline() {
        let mut c = EndingCamera::new(DVec3::ZERO);
        let mario = DVec3::new(0., 3000., 0.);
        let (eye, _) = c.step(mario, 2428.0);
        assert_eq!(eye, DVec3::new(165., 4725., 324.));
        for _ in 1..200 {
            c.step(mario, 2428.0);
        }
        assert_eq!(c.eye, DVec3::new(-86., 876., 790.));
        for _ in 200..320 {
            c.step(mario, 2428.0);
        }
        assert!(c.eye.x.abs() < 200.0 && c.eye.y > 800.0);
        for _ in 320..3000 {
            let (e, f) = c.step(mario, 906.0);
            assert!(e.is_finite() && f.is_finite());
        }
    }

    #[test]
    fn grand_star_camera_stays_finite_and_keeps_its_distance() {
        let mut c = GrandStarCamera::new(DVec3::new(0., 500., 400.));
        let pos = DVec3::new(0., 300., 0.);
        for _ in 0..700 {
            let (e, f) = c.step(pos, 0.0, 0.0);
            assert!(e.is_finite() && f.is_finite());
        }
        assert!(c.eye.distance(pos) > 300.0);
    }
}
