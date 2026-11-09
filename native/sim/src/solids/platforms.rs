//! Platforms of Rainbow Ride and Bowser in the Dark World / Fire Sea / Sky: ports of
//! `sliding_platform_2`, `square_platform_cycle`, `rotating_octagonal_plat`, `swing_platform`,
//! `activated_bf_plat`, `ferris_wheel`, `falling_rising_platform`, `seesaw_platform`,
//! `cruiser`, `elevator`, `tilting_inverted_pyramid` and the rotating bridge of `flamethrower`
//! (`src/game/behaviors/*.inc.c`).
use super::*;

pub fn make(spec: &SolidSpec) -> Option<Box<dyn Behavior>> {
    Some(match spec.behavior.as_str() {
        "bhvSlidingPlatform2" => Box::new(SlidingPlatform2::default()),
        "bhvSquarishPathMoving" => Box::new(SquarishPathMoving),
        "bhvOctagonalPlatformRotating" => Box::new(OctagonalPlatform),
        "bhvSwingPlatform" => Box::new(SwingPlatform::default()),
        "bhvActivatedBackAndForthPlatform" => Box::new(ActivatedBackAndForth::default()),
        "bhvFerrisWheelAxle" => Box::new(FerrisWheelAxle),
        "bhvFerrisWheelPlatform" => Box::new(FerrisWheelPlatform::default()),
        "bhvBitFSSinkingPlatforms" => Box::new(SinkingPlatform::default()),
        "bhvBitFSSinkingCagePlatform" => Box::new(SinkingCagePlatform { collide: true, ..Default::default() }),
        "bhvDDDMovingPole" => Box::new(SinkingCagePlatform { collide: false, ..Default::default() }),
        "bhvSquishablePlatform" => Box::new(SquishablePlatform::default()),
        "bhvSeesawPlatform" => Box::new(SeesawPlatform::default()),
        "bhvRRCruiserWing" => Box::new(CruiserWing::default()),
        "bhvRRRotatingBridgePlatform" => Box::new(RotatingBridge),
        "bhvRRElevatorPlatform" => Box::new(Elevator { rr: true, ..Default::default() }),
        // The Hazy Maze Cave elevators run the same `elevator.inc.c` (`oElevatorUnk100` is 0 for them).
        "bhvHMCElevatorPlatform" => Box::new(Elevator::default()),
        "bhvBitFSTiltingInvertedPyramid" => Box::new(TiltingPyramid::default()),
        "bhvPlatformOnTrack" => Box::new(PlatformOnTrack::new(spec.track.clone())),
        "bhvFloorSwitchAnimatesObject" => Box::new(FloorSwitch::default()),
        "bhvAnimatesOnFloorSwitchPress" => Box::new(AnimatesOnFloorSwitchPress::default()),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// sliding_platform_2.inc.c
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct SlidingPlatform2 {
    path_length: f64,
    vel: f64,
    direction: f64,
    distance: f64,
}

impl Behavior for SlidingPlatform2 {
    /// `bhv_sliding_plat_2_init`; the collision table lookup is done by the exporter.
    fn init(&mut self, o: &mut Obj) {
        let hi = o.bparams_hi();
        let idx = (hi & 0x0380) >> 7;
        self.path_length = 50.0 * (hi & 0x003F) as f64;
        if idx < 5 || idx > 6 {
            self.vel = 15.0;
            if hi & 0x0040 != 0 {
                o.move_yaw += 0x8000 as f64;
            }
        } else {
            self.vel = 10.0;
            self.direction = if hi & 0x0040 != 0 { -1.0 } else { 1.0 };
        }
    }

    /// `bhv_sliding_plat_2_loop`.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        if o.timer > 10 {
            self.distance += self.vel;
            if clamp_f32(&mut self.distance, -self.path_length, 0.0) {
                self.vel = -self.vel;
                o.timer = 0;
            }
        }
        let saved = o.pos;
        if self.direction != 0.0 {
            o.pos[1] = o.home[1] + self.distance * self.direction;
        } else {
            o.set_dist_from_home(self.distance);
        }
        o.vel = [o.pos[0] - saved[0], o.pos[1] - saved[1], o.pos[2] - saved[2]];
    }
}

// ---------------------------------------------------------------------------
// square_platform_cycle.inc.c
// ---------------------------------------------------------------------------

pub struct SquarishPathMoving;

impl SquarishPathMoving {
    /// `square_plat_set_yaw_until_timer`.
    fn set_yaw_until_timer(o: &mut Obj, yaw: f64, a: i64) -> bool {
        o.move_yaw = yaw;
        a < o.timer
    }
}

impl Behavior for SquarishPathMoving {
    /// `bhv_squarish_path_moving_loop`.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        o.forward_vel = 10.0;
        match o.action {
            0 => o.action = (o.bparam2() & 0x03) as i32 + 1,
            1 => if Self::set_yaw_until_timer(o, 0.0, 60) { o.action += 1 },
            2 => if Self::set_yaw_until_timer(o, 0x4000 as f64, 60) { o.action += 1 },
            3 => if Self::set_yaw_until_timer(o, 0x8000 as f64, 60) { o.action += 1 },
            4 => if Self::set_yaw_until_timer(o, 0xc000 as f64, 60) { o.action = 1 },
            _ => {}
        }
        o.move_using_fvel_and_gravity();
    }
}

