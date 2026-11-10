//! The OoT Z-target camera, ported from `z_camera.c` (engine-free, so the maths can be tested).
//!
//! OoT has three camera functions for a Z-held Link and picks one from what he targets:
//!
//! * [`ZKind::Battle`] = `Camera_Battle1` (`CAM_MODE_Z_TARGET_UNFRIENDLY`): an opponent.
//! * [`ZKind::Keep`] = `Camera_KeepOn1` (`CAM_MODE_Z_TARGET_FRIENDLY`): somebody to talk to.
//! * [`ZKind::Parallel`] = `Camera_Parallel1` (`CAM_MODE_Z_PARALLEL`): Z held with nothing to target.
//!
//! They are not a fixed placement behind Link. Each is a little state machine that runs once per
//! game frame (20 Hz in OoT, [`TICK`] here, interpolated for display):
//!
//! 1. `animTimer`: for the first ticks the eye swings from where the previous camera was to the
//!    far side of Link from the target (`Camera_Battle1`: `CAM_DEFAULT_ANIM_TIME` + `CAM_GLOBAL_24`).
//! 2. Afterwards the camera only turns when the target leaves a cone of `swingAngle` degrees around
//!    the view direction (10..80 degrees, wider the nearer the target); inside the cone it creeps
//!    towards an over-the-shoulder view that keeps the target `swingAngle` off centre.
//! 3. `at` is not Link's head but a point between Link and the target (`Camera_CalcAtForLockOn`,
//!    15..75 percent of the way), eased with the `atLERPStepScale` ramp (0.12, x1.1 per tick, up
//!    to the setting's cap).
//! 4. Pitch is the setting's swing pitch minus the head-to-target elevation, so a high target
//!    lowers the camera, eased at 0.1 per tick; the distance eases at 0.06 per tick.
//!
//! Distances in `z_camera_data.inc.c` are OoT units; Link is drawn at [`UNIT`] sim units per OoT unit.
//! The numbers are those of `CAM_SET_NORMAL0` (open areas, which Mario worlds use too) and
//! `CAM_SET_DUNGEON0` (dungeons). Not ported: the charged spin attack zoom, the roll
//! (`CAM_BATTLE1_ROLL_TARGET_BASE`, at most 6 degrees while running), the sword-swing fov squeeze
//! (x0.8) and the slope adjustment (`R_CAM_PARALLEL_LOCKON_CALC_SLOPE_Y_ADJ` is 0 in retail).
use bevy::math::DVec3;
use std::f64::consts::{PI, TAU};

/// Sim units per OoT unit (Link: 68 OoT units tall, drawn `link_height` = 68 x 4.4).
pub const UNIT: f64 = 4.4;
/// OoT runs its camera once per game frame, 20 per second.
pub const TICK: f64 = 1.0 / 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZKind {
    Battle,
    Keep,
    Parallel,
}

/// One setting's numbers (`CAM_FUNCDATA_*`), OoT units and degrees.
#[derive(Clone, Copy, Debug)]
pub struct ZParams {
    /// Fraction of Link's height added to the look-at height (negative: below the head).
    pub y_offset: f64,
    pub y_offset_off_ground: f64,
    /// Battle: `distance`; Keep: `unk_04` (minimum distance, also the "close" reference); Parallel: `distTarget`.
    pub distance: f64,
    /// Keep only: the largest distance.
    pub distance_max: f64,
    pub swing_yaw_initial: f64,
    pub swing_yaw_final: f64,
    pub swing_pitch_initial: f64,
    pub swing_pitch_final: f64,
    pub swing_pitch_adj: f64,
    /// Parallel: the fixed pitch.
    pub pitch: f64,
    pub fov: f64,
    pub at_lerp_ground: f64,
    pub at_lerp_air: f64,
}

