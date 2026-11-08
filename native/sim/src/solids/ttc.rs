//! Tick Tock Clock objects: ports of `src/game/behaviors/ttc_*.inc.c`.
//!
//! Every behavior reads `gTTCSpeedSetting` ([`super::TTC_SPEED_SLOW`] ...) which the castle's
//! clock painting fixes when Mario enters ([`speed_from_minute_hand`], `clock_arm.inc.c`).
use super::*;
use std::sync::{Arc, Mutex};

/// `bhv_rotating_clock_arm_loop` (clock_arm.inc.c): TTC's speed from the minute hand's roll angle
/// (`oFaceAngleRoll` as u16) at the moment Mario touches the painting.
pub fn speed_from_minute_hand(roll_angle: u32) -> i32 {
    let roll = roll_angle & 0xFFFF;
    if roll < 0xAAA {
        TTC_SPEED_STOPPED
    } else if roll < 0x6AA4 {
        TTC_SPEED_FAST
    } else if roll < 0x954C {
        TTC_SPEED_RANDOM
    } else if roll < 0xF546 {
        TTC_SPEED_SLOW
    } else {
        TTC_SPEED_STOPPED
    }
}

/// `bhvClockMinuteHand`: `oAngleVelRoll = -0x180` per frame in the castle.
pub const MINUTE_HAND_ANGLE_VEL: i64 = -0x180;

/// Roll of the castle's minute hand after `frames` frames spent in the castle (it starts at 0).
pub fn minute_hand_roll(frames: i64) -> u32 { ((frames * MINUTE_HAND_ANGLE_VEL) & 0xFFFF) as u32 }

/// Shared by all treadmills: the movtex speed (`*oTTCTreadmillBigSurface`) and the state of the
/// master treadmill, the first one in processing order.
#[derive(Default)]
pub struct TreadmillShared {
    pub master: Option<usize>,
    pub speed: f64,
    pub target_speed: f64,
    pub time_until_switch: i64,
}

pub fn make(spec: &SolidSpec, speed: i32) -> Option<Box<dyn Behavior>> {
    make_with(spec, speed, &TREADMILL.with(|t| t.clone()))
}

thread_local! {
    /// One treadmill state per thread (a course is simulated on one thread; `Solids::new` resets it).
    static TREADMILL: Arc<Mutex<TreadmillShared>> = Arc::new(Mutex::new(TreadmillShared::default()));
}

/// Reset the shared treadmill state (`sMasterTreadmill = NULL` in `bhv_ttc_treadmill_init`).
pub fn reset_shared() { TREADMILL.with(|t| *t.lock().unwrap() = TreadmillShared::default()); }