// ---------------------------------------------------------------------------
// rotating_octagonal_plat.inc.c
// ---------------------------------------------------------------------------

/// `sOctagonalPlatformAngularVelocities`.
pub const OCTAGONAL_ANGULAR_VELOCITIES: [f64; 4] = [300.0, -300.0, 600.0, -600.0];

pub struct OctagonalPlatform;

impl Behavior for OctagonalPlatform {
    /// `bhv_rotating_octagonal_plat_init` (collision choice `(bp >> 16) & 0xFF` is exported).
    fn init(&mut self, o: &mut Obj) {
        o.angle_vel[1] = OCTAGONAL_ANGULAR_VELOCITIES[((o.bparams >> 24) & 0xFF) as usize % 4];
    }

    /// `bhv_rotating_octagonal_plat_loop`.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        o.face[1] += o.angle_vel[1];
    }
}

// ---------------------------------------------------------------------------
// swing_platform.inc.c
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct SwingPlatform {
    angle: f64,
    speed: f64,
}

impl Behavior for SwingPlatform {
    /// `bhv_swing_platform_init`.
    fn init(&mut self, _o: &mut Obj) { self.angle = 0x2000 as f64; }

    /// `bhv_swing_platform_update`.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        let start_roll = o.face[2];
        if o.face[2] < 0.0 {
            self.speed += 4.0;
        } else {
            self.speed -= 4.0;
        }
        self.angle += self.speed;
        o.face[2] = self.angle.trunc();
        o.angle_vel[2] = o.face[2] - start_roll;
    }
}

// ---------------------------------------------------------------------------
// activated_bf_plat.inc.c
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct ActivatedBackAndForth {
    flip_rotation: f64,
    max_offset: f64,
    vertical: bool,
    vel: f64,
    offset: f64,
    countdown: i64,
}

impl Behavior for ActivatedBackAndForth {
    /// `bhv_activated_back_and_forth_platform_init`; platform type is `(hi & 0x300) >> 8`.
    fn init(&mut self, o: &mut Obj) {
        let hi = o.bparams_hi();
        let platform_type = (hi & 0x0300) >> 8;
        self.flip_rotation = if platform_type != 0 { 0.0 } else { 0x8000 as f64 };
        self.max_offset = 50.0 * (hi & 0x007F) as f64;
        if platform_type == 2 {
            self.max_offset -= 12.0;
        }
        self.vertical = hi & 0x0080 != 0;
    }

    /// `bhv_activated_back_and_forth_platform_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        o.vel[1] = if cx.riding { -6.0 } else { 6.0 };
        let dist_to_mario = js_hypot3(cx.m.pos[0] - o.pos[0], cx.m.pos[1] - o.pos[1], cx.m.pos[2] - o.pos[2]);
        if self.vel != 0.0 {
            if self.countdown != 0 {
                self.countdown -= 1;
            } else {
                self.offset += self.vel;
                if clamp_f32(&mut self.offset, 0.0, self.max_offset) || (self.vel > 0.0 && dist_to_mario > 3000.0) {
                    self.countdown = 20;
                    if o.vel[1] < 0.0 || self.vel > 0.0 {
                        self.vel = -self.vel;
                    } else {
                        self.vel = 0.0;
                    }
                    o.face[1] += self.flip_rotation;
                }
            }
        } else {
            if o.vel[1] < 0.0 {
                self.vel = 10.0;
            }
            self.countdown = 20;
        }
        let saved = o.pos;
        if self.vertical {
            o.pos[1] = o.home[1] + self.offset;
        } else {
            o.pos[1] += o.vel[1];
            clamp_f32(&mut o.pos[1], o.home[1] - 20.0, o.home[1]);
            o.set_dist_from_home(-self.offset);
        }
        o.vel = [o.pos[0] - saved[0], o.pos[1] - saved[1], o.pos[2] - saved[2]];
    }
}