pub fn params(kind: ZKind, dungeon: bool) -> ZParams {
    match kind {
        // sSetNormal0ModeZTargetUnfriendlyData / sSetDungeon0ModeZTargetUnfriendlyData
        ZKind::Battle => ZParams {
            y_offset: -0.20,
            y_offset_off_ground: -0.40,
            distance: 180.0,
            distance_max: 180.0,
            swing_yaw_initial: 10.0,
            swing_yaw_final: 80.0,
            swing_pitch_initial: 0.0,
            swing_pitch_final: 10.0,
            swing_pitch_adj: 0.25,
            pitch: 0.0,
            fov: if dungeon { 45.0 } else { 50.0 },
            at_lerp_ground: 0.8,
            at_lerp_air: 0.25,
        },
        // sSetNormal0ModeZTargetFriendlyData
        ZKind::Keep => ZParams {
            y_offset: -0.20,
            y_offset_off_ground: -0.50,
            distance: 120.0,
            distance_max: 140.0,
            swing_yaw_initial: 25.0,
            swing_yaw_final: 45.0,
            swing_pitch_initial: -5.0,
            swing_pitch_final: 15.0,
            swing_pitch_adj: 0.15,
            pitch: 0.0,
            fov: 45.0,
            at_lerp_ground: 0.5,
            at_lerp_air: 0.30,
        },
        // sSetNormal0ModeZParallelData (distTarget 250) / sSetDungeon0ModeZParallelData (150)
        ZKind::Parallel => ZParams {
            y_offset: -0.20,
            y_offset_off_ground: -0.40,
            distance: if dungeon { 150.0 } else { 250.0 },
            distance_max: 0.0,
            swing_yaw_initial: 0.0,
            swing_yaw_final: 0.0,
            swing_pitch_initial: 0.0,
            swing_pitch_final: 0.0,
            swing_pitch_adj: 0.0,
            pitch: 0.0,
            fov: 45.0,
            at_lerp_ground: 0.5,
            at_lerp_air: 0.20,
        },
    }
}

/// What the camera needs of the world for one tick.
#[derive(Clone, Copy, Debug)]
pub struct ZIn {
    /// Link's feet (sim units), his facing (radians, atan2(x, z) convention) and height (sim units).
    pub link: DVec3,
    pub link_yaw: f64,
    pub height: f64,
    /// Floor height under Link, and whether he stands on it (`playerGroundY == pos.y`).
    pub ground_y: f64,
    pub on_ground: bool,
    /// Link's speed over his top speed (`camera->speedRatio`), 0..1.
    pub speed_ratio: f64,
    /// The locked target's focus point and identity (a change restarts the swing), `None` for Parallel.
    pub target: Option<(DVec3, u64)>,
    pub friendly: bool,
    pub dungeon: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ZSample {
    pub eye: DVec3,
    pub at: DVec3,
    /// Degrees.
    pub fov: f64,
}

/// Ray probe: `(origin, unit direction, max length)` -> distance to the first wall.
pub type Probe<'a> = &'a dyn Fn(DVec3, DVec3, f64) -> Option<f64>;

pub fn wrap(a: f64) -> f64 {
    let a = a.rem_euclid(TAU);
    if a > PI {
        a - TAU
    } else {
        a
    }
}

/// `OLib_Vec3fDiffToVecGeo`: (radius, yaw, pitch) of `v`; yaw = atan2(x, z), pitch = elevation.
pub fn geo(v: DVec3) -> (f64, f64, f64) {
    let xz = v.x.hypot(v.z);
    (v.length(), v.x.atan2(v.z), v.y.atan2(xz))
}

pub fn from_geo(r: f64, yaw: f64, pitch: f64) -> DVec3 {
    DVec3::new(r * pitch.cos() * yaw.sin(), r * pitch.sin(), r * pitch.cos() * yaw.cos())
}

/// `Camera_LERPCeilF`: ease towards `target`, snapping inside `min_diff`.
fn lerp_ceil(target: f64, cur: f64, rate: f64, min_diff: f64) -> f64 {
    if (target - cur).abs() >= min_diff {
        cur + (target - cur) * rate
    } else {
        target
    }
}

/// `Camera_LERPCeilS` on radians (`min_diff` in radians).
fn lerp_ceil_angle(target: f64, cur: f64, rate: f64, min_diff: f64) -> f64 {
    let d = wrap(target - cur);
    if d.abs() >= min_diff {
        cur + d * rate
    } else {
        target
    }
}

#[derive(Clone, Debug)]
pub struct ZLock {
    kind: Option<ZKind>,
    target_id: Option<u64>,
    /// Link-relative look-at offset (`playerToAtOffset`), the look-at point, the eye before collision
    /// (`eyeNext`) and the displayed eye (`eye`).
    play_to_at: DVec3,
    at: DVec3,
    eye_next: DVec3,
    eye: DVec3,
    dist: f64,
    y_pos_offset: f64,
    at_lerp: f64,
    xz_rate: f64,
    y_rate: f64,
    fov: f64,
    anim_timer: i32,
    init_yaw: f64,
    init_pitch: f64,
    init_r: f64,
    para_yaw: f64,
    acc: f64,
    fresh: bool,
    prev: ZSample,
    cur: ZSample,
}

impl ZLock {
    /// Start from the camera that was showing (`Camera_Copy`-like): `eye`/`at` are its pose, `link` where Link is.
    pub fn engage(eye: DVec3, at: DVec3, link: DVec3, fov_deg: f64) -> Self {
        let s = ZSample { eye, at, fov: fov_deg };
        Self {
            kind: None,
            target_id: None,
            play_to_at: at - link,
            at,
            eye_next: eye,
            eye,
            dist: eye.distance(at),
            y_pos_offset: link.y,
            at_lerp: 0.1,
            xz_rate: 0.05,
            y_rate: 0.05,
            fov: fov_deg,
            anim_timer: 0,
            init_yaw: 0.0,
            init_pitch: 0.0,
            init_r: 0.0,
            para_yaw: 0.0,
            acc: 0.0,
            fresh: true,
            prev: s,
            cur: s,
        }
    }