pub fn make_with(spec: &SolidSpec, speed: i32, treadmill: &Arc<Mutex<TreadmillShared>>) -> Option<Box<dyn Behavior>> {
    Some(match spec.behavior.as_str() {
        "bhvTTCRotatingSolid" => Box::new(RotatingSolid::new(speed)),
        "bhvTTC2DRotator" => Box::new(Rotator2D::new(speed)),
        "bhvTTCCog" => Box::new(Cog::default()),
        "bhvTTCElevator" => Box::new(Elevator::default()),
        "bhvTTCMovingBar" => Box::new(MovingBar::new(speed)),
        "bhvTTCPendulum" => Box::new(Pendulum::new(speed)),
        "bhvTTCPitBlock" => Box::new(PitBlock::new(speed)),
        "bhvTTCSpinner" => Box::new(Spinner::default()),
        "bhvTTCTreadmill" => Box::new(Treadmill { shared: treadmill.clone(), speed }),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// ttc_rotating_solid.inc.c
// ---------------------------------------------------------------------------

/// `sTTCRotatingSolidInitialDelays`.
pub const ROTATING_SOLID_INITIAL_DELAYS: [i64; 4] = [120, 40, 0, 0];

pub struct RotatingSolid {
    num_sides: f64,
    num_turns: f64,
    rotation_delay: i64,
    vel_y: f64,
    sound_timer: i32,
    pub sounds: u32,
}

impl RotatingSolid {
    pub fn new(speed: i32) -> Self {
        Self { num_sides: 4.0, num_turns: 1.0, rotation_delay: ROTATING_SOLID_INITIAL_DELAYS[speed as usize], vel_y: 0.0, sound_timer: 0, sounds: 0 }
    }
}

impl Behavior for RotatingSolid {
    /// `bhv_ttc_rotating_solid_init`.
    fn init(&mut self, o: &mut Obj) {
        self.num_sides = if o.bparam2() == 0 { 4.0 } else { 3.0 };
    }

    /// `bhv_ttc_rotating_solid_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        if cx.ttc_speed != TTC_SPEED_STOPPED && o.timer > self.rotation_delay {
            if self.sound_timer != 0 {
                self.sound_timer -= 1;
                if self.sound_timer == 0 {
                    self.sounds += 1; // SOUND_GENERAL2_ROTATING_BLOCK_ALERT
                }
            } else if self.vel_y > 0.0 && o.pos[1] >= o.home[1] {
                let target_roll = (self.num_turns / self.num_sides * 65536.0).trunc();
                let start_roll = o.face[2];
                o.face_roll_approach(target_roll, 1200.0);
                o.angle_vel[2] = o.face[2] - start_roll;
                if o.angle_vel[2] == 0.0 {
                    // SOUND_GENERAL2_ROTATING_BLOCK_CLICK
                    self.num_turns = (self.num_turns + 1.0) % self.num_sides;
                    o.timer = 0;
                    if cx.ttc_speed == TTC_SPEED_RANDOM {
                        self.rotation_delay = random_mod_offset(5, 20, 7);
                    }
                }
            } else {
                self.vel_y += 0.5;
                o.pos[1] += self.vel_y;
                if o.pos[1] >= o.home[1] {
                    o.pos[1] = o.home[1];
                    self.sound_timer = 6;
                }
            }
        } else {
            self.vel_y = -5.0;
        }
    }
}

// ---------------------------------------------------------------------------
// ttc_2d_rotator.inc.c
// ---------------------------------------------------------------------------

/// `sTTC2DRotatorSpeeds`: hand, 2D cog.
pub const ROTATOR_SPEEDS: [f64; 2] = [-0x444 as f64, -0xCCC as f64];
/// `sTTC2DRotatorTimeBetweenTurns[bp][speed]`.
pub const ROTATOR_TIME_BETWEEN_TURNS: [[i64; 4]; 2] = [[40, 10, 10, 0], [20, 5, 5, 0]];

pub struct Rotator2D {
    min_time_until_next_turn: i64,
    increment: f64,
    speed: f64,
    target_yaw: f64,
    random_dir_timer: i64,
    bp: usize,
    setting: i32,
}

impl Rotator2D {
    pub fn new(setting: i32) -> Self {
        Self { min_time_until_next_turn: 0, increment: 0.0, speed: 0.0, target_yaw: 0.0, random_dir_timer: 0, bp: 0, setting }
    }
}

impl Behavior for Rotator2D {
    /// `bhv_ttc_2d_rotator_init`.
    fn init(&mut self, o: &mut Obj) {
        self.bp = o.bparam2() as usize;
        self.min_time_until_next_turn = ROTATOR_TIME_BETWEEN_TURNS[self.bp][self.setting as usize];
        self.speed = ROTATOR_SPEEDS[self.bp];
        self.increment = self.speed;
    }

    /// `bhv_ttc_2d_rotator_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        let start_yaw = o.face[1];
        if self.random_dir_timer != 0 {
            self.random_dir_timer -= 1;
        }
        if self.min_time_until_next_turn != 0 && o.face_yaw_approach(self.target_yaw, 0xC8 as f64) && o.timer > self.min_time_until_next_turn {
            self.target_yaw += self.increment;
            o.timer = 0;
            if cx.ttc_speed == TTC_SPEED_RANDOM {
                if self.random_dir_timer == 0 {
                    if random_u16() & 0x03 != 0 {
                        self.increment = self.speed;
                        self.random_dir_timer = random_mod_offset(90, 60, 4);
                    } else {
                        self.increment = -self.speed;
                        self.random_dir_timer = random_mod_offset(30, 30, 3);
                    }
                }
                self.min_time_until_next_turn = random_mod_offset(10, 20, 3);
            }
        }
        o.angle_vel[1] = o.face[1] - start_yaw;
    }

    /// Only the clock hand (bp 0) calls `load_object_collision_model`.
    fn solid(&self) -> bool { self.bp == 0 }
}

// ---------------------------------------------------------------------------
// ttc_cog.inc.c
// ---------------------------------------------------------------------------

/// `sTTCCogNormalSpeeds`.
pub const COG_NORMAL_SPEEDS: [f64; 2] = [200.0, 400.0];