// ---------------------------------------------------------------------------
// ferris_wheel.inc.c
// ---------------------------------------------------------------------------

/// `bhvFerrisWheelAxle`: `ADD_INT(oMoveAngleYaw, 0x4000)` once, `ADD_INT(oFaceAngleRoll, 400)` per frame.
pub struct FerrisWheelAxle;

impl Behavior for FerrisWheelAxle {
    fn init(&mut self, o: &mut Obj) { o.move_yaw += 0x4000 as f64; }
    fn update(&mut self, o: &mut Obj, _cx: &Cx) { o.face[2] += 400.0; }
}

/// `bhvFerrisWheelPlatform`. The exporter writes the axle's position and (move) yaw into the
/// platform spec (`pos`, `angle[1]`) and the platform number into `bparams` (as `spawn_object_relative`).
#[derive(Default)]
pub struct FerrisWheelPlatform {
    parent_pos: Vec3,
    parent_move_yaw: f64,
    parent_roll: f64,
}

impl Behavior for FerrisWheelPlatform {
    fn init(&mut self, o: &mut Obj) {
        self.parent_pos = o.pos;
        self.parent_move_yaw = o.move_yaw + 0x4000 as f64;
        o.face = [0.0; 3];
    }

    /// `bhv_ferris_wheel_platform_update`; the axle (spawned first) has already added 400 this frame.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        let saved = o.pos;
        self.parent_roll += 400.0;
        let offset_angle = to_s16(self.parent_roll + o.bparam2() as f64 * 0x4000 as f64) as f64;
        let offset_xz = 400.0 * coss(offset_angle);
        o.pos[0] = self.parent_pos[0] + offset_xz * sins(self.parent_move_yaw) + 300.0 * coss(self.parent_move_yaw);
        o.pos[1] = self.parent_pos[1] + 400.0 * sins(offset_angle);
        o.pos[2] = self.parent_pos[2] + offset_xz * coss(self.parent_move_yaw) + 300.0 * sins(self.parent_move_yaw);
        o.vel = [o.pos[0] - saved[0], o.pos[1] - saved[1], o.pos[2] - saved[2]];
    }
}

// ---------------------------------------------------------------------------
// falling_rising_platform.inc.c
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct SinkingPlatform {
    timer: f64,
}

impl Behavior for SinkingPlatform {
    /// `bhv_bitfs_sinking_platform_loop`.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        o.pos[1] -= sins(self.timer) * 0.58;
        self.timer += 0x100 as f64;
    }
}

#[derive(Default)]
pub struct SinkingCagePlatform {
    timer: f64,
    pub collide: bool,
}

impl Behavior for SinkingCagePlatform {
    /// `bhv_bitfs_sinking_cage_platform_loop`.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        if o.bparam2() != 0 {
            if o.timer == 0 {
                o.pos[1] -= 300.0;
            }
            o.pos[1] += sins(self.timer) * 7.0;
        } else {
            o.pos[1] -= sins(self.timer) * 3.0;
        }
        self.timer += 0x100 as f64;
    }
    fn solid(&self) -> bool { self.collide }
}

#[derive(Default)]
pub struct SquishablePlatform {
    timer: f64,
}

impl Behavior for SquishablePlatform {
    /// `bhv_platform_normals_init` leaves the transform at identity with the object's position.
    fn init(&mut self, o: &mut Obj) { o.face = [0.0; 3]; }

    /// `bhv_squishable_platform_loop`: the collision is scaled with the model.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        o.scale3[1] = (sins(self.timer) + 1.0) * 0.3 + 0.4;
        self.timer += 0x80 as f64;
    }
}

// ---------------------------------------------------------------------------
// seesaw_platform.inc.c
// ---------------------------------------------------------------------------

/// `oscillate_toward` (obj_behaviors_2.c) on an s32 `value`.
pub fn oscillate_toward(value: &mut f64, vel: &mut f64, target: f64, vel_close_to_zero: f64, mut accel: f64, slowdown: f64) -> bool {
    let start = *value;
    *value += vel.trunc();
    if *value == target || ((*value - target) * (start - target) < 0.0 && *vel > -vel_close_to_zero && *vel < vel_close_to_zero) {
        *value = target;
        *vel = 0.0;
        return true;
    }
    if *value >= target {
        accel = -accel;
    }
    if *vel * accel < 0.0 {
        accel *= slowdown;
    }
    *vel += accel;
    false
}