    pub fn kind(&self) -> Option<ZKind> {
        self.kind
    }

    /// Advance by `dt` seconds of display time (whole 20 Hz ticks are run, the sample blends the last two).
    pub fn advance(&mut self, dt: f64, i: &ZIn, probe: Probe) -> ZSample {
        self.acc += dt;
        let mut guard = 0;
        while self.acc >= TICK && guard < 6 {
            self.acc -= TICK;
            guard += 1;
            self.prev = self.cur;
            self.tick(i, probe);
            self.cur = ZSample { eye: self.eye, at: self.at, fov: self.fov };
        }
        if guard == 6 {
            self.acc = 0.0;
        }
        let k = (self.acc / TICK).clamp(0.0, 1.0);
        ZSample { eye: self.prev.eye.lerp(self.cur.eye, k), at: self.prev.at.lerp(self.cur.at, k), fov: self.prev.fov + (self.cur.fov - self.prev.fov) * k }
    }

    /// The latest sample without advancing (e.g. while fading out).
    pub fn sample(&self) -> ZSample {
        let k = (self.acc / TICK).clamp(0.0, 1.0);
        ZSample { eye: self.prev.eye.lerp(self.cur.eye, k), at: self.prev.at.lerp(self.cur.at, k), fov: self.prev.fov + (self.cur.fov - self.prev.fov) * k }
    }

    fn restart(&mut self, kind: ZKind, id: Option<u64>, link: DVec3) {
        self.kind = Some(kind);
        self.target_id = id;
        let (r, yaw, pitch) = geo(self.eye - self.at);
        self.init_r = r;
        self.init_yaw = yaw;
        self.init_pitch = pitch;
        self.y_pos_offset = link.y;
        // Battle1/KeepOn1: CAM_DEFAULT_ANIM_TIME + CAM_GLOBAL_24 = 5; Parallel1: CAM_DEFAULT_ANIM_TIME = 4.
        self.anim_timer = if kind == ZKind::Parallel { 4 } else { 5 };
    }

    fn tick(&mut self, i: &ZIn, probe: Probe) {
        let kind = match i.target {
            Some(_) if i.friendly => ZKind::Keep,
            Some(_) => ZKind::Battle,
            None => ZKind::Parallel,
        };
        let id = i.target.map(|t| t.1);
        if Some(kind) != self.kind || id != self.target_id {
            self.restart(kind, id, i.link);
        }
        let p = params(kind, i.dungeon);
        let h = i.height;
        // `yNormal`: the offsets are percentages of Link's height, corrected for how far from 68 it is.
        let y_norm = 0.9 + 0.1 * (68.0 * UNIT / h);
        let y_offset = p.y_offset * h * y_norm;
        let y_offset_air = p.y_offset_off_ground * h * y_norm;
        if std::mem::take(&mut self.fresh) {
            // The camera we took over from (`Camera_Normal1`) already holds its look-at about Link's chest.
            self.play_to_at = DVec3::new(0.0, h + y_offset, 0.0);
        }
        let sr = i.speed_ratio;
        // Rates that only move while Link does (`* camera->speedRatio`).
        let to_at_eye_next = geo(self.eye_next - self.at);
        let to_at_eye = geo(self.eye - self.at);
        match (kind, i.target) {
            (ZKind::Parallel, _) => self.parallel(i, &p, y_offset, y_offset_air, sr, to_at_eye, to_at_eye_next),
            (_, Some((t, _))) => self.lock_on(kind, i, &p, t, y_offset, y_offset_air, sr, to_at_eye, to_at_eye_next),
            _ => {}
        }
        // Collision: the eye is pulled in front of walls, instantly in and eased out.
        let clipped = self.clip(i, probe);
        self.eye = clipped;
    }