#[derive(Default)]
pub struct Cog {
    dir: f64,
    speed: f64,
    target_vel: f64,
}

impl Behavior for Cog {
    /// `bhv_ttc_cog_init`: `sTTCCogDirections[bp & TTC_COG_BP_DIR_MASK]` = { 1, -1 }.
    fn init(&mut self, o: &mut Obj) {
        self.dir = if o.bparam2() & 1 == 0 { 1.0 } else { -1.0 };
    }

    /// `bhv_ttc_cog_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        match cx.ttc_speed {
            TTC_SPEED_SLOW | TTC_SPEED_FAST => self.speed = COG_NORMAL_SPEEDS[cx.ttc_speed as usize],
            TTC_SPEED_RANDOM => {
                if approach_f32_ptr(&mut self.speed, self.target_vel, 50.0) {
                    self.target_vel = 200.0 * (random_u16() % 7) as f64 * random_sign();
                }
            }
            _ => {}
        }
        o.angle_vel[1] = (self.speed * self.dir).trunc();
        o.face[1] += o.angle_vel[1];
    }
}

// ---------------------------------------------------------------------------
// ttc_elevator.inc.c
// ---------------------------------------------------------------------------

/// `sTTCElevatorSpeeds`.
pub const ELEVATOR_SPEEDS: [f64; 4] = [6.0, 10.0, 6.0, 0.0];

pub struct Elevator {
    peak_y: f64,
    dir: f64,
    move_time: i64,
}

impl Default for Elevator {
    fn default() -> Self { Self { peak_y: 0.0, dir: 1.0, move_time: 0 } }
}

impl Behavior for Elevator {
    /// `bhv_ttc_elevator_init`.
    fn init(&mut self, o: &mut Obj) {
        let hi = ((o.bparams >> 16) & 0xFFFF) as f64;
        let peak_offset = if hi != 0.0 { 100.0 * hi } else { 500.0 };
        self.peak_y = o.pos[1] + peak_offset;
    }

    /// `bhv_ttc_elevator_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        o.vel[1] = ELEVATOR_SPEEDS[cx.ttc_speed as usize] * self.dir;
        if cx.ttc_speed == TTC_SPEED_RANDOM {
            if o.timer > self.move_time {
                self.dir = random_sign();
                self.move_time = random_mod_offset(30, 30, 6);
                o.timer = 0;
            } else if o.timer < 5 {
                o.vel[1] = 0.0;
            }
        }
        o.move_using_fvel_and_gravity();
        if clamp_f32(&mut o.pos[1], o.home[1], self.peak_y) {
            self.dir = -self.dir;
        }
    }
}

// ---------------------------------------------------------------------------
// ttc_moving_bar.inc.c
// ---------------------------------------------------------------------------

/// `TTC_MOVING_BAR_ACT_*`.
pub const BAR_WAIT: i32 = 0;
pub const BAR_PULL_BACK: i32 = 1;
pub const BAR_EXTEND: i32 = 2;
pub const BAR_RETRACT: i32 = 3;
/// `sTTCMovingBarDelays`.
pub const BAR_DELAYS: [i64; 4] = [55, 30, 55, 0];
/// `sTTCMovingBarRandomDelays`.
pub const BAR_RANDOM_DELAYS: [i64; 4] = [1, 12, 55, 100];

pub struct MovingBar {
    delay: i64,
    offset: f64,
    start_offset: f64,
    speed: f64,
    stopped_timer: i64,
    setting: i32,
}

impl MovingBar {
    pub fn new(setting: i32) -> Self {
        Self { delay: 0, offset: 0.0, start_offset: 0.0, speed: 0.0, stopped_timer: 0, setting }
    }
    pub fn offset(&self) -> f64 { self.offset }

    fn reset(&mut self, o: &mut Obj) {
        self.offset = 0.0;
        self.speed = 0.0;
        o.action = BAR_WAIT;
    }
}

impl Behavior for MovingBar {
    /// `bhv_ttc_moving_bar_init`.
    fn init(&mut self, o: &mut Obj) {
        self.delay = BAR_DELAYS[self.setting as usize];
        if self.delay == 0 {
            self.offset = 250.0;
        }
        self.stopped_timer = 10 * o.bparam2() as i64;
        o.move_yaw = 0x4000 as f64 - o.move_yaw;
    }