#[derive(Default)]
pub struct SeesawPlatform {
    pitch_vel: f64,
}

impl SeesawPlatform {
    pub fn pitch_vel(&self) -> f64 { self.pitch_vel }
}

impl Behavior for SeesawPlatform {
    /// `bhv_seesaw_platform_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        o.face[0] += self.pitch_vel.trunc();
        if cx.riding {
            let dx = cx.m.pos[0] - o.pos[0];
            let dz = cx.m.pos[2] - o.pos[2];
            let dist = js_hypot3(dx, cx.m.pos[1] - o.pos[1], dz);
            let angle_to_mario = atan2s(dz, dx) as f64;
            let mut rotation = dist * coss(angle_to_mario - o.move_yaw);
            if self.pitch_vel * rotation < 0.0 {
                rotation *= 0.04;
            } else {
                rotation *= 0.02;
            }
            self.pitch_vel += rotation;
            clamp_f32(&mut self.pitch_vel, -50.0, 50.0);
            // clamp_f32 only reports; the call above already clamped in place
        } else {
            oscillate_toward(&mut o.face[0], &mut self.pitch_vel, 0.0, 6.0, 3.0, 3.0);
        }
    }
}

// ---------------------------------------------------------------------------
// cruiser.inc.c
// ---------------------------------------------------------------------------

/// The Rainbow Ride cruiser's flapping wings (visual only; the script never loads collision).
#[derive(Default)]
pub struct CruiserWing {
    base_yaw: f64,
    base_pitch: f64,
}

impl Behavior for CruiserWing {
    /// `bhv_rr_cruiser_wing_init`.
    fn init(&mut self, o: &mut Obj) {
        self.base_yaw = o.face[1];
        self.base_pitch = o.face[0];
    }

    /// `bhv_rr_cruiser_wing_loop`.
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        let t = o.timer as f64 * 0x400 as f64;
        if o.bparam2() == 0 {
            o.face[1] = (self.base_yaw + sins(t) * 8192.0).trunc();
        } else {
            o.face[1] = (self.base_yaw - sins(t) * 8192.0).trunc();
        }
        o.face[0] = (self.base_pitch + coss(t) * 2048.0).trunc();
        if o.timer == 64 {
            o.timer = 0;
        }
    }
    fn solid(&self) -> bool { false }
}

// ---------------------------------------------------------------------------
// flamethrower.inc.c: bhv_rr_rotating_bridge_platform_loop
// ---------------------------------------------------------------------------

/// The rotating RR bridge. (The embedded `bhv_flamethrower_loop` call is not ported: the fire
/// jets are separate objects, see `DECOMP_TALLY.md`.)
pub struct RotatingBridge;

impl Behavior for RotatingBridge {
    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        o.move_yaw -= 0x80 as f64;
        o.angle_vel[1] = -(0x80 as f64);
        o.sync_face_yaw = true;
    }
}

// ---------------------------------------------------------------------------
// elevator.inc.c (bhvRRElevatorPlatform)
// ---------------------------------------------------------------------------

/// `sElevatorHeights` rows: { low, high, uses both }.
pub const ELEVATOR_HEIGHTS: [(f64, f64, i32); 7] = [
    (-51.0, 0.0, 0), (-461.0, 0.0, 0), (-512.0, 0.0, 0), (-2611.0, 0.0, 0), (-2360.0, 0.0, 0), (214.0, 0.0, 0), (-50.0, 1945.0, 1),
];

#[derive(Default)]
pub struct Elevator {
    /// `cur_obj_has_behavior(bhvRRElevatorPlatform)`: the Rainbow Ride elevators park at both ends (`oElevatorUnk100` 1).
    rr: bool,
    low: f64,
    high: f64,
    mid: f64,
    kind: i32,
}

impl Behavior for Elevator {
    /// `bhv_elevator_init` (RR elevator: `oElevatorUnk100 = 1`).
    fn init(&mut self, o: &mut Obj) {
        let (low, high, both) = ELEVATOR_HEIGHTS[(o.bparam2() as usize).min(6)];
        if both == 0 {
            self.low = low;
            self.high = o.home[1];
            self.mid = (self.low + self.high) / 2.0;
            self.kind = i32::from(self.rr);
        } else {
            self.low = low;
            self.high = high;
            self.mid = (self.low + self.high) / 2.0;
            self.kind = 2;
        }
    }