    /// `Camera_CalcAtForLockOn` / `Camera_CalcAtForParallel`: the look-at point.
    #[allow(clippy::too_many_arguments)]
    fn calc_at(&mut self, i: &ZIn, target: Option<DVec3>, y_off: f64, distance: f64, off_ground_flag: bool) {
        let h = i.height;
        let mut to_at = DVec3::new(0.0, h + y_off, 0.0);
        if let Some(t) = target {
            let head = i.link + DVec3::Y * h;
            let (r, yaw, pitch) = geo(t - head);
            let mut look_r = if distance < r {
                r * 0.15
            } else {
                let off = ((i.link.y - i.ground_y) / h).clamp(0.0, 1.0);
                let v = r * 0.75 - r * (0.75 - 0.15) * (r / distance);
                v - v * off * off
            };
            if off_ground_flag {
                look_r *= 0.2;
                self.xz_rate = 0.01;
                self.y_rate = 0.01;
            }
            to_at += from_geo(look_r, yaw, pitch);
        }
        if i.on_ground {
            self.y_pos_offset = lerp_ceil(i.link.y, self.y_pos_offset, 0.4, 0.1);
            to_at.y -= i.link.y - self.y_pos_offset;
            self.play_to_at = lerp_vec(to_at, self.play_to_at, self.y_rate, self.xz_rate);
        } else {
            let mut delta = i.link.y - self.y_pos_offset;
            if !off_ground_flag {
                // The vertical offset may not leave the view (fov * 0.4 either way).
                let dxz = (self.at.x - self.eye.x).hypot(self.at.z - self.eye.z);
                let lim = (self.fov * 0.4).to_radians().tan() * dxz;
                if lim < delta {
                    self.y_pos_offset += delta - lim;
                    delta = lim;
                } else if delta < -lim {
                    self.y_pos_offset += delta + lim;
                    delta = -lim;
                }
                to_at.y -= delta;
            } else {
                let dxz = (self.at.x - self.eye.x).hypot(self.at.z - self.eye.z);
                let a = delta.atan2(dxz);
                let phi = if a > 20f64.to_radians() {
                    1.0 - (a - 20f64.to_radians()).sin()
                } else if a < (-10f64).to_radians() {
                    1.0 - ((-10f64).to_radians() - a).sin()
                } else {
                    1.0
                };
                to_at.y -= delta * phi;
            }
            self.play_to_at = lerp_vec(to_at, self.play_to_at, 0.5, 0.5);
            self.y_rate = 0.5;
            self.xz_rate = 0.5;
        }
        let goal = i.link + self.play_to_at;
        self.at += (goal - self.at) * self.at_lerp.clamp(0.0, 1.0);
    }

    /// `Camera_ClampLERPScale`.
    fn ramp_at_lerp(&mut self, cap: f64) {
        self.at_lerp = if self.at_lerp < 0.12 {
            0.12
        } else if self.at_lerp >= cap {
            cap
        } else {
            1.1 * self.at_lerp
        };
    }