    /// `bhv_ttc_moving_bar_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        self.start_offset = self.offset;
        let saved = o.pos; // POS_OP_SAVE_POSITION
        self.offset += self.speed;
        match o.action {
            BAR_WAIT => {
                if self.delay != 0 && o.timer > self.delay {
                    if self.stopped_timer != 0 {
                        self.stopped_timer -= 1;
                    } else {
                        if cx.ttc_speed == TTC_SPEED_RANDOM {
                            self.delay = BAR_RANDOM_DELAYS[(random_u16() & 0x03) as usize];
                            if random_u16() % 2 == 0 {
                                self.stopped_timer = random_linear_offset(20, 100);
                            }
                        }
                        o.action = BAR_PULL_BACK;
                        self.speed = -8.0;
                    }
                }
            }
            BAR_PULL_BACK => {
                self.speed += 0.73;
                if self.speed > 0.0 {
                    if self.stopped_timer != 0 {
                        self.stopped_timer -= 1;
                        self.speed = 0.0;
                    } else {
                        o.action = BAR_EXTEND;
                        self.speed = 29.0;
                    }
                }
            }
            BAR_EXTEND => {
                if (self.offset == 250.0 || (250.0 - self.offset) * (250.0 - self.start_offset) < 0.0) && self.speed > -8.0 && self.speed < 8.0 {
                    o.action = BAR_RETRACT;
                    self.speed = 0.0;
                } else {
                    let mut accel = if self.offset < 250.0 { 6.4 } else { -6.4 };
                    if self.speed * accel < 0.0 {
                        accel *= 2.35;
                    }
                    self.speed += accel;
                    if cx.ttc_speed == TTC_SPEED_RANDOM && self.offset * self.start_offset < 0.0 && random_u16() % 4 == 0 {
                        self.reset(o);
                    }
                }
            }
            BAR_RETRACT => {
                if o.timer > 30 {
                    self.speed = -5.0;
                    if self.offset < 0.0 {
                        self.reset(o);
                    }
                }
            }
            _ => {}
        }
        o.set_dist_from_home(self.offset);
        // POS_OP_COMPUTE_VELOCITY
        o.vel = [o.pos[0] - saved[0], o.pos[1] - saved[1], o.pos[2] - saved[2]];
    }
}

// ---------------------------------------------------------------------------
// ttc_pendulum.inc.c
// ---------------------------------------------------------------------------

/// `sTTCPendulumInitialAccels`.
pub const PENDULUM_INITIAL_ACCELS: [f64; 4] = [13.0, 22.0, 13.0, 0.0];

pub struct Pendulum {
    angle: f64,
    angle_vel: f64,
    angle_accel: f64,
    accel_dir: f64,
    delay: i64,
    sound_timer: i64,
    setting: i32,
}

impl Pendulum {
    pub fn new(setting: i32) -> Self {
        Self { angle: 0.0, angle_vel: 0.0, angle_accel: 0.0, accel_dir: 1.0, delay: 0, sound_timer: 0, setting }
    }
    pub fn angle(&self) -> f64 { self.angle }
}

impl Behavior for Pendulum {
    /// `bhv_ttc_pendulum_init`.
    fn init(&mut self, _o: &mut Obj) {
        if self.setting != TTC_SPEED_STOPPED {
            self.angle_accel = PENDULUM_INITIAL_ACCELS[self.setting as usize];
            self.angle = 6500.0;
        } else {
            self.angle = 6371.5557;
        }
    }

    /// `bhv_ttc_pendulum_update`; no platform displacement (the angle velocity stays 0).
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        if cx.ttc_speed != TTC_SPEED_STOPPED {
            if self.sound_timer != 0 {
                self.sound_timer -= 1; // SOUND_GENERAL_PENDULUM_SWING at 0
            }
            if self.delay != 0 {
                self.delay -= 1;
            } else {
                if self.angle * self.accel_dir > 0.0 {
                    self.accel_dir = -self.accel_dir;
                }
                self.angle_vel += self.angle_accel * self.accel_dir;
                if self.angle_vel == 0.0 {
                    if cx.ttc_speed == TTC_SPEED_RANDOM {
                        self.angle_accel = if random_u16() % 3 != 0 { 13.0 } else { 42.0 };
                        if random_u16() % 2 == 0 {
                            self.delay = random_linear_offset(5, 30);
                        }
                    }
                    self.sound_timer = self.delay + 15;
                }
                self.angle += self.angle_vel;
            }
        }
        o.face[2] = self.angle.trunc();
    }
}