    /// `bhv_elevator_loop` (`elevator_act_0..4`).
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        let mario_y = cx.m.pos[1];
        let in_air = cx.m.action & ACT_FLAG_AIR != 0;
        match o.action {
            0 => {
                o.vel[1] = 0.0;
                if self.kind == 2 {
                    if cx.riding {
                        o.action = if o.pos[1] > self.mid { 2 } else { 1 };
                    }
                } else if mario_y > self.mid || self.kind == 1 {
                    o.pos[1] = self.high;
                    if cx.riding {
                        o.action = 2;
                    }
                } else {
                    o.pos[1] = self.low;
                    if cx.riding {
                        o.action = 1;
                    }
                }
            }
            1 => {
                approach_f32_signed(&mut o.vel[1], 10.0, 2.0);
                o.pos[1] += o.vel[1];
                if o.pos[1] > self.high {
                    o.pos[1] = self.high;
                    if self.kind == 2 || self.kind == 1 {
                        o.action = 3;
                    } else if mario_y < self.mid {
                        o.action = 2;
                    } else {
                        o.action = 3;
                    }
                }
            }
            2 => {
                approach_f32_signed(&mut o.vel[1], -10.0, -2.0);
                o.pos[1] += o.vel[1];
                if o.pos[1] < self.low {
                    o.pos[1] = self.low;
                    if self.kind == 1 {
                        o.action = 4;
                    } else if self.kind == 2 {
                        o.action = 3;
                    } else if mario_y > self.mid {
                        o.action = 1;
                    } else {
                        o.action = 3;
                    }
                }
            }
            3 => {
                o.vel[1] = 0.0;
                if !in_air && !cx.riding {
                    o.action = 0;
                }
            }
            4 => {
                o.vel[1] = 0.0;
                if !in_air && !cx.riding {
                    o.action = 1;
                }
            }
            _ => {}
        }
    }
}

/// `approach_f32_signed` (object_helpers.c).
pub fn approach_f32_signed(value: &mut f64, target: f64, increment: f64) -> bool {
    let mut reached = false;
    *value += increment;
    if increment >= 0.0 {
        if *value > target {
            *value = target;
            reached = true;
        }
    } else if *value < target {
        *value = target;
        reached = true;
    }
    reached
}

// ---------------------------------------------------------------------------
// tilting_inverted_pyramid.inc.c
// ---------------------------------------------------------------------------

/// `mtxf_align_terrain_normal` (math_util.c) with yaw 0.
pub fn mtxf_align_terrain_normal(up: Vec3, pos: Vec3, yaw: f64) -> Mat4 {
    fn norm(v: Vec3) -> Vec3 {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l == 0.0 { v } else { [v[0] / l, v[1] / l, v[2] / l] }
    }
    fn cross(a: Vec3, b: Vec3) -> Vec3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
    let lateral = [sins(yaw), 0.0, coss(yaw)];
    let up = norm(up);
    let left = norm(cross(up, lateral));
    let forward = norm(cross(left, up));
    let mut d = [[0.0; 4]; 4];
    d[0] = [left[0], left[1], left[2], 0.0];
    d[1] = [up[0], up[1], up[2], 0.0];
    d[2] = [forward[0], forward[1], forward[2], 0.0];
    d[3] = [pos[0], pos[1], pos[2], 1.0];
    d
}

/// `approach_by_increment`.
pub fn approach_by_increment(goal: f64, src: f64, inc: f64) -> f64 {
    if src <= goal {
        if goal - src < inc { goal } else { src + inc }
    } else if goal - src > -inc {
        goal
    } else {
        src - inc
    }
}

#[derive(Default)]
pub struct TiltingPyramid {
    normal: Vec3,
    mario_on_platform: bool,
}

impl Behavior for TiltingPyramid {
    /// `bhv_platform_normals_init`.
    fn init(&mut self, o: &mut Obj) {
        self.normal = [0.0, 1.0, 0.0];
        o.transform = Some(mtxf_align_terrain_normal(self.normal, o.pos, 0.0));
    }