    #[allow(clippy::too_many_arguments)]
    fn lock_on(&mut self, kind: ZKind, i: &ZIn, p: &ZParams, target: DVec3, y_offset: f64, y_offset_air: f64, speed_ratio: f64, to_at_eye: (f64, f64, f64), to_at_eye_next: (f64, f64, f64)) {
        let h = i.height;
        let keep = kind == ZKind::Keep;
        let off_ground = !i.on_ground;
        // Offset rates: Battle1 tends to 0.6 (x speedRatio), KeepOn1 to 1.0.
        let rate_goal = if keep { 1.0 } else { 0.6 };
        self.xz_rate = lerp_ceil(rate_goal, self.xz_rate, 0.5 * speed_ratio, 0.1);
        self.y_rate = lerp_ceil(rate_goal, self.y_rate, 0.2 * speed_ratio, 0.1);

        // Distance (battle: fixed; keep: the current one held inside [min, max]).
        let (distance, near_ref) = if keep {
            let (min, max) = (p.distance * UNIT, p.distance_max * UNIT);
            (to_at_eye_next.0.clamp(min, max), min)
        } else {
            (p.distance * UNIT, p.distance * UNIT)
        };
        // Link's pitch reference: head at height + yOffset.
        self.calc_at(i, Some(target), if off_ground { y_offset_air } else { y_offset }, near_ref, off_ground);
        let at_for_yaw = self.at;
        let head = i.link + DVec3::Y * (h + y_offset);
        let mut p2t = geo(target - head);
        let dist_ratio = if p2t.0 > near_ref { 1.0 } else { p2t.0 / near_ref };
        let (a_r, a_yaw, a_pitch) = geo(target - at_for_yaw);
        let at_to_target_r = distance - a_r.min(distance) * 0.5;
        let swing = p.swing_yaw_initial + (p.swing_yaw_final - p.swing_yaw_initial) * (1.1 - dist_ratio);
        let swing_edge = swing + 10.0;

        // distance eases at CAM_GLOBAL_11 = 0.06 per tick.
        self.dist = lerp_ceil(distance, self.dist, 0.06, 2.0 * UNIT);
        let mut r = self.dist;
        let mut yaw = to_at_eye_next.1;
        let look_err = wrap(a_yaw - (to_at_eye_next.1 - PI));
        let swing_rad = swing.to_radians();
        if self.anim_timer != 0 {
            // Swing from where the last camera was to the far side of Link from the target.
            let sp86 = (self.anim_timer - 1) as f64;
            let behind_yaw = geo(head - target).1; // = head-to-target yaw - pi
            let cr = to_at_eye.0;
            let step = 1.0 / 4.0;
            let r_goal = cr + (self.init_r - cr) * step * sp86;
            let yaw_goal = behind_yaw + wrap(self.init_yaw - behind_yaw) * step * sp86;
            r = lerp_ceil(r_goal, to_at_eye.0, 0.5, 1.0);
            yaw = lerp_ceil_angle(yaw_goal, to_at_eye.1, 0.5, 10.0 * TAU / 65536.0);
            p2t = geo(self.eye - self.at);
            self.anim_timer -= 1;
        } else if look_err.abs() > swing_rad {
            // The target left the cone: swing towards it, faster the further out it is.
            let err_deg = look_err.to_degrees();
            let reach = swing + (swing_edge - swing) * (at_to_target_r.min(r) / r);
            let k = (reach * reach - 2.0) / (reach - 360.0);
            let denom = k * err_deg + (2.0 - 360.0 * k);
            let turn = (err_deg * err_deg / denom).to_radians();
            yaw = to_at_eye_next.1 + look_err.signum() * turn;
        } else {
            let creep = (1.0 - speed_ratio) * if keep { 0.02 } else { 0.05 };
            let want = look_err.signum() * swing_rad;
            yaw = to_at_eye_next.1 - (want - look_err) * creep;
        }

        let pitch_deg = p.swing_pitch_initial + (p.swing_pitch_final - p.swing_pitch_initial) * dist_ratio;
        let mut pitch = pitch_deg.to_radians() - p2t.2 * (0.5 + dist_ratio * 0.5) + a_pitch * p.swing_pitch_adj;
        let lim = if keep { 70f64 } else { 60f64 }.to_radians();
        pitch = pitch.clamp(-lim, lim);
        let pitch = lerp_ceil_angle(pitch, to_at_eye_next.2, 0.1, 10.0 * TAU / 65536.0);
        self.eye_next = self.at + from_geo(r, yaw, pitch);
        self.dist = r;
        self.ramp_at_lerp(if off_ground { p.at_lerp_air } else { p.at_lerp_ground });
        // fov: Camera_Battle1 narrows slightly with the target's nearness.
        let fov_goal = if keep { p.fov } else { p.fov - p.fov * 0.05 * dist_ratio };
        self.fov = lerp_ceil(fov_goal, self.fov, 0.1, 0.5);
    }