// ---------------------------------------------------------------------------
// ttc_pit_block.inc.c
// ---------------------------------------------------------------------------

/// `sTTCPitBlockProperties[speed][up/down] = { speed, waitTime }`.
pub const PIT_BLOCK_PROPERTIES: [[(f64, i64); 2]; 4] = [
    [(11.0, 20), (-9.0, 30)],
    [(18.0, 15), (-11.0, 15)],
    [(11.0, 20), (-9.0, -1)],
    [(0.0, 0), (0.0, 0)],
];

pub struct PitBlock {
    peak_y: f64,
    dir: i32,
    wait_time: i64,
    setting: i32,
}

impl PitBlock {
    pub fn new(setting: i32) -> Self { Self { peak_y: 0.0, dir: 0, wait_time: 0, setting } }
}

impl Behavior for PitBlock {
    /// `bhv_ttc_pit_block_init` (the stopped setting parks it at the top).
    fn init(&mut self, o: &mut Obj) {
        self.peak_y = o.pos[1] + 330.0;
        if self.setting == TTC_SPEED_STOPPED {
            o.pos[1] += 330.0;
        }
    }

    /// `bhv_ttc_pit_block_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        if o.timer > self.wait_time {
            o.move_using_fvel_and_gravity();
            if clamp_f32(&mut o.pos[1], o.home[1], self.peak_y) {
                self.dir ^= 0x01;
                self.wait_time = PIT_BLOCK_PROPERTIES[cx.ttc_speed as usize][(self.dir & 1) as usize].1;
                if self.wait_time < 0 {
                    self.wait_time = random_mod_offset(10, 20, 6);
                }
                o.vel[1] = PIT_BLOCK_PROPERTIES[cx.ttc_speed as usize][self.dir as usize].0;
                o.timer = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ttc_spinner.inc.c
// ---------------------------------------------------------------------------

/// `sTTCSpinnerSpeeds`.
pub const SPINNER_SPEEDS: [f64; 4] = [200.0, 600.0, 200.0, 0.0];

#[derive(Default)]
pub struct Spinner {
    dir: f64,
    change_dir_timer: i64,
}

impl Behavior for Spinner {
    /// `bhv_ttc_spinner_update`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        o.angle_vel[0] = SPINNER_SPEEDS[cx.ttc_speed as usize];
        if cx.ttc_speed == TTC_SPEED_RANDOM {
            if o.timer > self.change_dir_timer {
                self.dir = random_sign();
                self.change_dir_timer = random_mod_offset(30, 30, 4);
                o.timer = 0;
            } else if o.timer > 5 {
                o.angle_vel[0] *= self.dir;
            } else {
                o.angle_vel[0] = 0.0;
            }
        }
        o.face[0] += o.angle_vel[0];
    }
}

// ---------------------------------------------------------------------------
// ttc_treadmill.inc.c
// ---------------------------------------------------------------------------

/// `sTTCTreadmillSpeeds`.
pub const TREADMILL_SPEEDS: [f64; 4] = [50.0, 100.0, 0.0, 0.0];

pub struct Treadmill {
    shared: Arc<Mutex<TreadmillShared>>,
    speed: i32,
}

impl Behavior for Treadmill {
    /// `bhv_ttc_treadmill_init`: both movtex speeds start at the setting's speed.
    fn init(&mut self, _o: &mut Obj) {
        let mut s = self.shared.lock().unwrap();
        s.speed = TREADMILL_SPEEDS[self.speed as usize];
        s.master = None;
    }

    /// `bhv_ttc_treadmill_update` followed by `cur_obj_compute_vel_xz`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        let mut s = self.shared.lock().unwrap();
        if s.master == Some(cx.index) || s.master.is_none() {
            s.master = Some(cx.index);
            if cx.ttc_speed == TTC_SPEED_RANDOM {
                if o.timer > s.time_until_switch {
                    let mut speed = s.speed;
                    if approach_f32_ptr(&mut speed, 0.0, 10.0) {
                        s.time_until_switch = random_mod_offset(10, 20, 7);
                        s.target_speed = random_sign() * 50.0;
                        o.timer = 0;
                    }
                    s.speed = speed;
                } else if o.timer > 5 {
                    let target = s.target_speed;
                    let mut speed = s.speed;
                    approach_f32_ptr(&mut speed, target, 10.0);
                    s.speed = speed;
                }
            }
        }
        o.forward_vel = 0.084 * s.speed;
        o.compute_vel_xz();
    }
}