    /// `bhv_tilting_inverted_pyramid_loop`: tilt toward Mario, carrying him with the rotation.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        let mut mario_on = false;
        let (mut dx, mut dy, mut dz) = (0.0, 1.0, 0.0);
        let mut before = [0.0; 3];
        let mut dist = [0.0; 3];
        if cx.riding {
            dist = [cx.m.pos[0] - o.pos[0], cx.m.pos[1] - o.pos[1], cx.m.pos[2] - o.pos[2]];
            before = linear_mtxf_mul_vec3f(&o.transform.unwrap(), dist);
            dx = cx.m.pos[0] - o.pos[0];
            dy = 500.0;
            dz = cx.m.pos[2] - o.pos[2];
            let d = (dx * dx + dy * dy + dz * dz).sqrt();
            if d != 0.0 {
                dx /= d;
                dy /= d;
                dz /= d;
            } else {
                dx = 0.0;
                dy = 1.0;
                dz = 0.0;
            }
            if self.mario_on_platform {
                mario_on = true;
            }
            self.mario_on_platform = true;
        } else {
            self.mario_on_platform = false;
        }
        self.normal = [
            approach_by_increment(dx, self.normal[0], 0.01),
            approach_by_increment(dy, self.normal[1], 0.01),
            approach_by_increment(dz, self.normal[2], 0.01),
        ];
        let t = mtxf_align_terrain_normal(self.normal, o.pos, 0.0);
        o.transform = Some(t);
        if mario_on {
            let after = linear_mtxf_mul_vec3f(&t, dist);
            o.mario_offset = [after[0] - before[0], after[1] - before[1], after[2] - before[2]];
        }
    }
}

// ---------------------------------------------------------------------------
// platform_on_track.inc.c + platform_on_track_update_pos_or_spawn_ball (obj_behaviors_2.c)
// ---------------------------------------------------------------------------

pub const PLAT_INIT: i32 = 0;
pub const PLAT_WAIT_FOR_MARIO: i32 = 1;
pub const PLAT_MOVE_ALONG_TRACK: i32 = 2;
pub const PLAT_PAUSE_BRIEFLY: i32 = 3;
pub const PLAT_FALL: i32 = 4;
/// `PLATFORM_ON_TRACK_BP_*` (bits of `oBhvParams >> 16`).
pub const BP_RETURN_TO_START: u32 = 1 << 8;
pub const BP_DONT_DISAPPEAR: u32 = 1 << 9;
pub const BP_DONT_TURN_YAW: u32 = 1 << 10;
pub const BP_DONT_TURN_ROLL: u32 = 1 << 11;
pub const WAYPOINT_FLAGS_END: f64 = -1.0;

/// The carpets of Rainbow Ride, the BitFS grate platform and the checkerboard lifts of the LLL volcano. The waypoints are `[flags, x, y, z]`
/// rows with a final `flags = -1` row (`TRAJECTORY_END`). The track balls (the dots that mark the
/// path) are not drawn.
pub struct PlatformOnTrack {
    track: Vec<[f64; 4]>,
    kind: u32,
    start_yaw: f64,
    prev_waypoint: usize,
    prev_waypoint_flags: f64,
    pitch: f64,
    yaw: f64,
    was_stood_on: bool,
    offset_y: f64,
    blinking: bool,
}

impl PlatformOnTrack {
    pub fn new(track: Vec<[f64; 4]>) -> Self {
        Self { track, kind: 0, start_yaw: 0.0, prev_waypoint: 0, prev_waypoint_flags: 0.0, pitch: 0.0, yaw: 0.0, was_stood_on: false, offset_y: 0.0, blinking: false }
    }

    fn wp(&self, i: usize) -> [f64; 3] { [self.track[i][1], self.track[i][2], self.track[i][3]] }

    /// `platform_on_track_update_pos_or_spawn_ball(0, ...)`.
    fn update_pos(&mut self, o: &mut Obj) {
        let saved = o.pos;
        let initial_prev = self.prev_waypoint;
        let mut next = initial_prev;
        self.prev_waypoint_flags = 0.0;
        let mut amount = o.forward_vel;
        let (mut x, mut y, mut z) = (o.pos[0], o.pos[1], o.pos[2]);
        let (mut dx, mut dy, mut dz, mut dist);
        let mut prev;
        loop {
            prev = next;
            next += 1;
            if self.track[next][0] == WAYPOINT_FLAGS_END {
                self.prev_waypoint_flags = WAYPOINT_FLAGS_END;
                if o.bparams_hi() & BP_RETURN_TO_START != 0 {
                    next = 0;
                } else {
                    return;
                }
            }
            let w = self.wp(next);
            dx = w[0] - x;
            dy = w[1] - y;
            dz = w[2] - z;
            dist = (dx * dx + dy * dy + dz * dz).sqrt();
            amount -= dist;
            x += dx;
            y += dy;
            z += dz;
            if amount <= 0.0 {
                break;
            }
        }
        let back = amount / dist;
        x += dx * back;
        y += dy * back;
        z += dz * back;
        if prev != initial_prev {
            if self.prev_waypoint_flags == 0.0 {
                self.prev_waypoint_flags = self.track[initial_prev][0];
            }
            self.prev_waypoint = prev;
        }
        o.pos = [x, y, z];
        o.vel = [o.pos[0] - saved[0], o.pos[1] - saved[1], o.pos[2] - saved[2]];
        self.pitch = atan2s(js_hypot(o.vel[0], o.vel[2]), -o.vel[1]) as f64;
        self.yaw = atan2s(o.vel[2], o.vel[0]) as f64;
    }