    #[allow(clippy::too_many_arguments)]
    fn parallel(&mut self, i: &ZIn, p: &ZParams, y_offset: f64, y_offset_air: f64, speed_ratio: f64, to_at_eye: (f64, f64, f64), to_at_eye_next: (f64, f64, f64)) {
        // Rates tend to their resting values at speed-scaled steps, as in Camera_Parallel1.
        self.xz_rate = lerp_ceil(0.05, self.xz_rate, 0.5 * speed_ratio, 0.1);
        self.y_rate = lerp_ceil(0.05, self.y_rate, 0.2 * speed_ratio, 0.1);
        if self.anim_timer != 0 {
            // `PARALLEL1_FLAG_1`: the target yaw is "behind Link" and follows him during the swing.
            self.para_yaw = i.link_yaw + PI;
        }
        let off_ground = !i.on_ground;
        let y_off = if off_ground { y_offset_air } else { y_offset };
        let (xz, y) = (self.xz_rate, self.y_rate);
        self.calc_at(i, None, y_off, 1.0, false);
        self.xz_rate = xz;
        self.y_rate = y;
        let (r, yaw, pitch);
        if self.anim_timer != 0 {
            let t = self.anim_timer as f64;
            let n = (t + 1.0) * t / 2.0;
            yaw = to_at_eye.1 + wrap(self.para_yaw - to_at_eye.1) / n * t;
            pitch = to_at_eye.2;
            r = to_at_eye.0;
            self.anim_timer -= 1;
        } else {
            self.dist = lerp_ceil(p.distance * 0.01 * i.height * y_norm_of(i.height), self.dist, 1.0 / 20.0, 2.0 * UNIT);
            r = self.dist;
            yaw = lerp_ceil_angle(self.para_yaw, to_at_eye_next.1, 0.8, 10.0 * TAU / 65536.0);
            let want = p.pitch.to_radians();
            pitch = lerp_ceil_angle(want, to_at_eye_next.2, 0.5, 4.0 * TAU / 65536.0).clamp(-30f64.to_radians(), 79f64.to_radians());
        }
        self.eye_next = self.at + from_geo(r, yaw, pitch);
        self.dist = r;
        self.ramp_at_lerp(if off_ground { p.at_lerp_air } else { p.at_lerp_ground });
        self.fov = lerp_ceil(p.fov, self.fov, 0.1, 0.5);
    }