    /// `platform_on_track_mario_not_on_platform` (+ `cur_obj_wait_then_blink(150, 40)`).
    fn mario_not_on_platform(&mut self, o: &mut Obj) {
        if o.bparams_hi() & BP_DONT_DISAPPEAR == 0 && o.timer >= 150 {
            let blinking = o.timer - 150;
            if blinking % 2 != 0 {
                o.visible = false;
                if blinking / 2 > 40 {
                    // platform_on_track_reset
                    o.action = PLAT_INIT;
                    o.visible = true;
                }
            } else {
                o.visible = true;
            }
        }
    }
}

impl Behavior for PlatformOnTrack {
    /// `bhv_platform_on_track_init`.
    fn init(&mut self, o: &mut Obj) {
        self.kind = (o.bparams_hi() & 0x0070) >> 4;
        // PLATFORM_ON_TRACK_TYPE_CHECKERED (the LLL volcano lifts): `header.gfx.scale[1] = 2.0f`, which
        // obj_apply_scale_to_matrix also applies to the collision.
        if self.kind == 2 {
            o.scale3[1] = 2.0;
        }
        self.start_yaw = o.move_yaw;
        o.gravity = -1.0;
        self.blinking = false;
    }

    /// `bhv_platform_on_track_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        match o.action {
            PLAT_INIT => {
                self.prev_waypoint = 0;
                self.prev_waypoint_flags = 0.0;
                let w = self.wp(0);
                o.pos = w;
                o.home = w;
                o.face[1] = self.start_yaw;
                o.forward_vel = 0.0;
                o.vel = [0.0; 3];
                self.was_stood_on = false;
                if self.kind != 1 {
                    o.face[2] = 0.0;
                }
                o.action = PLAT_WAIT_FOR_MARIO;
            }
            PLAT_WAIT_FOR_MARIO => {
                if cx.riding {
                    if o.timer > 20 {
                        o.action = PLAT_MOVE_ALONG_TRACK;
                    }
                } else {
                    o.timer = 0;
                }
            }
            PLAT_MOVE_ALONG_TRACK => {
                if self.prev_waypoint_flags == WAYPOINT_FLAGS_END && o.bparams_hi() & BP_RETURN_TO_START == 0 {
                    o.action = PLAT_FALL;
                } else {
                    o.forward_vel = 10.0;
                    self.update_pos(o);
                    o.move_yaw = self.yaw;
                    if o.bparams_hi() & BP_DONT_TURN_YAW == 0 {
                        let target = to_s16(o.move_yaw + 0x4000 as f64) as f64;
                        let mut yaw_speed = (abs_angle_diff(target, o.face[1]) / 20.0).trunc();
                        yaw_speed = clamp(yaw_speed, 100.0, 500.0);
                        let initial = to_s16(o.face[1]) as f64;
                        o.face_yaw_approach(target, yaw_speed);
                        o.angle_vel[1] = to_s16(o.face[1]) as f64 - initial;
                    }
                    if o.bparams_hi() & BP_DONT_TURN_ROLL != 0 {
                        let mut roll_speed = (abs_angle_diff(self.pitch, o.face[2]) / 20.0).trunc();
                        roll_speed = clamp(roll_speed, 100.0, 500.0);
                        let initial = to_s16(o.face[2]) as f64;
                        o.face_roll_approach(self.pitch, roll_speed);
                        o.angle_vel[2] = to_s16(o.face[2]) as f64 - initial;
                    }
                }
                if !cx.riding {
                    self.mario_not_on_platform(o);
                } else {
                    o.timer = 0;
                    o.visible = true;
                }
            }
            PLAT_PAUSE_BRIEFLY => {
                if o.timer > 20 {
                    o.action = PLAT_MOVE_ALONG_TRACK;
                }
            }
            _ => {
                o.move_using_vel_and_gravity();
                if !cx.riding {
                    self.mario_not_on_platform(o);
                } else {
                    o.timer = 0;
                }
            }
        }
        // the carpet dips when Mario first steps on it
        if self.kind == 0 {
            if !self.was_stood_on && cx.riding {
                self.offset_y = -8.0;
                self.was_stood_on = true;
            }
            approach_f32_ptr(&mut self.offset_y, 0.0, 0.5);
            o.pos[1] += self.offset_y;
        }
    }
}

/// `abs_angle_diff` (object_helpers.c).
pub fn abs_angle_diff(x0: f64, x1: f64) -> f64 {
    let mut diff = to_s16(x1 - x0);
    if diff == -0x8000 {
        diff = -0x7FFF;
    }
    (diff as f64).abs()
}

// ---------------------------------------------------------------------------
// purple_switch.inc.c (bhvFloorSwitchAnimatesObject) and animated_floor_switch.inc.c
// ---------------------------------------------------------------------------

pub const SWITCH_IDLE: i32 = 0;
pub const SWITCH_PRESSED: i32 = 1;
pub const SWITCH_TICKING: i32 = 2;
pub const SWITCH_UNPRESSED: i32 = 3;
pub const SWITCH_WAIT_FOR_MARIO_TO_GET_OFF: i32 = 4;

/// The purple floor switch of the BitDW / BitS / RR staircases (`oBhvParams2ndByte` = 1:
/// it ticks only while Mario stands on it, for at most 400 frames).
#[derive(Default)]
pub struct FloorSwitch;

impl FloorSwitch {
    /// `cur_obj_scale_over_time(2, 3, start, end)`: the Y scale (collision included).
    fn scale_y(o: &mut Obj, start: f64, end: f64) {
        o.scale3[1] = start + (end - start) * (o.timer as f64 / 3.0).min(1.0);
    }
}

impl Behavior for FloorSwitch {
    fn init(&mut self, o: &mut Obj) { o.scale3 = [1.5; 3]; }

    /// `bhv_purple_switch_loop`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        match o.action {
            SWITCH_IDLE => {
                o.scale3 = [1.5; 3];
                if cx.riding {
                    let dx = cx.m.pos[0] - o.pos[0];
                    let dz = cx.m.pos[2] - o.pos[2];
                    if js_hypot(dx, dz) < 127.5 {
                        o.action = SWITCH_PRESSED;
                    }
                }
            }
            SWITCH_PRESSED => {
                Self::scale_y(o, 1.5, 0.2);
                if o.timer == 3 {
                    o.action = SWITCH_TICKING;
                }
            }
            SWITCH_TICKING => {
                if o.bparam2() != 0 {
                    if o.bparam2() == 1 && !cx.riding {
                        o.action += 1;
                    } else if o.timer > 400 {
                        o.action = SWITCH_WAIT_FOR_MARIO_TO_GET_OFF;
                    }
                }
            }
            SWITCH_UNPRESSED => {
                Self::scale_y(o, 0.2, 1.5);
                if o.timer == 3 {
                    o.action = SWITCH_IDLE;
                }
            }
            _ => {
                if !cx.riding {
                    o.action = SWITCH_UNPRESSED;
                }
            }
        }
    }
}

/// `D_80331ACC`: ticks of animation per `oBhvParams2ndByte` (BitS, BitDW, RR).
pub const STAIRCASE_DURATIONS: [f64; 3] = [250.0, 200.0, 200.0];

/// The collapsing staircase / tricky triangles: while the purple switch ticks the object steps
/// through its five collision/model frames (`oFloorSwitchPressAnimationUnkF8 / 2`).
#[derive(Default)]
pub struct AnimatesOnFloorSwitchPress {
    latched: bool,
    toggle: bool,
    countdown: f64,
    frame: f64,
    init: bool,
}

impl Behavior for AnimatesOnFloorSwitchPress {
    /// `bhv_animates_on_floor_switch_press_loop`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        if !self.init {
            self.init = true;
            self.toggle = false;
        }
        let ticking = cx.floor_switch_ticking;
        if self.latched {
            if !ticking {
                self.latched = false;
            }
            self.countdown = if self.toggle { STAIRCASE_DURATIONS[(o.bparam2() as usize).min(2)] } else { 0.0 };
        } else if ticking {
            self.toggle = !self.toggle;
            self.latched = true;
        }
        if self.countdown != 0.0 {
            self.countdown -= 1.0;
            if self.countdown == 0.0 {
                self.toggle = false;
            }
            if self.frame < 9.0 {
                self.frame += 1.0;
            }
        } else {
            self.frame -= 2.0;
            if self.frame < 0.0 {
                self.frame = 0.0;
                self.toggle = true;
            }
        }
        o.variant = (self.frame / 2.0).trunc() as usize;
    }
}