    /// `Camera_BGCheckInfo` stand-in: keep the eye out of walls between `at` and `eye_next`; when a wall
    /// would leave the camera inside Link's lap the camera slides round him to the side with most room.
    fn clip(&mut self, i: &ZIn, probe: Probe) -> DVec3 {
        let at = self.at;
        let want = self.eye_next;
        let clear = |eye: DVec3| -> f64 {
            let d = eye - at;
            let len = d.length();
            if len < 1e-6 {
                return 0.0;
            }
            match probe(at, d / len, len) {
                Some(hit) => (hit - 0.12 * i.height).max(0.0),
                None => len,
            }
        };
        let want_len0 = (want - at).length();
        let c = clear(want);
        let mut best = (c, want);
        let enough = (1.4 * i.height).min(want_len0 * 0.9);
        if c < enough {
            let (r, yaw, pitch) = geo(want - at);
            for (dy, dp) in [(15.0, 0.0), (-15.0, 0.0), (30.0, 8.0), (-30.0, 8.0), (50.0, 15.0), (-50.0, 15.0), (75.0, 20.0), (-75.0, 20.0), (0.0, 25.0)] {
                let cand = at + from_geo(r, yaw + f64::to_radians(dy), pitch + f64::to_radians(dp));
                let cc = clear(cand);
                if cc > best.0 {
                    best = (cc, cand);
                    if cc >= enough {
                        break;
                    }
                }
            }
        }
        let (room, cand) = best;
        let dir = (cand - at).normalize_or_zero();
        let want_len = (cand - at).length();
        let new_len = room.min(want_len);
        // Instantly in, eased out: no popping through walls, no lurch back out when one is passed.
        let prev_len = (self.eye - at).length();
        let len = if new_len > prev_len && room >= want_len { prev_len + (new_len - prev_len) * 0.35 } else { new_len };
        at + dir * len
    }
}

fn y_norm_of(h: f64) -> f64 {
    0.9 + 0.1 * (68.0 * UNIT / h)
}

fn lerp_vec(target: DVec3, cur: DVec3, y_rate: f64, xz_rate: f64) -> DVec3 {
    DVec3::new(lerp_ceil(target.x, cur.x, xz_rate, 0.1), lerp_ceil(target.y, cur.y, y_rate, 0.1), lerp_ceil(target.z, cur.z, xz_rate, 0.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(link: DVec3, yaw: f64, target: Option<DVec3>) -> ZIn {
        ZIn { link, link_yaw: yaw, height: 68.0 * UNIT, ground_y: link.y, on_ground: true, speed_ratio: 0.0, target: target.map(|t| (t, 1)), friendly: false, dungeon: false }
    }

    fn no_wall(_: DVec3, _: DVec3, _: f64) -> Option<f64> {
        None
    }

    fn run(cam: &mut ZLock, i: &ZIn, secs: f64, probe: Probe) -> ZSample {
        let mut s = cam.sample();
        for _ in 0..(secs / (TICK / 2.0)) as usize {
            s = cam.advance(TICK / 2.0, i, probe);
        }
        s
    }

    /// Angle off the view axis of `p`, seen from `eye` looking at `at`.
    fn off_axis(eye: DVec3, at: DVec3, p: DVec3) -> f64 {
        let f = (at - eye).normalize();
        let v = (p - eye).normalize();
        f.dot(v).clamp(-1.0, 1.0).acos()
    }

    fn start(link: DVec3, from_yaw: f64) -> ZLock {
        let at = link + DVec3::Y * 250.0;
        let eye = at + from_geo(1100.0, from_yaw, 0.35);
        ZLock::engage(eye, at, link, 50.0)
    }

    #[test]
    fn battle_frames_link_and_the_target_for_all_distances_and_bearings() {
        let link = DVec3::ZERO;
        let h = 68.0 * UNIT;
        for &dist in &[250.0, 600.0, 1100.0, 1700.0] {
            for k in 0..12 {
                let bearing = k as f64 * TAU / 12.0;
                let target = from_geo(dist, bearing, 0.0) + DVec3::Y * 150.0;
                let i = input(link, bearing, Some(target));
                // The previous camera hung round the other side of Link, the hardest case.
                let mut cam = start(link, bearing + 1.9);
                let s = run(&mut cam, &i, 5.0, &no_wall);
                let half_fov = (s.fov * 0.5).to_radians();
                let head = link + DVec3::Y * h * 0.9;
                // `Battle1` keeps both inside the view, with margin.
                assert!(off_axis(s.eye, s.at, head) < half_fov * 1.05, "link out of view d={dist} b={bearing:.2}: {:.1} deg", off_axis(s.eye, s.at, head).to_degrees());
                assert!(off_axis(s.eye, s.at, target) < half_fov * 1.6, "target out of view d={dist} b={bearing:.2}: {:.1} deg of {:.1}", off_axis(s.eye, s.at, target).to_degrees(), half_fov.to_degrees());
                // ...and the camera sits on Link's side, further from the target than he is.
                assert!(s.eye.distance(target) > link.distance(target), "d={dist} b={bearing:.2}");
            }
        }
    }

    #[test]
    fn battle_settles_at_the_setting_distance_and_swings_to_the_shoulder() {
        let link = DVec3::ZERO;
        let target = DVec3::new(0.0, 100.0, 900.0);
        let i = input(link, 0.0, Some(target));
        let mut cam = start(link, 0.0);
        let s = run(&mut cam, &i, 6.0, &no_wall);
        let d = s.eye.distance(s.at);
        assert!((d - 180.0 * UNIT).abs() < 25.0, "Camera_Battle1 distance 180: {d}");
        // Not dead behind: the target creeps to `swingAngle` off the centre of the view.
        let to_target = geo(target - s.at).1;
        let view = geo(s.at - s.eye).1;
        let off = wrap(to_target - view).abs().to_degrees();
        assert!(off > 8.0 && off < 40.0, "{off}");
    }

    #[test]
    fn transition_is_continuous_and_takes_a_few_ticks() {
        let link = DVec3::ZERO;
        let target = DVec3::new(500.0, 100.0, 700.0);
        let i = input(link, 0.0, Some(target));
        let mut cam = start(link, 2.5);
        let first = cam.sample();
        let mut prev = first;
        let mut worst = 0.0f64;
        for n in 0..120 {
            let s = cam.advance(1.0 / 60.0, &i, &no_wall);
            worst = worst.max(s.eye.distance(prev.eye));
            prev = s;
            if n == 60 {
                // After a second the camera is already on Link's far side from the target.
                let to_t = geo(target - link).1;
                let eye_yaw = geo(s.eye - link).1;
                assert!(wrap(eye_yaw - (to_t - PI)).abs() < 1.2, "{}", wrap(eye_yaw - (to_t - PI)));
            }
        }
        assert!(worst < 450.0, "no teleport between frames: {worst}");
        assert!(first.eye.distance(prev.eye) > 100.0, "the camera did move");
    }

    #[test]
    fn parallel_swings_behind_link_once_then_holds() {
        let link = DVec3::ZERO;
        let i = input(link, 0.0, None);
        let mut cam = start(link, 2.0);
        let s = run(&mut cam, &i, 3.0, &no_wall);
        let eye_yaw = geo(s.eye - s.at).1;
        // Link faces +z (yaw 0): behind him is -z, yaw pi.
        assert!(wrap(eye_yaw - PI).abs() < 0.06, "{eye_yaw}");
        let d = s.eye.distance(s.at);
        assert!((d - 2.5 * 68.0 * UNIT).abs() < 40.0, "Parallel1 distTarget 250 percent of Link's height: {d}");
        assert!((s.fov - 45.0).abs() < 1.0);
        // Link turns round while Z is held: the camera stays where it is.
        let i2 = input(link, 1.5, None);
        let s2 = run(&mut cam, &i2, 2.0, &no_wall);
        assert!(s2.eye.distance(s.eye) < 60.0, "{}", s2.eye.distance(s.eye));
    }

    #[test]
    fn friendly_target_keeps_one_to_two_heights_away() {
        let link = DVec3::ZERO;
        let target = DVec3::new(0.0, 150.0, 400.0);
        let mut i = input(link, 0.0, Some(target));
        i.friendly = true;
        let mut cam = start(link, 3.0);
        let s = run(&mut cam, &i, 6.0, &no_wall);
        let d = s.eye.distance(s.at);
        assert!(d >= 119.0 * UNIT && d <= 141.0 * UNIT + 30.0, "KeepOn1 120..140: {d}");
        assert_eq!(cam.kind(), Some(ZKind::Keep));
    }

    #[test]
    fn the_eye_never_ends_up_behind_a_wall_nor_in_links_lap() {
        let link = DVec3::ZERO;
        let target = DVec3::new(0.0, 100.0, 900.0);
        let i = input(link, 0.0, Some(target));
        // A wall across z = -250 (behind Link): ray hits where it crosses.
        let wall = |o: DVec3, d: DVec3, max: f64| -> Option<f64> {
            if d.z >= -1e-9 {
                return None;
            }
            let t = (-250.0 - o.z) / d.z;
            (t >= 0.0 && t <= max).then_some(t)
        };
        let mut cam = start(link, 0.0);
        for _ in 0..400 {
            let s = cam.advance(1.0 / 60.0, &i, &wall);
            assert!(s.eye.z > -250.0, "eye passed the wall: {:?}", s.eye);
        }
        let s = cam.sample();
        assert!(s.eye.distance(link + DVec3::Y * 100.0) > 80.0, "camera inside Link: {:?}", s.eye);
        // Both are still in view from there.
        let half = (s.fov * 0.5).to_radians();
        assert!(off_axis(s.eye, s.at, target) < half * 2.0);
    }

    #[test]
    fn jumping_does_not_drag_the_look_at_up_with_him() {
        let mut link = DVec3::ZERO;
        let target = DVec3::new(0.0, 100.0, 900.0);
        let i = input(link, 0.0, Some(target));
        let mut cam = start(link, 0.0);
        let s0 = run(&mut cam, &i, 4.0, &no_wall);
        link.y = 180.0;
        let mut air = input(link, 0.0, Some(target));
        air.on_ground = false;
        air.ground_y = 0.0;
        let s1 = run(&mut cam, &air, 0.3, &no_wall);
        assert!(s1.at.y - s0.at.y < 150.0, "look-at rose by {}", s1.at.y - s0.at.y);
    }
}
